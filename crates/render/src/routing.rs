//! Orthogonal edge routing: default endpoint placement and path
//! construction.
//!
//! Implements `docs/edge-routing.md` (R1-R7): departure/arrival side
//! selection with a horizontal bias (R2/R3, issue #52), anchor
//! distribution across a shared side (R4, issue #51), a minimum jetty
//! stub at each end (R1), a bounded bend budget (R6), a third-node
//! avoidance fallback (R7), and lane separation for segments that would
//! otherwise coincide (R5).
//!
//! Scope: this pipeline governs *default* endpoints only — an edge with an
//! explicit `exitX/exitY`/`entryX/entryY` override keeps the legacy
//! verbatim behaviour ([`legacy_route`]); the design doc's invariants
//! "govern defaults only". Anchor distribution and lane separation also
//! only ever move points among *default* edges — a pinned endpoint never
//! moves and is never counted against another edge's side.

use std::collections::HashMap;

use crate::model::{Model, Vertex};
use crate::style::{EdgeEndpoints, StyleMap};

/// R1 — minimum straight run perpendicular to a side before a route may
/// bend, so arrowheads and departures stay visually attached to their box.
const JETTY: f64 = 10.0;
/// R5 — fixed gutter between two parallel lanes.
const LANE_GUTTER: f64 = 8.0;
const EPS: f64 = 1e-6;

/// One of the four cardinal sides of a rectangular cell an edge can attach
/// to. Defaults always resolve to a point strictly inside a side's span —
/// never a corner (issue #40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Side {
    Top,
    Bottom,
    Left,
    Right,
}

impl Side {
    /// `true` for the two sides that run horizontally (top/bottom). A
    /// route travels perpendicular to the side it lands on head-on, so a
    /// side entered head-on by vertical travel is a horizontal side, and
    /// vice versa.
    fn is_horizontal_side(self) -> bool {
        matches!(self, Side::Top | Side::Bottom)
    }

    fn opposite(self) -> Side {
        match self {
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

/// The resolved path of one edge, in drawing order. Two points is a plain
/// straight line; three is a single-bend L; five is the jetty/lane-nudge
/// Z (R1/R5). Never more than five (R6's three-bend ceiling).
#[derive(Debug, Clone)]
pub(crate) struct Route {
    pub points: Vec<(f64, f64)>,
}

/// One edge with both endpoints resolved to real vertices and neither end
/// pinned by an `exitX/exitY`/`entryX/entryY` override — the population
/// the R1-R7 pipeline runs over. `idx` is the position in `model.edges`,
/// carried through for R4/R5's declaration-order tie-break.
struct DefaultEdge<'a> {
    idx: usize,
    src: &'a Vertex,
    tgt: &'a Vertex,
}

/// Resolve a route for every edge in `model.edges`, in declaration order.
/// `None` marks a dangling edge (source or target id not found), matching
/// the renderer's existing tolerance for malformed input.
pub(crate) fn route_edges(model: &Model) -> Vec<Option<Route>> {
    let mut out: Vec<Option<Route>> = vec![None; model.edges.len()];
    let mut defaults: Vec<DefaultEdge> = Vec::new();

    for (i, e) in model.edges.iter().enumerate() {
        let Some(src) = model.vertices.iter().find(|v| v.id == e.source) else {
            continue;
        };
        let Some(tgt) = model.vertices.iter().find(|v| v.id == e.target) else {
            continue;
        };
        let style = StyleMap::parse(&e.style);
        let overrides = EdgeEndpoints::from_style(&style);

        if overrides.exit.is_some() || overrides.entry.is_some() {
            out[i] = Some(legacy_route(src, tgt, overrides.exit, overrides.entry));
            continue;
        }
        defaults.push(DefaultEdge { idx: i, src, tgt });
    }

    if defaults.is_empty() {
        return out;
    }

    // R2/R3 (+R7): resolve each default edge's departure/arrival side
    // independently, preferring the side pair that keeps clear of every
    // other node.
    let mut sides: Vec<(Side, Side)> = defaults
        .iter()
        .map(|d| {
            let obstacles: Vec<&Vertex> = model
                .vertices
                .iter()
                .filter(|v| v.id != d.src.id && v.id != d.tgt.id)
                .collect();
            resolve_sides(d.src, d.tgt, &obstacles)
        })
        .collect();

    // R4: distribute anchors across every (vertex, side) touched by more
    // than one default edge, ordered by continuation.
    let fractions = distribute_anchors(&defaults, &sides);

    // Build the natural route (R1 jetty, R6 bend budget) for every
    // default edge from its resolved sides and fraction.
    let mut routes: Vec<Route> = defaults
        .iter()
        .enumerate()
        .map(|(k, d)| {
            let (exit_side, entry_side) = sides[k];
            let exit_pt = side_anchor(d.src, exit_side, fractions[k].0);
            let entry_pt = side_anchor(d.tgt, entry_side, fractions[k].1);
            natural_route(exit_pt, exit_side, entry_pt, entry_side)
        })
        .collect();

    // R7 (node avoidance) and R5 (lane separation) can each undo the
    // other's fix — a node detour can land two edges back on a shared
    // seam, and a lane nudge is built blind to nodes, so it can route
    // straight back through one a detour just cleared. Alternate passes
    // until both settle (each is a no-op on an already-clear route, so
    // this converges quickly for the small diagrams this router targets)
    // rather than leaving whichever ran last to silently win.
    for _ in 0..4 {
        resolve_node_collisions(&defaults, &sides, &mut routes, model);
        resolve_lane_collisions(&defaults, &mut sides, &mut routes, model);
    }
    resolve_node_collisions(&defaults, &sides, &mut routes, model);

    for (d, route) in defaults.into_iter().zip(routes) {
        out[d.idx] = Some(route);
    }
    out
}

fn centre(v: &Vertex) -> (f64, f64) {
    (v.x + v.w / 2.0, v.y + v.h / 2.0)
}

fn extents_overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> bool {
    a0 < b1 - EPS && b0 < a1 - EPS
}

/// Absolute coordinate of `cell`'s side at fraction `frac` (`0..1`) along
/// the side's own span. `frac = 0.5` is the side's centre.
fn side_anchor(cell: &Vertex, side: Side, frac: f64) -> (f64, f64) {
    match side {
        Side::Top => (cell.x + frac * cell.w, cell.y),
        Side::Bottom => (cell.x + frac * cell.w, cell.y + cell.h),
        Side::Left => (cell.x, cell.y + frac * cell.h),
        Side::Right => (cell.x + cell.w, cell.y + frac * cell.h),
    }
}

fn side_centre(cell: &Vertex, side: Side) -> (f64, f64) {
    side_anchor(cell, side, 0.5)
}

/// Classify an explicit normalised override `(nx, ny)` (an `exitX/exitY`
/// or `entryX/entryY` pair, each `0..1`) by which side of the cell it sits
/// on. Used only to infer the *orientation* of a pinned anchor. An exact
/// corner override reads as the horizontal side (`Top`/`Bottom`) — an
/// arbitrary but deterministic tie-break, since the caller pinned a corner
/// on purpose and there is no "correct" orientation to recover.
fn side_of_override(nx: f32, ny: f32) -> Side {
    if ny <= 0.0 {
        Side::Top
    } else if ny >= 1.0 {
        Side::Bottom
    } else if nx <= 0.0 {
        Side::Left
    } else {
        Side::Right
    }
}

/// Move an explicit `exitX/exitY`/`entryX/entryY` override off a corner
/// (issue #49). A pinned anchor is honoured verbatim *unless* it names an
/// exact corner (both members at the extreme `0`/`1`) — that reads as
/// ambiguous between its two adjacent sides rather than a deliberate side
/// attachment, and literally rendering it leaves the edge departing or
/// arriving at the cell's corner. Corners are common by accident: an
/// author fanning out several edges from one box picks one bottom corner
/// per edge to keep them visually separate, without meaning "attach to
/// the corner" specifically.
///
/// The override is reinterpreted as a quarter-point on the side
/// [`side_of_override`] already ties a corner to (`Top`/`Bottom`), offset
/// toward whichever half the pinned corner named — e.g. `(0, 1)`
/// (bottom-left) becomes `(0.25, 1)`: still the bottom side, still biased
/// left, just off the exact corner. A non-corner override (either member
/// strictly inside `(0, 1)`) is returned unchanged.
fn nudge_corner_override(nx: f32, ny: f32) -> (f32, f32) {
    let is_extreme = |v: f32| v <= 0.0 || v >= 1.0;
    if !is_extreme(nx) || !is_extreme(ny) {
        return (nx, ny);
    }
    if side_of_override(nx, ny).is_horizontal_side() {
        (if nx <= 0.0 { 0.25 } else { 0.75 }, ny)
    } else {
        (nx, if ny <= 0.0 { 0.25 } else { 0.75 })
    }
}

/// The side of a cell, restricted to the given orientation, that faces the
/// direction `(dx, dy)` points *away* from the cell. `horizontal` selects
/// between `Top`/`Bottom` (`true`) and `Left`/`Right` (`false`).
fn facing_side(horizontal: bool, dx: f64, dy: f64) -> Side {
    if horizontal {
        if dy >= 0.0 { Side::Bottom } else { Side::Top }
    } else if dx >= 0.0 {
        Side::Right
    } else {
        Side::Left
    }
}

/// The corner of a single-bend route from `exit_pt` (on `exit_side`) to
/// `entry_pt`. Leaving a vertical side (`Left`/`Right`) travels
/// horizontally first, so the corner shares the entry point's x; leaving a
/// horizontal side (`Top`/`Bottom`) travels vertically first.
fn corner_for(exit_side: Side, exit_pt: (f64, f64), entry_pt: (f64, f64)) -> (f64, f64) {
    if exit_side.is_horizontal_side() {
        (exit_pt.0, entry_pt.1)
    } else {
        (entry_pt.0, exit_pt.1)
    }
}

/// Move `pt` outward, perpendicular to `side`, by `dist` — the direction a
/// route leaves (or approaches) that side.
fn advance(pt: (f64, f64), side: Side, dist: f64) -> (f64, f64) {
    match side {
        Side::Top => (pt.0, pt.1 - dist),
        Side::Bottom => (pt.0, pt.1 + dist),
        Side::Left => (pt.0 - dist, pt.1),
        Side::Right => (pt.0 + dist, pt.1),
    }
}

/// Replace the coordinate of `pt` that varies along `side`'s own span (the
/// coordinate R4 distributes fractions along) with `value`, keeping the
/// side's fixed (perpendicular) coordinate untouched.
fn replace_free_coord(pt: (f64, f64), side: Side, value: f64) -> (f64, f64) {
    if side.is_horizontal_side() {
        (value, pt.1)
    } else {
        (pt.0, value)
    }
}

fn points_close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < EPS && (a.1 - b.1).abs() < EPS
}

fn seg_len(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn segment_intersects_rect(p1: (f64, f64), p2: (f64, f64), r: &Vertex) -> bool {
    let (x0, x1) = (p1.0.min(p2.0), p1.0.max(p2.0));
    let (y0, y1) = (p1.1.min(p2.1), p1.1.max(p2.1));
    x0 < r.x + r.w && r.x < x1 && y0 < r.y + r.h && r.y < y1
}

fn path_hits_any(points: &[(f64, f64)], obstacles: &[&Vertex]) -> bool {
    points.windows(2).any(|w| {
        obstacles
            .iter()
            .any(|o| segment_intersects_rect(w[0], w[1], o))
    })
}

fn first_blocking_obstacle<'a>(route: &Route, obstacles: &[&'a Vertex]) -> Option<&'a Vertex> {
    route.points.windows(2).find_map(|w| {
        obstacles
            .iter()
            .find(|o| segment_intersects_rect(w[0], w[1], o))
            .copied()
    })
}

/// R7, node avoidance: the two candidate detour lanes clear of
/// `obstacle` — above/below it for a horizontal run, left/right of it for
/// a vertical one. Both are tried; a lane just past the obstacle's near
/// edge can still leave a *later* segment of the detour clipping a
/// different obstacle (or the same one again further along its column),
/// so [`resolve_node_collisions`] picks by whichever full route survives
/// and is shortest, not by which lane is nearer.
const CLEARANCE: f64 = 20.0;

fn clear_lanes(axis: Axis, obstacle: &Vertex) -> [f64; 2] {
    match axis {
        Axis::Horizontal => [obstacle.y - CLEARANCE, obstacle.y + obstacle.h + CLEARANCE],
        Axis::Vertical => [obstacle.x - CLEARANCE, obstacle.x + obstacle.w + CLEARANCE],
    }
}

fn route_length(route: &Route) -> f64 {
    route.points.windows(2).map(|w| seg_len(w[0], w[1])).sum()
}

/// R7's final fallback for a built route that clips a third node — most
/// often a colinear pair whose shared row/column a node happens to sit
/// in, since R4 can shift each end's fraction independently of the
/// other. Detours around the first node hit by trying both detour lanes
/// through both of R5's nudge shapes (they already build a route through
/// an arbitrary lane coordinate, which is exactly what a detour needs)
/// and keeping whichever fully clears every obstacle — preferring the
/// shortest, per R6's tie-break, so a lane that technically clears but
/// backtracks the long way around loses to one that doesn't. If nothing
/// clears, the original route is left as-is — the explicit "accept the
/// overlap" fallback.
fn resolve_node_collisions(
    defaults: &[DefaultEdge],
    sides: &[(Side, Side)],
    routes: &mut [Route],
    model: &Model,
) {
    for k in 0..routes.len() {
        let d = &defaults[k];
        let obstacles: Vec<&Vertex> = model
            .vertices
            .iter()
            .filter(|v| v.id != d.src.id && v.id != d.tgt.id)
            .collect();
        let Some(blocker) = first_blocking_obstacle(&routes[k], &obstacles) else {
            continue;
        };
        let Some(axis) = routes[k]
            .points
            .windows(2)
            .find_map(|w| {
                segment_intersects_rect(w[0], w[1], blocker).then(|| segment_axis(w[0], w[1]))
            })
            .flatten()
        else {
            continue;
        };
        let (exit_side, entry_side) = sides[k];
        let exit_pt = routes[k].points[0];
        let entry_pt = *routes[k].points.last().unwrap();
        let candidates: Vec<Route> = clear_lanes(axis, blocker)
            .into_iter()
            .flat_map(|lane| {
                [
                    nudge_near_exit(exit_pt, exit_side, entry_pt, entry_side, lane),
                    nudge_near_entry(exit_pt, exit_side, entry_pt, entry_side, lane),
                ]
            })
            .filter(|r| first_blocking_obstacle(r, &obstacles).is_none())
            .collect();
        // Prefer a detour clear of every other edge's *current* route too
        // — otherwise this pass and R5's lane separation can each keep
        // undoing the other's fix — but a node collision (an edge
        // slicing through a box) is worse than a shared seam, so fall
        // back to the shortest merely-obstacle-clear candidate rather
        // than leave the box-crossing route in place.
        let detour = candidates
            .iter()
            .filter(|r| !collides_with_others(r, routes, k))
            .min_by(|a, b| route_length(a).partial_cmp(&route_length(b)).unwrap())
            .or_else(|| {
                candidates
                    .iter()
                    .min_by(|a, b| route_length(a).partial_cmp(&route_length(b)).unwrap())
            })
            .cloned();
        if let Some(clear) = detour {
            routes[k] = clear;
        }
    }
}

/// R3: the unique side pair for a colinear pair of boxes (shared centre x
/// or y) — a straight, bend-free line connecting facing sides. `None` when
/// the boxes are genuinely diagonal to each other.
fn colinear_sides(src: &Vertex, tgt: &Vertex) -> Option<(Side, Side)> {
    let (sx, sy) = centre(src);
    let (tx, ty) = centre(tgt);
    let dx = tx - sx;
    let dy = ty - sy;
    if dy.abs() < EPS {
        let side = if dx >= 0.0 { Side::Right } else { Side::Left };
        return Some((side, side.opposite()));
    }
    if dx.abs() < EPS {
        let side = if dy >= 0.0 { Side::Bottom } else { Side::Top };
        return Some((side, side.opposite()));
    }
    None
}

/// R2/R7: the two valid perpendicular-arrival side combinations for a
/// diagonally-offset pair, in preference order — `[0]` is R2's default
/// (horizontal-biased departure), `[1]` is the other axis-first
/// combination, kept as R7's fallback when the default runs through a
/// third node.
fn side_candidates(src: &Vertex, tgt: &Vertex) -> [(Side, Side); 2] {
    let (sx, sy) = centre(src);
    let (tx, ty) = centre(tgt);
    let dx = tx - sx;
    let dy = ty - sy;
    let x_overlap = extents_overlap(src.x, src.x + src.w, tgt.x, tgt.x + tgt.w);
    // Depart via a vertical side (Left/Right) whenever there's no
    // x-extent overlap (a meaningful horizontal offset, R2's horizontal
    // bias); depart via a horizontal side (Top/Bottom) only when the
    // boxes could run a straight-ish vertical line (x-extents overlap).
    let default_depart_vertical_side = !x_overlap;
    let combo = |depart_vertical_side: bool| -> (Side, Side) {
        let exit = facing_side(!depart_vertical_side, dx, dy);
        let entry = facing_side(depart_vertical_side, -dx, -dy);
        (exit, entry)
    };
    [
        combo(default_depart_vertical_side),
        combo(!default_depart_vertical_side),
    ]
}

/// Resolve one default edge's departure/arrival sides: colinear boxes get
/// the unique facing pair (R3); diagonal boxes get R2's default unless it
/// runs through a third node, in which case R7's fallback candidate is
/// used instead. Falls back to the R2 default (accepting the overlap) if
/// neither candidate is clear — R7's final, explicit fallback.
fn resolve_sides(src: &Vertex, tgt: &Vertex, obstacles: &[&Vertex]) -> (Side, Side) {
    if let Some(sides) = colinear_sides(src, tgt) {
        return sides;
    }
    let candidates = side_candidates(src, tgt);
    for (exit_side, entry_side) in candidates {
        let exit_pt = side_centre(src, exit_side);
        let entry_pt = side_centre(tgt, entry_side);
        let corner = corner_for(exit_side, exit_pt, entry_pt);
        if !path_hits_any(&[exit_pt, corner, entry_pt], obstacles) {
            return (exit_side, entry_side);
        }
    }
    candidates[0]
}

#[derive(Clone, Copy)]
enum Role {
    Exit,
    Entry,
}

struct Touch {
    k: usize,
    role: Role,
}

/// The far endpoint's centre coordinate along a side's own axis — R4's
/// continuation key. For a `Role::Exit` touch (this edge departs the
/// group's vertex) the far endpoint is the edge's target; for
/// `Role::Entry` it's the source.
fn far_coord(defaults: &[DefaultEdge], t: &Touch, axis_is_y: bool) -> f64 {
    let far = match t.role {
        Role::Exit => defaults[t.k].tgt,
        Role::Entry => defaults[t.k].src,
    };
    let c = centre(far);
    if axis_is_y { c.1 } else { c.0 }
}

/// R4: assign each default edge's exit/entry fraction. Every `(vertex,
/// side)` touched by `k` edges gets `k` distinct anchors at `1/(k+1) ..
/// k/(k+1)`, ordered by continuation — the far endpoint's coordinate along
/// the side's own axis, so each edge leaves already heading where it's
/// going — ties broken by declaration order.
#[allow(clippy::cast_precision_loss)] // edges-per-side counts are tiny, never near f64's mantissa limit
fn distribute_anchors(defaults: &[DefaultEdge], sides: &[(Side, Side)]) -> Vec<(f64, f64)> {
    let mut groups: HashMap<(&str, Side), Vec<Touch>> = HashMap::new();
    for (k, d) in defaults.iter().enumerate() {
        let (exit_side, entry_side) = sides[k];
        groups
            .entry((d.src.id.as_str(), exit_side))
            .or_default()
            .push(Touch {
                k,
                role: Role::Exit,
            });
        groups
            .entry((d.tgt.id.as_str(), entry_side))
            .or_default()
            .push(Touch {
                k,
                role: Role::Entry,
            });
    }

    let mut exit_frac = vec![0.5_f64; defaults.len()];
    let mut entry_frac = vec![0.5_f64; defaults.len()];

    for ((_, side), mut touches) in groups {
        // Left/Right sides run vertically, so their fraction axis (and
        // continuation order) is y; Top/Bottom's is x.
        let axis_is_y = !side.is_horizontal_side();
        touches.sort_by(|a, b| {
            let fa = far_coord(defaults, a, axis_is_y);
            let fb = far_coord(defaults, b, axis_is_y);
            fa.partial_cmp(&fb)
                .unwrap()
                .then_with(|| defaults[a.k].idx.cmp(&defaults[b.k].idx))
        });
        let count = touches.len();
        for (rank, t) in touches.iter().enumerate() {
            let frac = (rank + 1) as f64 / (count + 1) as f64;
            match t.role {
                Role::Exit => exit_frac[t.k] = frac,
                Role::Entry => entry_frac[t.k] = frac,
            }
        }
    }

    (0..defaults.len())
        .map(|k| (exit_frac[k], entry_frac[k]))
        .collect()
}

/// Build the default route between two resolved anchor points (R1, R6):
/// a straight line when they already share an axis, a single-bend L when
/// both legs clear the R1 jetty minimum, or a jetty-stretched Z when one
/// leg would otherwise be shorter than [`JETTY`].
fn natural_route(
    exit_pt: (f64, f64),
    exit_side: Side,
    entry_pt: (f64, f64),
    entry_side: Side,
) -> Route {
    let corner = corner_for(exit_side, exit_pt, entry_pt);
    if points_close(corner, exit_pt) || points_close(corner, entry_pt) {
        return Route {
            points: vec![exit_pt, entry_pt],
        };
    }
    if seg_len(exit_pt, corner) >= JETTY && seg_len(corner, entry_pt) >= JETTY {
        return Route {
            points: vec![exit_pt, corner, entry_pt],
        };
    }
    jetty_route(exit_pt, exit_side, entry_pt, entry_side)
}

/// R1: a route whose two stubs (leaving `exit_pt`, arriving at `entry_pt`)
/// are each exactly [`JETTY`] long, with a single corner between them.
fn jetty_route(
    exit_pt: (f64, f64),
    exit_side: Side,
    entry_pt: (f64, f64),
    entry_side: Side,
) -> Route {
    let mid1 = advance(exit_pt, exit_side, JETTY);
    let mid2 = advance(entry_pt, entry_side, JETTY);
    let corner = corner_for(exit_side, mid1, mid2);
    let mut points = vec![exit_pt, mid1, corner, mid2, entry_pt];
    points.dedup_by(|a, b| points_close(*a, *b));
    Route { points }
}

/// A two-bend Z between two *parallel* faces (both horizontal or both
/// vertical). A single-bend L can be perpendicular to only one of them, so
/// the arrowhead would slide along the pinned entry face (issue #55). The
/// route instead leaves `exit_pt` perpendicular to its face, crosses on a
/// mid lane, and arrives at `entry_pt` perpendicular to its face.
fn crossover_route(exit_pt: (f64, f64), exit_side: Side, entry_pt: (f64, f64)) -> Route {
    let (p1, p2) = if exit_side.is_horizontal_side() {
        let lane = f64::midpoint(exit_pt.1, entry_pt.1);
        ((exit_pt.0, lane), (entry_pt.0, lane))
    } else {
        let lane = f64::midpoint(exit_pt.0, entry_pt.0);
        ((lane, exit_pt.1), (lane, entry_pt.1))
    };
    let mut points = vec![exit_pt, p1, p2, entry_pt];
    points.dedup_by(|a, b| points_close(*a, *b));
    Route { points }
}

/// R5, nudging the leg adjacent to `exit_pt`: leave `exit_pt` normally,
/// bend onto `lane`, run the trunk there, then bend back onto the entry
/// anchor's own axis for a perpendicular arrival.
fn nudge_near_exit(
    exit_pt: (f64, f64),
    exit_side: Side,
    entry_pt: (f64, f64),
    _entry_side: Side,
    lane: f64,
) -> Route {
    let p1 = advance(exit_pt, exit_side, JETTY);
    let p2 = replace_free_coord(p1, exit_side, lane);
    let p3 = corner_for(exit_side, p2, entry_pt);
    let mut points = vec![exit_pt, p1, p2, p3, entry_pt];
    points.dedup_by(|a, b| points_close(*a, *b));
    Route { points }
}

/// R5, nudging the leg adjacent to `entry_pt` — the mirror of
/// [`nudge_near_exit`].
fn nudge_near_entry(
    exit_pt: (f64, f64),
    _exit_side: Side,
    entry_pt: (f64, f64),
    entry_side: Side,
    lane: f64,
) -> Route {
    let p3 = advance(entry_pt, entry_side, JETTY);
    let p2 = replace_free_coord(p3, entry_side, lane);
    let p1 = corner_for(entry_side, p2, exit_pt);
    let mut points = vec![exit_pt, p1, p2, p3, entry_pt];
    points.dedup_by(|a, b| points_close(*a, *b));
    Route { points }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    /// Constant y — a horizontal run.
    Horizontal,
    /// Constant x — a vertical run.
    Vertical,
}

fn segment_axis(p1: (f64, f64), p2: (f64, f64)) -> Option<Axis> {
    if (p1.1 - p2.1).abs() < EPS {
        Some(Axis::Horizontal)
    } else if (p1.0 - p2.0).abs() < EPS {
        Some(Axis::Vertical)
    } else {
        None
    }
}

/// The coordinate of `p` that varies along a segment of `axis` — x for a
/// horizontal (constant-y) run, y for a vertical (constant-x) one. Used
/// both for a segment's range (this axis) and, via [`opposite_axis`], its
/// fixed shared coordinate.
fn range_coord(p: (f64, f64), axis: Axis) -> f64 {
    match axis {
        Axis::Horizontal => p.0,
        Axis::Vertical => p.1,
    }
}

fn ranges_overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> bool {
    let (a0, a1) = (a0.min(a1), a0.max(a1));
    let (b0, b1) = (b0.min(b1), b0.max(b1));
    a0 < b1 - EPS && b0 < a1 - EPS
}

/// Find one set of default-edge routes (indices into `routes`) that share
/// an exact, overlapping segment — same axis, same fixed coordinate,
/// overlapping range on the other axis (invariant 2's "no shared seams").
/// Returns the shared axis and coordinate alongside the group so the
/// caller can spread every member into its own lane at once.
fn find_collision_group(routes: &[Route]) -> Option<(Axis, f64, Vec<usize>)> {
    for i in 0..routes.len() {
        for si in 0..routes[i].points.len().saturating_sub(1) {
            let (p1, p2) = (routes[i].points[si], routes[i].points[si + 1]);
            let Some(axis) = segment_axis(p1, p2) else {
                continue;
            };
            let coord = range_coord(p1, opposite_axis(axis));
            let mut group = vec![i];
            for (j, route_j) in routes.iter().enumerate() {
                if j == i {
                    continue;
                }
                for sj in 0..route_j.points.len().saturating_sub(1) {
                    let (q1, q2) = (route_j.points[sj], route_j.points[sj + 1]);
                    if segment_axis(q1, q2) != Some(axis) {
                        continue;
                    }
                    let qcoord = range_coord(q1, opposite_axis(axis));
                    if (qcoord - coord).abs() < EPS
                        && ranges_overlap(
                            range_coord(p1, axis),
                            range_coord(p2, axis),
                            range_coord(q1, axis),
                            range_coord(q2, axis),
                        )
                    {
                        group.push(j);
                        break;
                    }
                }
            }
            if group.len() > 1 {
                return Some((axis, coord, group));
            }
        }
    }
    None
}

fn opposite_axis(axis: Axis) -> Axis {
    match axis {
        Axis::Horizontal => Axis::Vertical,
        Axis::Vertical => Axis::Horizontal,
    }
}

/// `true` if any segment of `route` shares an exact, overlapping segment
/// with any route in `routes` other than `routes[skip]` — the same "no
/// shared seams" check [`find_collision_group`] runs pairwise, but against
/// one candidate route instead of scanning for the first pair.
fn collides_with_others(route: &Route, routes: &[Route], skip: usize) -> bool {
    for (j, other) in routes.iter().enumerate() {
        if j == skip {
            continue;
        }
        for w1 in route.points.windows(2) {
            let Some(axis) = segment_axis(w1[0], w1[1]) else {
                continue;
            };
            for w2 in other.points.windows(2) {
                if segment_axis(w2[0], w2[1]) != Some(axis) {
                    continue;
                }
                let c1 = range_coord(w1[0], opposite_axis(axis));
                let c2 = range_coord(w2[0], opposite_axis(axis));
                if (c1 - c2).abs() < EPS
                    && ranges_overlap(
                        range_coord(w1[0], axis),
                        range_coord(w1[1], axis),
                        range_coord(w2[0], axis),
                        range_coord(w2[1], axis),
                    )
                {
                    return true;
                }
            }
        }
    }
    false
}

/// R7's other candidate for edge `k`: the axis-first combination R2
/// *didn't* default to. `None` for a colinear pair (no alternate exists)
/// or when `sides[k]` no longer matches either candidate (already
/// flipped once). The candidate is built at each side's plain centre —
/// this only runs as a two-edge collision's last-resort escape hatch, so
/// it doesn't re-run R4 distribution for the vertex it moves to.
fn flip_candidate(
    defaults: &[DefaultEdge],
    sides: &[(Side, Side)],
    k: usize,
    model: &Model,
) -> Option<(Route, (Side, Side))> {
    let d = &defaults[k];
    if colinear_sides(d.src, d.tgt).is_some() {
        return None;
    }
    let candidates = side_candidates(d.src, d.tgt);
    let current = sides[k];
    let alt = if current == candidates[0] {
        candidates[1]
    } else if current == candidates[1] {
        candidates[0]
    } else {
        return None;
    };
    let exit_pt = side_centre(d.src, alt.0);
    let entry_pt = side_centre(d.tgt, alt.1);
    let obstacles: Vec<&Vertex> = model
        .vertices
        .iter()
        .filter(|v| v.id != d.src.id && v.id != d.tgt.id)
        .collect();
    let corner = corner_for(alt.0, exit_pt, entry_pt);
    if path_hits_any(&[exit_pt, corner, entry_pt], &obstacles) {
        return None;
    }
    Some((natural_route(exit_pt, alt.0, entry_pt, alt.1), alt))
}

/// R5: repeatedly find a colliding segment group and spread its members
/// into parallel lanes [`LANE_GUTTER`] apart, ordered by continuation (the
/// route's far endpoint, then declaration order), until no exact,
/// overlapping segment remains. Bounded so a pathological layout degrades
/// to "accept the overlap" rather than looping forever.
#[allow(clippy::cast_precision_loss)] // group sizes are a handful of edges, never near f64's mantissa limit
fn resolve_lane_collisions(
    defaults: &[DefaultEdge],
    sides: &mut [(Side, Side)],
    routes: &mut [Route],
    model: &Model,
) {
    for _ in 0..50 {
        let Some((axis, coord, mut group)) = find_collision_group(routes) else {
            return;
        };
        group.sort_by(|&a, &b| {
            let fa = range_coord(*routes[a].points.last().unwrap(), axis);
            let fb = range_coord(*routes[b].points.last().unwrap(), axis);
            fa.partial_cmp(&fb)
                .unwrap()
                .then_with(|| defaults[a].idx.cmp(&defaults[b].idx))
        });

        // A plain two-edge collision is the classic "forced crossing"
        // shape: two L-routes that happen to run down the same line.
        // Nudging both into 8px-apart lanes technically satisfies
        // invariant 2 but still reads as one doubled line at a glance —
        // try R7's other axis-first candidate for the later edge first,
        // so the pair crosses transversally instead (the FAQ's "a
        // perpendicular crossing is unambiguous").
        if group.len() == 2 {
            let k = group[1];
            if let Some((flip_route, flip_sides)) = flip_candidate(defaults, sides, k, model)
                && !collides_with_others(&flip_route, routes, k)
            {
                routes[k] = flip_route;
                sides[k] = flip_sides;
                continue;
            }
        }

        let count = group.len();
        for (rank, &k) in group.iter().enumerate() {
            let offset = (rank as f64 - (count - 1) as f64 / 2.0) * LANE_GUTTER;
            if offset.abs() < EPS {
                continue;
            }
            let lane = coord + offset;
            let (exit_side, entry_side) = sides[k];
            let exit_pt = routes[k].points[0];
            let entry_pt = *routes[k].points.last().unwrap();
            // Whether the collision sits on the leg leaving `exit_pt`
            // (constant on exit's own perpendicular axis) or the leg
            // arriving at `entry_pt` decides which nudge keeps both
            // anchors — and the perpendicular head-on arrival — intact.
            let exit_leg_is_horizontal = !exit_side.is_horizontal_side();
            let collision_near_exit = match axis {
                Axis::Horizontal => exit_leg_is_horizontal,
                Axis::Vertical => !exit_leg_is_horizontal,
            };
            routes[k] = if collision_near_exit {
                nudge_near_exit(exit_pt, exit_side, entry_pt, entry_side, lane)
            } else {
                nudge_near_entry(exit_pt, exit_side, entry_pt, entry_side, lane)
            };
        }
    }
}

/// Legacy endpoint resolution for an edge with at least one explicit
/// `exitX/exitY`/`entryX/entryY` override — verbatim per the design doc
/// ("invariants govern defaults only"): no jetty, no anchor distribution,
/// no lane separation, just the pinned point(s) plus a perpendicular
/// default for whichever end is left unpinned.
fn legacy_route(
    src: &Vertex,
    tgt: &Vertex,
    exit_override: Option<(f32, f32)>,
    entry_override: Option<(f32, f32)>,
) -> Route {
    let exit_override = exit_override.map(|(nx, ny)| nudge_corner_override(nx, ny));
    let entry_override = entry_override.map(|(nx, ny)| nudge_corner_override(nx, ny));

    let src_centre = centre(src);
    let tgt_centre = centre(tgt);
    let dx = tgt_centre.0 - src_centre.0;
    let dy = tgt_centre.1 - src_centre.1;
    let colinear = dx.abs() < EPS || dy.abs() < EPS;

    let (exit_pt, exit_side) = if let Some((nx, ny)) = exit_override {
        (
            (src.x + f64::from(nx) * src.w, src.y + f64::from(ny) * src.h),
            side_of_override(nx, ny),
        )
    } else {
        let side = match entry_override {
            Some((enx, eny)) => {
                let entry_side = side_of_override(enx, eny);
                if colinear {
                    entry_side.opposite()
                } else {
                    facing_side(!entry_side.is_horizontal_side(), dx, dy)
                }
            }
            None => facing_side(dy.abs() >= dx.abs(), dx, dy),
        };
        (side_centre(src, side), side)
    };

    let (entry_pt, entry_side) = if let Some((nx, ny)) = entry_override {
        (
            (tgt.x + f64::from(nx) * tgt.w, tgt.y + f64::from(ny) * tgt.h),
            side_of_override(nx, ny),
        )
    } else {
        let side = if colinear {
            exit_side.opposite()
        } else {
            facing_side(!exit_side.is_horizontal_side(), -dx, -dy)
        };
        (side_centre(tgt, side), side)
    };

    let parallel_faces = exit_side.is_horizontal_side() == entry_side.is_horizontal_side();
    let straight = if exit_side.is_horizontal_side() {
        (exit_pt.0 - entry_pt.0).abs() < EPS
    } else {
        (exit_pt.1 - entry_pt.1).abs() < EPS
    };
    if parallel_faces && !straight {
        return crossover_route(exit_pt, exit_side, entry_pt);
    }

    match legacy_orthogonal_corner(src, exit_pt, entry_pt) {
        Some(corner) => Route {
            points: vec![exit_pt, corner, entry_pt],
        },
        None => Route {
            points: vec![exit_pt, entry_pt],
        },
    }
}

/// Compute the single corner of a two-segment right-angle route from
/// `start` to `end`, given the cell `start` sits on. `None` when the
/// endpoints are colinear (a single segment is already the route) or
/// `start` isn't on any edge of `src` (no sensible orientation to pick).
fn legacy_orthogonal_corner(
    src: &Vertex,
    start: (f64, f64),
    end: (f64, f64),
) -> Option<(f64, f64)> {
    if (start.0 - end.0).abs() < EPS || (start.1 - end.1).abs() < EPS {
        return None;
    }
    let on_vertical_side =
        (start.0 - src.x).abs() < 1e-6 || (start.0 - (src.x + src.w)).abs() < 1e-6;
    let on_horizontal_side =
        (start.1 - src.y).abs() < 1e-6 || (start.1 - (src.y + src.h)).abs() < 1e-6;
    if on_vertical_side {
        Some((end.0, start.1))
    } else if on_horizontal_side {
        Some((start.0, end.1))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Edge;

    /// Build a plain, style-less `Vertex` at the given box — the routing
    /// pipeline works purely off cell geometry, not a declared `points=`
    /// constraint set, so tests don't need one.
    fn plain_vertex(id: &str, x: f64, y: f64, w: f64, h: f64) -> Vertex {
        Vertex {
            id: id.into(),
            label: String::new(),
            style: String::new(),
            x,
            y,
            w,
            h,
        }
    }

    fn edge(id: &str, source: &str, target: &str, style: &str) -> Edge {
        Edge {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            style: style.into(),
        }
    }

    fn endpoints(route: &Route) -> (f64, f64, f64, f64) {
        let (sx, sy) = route.points[0];
        let (tx, ty) = *route.points.last().unwrap();
        (sx, sy, tx, ty)
    }

    fn final_segment(route: &Route) -> ((f64, f64), (f64, f64)) {
        let n = route.points.len();
        (route.points[n - 2], route.points[n - 1])
    }

    /// Route the single edge `a -> b` in an otherwise-empty model and
    /// return its endpoints — the common case for endpoint-selection
    /// tests, where R4 distribution never kicks in (`k == 1` everywhere).
    fn route_pair(a: Vertex, b: Vertex, style: &str) -> Route {
        let model = Model {
            vertices: vec![a, b],
            edges: vec![edge("e1", "a", "b", style)],
        };
        route_edges(&model).into_iter().next().unwrap().unwrap()
    }

    fn is_corner(x: f64, y: f64, v: &Vertex) -> bool {
        let touches_vertical_side = (x - v.x).abs() < 1e-9 || (x - (v.x + v.w)).abs() < 1e-9;
        let touches_horizontal_side = (y - v.y).abs() < 1e-9 || (y - (v.y + v.h)).abs() < 1e-9;
        touches_vertical_side && touches_horizontal_side
    }

    fn shares_a_segment(a: &Route, b: &Route) -> bool {
        collides_with_others(a, std::slice::from_ref(b), usize::MAX)
    }

    #[test]
    fn default_endpoints_snap_to_side_centres_same_row() {
        // Two boxes on the same row (aligned y): a straight horizontal
        // line, so both ends share the same orientation (left/right).
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 300.0, 0.0, 78.0, 78.0);
        let route = route_pair(a, b, "");
        assert_eq!(
            endpoints(&route),
            (78.0, 39.0, 300.0, 39.0),
            "should run straight from A's right-mid to B's left-mid"
        );
    }

    #[test]
    fn default_endpoints_never_land_on_a_corner_when_diagonal() {
        // Two boxes offset both horizontally and vertically (issue #40's
        // reported pattern) — neither end may resolve to a corner, and the
        // route must land head-on: the exit side and entry side must be on
        // perpendicular axes so the router's single bend arrives travelling
        // straight into the entered side rather than sliding along it.
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 300.0, 200.0, 78.0, 78.0);
        let route = route_pair(a.clone(), b.clone(), "");
        let (sx, sy, tx, ty) = endpoints(&route);

        assert!(
            !is_corner(sx, sy, &a),
            "exit landed on a corner: ({sx}, {sy})"
        );
        assert!(
            !is_corner(tx, ty, &b),
            "entry landed on a corner: ({tx}, {ty})"
        );

        // No x-extent overlap between A and B, so R2's horizontal bias
        // departs A's right side regardless of which axis' offset is
        // larger; the entry is forced onto the perpendicular (top/bottom)
        // axis of B for a head-on arrival.
        assert_eq!((sx, sy), (78.0, 39.0), "exit should be A's right-mid");
        assert_eq!((tx, ty), (339.0, 200.0), "entry should be B's top-mid");
    }

    #[test]
    fn r2_prefers_horizontal_departure_even_when_vertical_offset_dominates() {
        // Issue #52: B sits mostly below A (the vertical centre-to-centre
        // offset, 300, dwarfs the horizontal one, 200), but A and B's
        // x-extents don't overlap — no straight-ish vertical run exists —
        // so R2 still departs Left/Right, arriving head-on at B's top
        // rather than sliding a horizontal line into B's side.
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 200.0, 300.0, 78.0, 78.0);
        let route = route_pair(a, b, "");
        assert_eq!(
            endpoints(&route),
            (78.0, 39.0, 239.0, 300.0),
            "exit should be A's right-mid, entry B's top-mid"
        );
    }

    #[test]
    fn r2_allows_vertical_departure_when_x_extents_overlap() {
        // A straight-ish vertical run exists here (B's x-extent overlaps
        // A's), so R2 allows a Top/Bottom departure.
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 20.0, 300.0, 78.0, 78.0);
        let (exit_side, entry_side) = resolve_sides(&a, &b, &[]);
        assert_eq!(exit_side, Side::Bottom);
        assert_eq!(entry_side, Side::Left);
    }

    #[test]
    fn edge_endpoint_overrides_take_priority_over_defaults() {
        // Both ends explicitly pinned: the override wins verbatim even
        // though the boxes are diagonally offset (which would otherwise
        // pick different sides by default).
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 200.0, 300.0, 78.0, 78.0);
        let route = route_pair(a, b, "exitX=1;exitY=0.5;entryX=0;entryY=0.5;");
        assert_eq!(
            endpoints(&route),
            (78.0, 39.0, 200.0, 339.0),
            "both ends must land exactly on the pinned side-mids"
        );
    }

    #[test]
    fn one_sided_override_still_gets_a_perpendicular_default_partner() {
        // Only the exit is pinned (to A's bottom-mid); the entry is left
        // to default. Since A's bottom is a horizontal side, the entry
        // must default to a vertical (left/right) side of B, not a corner
        // and not another horizontal side.
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 300.0, 200.0, 78.0, 78.0);
        let route = route_pair(a, b, "exitX=0.5;exitY=1;");
        let (_, _, tx, ty) = endpoints(&route);
        assert_eq!((tx, ty), (300.0, 239.0), "entry should be B's left-mid");
    }

    #[test]
    fn explicit_corner_exit_override_is_nudged_off_the_corner() {
        // Issue #49 repro: a fan-out edge pins its exit to a bottom corner
        // instead of a side midpoint. The literal corner must never be
        // used verbatim — it's reinterpreted as a quarter-point on the
        // bottom side, still biased toward the pinned corner's half.
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 200.0, 300.0, 78.0, 78.0);

        let route = route_pair(a.clone(), b.clone(), "exitX=0;exitY=1;");
        assert_eq!(
            (endpoints(&route).0, endpoints(&route).1),
            (19.5, 78.0),
            "exit should be A's bottom-quarter-from-left, not the corner (0, 78)"
        );

        let route = route_pair(a, b, "exitX=1;exitY=1;");
        assert_eq!(
            (endpoints(&route).0, endpoints(&route).1),
            (58.5, 78.0),
            "exit should be A's bottom-quarter-from-right, not the corner (78, 78)"
        );
    }

    #[test]
    fn explicit_corner_entry_override_is_nudged_off_the_corner() {
        // Same guard on the arrival side: a pinned entry at an exact corner
        // is nudged to a quarter-point instead of landing on the corner.
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 200.0, 300.0, 78.0, 78.0);
        let route = route_pair(a, b, "entryX=0;entryY=0;");
        let (_, _, tx, ty) = endpoints(&route);
        assert_eq!(
            (tx, ty),
            (219.5, 300.0),
            "entry should be B's top-quarter-from-left, not the corner (200, 300)"
        );
    }

    #[test]
    fn all_four_corner_overrides_resolve_to_quarter_points_never_corners() {
        let corners = [
            "exitX=0;exitY=0;",
            "exitX=1;exitY=0;",
            "exitX=0;exitY=1;",
            "exitX=1;exitY=1;",
        ];
        for style in corners {
            let src = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
            let tgt = plain_vertex("b", 300.0, 300.0, 78.0, 78.0);
            let route = route_pair(src.clone(), tgt, style);
            let (sx, sy, _, _) = endpoints(&route);
            assert!(
                !is_corner(sx, sy, &src),
                "exit override {style} landed on a corner: ({sx}, {sy})"
            );
        }
    }

    #[test]
    fn pinned_entry_face_is_entered_perpendicular_issue_55() {
        // Issue #55: an entry pinned to a face whose orientation is parallel
        // to the router's would-be final segment must still be entered
        // head-on. The final segment has to run perpendicular to the pinned
        // face — never along it — so the arrowhead points into the box.
        let a = plain_vertex("a", 300.0, 40.0, 78.0, 78.0);
        let c = plain_vertex("b", 560.0, 340.0, 78.0, 78.0);
        let d = plain_vertex("b", 60.0, 340.0, 78.0, 78.0);

        // a -> c enters c's left face: the final segment must be horizontal.
        let route = route_pair(a.clone(), c, "exitX=1;exitY=0.5;entryX=0;entryY=0.5;");
        let (prev, last) = final_segment(&route);
        assert_eq!(last, (560.0, 379.0), "arrowhead lands on the pinned entry");
        assert!(
            (prev.1 - last.1).abs() < EPS && (prev.0 - last.0).abs() > EPS,
            "left-face entry must be entered by a horizontal segment: {route:?}"
        );

        // a -> d enters d's top face: the final segment must be vertical.
        let route = route_pair(a, d, "exitX=0;exitY=1;entryX=0.5;entryY=0;");
        let (prev, last) = final_segment(&route);
        assert_eq!(last, (99.0, 340.0), "arrowhead lands on the pinned entry");
        assert!(
            (prev.0 - last.0).abs() < EPS && (prev.1 - last.1).abs() > EPS,
            "top-face entry must be entered by a vertical segment: {route:?}"
        );
    }

    #[test]
    fn legacy_orthogonal_corner_horizontal_first_from_right_edge() {
        // Source endpoint sits on the right edge of source (x = 78). The
        // route must leave horizontally, so the corner is at (end.x, start.y).
        let src = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let corner = legacy_orthogonal_corner(&src, (78.0, 39.0), (300.0, 100.0)).unwrap();
        assert!(
            (corner.0 - 300.0).abs() < 1e-9,
            "corner.x = end.x: {corner:?}"
        );
        assert!(
            (corner.1 - 39.0).abs() < 1e-9,
            "corner.y = start.y: {corner:?}"
        );
    }

    #[test]
    fn legacy_orthogonal_corner_vertical_first_from_bottom_edge() {
        let src = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let corner = legacy_orthogonal_corner(&src, (39.0, 78.0), (200.0, 300.0)).unwrap();
        assert!(
            (corner.0 - 39.0).abs() < 1e-9,
            "corner.x = start.x: {corner:?}"
        );
        assert!(
            (corner.1 - 300.0).abs() < 1e-9,
            "corner.y = end.y: {corner:?}"
        );
    }

    #[test]
    fn legacy_orthogonal_corner_colinear_endpoints_yield_none() {
        let src = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        assert!(legacy_orthogonal_corner(&src, (78.0, 39.0), (300.0, 39.0)).is_none());
        assert!(legacy_orthogonal_corner(&src, (39.0, 78.0), (39.0, 300.0)).is_none());
    }

    #[test]
    fn r4_distributes_shared_side_anchors_ordered_by_continuation() {
        // Three edges leave `hub`'s right side for targets at different
        // heights (issue #51's overlapping-departure repro). Each gets its
        // own anchor at 1/4, 2/4, 3/4 of the side, ordered by the target's
        // y — not declaration order — so every edge leaves already heading
        // toward where it's going.
        let hub = plain_vertex("hub", 0.0, 0.0, 78.0, 78.0);
        let up = plain_vertex("up", 300.0, -200.0, 78.0, 78.0);
        let mid = plain_vertex("mid", 300.0, 0.0, 78.0, 78.0);
        let down = plain_vertex("down", 300.0, 200.0, 78.0, 78.0);
        // Declared out of continuation order on purpose — the fractions
        // must still come out sorted by the target's height, not by index.
        let defaults = vec![
            DefaultEdge {
                idx: 0,
                src: &hub,
                tgt: &down,
            },
            DefaultEdge {
                idx: 1,
                src: &hub,
                tgt: &up,
            },
            DefaultEdge {
                idx: 2,
                src: &hub,
                tgt: &mid,
            },
        ];
        let sides = vec![
            (Side::Right, Side::Left),
            (Side::Right, Side::Bottom),
            (Side::Right, Side::Left),
        ];
        let fractions = distribute_anchors(&defaults, &sides);
        let close = |a: f64, b: f64| (a - b).abs() < EPS;
        assert!(
            close(fractions[1].0, 0.25),
            "up (topmost target) gets the top anchor"
        );
        assert!(close(fractions[2].0, 0.5), "mid gets the middle anchor");
        assert!(
            close(fractions[0].0, 0.75),
            "down (bottommost target) gets the bottom anchor"
        );
        // Each target is alone on its own entry side — k = 1, so its
        // fraction is the plain centre.
        assert!(close(fractions[0].1, 0.5));
        assert!(close(fractions[1].1, 0.5));
        assert!(close(fractions[2].1, 0.5));
    }

    #[test]
    fn r7_falls_back_to_the_other_axis_when_default_route_crosses_a_third_node() {
        // `mid` sits directly in the path R2's default (Right/Bottom)
        // would take from `hub` to `up`; R7's fallback candidate departs
        // hub's top instead, clearing it entirely.
        let hub = plain_vertex("hub", 0.0, 0.0, 78.0, 78.0);
        let up = plain_vertex("up", 300.0, -200.0, 78.0, 78.0);
        let mid = plain_vertex("mid", 300.0, 0.0, 78.0, 78.0);
        let (exit_side, entry_side) = resolve_sides(&hub, &up, &[&mid]);
        assert_eq!(exit_side, Side::Top);
        assert_eq!(entry_side, Side::Left);
    }

    #[test]
    fn r7_detours_around_a_node_a_colinear_route_would_otherwise_cross() {
        // Gallery case 07: `worker` and `db` share a row with `bus`
        // sitting directly between them. `resolve_sides` alone can't see
        // this — R4 shifts each end's anchor independently once `bus`
        // also touches `worker`'s side, so the route built from the
        // resolved (Left, Right) pair isn't even a straight line by the
        // time real fractions are in, and can still clip `bus`. The
        // node-avoidance pass must detour around it.
        let worker = plain_vertex("worker", 640.0, 600.0, 78.0, 78.0);
        let bus = plain_vertex("bus", 360.0, 600.0, 78.0, 78.0);
        let db = plain_vertex("db", 80.0, 600.0, 78.0, 78.0);
        let model = Model {
            vertices: vec![worker.clone(), bus.clone(), db.clone()],
            edges: vec![
                edge("e1", "bus", "worker", ""),
                edge("e2", "worker", "db", ""),
            ],
        };
        let routes = route_edges(&model);
        let worker_to_db = routes[1].as_ref().unwrap();
        let obstacles = [&bus];
        assert!(
            !path_hits_any(&worker_to_db.points, &obstacles),
            "worker -> db route still clips bus: {worker_to_db:?}"
        );
    }

    #[test]
    fn r1_jetty_enforces_minimum_stub_length() {
        // The natural corner would leave B's approach leg only 4px long —
        // short of the R1 minimum — so the router must insert an explicit
        // jetty stub at each end instead of a single tight bend.
        let exit_pt = (78.0, 39.0);
        let entry_pt = (300.0, 35.0);
        let route = natural_route(exit_pt, Side::Right, entry_pt, Side::Top);
        assert_eq!(
            route.points.len(),
            5,
            "short leg should promote to a jetty Z: {route:?}"
        );
        let first_leg = seg_len(route.points[0], route.points[1]);
        let last_leg = seg_len(route.points[3], route.points[4]);
        assert!(
            (first_leg - JETTY).abs() < 1e-9,
            "first stub should be exactly JETTY: {first_leg}"
        );
        assert!(
            (last_leg - JETTY).abs() < 1e-9,
            "final stub should be exactly JETTY: {last_leg}"
        );
    }

    #[test]
    fn r5_separates_two_edges_that_would_otherwise_share_a_seam() {
        // Gallery case 04 ("forced crossing"): tl -> br and bl -> tr sit
        // in mirrored corners, so their natural L-routes both run down
        // x = 519 with overlapping y-ranges. R5 must nudge one apart so
        // the two routes cross transversally instead of coinciding.
        let model = Model {
            vertices: vec![
                plain_vertex("tl", 80.0, 80.0, 78.0, 78.0),
                plain_vertex("tr", 480.0, 80.0, 78.0, 78.0),
                plain_vertex("bl", 80.0, 440.0, 78.0, 78.0),
                plain_vertex("br", 480.0, 440.0, 78.0, 78.0),
            ],
            edges: vec![edge("e1", "tl", "br", ""), edge("e2", "bl", "tr", "")],
        };
        let routes = route_edges(&model);
        let r1 = routes[0].as_ref().unwrap();
        let r2 = routes[1].as_ref().unwrap();
        assert!(
            !shares_a_segment(r1, r2),
            "routes still share a seam:\ntl->br: {r1:?}\nbl->tr: {r2:?}"
        );
    }

    #[test]
    fn renders_edge_with_explicit_entry_exit_overrides() {
        let a = plain_vertex("a", 0.0, 0.0, 78.0, 78.0);
        let b = plain_vertex("b", 200.0, 0.0, 78.0, 78.0);
        let route = route_pair(a, b, "exitX=1;exitY=0.5;entryX=0;entryY=0.5;");
        // Colinear (both endpoints at y=39): a straight two-point route.
        assert_eq!(route.points, vec![(78.0, 39.0), (200.0, 39.0)]);
    }

    /// The edge-case gallery (`docs/edge-routing.md`'s "99%" test): every
    /// case renders with zero invariant violations, checked directly
    /// against the same geometry the gallery JSONs describe (all boxes
    /// are the default 78x78 AWS tile). This is the authoritative,
    /// pixel-level check the PR's rendered screenshots back up visually.
    fn assert_gallery_invariants(model: &Model) {
        let routes: Vec<Route> = route_edges(model).into_iter().map(|r| r.unwrap()).collect();
        for (i, route) in routes.iter().enumerate() {
            let e = &model.edges[i];
            let obstacles: Vec<&Vertex> = model
                .vertices
                .iter()
                .filter(|v| v.id != e.source && v.id != e.target)
                .collect();
            assert!(
                !path_hits_any(&route.points, &obstacles),
                "{} -> {} passes through a node: {route:?}",
                e.source,
                e.target
            );
            for (j, other) in routes.iter().enumerate() {
                if i == j {
                    continue;
                }
                assert!(
                    !collides_with_others(route, std::slice::from_ref(other), usize::MAX),
                    "{} -> {} shares a seam with {} -> {}: {route:?} / {other:?}",
                    e.source,
                    e.target,
                    model.edges[j].source,
                    model.edges[j].target
                );
            }
        }
    }

    #[test]
    fn gallery_01_fanout_updownright_has_no_invariant_violations() {
        let model = Model {
            vertices: vec![
                plain_vertex("hub", 80.0, 300.0, 78.0, 78.0),
                plain_vertex("up", 420.0, 60.0, 78.0, 78.0),
                plain_vertex("mid", 420.0, 300.0, 78.0, 78.0),
                plain_vertex("down", 420.0, 540.0, 78.0, 78.0),
            ],
            edges: vec![
                edge("e1", "hub", "up", ""),
                edge("e2", "hub", "mid", ""),
                edge("e3", "hub", "down", ""),
            ],
        };
        assert_gallery_invariants(&model);
    }

    #[test]
    fn gallery_02_diag_below_has_no_invariant_violations() {
        let model = Model {
            vertices: vec![
                plain_vertex("src", 320.0, 80.0, 78.0, 78.0),
                plain_vertex("bl", 80.0, 420.0, 78.0, 78.0),
                plain_vertex("br", 560.0, 420.0, 78.0, 78.0),
            ],
            edges: vec![edge("e1", "src", "bl", ""), edge("e2", "src", "br", "")],
        };
        assert_gallery_invariants(&model);
    }

    #[test]
    fn gallery_03_fanin_has_no_invariant_violations() {
        let model = Model {
            vertices: vec![
                plain_vertex("a", 80.0, 60.0, 78.0, 78.0),
                plain_vertex("b", 80.0, 300.0, 78.0, 78.0),
                plain_vertex("c", 80.0, 540.0, 78.0, 78.0),
                plain_vertex("sink", 480.0, 300.0, 78.0, 78.0),
            ],
            edges: vec![
                edge("e1", "a", "sink", ""),
                edge("e2", "b", "sink", ""),
                edge("e3", "c", "sink", ""),
            ],
        };
        assert_gallery_invariants(&model);
    }

    #[test]
    fn gallery_04_crossing_has_no_invariant_violations() {
        let model = Model {
            vertices: vec![
                plain_vertex("tl", 80.0, 80.0, 78.0, 78.0),
                plain_vertex("tr", 480.0, 80.0, 78.0, 78.0),
                plain_vertex("bl", 80.0, 440.0, 78.0, 78.0),
                plain_vertex("br", 480.0, 440.0, 78.0, 78.0),
            ],
            edges: vec![edge("e1", "tl", "br", ""), edge("e2", "bl", "tr", "")],
        };
        assert_gallery_invariants(&model);
    }

    #[test]
    fn gallery_05_seam_risk_has_no_invariant_violations() {
        let model = Model {
            vertices: vec![
                plain_vertex("src", 80.0, 80.0, 78.0, 78.0),
                plain_vertex("t1", 480.0, 300.0, 78.0, 78.0),
                plain_vertex("t2", 480.0, 500.0, 78.0, 78.0),
            ],
            edges: vec![edge("e1", "src", "t1", ""), edge("e2", "src", "t2", "")],
        };
        assert_gallery_invariants(&model);
    }

    #[test]
    fn gallery_06_stream_consumer_has_no_invariant_violations() {
        let model = Model {
            vertices: vec![
                plain_vertex("prod", 80.0, 280.0, 78.0, 78.0),
                plain_vertex("stream", 320.0, 280.0, 78.0, 78.0),
                plain_vertex("cons", 560.0, 280.0, 78.0, 78.0),
                plain_vertex("store", 820.0, 120.0, 78.0, 78.0),
                plain_vertex("dlq", 820.0, 440.0, 78.0, 78.0),
            ],
            edges: vec![
                edge("e1", "prod", "stream", ""),
                edge("e2", "stream", "cons", ""),
                edge("e3", "cons", "store", ""),
                edge("e4", "cons", "dlq", ""),
            ],
        };
        assert_gallery_invariants(&model);
    }

    #[test]
    fn gallery_07_complex_has_no_invariant_violations() {
        let model = Model {
            vertices: vec![
                plain_vertex("web", 80.0, 80.0, 78.0, 78.0),
                plain_vertex("api", 360.0, 80.0, 78.0, 78.0),
                plain_vertex("auth", 640.0, 80.0, 78.0, 78.0),
                plain_vertex("svc", 360.0, 340.0, 78.0, 78.0),
                plain_vertex("db", 80.0, 600.0, 78.0, 78.0),
                plain_vertex("bus", 360.0, 600.0, 78.0, 78.0),
                plain_vertex("worker", 640.0, 600.0, 78.0, 78.0),
                plain_vertex("bucket", 640.0, 340.0, 78.0, 78.0),
            ],
            edges: vec![
                edge("e1", "web", "api", ""),
                edge("e2", "api", "auth", ""),
                edge("e3", "api", "svc", ""),
                edge("e4", "svc", "db", ""),
                edge("e5", "svc", "bus", ""),
                edge("e6", "svc", "bucket", ""),
                edge("e7", "bus", "worker", ""),
                edge("e8", "worker", "db", ""),
                edge("e9", "auth", "svc", ""),
            ],
        };
        assert_gallery_invariants(&model);
    }
}
