use cetz_studio::{
    edit::{self, Command},
    model,
    routing::{self, Geometry, Limits, MeasuredEdge, MeasuredNode, Options, Point, Rect, Side},
};
use std::time::Duration;

fn point(x: f64, y: f64) -> Point {
    Point { x, y }
}
fn node(id: &str, x: f64, y: f64) -> MeasuredNode {
    MeasuredNode {
        id: id.into(),
        bounds: Rect {
            min: point(x - 10., y - 10.),
            max: point(x + 10., y + 10.),
        },
        ports: vec![
            point(x + 10., y),
            point(x, y + 10.),
            point(x - 10., y),
            point(x, y - 10.),
        ],
    }
}
fn fixture() -> (String, model::Diagram, Geometry) {
    let source = "#graph(n(0,0,<a>,[α $x$]), n(50,0,<obstacle>,[fixed]), n(100,0,<b>,[β]), edge(<a.east>, (25mm, 0mm), /* keep , this */ <b.west>, \"-|>\", [$x in RR^d$], label-pos:(1,.7), stroke:blue))".to_string();
    let diagram = model::parse(&source, None).unwrap();
    let geometry = Geometry {
        nodes: vec![
            node("a", 0., 0.),
            node("obstacle", 50., 0.),
            node("b", 100., 0.),
        ],
        edges: vec![MeasuredEdge {
            points: vec![point(10., 0.), point(25., 0.), point(90., 0.)],
            width_mm: 0.4,
            corner_mm: 1.,
            kind: "poly".into(),
        }],
    };
    (source, diagram, geometry)
}
fn route(source: &str, d: &model::Diagram, g: &Geometry) -> routing::Plan {
    routing::plan(
        source,
        d,
        g,
        &["e0".into()],
        &Options::default(),
        Limits::default(),
    )
    .unwrap()
}
#[test]
fn routes_around_obstacle_with_explicit_ports_and_clearance() {
    let (s, d, g) = fixture();
    let p = route(&s, &d, &g);
    let r = &p.routes[0];
    assert_eq!(r.points[0], point(10., 0.));
    assert_eq!(*r.points.last().unwrap(), point(90., 0.));
    assert!(r.points[1].x > r.points[0].x);
    assert!(r.points[r.points.len() - 2].x < 90.);
    for pair in r.points.windows(2) {
        assert!(pair[0].x == pair[1].x || pair[0].y == pair[1].y);
        for k in 0..=200 {
            let t = k as f64 / 200.;
            let x = pair[0].x * (1. - t) + pair[1].x * t;
            let y = pair[0].y * (1. - t) + pair[1].y * t;
            assert!(!(x > 38. && x < 62. && y > -12. && y < 12.));
        }
    }
}
#[test]
fn patches_only_route_expressions_and_preserves_comments_labels_and_nodes() {
    let (s, d, g) = fixture();
    let p = route(&s, &d, &g);
    let next = routing::patch_routes(&s, &d, &p.routes).unwrap();
    assert_eq!(s.split("edge(").next(), next.split("edge(").next());
    for text in [
        "/* keep , this */",
        "[$x in RR^d$]",
        "\"-|>\"",
        "stroke:blue",
        "<a.east>",
        "<b.west>",
    ] {
        assert!(next.contains(text));
    }
    let parsed = model::parse(&next, None).unwrap();
    assert_eq!(parsed.edges[0].vertices.len(), p.routes[0].points.len());
    assert!(p.routes[0].label_position.is_some());
}
#[test]
fn preserves_scalar_label_position_spelling() {
    let (s, _, mut g) = fixture();
    let s = s.replace("(1,.7)", ".70000");
    let d = model::parse(&s, None).unwrap();
    g.edges[0].corner_mm = 0.;
    let p = route(&s, &d, &g);
    assert_eq!(p.routes[0].label_position, None);
    assert!(routing::patch_routes(&s, &d, &p.routes)
        .unwrap()
        .contains("label-pos:.70000"));
}
#[test]
fn bounds_and_budget_exhaustion_return_errors_without_patches() {
    let (s, d, g) = fixture();
    for limits in [
        Limits {
            expansions: 0,
            ..Limits::default()
        },
        Limits {
            grid_points: 1,
            ..Limits::default()
        },
        Limits {
            elapsed: Duration::ZERO,
            ..Limits::default()
        },
        Limits {
            nodes: 2,
            ..Limits::default()
        },
    ] {
        assert!(routing::plan(&s, &d, &g, &["e0".into()], &Options::default(), limits).is_err());
    }
}
#[test]
fn blocked_port_and_enclosed_start_do_not_create_partial_paths() {
    let (s, d, mut g) = fixture();
    g.nodes.push(node("blocker", 20., 0.));
    assert!(routing::plan(
        &s,
        &d,
        &g,
        &["e0".into()],
        &Options::default(),
        Limits::default()
    )
    .unwrap_err()
    .to_string()
    .contains("blocked"));
}
#[test]
fn unsupported_source_is_explained_not_overridden() {
    let (s, _, g) = fixture();
    for (from, to) in [
        ("<a.east>", "<a.north-east>"),
        ("<a.east>", "(0mm,0mm)"),
        ("25mm", "offset"),
        ("stroke:blue", "snap-to:none"),
        ("stroke:blue", "shift:2mm"),
        ("(1,.7)", "computed"),
        ("<b.west>", "<a.west>"),
        ("(25mm, 0mm)", "(25mm /* embedded */, 0mm)"),
    ] {
        let source = s.replace(from, to);
        let d = model::parse(&source, None).unwrap();
        assert!(
            routing::plan(
                &source,
                &d,
                &g,
                &["e0".into()],
                &Options::default(),
                Limits::default()
            )
            .is_err(),
            "{source}"
        );
    }
}
#[test]
fn routing_command_cannot_be_forged_over_http() {
    assert!(serde_json::from_value::<Command>(
        serde_json::json!({"kind":"apply_routes","routes":[],"geometry":{}})
    )
    .is_err());
}
#[test]
fn no_clearance_nan_duplicates_or_unbounded_batch() {
    let (s, d, g) = fixture();
    for clearance_mm in [-1., 26., f64::NAN] {
        assert!(routing::plan(
            &s,
            &d,
            &g,
            &["e0".into()],
            &Options { clearance_mm },
            Limits::default()
        )
        .is_err());
    }
    for edges in [
        vec![],
        vec!["e0".into(), "e0".into()],
        vec!["nope".into()],
        vec!["e0".into(); 33],
    ] {
        assert!(routing::plan(&s, &d, &g, &edges, &Options::default(), Limits::default()).is_err());
    }
}
#[test]
fn changed_measured_node_is_not_a_fixed_node_success() {
    let (s, d, g) = fixture();
    let p = route(&s, &d, &g);
    let mut changed = g.clone();
    changed.nodes[1].bounds.min.x += 1.;
    assert!(routing::verify_fixed_geometry(&g, &changed, &p.routes, &d).is_err());
}
#[test]
fn auto_ports_remain_bare_named_source_references() {
    let (s, _, mut g) = fixture();
    let s = s.replace("<a.east>", "<a>").replace("<b.west>", "<b>");
    let d = model::parse(&s, None).unwrap();
    g.edges[0].points[0] = point(0., 0.);
    *g.edges[0].points.last_mut().unwrap() = point(100., 0.);
    let p = route(&s, &d, &g);
    assert_eq!(p.routes[0].start_side, Side::East);
    assert_eq!(p.routes[0].end_side, Side::West);
    let patched = routing::patch_routes(&s, &d, &p.routes).unwrap();
    assert!(patched.contains("edge(<a>,"));
    assert!(patched.contains("<b>"));
}
#[test]
fn batch_adoption_is_one_source_transaction() {
    let (s, _, mut g) = fixture();
    let s = s.replace(
        "stroke:blue))",
        "stroke:blue), edge(<a.south>,<b.south>,\"--|>\",[second]))",
    );
    let d = model::parse(&s, None).unwrap();
    g.edges.push(MeasuredEdge {
        points: vec![point(0., 10.), point(100., 10.)],
        width_mm: 0.4,
        corner_mm: 1.,
        kind: "line".into(),
    });
    let p = routing::plan(
        &s,
        &d,
        &g,
        &["e0".into(), "e1".into()],
        &Options::default(),
        Limits::default(),
    )
    .unwrap();
    let next = edit::apply(
        &s,
        &d,
        &Command::ApplyRoutes {
            routes: p.routes,
            geometry: g,
        },
    )
    .unwrap();
    assert_eq!(model::parse(&next, None).unwrap().edges.len(), 2);
    assert!(next.contains("[second]"));
}

fn crossing_fixture() -> (String, model::Diagram, Geometry) {
    let source = "#graph(\
        n(0,0,<left>,[L]), n(100,0,<right>,[R]),\
        n(50,50,<top>,[T]), n(50,-50,<bottom>,[B]),\
        edge(<left.east>,<right.west>),\
        edge(<top.south>,<bottom.north>)\
    )"
    .to_string();
    let diagram = model::parse(&source, None).unwrap();
    let geometry = Geometry {
        nodes: vec![
            node("left", 0., 0.),
            node("right", 100., 0.),
            node("top", 50., -50.),
            node("bottom", 50., 50.),
        ],
        edges: vec![
            MeasuredEdge {
                points: vec![point(10., 0.), point(90., 0.)],
                width_mm: 0.4,
                corner_mm: 0.,
                kind: "line".into(),
            },
            MeasuredEdge {
                points: vec![point(50., -40.), point(50., 40.)],
                width_mm: 0.4,
                corner_mm: 0.,
                kind: "line".into(),
            },
        ],
    };
    (source, diagram, geometry)
}

#[test]
fn batch_routing_is_selection_order_independent_and_avoids_a_crossing() {
    let (source, diagram, geometry) = crossing_fixture();
    let forward = routing::plan(
        &source,
        &diagram,
        &geometry,
        &["e0".into(), "e1".into()],
        &Options::default(),
        Limits::default(),
    )
    .unwrap();
    let reverse = routing::plan(
        &source,
        &diagram,
        &geometry,
        &["e1".into(), "e0".into()],
        &Options::default(),
        Limits::default(),
    )
    .unwrap();

    assert_eq!(
        serde_json::to_value(&forward.routes).unwrap(),
        serde_json::to_value(&reverse.routes).unwrap()
    );
    assert_eq!(forward.metrics, reverse.metrics);
    assert_eq!(
        forward.metrics.crossings,
        0,
        "{}",
        serde_json::to_string_pretty(&forward.routes).unwrap()
    );
    assert_eq!(forward.metrics.overlaps, 0);
    let vertical = forward
        .routes
        .iter()
        .find(|route| route.edge == "e1")
        .unwrap();
    assert!(vertical
        .points
        .iter()
        .any(|point| point.x < 10. || point.x > 90.));
}

#[test]
fn batch_routing_penalizes_collinear_overlap_and_reports_unavoidable_shared_corridors() {
    let source = "#graph(n(0,0,<a>,[A]), n(100,0,<b>,[B]), \
                  edge(<a.east>,<b.west>), edge(<a.east>,<b.west>))"
        .to_string();
    let diagram = model::parse(&source, None).unwrap();
    let geometry = Geometry {
        nodes: vec![node("a", 0., 0.), node("b", 100., 0.)],
        edges: vec![
            MeasuredEdge {
                points: vec![point(10., 0.), point(90., 0.)],
                width_mm: 0.4,
                corner_mm: 0.,
                kind: "line".into(),
            },
            MeasuredEdge {
                points: vec![point(10., 0.), point(90., 0.)],
                width_mm: 0.4,
                corner_mm: 0.,
                kind: "line".into(),
            },
        ],
    };
    let plan = routing::plan(
        &source,
        &diagram,
        &geometry,
        &["e0".into(), "e1".into()],
        &Options::default(),
        Limits::default(),
    )
    .unwrap();

    let second = plan.routes.iter().find(|route| route.edge == "e1").unwrap();
    assert!(second.points.iter().any(|point| point.y.abs() > 1.));
    assert_eq!(plan.metrics.crossings, 0);
    assert_eq!(plan.metrics.overlaps, 2);
    assert!(plan.metrics.overlap_mm < 25.);
    assert!(plan.metrics.total_length_mm > 160.);
    assert!(plan.metrics.bends >= 2);
}
