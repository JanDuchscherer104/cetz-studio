//! Static provenance and control discovery. Typst owns parsing, imports at
//! runtime, evaluation and layout; this module only follows verified identities.
use super::adapters::{Library, Rule, Shape};
use super::{line, literal, Parameter, ParameterKind, ParameterOrigin, ParsedParameters};
use anyhow::{ensure, Context, Result};
use std::collections::{BTreeMap, HashSet};
use typst_syntax::{ast, ast::AstNode, Source, SyntaxKind as K, SyntaxNode};

#[derive(Clone)]
struct Symbol {
    library: Library,
    path: String,
}

impl Symbol {
    fn child(&self, field: &str) -> Self {
        Self {
            library: self.library,
            path: if self.path.is_empty() {
                field.to_string()
            } else {
                format!("{}.{field}", self.path)
            },
        }
    }

    fn declaration(&self) -> bool {
        self.library == Library::Studio && self.path == "param"
    }
}

type Environment = BTreeMap<String, Symbol>;

fn resolve(expr: ast::Expr<'_>, environment: &Environment) -> Option<Symbol> {
    match expr {
        ast::Expr::Ident(name) => environment.get(name.as_str()).cloned(),
        ast::Expr::FieldAccess(field) => {
            let parent = resolve(field.target(), environment)?;
            parent
                .library
                .exports(&parent.path)
                .contains(&field.field().as_str())
                .then(|| parent.child(field.field().as_str()))
        }
        ast::Expr::Parenthesized(group) => resolve(group.expr(), environment),
        ast::Expr::FuncCall(call) => {
            let ast::Expr::FieldAccess(field) = call.callee() else {
                return None;
            };
            let target = resolve(field.target(), environment)?;
            // .with is upstream partial application, not an arbitrary wrapper.
            // Studio.param presets would hide bounds; they are not propagated.
            (field.field().as_str() == "with" && !target.library.rules(&target.path).is_empty())
                .then_some(target)
        }
        _ => None,
    }
}

fn imported(import: ast::ModuleImport<'_>, environment: &Environment) -> Option<Symbol> {
    if let ast::Expr::Str(path) = import.source() {
        return Some(Symbol {
            library: Library::from_import(&path.get())?,
            path: String::new(),
        });
    }
    resolve(import.source(), environment)
}

fn import_bindings(
    import: ast::ModuleImport<'_>,
    environment: &Environment,
) -> Vec<(String, Option<Symbol>)> {
    let symbol = imported(import, environment);
    match import.imports() {
        None => import
            .new_name()
            .map(|name| name.as_str().to_string())
            .or_else(|| import.bare_name().ok().map(|name| name.to_string()))
            .map(|name| vec![(name, symbol)])
            .unwrap_or_default(),
        Some(ast::Imports::Items(items)) => items
            .iter()
            .map(|item| {
                let mut current = symbol.clone();
                for field in item.path().iter() {
                    current = current.and_then(|parent| {
                        parent
                            .library
                            .exports(&parent.path)
                            .contains(&field.as_str())
                            .then(|| parent.child(field.as_str()))
                    });
                }
                (item.bound_name().as_str().to_string(), current)
            })
            .collect(),
        Some(ast::Imports::Wildcard) => symbol
            .map(|parent| {
                parent
                    .library
                    .exports(&parent.path)
                    .iter()
                    .map(|name| (name.to_string(), Some(parent.child(name))))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

struct Discovery<'a> {
    source: &'a Source,
    explicit: Vec<Parameter>,
    inferred: Vec<Parameter>,
    warnings: Vec<String>,
    declarations: bool,
    mutations: HashSet<String>,
}

pub(super) fn parse(source: &Source) -> ParsedParameters {
    // Bound traversal and blacklist assignment targets. This is deliberately
    // conservative and never simulates control flow or executes library code.
    let mut mutations = HashSet::new();
    if !scan(source.root(), 0, &mut 0, &mut mutations) {
        return ParsedParameters {
            parameters: vec![],
            warnings: vec!["Control discovery exceeds its syntax traversal budget".into()],
        };
    }
    let mut discovery = Discovery {
        source,
        explicit: vec![],
        inferred: vec![],
        warnings: vec![],
        declarations: false,
        mutations,
    };
    discovery.scope(source.root(), &Environment::new(), true);
    let parameters = if discovery.declarations {
        discovery.explicit
    } else {
        if !discovery.inferred.is_empty() {
            discovery.warnings.push("Library controls edit source occurrences, not independently mapped rendered objects. A preset can affect multiple consumers; per-call overrides may hide it. Use explicit studio.param declarations to restrict the editing surface.".into());
        }
        discovery.inferred
    };
    ParsedParameters {
        parameters,
        warnings: discovery.warnings,
    }
}

fn identifiers(node: &SyntaxNode, output: &mut HashSet<String>) {
    if let Some(name) = node.cast::<ast::Ident>() {
        output.insert(name.as_str().to_string());
    }
    for child in node.children() {
        identifiers(child, output);
    }
}

fn scan(
    node: &SyntaxNode,
    depth: usize,
    count: &mut usize,
    mutations: &mut HashSet<String>,
) -> bool {
    *count += 1;
    if depth > 128 || *count > 100_000 {
        return false;
    }
    if let Some(binary) = node.cast::<ast::Binary>() {
        if matches!(
            binary.op(),
            ast::BinOp::Assign
                | ast::BinOp::AddAssign
                | ast::BinOp::SubAssign
                | ast::BinOp::MulAssign
                | ast::BinOp::DivAssign
        ) {
            identifiers(binary.lhs().to_untyped(), mutations);
        }
    }
    if let Some(assignment) = node.cast::<ast::DestructAssignment>() {
        identifiers(assignment.pattern().to_untyped(), mutations);
    }
    node.children()
        .all(|child| scan(child, depth + 1, count, mutations))
}

impl Discovery<'_> {
    fn warn(&mut self, node: &SyntaxNode, message: impl std::fmt::Display) {
        let at = self
            .source
            .range(node.span())
            .map(|span| line(self.source, &span))
            .unwrap_or(1);
        self.warnings.push(format!("Line {at}: {message}"));
    }

    fn scope(&mut self, node: &SyntaxNode, inherited: &Environment, top: bool) {
        let mut environment = inherited.clone();
        let mut counts = BTreeMap::<String, usize>::new();
        // Collect declarations before discovery so ambiguous/rebound names fail
        // closed, including a parameter redeclared later in the same scope.
        let mut preview = inherited.clone();
        for child in node.children() {
            if let Some(binding) = child.cast::<ast::LetBinding>() {
                for name in binding.kind().bindings() {
                    *counts.entry(name.as_str().to_string()).or_default() += 1;
                }
                if let Some(ast::Expr::FuncCall(call)) = binding.init() {
                    if resolve(call.callee(), &preview).is_some_and(|s| s.declaration()) {
                        self.declarations = true;
                    }
                }
                self.bind(binding, &mut preview, &HashSet::new());
            } else if let Some(import) = child.cast::<ast::ModuleImport>() {
                for (name, _) in import_bindings(import, &preview) {
                    *counts.entry(name).or_default() += 1;
                }
                self.import(import, &mut preview, &HashSet::new());
            }
        }
        let mut blocked = self.mutations.clone();
        let duplicates = counts
            .into_iter()
            .filter_map(|(name, count)| (count > 1).then_some(name))
            .collect::<Vec<_>>();
        if !duplicates.is_empty() {
            self.warn(
                node,
                format!(
                    "Duplicate bindings are read-only: {}",
                    duplicates.join(", ")
                ),
            );
            blocked.extend(duplicates);
        }
        environment.retain(|name, _| !blocked.contains(name));
        for child in node.children() {
            if let Some(import) = child.cast::<ast::ModuleImport>() {
                self.import(import, &mut environment, &blocked);
            } else if let Some(binding) = child.cast::<ast::LetBinding>() {
                self.binding(binding, &environment, &blocked, top);
                self.bind(binding, &mut environment, &blocked);
            } else {
                self.walk(child, &environment);
            }
        }
    }

    fn import(
        &self,
        import: ast::ModuleImport<'_>,
        environment: &mut Environment,
        blocked: &HashSet<String>,
    ) {
        let bindings = import_bindings(import, environment);
        if matches!(import.imports(), Some(ast::Imports::Wildcard)) {
            // We do not enumerate an arbitrary module's complete export set.
            // Clear inherited provenance rather than assume nothing is shadowed.
            environment.clear();
        }
        for (name, symbol) in bindings {
            environment.remove(&name);
            if !blocked.contains(&name) {
                if let Some(symbol) = symbol {
                    environment.insert(name, symbol);
                }
            }
        }
    }

    fn bind(
        &self,
        binding: ast::LetBinding<'_>,
        environment: &mut Environment,
        blocked: &HashSet<String>,
    ) {
        let value = binding.init().and_then(|expr| resolve(expr, environment));
        let names = binding.kind().bindings();
        for name in &names {
            environment.remove(name.as_str());
        }
        if let (
            ast::LetBindingKind::Normal(ast::Pattern::Normal(ast::Expr::Ident(name))),
            Some(value),
        ) = (binding.kind(), value)
        {
            if !blocked.contains(name.as_str()) {
                environment.insert(name.as_str().to_string(), value);
            }
        }
    }

    fn binding(
        &mut self,
        binding: ast::LetBinding<'_>,
        environment: &Environment,
        blocked: &HashSet<String>,
        top: bool,
    ) {
        let Some(init) = binding.init() else {
            return;
        };
        if let ast::Expr::FuncCall(call) = init {
            let symbol = resolve(call.callee(), environment);
            let claimed = symbol.as_ref().is_some_and(Symbol::declaration)
                || matches!(call.callee(), ast::Expr::FieldAccess(field) if field.field().as_str() == "param");
            if claimed {
                self.declarations = true;
                let result = (|| {
                    ensure!(top, "Studio declarations must be direct top-level bindings");
                    ensure!(
                        symbol.is_some_and(|symbol| symbol.declaration()),
                        "Studio declaration import is unknown, shadowed or rebound"
                    );
                    let ast::LetBindingKind::Normal(ast::Pattern::Normal(ast::Expr::Ident(name))) =
                        binding.kind()
                    else {
                        anyhow::bail!("Studio declarations require one binding name");
                    };
                    ensure!(
                        !blocked.contains(name.as_str()),
                        "Duplicate or assigned parameter name is read-only"
                    );
                    explicit(self.source, name.as_str(), call)
                })();
                match result {
                    Ok(parameter) => self.explicit.push(parameter),
                    Err(error) => self.warn(binding.to_untyped(), error),
                }
                return;
            }
        }
        self.walk(init.to_untyped(), environment);
    }

    fn walk(&mut self, node: &SyntaxNode, environment: &Environment) {
        match node.kind() {
            K::Raw
            | K::Closure
            | K::ForLoop
            | K::WhileLoop
            | K::Conditional
            | K::Contextual
            | K::ShowRule
            | K::SetRule => return,
            K::Markup | K::Code => {
                self.scope(node, environment, false);
                return;
            }
            _ => {}
        }
        if let Some(call) = node.cast::<ast::FuncCall>() {
            let (symbol, preset) = match call.callee() {
                ast::Expr::FieldAccess(field) if field.field().as_str() == "with" => {
                    (resolve(field.target(), environment), true)
                }
                callee => (resolve(callee, environment), false),
            };
            if let Some(symbol) = symbol {
                if symbol.declaration() {
                    self.declarations = true;
                    self.warn(
                        node,
                        "studio.param must directly initialize a top-level binding",
                    );
                } else if !symbol.library.rules(&symbol.path).is_empty() {
                    match call_arguments(call) {
                        Ok((_, named)) => {
                            for rule in symbol.library.rules(&symbol.path) {
                                if let Some(expr) = named.get(rule.argument) {
                                    match inferred(self.source, &symbol, rule, *expr, preset) {
                                        Ok(parameters) => self.inferred.extend(parameters),
                                        Err(error) => self.warn(
                                            node,
                                            format!(
                                                "{}.{} is read-only: {error}",
                                                symbol.path, rule.argument
                                            ),
                                        ),
                                    }
                                }
                            }
                        }
                        Err(error) => self.warn(node, error),
                    }
                }
            }
        }
        for child in node.children() {
            self.walk(child, environment);
        }
    }
}

type Arguments<'a> = (Vec<ast::Expr<'a>>, BTreeMap<String, ast::Expr<'a>>);

fn call_arguments(call: ast::FuncCall<'_>) -> Result<Arguments<'_>> {
    let mut positional = vec![];
    let mut named = BTreeMap::new();
    for item in call.args().items() {
        match item {
            ast::Arg::Pos(expr) => positional.push(expr),
            ast::Arg::Named(arg) => {
                ensure!(
                    named
                        .insert(arg.name().as_str().to_string(), arg.expr())
                        .is_none(),
                    "Duplicate arguments are read-only"
                );
            }
            ast::Arg::Spread(_) => anyhow::bail!(
                "Spread arguments are read-only; effective overrides are not inferred"
            ),
        }
    }
    Ok((positional, named))
}

fn explicit(source: &Source, name: &str, call: ast::FuncCall<'_>) -> Result<Parameter> {
    let (positional, named) = call_arguments(call)?;
    ensure!(
        positional.len() == 1,
        "studio.param requires exactly one literal value"
    );
    ensure!(
        named
            .keys()
            .all(|name| ["label", "min", "max", "step", "kind"].contains(&name.as_str())),
        "Unsupported studio.param option"
    );
    let mut parameter = literal(source, positional[0])?;
    parameter.id = name.to_string();
    parameter.label = match named.get("label") {
        Some(ast::Expr::Str(label)) => label.get().to_string(),
        None => name.to_string(),
        _ => anyhow::bail!("Parameter label must be a string literal"),
    };
    ensure!(
        !parameter.label.is_empty() && parameter.label.chars().count() <= 120,
        "Parameter label must contain 1–120 characters"
    );
    if let Some(kind) = named.get("kind") {
        let ast::Expr::Str(kind) = kind else {
            anyhow::bail!("Parameter kind must be a string literal");
        };
        let expected = match parameter.kind {
            ParameterKind::Length => "length",
            ParameterKind::Color => "color",
            ParameterKind::Bool => "bool",
            ParameterKind::Number => match parameter.unit.as_deref() {
                Some("deg" | "rad") => "angle",
                Some("%") => "ratio",
                _ => "number",
            },
        };
        ensure!(
            kind.get().as_str() == expected,
            "Declared kind does not match the literal kind"
        );
    }
    let bound = |name: &str| -> Result<Option<f64>> {
        named
            .get(name)
            .map(|expr| {
                let bound = literal(source, *expr)?;
                ensure!(
                    bound.kind == parameter.kind && bound.unit == parameter.unit,
                    "{name} must use the same unit as the value"
                );
                bound
                    .value
                    .as_f64()
                    .context("Bounds require numeric values")
            })
            .transpose()
    };
    parameter.min = bound("min")?;
    parameter.max = bound("max")?;
    parameter.step = bound("step")?;
    if let (Some(min), Some(max)) = (parameter.min, parameter.max) {
        ensure!(min <= max, "min must not exceed max");
    }
    if let Some(step) = parameter.step {
        ensure!(step > 0.0, "step must be positive");
    }
    if let Some(value) = parameter.value.as_f64() {
        ensure!(
            parameter.min.is_none_or(|min| value >= min),
            "Value is below min"
        );
        ensure!(
            parameter.max.is_none_or(|max| value <= max),
            "Value is above max"
        );
    }
    Ok(parameter)
}

fn array(expr: ast::Expr<'_>, count: usize) -> Result<Vec<ast::Expr<'_>>> {
    let ast::Expr::Array(array) = expr else {
        anyhow::bail!("Expected a literal tuple");
    };
    let items = array
        .items()
        .map(|item| match item {
            ast::ArrayItem::Pos(expr) => Ok(expr),
            _ => anyhow::bail!("Tuple spreads remain source-owned"),
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(items.len() == count, "Unexpected tuple dimensions");
    Ok(items)
}

fn inferred(
    source: &Source,
    symbol: &Symbol,
    rule: &Rule,
    expr: ast::Expr<'_>,
    preset: bool,
) -> Result<Vec<Parameter>> {
    let leaves = match rule.shape {
        Shape::Scalar => vec![(String::new(), expr)],
        Shape::Pair | Shape::Triple => array(
            expr,
            if matches!(rule.shape, Shape::Pair) {
                2
            } else {
                3
            },
        )?
        .into_iter()
        .enumerate()
        .map(|(i, expr)| (format!("[{i}]"), expr))
        .collect(),
        Shape::TwoTriples => {
            let mut leaves = vec![];
            for (i, row) in array(expr, 2)?.into_iter().enumerate() {
                for (j, expr) in array(row, 3)?.into_iter().enumerate() {
                    leaves.push((format!("[{i}][{j}]"), expr));
                }
            }
            leaves
        }
    };
    leaves
        .into_iter()
        .map(|(index, expr)| {
            let mut parameter = literal(source, expr)?;
            ensure!(
                rule.scalar.accepts(&parameter),
                "Literal does not use the reviewed argument type/units"
            );
            let value = parameter
                .value
                .as_f64()
                .context("Expected numeric display property")?;
            ensure!(
                value >= rule.min,
                "Original value is outside the supported display range"
            );
            let argument = format!("{}{index}", rule.argument);
            parameter.id = format!("library:{}:{}:{argument}", parameter.span.start, symbol.path);
            parameter.label = format!(
                "{} · {argument} · line {}{}",
                symbol.path,
                parameter.line,
                if preset { " (preset)" } else { "" }
            );
            parameter.min = Some(rule.min);
            parameter.origin = Some(ParameterOrigin {
                package: symbol.library.package().into(),
                function: symbol.path.clone(),
                argument,
                preset,
            });
            Ok(parameter)
        })
        .collect()
}
