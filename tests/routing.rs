use cetz_studio::{
    edit::{self, Command},
    model,
    render::Compiler,
    routing::{self, Route},
    session::Session,
};
use std::{fs, path::PathBuf, time::Duration};

const SOURCE: &str = r#"// Unicode and equations must remain byte-for-byte: α → β
#graph(
  n(0, 0, <a>, [$Q_h$]),
  n(40, 0, <block>, [Obstacle]),
  n(80, 0, <b>, [$K,V$]),
  edge(<a.east>, /* attachment */ <b.west>, "-|>", [$bold(u)_i$], stroke: .7pt),
  edge(<a.south>, <b.south>, "->"),
)
"#;

fn route(edge: &str) -> Route {
    Route {
        edge: edge.into(),
        points: vec![[12.0, 0.0], [12.0, -18.0], [68.0, -18.0], [68.0, 0.0]],
    }
}

fn apply(source: &str, routes: Vec<Route>) -> anyhow::Result<String> {
    edit::apply(
        source,
        &model::parse(source, None)?,
        &Command::RouteEdges { routes },
    )
}

#[test]
fn only_route_literals_change_not_nodes_labels_marks_styles_or_trivia() {
    let next = apply(SOURCE, vec![route("e0")]).unwrap();
    assert_eq!(
        next,
        SOURCE.replacen(
            "<a.east>",
            "<a.east>, (12mm, 0mm), (12mm, 18mm), (68mm, 18mm), (68mm, 0mm)",
            1,
        )
    );
    let original = model::parse(SOURCE, None).unwrap();
    let parsed = model::parse(&next, None).unwrap();
    assert_eq!(
        serde_json::to_value(&original.nodes).unwrap(),
        serde_json::to_value(&parsed.nodes).unwrap()
    );
    assert_eq!(parsed.edges[0].vertices.len(), 6);
    assert_eq!(parsed.edges[1].vertices.len(), 2);
}

#[test]
fn reuses_existing_units_and_keeps_comments_inside_and_between_tuples() {
    let source = SOURCE.replace(
        "<a.east>,",
        "<a.east>, (1cm, /* y */ 0mm), /* retained */ (1cm, 2cm),",
    );
    let next = apply(&source, vec![route("e0")]).unwrap();
    let expected = source
        .replace("(1cm, /* y */ 0mm)", "(1.2cm, /* y */ 0mm)")
        .replace("(1cm, 2cm)", "(1.2cm, 1.8cm), (68mm, 18mm), (68mm, 0mm)");
    assert_eq!(next, expected);
    assert_eq!(apply(&next, vec![route("e0")]).unwrap(), next);
}

#[test]
fn all_or_nothing_batch_and_typed_command_limits() {
    assert!(apply(SOURCE, vec![route("e0"), route("missing")]).is_err());
    assert!(apply(SOURCE, vec![route("e0"), route("e0")]).is_err());
    assert!(apply(SOURCE, vec![]).is_err());
    for points in [
        vec![],
        vec![[f64::NAN, 0.0]],
        vec![[100_001.0, 0.0]],
        vec![[0.0, 0.0], [1.0, 1.0]],
        vec![[0.0, 0.0], [0.0, 0.0]],
        vec![[0.0, 0.0]; routing::MAX_POINTS + 1],
    ] {
        assert!(apply(
            SOURCE,
            vec![Route {
                edge: "e0".into(),
                points,
            }]
        )
        .is_err());
    }
    let next = apply(SOURCE, vec![route("e0"), route("e1")]).unwrap();
    assert!(model::parse(&next, None)
        .unwrap()
        .edges
        .iter()
        .all(|e| e.vertices.len() == 6));
    assert!(serde_json::from_str::<Command>(
        r#"{"kind":"route_edges","routes":[],"source":"replacement"}"#
    )
    .is_err());
}

#[test]
fn source_capabilities_refuse_unproven_geometry_with_explanations() {
    let cases = [
        SOURCE.replace("<a.east>", "<a.north-east>"),
        SOURCE.replace("<a.east>", "<b.east>"),
        SOURCE.replace("<a.east>", "(0mm, 0mm)"),
        SOURCE.replace("<a.east>,", "<a.east>, <block>,"),
        SOURCE.replace("<a.east>,", "<a.east>, (1, 2),"),
        SOURCE.replace("n(40, 0", "n(20 + 20, 0"),
        SOURCE.replace("#graph(", "#graph(\n  ..generated,"),
        SOURCE.replace("stroke: .7pt", "bend: 30deg"),
        SOURCE.replace("stroke: .7pt", "label-pos: computed"),
        SOURCE.replace("\"-|>\"", "\"r\""),
        SOURCE.replace("\"-|>\"", "computed-mark"),
    ];
    for source in cases {
        let diagram = model::parse(&source, None).unwrap();
        let caps = routing::capabilities(&source, Some(&diagram), true);
        assert!(caps[0].reason.is_some(), "{source}");
        assert!(apply(&source, vec![route("e0")]).is_err(), "{source}");
    }
    let diagram = model::parse(SOURCE, None).unwrap();
    assert!(routing::capabilities(SOURCE, Some(&diagram), false)[0]
        .reason
        .is_some());
    assert!(routing::capabilities(SOURCE, Some(&diagram), true)[0]
        .reason
        .is_none());
}

#[test]
fn longest_node_name_matching_and_no_waypoint_deletion() {
    let source = SOURCE.replace("<a", "<a.part");
    let diagram = model::parse(&source, None).unwrap();
    let caps = routing::capabilities(&source, Some(&diagram), true);
    assert_eq!(caps[0].start.as_ref().unwrap().node, "a.part");
    assert_eq!(caps[0].start.as_ref().unwrap().port, "east");
    let routed = apply(SOURCE, vec![route("e0")]).unwrap();
    assert!(apply(
        &routed,
        vec![Route {
            edge: "e0".into(),
            points: vec![[12.0, 0.0]],
        }]
    )
    .is_err());
}

#[test]
#[ignore = "requires the real Typst 0.14.2 compiler and pinned Fletcher package"]
fn native_route_batch_undo_redo_save_reopen_and_rejections() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("routing.typ");
    let source = include_str!("../examples/routing.typ");
    fs::write(&path, source).unwrap();
    let compiler = Compiler {
        executable: PathBuf::from(std::env::var_os("TYPST").unwrap_or_else(|| "typst".into())),
        root: directory.path().to_path_buf(),
        timeout: Duration::from_secs(120),
        font_paths: vec![],
    };
    let mut session = Session::open(path.clone(), compiler.clone(), None).unwrap();
    session.render().unwrap();
    assert!(
        session.snapshot().capabilities.graph_gestures,
        "{}",
        session.snapshot().diagnostics
    );
    let original = session.snapshot();
    session
        .edit(
            0,
            Command::RouteEdges {
                routes: vec![route("e0")],
            },
        )
        .unwrap();
    let routed = session.snapshot();
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
    assert_eq!(routed.revision, 1);
    assert!(routed.dirty && routed.undo);
    assert_eq!(
        serde_json::to_value(&original.diagram.unwrap().nodes).unwrap(),
        serde_json::to_value(&routed.diagram.unwrap().nodes).unwrap()
    );
    session.history(1, false).unwrap();
    assert_eq!(session.snapshot().source, source);
    assert!(!session.snapshot().undo);
    session.history(2, true).unwrap();
    assert_eq!(session.snapshot().source, routed.source);
    assert!(session
        .edit(
            0,
            Command::RouteEdges {
                routes: vec![route("e0")],
            }
        )
        .is_err());
    assert_eq!(session.snapshot().revision, 3);
    let backup = session.save(3).unwrap().unwrap();
    assert_eq!(fs::read_to_string(backup).unwrap(), source);
    assert_eq!(fs::read_to_string(&path).unwrap(), routed.source);
    let mut reopened = Session::open(path.clone(), compiler.clone(), None).unwrap();
    reopened.render().unwrap();
    assert_eq!(reopened.snapshot().source, routed.source);
    assert!(reopened.snapshot().capabilities.graph_gestures);

    let before = reopened.snapshot();
    let command = || Command::RouteEdges {
        routes: vec![route("e1")],
    };
    reopened.compiler.executable = directory.path().join("missing-compiler");
    assert!(reopened.edit(0, command()).is_err());
    assert_eq!(reopened.snapshot().source, before.source);
    assert_eq!(reopened.snapshot().revision, before.revision);
    reopened.compiler = compiler;
    fs::write(&path, format!("{}\n// external change\n", before.source)).unwrap();
    assert!(reopened.edit(0, command()).is_err());
    assert_eq!(reopened.snapshot().source, before.source);
    assert_eq!(reopened.snapshot().revision, before.revision);
}
