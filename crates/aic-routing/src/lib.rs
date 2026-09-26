//! Deterministic bounded local A*, including catalog-backed crossing modules.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::time::Instant;

use aic_layout::logistics::{LogisticsComponentKind, TransportKind};
use aic_layout::rate::Rate;
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use aic_layout::geometry::*;
use aic_layout::logistics::ValidatedLogisticsComponentCatalog;
use aic_layout::projection::cardinal_index;
type Result<T> = std::result::Result<T, Diagnostic>;
const MAX_PATH_STATES: usize = 1_000_000;

/// Borrowed routed geometry; routing has no dependency on factory recipes.
pub struct Carrier<'a> {
    pub id: usize,
    pub network: usize,
    pub transport: TransportKind,
    pub rate: Rate,
    pub cells: &'a [Point],
}

pub struct RouteJob {
    pub id: usize,
    pub transport: TransportKind,
    pub rate: Rate,
}

mod negotiated;
mod reachability;
pub use negotiated::{
    GlobalRoute, GlobalRouteJob, GlobalRouteTerminal, GlobalRoutingComparison, GlobalRoutingConfig,
    GlobalRoutingOverflowCell, GlobalRoutingProblem, GlobalRoutingResult, compare_global_routing,
};
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Crossing {
    pub position: Point,
    pub direction: usize,
    pub component: String,
    pub rotation: i64,
    pub transport: TransportKind,
    pub kind: LogisticsComponentKind,
    pub carrier_chain: usize,
    pub carrier_network: usize,
}
pub type Crossings = IndexMap<(Point, usize), Crossing>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossingVisit {
    RouteCell,
    CatalogEntry,
    Candidate,
}

pub fn crossing_options(
    carriers: &[Carrier<'_>],
    job: &RouteJob,
    catalogs: &ValidatedLogisticsComponentCatalog,
    excluded: &IndexSet<Point>,
) -> Crossings {
    match crossing_options_with_visit(carriers, job, catalogs, excluded, |_| {
        Ok::<(), std::convert::Infallible>(())
    }) {
        Ok(options) => options,
        Err(never) => match never {},
    }
}

/// The visit hook lets a caller stop crossing preparation inside long route
/// and catalog scans without changing the legal crossing set.
pub fn crossing_options_with_visit<E>(
    carriers: &[Carrier<'_>],
    job: &RouteJob,
    catalogs: &ValidatedLogisticsComponentCatalog,
    excluded: &IndexSet<Point>,
    mut visit: impl FnMut(CrossingVisit) -> std::result::Result<(), E>,
) -> std::result::Result<Crossings, E> {
    let mut visits = IndexMap::<Point, Vec<(&Carrier<'_>, usize)>>::new();
    for c in carriers.iter().filter(|c| c.transport == job.transport) {
        for (i, p) in c.cells.iter().enumerate() {
            visit(CrossingVisit::RouteCell)?;
            visits.entry(*p).or_default().push((c, i));
        }
    }
    let sufficient = |quantity: i64, duration: i64, rate: Rate| {
        aic_layout::validation::within_capacity(rate, quantity, duration)
    };
    let mut poses = Vec::new();
    for component in &catalogs.catalog().components {
        visit(CrossingVisit::CatalogEntry)?;
        if component.kind == LogisticsComponentKind::Bridge
            && component.transport == job.transport
            && component.footprint.width == 1
            && component.footprint.height == 1
            && sufficient(
                component.capacity.quantity,
                component.capacity.duration_ms,
                job.rate,
            )
        {
            for &rotation in &component.allowed_rotations {
                visit(CrossingVisit::CatalogEntry)?;
                poses.push((component, rotation));
            }
        }
    }
    let mut options = IndexMap::new();
    for (p, xs) in visits {
        visit(CrossingVisit::Candidate)?;
        if excluded.contains(&p) {
            continue;
        }
        let mut carrier_direction = None;
        let mut carrier_network = None;
        let mut carrier_rate = Rate::zero();
        let mut carrier_chain = None;
        let mut straight = true;
        for (c, i) in xs {
            visit(CrossingVisit::RouteCell)?;
            if c.id == job.id || i == 0 || i + 1 == c.cells.len() {
                straight = false;
                break;
            }
            let Some(d) = direction(c.cells[i - 1], p) else {
                straight = false;
                break;
            };
            if direction(p, c.cells[i + 1]) != Some(d)
                || carrier_direction.is_some_and(|old| old != d)
                || carrier_network.is_some_and(|old| old != c.network)
            {
                straight = false;
                break;
            }
            let Ok(rate) = carrier_rate.checked_add(c.rate) else {
                straight = false;
                break;
            };
            carrier_rate = rate;
            carrier_direction = Some(d);
            carrier_network = Some(c.network);
            carrier_chain.get_or_insert(c.id);
        }
        if !straight {
            continue;
        }
        let Some(d) = carrier_direction else { continue };
        for next in [(d + 1) % 4, (d + 3) % 4] {
            let mut pose = None;
            for &(def, rotation) in &poses {
                visit(CrossingVisit::CatalogEntry)?;
                if sufficient(
                    def.capacity.quantity,
                    def.capacity.duration_ms,
                    carrier_rate,
                ) && [d, next].iter().all(|&axis| {
                    def.input_directions.iter().any(|&e| {
                        (cardinal_index(e) + rotation as usize / 90) % 4 == (axis + 2) % 4
                    }) && def
                        .output_directions
                        .iter()
                        .any(|&e| (cardinal_index(e) + rotation as usize / 90) % 4 == axis)
                }) {
                    pose = Some((def, rotation));
                    break;
                }
            }
            if let Some((def, rotation)) = pose {
                options.insert(
                    (p, next),
                    Crossing {
                        position: p,
                        direction: next,
                        component: def.id.clone(),
                        rotation,
                        transport: job.transport,
                        kind: LogisticsComponentKind::Bridge,
                        carrier_chain: carrier_chain.expect("nonempty carrier visits"),
                        carrier_network: carrier_network.expect("nonempty carrier visits"),
                    },
                );
            }
        }
    }
    Ok(options)
}

pub struct PathOptions<'a> {
    pub first: Option<usize>,
    pub last: Option<usize>,
    pub work_budget: usize,
    pub scan_budget: usize,
    pub frontier_budget: usize,
    pub deadline: Option<Instant>,
    pub crossings: &'a Crossings,
    pub crossing_limit: usize,
    /// Tie preference between a legal shared-cell step and a bridge step.
    /// Both transitions remain available and have the same physical length.
    pub prefer_crossings: bool,
    pub allow_zero_length: bool,
    /// Optional nonnegative additive step costs. These affect path ordering
    /// only; blocked cells and crossing legality remain exact graph rules.
    pub cell_penalties: Option<&'a IndexMap<Point, u32>>,
    /// Optional nonnegative additive directed-edge costs, keyed by the cell
    /// entered and the cardinal movement direction. They affect ordering only.
    pub edge_penalties: Option<&'a IndexMap<(Point, usize), u32>>,
}
#[derive(Debug, Serialize)]
pub struct PathResult {
    pub status: &'static str,
    pub expanded: usize,
    pub preflight_expanded: usize,
    pub routing_scans: usize,
    pub frontier_entries_peak: usize,
    pub preflight_status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cells: Option<Vec<Point>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crossings: Option<Vec<Crossing>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steps: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns: Option<i64>,
    #[serde(skip)]
    pub blocked_frontier: Vec<Point>,
}
impl PathResult {
    pub fn stopped(status: &'static str, expanded: usize) -> Self {
        Self {
            status,
            expanded,
            preflight_expanded: 0,
            routing_scans: 0,
            frontier_entries_peak: 0,
            preflight_status: "skipped",
            cells: None,
            crossings: None,
            steps: None,
            turns: None,
            blocked_frontier: vec![],
        }
    }
    fn found(
        cells: Vec<Point>,
        crossings: Vec<Crossing>,
        steps: i64,
        turns: i64,
        expanded: usize,
    ) -> Self {
        Self {
            status: "found",
            expanded,
            preflight_expanded: 0,
            routing_scans: 0,
            frontier_entries_peak: 0,
            preflight_status: "skipped",
            cells: Some(cells),
            crossings: Some(crossings),
            steps: Some(steps),
            turns: Some(turns),
            blocked_frontier: vec![],
        }
    }
}
#[derive(Clone)]
struct State {
    point: Point,
    direction: usize,
    used: usize,
    id: i64,
    g: i64,
    via: Option<Crossing>,
    parent: Option<usize>,
}

pub fn shortest_path(
    start: Point,
    end: Point,
    blocked: &IndexSet<Point>,
    bounds: Bounds,
    options: PathOptions<'_>,
) -> Result<PathResult> {
    let PathOptions {
        first,
        last,
        work_budget,
        scan_budget,
        frontier_budget,
        deadline,
        crossings,
        crossing_limit,
        prefer_crossings,
        allow_zero_length,
        cell_penalties,
        edge_penalties,
    } = options;
    let width = bounds.width();
    let height = bounds.height();
    let area = i128::from(width) * i128::from(height);
    if width < 1
        || height < 1
        || area > 250000
        || work_budget < 1
        || scan_budget < 1
        || frontier_budget < 1
        || first.is_some_and(|d| d > 3)
        || last.is_some_and(|d| d > 3)
    {
        let mut d = Diagnostic::new(
            "path-invalid-input",
            "Invalid grid, endpoint, direction, crossing or work budget",
            serde_json::Value::Null,
        );
        d.stage = "local-path";
        return Err(d);
    }
    let mut penalty_scans = 0usize;
    let invalid_penalty = || {
        let mut diagnostic = Diagnostic::new(
            "path-invalid-input",
            "Invalid grid, endpoint, direction, crossing or work budget",
            serde_json::Value::Null,
        );
        diagnostic.stage = "local-path";
        diagnostic
    };
    if let Some(penalties) = cell_penalties {
        for (&point, &penalty) in penalties {
            penalty_scans += 1;
            if penalty_scans >= scan_budget {
                let mut result = PathResult::stopped("scan-budget", 0);
                result.routing_scans = penalty_scans;
                return Ok(result);
            }
            if penalty_scans % 256 == 0
                && deadline.is_some_and(|deadline| Instant::now() >= deadline)
            {
                let mut result = PathResult::stopped("deadline", 0);
                result.routing_scans = penalty_scans;
                return Ok(result);
            }
            if !bounds.contains(point) || penalty > 1_000_000 {
                return Err(invalid_penalty());
            }
        }
    }
    if let Some(penalties) = edge_penalties {
        for (&(point, direction), &penalty) in penalties {
            penalty_scans += 1;
            if penalty_scans >= scan_budget {
                let mut result = PathResult::stopped("scan-budget", 0);
                result.routing_scans = penalty_scans;
                return Ok(result);
            }
            if penalty_scans % 256 == 0
                && deadline.is_some_and(|deadline| Instant::now() >= deadline)
            {
                let mut result = PathResult::stopped("deadline", 0);
                result.routing_scans = penalty_scans;
                return Ok(result);
            }
            if !bounds.contains(point) || direction > 3 || penalty > 1_000_000 {
                return Err(invalid_penalty());
            }
        }
    }
    if !bounds.contains(start)
        || !bounds.contains(end)
        || blocked.contains(&start)
        || blocked.contains(&end)
    {
        return Ok(PathResult::stopped("blocked-endpoint", 0));
    }
    if start == end {
        return Ok(if allow_zero_length && first.is_none() && last.is_none() {
            PathResult::found(vec![start], vec![], 0, 0, 0)
        } else {
            PathResult::stopped("same-endpoint", 0)
        });
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Ok(PathResult::stopped("deadline", 0));
    }
    if penalty_scans >= scan_budget {
        let mut result = PathResult::stopped("scan-budget", 0);
        result.routing_scans = penalty_scans;
        return Ok(result);
    }
    // Bound necessary reachability work independently, and charge it to the
    // same transaction allowance. Small calls retain most of their A* budget.
    let probe = reachability::probe(
        start,
        end,
        blocked,
        bounds,
        &PathOptions {
            first,
            last,
            work_budget,
            scan_budget: scan_budget - penalty_scans,
            frontier_budget,
            deadline,
            crossings,
            crossing_limit,
            prefer_crossings,
            allow_zero_length,
            cell_penalties: None,
            edge_penalties: None,
        },
        256.min(work_budget / 4),
    );
    if matches!(probe.status, "deadline" | "scan-budget" | "frontier-budget") {
        let mut result = PathResult::stopped(probe.status, probe.expanded);
        result.preflight_expanded = probe.expanded;
        result.preflight_status = probe.status;
        result.routing_scans = penalty_scans.saturating_add(probe.scans);
        result.frontier_entries_peak = probe.frontier_peak;
        return Ok(result);
    }
    if probe.status == "unreachable" {
        let mut result = PathResult::stopped("no-path", probe.expanded);
        result.preflight_expanded = probe.expanded;
        result.preflight_status = probe.status;
        result.routing_scans = penalty_scans.saturating_add(probe.scans);
        result.frontier_entries_peak = probe.frontier_peak;
        result.blocked_frontier = probe.frontier;
        return Ok(result);
    }
    // A simple route cannot cross more cells than the bounded grid contains.
    // This reduces a redundant caller limit without imposing a game rule.
    let crossing_limit = crossing_limit.min(area as usize);
    let weight = i64::try_from(area * 4 * (crossing_limit as i128 + 1) + 1)
        .expect("bounded grid and crossing count fit i64");
    let id = |p: Point, d: usize, used: usize, entered_bridge: bool| {
        ((((p.y - bounds.min_y) * width + p.x - bounds.min_x) * 5 + d as i64)
            * (crossing_limit as i64 + 1)
            + used as i64)
            * 2
            + i64::from(entered_bridge)
    };
    let initial = id(start, 4, 0, false);
    let mut states = vec![State {
        point: start,
        direction: 4,
        used: 0,
        id: initial,
        g: 0,
        via: None,
        parent: None,
    }];
    let mut heap = BinaryHeap::from([Reverse((
        start.distance(end) * weight,
        0_i64,
        0_i64,
        initial,
        0_usize,
    ))]);
    let mut best = IndexMap::from([(initial, 0)]);
    let mut expanded = probe.expanded;
    let mut scans = penalty_scans.saturating_add(probe.scans);
    let mut frontier_peak = probe.frontier_peak.max(heap.len());
    // Bound diagnostic collection independently of the path-state allocation.
    let mut frontier = IndexSet::new();
    let stopped = |status, expanded, scans, frontier_peak, frontier: IndexSet<Point>| {
        let mut result = PathResult::stopped(status, expanded);
        result.blocked_frontier = frontier.into_iter().collect();
        result.preflight_expanded = probe.expanded;
        result.preflight_status = probe.status;
        result.routing_scans = scans;
        result.frontier_entries_peak = frontier_peak;
        result
    };
    while let Some(Reverse((_, _, _, _, pi))) = heap.pop() {
        scans = scans.saturating_add(1);
        if scans > scan_budget {
            return Ok(stopped(
                "scan-budget",
                expanded,
                scans,
                frontier_peak,
                frontier,
            ));
        }
        if expanded % 64 == 0 && deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Ok(stopped(
                "deadline",
                expanded,
                scans,
                frontier_peak,
                frontier,
            ));
        }
        let p = states[pi].clone();
        if best.get(&p.id) != Some(&p.g) {
            continue;
        }
        if expanded == work_budget {
            return Ok(stopped(
                "work-budget",
                expanded,
                scans,
                frontier_peak,
                frontier,
            ));
        }
        expanded += 1;
        if p.point == end && (last.is_none() || last == Some(p.direction)) {
            let mut cells = Vec::new();
            let mut crossings = Vec::new();
            let mut current = Some(pi);
            while let Some(i) = current {
                scans = scans.saturating_add(1);
                if scans > scan_budget {
                    return Ok(stopped(
                        "scan-budget",
                        expanded,
                        scans,
                        frontier_peak,
                        frontier,
                    ));
                }
                if cells.len() % 64 == 0
                    && deadline.is_some_and(|deadline| Instant::now() >= deadline)
                {
                    return Ok(stopped(
                        "deadline",
                        expanded,
                        scans,
                        frontier_peak,
                        frontier,
                    ));
                }
                let s = &states[i];
                cells.push(s.point);
                if let Some(via) = &s.via {
                    crossings.push(via.clone());
                }
                current = s.parent;
            }
            cells.reverse();
            crossings.reverse();
            scans = scans.saturating_add(cells.len());
            if scans > scan_budget {
                return Ok(stopped(
                    "scan-budget",
                    expanded,
                    scans,
                    frontier_peak,
                    frontier,
                ));
            }
            if cells.iter().collect::<IndexSet<_>>().len() != cells.len() {
                return Ok(stopped(
                    "repeated-cell",
                    expanded,
                    scans,
                    frontier_peak,
                    frontier,
                ));
            }
            let steps = i64::try_from(cells.len() - 1).expect("bounded path length fits i64");
            let turns = p.g % weight;
            let mut result = PathResult::found(cells, crossings, steps, turns, expanded);
            result.preflight_expanded = probe.expanded;
            result.preflight_status = probe.status;
            result.routing_scans = scans;
            result.frontier_entries_peak = frontier_peak;
            return Ok(result);
        }
        for d in 0..4 {
            scans = scans.saturating_add(1);
            if scans > scan_budget {
                return Ok(stopped(
                    "scan-budget",
                    expanded,
                    scans,
                    frontier_peak,
                    frontier,
                ));
            }
            if p.via.is_some() && d != p.direction {
                continue;
            }
            if p.direction == 4 && first.is_some_and(|first| d != first) {
                continue;
            }
            if p.direction != 4 && (d + 2) % 4 == p.direction {
                continue;
            }
            let q = p.point.offset(d);
            if !bounds.contains(q) || q == start || (q == end && last.is_some_and(|last| d != last))
            {
                continue;
            }
            let site = if crossing_limit == 0 {
                None
            } else {
                crossings.get(&(q, d))
            };
            if blocked.contains(&q) && (site.is_none() || p.used >= crossing_limit) {
                if frontier.len() < 2048 {
                    frontier.insert(q);
                }
                continue;
            }
            if site.is_some_and(|site| site.position != q || site.direction != d) {
                let mut e = Diagnostic::new(
                    "path-invalid-crossing",
                    "Crossing module does not match its entry cell and direction",
                    serde_json::Value::Null,
                );
                e.stage = "local-path";
                return Err(e);
            }
            let penalty = cell_penalties
                .and_then(|penalties| penalties.get(&q))
                .copied()
                .unwrap_or(0)
                .saturating_add(
                    edge_penalties
                        .and_then(|penalties| penalties.get(&(q, d)))
                        .copied()
                        .unwrap_or(0),
                )
                .min(1_000_000);
            let Some(g) = (i64::from(penalty) + 1)
                .checked_mul(weight)
                .and_then(|step| p.g.checked_add(step))
                .and_then(|cost| cost.checked_add(i64::from(p.direction != 4 && p.direction != d)))
            else {
                return Ok(stopped(
                    "resource-limit",
                    expanded,
                    scans,
                    frontier_peak,
                    frontier,
                ));
            };
            for choice in 0..=usize::from(site.is_some()) {
                scans = scans.saturating_add(1);
                if scans > scan_budget {
                    return Ok(stopped(
                        "scan-budget",
                        expanded,
                        scans,
                        frontier_peak,
                        frontier,
                    ));
                }
                let via = if choice == 0 { None } else { site };
                if via.is_none() && blocked.contains(&q)
                    || via.is_some() && p.used >= crossing_limit
                {
                    continue;
                }
                let used = p.used + usize::from(via.is_some());
                let qid = id(q, d, used, via.is_some());
                if best.get(&qid).is_some_and(|&old| old <= g) {
                    continue;
                }
                if states.len() == MAX_PATH_STATES {
                    return Ok(stopped(
                        "resource-limit",
                        expanded,
                        scans,
                        frontier_peak,
                        frontier,
                    ));
                }
                best.insert(qid, g);
                let qi = states.len();
                states.push(State {
                    point: q,
                    direction: d,
                    used,
                    id: qid,
                    g,
                    via: via.cloned(),
                    parent: Some(pi),
                });
                let preference = if prefer_crossings {
                    -(used as i64)
                } else {
                    used as i64
                };
                let Some(priority) = q
                    .distance(end)
                    .checked_mul(weight)
                    .and_then(|distance| g.checked_add(distance))
                else {
                    return Ok(stopped(
                        "resource-limit",
                        expanded,
                        scans,
                        frontier_peak,
                        frontier,
                    ));
                };
                heap.push(Reverse((priority, g, preference, qid, qi)));
                frontier_peak = frontier_peak.max(heap.len());
                if frontier_peak > frontier_budget {
                    return Ok(stopped(
                        "frontier-budget",
                        expanded,
                        scans,
                        frontier_peak,
                        frontier,
                    ));
                }
            }
        }
    }
    Ok(stopped("no-path", expanded, scans, frontier_peak, frontier))
}
