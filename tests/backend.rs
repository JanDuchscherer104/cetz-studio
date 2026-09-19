use cetz_studio::{
    edit::Command,
    render::Compiler,
    session::{Mode, Session},
};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};
use tempfile::tempdir;

fn compiler(root: PathBuf, executable: PathBuf) -> Compiler {
    Compiler {
        executable,
        root,
        timeout: Duration::from_secs(5),
        font_paths: vec![],
    }
}

#[cfg(unix)]
fn fake_typst(root: &std::path::Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = root.join("fake-typst");
    let script = format!(
        "#!/bin/sh\nset -eu\ninput=\"$6\"\noutput=\"$7\"\npage() {{ printf '%s' \"$output\" | sed \"s/{{p}}/$1/\"; }}\n{body}\n"
    );
    fs::write(&path, script).unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

#[cfg(unix)]
#[test]
fn arbitrary_typst_opens_render_only_and_multipage_is_bounded_protocol() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    fs::write(&path, "First page #pagebreak() Second page").unwrap();
    let executable = fake_typst(
        root.path(),
        r#"
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><text>one</text></svg>' > "$(page 1)"
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><text>two</text></svg>' > "$(page 2)"
"#,
    );
    let mut session = Session::open(path, compiler(root.path().into(), executable), None).unwrap();
    session.render().unwrap();
    let snapshot = session.snapshot();
    assert!(snapshot.diagram.is_none());
    assert_eq!(snapshot.pages.len(), 2);
    assert_eq!(snapshot.svg.as_ref(), snapshot.pages.first());
    assert!(snapshot.capabilities.multi_page);
    assert!(!snapshot.capabilities.graph_gestures);
    assert!(matches!(snapshot.mode, Mode::RenderOnly));
    assert!(snapshot.warnings.iter().any(|w| w.contains("read-only")));
}

#[cfg(unix)]
#[test]
fn preview_storage_uses_source_volume_and_cleans_up() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    fs::write(&path, "A preview").unwrap();
    let executable = fake_typst(
        root.path(),
        r#"
test "$(dirname "$input")" = "$(dirname "$(dirname "$output")")" || exit 23
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>' > "$(page 1)"
"#,
    );
    let mut session = Session::open(path, compiler(root.path().into(), executable), None).unwrap();
    session.render().unwrap();
    assert!(session.snapshot().preview_current);
    let entries = fs::read_dir(root.path()).unwrap().count();
    assert_eq!(
        entries, 2,
        "Only the original source and fake compiler remain"
    );
}

#[cfg(unix)]
#[test]
fn silent_compiler_failure_reports_exit_status() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    fs::write(&path, "A preview").unwrap();
    let executable = fake_typst(root.path(), "exit 7");
    let mut session = Session::open(path, compiler(root.path().into(), executable), None).unwrap();
    let error = session.render().unwrap_err().to_string();
    assert!(error.contains("exit status: 7"), "{error}");
    assert!(error.contains("no readable diagnostics"), "{error}");
    assert!(!session.snapshot().preview_current);
}

#[cfg(unix)]
#[test]
fn compiler_logs_are_drained_with_bounded_diagnostics() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    fs::write(&path, "A preview").unwrap();
    let executable = fake_typst(root.path(), "dd if=/dev/zero bs=1024 count=256 2>/dev/null\n{ dd if=/dev/zero bs=1024 count=256 2>/dev/null; } >&2\nexit 8");
    let mut session = Session::open(path, compiler(root.path().into(), executable), None).unwrap();
    let error = session.render().unwrap_err().to_string();
    assert!(
        error.contains("exit status: 8"),
        "Expected compiler exit status"
    );
    assert!(error.len() < 129 * 1024, "Diagnostics must be bounded");
    assert!(
        error.len() >= 128 * 1024,
        "Retain the bounded diagnostic prefix"
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[cfg(unix)]
#[test]
fn instrumentation_failure_falls_back_to_clean_render() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let source = "#graph(n(1, 2, <a>, [A]))";
    fs::write(&path, source).unwrap();
    let executable = fake_typst(
        root.path(),
        r#"
if grep -q __cetz_studio_ "$input"; then echo 'instrumentation rejected' >&2; exit 1; fi
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>' > "$(page 1)"
"#,
    );
    let mut session = Session::open(path, compiler(root.path().into(), executable), None).unwrap();
    session.render().unwrap();
    let snapshot = session.snapshot();
    assert!(snapshot.diagram.is_some());
    assert!(!snapshot.capabilities.graph_gestures);
    assert!(snapshot.svg.is_some());
    assert!(snapshot
        .diagnostics
        .contains("Graph gestures are unavailable"));
    assert!(session
        .edit(
            0,
            Command::MoveNode {
                id: "a".into(),
                x: 3.0,
                y: 2.0,
            },
        )
        .is_err());
}

#[cfg(unix)]
#[test]
fn graph_edit_rolls_back_if_candidate_loses_instrumentation() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let source = "#graph(n(1, 2, <a>, [A]))";
    fs::write(&path, source).unwrap();
    let executable = fake_typst(
        root.path(),
        r#"
if grep -q __cetz_studio_ "$input" && grep -q 'n(3' "$input"; then exit 1; fi
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>' > "$(page 1)"
"#,
    );
    let mut session = Session::open(path, compiler(root.path().into(), executable), None).unwrap();
    session.render().unwrap();
    assert!(session.snapshot().capabilities.graph_gestures);
    assert!(session
        .edit(
            0,
            Command::MoveNode {
                id: "a".into(),
                x: 3.0,
                y: 2.0,
            },
        )
        .is_err());
    let snapshot = session.snapshot();
    assert_eq!(snapshot.source, source);
    assert_eq!(snapshot.revision, 0);
    assert!(!snapshot.dirty);
}

#[cfg(unix)]
#[test]
fn failed_parameter_compile_rolls_back_source_and_history() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let source = "#import \"@preview/cetz-studio:0.1.0\" as studio\n#let width = studio.param(2mm, min: 1mm, max: 4mm)\n#rect(width: width)";
    fs::write(&path, source).unwrap();
    let executable = fake_typst(
        root.path(),
        r#"
if grep -q '3mm' "$input"; then echo 'injected compile failure' >&2; exit 1; fi
printf '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>' > "$(page 1)"
"#,
    );
    let mut session =
        Session::open(path.clone(), compiler(root.path().into(), executable), None).unwrap();
    session.render().unwrap();
    assert!(session
        .edit(
            0,
            Command::SetParameter {
                id: "width".into(),
                value: Value::from(3),
            },
        )
        .is_err());
    let snapshot = session.snapshot();
    assert_eq!(snapshot.source, source);
    assert_eq!(snapshot.revision, 0);
    assert!(!snapshot.dirty);
    assert!(!snapshot.undo);
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
#[ignore = "Requires installed Typst and cached CeTZ 0.5.2 package"]
fn actual_cetz_figure_renders_without_semantic_adapter() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let source = "#import \"@preview/cetz:0.5.2\"\n#cetz.canvas({ import cetz.draw: *; circle((0, 0), radius: 1) })";
    fs::write(&path, source).unwrap();
    let executable = std::env::var_os("TYPST")
        .map(PathBuf::from)
        .unwrap_or_else(|| "typst".into());
    let mut native = compiler(root.path().into(), executable);
    native.timeout = Duration::from_secs(90);
    let mut session = Session::open(path, native, None).unwrap();
    session.render().unwrap();
    let snapshot = session.snapshot();
    assert!(snapshot.diagram.is_none());
    assert_eq!(snapshot.pages.len(), 1);
    assert!(snapshot.svg.unwrap().contains("<svg"));
}
