//! Source-preserving controls, discovered with Typst's own typed syntax tree.
//!
//! Explicit Studio declarations define the author-approved editing surface. In
//! files without declarations, versioned library adapters can expose literal
//! display arguments. Both lanes use the same validation, patch and session.
mod adapters;
mod discover;

use crate::edit::{apply_patches, Patch};
use crate::model::Span;
use anyhow::{ensure, Context, Result};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use typst_syntax::{ast, ast::AstNode, Source};

const MAX_PARAMETERS: usize = 256;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterKind {
    /// Numeric magnitude; `unit` also distinguishes angles and percentages.
    Number,
    Length,
    Color,
    Bool,
}

/// A source occurrence, not a claimed mapping to one rendered object.
#[derive(Clone, Debug, Serialize)]
pub struct ParameterOrigin {
    pub package: String,
    pub function: String,
    pub argument: String,
    pub preset: bool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<ParameterOrigin>,
    #[serde(skip)]
    span: Span,
    #[serde(skip)]
    source_hash: [u8; 32],
}

#[derive(Clone, Debug, Default)]
pub struct ParsedParameters {
    pub parameters: Vec<Parameter>,
    pub warnings: Vec<String>,
}

/// Discover permitted controls without reading files or evaluating Typst code.
/// Units are retained in their authored scale. No inverse expression solving,
/// library-source rewriting, or per-instance interpretation is performed.
pub fn parse(source: &str) -> ParsedParameters {
    let syntax = Source::detached(source);
    if syntax.root().erroneous() {
        return ParsedParameters {
            parameters: vec![],
            warnings: vec![
                "Parameters are read-only because the Typst source has syntax errors".into(),
            ],
        };
    }
    let mut parsed = discover::parse(&syntax);
    if parsed.parameters.len() > MAX_PARAMETERS {
        parsed.parameters.clear();
        parsed.warnings.push(format!(
            "More than {MAX_PARAMETERS} parameters discovered; all controls are read-only"
        ));
    }
    let source_hash: [u8; 32] = Sha256::digest(source.as_bytes()).into();
    for parameter in &mut parsed.parameters {
        parameter.source_hash = source_hash;
    }
    // One malformed/generated figure must not flood the editor's diagnostics.
    if parsed.warnings.len() > 32 {
        let omitted = parsed.warnings.len() - 32;
        parsed.warnings.truncate(32);
        parsed.warnings.push(format!("{omitted} further control diagnostics omitted"));
    }
    parsed
}

fn line(source: &Source, span: &Span) -> usize {
    source.text()[..span.start].bytes().filter(|&b| b == b'\n').count() + 1
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Decode only an actual literal node. Parentheses are retained outside the
/// patch; embedded comments in a signed expression are deliberately refused.
fn literal(source: &Source, expr: ast::Expr<'_>) -> Result<Parameter> {
    if let ast::Expr::Parenthesized(group) = expr {
        return literal(source, group.expr());
    }
    let span = source.range(expr.span()).context("Literal has no source range")?;
    let (kind, value, unit) = match expr {
        ast::Expr::Int(_) | ast::Expr::Float(_) | ast::Expr::Numeric(_) | ast::Expr::Unary(_) => {
            let (value, unit) = numeric(expr)?;
            ensure!(value.is_finite(), "Numeric values must be finite");
            let kind = if matches!(unit, Some("mm" | "cm" | "pt" | "in" | "em")) {
                ParameterKind::Length
            } else {
                ParameterKind::Number
            };
            (kind, Value::from(value), unit.map(str::to_string))
        }
        ast::Expr::Bool(value) => (ParameterKind::Bool, Value::from(value.get()), None),
        ast::Expr::Str(value) => {
            let value = value.get().to_string();
            ensure!(is_hex_color(&value), "Only #rrggbb string literals are color controls");
            (ParameterKind::Color, Value::from(value), None)
        }
        _ => anyhow::bail!("Computed arguments remain source-owned; declare their inputs with studio.param"),
    };
    Ok(Parameter {
        id: String::new(),
        label: String::new(),
        kind,
        value,
        unit,
        min: None,
        max: None,
        step: None,
        line: line(source, &span),
        origin: None,
        span,
        source_hash: [0; 32],
    })
}

/// Decode units and magnitudes through Typst, not a parallel number lexer.
fn numeric(expr: ast::Expr<'_>) -> Result<(f64, Option<&'static str>)> {
    match expr {
        ast::Expr::Int(value) => {
            let value = value.get();
            ensure!(value.unsigned_abs() <= (1u64 << 53), "Integer is outside exact JSON numeric control range");
            Ok((value as f64, None))
        }
        ast::Expr::Float(value) => Ok((value.get(), None)),
        ast::Expr::Numeric(value) => {
            let (value, unit) = value.get();
            let unit = match unit {
                ast::Unit::Mm => "mm",
                ast::Unit::Cm => "cm",
                ast::Unit::Pt => "pt",
                ast::Unit::In => "in",
                ast::Unit::Em => "em",
                ast::Unit::Deg => "deg",
                ast::Unit::Rad => "rad",
                ast::Unit::Percent => "%",
                ast::Unit::Fr => anyhow::bail!("Fractional layout units are not editable controls"),
            };
            Ok((value, Some(unit)))
        }
        ast::Expr::Unary(unary) => {
            ensure!(unary.to_untyped().children().all(|child| !matches!(child.kind(), typst_syntax::SyntaxKind::BlockComment | typst_syntax::SyntaxKind::LineComment)), "Comments inside a signed literal remain source-owned");
            ensure!(matches!(unary.expr(), ast::Expr::Int(_) | ast::Expr::Float(_) | ast::Expr::Numeric(_)), "Only a sign applied directly to a numeric literal is editable");
            let (value, unit) = numeric(unary.expr())?;
            match unary.op() {
                ast::UnOp::Pos => Ok((value, unit)),
                ast::UnOp::Neg => Ok((-value, unit)),
                ast::UnOp::Not => anyhow::bail!("Computed boolean values remain source-owned"),
            }
        }
        _ => anyhow::bail!("Expected a numeric literal, not computation"),
    }
}

fn validate_number(parameter: &Parameter, value: f64) -> Result<()> {
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
    Ok(())
}

/// Apply exactly one discovered literal edit. Stale discovery is rejected even
/// outside Session; no-op edits preserve original spelling and comments.
pub fn apply(source: &str, parameters: &[Parameter], id: &str, value: &Value) -> Result<String> {
    let parameter = parameters
        .iter()
        .find(|parameter| parameter.id == id)
        .context("Unknown or read-only parameter")?;
    let current_hash: [u8; 32] = Sha256::digest(source.as_bytes()).into();
    ensure!(current_hash == parameter.source_hash, "Stale parameter source; rediscover controls before editing");
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
            let value = value.as_f64().context("Numeric parameter requires a JSON number")?;
            validate_number(parameter, value)?;
            format!("{}{}", value, parameter.unit.as_deref().unwrap_or(""))
        }
        ParameterKind::Color => {
            let value = value.as_str().context("Color parameter requires a JSON string")?;
            ensure!(is_hex_color(value), "Color must use #rrggbb syntax");
            serde_json::to_string(value)?
        }
        ParameterKind::Bool => value
            .as_bool()
            .context("Boolean parameter requires true or false")?
            .to_string(),
    };
    apply_patches(source, vec![Patch { span: parameter.span.clone(), replacement }])
}
