//! Bounded project discovery and lazy per-file compatibility checks.
use crate::{
    render::Compiler,
    session::{Capabilities, Mode, Session, Snapshot},
};
use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

const MAX_FILES: usize = 2_048;
const MAX_ENTRIES: usize = 20_000;
const MAX_DEPTH: usize = 32;
const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Unchecked,
    Ready,
    Error,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectFile {
    pub path: String,
    pub name: String,
    pub status: CheckStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<Capabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub active: bool,
    pub dirty: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ProjectSnapshot {
    pub root: String,
    pub active_file: String,
    pub files: Vec<ProjectFile>,
}

#[derive(Clone, Debug)]
pub struct Project {
    root: PathBuf,
    files: BTreeMap<String, ProjectFile>,
    visited: usize,
}

impl Project {
    pub fn scan(root: &Path) -> Result<Self> {
        let root = root
            .canonicalize()
            .context("Cannot resolve project folder")?;
        ensure!(
            fs::symlink_metadata(&root)?.file_type().is_dir(),
            "Project root must be a regular directory"
        );
        let mut project = Self {
            root,
            files: BTreeMap::new(),
            visited: 0,
        };
        project.visit(Path::new(""), 0)?;
        Ok(project)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn first_file(&self) -> Option<&str> {
        self.files.keys().next().map(String::as_str)
    }

    pub fn resolve(&self, relative: &str) -> Result<PathBuf> {
        let relative_path = Path::new(relative);
        ensure!(
            !relative_path.as_os_str().is_empty() && !relative_path.is_absolute(),
            "Choose a project-relative Typst file"
        );
        ensure!(
            relative_path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
            "Project path cannot contain parent or special components"
        );
        ensure!(
            relative_path.extension().is_some_and(|ext| ext == "typ"),
            "Expected a .typ source file"
        );
        ensure!(
            self.files.contains_key(relative),
            "File is not present in the project index"
        );
        let candidate = self.root.join(relative_path);
        let metadata = fs::symlink_metadata(&candidate).context("Cannot inspect project file")?;
        ensure!(
            metadata.file_type().is_file(),
            "Project file must be regular and cannot be a symlink"
        );
        ensure!(metadata.len() <= MAX_SOURCE_BYTES, "Source exceeds 2 MiB");
        let canonical = candidate.canonicalize()?;
        ensure!(
            canonical.starts_with(&self.root),
            "Project file escapes the selected folder"
        );
        Ok(canonical)
    }

    pub fn check(
        &mut self,
        relative: &str,
        compiler: Compiler,
        scale: Option<f64>,
    ) -> Result<ProjectFile> {
        let path = self.resolve(relative)?;
        let result = Session::open(path, compiler, scale).and_then(|mut session| {
            session.render()?;
            Ok(session.snapshot())
        });
        let file = self
            .files
            .get_mut(relative)
            .context("File disappeared from project index")?;
        apply_check(file, result.as_ref().ok(), result.as_ref().err());
        Ok(file.clone())
    }

    pub fn mark_snapshot(&mut self, relative: &str, snapshot: &Snapshot) {
        if let Some(file) = self.files.get_mut(relative) {
            apply_check(file, Some(snapshot), None);
        }
    }

    pub fn mark_error(&mut self, relative: &str, error: &anyhow::Error) {
        if let Some(file) = self.files.get_mut(relative) {
            apply_check(file, None, Some(error));
        }
    }

    pub fn snapshot(&self, active_path: &Path, dirty: bool) -> ProjectSnapshot {
        let active = active_path
            .strip_prefix(&self.root)
            .ok()
            .and_then(path_key)
            .unwrap_or_default();
        let files = self
            .files
            .values()
            .cloned()
            .map(|mut file| {
                file.active = file.path == active;
                file.dirty = file.active && dirty;
                file
            })
            .collect();
        ProjectSnapshot {
            root: self.root.to_string_lossy().into_owned(),
            active_file: active,
            files,
        }
    }

    fn visit(&mut self, relative: &Path, depth: usize) -> Result<()> {
        ensure!(
            depth <= MAX_DEPTH,
            "Project directory nesting exceeds {MAX_DEPTH} levels"
        );
        let directory = self.root.join(relative);
        let mut entries = fs::read_dir(&directory)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            self.visited += 1;
            ensure!(
                self.visited <= MAX_ENTRIES,
                "Project contains more than {MAX_ENTRIES} directory entries"
            );
            let name = entry.file_name();
            let Some(name_text) = name.to_str() else {
                continue;
            };
            let path = relative.join(&name);
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if excluded_directory(name_text) {
                    continue;
                }
                self.visit(&path, depth + 1)?;
            } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "typ") {
                let metadata = entry.metadata()?;
                if metadata.len() > MAX_SOURCE_BYTES {
                    continue;
                }
                ensure!(
                    self.files.len() < MAX_FILES,
                    "Project contains more than {MAX_FILES} Typst files"
                );
                let Some(key) = path_key(&path) else { continue };
                self.files.insert(
                    key.clone(),
                    ProjectFile {
                        path: key,
                        name: name_text.to_owned(),
                        status: CheckStatus::Unchecked,
                        mode: None,
                        capabilities: None,
                        error: None,
                        active: false,
                        dirty: false,
                    },
                );
            }
        }
        Ok(())
    }
}

fn apply_check(file: &mut ProjectFile, snapshot: Option<&Snapshot>, error: Option<&anyhow::Error>) {
    if let Some(snapshot) = snapshot.filter(|snapshot| snapshot.preview_current) {
        file.status = CheckStatus::Ready;
        file.mode = Some(snapshot.mode.clone());
        file.capabilities = Some(snapshot.capabilities.clone());
        file.error = None;
    } else {
        file.status = CheckStatus::Error;
        file.mode = None;
        file.capabilities = None;
        file.error = Some(
            error
                .map(|e| format!("{e:#}"))
                .or_else(|| {
                    snapshot
                        .map(|value| value.diagnostics.clone())
                        .filter(|value| !value.is_empty())
                })
                .unwrap_or_else(|| "Compatibility check failed".into()),
        );
    }
}

fn excluded_directory(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "target" | "node_modules" | "__pycache__" | "dist" | "build"
        )
}

fn path_key(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.components() {
        let Component::Normal(value) = part else {
            return None;
        };
        parts.push(value.to_str()?.to_owned());
    }
    Some(parts.join("/"))
}

pub fn relative_key(root: &Path, path: &Path) -> Result<String> {
    path_key(
        path.strip_prefix(root)
            .context("Active file is outside project root")?,
    )
    .context("Active file path is not representable as UTF-8")
}

pub fn require_clean(dirty: bool, discard: bool) -> Result<()> {
    if dirty && !discard {
        bail!("The current file has unsaved changes. Save it or explicitly discard the draft before switching.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scan_is_bounded_to_visible_typst_files() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("a.typ"), "hello").unwrap();
        fs::write(root.path().join("a.txt"), "no").unwrap();
        fs::create_dir(root.path().join("sub")).unwrap();
        fs::write(root.path().join("sub/b.typ"), "hello").unwrap();
        fs::create_dir(root.path().join(".hidden")).unwrap();
        fs::write(root.path().join(".hidden/c.typ"), "hello").unwrap();
        let project = Project::scan(root.path()).unwrap();
        let snapshot = project.snapshot(&root.path().join("a.typ"), false);
        assert_eq!(
            snapshot
                .files
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            ["a.typ", "sub/b.typ"]
        );
        assert!(snapshot
            .files
            .iter()
            .all(|f| f.status == CheckStatus::Unchecked));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_files_and_directories_are_not_indexed() {
        use std::os::unix::fs::symlink;
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("outside.typ"), "hello").unwrap();
        symlink(
            outside.path().join("outside.typ"),
            root.path().join("file.typ"),
        )
        .unwrap();
        symlink(outside.path(), root.path().join("dir")).unwrap();
        assert!(Project::scan(root.path()).unwrap().first_file().is_none());
    }

    #[test]
    fn resolution_rejects_parent_paths_and_unindexed_files() {
        let root = tempdir().unwrap();
        fs::write(root.path().join("a.typ"), "hello").unwrap();
        let project = Project::scan(root.path()).unwrap();
        assert!(project.resolve("../a.typ").is_err());
        assert!(project.resolve("missing.typ").is_err());
        assert!(project.resolve("a.typ").is_ok());
    }

    #[test]
    fn dirty_switch_requires_explicit_discard() {
        assert!(require_clean(true, false).is_err());
        assert!(require_clean(true, true).is_ok());
        assert!(require_clean(false, false).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn failed_lazy_check_is_cached_as_an_error() {
        use std::time::Duration;
        let root = tempdir().unwrap();
        fs::write(root.path().join("broken.typ"), "#unknown-function()").unwrap();
        let mut project = Project::scan(root.path()).unwrap();
        let compiler = Compiler {
            executable: PathBuf::from("/bin/false"),
            root: root.path().to_path_buf(),
            timeout: Duration::from_secs(1),
            font_paths: Vec::new(),
        };
        let checked = project.check("broken.typ", compiler, None).unwrap();
        assert_eq!(checked.status, CheckStatus::Error);
        assert!(checked.error.is_some());
        assert!(checked.capabilities.is_none());
    }
}
