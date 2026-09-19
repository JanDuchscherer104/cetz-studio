use cetz_studio::{
    edit::{self, Command, Patch},
    model::{self, Vertex},
    parameters,
    render::instrument,
    session::{canonical_figure, save_checked},
};
use std::fs;
use tempfile::tempdir;

const SOURCE: &str = "// αβ Unicode before source spans\n#graph(\n n(1.00, -2, <a>, [Keep *this* and $x_i$]),\n n(30, 20, <b>, [Keep #box[all, nested] content]),\n edge(<a>, (15mm, 2mm), (15mm, -20mm), <b.west>, \"-|>\", [$f_i$], label-pos: (1, .5)),\n)\n";

#[test]
fn noop_is_byte_identical() {
    let d = model::parse(SOURCE, None).unwrap();
    let result = edit::apply(
        SOURCE,
        &d,
        &Command::MoveNode {
            id: "a".into(),
            x: 1.0,
            y: -2.0,
        },
    )
    .unwrap();
    assert_eq!(result.as_bytes(), SOURCE.as_bytes());
}
#[test]
fn node_move_only_replaces_two_literals() {
    let d = model::parse(SOURCE, None).unwrap();
    let result = edit::apply(
        SOURCE,
        &d,
        &Command::MoveNode {
            id: "a".into(),
            x: 8.5,
            y: 3.0,
        },
    )
    .unwrap();
    assert_eq!(result, SOURCE.replacen("n(1.00, -2,", "n(8.5, 3,", 1));
}
#[test]
fn nested_content_and_comments_are_opaque() {
    let source = SOURCE.replace(
        "[Keep *this* and $x_i$]",
        "[Keep #grid(columns: 2, [a,b], [c]) $x_i$ // n(88,99,<fake>,[])\n]",
    );
    let d = model::parse(&source, None).unwrap();
    assert_eq!(d.nodes.len(), 2);
    assert_eq!(d.edges.len(), 1);
}
#[test]
fn comments_between_args_survive() {
    let source = SOURCE.replace("1.00, -2", "1.00 /* x */, /* y */ -2");
    let d = model::parse(&source, None).unwrap();
    let result = edit::apply(
        &source,
        &d,
        &Command::MoveNode {
            id: "a".into(),
            x: 2.0,
            y: 4.0,
        },
    )
    .unwrap();
    assert_eq!(
        result,
        source.replace("1.00 /* x */, /* y */ -2", "2 /* x */, /* y */ 4")
    );
}
#[test]
fn alias_scale_is_detected_and_inverted() {
    let source = format!("#let n = content-at.with(y-scale: .65)\n{SOURCE}");
    let d = model::parse(&source, None).unwrap();
    assert_eq!(d.y_scale, 0.65);
    assert!((d.nodes[0].position.as_ref().unwrap().y + 1.3).abs() < 1e-9);
    let result = edit::apply(
        &source,
        &d,
        &Command::MoveNode {
            id: "a".into(),
            x: 1.0,
            y: 6.5,
        },
    )
    .unwrap();
    assert_eq!(result, source.replace("n(1.00, -2,", "n(1.00, 10,"));
}
#[test]
fn source_fixture_identity_and_expected_shape() {
    use sha2::{Digest, Sha256};
    let source = include_str!("../fixtures/aria-overview.typ");
    assert_eq!(
        format!("{:x}", Sha256::digest(source.as_bytes())),
        include_str!("../fixtures/aria-overview.sha256").trim()
    );
    let d = model::parse(source, None).unwrap();
    assert_eq!(d.nodes.len(), 12);
    assert_eq!(d.edges.len(), 12);
    assert_eq!(d.y_scale, 0.65);
    assert_eq!(d.edges[8].vertices.len(), 4);
    assert!(d.warnings.iter().any(|w| w.contains("literal coordinates")));
}
#[test]
fn physical_waypoint_y_is_inverted_without_changing_units() {
    let source = SOURCE.replace("(15mm, 2mm)", "(1.5cm, 72pt)");
    let d = model::parse(&source, None).unwrap();
    let result = edit::apply(
        &source,
        &d,
        &Command::MoveWaypoint {
            edge: "e0".into(),
            vertex: 1,
            x: 20.0,
            y: -50.8,
        },
    )
    .unwrap();
    assert!(result.contains("(2cm, 144pt)"));
    assert!(result.contains("<b.west>, \"-|>\", [$f_i$]"));
}
#[test]
fn orthogonal_segment_preserves_other_coordinates() {
    let d = model::parse(SOURCE, None).unwrap();
    let result = edit::apply(
        SOURCE,
        &d,
        &Command::MoveSegment {
            edge: "e0".into(),
            segment: 1,
            delta: 3.0,
        },
    )
    .unwrap();
    assert_eq!(result, SOURCE.replace("(15mm,", "(18mm,"));
}
#[test]
fn node_attached_segment_cannot_be_detached() {
    let d = model::parse(SOURCE, None).unwrap();
    assert!(edit::apply(
        SOURCE,
        &d,
        &Command::MoveSegment {
            edge: "e0".into(),
            segment: 0,
            delta: 3.0
        }
    )
    .is_err());
}
#[test]
fn named_endpoint_is_not_a_draggable_point() {
    let d = model::parse(SOURCE, None).unwrap();
    assert!(edit::apply(
        SOURCE,
        &d,
        &Command::MoveWaypoint {
            edge: "e0".into(),
            vertex: 0,
            x: 10.0,
            y: 20.0
        }
    )
    .is_err());
}
#[test]
fn diagonal_segment_is_rejected() {
    let source = SOURCE.replace("(15mm, -20mm)", "(16mm, -20mm)");
    let d = model::parse(&source, None).unwrap();
    assert!(edit::apply(
        &source,
        &d,
        &Command::MoveSegment {
            edge: "e0".into(),
            segment: 1,
            delta: 1.0
        }
    )
    .is_err());
}
#[test]
fn changing_port_keeps_node_identity() {
    let d = model::parse(SOURCE, None).unwrap();
    let result = edit::apply(
        SOURCE,
        &d,
        &Command::SetPort {
            edge: "e0".into(),
            end: "end".into(),
            port: "north".into(),
        },
    )
    .unwrap();
    assert_eq!(result, SOURCE.replace("<b.west>", "<b.north>"));
}
#[test]
fn node_id_with_dot_uses_longest_match() {
    let source = SOURCE
        .replace("<b>", "<b.sub>")
        .replace("<b.west>", "<b.sub.west>");
    let d = model::parse(&source, None).unwrap();
    let result = edit::apply(
        &source,
        &d,
        &Command::SetPort {
            edge: "e0".into(),
            end: "end".into(),
            port: "south".into(),
        },
    )
    .unwrap();
    assert!(result.contains("<b.sub.south>"));
}
#[test]
fn port_input_cannot_inject_source() {
    let d = model::parse(SOURCE, None).unwrap();
    assert!(edit::apply(
        SOURCE,
        &d,
        &Command::SetPort {
            edge: "e0".into(),
            end: "end".into(),
            port: "other>, n(1,2,<evil>,[])".into()
        }
    )
    .is_err());
}
#[test]
fn label_move_preserves_math_and_arrow() {
    let d = model::parse(SOURCE, None).unwrap();
    let result = edit::apply(
        SOURCE,
        &d,
        &Command::SetLabel {
            edge: "e0".into(),
            segment: 2,
            fraction: 0.7,
        },
    )
    .unwrap();
    assert_eq!(result, SOURCE.replace("(1, .5)", "(2, 0.7)"));
}
#[test]
fn absent_label_position_is_inserted_without_reformatting() {
    let source = SOURCE.replace(", label-pos: (1, .5)", "");
    let d = model::parse(&source, None).unwrap();
    let result = edit::apply(
        &source,
        &d,
        &Command::SetLabel {
            edge: "e0".into(),
            segment: 1,
            fraction: 0.6,
        },
    )
    .unwrap();
    assert_eq!(
        result,
        source.replace("edge(", "edge(label-pos: (1, 0.6), ")
    );
}
#[test]
fn invalid_label_segment_and_fraction_are_rejected() {
    let d = model::parse(SOURCE, None).unwrap();
    for (segment, fraction) in [(3, 0.5), (0, -0.1), (0, f64::NAN)] {
        assert!(edit::apply(
            SOURCE,
            &d,
            &Command::SetLabel {
                edge: "e0".into(),
                segment,
                fraction
            }
        )
        .is_err());
    }
}
#[test]
fn computed_label_position_is_not_destroyed() {
    let source = SOURCE.replace("(1, .5)", "label_anchor");
    let d = model::parse(&source, None).unwrap();
    assert!(edit::apply(
        &source,
        &d,
        &Command::SetLabel {
            edge: "e0".into(),
            segment: 1,
            fraction: 0.6
        }
    )
    .is_err());
}
#[test]
fn nonfinite_and_outlandish_positions_are_rejected() {
    let d = model::parse(SOURCE, None).unwrap();
    for x in [f64::NAN, f64::INFINITY, 100_001.0] {
        assert!(edit::apply(
            SOURCE,
            &d,
            &Command::MoveNode {
                id: "a".into(),
                x,
                y: 1.0
            }
        )
        .is_err());
    }
}
#[test]
fn duplicates_and_multiple_diagrams_are_rejected() {
    assert!(model::parse(&SOURCE.replace("<b>, [Keep", "<a>, [Keep"), None).is_err());
    assert!(model::parse(&format!("{SOURCE}{SOURCE}"), None).is_err());
}
#[test]
fn computed_node_position_is_readonly() {
    let source = SOURCE.replace("n(1.00, -2,", "n(x + 2, -2,");
    let d = model::parse(&source, None).unwrap();
    assert!(!d.nodes[0].editable);
    assert!(edit::apply(
        &source,
        &d,
        &Command::MoveNode {
            id: "a".into(),
            x: 3.0,
            y: 1.0
        }
    )
    .is_err());
}
#[test]
fn elastic_grid_is_not_mistaken_for_millimetres() {
    let d = model::parse(
        "#diagram(card((0, 1), <a>, [A], []), card((1,1),<b>,[B],[]), edge(<a>,(0,2),<b>,\"->\"))",
        None,
    )
    .unwrap();
    assert!(d.nodes.iter().all(|n| !n.editable));
    assert!(matches!(&d.edges[0].vertices[1], Vertex::Point { point } if !point.editable));
}
#[test]
fn custom_routing_is_not_advertised_as_editable() {
    let source = SOURCE.replace("label-pos: (1, .5)", "label-pos: (1, .5), bend: 20deg");
    let d = model::parse(&source, None).unwrap();
    assert!(!d.edges[0].editable);
}
#[test]
fn utf8_and_overlap_guards_reject_bad_patches() {
    assert!(edit::apply_patches(
        "αb",
        vec![Patch {
            span: 1..2,
            replacement: "x".into()
        }]
    )
    .is_err());
    assert!(edit::apply_patches(
        "abcdef",
        vec![
            Patch {
                span: 0..3,
                replacement: "x".into()
            },
            Patch {
                span: 2..4,
                replacement: "y".into()
            }
        ]
    )
    .is_err());
}
#[test]
fn preview_instrumentation_is_a_separate_document() {
    let d = model::parse(SOURCE, None).unwrap();
    let instrumented = instrument(SOURCE, &d).unwrap();
    assert!(instrumented.contains("render: __cetz_studio_render"));
    assert!(instrumented.contains("draw.floating"));
    assert!(instrumented.contains("draw-diagram(grid, nodes, decorated"));
    assert!(!SOURCE.contains("__cetz_studio_"));
}
#[test]
fn protocol_cannot_replace_the_document() {
    assert!(serde_json::from_str::<Command>(
        r#"{"kind":"move_node","id":"a","x":1,"y":2,"source":"evil"}"#
    )
    .is_err());
}
#[test]
fn save_retains_backup_and_conflicts_do_not_overwrite() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("figure.typ");
    fs::write(&path, SOURCE).unwrap();
    let backup = save_checked(&path, SOURCE, "new source").unwrap();
    assert_eq!(fs::read_to_string(&backup).unwrap(), SOURCE);
    assert_eq!(fs::read_to_string(&path).unwrap(), "new source");
    fs::write(&path, "concurrent edit").unwrap();
    assert!(save_checked(&path, "new source", "clobber").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "concurrent edit");
}
#[test]
fn root_containment_is_checked() {
    let root = tempdir().unwrap();
    let other = tempdir().unwrap();
    let path = other.path().join("figure.typ");
    fs::write(&path, SOURCE).unwrap();
    assert!(canonical_figure(root.path(), &path).is_err());
}
#[cfg(unix)]
#[test]
fn symlinks_are_rejected_and_mode_is_preserved() {
    use std::os::unix::{fs::symlink, fs::PermissionsExt};
    let temp = tempdir().unwrap();
    let file = temp.path().join("file.typ");
    let link = temp.path().join("link.typ");
    fs::write(&file, SOURCE).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o640)).unwrap();
    symlink(&file, &link).unwrap();
    assert!(canonical_figure(temp.path(), &link).is_err());
    assert!(save_checked(&link, SOURCE, "no").is_err());
    save_checked(&file, SOURCE, "new").unwrap();
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[test]
fn explicit_scale_override_does_not_evaluate_computed_alias() {
    let source = format!("#let n = content-at.with(y-scale: configured_scale)\n{SOURCE}");
    assert!(model::parse(&source, None).is_err());
    let d = model::parse(&source, Some(0.65)).unwrap();
    assert_eq!(d.y_scale, 0.65);
    assert!((d.nodes[0].position.as_ref().unwrap().y + 1.3).abs() < 1e-9);
}

#[test]
fn studio_card_uses_named_identity_and_literal_tuple_coordinates() {
    let source = "#studio.diagram(studio.card((10mm, 20mm), [Title], name: <card>))";
    let diagram = model::parse(source, None).unwrap();
    assert_eq!(diagram.nodes.len(), 1);
    assert_eq!(diagram.nodes[0].id, "card");
    assert!(diagram.nodes[0].editable);
    assert_eq!(diagram.nodes[0].position.as_ref().unwrap().x, 10.0);
    assert_eq!(diagram.nodes[0].position.as_ref().unwrap().y, -20.0);
}

#[test]
fn studio_fletcher_example_exposes_params_and_only_literal_cards() {
    let source = include_str!("../examples/studio-fletcher.typ");
    let parameters = parameters::parse(source);
    assert!(parameters.warnings.is_empty(), "{:?}", parameters.warnings);
    assert_eq!(parameters.parameters.len(), 4);

    let diagram = model::parse(source, None).unwrap();
    assert_eq!(diagram.nodes.len(), 4);
    assert!(diagram.nodes[0].editable);
    assert!(diagram.nodes[1..].iter().all(|node| !node.editable));
}
