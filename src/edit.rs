//! Validated commands, not replacement files, cross the browser/Rust boundary.
use crate::model::{self, Diagram, Point, Scalar, Span, TextEncoding, TextField, Vertex};
use anyhow::{bail, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Server-owned, measured proposal. HTTP clients cannot inject a route.
    #[serde(skip_deserializing)]
    ApplyRoutes {
        routes: Vec<crate::routing::Route>,
        geometry: crate::routing::Geometry,
    },
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
    SetNodeText {
        id: String,
        field: String,
        text: String,
    },
    SetEdgeText {
        edge: String,
        field: String,
        text: String,
    },
    SetNodeSource {
        id: String,
        field: String,
        source: String,
    },
    SetEdgeSource {
        edge: String,
        field: String,
        source: String,
    },
    DuplicateNode {
        id: String,
    },
    AddEdge {
        from: String,
        to: String,
        label: Option<String>,
        arrow: String,
    },
    InsertNode {
        primitive: String,
        x: f64,
        y: f64,
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        text: Option<String>,
    },
    DeleteEdge {
        edge: String,
    },
    DeleteNode {
        id: String,
        cascade: bool,
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

fn point_patches(point: &Point, x: f64, y: f64, patches: &mut Vec<Patch>) -> Result<()> {
    ensure!(point.editable, "Elastic/computed coordinates are read-only");
    let axes = point
        .axes
        .as_ref()
        .context("No literal source coordinates")?;
    scalar_patch(&axes[0], x, patches)?;
    scalar_patch(&axes[1], y, patches)
}

fn valid_identifier(id: &str) -> bool {
    let mut chars = id.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        && id.len() <= 120
}

fn safe_plain_text(text: &str) -> Result<()> {
    ensure!(text.len() <= 4096, "Text exceeds 4 KiB");
    ensure!(
        !text
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t')),
        "Text contains an unsupported control character"
    );
    Ok(())
}

fn typst_string(text: &str) -> Result<String> {
    safe_plain_text(text)?;
    let mut encoded = String::with_capacity(text.len() + 2);
    encoded.push('"');
    for character in text.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\t' => encoded.push_str("\\t"),
            other => encoded.push(other),
        }
    }
    encoded.push('"');
    Ok(encoded)
}

fn text_patch(field: &TextField, text: &str, patches: &mut Vec<Patch>) -> Result<()> {
    ensure!(
        field.editable,
        "{}",
        field
            .reason
            .as_deref()
            .unwrap_or("This text field is read-only")
    );
    let span = field
        .span
        .clone()
        .context("Text field has no literal source span")?;
    if text == field.value {
        return Ok(());
    }
    let replacement = match field.encoding {
        TextEncoding::Content | TextEncoding::String => typst_string(text)?,
        TextEncoding::EdgeLabelPositional => format!("label: {}", typst_string(text)?),
        TextEncoding::Unsupported => bail!("This text field is read-only"),
    };
    patches.push(Patch { span, replacement });
    Ok(())
}

fn source_patch(field: &TextField, source: &str, patches: &mut Vec<Patch>) -> Result<()> {
    ensure!(field.source_editable, "This content field is read-only");
    model::validate_argument_source(source)?;
    if source == field.source {
        return Ok(());
    }
    let replacement = if field.positional_edge_label {
        format!("label: {source}")
    } else {
        source.to_string()
    };
    patches.push(Patch {
        span: field
            .raw_span
            .clone()
            .context("Content field has no source span")?,
        replacement,
    });
    Ok(())
}

fn fresh_id(diagram: &Diagram, requested: Option<&str>, base: &str) -> Result<String> {
    if let Some(id) = requested {
        ensure!(
            valid_identifier(id),
            "Node name must use letters, numbers, _, - or . and start with a letter or _"
        );
        ensure!(
            !diagram.nodes.iter().any(|node| node.id == id),
            "Node name already exists"
        );
        return Ok(id.to_string());
    }
    for suffix in 1..=4096 {
        let candidate = if suffix == 1 {
            base.to_string()
        } else {
            format!("{base}-{suffix}")
        };
        if !diagram.nodes.iter().any(|node| node.id == candidate) {
            return Ok(candidate);
        }
    }
    bail!("Could not allocate a unique node name")
}

fn line_indent(source: &str, offset: usize) -> &str {
    let line_start = source[..offset].rfind('\n').map_or(0, |at| at + 1);
    let prefix = &source[line_start..offset];
    &prefix[..prefix.len() - prefix.trim_start().len()]
}

fn insert_after_argument(
    source: &str,
    diagram: &Diagram,
    argument_index: usize,
    expression: &str,
) -> Result<Vec<Patch>> {
    let indent = diagram
        .nodes
        .first()
        .map(|node| line_indent(source, node.call.span.start))
        .filter(|indent| !indent.is_empty())
        .unwrap_or("  ");
    let argument = diagram
        .call
        .arguments
        .get(argument_index)
        .context("Graph insertion point is unavailable")?;
    if let Some(next) = diagram.call.arguments.get(argument_index + 1) {
        return Ok(vec![Patch {
            span: next.outer_span.start..next.outer_span.start,
            replacement: format!("{expression},\n{indent}"),
        }]);
    }
    let has_comma = diagram
        .call
        .commas
        .iter()
        .any(|comma| comma.start >= argument.outer_span.end);
    let mut patches = Vec::new();
    if !has_comma {
        patches.push(Patch {
            span: argument.outer_span.end..argument.outer_span.end,
            replacement: ",".into(),
        });
    }
    patches.push(Patch {
        span: diagram.call.args_close..diagram.call.args_close,
        replacement: format!("\n{indent}{expression},"),
    });
    Ok(patches)
}

fn append_graph_argument(source: &str, diagram: &Diagram, expression: &str) -> Result<Vec<Patch>> {
    let last_positional = diagram
        .call
        .arguments
        .iter()
        .rposition(|argument| argument.name.is_none())
        .context("Graph has no direct positional insertion point")?;
    insert_after_argument(source, diagram, last_positional, expression)
}

fn remove_graph_argument(diagram: &Diagram, span: &Span) -> Result<Vec<Patch>> {
    let index = diagram
        .call
        .arguments
        .iter()
        .position(|argument| argument.outer_span == *span)
        .context("Element is not a direct graph argument")?;
    let limit = diagram
        .call
        .arguments
        .get(index + 1)
        .map_or(diagram.call.args_close, |argument| {
            argument.outer_span.start
        });
    let mut patches = vec![Patch {
        span: span.clone(),
        replacement: String::new(),
    }];
    if let Some(comma) = diagram
        .call
        .commas
        .iter()
        .find(|comma| comma.start >= span.end && comma.end <= limit)
    {
        patches.push(Patch {
            span: comma.clone(),
            replacement: String::new(),
        });
    }
    Ok(patches)
}

fn edge_callee(diagram: &Diagram) -> Result<&'static str> {
    match diagram.call.callee.as_str() {
        "studio.diagram" => Ok("studio.edge"),
        "f.diagram" => Ok("f.edge"),
        "fletcher.diagram" => Ok("fletcher.edge"),
        "graph" | "diagram" => Ok("edge"),
        _ => bail!("This diagram constructor does not support inserting edges"),
    }
}

fn arrow_source(arrow: &str) -> Result<&'static str> {
    match arrow {
        "none" => Ok("-"),
        "forward" => Ok("->"),
        "backward" => Ok("<-"),
        "both" => Ok("<->"),
        _ => bail!("Arrow must be none, forward, backward, or both"),
    }
}

fn studio_imported(diagram: &Diagram) -> bool {
    diagram.import_aliases.get("studio").is_some_and(|path| {
        path.ends_with("cetz-studio/lib.typ")
            || path.starts_with("@preview/cetz-studio:")
            || path.starts_with("@local/cetz-studio:")
    })
}

fn fletcher_constructor(diagram: &Diagram) -> Option<(&'static str, &'static str)> {
    if studio_imported(diagram) {
        return Some(("studio.fletcher.node", "studio.shapes"));
    }
    for (alias, prefix) in [("f", "f"), ("fletcher", "fletcher")] {
        if diagram.import_aliases.get(alias).is_some_and(|path| {
            path.starts_with("@preview/fletcher:") || path.ends_with("fletcher/lib.typ")
        }) {
            return Some((
                if prefix == "f" {
                    "f.node"
                } else {
                    "fletcher.node"
                },
                if prefix == "f" {
                    "f.shapes"
                } else {
                    "fletcher.shapes"
                },
            ));
        }
    }
    None
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
        Command::ApplyRoutes { routes, .. } => {
            return crate::routing::patch_routes(source, diagram, routes)
        }
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
        Command::SetNodeText { id, field, text } => {
            let node = diagram
                .nodes
                .iter()
                .find(|node| &node.id == id)
                .context("Unknown node")?;
            let field = node
                .text_fields
                .iter()
                .find(|candidate| &candidate.id == field)
                .context("Unknown node text field")?;
            text_patch(field, text, &mut patches)?;
        }
        Command::SetEdgeText {
            edge: id,
            field,
            text,
        } => {
            let edge = diagram
                .edges
                .iter()
                .find(|edge| &edge.id == id)
                .context("Unknown edge")?;
            let field = edge
                .text_fields
                .iter()
                .find(|candidate| &candidate.id == field)
                .context("Unknown edge text field")?;
            text_patch(field, text, &mut patches)?;
        }
        Command::SetNodeSource { id, field, source } => {
            let node = diagram
                .nodes
                .iter()
                .find(|node| &node.id == id)
                .context("Unknown node")?;
            let field = node
                .text_fields
                .iter()
                .find(|candidate| &candidate.id == field)
                .context("Unknown node content field")?;
            source_patch(field, source, &mut patches)?;
        }
        Command::SetEdgeSource {
            edge: id,
            field,
            source,
        } => {
            let edge = diagram
                .edges
                .iter()
                .find(|edge| &edge.id == id)
                .context("Unknown edge")?;
            let field = edge
                .text_fields
                .iter()
                .find(|candidate| &candidate.id == field)
                .context("Unknown edge content field")?;
            source_patch(field, source, &mut patches)?;
        }
        Command::DuplicateNode { id } => {
            let node = diagram
                .nodes
                .iter()
                .find(|node| &node.id == id)
                .context("Unknown node")?;
            let point = node
                .position
                .as_ref()
                .context("Node has no source position")?;
            ensure!(
                point.editable,
                "Computed node positions cannot be duplicated safely"
            );
            let copied_id = fresh_id(diagram, None, &format!("{}-copy", node.id))?;
            let mut clone_patches = vec![Patch {
                span: node.name_span.clone(),
                replacement: format!("<{copied_id}>"),
            }];
            point_patches(point, point.x + 10.0, point.y + 10.0, &mut clone_patches)?;
            let call_start = node.call.span.start;
            let call_end = node.call.span.end;
            for patch in &mut clone_patches {
                ensure!(
                    patch.span.start >= call_start && patch.span.end <= call_end,
                    "Node source is not self-contained"
                );
                patch.span = patch.span.start - call_start..patch.span.end - call_start;
            }
            let cloned = apply_patches(&source[call_start..call_end], clone_patches)?;
            let argument_index = diagram
                .call
                .arguments
                .iter()
                .position(|argument| argument.outer_span == node.call.span)
                .context("Node is not a direct graph argument")?;
            patches.extend(insert_after_argument(
                source,
                diagram,
                argument_index,
                &cloned,
            )?);
        }
        Command::AddEdge {
            from,
            to,
            label,
            arrow,
        } => {
            ensure!(
                from != to,
                "Self-edges are not supported in the first authoring release"
            );
            ensure!(
                diagram.nodes.iter().any(|node| &node.id == from),
                "Unknown source node"
            );
            ensure!(
                diagram.nodes.iter().any(|node| &node.id == to),
                "Unknown target node"
            );
            let label = label
                .as_deref()
                .filter(|label| !label.is_empty())
                .map(|label| Ok::<_, anyhow::Error>(format!(", label: {}", typst_string(label)?)))
                .transpose()?
                .unwrap_or_default();
            let expression = format!(
                "{}(<{}>, <{}>, \"{}\"{})",
                edge_callee(diagram)?,
                from,
                to,
                arrow_source(arrow)?,
                label
            );
            patches.extend(append_graph_argument(source, diagram, &expression)?);
        }
        Command::InsertNode {
            primitive,
            x,
            y,
            name,
            text,
        } => {
            bounded(*x)?;
            bounded(*y)?;
            let default_base = match primitive.as_str() {
                "fletcher-rect" => "rectangle",
                "fletcher-ellipse" => "ellipse",
                "fletcher-diamond" => "diamond",
                "studio-node" => "node",
                "studio-card" => "card",
                _ => bail!("Unknown node primitive"),
            };
            let id = fresh_id(diagram, name.as_deref(), default_base)?;
            let label = typst_string(text.as_deref().unwrap_or(default_base))?;
            let pos = format!("({}mm, {}mm)", number(*x), number(-*y));
            let expression = match primitive.as_str() {
                "studio-node" if studio_imported(diagram) => {
                    format!("studio.node({pos}, {label}, name: <{id}>)")
                }
                "studio-card" if studio_imported(diagram) => {
                    format!("studio.card({pos}, {label}, body: [], name: <{id}>)")
                }
                "fletcher-rect" | "fletcher-ellipse" | "fletcher-diamond" => {
                    let (constructor, shapes) = fletcher_constructor(diagram).context(
                        "Fletcher presets need an explicit studio, f, or fletcher import alias",
                    )?;
                    let shape = primitive.strip_prefix("fletcher-").unwrap();
                    format!("{constructor}({pos}, {label}, name: <{id}>, shape: {shapes}.{shape})")
                }
                "studio-node" | "studio-card" => {
                    bail!("Studio presets need an explicit studio import alias")
                }
                _ => bail!("Unknown node primitive"),
            };
            patches.extend(append_graph_argument(source, diagram, &expression)?);
        }
        Command::DeleteEdge { edge: id } => {
            let edge = diagram
                .edges
                .iter()
                .find(|edge| &edge.id == id)
                .context("Unknown edge")?;
            patches.extend(remove_graph_argument(diagram, &edge.call.span)?);
        }
        Command::DeleteNode { id, cascade } => {
            let node = diagram
                .nodes
                .iter()
                .find(|node| &node.id == id)
                .context("Unknown node")?;
            ensure!(
                node.deletable,
                "{}",
                node.delete_reason
                    .as_deref()
                    .unwrap_or("This node cannot be deleted")
            );
            let attached = diagram
                .edges
                .iter()
                .filter(|edge| {
                    edge.vertices.iter().any(|vertex| match vertex {
                        Vertex::Anchor { name, .. } => {
                            endpoint(name, diagram).as_deref() == Some(id)
                        }
                        Vertex::Point { .. } => false,
                    })
                })
                .collect::<Vec<_>>();
            ensure!(
                *cascade || attached.is_empty(),
                "Node has {} attached edge(s); retry with cascade: true to delete them in the same undo step",
                attached.len()
            );
            patches.extend(remove_graph_argument(diagram, &node.call.span)?);
            if *cascade {
                for edge in attached {
                    patches.extend(remove_graph_argument(diagram, &edge.call.span)?);
                }
            }
        }
        Command::SetParameter { .. } => bail!("Parameter commands use the parameter interface"),
    }
    let next = apply_patches(source, patches)?;
    let parsed = model::parse(&next, Some(diagram.y_scale))?;
    match command {
        Command::DuplicateNode { .. } | Command::InsertNode { .. } => {
            ensure!(
                parsed.nodes.len() == diagram.nodes.len() + 1,
                "Node insertion failed"
            );
            ensure!(
                parsed.edges.len() == diagram.edges.len(),
                "Edge count changed unexpectedly"
            );
        }
        Command::AddEdge { .. } => {
            ensure!(
                parsed
                    .nodes
                    .iter()
                    .map(|n| &n.id)
                    .eq(diagram.nodes.iter().map(|n| &n.id)),
                "Node identity changed unexpectedly"
            );
            ensure!(
                parsed.edges.len() == diagram.edges.len() + 1,
                "Edge insertion failed"
            );
        }
        Command::DeleteEdge { .. } => {
            ensure!(
                parsed
                    .nodes
                    .iter()
                    .map(|n| &n.id)
                    .eq(diagram.nodes.iter().map(|n| &n.id)),
                "Node identity changed unexpectedly"
            );
            ensure!(
                parsed.edges.len() + 1 == diagram.edges.len(),
                "Edge deletion failed"
            );
        }
        Command::DeleteNode { id, cascade } => {
            let expected_nodes = diagram
                .nodes
                .iter()
                .filter(|node| &node.id != id)
                .map(|node| &node.id);
            ensure!(
                parsed.nodes.iter().map(|node| &node.id).eq(expected_nodes),
                "Node deletion changed unrelated identities"
            );
            let removed_edges = if *cascade {
                diagram
                    .edges
                    .iter()
                    .filter(|edge| {
                        edge.vertices.iter().any(|vertex| match vertex {
                            Vertex::Anchor { name, .. } => {
                                endpoint(name, diagram).as_deref() == Some(id)
                            }
                            Vertex::Point { .. } => false,
                        })
                    })
                    .count()
            } else {
                0
            };
            ensure!(
                parsed.edges.len() + removed_edges == diagram.edges.len(),
                "Node deletion changed unrelated edges"
            );
        }
        _ => {
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
        }
    }
    Ok(next)
}
