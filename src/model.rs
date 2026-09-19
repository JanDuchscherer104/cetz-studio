//! Deliberately bounded Typst adapter, using the real Typst concrete syntax tree.
//! All offsets are UTF-8 byte offsets, including trivia. No regex parser and no
//! whole-document pretty-printer. Unknown expressions remain unmodified.
use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::Serialize;
use std::{collections::HashSet, ops::Range};
use typst_syntax::{SyntaxKind as K, SyntaxNode};

pub const PT_PER_MM: f64 = 72.0 / 25.4;
pub type Span = Range<usize>;

#[derive(Clone, Debug)]
pub struct Scalar {
    pub span: Span,
    pub value: f64,
    pub unit: String,
    /// Millimetres in editor coordinates per source unit. Editor y is down.
    pub factor: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub editable: bool,
    #[serde(skip)]
    pub axes: Option<[Scalar; 2]>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Node {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub position: Option<Point>,
    pub editable: bool,
    pub line: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Vertex {
    Anchor {
        name: String,
        #[serde(skip)]
        span: Span,
    },
    Point {
        point: Point,
    },
}

#[derive(Clone, Debug, Serialize)]
pub struct Edge {
    pub id: String,
    pub vertices: Vec<Vertex>,
    pub has_label: bool,
    pub editable: bool,
    pub label_position: Option<[f64; 2]>,
    pub line: usize,
    #[serde(skip)]
    pub call: Call,
}

#[derive(Clone, Debug)]
pub struct Argument {
    pub name: Option<String>,
    pub span: Span,
    pub kind: K,
    pub items: Vec<(K, Span)>,
}

#[derive(Clone, Debug)]
pub struct Call {
    pub callee: String,
    pub span: Span,
    pub args_open: usize,
    pub arguments: Vec<Argument>,
}
impl Call {
    pub fn positional(&self) -> Vec<&Argument> {
        self.arguments.iter().filter(|a| a.name.is_none()).collect()
    }
    pub fn named(&self, name: &str) -> Option<&Argument> {
        self.arguments
            .iter()
            .find(|a| a.name.as_deref() == Some(name))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Diagram {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub warnings: Vec<String>,
    pub y_scale: f64,
    #[serde(skip)]
    pub call: Call,
}

fn children(node: &SyntaxNode, offset: usize) -> Vec<(&SyntaxNode, Span)> {
    let mut at = offset;
    node.children()
        .map(|child| {
            let span = at..at + child.len();
            at = span.end;
            (child, span)
        })
        .collect()
}

fn expression_children(node: &SyntaxNode, offset: usize) -> Vec<(&SyntaxNode, Span)> {
    children(node, offset)
        .into_iter()
        .filter(|(n, _)| {
            !n.kind().is_trivia()
                && !matches!(n.kind(), K::LeftParen | K::RightParen | K::Comma | K::Colon)
        })
        .collect()
}

fn argument(node: &SyntaxNode, span: Span, source: &str) -> Result<Argument> {
    let (name, value, range) = if node.kind() == K::Named {
        let ch = expression_children(node, span.start);
        ensure!(ch.len() == 2, "Unsupported named argument");
        (
            Some(source[ch[0].1.clone()].to_string()),
            ch[1].0,
            ch[1].1.clone(),
        )
    } else {
        (None, node, span)
    };
    let items = if value.kind() == K::Array {
        expression_children(value, range.start)
            .into_iter()
            .map(|(n, s)| (n.kind(), s))
            .collect()
    } else {
        vec![]
    };
    Ok(Argument {
        name,
        span: range,
        kind: value.kind(),
        items,
    })
}

fn parse_call(node: &SyntaxNode, span: Span, source: &str) -> Result<Option<Call>> {
    if node.kind() != K::FuncCall {
        return Ok(None);
    }
    let all = children(node, span.start);
    let Some((args, args_span)) = all.iter().find(|(n, _)| n.kind() == K::Args) else {
        return Ok(None);
    };
    // Args may be a trailing content block without parentheses. Not our DSL.
    if source.as_bytes().get(args_span.start) != Some(&b'(') {
        return Ok(None);
    }
    let callee = source[span.start..args_span.start].trim().to_string();
    let arguments = expression_children(args, args_span.start)
        .into_iter()
        .map(|(n, s)| argument(n, s, source))
        .collect::<Result<_>>()?;
    Ok(Some(Call {
        callee,
        span,
        args_open: args_span.start,
        arguments,
    }))
}

fn collect_calls(
    node: &SyntaxNode,
    offset: usize,
    source: &str,
    calls: &mut Vec<Call>,
) -> Result<()> {
    // Never interpret code examples, a loop's body, or a function definition as
    // one independently instantiated figure. Direct declarations are supported.
    if matches!(
        node.kind(),
        K::Raw | K::LetBinding | K::Closure | K::ForLoop | K::WhileLoop
    ) {
        return Ok(());
    }
    if let Some(call) = parse_call(node, offset..offset + node.len(), source)? {
        calls.push(call);
    }
    for (child, span) in children(node, offset) {
        collect_calls(child, span.start, source, calls)?;
    }
    Ok(())
}

fn numeric(source: &str, span: Span, factor: f64) -> Result<Scalar> {
    let raw = source[span.clone()].trim();
    let (number, unit) = ["mm", "cm", "pt", "in"]
        .into_iter()
        .find_map(|u| raw.strip_suffix(u).map(|v| (v, u)))
        .unwrap_or((raw, ""));
    // A numeric literal (optionally signed), not a computed expression. This
    // intentionally rejects comments embedded in a unary expression.
    let value: f64 = number
        .trim()
        .parse()
        .context("Position must be a numeric literal")?;
    ensure!(value.is_finite(), "Position must be finite");
    Ok(Scalar {
        span,
        value,
        unit: unit.to_string(),
        factor,
    })
}

fn physical_factor(unit: &str) -> Option<f64> {
    match unit {
        "mm" => Some(1.0),
        "cm" => Some(10.0),
        "pt" => Some(1.0 / PT_PER_MM),
        "in" => Some(25.4),
        _ => None,
    }
}

fn scalar_pair(source: &str, x: Span, y: Span, scale: f64) -> Result<Point> {
    ensure!(scale.is_finite() && scale > 0.0, "Unsupported y-scale");
    let ax = numeric(source, x, 1.0)?;
    let ay = numeric(source, y, scale)?;
    ensure!(
        ax.unit.is_empty() && ay.unit.is_empty(),
        "n(x,y) requires unitless millimetre arguments"
    );
    Ok(Point {
        x: ax.value,
        y: ay.value * scale,
        editable: true,
        axes: Some([ax, ay]),
    })
}

fn tuple_point(source: &str, arg: &Argument) -> Option<Point> {
    if arg.kind != K::Array || arg.items.len() != 2 {
        return None;
    }
    let mut x = numeric(source, arg.items[0].1.clone(), 1.0).ok()?;
    let mut y = numeric(source, arg.items[1].1.clone(), 1.0).ok()?;
    match (physical_factor(&x.unit), physical_factor(&y.unit)) {
        (Some(fx), Some(fy)) => {
            x.factor = fx;
            y.factor = -fy; // CeTZ's physical y axis increases upwards.
            Some(Point {
                x: x.value * fx,
                y: -y.value * fy,
                editable: true,
                axes: Some([x, y]),
            })
        }
        _ => Some(Point {
            x: x.value,
            y: y.value,
            editable: false,
            axes: None,
        }),
    }
}

pub fn label_text(source: &str, arg: &Argument) -> Option<String> {
    if arg.kind != K::Label {
        return None;
    }
    let text = source[arg.span.clone()].trim();
    text.strip_prefix('<')?
        .strip_suffix('>')
        .map(str::to_string)
}

fn call_title(source: &str, args: &[&Argument]) -> String {
    args.iter()
        .find(|a| a.kind == K::ContentBlock)
        .map(|a| {
            source[a.span.clone()]
                .trim_matches(['[', ']'])
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(90)
                .collect()
        })
        .unwrap_or_default()
}

fn line(source: &str, offset: usize) -> usize {
    source[..offset].bytes().filter(|&c| c == b'\n').count() + 1
}

fn n_scale(root: &SyntaxNode, source: &str) -> Result<f64> {
    // The only inferred alias is the actual ARIA overview convention:
    // #let n = content-at.with(y-scale: .65). No arbitrary evaluation.
    for (child, span) in children(root, 0) {
        if child.kind() != K::LetBinding {
            continue;
        }
        let parts = children(child, span.start);
        let name = parts.iter().find(|(n, _)| n.kind() == K::Ident);
        if name.is_none_or(|(_, s)| &source[s.clone()] != "n") {
            continue;
        }
        for (value, s) in parts {
            if let Some(call) = parse_call(value, s, source)? {
                if call.callee == "content-at.with" {
                    if let Some(a) = call.named("y-scale") {
                        let scalar = numeric(source, a.span.clone(), 1.0)?;
                        ensure!(
                            scalar.unit.is_empty() && scalar.value > 0.0,
                            "Invalid y-scale"
                        );
                        return Ok(scalar.value);
                    }
                }
            }
        }
    }
    Ok(1.0)
}

pub fn parse(source: &str, scale_override: Option<f64>) -> Result<Diagram> {
    ensure!(
        source.len() <= 2 * 1024 * 1024,
        "Figure source exceeds 2 MiB"
    );
    let root = typst_syntax::parse(source);
    if root.erroneous() {
        bail!("Typst syntax error: {:?}", root.errors());
    }
    // An explicit adapter must not eagerly evaluate an unsupported alias.
    let scale = match scale_override {
        Some(scale) => scale,
        None => n_scale(&root, source)?,
    };
    ensure!(
        scale.is_finite() && scale > 0.0 && scale <= 100.0,
        "Invalid y-scale"
    );
    let mut calls = Vec::new();
    collect_calls(&root, 0, source, &mut calls)?;
    let roots: Vec<_> = calls
        .iter()
        .filter(|c| {
            matches!(
                c.callee.as_str(),
                "graph" | "diagram" | "f.diagram" | "fletcher.diagram" | "studio.diagram"
            )
        })
        .collect();
    ensure!(roots.len() == 1, "Expected exactly one direct graph()/diagram() call, found {}. Split multi-panel figures into standalone sources.", roots.len());
    let graph = (*roots[0]).clone();
    ensure!(
        graph.named("render").is_none(),
        "Custom render callbacks are read-only in this prototype"
    );
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut warnings = Vec::new();
    let mut names = HashSet::new();
    // Only immediate positional graph arguments are source-editable. Nested
    // labels, embedded backbone glyphs and closures are deliberately opaque.
    for arg in graph.arguments.iter().filter(|a| a.name.is_none()) {
        let Some(call) = calls.iter().find(|c| c.span == arg.span) else {
            warnings.push(format!(
                "Line {}: computed graph argument is not editable",
                line(source, arg.span.start)
            ));
            continue;
        };
        let p = call.positional();
        if matches!(
            call.callee.as_str(),
            "edge" | "f.edge" | "fletcher.edge" | "studio.edge"
        ) {
            let mut vertices = Vec::new();
            for a in &p {
                if let Some(name) = label_text(source, a) {
                    vertices.push(Vertex::Anchor {
                        name,
                        span: a.span.clone(),
                    });
                } else if let Some(point) = tuple_point(source, a) {
                    vertices.push(Vertex::Point { point });
                } else {
                    break;
                }
            }
            let editable = vertices.len() >= 2
                && [
                    "bend",
                    "corner",
                    "shift",
                    "extrude",
                    "loop",
                    "label-wrapper",
                ]
                .iter()
                .all(|k| call.named(k).is_none());
            let label = call.named("label-pos");
            let label_position = label.and_then(|a| {
                if a.items.len() == 2 {
                    Some([
                        numeric(source, a.items[0].1.clone(), 1.0).ok()?.value,
                        numeric(source, a.items[1].1.clone(), 1.0).ok()?.value,
                    ])
                } else {
                    Some([0.0, numeric(source, a.span.clone(), 1.0).ok()?.value])
                }
            });
            let has_label =
                p.iter().any(|a| a.kind == K::ContentBlock) || call.named("label").is_some();
            if !editable {
                warnings.push(format!(
                    "Line {}: edge has unsupported routing; source retained",
                    line(source, call.span.start)
                ));
            }
            edges.push(Edge {
                id: format!("e{}", edges.len()),
                vertices,
                has_label,
                editable,
                label_position,
                line: line(source, call.span.start),
                call: call.clone(),
            });
            continue;
        }
        let named = call.named("name");
        let (id_arg, position) = match call.callee.as_str() {
            "n" | "content-at" | "junction" if p.len() >= 3 => {
                let own_scale = if call.callee == "n" { scale } else { 1.0 };
                let own_scale = if let Some(a) = call.named("y-scale") {
                    numeric(source, a.span.clone(), 1.0)
                        .ok()
                        .filter(|n| n.unit.is_empty())
                        .map(|n| n.value)
                        .unwrap_or(f64::NAN)
                } else {
                    own_scale
                };
                (
                    Some(p[2]),
                    scalar_pair(source, p[0].span.clone(), p[1].span.clone(), own_scale).ok(),
                )
            }
            "addition" | "named-entity" | "card" if p.len() >= 2 => {
                (Some(p[1]), tuple_point(source, p[0]))
            }
            "entity" | "node" | "f.node" | "fletcher.node" | "studio.node" | "studio.card"
            | "content-node"
                if !p.is_empty() =>
            {
                (named, tuple_point(source, p[0]))
            }
            _ => {
                warnings.push(format!(
                    "Line {}: unsupported graph constructor {}",
                    line(source, call.span.start),
                    call.callee
                ));
                continue;
            }
        };
        let id = id_arg.and_then(|a| label_text(source, a)).ok_or_else(|| {
            anyhow!(
                "Line {}: editable nodes need a literal <name>",
                line(source, call.span.start)
            )
        })?;
        ensure!(names.insert(id.clone()), "Duplicate node name <{id}>");
        let editable = position
            .as_ref()
            .is_some_and(|p| p.editable && p.x.is_finite() && p.y.is_finite())
            && call.named("enclose").is_none();
        if !editable {
            warnings.push(format!(
                "<{id}> uses elastic/computed coordinates and is read-only"
            ));
        }
        nodes.push(Node {
            id,
            title: call_title(source, &p),
            kind: call.callee.clone(),
            position,
            editable,
            line: line(source, call.span.start),
        });
    }
    ensure!(!nodes.is_empty(), "No named nodes recognized");
    ensure!(
        nodes.len() < 4096 && edges.len() < 256 && edges.iter().all(|e| e.vertices.len() < 256),
        "Diagram exceeds prototype limits"
    );
    if edges
        .iter()
        .any(|e| matches!(e.vertices.first(), Some(Vertex::Point { .. })))
    {
        warnings.push("Some edges start at literal coordinates: moving a nearby node does not move those branch origins.".into());
    }
    Ok(Diagram {
        nodes,
        edges,
        warnings,
        y_scale: scale,
        call: graph,
    })
}
