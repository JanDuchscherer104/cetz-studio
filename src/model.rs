//! Deliberately bounded Typst adapter, using the real Typst concrete syntax tree.
//! All offsets are UTF-8 byte offsets, including trivia. No regex parser and no
//! whole-document pretty-printer. Unknown expressions remain unmodified.
use anyhow::{anyhow, bail, ensure, Context, Result};
use serde::Serialize;
use std::{collections::HashMap, collections::HashSet, ops::Range};
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
    pub text_fields: Vec<TextField>,
    pub attached_edges: usize,
    pub deletable: bool,
    pub delete_reason: Option<String>,
    #[serde(skip)]
    pub call: Call,
    #[serde(skip)]
    pub name_span: Span,
}

#[derive(Clone, Debug, Serialize)]
pub struct TextField {
    pub id: String,
    pub value: String,
    pub editable: bool,
    pub reason: Option<String>,
    pub source: String,
    pub source_editable: bool,
    #[serde(skip)]
    pub span: Option<Span>,
    #[serde(skip)]
    pub raw_span: Option<Span>,
    #[serde(skip)]
    pub encoding: TextEncoding,
    #[serde(skip)]
    pub positional_edge_label: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum TextEncoding {
    #[default]
    Unsupported,
    Content,
    String,
    EdgeLabelPositional,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Vertex {
    Anchor {
        name: String,
        #[serde(skip)]
        span: Span,
        #[serde(skip)]
        argument_index: usize,
    },
    Point {
        point: Point,
        #[serde(skip)]
        argument_index: usize,
    },
}

impl Vertex {
    pub fn argument_index(&self) -> usize {
        match self {
            Self::Anchor { argument_index, .. } | Self::Point { argument_index, .. } => {
                *argument_index
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeRoute {
    Polyline,
    Bezier,
}

#[derive(Clone, Debug, Serialize)]
pub struct Edge {
    pub id: String,
    pub vertices: Vec<Vertex>,
    pub has_label: bool,
    pub editable: bool,
    pub waypoint_editable: bool,
    pub waypoint_reason: Option<String>,
    pub route: EdgeRoute,
    pub route_editable: bool,
    pub route_reason: Option<String>,
    pub label_position: Option<[f64; 2]>,
    pub line: usize,
    pub text_fields: Vec<TextField>,
    #[serde(skip)]
    pub call: Call,
}

#[derive(Clone, Debug)]
pub struct Argument {
    pub name: Option<String>,
    pub outer_span: Span,
    pub span: Span,
    pub kind: K,
    pub items: Vec<(K, Span)>,
    pub plain_text: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Call {
    pub callee: String,
    pub span: Span,
    pub args_open: usize,
    pub args_close: usize,
    pub commas: Vec<Span>,
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
    pub insert_primitives: Vec<String>,
    pub insertion_reason: Option<String>,
    pub opaque_references: bool,
    #[serde(skip)]
    pub call: Call,
    #[serde(skip)]
    pub import_aliases: HashMap<String, String>,
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
        (None, node, span.clone())
    };
    let items = if value.kind() == K::Array {
        expression_children(value, range.start)
            .into_iter()
            .map(|(n, s)| (n.kind(), s))
            .collect()
    } else {
        vec![]
    };
    let plain_text = literal_content(value, range.clone(), source);
    Ok(Argument {
        name,
        outer_span: span,
        span: range,
        kind: value.kind(),
        items,
        plain_text,
    })
}

fn literal_content(node: &SyntaxNode, span: Span, source: &str) -> Option<String> {
    if node.kind() != K::ContentBlock {
        return None;
    }
    fn validate(node: &SyntaxNode) -> bool {
        match node.kind() {
            K::ContentBlock | K::Markup => node.children().all(validate),
            K::LeftBracket | K::RightBracket | K::Text | K::Space => true,
            _ => false,
        }
    }
    if !validate(node) {
        return None;
    }
    source[span]
        .trim()
        .strip_prefix('[')?
        .strip_suffix(']')
        .map(str::to_string)
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
    let commas = children(args, args_span.start)
        .into_iter()
        .filter(|(node, _)| node.kind() == K::Comma)
        .map(|(_, span)| span)
        .collect();
    Ok(Some(Call {
        callee,
        span,
        args_open: args_span.start,
        args_close: args_span.end - 1,
        commas,
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

pub fn validate_argument_source(source: &str) -> Result<()> {
    ensure!(!source.is_empty(), "Content source cannot be empty");
    ensure!(source.len() <= 64 * 1024, "Content source exceeds 64 KiB");
    ensure!(
        source.trim() == source,
        "Content source cannot start or end with trivia"
    );
    let prefix = "#probe(";
    let wrapped = format!("{prefix}{source})");
    let root = typst_syntax::parse(&wrapped);
    ensure!(
        !root.erroneous(),
        "Content source is not valid Typst syntax"
    );
    let mut calls = Vec::new();
    collect_calls(&root, 0, &wrapped, &mut calls)?;
    let call = calls
        .iter()
        .find(|call| call.callee == "probe")
        .context("Content source did not parse as an argument")?;
    ensure!(
        call.arguments.len() == 1,
        "Content source must be exactly one Typst expression"
    );
    let argument = &call.arguments[0];
    ensure!(
        argument.name.is_none() && argument.kind != K::Spread,
        "Content source must be a value, not an argument declaration"
    );
    ensure!(
        argument.outer_span == (prefix.len()..prefix.len() + source.len()),
        "Content source must be exactly one Typst expression"
    );
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

fn text_field(source: &str, arg: Option<&Argument>, id: &str) -> TextField {
    let reason = "Contains Typst markup or computed content; rich content stays source-owned";
    let Some(arg) = arg else {
        return TextField {
            id: id.into(),
            value: String::new(),
            editable: false,
            reason: Some("No literal text field is present in this constructor".into()),
            source: String::new(),
            source_editable: false,
            span: None,
            raw_span: None,
            encoding: TextEncoding::Unsupported,
            positional_edge_label: false,
        };
    };
    let raw = source[arg.span.clone()].trim();
    if arg.kind == K::ContentBlock {
        let inner = raw
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
            .unwrap_or("");
        if let Some(value) = &arg.plain_text {
            return TextField {
                id: id.into(),
                value: value.clone(),
                editable: true,
                reason: None,
                source: raw.to_string(),
                source_editable: true,
                span: Some(arg.span.clone()),
                raw_span: Some(arg.span.clone()),
                encoding: TextEncoding::Content,
                positional_edge_label: false,
            };
        }
        return TextField {
            id: id.into(),
            value: inner
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(90)
                .collect(),
            editable: false,
            reason: Some(reason.into()),
            source: raw.to_string(),
            source_editable: true,
            span: None,
            raw_span: Some(arg.span.clone()),
            encoding: TextEncoding::Unsupported,
            positional_edge_label: false,
        };
    }
    if arg.kind == K::Str {
        if let Ok(value) = serde_json::from_str::<String>(raw) {
            return TextField {
                id: id.into(),
                value,
                editable: true,
                reason: None,
                source: raw.to_string(),
                source_editable: true,
                span: Some(arg.span.clone()),
                raw_span: Some(arg.span.clone()),
                encoding: TextEncoding::String,
                positional_edge_label: false,
            };
        }
    }
    TextField {
        id: id.into(),
        value: raw.chars().take(90).collect(),
        editable: false,
        reason: Some(reason.into()),
        source: raw.to_string(),
        source_editable: true,
        span: None,
        raw_span: Some(arg.span.clone()),
        encoding: TextEncoding::Unsupported,
        positional_edge_label: false,
    }
}

fn node_text_fields(source: &str, call: &Call, positional: &[&Argument]) -> Vec<TextField> {
    let content = |index: usize| {
        positional
            .iter()
            .filter(|arg| matches!(arg.kind, K::ContentBlock | K::Str))
            .nth(index)
            .copied()
    };
    match call.callee.as_str() {
        "studio.card" => vec![
            text_field(source, positional.get(1).copied(), "title"),
            text_field(source, call.named("body"), "body"),
        ],
        "n" | "content-at" => vec![
            text_field(source, positional.get(3).copied(), "title"),
            text_field(source, call.named("body"), "body"),
        ],
        "junction" => Vec::new(),
        "node"
        | "f.node"
        | "fletcher.node"
        | "studio.fletcher.node"
        | "studio.node"
        | "content-node" => {
            vec![text_field(source, positional.get(1).copied(), "title")]
        }
        "card" | "addition" | "named-entity" => vec![
            text_field(source, content(0), "title"),
            text_field(source, content(1), "body"),
        ],
        _ => Vec::new(),
    }
}

fn line(source: &str, offset: usize) -> usize {
    source[..offset].bytes().filter(|&c| c == b'\n').count() + 1
}

fn import_aliases(root: &SyntaxNode, source: &str) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let mut ambiguous = HashSet::new();
    for (node, span) in children(root, 0) {
        if node.kind() == K::LetBinding {
            if let Some((_, name_span)) = children(node, span.start)
                .iter()
                .find(|(part, _)| part.kind() == K::Ident)
            {
                let name = source[name_span.clone()].to_string();
                ambiguous.insert(name.clone());
                aliases.remove(&name);
            }
            continue;
        }
        if node.kind() != K::ModuleImport {
            continue;
        }
        let parts = children(node, span.start);
        let Some((_, path_span)) = parts.iter().find(|(part, _)| part.kind() == K::Str) else {
            continue;
        };
        let raw = source[path_span.clone()].trim();
        let Ok(path) = serde_json::from_str::<String>(raw) else {
            continue;
        };
        let Some((_, alias_span)) = parts.iter().rev().find(|(part, _)| part.kind() == K::Ident)
        else {
            continue;
        };
        let alias = source[alias_span.clone()].to_string();
        if ambiguous.contains(&alias) || aliases.contains_key(&alias) {
            ambiguous.insert(alias.clone());
            aliases.remove(&alias);
        } else {
            aliases.insert(alias, path);
        }
    }
    fn collect_bindings(node: &SyntaxNode, names: &mut HashSet<String>) {
        if let Some(binding) = node.cast::<typst_syntax::ast::LetBinding>() {
            names.extend(
                binding
                    .kind()
                    .bindings()
                    .into_iter()
                    .map(|id| id.get().to_string()),
            );
        }
        for child in node.children() {
            collect_bindings(child, names);
        }
    }
    let mut bindings = HashSet::new();
    collect_bindings(root, &mut bindings);
    aliases.retain(|alias, _| !bindings.contains(alias));
    aliases
}

fn insertion_capabilities(aliases: &HashMap<String, String>) -> (Vec<String>, Option<String>) {
    let studio = aliases.iter().any(|(alias, path)| {
        alias == "studio"
            && (path.ends_with("cetz-studio/lib.typ")
                || path.starts_with("@preview/cetz-studio:")
                || path.starts_with("@local/cetz-studio:"))
    });
    let fletcher = aliases.iter().any(|(alias, path)| {
        matches!(alias.as_str(), "f" | "fletcher")
            && (path.starts_with("@preview/fletcher:") || path.ends_with("fletcher/lib.typ"))
    });
    let mut primitives = Vec::new();
    if studio {
        primitives.extend(["studio-node", "studio-card"]);
        primitives.extend(["fletcher-rect", "fletcher-ellipse", "fletcher-diamond"]);
    } else if fletcher {
        primitives.extend(["fletcher-rect", "fletcher-ellipse", "fletcher-diamond"]);
    }
    let reason = primitives.is_empty().then(|| {
        "Node insertion needs an explicit Cetz Studio or Fletcher import alias; CeTZ canvas insertion is not supported".into()
    });
    (primitives.into_iter().map(str::to_string).collect(), reason)
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
    let import_aliases = import_aliases(&root, source);
    let (insert_primitives, insertion_reason) = insertion_capabilities(&import_aliases);
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
    let mut opaque_references = false;
    let mut names = HashSet::new();
    // Only immediate positional graph arguments are source-editable. Nested
    // labels, embedded backbone glyphs and closures are deliberately opaque.
    for arg in graph.arguments.iter().filter(|a| a.name.is_none()) {
        let Some(call) = calls.iter().find(|c| c.span == arg.span) else {
            opaque_references = true;
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
                let argument_index = call
                    .arguments
                    .iter()
                    .position(|candidate| candidate.outer_span == a.outer_span)
                    .context("Edge vertex is not a direct call argument")?;
                if let Some(name) = label_text(source, a) {
                    vertices.push(Vertex::Anchor {
                        name,
                        span: a.span.clone(),
                        argument_index,
                    });
                } else if let Some(point) = tuple_point(source, a) {
                    vertices.push(Vertex::Point {
                        point,
                        argument_index,
                    });
                } else {
                    break;
                }
            }
            // The unparsed tail may contain generated vertices rather than a
            // label. Such references cannot safely participate in deletion.
            opaque_references |= call.named("vertices").is_some()
                || p[vertices.len()..]
                    .iter()
                    .any(|arg| !matches!(arg.kind, K::Str | K::ContentBlock));
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
            let route = match call.named("route") {
                None => EdgeRoute::Polyline,
                Some(argument) => {
                    let raw = source[argument.span.clone()].trim();
                    match serde_json::from_str::<String>(raw).ok().as_deref() {
                        Some("bezier") => EdgeRoute::Bezier,
                        Some("polyline") => EdgeRoute::Polyline,
                        _ => {
                            warnings.push(format!(
                                "Line {}: route must be the literal string \"polyline\" or \"bezier\"",
                                line(source, call.span.start)
                            ));
                            EdgeRoute::Polyline
                        }
                    }
                }
            };
            let waypoint_reason = (!editable).then(|| {
                "Waypoint editing requires direct literal vertices without computed routing options"
                    .into()
            });
            let studio_edge = call.callee == "studio.edge" && graph.callee == "studio.diagram";
            let valid_controls = vertices.len() <= 4
                && vertices
                    .iter()
                    .skip(1)
                    .take(vertices.len().saturating_sub(2))
                    .all(|vertex| matches!(vertex, Vertex::Point { point, .. } if point.editable));
            let literal_route = call.named("route").is_none_or(|argument| {
                matches!(
                    source[argument.span.clone()].trim(),
                    "\"polyline\"" | "\"bezier\""
                )
            });
            let named_ends = matches!(vertices.first(), Some(Vertex::Anchor { .. }))
                && matches!(vertices.last(), Some(Vertex::Anchor { .. }));
            let route_editable = editable
                && studio_edge
                && valid_controls
                && literal_route
                && named_ends
                && call.named("kind").is_none();
            let route_reason = (!route_editable).then(|| {
                if !studio_edge {
                    "Bezier routing requires studio.edge inside studio.diagram".into()
                } else if !editable {
                    "Bezier routing is unavailable with computed or custom routing options".into()
                } else {
                    "Bezier routing needs named endpoints, one or two literal controls, and no computed route or custom kind".into()
                }
            });
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
            let label_arg = call.named("label").or_else(|| {
                p.iter()
                    .skip(vertices.len())
                    .find(|arg| arg.kind != K::Str)
                    .copied()
            });
            let has_label = label_arg.is_some();
            if !editable {
                warnings.push(format!(
                    "Line {}: edge has unsupported routing; source retained",
                    line(source, call.span.start)
                ));
            }
            let mut text_label = text_field(source, label_arg, "label");
            if label_arg.is_some_and(|argument| argument.name.is_none()) && text_label.editable {
                text_label.encoding = TextEncoding::EdgeLabelPositional;
            }
            text_label.positional_edge_label =
                label_arg.is_some_and(|argument| argument.name.is_none());
            edges.push(Edge {
                id: format!("e{}", edges.len()),
                vertices,
                has_label,
                editable,
                waypoint_editable: editable,
                waypoint_reason,
                route,
                route_editable,
                route_reason,
                label_position,
                line: line(source, call.span.start),
                text_fields: vec![text_label],
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
            "entity"
            | "node"
            | "f.node"
            | "fletcher.node"
            | "studio.fletcher.node"
            | "studio.node"
            | "studio.card"
            | "content-node"
                if !p.is_empty() =>
            {
                (named, tuple_point(source, p[0]))
            }
            _ => {
                opaque_references = true;
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
        let text_fields = node_text_fields(source, call, &p);
        let title = text_fields
            .iter()
            .find(|field| field.id == "title")
            .map(|field| field.value.clone())
            .unwrap_or_else(|| call_title(source, &p));
        nodes.push(Node {
            id,
            title,
            kind: call.callee.clone(),
            position,
            editable,
            line: line(source, call.span.start),
            text_fields,
            attached_edges: 0,
            deletable: false,
            delete_reason: None,
            call: call.clone(),
            name_span: id_arg.expect("validated node name").span.clone(),
        });
    }
    ensure!(!nodes.is_empty(), "No named nodes recognized");
    ensure!(
        nodes.len() < 4096 && edges.len() < 256 && edges.iter().all(|e| e.vertices.len() < 256),
        "Diagram exceeds prototype limits"
    );
    let only_node = nodes.len() == 1;
    let node_ids: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();
    let mut attached = vec![0usize; nodes.len()];
    for edge in &edges {
        let mut seen = HashSet::new();
        for vertex in &edge.vertices {
            if let Vertex::Anchor { name, .. } = vertex {
                let mut candidate = name.as_str();
                loop {
                    if let Some(&index) = node_ids.get(candidate) {
                        seen.insert(index);
                        break;
                    }
                    let Some((prefix, _)) = candidate.rsplit_once('.') else {
                        break;
                    };
                    candidate = prefix;
                }
            }
        }
        for index in seen {
            attached[index] += 1;
        }
    }
    drop(node_ids);
    for (node, count) in nodes.iter_mut().zip(attached) {
        node.attached_edges = count;
        node.delete_reason = if only_node {
            Some("The final recognized node cannot be deleted because the editor requires a non-empty graph".into())
        } else if opaque_references {
            Some("Deletion is unavailable because computed or unsupported graph arguments may reference this node".into())
        } else {
            None
        };
        node.deletable = node.delete_reason.is_none();
    }
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
        insert_primitives,
        insertion_reason,
        opaque_references,
        call: graph,
        import_aliases,
    })
}
