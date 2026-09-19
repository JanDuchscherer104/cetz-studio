//! Validated commands, not replacement files, cross the browser/Rust boundary.
use crate::model::{self, Diagram, Point, Scalar, Span, Vertex};
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    MoveNode {
        id: String,
        x: f64,
        y: f64,
    },
    MoveWaypoint {
        edge: String,
        vertex: usize,
        x: f64,
        y: f64,
    },
    MoveSegment {
        edge: String,
        segment: usize,
        delta: f64,
    },
    SetPort {
        edge: String,
        end: String,
        port: String,
    },
    SetLabel {
        edge: String,
        segment: usize,
        fraction: f64,
    },
    RouteEdges {
        routes: Vec<crate::routing::Route>,
    },
    SetParameter {
        id: String,
        value: Value,
    },
}

#[derive(Clone, Debug)]
pub struct Patch {
    pub span: Span,
    pub replacement: String,
}

fn bounded(value: f64) -> Result<()> {
    ensure!(
        value.is_finite() && value.abs() <= 100_000.0,
        "Coordinate must be finite and within ±100,000 mm"
    );
    Ok(())
}

pub fn number(value: f64) -> String {
    let value = if value.abs() < 0.000_000_5 {
        0.0
    } else {
        value
    };
    let value = format!("{value:.6}");
    value
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn scalar_patch(scalar: &Scalar, mm: f64, patches: &mut Vec<Patch>) -> Result<()> {
    bounded(mm)?;
    ensure!(
        scalar.factor.is_finite() && scalar.factor != 0.0,
        "Invalid source coordinate scale"
    );
    let value = mm / scalar.factor;
    bounded(value)?;
    // A no-op preserves original spelling, exponent notation and whitespace.
    if (value - scalar.value).abs() > 1e-9 {
        patches.push(Patch {
            span: scalar.span.clone(),
            replacement: format!("{}{}", number(value), scalar.unit),
        });
    }
    Ok(())
}

pub(crate) fn point_patches(point: &Point, x: f64, y: f64, patches: &mut Vec<Patch>) -> Result<()> {
    ensure!(point.editable, "Elastic/computed coordinates are read-only");
    let axes = point
        .axes
        .as_ref()
        .context("No literal source coordinates")?;
    scalar_patch(&axes[0], x, patches)?;
    scalar_patch(&axes[1], y, patches)
}

pub fn apply_patches(source: &str, mut patches: Vec<Patch>) -> Result<String> {
    patches.sort_by_key(|p| (p.span.start, p.span.end));
    for p in &patches {
        ensure!(
            p.span.start <= p.span.end && p.span.end <= source.len(),
            "Invalid patch range"
        );
        ensure!(
            source.is_char_boundary(p.span.start) && source.is_char_boundary(p.span.end),
            "Patch splits UTF-8"
        );
    }
    ensure!(
        patches.windows(2).all(|p| p[0].span.end <= p[1].span.start),
        "Overlapping edit ranges"
    );
    let mut output = source.to_string();
    for p in patches.into_iter().rev() {
        output.replace_range(p.span, &p.replacement);
    }
    Ok(output)
}

fn edge<'a>(diagram: &'a Diagram, id: &str) -> Result<&'a model::Edge> {
    let e = diagram
        .edges
        .iter()
        .find(|e| e.id == id)
        .context("Unknown edge")?;
    ensure!(
        e.editable,
        "This edge has unsupported routing and is read-only"
    );
    Ok(e)
}

fn endpoint(name: &str, diagram: &Diagram) -> Option<String> {
    // Longest match allows node IDs containing dots without truncating identity.
    diagram
        .nodes
        .iter()
        .filter(|n| name == n.id || name.starts_with(&format!("{}.", n.id)))
        .max_by_key(|n| n.id.len())
        .map(|n| n.id.clone())
}

pub fn apply(source: &str, diagram: &Diagram, command: &Command) -> Result<String> {
    let mut patches = Vec::new();
    match command {
        Command::MoveNode { id, x, y } => {
            let n = diagram
                .nodes
                .iter()
                .find(|n| &n.id == id)
                .context("Unknown node")?;
            ensure!(n.editable, "This node is read-only");
            point_patches(
                n.position.as_ref().context("Missing node position")?,
                *x,
                *y,
                &mut patches,
            )?;
        }
        Command::MoveWaypoint {
            edge: id,
            vertex,
            x,
            y,
        } => {
            let e = edge(diagram, id)?;
            match e.vertices.get(*vertex).context("Unknown vertex")? {
                Vertex::Point { point } => point_patches(point, *x, *y, &mut patches)?,
                _ => bail!("Named endpoints cannot be dragged away from their nodes"),
            }
        }
        Command::MoveSegment {
            edge: id,
            segment,
            delta,
        } => {
            bounded(*delta)?;
            let e = edge(diagram, id)?;
            let right = segment.checked_add(1).context("Invalid segment")?;
            let (a, b) = match (e.vertices.get(*segment), e.vertices.get(right)) {
                (Some(Vertex::Point { point: a }), Some(Vertex::Point { point: b })) => (a, b),
                _ => bail!("Segment dragging requires two literal waypoints; named endpoints stay attached"),
            };
            if (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() > 1e-6 {
                point_patches(a, a.x + delta, a.y, &mut patches)?;
                point_patches(b, b.x + delta, b.y, &mut patches)?;
            } else if (a.y - b.y).abs() < 1e-6 && (a.x - b.x).abs() > 1e-6 {
                point_patches(a, a.x, a.y + delta, &mut patches)?;
                point_patches(b, b.x, b.y + delta, &mut patches)?;
            } else {
                bail!("Only non-degenerate orthogonal segments can be dragged");
            }
        }
        Command::SetPort {
            edge: id,
            end,
            port,
        } => {
            ensure!(
                [
                    "auto",
                    "north",
                    "south",
                    "east",
                    "west",
                    "north-east",
                    "north-west",
                    "south-east",
                    "south-west"
                ]
                .contains(&port.as_str()),
                "Unsupported port"
            );
            let e = edge(diagram, id)?;
            let v = match end.as_str() {
                "start" => e.vertices.first(),
                "end" => e.vertices.last(),
                _ => bail!("Endpoint must be start or end"),
            }
            .context("Empty edge")?;
            let Vertex::Anchor { name, span } = v else {
                bail!("This endpoint is a literal coordinate, not a named attachment")
            };
            let node = endpoint(name, diagram).context("Cannot resolve endpoint node")?;
            let new_name = if port == "auto" {
                node
            } else {
                format!("{node}.{port}")
            };
            if new_name != *name {
                patches.push(Patch {
                    span: span.clone(),
                    replacement: format!("<{new_name}>"),
                });
            }
        }
        Command::SetLabel {
            edge: id,
            segment,
            fraction,
        } => {
            let e = edge(diagram, id)?;
            ensure!(e.has_label, "Edge has no label to move");
            ensure!(
                *segment < e.vertices.len().saturating_sub(1),
                "Label segment is out of range"
            );
            ensure!(
                fraction.is_finite() && (0.0..=1.0).contains(fraction),
                "Label fraction must be in [0,1]"
            );
            // A numeric source expression is not silently overwritten with its
            // evaluated result. Only absent or literal positions can be edited.
            ensure!(
                e.call.named("label-pos").is_none() || e.label_position.is_some(),
                "Computed label positions are read-only"
            );
            let position = [*segment as f64, *fraction];
            if e.label_position != Some(position) {
                let text = format!("({}, {})", segment, number(*fraction));
                if let Some(arg) = e.call.named("label-pos") {
                    patches.push(Patch {
                        span: arg.span.clone(),
                        replacement: text,
                    });
                } else {
                    let at = e.call.args_open + 1;
                    patches.push(Patch {
                        span: at..at,
                        replacement: format!("label-pos: {text}, "),
                    });
                }
            }
        }
        Command::RouteEdges { routes } => {
            patches.extend(crate::routing::patches(source, diagram, routes)?);
        }
        Command::SetParameter { .. } => bail!("Parameter commands use the parameter interface"),
    }
    let next = apply_patches(source, patches)?;
    let parsed = model::parse(&next, Some(diagram.y_scale))?;
    ensure!(
        parsed
            .nodes
            .iter()
            .map(|n| &n.id)
            .eq(diagram.nodes.iter().map(|n| &n.id)),
        "Node identity changed unexpectedly"
    );
    ensure!(
        parsed.edges.len() == diagram.edges.len(),
        "Edge count changed unexpectedly"
    );
    Ok(next)
}
