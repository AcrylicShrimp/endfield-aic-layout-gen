//! Bounded diverse route candidates for stable constructive states.

use std::collections::{BTreeSet, HashSet};

use aic_layout::{
    facilities::{FacilityPortDirection, FacilityPortEdge},
    geometry::{Bounds, Point, layer},
    logistics::{LogisticsComponentKind, TransportKind},
    physical::{Bridge, BridgeAxis},
    projection::edge_index,
    validation::within_capacity,
};
use aic_routing::{Carrier, Crossing, PathOptions, RouteJob, crossing_options, shortest_path};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use super::{Command, Diagnostic, Direction, Session, Step, error};

pub const SCHEMA: &str = "aic-constructive-route-candidates-v3";
pub const GUIDANCE_SCHEMA: &str = "aic-constructive-route-guidance-v2";
pub const GUIDANCE_GRID_CHANNELS: usize = 16;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub bounds: Bounds,
    pub max_candidates: usize,
    pub max_searches: usize,
    pub work_per_search: usize,
    pub max_expanded: usize,
    pub overlap_penalty: u32,
    /// Missing in older saved panels means the prior crossing-disabled behavior.
    #[serde(default)]
    pub max_crossings_per_route: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub state_revision: u64,
    pub connection: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NetworkRequest {
    pub state_revision: u64,
    pub connections: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Guidance {
    pub schema: String,
    pub state_revision: u64,
    pub connection: String,
    pub model_version: String,
    pub heads: Vec<GuidanceHead>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GuidanceHead {
    pub id: String,
    pub directed_costs: Vec<DirectedCost>,
    pub predicted_success: f32,
    pub uncertainty: f32,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DirectedCost {
    pub point: Point,
    pub direction: usize,
    pub cost: u32,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct GuidanceInput {
    pub schema: &'static str,
    pub state_revision: u64,
    pub connection: String,
    pub catalog_identity: String,
    pub bounds: Bounds,
    pub channels: usize,
    pub bridge_resources: Vec<Bridge>,
    /// Channel-major `[channel][y][x]` values.
    pub grid: Vec<f32>,
    pub graph: std::sync::Arc<super::learning::Input>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Termination {
    CandidateLimit,
    SearchLimit,
    WorkBudget,
    ResourceLimit,
    FrontierExhausted,
    NoCompatibleEndpoints,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Provenance {
    Anchor,
    Guided {
        model_version: String,
        head: String,
    },
    Diversified {
        model_version: Option<String>,
        head: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub id: usize,
    pub state_revision: u64,
    pub connection: String,
    pub source_port: Option<String>,
    pub target_port: Option<String>,
    pub cells: Vec<Point>,
    pub commands: Vec<Command>,
    #[serde(default)]
    pub crossings: Vec<Crossing>,
    pub steps: i64,
    pub turns: i64,
    pub expanded: usize,
    pub overlap_with_previous: usize,
    pub learned_cost: u64,
    pub predicted_success: Option<f32>,
    pub guidance_uncertainty: Option<f32>,
    pub provenance: Provenance,
}

/// One atomic same-commodity network witness. Its member routes were generated
/// and replayed in order, so later routes see exact residual shared capacity.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NetworkCandidate {
    pub id: usize,
    pub state_revision: u64,
    pub commodity: String,
    pub transport: TransportKind,
    pub routes: Vec<Candidate>,
    pub steps: i64,
    pub turns: i64,
    pub expanded: usize,
    pub shared_cells: usize,
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
    pub crossing_limit: usize,
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
            || area.is_some_and(|area| self.max_crossings_per_route > area as usize)
        {
            return Err(error(
                "route-candidate-config",
                "route-candidates",
                "invalid bounds, candidate count, search count, work budget, overlap penalty or crossing limit",
            ));
        }
        Ok(())
    }
}

impl Guidance {
    fn validate(
        &self,
        request: &Request,
        bounds: Bounds,
    ) -> Result<Vec<IndexMap<(Point, usize), u32>>, Diagnostic> {
        let area = usize::try_from(bounds.width())
            .ok()
            .and_then(|width| {
                usize::try_from(bounds.height())
                    .ok()
                    .and_then(|height| width.checked_mul(height))
            })
            .unwrap_or(0);
        if self.schema != GUIDANCE_SCHEMA
            || self.state_revision != request.state_revision
            || self.connection != request.connection
            || self.model_version.is_empty()
            || self.model_version.len() > 256
            || !(1..=8).contains(&self.heads.len())
        {
            return Err(error(
                "route-guidance-identity",
                &request.connection,
                "route guidance schema, state, connection, model version or head count is invalid",
            ));
        }
        let mut ids = BTreeSet::new();
        let mut maps = Vec::with_capacity(self.heads.len());
        for head in &self.heads {
            if head.id.is_empty()
                || head.id.len() > 128
                || !ids.insert(head.id.as_str())
                || !head.predicted_success.is_finite()
                || !(0.0..=1.0).contains(&head.predicted_success)
                || !head.uncertainty.is_finite()
                || !(0.0..=1.0).contains(&head.uncertainty)
                || head.directed_costs.len() > area.saturating_mul(4)
            {
                return Err(error(
                    "route-guidance-head",
                    &head.id,
                    "route guidance head metadata or directed-cost count is invalid",
                ));
            }
            let mut map = IndexMap::with_capacity(head.directed_costs.len());
            for cost in &head.directed_costs {
                if !bounds.contains(cost.point)
                    || cost.direction > 3
                    || cost.cost > 1_000_000
                    || map
                        .insert((cost.point, cost.direction), cost.cost)
                        .is_some()
                {
                    return Err(error(
                        "route-guidance-cost",
                        &head.id,
                        "route guidance contains an invalid or duplicate directed edge cost",
                    ));
                }
            }
            maps.push(map);
        }
        Ok(maps)
    }
}

impl Session {
    pub fn route_guidance_input(
        &self,
        request: &Request,
        candidate: &Config,
        learning: &super::learning::Config,
    ) -> Result<GuidanceInput, Diagnostic> {
        candidate.validate()?;
        if learning.placement != candidate.bounds {
            return Err(error(
                "route-guidance-bounds",
                &request.connection,
                "candidate and learning bounds must match",
            ));
        }
        if request.state_revision != self.revision || self.active.is_some() {
            return Err(error(
                "route-guidance-state",
                &request.connection,
                "route guidance input requires the exact stable state revision",
            ));
        }
        let Some(&connection) = self.connections.get(&request.connection) else {
            return Err(error(
                "unknown-connection",
                &request.connection,
                "route guidance references an unknown logical connection",
            ));
        };
        if self.routes[connection].is_some() {
            return Err(error(
                "completed-connection",
                &request.connection,
                "route guidance references a completed logical connection",
            ));
        }
        let width = usize::try_from(candidate.bounds.width()).expect("validated positive width");
        let height = usize::try_from(candidate.bounds.height()).expect("validated positive height");
        let area = width * height;
        let mut grid = vec![0.0; GUIDANCE_GRID_CHANNELS * area];
        let index = |point: Point| {
            usize::try_from(point.y - candidate.bounds.min_y).expect("point in bounds") * width
                + usize::try_from(point.x - candidate.bounds.min_x).expect("point in bounds")
        };
        let mark = |grid: &mut [f32], channel: usize, point: Point, value: f32| {
            if candidate.bounds.contains(point) {
                let offset = channel * area + index(point);
                grid[offset] = grid[offset].max(value);
            }
        };
        for &point in &self.bodies {
            mark(&mut grid, 0, point, 1.0);
        }
        for pose in self.terminal_poses.iter().flatten() {
            mark(&mut grid, 1, pose.position, 1.0);
        }
        let transport_layer = layer(self.problem.connections[connection].transport);
        for (&point, routes) in &self.occupied[transport_layer] {
            let channel = if self.shared_cell_available(point, connection) {
                2
            } else {
                3
            };
            mark(&mut grid, channel, point, 1.0);
            mark(&mut grid, 13, point, (routes.len() as f32 / 8.0).min(1.0));
        }
        for &point in self.occupied[1 - transport_layer].keys() {
            mark(&mut grid, 4, point, 1.0);
        }
        let mut bridge_resources = self.bridges.values().cloned().collect::<Vec<_>>();
        bridge_resources.sort_by(|left, right| left.id.cmp(&right.id));
        for bridge in &bridge_resources {
            let transport = self
                .catalogs
                .components
                .component(&bridge.component)
                .expect("installed bridge has a validated catalog component")
                .transport;
            if layer(transport) != transport_layer {
                continue;
            }
            for (axis, channel) in bridge.channels.iter().enumerate() {
                mark(
                    &mut grid,
                    14 + axis,
                    bridge.position,
                    (channel.connections.len() as f32 / 8.0).min(1.0),
                );
            }
        }
        for pair in self.endpoint_pairs(connection, candidate.bounds)? {
            mark(&mut grid, 5, pair.source.point, 1.0);
            mark(&mut grid, 6, pair.target.point, 1.0);
        }
        let mut remaining = 0usize;
        for other in 0..self.problem.connections.len() {
            if other == connection || self.routes[other].is_some() {
                continue;
            }
            remaining += 1;
            if let Ok(pairs) = self.endpoint_pairs(other, candidate.bounds) {
                for pair in pairs {
                    mark(&mut grid, 7, pair.source.point, 1.0);
                    mark(&mut grid, 8, pair.target.point, 1.0);
                }
            }
        }
        for y in candidate.bounds.min_y..=candidate.bounds.max_y {
            for x in candidate.bounds.min_x..=candidate.bounds.max_x {
                let point = Point { x, y };
                if x == candidate.bounds.min_x
                    || x == candidate.bounds.max_x
                    || y == candidate.bounds.min_y
                    || y == candidate.bounds.max_y
                {
                    mark(&mut grid, 9, point, 1.0);
                }
                grid[(10 + transport_layer) * area + index(point)] = 1.0;
                let rate = &self.problem.connections[connection].rate;
                grid[12 * area + index(point)] =
                    (rate.numerator as f32 / rate.denominator as f32).ln_1p();
                grid[13 * area + index(point)] = grid[13 * area + index(point)]
                    .max((remaining as f32 / self.problem.connections.len() as f32).min(1.0));
            }
        }
        Ok(GuidanceInput {
            schema: GUIDANCE_SCHEMA,
            state_revision: request.state_revision,
            connection: request.connection.clone(),
            catalog_identity: self.catalogs.identity.clone(),
            bounds: candidate.bounds,
            channels: GUIDANCE_GRID_CHANNELS,
            bridge_resources,
            grid,
            graph: self.route_guidance_learning_input(learning, connection)?,
        })
    }

    pub fn route_candidates(
        &self,
        request: &Request,
        config: &Config,
    ) -> Result<Report, Diagnostic> {
        self.route_candidates_inner(request, config, None)
    }

    pub fn route_candidates_with_guidance(
        &self,
        request: &Request,
        config: &Config,
        guidance: &Guidance,
    ) -> Result<Report, Diagnostic> {
        self.route_candidates_inner(request, config, Some(guidance))
    }

    fn route_candidates_inner(
        &self,
        request: &Request,
        config: &Config,
        guidance: Option<&Guidance>,
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
                config.max_crossings_per_route,
            ));
        }
        let guidance_maps = guidance
            .map(|guidance| guidance.validate(request, config.bounds))
            .transpose()?;
        let guidance_heads = guidance.map_or(&[][..], |guidance| guidance.heads.as_slice());
        let blocked = self.route_blocked_cells(connection, config.bounds);
        let crossings = if config.max_crossings_per_route == 0 {
            Default::default()
        } else {
            let mut networks = IndexMap::<&str, usize>::new();
            let carriers = self
                .routes
                .iter()
                .enumerate()
                .filter_map(|(ci, route)| {
                    let route = route.as_ref()?;
                    let obligation = &self.problem.connections[ci];
                    let network = match networks.get(obligation.commodity.as_str()) {
                        Some(&network) => network,
                        None => {
                            let network = networks.len();
                            networks.insert(&obligation.commodity, network);
                            network
                        }
                    };
                    Some(Carrier {
                        id: ci,
                        network,
                        transport: obligation.transport,
                        rate: obligation.rate,
                        cells: &route.cells,
                    })
                })
                .collect::<Vec<_>>();
            let obligation = &self.problem.connections[connection];
            let excluded = self
                .bodies
                .iter()
                .copied()
                .chain(self.terminal_at.keys().copied())
                .chain(
                    self.junctions
                        .keys()
                        .filter(|(transport, _)| *transport == obligation.transport)
                        .map(|(_, point)| *point),
                )
                .chain(
                    self.bridges
                        .keys()
                        .filter(|(transport, _)| *transport == obligation.transport)
                        .map(|(_, point)| *point),
                )
                .collect::<IndexSet<_>>();
            let mut crossings = crossing_options(
                &carriers,
                &RouteJob {
                    id: connection,
                    transport: obligation.transport,
                    rate: obligation.rate,
                },
                &self.catalogs.components,
                &excluded,
            );
            // An installed bridge is occupied, but a new route can join an
            // existing directed channel when its exact residual capacity fits.
            for (&(transport, position), bridge) in &self.bridges {
                if transport != obligation.transport || !config.bounds.contains(position) {
                    continue;
                }
                let Some(definition) = self.catalogs.components.component(&bridge.component) else {
                    continue;
                };
                let line = self.catalogs.transports.capacity(transport);
                for channel in &bridge.channels {
                    if channel.commodity != obligation.commodity {
                        continue;
                    }
                    let mut heading = None;
                    let mut load = obligation.rate;
                    let mut first_owner = None;
                    let mut valid = true;
                    for owner in &channel.connections {
                        let Some(&ci) = self.connections.get(owner) else {
                            valid = false;
                            break;
                        };
                        let Some(route) = self.routes[ci].as_ref() else {
                            valid = false;
                            break;
                        };
                        let Some(pi) = route.cells.iter().position(|&point| point == position)
                        else {
                            valid = false;
                            break;
                        };
                        if pi == 0 || pi + 1 == route.cells.len() {
                            valid = false;
                            break;
                        }
                        let Some(step_direction) =
                            aic_layout::geometry::direction(route.cells[pi - 1], position)
                        else {
                            valid = false;
                            break;
                        };
                        if aic_layout::geometry::direction(route.cells[pi], route.cells[pi + 1])
                            != Some(step_direction)
                            || heading.is_some_and(|old| old != step_direction)
                        {
                            valid = false;
                            break;
                        }
                        let Ok(next_load) = load.checked_add(self.problem.connections[ci].rate)
                        else {
                            valid = false;
                            break;
                        };
                        load = next_load;
                        heading = Some(step_direction);
                        first_owner.get_or_insert(ci);
                    }
                    let (Some(direction), Some(carrier_chain)) = (heading, first_owner) else {
                        continue;
                    };
                    if !valid
                        || !within_capacity(
                            load,
                            definition.capacity.quantity,
                            definition.capacity.duration_ms,
                        )
                        || !within_capacity(load, line.quantity, line.duration_ms)
                    {
                        continue;
                    }
                    let Some(&carrier_network) = networks.get(channel.commodity.as_str()) else {
                        continue;
                    };
                    crossings.insert(
                        (position, direction),
                        Crossing {
                            position,
                            direction,
                            component: bridge.component.clone(),
                            rotation: bridge.rotation,
                            transport,
                            kind: LogisticsComponentKind::Bridge,
                            carrier_chain,
                            carrier_network,
                        },
                    );
                }
            }
            crossings
        };
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
        let mut phase = 0usize;
        let termination = 'rounds: loop {
            let anchor = phase == 0;
            let guided = phase > 0 && phase <= guidance_heads.len();
            let guidance_index = if guided {
                Some(phase - 1)
            } else if !anchor && !guidance_heads.is_empty() {
                Some((phase - guidance_heads.len() - 1) % guidance_heads.len())
            } else {
                None
            };
            let edge_penalties =
                guidance_index.and_then(|index| guidance_maps.as_ref().map(|maps| &maps[index]));
            let phase_target = if anchor && !guidance_heads.is_empty() {
                1
            } else if guided {
                1 + ((config.max_candidates - 1) * phase).div_ceil(guidance_heads.len())
            } else {
                config.max_candidates
            }
            .min(config.max_candidates);
            let mut phase_searches = 0usize;
            'phase: for pair in &pairs {
                if candidates.len() == config.max_candidates {
                    break 'rounds Termination::CandidateLimit;
                }
                if candidates.len() >= phase_target {
                    break 'phase;
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
                        crossing_limit: config.max_crossings_per_route,
                        prefer_crossings: phase > 0,
                        allow_zero_length: false,
                        cell_penalties: (!anchor && !guided).then_some(&penalties),
                        edge_penalties,
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
                phase_searches += 1;
                expanded += result.expanded;
                preflight_expanded += result.preflight_expanded;
                if result.status == "resource-limit" {
                    break 'rounds Termination::ResourceLimit;
                }
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
                let found_crossings = result.crossings.unwrap_or_default();
                guides_found += 1;
                let source_port = pair.source.port.clone();
                let target_port = pair.target.port.clone();
                let key = (
                    source_port.clone(),
                    target_port.clone(),
                    cells.clone(),
                    found_crossings
                        .iter()
                        .map(|crossing| {
                            (
                                crossing.position,
                                crossing.direction,
                                crossing.component.clone(),
                                crossing.rotation,
                            )
                        })
                        .collect::<Vec<_>>(),
                );
                let overlap = maximum_overlap(&cells, &candidates);
                let learned_cost = edge_penalties
                    .map(|costs| directed_cost(&cells, costs))
                    .unwrap_or(0);
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
                    &found_crossings,
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
                    crossings: found_crossings,
                    steps: result.steps.expect("found path has a step count"),
                    turns: result.turns.expect("found path has a turn count"),
                    expanded: result.expanded,
                    overlap_with_previous: overlap,
                    learned_cost,
                    predicted_success: guidance_index
                        .map(|index| guidance_heads[index].predicted_success),
                    guidance_uncertainty: guidance_index
                        .map(|index| guidance_heads[index].uncertainty),
                    provenance: if anchor {
                        Provenance::Anchor
                    } else if guided {
                        let index = guidance_index.expect("guided phase has guidance head");
                        Provenance::Guided {
                            model_version: guidance
                                .expect("guided phase has guidance")
                                .model_version
                                .clone(),
                            head: guidance_heads[index].id.clone(),
                        }
                    } else {
                        Provenance::Diversified {
                            model_version: guidance.map(|guidance| guidance.model_version.clone()),
                            head: guidance_index.map(|index| guidance_heads[index].id.clone()),
                        }
                    },
                });
            }
            if anchor && phase_searches < pairs.len() {
                anchor_complete = false;
            }
            if anchor && anchor_complete && anchor_no_path == pairs.len() {
                break Termination::FrontierExhausted;
            }
            if searches >= config.max_searches {
                break Termination::SearchLimit;
            }
            phase += 1;
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
            crossing_limit: config.max_crossings_per_route,
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
                .all(|command| {
                    matches!(
                        command,
                        Command::Extend { .. } | Command::ReserveBridge { .. }
                    )
                });
        if !valid_shape {
            return Err(error(
                "route-candidate-witness",
                &candidate.connection,
                "route candidate does not contain one complete route witness",
            ));
        }
        let reservations = candidate
            .commands
            .iter()
            .filter_map(|command| match command {
                Command::ReserveBridge {
                    position,
                    component,
                    rotation,
                    crossed_axis,
                } => Some((*position, component.as_str(), *rotation, *crossed_axis)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut networks = IndexMap::<&str, usize>::new();
        for (ci, route) in self.routes.iter().enumerate() {
            if route.is_some() {
                let commodity = self.problem.connections[ci].commodity.as_str();
                let next = networks.len();
                networks.entry(commodity).or_insert(next);
            }
        }
        if reservations.len() != candidate.crossings.len()
            || reservations.iter().zip(&candidate.crossings).any(
                |(&(position, component, rotation, crossed_axis), crossing)| {
                    let axis = if crossing.direction % 2 == 0 {
                        BridgeAxis::Vertical
                    } else {
                        BridgeAxis::Horizontal
                    };
                    crossing.position != position
                        || crossing.component != component
                        || crossing.rotation != rotation
                        || crossing.kind != LogisticsComponentKind::Bridge
                        || crossing.transport != self.problem.connections[connection].transport
                        || crossed_axis != axis
                        || candidate
                            .cells
                            .iter()
                            .position(|&point| point == position)
                            .and_then(|pi| pi.checked_sub(1).map(|previous| (pi, previous)))
                            .and_then(|(pi, previous)| {
                                aic_layout::geometry::direction(
                                    candidate.cells[previous],
                                    candidate.cells[pi],
                                )
                            })
                            != Some(crossing.direction)
                        || self
                            .bridges
                            .get(&(crossing.transport, position))
                            .and_then(|bridge| {
                                bridge.channels[crossing.direction % 2]
                                    .connections
                                    .first()
                                    .and_then(|owner| self.connections.get(owner).copied())
                            })
                            .or_else(|| {
                                self.occupied[layer(crossing.transport)]
                                    .get(&position)
                                    .and_then(|users| users.iter().copied().min())
                            })
                            != Some(crossing.carrier_chain)
                        || self
                            .problem
                            .connections
                            .get(crossing.carrier_chain)
                            .and_then(|carrier| networks.get(carrier.commodity.as_str()).copied())
                            != Some(crossing.carrier_network)
                },
            )
        {
            return Err(error(
                "route-candidate-bridge-witness",
                &candidate.connection,
                "bridge metadata does not match the reserved native commands",
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
        if candidate.crossings.iter().any(|crossing| {
            fork.bridges
                .get(&(crossing.transport, crossing.position))
                .is_none_or(|bridge| {
                    bridge.component != crossing.component
                        || bridge.rotation != crossing.rotation
                        || !bridge.channels[crossing.direction % 2]
                            .connections
                            .contains(&candidate.connection)
                })
        }) {
            return Err(error(
                "route-candidate-bridge-witness",
                &candidate.connection,
                "replayed bridge axes do not match the candidate witness",
            ));
        }
        *self = fork;
        Ok(steps)
    }

    /// Atomically applies a complete same-commodity network witness. Each
    /// member route is replayed against the state produced by the preceding
    /// member, which delegates cell and port capacity to the native session.
    pub fn apply_network_candidate(
        &mut self,
        candidate: &NetworkCandidate,
    ) -> Result<Vec<Step>, Diagnostic> {
        if self.revision != candidate.state_revision
            || candidate.routes.len() < 2
            || candidate.commodity.is_empty()
        {
            return Err(error(
                "network-candidate-identity",
                &candidate.commodity,
                "network candidate is stale, empty, or does not contain multiple routes",
            ));
        }
        let mut connections = BTreeSet::new();
        let mut fork = self.clone();
        let mut steps = Vec::new();
        for route in &candidate.routes {
            let Some(&connection) = fork.connections.get(&route.connection) else {
                return Err(error(
                    "network-candidate-connection",
                    &route.connection,
                    "network candidate references an unknown connection",
                ));
            };
            let obligation = &fork.problem.connections[connection];
            if obligation.commodity != candidate.commodity
                || obligation.transport != candidate.transport
                || !connections.insert(route.connection.as_str())
            {
                return Err(error(
                    "network-candidate-task",
                    &route.connection,
                    "network routes must be distinct and share commodity and transport",
                ));
            }
            steps.extend(fork.apply_route_candidate(route)?);
        }
        let steps_total = candidate
            .routes
            .iter()
            .map(|route| route.steps)
            .sum::<i64>();
        let turns_total = candidate
            .routes
            .iter()
            .map(|route| route.turns)
            .sum::<i64>();
        let expanded_total = candidate
            .routes
            .iter()
            .map(|route| route.expanded)
            .sum::<usize>();
        if candidate.steps != steps_total
            || candidate.turns != turns_total
            || candidate.expanded != expanded_total
            || candidate.shared_cells != network_shared_cells(&candidate.routes)
        {
            return Err(error(
                "network-candidate-metadata",
                &candidate.commodity,
                "network candidate aggregate work or sharing metadata differs from its routes",
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
                .filter(|&point| {
                    self.bridges.contains_key(&(obligation.transport, point))
                        || !self.shared_cell_available(point, connection)
                }),
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
        crossings: &[Crossing],
    ) -> Vec<Command> {
        let mut commands = vec![Command::BeginRoute {
            connection: connection.to_owned(),
            source_port,
            target_port,
        }];
        let at = crossings
            .iter()
            .map(|crossing| (crossing.position, crossing))
            .collect::<IndexMap<_, _>>();
        let mut start = 0usize;
        while start + 1 < cells.len() {
            let heading = direction(cells[start], cells[start + 1]);
            if let Some(crossing) = at.get(&cells[start + 1]) {
                commands.push(Command::ReserveBridge {
                    position: crossing.position,
                    component: crossing.component.clone(),
                    rotation: crossing.rotation,
                    crossed_axis: if crossing.direction % 2 == 0 {
                        BridgeAxis::Vertical
                    } else {
                        BridgeAxis::Horizontal
                    },
                });
                commands.push(Command::Extend {
                    direction: heading,
                    length: 1,
                });
                start += 1;
                continue;
            }
            let mut end = start + 1;
            while end + 1 < cells.len()
                && direction(cells[end], cells[end + 1]) == heading
                && !at.contains_key(&cells[end + 1])
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
    crossing_limit: usize,
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
        crossing_limit,
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

fn directed_cost(cells: &[Point], costs: &IndexMap<(Point, usize), u32>) -> u64 {
    cells
        .windows(2)
        .map(|edge| {
            let direction = match (edge[1].x - edge[0].x, edge[1].y - edge[0].y) {
                (1, 0) => 0,
                (0, 1) => 1,
                (-1, 0) => 2,
                (0, -1) => 3,
                _ => unreachable!("routing kernel returns cardinal unit steps"),
            };
            u64::from(costs.get(&(edge[1], direction)).copied().unwrap_or(0))
        })
        .sum()
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

pub fn network_shared_cells(routes: &[Candidate]) -> usize {
    let mut uses = IndexMap::<Point, usize>::new();
    for route in routes {
        for point in route.cells.iter().copied().collect::<BTreeSet<_>>() {
            *uses.entry(point).or_default() += 1;
        }
    }
    uses.values().filter(|&&count| count > 1).count()
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
