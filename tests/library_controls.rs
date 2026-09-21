//! Real package compilation through the unchanged Session interface.
use cetz_studio::{edit::Command, render::Compiler, session::Session};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};

fn compiler(root: PathBuf) -> Compiler {
    Compiler {
        executable: std::env::var_os("TYPST")
            .map(PathBuf::from)
            .unwrap_or_else(|| "typst".into()),
        root,
        timeout: Duration::from_secs(60),
        font_paths: vec![],
    }
}

#[test]
#[ignore = "requires the real Typst CLI and version-pinned packages"]
fn native_library_controls_round_trip() {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/library-controls");
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".ci/library-controls");
    fs::create_dir_all(&output).unwrap();
    for (name, argument, value, old, replacement) in [
        ("cetz", "length", 10.0, "length: 8mm", "length: 10mm"),
        ("fletcher", "width", 32.0, "width: 28mm", "width: 32mm"),
        (
            "scenery",
            "azimuth",
            50.0,
            "azimuth: 30deg",
            "azimuth: 50deg",
        ),
        ("plotsy", "scale-dim[2]", 0.8, "(1, 1, 0.5)", "(1, 1, 0.8)"),
        ("maquette", "azimuth", 60.0, "azimuth: 30", "azimuth: 60"),
    ] {
        let temporary = tempfile::tempdir().unwrap();
        let file = temporary.path().join(format!("{name}.typ"));
        let original = fs::read_to_string(fixtures.join(format!("{name}.typ"))).unwrap();
        fs::write(&file, &original).unwrap();
        fs::copy(
            fixtures.join("tetra.obj"),
            temporary.path().join("tetra.obj"),
        )
        .unwrap();
        let compiler = compiler(temporary.path().to_path_buf());
        let mut session = Session::open(file.clone(), compiler.clone(), None).unwrap();
        session
            .render()
            .unwrap_or_else(|error| panic!("{name}: {error:#}"));
        let before = session.snapshot();
        assert!(before.preview_current, "{name}");
        assert_eq!(before.pages.len(), 1, "{name}");
        let parameter = before
            .parameters
            .iter()
            .find(|p| {
                p.origin
                    .as_ref()
                    .is_some_and(|origin| origin.argument == argument)
            })
            .unwrap_or_else(|| panic!("{name}: {:?}", before.warnings));
        let id = parameter.id.clone();
        session
            .edit(
                0,
                Command::SetParameter {
                    id: id.clone(),
                    value: parameter.value.clone(),
                },
            )
            .unwrap();
        assert_eq!(session.revision, 0, "{name}: no-op changed revision");
        assert!(
            session.save(0).unwrap().is_none(),
            "{name}: no-op created backup"
        );
        session
            .edit(
                0,
                Command::SetParameter {
                    id: id.clone(),
                    value: Value::from(value),
                },
            )
            .unwrap();
        let after = session.snapshot();
        let expected = original.replacen(old, replacement, 1);
        assert_eq!(after.source, expected, "{name}: nonlocal source change");
        assert_ne!(
            after.pages, before.pages,
            "{name}: edit had no rendered effect"
        );
        assert_eq!(
            fs::read_to_string(&file).unwrap(),
            original,
            "{name}: draft touched disk"
        );
        assert!(session
            .edit(
                0,
                Command::SetParameter {
                    id,
                    value: Value::from(value)
                }
            )
            .is_err());
        session.history(1, false).unwrap();
        assert_eq!(session.snapshot().source, original, "{name}: undo");
        session.history(2, true).unwrap();
        assert_eq!(session.snapshot().source, expected, "{name}: redo");
        let backup = session.save(3).unwrap().unwrap();
        assert_eq!(
            fs::read_to_string(backup).unwrap(),
            original,
            "{name}: backup"
        );
        let mut reopened = Session::open(file, compiler, None).unwrap();
        reopened.render().unwrap();
        assert_eq!(reopened.snapshot().source, expected, "{name}: reopen");
        assert!(reopened.snapshot().parameters.iter().any(|p| p
            .origin
            .as_ref()
            .is_some_and(|origin| origin.argument == argument)
            && p.value == Value::from(value)));
        assert_eq!(
            fs::read(temporary.path().join("tetra.obj")).unwrap(),
            fs::read(fixtures.join("tetra.obj")).unwrap()
        );
        fs::write(output.join(format!("{name}-before.svg")), &before.pages[0]).unwrap();
        fs::write(output.join(format!("{name}-after.svg")), &after.pages[0]).unwrap();
        fs::write(output.join(format!("{name}-source.diff")), &after.diff).unwrap();
        fs::write(
            output.join(format!("{name}-controls.json")),
            serde_json::to_vec_pretty(&before.parameters).unwrap(),
        )
        .unwrap();
    }
}

#[test]
#[ignore = "requires the real Typst CLI and CeTZ"]
fn invalid_library_edit_preserves_accepted_source_and_render() {
    let temporary = tempfile::tempdir().unwrap();
    let file = temporary.path().join("guard.typ");
    let source = "#import \"@preview/cetz:0.5.2\" as c\n#set page(width: 70mm, height: 50mm)\n#c.canvas(length: 8mm, { c.draw.circle((0,0), radius: 1) })\n".to_string();
    fs::write(&file, &source).unwrap();
    let mut session =
        Session::open(file.clone(), compiler(temporary.path().to_path_buf()), None).unwrap();
    session.render().unwrap();
    let before = session.snapshot();
    let parameter = before
        .parameters
        .iter()
        .find(|p| {
            p.origin
                .as_ref()
                .is_some_and(|origin| origin.argument == "length")
        })
        .unwrap();
    assert!(session
        .edit(
            0,
            Command::SetParameter {
                id: parameter.id.clone(),
                value: Value::from(-1)
            }
        )
        .is_err());
    assert_eq!(session.snapshot().source, before.source);
    assert_eq!(session.snapshot().pages, before.pages);
    // A valid numeric edit must still roll back when the real compilation fails.
    fs::write(temporary.path().join("dependency.typ"), "#let value = 1").unwrap();
    let source = source + "\n#include \"dependency.typ\"\n";
    fs::write(&file, &source).unwrap();
    let mut session =
        Session::open(file.clone(), compiler(temporary.path().to_path_buf()), None).unwrap();
    session.render().unwrap();
    let before = session.snapshot();
    let parameter = before
        .parameters
        .iter()
        .find(|p| {
            p.origin
                .as_ref()
                .is_some_and(|origin| origin.argument == "length")
        })
        .unwrap();
    fs::write(
        temporary.path().join("dependency.typ"),
        "#panic(\"failed dependency\")",
    )
    .unwrap();
    assert!(session
        .edit(
            0,
            Command::SetParameter {
                id: parameter.id.clone(),
                value: Value::from(10)
            }
        )
        .is_err());
    assert_eq!(session.snapshot().source, before.source);
    assert_eq!(session.snapshot().pages, before.pages);
    assert_eq!(session.revision, 0);
    assert_eq!(fs::read_to_string(file).unwrap(), source);
}
