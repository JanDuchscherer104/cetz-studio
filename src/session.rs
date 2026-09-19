//! One open figure, bounded undo history, preview validation, and conservative saves.
use crate::{
    edit,
    model::{self, Diagram},
    parameters::{self, Parameter},
    render::{Compiler, Rendered},
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use similar::TextDiff;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;
use uuid::Uuid;

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub filename: String,
    pub revision: u64,
    pub disk_hash: String,
    pub source: String,
    pub diagram: Option<Diagram>,
    pub parameters: Vec<Parameter>,
    pub mode: Mode,
    pub capabilities: Capabilities,
    pub warnings: Vec<String>,
    pub svg: Option<String>,
    pub pages: Vec<String>,
    pub diagnostics: String,
    pub diff: String,
    pub dirty: bool,
    pub undo: bool,
    pub redo: bool,
    pub preview_current: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    RenderOnly,
    ParameterEditing,
    GraphEditing,
}

#[derive(Clone, Debug, Serialize)]
pub struct Capabilities {
    pub parameters: bool,
    pub graph_gestures: bool,
    pub multi_page: bool,
}

pub struct Session {
    pub path: PathBuf,
    pub compiler: Compiler,
    disk_source: String,
    source: String,
    pub revision: u64,
    scale_override: Option<f64>,
    diagram: Option<Diagram>,
    semantic_warning: Option<String>,
    parameters: Vec<Parameter>,
    parameter_warnings: Vec<String>,
    rendered: Option<Rendered>,
    render_error: Option<String>,
    undo: Vec<String>,
    redo: Vec<String>,
}

pub fn canonical_figure(root: &Path, file: &Path) -> Result<(PathBuf, PathBuf)> {
    let root = root.canonicalize().context("Cannot resolve --root")?;
    let candidate = if file.is_absolute() {
        file.to_path_buf()
    } else {
        std::env::current_dir()?.join(file)
    };
    let metadata = fs::symlink_metadata(&candidate).context("Cannot inspect figure file")?;
    ensure!(
        metadata.file_type().is_file(),
        "Figure must be a regular file, not a symlink or special file"
    );
    let file = candidate.canonicalize()?;
    ensure!(
        file.starts_with(&root),
        "Figure must be contained under --root"
    );
    ensure!(
        file.extension().is_some_and(|x| x == "typ"),
        "Expected a .typ source file"
    );
    ensure!(metadata.len() <= 2 * 1024 * 1024, "Source exceeds 2 MiB");
    Ok((root, file))
}

impl Session {
    pub fn open(path: PathBuf, compiler: Compiler, scale: Option<f64>) -> Result<Self> {
        let source = fs::read_to_string(&path).context("Cannot read UTF-8 figure")?;
        let (diagram, semantic_warning) = semantic_model(&source, scale);
        let parsed_parameters = parameters::parse(&source);
        Ok(Self {
            path,
            compiler,
            disk_source: source.clone(),
            source,
            revision: 0,
            scale_override: scale,
            diagram,
            semantic_warning,
            parameters: parsed_parameters.parameters,
            parameter_warnings: parsed_parameters.warnings,
            rendered: None,
            render_error: None,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    pub fn check_revision(&self, expected: u64) -> Result<()> {
        ensure!(
            expected == self.revision,
            "Stale browser revision; reload the session before editing"
        );
        Ok(())
    }

    pub fn check_disk(&self) -> Result<()> {
        let metadata = fs::symlink_metadata(&self.path)?;
        ensure!(
            metadata.file_type().is_file(),
            "Figure was replaced with a symlink or special file; save refused"
        );
        ensure!(
            metadata.len() <= 2 * 1024 * 1024,
            "Figure on disk is now too large; save refused"
        );
        let source = fs::read_to_string(&self.path)?;
        ensure!(source == self.disk_source, "External edit detected. Save refused; your draft is retained. Export the draft or resolve the file outside the editor.");
        Ok(())
    }

    pub fn render(&mut self) -> Result<()> {
        match self
            .compiler
            .render(&self.path, &self.source, self.diagram.as_ref())
        {
            Ok(rendered) => {
                self.rendered = Some(rendered);
                self.render_error = None;
                Ok(())
            }
            Err(error) => {
                self.render_error = Some(format!("{error:#}"));
                self.rendered = None;
                Err(error)
            }
        }
    }

    pub fn edit(&mut self, expected: u64, command: edit::Command) -> Result<()> {
        self.check_revision(expected)?;
        if matches!(&command, edit::Command::ApplyRoutes { .. }) {
            self.check_disk()?;
        }
        let parameter_command = matches!(&command, edit::Command::SetParameter { .. });
        if !parameter_command {
            ensure!(
                self.graph_gestures_enabled(),
                "Graph gestures require a current, instrumented single-page preview"
            );
        }
        let next = match &command {
            edit::Command::SetParameter { id, value } => {
                parameters::apply(&self.source, &self.parameters, id, value)?
            }
            _ => edit::apply(
                &self.source,
                self.diagram
                    .as_ref()
                    .context("Graph gestures are unavailable for this source")?,
                &command,
            )?,
        };
        if next == self.source {
            return Ok(());
        }
        let (diagram, semantic_warning) = semantic_model(&next, self.scale_override);
        let parsed_parameters = parameters::parse(&next);
        // Compile before adopting a command. A failing move cannot make a broken
        // draft the accepted state, even though the original is never touched.
        let rendered = self.compiler.render(&self.path, &next, diagram.as_ref())?;
        if !parameter_command {
            ensure!(
                rendered.instrumented && rendered.pages.len() == 1,
                "Graph edit could not be verified against an instrumented single-page preview"
            );
        }
        if let edit::Command::ApplyRoutes { routes, geometry } = &command {
            let next_diagram = diagram
                .as_ref()
                .context("Routed graph is not source-editable")?;
            let measured =
                crate::routing::measure(&self.compiler, &self.path, &next, next_diagram)?;
            crate::routing::verify_fixed_geometry(geometry, &measured, routes, next_diagram)?;
            self.check_disk()?;
        }
        self.undo.push(self.source.clone());
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.source = next;
        self.diagram = diagram;
        self.semantic_warning = semantic_warning;
        self.parameters = parsed_parameters.parameters;
        self.parameter_warnings = parsed_parameters.warnings;
        self.rendered = Some(rendered);
        self.render_error = None;
        self.revision += 1;
        Ok(())
    }

    pub fn history(&mut self, expected: u64, redo: bool) -> Result<()> {
        self.check_revision(expected)?;
        let next = if redo {
            self.redo.last()
        } else {
            self.undo.last()
        }
        .context("Nothing to undo/redo")?
        .clone();
        let (diagram, semantic_warning) = semantic_model(&next, self.scale_override);
        let parsed_parameters = parameters::parse(&next);
        let rendered = self.compiler.render(&self.path, &next, diagram.as_ref())?;
        if redo {
            self.redo.pop();
            self.undo.push(self.source.clone());
        } else {
            self.undo.pop();
            self.redo.push(self.source.clone());
        }
        self.source = next;
        self.diagram = diagram;
        self.semantic_warning = semantic_warning;
        self.parameters = parsed_parameters.parameters;
        self.parameter_warnings = parsed_parameters.warnings;
        self.rendered = Some(rendered);
        self.render_error = None;
        self.revision += 1;
        Ok(())
    }

    pub fn save(&mut self, expected: u64) -> Result<Option<PathBuf>> {
        self.check_revision(expected)?;
        self.check_disk()?;
        if self.source == self.disk_source {
            return Ok(None);
        }
        // Validate the uninstrumented publication source independently.
        self.compiler.render(&self.path, &self.source, None)?;
        self.check_disk()?;
        let backup = save_checked(&self.path, &self.disk_source, &self.source)?;
        self.disk_source = self.source.clone();
        self.revision += 1;
        Ok(Some(backup))
    }

    pub fn snapshot(&self) -> Snapshot {
        let diff = TextDiff::from_lines(&self.disk_source, &self.source)
            .unified_diff()
            .context_radius(3)
            .header("on disk", "layout draft")
            .to_string();
        let graph_gestures = self.graph_gestures_enabled();
        let capabilities = Capabilities {
            parameters: !self.parameters.is_empty(),
            graph_gestures,
            multi_page: self.rendered.as_ref().is_some_and(|r| r.pages.len() > 1),
        };
        let mode = if capabilities.graph_gestures {
            Mode::GraphEditing
        } else if capabilities.parameters {
            Mode::ParameterEditing
        } else {
            Mode::RenderOnly
        };
        let mut warnings = self.parameter_warnings.clone();
        if let Some(diagram) = &self.diagram {
            warnings.extend(diagram.warnings.iter().cloned());
        }
        if let Some(warning) = &self.semantic_warning {
            warnings.push(warning.clone());
        }
        if self.diagram.is_some() && self.rendered.as_ref().is_some_and(|r| !r.instrumented) {
            warnings.push("Graph gestures are unavailable for this preview; source parameters remain editable".into());
        }
        Snapshot {
            filename: self
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string(),
            revision: self.revision,
            disk_hash: hash(self.disk_source.as_bytes()),
            source: self.source.clone(),
            diagram: self.diagram.clone(),
            parameters: self.parameters.clone(),
            mode,
            capabilities,
            warnings,
            svg: self.rendered.as_ref().map(|r| r.svg.clone()),
            pages: self
                .rendered
                .as_ref()
                .map(|r| r.pages.clone())
                .unwrap_or_default(),
            diagnostics: self.render_error.clone().unwrap_or_else(|| {
                self.rendered
                    .as_ref()
                    .map(|r| r.diagnostics.clone())
                    .unwrap_or_default()
            }),
            diff,
            dirty: self.source != self.disk_source,
            undo: !self.undo.is_empty(),
            redo: !self.redo.is_empty(),
            preview_current: self.rendered.is_some() && self.render_error.is_none(),
        }
    }

    fn graph_gestures_enabled(&self) -> bool {
        self.diagram.is_some()
            && self
                .rendered
                .as_ref()
                .is_some_and(|rendered| rendered.instrumented && rendered.pages.len() == 1)
    }
}

fn semantic_model(source: &str, scale: Option<f64>) -> (Option<Diagram>, Option<String>) {
    match model::parse(source, scale) {
        Ok(diagram) => (Some(diagram), None),
        Err(error) => (
            None,
            Some(format!("Graph elements are read-only: {error:#}")),
        ),
    }
}

/// Atomic replacement with exact-content conflict checks and a durable backup.
/// Checks detect changes before commit, but cannot provide filesystem CAS against
/// a non-cooperating writer in the final check-to-rename interval. Stop other
/// writers while saving. File mode is retained; owner/ACL/xattr preservation is
/// not promised by this prototype.
pub fn save_checked(path: &Path, expected: &str, candidate: &str) -> Result<PathBuf> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        metadata.file_type().is_file(),
        "Refusing to replace a non-regular file"
    );
    ensure!(
        fs::read_to_string(path)? == expected,
        "External edit detected; save refused"
    );
    let parent = path.parent().context("Missing parent")?;
    let directory = parent.join(".cetz-studio-backups");
    if directory.exists() {
        let m = fs::symlink_metadata(&directory)?;
        ensure!(
            m.file_type().is_dir(),
            "Backup directory must not be a symlink"
        );
    } else {
        fs::create_dir(&directory)?;
    }
    let name = path
        .file_name()
        .context("Missing filename")?
        .to_string_lossy();
    let backup = directory.join(format!(
        "{name}.{}.{}.bak",
        &hash(expected.as_bytes())[..12],
        Uuid::new_v4()
    ));
    let mut backup_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&backup)?;
    backup_file.set_permissions(metadata.permissions().clone())?;
    backup_file.write_all(expected.as_bytes())?;
    backup_file.sync_all()?;
    let mut staged = NamedTempFile::new_in(parent)?;
    staged.as_file().set_permissions(metadata.permissions())?;
    staged.write_all(candidate.as_bytes())?;
    staged.as_file().sync_all()?;
    // Check after staging/backup rather than merely at the start of a request.
    let now = fs::symlink_metadata(path)?;
    ensure!(
        now.file_type().is_file() && fs::read_to_string(path)? == expected,
        "External edit detected immediately before commit; save refused"
    );
    staged
        .persist(path)
        .map_err(|e| e.error)
        .context("Atomic source replacement failed")?;
    // Directory fsync is supported on Unix. No spurious 'save failed' report
    // after a successful replacement on platforms that do not support it.
    #[cfg(unix)]
    {
        if let Ok(dir) = fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(backup)
}
