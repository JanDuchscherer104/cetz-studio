//! Ask the pinned Fletcher implementation for resolved layout, not browser estimates.
use super::Geometry;
use crate::{model::Diagram, render::Compiler};
use anyhow::{bail, ensure, Context, Result};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
use tempfile::{Builder, NamedTempFile};
use wait_timeout::ChildExt;

pub fn measure(
    compiler: &Compiler,
    original: &Path,
    source: &str,
    diagram: &Diagram,
) -> Result<Geometry> {
    ensure!(
        !source.contains("__cetz_studio_") && !source.contains("<cetz-studio-routing-geometry>"),
        "Figure uses a reserved routing identifier"
    );
    let mut query_source = source.to_string();
    query_source.insert_str(
        diagram.call.args_open + 1,
        "render: __cetz_studio_measure, ",
    );
    let mut query = Builder::new()
        .prefix(".cetz-studio-routing-")
        .suffix(".typ")
        .tempfile_in(original.parent().context("Missing figure directory")?)?;
    write!(query, "{}\n{}", include_str!("measure.typ"), query_source)?;
    query.flush()?;
    let parent = original.parent().context("Missing figure directory")?;
    let stdout = NamedTempFile::new_in(parent)?;
    let stderr = NamedTempFile::new_in(parent)?;
    let mut command = Command::new(&compiler.executable);
    command
        .arg("query")
        .arg("--root")
        .arg(&compiler.root)
        .arg("--field")
        .arg("value")
        .arg("--one")
        .arg("--format")
        .arg("json")
        .arg(query.path())
        .arg("<cetz-studio-routing-geometry>")
        .current_dir(&compiler.root)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.reopen()?))
        .stderr(Stdio::from(stderr.reopen()?));
    for path in &compiler.font_paths {
        command.arg("--font-path").arg(path);
    }
    let mut child = command
        .spawn()
        .context("Cannot launch Typst geometry query")?;
    let status = match child.wait_timeout(compiler.timeout)? {
        Some(status) => status,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Routing geometry query timed out; draft unchanged");
        }
    };
    let mut diagnostics = String::new();
    stderr
        .reopen()?
        .take(128 * 1024)
        .read_to_string(&mut diagnostics)?;
    ensure!(
        status.success(),
        "Typst routing geometry query failed; draft unchanged:\n{diagnostics}"
    );
    ensure!(
        fs::metadata(stdout.path())?.len() <= 2 * 1024 * 1024,
        "Routing geometry exceeds 2 MiB"
    );
    let geometry: Geometry =
        serde_json::from_reader(stdout.reopen()?).context("Invalid routing geometry receipt")?;
    ensure!(
        geometry.edges.len() == diagram.edges.len(),
        "Generated edges cannot be routed through a source override"
    );
    Ok(geometry)
}
