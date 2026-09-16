//! Research-only fixed-placement comparison between irreversible sequential
//! routing and bounded negotiated congestion.

use std::cmp::Reverse;
use std::collections::{BTreeSet, BinaryHeap};

use aic_layout::geometry::{Bounds, Diagnostic, Point, direction, layer};
use aic_layout::logistics::TransportKind;
use aic_layout::rate::Rate;
use serde::{Deserialize, Serialize};

const STEP_COST: u64 = 100;
const TURN_COST: u64 = 1;
const MAX_AREA: usize = 250_000;
const MAX_JOBS: usize = 4_096;
const MAX_TERMINALS: usize = 65_536;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GlobalRouteTerminal {
    pub id: String,
    pub point: Point,
    /// Required direction of the first source step or final target step.
    pub direction: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GlobalRouteJob {
    pub id: String,
    pub commodity: String,
    pub transport: TransportKind,
    pub rate: Rate,
    pub sources: Vec<GlobalRouteTerminal>,
    pub targets: Vec<GlobalRouteTerminal>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GlobalRoutingProblem {
    pub bounds: Bounds,
    /// Hard obstacles apply to both transport layers. Terminals must be outside them.
    pub blocked: Vec<Point>,
    pub jobs: Vec<GlobalRouteJob>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GlobalRoutingConfig {
    pub max_iterations: usize,
    pub work_per_route: usize,
    pub present_penalty: u32,
    pub history_increment: u32,
}

impl Default for GlobalRoutingConfig {
    fn default() -> Self {
        Self {
            max_iterations: 24,
            work_per_route: 100_000,
            present_penalty: 8,
            history_increment: 2,
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct GlobalRoute {
    pub job: String,
    pub source_terminal: String,
    pub target_terminal: String,
    pub cells: Vec<Point>,
    pub turns: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct GlobalRoutingOverflowCell {
    pub transport: TransportKind,
    pub point: Point,
    pub usage: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct GlobalRoutingResult {
    pub complete: bool,
    pub legal: bool,
    pub iterations: usize,
    pub selected_iteration: usize,
    pub routed: usize,
    pub unresolved: Vec<String>,
    pub total_overflow: usize,
    pub maximum_overflow: usize,
    pub overused_cells: usize,
    pub overflow_cells: Vec<GlobalRoutingOverflowCell>,
    pub route_searches: usize,
    pub expanded: usize,
    pub wire_cells: usize,
    pub turns: usize,
    pub routes: Vec<GlobalRoute>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct GlobalRoutingComparison {
    pub schema_version: u32,
    pub sequential_policy: &'static str,
    pub negotiated_policy: &'static str,
    pub config: GlobalRoutingConfig,
    pub sequential: GlobalRoutingResult,
    pub negotiated: GlobalRoutingResult,
}

pub fn compare_global_routing(
    problem: &GlobalRoutingProblem,
    config: &GlobalRoutingConfig,
) -> Result<GlobalRoutingComparison, Diagnostic> {
    let prepared = Prepared::new(problem, config)?;
    let order = (0..problem.jobs.len()).collect::<Vec<_>>();
    let sequential = prepared.route_iteration(&order, &prepared.empty_costs(), true);
    let negotiated = prepared.negotiate(config);
    Ok(GlobalRoutingComparison {
        schema_version: 1,
        sequential_policy: "stable-input-order/commit-exclusive",
        negotiated_policy: "completion-first/present-history/rip-up-all/conflict-order",
        config: config.clone(),
        sequential,
        negotiated,
    })
}

struct Prepared<'a> {
    problem: &'a GlobalRoutingProblem,
    width: usize,
    work_per_route: usize,
    present_penalty: u32,
    blocked: Vec<bool>,
    reserved: [Vec<bool>; 2],
}

impl<'a> Prepared<'a> {
    fn new(
        problem: &'a GlobalRoutingProblem,
        config: &GlobalRoutingConfig,
    ) -> Result<Self, Diagnostic> {
        let width = usize::try_from(problem.bounds.width()).ok();
        let height = usize::try_from(problem.bounds.height()).ok();
        let area = width.and_then(|w| height.and_then(|h| w.checked_mul(h)));
        if area.is_none_or(|area| area == 0 || area > MAX_AREA)
            || problem.jobs.is_empty()
            || problem.jobs.len() > MAX_JOBS
            || config.max_iterations == 0
            || config.max_iterations > 1_024
            || config.work_per_route == 0
            || config.work_per_route > 10_000_000
            || config.present_penalty == 0
            || config.history_increment == 0
        {
            return Err(error(
                "global-routing-input",
                "invalid bounds, job count or routing work configuration",
            ));
        }
        let (width, area) = (width.unwrap(), area.unwrap());
        let mut prepared = Self {
            problem,
            width,
            work_per_route: config.work_per_route,
            present_penalty: config.present_penalty,
            blocked: vec![false; area],
            reserved: [vec![false; area], vec![false; area]],
        };
        for &point in &problem.blocked {
            let Some(index) = prepared.index(point) else {
                return Err(error(
                    "global-routing-obstacle",
                    "hard obstacle lies outside the routing bounds",
                ));
            };
            prepared.blocked[index] = true;
        }
        let mut job_ids = BTreeSet::new();
        let mut terminal_count = 0usize;
        for job in &problem.jobs {
            if !job_ids.insert(job.id.as_str())
                || job.commodity.is_empty()
                || job.rate.numerator <= 0
                || job.rate.denominator <= 0
                || job.sources.is_empty()
                || job.targets.is_empty()
            {
                return Err(error(
                    "global-routing-job",
                    "job IDs must be unique and endpoint domains must be nonempty",
                ));
            }
            terminal_count = terminal_count
                .checked_add(job.sources.len() + job.targets.len())
                .unwrap_or(usize::MAX);
            let mut ids = BTreeSet::new();
            for terminal in job.sources.iter().chain(&job.targets) {
                let Some(index) = prepared.index(terminal.point) else {
                    return Err(error(
                        "global-routing-terminal",
                        "terminal lies outside the routing bounds",
                    ));
                };
                if prepared.blocked[index]
                    || terminal.direction.is_some_and(|direction| direction > 3)
                    || !ids.insert(terminal.id.as_str())
                {
                    return Err(error(
                        "global-routing-terminal",
                        "terminal is blocked, duplicated within a job, or has an invalid direction",
                    ));
                }
                prepared.reserved[layer(job.transport)][index] = true;
            }
        }
        if terminal_count > MAX_TERMINALS {
            return Err(error(
                "global-routing-terminal-guard",
                "terminal materialization exceeds the safety guard",
            ));
        }
        Ok(prepared)
    }

    fn empty_costs(&self) -> [Vec<u32>; 2] {
        [vec![0; self.blocked.len()], vec![0; self.blocked.len()]]
    }

    fn negotiate(&self, config: &GlobalRoutingConfig) -> GlobalRoutingResult {
        let mut history = self.empty_costs();
        let mut order = (0..self.problem.jobs.len()).collect::<Vec<_>>();
        let mut best: Option<GlobalRoutingResult> = None;
        let mut searches = 0usize;
        let mut expanded = 0usize;
        let mut performed = 0usize;
        for iteration in 1..=config.max_iterations {
            performed = iteration;
            let mut candidate = self.route_iteration(&order, &history, false);
            searches += candidate.route_searches;
            expanded += candidate.expanded;
            candidate.iterations = iteration;
            candidate.selected_iteration = iteration;
            if best
                .as_ref()
                .is_none_or(|current| result_key(&candidate) < result_key(current))
            {
                best = Some(candidate.clone());
            }
            if candidate.legal {
                break;
            }
            let usage = self.usage(&candidate.routes);
            for transport in 0..2 {
                for (index, &count) in usage[transport].iter().enumerate() {
                    if count > 1 {
                        history[transport][index] = history[transport][index]
                            .saturating_add(config.history_increment.saturating_mul(count - 1));
                    }
                }
            }
            let mut conflict = vec![usize::MAX; self.problem.jobs.len()];
            for (job_index, job) in self.problem.jobs.iter().enumerate() {
                if let Some(route) = candidate.routes.iter().find(|route| route.job == job.id) {
                    let transport = layer(job.transport);
                    conflict[job_index] = route
                        .cells
                        .iter()
                        .filter_map(|&point| self.index(point))
                        .map(|index| usage[transport][index].saturating_sub(1) as usize)
                        .sum();
                }
            }
            order.sort_by_key(|&job_index| (Reverse(conflict[job_index]), job_index));
        }
        let mut best = best.expect("validated configuration performs at least one iteration");
        best.iterations = performed;
        best.route_searches = searches;
        best.expanded = expanded;
        best
    }

    fn route_iteration(
        &self,
        order: &[usize],
        history: &[Vec<u32>; 2],
        sequential: bool,
    ) -> GlobalRoutingResult {
        let mut usage = [
            vec![0u32; self.blocked.len()],
            vec![0u32; self.blocked.len()],
        ];
        let mut routes = Vec::new();
        let mut unresolved = Vec::new();
        let mut expanded = 0usize;
        for &job_index in order {
            let job = &self.problem.jobs[job_index];
            let transport = layer(job.transport);
            let search = self.find_route(job, &usage[transport], &history[transport], sequential);
            expanded += search.expanded;
            if let Some(route) = search.route {
                for &point in &route.cells {
                    usage[transport][self.index(point).unwrap()] += 1;
                }
                routes.push(route);
            } else {
                unresolved.push(job.id.clone());
            }
        }
        routes.sort_by(|left, right| left.job.cmp(&right.job));
        unresolved.sort();
        let (total_overflow, maximum_overflow, overflow_cells) = self.overflow(&usage);
        let wire_cells = routes.iter().map(|route| route.cells.len()).sum();
        let turns = routes.iter().map(|route| route.turns).sum();
        let complete = unresolved.is_empty();
        GlobalRoutingResult {
            complete,
            legal: complete && total_overflow == 0,
            iterations: 1,
            selected_iteration: 1,
            routed: routes.len(),
            unresolved,
            total_overflow,
            maximum_overflow,
            overused_cells: overflow_cells.len(),
            overflow_cells,
            route_searches: order.len(),
            expanded,
            wire_cells,
            turns,
            routes,
        }
    }

    fn find_route(
        &self,
        job: &GlobalRouteJob,
        usage: &[u32],
        history: &[u32],
        sequential: bool,
    ) -> SearchResult {
        let transport = layer(job.transport);
        let mut states = Vec::<SearchState>::new();
        let mut best = vec![u64::MAX; self.blocked.len() * 5];
        let mut queue = BinaryHeap::new();
        for (source_index, source) in job.sources.iter().enumerate() {
            let index = self.index(source.point).unwrap();
            if sequential && usage[index] > 0 {
                continue;
            }
            let cost = congestion_cost(
                usage[index],
                history[index],
                self.present_penalty,
                sequential,
                0,
            );
            if cost < best[index * 5 + 4] {
                best[index * 5 + 4] = cost;
                let state_index = states.len();
                states.push(SearchState {
                    point: source.point,
                    direction: 4,
                    cost,
                    parent: None,
                    source_index,
                });
                queue.push(Reverse((cost, cost, index, 4usize, state_index)));
            }
        }
        let mut expanded = 0usize;
        while let Some(Reverse((_, queued_cost, _, _, state_index))) = queue.pop() {
            let state = states[state_index].clone();
            let point_index = self.index(state.point).unwrap();
            if best[point_index * 5 + state.direction] != queued_cost {
                continue;
            }
            if expanded == self.work_per_route {
                break;
            }
            expanded += 1;
            if state.direction != 4 {
                if let Some((_, target)) = job.targets.iter().enumerate().find(|(_, target)| {
                    target.point != job.sources[state.source_index].point
                        && target.point == state.point
                        && target
                            .direction
                            .is_none_or(|required| required == state.direction)
                }) {
                    let mut cells = Vec::new();
                    let mut cursor = Some(state_index);
                    while let Some(index) = cursor {
                        cells.push(states[index].point);
                        cursor = states[index].parent;
                    }
                    cells.reverse();
                    let turns = cells
                        .windows(3)
                        .filter(|cells| {
                            direction(cells[0], cells[1]) != direction(cells[1], cells[2])
                        })
                        .count();
                    return SearchResult {
                        route: Some(GlobalRoute {
                            job: job.id.clone(),
                            source_terminal: job.sources[state.source_index].id.clone(),
                            target_terminal: target.id.clone(),
                            cells,
                            turns,
                        }),
                        expanded,
                    };
                }
            }
            for next_direction in 0..4 {
                if state.direction == 4
                    && job.sources[state.source_index]
                        .direction
                        .is_some_and(|required| required != next_direction)
                {
                    continue;
                }
                if state.direction != 4 && (state.direction + 2) % 4 == next_direction {
                    continue;
                }
                let next = state.point.offset(next_direction);
                let Some(next_index) = self.index(next) else {
                    continue;
                };
                if self.blocked[next_index] || sequential && usage[next_index] > 0 {
                    continue;
                }
                if self.reserved[transport][next_index]
                    && !job.targets.iter().any(|target| {
                        target.point != job.sources[state.source_index].point
                            && target.point == next
                            && target
                                .direction
                                .is_none_or(|required| required == next_direction)
                    })
                {
                    continue;
                }
                let turn = u64::from(state.direction != 4 && state.direction != next_direction);
                let next_cost = state.cost.saturating_add(congestion_cost(
                    usage[next_index],
                    history[next_index],
                    self.present_penalty,
                    sequential,
                    turn,
                ));
                let slot = next_index * 5 + next_direction;
                if best[slot] <= next_cost {
                    continue;
                }
                best[slot] = next_cost;
                let next_state = states.len();
                states.push(SearchState {
                    point: next,
                    direction: next_direction,
                    cost: next_cost,
                    parent: Some(state_index),
                    source_index: state.source_index,
                });
                let distance = job
                    .targets
                    .iter()
                    .map(|target| next.distance(target.point))
                    .min()
                    .unwrap() as u64;
                queue.push(Reverse((
                    next_cost.saturating_add(distance.saturating_mul(STEP_COST)),
                    next_cost,
                    next_index,
                    next_direction,
                    next_state,
                )));
            }
        }
        SearchResult {
            route: None,
            expanded,
        }
    }

    fn usage(&self, routes: &[GlobalRoute]) -> [Vec<u32>; 2] {
        let mut usage = [
            vec![0u32; self.blocked.len()],
            vec![0u32; self.blocked.len()],
        ];
        for route in routes {
            let job = self
                .problem
                .jobs
                .iter()
                .find(|job| job.id == route.job)
                .unwrap();
            for &point in &route.cells {
                usage[layer(job.transport)][self.index(point).unwrap()] += 1;
            }
        }
        usage
    }

    fn overflow(&self, usage: &[Vec<u32>; 2]) -> (usize, usize, Vec<GlobalRoutingOverflowCell>) {
        let mut total = 0usize;
        let mut maximum = 0usize;
        let mut cells = Vec::new();
        for (transport_index, layer_usage) in usage.iter().enumerate() {
            for (index, &count) in layer_usage.iter().enumerate() {
                let overflow = count.saturating_sub(1) as usize;
                if overflow == 0 {
                    continue;
                }
                total += overflow;
                maximum = maximum.max(overflow);
                cells.push(GlobalRoutingOverflowCell {
                    transport: if transport_index == 0 {
                        TransportKind::Belt
                    } else {
                        TransportKind::Pipe
                    },
                    point: Point {
                        x: self.problem.bounds.min_x + (index % self.width) as i64,
                        y: self.problem.bounds.min_y + (index / self.width) as i64,
                    },
                    usage: count,
                });
            }
        }
        (total, maximum, cells)
    }

    fn index(&self, point: Point) -> Option<usize> {
        if !self.problem.bounds.contains(point) {
            return None;
        }
        let x = usize::try_from(point.x - self.problem.bounds.min_x).ok()?;
        let y = usize::try_from(point.y - self.problem.bounds.min_y).ok()?;
        Some(y * self.width + x)
    }
}

#[derive(Clone)]
struct SearchState {
    point: Point,
    direction: usize,
    cost: u64,
    parent: Option<usize>,
    source_index: usize,
}

struct SearchResult {
    route: Option<GlobalRoute>,
    expanded: usize,
}

fn congestion_cost(
    usage: u32,
    history: u32,
    present_penalty: u32,
    sequential: bool,
    turn: u64,
) -> u64 {
    if sequential && usage > 0 {
        return u64::MAX;
    }
    STEP_COST
        .saturating_mul(1 + u64::from(history) + u64::from(present_penalty) * u64::from(usage))
        .saturating_add(TURN_COST * turn)
}

fn result_key(result: &GlobalRoutingResult) -> (usize, usize, usize, usize, usize) {
    (
        result.unresolved.len(),
        result.total_overflow,
        result.maximum_overflow,
        result.wire_cells,
        result.turns,
    )
}

fn error(code: &str, message: &str) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(code, message, serde_json::Value::Null);
    diagnostic.stage = "global-routing-comparison";
    diagnostic
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terminal(id: &str, x: i64, y: i64, direction: usize) -> GlobalRouteTerminal {
        GlobalRouteTerminal {
            id: id.into(),
            point: Point { x, y },
            direction: Some(direction),
        }
    }

    #[test]
    fn negotiation_reroutes_around_a_sequential_order_trap() {
        let problem = GlobalRoutingProblem {
            bounds: Bounds {
                min_x: 0,
                min_y: 0,
                max_x: 6,
                max_y: 4,
            },
            blocked: vec![],
            jobs: vec![
                GlobalRouteJob {
                    id: "flexible-horizontal".into(),
                    commodity: "fixture".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    sources: vec![terminal("left", 0, 2, 0)],
                    targets: vec![terminal("right", 6, 2, 0)],
                },
                GlobalRouteJob {
                    id: "constrained-vertical".into(),
                    commodity: "fixture".into(),
                    transport: TransportKind::Belt,
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    sources: vec![terminal("top", 3, 1, 1)],
                    targets: vec![terminal("bottom", 3, 3, 1)],
                },
            ],
        };
        let comparison = compare_global_routing(
            &problem,
            &GlobalRoutingConfig {
                max_iterations: 8,
                ..Default::default()
            },
        )
        .unwrap();

        assert!(!comparison.sequential.complete);
        assert_eq!(comparison.sequential.unresolved, ["constrained-vertical"]);
        assert!(comparison.negotiated.legal);
        assert!(comparison.negotiated.iterations > 1);
        assert_eq!(comparison.negotiated.total_overflow, 0);
    }

    #[test]
    fn multi_terminal_search_selects_a_reachable_compatible_terminal() {
        let problem = GlobalRoutingProblem {
            bounds: Bounds {
                min_x: 0,
                min_y: 0,
                max_x: 4,
                max_y: 2,
            },
            blocked: vec![Point { x: 2, y: 0 }],
            jobs: vec![GlobalRouteJob {
                id: "job".into(),
                commodity: "fixture".into(),
                transport: TransportKind::Pipe,
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
                sources: vec![
                    terminal("blocked-side", 1, 0, 0),
                    terminal("open-side", 0, 2, 0),
                ],
                targets: vec![terminal("target", 4, 2, 0)],
            }],
        };
        let comparison = compare_global_routing(&problem, &GlobalRoutingConfig::default()).unwrap();
        assert_eq!(comparison.negotiated.routes[0].source_terminal, "open-side");
    }

    #[test]
    fn overlapping_boundary_domains_cannot_close_at_the_selected_source_cell() {
        let problem = GlobalRoutingProblem {
            bounds: Bounds {
                min_x: 0,
                min_y: 0,
                max_x: 2,
                max_y: 1,
            },
            blocked: vec![],
            jobs: vec![GlobalRouteJob {
                id: "boundary-only".into(),
                commodity: "fixture".into(),
                transport: TransportKind::Belt,
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
                sources: vec![terminal("source", 0, 0, 0)],
                targets: vec![terminal("same-cell", 0, 0, 2), terminal("other", 2, 0, 0)],
            }],
        };

        let comparison = compare_global_routing(&problem, &GlobalRoutingConfig::default()).unwrap();
        assert_eq!(comparison.negotiated.routes[0].target_terminal, "other");
        assert_eq!(
            comparison.negotiated.routes[0].cells,
            [
                Point { x: 0, y: 0 },
                Point { x: 1, y: 0 },
                Point { x: 2, y: 0 }
            ]
        );
    }
}
