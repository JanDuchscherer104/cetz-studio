//! Optional edge-only routing. Typst supplies geometry; A* never moves nodes.
//!
//! Search uses the rectilinear visibility grid induced by inflated obstacle
//! boundaries, not a pixel raster. Ports and their outward exit corridors are
//! fixed. Edges are routed independently: crossings/overlaps and label avoidance
//! are deliberately not optimized. Every batch is all-or-nothing.
pub mod jobs;
mod measure;
mod patches;

pub use measure::measure;
pub use patches::{eligibility, eligibility_list, patch_routes, Eligibility};

use anyhow::{bail, ensure, Context, Result};
use pathfinding::prelude::astar;
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
    collections::HashSet,
    time::{Duration, Instant},
};

const EPS: f64 = 1e-7;
// More than the 0.5e-6 mm rounding error of edit::number.
const GUARD_MM: f64 = 0.002;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
impl Point {
    fn distance(self, other: Self) -> f64 {
        (self.x - other.x).abs() + (self.y - other.y).abs()
    }
    fn valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.x.abs() <= 100_000.0
            && self.y.abs() <= 100_000.0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Rect {
    pub min: Point,
    pub max: Point,
}
impl Rect {
    fn inflate(self, d: f64) -> Self {
        Self {
            min: Point {
                x: self.min.x - d,
                y: self.min.y - d,
            },
            max: Point {
                x: self.max.x + d,
                y: self.max.y + d,
            },
        }
    }
    fn valid(self) -> bool {
        self.min.valid() && self.max.valid() && self.min.x <= self.max.x && self.min.y <= self.max.y
    }
    fn contains(self, p: Point) -> bool {
        p.x > self.min.x + EPS
            && p.x < self.max.x - EPS
            && p.y > self.min.y + EPS
            && p.y < self.max.y - EPS
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    East,
    South,
    West,
    North,
}
impl Side {
    fn opposite(self) -> Self {
        match self {
            Self::East => Self::West,
            Self::West => Self::East,
            Self::North => Self::South,
            Self::South => Self::North,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::East => "east",
            Self::West => "west",
            Self::North => "north",
            Self::South => "south",
        }
    }
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "east" => Some(Self::East),
            "west" => Some(Self::West),
            "north" => Some(Self::North),
            "south" => Some(Self::South),
            _ => None,
        }
    }
    fn index(self) -> usize {
        match self {
            Self::East => 0,
            Self::South => 1,
            Self::West => 2,
            Self::North => 3,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MeasuredNode {
    pub id: String,
    pub bounds: Rect,
    /// East, south, west, north. Obtained from Fletcher's actual anchor resolver.
    pub ports: Vec<Point>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct MeasuredEdge {
    pub points: Vec<Point>,
    pub width_mm: f64,
    pub corner_mm: f64,
    pub kind: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Geometry {
    pub nodes: Vec<MeasuredNode>,
    pub edges: Vec<MeasuredEdge>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub clearance_mm: f64,
}
impl Default for Options {
    fn default() -> Self {
        Self { clearance_mm: 2.0 }
    }
}
impl Options {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.clearance_mm.is_finite() && (0.0..=25.0).contains(&self.clearance_mm),
            "Clearance must be between 0 and 25 mm"
        );
        Ok(())
    }
}

/// Deployment limits, not client-controlled. Tests can tighten them deterministically.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub nodes: usize,
    pub edges: usize,
    pub grid_points: usize,
    pub expansions: usize,
    pub elapsed: Duration,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            nodes: 128,
            edges: 32,
            grid_points: 70_000,
            expansions: 50_000,
            elapsed: Duration::from_millis(500),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Route {
    pub edge: String,
    /// Includes the two port positions; only interior points become waypoints.
    pub points: Vec<Point>,
    pub start_side: Side,
    pub end_side: Side,
    /// An existing explicit label position is mapped to the new path by length.
    pub label_position: Option<[f64; 2]>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub routes: Vec<Route>,
    pub search_ms: f64,
    pub expansions: usize,
}

fn segment_clear(a: Point, b: Point, obstacles: &[Rect], ignore: Option<usize>) -> bool {
    if (a.x - b.x).abs() > EPS && (a.y - b.y).abs() > EPS {
        return false;
    }
    obstacles.iter().enumerate().all(|(i, r)| {
        if Some(i) == ignore {
            return true;
        }
        if (a.x - b.x).abs() <= EPS {
            !(a.x > r.min.x + EPS
                && a.x < r.max.x - EPS
                && a.y.min(b.y) < r.max.y - EPS
                && a.y.max(b.y) > r.min.y + EPS)
        } else {
            !(a.y > r.min.y + EPS
                && a.y < r.max.y - EPS
                && a.x.min(b.x) < r.max.x - EPS
                && a.x.max(b.x) > r.min.x + EPS)
        }
    })
}

fn exit(port: Point, bounds: Rect, side: Side) -> Point {
    match side {
        Side::East => Point {
            x: port.x.max(bounds.max.x) + GUARD_MM,
            ..port
        },
        Side::West => Point {
            x: port.x.min(bounds.min.x) - GUARD_MM,
            ..port
        },
        Side::South => Point {
            y: port.y.max(bounds.max.y) + GUARD_MM,
            ..port
        },
        Side::North => Point {
            y: port.y.min(bounds.min.y) - GUARD_MM,
            ..port
        },
    }
}
fn simplify(points: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::new();
    for p in points {
        if out.last().is_some_and(|q| p.distance(*q) < EPS) {
            continue;
        }
        while out.len() >= 2 {
            let a = out[out.len() - 2];
            let b = out[out.len() - 1];
            let straight = ((a.x - b.x).abs() < EPS
                && (b.x - p.x).abs() < EPS
                && (b.y - a.y) * (p.y - b.y) >= 0.0)
                || ((a.y - b.y).abs() < EPS
                    && (b.y - p.y).abs() < EPS
                    && (b.x - a.x) * (p.x - b.x) >= 0.0);
            if !straight {
                break;
            }
            out.pop();
        }
        out.push(p);
    }
    out
}
fn coordinates(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    values.dedup_by(|a, b| a == b);
    values
}
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct State {
    x: usize,
    y: usize,
    direction: Side,
}

struct Budget {
    start: Instant,
    limits: Limits,
    expansions: Cell<usize>,
    stopped: Cell<bool>,
}
impl Budget {
    fn check(&self) -> bool {
        if self.expansions.get() >= self.limits.expansions
            || self.start.elapsed() >= self.limits.elapsed
        {
            self.stopped.set(true);
        }
        self.stopped.get()
    }
}

fn search(
    from: Point,
    to: Point,
    start_side: Side,
    end_side: Side,
    obstacles: &[Rect],
    budget: &Budget,
) -> Result<Vec<Point>> {
    let mut xs = vec![from.x, to.x];
    let mut ys = vec![from.y, to.y];
    for r in obstacles {
        xs.extend([r.min.x, r.max.x]);
        ys.extend([r.min.y, r.max.y]);
    }
    let xs = coordinates(xs);
    let ys = coordinates(ys);
    ensure!(
        xs.len().saturating_mul(ys.len()) <= budget.limits.grid_points,
        "Routing grid exceeds the bounded-search limit"
    );
    let ix = |v: f64| {
        xs.binary_search_by(|x| x.total_cmp(&v))
            .expect("grid includes endpoint")
    };
    let iy = |v: f64| {
        ys.binary_search_by(|y| y.total_cmp(&v))
            .expect("grid includes endpoint")
    };
    let point = |s: &State| Point {
        x: xs[s.x],
        y: ys[s.y],
    };
    let initial = State {
        x: ix(from.x),
        y: iy(from.y),
        direction: start_side,
    };
    let goal = (ix(to.x), iy(to.y));
    let result = astar(
        &initial,
        |s| {
            if budget.check() {
                return Vec::new();
            }
            budget.expansions.set(budget.expansions.get() + 1);
            let neighbours = [
                (
                    s.x.checked_add(1).filter(|x| *x < xs.len()),
                    Some(s.y),
                    Side::East,
                ),
                (
                    Some(s.x),
                    s.y.checked_add(1).filter(|y| *y < ys.len()),
                    Side::South,
                ),
                (s.x.checked_sub(1), Some(s.y), Side::West),
                (Some(s.x), s.y.checked_sub(1), Side::North),
            ];
            neighbours
                .into_iter()
                .filter_map(|(x, y, direction)| {
                    if direction == s.direction.opposite() {
                        return None;
                    }
                    let next = State {
                        x: x?,
                        y: y?,
                        direction,
                    };
                    let a = point(s);
                    let b = point(&next);
                    if !segment_clear(a, b, obstacles, None) {
                        return None;
                    }
                    let cost = (a.distance(b) * 1000.0).ceil() as u64
                        + if direction != s.direction { 5_000 } else { 0 };
                    Some((next, cost))
                })
                .collect::<Vec<_>>()
        },
        |s| (point(s).distance(to) * 1000.0).floor() as u64,
        // Stop immediately rather than draining an open set after exhaustion.
        // The resulting partial path is discarded below, never offered as a route.
        |s| budget.check() || ((s.x, s.y) == goal && s.direction != end_side),
    );
    ensure!(
        !budget.stopped.get(),
        "Routing exhausted its search/time budget; draft unchanged"
    );
    let (states, _) = result.context("No route exists for these ports and clearance")?;
    Ok(states.iter().map(point).collect())
}

pub(crate) fn endpoint<'a>(
    name: &str,
    diagram: &'a crate::model::Diagram,
) -> Option<(&'a str, Option<Side>)> {
    let n = diagram
        .nodes
        .iter()
        .filter(|n| name == n.id || name.starts_with(&format!("{}.", n.id)))
        .max_by_key(|n| n.id.len())?;
    if name == n.id {
        return Some((&n.id, None));
    }
    Some((&n.id, Some(Side::parse(&name[n.id.len() + 1..])?)))
}

fn remap_label(old: &[Point], new: &[Point], position: [f64; 2]) -> Result<[f64; 2]> {
    let index = position[0];
    ensure!(
        index.is_finite()
            && index >= 0.0
            && index.fract() == 0.0
            && (index as usize) < old.len().saturating_sub(1)
            && position[1].is_finite()
            && (0.0..=1.0).contains(&position[1]),
        "Unsupported label segment/fraction"
    );
    let lengths: Vec<_> = old
        .windows(2)
        .map(|p| (p[0].x - p[1].x).hypot(p[0].y - p[1].y))
        .collect();
    let total: f64 = lengths.iter().sum();
    ensure!(total > EPS, "Cannot map a label from a zero-length route");
    let fraction = (lengths[..index as usize].iter().sum::<f64>()
        + lengths[index as usize] * position[1])
        / total;
    let lengths: Vec<_> = new
        .windows(2)
        .map(|p| (p[0].x - p[1].x).hypot(p[0].y - p[1].y))
        .collect();
    let mut target = fraction * lengths.iter().sum::<f64>();
    for (i, length) in lengths.iter().enumerate() {
        if target <= *length || i == lengths.len() - 1 {
            return Ok([i as f64, (target / length.max(EPS)).clamp(0.0, 1.0)]);
        }
        target -= length;
    }
    bail!("Cannot place a label on an empty route")
}

pub fn plan(
    source: &str,
    diagram: &crate::model::Diagram,
    geometry: &Geometry,
    edges: &[String],
    options: &Options,
    limits: Limits,
) -> Result<Plan> {
    options.validate()?;
    ensure!(
        !edges.is_empty() && edges.len() <= limits.edges,
        "Select 1–{} edges",
        limits.edges
    );
    ensure!(
        edges.iter().collect::<HashSet<_>>().len() == edges.len(),
        "Duplicate selected edges"
    );
    ensure!(
        !geometry.nodes.is_empty() && geometry.nodes.len() <= limits.nodes,
        "Measured scene exceeds the {}-node limit",
        limits.nodes
    );
    ensure!(
        geometry.edges.len() == diagram.edges.len(),
        "Computed edges cannot be mapped one-to-one"
    );
    let mut ids = HashSet::new();
    for n in &geometry.nodes {
        ensure!(
            ids.insert(&n.id)
                && n.bounds.valid()
                && (n.ports.is_empty() || n.ports.len() == 4)
                && n.ports.iter().all(|p| p.valid()),
            "Invalid or ambiguous measured node bounds/ports"
        );
    }
    let budget = Budget {
        start: Instant::now(),
        limits,
        expansions: Cell::new(0),
        stopped: Cell::new(false),
    };
    let mut routes = Vec::new();
    for id in edges {
        ensure!(
            !budget.check(),
            "Routing exhausted its search/time budget; draft unchanged"
        );
        let index = diagram
            .edges
            .iter()
            .position(|e| &e.id == id)
            .context("Unknown selected edge")?;
        let edge = &diagram.edges[index];
        eligibility(source, diagram, edge)?;
        let measured = &geometry.edges[index];
        ensure!(
            matches!(measured.kind.as_str(), "line" | "poly")
                && measured.points.len() == edge.vertices.len()
                && measured.points.iter().all(|p| p.valid())
                && measured.width_mm.is_finite()
                && (0.0..=20.0).contains(&measured.width_mm)
                && measured.corner_mm.is_finite()
                && (0.0..=100.0).contains(&measured.corner_mm),
            "Unsupported measured edge geometry/style"
        );
        let anchor = |v: &crate::model::Vertex| -> Result<(&str, Option<Side>)> {
            let crate::model::Vertex::Anchor { name, .. } = v else {
                bail!("Named endpoint required")
            };
            endpoint(name, diagram).context("Unsupported endpoint port")
        };
        let (start_id, start_side) = anchor(&edge.vertices[0])?;
        let (end_id, end_side) = anchor(edge.vertices.last().context("Missing endpoint")?)?;
        ensure!(
            start_id != end_id,
            "Self-loops are not eligible for automatic routing"
        );
        let find = |name: &str| {
            geometry
                .nodes
                .iter()
                .position(|n| n.id == name)
                .context("Missing measured endpoint node")
        };
        let a = find(start_id)?;
        let b = find(end_id)?;
        ensure!(
            geometry.nodes[a].ports.len() == 4 && geometry.nodes[b].ports.len() == 4,
            "Measured cardinal ports are unavailable"
        );
        let ac = measured.points[0];
        let bc = *measured
            .points
            .last()
            .context("Missing measured endpoint")?;
        let auto = if (bc.x - ac.x).abs() >= (bc.y - ac.y).abs() {
            if bc.x >= ac.x {
                Side::East
            } else {
                Side::West
            }
        } else if bc.y >= ac.y {
            Side::South
        } else {
            Side::North
        };
        let start_side = start_side.unwrap_or(auto);
        let end_side = end_side.unwrap_or(auto.opposite());
        let from = geometry.nodes[a].ports[start_side.index()];
        let to = geometry.nodes[b].ports[end_side.index()];
        // Preserve rounded-corner style while allowing even the inside of a
        // fillet and the stroke envelope to keep the requested clearance.
        let padding =
            options.clearance_mm + measured.width_mm / 2.0 + measured.corner_mm + GUARD_MM;
        let obstacles: Vec<_> = geometry
            .nodes
            .iter()
            .map(|n| n.bounds.inflate(padding))
            .collect();
        ensure!(
            obstacles.iter().all(|r| r.valid()),
            "Inflated bounds exceed coordinate limits"
        );
        let first = exit(from, obstacles[a], start_side);
        let last = exit(to, obstacles[b], end_side);
        ensure!(
            first.valid() && last.valid(),
            "Port exits exceed coordinate limits"
        );
        ensure!(
            segment_clear(from, first, &obstacles, Some(a))
                && segment_clear(last, to, &obstacles, Some(b))
                && !obstacles
                    .iter()
                    .any(|r| r.contains(first) || r.contains(last)),
            "A selected port is blocked at this clearance; choose another port or reduce clearance"
        );
        let middle = search(first, last, start_side, end_side, &obstacles, &budget)
            .with_context(|| format!("Edge {id}"))?;
        let points = simplify(
            std::iter::once(from)
                .chain(middle)
                .chain(std::iter::once(to))
                .collect(),
        );
        ensure!(
            points.len() >= 2 && points.len() <= 128,
            "Route exceeds the 128-vertex limit"
        );
        // Scalar label-pos syntax is left byte-identical. Only segment-indexed
        // literal positions need remapping when the number of segments changes.
        let label_position = if edge
            .call
            .named("label-pos")
            .is_some_and(|arg| arg.items.len() == 2)
        {
            Some(remap_label(
                &measured.points,
                &points,
                edge.label_position.context("Computed label position")?,
            )?)
        } else {
            None
        };
        routes.push(Route {
            edge: id.clone(),
            points,
            start_side,
            end_side,
            label_position,
        });
    }
    ensure!(
        !budget.check(),
        "Routing exhausted its search/time budget; draft unchanged"
    );
    Ok(Plan {
        routes,
        search_ms: budget.start.elapsed().as_secs_f64() * 1000.0,
        expansions: budget.expansions.get(),
    })
}

/// A new route may not alter any measured node position/extent/port, even when
/// its unchanged source coordinates participate in Fletcher's elastic layout.
pub fn verify_fixed_geometry(
    before: &Geometry,
    after: &Geometry,
    routes: &[Route],
    diagram: &crate::model::Diagram,
) -> Result<()> {
    let close = |a: f64, b: f64| (a - b).abs() <= 0.0001;
    let point_close = |a: Point, b: Point| close(a.x, b.x) && close(a.y, b.y);
    ensure!(
        before.nodes.len() == after.nodes.len() && before.edges.len() == after.edges.len(),
        "Measured graph changed during route adoption"
    );
    for (a, b) in before.nodes.iter().zip(&after.nodes) {
        ensure!(
            a.id == b.id
                && point_close(a.bounds.min, b.bounds.min)
                && point_close(a.bounds.max, b.bounds.max)
                && a.ports.len() == b.ports.len()
                && a.ports
                    .iter()
                    .zip(&b.ports)
                    .all(|(a, b)| point_close(*a, *b)),
            "Node bounds or ports changed while applying routes; draft retained"
        );
    }
    for (a, b) in before.edges.iter().zip(&after.edges) {
        ensure!(
            close(a.width_mm, b.width_mm) && close(a.corner_mm, b.corner_mm),
            "Edge style changed during route adoption"
        );
    }
    for route in routes {
        let i = diagram
            .edges
            .iter()
            .position(|e| e.id == route.edge)
            .context("Unknown route")?;
        let measured = &after.edges[i];
        ensure!(
            measured.points.len() == route.points.len()
                && measured.points[1..measured.points.len() - 1]
                    .iter()
                    .zip(&route.points[1..route.points.len() - 1])
                    .all(|(a, b)| point_close(*a, *b)),
            "Compiled route does not match proposed waypoints"
        );
    }
    Ok(())
}
