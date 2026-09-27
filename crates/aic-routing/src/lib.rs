//! Deterministic bounded local A*, including catalog-backed crossing modules.

use std::time::Instant;

use aic_layout::logistics::{LogisticsComponentKind, TransportKind};
use aic_layout::rate::Rate;
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use aic_layout::geometry::*;
use aic_layout::logistics::ValidatedLogisticsComponentCatalog;
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

mod crossing_options_cursor;
mod negotiated;
pub mod prepared;
mod reachability;
pub use crossing_options_cursor::CrossingOptionsCursor;
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
    let mut cursor = CrossingOptionsCursor::new();
    cursor.advance(carriers, job, catalogs, excluded, &mut visit)?;
    Ok(cursor.finish())
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
pub fn shortest_path(
    start: Point,
    end: Point,
    blocked: &IndexSet<Point>,
    bounds: Bounds,
    options: PathOptions<'_>,
) -> Result<PathResult> {
    use prepared::{Advance, Frontier, PreflightStatus, PreparedGraph, Reservation};

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
    let width = i128::from(bounds.max_x) - i128::from(bounds.min_x) + 1;
    let height = i128::from(bounds.max_y) - i128::from(bounds.min_y) + 1;
    let invalid_input = || {
        let mut diagnostic = Diagnostic::new(
            "path-invalid-input",
            "Invalid grid, endpoint, direction, crossing or work budget",
            serde_json::Value::Null,
        );
        diagnostic.stage = "local-path";
        diagnostic
    };
    if width < 1
        || height < 1
        || work_budget == 0
        || scan_budget == 0
        || frontier_budget == 0
        || first.is_some_and(|direction| direction > 3)
        || last.is_some_and(|direction| direction > 3)
    {
        return Err(invalid_input());
    }
    let mut input_scans = 0usize;
    let mut scan_input = || -> Option<PathResult> {
        input_scans = input_scans.saturating_add(1);
        if input_scans >= scan_budget {
            let mut result = PathResult::stopped("scan-budget", 0);
            result.routing_scans = input_scans;
            return Some(result);
        }
        if input_scans % 256 == 0 && deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            let mut result = PathResult::stopped("deadline", 0);
            result.routing_scans = input_scans;
            return Some(result);
        }
        None
    };
    if let Some(penalties) = cell_penalties {
        for (&point, &penalty) in penalties {
            if let Some(stopped) = scan_input() {
                return Ok(stopped);
            }
            if !bounds.contains(point) || penalty > 1_000_000 {
                return Err(invalid_input());
            }
        }
    }
    if let Some(penalties) = edge_penalties {
        for (&(point, direction), &penalty) in penalties {
            if let Some(stopped) = scan_input() {
                return Ok(stopped);
            }
            if !bounds.contains(point) || direction > 3 || penalty > 1_000_000 {
                return Err(invalid_input());
            }
        }
    }
    for (&(point, direction), crossing) in crossings {
        if let Some(stopped) = scan_input() {
            return Ok(stopped);
        }
        if point != crossing.position || direction != crossing.direction {
            let mut diagnostic = Diagnostic::new(
                "path-invalid-crossing",
                "Crossing module does not match its entry cell and direction",
                serde_json::Value::Null,
            );
            diagnostic.stage = "local-path";
            return Err(diagnostic);
        }
    }
    if !bounds.contains(start)
        || !bounds.contains(end)
        || blocked.contains(&start)
        || blocked.contains(&end)
    {
        let mut result = PathResult::stopped("blocked-endpoint", 0);
        result.routing_scans = input_scans;
        return Ok(result);
    }
    if start == end {
        let mut result = if allow_zero_length && first.is_none() && last.is_none() {
            PathResult::found(vec![start], vec![], 0, 0, 0)
        } else {
            PathResult::stopped("same-endpoint", 0)
        };
        result.routing_scans = input_scans;
        return Ok(result);
    }
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        let mut result = PathResult::stopped("deadline", 0);
        result.routing_scans = input_scans;
        return Ok(result);
    }

    // A one-shot caller owns these caches only for this call. The input
    // collections are borrowed so preparation does not copy the full grid.
    let mut graph = PreparedGraph::borrowed(bounds, blocked, crossings, deadline.is_some());
    let hard_deadline = deadline.unwrap_or_else(Instant::now);
    let mut reserve = |_| Reservation::Granted;
    let preflight = graph.advance_preflight(
        start,
        end,
        first,
        last,
        crossing_limit,
        work_budget,
        work_budget,
        scan_budget - input_scans,
        frontier_budget,
        hard_deadline,
        hard_deadline,
        &mut reserve,
    );
    let decorate_input = |mut result: PathResult| {
        result.routing_scans = result.routing_scans.saturating_add(input_scans);
        result
    };
    let (preflight_expanded, preflight_scans) = (preflight.expanded, preflight.scans);
    match preflight.status {
        PreflightStatus::Stopped(result) => return Ok(decorate_input(result)),
        PreflightStatus::Pending => {
            let status = if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                "deadline"
            } else if preflight_scans >= scan_budget - input_scans {
                "scan-budget"
            } else {
                "work-budget"
            };
            let mut result = PathResult::stopped(status, preflight_expanded);
            result.preflight_expanded = preflight_expanded;
            result.preflight_status = status;
            result.routing_scans = input_scans.saturating_add(preflight_scans);
            result.frontier_entries_peak = preflight.frontier_peak;
            return Ok(result);
        }
        PreflightStatus::Ready => {}
    }
    if preflight.frontier_peak > frontier_budget {
        let mut result = PathResult::stopped("frontier-budget", preflight_expanded);
        result.preflight_expanded = preflight_expanded;
        result.preflight_status = "frontier-budget";
        result.routing_scans = input_scans.saturating_add(preflight_scans);
        result.frontier_entries_peak = preflight.frontier_peak;
        return Ok(result);
    }

    let mut frontier = match Frontier::new_borrowed(
        &mut graph,
        start,
        end,
        first,
        last,
        crossing_limit,
        prefer_crossings,
        work_budget,
        preflight_expanded,
        preflight_scans,
        cell_penalties,
        edge_penalties,
        &mut reserve,
    ) {
        Ok(frontier) => frontier,
        Err(result) => return Ok(decorate_input(result)),
    };
    let remaining_work = work_budget.saturating_sub(preflight_expanded);
    let remaining_scans = scan_budget
        .saturating_sub(input_scans)
        .saturating_sub(preflight_scans);
    match frontier.advance(
        &mut graph,
        remaining_work,
        remaining_scans,
        hard_deadline,
        hard_deadline,
        remaining_work,
        remaining_scans,
        frontier_budget,
        &mut reserve,
    ) {
        Advance::Complete(result) => Ok(decorate_input(result)),
        Advance::Pending => {
            let (expanded, scans, frontier_peak) = frontier.work_counts();
            let status = if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                "deadline"
            } else if scans >= remaining_scans {
                "scan-budget"
            } else {
                "work-budget"
            };
            let mut result =
                PathResult::stopped(status, preflight_expanded.saturating_add(expanded));
            result.preflight_expanded = preflight_expanded;
            result.preflight_status = frontier.preflight_status();
            result.routing_scans = input_scans
                .saturating_add(preflight_scans)
                .saturating_add(scans);
            result.frontier_entries_peak = frontier_peak;
            Ok(result)
        }
    }
}
