//! Deterministic bounded local A*, including catalog-backed crossing modules.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use aic_layout::logistics::{LogisticsComponentKind, TransportKind};
use aic_layout::rate::Rate;
use indexmap::{IndexMap, IndexSet};
use serde::Serialize;

use aic_layout::geometry::*;
use aic_layout::logistics::ValidatedLogisticsComponentCatalog;
use aic_layout::projection::cardinal_index;
type Result<T> = std::result::Result<T, Diagnostic>;

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

#[derive(Clone, Debug, Serialize)]
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

pub fn crossing_options(
    carriers: &[Carrier<'_>],
    job: &RouteJob,
    catalogs: &ValidatedLogisticsComponentCatalog,
    excluded: &IndexSet<Point>,
) -> Crossings {
    let mut visits = IndexMap::<Point, Vec<(&Carrier<'_>, usize)>>::new();
    for c in carriers.iter().filter(|c| c.transport == job.transport) {
        for (i, p) in c.cells.iter().enumerate() {
            visits.entry(*p).or_default().push((c, i));
        }
    }
    let sufficient = |quantity: i64, duration: i64, rate: Rate| {
        aic_layout::validation::within_capacity(rate, quantity, duration)
    };
    let poses: Vec<_> = catalogs
        .catalog()
        .components
        .iter()
        .filter(|c| {
            c.kind == LogisticsComponentKind::Bridge
                && c.transport == job.transport
                && c.footprint.width == 1
                && c.footprint.height == 1
                && sufficient(c.capacity.quantity, c.capacity.duration_ms, job.rate)
        })
        .flat_map(|c| {
            c.allowed_rotations
                .iter()
                .map(move |&rotation| (c, rotation))
        })
        .collect();
    let mut options = IndexMap::new();
    for (p, xs) in visits {
        if xs.len() != 1 || excluded.contains(&p) {
            continue;
        }
        let (c, i) = xs[0];
        if c.id == job.id || i == 0 || i + 1 == c.cells.len() {
            continue;
        }
        let Some(d) = direction(c.cells[i - 1], p) else {
            continue;
        };
        if direction(p, c.cells[i + 1]) != Some(d) {
            continue;
        }
        for next in [(d + 1) % 4, (d + 3) % 4] {
            let pose = poses.iter().find(|&&(def, rotation)| {
                sufficient(def.capacity.quantity, def.capacity.duration_ms, c.rate)
                    && [d, next].iter().all(|&axis| {
                        def.input_directions.iter().any(|&e| {
                            (cardinal_index(e) + rotation as usize / 90) % 4 == (axis + 2) % 4
                        }) && def
                            .output_directions
                            .iter()
                            .any(|&e| (cardinal_index(e) + rotation as usize / 90) % 4 == axis)
                    })
            });
            if let Some(&(def, rotation)) = pose {
                options.insert(
                    (p, next),
                    Crossing {
                        position: p,
                        direction: next,
                        component: def.id.clone(),
                        rotation,
                        transport: job.transport,
                        kind: LogisticsComponentKind::Bridge,
                        carrier_chain: c.id,
                        carrier_network: c.network,
                    },
                );
            }
        }
    }
    options
}

pub struct PathOptions<'a> {
    pub first: Option<usize>,
    pub last: Option<usize>,
    pub work_budget: usize,
    pub crossings: &'a Crossings,
    pub crossing_limit: usize,
    pub allow_zero_length: bool,
    /// Optional nonnegative additive step costs. These affect path ordering
    /// only; blocked cells and crossing legality remain exact graph rules.
    pub cell_penalties: Option<&'a IndexMap<Point, u32>>,
}
#[derive(Debug, Serialize)]
pub struct PathResult {
    pub status: &'static str,
    pub expanded: usize,
    pub preflight_expanded: usize,
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
        crossings,
        crossing_limit,
        allow_zero_length,
        cell_penalties,
    } = options;
    let width = bounds.width();
    let height = bounds.height();
    let area = i128::from(width) * i128::from(height);
    if width < 1
        || height < 1
        || area > 250000
        || work_budget < 1
        || first.is_some_and(|d| d > 3)
        || last.is_some_and(|d| d > 3)
        || crossing_limit > 8
        || cell_penalties.is_some_and(|penalties| {
            penalties
                .iter()
                .any(|(point, &penalty)| !bounds.contains(*point) || penalty > 1_000_000)
        })
    {
        let mut d = Diagnostic::new(
            "path-invalid-input",
            "Invalid grid, endpoint, direction, crossing or work budget",
            serde_json::Value::Null,
        );
        d.stage = "local-path";
        return Err(d);
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
            crossings,
            crossing_limit,
            allow_zero_length,
            cell_penalties: None,
        },
        256.min(work_budget / 4),
    );
    if probe.status == "unreachable" {
        let mut result = PathResult::stopped("no-path", probe.expanded);
        result.preflight_expanded = probe.expanded;
        result.preflight_status = probe.status;
        result.blocked_frontier = probe.frontier;
        return Ok(result);
    }
    let weight = width * height * 4 * (crossing_limit as i64 + 1) + 1;
    let id = |p: Point, d: usize, used: usize| {
        (((p.y - bounds.min_y) * width + p.x - bounds.min_x) * 5 + d as i64)
            * (crossing_limit as i64 + 1)
            + used as i64
    };
    let initial = id(start, 4, 0);
    let mut states = vec![State {
        point: start,
        direction: 4,
        used: 0,
        id: initial,
        g: 0,
        via: None,
    }];
    let mut heap = BinaryHeap::from([Reverse((
        start.distance(end) * weight,
        0_i64,
        initial,
        0_usize,
    ))]);
    let mut best = IndexMap::from([(initial, 0)]);
    // Match the oracle's map from state ID to its latest predecessor object.
    let mut parents = IndexMap::<i64, usize>::new();
    let mut expanded = probe.expanded;
    // Bound diagnostic collection independently of the path-state allocation.
    let mut frontier = IndexSet::new();
    let stopped = |status, expanded, frontier: IndexSet<Point>| {
        let mut result = PathResult::stopped(status, expanded);
        result.blocked_frontier = frontier.into_iter().collect();
        result.preflight_expanded = probe.expanded;
        result.preflight_status = probe.status;
        result
    };
    while let Some(Reverse((_, _, _, pi))) = heap.pop() {
        let p = states[pi].clone();
        if best.get(&p.id) != Some(&p.g) {
            continue;
        }
        if expanded == work_budget {
            return Ok(stopped("work-budget", expanded, frontier));
        }
        expanded += 1;
        if p.point == end && (last.is_none() || last == Some(p.direction)) {
            let mut cells = Vec::new();
            let mut crossings = Vec::new();
            let mut current = Some(pi);
            while let Some(i) = current {
                let s = &states[i];
                cells.push(s.point);
                if let Some(via) = &s.via {
                    crossings.push(via.clone());
                }
                current = parents.get(&s.id).copied();
            }
            cells.reverse();
            crossings.reverse();
            if cells.iter().collect::<IndexSet<_>>().len() != cells.len() {
                return Ok(stopped("repeated-cell", expanded, frontier));
            }
            let steps = i64::try_from(cells.len() - 1).expect("bounded path length fits i64");
            let mut result = PathResult::found(cells, crossings, steps, p.g % weight, expanded);
            result.preflight_expanded = probe.expanded;
            result.preflight_status = probe.status;
            return Ok(result);
        }
        for d in 0..4 {
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
            let via = if blocked.contains(&q) {
                let Some(site) = crossings.get(&(q, d)) else {
                    if frontier.len() < 2048 {
                        frontier.insert(q);
                    }
                    continue;
                };
                if p.used >= crossing_limit {
                    if frontier.len() < 2048 {
                        frontier.insert(q);
                    }
                    continue;
                }
                if site.position != q || site.direction != d {
                    let mut e = Diagnostic::new(
                        "path-invalid-crossing",
                        "Crossing module does not match its entry cell and direction",
                        serde_json::Value::Null,
                    );
                    e.stage = "local-path";
                    return Err(e);
                }
                Some(site.clone())
            } else {
                None
            };
            if !bounds.contains(q) || q == start || (q == end && last.is_some_and(|last| d != last))
            {
                continue;
            }
            let used = p.used + usize::from(via.is_some());
            let penalty = cell_penalties
                .and_then(|penalties| penalties.get(&q))
                .copied()
                .unwrap_or(0);
            let g = p.g
                + (i64::from(penalty) + 1) * weight
                + i64::from(p.direction != 4 && p.direction != d);
            let qid = id(q, d, used);
            if best.get(&qid).is_some_and(|&old| old <= g) {
                continue;
            }
            best.insert(qid, g);
            parents.insert(qid, pi);
            let qi = states.len();
            states.push(State {
                point: q,
                direction: d,
                used,
                id: qid,
                g,
                via,
            });
            heap.push(Reverse((g + q.distance(end) * weight, g, qid, qi)));
        }
    }
    Ok(stopped("no-path", expanded, frontier))
}
