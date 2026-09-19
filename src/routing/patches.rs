//! Route adoption changes coordinate expressions, never regenerates an edge call.
use super::{endpoint, Point, Route};
use crate::{
    edit::{apply_patches, number, Patch},
    model::{Diagram, Edge, Span, Vertex},
};
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use std::collections::HashSet;
use typst_syntax::{SyntaxKind as K, SyntaxNode};

#[derive(Clone, Debug, Serialize)]
pub struct Eligibility {
    pub edge: String,
    pub eligible: bool,
    pub reason: Option<String>,
}

pub fn eligibility(source: &str, diagram: &Diagram, edge: &Edge) -> Result<()> {
    ensure!(
        edge.editable && edge.vertices.len() >= 2,
        "Only direct, non-curved Fletcher edges with literal waypoints are supported"
    );
    let endpoints = [
        edge.vertices.first().unwrap(),
        edge.vertices.last().unwrap(),
    ];
    let mut names = Vec::new();
    for v in endpoints {
        let Vertex::Anchor { name, .. } = v else {
            anyhow::bail!("Both ends must reference named nodes; coordinate/junction inference is not permitted")
        };
        let (node, _) = endpoint(name, diagram)
            .context("Choose a cardinal port (north, south, east, west) or auto")?;
        names.push(node);
    }
    ensure!(
        names[0] != names[1],
        "Self-loops are not supported by the edge-only router"
    );
    ensure!(
        edge.vertices[1..edge.vertices.len() - 1]
            .iter()
            .all(|v| matches!(v, Vertex::Point { point } if point.editable)),
        "Computed, elastic, or intermediate named vertices are read-only"
    );
    // This is intentionally stricter than manual movement. A new routing path
    // cannot preserve arbitrary callbacks, explicit snapping overrides, loops,
    // offsets, multiple strokes, or a generated vertex stream.
    for arg in &edge.call.arguments {
        if let Some(name) = &arg.name {
            ensure!(
                [
                    "stroke",
                    "mark-scale",
                    "label",
                    "label-pos",
                    "label-side",
                    "label-sep",
                    "label-size",
                    "label-angle",
                    "label-anchor",
                    "label-fill",
                    "crossing-fill",
                    "crossing-thickness",
                    "corner-radius",
                ]
                .contains(&name.as_str()),
                "Automatic routing does not support edge option `{name}`"
            );
        } else {
            ensure!(
                arg.kind != K::Spread,
                "Spread/generated edge arguments are read-only"
            );
        }
    }
    let p = edge.call.positional();
    ensure!(
        p.len() >= edge.vertices.len(),
        "Source vertex mapping is incomplete"
    );
    let coordinate_range = p[0].span.start..p[edge.vertices.len() - 1].span.end;
    ensure!(
        !edge.call.arguments.iter().any(|a| a.name.is_some()
            && a.span.start >= coordinate_range.start
            && a.span.end <= coordinate_range.end),
        "Interleaved named options and vertices are not supported"
    );
    ensure!(
        p[edge.vertices.len()..]
            .iter()
            .all(|a| matches!(a.kind, K::Str | K::ContentBlock)),
        "Computed positional edge arguments are read-only"
    );
    // Preserve comments even when removing a waypoint: comments inside its
    // expression cannot be relocated safely, so refuse instead of dropping them.
    for a in &p[1..edge.vertices.len() - 1] {
        ensure!(!has_comment(&source[a.span.clone()]),
            "Waypoint contains an embedded comment; move the comment outside its coordinate expression first");
    }
    if let Some(arg) = edge.call.named("label-pos") {
        ensure!(
            edge.label_position.is_some() && !has_comment(&source[arg.span.clone()]),
            "Computed or commented label positions cannot be remapped"
        );
    }
    Ok(())
}

pub fn eligibility_list(source: &str, diagram: &Diagram) -> Vec<Eligibility> {
    diagram
        .edges
        .iter()
        .map(|edge| {
            let result = eligibility(source, diagram, edge);
            Eligibility {
                edge: edge.id.clone(),
                eligible: result.is_ok(),
                reason: result.err().map(|e| e.to_string()),
            }
        })
        .collect()
}

fn has_comment(expression: &str) -> bool {
    fn contains(node: &SyntaxNode) -> bool {
        matches!(node.kind(), K::LineComment | K::BlockComment) || node.children().any(contains)
    }
    contains(&typst_syntax::parse(&format!("#({expression})")))
}

fn comma_spans(node: &SyntaxNode, offset: usize, out: &mut Vec<Span>) {
    if node.kind() == K::Comma {
        out.push(offset..offset + node.len());
    }
    let mut at = offset;
    for child in node.children() {
        comma_spans(child, at, out);
        at += child.len();
    }
}

fn valid_path(points: &[Point]) -> bool {
    (2..=128).contains(&points.len())
        && points.iter().all(|p| p.valid())
        && points.windows(2).all(|p| {
            p[0].distance(p[1]) > 1e-7
                && ((p[0].x - p[1].x).abs() < 1e-7 || (p[0].y - p[1].y).abs() < 1e-7)
        })
}

/// Internal, typed command payload, never accepted directly from the browser.
/// Patches are collected against one source revision, so a batch is one edit.
pub fn patch_routes(source: &str, diagram: &Diagram, routes: &[Route]) -> Result<String> {
    ensure!(
        !routes.is_empty() && routes.len() <= 32,
        "Route batch must contain 1–32 edges"
    );
    let mut ids = HashSet::new();
    let mut patches = Vec::new();
    let mut commas = Vec::new();
    comma_spans(&typst_syntax::parse(source), 0, &mut commas);
    for route in routes {
        ensure!(ids.insert(&route.edge), "Duplicate route adoption");
        let edge = diagram
            .edges
            .iter()
            .find(|e| e.id == route.edge)
            .context("Unknown routed edge")?;
        eligibility(source, diagram, edge)?;
        ensure!(
            valid_path(&route.points),
            "Invalid orthogonal route proposal"
        );
        let p = edge.call.positional();
        // The endpoint expressions (including their exact ports) remain intact.
        // Remove only interior expressions and their own following commas.
        for i in 1..edge.vertices.len() - 1 {
            patches.push(Patch {
                span: p[i].span.clone(),
                replacement: String::new(),
            });
            let comma = commas
                .iter()
                .filter(|c| c.start >= p[i].span.end && c.end <= p[i + 1].span.start)
                .collect::<Vec<_>>();
            ensure!(
                comma.len() == 1,
                "Cannot map the waypoint delimiter without losing source trivia"
            );
            patches.push(Patch {
                span: comma[0].clone(),
                replacement: String::new(),
            });
        }
        let interior = &route.points[1..route.points.len() - 1];
        if !interior.is_empty() {
            let replacement = interior
                .iter()
                .map(|p| format!(", ({}mm, {}mm)", number(p.x), number(-p.y)))
                .collect::<String>();
            patches.push(Patch {
                span: p[0].span.end..p[0].span.end,
                replacement,
            });
        }
        if let Some([segment, fraction]) = route.label_position {
            ensure!(
                segment.is_finite()
                    && segment >= 0.0
                    && segment.fract() == 0.0
                    && (segment as usize) < route.points.len() - 1
                    && fraction.is_finite()
                    && (0.0..=1.0).contains(&fraction),
                "Invalid remapped label position"
            );
            let arg = edge
                .call
                .named("label-pos")
                .context("Cannot invent a label position")?;
            ensure!(
                arg.items.len() == 2,
                "Only segment-indexed labels require remapping"
            );
            patches.push(Patch {
                span: arg.span.clone(),
                replacement: format!("({}, {})", number(segment), number(fraction)),
            });
        }
    }
    let next = apply_patches(source, patches)?;
    let parsed = crate::model::parse(&next, Some(diagram.y_scale))?;
    ensure!(
        parsed
            .nodes
            .iter()
            .map(|n| &n.id)
            .eq(diagram.nodes.iter().map(|n| &n.id))
            && parsed.edges.len() == diagram.edges.len(),
        "Route adoption changed graph identities"
    );
    Ok(next)
}
