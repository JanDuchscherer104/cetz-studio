//! Explicit, source-preserving editor controls.
//!
//! Only direct top-level bindings of the form
//! `#let name = studio.param(literal, ...)` are editable.  This keeps the seam
//! deliberately small: Typst owns computation, while Studio replaces one
//! validated literal span and then asks Typst to compile the result.
use crate::edit::{apply_patches, Patch};
use crate::model::Span;
use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use typst_syntax::{
    ast::{AstNode, LetBinding},
    SyntaxKind as K, SyntaxNode,
};

const MAX_PARAMETERS: usize = 256;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterKind {
    Number,
    Length,
    Color,
    Bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Parameter {
    pub id: String,
    pub label: String,
    pub kind: ParameterKind,
    pub value: Value,
    pub unit: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub line: usize,
    #[serde(skip)]
    span: Span,
}

#[derive(Clone, Debug, Default)]
pub struct ParsedParameters {
    pub parameters: Vec<Parameter>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
struct Argument {
    name: Option<String>,
    span: Span,
    kind: K,
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

fn expressions(node: &SyntaxNode, offset: usize) -> Vec<(&SyntaxNode, Span)> {
    children(node, offset)
        .into_iter()
        .filter(|(n, _)| {
            !n.kind().is_trivia()
                && !matches!(
                    n.kind(),
                    K::LeftParen | K::RightParen | K::Comma | K::Colon | K::Eq | K::Let
                )
        })
        .collect()
}

fn arguments(node: &SyntaxNode, offset: usize, source: &str) -> Result<Vec<Argument>> {
    expressions(node, offset)
        .into_iter()
        .map(|(node, span)| {
            if node.kind() == K::Named {
                let parts = expressions(node, span.start);
                ensure!(
                    parts.len() == 2 && parts[0].0.kind() == K::Ident,
                    "unsupported named argument"
                );
                Ok(Argument {
                    name: Some(source[parts[0].1.clone()].to_string()),
                    span: parts[1].1.clone(),
                    kind: parts[1].0.kind(),
                })
            } else {
                Ok(Argument {
                    name: None,
                    span,
                    kind: node.kind(),
                })
            }
        })
        .collect()
}

fn string(source: &str, span: Span) -> Result<String> {
    serde_json::from_str(&source[span]).context("expected a literal string")
}

fn numeric(source: &str, span: Span) -> Result<(f64, String)> {
    let raw = source[span].trim();
    let (number, unit) = ["mm", "cm", "pt", "in"]
        .into_iter()
        .find_map(|unit| raw.strip_suffix(unit).map(|number| (number, unit)))
        .unwrap_or((raw, ""));
    let value: f64 = number.parse().context("expected a numeric literal")?;
    ensure!(value.is_finite(), "numeric values must be finite");
    Ok((value, unit.to_string()))
}

fn numeric_kind(kind: K) -> bool {
    matches!(kind, K::Int | K::Float | K::Numeric | K::Unary)
}

fn studio_import_is_unambiguous(root: &SyntaxNode, source: &str) -> bool {
    let imports = children(root, 0)
        .into_iter()
        .filter_map(|(node, span)| {
            if node.kind() != K::ModuleImport {
                return None;
            }
            let parts = children(node, span.start);
            let path = parts
                .iter()
                .find(|(n, _)| n.kind() == K::Str)
                .and_then(|(_, s)| string(source, s.clone()).ok());
            let alias = parts
                .iter()
                .rev()
                .find(|(n, _)| n.kind() == K::Ident)
                .map(|(_, s)| &source[s.clone()]);
            if alias != Some("studio") {
                return None;
            }
            Some(path.as_deref().is_some_and(|p| {
                p.ends_with("cetz-studio/lib.typ")
                    || valid_package_path(p, "@preview/cetz-studio:")
                    || valid_package_path(p, "@local/cetz-studio:")
            }))
        })
        .collect::<Vec<_>>();
    imports == [true]
}

fn valid_package_path(path: &str, prefix: &str) -> bool {
    path.strip_prefix(prefix)
        .is_some_and(|version| !version.is_empty() && !version.contains(['/', ' ', '\t', '\n']))
}

fn line(source: &str, offset: usize) -> usize {
    source[..offset].bytes().filter(|&b| b == b'\n').count() + 1
}

fn binding_names(node: &SyntaxNode) -> Vec<String> {
    LetBinding::from_untyped(node)
        .map(|binding| {
            binding
                .kind()
                .bindings()
                .into_iter()
                .map(|ident| ident.as_str().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn parse_binding(node: &SyntaxNode, span: Span, source: &str) -> Result<Option<Parameter>> {
    let parts = expressions(node, span.start);
    let Some((name_node, name_span)) = parts.iter().find(|(n, _)| n.kind() == K::Ident) else {
        return Ok(None);
    };
    let _ = name_node;
    let Some((call, call_span)) = parts.iter().find(|(n, _)| n.kind() == K::FuncCall) else {
        return Ok(None);
    };
    let call_parts = children(call, call_span.start);
    let Some((_callee, callee_span)) = call_parts.iter().find(|(n, _)| n.kind() == K::FieldAccess)
    else {
        return Ok(None);
    };
    if source[callee_span.clone()].trim() != "studio.param" {
        return Ok(None);
    }
    let args_node = call_parts
        .iter()
        .find(|(n, _)| n.kind() == K::Args)
        .context("studio.param requires parentheses")?;
    let args = arguments(args_node.0, args_node.1.start, source)?;
    let positional: Vec<_> = args.iter().filter(|a| a.name.is_none()).collect();
    ensure!(
        positional.len() == 1,
        "studio.param requires exactly one literal value"
    );
    for arg in &args {
        if let Some(name) = &arg.name {
            ensure!(
                ["label", "min", "max", "step", "kind"].contains(&name.as_str()),
                "unsupported studio.param option {name:?}"
            );
        }
    }
    let mut option_names = HashSet::new();
    ensure!(
        args.iter()
            .filter_map(|argument| argument.name.as_deref())
            .all(|name| option_names.insert(name)),
        "studio.param options must not be repeated"
    );
    let named = |name: &str| args.iter().find(|a| a.name.as_deref() == Some(name));
    let id = source[name_span.clone()].to_string();
    ensure!(!id.is_empty(), "parameter name is empty");
    let label = named("label")
        .map(|a| string(source, a.span.clone()))
        .transpose()?
        .unwrap_or_else(|| id.clone());
    ensure!(
        !label.is_empty() && label.chars().count() <= 120,
        "parameter label must contain 1–120 characters"
    );
    let declared_kind = named("kind")
        .map(|a| string(source, a.span.clone()))
        .transpose()?;
    let value_arg = positional[0];
    let (kind, value, unit) = match value_arg.kind {
        kind if numeric_kind(kind) => {
            let (value, unit) = numeric(source, value_arg.span.clone())?;
            let inferred = if unit.is_empty() {
                ParameterKind::Number
            } else {
                ParameterKind::Length
            };
            (
                inferred,
                Value::from(value),
                (!unit.is_empty()).then_some(unit),
            )
        }
        K::Str => {
            let value = string(source, value_arg.span.clone())?;
            ensure!(
                is_hex_color(&value),
                "string parameters must be #rrggbb colors"
            );
            (ParameterKind::Color, Value::from(value), None)
        }
        K::Bool => {
            let value: bool = source[value_arg.span.clone()]
                .parse()
                .context("expected true or false")?;
            (ParameterKind::Bool, Value::from(value), None)
        }
        _ => bail!("parameter value must be a numeric, length, #rrggbb string, or boolean literal"),
    };
    if let Some(declared) = declared_kind {
        let expected = match kind {
            ParameterKind::Number => "number",
            ParameterKind::Length => "length",
            ParameterKind::Color => "color",
            ParameterKind::Bool => "bool",
        };
        ensure!(
            declared == expected,
            "declared kind {declared:?} does not match literal kind {expected:?}"
        );
    }
    let parse_bound = |name: &str| -> Result<Option<f64>> {
        named(name)
            .map(|a| {
                ensure!(
                    matches!(kind, ParameterKind::Number | ParameterKind::Length),
                    "{name} is only valid for numeric parameters"
                );
                ensure!(numeric_kind(a.kind), "{name} must be a numeric literal");
                let (value, bound_unit) = numeric(source, a.span.clone())?;
                ensure!(
                    bound_unit == unit.clone().unwrap_or_default(),
                    "{name} must use the same unit as the value"
                );
                Ok(value)
            })
            .transpose()
    };
    let min = parse_bound("min")?;
    let max = parse_bound("max")?;
    let step = parse_bound("step")?;
    if let (Some(min), Some(max)) = (min, max) {
        ensure!(min <= max, "min must not exceed max");
    }
    let number_value = value.as_f64();
    if let (Some(value), Some(min)) = (number_value, min) {
        ensure!(value >= min, "value is below min");
    }
    if let (Some(value), Some(max)) = (number_value, max) {
        ensure!(value <= max, "value is above max");
    }
    if let Some(step) = step {
        ensure!(step > 0.0, "step must be positive");
    }
    Ok(Some(Parameter {
        id,
        label,
        kind,
        value,
        unit,
        min,
        max,
        step,
        line: line(source, span.start),
        span: value_arg.span.clone(),
    }))
}

pub fn parse(source: &str) -> ParsedParameters {
    let root = typst_syntax::parse(source);
    if root.erroneous() {
        return ParsedParameters {
            parameters: vec![],
            warnings: vec![
                "Parameters are read-only because the Typst source has syntax errors".into(),
            ],
        };
    }
    let direct = children(&root, 0);
    let has_declarations = direct.iter().any(|(node, span)| {
        node.kind() == K::LetBinding && source[span.clone()].contains("studio.param")
    });
    let direct_bindings = direct
        .iter()
        .filter(|(node, _)| node.kind() == K::LetBinding)
        .flat_map(|(node, _)| binding_names(node))
        .fold(HashMap::new(), |mut counts, name| {
            *counts.entry(name).or_insert(0_usize) += 1;
            counts
        });
    if has_declarations
        && (!studio_import_is_unambiguous(&root, source) || direct_bindings.contains_key("studio"))
    {
        return ParsedParameters { parameters: vec![], warnings: vec!["studio.param declarations are read-only: import the CeTZ Studio package exactly once as `studio` and do not rebind that alias".into()] };
    }
    let mut parsed = ParsedParameters::default();
    for (node, span) in direct {
        if node.kind() != K::LetBinding || !source[span.clone()].contains("studio.param") {
            continue;
        }
        match parse_binding(node, span.clone(), source) {
            Ok(Some(parameter)) => parsed.parameters.push(parameter),
            Ok(None) => parsed.warnings.push(format!("Line {}: studio.param declaration is not a direct literal binding and is read-only", line(source, span.start))),
            Err(error) => parsed.warnings.push(format!("Line {}: malformed studio.param declaration is read-only: {error:#}", line(source, span.start))),
        }
    }
    if parsed.parameters.len() > MAX_PARAMETERS {
        parsed.parameters.clear();
        parsed.warnings.push(format!(
            "More than {MAX_PARAMETERS} parameters declared; all controls are read-only"
        ));
    }
    let duplicate_parameters = parsed
        .parameters
        .iter()
        .filter_map(|parameter| {
            (direct_bindings.get(&parameter.id).copied().unwrap_or(0) > 1)
                .then_some(parameter.id.clone())
        })
        .collect::<HashSet<_>>();
    let duplicates = duplicate_parameters.into_iter().collect::<Vec<_>>();
    if !duplicates.is_empty() {
        parsed
            .parameters
            .retain(|parameter| !duplicates.contains(&parameter.id));
        parsed.warnings.push(format!(
            "Duplicate parameter names are read-only: {}",
            duplicates.join(", ")
        ));
    }
    parsed
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn apply(source: &str, parameters: &[Parameter], id: &str, value: &Value) -> Result<String> {
    let parameter = parameters
        .iter()
        .find(|p| p.id == id)
        .context("Unknown or read-only parameter")?;
    let unchanged = match parameter.kind {
        ParameterKind::Number | ParameterKind::Length => {
            matches!((parameter.value.as_f64(), value.as_f64()), (Some(current), Some(next)) if current == next)
        }
        ParameterKind::Color => {
            matches!((parameter.value.as_str(), value.as_str()), (Some(current), Some(next)) if current == next)
        }
        ParameterKind::Bool => {
            matches!((parameter.value.as_bool(), value.as_bool()), (Some(current), Some(next)) if current == next)
        }
    };
    if unchanged {
        return Ok(source.to_string());
    }
    let replacement = match parameter.kind {
        ParameterKind::Number | ParameterKind::Length => {
            let value = value
                .as_f64()
                .context("Numeric parameter requires a JSON number")?;
            ensure!(value.is_finite(), "Parameter value must be finite");
            if let Some(min) = parameter.min {
                ensure!(value >= min, "Parameter value is below min");
            }
            if let Some(max) = parameter.max {
                ensure!(value <= max, "Parameter value is above max");
            }
            if let Some(step) = parameter.step {
                let ratio = (value - parameter.min.unwrap_or(0.0)) / step;
                ensure!(
                    ratio.is_finite() && (ratio - ratio.round()).abs() <= 1e-7,
                    "Parameter value does not align with its declared step"
                );
            }
            format!("{}{}", value, parameter.unit.as_deref().unwrap_or(""))
        }
        ParameterKind::Color => {
            let value = value
                .as_str()
                .context("Color parameter requires a JSON string")?;
            ensure!(is_hex_color(value), "Color must use #rrggbb syntax");
            serde_json::to_string(value)?
        }
        ParameterKind::Bool => value
            .as_bool()
            .context("Boolean parameter requires true or false")?
            .to_string(),
    };
    apply_patches(
        source,
        vec![Patch {
            span: parameter.span.clone(),
            replacement,
        }],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREFIX: &str = "#import \"@preview/cetz-studio:0.1.0\" as studio\n";

    #[test]
    fn parses_and_edits_only_explicit_literals() {
        let source = format!("{PREFIX}#let radius = studio.param(20mm, label: \"Radius\", min: 5mm, max: 50mm, step: 1mm)\n#circle(radius: radius)");
        let parsed = parse(&source);
        assert!(parsed.warnings.is_empty());
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
            assert!(
                parsed.parameters.is_empty(),
                "unexpected editable parameter for {source}"
            );
            assert!(!parsed.warnings.is_empty());
        }
    }

    #[test]
    fn bounds_and_json_types_are_enforced() {
        let source = format!("{PREFIX}#let n = studio.param(2, min: 1, max: 3)\n#let color = studio.param(\"#d8eadd\")\n#let visible = studio.param(true)");
        let parsed = parse(&source);
        assert!(apply(&source, &parsed.parameters, "n", &Value::from(4)).is_err());
        assert!(apply(
            &source,
            &parsed.parameters,
            "color",
            &Value::from("\"; evil")
        )
        .is_err());
        assert!(apply(&source, &parsed.parameters, "visible", &Value::from("true")).is_err());
    }

    #[test]
    fn signed_literals_work_and_duplicate_names_do_not() {
        let signed = format!("{PREFIX}#let offset = studio.param(-2.5, min: -4, max: 0)");
        let parsed = parse(&signed);
        assert!(!parsed.parameters.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.parameters[0].value, Value::from(-2.5));

        let duplicate =
            format!("{PREFIX}#let size = studio.param(1mm)\n#let size = studio.param(2mm)");
        let parsed = parse(&duplicate);
        assert!(parsed.parameters.is_empty());
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("Duplicate")));
    }

    #[test]
    fn no_op_preserves_spelling_and_exponents_are_numbers() {
        let source =
            format!("{PREFIX}#let opacity = studio.param(1.000)\n#let scale = studio.param(1e3)");
        let parsed = parse(&source);
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.parameters[1].value, Value::from(1000.0));
        assert_eq!(
            apply(&source, &parsed.parameters, "opacity", &Value::from(1.0)).unwrap(),
            source
        );
        assert_eq!(
            apply(&source, &parsed.parameters, "scale", &Value::from(1000)).unwrap(),
            source
        );
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
            assert!(
                parsed.parameters.is_empty(),
                "unexpected controls for {source}"
            );
            assert!(!parsed.warnings.is_empty());
        }
    }
}
