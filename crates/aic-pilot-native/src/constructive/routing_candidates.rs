//! Bounded diverse route candidates for stable constructive states.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    io::Write,
    time::{Duration, Instant},
};

use aic_layout::{
    facilities::{FacilityPortDirection, FacilityPortEdge},
    geometry::{Bounds, Point, layer},
    logistics::{LogisticsComponentKind, TransportKind},
    physical::{Bridge, BridgeAxis},
    projection::{edge_index, project_port},
    validation::within_capacity,
};
use aic_routing::{
    Carrier, Crossing, CrossingVisit, PathOptions, RouteJob, crossing_options_with_visit,
    shortest_path,
};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use super::{Command, Diagnostic, Direction, ReplayMeter, ReplayWork, Session, Step, error};

pub const SCHEMA: &str = "aic-constructive-route-candidates-v4";
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
    #[serde(default)]
    pub request_allowance: AllowanceCaps,
}

/// Separate count ceilings for one logical-network request. Each ceiling is
/// finite; units are deliberately not combined into a synthetic work score.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AllowanceCaps {
    pub endpoint_visits: usize,
    pub raster_cells: usize,
    pub graph_rows: usize,
    pub route_cells: usize,
    pub blocked_visits: usize,
    pub copied_bytes: usize,
    pub inference_elements: usize,
    pub expansions: usize,
    pub frontier_entries: usize,
    pub routing_scans: usize,
    pub replay_route_cells: usize,
    pub replay_users: usize,
    pub catalog_visits: usize,
    pub validation_records: usize,
    pub published_bytes: usize,
    pub peak_live_bytes: usize,
    pub max_candidates: usize,
    pub max_variants: usize,
    pub max_inference_batch: usize,
    pub deadline_ms: u64,
}

impl Default for AllowanceCaps {
    fn default() -> Self {
        Self {
            endpoint_visits: 1_000_000,
            raster_cells: 10_000_000,
            graph_rows: 10_000_000,
            route_cells: 50_000_000,
            blocked_visits: 10_000_000,
            copied_bytes: 512 * 1024 * 1024,
            inference_elements: 100_000_000,
            expansions: 100_000_000,
            frontier_entries: 10_000_000,
            routing_scans: 100_000_000,
            replay_route_cells: 50_000_000,
            replay_users: 10_000_000,
            catalog_visits: 10_000_000,
            validation_records: 10_000_000,
            published_bytes: 64 * 1024 * 1024,
            peak_live_bytes: 1024 * 1024 * 1024,
            max_candidates: 64,
            max_variants: 64,
            max_inference_batch: 8,
            deadline_ms: 60_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum WorkKind {
    EndpointVisits,
    RasterCells,
    GraphRows,
    RouteCells,
    BlockedVisits,
    CopiedBytes,
    InferenceElements,
    Expansions,
    FrontierEntries,
    RoutingScans,
    ReplayRouteCells,
    ReplayUsers,
    CatalogVisits,
    ValidationRecords,
    PublishedBytes,
    Candidates,
    Variants,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AllowanceStop {
    pub reason: &'static str,
    pub category: Option<WorkKind>,
    pub limit: usize,
    pub used: usize,
    pub deadline_overshoot_ms: u64,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AllowanceUsage {
    pub categories: BTreeMap<WorkKind, usize>,
    pub peak_live_bytes: usize,
    pub deadline_overshoot_ms: u64,
}

pub struct RequestAllowance {
    caps: AllowanceCaps,
    used: BTreeMap<WorkKind, usize>,
    deadline: Instant,
    reserved_bytes: usize,
    peak_live_bytes: usize,
}

#[derive(Debug)]
pub enum RequestFailure {
    Invalid(Diagnostic),
    Stopped(AllowanceStop),
    PendingPlacement { endpoints: Vec<String> },
    NoCompatibleEndpoints { connection: String },
}

impl From<Diagnostic> for RequestFailure {
    fn from(value: Diagnostic) -> Self {
        Self::Invalid(value)
    }
}

impl From<AllowanceStop> for RequestFailure {
    fn from(value: AllowanceStop) -> Self {
        Self::Stopped(value)
    }
}

impl RequestFailure {
    fn into_diagnostic(self, connection: &str) -> Diagnostic {
        match self {
            Self::Invalid(diagnostic) => diagnostic,
            Self::Stopped(stop) => error(
                if stop.reason == "deadline" {
                    "route-request-deadline"
                } else {
                    "route-request-work-limit"
                },
                connection,
                format!(
                    "request preparation stopped: {} {:?}, used {}, limit {}",
                    stop.reason, stop.category, stop.used, stop.limit
                ),
            ),
            Self::PendingPlacement { endpoints } => error(
                "route-request-not-ready",
                connection,
                format!(
                    "route endpoint placement is pending: {}",
                    endpoints.join(", ")
                ),
            ),
            Self::NoCompatibleEndpoints { connection } => error(
                "route-no-compatible-endpoints",
                connection,
                "placed route endpoints have no compatible available port pair",
            ),
        }
    }
}

impl RequestAllowance {
    pub fn new(caps: AllowanceCaps) -> Self {
        let deadline = Instant::now() + Duration::from_millis(caps.deadline_ms);
        Self {
            caps,
            used: BTreeMap::new(),
            deadline,
            reserved_bytes: 0,
            peak_live_bytes: 0,
        }
    }

    pub fn caps(&self) -> &AllowanceCaps {
        &self.caps
    }

    pub fn remaining(&self, kind: WorkKind) -> usize {
        self.limit(kind)
            .saturating_sub(self.used.get(&kind).copied().unwrap_or(0))
    }

    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    pub fn check_deadline(&self) -> Result<(), AllowanceStop> {
        if let Some(overrun) = Instant::now().checked_duration_since(self.deadline) {
            return Err(AllowanceStop {
                reason: "deadline",
                category: None,
                limit: self.caps.deadline_ms as usize,
                used: self
                    .caps
                    .deadline_ms
                    .saturating_add(overrun.as_millis() as u64) as usize,
                deadline_overshoot_ms: overrun.as_millis() as u64,
            });
        }
        Ok(())
    }

    pub fn charge(&mut self, kind: WorkKind, count: usize) -> Result<(), AllowanceStop> {
        self.check_deadline()?;
        let limit = self.limit(kind);
        let old = self.used.get(&kind).copied().unwrap_or(0);
        let used = old.saturating_add(count);
        if used > limit {
            return Err(AllowanceStop {
                reason: "work-limit",
                category: Some(kind),
                limit,
                used: old,
                deadline_overshoot_ms: 0,
            });
        }
        self.used.insert(kind, used);
        Ok(())
    }

    /// Account work a bounded kernel has already performed. Such kernels may
    /// report one final probe beyond a cap; retain that real use in diagnostics.
    pub fn record_completed(&mut self, kind: WorkKind, count: usize) -> Result<(), AllowanceStop> {
        let limit = self.limit(kind);
        let used = self
            .used
            .get(&kind)
            .copied()
            .unwrap_or(0)
            .saturating_add(count);
        self.used.insert(kind, used);
        if used > limit {
            return Err(AllowanceStop {
                reason: "work-limit",
                category: Some(kind),
                limit,
                used,
                deadline_overshoot_ms: 0,
            });
        }
        self.check_deadline()
    }

    /// Record a live high-water mark, such as A* frontier occupancy. Searches
    /// run sequentially, so their peaks do not add to one another.
    pub fn record_peak(&mut self, kind: WorkKind, count: usize) -> Result<(), AllowanceStop> {
        let limit = self.limit(kind);
        let used = self.used.get(&kind).copied().unwrap_or(0).max(count);
        self.used.insert(kind, used);
        if used > limit {
            return Err(AllowanceStop {
                reason: "work-limit",
                category: Some(kind),
                limit,
                used,
                deadline_overshoot_ms: 0,
            });
        }
        self.check_deadline()
    }

    pub fn exhausted_stop(&self, kind: WorkKind) -> AllowanceStop {
        AllowanceStop {
            reason: "work-limit",
            category: Some(kind),
            limit: self.limit(kind),
            used: self.used.get(&kind).copied().unwrap_or(0),
            deadline_overshoot_ms: 0,
        }
    }

    pub fn preflight(&self, kind: WorkKind, upper_bound: usize) -> Result<(), AllowanceStop> {
        self.check_deadline()?;
        let limit = self.limit(kind);
        let used = self
            .used
            .get(&kind)
            .copied()
            .unwrap_or(0)
            .saturating_add(upper_bound);
        if used > limit {
            return Err(AllowanceStop {
                reason: "work-limit",
                category: Some(kind),
                limit,
                used: self.used.get(&kind).copied().unwrap_or(0),
                deadline_overshoot_ms: 0,
            });
        }
        Ok(())
    }

    pub fn reserve_peak(&mut self, bytes: usize) -> Result<(), AllowanceStop> {
        self.check_deadline()?;
        let peak = self.reserved_bytes.saturating_add(bytes);
        if peak > self.caps.peak_live_bytes {
            return Err(AllowanceStop {
                reason: "peak-memory-limit",
                category: None,
                limit: self.caps.peak_live_bytes,
                used: self.reserved_bytes,
                deadline_overshoot_ms: 0,
            });
        }
        self.reserved_bytes = peak;
        self.peak_live_bytes = self.peak_live_bytes.max(peak);
        Ok(())
    }

    pub fn release_peak(&mut self, bytes: usize) {
        self.reserved_bytes = self
            .reserved_bytes
            .checked_sub(bytes)
            .expect("request reservation release must match a prior reserve");
    }

    pub fn usage(&self) -> AllowanceUsage {
        AllowanceUsage {
            categories: self.used.clone(),
            peak_live_bytes: self.peak_live_bytes,
            deadline_overshoot_ms: Instant::now()
                .checked_duration_since(self.deadline)
                .map_or(0, |duration| duration.as_millis() as u64),
        }
    }

    fn limit(&self, kind: WorkKind) -> usize {
        match kind {
            WorkKind::EndpointVisits => self.caps.endpoint_visits,
            WorkKind::RasterCells => self.caps.raster_cells,
            WorkKind::GraphRows => self.caps.graph_rows,
            WorkKind::RouteCells => self.caps.route_cells,
            WorkKind::BlockedVisits => self.caps.blocked_visits,
            WorkKind::CopiedBytes => self.caps.copied_bytes,
            WorkKind::InferenceElements => self.caps.inference_elements,
            WorkKind::Expansions => self.caps.expansions,
            WorkKind::FrontierEntries => self.caps.frontier_entries,
            WorkKind::RoutingScans => self.caps.routing_scans,
            WorkKind::ReplayRouteCells => self.caps.replay_route_cells,
            WorkKind::ReplayUsers => self.caps.replay_users,
            WorkKind::CatalogVisits => self.caps.catalog_visits,
            WorkKind::ValidationRecords => self.caps.validation_records,
            WorkKind::PublishedBytes => self.caps.published_bytes,
            WorkKind::Candidates => self.caps.max_candidates,
            WorkKind::Variants => self.caps.max_variants,
        }
    }
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
    /// Unfinished obligations waiting for a block or terminal pose. Their
    /// identities remain in the graph even though they have no raster ports.
    pub pending_connections: Vec<String>,
    #[serde(skip)]
    pub reserved_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteReadiness {
    Ready,
    PendingPlacement { endpoints: Vec<String> },
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
    WorkLimit,
    Deadline,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowance: Option<AllowanceUsage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowance_stop: Option<AllowanceStop>,
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
            || !(1..=3_600_000).contains(&self.request_allowance.deadline_ms)
            || self.request_allowance.max_inference_batch == 0
            || self.request_allowance.max_inference_batch > 64
            || self.request_allowance.peak_live_bytes == 0
            || [
                self.request_allowance.endpoint_visits,
                self.request_allowance.raster_cells,
                self.request_allowance.graph_rows,
                self.request_allowance.route_cells,
                self.request_allowance.blocked_visits,
                self.request_allowance.copied_bytes,
                self.request_allowance.inference_elements,
                self.request_allowance.expansions,
                self.request_allowance.frontier_entries,
                self.request_allowance.routing_scans,
                self.request_allowance.replay_route_cells,
                self.request_allowance.replay_users,
                self.request_allowance.catalog_visits,
                self.request_allowance.validation_records,
                self.request_allowance.published_bytes,
                self.request_allowance.max_candidates,
                self.request_allowance.max_variants,
            ]
            .contains(&0)
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
    fn validate_with_allowance(
        &self,
        request: &Request,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<IndexMap<(Point, usize), u32>>, RequestFailure> {
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
            )
            .into());
        }
        let mut ids = BTreeSet::new();
        let mut maps = Vec::with_capacity(self.heads.len());
        for head in &self.heads {
            allowance.check_deadline()?;
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
                )
                .into());
            }
            allowance.preflight(WorkKind::RoutingScans, head.directed_costs.len())?;
            let mut map = IndexMap::with_capacity(head.directed_costs.len());
            for cost in &head.directed_costs {
                allowance.charge(WorkKind::RoutingScans, 1)?;
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
                    )
                    .into());
                }
            }
            maps.push(map);
        }
        Ok(maps)
    }
}

impl Session {
    /// Inspect only placement flags and stable identifiers before allocating
    /// endpoint domains or model inputs.
    pub fn route_request_readiness(&self, request: &Request) -> Result<RouteReadiness, Diagnostic> {
        if request.state_revision != self.revision || self.active.is_some() {
            return Err(error(
                "route-readiness-state",
                &request.connection,
                "route readiness requires the exact stable state revision",
            ));
        }
        let Some(&connection) = self.connections.get(&request.connection) else {
            return Err(error(
                "unknown-connection",
                &request.connection,
                "route readiness references an unknown logical connection",
            ));
        };
        if self.routes[connection].is_some() {
            return Err(error(
                "completed-connection",
                &request.connection,
                "route readiness references a completed logical connection",
            ));
        }
        let obligation = &self.problem.connections[connection];
        let mut pending = Vec::new();
        for endpoint in [&obligation.source, &obligation.target] {
            let placed = if let Some(&block) = self.blocks.get(endpoint) {
                self.poses[block].is_some()
            } else {
                self.terminal_poses[self.terminals[endpoint]].is_some()
            };
            if !placed && !pending.contains(endpoint) {
                pending.push(endpoint.clone());
            }
        }
        if pending.is_empty() {
            Ok(RouteReadiness::Ready)
        } else {
            Ok(RouteReadiness::PendingPlacement { endpoints: pending })
        }
    }

    pub fn route_guidance_input(
        &self,
        request: &Request,
        candidate: &Config,
        learning: &super::learning::Config,
    ) -> Result<GuidanceInput, Diagnostic> {
        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        self.route_guidance_input_with_allowance(request, candidate, learning, &mut allowance)
            .map_err(|failure| failure.into_diagnostic(&request.connection))
    }

    pub fn route_guidance_input_with_allowance(
        &self,
        request: &Request,
        candidate: &Config,
        learning: &super::learning::Config,
        allowance: &mut RequestAllowance,
    ) -> Result<GuidanceInput, RequestFailure> {
        candidate.validate()?;
        allowance.check_deadline()?;
        if learning.placement != candidate.bounds {
            return Err(error(
                "route-guidance-bounds",
                &request.connection,
                "candidate and learning bounds must match",
            )
            .into());
        }
        if request.state_revision != self.revision || self.active.is_some() {
            return Err(error(
                "route-guidance-state",
                &request.connection,
                "route guidance input requires the exact stable state revision",
            )
            .into());
        }
        let Some(&connection) = self.connections.get(&request.connection) else {
            return Err(error(
                "unknown-connection",
                &request.connection,
                "route guidance references an unknown logical connection",
            )
            .into());
        };
        if self.routes[connection].is_some() {
            return Err(error(
                "completed-connection",
                &request.connection,
                "route guidance references a completed logical connection",
            )
            .into());
        }
        if let RouteReadiness::PendingPlacement { endpoints } =
            self.route_request_readiness(request)?
        {
            return Err(RequestFailure::PendingPlacement { endpoints });
        }
        let pair_bytes = self
            .routes
            .iter()
            .enumerate()
            .filter(|(_, route)| route.is_none())
            .fold(0usize, |total, (index, _)| {
                total.saturating_add(self.endpoint_pair_reservation_bytes(index))
            });
        allowance.reserve_peak(pair_bytes)?;
        let prepared = (|| -> Result<GuidanceInput, RequestFailure> {
            let selected_pairs = self.endpoint_pairs(connection, candidate.bounds, allowance)?;
            if selected_pairs.is_empty() {
                return Err(RequestFailure::NoCompatibleEndpoints {
                    connection: request.connection.clone(),
                });
            }
            let mut remaining_pairs = Vec::new();
            let mut pending_connections = Vec::new();
            for other in 0..self.problem.connections.len() {
                if other == connection || self.routes[other].is_some() {
                    continue;
                }
                let request = Request {
                    state_revision: self.revision,
                    connection: self.problem.connections[other].id.clone(),
                };
                match self.route_request_readiness(&request)? {
                    RouteReadiness::Ready => {
                        let pairs = self.endpoint_pairs(other, candidate.bounds, allowance)?;
                        if pairs.is_empty() {
                            return Err(RequestFailure::NoCompatibleEndpoints {
                                connection: request.connection,
                            });
                        }
                        remaining_pairs.extend(pairs);
                    }
                    RouteReadiness::PendingPlacement { .. } => {
                        pending_connections.push(request.connection);
                    }
                }
            }
            let width =
                usize::try_from(candidate.bounds.width()).expect("validated positive width");
            let height =
                usize::try_from(candidate.bounds.height()).expect("validated positive height");
            let area = width * height;
            let raster_cells = area.saturating_mul(GUIDANCE_GRID_CHANNELS);
            let (graph_rows, graph_route_cells, graph_bytes) = self.guidance_graph_bounds();
            let bridge_copy_bytes = self.bridge_copy_bytes().saturating_mul(2);
            allowance.preflight(WorkKind::RasterCells, raster_cells)?;
            allowance.preflight(WorkKind::GraphRows, graph_rows)?;
            allowance.preflight(WorkKind::RouteCells, graph_route_cells.saturating_mul(2))?;
            allowance.preflight(WorkKind::CopiedBytes, bridge_copy_bytes)?;
            let reserved_bytes = raster_cells
                .saturating_mul(std::mem::size_of::<f32>())
                .saturating_add(graph_bytes)
                .saturating_add(bridge_copy_bytes);
            allowance.reserve_peak(reserved_bytes)?;
            let built = (|| -> Result<GuidanceInput, RequestFailure> {
                allowance.charge(WorkKind::CopiedBytes, bridge_copy_bytes)?;
                let mut grid = vec![0.0; GUIDANCE_GRID_CHANNELS * area];
                let index = |point: Point| {
                    usize::try_from(point.y - candidate.bounds.min_y).expect("point in bounds")
                        * width
                        + usize::try_from(point.x - candidate.bounds.min_x)
                            .expect("point in bounds")
                };
                let mark = |grid: &mut [f32], channel: usize, point: Point, value: f32| {
                    if candidate.bounds.contains(point) {
                        let offset = channel * area + index(point);
                        grid[offset] = grid[offset].max(value);
                    }
                };
                for &point in &self.bodies {
                    allowance.charge(WorkKind::BlockedVisits, 1)?;
                    mark(&mut grid, 0, point, 1.0);
                }
                for pose in self.terminal_poses.iter().flatten() {
                    allowance.charge(WorkKind::BlockedVisits, 1)?;
                    mark(&mut grid, 1, pose.position, 1.0);
                }
                let transport_layer = layer(self.problem.connections[connection].transport);
                for (&point, routes) in &self.occupied[transport_layer] {
                    allowance.charge(WorkKind::RouteCells, routes.len())?;
                    let channel = if self.shared_cell_available(point, connection) {
                        2
                    } else {
                        3
                    };
                    mark(&mut grid, channel, point, 1.0);
                    mark(&mut grid, 13, point, (routes.len() as f32 / 8.0).min(1.0));
                }
                for &point in self.occupied[1 - transport_layer].keys() {
                    allowance.charge(WorkKind::RouteCells, 1)?;
                    mark(&mut grid, 4, point, 1.0);
                }
                let mut bridge_resources = self.bridges.values().cloned().collect::<Vec<_>>();
                bridge_resources.sort_by(|left, right| left.id.cmp(&right.id));
                for bridge in &bridge_resources {
                    allowance.charge(WorkKind::ValidationRecords, 1)?;
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
                for pair in selected_pairs {
                    allowance.charge(WorkKind::EndpointVisits, 1)?;
                    mark(&mut grid, 5, pair.source.point, 1.0);
                    mark(&mut grid, 6, pair.target.point, 1.0);
                }
                let remaining =
                    self.problem.connections.len() - self.routes.iter().flatten().count() - 1;
                for pair in remaining_pairs {
                    allowance.charge(WorkKind::EndpointVisits, 1)?;
                    mark(&mut grid, 7, pair.source.point, 1.0);
                    mark(&mut grid, 8, pair.target.point, 1.0);
                }
                for y in candidate.bounds.min_y..=candidate.bounds.max_y {
                    for x in candidate.bounds.min_x..=candidate.bounds.max_x {
                        allowance.charge(WorkKind::RasterCells, GUIDANCE_GRID_CHANNELS)?;
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
                        grid[13 * area + index(point)] = grid[13 * area + index(point)].max(
                            (remaining as f32 / self.problem.connections.len() as f32).min(1.0),
                        );
                    }
                }
                allowance.check_deadline()?;
                let graph = self.route_guidance_learning_input_with_allowance(
                    learning, connection, allowance,
                )?;
                Ok(GuidanceInput {
                    schema: GUIDANCE_SCHEMA,
                    state_revision: request.state_revision,
                    connection: request.connection.clone(),
                    catalog_identity: self.catalogs.identity.clone(),
                    bounds: candidate.bounds,
                    channels: GUIDANCE_GRID_CHANNELS,
                    bridge_resources,
                    grid,
                    graph,
                    pending_connections,
                    reserved_bytes,
                })
            })();
            if built.is_err() {
                allowance.release_peak(reserved_bytes);
            }
            built
        })();
        allowance.release_peak(pair_bytes);
        prepared
    }

    pub fn route_candidates(
        &self,
        request: &Request,
        config: &Config,
    ) -> Result<Report, Diagnostic> {
        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        self.route_candidates_inner(request, config, None, &mut allowance, true)
    }

    pub fn route_candidates_with_guidance(
        &self,
        request: &Request,
        config: &Config,
        guidance: &Guidance,
    ) -> Result<Report, Diagnostic> {
        let mut allowance = RequestAllowance::new(config.request_allowance.clone());
        self.route_candidates_inner(request, config, Some(guidance), &mut allowance, true)
    }

    pub fn route_candidates_with_guidance_allowance(
        &self,
        request: &Request,
        config: &Config,
        guidance: &Guidance,
        allowance: &mut RequestAllowance,
    ) -> Result<Report, Diagnostic> {
        self.route_candidates_inner(request, config, Some(guidance), allowance, true)
    }

    pub fn route_candidates_for_network_with_allowance(
        &self,
        request: &Request,
        config: &Config,
        guidance: &Guidance,
        allowance: &mut RequestAllowance,
    ) -> Result<Report, Diagnostic> {
        self.route_candidates_inner(request, config, Some(guidance), allowance, false)
    }

    fn route_candidates_inner(
        &self,
        request: &Request,
        config: &Config,
        guidance: Option<&Guidance>,
        allowance: &mut RequestAllowance,
        publish_candidates: bool,
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

        let pair_phase_bytes = self.endpoint_pair_reservation_bytes(connection);
        if let Err(stop) = allowance.reserve_peak(pair_phase_bytes) {
            return Ok(stopped_report(
                request,
                config.max_crossings_per_route,
                0,
                allowance,
                stop,
            ));
        }
        let pair_result = self.endpoint_pairs(connection, config.bounds, allowance);
        allowance.release_peak(pair_phase_bytes);
        let pairs = match pair_result {
            Ok(pairs) => pairs,
            Err(RequestFailure::Invalid(diagnostic)) => return Err(diagnostic),
            Err(RequestFailure::Stopped(stop)) => {
                return Ok(stopped_report(
                    request,
                    config.max_crossings_per_route,
                    0,
                    allowance,
                    stop,
                ));
            }
            Err(RequestFailure::PendingPlacement { endpoints }) => {
                return Err(error(
                    "route-request-not-ready",
                    &request.connection,
                    format!("route endpoints are pending: {}", endpoints.join(", ")),
                ));
            }
            Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                unreachable!("endpoint enumeration returns an empty vector")
            }
        };
        if pairs.is_empty() {
            let mut report = empty_report(
                request,
                Termination::NoCompatibleEndpoints,
                0,
                false,
                config.max_crossings_per_route,
            );
            report.allowance = Some(allowance.usage());
            return Ok(report);
        }
        let guidance_map_bytes = guidance.map_or(0, |guidance| {
            guidance.heads.iter().fold(0usize, |total, head| {
                total.saturating_add(head.directed_costs.len().saturating_mul(96))
            })
        });
        // The tree used to order blocked sites and its final indexed set may
        // coexist. Keep their reservation through all searches using the set.
        let blocked_sites = self
            .bodies
            .len()
            .saturating_add(self.terminal_poses.len())
            .saturating_add(
                self.occupied[layer(self.problem.connections[connection].transport)].len(),
            );
        let mut carrier_cells = 0usize;
        for route in self.routes.iter().flatten() {
            if let Err(stop) = allowance.charge(WorkKind::ValidationRecords, 1) {
                return Ok(stopped_report(
                    request,
                    config.max_crossings_per_route,
                    pairs.len(),
                    allowance,
                    stop,
                ));
            }
            carrier_cells = carrier_cells.saturating_add(route.cells.len());
        }
        let crossing_bytes = if config.max_crossings_per_route == 0 {
            0
        } else {
            carrier_cells
                .saturating_mul(640)
                .saturating_add(blocked_sites.saturating_mul(64))
                .saturating_add(self.routes.len().saturating_mul(128))
                .saturating_add(self.bridges.len().saturating_mul(512))
                .saturating_add(
                    self.catalogs
                        .components
                        .catalog()
                        .components
                        .len()
                        .saturating_mul(256),
                )
        };
        let mut persistent_bytes = guidance_map_bytes
            .saturating_add(pairs.len().saturating_mul(512))
            .saturating_add(blocked_sites.saturating_mul(192))
            .saturating_add(crossing_bytes);
        if let Err(stop) = allowance.reserve_peak(persistent_bytes) {
            return Ok(stopped_report(
                request,
                config.max_crossings_per_route,
                pairs.len(),
                allowance,
                stop,
            ));
        }
        let result = (|| -> Result<Report, Diagnostic> {
            let guidance_maps = match guidance
                .map(|guidance| guidance.validate_with_allowance(request, config.bounds, allowance))
                .transpose()
            {
                Ok(maps) => maps,
                Err(RequestFailure::Invalid(diagnostic)) => return Err(diagnostic),
                Err(RequestFailure::Stopped(stop)) => {
                    return Ok(stopped_report(
                        request,
                        config.max_crossings_per_route,
                        pairs.len(),
                        allowance,
                        stop,
                    ));
                }
                Err(RequestFailure::PendingPlacement { .. }) => {
                    unreachable!("guidance validation does not inspect placement")
                }
                Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                    unreachable!("guidance validation does not inspect ports")
                }
            };
            let guidance_heads = guidance.map_or(&[][..], |guidance| guidance.heads.as_slice());
            let blocked =
                match self.route_blocked_cells_bounded(connection, config.bounds, allowance) {
                    Ok(blocked) => blocked,
                    Err(stop) => {
                        return Ok(stopped_report(
                            request,
                            config.max_crossings_per_route,
                            pairs.len(),
                            allowance,
                            stop,
                        ));
                    }
                };
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
                let mut excluded = IndexSet::new();
                for &point in &self.bodies {
                    if let Err(stop) = allowance.charge(WorkKind::BlockedVisits, 1) {
                        return Ok(stopped_report(
                            request,
                            config.max_crossings_per_route,
                            pairs.len(),
                            allowance,
                            stop,
                        ));
                    }
                    excluded.insert(point);
                }
                for &point in self.terminal_at.keys() {
                    if let Err(stop) = allowance.charge(WorkKind::BlockedVisits, 1) {
                        return Ok(stopped_report(
                            request,
                            config.max_crossings_per_route,
                            pairs.len(),
                            allowance,
                            stop,
                        ));
                    }
                    excluded.insert(point);
                }
                for &(transport, point) in self.junctions.keys().chain(self.bridges.keys()) {
                    if let Err(stop) = allowance.charge(WorkKind::BlockedVisits, 1) {
                        return Ok(stopped_report(
                            request,
                            config.max_crossings_per_route,
                            pairs.len(),
                            allowance,
                            stop,
                        ));
                    }
                    if transport == obligation.transport {
                        excluded.insert(point);
                    }
                }
                let mut crossings = match crossing_options_with_visit(
                    &carriers,
                    &RouteJob {
                        id: connection,
                        transport: obligation.transport,
                        rate: obligation.rate,
                    },
                    &self.catalogs.components,
                    &excluded,
                    |visit| {
                        allowance.charge(
                            match visit {
                                CrossingVisit::RouteCell => WorkKind::RouteCells,
                                CrossingVisit::CatalogEntry => WorkKind::CatalogVisits,
                                CrossingVisit::Candidate => WorkKind::RoutingScans,
                            },
                            1,
                        )
                    },
                ) {
                    Ok(crossings) => crossings,
                    Err(stop) => {
                        return Ok(stopped_report(
                            request,
                            config.max_crossings_per_route,
                            pairs.len(),
                            allowance,
                            stop,
                        ));
                    }
                };
                // An installed bridge is occupied, but a new route can join an
                // existing directed channel when its exact residual capacity fits.
                for (&(transport, position), bridge) in &self.bridges {
                    if transport != obligation.transport || !config.bounds.contains(position) {
                        continue;
                    }
                    let Some(definition) = self.catalogs.components.component(&bridge.component)
                    else {
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
                            let mut pi = None;
                            for (index, &point) in route.cells.iter().enumerate() {
                                if let Err(stop) = allowance.charge(WorkKind::RouteCells, 1) {
                                    return Ok(stopped_report(
                                        request,
                                        config.max_crossings_per_route,
                                        pairs.len(),
                                        allowance,
                                        stop,
                                    ));
                                }
                                if point == position {
                                    pi = Some(index);
                                    break;
                                }
                            }
                            let Some(pi) = pi else {
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
                        let Some(&carrier_network) = networks.get(channel.commodity.as_str())
                        else {
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
            let mut allowance_stop = None;
            macro_rules! keep_or_stop {
            ($label:lifetime, $work:expr) => {
                match $work {
                    Ok(value) => value,
                    Err(stop) => {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break $label termination;
                    }
                }
            };
        }
            let termination = 'rounds: loop {
                if let Err(stop) = allowance.check_deadline() {
                    let termination = stop_termination(&stop);
                    allowance_stop = Some(stop);
                    break termination;
                }
                let anchor = phase == 0;
                let guided = phase > 0 && phase <= guidance_heads.len();
                let guidance_index = if guided {
                    Some(phase - 1)
                } else if !anchor && !guidance_heads.is_empty() {
                    Some((phase - guidance_heads.len() - 1) % guidance_heads.len())
                } else {
                    None
                };
                let edge_penalties = guidance_index
                    .and_then(|index| guidance_maps.as_ref().map(|maps| &maps[index]));
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
                    let allowance_remaining = allowance.remaining(WorkKind::Expansions);
                    if allowance_remaining == 0 {
                        let stop = allowance.charge(WorkKind::Expansions, 1).unwrap_err();
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                    let work_budget = remaining
                        .min(config.work_per_search)
                        .min(allowance_remaining);
                    let scan_bound = penalties
                        .len()
                        .saturating_add(edge_penalties.map_or(0, |costs| costs.len()));
                    if let Err(stop) = allowance.preflight(WorkKind::RoutingScans, scan_bound) {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                    let state_bound = work_budget
                        .saturating_mul(8)
                        .saturating_add(4)
                        .min(1_000_000);
                    let search_bytes = state_bound
                        .saturating_mul(256)
                        .saturating_add(blocked.len().saturating_mul(128))
                        .saturating_add(crossings.len().saturating_mul(256))
                        .saturating_add(pairs.len().saturating_mul(128))
                        .saturating_add(
                            candidates
                                .iter()
                                .map(|candidate: &Candidate| {
                                    candidate.cells.len().saturating_mul(128)
                                })
                                .sum::<usize>(),
                        );
                    if let Err(stop) = allowance.reserve_peak(search_bytes) {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                    let result = shortest_path(
                        pair.source.point,
                        pair.target.point,
                        &blocked,
                        config.bounds,
                        PathOptions {
                            first: pair.source.direction,
                            last: pair.target.direction,
                            work_budget,
                            scan_budget: allowance.remaining(WorkKind::RoutingScans).max(1),
                            frontier_budget: allowance.caps().frontier_entries,
                            deadline: Some(allowance.deadline()),
                            crossings: &crossings,
                            crossing_limit: config.max_crossings_per_route,
                            prefer_crossings: phase > 0,
                            allow_zero_length: false,
                            cell_penalties: (!anchor && !guided).then_some(&penalties),
                            edge_penalties,
                        },
                    );
                    allowance.release_peak(search_bytes);
                    let result = result.map_err(|failure| {
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
                    let scan_stop = allowance
                        .record_completed(WorkKind::RoutingScans, result.routing_scans)
                        .err();
                    let frontier_stop = allowance
                        .record_peak(WorkKind::FrontierEntries, result.frontier_entries_peak)
                        .err();
                    let expansion_stop = allowance
                        .record_completed(WorkKind::Expansions, result.expanded)
                        .err();
                    if let Some(stop) = scan_stop.or(frontier_stop).or(expansion_stop) {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
                    if result.status == "deadline" {
                        let stop = allowance.check_deadline().unwrap_err();
                        allowance_stop = Some(stop);
                        break 'rounds Termination::Deadline;
                    }
                    if result.status == "scan-budget" || result.status == "frontier-budget" {
                        let kind = if result.status == "scan-budget" {
                            WorkKind::RoutingScans
                        } else {
                            WorkKind::FrontierEntries
                        };
                        let stop = allowance.exhausted_stop(kind);
                        allowance_stop = Some(stop);
                        break 'rounds Termination::WorkLimit;
                    }
                    if let Err(stop) = allowance.check_deadline() {
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
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
                    // The path key, penalties, command buffer and candidate
                    // coexist during postprocessing. Keep a conservative
                    // reservation through that phase, then retain only the
                    // entries that remain live for later searches.
                    let postprocess_bytes = cells
                        .len()
                        .saturating_mul(2048)
                        .saturating_add(found_crossings.len().saturating_mul(1024))
                        .saturating_add(4096);
                    keep_or_stop!('rounds, allowance.reserve_peak(postprocess_bytes));
                    persistent_bytes = persistent_bytes.saturating_add(postprocess_bytes);
                    let penalties_before = penalties.len();
                    guides_found += 1;
                    let source_port = pair.source.port.clone();
                    let target_port = pair.target.port.clone();
                    keep_or_stop!('rounds, allowance.charge(WorkKind::RoutingScans, cells.len().saturating_add(found_crossings.len())));
                    keep_or_stop!('rounds, allowance.charge(WorkKind::CopiedBytes, cells.len().saturating_mul(std::mem::size_of::<Point>())));
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
                    let overlap =
                        keep_or_stop!('rounds, maximum_overlap(&cells, &candidates, allowance));
                    let learned_cost = if let Some(costs) = edge_penalties {
                        keep_or_stop!('rounds, directed_cost(&cells, costs, allowance))
                    } else {
                        0
                    };
                    keep_or_stop!('rounds, add_overlap_penalty(&mut penalties, &cells, config.overlap_penalty, allowance));
                    let penalty_bytes = penalties
                        .len()
                        .saturating_sub(penalties_before)
                        .saturating_mul(128);
                    if !semantic.insert(key) {
                        duplicate_guides += 1;
                        allowance.release_peak(postprocess_bytes - penalty_bytes);
                        persistent_bytes -= postprocess_bytes - penalty_bytes;
                        continue;
                    }
                    let key_bytes = cells
                        .len()
                        .saturating_mul(64)
                        .saturating_add(found_crossings.len().saturating_mul(256))
                        .saturating_add(
                            source_port
                                .as_ref()
                                .map_or(0, String::len)
                                .saturating_mul(2),
                        )
                        .saturating_add(
                            target_port
                                .as_ref()
                                .map_or(0, String::len)
                                .saturating_mul(2),
                        )
                        .saturating_add(512);
                    let commands = self.route_commands(
                        &request.connection,
                        source_port.clone(),
                        target_port.clone(),
                        &cells,
                        &found_crossings,
                        allowance,
                    );
                    let commands = keep_or_stop!('rounds, commands);
                    match self.replay_route_witness_with_allowance(
                        request.state_revision,
                        &commands,
                        &request.connection,
                        cells.len(),
                        found_crossings.len(),
                        allowance,
                    ) {
                        Ok(_) => {}
                        Err(RequestFailure::Invalid(diagnostic)) => {
                            rejections.push(Rejection {
                                source_port,
                                target_port,
                                diagnostic,
                            });
                            let retained =
                                penalty_bytes.saturating_add(key_bytes).saturating_add(2048);
                            let slack = postprocess_bytes.saturating_sub(retained);
                            allowance.release_peak(slack);
                            persistent_bytes -= slack;
                            continue;
                        }
                        Err(RequestFailure::Stopped(stop)) => {
                            let termination = stop_termination(&stop);
                            allowance_stop = Some(stop);
                            break 'rounds termination;
                        }
                        Err(RequestFailure::PendingPlacement { endpoints }) => {
                            return Err(error(
                                "route-request-not-ready",
                                &request.connection,
                                format!("route endpoints are pending: {}", endpoints.join(", ")),
                            ));
                        }
                        Err(RequestFailure::NoCompatibleEndpoints { .. }) => {
                            unreachable!("native replay does not prepare endpoints")
                        }
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
                                model_version: guidance
                                    .map(|guidance| guidance.model_version.clone()),
                                head: guidance_index.map(|index| guidance_heads[index].id.clone()),
                            }
                        },
                    });
                    let candidate = candidates.last().expect("candidate just pushed");
                    let candidate_bytes = candidate
                        .cells
                        .len()
                        .saturating_mul(64)
                        .saturating_add(candidate.commands.len().saturating_mul(128))
                        .saturating_add(candidate.crossings.len().saturating_mul(256))
                        .saturating_add(1024);
                    let retained = penalty_bytes
                        .saturating_add(key_bytes)
                        .saturating_add(candidate_bytes);
                    let slack = postprocess_bytes.saturating_sub(retained);
                    allowance.release_peak(slack);
                    persistent_bytes -= slack;
                    let publication = if publish_candidates {
                        match json_size_with_allowance(candidate, allowance) {
                            Ok(_) => allowance.charge(WorkKind::Candidates, 1),
                            Err(RequestFailure::Stopped(stop)) => Err(stop),
                            Err(RequestFailure::Invalid(diagnostic)) => return Err(diagnostic),
                            Err(_) => {
                                unreachable!("candidate serialization does not prepare endpoints")
                            }
                        }
                    } else {
                        let bytes = candidate
                            .cells
                            .len()
                            .saturating_mul(std::mem::size_of::<Point>())
                            .saturating_add(
                                candidate
                                    .commands
                                    .len()
                                    .saturating_mul(std::mem::size_of::<Command>()),
                            )
                            .saturating_add(512);
                        allowance.charge(WorkKind::CopiedBytes, bytes)
                    };
                    if let Err(stop) = publication.and_then(|_| allowance.check_deadline()) {
                        candidates.pop();
                        let termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        break 'rounds termination;
                    }
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
            let mut termination = termination;
            // A crossing-count cap prunes otherwise legal paths. Exhausting that
            // reduced graph cannot prove the complete fixed request impossible.
            let crossing_domain_complete = if config.max_crossings_per_route == 0 {
                self.routes.iter().all(Option::is_none)
            } else {
                crossings.len() <= config.max_crossings_per_route
            };
            if termination == Termination::FrontierExhausted && !crossing_domain_complete {
                termination = Termination::ResourceLimit;
            }
            let (minimum_pairwise_dissimilarity, mean_pairwise_dissimilarity) =
                match diversity(&candidates, allowance) {
                    Ok(metrics) => metrics,
                    Err(stop) => {
                        termination = stop_termination(&stop);
                        allowance_stop = Some(stop);
                        (None, None)
                    }
                };
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
                allowance: Some(allowance.usage()),
                allowance_stop,
            })
        })();
        allowance.release_peak(persistent_bytes);
        result
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
        let mut valid_shape = matches!(
            candidate.commands.first(),
            Some(Command::BeginRoute {
                connection,
                source_port,
                target_port,
            }) if connection == &candidate.connection
                && source_port == &candidate.source_port
                && target_port == &candidate.target_port
        ) && candidate.commands.len() >= 2
            && candidate.commands.last() == Some(&Command::Connect);
        if valid_shape {
            for command in &candidate.commands[1..candidate.commands.len() - 1] {
                self.charge_replay(ReplayWork::Validation, 1)?;
                if !matches!(
                    command,
                    Command::Extend { .. } | Command::ReserveBridge { .. }
                ) {
                    valid_shape = false;
                    break;
                }
            }
        }
        if !valid_shape {
            return Err(error(
                "route-candidate-witness",
                &candidate.connection,
                "route candidate does not contain one complete route witness",
            ));
        }
        let mut reservations = Vec::new();
        for command in &candidate.commands {
            self.charge_replay(ReplayWork::Validation, 1)?;
            if let Some(reservation) = match command {
                Command::ReserveBridge {
                    position,
                    component,
                    rotation,
                    crossed_axis,
                } => Some((*position, component.as_str(), *rotation, *crossed_axis)),
                _ => None,
            } {
                reservations.push(reservation);
            }
        }
        let mut networks = IndexMap::<&str, usize>::new();
        for (ci, route) in self.routes.iter().enumerate() {
            self.charge_replay(ReplayWork::Validation, 1)?;
            if route.is_some() {
                let commodity = self.problem.connections[ci].commodity.as_str();
                let next = networks.len();
                networks.entry(commodity).or_insert(next);
            }
        }
        let mut crossing_mismatch = reservations.len() != candidate.crossings.len();
        if !crossing_mismatch {
            for (&(position, component, rotation, crossed_axis), crossing) in
                reservations.iter().zip(&candidate.crossings)
            {
                self.charge_replay(ReplayWork::Validation, 1)?;
                let axis = if crossing.direction % 2 == 0 {
                    BridgeAxis::Vertical
                } else {
                    BridgeAxis::Horizontal
                };
                let mut incoming_direction = None;
                for (pi, &point) in candidate.cells.iter().enumerate() {
                    self.charge_replay(ReplayWork::Validation, 1)?;
                    if point == position {
                        if pi > 0 {
                            incoming_direction =
                                aic_layout::geometry::direction(candidate.cells[pi - 1], point);
                        }
                        break;
                    }
                }
                let bridge_owner =
                    self.bridges
                        .get(&(crossing.transport, position))
                        .and_then(|bridge| {
                            bridge.channels[crossing.direction % 2]
                                .connections
                                .first()
                                .and_then(|owner| self.connections.get(owner).copied())
                        });
                let carrier_owner = if let Some(owner) = bridge_owner {
                    Some(owner)
                } else {
                    let mut first_user = None;
                    if let Some(users) = self.occupied[layer(crossing.transport)].get(&position) {
                        for &user in users {
                            self.charge_replay(ReplayWork::Users, 1)?;
                            first_user =
                                Some(first_user.map_or(user, |first: usize| first.min(user)));
                        }
                    }
                    first_user
                };
                crossing_mismatch = crossing.position != position
                    || crossing.component != component
                    || crossing.rotation != rotation
                    || crossing.kind != LogisticsComponentKind::Bridge
                    || crossing.transport != self.problem.connections[connection].transport
                    || crossed_axis != axis
                    || incoming_direction != Some(crossing.direction)
                    || carrier_owner != Some(crossing.carrier_chain)
                    || self
                        .problem
                        .connections
                        .get(crossing.carrier_chain)
                        .and_then(|carrier| networks.get(carrier.commodity.as_str()).copied())
                        != Some(crossing.carrier_network);
                if crossing_mismatch {
                    break;
                }
            }
        }
        if crossing_mismatch {
            return Err(error(
                "route-candidate-bridge-witness",
                &candidate.connection,
                "bridge metadata does not match the reserved native commands",
            ));
        }
        let (fork, steps) =
            self.replay_route_witness(candidate.state_revision, &candidate.commands)?;
        let geometry_matches = if let Some(route) = fork.routes[connection].as_ref() {
            if route.cells.len() != candidate.cells.len() {
                false
            } else {
                let mut same = true;
                for (actual, expected) in route.cells.iter().zip(&candidate.cells) {
                    self.charge_replay(ReplayWork::Validation, 1)?;
                    if actual != expected {
                        same = false;
                        break;
                    }
                }
                same
            }
        } else {
            false
        };
        if !geometry_matches {
            return Err(error(
                "route-candidate-geometry",
                &candidate.connection,
                "replayed route does not match candidate geometry",
            ));
        }
        let mut crossings_match = true;
        for crossing in &candidate.crossings {
            self.charge_replay(ReplayWork::Validation, 1)?;
            let Some(bridge) = fork.bridges.get(&(crossing.transport, crossing.position)) else {
                crossings_match = false;
                break;
            };
            if bridge.component != crossing.component || bridge.rotation != crossing.rotation {
                crossings_match = false;
                break;
            }
            let mut connection_found = false;
            for owner in &bridge.channels[crossing.direction % 2].connections {
                self.charge_replay(ReplayWork::Validation, 1)?;
                if owner == &candidate.connection {
                    connection_found = true;
                    break;
                }
            }
            if !connection_found {
                crossings_match = false;
                break;
            }
        }
        if !crossings_match {
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
        let shared_cells = if self.replay_meter.is_some() {
            network_shared_cells_with_replay(&candidate.routes, |count| {
                self.charge_replay(ReplayWork::Validation, count)
            })?
        } else {
            network_shared_cells(&candidate.routes)
        };
        if candidate.steps != steps_total
            || candidate.turns != turns_total
            || candidate.expanded != expanded_total
            || candidate.shared_cells != shared_cells
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

    fn would_complete_after_route(
        &self,
        connection: &str,
        allowance: &mut RequestAllowance,
    ) -> Result<bool, AllowanceStop> {
        allowance.charge(
            WorkKind::ValidationRecords,
            self.poses
                .len()
                .saturating_add(self.terminal_poses.len())
                .saturating_add(self.routes.len()),
        )?;
        Ok(self.poses.iter().all(Option::is_some)
            && self.terminal_poses.iter().all(Option::is_some)
            && self.routes.iter().enumerate().all(|(index, route)| {
                route.is_some() || self.problem.connections[index].id == connection
            }))
    }

    fn apply_with_request_allowance<T>(
        &mut self,
        allowance: &mut RequestAllowance,
        copied_bytes: usize,
        peak_bytes: usize,
        apply: impl FnOnce(&mut Session) -> Result<T, Diagnostic>,
    ) -> Result<T, RequestFailure> {
        allowance.preflight(WorkKind::CopiedBytes, copied_bytes)?;
        allowance.reserve_peak(peak_bytes)?;
        if let Err(stop) = allowance.charge(WorkKind::CopiedBytes, copied_bytes) {
            allowance.release_peak(peak_bytes);
            return Err(RequestFailure::Stopped(stop));
        }
        let meter = std::sync::Arc::new(ReplayMeter::new([
            allowance.remaining(WorkKind::ReplayRouteCells),
            allowance.remaining(WorkKind::ReplayUsers),
            allowance.remaining(WorkKind::CatalogVisits),
            allowance.remaining(WorkKind::ValidationRecords),
        ]));
        let mut fork = self.clone();
        fork.replay_deadline = Some(allowance.deadline());
        fork.replay_meter = Some(meter.clone());
        let result = apply(&mut fork);
        let mut completed_stop = None;
        for (kind, used) in [
            WorkKind::ReplayRouteCells,
            WorkKind::ReplayUsers,
            WorkKind::CatalogVisits,
            WorkKind::ValidationRecords,
        ]
        .into_iter()
        .zip(meter.usage())
        {
            if let Err(stop) = allowance.record_completed(kind, used) {
                completed_stop.get_or_insert(stop);
            }
        }
        let stop = meter
            .stopped()
            .map(|kind| {
                allowance.exhausted_stop(match kind {
                    ReplayWork::RouteCells => WorkKind::ReplayRouteCells,
                    ReplayWork::Users => WorkKind::ReplayUsers,
                    ReplayWork::Catalog => WorkKind::CatalogVisits,
                    ReplayWork::Validation => WorkKind::ValidationRecords,
                })
            })
            .or(completed_stop)
            .or_else(|| allowance.check_deadline().err());
        allowance.release_peak(peak_bytes);
        if let Some(stop) = stop {
            return Err(RequestFailure::Stopped(stop));
        }
        let value = result.map_err(RequestFailure::Invalid)?;
        fork.replay_meter = None;
        fork.replay_deadline = None;
        *self = fork;
        Ok(value)
    }

    pub fn apply_route_candidate_with_allowance(
        &mut self,
        candidate: &Candidate,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<Step>, RequestFailure> {
        let root_bytes = self.clone_reservation_bytes();
        let final_layout_copy =
            self.would_complete_after_route(&candidate.connection, allowance)?;
        let projected_bytes = root_bytes
            .saturating_add(candidate.cells.len().saturating_mul(4096))
            .saturating_add(candidate.crossings.len().saturating_mul(4096))
            .saturating_add(candidate.commands.len().saturating_mul(128))
            .saturating_add(4096);
        let copies = root_bytes
            .saturating_mul(2)
            .saturating_add(if final_layout_copy {
                projected_bytes
            } else {
                0
            });
        let peak = root_bytes
            .saturating_add(projected_bytes.saturating_mul(1 + usize::from(final_layout_copy)));
        self.apply_with_request_allowance(allowance, copies, peak, |fork| {
            fork.apply_route_candidate(candidate)
        })
    }

    pub fn apply_network_candidate_with_allowance(
        &mut self,
        candidate: &NetworkCandidate,
        projected_final_clone_bytes: usize,
        projected_copied_bytes: usize,
        final_layout_copy: bool,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<Step>, RequestFailure> {
        let root_bytes = self.clone_reservation_bytes();
        let final_bytes = projected_final_clone_bytes.max(root_bytes);
        let peak_bytes = root_bytes
            .saturating_add(final_bytes.saturating_mul(2 + usize::from(final_layout_copy)));
        self.apply_with_request_allowance(allowance, projected_copied_bytes, peak_bytes, |fork| {
            fork.apply_network_candidate(candidate)
        })
    }

    fn endpoint_pairs(
        &self,
        connection: usize,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<EndpointPair>, RequestFailure> {
        let obligation = &self.problem.connections[connection];
        let sources = self.route_candidate_endpoints(
            &obligation.source,
            FacilityPortDirection::Output,
            connection,
            allowance,
        )?;
        let targets = self.route_candidate_endpoints(
            &obligation.target,
            FacilityPortDirection::Input,
            connection,
            allowance,
        )?;
        let mut ordered = BTreeMap::new();
        for source in &sources {
            for target in &targets {
                allowance.charge(WorkKind::EndpointVisits, 1)?;
                if bounds.contains(source.point) && bounds.contains(target.point) {
                    allowance.charge(WorkKind::RoutingScans, 1)?;
                    ordered.insert(
                        (
                            source.point.distance(target.point),
                            source.port.clone(),
                            target.port.clone(),
                        ),
                        EndpointPair {
                            source: source.clone(),
                            target: target.clone(),
                        },
                    );
                }
            }
        }
        Ok(ordered.into_values().collect())
    }

    fn endpoint_pair_reservation_bytes(&self, connection: usize) -> usize {
        let obligation = &self.problem.connections[connection];
        let count = |endpoint: &str| {
            self.blocks.get(endpoint).map_or(1, |&block| {
                self.catalogs
                    .facilities
                    .facility(&self.problem.blocks[block].facility)
                    .expect("admitted facility")
                    .ports
                    .len()
            })
        };
        let sources = count(&obligation.source);
        let targets = count(&obligation.target);
        sources
            .saturating_add(targets)
            .saturating_mul(256)
            .saturating_add(sources.saturating_mul(targets).saturating_mul(1024))
    }

    fn guidance_graph_bounds(&self) -> (usize, usize, usize) {
        let entities = self
            .problem
            .blocks
            .len()
            .saturating_add(self.problem.terminals.len())
            .saturating_add(self.problem.connections.len());
        let relations = self.problem.connections.len().saturating_mul(2);
        let route_cells = self
            .routes
            .iter()
            .flatten()
            .map(|route| route.cells.len())
            .sum::<usize>();
        let bridge_rows = self
            .bridges
            .values()
            .flat_map(|bridge| bridge.channels.iter())
            .map(|channel| channel.connections.len())
            .sum::<usize>();
        let spatial = self
            .bodies
            .len()
            .saturating_add(route_cells)
            .saturating_add(bridge_rows)
            .saturating_add(self.terminal_poses.iter().flatten().count());
        let rows = entities.saturating_add(relations).saturating_add(spatial);
        let bytes = entities
            .saturating_mul(super::learning::ENTITY_FEATURES * std::mem::size_of::<f32>())
            .saturating_add(relations.saturating_mul(
                2 * std::mem::size_of::<i64>()
                    + super::learning::RELATION_FEATURES * std::mem::size_of::<f32>(),
            ))
            .saturating_add(spatial.saturating_mul(
                super::learning::SPATIAL_FEATURES * std::mem::size_of::<f32>()
                    + std::mem::size_of::<i64>(),
            ))
            .saturating_mul(2);
        (rows, route_cells, bytes)
    }

    fn bridge_copy_bytes(&self) -> usize {
        self.bridges
            .values()
            .map(|bridge| {
                std::mem::size_of::<Bridge>()
                    .saturating_add(bridge.id.len())
                    .saturating_add(bridge.component.len())
                    .saturating_add(
                        bridge
                            .channels
                            .iter()
                            .map(|channel| {
                                channel
                                    .commodity
                                    .len()
                                    .saturating_add(
                                        channel.connections.iter().map(String::len).sum::<usize>(),
                                    )
                                    .saturating_add(
                                        channel
                                            .connections
                                            .len()
                                            .saturating_mul(std::mem::size_of::<String>()),
                                    )
                            })
                            .sum::<usize>(),
                    )
            })
            .sum()
    }

    fn route_candidate_endpoints(
        &self,
        endpoint: &str,
        expected: FacilityPortDirection,
        connection: usize,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<Endpoint>, RequestFailure> {
        let obligation = &self.problem.connections[connection];
        let Some(&block) = self.blocks.get(endpoint) else {
            allowance.charge(WorkKind::EndpointVisits, 1)?;
            let (point, _) = self.route_endpoint(endpoint, None, expected, obligation)?;
            return Ok(vec![Endpoint {
                port: None,
                point,
                direction: None,
            }]);
        };
        let pose = self.poses[block].as_ref().ok_or_else(|| {
            error(
                "unplaced-endpoint",
                endpoint,
                "route endpoint facility is not placed",
            )
        })?;
        let definition = self
            .catalogs
            .facilities
            .facility(&self.problem.blocks[block].facility)
            .expect("admitted facility definition");
        let mut endpoints = Vec::new();
        for port_definition in &definition.ports {
            allowance.charge(WorkKind::EndpointVisits, 1)?;
            let port = project_port(definition, port_definition, pose.rotation, pose.position);
            let available = {
                if port.direction != expected || port.transport != obligation.transport {
                    false
                } else {
                    self.used_ports
                        .get(&(block, port.id.clone()))
                        .is_none_or(|used| {
                            let Ok(total) = used.rate.checked_add(obligation.rate) else {
                                return false;
                            };
                            let capacity = self.catalogs.transports.capacity(obligation.transport);
                            used.commodity == obligation.commodity
                                && used.transport == obligation.transport
                                && used.direction == expected
                                && within_capacity(total, capacity.quantity, capacity.duration_ms)
                        })
                }
            };
            if available {
                endpoints.push(Endpoint {
                    port: Some(port.id),
                    point: port.connection,
                    direction: Some(match expected {
                        FacilityPortDirection::Output => edge_index(port.edge),
                        FacilityPortDirection::Input => opposite(port.edge),
                    }),
                });
            }
        }
        endpoints.sort_by(|left, right| left.port.cmp(&right.port));
        Ok(endpoints)
    }

    fn route_blocked_cells_bounded(
        &self,
        connection: usize,
        bounds: Bounds,
        allowance: &mut RequestAllowance,
    ) -> Result<IndexSet<Point>, AllowanceStop> {
        let obligation = &self.problem.connections[connection];
        let occupied = &self.occupied[layer(obligation.transport)];
        let mut blocked = std::collections::BTreeSet::new();
        for &point in &self.bodies {
            allowance.charge(WorkKind::BlockedVisits, 1)?;
            if bounds.contains(point) {
                blocked.insert((point.y, point.x));
            }
        }
        for (terminal, pose) in self.terminal_poses.iter().enumerate() {
            allowance.charge(WorkKind::BlockedVisits, 1)?;
            let id = &self.problem.terminals[terminal].id;
            if id != &obligation.source && id != &obligation.target {
                if let Some(pose) = pose {
                    if bounds.contains(pose.position) {
                        blocked.insert((pose.position.y, pose.position.x));
                    }
                }
            }
        }
        for (&point, users) in occupied {
            allowance.charge(WorkKind::BlockedVisits, 1)?;
            if !bounds.contains(point) {
                continue;
            }
            let blocked_here = if self.bridges.contains_key(&(obligation.transport, point)) {
                true
            } else {
                allowance.charge(WorkKind::BlockedVisits, users.len())?;
                !self.shared_cell_available(point, connection)
            };
            if blocked_here {
                blocked.insert((point.y, point.x));
            }
        }
        Ok(blocked.into_iter().map(|(y, x)| Point { x, y }).collect())
    }

    fn route_commands(
        &self,
        connection: &str,
        source_port: Option<String>,
        target_port: Option<String>,
        cells: &[Point],
        crossings: &[Crossing],
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<Command>, AllowanceStop> {
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
            allowance.charge(WorkKind::RoutingScans, 1)?;
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
                allowance.charge(WorkKind::RoutingScans, 1)?;
                end += 1;
            }
            commands.push(Command::Extend {
                direction: heading,
                length: (end - start) as i64,
            });
            start = end;
        }
        commands.push(Command::Connect);
        Ok(commands)
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

    fn replay_route_witness_with_allowance(
        &self,
        state_revision: u64,
        commands: &[Command],
        connection: &str,
        route_cells: usize,
        route_crossings: usize,
        allowance: &mut RequestAllowance,
    ) -> Result<(Session, Vec<Step>), RequestFailure> {
        if self.revision != state_revision {
            return Err(error(
                "stale-route-candidate",
                "route-candidate",
                "route candidate belongs to an earlier constructive revision",
            )
            .into());
        }
        let final_layout_copy = self.would_complete_after_route(connection, allowance)?;
        let root_bytes = self.clone_reservation_bytes();
        let final_bytes = root_bytes
            .saturating_add(route_cells.saturating_mul(4096))
            .saturating_add(route_crossings.saturating_mul(4096))
            .saturating_add(commands.len().saturating_mul(128))
            .saturating_add(4096);
        let copied = root_bytes.saturating_add(if final_layout_copy { final_bytes } else { 0 });
        let peak = final_bytes.saturating_mul(1 + usize::from(final_layout_copy));
        allowance.preflight(WorkKind::CopiedBytes, copied)?;
        allowance.reserve_peak(peak)?;
        if let Err(stop) = allowance.charge(WorkKind::CopiedBytes, copied) {
            allowance.release_peak(peak);
            return Err(RequestFailure::Stopped(stop));
        }
        let mut fork = self.clone();
        fork.replay_deadline = Some(allowance.deadline());
        fork.replay_meter = Some(std::sync::Arc::new(ReplayMeter::new([
            allowance.remaining(WorkKind::ReplayRouteCells),
            allowance.remaining(WorkKind::ReplayUsers),
            allowance.remaining(WorkKind::CatalogVisits),
            allowance.remaining(WorkKind::ValidationRecords),
        ])));
        let mut steps = Vec::with_capacity(commands.len());
        let mut failure = None;
        for command in commands {
            let records = 1usize
                .saturating_add(fork.poses.len().saturating_mul(3))
                .saturating_add(fork.terminal_poses.len().saturating_mul(3))
                .saturating_add(fork.routes.len().saturating_mul(3));
            if let Err(stop) = allowance.charge(WorkKind::ValidationRecords, records) {
                failure = Some(RequestFailure::Stopped(stop));
                break;
            }
            let step = fork.step(command.clone());
            if !step.accepted || step.diagnostic.is_some() {
                failure = Some(RequestFailure::Invalid(step.diagnostic.unwrap_or_else(
                    || {
                        error(
                            "route-candidate-replay",
                            "route-candidate",
                            "native replay rejected a route candidate",
                        )
                    },
                )));
                break;
            }
            steps.push(step);
        }
        let meter = fork.replay_meter.take().expect("replay meter installed");
        let mut completed_stop = None;
        for (kind, used) in [
            WorkKind::ReplayRouteCells,
            WorkKind::ReplayUsers,
            WorkKind::CatalogVisits,
            WorkKind::ValidationRecords,
        ]
        .into_iter()
        .zip(meter.usage())
        {
            if let Err(stop) = allowance.record_completed(kind, used) {
                completed_stop.get_or_insert(stop);
            }
        }
        if let Some(kind) = meter.stopped() {
            let kind = match kind {
                ReplayWork::RouteCells => WorkKind::ReplayRouteCells,
                ReplayWork::Users => WorkKind::ReplayUsers,
                ReplayWork::Catalog => WorkKind::CatalogVisits,
                ReplayWork::Validation => WorkKind::ValidationRecords,
            };
            failure = Some(RequestFailure::Stopped(allowance.exhausted_stop(kind)));
        } else if let Some(stop) = completed_stop {
            failure = Some(RequestFailure::Stopped(stop));
        } else if let Err(stop) = allowance.check_deadline() {
            failure = Some(RequestFailure::Stopped(stop));
        }
        allowance.release_peak(peak);
        if let Some(failure) = failure {
            return Err(failure);
        }
        fork.replay_deadline = None;
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
        allowance: None,
        allowance_stop: None,
    }
}

fn stopped_report(
    request: &Request,
    crossing_limit: usize,
    endpoint_pairs: usize,
    allowance: &RequestAllowance,
    stop: AllowanceStop,
) -> Report {
    let termination = stop_termination(&stop);
    let mut report = empty_report(request, termination, endpoint_pairs, false, crossing_limit);
    report.allowance = Some(allowance.usage());
    report.allowance_stop = Some(stop);
    report
}

fn stop_termination(stop: &AllowanceStop) -> Termination {
    if stop.reason == "deadline" {
        Termination::Deadline
    } else {
        Termination::WorkLimit
    }
}

fn opposite(edge: FacilityPortEdge) -> usize {
    (edge_index(edge) + 2) % 4
}

fn json_size_with_allowance<T: Serialize>(
    value: &T,
    allowance: &mut RequestAllowance,
) -> Result<usize, RequestFailure> {
    struct Count<'a> {
        bytes: usize,
        pending: usize,
        allowance: &'a mut RequestAllowance,
        stopped: Option<AllowanceStop>,
    }
    impl Write for Count<'_> {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes = self.bytes.saturating_add(bytes.len());
            self.pending = self.pending.saturating_add(bytes.len());
            if self.pending >= 4096 {
                if let Err(stop) = self
                    .allowance
                    .record_completed(WorkKind::PublishedBytes, self.pending)
                {
                    self.stopped = Some(stop);
                    return Err(std::io::Error::other(
                        "candidate encoding allowance stopped",
                    ));
                }
                self.pending = 0;
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count {
        bytes: 0,
        pending: 0,
        allowance,
        stopped: None,
    };
    if let Err(failure) = serde_json::to_writer(&mut count, value) {
        if let Some(stop) = count.stopped {
            return Err(RequestFailure::Stopped(stop));
        }
        return Err(RequestFailure::Invalid(error(
            "route-candidate-publication",
            "route-candidate",
            failure.to_string(),
        )));
    }
    count
        .allowance
        .record_completed(WorkKind::PublishedBytes, count.pending)?;
    Ok(count.bytes)
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

fn add_overlap_penalty(
    penalties: &mut IndexMap<Point, u32>,
    cells: &[Point],
    amount: u32,
    allowance: &mut RequestAllowance,
) -> Result<(), AllowanceStop> {
    for point in interior(cells) {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        let entry = penalties.entry(point).or_default();
        *entry = entry.saturating_add(amount).min(1_000_000);
    }
    Ok(())
}

fn directed_cost(
    cells: &[Point],
    costs: &IndexMap<(Point, usize), u32>,
    allowance: &mut RequestAllowance,
) -> Result<u64, AllowanceStop> {
    let mut total = 0u64;
    for edge in cells.windows(2) {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        let direction = match (edge[1].x - edge[0].x, edge[1].y - edge[0].y) {
            (1, 0) => 0,
            (0, 1) => 1,
            (-1, 0) => 2,
            (0, -1) => 3,
            _ => unreachable!("routing kernel returns cardinal unit steps"),
        };
        total = total.saturating_add(u64::from(
            costs.get(&(edge[1], direction)).copied().unwrap_or(0),
        ));
    }
    Ok(total)
}

fn maximum_overlap(
    cells: &[Point],
    candidates: &[Candidate],
    allowance: &mut RequestAllowance,
) -> Result<usize, AllowanceStop> {
    let mut current = HashSet::new();
    for point in interior(cells) {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        current.insert(point);
    }
    let mut maximum = 0;
    for candidate in candidates {
        let mut overlap = 0;
        for point in interior(&candidate.cells) {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            overlap += usize::from(current.contains(&point));
        }
        maximum = maximum.max(overlap);
    }
    Ok(maximum)
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

fn network_shared_cells_with_replay(
    routes: &[Candidate],
    mut visit: impl FnMut(usize) -> Result<(), Diagnostic>,
) -> Result<usize, Diagnostic> {
    let mut uses = IndexMap::<Point, usize>::new();
    for route in routes {
        let mut unique = HashSet::new();
        for &point in &route.cells {
            visit(1)?;
            unique.insert(point);
        }
        for point in unique {
            visit(1)?;
            *uses.entry(point).or_default() += 1;
        }
    }
    let mut shared = 0;
    for count in uses.values() {
        visit(1)?;
        shared += usize::from(*count > 1);
    }
    Ok(shared)
}

pub fn network_shared_cells_with_allowance(
    routes: &[Candidate],
    allowance: &mut RequestAllowance,
) -> Result<usize, AllowanceStop> {
    let mut uses = IndexMap::<Point, usize>::new();
    for route in routes {
        let mut unique = HashSet::new();
        for &point in &route.cells {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            unique.insert(point);
        }
        for point in unique {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            *uses.entry(point).or_default() += 1;
        }
    }
    let mut shared = 0;
    for count in uses.values() {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        shared += usize::from(*count > 1);
    }
    Ok(shared)
}

fn diversity(
    candidates: &[Candidate],
    allowance: &mut RequestAllowance,
) -> Result<(Option<f64>, Option<f64>), AllowanceStop> {
    let mut minimum = f64::INFINITY;
    let mut total = 0.0;
    let mut pairs = 0usize;
    for left_index in 0..candidates.len() {
        let mut left = HashSet::new();
        for point in interior(&candidates[left_index].cells) {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            left.insert(point);
        }
        for right in &candidates[left_index + 1..] {
            let mut right_set = HashSet::new();
            for point in interior(&right.cells) {
                allowance.charge(WorkKind::RoutingScans, 1)?;
                right_set.insert(point);
            }
            allowance.charge(
                WorkKind::RoutingScans,
                left.len().saturating_add(right_set.len()),
            )?;
            let intersection = left.intersection(&right_set).count();
            let union = left.union(&right_set).count();
            let value = if union == 0 {
                0.0
            } else {
                1.0 - intersection as f64 / union as f64
            };
            minimum = minimum.min(value);
            total += value;
            pairs += 1;
        }
    }
    if pairs == 0 {
        return Ok((None, None));
    }
    Ok((Some(minimum), Some(total / pairs as f64)))
}
