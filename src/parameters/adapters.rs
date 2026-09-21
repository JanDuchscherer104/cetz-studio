//! Reviewed argument semantics, not a second parser or renderer. Add an exact
//! package version only with a native fixture; unknown versions stay read-only.
use super::{Parameter, ParameterKind};
use typst_syntax::package::PackageSpec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Library {
    Studio,
    Cetz,
    Fletcher,
    Scenery,
    Plotsy,
    Maquette,
}

impl Library {
    pub fn from_import(path: &str) -> Option<Self> {
        // Retain the explicit declaration convention for local Studio helpers.
        if path.ends_with("cetz-studio/lib.typ") {
            return Some(Self::Studio);
        }
        let spec: PackageSpec = path.parse().ok()?;
        if spec.name.as_str() == "cetz-studio"
            && ["preview", "local"].contains(&spec.namespace.as_str())
            && spec.version.to_string() == "0.1.0"
        {
            return Some(Self::Studio);
        }
        if spec.namespace.as_str() != "preview" {
            return None;
        }
        match (spec.name.as_str(), spec.version.to_string().as_str()) {
            ("cetz", "0.5.2") => Some(Self::Cetz),
            ("fletcher", "0.5.8") => Some(Self::Fletcher),
            ("scenery", "0.1.0") => Some(Self::Scenery),
            ("plotsy-3d", "0.2.1") => Some(Self::Plotsy),
            ("maquette", "0.1.2") => Some(Self::Maquette),
            _ => None,
        }
    }

    pub fn package(self) -> &'static str {
        match self {
            Self::Studio => "cetz-studio:0.1.0",
            Self::Cetz => "cetz:0.5.2",
            Self::Fletcher => "fletcher:0.5.8",
            Self::Scenery => "scenery:0.1.0",
            Self::Plotsy => "plotsy-3d:0.2.1",
            Self::Maquette => "maquette:0.1.2",
        }
    }

    pub fn exports(self, path: &str) -> &'static [&'static str] {
        match (self, path) {
            (Self::Studio, "") => &["param"],
            (Self::Cetz, "") => &["canvas", "draw"],
            (Self::Cetz, "draw") => &["circle", "rect", "line"],
            (Self::Fletcher, "") => &["diagram", "node"],
            (Self::Scenery, "") => &["camera", "render-scene"],
            (Self::Plotsy, "") => &[
                "plot-3d-surface", "plot-3d-parametric-surface",
                "plot-3d-parametric-curve", "plot-3d-vector-field",
            ],
            (Self::Maquette, "") => &["render-obj", "render-stl", "render-ply"],
            _ => &[],
        }
    }

    pub fn rules(self, path: &str) -> &'static [Rule] {
        match (self, path) {
            (Self::Cetz, "canvas") => &[
                Rule::length("length", 0.000001), Rule::number("padding", 0.0),
            ],
            (Self::Cetz, "draw.circle" | "draw.rect") => &[Rule::number("radius", 0.0)],
            (Self::Cetz, "draw.line") => &[Rule::length("stroke", 0.0)],
            (Self::Fletcher, "diagram") => &[
                Rule::length("spacing", 0.0), Rule::length("edge-corner-radius", 0.0),
                Rule::tuple("cell-size", Scalar::Length, Shape::Pair, 0.000001),
            ],
            (Self::Fletcher, "node") => &[
                Rule::length("width", 0.000001), Rule::length("height", 0.000001),
                Rule::length("inset", 0.0), Rule::length("corner-radius", 0.0),
            ],
            (Self::Scenery, "camera") => &[
                Rule::angle("azimuth"), Rule::angle("elevation"), Rule::number("distance", 0.000001),
            ],
            (Self::Scenery, "render-scene") => &[Rule::length("width", 0.000001)],
            (Self::Plotsy, "plot-3d-surface" | "plot-3d-parametric-surface" | "plot-3d-parametric-curve" | "plot-3d-vector-field") => PLOT_RULES,
            (Self::Maquette, "render-obj" | "render-stl" | "render-ply") => &[
                Rule::number("azimuth", -3600.0), Rule::number("elevation", -3600.0),
                Rule::number("distance", 0.000001), Rule::length("width", 0.000001),
                Rule::length("height", 0.000001),
            ],
            _ => &[],
        }
    }
}

const PLOT_RULES: &[Rule] = &[
    Rule::tuple("scale-dim", Scalar::Number, Shape::Triple, 0.000001),
    Rule::tuple("rotation-matrix", Scalar::Number, Shape::TwoTriples, -1000000.0),
    Rule::length("axis-label-size", 0.000001), Rule::length("rear-axis-text-size", 0.000001),
    Rule::length("dot-thickness", 0.0), Rule::length("front-axis-thickness", 0.0),
];

#[derive(Clone, Copy)]
pub(super) enum Scalar { Number, Length, Angle }

impl Scalar {
    pub fn accepts(self, parameter: &Parameter) -> bool {
        match self {
            Self::Number => parameter.kind == ParameterKind::Number && parameter.unit.is_none(),
            Self::Length => parameter.kind == ParameterKind::Length,
            Self::Angle => matches!(parameter.unit.as_deref(), Some("deg" | "rad")),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum Shape { Scalar, Pair, Triple, TwoTriples }

#[derive(Clone, Copy)]
pub(super) struct Rule {
    pub argument: &'static str,
    pub scalar: Scalar,
    pub shape: Shape,
    pub min: f64,
}

impl Rule {
    const fn number(argument: &'static str, min: f64) -> Self {
        Self { argument, scalar: Scalar::Number, shape: Shape::Scalar, min }
    }
    const fn length(argument: &'static str, min: f64) -> Self {
        Self { argument, scalar: Scalar::Length, shape: Shape::Scalar, min }
    }
    const fn angle(argument: &'static str) -> Self {
        Self { argument, scalar: Scalar::Angle, shape: Shape::Scalar, min: -1000000.0 }
    }
    const fn tuple(argument: &'static str, scalar: Scalar, shape: Shape, min: f64) -> Self {
        Self { argument, scalar, shape, min }
    }
}
