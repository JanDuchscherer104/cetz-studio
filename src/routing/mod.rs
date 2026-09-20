//! Optional edge-only routing. Typst supplies geometry; A* never moves nodes.
//!
//! Search uses the rectilinear visibility grid induced by inflated obstacle
//! boundaries, not a pixel raster. Ports and their outward exit corridors are
//! fixed. Selected edges are routed as a deterministic batch: constrained edges
//! go first and later routes pay explicit crossing/overlap costs. Label avoidance
//! is deliberately not optimized. Every batch is all-or-nothing.
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
const BEND_PENALTY_UM: u64 = 5_000;
const CROSSING_PENALTY_UM: u64 = 250_000;
const OVERLAP_PENALTY_UM: u64 = 300_000;
const OVERLAP_LENGTH_MULTIPLIER: f64 = 4_000.0;

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
    pub metrics: PlanMetrics,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct PlanMetrics {
    pub crossings: usize,
    pub overlaps: usize,
    pub overlap_mm: f64,
    pub total_length_mm: f64,
    pub bends: usize,
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    a: Point,
    b: Point,
}

impl Segment {
    fn vertical(self) -> bool {
        (self.a.x - self.b.x).abs() <= EPS
    }

    fn length(self) -> f64 {
        self.a.distance(self.b)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Interactions {
    crossings: usize,
    overlaps: usize,
    overlap_mm: f64,
}

fn route_segments(points: &[Point]) -> impl Iterator<Item = Segment> + '_ {
    points.windows(2).map(|p| Segment { a: p[0], b: p[1] })
}

fn between(value: f64, a: f64, b: f64) -> bool {
    value >= a.min(b) - EPS && value <= a.max(b) + EPS
}

fn strictly_between(value: f64, a: f64, b: f64) -> bool {
    value > a.min(b) + EPS && value < a.max(b) - EPS
}

fn segment_interactions(a: Segment, b: Segment) -> Interactions {
    if a.vertical() != b.vertical() {
        let (vertical, horizontal) = if a.vertical() { (a, b) } else { (b, a) };
        let point = Point {
            x: vertical.a.x,
            y: horizontal.a.y,
        };
        if strictly_between(point.y, vertical.a.y, vertical.b.y)
            && strictly_between(point.x, horizontal.a.x, horizontal.b.x)
        {
            return Interactions {
                crossings: 1,
                ..Interactions::default()
            };
        }
        return Interactions::default();
    }

    let collinear = if a.vertical() {
        (a.a.x - b.a.x).abs() <= EPS
    } else {
        (a.a.y - b.a.y).abs() <= EPS
    };
    if !collinear {
        return Interactions::default();
    }
    let (a0, a1, b0, b1) = if a.vertical() {
        (a.a.y, a.b.y, b.a.y, b.b.y)
    } else {
        (a.a.x, a.b.x, b.a.x, b.b.x)
    };
    let overlap = a0.max(a1).min(b0.max(b1)) - a0.min(a1).max(b0.min(b1));
    if overlap > EPS {
        Interactions {
            overlaps: 1,
            overlap_mm: overlap,
            ..Interactions::default()
        }
    } else {
        Interactions::default()
    }
}

fn routing_segment_interactions(candidate: Segment, routed: Segment) -> Interactions {
    if candidate.vertical() != routed.vertical() {
        let (vertical, horizontal) = if candidate.vertical() {
            (candidate, routed)
        } else {
            (routed, candidate)
        };
        let point = Point {
            x: vertical.a.x,
            y: horizontal.a.y,
        };
        let on_candidate = if candidate.vertical() {
            between(point.y, candidate.a.y, candidate.b.y)
        } else {
            between(point.x, candidate.a.x, candidate.b.x)
        };
        let inside_routed = if routed.vertical() {
            strictly_between(point.y, routed.a.y, routed.b.y)
        } else {
            strictly_between(point.x, routed.a.x, routed.b.x)
        };
        // Prior-route coordinates split the visibility grid at every crossing.
        // Charge the step leaving that grid point, not both the arriving and
        // leaving steps. A touch that turns away therefore is not a crossing.
        let arriving_at_intersection =
            point.distance(candidate.b) <= EPS && point.distance(candidate.a) > EPS;
        if on_candidate && inside_routed && !arriving_at_intersection {
            return Interactions {
                crossings: 1,
                ..Interactions::default()
            };
        }
        return Interactions::default();
    }
    segment_interactions(candidate, routed)
}

fn routing_interactions_with(segment: Segment, routes: &[Vec<Point>]) -> Interactions {
    routes
        .iter()
        .flat_map(|points| route_segments(points))
        .map(|other| routing_segment_interactions(segment, other))
        .fold(Interactions::default(), |mut total, value| {
            total.crossings += value.crossings;
            total.overlaps += value.overlaps;
            total.overlap_mm += value.overlap_mm;
            total
        })
}

fn interaction_cost(interactions: Interactions) -> u64 {
    (interactions.crossings as u64)
        .saturating_mul(CROSSING_PENALTY_UM)
        .saturating_add((interactions.overlaps as u64).saturating_mul(OVERLAP_PENALTY_UM))
        .saturating_add((interactions.overlap_mm * OVERLAP_LENGTH_MULTIPLIER).ceil() as u64)
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
    routed: &[Vec<Point>],
    budget: &Budget,
) -> Result<Vec<Point>> {
    let mut xs = vec![from.x, to.x];
    let mut ys = vec![from.y, to.y];
    for r in obstacles {
        xs.extend([r.min.x, r.max.x]);
        ys.extend([r.min.y, r.max.y]);
    }
    for p in routed.iter().flatten() {
        xs.push(p.x);
        ys.push(p.y);
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
                        + if direction != s.direction {
                            BEND_PENALTY_UM
                        } else {
                            0
                        }
                        + interaction_cost(routing_interactions_with(Segment { a, b }, routed));
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

struct PreparedEdge {
    id: String,
    source_index: usize,
    direct_options: usize,
    blocking_obstacles: usize,
    start_side: Side,
    end_side: Side,
    from: Point,
    to: Point,
    first: Point,
    last: Point,
    obstacles: Vec<Rect>,
    old_points: Vec<Point>,
    old_label_position: Option<[f64; 2]>,
    remap_label: bool,
}

fn direct_options(from: Point, to: Point, obstacles: &[Rect]) -> usize {
    if (from.x - to.x).abs() <= EPS || (from.y - to.y).abs() <= EPS {
        return usize::from(segment_clear(from, to, obstacles, None));
    }
    [Point { x: to.x, y: from.y }, Point { x: from.x, y: to.y }]
        .into_iter()
        .filter(|elbow| {
            segment_clear(from, *elbow, obstacles, None)
                && segment_clear(*elbow, to, obstacles, None)
        })
        .count()
}

fn blocks_span(rect: Rect, from: Point, to: Point) -> bool {
    rect.max.x > from.x.min(to.x) + EPS
        && rect.min.x < from.x.max(to.x) - EPS
        && rect.max.y > from.y.min(to.y) + EPS
        && rect.min.y < from.y.max(to.y) - EPS
}

fn plan_metrics(routes: &[Route]) -> PlanMetrics {
    let total_length_mm = routes
        .iter()
        .flat_map(|route| route_segments(&route.points))
        .map(Segment::length)
        .sum();
    let bends = routes
        .iter()
        .map(|route| route.points.len().saturating_sub(2))
        .sum();
    let mut interactions = Interactions::default();
    for (index, route) in routes.iter().enumerate() {
        for segment in route_segments(&route.points) {
            for prior in &routes[..index] {
                for other in route_segments(&prior.points) {
                    let value = segment_interactions(segment, other);
                    interactions.crossings += value.crossings;
                    interactions.overlaps += value.overlaps;
                    interactions.overlap_mm += value.overlap_mm;
                }
            }
        }
    }
    PlanMetrics {
        crossings: interactions.crossings,
        overlaps: interactions.overlaps,
        overlap_mm: interactions.overlap_mm,
        total_length_mm,
        bends,
    }
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
    let mut prepared = Vec::with_capacity(edges.len());
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
        let direct_options = direct_options(first, last, &obstacles);
        let blocking_obstacles = obstacles
            .iter()
            .enumerate()
            .filter(|(index, rect)| *index != a && *index != b && blocks_span(**rect, first, last))
            .count();
        prepared.push(PreparedEdge {
            id: id.clone(),
            source_index: index,
            direct_options,
            blocking_obstacles,
            start_side,
            end_side,
            from,
            to,
            first,
            last,
            obstacles,
            old_points: measured.points.clone(),
            old_label_position: edge.label_position,
            remap_label: edge
                .call
                .named("label-pos")
                .is_some_and(|arg| arg.items.len() == 2),
        });
    }

    // Fewer unobstructed elbow choices and more intervening obstacles make an
    // edge more constrained. Source index is the stable final tie-breaker, so
    // browser selection order cannot change a batch proposal.
    prepared.sort_by_key(|edge| {
        (
            edge.direct_options,
            std::cmp::Reverse(edge.blocking_obstacles),
            edge.source_index,
        )
    });

    let mut routed_paths = Vec::with_capacity(prepared.len());
    let mut indexed_routes = Vec::with_capacity(prepared.len());
    for edge in prepared {
        ensure!(
            !budget.check(),
            "Routing exhausted its search/time budget; draft unchanged"
        );
        let middle = search(
            edge.first,
            edge.last,
            edge.start_side,
            edge.end_side,
            &edge.obstacles,
            &routed_paths,
            &budget,
        )
        .with_context(|| format!("Edge {}", edge.id))?;
        let points = simplify(
            std::iter::once(edge.from)
                .chain(middle)
                .chain(std::iter::once(edge.to))
                .collect(),
        );
        ensure!(
            points.len() >= 2 && points.len() <= 128,
            "Route exceeds the 128-vertex limit"
        );
        // Scalar label-pos syntax is left byte-identical. Only segment-indexed
        // literal positions need remapping when the number of segments changes.
        let label_position = if edge.remap_label {
            Some(remap_label(
                &edge.old_points,
                &points,
                edge.old_label_position.context("Computed label position")?,
            )?)
        } else {
            None
        };
        routed_paths.push(points.clone());
        indexed_routes.push((
            edge.source_index,
            Route {
                edge: edge.id,
                points,
                start_side: edge.start_side,
                end_side: edge.end_side,
                label_position,
            },
        ));
    }
    ensure!(
        !budget.check(),
        "Routing exhausted its search/time budget; draft unchanged"
    );
    indexed_routes.sort_by_key(|(index, _)| *index);
    let routes = indexed_routes
        .into_iter()
        .map(|(_, route)| route)
        .collect::<Vec<_>>();
    let metrics = plan_metrics(&routes);
    Ok(Plan {
        routes,
        search_ms: budget.start.elapsed().as_secs_f64() * 1000.0,
        expansions: budget.expansions.get(),
        metrics,
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
