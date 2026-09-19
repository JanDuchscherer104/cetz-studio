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
fn literal_node_and_edge_text_are_editable_without_touching_style() {
    let source = "#graph(\n n(1, 2, <a>, [Alpha]),\n n(3, 4, <b>, [Beta]),\n edge(<a>, <b>, \"->\", [old], stroke: 2pt),\n)";
    let diagram = model::parse(source, None).unwrap();
    assert_eq!(diagram.nodes[0].text_fields[0].id, "title");
    assert!(diagram.nodes[0].text_fields[0].editable);
    assert_eq!(diagram.edges[0].text_fields[0].value, "old");

    let node_changed = edit::apply(
        source,
        &diagram,
        &Command::SetNodeText {
            id: "a".into(),
            field: "title".into(),
            text: "First node".into(),
        },
    )
    .unwrap();
    assert_eq!(node_changed, source.replace("[Alpha]", "\"First node\""));

    let diagram = model::parse(&node_changed, None).unwrap();
    let edge_changed = edit::apply(
        &node_changed,
        &diagram,
        &Command::SetEdgeText {
            edge: "e0".into(),
            field: "label".into(),
            text: "new label".into(),
        },
    )
    .unwrap();
    assert!(edge_changed.contains("label: \"new label\", stroke: 2pt"));
    let reparsed = model::parse(&edge_changed, None).unwrap();
    assert_eq!(reparsed.edges[0].text_fields[0].value, "new label");
    let changed_again = edit::apply(
        &edge_changed,
        &reparsed,
        &Command::SetEdgeText {
            edge: "e0".into(),
            field: "label".into(),
            text: "final label".into(),
        },
    )
    .unwrap();
    assert!(changed_again.contains("label: \"final label\", stroke: 2pt"));
}

#[test]
fn rich_or_computed_text_is_explicitly_read_only() {
    let diagram = model::parse(SOURCE, None).unwrap();
    let title = &diagram.nodes[0].text_fields[0];
    assert!(!title.editable);
    assert!(title.reason.as_deref().unwrap().contains("Typst markup"));
    assert!(title.source_editable);
    assert_eq!(title.source, "[Keep *this* and $x_i$]");
    let error = edit::apply(
        SOURCE,
        &diagram,
        &Command::SetNodeText {
            id: "a".into(),
            field: "title".into(),
            text: "replacement".into(),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("Typst markup"));
    assert!(SOURCE.contains("$x_i$"));
}

#[test]
fn raw_content_editor_can_replace_selected_math_without_flattening_it() {
    let diagram = model::parse(SOURCE, None).unwrap();
    let node_changed = edit::apply(
        SOURCE,
        &diagram,
        &Command::SetNodeSource {
            id: "a".into(),
            field: "title".into(),
            source: "[Score $x^2 + y^2$]".into(),
        },
    )
    .unwrap();
    assert_eq!(
        node_changed,
        SOURCE.replace("[Keep *this* and $x_i$]", "[Score $x^2 + y^2$]")
    );

    let diagram = model::parse(&node_changed, None).unwrap();
    let edge_changed = edit::apply(
        &node_changed,
        &diagram,
        &Command::SetEdgeSource {
            edge: "e0".into(),
            field: "label".into(),
            source: "[$x + y$]".into(),
        },
    )
    .unwrap();
    assert!(edge_changed.contains("label: [$x + y$], label-pos: (1, .5)"));
    let reparsed = model::parse(&edge_changed, None).unwrap();
    assert_eq!(reparsed.edges[0].text_fields[0].source, "[$x + y$]");
}

#[test]
fn raw_content_editor_rejects_graph_structure_injection() {
    let diagram = model::parse(SOURCE, None).unwrap();
    for source in [
        "[safe], edge(<a>, <b>, \"->\")",
        "[safe]) #graph(n(1, 2, <evil>, [bad]))",
        "label: [safe]",
        "..payload",
    ] {
        assert!(
            edit::apply(
                SOURCE,
                &diagram,
                &Command::SetNodeSource {
                    id: "a".into(),
                    field: "title".into(),
                    source: source.into(),
                },
            )
            .is_err(),
            "{source}"
        );
    }
}

#[test]
fn computed_content_expression_is_raw_editable_but_not_plain_editable() {
    let source = "#graph(n(1, 2, <a>, title_value), n(3, 4, <b>, [B]))";
    let diagram = model::parse(source, None).unwrap();
    let field = &diagram.nodes[0].text_fields[0];
    assert!(!field.editable);
    assert!(field.source_editable);
    assert_eq!(field.source, "title_value");
    let changed = edit::apply(
        source,
        &diagram,
        &Command::SetNodeSource {
            id: "a".into(),
            field: "title".into(),
            source: "[Literal now]".into(),
        },
    )
    .unwrap();
    assert!(changed.contains("<a>, [Literal now]"));
}

#[test]
fn comments_and_structural_markup_are_never_mistaken_for_plain_text() {
    for content in [
        "[visible // hidden\n]",
        "[visible /* hidden */]",
        "[= Heading]",
        "[- item]",
    ] {
        let source = format!("#graph(n(1, 2, <a>, {content}))");
        let diagram = model::parse(&source, None).unwrap();
        assert!(!diagram.nodes[0].text_fields[0].editable, "{content}");
    }
}

#[test]
fn n_title_and_body_are_independent_and_new_punctuation_is_string_encoded() {
    let source = "#graph(n(1, 2, <a>, [Title], body: [plain body]))";
    let diagram = model::parse(source, None).unwrap();
    assert_eq!(diagram.nodes[0].text_fields.len(), 2);
    assert_eq!(diagram.nodes[0].text_fields[1].id, "body");
    let replacement = "cost: $5 #1 [draft] \"q\" \\path\nnext";
    let changed = edit::apply(
        source,
        &diagram,
        &Command::SetNodeText {
            id: "a".into(),
            field: "body".into(),
            text: replacement.into(),
        },
    )
    .unwrap();
    assert!(changed.contains("body: \"cost: $5 #1 [draft] \\\"q\\\" \\\\path\\nnext\""));
    let reparsed = model::parse(&changed, None).unwrap();
    assert!(reparsed.nodes[0].text_fields[1].editable);
    assert_eq!(reparsed.nodes[0].text_fields[1].value, replacement);
}

#[test]
fn duplicate_node_preserves_call_and_offsets_literal_position() {
    let source = "#graph(\n n(1.00, -2, <a>, [Alpha], fill: red), // describes a\n n(30, 20, <b>, [Beta]),\n)";
    let diagram = model::parse(source, None).unwrap();
    let changed =
        edit::apply(source, &diagram, &Command::DuplicateNode { id: "a".into() }).unwrap();
    assert!(changed.contains("n(11, 8, <a-copy>, [Alpha], fill: red)"));
    let comment = changed.find("// describes a").unwrap();
    let copy = changed.find("<a-copy>").unwrap();
    assert!(
        comment < copy,
        "the source comment must remain with the original node"
    );
    let parsed = model::parse(&changed, None).unwrap();
    assert_eq!(parsed.nodes.len(), 3);
    assert_eq!(parsed.nodes[1].id, "a-copy");
}

#[test]
fn add_edge_uses_named_nodes_and_validated_arrow() {
    let source = "#import \"@local/cetz-studio:0.1.0\" as studio\n#studio.diagram(\n studio.node((0mm, 0mm), [A], name: <a>),\n studio.node((20mm, 0mm), [B], name: <b>),\n)";
    let diagram = model::parse(source, None).unwrap();
    let changed = edit::apply(
        source,
        &diagram,
        &Command::AddEdge {
            from: "a".into(),
            to: "b".into(),
            label: Some("review".into()),
            arrow: "both".into(),
        },
    )
    .unwrap();
    assert!(changed.contains("studio.edge(<a>, <b>, \"<->\", label: \"review\")"));
    assert_eq!(model::parse(&changed, None).unwrap().edges.len(), 1);

    let bad = Command::AddEdge {
        from: "a".into(),
        to: "b".into(),
        label: None,
        arrow: "Typst code".into(),
    };
    assert!(edit::apply(source, &diagram, &bad).is_err());
}

#[test]
fn studio_and_fletcher_node_presets_insert_only_into_matching_diagrams() {
    let studio = "#import \"@local/cetz-studio:0.1.0\" as studio\n#studio.diagram(\n studio.node((0mm, 0mm), [A], name: <a>),\n)";
    let diagram = model::parse(studio, None).unwrap();
    let changed = edit::apply(
        studio,
        &diagram,
        &Command::InsertNode {
            primitive: "studio-card".into(),
            x: 20.0,
            y: 15.0,
            name: Some("decision".into()),
            text: Some("Review".into()),
        },
    )
    .unwrap();
    assert!(changed.contains("studio.card((20mm, -15mm), \"Review\", body: [], name: <decision>)"));

    let fletcher = "#import \"@preview/fletcher:0.5.8\" as f\n#f.diagram(\n f.node((0mm, 0mm), [A], name: <a>),\n)";
    let diagram = model::parse(fletcher, None).unwrap();
    let changed = edit::apply(
        fletcher,
        &diagram,
        &Command::InsertNode {
            primitive: "fletcher-diamond".into(),
            x: 12.0,
            y: -8.0,
            name: None,
            text: None,
        },
    )
    .unwrap();
    assert!(changed.contains("shape: f.shapes.diamond"));
    assert!(edit::apply(
        fletcher,
        &diagram,
        &Command::InsertNode {
            primitive: "studio-node".into(),
            x: 0.0,
            y: 0.0,
            name: None,
            text: None,
        },
    )
    .is_err());

    let mixed = "#import \"@preview/fletcher:0.5.8\" as f\n#import \"@local/cetz-studio:0.1.0\" as studio\n#f.diagram(\n f.node((0mm, 0mm), [A], name: <a>), // keep this comment\n debug: false,\n)";
    let mut current = mixed.to_string();
    for (index, primitive) in [
        "fletcher-rect",
        "fletcher-ellipse",
        "fletcher-diamond",
        "studio-node",
        "studio-card",
    ]
    .into_iter()
    .enumerate()
    {
        let diagram = model::parse(&current, None).unwrap();
        assert!(diagram
            .insert_primitives
            .iter()
            .any(|item| item == primitive));
        current = edit::apply(
            &current,
            &diagram,
            &Command::InsertNode {
                primitive: primitive.into(),
                x: 20.0 + index as f64 * 10.0,
                y: 10.0,
                name: None,
                text: Some(format!("item {index}")),
            },
        )
        .unwrap();
    }
    assert_eq!(model::parse(&current, None).unwrap().nodes.len(), 6);
    let comment = current.find("// keep this comment").unwrap();
    let first_inserted = current.find("<rectangle>").unwrap();
    let named_option = current.find("debug: false").unwrap();
    assert!(comment < first_inserted && first_inserted < named_option);
}

#[test]
fn gallery_requires_a_unique_unshadowed_import_alias() {
    let source = "#import \"@local/cetz-studio:0.1.0\" as studio\n#let studio = (: )\n#diagram(node((0mm, 0mm), [A], name: <a>))";
    let diagram = model::parse(source, None).unwrap();
    assert!(diagram.insert_primitives.is_empty());
    assert!(diagram
        .insertion_reason
        .as_deref()
        .unwrap()
        .contains("explicit"));
}

#[test]
fn delete_edge_preserves_nodes_named_options_and_comments() {
    let source = "#graph(\n n(1, 2, <a>, [A]),\n n(3, 4, <b>, [B]),\n edge(<a>, <b>, \"->\"), // edge explanation\n debug: false,\n)";
    let diagram = model::parse(source, None).unwrap();
    let changed =
        edit::apply(source, &diagram, &Command::DeleteEdge { edge: "e0".into() }).unwrap();
    assert!(changed.contains("// edge explanation"));
    assert!(changed.contains("debug: false"));
    let reparsed = model::parse(&changed, None).unwrap();
    assert_eq!(reparsed.nodes.len(), 2);
    assert!(reparsed.edges.is_empty());
}

#[test]
fn delete_node_requires_explicit_cascade_and_removes_attached_edges_once() {
    let source = "#graph(\n n(1, 2, <a>, [A]), // node explanation\n n(3, 4, <b>, [B]),\n edge(<a.east>, <b>, \"->\"),\n)";
    let diagram = model::parse(source, None).unwrap();
    assert_eq!(diagram.nodes[0].attached_edges, 1);
    let error = edit::apply(
        source,
        &diagram,
        &Command::DeleteNode {
            id: "a".into(),
            cascade: false,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("cascade: true"));

    let changed = edit::apply(
        source,
        &diagram,
        &Command::DeleteNode {
            id: "a".into(),
            cascade: true,
        },
    )
    .unwrap();
    assert!(changed.contains("// node explanation"));
    assert!(changed.contains("<b>"));
    assert!(!changed.contains("<a>"));
    let reparsed = model::parse(&changed, None).unwrap();
    assert_eq!(reparsed.nodes.len(), 1);
    assert!(reparsed.edges.is_empty());
}

#[test]
fn node_deletion_refuses_opaque_references_and_the_final_node() {
    let opaque = "#graph(n(1, 2, <a>, [A]), n(3, 4, <b>, [B]), if true { edge(<a>, <b>, \"->\") })";
    let diagram = model::parse(opaque, None).unwrap();
    assert!(diagram.opaque_references);
    assert!(diagram.nodes.iter().all(|node| !node.deletable));
    let error = edit::apply(
        opaque,
        &diagram,
        &Command::DeleteNode {
            id: "a".into(),
            cascade: true,
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("may reference"));

    let single = "#graph(n(1, 2, <a>, [A]))";
    let diagram = model::parse(single, None).unwrap();
    assert_eq!(
        diagram.nodes[0].delete_reason.as_deref().unwrap(),
        "The final recognized node cannot be deleted because the editor requires a non-empty graph"
    );
    assert!(edit::apply(
        single,
        &diagram,
        &Command::DeleteNode {
            id: "a".into(),
            cascade: true,
        },
    )
    .is_err());
}

#[test]
fn nested_alias_shadowing_disables_gallery_insertion() {
    let source = "#import \"@local/cetz-studio:0.1.0\" as studio\n#{ let studio = (:); studio.diagram(studio.node((0mm, 0mm), [A], name: <a>)) }";
    let diagram = model::parse(source, None).unwrap();
    assert!(diagram.insert_primitives.is_empty());
    assert!(edit::apply(
        source,
        &diagram,
        &Command::InsertNode {
            primitive: "studio-node".into(),
            x: 10.0,
            y: 10.0,
            name: None,
            text: None,
        },
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
