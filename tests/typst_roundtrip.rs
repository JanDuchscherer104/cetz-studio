//! Opt-in real compiler integration. No substitute renderer in these tests.
use cetz_studio::{edit::Command, model, render::Compiler, session::Session};
use serde_json::Value;
use std::{fs, path::PathBuf, time::Duration};
use tempfile::tempdir;

fn compiler(root: PathBuf) -> Compiler {
    Compiler {
        executable: std::env::var_os("TYPST")
            .map(PathBuf::from)
            .unwrap_or_else(|| "typst".into()),
        root,
        timeout: Duration::from_secs(90),
        font_paths: vec![],
    }
}

#[test]
#[ignore = "Requires installed Typst and the real Fletcher 0.5.8 package"]
fn actual_compiler_instrumentation_preserves_canvas_bounds() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let source = include_str!("../examples/demo.typ");
    fs::write(&path, source).unwrap();
    let d = model::parse(source, None).unwrap();
    let compiler = compiler(root.path().into());
    let clean = compiler.render(&path, source, None).unwrap();
    let marked = compiler.render(&path, source, Some(&d)).unwrap();
    let a = roxmltree::Document::parse(&clean.svg).unwrap();
    let b = roxmltree::Document::parse(&marked.svg).unwrap();
    assert_eq!(
        a.root_element().attribute("viewBox"),
        b.root_element().attribute("viewBox")
    );
    for marker in [
        "#a00000", "#a00001", "#a00002", "#a10000", "#a20801", "#a30008",
    ] {
        assert!(marked.svg.contains(marker), "Missing marker {marker}");
        assert!(
            !clean.svg.contains(marker),
            "Instrument leaked into clean render: {marker}"
        );
    }
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
#[ignore = "Requires installed Typst and the real Fletcher 0.5.8 package"]
fn actual_move_save_reopen_and_undo() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let source = include_str!("../examples/demo.typ");
    fs::write(&path, source).unwrap();
    let mut session = Session::open(path.clone(), compiler(root.path().into()), None).unwrap();
    session.render().unwrap();
    session
        .edit(
            0,
            Command::MoveNode {
                id: "trunk".into(),
                x: 24.0,
                y: 40.0,
            },
        )
        .unwrap();
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        source,
        "An edit must not write the original"
    );
    let expected = source.replace("n(20, 40, <trunk>", "n(24, 40, <trunk>");
    assert_eq!(session.snapshot().source, expected);
    let backup = session.save(1).unwrap().unwrap();
    assert_eq!(fs::read_to_string(backup).unwrap(), source);
    assert_eq!(fs::read_to_string(&path).unwrap(), expected);
    let reopened = Session::open(path.clone(), compiler(root.path().into()), None).unwrap();
    assert_eq!(reopened.snapshot().source, expected);
    session.history(2, false).unwrap();
    session.save(3).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}

#[test]
#[ignore = "Requires installed Typst and cached CeTZ 0.5.2/Fletcher 0.5.8 packages"]
fn actual_studio_package_parameter_edit_compiles_without_touching_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = root.join("examples/studio-fletcher.typ");
    let source = fs::read_to_string(&path).unwrap();
    let mut session = Session::open(path.clone(), compiler(root), None).unwrap();
    session.render().unwrap();
    assert!(session.snapshot().capabilities.graph_gestures);
    session
        .edit(
            0,
            Command::SetParameter {
                id: "x-gap".into(),
                value: Value::from(44),
            },
        )
        .unwrap();
    assert!(session.snapshot().source.contains("studio.param(44mm"));
    assert_eq!(fs::read_to_string(path).unwrap(), source);
}

#[test]
#[ignore = "Requires installed Typst and the real Fletcher 0.5.8 package"]
fn mismatched_wrapper_falls_back_and_graph_command_is_rejected() {
    let root = tempdir().unwrap();
    let path = root.path().join("figure.typ");
    let source =
        include_str!("../examples/demo.typ").replace("(x * 1mm, -y * 1mm)", "(x * 2mm, -y * 1mm)");
    fs::write(&path, &source).unwrap();
    let mut session = Session::open(path, compiler(root.path().into()), None).unwrap();
    session.render().unwrap();
    let snapshot = session.snapshot();
    assert!(!snapshot.capabilities.graph_gestures);
    assert!(snapshot.svg.is_some());
    assert!(snapshot
        .diagnostics
        .contains("Graph gestures are unavailable"));
    assert!(session
        .edit(
            0,
            Command::MoveNode {
                id: "trunk".into(),
                x: 24.0,
                y: 40.0,
            },
        )
        .is_err());
    assert_eq!(session.snapshot().source, source);

    let edge_path = root.path().join("edge-figure.typ");
    let edge_source = include_str!("../examples/demo.typ").replace(
        "#let edge(..args) = f.edge(..args.pos(), ..(",
        "#let edge(..args) = f.edge((0mm, 0mm), ..args.pos(), ..(",
    );
    fs::write(&edge_path, &edge_source).unwrap();
    let mut edge_session = Session::open(edge_path, compiler(root.path().into()), None).unwrap();
    edge_session.render().unwrap();
    let edge_snapshot = edge_session.snapshot();
    assert!(!edge_snapshot.capabilities.graph_gestures);
    assert!(edge_snapshot.svg.is_some());
    assert!(edge_snapshot
        .diagnostics
        .contains("Graph gestures are unavailable"));
}

#[test]
#[ignore = "Requires installed Typst and cached CeTZ 0.5.2/Fletcher 0.5.8 packages"]
fn actual_all_gallery_presets_compile_inside_fletcher_diagram() {
    use std::io::Write;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut file = tempfile::Builder::new()
        .prefix(".gallery-")
        .suffix(".typ")
        .tempfile_in(&root)
        .unwrap();
    let source = r#"#import "@preview/fletcher:0.5.8" as f
#import "./typst/cetz-studio/lib.typ" as studio
#set page(width: auto, height: auto, margin: 4mm)
#f.diagram(
  f.node((0mm, 0mm), [Start], name: <start>), // trailing comment is preserved
  f.node((15mm, 0mm), [End], name: <end>),
  f.edge(<start>, <end>, "->", [Flow]),
  debug: false,
)
"#;
    file.write_all(source.as_bytes()).unwrap();
    file.flush().unwrap();
    let mut session = Session::open(file.path().into(), compiler(root), None).unwrap();
    session.render().unwrap();
    session
        .edit(
            0,
            Command::SetEdgeText {
                edge: "e0".into(),
                field: "label".into(),
                text: "first label".into(),
            },
        )
        .unwrap();
    session
        .edit(
            1,
            Command::SetEdgeText {
                edge: "e0".into(),
                field: "label".into(),
                text: "second $label [literal]".into(),
            },
        )
        .unwrap();
    session
        .edit(
            2,
            Command::SetNodeText {
                id: "start".into(),
                field: "title".into(),
                text: "Start #1 [$5]\nsecond line".into(),
            },
        )
        .unwrap();
    session
        .edit(
            3,
            Command::SetNodeSource {
                id: "start".into(),
                field: "title".into(),
                source: "[Start $alpha^2$]".into(),
            },
        )
        .unwrap();
    session
        .edit(
            4,
            Command::SetEdgeSource {
                edge: "e0".into(),
                field: "label".into(),
                source: "[$x^2 + y^2$]".into(),
            },
        )
        .unwrap();
    session
        .edit(5, Command::DuplicateNode { id: "start".into() })
        .unwrap();
    session
        .edit(
            6,
            Command::AddEdge {
                from: "start-copy".into(),
                to: "end".into(),
                label: Some("copied route".into()),
                arrow: "forward".into(),
            },
        )
        .unwrap();

    for (revision, primitive) in [
        "fletcher-rect",
        "fletcher-ellipse",
        "fletcher-diamond",
        "studio-node",
        "studio-card",
    ]
    .into_iter()
    .enumerate()
    {
        session
            .edit(
                revision as u64 + 7,
                Command::InsertNode {
                    primitive: primitive.into(),
                    x: 20.0 + revision as f64 * 15.0,
                    y: 10.0,
                    name: None,
                    text: Some(format!("item {revision}")),
                },
            )
            .unwrap();
    }
    let snapshot = session.snapshot();
    let diagram = snapshot.diagram.unwrap();
    assert_eq!(diagram.nodes.len(), 8);
    assert_eq!(diagram.edges.len(), 2);
    assert!(snapshot.source.contains("label: [$x^2 + y^2$]"));
    assert!(snapshot.source.contains("[Start $alpha^2$]"));
    assert!(snapshot.source.contains("<start-copy>"));
    assert!(snapshot.source.contains("trailing comment is preserved"));
    assert_eq!(fs::read_to_string(file.path()).unwrap(), source);
}

#[test]
#[ignore = "Requires installed Typst and cached Fletcher 0.5.8 package"]
fn actual_cascade_delete_is_one_compiled_undo_step() {
    use std::io::Write;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut file = tempfile::Builder::new()
        .prefix(".delete-")
        .suffix(".typ")
        .tempfile_in(&root)
        .unwrap();
    let source = r#"#import "@preview/fletcher:0.5.8" as f
#set page(width: auto, height: auto, margin: 4mm)
#f.diagram(
  f.node((0mm, 0mm), [Delete], name: <delete>), // preserve this comment
  f.node((20mm, 0mm), [Keep], name: <keep>),
  f.edge(<delete>, <keep>, "->", [attached]),
  debug: false,
)
"#;
    file.write_all(source.as_bytes()).unwrap();
    file.flush().unwrap();
    let mut session = Session::open(file.path().into(), compiler(root), None).unwrap();
    session.render().unwrap();
    session
        .edit(
            0,
            Command::DeleteNode {
                id: "delete".into(),
                cascade: true,
            },
        )
        .unwrap();
    let deleted = session.snapshot();
    assert_eq!(deleted.diagram.as_ref().unwrap().nodes.len(), 1);
    assert!(deleted.diagram.as_ref().unwrap().edges.is_empty());
    assert!(deleted.source.contains("preserve this comment"));
    assert!(deleted.source.contains("debug: false"));
    assert!(!deleted.source.contains("<delete>"));
    session.history(1, false).unwrap();
    assert_eq!(session.snapshot().source, source);
    assert_eq!(fs::read_to_string(file.path()).unwrap(), source);
}
