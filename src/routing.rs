//! Admission and source patches for optional obstacle routing.
//!
//! Search is a disposable browser Worker. This module, not the Worker, owns the
//! source contract: only proven route literals may change; anchors and all other
//! arguments retain their bytes. The session compiles the whole batch once.
use crate::{
    edit::{number, point_patches, Patch},
    model::{Diagram, Edge, Vertex},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const MAX_EDGES: usize = 32;
pub const MAX_POINTS: usize = 96;
pub const MAX_NODES: usize = 80;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub edge: String,
    /// Interior vertices in downward-positive millimetres, never source text.
    pub points: Vec<[f64; 2]>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Endpoint {
    pub node: String,
    pub port: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Capability {
    pub edge: String,
    pub reason: Option<String>,
    pub start: Option<Endpoint>,
    pub end: Option<Endpoint>,
    pub minimum_points: usize,
}

fn endpoint(vertex: &Vertex, diagram: &Diagram) -> Result<Endpoint> {
    let Vertex::Anchor { name, .. } = vertex else {
        anyhow::bail!("Routing requires two named attachment endpoints")
    };
    let node = diagram
        .nodes
        .iter()
        .filter(|node| name == &node.id || name.starts_with(&format!("{}.", node.id)))
        .max_by_key(|node| node.id.len())
        .context("Cannot resolve the attachment node")?;
    let port = name
        .strip_prefix(&node.id)
        .unwrap_or_default()
        .strip_prefix('.')
        .unwrap_or("auto");
    ensure!(
        ["auto", "north", "east", "south", "west"].contains(&port),
        "Only automatic and cardinal ports can be routed; diagonal/custom ports stay manual"
    );
    Ok(Endpoint {
        node: node.id.clone(),
        port: port.to_owned(),
    })
}

fn eligible(source: &str, diagram: &Diagram, edge: &Edge) -> Result<(Endpoint, Endpoint)> {
    ensure!(
        diagram.nodes.len() <= MAX_NODES,
        "Routing supports at most {MAX_NODES} measured nodes"
    );
    ensure!(
        diagram.call.positional().len() == diagram.nodes.len() + diagram.edges.len(),
        "Computed or unsupported graph objects prevent a complete obstacle map"
    );
    ensure!(
        diagram.nodes.iter().all(|node| node.editable),
        "Routing requires fixed literal node positions; computed/elastic nodes stay source-owned"
    );
    ensure!(
        edge.editable,
        "Computed or unsupported edge geometry requires manual routing"
    );
    // A positional mode/direction string can create extra route geometry. Only
    // the proven coordinate prefix is rewritten; unknown arguments are refused.
    let positional = edge.call.positional();
    let tail = &positional[edge.vertices.len()..];
    ensure!(
        tail.iter().all(|arg| {
            arg.kind == typst_syntax::SyntaxKind::ContentBlock
                || (arg.kind == typst_syntax::SyntaxKind::Str
                    && [
                        "\"-\"",
                        "\"--\"",
                        "\"->\"",
                        "\"<-\"",
                        "\"<->\"",
                        "\"-|>\"",
                        "\"<|-\"",
                        "\"<|-|>\"",
                    ]
                    .contains(&source[arg.span.clone()].trim()))
        }),
        "Computed edge arguments or relative routing modes stay source-owned"
    );
    ensure!(
        edge.call.arguments.iter().all(|arg| {
            arg.name.as_ref().is_none_or(|name| {
                [
                    "label",
                    "label-pos",
                    "label-side",
                    "label-angle",
                    "label-sep",
                    "label-anchor",
                    "stroke",
                    "name",
                    "corner-radius",
                    "mark-scale",
                ]
                .contains(&name.as_str())
            })
        }),
        "This edge has geometry options outside the supported routing subset"
    );
    ensure!(
        edge.call.named("label-pos").is_none() || edge.label_position.is_some(),
        "Computed label positions stay source-owned"
    );
    let start = endpoint(edge.vertices.first().context("Missing start")?, diagram)?;
    let end = endpoint(edge.vertices.last().context("Missing end")?, diagram)?;
    ensure!(start.node != end.node, "Self loops require manual routing");
    ensure!(
        edge.vertices.len().saturating_sub(2) <= MAX_POINTS,
        "Too many existing waypoints to route safely"
    );
    ensure!(
        edge.vertices[1..edge.vertices.len() - 1]
            .iter()
            .all(|vertex| matches!(vertex, Vertex::Point { point } if point.editable)),
        "Intermediate anchors and computed/elastic waypoints stay source-owned"
    );
    Ok((start, end))
}

/// Snapshot capabilities are generated by the same admission check used at edit
/// time. The browser adds only measured-geometry availability checks.
pub fn capabilities(source: &str, diagram: Option<&Diagram>, current: bool) -> Vec<Capability> {
    let Some(diagram) = diagram else {
        return Vec::new();
    };
    diagram
        .edges
        .iter()
        .map(|edge| {
            let admitted = if current {
                eligible(source, diagram, edge)
            } else {
                Err(anyhow::anyhow!(
                    "Routing requires a current instrumented single-page preview"
                ))
            };
            let (reason, start, end) = match admitted {
                Ok((start, end)) => (None, Some(start), Some(end)),
                Err(error) => (Some(error.to_string()), None, None),
            };
            Capability {
                edge: edge.id.clone(),
                reason,
                start,
                end,
                minimum_points: edge.vertices.len().saturating_sub(2),
            }
        })
        .collect()
}

pub fn patches(source: &str, diagram: &Diagram, routes: &[Route]) -> Result<Vec<Patch>> {
    ensure!(
        !routes.is_empty() && routes.len() <= MAX_EDGES,
        "Route 1–{MAX_EDGES} edges in one command"
    );
    ensure!(
        routes.iter().map(|route| route.points.len()).sum::<usize>() <= 1536,
        "Route batch exceeds the 1536-waypoint transport limit"
    );
    let mut seen = HashSet::new();
    let mut patches = Vec::new();
    for route in routes {
        ensure!(seen.insert(&route.edge), "Duplicate edge in route batch");
        let edge = diagram
            .edges
            .iter()
            .find(|edge| edge.id == route.edge)
            .context("Unknown route edge")?;
        eligible(source, diagram, edge)?;
        let existing = edge.vertices.len() - 2;
        ensure!(
            !route.points.is_empty()
                && route.points.len() >= existing
                && route.points.len() <= MAX_POINTS,
            "Route must retain existing waypoint literals and contain at most {MAX_POINTS} points"
        );
        ensure!(
            route
                .points
                .iter()
                .flatten()
                .all(|value| value.is_finite() && value.abs() <= 100_000.0),
            "Route coordinates must be finite and within ±100,000 mm"
        );
        ensure!(
            route.points.windows(2).all(|pair| {
                let dx = (pair[0][0] - pair[1][0]).abs();
                let dy = (pair[0][1] - pair[1][1]).abs();
                (dx < 1e-6 && dy >= 1e-6) || (dy < 1e-6 && dx >= 1e-6)
            }),
            "Route segments must be non-degenerate and orthogonal"
        );
        for (vertex, position) in edge.vertices[1..1 + existing].iter().zip(&route.points) {
            if let Vertex::Point { point } = vertex {
                // Reuse the existing literal units and preserve embedded trivia.
                point_patches(point, position[0], position[1], &mut patches)?;
            }
        }
        if route.points.len() > existing {
            let arguments = edge.call.positional();
            let at = arguments[existing].span.end;
            let replacement = route.points[existing..]
                .iter()
                .map(|point| format!(", ({}mm, {}mm)", number(point[0]), number(-point[1])))
                .collect::<String>();
            patches.push(Patch {
                span: at..at,
                replacement,
            });
        }
    }
    Ok(patches)
}
