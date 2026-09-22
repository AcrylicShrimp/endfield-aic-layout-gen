//! Bounded diverse route candidates for stable constructive states.

use std::collections::{BTreeSet, HashSet};

use aic_layout::{
    facilities::{FacilityPortDirection, FacilityPortEdge},
    geometry::{Bounds, Point, layer},
    projection::edge_index,
};
use aic_routing::{Crossings, PathOptions, shortest_path};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use super::{Command, Diagnostic, Direction, Session, Step, error};

pub const SCHEMA: &str = "aic-constructive-route-candidates-v1";

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub bounds: Bounds,
    pub max_candidates: usize,
    pub max_searches: usize,
    pub work_per_search: usize,
    pub max_expanded: usize,
    pub overlap_penalty: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub state_revision: u64,
    pub connection: String,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Termination {
    CandidateLimit,
    SearchLimit,
    WorkBudget,
    FrontierExhausted,
    NoCompatibleEndpoints,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Candidate {
    pub id: usize,
    pub state_revision: u64,
    pub connection: String,
    pub source_port: Option<String>,
    pub target_port: Option<String>,
    pub cells: Vec<Point>,
    pub commands: Vec<Command>,
    pub steps: i64,
    pub turns: i64,
    pub expanded: usize,
    pub overlap_with_previous: usize,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Rejection {
    pub source_port: Option<String>,
    pub target_port: Option<String>,
    pub diagnostic: Diagnostic,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Report {
    pub schema: &'static str,
    pub state_revision: u64,
    pub connection: String,
    pub termination: Termination,
    pub fixed_request_proven_infeasible: bool,
    pub endpoint_pairs: usize,
    pub searches: usize,
    pub expanded: usize,
    pub preflight_expanded: usize,
    pub guides_found: usize,
    pub duplicate_guides: usize,
    pub native_rejections: Vec<Rejection>,
    pub minimum_pairwise_dissimilarity: Option<f64>,
    pub mean_pairwise_dissimilarity: Option<f64>,
    pub candidates: Vec<Candidate>,
}

#[derive(Clone)]
struct Endpoint {
    port: Option<String>,
    point: Point,
    direction: Option<usize>,
}

#[derive(Clone)]
struct EndpointPair {
    source: Endpoint,
    target: Endpoint,
}

impl Config {
    fn validate(&self) -> Result<(), Diagnostic> {
        let width = self
            .bounds
            .max_x
            .checked_sub(self.bounds.min_x)
            .and_then(|value| value.checked_add(1));
        let height = self
            .bounds
            .max_y
            .checked_sub(self.bounds.min_y)
            .and_then(|value| value.checked_add(1));
        let area = width.and_then(|width| height.and_then(|height| width.checked_mul(height)));
        if width.is_none_or(|width| width <= 0)
            || height.is_none_or(|height| height <= 0)
            || area.is_none_or(|area| area > 250_000)
            || !(1..=64).contains(&self.max_candidates)
            || !(1..=4_096).contains(&self.max_searches)
            || !(1..=10_000_000).contains(&self.work_per_search)
            || !(1..=100_000_000).contains(&self.max_expanded)
            || !(1..=1_000_000).contains(&self.overlap_penalty)
        {
            return Err(error(
                "route-candidate-config",
                "route-candidates",
                "invalid bounds, candidate count, search count, work budget or overlap penalty",
            ));
        }
        Ok(())
    }
}

impl Session {
    pub fn route_candidates(
        &self,
        request: &Request,
        config: &Config,
    ) -> Result<Report, Diagnostic> {
        config.validate()?;
        if request.state_revision != self.revision {
            return Err(error(
                "stale-route-candidate-request",
                &request.connection,
                "routing request belongs to an earlier constructive revision",
            ));
        }
        if self.active.is_some() {
            return Err(error(
                "active-route",
                &request.connection,
                "route candidates require a stable state with no active partial route",
            ));
        }
        let Some(&connection) = self.connections.get(&request.connection) else {
            return Err(error(
                "unknown-connection",
                &request.connection,
                "route candidate request references an unknown logical connection",
            ));
        };
        if self.routes[connection].is_some() {
            return Err(error(
                "completed-connection",
                &request.connection,
                "route candidate request references a completed logical connection",
            ));
        }

        let pairs = self.endpoint_pairs(connection, config.bounds)?;
        if pairs.is_empty() {
            return Ok(empty_report(
                request,
                Termination::NoCompatibleEndpoints,
                0,
                false,
            ));
        }
        let blocked = self.route_blocked_cells(connection, config.bounds);
        let crossings = Crossings::new();
        let mut penalties = IndexMap::<Point, u32>::new();
        let mut semantic = BTreeSet::new();
        let mut candidates = Vec::new();
        let mut rejections = Vec::new();
        let mut searches = 0usize;
        let mut expanded = 0usize;
        let mut preflight_expanded = 0usize;
        let mut guides_found = 0usize;
        let mut duplicate_guides = 0usize;
        let mut anchor_no_path = 0usize;
        let mut anchor_complete = true;
        let termination = 'rounds: loop {
            let anchor = searches < pairs.len();
            for pair in &pairs {
                if candidates.len() == config.max_candidates {
                    break 'rounds Termination::CandidateLimit;
                }
                if searches == config.max_searches {
                    break 'rounds Termination::SearchLimit;
                }
                let remaining = config.max_expanded.saturating_sub(expanded);
                if remaining == 0 {
                    break 'rounds Termination::WorkBudget;
                }
                let work_budget = remaining.min(config.work_per_search);
                let result = shortest_path(
                    pair.source.point,
                    pair.target.point,
                    &blocked,
                    config.bounds,
                    PathOptions {
                        first: pair.source.direction,
                        last: pair.target.direction,
                        work_budget,
                        crossings: &crossings,
                        crossing_limit: 0,
                        allow_zero_length: false,
                        cell_penalties: (!anchor).then_some(&penalties),
                    },
                )
                .map_err(|failure| {
                    error(
                        "route-candidate-search",
                        &request.connection,
                        failure.message,
                    )
                })?;
                searches += 1;
                expanded += result.expanded;
                preflight_expanded += result.preflight_expanded;
                if result.status == "work-budget" {
                    if anchor {
                        anchor_complete = false;
                    }
                    if expanded >= config.max_expanded {
                        break 'rounds Termination::WorkBudget;
                    }
                    continue;
                }
                let Some(cells) = result.cells else {
                    if anchor && result.status == "no-path" {
                        anchor_no_path += 1;
                    }
                    continue;
                };
                guides_found += 1;
                let source_port = pair.source.port.clone();
                let target_port = pair.target.port.clone();
                let key = (source_port.clone(), target_port.clone(), cells.clone());
                let overlap = maximum_overlap(&cells, &candidates);
                add_overlap_penalty(&mut penalties, &cells, config.overlap_penalty);
                if !semantic.insert(key) {
                    duplicate_guides += 1;
                    continue;
                }
                let commands = self.route_commands(
                    &request.connection,
                    source_port.clone(),
                    target_port.clone(),
                    &cells,
                );
                if let Err(diagnostic) =
                    self.replay_route_witness(request.state_revision, &commands)
                {
                    rejections.push(Rejection {
                        source_port,
                        target_port,
                        diagnostic,
                    });
                    continue;
                }
                candidates.push(Candidate {
                    id: candidates.len(),
                    state_revision: request.state_revision,
                    connection: request.connection.clone(),
                    source_port,
                    target_port,
                    cells,
                    commands,
                    steps: result.steps.expect("found path has a step count"),
                    turns: result.turns.expect("found path has a turn count"),
                    expanded: result.expanded,
                    overlap_with_previous: overlap,
                });
            }
            if anchor && anchor_complete && anchor_no_path == pairs.len() {
                break Termination::FrontierExhausted;
            }
            if searches >= config.max_searches {
                break Termination::SearchLimit;
            }
        };
        let (minimum_pairwise_dissimilarity, mean_pairwise_dissimilarity) = diversity(&candidates);
        Ok(Report {
            schema: SCHEMA,
            state_revision: request.state_revision,
            connection: request.connection.clone(),
            termination,
            fixed_request_proven_infeasible: termination == Termination::FrontierExhausted
                && candidates.is_empty(),
            endpoint_pairs: pairs.len(),
            searches,
            expanded,
            preflight_expanded,
            guides_found,
            duplicate_guides,
            native_rejections: rejections,
            minimum_pairwise_dissimilarity,
            mean_pairwise_dissimilarity,
            candidates,
        })
    }

    /// Atomically applies a candidate only to the exact state that produced it.
    pub fn apply_route_candidate(
        &mut self,
        candidate: &Candidate,
    ) -> Result<Vec<Step>, Diagnostic> {
        let Some(&connection) = self.connections.get(&candidate.connection) else {
            return Err(error(
                "route-candidate-connection",
                &candidate.connection,
                "route candidate references an unknown logical connection",
            ));
        };
        let valid_shape = matches!(
            candidate.commands.first(),
            Some(Command::BeginRoute {
                connection,
                source_port,
                target_port,
            }) if connection == &candidate.connection
                && source_port == &candidate.source_port
                && target_port == &candidate.target_port
        ) && candidate.commands.last() == Some(&Command::Connect)
            && candidate.commands[1..candidate.commands.len().saturating_sub(1)]
                .iter()
                .all(|command| matches!(command, Command::Extend { .. }));
        if !valid_shape {
            return Err(error(
                "route-candidate-witness",
                &candidate.connection,
                "route candidate does not contain one complete route witness",
            ));
        }
        let (fork, steps) =
            self.replay_route_witness(candidate.state_revision, &candidate.commands)?;
        if fork.routes[connection]
            .as_ref()
            .is_none_or(|route| route.cells != candidate.cells)
        {
            return Err(error(
                "route-candidate-geometry",
                &candidate.connection,
                "replayed route does not match candidate geometry",
            ));
        }
        *self = fork;
        Ok(steps)
    }

    fn endpoint_pairs(
        &self,
        connection: usize,
        bounds: Bounds,
    ) -> Result<Vec<EndpointPair>, Diagnostic> {
        let obligation = &self.problem.connections[connection];
        let sources = self.route_candidate_endpoints(
            &obligation.source,
            FacilityPortDirection::Output,
            connection,
        )?;
        let targets = self.route_candidate_endpoints(
            &obligation.target,
            FacilityPortDirection::Input,
            connection,
        )?;
        let mut pairs = sources
            .into_iter()
            .flat_map(|source| {
                targets.iter().cloned().map(move |target| EndpointPair {
                    source: source.clone(),
                    target,
                })
            })
            .filter(|pair| bounds.contains(pair.source.point) && bounds.contains(pair.target.point))
            .collect::<Vec<_>>();
        pairs.sort_by(|left, right| {
            left.source
                .point
                .distance(left.target.point)
                .cmp(&right.source.point.distance(right.target.point))
                .then_with(|| left.source.port.cmp(&right.source.port))
                .then_with(|| left.target.port.cmp(&right.target.port))
        });
        Ok(pairs)
    }

    fn route_candidate_endpoints(
        &self,
        endpoint: &str,
        expected: FacilityPortDirection,
        connection: usize,
    ) -> Result<Vec<Endpoint>, Diagnostic> {
        let obligation = &self.problem.connections[connection];
        let Some(&block) = self.blocks.get(endpoint) else {
            let (point, _) = self.route_endpoint(endpoint, None, expected, obligation)?;
            return Ok(vec![Endpoint {
                port: None,
                point,
                direction: None,
            }]);
        };
        let mut endpoints = self
            .projected_ports(block)?
            .into_iter()
            .filter(|port| {
                self.selected_port(
                    block,
                    &port.id,
                    expected,
                    obligation.transport,
                    &obligation.commodity,
                    obligation.rate,
                    &obligation.id,
                )
                .is_ok()
            })
            .map(|port| Endpoint {
                port: Some(port.id),
                point: port.connection,
                direction: Some(match expected {
                    FacilityPortDirection::Output => edge_index(port.edge),
                    FacilityPortDirection::Input => opposite(port.edge),
                }),
            })
            .collect::<Vec<_>>();
        endpoints.sort_by(|left, right| left.port.cmp(&right.port));
        Ok(endpoints)
    }

    fn route_blocked_cells(&self, connection: usize, bounds: Bounds) -> IndexSet<Point> {
        let obligation = &self.problem.connections[connection];
        let mut blocked = self
            .bodies
            .iter()
            .copied()
            .filter(|point| bounds.contains(*point))
            .collect::<Vec<_>>();
        blocked.extend(
            self.terminal_poses
                .iter()
                .enumerate()
                .filter(|(terminal, _)| {
                    let id = &self.problem.terminals[*terminal].id;
                    id != &obligation.source && id != &obligation.target
                })
                .filter_map(|(_, pose)| pose.as_ref().map(|pose| pose.position))
                .filter(|point| bounds.contains(*point)),
        );
        blocked.extend(
            self.occupied[layer(obligation.transport)]
                .keys()
                .copied()
                .filter(|point| bounds.contains(*point))
                .filter(|&point| !self.shared_cell_available(point, connection)),
        );
        blocked.sort_by_key(|point| (point.y, point.x));
        blocked.dedup();
        blocked.into_iter().collect()
    }

    fn route_commands(
        &self,
        connection: &str,
        source_port: Option<String>,
        target_port: Option<String>,
        cells: &[Point],
    ) -> Vec<Command> {
        let mut commands = vec![Command::BeginRoute {
            connection: connection.to_owned(),
            source_port,
            target_port,
        }];
        let mut start = 0usize;
        while start + 1 < cells.len() {
            let heading = direction(cells[start], cells[start + 1]);
            let mut end = start + 1;
            while end + 1 < cells.len()
                && direction(cells[end], cells[end + 1]) == heading
                && end - start < self.limits.max_extension as usize
            {
                end += 1;
            }
            commands.push(Command::Extend {
                direction: heading,
                length: (end - start) as i64,
            });
            start = end;
        }
        commands.push(Command::Connect);
        commands
    }

    fn replay_route_witness(
        &self,
        state_revision: u64,
        commands: &[Command],
    ) -> Result<(Session, Vec<Step>), Diagnostic> {
        if self.revision != state_revision {
            return Err(error(
                "stale-route-candidate",
                "route-candidate",
                "route candidate belongs to an earlier constructive revision",
            ));
        }
        let mut fork = self.clone();
        let mut steps = Vec::with_capacity(commands.len());
        for command in commands {
            let step = fork.step(command.clone());
            if !step.accepted || step.diagnostic.is_some() {
                return Err(step.diagnostic.unwrap_or_else(|| {
                    error(
                        "route-candidate-replay",
                        "route-candidate",
                        "native replay rejected a route candidate",
                    )
                }));
            }
            steps.push(step);
        }
        Ok((fork, steps))
    }
}

fn empty_report(
    request: &Request,
    termination: Termination,
    endpoint_pairs: usize,
    proven: bool,
) -> Report {
    Report {
        schema: SCHEMA,
        state_revision: request.state_revision,
        connection: request.connection.clone(),
        termination,
        fixed_request_proven_infeasible: proven,
        endpoint_pairs,
        searches: 0,
        expanded: 0,
        preflight_expanded: 0,
        guides_found: 0,
        duplicate_guides: 0,
        native_rejections: vec![],
        minimum_pairwise_dissimilarity: None,
        mean_pairwise_dissimilarity: None,
        candidates: vec![],
    }
}

fn opposite(edge: FacilityPortEdge) -> usize {
    (edge_index(edge) + 2) % 4
}

fn direction(from: Point, to: Point) -> Direction {
    match (to.x - from.x, to.y - from.y) {
        (1, 0) => Direction::East,
        (0, 1) => Direction::South,
        (-1, 0) => Direction::West,
        (0, -1) => Direction::North,
        _ => unreachable!("routing kernel returns cardinal unit steps"),
    }
}

fn interior(cells: &[Point]) -> impl Iterator<Item = Point> + '_ {
    cells
        .iter()
        .copied()
        .skip(1)
        .take(cells.len().saturating_sub(2))
}

fn add_overlap_penalty(penalties: &mut IndexMap<Point, u32>, cells: &[Point], amount: u32) {
    for point in interior(cells) {
        let entry = penalties.entry(point).or_default();
        *entry = entry.saturating_add(amount).min(1_000_000);
    }
}

fn maximum_overlap(cells: &[Point], candidates: &[Candidate]) -> usize {
    let cells = interior(cells).collect::<HashSet<_>>();
    candidates
        .iter()
        .map(|candidate| {
            interior(&candidate.cells)
                .filter(|point| cells.contains(point))
                .count()
        })
        .max()
        .unwrap_or(0)
}

fn diversity(candidates: &[Candidate]) -> (Option<f64>, Option<f64>) {
    let mut values = Vec::new();
    for left_index in 0..candidates.len() {
        let left = interior(&candidates[left_index].cells).collect::<HashSet<_>>();
        for right in &candidates[left_index + 1..] {
            let right = interior(&right.cells).collect::<HashSet<_>>();
            let intersection = left.intersection(&right).count();
            let union = left.union(&right).count();
            values.push(if union == 0 {
                0.0
            } else {
                1.0 - intersection as f64 / union as f64
            });
        }
    }
    if values.is_empty() {
        return (None, None);
    }
    let minimum = values.iter().copied().fold(f64::INFINITY, f64::min);
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    (Some(minimum), Some(mean))
}
