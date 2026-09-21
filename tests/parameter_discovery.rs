//! Both editor callers and these tests use the same parse/apply interface.
use cetz_studio::parameters::{apply, parse, ParameterKind};
use serde_json::Value;

const PREFIX: &str = "#import \"@preview/cetz-studio:0.1.0\" as studio\n";
const SCENERY: &str = "#import \"@preview/scenery:0.1.0\" as scene\n";

#[test]
fn parses_and_edits_only_explicit_literals() {
    let source = format!("{PREFIX}#let radius = studio.param(20mm, label: \"Radius\", min: 5mm, max: 50mm, step: 1mm)\n#circle(radius: radius)");
    let parsed = parse(&source);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.parameters[0].kind, ParameterKind::Length);
    assert!(apply(&source, &parsed.parameters, "radius", &Value::from(24.5)).is_err());
    let next = apply(&source, &parsed.parameters, "radius", &Value::from(24)).unwrap();
    assert_eq!(next, source.replacen("20mm", "24mm", 1));
}

#[test]
fn refuses_wrong_import_computed_and_mixed_units() {
    let sources = [
        "#import \"other.typ\" as studio\n#let x = studio.param(2)",
        "#import \"cetz-studio/lib.typ\" as studio\n#let x = studio.param(1 + 2)",
        "#import \"cetz-studio/lib.typ\" as studio\n#let x = studio.param(20mm, min: 1cm)",
    ];
    for source in sources {
        let parsed = parse(source);
        assert!(parsed.parameters.is_empty(), "{source}");
        assert!(!parsed.warnings.is_empty(), "{source}");
    }
}

#[test]
fn bounds_and_json_types_are_enforced() {
    let source = format!("{PREFIX}#let n = studio.param(2, min: 1, max: 3)\n#let color = studio.param(\"#d8eadd\")\n#let visible = studio.param(true)");
    let parsed = parse(&source);
    assert!(apply(&source, &parsed.parameters, "n", &Value::from(4)).is_err());
    assert!(apply(&source, &parsed.parameters, "color", &Value::from("\"; evil")).is_err());
    assert!(apply(&source, &parsed.parameters, "visible", &Value::from("true")).is_err());
    assert!(apply(&source, &parsed.parameters, "absent", &Value::from(1)).is_err());
}

#[test]
fn signed_literals_work_and_duplicate_names_do_not() {
    let source = format!("{PREFIX}#let offset = studio.param(-2.5, min: -4, max: 0)");
    let parsed = parse(&source);
    assert!(!parsed.parameters.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.parameters[0].value, Value::from(-2.5));
    let duplicate = format!("{PREFIX}#let size = studio.param(1mm)\n#let size = studio.param(2mm)");
    let parsed = parse(&duplicate);
    assert!(parsed.parameters.is_empty());
    assert!(parsed.warnings.iter().any(|warning| warning.contains("Duplicate")));
}

#[test]
fn no_op_preserves_spelling_and_exponents_are_numbers() {
    let source = format!("{PREFIX}#let opacity = studio.param(1.000)\n#let scale = studio.param(1e3)");
    let parsed = parse(&source);
    assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
    assert_eq!(parsed.parameters[1].value, Value::from(1000.0));
    assert_eq!(apply(&source, &parsed.parameters, "opacity", &Value::from(1.0)).unwrap(), source);
    assert_eq!(apply(&source, &parsed.parameters, "scale", &Value::from(1000)).unwrap(), source);
}

#[test]
fn shadowed_aliases_and_parameter_names_are_read_only() {
    let cases = [
        format!("{PREFIX}#let studio = (: )\n#let size = studio.param(2)"),
        format!("{PREFIX}#import \"other.typ\" as studio\n#let size = studio.param(2)"),
        format!("{PREFIX}#let size = studio.param(2)\n#let size = 3"),
        format!("{PREFIX}#let size = 3\n#let size = studio.param(2)"),
    ];
    for source in cases {
        let parsed = parse(&source);
        assert!(parsed.parameters.is_empty(), "{source}");
        assert!(!parsed.warnings.is_empty());
    }
}

#[test]
fn renamed_module_and_named_parameter_imports_work() {
    for source in [
        "#import \"@local/cetz-studio:0.1.0\" as controls\n#let x = controls.param(2)",
        "#import \"@local/cetz-studio:0.1.0\": param as control\n#let x = control(2)",
        "#import \"@preview/cetz-studio:0.1.0\": param\n#let x = param(2)",
    ] {
        let parsed = parse(source);
        assert_eq!(parsed.parameters.len(), 1, "{:?}", parsed.warnings);
        assert_eq!(apply(source, &parsed.parameters, "x", &Value::from(3)).unwrap(), source.replacen("(2)", "(3)", 1));
    }
}

#[test]
fn angles_ratios_and_relative_lengths_preserve_authored_units() {
    for (value, next, expected) in [("30deg", 35.0, "35deg"), ("0.5rad", 0.7, "0.7rad"), ("50%", 60.0, "60%"), ("1.2em", 1.5, "1.5em")] {
        let source = format!("{PREFIX}#let x = studio.param({value})");
        let parsed = parse(&source);
        assert_eq!(parsed.parameters.len(), 1, "{value}: {:?}", parsed.warnings);
        assert_eq!(apply(&source, &parsed.parameters, "x", &Value::from(next)).unwrap(), source.replace(value, expected));
    }
}

#[test]
fn bounds_retain_the_authored_angle_scale() {
    let source = format!("{PREFIX}#let a = studio.param(20deg, min: -30deg, max: 30deg, step: 5deg, kind: \"angle\")");
    let parsed = parse(&source);
    assert_eq!(parsed.parameters.len(), 1, "{:?}", parsed.warnings);
    assert!(apply(&source, &parsed.parameters, "a", &Value::from(21)).is_err());
    assert!(apply(&source, &parsed.parameters, "a", &Value::from(35)).is_err());
    assert!(apply(&source, &parsed.parameters, "a", &Value::from(25)).is_ok());
}

#[test]
fn parenthesized_literals_and_unicode_comments_survive() {
    let source = format!("{PREFIX}// αβ 日本語\n#let x = studio.param(( /* keep */ -2.5 /* too */ ))\n[µ]");
    let parsed = parse(&source);
    assert_eq!(parsed.parameters.len(), 1, "{:?}", parsed.warnings);
    assert_eq!(apply(&source, &parsed.parameters, "x", &Value::from(-3.0)).unwrap(), source.replace("-2.5", "-3"));
}

#[test]
fn stale_discovery_is_rejected_even_for_a_no_op() {
    let source = format!("{PREFIX}#let x = studio.param(2)");
    let parsed = parse(&source);
    assert!(apply(&(source.clone() + " "), &parsed.parameters, "x", &Value::from(2)).is_err());
    assert!(apply(&source.replace("param(2)", "param(3)"), &parsed.parameters, "x", &Value::from(4)).is_err());
}

#[test]
fn automatic_camera_controls_need_no_studio_import() {
    let source = format!("{SCENERY}#scene.render-scene(data, scene.camera(azimuth: -38deg, elevation: 19deg), width: 69mm)");
    let parsed = parse(&source);
    assert_eq!(parsed.parameters.len(), 3, "{:?}", parsed.warnings);
    let camera = parsed.parameters.iter().find(|p| p.origin.as_ref().unwrap().argument == "azimuth").unwrap();
    assert_eq!(camera.unit.as_deref(), Some("deg"));
    assert_eq!(apply(&source, &parsed.parameters, &camera.id, &Value::from(-35)).unwrap(), source.replace("-38deg", "-35deg"));
}

#[test]
fn aliases_named_imports_and_with_presets_keep_provenance() {
    let source = "#import \"@preview/scenery:0.1.0\": camera as view\n#let preset = view.with(azimuth: 30deg)\n#let other = preset\n#other(elevation: 20deg)";
    let parsed = parse(source);
    assert_eq!(parsed.parameters.len(), 2, "{:?}", parsed.warnings);
    assert!(parsed.parameters[0].origin.as_ref().unwrap().preset);
    assert_eq!(parsed.parameters[0].origin.as_ref().unwrap().function, "camera");
    assert!(!parsed.parameters[1].origin.as_ref().unwrap().preset);
}

#[test]
fn scoped_cetz_draw_imports_do_not_leak() {
    let source = "#import \"@preview/cetz:0.5.2\"\n#cetz.canvas(length: 8mm, { import cetz.draw: circle as disk; disk((0, 0), radius: 1) })\n#disk((0, 0), radius: 9)";
    let parsed = parse(source);
    assert_eq!(parsed.parameters.len(), 2, "{:?}", parsed.warnings);
    assert_eq!(parsed.parameters[1].value, Value::from(1.0));
}

#[test]
fn wildcard_draw_imports_use_reviewed_names_only() {
    let source = "#import \"@preview/cetz:0.5.2\" as c\n#c.canvas({ import c.draw: *; circle((0, 0), radius: 2) })";
    let parsed = parse(source);
    assert_eq!(parsed.parameters.len(), 1, "{:?}", parsed.warnings);
    assert_eq!(parsed.parameters[0].origin.as_ref().unwrap().function, "draw.circle");
}

#[test]
fn unknown_wildcard_imports_clear_inherited_provenance() {
    let source = format!("{SCENERY}#import \"unknown.typ\": *\n#scene.camera(azimuth: 30deg)");
    assert!(parse(&source).parameters.is_empty());
}

#[test]
fn local_shadowing_does_not_poison_outer_scope() {
    let source = format!("{SCENERY}#{{ let scene = (:); scene.camera(azimuth: 10deg) }}\n#scene.camera(azimuth: 20deg)");
    let parsed = parse(&source);
    assert_eq!(parsed.parameters.len(), 1, "{:?}", parsed.warnings);
    assert_eq!(parsed.parameters[0].value, Value::from(20.0));
}

#[test]
fn unknown_versions_and_same_named_functions_do_not_grant_controls() {
    for source in [
        "#import \"@preview/scenery:99.0.0\" as s\n#s.camera(azimuth: 30deg)",
        "#import \"scenery.typ\" as s\n#s.camera(azimuth: 30deg)",
        "#let camera(..args) = none\n#camera(azimuth: 30deg)",
    ] {
        assert!(parse(source).parameters.is_empty(), "{source}");
    }
}

#[test]
fn loops_closures_and_branches_are_not_misreported_as_instances() {
    let source = format!("{SCENERY}#let f() = scene.camera(azimuth: 10deg)\n#for i in (1,2) {{ scene.camera(azimuth: 20deg) }}\n#if true {{ scene.camera(azimuth: 25deg) }}\n#scene.camera(azimuth: 30deg)");
    let parsed = parse(&source);
    assert_eq!(parsed.parameters.len(), 1, "{:?}", parsed.warnings);
    assert_eq!(parsed.parameters[0].value, Value::from(30.0));
}

#[test]
fn assignments_invalidate_symbol_provenance() {
    let source = format!("{SCENERY}#let c = scene.camera\n#c = other\n#c(azimuth: 30deg)");
    assert!(parse(&source).parameters.is_empty());
}

#[test]
fn declarations_seal_the_author_selected_surface() {
    let source = format!("{PREFIX}{SCENERY}#let az = studio.param(30deg)\n#scene.camera(azimuth: 50deg, elevation: 20deg)");
    let parsed = parse(&source);
    assert_eq!(parsed.parameters.len(), 1, "{:?}", parsed.warnings);
    assert_eq!(parsed.parameters[0].id, "az");
}

#[test]
fn malformed_declarations_do_not_fall_back_to_implicit_controls() {
    let source = format!("{PREFIX}{SCENERY}#let az = studio.param(10deg + 20deg)\n#scene.camera(azimuth: 50deg)");
    assert!(parse(&source).parameters.is_empty());
}

#[test]
fn spread_and_duplicate_named_arguments_fail_closed() {
    for args in ["azimuth: 30deg, ..config", "azimuth: 30deg, azimuth: 40deg"] {
        let source = format!("{SCENERY}#scene.camera({args})");
        assert!(parse(&source).parameters.is_empty(), "{source}");
    }
}

#[test]
fn computed_display_properties_are_not_evaluated() {
    let source = format!("{SCENERY}#let az = 30deg\n#scene.camera(azimuth: az, elevation: 10deg + 5deg)");
    let parsed = parse(&source);
    assert!(parsed.parameters.is_empty());
    assert_eq!(parsed.warnings.len(), 2);
}

#[test]
fn plotsy_tuples_are_leaf_edits_not_reprinted_expressions() {
    let source = "#import \"@preview/plotsy-3d:0.2.1\": plot-3d-surface as plot\n#plot((x, y) => x*y, scale-dim: (1.00, /* keep */ 2, 3), rotation-matrix: ((1, 0, 0), (0, 1, 0)), axis-label-size: 1.2em, xdomain: (-1, 1))";
    let parsed = parse(source);
    assert_eq!(parsed.parameters.len(), 10, "{:?}", parsed.warnings);
    let parameter = parsed.parameters.iter().find(|p| p.origin.as_ref().unwrap().argument == "scale-dim[1]").unwrap();
    let next = apply(source, &parsed.parameters, &parameter.id, &Value::from(2.5)).unwrap();
    assert_eq!(next, source.replace("/* keep */ 2", "/* keep */ 2.5"));
    assert!(parsed.parameters.iter().all(|p| !p.label.contains("xdomain")));
}

#[test]
fn malformed_or_computed_tuples_are_wholly_read_only() {
    for tuple in ["(1, 2)", "(1, value, 3)", "(1, ..rest)"] {
        let source = format!("#import \"@preview/plotsy-3d:0.2.1\" as p\n#p.plot-3d-surface(f, scale-dim: {tuple})");
        assert!(parse(&source).parameters.is_empty(), "{source}");
    }
}

#[test]
fn maquette_numeric_angles_are_not_conflated_with_scenery_angles() {
    let source = "#import \"@preview/maquette:0.1.2\": render-obj\n#render-obj(data, azimuth: 30, elevation: 10, width: 4cm)";
    let parsed = parse(source);
    assert_eq!(parsed.parameters.len(), 3, "{:?}", parsed.warnings);
    assert!(parsed.parameters[0].unit.is_none());
    assert_eq!(parsed.parameters[2].unit.as_deref(), Some("cm"));
    assert!(parse(&format!("{SCENERY}#scene.camera(azimuth: 30)")).parameters.is_empty());
}

#[test]
fn fletcher_layout_arguments_preserve_rich_node_content() {
    let source = "#import \"@preview/fletcher:0.5.8\": diagram, node\n#diagram(cell-size: (30mm, 20mm), node((0,0), [$Q_h$], width: 25mm))";
    let parsed = parse(source);
    assert_eq!(parsed.parameters.len(), 3, "{:?}", parsed.warnings);
    let parameter = parsed.parameters.iter().find(|p| p.origin.as_ref().unwrap().argument == "width").unwrap();
    assert_eq!(apply(source, &parsed.parameters, &parameter.id, &Value::from(27)).unwrap(), source.replace("25mm", "27mm"));
}

#[test]
fn parameter_and_traversal_budgets_fail_closed() {
    let source = format!("{PREFIX}{}", (0..257).map(|i| format!("#let p{i} = studio.param(1)\n")).collect::<String>());
    assert!(parse(&source).parameters.is_empty());
    let source = format!("{SCENERY}#{}scene.camera(azimuth: 30deg){}", "[".repeat(150), "]".repeat(150));
    assert!(parse(&source).parameters.is_empty());
}

#[test]
fn syntax_errors_disable_all_controls() {
    assert!(parse(&format!("{SCENERY}#scene.camera(azimuth: 30deg")).parameters.is_empty());
}
