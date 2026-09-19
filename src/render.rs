//! The installed Typst compiler renders both previews and save validation.
//! Temporary siblings preserve relative imports. Neither the original figure nor
//! the shared architecture library is changed for preview instrumentation.
use crate::{
    edit::number,
    model::{Diagram, Vertex},
};
use anyhow::{bail, ensure, Context, Result};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
use tempfile::{Builder, NamedTempFile};
use wait_timeout::ChildExt;

pub const MARKER_STROKE: f64 = 0.000_123_45;

#[derive(Clone, Debug)]
pub struct Compiler {
    pub executable: PathBuf,
    pub root: PathBuf,
    pub timeout: Duration,
    pub font_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct Rendered {
    /// Compatibility view of the first page.
    pub svg: String,
    pub pages: Vec<String>,
    pub diagnostics: String,
    /// True only when graph markers were compiled into a single page.
    pub instrumented: bool,
}

pub fn instrument(source: &str, diagram: &Diagram) -> Result<String> {
    ensure!(
        !source.contains("__cetz_studio_"),
        "Source uses a reserved prototype identifier"
    );
    let bezier = include_str!("../typst/cetz-studio/bezier.typ")
        .replace("draw-edge", "__cetz_studio_draw_bezier")
        .replace("point(", "__cetz_studio_bezier_point(");
    let nodes = diagram
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let (expected, check) = n
                .position
                .as_ref()
                .filter(|point| n.editable && point.editable)
                .map(|point| (typst_point(point.x, point.y), "true"))
                .unwrap_or_else(|| ("none".into(), "false"));
            format!("(<{}>, \"a1{i:04x}\", {expected}, {check})", n.id)
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let vertices = diagram
        .edges
        .iter()
        .map(|e| e.vertices.len().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let edge_expectations = diagram
        .edges
        .iter()
        .map(|edge| {
            let vertices = edge
                .vertices
                .iter()
                .map(|vertex| match vertex {
                    Vertex::Point { point, .. } if point.editable => typst_point(point.x, point.y),
                    _ => "none".into(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let comma = if edge.vertices.len() == 1 { "," } else { "" };
            format!("({}, ({vertices}{comma}))", edge.editable)
        })
        .collect::<Vec<_>>()
        .join(",\n");
    let prelude = format!(
        r##"#import "@preview/fletcher:0.5.8" as __cetz_studio_f
#let __cetz_studio_box(pos, size, color) = {{
  let (x, y) = (pos.at(0), pos.at(1))
  let (w, h) = size
  __cetz_studio_f.cetz.draw.floating(
    __cetz_studio_f.cetz.draw.rect(
      (x - w/2, y - h/2), (x + w/2, y + h/2),
      stroke: 0.00012345pt + rgb(color), fill: none,
    )
  )
}}
#let __cetz_studio_hex(i) = {{
  let alphabet = "0123456789abcdef"
  range(4).rev().map(k => alphabet.at(int(calc.rem(calc.floor(i / calc.pow(16, k)), 16)))).join()
}}
#let __cetz_studio_close(actual, expected) = {{
  (calc.abs(actual.at(0) - expected.at(0)) <= 0.001mm) and (calc.abs(actual.at(1) - expected.at(1)) <= 0.001mm)
}}
{bezier}
#let __cetz_studio_render(grid, nodes, edges, options) = {{
  let wanted = ({nodes},)
  let counts = ({vertices}{vertices_comma})
  let expected-edges = ({edge_expectations}{edges_comma})
  assert(edges.len() == counts.len(), message: "Editor cannot map dynamically generated edges. Use a direct standalone graph.")
  let decorated = edges.enumerate().map(((i, e)) => {{
    let original = e.label-wrapper
    e.label-wrapper = ee => box(original(ee), stroke: 0.00012345pt + rgb("a3" + __cetz_studio_hex(i)))
    e
  }})
  __cetz_studio_f.cetz.canvas({{
    for edge in decorated.filter(edge => edge.kind == "bezier") {{
      let attachments = __cetz_studio_f.find-nodes-for-edge(grid, nodes, edge)
      __cetz_studio_draw_bezier(__cetz_studio_f, edge, attachments, debug: options.debug)
    }}
    __cetz_studio_f.draw-diagram(grid, nodes, decorated.filter(edge => edge.kind != "bezier"), debug: options.debug)
    // Floating fiducials do not change the canvas bounds. Browser removes them.
    __cetz_studio_box((0pt, 0pt), (.02pt, .02pt), "a00000")
    __cetz_studio_box((10mm, 0pt), (.02pt, .02pt), "a00001")
    __cetz_studio_box((0pt, -10mm), (.02pt, .02pt), "a00002")
    for (name, color, expected, check) in wanted {{
      let matches = nodes.filter(n => n.name == name)
      assert(matches.len() == 1, message: "Editor node mapping is not one-to-one.")
      let node = matches.first()
      if check {{
        assert(__cetz_studio_close(node.pos.xyz, expected),
          message: "Editor node coordinates do not match the source literals.")
      }}
      __cetz_studio_box(node.pos.xyz, node.size, color)
    }}
    for (i, e) in edges.enumerate() {{
      let (editable, expected) = expected-edges.at(i)
      if editable {{
        assert(e.final-vertices.len() == counts.at(i),
          message: "Editor edge vertices do not map one-to-one to source vertices.")
        for (j, wanted) in expected.enumerate() {{
          if wanted != none {{
            assert(__cetz_studio_close(e.final-vertices.at(j), wanted),
              message: "Editor waypoint does not match its source literal.")
          }}
        }}
      }}
      if e.final-vertices.len() == counts.at(i) {{
        for (j, p) in e.final-vertices.enumerate() {{
          __cetz_studio_box(p, (.02pt, .02pt), "a2" + __cetz_studio_hex(i*256+j))
        }}
      }}
    }}
  }})
}}
"##,
        vertices_comma = if diagram.edges.is_empty() { "" } else { "," },
        edges_comma = if diagram.edges.is_empty() { "" } else { "," },
    );
    let at = diagram.call.args_open + 1;
    let mut source = source.to_string();
    source.insert_str(at, "render: __cetz_studio_render, ");
    Ok(format!("{prelude}\n{source}"))
}

fn typst_point(x: f64, editor_y: f64) -> String {
    format!("({}mm, {}mm)", number(x), number(-editor_y))
}

fn capped_text(path: &Path, limit: u64) -> Result<String> {
    use std::io::Read;
    let mut buf = String::new();
    fs::File::open(path)?.take(limit).read_to_string(&mut buf)?;
    Ok(buf)
}

impl Compiler {
    pub fn render(
        &self,
        original: &Path,
        source: &str,
        diagram: Option<&Diagram>,
    ) -> Result<Rendered> {
        if let Some(diagram) = diagram {
            match instrument(source, diagram)
                .and_then(|marked| self.compile(original, &marked, true))
            {
                Ok(rendered) if rendered.pages.len() == 1 => return Ok(rendered),
                Ok(rendered) => {
                    let reason = format!("Graph gestures are disabled because the instrumented figure produced {} pages", rendered.pages.len());
                    let mut clean = self.compile(original, source, false)?;
                    clean.diagnostics = join_diagnostics(&reason, &clean.diagnostics);
                    return Ok(clean);
                }
                Err(error) => {
                    // Instrumentation is an optional adapter. A valid Typst file
                    // must still render when its graph shape is unsupported.
                    let mut clean = self.compile(original, source, false)?;
                    clean.diagnostics = join_diagnostics(
                        &format!("Graph gestures are unavailable: {error:#}"),
                        &clean.diagnostics,
                    );
                    return Ok(clean);
                }
            }
        }
        self.compile(original, source, false)
    }

    fn compile(&self, original: &Path, source: &str, instrumented: bool) -> Result<Rendered> {
        let parent = original
            .parent()
            .context("Figure has no parent directory")?;
        let mut temp = Builder::new()
            .prefix(".cetz-studio-")
            .suffix(".typ")
            .tempfile_in(parent)?;
        temp.write_all(source.as_bytes())?;
        temp.flush()?;
        let output = Builder::new().prefix("cetz-studio-render-").tempdir()?;
        // Typst replaces {p} with one-based page numbers, including for a
        // single-page document. This avoids probing or overwriting page files.
        let svg_path = output.path().join("page-{p}.svg");
        let stderr = NamedTempFile::new()?;
        let stdout = NamedTempFile::new()?;
        let mut command = Command::new(&self.executable);
        command
            .arg("compile")
            .arg("--root")
            .arg(&self.root)
            .arg("--format")
            .arg("svg")
            .arg(temp.path())
            .arg(&svg_path)
            .current_dir(&self.root)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout.reopen()?))
            .stderr(Stdio::from(stderr.reopen()?));
        for path in &self.font_paths {
            command.arg("--font-path").arg(path);
        }
        let mut child = command.spawn().with_context(|| {
            format!(
                "Cannot launch {}. Install Typst or pass --typst /path/to/typst.",
                self.executable.display()
            )
        })?;
        let status = match child.wait_timeout(self.timeout)? {
            Some(status) => status,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                bail!(
                    "Typst compilation exceeded {} seconds; original source is unchanged",
                    self.timeout.as_secs()
                );
            }
        };
        let diagnostics = capped_text(stderr.path(), 128 * 1024)?;
        ensure!(
            status.success(),
            "Typst failed; original source is unchanged:\n{diagnostics}"
        );
        let mut paths = fs::read_dir(output.path())?
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                let name = path.file_name()?.to_str()?;
                let page = name
                    .strip_prefix("page-")?
                    .strip_suffix(".svg")?
                    .parse::<usize>()
                    .ok()?;
                Some((page, path))
            })
            .collect::<Vec<_>>();
        paths.sort_by_key(|(page, _)| *page);
        ensure!(!paths.is_empty(), "Typst produced no SVG pages");
        ensure!(
            paths.len() <= 64,
            "Figure exceeds the 64-page preview limit"
        );
        ensure!(
            paths
                .iter()
                .enumerate()
                .all(|(index, (page, _))| *page == index + 1),
            "Typst produced a non-contiguous page set"
        );
        let mut pages = Vec::with_capacity(paths.len());
        let mut total = 0_u64;
        for (_, path) in paths {
            let size = fs::metadata(&path)?.len();
            ensure!(size <= 32 * 1024 * 1024, "One SVG page exceeds 32 MiB");
            total = total
                .checked_add(size)
                .context("SVG output size overflow")?;
            ensure!(
                total <= 128 * 1024 * 1024,
                "SVG pages exceed 128 MiB in total"
            );
            let svg = fs::read_to_string(&path)?;
            validate_svg(&svg)?;
            pages.push(svg);
        }
        let svg = pages[0].clone();
        Ok(Rendered {
            svg,
            pages,
            diagnostics,
            instrumented,
        })
    }
}

fn join_diagnostics(note: &str, diagnostics: &str) -> String {
    if diagnostics.trim().is_empty() {
        note.to_string()
    } else {
        format!("{note}\n{diagnostics}")
    }
}

fn validate_svg(svg: &str) -> Result<()> {
    let doc = roxmltree::Document::parse(svg).context("Compiler output is not valid SVG XML")?;
    ensure!(
        doc.root_element().tag_name().name() == "svg",
        "Compiler output is not an SVG"
    );
    ensure!(
        !doc.descendants()
            .any(|n| n.has_tag_name("script") || n.has_tag_name("foreignObject")),
        "Active SVG content is not supported"
    );
    Ok(())
}
