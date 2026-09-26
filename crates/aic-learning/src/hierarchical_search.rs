//! Stable-state macro search over placement commands and complete route witnesses.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    sync::Arc,
    time::Instant,
};

use aic_layout::{
    geometry::{Bounds, Point},
    physical::{BridgeAxis, Problem},
    validation::Catalogs,
};
use aic_pilot_native::constructive::{
    Command, Limits, Session,
    learning::{CommandKind, Config as DomainConfig},
    routing_candidates::{
        AllowanceStop, AllowanceUsage, Candidate, Config as CandidateConfig, NetworkCandidate,
        Provenance, RepairCandidate, RepairRequest, Report as RouteReport, Request,
        RequestAllowance, RequestFailure, RouteReadiness, Termination as RouteTermination,
        WorkKind, network_shared_cells_with_allowance,
    },
};
use aic_torch::tch::{self, Device, Tensor};
use anyhow::{Context, Result, ensure};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    constructive_problems::ProblemGenerator,
    input::Tensors,
    outer_model::{
        ACTION_FEATURES, AREA_HEAD, Batch as OuterBatch, COMPLETION_HEAD,
        CompiledModel as CompiledOuterModel, Model as OuterModel, ParameterOrder as OuterOrder,
        TRANSPORT_HEAD, TURNS_HEAD, normalized_layout_quality,
    },
    outer_training::Config as OuterTrainingConfig,
    route_guidance::{
        CompiledModel as CompiledGuidanceModel, Model as GuidanceModel, ParameterOrder,
    },
    route_guidance_training::Config as GuidanceTrainingConfig,
};

const CONFIG_SCHEMA: &str = "aic-hierarchical-search-config-v2";
pub(crate) const PANEL_SCHEMA: &str = "aic-hierarchical-search-panel-v1";
pub const REPORT_SCHEMA: &str = "aic-hierarchical-search-report-v9";
const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
const ROUTE_ATTEMPT_SAMPLE_LIMIT: usize = 32;

fn default_anchor_prior_mass() -> f64 {
    0.25
}

fn default_compiled_inference() -> bool {
    true
}

fn default_run_beam() -> bool {
    true
}

fn default_gumbel_assignment() -> GumbelAssignment {
    GumbelAssignment::Rank
}

fn is_rank_gumbel_assignment(value: &GumbelAssignment) -> bool {
    *value == GumbelAssignment::Rank
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GumbelAssignment {
    Rank,
    ActionIdentity,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "algorithm", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RootSelection {
    Puct,
    GumbelSequentialHalving {
        candidates: usize,
        anchor_candidates: usize,
        anchor_policy_mass: f64,
        seed: u64,
        value_scale: f64,
        max_visit_init: f64,
        #[serde(
            default = "default_gumbel_assignment",
            skip_serializing_if = "is_rank_gumbel_assignment"
        )]
        gumbel_assignment: GumbelAssignment,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub bounds: Bounds,
    pub extension_lengths: Vec<i64>,
    pub candidate: CandidateConfig,
    /// Zero disables repair. A positive value authorizes removal of at most
    /// this many currently committed routes in one atomic transaction.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub repair_max_removed: usize,
    pub cpu_threads: usize,
    pub placement_page: usize,
    pub beam_width: usize,
    #[serde(default = "default_run_beam")]
    pub run_beam: bool,
    pub simulations_per_move: usize,
    pub horizon: usize,
    pub max_moves: usize,
    pub exploration: f64,
    pub root_selection: RootSelection,
    #[serde(default = "default_anchor_prior_mass")]
    pub anchor_prior_mass: f64,
    #[serde(default = "default_compiled_inference")]
    pub compiled_inference: bool,
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Panel {
    pub(crate) schema: String,
    pub(crate) partition: String,
    pub(crate) limits: Limits,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    preparation: Option<PanelPreparation>,
    pub(crate) cases: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PanelPreparation {
    sampling_sha256: String,
    seed: u64,
    namespace: String,
    source_partition: String,
    min_facilities: usize,
    max_facilities: usize,
    max_terminals: usize,
    max_connections: usize,
    sampling_stats: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Case {
    pub(crate) id: String,
    pub(crate) problem: Problem,
    pub(crate) prefix: Vec<Command>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub(crate) enum MacroAction {
    Command(Command),
    Route(Candidate),
    Network(NetworkCandidate),
    Repair {
        request: RepairRequest,
        candidate: RepairCandidate,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ActionSummary {
    kind: String,
    entity: String,
    state_revision: u64,
    steps: Option<i64>,
    provenance: Option<Provenance>,
    #[serde(default)]
    bridge_uses: Vec<BridgeUseSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    removed_connections: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct BridgeUseSummary {
    position: Point,
    component: String,
    rotation: i64,
    direction: usize,
}

fn bridge_uses<'a>(routes: impl Iterator<Item = &'a Candidate>) -> Vec<BridgeUseSummary> {
    routes
        .flat_map(|route| route.crossings.iter())
        .map(|crossing| BridgeUseSummary {
            position: crossing.position,
            component: crossing.component.clone(),
            rotation: crossing.rotation,
            direction: crossing.direction,
        })
        .collect()
}

fn bridge_action_features<'a>(
    after: &Session,
    routes: impl Iterator<Item = &'a Candidate>,
) -> [f32; 6] {
    let mut values = [0.0; 6];
    let mut count = 0usize;
    for crossing in routes.flat_map(|route| route.crossings.iter()) {
        let Some(bridge) = after.bridge_at(crossing.transport, crossing.position) else {
            continue;
        };
        let (code, loads) = after.bridge_component_features(bridge);
        let axis = crossing.direction % 2;
        count += 1;
        values[1] += code;
        values[2] += loads[axis];
        values[3] += loads[1 - axis];
        values[4] += (axis == 0) as u8 as f32;
        values[5] += crossing.rotation as f32 / 270.0;
    }
    if count > 0 {
        values[0] = count as f32;
        for value in &mut values[1..] {
            *value /= count as f32;
        }
    }
    values
}

/// Compact next-generation target emitted once per committed MCTS root.
/// The full candidate witnesses make the record independently replayable from
/// the frozen panel prefix and previously selected root actions.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RootActionRecord {
    pub action: MacroAction,
    pub prior: f64,
    pub anchor_prior: f64,
    pub visits: u32,
    pub q: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_target: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RootRecord {
    pub state_sha256: String,
    pub state_revision: u64,
    pub actions: Vec<RootActionRecord>,
    pub selected_action: usize,
    pub later_completed_connections: usize,
    pub later_total_connections: usize,
    pub later_footprint_area: u64,
    pub later_transport_cells: u64,
    pub later_turns: u64,
    pub later_fully_validated: bool,
    pub later_termination: SearchTermination,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SearchTermination {
    FullyValidated,
    NoMacroActions,
    MoveBudgetExhausted,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Counters {
    expansions: usize,
    pub(crate) generated_macro_samples: usize,
    placement_samples: usize,
    route_samples: usize,
    route_searches: usize,
    route_expanded: usize,
    route_preflight_expanded: usize,
    native_rejections: usize,
    guidance_batches: usize,
    guidance_inputs: usize,
    network_tasks: usize,
    #[serde(default)]
    network_unknown: usize,
    #[serde(default)]
    pending_route_networks: usize,
    #[serde(default)]
    pending_route_connections: usize,
    network_samples: usize,
    shared_network_samples: usize,
    route_candidate_limit: usize,
    route_search_limit: usize,
    route_work_budget: usize,
    #[serde(default)]
    route_resource_limit: usize,
    route_frontier_exhausted: usize,
    route_no_compatible_endpoints: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    route_no_compatible_connection: Option<String>,
    #[serde(default)]
    route_work_limit: usize,
    #[serde(default)]
    route_deadline: usize,
    #[serde(default, skip_deserializing, skip_serializing_if = "Vec::is_empty")]
    route_attempt_samples: Vec<RouteAttemptRecord>,
    route_attempt_samples_dropped: usize,
    #[serde(default, skip_deserializing, skip_serializing_if = "Vec::is_empty")]
    request_stops: Vec<RequestStopRecord>,
    #[serde(default)]
    request_stops_total: usize,
    #[serde(skip)]
    last_request_stop: Option<(String, AllowanceStop)>,
}

#[derive(Clone, Debug, Serialize)]
struct RequestStopRecord {
    state_revision: u64,
    connection: String,
    stop: AllowanceStop,
    usage: AllowanceUsage,
}

#[derive(Clone, Debug, Serialize)]
struct RouteAttemptRecord {
    state_revision: u64,
    connection: String,
    termination: RouteTermination,
    fixed_request_proven_infeasible: bool,
    candidates: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SearchResult {
    algorithm: String,
    status: String,
    pub(crate) fully_validated: bool,
    termination: SearchTermination,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    no_macro_diagnostic: Option<MacroDiagnostic>,
    completed_connections: usize,
    total_connections: usize,
    pub(crate) footprint_area: Option<u64>,
    pub(crate) transport_cells: Option<u64>,
    pub(crate) turns: Option<u64>,
    moves: usize,
    simulations: usize,
    pub(crate) elapsed_seconds: f64,
    samples_per_minute: f64,
    pub(crate) counters: Counters,
    trace: Vec<ActionSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) root_records: Vec<RootRecord>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct CaseReport {
    pub(crate) id: String,
    prefix_commands: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) beam: Option<SearchResult>,
    pub(crate) mcts: SearchResult,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Report {
    pub(crate) schema: String,
    status: String,
    pub(crate) partition: String,
    device: String,
    pub(crate) search_config_sha256: String,
    pub(crate) panel_sha256: String,
    pub(crate) catalog_sha256: String,
    pub(crate) lower_config_sha256: String,
    pub(crate) lower_parameter_order_sha256: String,
    pub(crate) lower_checkpoint_sha256: String,
    pub(crate) outer_config_sha256: String,
    pub(crate) outer_parameter_order_sha256: String,
    pub(crate) outer_checkpoint_sha256: String,
    pub(crate) root_records_retained: bool,
    pub(crate) cases: Vec<CaseReport>,
}

pub(crate) struct Published<T> {
    pub(crate) value: T,
    pub(crate) sha256: String,
    pub(crate) encoded_bytes: u64,
}

enum GuidanceRunner<'a> {
    Eager(&'a GuidanceModel),
    Compiled(CompiledGuidanceModel<'a>),
}

impl GuidanceRunner<'_> {
    fn predict_native(
        &self,
        inputs: &[&aic_pilot_native::constructive::routing_candidates::GuidanceInput],
        model_version: &str,
        device: Device,
    ) -> Result<Vec<aic_pilot_native::constructive::routing_candidates::Guidance>> {
        match self {
            Self::Eager(model) => model.predict_native(inputs, model_version, device),
            Self::Compiled(model) => model.predict_native(inputs, model_version, device),
        }
    }
}

enum OuterRunner<'a> {
    Eager(&'a OuterModel),
    Compiled(CompiledOuterModel<'a>),
}

impl OuterRunner<'_> {
    fn forward(&self, batch: &OuterBatch) -> Result<crate::outer_model::Prediction> {
        match self {
            Self::Eager(model) => model.forward(batch),
            Self::Compiled(model) => model.forward(batch),
        }
    }
}

pub(crate) struct Factory<'a> {
    guidance: GuidanceRunner<'a>,
    guidance_heads: usize,
    guidance_version: &'a str,
    device: Device,
    domain: DomainConfig,
    candidate: &'a CandidateConfig,
    repair_max_removed: usize,
    placement_page: usize,
    anchor_prior_mass: f64,
    pool: rayon::ThreadPool,
    outer: Option<OuterRunner<'a>>,
}

#[derive(Clone, Copy)]
struct PlacementPages {
    offsets: [usize; 2],
    exhausted: [bool; 2],
    loaded_pages: usize,
}

struct ActionBatch {
    actions: Vec<MacroAction>,
    placement_pages: PlacementPages,
    no_macro_diagnostic: Option<MacroDiagnostic>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct MacroDiagnostic {
    code: String,
    severity: String,
    stage: String,
    message: String,
    entity: String,
}

struct ActionScores {
    priors: Vec<f64>,
    anchor_priors: Vec<f64>,
    model_value: f64,
}

impl Factory<'_> {
    pub(crate) fn apply_action(&self, action: &MacroAction, session: &Session) -> Result<Session> {
        action.apply(session, self.candidate, self.repair_max_removed)
    }

    pub(crate) fn new<'a>(
        guidance: &'a GuidanceModel,
        guidance_version: &'a str,
        device: Device,
        domain: DomainConfig,
        candidate: &'a CandidateConfig,
        repair_max_removed: usize,
        placement_page: usize,
        cpu_threads: usize,
        anchor_prior_mass: f64,
        compiled_inference: bool,
        outer: Option<&'a OuterModel>,
    ) -> Result<Factory<'a>> {
        Ok(Factory {
            guidance_heads: guidance.config.heads,
            guidance: if compiled_inference {
                GuidanceRunner::Compiled(CompiledGuidanceModel::new(guidance))
            } else {
                GuidanceRunner::Eager(guidance)
            },
            guidance_version,
            device,
            domain,
            candidate,
            repair_max_removed,
            placement_page,
            anchor_prior_mass,
            pool: rayon::ThreadPoolBuilder::new()
                .num_threads(cpu_threads)
                .thread_name(|index| format!("hierarchical-route-{index}"))
                .build()?,
            outer: outer.map(|outer| {
                if compiled_inference {
                    OuterRunner::Compiled(CompiledOuterModel::new(outer))
                } else {
                    OuterRunner::Eager(outer)
                }
            }),
        })
    }

    fn action_batch(
        &self,
        session: &Session,
        counters: &mut Counters,
        exhaust_placements: bool,
    ) -> Result<ActionBatch> {
        ensure!(
            !session.has_active_route(),
            "aic-learning.hierarchical-state: macro state contains an active partial route"
        );
        let before = (
            counters.request_stops_total,
            counters.route_deadline,
            counters.route_work_limit,
            counters.route_frontier_exhausted,
            counters.route_no_compatible_endpoints,
            counters.network_unknown,
            counters.pending_route_networks,
            counters.route_resource_limit,
            counters.route_search_limit,
            counters.route_work_budget,
            counters.route_candidate_limit,
        );
        let mut actions = Vec::new();
        let mut placement_pages = PlacementPages {
            offsets: [0; 2],
            exhausted: [true; 2],
            loaded_pages: 1,
        };
        if session.has_unplaced_entities() {
            let placement_observation = session.learning_observation(&self.domain)?;
            for (index, kind) in [CommandKind::Place, CommandKind::PlaceTerminal]
                .into_iter()
                .enumerate()
            {
                loop {
                    let page = placement_observation.domain.command_page(
                        kind,
                        placement_pages.offsets[index],
                        self.placement_page,
                    );
                    placement_pages.offsets[index] = page.next_offset;
                    placement_pages.exhausted[index] = page.exhausted;
                    actions.extend(
                        page.commands
                            .into_iter()
                            .map(|factored| MacroAction::Command(factored.command.command)),
                    );
                    if !exhaust_placements || page.exhausted {
                        break;
                    }
                    placement_pages.loaded_pages += 1;
                }
            }
        }

        let mut unfinished = BTreeMap::new();
        for (index, connection) in session.problem().connections.iter().enumerate() {
            if session.route_is_complete(index) {
                continue;
            }
            unfinished
                .entry((connection.commodity.clone(), connection.transport))
                .or_insert_with(Vec::new)
                .push(Request {
                    state_revision: session.revision(),
                    connection: connection.id.clone(),
                });
        }
        // A logical network becomes a macro only when every unfinished member
        // has placed endpoints. Do not silently route one temporarily-ready
        // edge as a singleton and thereby consume the future network's ports.
        let mut ready_singletons = Vec::new();
        let mut ready_networks = Vec::new();
        for requests in unfinished.into_values() {
            let mut pending = 0usize;
            for request in &requests {
                match session
                    .route_request_readiness(request)
                    .map_err(anyhow::Error::new)?
                {
                    RouteReadiness::Ready => {}
                    RouteReadiness::PendingPlacement { .. } => pending += 1,
                }
            }
            if pending > 0 {
                counters.pending_route_networks += 1;
                counters.pending_route_connections += pending;
                continue;
            }
            if requests.len() == 1 {
                ready_singletons.push(requests[0].clone());
            } else {
                ready_networks.push((
                    requests,
                    RequestAllowance::new(self.candidate.request_allowance.clone()),
                ));
            }
        }
        let max_batch = self.candidate.request_allowance.max_inference_batch;
        for requests in ready_singletons.chunks(max_batch) {
            // Materialize only one bounded model batch at a time. Retaining
            // every ready singleton's raster would bypass the batch cap.
            let mut prepared = Vec::with_capacity(requests.len());
            for request in requests {
                let mut allowance = RequestAllowance::new(self.candidate.request_allowance.clone());
                match session.route_guidance_input_with_allowance(
                    request,
                    self.candidate,
                    &self.domain,
                    &mut allowance,
                ) {
                    Ok(input) => prepared.push((request.clone(), input, allowance)),
                    Err(RequestFailure::Invalid(diagnostic)) => {
                        return Err(anyhow::Error::new(diagnostic));
                    }
                    Err(RequestFailure::Stopped(stop)) => {
                        record_request_stop(counters, request, &allowance, stop)
                    }
                    Err(RequestFailure::PendingPlacement { .. }) => {
                        counters.pending_route_networks += 1;
                        counters.pending_route_connections += 1;
                    }
                    Err(RequestFailure::NoCompatibleEndpoints { connection }) => {
                        counters.route_no_compatible_endpoints += 1;
                        counters.route_no_compatible_connection = Some(connection);
                    }
                }
            }
            let chunk = prepared.as_mut_slice();
            let mut valid = Vec::new();
            let mut inference_reservations = vec![0usize; chunk.len()];
            for (index, (request, input, allowance)) in chunk.iter_mut().enumerate() {
                let elements = input
                    .grid
                    .len()
                    .saturating_add(
                        input.graph.entity_features.len()
                            * aic_pilot_native::constructive::learning::ENTITY_FEATURES,
                    )
                    .saturating_add(
                        input.graph.relation_features.len()
                            * aic_pilot_native::constructive::learning::RELATION_FEATURES,
                    )
                    .saturating_add(
                        input.graph.spatial_features.len()
                            * aic_pilot_native::constructive::learning::SPATIAL_FEATURES,
                    );
                let bytes = inference_reservation_bytes(input, self.guidance_heads);
                match allowance
                    .preflight(WorkKind::InferenceElements, elements)
                    .and_then(|_| allowance.reserve_peak(bytes))
                {
                    Ok(()) => {
                        inference_reservations[index] = bytes;
                        allowance
                            .charge(WorkKind::InferenceElements, elements)
                            .expect("preflighted inference elements");
                        valid.push(index);
                    }
                    Err(stop) => record_request_stop(counters, request, allowance, stop),
                }
            }
            if valid.is_empty() {
                continue;
            }
            let inputs = valid
                .iter()
                .map(|&index| &chunk[index].1)
                .collect::<Vec<_>>();
            let guidance =
                self.guidance
                    .predict_native(&inputs, self.guidance_version, self.device)?;
            counters.guidance_batches += 1;
            counters.guidance_inputs += inputs.len();
            drop(inputs);
            let mut guidance_by_index = vec![None; chunk.len()];
            let mut guidance_reservations = vec![0usize; chunk.len()];
            for (index, guide) in valid.into_iter().zip(guidance) {
                let (request, _, allowance) = &mut chunk[index];
                allowance.release_peak(inference_reservations[index]);
                let bytes = guide.heads.iter().fold(0usize, |total, head| {
                    total.saturating_add(head.directed_costs.len().saturating_mul(32))
                });
                match allowance.reserve_peak(bytes) {
                    Ok(()) => {
                        guidance_reservations[index] = bytes;
                        guidance_by_index[index] = Some(guide);
                    }
                    Err(stop) => record_request_stop(counters, request, allowance, stop),
                }
            }
            let reports = self.pool.install(|| {
                chunk
                    .par_iter_mut()
                    .zip(guidance_by_index.par_iter())
                    .map(|((request, _, allowance), guidance)| {
                        guidance.as_ref().map(|guidance| {
                            if let Err(stop) = allowance.check_deadline() {
                                return Err(RequestFailure::Stopped(stop));
                            }
                            session
                                .route_candidates_with_guidance_allowance(
                                    request,
                                    self.candidate,
                                    guidance,
                                    allowance,
                                )
                                .map_err(RequestFailure::Invalid)
                        })
                    })
                    .collect::<Vec<_>>()
            });
            for ((request, _, allowance), report) in chunk.iter_mut().zip(reports) {
                match report {
                    Some(Ok(report)) => {
                        record_route_report(counters, &report);
                        actions.extend(report.candidates.into_iter().map(MacroAction::Route));
                    }
                    Some(Err(RequestFailure::Invalid(diagnostic))) => {
                        return Err(anyhow::Error::new(diagnostic));
                    }
                    Some(Err(RequestFailure::Stopped(stop))) => {
                        record_request_stop(counters, request, allowance, stop)
                    }
                    Some(Err(RequestFailure::PendingPlacement { .. })) => {
                        counters.pending_route_networks += 1;
                        counters.pending_route_connections += 1;
                    }
                    Some(Err(RequestFailure::NoCompatibleEndpoints { .. })) => {
                        unreachable!("route search does not prepare endpoints")
                    }
                    None => {}
                }
            }
            for (index, (_, _, allowance)) in chunk.iter_mut().enumerate() {
                allowance.release_peak(guidance_reservations[index]);
            }
        }
        for (requests, allowance) in &mut ready_networks {
            let connections = requests
                .iter()
                .map(|request| request.connection.clone())
                .collect::<Vec<_>>();
            actions.extend(
                self.network_actions(session, &connections, counters, allowance)?
                    .into_iter()
                    .map(MacroAction::Network),
            );
        }
        counters.expansions += 1;
        counters.generated_macro_samples += actions.len();
        counters.placement_samples += actions
            .iter()
            .filter(|action| matches!(action, MacroAction::Command(_)))
            .count();
        counters.route_samples += actions
            .iter()
            .filter(|action| matches!(action, MacroAction::Route(_)))
            .count();
        counters.network_samples += actions
            .iter()
            .filter(|action| matches!(action, MacroAction::Network(_)))
            .count();
        counters.shared_network_samples += actions
            .iter()
            .filter(|action| {
                matches!(action, MacroAction::Network(candidate) if candidate.shared_cells > 0)
            })
            .count();
        Ok(ActionBatch {
            no_macro_diagnostic: actions
                .is_empty()
                .then(|| no_macro_reason(counters, before, session.revision())),
            actions,
            placement_pages,
        })
    }

    pub(crate) fn actions(
        &self,
        session: &Session,
        counters: &mut Counters,
    ) -> Result<Vec<MacroAction>> {
        Ok(self.action_batch(session, counters, false)?.actions)
    }

    fn next_placement_page(
        &self,
        session: &Session,
        pages: &mut PlacementPages,
        counters: &mut Counters,
    ) -> Result<Vec<MacroAction>> {
        let observation = session.learning_observation(&self.domain)?;
        let mut actions = Vec::new();
        for (index, kind) in [CommandKind::Place, CommandKind::PlaceTerminal]
            .into_iter()
            .enumerate()
        {
            if pages.exhausted[index] {
                continue;
            }
            let page =
                observation
                    .domain
                    .command_page(kind, pages.offsets[index], self.placement_page);
            pages.offsets[index] = page.next_offset;
            pages.exhausted[index] = page.exhausted;
            actions.extend(
                page.commands
                    .into_iter()
                    .map(|factored| MacroAction::Command(factored.command.command)),
            );
        }
        pages.loaded_pages += 1;
        counters.generated_macro_samples += actions.len();
        counters.placement_samples += actions.len();
        Ok(actions)
    }

    fn network_actions(
        &self,
        root: &Session,
        connections: &[String],
        counters: &mut Counters,
        allowance: &mut RequestAllowance,
    ) -> Result<Vec<NetworkCandidate>> {
        #[derive(Clone)]
        struct Partial {
            variant: usize,
            order: Vec<String>,
            session: Session,
            routes: Vec<Candidate>,
            retained_bytes: usize,
            candidate_bytes: usize,
            replay_copy_bytes: usize,
        }

        counters.network_tasks += 1;
        let request = Request {
            state_revision: root.revision(),
            connection: connections
                .first()
                .cloned()
                .unwrap_or_else(|| "network".into()),
        };
        let first = match session_network_identity(root, connections, allowance) {
            Ok(first) => first,
            Err(RequestFailure::Stopped(stop)) => {
                record_request_stop(counters, &request, allowance, stop);
                counters.network_unknown += 1;
                return Ok(Vec::new());
            }
            Err(RequestFailure::Invalid(diagnostic)) => return Err(anyhow::Error::new(diagnostic)),
            Err(_) => unreachable!("network identity does not prepare endpoints"),
        };
        let mut semantic = BTreeSet::new();
        let mut semantic_reservation = 0usize;
        let mut candidates = Vec::new();
        let mut output_reservation = 0usize;
        let count = self.candidate.max_candidates;
        let mut page_size = self
            .candidate
            .request_allowance
            .max_inference_batch
            .min(count);
        let clone_bytes = root.clone_reservation_bytes();
        let mut page_start = 0;
        let mut charged_variant_end = 0usize;
        while page_start < count {
            // A remaining variant allowance may be smaller than a full page.
            // Build that smaller page so a page size never hides work that is
            // still within the caller's explicit variant limit.
            let mut page_count = page_size.min(count - page_start).min(
                charged_variant_end
                    .saturating_add(allowance.remaining(WorkKind::Variants))
                    .saturating_sub(page_start),
            );
            if page_count == 0 {
                let stop = allowance.charge(WorkKind::Variants, 1).unwrap_err();
                record_request_stop(counters, &request, allowance, stop);
                allowance.release_peak(semantic_reservation);
                allowance.release_peak(output_reservation);
                counters.network_unknown += 1;
                return Ok(candidates);
            }
            let page_bytes =
                match reserve_network_clone_page(allowance, clone_bytes, &mut page_count) {
                    Ok(bytes) => bytes,
                    Err(stop) => {
                        record_request_stop(counters, &request, allowance, stop);
                        allowance.release_peak(semantic_reservation);
                        allowance.release_peak(output_reservation);
                        counters.network_unknown += 1;
                        return Ok(candidates);
                    }
                };
            let mut reserved = page_bytes;
            let newly_touched = page_start
                .saturating_add(page_count)
                .saturating_sub(charged_variant_end);
            if let Err(stop) = allowance
                .charge(WorkKind::Variants, newly_touched)
                .and_then(|_| allowance.charge(WorkKind::CopiedBytes, page_bytes))
            {
                allowance.release_peak(reserved);
                record_request_stop(counters, &request, allowance, stop);
                allowance.release_peak(semantic_reservation);
                allowance.release_peak(output_reservation);
                counters.network_unknown += 1;
                return Ok(candidates);
            }
            charged_variant_end = charged_variant_end.max(page_start + page_count);
            let stop_count_before_page = counters.request_stops.len();
            let stop_total_before_page = counters.request_stops_total;
            let last_stop_before_page = counters.last_request_stop.clone();
            let deadline_before_page = counters.route_deadline;
            let work_limit_before_page = counters.route_work_limit;
            let output_before_page = output_reservation;
            let mut page_peak_stop = false;
            macro_rules! record_page_stop {
                ($request:expr, $stop:expr) => {{
                    let stop = $stop;
                    page_peak_stop = stop.reason == "peak-memory-limit";
                    record_request_stop(counters, $request, allowance, stop);
                }};
            }
            let mut partials = (page_start..page_start + page_count)
                .map(|variant| {
                    let mut order = connections.to_vec();
                    let rotation = variant % order.len();
                    order.rotate_left(rotation);
                    if (variant / order.len()) % 2 == 1 {
                        order.reverse();
                    }
                    Partial {
                        variant,
                        order,
                        session: root.clone(),
                        routes: Vec::with_capacity(connections.len()),
                        retained_bytes: 0,
                        candidate_bytes: 0,
                        replay_copy_bytes: 0,
                    }
                })
                .collect::<Vec<_>>();
            let mut page_stopped = false;
            let mut root_no_compatible = false;
            for depth in 0..connections.len() {
                if partials.is_empty() {
                    break;
                }
                let mut prepared = Vec::with_capacity(partials.len());
                for partial in &partials {
                    let member = Request {
                        state_revision: partial.session.revision(),
                        connection: partial.order[depth].clone(),
                    };
                    match partial.session.route_guidance_input_with_allowance(
                        &member,
                        self.candidate,
                        &self.domain,
                        allowance,
                    ) {
                        Ok(input) => {
                            reserved = reserved.saturating_add(input.reserved_bytes);
                            prepared.push(Some((member, input)));
                        }
                        Err(RequestFailure::Invalid(diagnostic)) => {
                            return Err(anyhow::Error::new(diagnostic));
                        }
                        Err(RequestFailure::Stopped(stop)) => {
                            record_page_stop!(&member, stop);
                            page_stopped = true;
                            break;
                        }
                        Err(RequestFailure::PendingPlacement { .. }) => {
                            counters.pending_route_networks += 1;
                            counters.pending_route_connections += 1;
                            page_stopped = true;
                            break;
                        }
                        Err(RequestFailure::NoCompatibleEndpoints { connection }) => {
                            let root_request = Request {
                                state_revision: root.revision(),
                                connection: connection.clone(),
                            };
                            match root.route_guidance_input_with_allowance(
                                &root_request,
                                self.candidate,
                                &self.domain,
                                allowance,
                            ) {
                                Err(RequestFailure::NoCompatibleEndpoints { connection }) => {
                                    counters.route_no_compatible_endpoints += 1;
                                    counters.route_no_compatible_connection = Some(connection);
                                    root_no_compatible = true;
                                    page_stopped = true;
                                    break;
                                }
                                Ok(input) => {
                                    allowance.release_peak(input.reserved_bytes);
                                    prepared.push(None);
                                }
                                Err(RequestFailure::Stopped(stop)) => {
                                    record_page_stop!(&root_request, stop);
                                    page_stopped = true;
                                    break;
                                }
                                Err(RequestFailure::Invalid(diagnostic)) => {
                                    return Err(anyhow::Error::new(diagnostic));
                                }
                                Err(RequestFailure::PendingPlacement { .. }) => {
                                    counters.pending_route_networks += 1;
                                    counters.pending_route_connections += 1;
                                    page_stopped = true;
                                    break;
                                }
                            }
                        }
                    }
                }
                if page_stopped {
                    break;
                }
                for (member, input) in prepared.iter().flatten() {
                    let elements = input
                        .grid
                        .len()
                        .saturating_add(
                            input.graph.entity_features.len()
                                * aic_pilot_native::constructive::learning::ENTITY_FEATURES,
                        )
                        .saturating_add(
                            input.graph.relation_features.len()
                                * aic_pilot_native::constructive::learning::RELATION_FEATURES,
                        )
                        .saturating_add(
                            input.graph.spatial_features.len()
                                * aic_pilot_native::constructive::learning::SPATIAL_FEATURES,
                        );
                    if let Err(stop) = allowance.charge(WorkKind::InferenceElements, elements) {
                        record_page_stop!(member, stop);
                        page_stopped = true;
                        break;
                    }
                }
                if page_stopped {
                    break;
                }
                let inputs = prepared
                    .iter()
                    .flatten()
                    .map(|(_, input)| input)
                    .collect::<Vec<_>>();
                if inputs.is_empty() {
                    for partial in partials.drain(..) {
                        let bytes = clone_bytes.saturating_add(partial.retained_bytes);
                        allowance.release_peak(bytes);
                        reserved -= bytes;
                    }
                    partials.clear();
                    break;
                }
                let inference_bytes = inputs.iter().fold(0usize, |total, input| {
                    total.saturating_add(inference_reservation_bytes(input, self.guidance_heads))
                });
                if let Err(stop) = allowance.reserve_peak(inference_bytes) {
                    record_page_stop!(&request, stop);
                    page_stopped = true;
                    break;
                }
                reserved = reserved.saturating_add(inference_bytes);
                let guidance =
                    self.guidance
                        .predict_native(&inputs, self.guidance_version, self.device)?;
                allowance.release_peak(inference_bytes);
                reserved -= inference_bytes;
                let guidance_bytes = guidance.iter().fold(0usize, |total, guide| {
                    total.saturating_add(guide.heads.iter().fold(0usize, |head_total, head| {
                        head_total.saturating_add(head.directed_costs.len().saturating_mul(32))
                    }))
                });
                if let Err(stop) = allowance.reserve_peak(guidance_bytes) {
                    record_page_stop!(&request, stop);
                    page_stopped = true;
                    break;
                }
                reserved = reserved.saturating_add(guidance_bytes);
                counters.guidance_batches += 1;
                counters.guidance_inputs += inputs.len();
                if let Err(stop) = allowance.check_deadline() {
                    record_page_stop!(&request, stop);
                    page_stopped = true;
                    break;
                }
                let mut next = Vec::with_capacity(partials.len());
                let mut guides = guidance.iter();
                for (mut partial, prepared) in
                    std::mem::take(&mut partials).into_iter().zip(&prepared)
                {
                    let Some((member, _)) = prepared else {
                        let bytes = clone_bytes.saturating_add(partial.retained_bytes);
                        allowance.release_peak(bytes);
                        reserved -= bytes;
                        continue;
                    };
                    let guide = guides.next().expect("one guide per prepared input");
                    let report = partial
                        .session
                        .route_candidates_for_network_with_allowance(
                            member,
                            self.candidate,
                            guide,
                            allowance,
                        )
                        .map_err(anyhow::Error::new)?;
                    let report_bytes = route_report_reservation_bytes(&report);
                    if let Err(stop) = allowance.reserve_peak(report_bytes) {
                        record_page_stop!(member, stop);
                        page_stopped = true;
                        break;
                    }
                    reserved = reserved.saturating_add(report_bytes);
                    record_route_report(counters, &report);
                    if let Some(stop) = &report.allowance_stop {
                        page_peak_stop = stop.reason == "peak-memory-limit";
                        page_stopped = true;
                        break;
                    }
                    if report.candidates.is_empty() {
                        drop(report);
                        allowance.release_peak(report_bytes);
                        reserved -= report_bytes;
                        let bytes = clone_bytes.saturating_add(partial.retained_bytes);
                        allowance.release_peak(bytes);
                        reserved -= bytes;
                        continue;
                    }
                    let choice = if partial.variant == 0 {
                        0
                    } else {
                        1 + (partial.variant + depth - 1)
                            % report.candidates.len().saturating_sub(1).max(1)
                    }
                    .min(report.candidates.len() - 1);
                    let selected = &report.candidates[choice];
                    let before_replay_bytes = partial.session.clone_reservation_bytes();
                    // Each retained page variant gains a routed session and a
                    // candidate witness. Reserve that growth before replay;
                    // these variants coexist until the page finishes.
                    let growth_bytes = selected
                        .cells
                        .len()
                        .saturating_mul(4096)
                        .saturating_add(selected.crossings.len().saturating_mul(4096))
                        .saturating_add(selected.commands.len().saturating_mul(128))
                        .saturating_add(4096);
                    if let Err(stop) = allowance.reserve_peak(growth_bytes) {
                        record_page_stop!(member, stop);
                        page_stopped = true;
                        break;
                    }
                    reserved = reserved.saturating_add(growth_bytes);
                    let candidate = selected.clone();
                    match partial
                        .session
                        .apply_route_candidate_with_allowance(&candidate, allowance)
                    {
                        Ok(_) => {}
                        Err(RequestFailure::Invalid(diagnostic)) => {
                            return Err(anyhow::Error::new(diagnostic));
                        }
                        Err(RequestFailure::Stopped(stop)) => {
                            record_page_stop!(member, stop);
                            page_stopped = true;
                            break;
                        }
                        Err(
                            RequestFailure::PendingPlacement { .. }
                            | RequestFailure::NoCompatibleEndpoints { .. },
                        ) => unreachable!("native replay does not prepare endpoints"),
                    }
                    let candidate_bytes = candidate
                        .cells
                        .len()
                        .saturating_mul(64)
                        .saturating_add(candidate.commands.len().saturating_mul(128))
                        .saturating_add(candidate.crossings.len().saturating_mul(256))
                        .saturating_add(1024);
                    let retained_growth = partial
                        .session
                        .clone_reservation_bytes()
                        .saturating_sub(before_replay_bytes)
                        .saturating_add(candidate_bytes);
                    if retained_growth > growth_bytes {
                        let extra = retained_growth - growth_bytes;
                        if let Err(stop) = allowance.reserve_peak(extra) {
                            record_page_stop!(member, stop);
                            page_stopped = true;
                            break;
                        }
                        reserved = reserved.saturating_add(extra);
                    } else {
                        let slack = growth_bytes - retained_growth;
                        allowance.release_peak(slack);
                        reserved -= slack;
                    }
                    partial.retained_bytes = partial.retained_bytes.saturating_add(retained_growth);
                    partial.candidate_bytes =
                        partial.candidate_bytes.saturating_add(candidate_bytes);
                    partial.replay_copy_bytes = partial
                        .replay_copy_bytes
                        .saturating_add(before_replay_bytes);
                    if let Err(stop) = allowance.check_deadline() {
                        record_page_stop!(member, stop);
                        page_stopped = true;
                        break;
                    }
                    partial.routes.push(candidate);
                    next.push(partial);
                    drop(report);
                    allowance.release_peak(report_bytes);
                    reserved -= report_bytes;
                }
                let input_bytes = prepared
                    .iter()
                    .flatten()
                    .map(|(_, input)| input.reserved_bytes)
                    .sum::<usize>();
                allowance.release_peak(input_bytes);
                reserved = reserved.saturating_sub(input_bytes);
                allowance.release_peak(guidance_bytes);
                reserved -= guidance_bytes;
                if page_stopped {
                    break;
                }
                partials = next;
            }
            if page_stopped {
                allowance.release_peak(reserved);
                if root_no_compatible {
                    allowance.release_peak(semantic_reservation);
                    allowance.release_peak(output_reservation);
                    return Ok(candidates);
                }
                if page_peak_stop && page_count > 1 {
                    counters.request_stops.truncate(stop_count_before_page);
                    counters.request_stops_total = stop_total_before_page;
                    counters.last_request_stop = last_stop_before_page.clone();
                    counters.route_deadline = deadline_before_page;
                    counters.route_work_limit = work_limit_before_page;
                    page_size = (page_count + 1) / 2;
                    continue;
                }
                allowance.release_peak(semantic_reservation);
                allowance.release_peak(output_reservation);
                counters.network_unknown += 1;
                return Ok(candidates);
            }
            let mut page_keys = BTreeSet::new();
            let mut page_candidates = Vec::new();
            let mut key_reservation = 0usize;
            for partial in partials {
                if partial.routes.len() != connections.len() {
                    let discarded_bytes = clone_bytes.saturating_add(partial.retained_bytes);
                    allowance.release_peak(discarded_bytes);
                    reserved -= discarded_bytes;
                    continue;
                }
                let route_cells = partial
                    .routes
                    .iter()
                    .map(|route| route.cells.len())
                    .sum::<usize>();
                let route_commands = partial
                    .routes
                    .iter()
                    .map(|route| route.commands.len())
                    .sum::<usize>();
                let key_bytes = route_cells
                    .saturating_mul(64)
                    .saturating_add(route_commands.saturating_mul(64))
                    .saturating_add(partial.routes.iter().fold(0usize, |total, route| {
                        total
                            .saturating_add(128)
                            .saturating_add(route.connection.len().saturating_mul(2))
                            .saturating_add(
                                route
                                    .source_port
                                    .as_ref()
                                    .map_or(0, String::len)
                                    .saturating_mul(2),
                            )
                            .saturating_add(
                                route
                                    .target_port
                                    .as_ref()
                                    .map_or(0, String::len)
                                    .saturating_mul(2),
                            )
                    }));
                if let Err(stop) = allowance.reserve_peak(key_bytes) {
                    record_page_stop!(&request, stop);
                    page_stopped = true;
                    break;
                }
                let key_result = network_witness_key_bounded(&partial.routes, allowance);
                let key = match key_result {
                    Ok(key) => key,
                    Err(stop) => {
                        allowance.release_peak(key_bytes);
                        record_page_stop!(&request, stop);
                        page_stopped = true;
                        break;
                    }
                };
                if semantic.contains(&key) || !page_keys.insert(key) {
                    allowance.release_peak(key_bytes);
                    let discarded_bytes = clone_bytes.saturating_add(partial.retained_bytes);
                    allowance.release_peak(discarded_bytes);
                    reserved -= discarded_bytes;
                    continue;
                }
                key_reservation = key_reservation.saturating_add(key_bytes);
                let shared_bytes = route_cells.saturating_mul(64);
                if let Err(stop) = allowance.reserve_peak(shared_bytes) {
                    record_page_stop!(&request, stop);
                    page_stopped = true;
                    break;
                }
                let shared_cells =
                    match network_shared_cells_with_allowance(&partial.routes, allowance) {
                        Ok(shared) => shared,
                        Err(stop) => {
                            allowance.release_peak(shared_bytes);
                            record_page_stop!(&request, stop);
                            page_stopped = true;
                            break;
                        }
                    };
                allowance.release_peak(shared_bytes);
                let retained_witness_bytes = partial.candidate_bytes;
                let final_clone_bytes = partial.session.clone_reservation_bytes();
                let final_layout_copy = partial.session.metrics().is_some();
                let replay_copied_bytes = clone_bytes
                    .saturating_mul(2)
                    .saturating_add(partial.replay_copy_bytes)
                    .saturating_add(if final_layout_copy {
                        final_clone_bytes
                    } else {
                        0
                    });
                let discarded_session_bytes = clone_bytes
                    .saturating_add(partial.retained_bytes)
                    .saturating_sub(retained_witness_bytes);
                let candidate = NetworkCandidate {
                    id: candidates.len() + page_candidates.len(),
                    state_revision: root.revision(),
                    commodity: first.0.clone(),
                    transport: first.1,
                    steps: partial.routes.iter().map(|route| route.steps).sum(),
                    turns: partial.routes.iter().map(|route| route.turns).sum(),
                    expanded: partial.routes.iter().map(|route| route.expanded).sum(),
                    shared_cells,
                    routes: partial.routes,
                };
                drop(partial.session);
                allowance.release_peak(discarded_session_bytes);
                reserved -= discarded_session_bytes;
                let replay_bytes = root.clone_reservation_bytes();
                let replay_cost = allowance
                    .preflight(WorkKind::CopiedBytes, replay_bytes)
                    .and_then(|_| allowance.charge(WorkKind::CopiedBytes, replay_bytes))
                    .and_then(|_| allowance.reserve_peak(replay_bytes));
                if let Err(stop) = replay_cost {
                    record_page_stop!(&request, stop);
                    page_stopped = true;
                    break;
                }
                reserved = reserved.saturating_add(replay_bytes);
                let mut replay = root.clone();
                match replay.apply_network_candidate_with_allowance(
                    &candidate,
                    final_clone_bytes,
                    replay_copied_bytes,
                    final_layout_copy,
                    allowance,
                ) {
                    Ok(_) => {}
                    Err(RequestFailure::Invalid(diagnostic)) => {
                        return Err(anyhow::Error::new(diagnostic));
                    }
                    Err(RequestFailure::Stopped(stop)) => {
                        record_page_stop!(&request, stop);
                        page_stopped = true;
                        break;
                    }
                    Err(
                        RequestFailure::PendingPlacement { .. }
                        | RequestFailure::NoCompatibleEndpoints { .. },
                    ) => unreachable!("native replay does not prepare endpoints"),
                }
                allowance.release_peak(replay_bytes);
                reserved -= replay_bytes;
                let bytes = match json_encoded_len_with_allowance(&candidate, allowance) {
                    Ok(bytes) => bytes,
                    Err(failure) => match failure {
                        RequestFailure::Stopped(stop) => {
                            record_page_stop!(&request, stop);
                            page_stopped = true;
                            break;
                        }
                        RequestFailure::Invalid(diagnostic) => {
                            return Err(anyhow::Error::new(diagnostic));
                        }
                        _ => unreachable!("JSON encoding does not prepare endpoints"),
                    },
                };
                let output_bytes = bytes.max(retained_witness_bytes);
                let additional_bytes = output_bytes - retained_witness_bytes;
                if let Err(stop) = allowance.reserve_peak(additional_bytes) {
                    record_page_stop!(&request, stop);
                    page_stopped = true;
                    break;
                }
                if let Err(stop) = allowance
                    .preflight(WorkKind::Candidates, page_candidates.len() + 1)
                    .and_then(|_| allowance.check_deadline())
                {
                    record_page_stop!(&request, stop);
                    allowance.release_peak(additional_bytes);
                    page_stopped = true;
                    break;
                }
                reserved -= retained_witness_bytes;
                output_reservation = output_reservation.saturating_add(output_bytes);
                page_candidates.push(candidate);
            }
            allowance.release_peak(reserved);
            if page_stopped {
                allowance.release_peak(key_reservation);
                if page_peak_stop && page_count > 1 {
                    allowance.release_peak(output_reservation - output_before_page);
                    output_reservation = output_before_page;
                    counters.request_stops.truncate(stop_count_before_page);
                    counters.request_stops_total = stop_total_before_page;
                    counters.last_request_stop = last_stop_before_page.clone();
                    counters.route_deadline = deadline_before_page;
                    counters.route_work_limit = work_limit_before_page;
                    page_size = (page_count + 1) / 2;
                    continue;
                }
                allowance.release_peak(semantic_reservation);
                allowance.release_peak(output_reservation);
                counters.network_unknown += 1;
                return Ok(candidates);
            }
            if let Err(stop) = allowance.charge(WorkKind::Candidates, page_candidates.len()) {
                record_request_stop(counters, &request, allowance, stop);
                allowance.release_peak(key_reservation);
                allowance.release_peak(semantic_reservation);
                allowance.release_peak(output_reservation);
                counters.network_unknown += 1;
                return Ok(candidates);
            }
            semantic.extend(page_keys);
            semantic_reservation = semantic_reservation.saturating_add(key_reservation);
            candidates.extend(page_candidates);
            page_start += page_count;
        }
        allowance.release_peak(semantic_reservation);
        allowance.release_peak(output_reservation);
        if candidates.is_empty() {
            counters.network_unknown += 1;
        }
        Ok(candidates)
    }

    fn score_actions(
        &self,
        session: &Session,
        actions: &[MacroAction],
        children: &[Session],
    ) -> Result<ActionScores> {
        let Some(model) = self.outer.as_ref() else {
            let scores = actions
                .iter()
                .zip(children)
                .map(|(action, child)| action.prior_score(session, child, self.domain.placement))
                .collect::<Vec<_>>();
            let maximum = scores.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let weights = scores
                .iter()
                .map(|score| (score - maximum).exp())
                .collect::<Vec<_>>();
            let denominator = weights.iter().sum::<f64>();
            let anchor_priors = weights
                .into_iter()
                .map(|weight| weight / denominator.max(f64::MIN_POSITIVE))
                .collect::<Vec<_>>();
            return Ok(ActionScores {
                priors: anchor_priors.clone(),
                anchor_priors,
                model_value: state_value(session, self.domain.placement),
            });
        };
        let input = session
            .macro_learning_input(&self.domain)
            .map_err(anyhow::Error::new)?;
        let features = actions
            .iter()
            .zip(children)
            .map(|(action, child)| action.features(session, child, self.domain.placement))
            .collect::<Vec<_>>();
        let entity_references = actions
            .iter()
            .map(|action| action.entity_index(session.problem()))
            .collect::<Result<Vec<_>>>()?;
        let batch = OuterBatch::from_rows(
            &[input.as_ref()],
            &[features.as_slice()],
            &[entity_references.as_slice()],
            self.device,
        )?;
        let prediction = tch::no_grad(|| model.forward(&batch))?;
        let learned = Vec::<f32>::try_from(prediction.logp.exp().to_device(Device::Cpu))?
            .into_iter()
            .map(f64::from)
            .collect::<Vec<_>>();
        let anchor_scores = actions
            .iter()
            .zip(children)
            .map(|(action, child)| action.prior_score(session, child, self.domain.placement))
            .collect::<Vec<_>>();
        let maximum = anchor_scores
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let anchor_weights = anchor_scores
            .iter()
            .map(|score| (score - maximum).exp())
            .collect::<Vec<_>>();
        let denominator = anchor_weights.iter().sum::<f64>();
        let anchor_priors = anchor_weights
            .into_iter()
            .map(|anchor| anchor / denominator.max(f64::MIN_POSITIVE))
            .collect::<Vec<_>>();
        let priors = learned
            .into_iter()
            .zip(&anchor_priors)
            .map(|(learned, anchor)| {
                learned * (1.0 - self.anchor_prior_mass()) + anchor * self.anchor_prior_mass()
            })
            .collect();
        let completion = prediction.values.double_value(&[0, COMPLETION_HEAD]);
        let penalty = lexicographic_quality_penalty(
            prediction.values.double_value(&[0, AREA_HEAD]),
            prediction.values.double_value(&[0, TRANSPORT_HEAD]),
            prediction.values.double_value(&[0, TURNS_HEAD]),
            self.domain.placement,
            session.problem().connections.len(),
        );
        Ok(ActionScores {
            priors,
            anchor_priors,
            model_value: completion * predicted_quality_value(penalty),
        })
    }

    fn anchor_prior_mass(&self) -> f64 {
        // Config validation owns the range. Keeping this accessor local makes
        // it explicit that the anchor participates only in action ordering.
        self.anchor_prior_mass
    }
}

fn inference_reservation_bytes(
    input: &aic_pilot_native::constructive::routing_candidates::GuidanceInput,
    heads: usize,
) -> usize {
    let graph_elements = input
        .graph
        .entity_features
        .len()
        .saturating_mul(aic_pilot_native::constructive::learning::ENTITY_FEATURES)
        .saturating_add(
            input
                .graph
                .relation_features
                .len()
                .saturating_mul(aic_pilot_native::constructive::learning::RELATION_FEATURES),
        )
        .saturating_add(
            input
                .graph
                .spatial_features
                .len()
                .saturating_mul(aic_pilot_native::constructive::learning::SPATIAL_FEATURES),
        );
    // Keep the native input, tensor copies, intermediate model activations,
    // dense prediction and sparse directed-cost conversion live together.
    input
        .grid
        .len()
        .saturating_add(graph_elements)
        .saturating_mul(32)
        .saturating_add(
            input
                .grid
                .len()
                .saturating_div(
                    aic_pilot_native::constructive::routing_candidates::GUIDANCE_GRID_CHANNELS,
                )
                .saturating_mul(heads)
                .saturating_mul(4)
                .saturating_mul(48),
        )
}

fn json_encoded_len_with_allowance(
    value: &impl Serialize,
    allowance: &mut RequestAllowance,
) -> std::result::Result<usize, RequestFailure> {
    struct Counter<'a> {
        bytes: usize,
        pending: usize,
        allowance: &'a mut RequestAllowance,
        stopped: Option<AllowanceStop>,
    }
    impl Write for Counter<'_> {
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
    let mut counter = Counter {
        bytes: 0,
        pending: 0,
        allowance,
        stopped: None,
    };
    if let Err(error) = serde_json::to_writer(&mut counter, value) {
        if let Some(stop) = counter.stopped {
            return Err(RequestFailure::Stopped(stop));
        }
        return Err(RequestFailure::Invalid(
            aic_pilot_native::constructive::Diagnostic {
                code: "hierarchical.candidate-encoding-failed".to_owned(),
                severity: "error",
                stage: "hierarchical-search",
                message: error.to_string(),
                entity: "network-candidate".to_owned(),
            },
        ));
    }
    counter
        .allowance
        .record_completed(WorkKind::PublishedBytes, counter.pending)?;
    Ok(counter.bytes)
}

fn route_report_reservation_bytes(report: &RouteReport) -> usize {
    report
        .candidates
        .iter()
        .fold(1024usize, |total, candidate| {
            total
                .saturating_add(candidate.cells.len().saturating_mul(64))
                .saturating_add(candidate.commands.len().saturating_mul(128))
                .saturating_add(candidate.crossings.len().saturating_mul(256))
                .saturating_add(1024)
        })
        .saturating_add(report.native_rejections.len().saturating_mul(2048))
}

fn record_route_report(counters: &mut Counters, report: &RouteReport) {
    counters.route_searches += report.searches;
    counters.route_expanded += report.expanded;
    counters.route_preflight_expanded += report.preflight_expanded;
    counters.native_rejections += report.native_rejections.len();
    match report.termination {
        RouteTermination::CandidateLimit => counters.route_candidate_limit += 1,
        RouteTermination::SearchLimit => counters.route_search_limit += 1,
        RouteTermination::WorkBudget => counters.route_work_budget += 1,
        RouteTermination::ResourceLimit => counters.route_resource_limit += 1,
        RouteTermination::FrontierExhausted => counters.route_frontier_exhausted += 1,
        RouteTermination::NoCompatibleEndpoints => {
            counters.route_no_compatible_endpoints += 1;
            counters.route_no_compatible_connection = Some(report.connection.clone());
        }
        RouteTermination::WorkLimit => counters.route_work_limit += 1,
        RouteTermination::Deadline => counters.route_deadline += 1,
    }
    if let (Some(stop), Some(usage)) = (&report.allowance_stop, &report.allowance) {
        capture_request_stop(
            counters,
            RequestStopRecord {
                state_revision: report.state_revision,
                connection: report.connection.clone(),
                stop: stop.clone(),
                usage: usage.clone(),
            },
        );
    }
    if counters.route_attempt_samples.len() < ROUTE_ATTEMPT_SAMPLE_LIMIT {
        counters.route_attempt_samples.push(RouteAttemptRecord {
            state_revision: report.state_revision,
            connection: report.connection.clone(),
            termination: report.termination,
            fixed_request_proven_infeasible: report.fixed_request_proven_infeasible,
            candidates: report.candidates.len(),
        });
    } else {
        counters.route_attempt_samples_dropped += 1;
    }
}

fn record_request_stop(
    counters: &mut Counters,
    request: &Request,
    allowance: &RequestAllowance,
    stop: AllowanceStop,
) {
    if stop.reason == "deadline" {
        counters.route_deadline += 1;
    } else {
        counters.route_work_limit += 1;
    }
    capture_request_stop(
        counters,
        RequestStopRecord {
            state_revision: request.state_revision,
            connection: request.connection.clone(),
            stop,
            usage: allowance.usage(),
        },
    );
}

fn capture_request_stop(counters: &mut Counters, record: RequestStopRecord) {
    counters.request_stops_total += 1;
    counters.last_request_stop = Some((record.connection.clone(), record.stop.clone()));
    if counters.request_stops.len() < ROUTE_ATTEMPT_SAMPLE_LIMIT {
        counters.request_stops.push(record);
    }
}

fn no_macro_reason(
    counters: &Counters,
    before: (
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    ),
    revision: u64,
) -> MacroDiagnostic {
    let (code, entity, message) = if counters.route_no_compatible_endpoints > before.4 {
        (
            "hierarchical.no-compatible-endpoints",
            counters
                .route_no_compatible_connection
                .clone()
                .unwrap_or_else(|| "macro-state".into()),
            "placed endpoints have no compatible port pair".into(),
        )
    } else if counters.request_stops_total > before.0 {
        let (connection, stop) = counters
            .last_request_stop
            .as_ref()
            .expect("new stop record");
        (
            if stop.reason == "deadline" {
                "hierarchical.request-deadline"
            } else {
                "hierarchical.request-work-limit"
            },
            connection.clone(),
            format!(
                "request stopped at {:?}: used {}, limit {}",
                stop.category, stop.used, stop.limit
            ),
        )
    } else if counters.route_deadline > before.1 {
        (
            "hierarchical.request-deadline",
            "macro-state".into(),
            "routing request deadline reached".into(),
        )
    } else if counters.route_work_limit > before.2 {
        (
            "hierarchical.request-work-limit",
            "macro-state".into(),
            "routing request work allowance exhausted".into(),
        )
    } else if counters.route_search_limit > before.8 {
        (
            "hierarchical.route-search-limit",
            "macro-state".into(),
            "route search count limit reached before finding a candidate".into(),
        )
    } else if counters.route_work_budget > before.9 {
        (
            "hierarchical.route-work-budget",
            "macro-state".into(),
            "route expansion budget exhausted before finding a candidate".into(),
        )
    } else if counters.route_candidate_limit > before.10 {
        (
            "hierarchical.route-candidate-limit",
            "macro-state".into(),
            "route candidate limit reached before publishing a candidate".into(),
        )
    } else if counters.network_unknown > before.5 {
        (
            "hierarchical.network-unknown",
            "macro-state".into(),
            "finite network variants produced no completed candidate or stopped early".into(),
        )
    } else if counters.route_resource_limit > before.7 {
        (
            "hierarchical.route-resource-limit",
            "macro-state".into(),
            "route search exhausted a restricted legal graph; full feasibility remains unknown"
                .into(),
        )
    } else if counters.route_frontier_exhausted > before.3 {
        (
            "hierarchical.frontier-exhausted",
            "macro-state".into(),
            "complete fixed point-to-point route graph was exhausted locally".into(),
        )
    } else if counters.pending_route_networks > before.6 {
        (
            "hierarchical.not-ready",
            "macro-state".into(),
            "routing endpoints are not yet placed".into(),
        )
    } else {
        (
            "hierarchical.no-macro-actions",
            "macro-state".into(),
            "no legal placement or routing macro was available".into(),
        )
    };
    MacroDiagnostic {
        code: code.into(),
        severity: "warning".into(),
        stage: "hierarchical-search".into(),
        message: format!("{message} at revision {revision}"),
        entity,
    }
}

impl MacroAction {
    pub(crate) fn entity_index(&self, problem: &Problem) -> Result<i64> {
        let index = match self {
            Self::Command(Command::Place { block, .. }) => problem
                .blocks
                .iter()
                .position(|candidate| candidate.id == *block),
            Self::Command(Command::PlaceTerminal { terminal, .. }) => problem
                .terminals
                .iter()
                .position(|candidate| candidate.id == *terminal)
                .map(|index| problem.blocks.len() + index),
            Self::Command(_) => None,
            Self::Route(candidate) => problem
                .connections
                .iter()
                .position(|connection| connection.id == candidate.connection)
                .map(|index| problem.blocks.len() + problem.terminals.len() + index),
            Self::Network(candidate) => candidate.routes.first().and_then(|route| {
                problem
                    .connections
                    .iter()
                    .position(|connection| connection.id == route.connection)
                    .map(|index| problem.blocks.len() + problem.terminals.len() + index)
            }),
            Self::Repair { request, .. } => request.selected_connections.first().and_then(|id| {
                problem
                    .connections
                    .iter()
                    .position(|connection| connection.id == *id)
                    .map(|index| problem.blocks.len() + problem.terminals.len() + index)
            }),
        };
        index.map(|index| index as i64).context(
            "aic-learning.hierarchical-action-entity: action entity is absent from problem",
        )
    }

    /// Reconstruct repair authority from the live state and trusted search
    /// configuration before accepting an action from a serialized record.
    pub(crate) fn apply(
        &self,
        session: &Session,
        candidate_config: &CandidateConfig,
        repair_max_removed: usize,
    ) -> Result<Session> {
        if let Self::Repair { request, candidate } = self {
            let trusted = session.repair_request_for_policy(
                &request.selected_connections,
                repair_max_removed,
                candidate_config,
            )?;
            ensure!(
                trusted == *request,
                "aic-learning.hierarchical-repair-authority: serialized repair request differs from trusted state and policy"
            );
            return session
                .repaired_candidate(&trusted, candidate)
                .map(|(draft, _)| draft)
                .map_err(anyhow::Error::new);
        }
        let mut fork = session.clone();
        match self {
            Self::Command(command) => {
                let step = fork.step(command.clone());
                ensure!(
                    step.accepted && step.diagnostic.is_none(),
                    "aic-learning.hierarchical-command: native macro command rejected: {:?}",
                    step.diagnostic
                );
            }
            Self::Route(candidate) => {
                fork.apply_route_candidate(candidate)
                    .map_err(anyhow::Error::new)?;
            }
            Self::Network(candidate) => {
                fork.apply_network_candidate(candidate)
                    .map_err(anyhow::Error::new)?;
            }
            Self::Repair { .. } => unreachable!("repair was applied transactionally above"),
        }
        ensure!(
            fork.observe().active.is_none(),
            "aic-learning.hierarchical-action: macro action left a partial route"
        );
        Ok(fork)
    }

    fn summary(&self, revision: u64) -> ActionSummary {
        match self {
            Self::Command(Command::Place { block, .. }) => ActionSummary {
                kind: "place".into(),
                entity: block.clone(),
                state_revision: revision,
                steps: None,
                provenance: None,
                bridge_uses: vec![],
                removed_connections: vec![],
            },
            Self::Command(Command::PlaceTerminal { terminal, .. }) => ActionSummary {
                kind: "place-terminal".into(),
                entity: terminal.clone(),
                state_revision: revision,
                steps: None,
                provenance: None,
                bridge_uses: vec![],
                removed_connections: vec![],
            },
            Self::Command(_) => unreachable!("macro command generator exposes only placements"),
            Self::Route(candidate) => ActionSummary {
                kind: "route".into(),
                entity: candidate.connection.clone(),
                state_revision: candidate.state_revision,
                steps: Some(candidate.steps),
                provenance: Some(candidate.provenance.clone()),
                bridge_uses: bridge_uses(std::iter::once(candidate)),
                removed_connections: vec![],
            },
            Self::Network(candidate) => ActionSummary {
                kind: "network".into(),
                entity: candidate.commodity.clone(),
                state_revision: candidate.state_revision,
                steps: Some(candidate.steps),
                provenance: None,
                bridge_uses: bridge_uses(candidate.routes.iter()),
                removed_connections: vec![],
            },
            Self::Repair { request, candidate } => ActionSummary {
                kind: "repair".into(),
                entity: request.selected_connections.join(","),
                state_revision: request.state_revision,
                steps: Some(candidate.replacements.iter().map(|route| route.steps).sum()),
                provenance: None,
                bridge_uses: bridge_uses(candidate.replacements.iter()),
                removed_connections: candidate.removed.clone(),
            },
        }
    }

    fn prior_score(&self, before: &Session, after: &Session, bounds: Bounds) -> f64 {
        let before = construction_progress(before);
        let after_progress = construction_progress(after);
        let progress = (after_progress - before) * 8.0;
        match self {
            Self::Command(command) => progress + placement_anchor_score(command, after, bounds),
            Self::Route(candidate) => {
                progress + f64::from(candidate.predicted_success.unwrap_or(0.5))
                    - candidate.steps as f64 * 0.002
                    - candidate.learned_cost as f64 * 0.0001
            }
            Self::Network(candidate) => {
                let success = candidate
                    .routes
                    .iter()
                    .map(|route| f64::from(route.predicted_success.unwrap_or(0.5)))
                    .sum::<f64>()
                    / candidate.routes.len() as f64;
                progress + success + candidate.shared_cells as f64 * 0.002
                    - candidate.steps as f64 * 0.001
            }
            Self::Repair { candidate, .. } => {
                let steps = candidate
                    .replacements
                    .iter()
                    .map(|route| route.steps)
                    .sum::<i64>();
                progress + 0.5 - steps as f64 * 0.001 - candidate.removed.len() as f64 * 0.02
            }
        }
    }

    pub(crate) fn features(
        &self,
        before: &Session,
        after: &Session,
        bounds: Bounds,
    ) -> [f32; ACTION_FEATURES] {
        let mut row = [0.0; ACTION_FEATURES];
        let scale = bounds.width().max(bounds.height()).max(1) as f32;
        row[3] = construction_progress(before) as f32;
        row[4] = construction_progress(after) as f32;
        row[5] = row[4] - row[3];
        row[23] = after.layout().is_some() as u8 as f32;
        match self {
            Self::Command(Command::Place {
                position, rotation, ..
            }) => {
                row[0] = 1.0;
                row[6] = position.x as f32 / scale;
                row[7] = position.y as f32 / scale;
                row[8] = (*rotation as f32).to_radians().sin();
                row[9] = (*rotation as f32).to_radians().cos();
            }
            Self::Command(Command::PlaceTerminal { position, .. }) => {
                row[1] = 1.0;
                row[6] = position.x as f32 / scale;
                row[7] = position.y as f32 / scale;
            }
            Self::Command(_) => unreachable!("macro command generator exposes only placements"),
            Self::Route(candidate) => {
                row[2] = 1.0;
                let connection = before
                    .problem()
                    .connections
                    .iter()
                    .position(|connection| connection.id == candidate.connection)
                    .unwrap_or(0);
                row[6] = connection as f32 / before.problem().connections.len().max(1) as f32;
                row[7] = candidate.steps as f32 / scale;
                row[8] = candidate.turns as f32 / candidate.steps.max(1) as f32;
                row[9] =
                    candidate.overlap_with_previous as f32 / candidate.cells.len().max(1) as f32;
                row[10] = (candidate.learned_cost as f32).ln_1p() / 16.0;
                row[11] = candidate.predicted_success.unwrap_or(0.5);
                row[12] = candidate.guidance_uncertainty.unwrap_or(1.0);
                match &candidate.provenance {
                    Provenance::Anchor => row[13] = 1.0,
                    Provenance::Guided { .. } => row[14] = 1.0,
                    Provenance::Diversified { .. } => row[15] = 1.0,
                }
                row[16] = candidate.source_port.is_some() as u8 as f32;
                row[17] = candidate.target_port.is_some() as u8 as f32;
                if let Some(min_x) = candidate.cells.iter().map(|point| point.x).min() {
                    row[18] = min_x as f32 / scale;
                    row[19] =
                        candidate.cells.iter().map(|point| point.x).max().unwrap() as f32 / scale;
                    row[20] =
                        candidate.cells.iter().map(|point| point.y).min().unwrap() as f32 / scale;
                    row[21] =
                        candidate.cells.iter().map(|point| point.y).max().unwrap() as f32 / scale;
                }
                row[22] = candidate.cells.len() as f32 / (bounds.width() * bounds.height()) as f32;
                row[24..30]
                    .copy_from_slice(&bridge_action_features(after, std::iter::once(candidate)));
            }
            Self::Network(candidate) => {
                row[2] = 1.0;
                let first = &candidate.routes[0];
                let connection = before
                    .problem()
                    .connections
                    .iter()
                    .position(|connection| connection.id == first.connection)
                    .unwrap_or(0);
                row[6] = connection as f32 / before.problem().connections.len().max(1) as f32;
                row[7] = candidate.steps as f32 / scale;
                row[8] = candidate.turns as f32 / candidate.steps.max(1) as f32;
                let cells = candidate
                    .routes
                    .iter()
                    .flat_map(|route| route.cells.iter().copied())
                    .collect::<BTreeSet<_>>();
                row[9] = candidate.shared_cells as f32 / cells.len().max(1) as f32;
                row[10] = (candidate
                    .routes
                    .iter()
                    .map(|route| route.learned_cost)
                    .sum::<u64>() as f32)
                    .ln_1p()
                    / 16.0;
                row[11] = candidate
                    .routes
                    .iter()
                    .map(|route| route.predicted_success.unwrap_or(0.5))
                    .sum::<f32>()
                    / candidate.routes.len() as f32;
                row[12] = candidate
                    .routes
                    .iter()
                    .map(|route| route.guidance_uncertainty.unwrap_or(1.0))
                    .sum::<f32>()
                    / candidate.routes.len() as f32;
                for route in &candidate.routes {
                    match route.provenance {
                        Provenance::Anchor => row[13] = 1.0,
                        Provenance::Guided { .. } => row[14] = 1.0,
                        Provenance::Diversified { .. } => row[15] = 1.0,
                    }
                }
                row[16] = candidate
                    .routes
                    .iter()
                    .any(|route| route.source_port.is_some()) as u8
                    as f32;
                row[17] = candidate
                    .routes
                    .iter()
                    .any(|route| route.target_port.is_some()) as u8
                    as f32;
                if let Some(min_x) = cells.iter().map(|point| point.x).min() {
                    row[18] = min_x as f32 / scale;
                    row[19] = cells.iter().map(|point| point.x).max().unwrap() as f32 / scale;
                    row[20] = cells.iter().map(|point| point.y).min().unwrap() as f32 / scale;
                    row[21] = cells.iter().map(|point| point.y).max().unwrap() as f32 / scale;
                }
                row[22] = cells.len() as f32 / (bounds.width() * bounds.height()) as f32;
                row[24..30]
                    .copy_from_slice(&bridge_action_features(after, candidate.routes.iter()));
            }
            Self::Repair { request, candidate } => {
                // The route-kind channel uses 2 for a transactional repair;
                // ordinary route and network witnesses use 1.
                row[2] = 2.0;
                let connection = request
                    .selected_connections
                    .first()
                    .and_then(|id| {
                        before
                            .problem()
                            .connections
                            .iter()
                            .position(|connection| connection.id == *id)
                    })
                    .unwrap_or(0);
                row[6] = connection as f32 / before.problem().connections.len().max(1) as f32;
                let steps = candidate
                    .replacements
                    .iter()
                    .map(|route| route.steps)
                    .sum::<i64>();
                row[7] = steps as f32 / scale;
                row[8] = candidate.removed.len() as f32
                    / before.problem().connections.len().max(1) as f32;
                let cells = candidate
                    .replacements
                    .iter()
                    .flat_map(|route| route.cells.iter().copied())
                    .collect::<BTreeSet<_>>();
                row[9] = candidate.expanded as f32 / 100_000.0;
                row[10] = (candidate
                    .replacements
                    .iter()
                    .map(|route| route.learned_cost)
                    .sum::<u64>() as f32)
                    .ln_1p()
                    / 16.0;
                row[11] = candidate
                    .replacements
                    .iter()
                    .map(|route| route.predicted_success.unwrap_or(0.5))
                    .sum::<f32>()
                    / candidate.replacements.len().max(1) as f32;
                row[12] = candidate
                    .replacements
                    .iter()
                    .map(|route| route.guidance_uncertainty.unwrap_or(1.0))
                    .sum::<f32>()
                    / candidate.replacements.len().max(1) as f32;
                for route in &candidate.replacements {
                    match route.provenance {
                        Provenance::Anchor => row[13] = 1.0,
                        Provenance::Guided { .. } => row[14] = 1.0,
                        Provenance::Diversified { .. } => row[15] = 1.0,
                    }
                }
                row[16] = candidate
                    .replacements
                    .iter()
                    .any(|route| route.source_port.is_some()) as u8
                    as f32;
                row[17] = candidate
                    .replacements
                    .iter()
                    .any(|route| route.target_port.is_some()) as u8
                    as f32;
                if let Some(min_x) = cells.iter().map(|point| point.x).min() {
                    row[18] = min_x as f32 / scale;
                    row[19] = cells.iter().map(|point| point.x).max().unwrap() as f32 / scale;
                    row[20] = cells.iter().map(|point| point.y).min().unwrap() as f32 / scale;
                    row[21] = cells.iter().map(|point| point.y).max().unwrap() as f32 / scale;
                }
                row[22] = cells.len() as f32 / (bounds.width() * bounds.height()) as f32;
                row[24..30].copy_from_slice(&bridge_action_features(
                    after,
                    candidate.replacements.iter(),
                ));
            }
        }
        row
    }
}

fn placement_anchor_score(command: &Command, session: &Session, bounds: Bounds) -> f64 {
    let span = bounds.width().max(bounds.height()).max(1) as f64;
    let position = match command {
        Command::Place { position, .. } | Command::PlaceTerminal { position, .. } => *position,
        _ => return 0.0,
    };
    let center_x = (bounds.min_x + bounds.max_x) as f64 * 0.5;
    let center_y = (bounds.min_y + bounds.max_y) as f64 * 0.5;
    let center_distance =
        ((position.x as f64 - center_x).abs() + (position.y as f64 - center_y).abs()) / span;
    let observation = session.observe();
    let endpoint = |id: &str| {
        session
            .problem()
            .blocks
            .iter()
            .position(|block| block.id == id)
            .and_then(|index| observation.poses[index].as_ref().map(|pose| pose.position))
            .or_else(|| {
                session
                    .problem()
                    .terminals
                    .iter()
                    .position(|terminal| terminal.id == id)
                    .and_then(|index| {
                        observation.terminal_poses[index]
                            .as_ref()
                            .map(|pose| pose.position)
                    })
            })
    };
    let connected_distance = session
        .problem()
        .connections
        .iter()
        .filter_map(|connection| {
            endpoint(&connection.source)
                .zip(endpoint(&connection.target))
                .map(|(source, target)| source.distance(target) as f64 / span)
        })
        .sum::<f64>();
    -center_distance * 0.08 - connected_distance * 0.12
}

type NetworkWitnessKey = Vec<(
    String,
    Option<String>,
    Option<String>,
    Vec<Point>,
    Vec<(Point, String, i64, BridgeAxis)>,
)>;

#[cfg(test)]
fn network_witness_key(routes: &[Candidate]) -> NetworkWitnessKey {
    let mut key = routes
        .iter()
        .map(|route| {
            let bridges = route
                .commands
                .iter()
                .filter_map(|command| match command {
                    Command::ReserveBridge {
                        position,
                        component,
                        rotation,
                        crossed_axis,
                    } => Some((*position, component.clone(), *rotation, *crossed_axis)),
                    _ => None,
                })
                .collect();
            (
                route.connection.clone(),
                route.source_port.clone(),
                route.target_port.clone(),
                route.cells.clone(),
                bridges,
            )
        })
        .collect::<NetworkWitnessKey>();
    key.sort_by(|left, right| left.0.cmp(&right.0));
    key
}

fn reserve_network_clone_page(
    allowance: &mut RequestAllowance,
    clone_bytes: usize,
    page_count: &mut usize,
) -> std::result::Result<usize, AllowanceStop> {
    loop {
        let bytes = clone_bytes.saturating_mul(*page_count);
        let setup = allowance
            .preflight(WorkKind::CopiedBytes, bytes)
            .and_then(|_| allowance.reserve_peak(bytes));
        match setup {
            Ok(()) => return Ok(bytes),
            Err(stop)
                if *page_count > 1
                    && (stop.reason == "peak-memory-limit"
                        || stop.category == Some(WorkKind::CopiedBytes)) =>
            {
                *page_count = (*page_count + 1) / 2;
            }
            Err(stop) => return Err(stop),
        }
    }
}

fn network_witness_key_bounded(
    routes: &[Candidate],
    allowance: &mut RequestAllowance,
) -> std::result::Result<NetworkWitnessKey, AllowanceStop> {
    let mut key = Vec::with_capacity(routes.len());
    for route in routes {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        let mut cells = Vec::with_capacity(route.cells.len());
        for &point in &route.cells {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            cells.push(point);
        }
        let mut bridges = Vec::new();
        for command in &route.commands {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            if let Command::ReserveBridge {
                position,
                component,
                rotation,
                crossed_axis,
            } = command
            {
                bridges.push((*position, component.clone(), *rotation, *crossed_axis));
            }
        }
        key.push((
            route.connection.clone(),
            route.source_port.clone(),
            route.target_port.clone(),
            cells,
            bridges,
        ));
    }
    allowance.charge(WorkKind::RoutingScans, key.len().saturating_mul(2))?;
    key.sort_by(|left, right| left.0.cmp(&right.0));
    allowance.check_deadline()?;
    Ok(key)
}

fn session_network_identity(
    session: &Session,
    connections: &[String],
    allowance: &mut RequestAllowance,
) -> std::result::Result<(String, aic_layout::logistics::TransportKind), RequestFailure> {
    let invalid = |message: &str| {
        RequestFailure::Invalid(aic_pilot_native::constructive::Diagnostic {
            code: "hierarchical.network-identity".into(),
            severity: "error",
            stage: "hierarchical-search",
            message: message.into(),
            entity: connections
                .first()
                .cloned()
                .unwrap_or_else(|| "network".into()),
        })
    };
    if connections.len() < 2 {
        return Err(invalid("network task needs multiple connections"));
    }
    let mut unique = BTreeSet::new();
    let mut first = None;
    for id in connections {
        allowance.charge(WorkKind::RoutingScans, 1)?;
        if !unique.insert(id.as_str()) {
            return Err(invalid("network task repeats a connection"));
        }
        let mut found = None;
        for connection in &session.problem().connections {
            allowance.charge(WorkKind::RoutingScans, 1)?;
            if connection.id == *id {
                found = Some(connection);
                break;
            }
        }
        let connection =
            found.ok_or_else(|| invalid("network task references an unknown connection"))?;
        let identity = (connection.commodity.clone(), connection.transport);
        if first.as_ref().is_some_and(|first| first != &identity) {
            return Err(invalid("network task mixes incompatible connections"));
        }
        first.get_or_insert(identity);
    }
    Ok(first.expect("nonempty network task"))
}

pub fn run(
    config_path: &Path,
    panel_path: &Path,
    lower_training_output: &Path,
    outer_training_output: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
) -> Result<Value> {
    let panel = load_published_panel(panel_path)?;
    let report = collect_and_publish(
        config_path,
        &panel,
        lower_training_output,
        outer_training_output,
        catalog_root,
        output,
        device,
        true,
    )?;
    Ok(report_summary(output, &report.value))
}

pub(crate) fn run_training_published(
    config_path: &Path,
    panel: &Published<Panel>,
    lower_training_output: &Path,
    outer_training_output: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
) -> Result<Published<Report>> {
    collect_and_publish(
        config_path,
        panel,
        lower_training_output,
        outer_training_output,
        catalog_root,
        output,
        device,
        true,
    )
}

pub(crate) fn run_evaluation_published(
    config_path: &Path,
    panel: &Published<Panel>,
    lower_training_output: &Path,
    outer_training_output: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
) -> Result<Published<Report>> {
    collect_and_publish(
        config_path,
        panel,
        lower_training_output,
        outer_training_output,
        catalog_root,
        output,
        device,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn collect_and_publish(
    config_path: &Path,
    panel: &Published<Panel>,
    lower_training_output: &Path,
    outer_training_output: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
    retain_root_records: bool,
) -> Result<Published<Report>> {
    ensure!(
        !output.exists(),
        "aic-learning.hierarchical-output: {} already exists",
        output.display()
    );
    let config_bytes = bounded_read(config_path)?;
    let config: Config = serde_json::from_slice(&config_bytes)?;
    validate_config(&config)?;
    validate_panel(&panel.value)?;
    let lower_config_bytes = bounded_read(&lower_training_output.join("config.json"))?;
    let lower_order_bytes = bounded_read(&lower_training_output.join("parameter-order.json"))?;
    let lower_config: GuidanceTrainingConfig = serde_json::from_slice(&lower_config_bytes)?;
    ensure!(
        lower_config.learning_domain
            == (DomainConfig {
                placement: config.bounds,
                extension_lengths: config.extension_lengths.clone(),
            }),
        "aic-learning.hierarchical-lower-domain: search and lower model domains differ"
    );
    let order: ParameterOrder = serde_json::from_slice(&lower_order_bytes)?;
    let checkpoint = lower_training_output.join("state.safetensors");
    let values: Tensors = Tensor::read_safetensors(&checkpoint)?.into_iter().collect();
    let guidance = GuidanceModel::load(&lower_config.model, &order, &values, device)?;
    let checkpoint_sha256 = crate::input::sha256(&checkpoint)?;
    let outer_config_bytes = bounded_read(&outer_training_output.join("config.json"))?;
    let outer_order_bytes = bounded_read(&outer_training_output.join("parameter-order.json"))?;
    let outer_config: OuterTrainingConfig = serde_json::from_slice(&outer_config_bytes)?;
    let outer_order: OuterOrder = serde_json::from_slice(&outer_order_bytes)?;
    ensure!(
        semantic_sha256(&config)? == semantic_sha256(&outer_config.search)?,
        "aic-learning.hierarchical-search-config: collection config differs from the outer model search contract"
    );
    let outer_checkpoint = outer_training_output.join("state.safetensors");
    let outer_checkpoint_sha256 = crate::input::sha256(&outer_checkpoint)?;
    let outer_values: Tensors = Tensor::read_safetensors(&outer_checkpoint)?
        .into_iter()
        .collect();
    let outer = OuterModel::load(&outer_config.model, &outer_order, &outer_values, device)?;
    let loaded = aic_layout_gen::io::load_catalogs(catalog_root)?;
    let catalogs = Arc::new(loaded.catalogs);
    crate::input::verify_training_artifact(
        lower_training_output,
        &catalogs.identity,
        &checkpoint_sha256,
    )?;
    crate::input::verify_training_artifact(
        outer_training_output,
        &catalogs.identity,
        &outer_checkpoint_sha256,
    )?;
    let factory = Factory::new(
        &guidance,
        &checkpoint_sha256,
        device,
        DomainConfig {
            placement: config.bounds,
            extension_lengths: config.extension_lengths.clone(),
        },
        &config.candidate,
        config.repair_max_removed,
        config.placement_page,
        config.cpu_threads,
        config.anchor_prior_mass,
        config.compiled_inference,
        Some(&outer),
    )?;
    let mut cases = Vec::with_capacity(panel.value.cases.len());
    for case in &panel.value.cases {
        let root = prefix_session(case, &panel.value, catalogs.clone())?;
        let beam = config
            .run_beam
            .then(|| beam_search(root.clone(), &factory, &config))
            .transpose()?;
        let mut mcts = mcts_search(root, &factory, &config, &case.id)?;
        if !retain_root_records {
            mcts.root_records.clear();
        }
        cases.push(CaseReport {
            id: case.id.clone(),
            prefix_commands: case.prefix.len(),
            beam,
            mcts,
        });
    }
    let report = Report {
        schema: REPORT_SCHEMA.into(),
        status: "complete".into(),
        partition: panel.value.partition.clone(),
        device: format!("{device:?}"),
        search_config_sha256: semantic_sha256(&config)?,
        panel_sha256: panel.sha256.clone(),
        catalog_sha256: catalogs.identity.clone(),
        lower_config_sha256: bytes_sha256(&lower_config_bytes),
        lower_parameter_order_sha256: bytes_sha256(&lower_order_bytes),
        lower_checkpoint_sha256: checkpoint_sha256,
        outer_config_sha256: bytes_sha256(&outer_config_bytes),
        outer_parameter_order_sha256: bytes_sha256(&outer_order_bytes),
        outer_checkpoint_sha256,
        root_records_retained: retain_root_records,
        cases,
    };
    publish_json(output, report)
}

fn report_summary(output: &Path, report: &Report) -> Value {
    let run_beam = report.cases.iter().any(|case| case.beam.is_some());
    json!({
        "schema": REPORT_SCHEMA,
        "status": "complete",
        "output": output,
        "cases": report.cases.len(),
        "beam_completed": run_beam.then(|| report.cases.iter().filter(|case| {
            case.beam.as_ref().is_some_and(|result| result.fully_validated)
        }).count()),
        "mcts_completed": report.cases.iter().filter(|case| case.mcts.fully_validated).count(),
        "beam_samples_per_minute": run_beam.then(|| aggregate_throughput(
            report.cases.iter().filter_map(|case| case.beam.as_ref())
        )),
        "mcts_samples_per_minute": aggregate_throughput(report.cases.iter().map(|case| &case.mcts)),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn prepare_panel(
    sampling_path: &Path,
    catalog_root: &Path,
    output: &Path,
    partition: &str,
    cases: usize,
    seed: u64,
    namespace: &str,
    min_facilities: usize,
    max_facilities: usize,
    max_terminals: usize,
    max_connections: usize,
    max_turns: u64,
    max_extension: i64,
) -> Result<Value> {
    let published = prepare_panel_published(
        sampling_path,
        catalog_root,
        output,
        partition,
        cases,
        seed,
        namespace,
        min_facilities,
        max_facilities,
        max_terminals,
        max_connections,
        max_turns,
        max_extension,
    )?;
    let sampling_stats = published
        .value
        .preparation
        .as_ref()
        .map(|preparation| preparation.sampling_stats.clone());
    Ok(json!({
        "schema": PANEL_SCHEMA,
        "status": "complete",
        "output": output,
        "partition": partition,
        "cases": published.value.cases.len(),
        "min_facilities": min_facilities,
        "max_facilities": max_facilities,
        "max_terminals": max_terminals,
        "max_connections": max_connections,
        "sampling_stats": sampling_stats,
    }))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn prepare_panel_published(
    sampling_path: &Path,
    catalog_root: &Path,
    output: &Path,
    partition: &str,
    cases: usize,
    seed: u64,
    namespace: &str,
    min_facilities: usize,
    max_facilities: usize,
    max_terminals: usize,
    max_connections: usize,
    max_turns: u64,
    max_extension: i64,
) -> Result<Published<Panel>> {
    ensure!(
        !output.exists(),
        "aic-learning.hierarchical-panel-output: {} already exists",
        output.display()
    );
    ensure!(
        matches!(partition, "training" | "held-out")
            && (1..=4096).contains(&cases)
            && !namespace.is_empty()
            && max_terminals > 0
            && max_connections > 0
            && max_turns > 0
            && max_extension > 0,
        "aic-learning.hierarchical-panel-request: invalid partition, count, namespace or limits"
    );
    let source_partition = if partition == "training" {
        "train"
    } else {
        "validation"
    };
    let generator = ProblemGenerator::load(sampling_path, catalog_root)?;
    let sampled = generator.sample_facility_range(
        source_partition,
        cases,
        seed,
        namespace,
        min_facilities,
        max_facilities,
        max_terminals,
        max_connections,
    )?;
    let sampling_stats = serde_json::to_value(&sampled.stats)?;
    let panel = Panel {
        schema: PANEL_SCHEMA.into(),
        partition: partition.into(),
        limits: Limits {
            max_turns,
            max_extension,
        },
        preparation: Some(PanelPreparation {
            sampling_sha256: generator.sampling_sha256.clone(),
            seed,
            namespace: namespace.into(),
            source_partition: source_partition.into(),
            min_facilities,
            max_facilities,
            max_terminals,
            max_connections,
            sampling_stats: sampling_stats.clone(),
        }),
        cases: sampled
            .problems
            .into_iter()
            .map(|prepared| Case {
                id: prepared.id,
                problem: prepared.problem,
                prefix: Vec::new(),
            })
            .collect(),
    };
    validate_panel(&panel)?;
    publish_json(output, panel)
}

fn beam_search(root: Session, factory: &Factory<'_>, config: &Config) -> Result<SearchResult> {
    let started = Instant::now();
    let mut counters = Counters::default();
    let mut beam = vec![(root, Vec::<ActionSummary>::new())];
    let mut best = beam[0].clone();
    let mut termination = SearchTermination::MoveBudgetExhausted;
    let mut no_macro_diagnostic = None;
    for _ in 0..config.max_moves {
        let mut next = Vec::new();
        let mut exhausted_diagnostic = None;
        for (session, trace) in &beam {
            if session.layout().is_some() {
                termination = SearchTermination::FullyValidated;
                best = (session.clone(), trace.clone());
                beam = vec![best.clone()];
                break;
            }
            let batch = factory.action_batch(session, &mut counters, false)?;
            if batch.actions.is_empty() {
                let specific = batch
                    .no_macro_diagnostic
                    .as_ref()
                    .is_some_and(|diagnostic| {
                        diagnostic.code == "hierarchical.no-compatible-endpoints"
                    });
                let prior_specific =
                    exhausted_diagnostic
                        .as_ref()
                        .is_some_and(|diagnostic: &MacroDiagnostic| {
                            diagnostic.code == "hierarchical.no-compatible-endpoints"
                        });
                if exhausted_diagnostic.is_none() || (specific && !prior_specific) {
                    exhausted_diagnostic = batch.no_macro_diagnostic;
                }
            }
            for action in batch.actions {
                let child = action.apply(session, &config.candidate, config.repair_max_removed)?;
                let mut child_trace = trace.clone();
                child_trace.push(action.summary(session.revision()));
                if state_value(&child, config.bounds) > state_value(&best.0, config.bounds) {
                    best = (child.clone(), child_trace.clone());
                }
                next.push((child, child_trace));
            }
        }
        if beam
            .first()
            .is_some_and(|(session, _)| session.layout().is_some())
        {
            break;
        }
        if next.is_empty() {
            termination = SearchTermination::NoMacroActions;
            no_macro_diagnostic = exhausted_diagnostic;
            break;
        }
        next.sort_by(|left, right| {
            state_value(&right.0, config.bounds).total_cmp(&state_value(&left.0, config.bounds))
        });
        let mut identities = HashSet::new();
        next.retain(|(session, _)| {
            identities.insert(serde_json::to_string(&session.observe()).unwrap())
        });
        next.truncate(config.beam_width);
        beam = next;
        if let Some(completed) = beam.iter().find(|(session, _)| session.layout().is_some()) {
            best = completed.clone();
            termination = SearchTermination::FullyValidated;
            break;
        }
    }
    if let Some(completed) = beam.iter().find(|(session, _)| session.layout().is_some()) {
        best = completed.clone();
        termination = SearchTermination::FullyValidated;
    }
    Ok(finish_result(
        "beam",
        best.0,
        best.1,
        Vec::new(),
        0,
        counters,
        termination,
        no_macro_diagnostic,
        started,
    ))
}

struct Edge {
    action: MacroAction,
    prior: f64,
    anchor_prior: f64,
    visits: u32,
    value_sum: f64,
    child: Option<usize>,
}

struct Node {
    session: Session,
    visits: u32,
    value: Option<f64>,
    actions: Option<Vec<Edge>>,
    placement_pages: Option<PlacementPages>,
    no_macro_diagnostic: Option<MacroDiagnostic>,
}

fn mcts_search(
    mut working: Session,
    factory: &Factory<'_>,
    config: &Config,
    case_id: &str,
) -> Result<SearchResult> {
    let started = Instant::now();
    let mut counters = Counters::default();
    let mut trace = Vec::new();
    let mut root_records = Vec::new();
    let mut simulations = 0usize;
    let mut termination = SearchTermination::MoveBudgetExhausted;
    let mut no_macro_diagnostic = None;
    for _ in 0..config.max_moves {
        if working.layout().is_some() {
            termination = SearchTermination::FullyValidated;
            break;
        }
        let mut nodes = vec![Node {
            session: working.clone(),
            visits: 0,
            value: None,
            actions: None,
            placement_pages: None,
            no_macro_diagnostic: None,
        }];
        let state_identity = state_sha256(&working)?;
        let (selected, policy_targets) = match &config.root_selection {
            RootSelection::Puct => {
                for _ in 0..config.simulations_per_move {
                    simulate(&mut nodes, 0, None, factory, config, &mut counters)?;
                    simulations += 1;
                }
                ensure_node_expanded(&mut nodes, 0, factory, &mut counters, false)?;
                let selected = nodes[0]
                    .actions
                    .as_ref()
                    .expect("expanded root")
                    .iter()
                    .enumerate()
                    .max_by(|left, right| {
                        left.1
                            .visits
                            .cmp(&right.1.visits)
                            .then_with(|| left.1.prior.total_cmp(&right.1.prior))
                    })
                    .map(|(index, _)| index);
                (selected, None)
            }
            RootSelection::GumbelSequentialHalving {
                candidates,
                anchor_candidates,
                anchor_policy_mass,
                seed,
                value_scale,
                max_visit_init,
                gumbel_assignment,
            } => {
                ensure_node_expanded(&mut nodes, 0, factory, &mut counters, true)?;
                let decision = gumbel_sequential_halving(
                    &mut nodes,
                    0,
                    factory,
                    config,
                    &mut counters,
                    GumbelControls {
                        candidates: *candidates,
                        anchor_candidates: *anchor_candidates,
                        anchor_policy_mass: *anchor_policy_mass,
                        seed: *seed,
                        value_scale: *value_scale,
                        max_visit_init: *max_visit_init,
                        gumbel_assignment: *gumbel_assignment,
                    },
                    case_id,
                    &state_identity,
                )?;
                simulations += decision.simulations;
                (decision.selected, decision.policy_targets)
            }
        };
        let Some(selected) = selected else {
            termination = SearchTermination::NoMacroActions;
            no_macro_diagnostic = nodes[0].no_macro_diagnostic.clone();
            break;
        };
        let actions = nodes[0].actions.as_ref().expect("expanded root");
        let action = actions[selected].action.clone();
        let revision = working.revision();
        root_records.push(RootRecord {
            state_sha256: state_identity,
            state_revision: revision,
            actions: actions
                .iter()
                .enumerate()
                .map(|(index, edge)| RootActionRecord {
                    action: edge.action.clone(),
                    prior: edge.prior,
                    anchor_prior: edge.anchor_prior,
                    visits: edge.visits,
                    q: if edge.visits == 0 {
                        0.0
                    } else {
                        edge.value_sum / f64::from(edge.visits)
                    },
                    policy_target: policy_targets.as_ref().map(|targets| targets[index]),
                })
                .collect(),
            selected_action: selected,
            later_completed_connections: 0,
            later_total_connections: 0,
            later_footprint_area: 0,
            later_transport_cells: 0,
            later_turns: 0,
            later_fully_validated: false,
            later_termination: SearchTermination::MoveBudgetExhausted,
        });
        working = action.apply(&working, &config.candidate, config.repair_max_removed)?;
        trace.push(action.summary(revision));
    }
    let final_observation = working.observe();
    if final_observation.fully_validated {
        termination = SearchTermination::FullyValidated;
    }
    for record in &mut root_records {
        record.later_completed_connections = final_observation.completed;
        record.later_total_connections = final_observation.total;
        record.later_footprint_area = working.metrics().map_or(0, |metrics| metrics.area);
        record.later_transport_cells = working
            .metrics()
            .map_or(0, |metrics| metrics.transport_cells);
        record.later_turns = working.metrics().map_or(0, |metrics| metrics.turns);
        record.later_fully_validated = final_observation.fully_validated;
        record.later_termination = termination;
    }
    Ok(finish_result(
        match &config.root_selection {
            RootSelection::Puct => "mcts",
            RootSelection::GumbelSequentialHalving { .. } => "gumbel-mcts",
        },
        working,
        trace,
        root_records,
        simulations,
        counters,
        termination,
        no_macro_diagnostic,
        started,
    ))
}

#[derive(Clone, Copy)]
struct GumbelControls {
    candidates: usize,
    anchor_candidates: usize,
    anchor_policy_mass: f64,
    seed: u64,
    value_scale: f64,
    max_visit_init: f64,
    gumbel_assignment: GumbelAssignment,
}

struct RootDecision {
    selected: Option<usize>,
    policy_targets: Option<Vec<f64>>,
    simulations: usize,
}

#[allow(clippy::too_many_arguments)]
fn gumbel_sequential_halving(
    nodes: &mut Vec<Node>,
    root: usize,
    factory: &Factory<'_>,
    config: &Config,
    counters: &mut Counters,
    controls: GumbelControls,
    case_id: &str,
    state_identity: &str,
) -> Result<RootDecision> {
    let actions = nodes[root].actions.as_ref().expect("expanded root");
    if actions.is_empty() {
        return Ok(RootDecision {
            selected: None,
            policy_targets: None,
            simulations: 0,
        });
    }
    let gumbels = match controls.gumbel_assignment {
        GumbelAssignment::Rank => {
            deterministic_gumbels(controls.seed, case_id, state_identity, actions.len())
        }
        GumbelAssignment::ActionIdentity => {
            deterministic_action_gumbels(controls.seed, state_identity, actions)?
        }
    };
    let mut remaining = considered_root_actions(
        actions,
        &gumbels,
        controls.candidates,
        controls.anchor_candidates,
        controls.anchor_policy_mass,
    );
    let mut simulations = 0usize;
    while remaining.len() > 1 {
        let rounds_left =
            remaining.len().ilog2() as usize + usize::from(!remaining.len().is_power_of_two());
        let available = config.simulations_per_move - simulations;
        let visits_each = (available / (remaining.len() * rounds_left)).max(1);
        for edge in remaining.iter().copied() {
            for _ in 0..visits_each {
                simulate(nodes, root, Some(edge), factory, config, counters)?;
                simulations += 1;
            }
        }
        let transformed =
            completed_q_transform(nodes, root, controls.value_scale, controls.max_visit_init);
        remaining.sort_by(|left, right| {
            root_rank_score(
                &nodes[root].actions.as_ref().unwrap()[*right],
                gumbels[*right],
                transformed[*right],
                controls.anchor_policy_mass,
            )
            .total_cmp(&root_rank_score(
                &nodes[root].actions.as_ref().unwrap()[*left],
                gumbels[*left],
                transformed[*left],
                controls.anchor_policy_mass,
            ))
            .then_with(|| left.cmp(right))
        });
        remaining.truncate(remaining.len().div_ceil(2));
    }
    let selected = remaining[0];
    while simulations < config.simulations_per_move {
        simulate(nodes, root, Some(selected), factory, config, counters)?;
        simulations += 1;
    }
    let transformed =
        completed_q_transform(nodes, root, controls.value_scale, controls.max_visit_init);
    let policy_targets = improved_policy(
        nodes[root].actions.as_ref().unwrap(),
        &transformed,
        controls.anchor_policy_mass,
    );
    Ok(RootDecision {
        selected: Some(selected),
        policy_targets: Some(policy_targets),
        simulations,
    })
}

fn considered_root_actions(
    actions: &[Edge],
    gumbels: &[f64],
    requested: usize,
    requested_anchors: usize,
    anchor_policy_mass: f64,
) -> Vec<usize> {
    let count = requested.min(actions.len());
    let anchor_count = requested_anchors.min(count);
    let mut by_anchor = (0..actions.len()).collect::<Vec<_>>();
    by_anchor.sort_by(|left, right| {
        actions[*right]
            .anchor_prior
            .total_cmp(&actions[*left].anchor_prior)
            .then_with(|| left.cmp(right))
    });
    let mut selected = by_anchor.into_iter().take(anchor_count).collect::<Vec<_>>();
    let anchor_indices = selected.iter().copied().collect::<HashSet<_>>();
    let mut by_gumbel = (0..actions.len())
        .filter(|index| !anchor_indices.contains(index))
        .collect::<Vec<_>>();
    by_gumbel.sort_by(|left, right| {
        root_prior_score(&actions[*right], gumbels[*right], anchor_policy_mass)
            .total_cmp(&root_prior_score(
                &actions[*left],
                gumbels[*left],
                anchor_policy_mass,
            ))
            .then_with(|| left.cmp(right))
    });
    selected.extend(by_gumbel.into_iter().take(count - selected.len()));
    selected.sort_unstable();
    selected
}

fn root_policy_prior(edge: &Edge, anchor_policy_mass: f64) -> f64 {
    edge.prior * (1.0 - anchor_policy_mass) + edge.anchor_prior * anchor_policy_mass
}

fn root_prior_score(edge: &Edge, gumbel: f64, anchor_policy_mass: f64) -> f64 {
    gumbel
        + root_policy_prior(edge, anchor_policy_mass)
            .max(f64::MIN_POSITIVE)
            .ln()
}

fn root_rank_score(edge: &Edge, gumbel: f64, transformed_q: f64, anchor_policy_mass: f64) -> f64 {
    root_prior_score(edge, gumbel, anchor_policy_mass) + transformed_q
}

fn completed_q_transform(
    nodes: &[Node],
    root: usize,
    value_scale: f64,
    max_visit_init: f64,
) -> Vec<f64> {
    let actions = nodes[root].actions.as_ref().expect("expanded root");
    let root_value = nodes[root].value.unwrap_or_default();
    let total_visits = actions
        .iter()
        .map(|edge| u64::from(edge.visits))
        .sum::<u64>();
    let total_value = actions.iter().map(|edge| edge.value_sum).sum::<f64>();
    let mixed_value = (root_value + total_value) / (1.0 + total_visits as f64);
    let completed = actions
        .iter()
        .map(|edge| {
            if edge.visits == 0 {
                mixed_value
            } else {
                edge.value_sum / f64::from(edge.visits)
            }
        })
        .collect::<Vec<_>>();
    let minimum = completed.iter().copied().fold(root_value, f64::min);
    let maximum = completed.iter().copied().fold(root_value, f64::max);
    let denominator = maximum - minimum;
    let max_visits = actions.iter().map(|edge| edge.visits).max().unwrap_or(0);
    let scale = (max_visit_init + f64::from(max_visits)) * value_scale;
    completed
        .into_iter()
        .map(|q| {
            if denominator <= 1e-8 {
                0.0
            } else {
                (q - minimum) / denominator * scale
            }
        })
        .collect()
}

fn improved_policy(actions: &[Edge], transformed_q: &[f64], anchor_policy_mass: f64) -> Vec<f64> {
    let logits = actions
        .iter()
        .zip(transformed_q)
        .map(|(edge, q)| {
            root_policy_prior(edge, anchor_policy_mass)
                .max(f64::MIN_POSITIVE)
                .ln()
                + q
        })
        .collect::<Vec<_>>();
    let maximum = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let weights = logits
        .into_iter()
        .map(|logit| (logit - maximum).exp())
        .collect::<Vec<_>>();
    let denominator = weights.iter().sum::<f64>();
    weights
        .into_iter()
        .map(|weight| weight / denominator.max(f64::MIN_POSITIVE))
        .collect()
}

fn deterministic_gumbels(seed: u64, case_id: &str, state_identity: &str, count: usize) -> Vec<f64> {
    let mut digest = Sha256::new();
    digest.update(seed.to_le_bytes());
    digest.update([0]);
    digest.update(case_id.as_bytes());
    digest.update([0]);
    digest.update(state_identity.as_bytes());
    let bytes = digest.finalize();
    let mut state = u64::from_le_bytes(bytes[..8].try_into().expect("SHA-256 prefix"));
    (0..count)
        .map(|_| gumbel_from_bits(splitmix64(&mut state)))
        .collect()
}

fn deterministic_action_gumbels(
    seed: u64,
    state_identity: &str,
    actions: &[Edge],
) -> Result<Vec<f64>> {
    let mut prefix = Sha256::new();
    prefix.update(seed.to_le_bytes());
    prefix.update([0]);
    // State identity already hashes the problem and observation. Case IDs are
    // sampling metadata, so an identical physical state keeps the same noise.
    prefix.update(state_identity.as_bytes());
    prefix.update([0]);
    actions
        .iter()
        .map(|edge| {
            let mut digest = prefix.clone();
            digest.update(physical_action_identity(&edge.action)?);
            let bytes = digest.finalize();
            let bits = u64::from_le_bytes(bytes[..8].try_into().expect("SHA-256 prefix"));
            Ok(gumbel_from_bits(bits))
        })
        .collect()
}

fn physical_action_identity(action: &MacroAction) -> Result<Vec<u8>> {
    match action {
        MacroAction::Command(command) => Ok(serde_json::to_vec(&("command", command))?),
        // A route's command witness determines its physical effect in this
        // state. Candidate IDs, search work and model predictions can vary
        // without changing that witness.
        MacroAction::Route(candidate) => Ok(serde_json::to_vec(&("route", &candidate.commands))?),
        MacroAction::Network(candidate) => {
            let commands = candidate
                .routes
                .iter()
                .map(|route| &route.commands)
                .collect::<Vec<_>>();
            Ok(serde_json::to_vec(&("network", commands))?)
        }
        MacroAction::Repair { request, candidate } => {
            let commands = candidate
                .replacements
                .iter()
                .map(|route| &route.commands)
                .collect::<Vec<_>>();
            Ok(serde_json::to_vec(&(
                "repair",
                &request.state_sha256,
                &candidate.removed,
                commands,
            ))?)
        }
    }
}

fn gumbel_from_bits(bits: u64) -> f64 {
    let uniform = ((bits >> 11) as f64 + 0.5) / ((1u64 << 53) as f64);
    -(-uniform.ln()).ln()
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut value = *state;
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn minimum_sequential_halving_simulations(mut candidates: usize) -> usize {
    let mut simulations = 0usize;
    while candidates > 1 {
        simulations += candidates;
        candidates = candidates.div_ceil(2);
    }
    simulations
}

fn simulate(
    nodes: &mut Vec<Node>,
    root: usize,
    forced_root_edge: Option<usize>,
    factory: &Factory<'_>,
    config: &Config,
    counters: &mut Counters,
) -> Result<()> {
    let mut current = root;
    let mut path = Vec::<(usize, usize)>::new();
    let value = loop {
        if let Some(value) = simulation_leaf_value(&nodes[current].session, path.len(), config) {
            break value;
        }
        if nodes[current].actions.is_none() {
            break ensure_node_expanded(nodes, current, factory, counters, false)?;
        }
        maybe_widen_placements(nodes, current, factory, counters)?;
        let Some(actions) = nodes[current]
            .actions
            .as_ref()
            .filter(|actions| !actions.is_empty())
        else {
            break state_value(&nodes[current].session, config.bounds);
        };
        let edge = if current == root && path.is_empty() {
            forced_root_edge.unwrap_or_else(|| select_puct_edge(nodes, current, actions, config))
        } else {
            select_puct_edge(nodes, current, actions, config)
        };
        path.push((current, edge));
        if let Some(child) = nodes[current].actions.as_ref().unwrap()[edge].child {
            current = child;
            continue;
        }
        let child_session = nodes[current].actions.as_ref().unwrap()[edge]
            .action
            .apply(
                &nodes[current].session,
                &config.candidate,
                config.repair_max_removed,
            )?;
        let child = nodes.len();
        nodes.push(Node {
            session: child_session,
            visits: 0,
            value: None,
            actions: None,
            placement_pages: None,
            no_macro_diagnostic: None,
        });
        nodes[current].actions.as_mut().unwrap()[edge].child = Some(child);
        current = child;
        break ensure_node_expanded(nodes, current, factory, counters, false)?;
    };
    nodes[current].visits += 1;
    for (node, edge) in path.into_iter().rev() {
        nodes[node].visits += 1;
        let edge = &mut nodes[node].actions.as_mut().unwrap()[edge];
        edge.visits += 1;
        edge.value_sum += value;
    }
    Ok(())
}

fn select_puct_edge(nodes: &[Node], node: usize, actions: &[Edge], config: &Config) -> usize {
    let parent_visits = nodes[node].visits.max(1) as f64;
    // Actions are stored in descending prior order. Progressive widening keeps
    // large placement pages from consuming all internal-tree simulations.
    let widened = (parent_visits.sqrt().ceil() as usize).clamp(1, actions.len());
    actions[..widened]
        .iter()
        .enumerate()
        .max_by(|left, right| {
            let score = |edge: &Edge| {
                let q = if edge.visits == 0 {
                    nodes[node].value.unwrap_or_default()
                } else {
                    edge.value_sum / f64::from(edge.visits)
                };
                q + config.exploration * edge.prior * parent_visits.sqrt()
                    / (1.0 + f64::from(edge.visits))
            };
            score(left.1).total_cmp(&score(right.1))
        })
        .map(|(index, _)| index)
        .expect("nonempty actions")
}

fn simulation_leaf_value(session: &Session, depth: usize, config: &Config) -> Option<f64> {
    (session.layout().is_some() || depth == config.horizon)
        .then(|| state_value(session, config.bounds))
}

fn ensure_node_expanded(
    nodes: &mut [Node],
    node: usize,
    factory: &Factory<'_>,
    counters: &mut Counters,
    exhaust_placements: bool,
) -> Result<f64> {
    if nodes[node].actions.is_some() {
        return Ok(nodes[node]
            .value
            .unwrap_or_else(|| state_value(&nodes[node].session, factory.domain.placement)));
    }
    let batch = factory.action_batch(&nodes[node].session, counters, exhaust_placements)?;
    nodes[node].no_macro_diagnostic = batch.no_macro_diagnostic;
    let actions = batch.actions;
    nodes[node].placement_pages = Some(batch.placement_pages);
    if actions.is_empty() {
        let value = state_value(&nodes[node].session, factory.domain.placement);
        nodes[node].value = Some(value);
        nodes[node].actions = Some(vec![]);
        return Ok(value);
    }
    let children = actions
        .iter()
        .map(|action| {
            action.apply(
                &nodes[node].session,
                factory.candidate,
                factory.repair_max_removed,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let scores = factory.score_actions(&nodes[node].session, &actions, &children)?;
    // Expansion already paid for and natively applied every exposed macro.
    // Preserve that exact one-step evidence. The current outer checkpoint was
    // trained on fully placed routing states, so its completion head is not a
    // valid value estimate while any facility or terminal remains unplaced.
    let exact_value = children
        .iter()
        .map(|session| state_value(session, factory.domain.placement))
        .fold(
            state_value(&nodes[node].session, factory.domain.placement),
            f64::max,
        );
    let observation = nodes[node].session.observe();
    let all_entities_placed = observation.poses.iter().all(Option::is_some)
        && observation.terminal_poses.iter().all(Option::is_some);
    let value = if all_entities_placed {
        scores.model_value.max(exact_value)
    } else {
        exact_value
    };
    let mut edges = actions
        .into_iter()
        .zip(scores.priors)
        .zip(scores.anchor_priors)
        .map(|((action, prior), anchor_prior)| Edge {
            action,
            prior,
            anchor_prior,
            visits: 0,
            value_sum: 0.0,
            child: None,
        })
        .collect::<Vec<_>>();
    edges.sort_by(|left, right| right.prior.total_cmp(&left.prior));
    nodes[node].actions = Some(edges);
    nodes[node].value = Some(value);
    Ok(value)
}

fn maybe_widen_placements(
    nodes: &mut [Node],
    node: usize,
    factory: &Factory<'_>,
    counters: &mut Counters,
) -> Result<()> {
    let desired_pages = (f64::from(nodes[node].visits.max(1)).sqrt().floor() as usize).max(1);
    let Some(mut pages) = nodes[node].placement_pages else {
        return Ok(());
    };
    if desired_pages <= pages.loaded_pages || pages.exhausted.into_iter().all(|exhausted| exhausted)
    {
        return Ok(());
    }
    let session = nodes[node].session.clone();
    let additional = factory.next_placement_page(&session, &mut pages, counters)?;
    nodes[node].placement_pages = Some(pages);
    if additional.is_empty() {
        return Ok(());
    }
    let existing = nodes[node].actions.as_ref().expect("expanded node");
    let mut actions = existing
        .iter()
        .map(|edge| edge.action.clone())
        .collect::<Vec<_>>();
    actions.extend(additional.iter().cloned());
    let children = actions
        .iter()
        .map(|action| action.apply(&session, factory.candidate, factory.repair_max_removed))
        .collect::<Result<Vec<_>>>()?;
    let scores = factory.score_actions(&session, &actions, &children)?;
    let existing_count = existing.len();
    let edges = nodes[node].actions.as_mut().expect("expanded node");
    for ((edge, prior), anchor_prior) in edges
        .iter_mut()
        .zip(&scores.priors)
        .zip(&scores.anchor_priors)
    {
        edge.prior = *prior;
        edge.anchor_prior = *anchor_prior;
    }
    edges.extend(
        additional
            .into_iter()
            .zip(scores.priors.into_iter().skip(existing_count))
            .zip(scores.anchor_priors.into_iter().skip(existing_count))
            .map(|((action, prior), anchor_prior)| Edge {
                action,
                prior,
                anchor_prior,
                visits: 0,
                value_sum: 0.0,
                child: None,
            }),
    );
    edges.sort_by(|left, right| right.prior.total_cmp(&left.prior));
    let exact_value = children
        .iter()
        .map(|child| state_value(child, factory.domain.placement))
        .fold(state_value(&session, factory.domain.placement), f64::max);
    let observation = session.observe();
    let all_entities_placed = observation.poses.iter().all(Option::is_some)
        && observation.terminal_poses.iter().all(Option::is_some);
    let widened_value = if all_entities_placed {
        scores.model_value.max(exact_value)
    } else {
        exact_value
    };
    nodes[node].value = Some(nodes[node].value.unwrap_or_default().max(widened_value));
    Ok(())
}

fn prefix_session(case: &Case, panel: &Panel, catalogs: Arc<Catalogs>) -> Result<Session> {
    let mut session =
        Session::new(case.problem.clone(), catalogs, panel.limits).map_err(anyhow::Error::new)?;
    for (index, command) in case.prefix.iter().enumerate() {
        let step = session.step(command.clone());
        ensure!(
            step.accepted && step.diagnostic.is_none(),
            "aic-learning.hierarchical-prefix: case {} command {} rejected: {:?}",
            case.id,
            index,
            step.diagnostic
        );
    }
    ensure!(
        session.observe().active.is_none(),
        "aic-learning.hierarchical-prefix: case {} left an active route",
        case.id
    );
    Ok(session)
}

fn construction_progress(session: &Session) -> f64 {
    let observation = session.observe();
    let complete = observation
        .poses
        .iter()
        .filter(|pose| pose.is_some())
        .count()
        + observation
            .terminal_poses
            .iter()
            .filter(|pose| pose.is_some())
            .count()
        + observation
            .routes
            .iter()
            .filter(|route| route.is_some())
            .count();
    let total =
        observation.poses.len() + observation.terminal_poses.len() + observation.routes.len();
    complete as f64 / total.max(1) as f64
}

fn lexicographic_quality_penalty(
    area: f64,
    transport: f64,
    turns: f64,
    bounds: Bounds,
    connections: usize,
) -> f64 {
    let canvas = (bounds.width() * bounds.height()).max(1) as f64;
    let routed = canvas * connections.max(1) as f64;
    (area + transport / (canvas + 1.0) + turns / ((canvas + 1.0) * (routed + 1.0))).clamp(0.0, 1.0)
}

fn predicted_quality_value(penalty: f64) -> f64 {
    0.5 + 0.5 * (1.0 - penalty.clamp(0.0, 1.0))
}

fn completed_quality_value(penalty: f64) -> f64 {
    1.0 + (1.0 - penalty.clamp(0.0, 1.0))
}

fn state_value(session: &Session, bounds: Bounds) -> f64 {
    if let Some(metrics) = session.metrics() {
        let quality = normalized_layout_quality(
            metrics.area,
            metrics.transport_cells,
            metrics.turns,
            bounds.width() * bounds.height(),
            session.problem().connections.len(),
        );
        completed_quality_value(lexicographic_quality_penalty(
            f64::from(quality[0]),
            f64::from(quality[1]),
            f64::from(quality[2]),
            bounds,
            session.problem().connections.len(),
        ))
    } else {
        construction_progress(session)
    }
}

fn finish_result(
    algorithm: &'static str,
    session: Session,
    trace: Vec<ActionSummary>,
    root_records: Vec<RootRecord>,
    simulations: usize,
    counters: Counters,
    termination: SearchTermination,
    no_macro_diagnostic: Option<MacroDiagnostic>,
    started: Instant,
) -> SearchResult {
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let observation = session.observe();
    let metrics = session.metrics();
    SearchResult {
        algorithm: algorithm.into(),
        status: if observation.fully_validated {
            "complete"
        } else {
            "incomplete"
        }
        .into(),
        fully_validated: observation.fully_validated,
        termination,
        no_macro_diagnostic: (termination == SearchTermination::NoMacroActions)
            .then_some(no_macro_diagnostic)
            .flatten(),
        completed_connections: observation.completed,
        total_connections: observation.total,
        footprint_area: metrics.map(|metrics| metrics.area),
        transport_cells: metrics.map(|metrics| metrics.transport_cells),
        turns: metrics.map(|metrics| metrics.turns),
        moves: trace.len(),
        simulations,
        elapsed_seconds,
        samples_per_minute: if elapsed_seconds == 0.0 {
            0.0
        } else {
            counters.generated_macro_samples as f64 * 60.0 / elapsed_seconds
        },
        counters,
        trace,
        root_records,
    }
}

fn state_sha256(session: &Session) -> Result<String> {
    let bytes = serde_json::to_vec(&(session.problem(), session.observe()))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn aggregate_throughput<'a>(results: impl Iterator<Item = &'a SearchResult>) -> f64 {
    let (samples, seconds) = results.fold((0usize, 0.0), |(samples, seconds), result| {
        (
            samples + result.counters.generated_macro_samples,
            seconds + result.elapsed_seconds,
        )
    });
    if seconds == 0.0 {
        0.0
    } else {
        samples as f64 * 60.0 / seconds
    }
}

fn validate_config(config: &Config) -> Result<()> {
    ensure!(
        config.schema == CONFIG_SCHEMA
            && config.bounds == config.candidate.bounds
            && config.repair_max_removed <= 32768
            && !config.extension_lengths.is_empty()
            && config
                .extension_lengths
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            && config.extension_lengths.iter().all(|length| *length > 0)
            && (1..=256).contains(&config.cpu_threads)
            && (1..=4096).contains(&config.placement_page)
            && (1..=4096).contains(&config.beam_width)
            && (2..=1_000_000).contains(&config.simulations_per_move)
            && (1..=4096).contains(&config.horizon)
            && (1..=4096).contains(&config.max_moves)
            && config.exploration.is_finite()
            && config.exploration > 0.0
            && config.anchor_prior_mass.is_finite()
            && (0.0..=1.0).contains(&config.anchor_prior_mass),
        "aic-learning.hierarchical-config: invalid schema or bounded search controls"
    );
    if let RootSelection::GumbelSequentialHalving {
        candidates,
        anchor_candidates,
        anchor_policy_mass,
        value_scale,
        max_visit_init,
        ..
    } = &config.root_selection
    {
        ensure!(
            (2..=256).contains(candidates)
                && *anchor_candidates <= *candidates
                && anchor_policy_mass.is_finite()
                && (0.0..=1.0).contains(anchor_policy_mass)
                && value_scale.is_finite()
                && *value_scale > 0.0
                && max_visit_init.is_finite()
                && *max_visit_init >= 0.0
                && config.simulations_per_move
                    >= minimum_sequential_halving_simulations(*candidates),
            "aic-learning.hierarchical-config: invalid Gumbel root controls"
        );
    }
    Ok(())
}

fn validate_panel(panel: &Panel) -> Result<()> {
    ensure!(
        panel.schema == PANEL_SCHEMA
            && matches!(panel.partition.as_str(), "training" | "held-out")
            && !panel.cases.is_empty()
            && panel.cases.len() <= 4096,
        "aic-learning.hierarchical-panel: invalid schema, partition or cases"
    );
    let mut ids = BTreeSet::new();
    ensure!(
        panel
            .cases
            .iter()
            .all(|case| !case.id.is_empty() && ids.insert(case.id.as_str())),
        "aic-learning.hierarchical-panel: duplicate or empty case ID"
    );
    Ok(())
}

fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    ensure!(
        metadata.len() <= MAX_INPUT_BYTES,
        "aic-learning.hierarchical-input: {} exceeds {} bytes",
        path.display(),
        MAX_INPUT_BYTES
    );
    fs::read(path).with_context(|| format!("read hierarchical input {}", path.display()))
}

fn bytes_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn semantic_sha256(value: &impl Serialize) -> Result<String> {
    Ok(bytes_sha256(&serde_json::to_vec(value)?))
}

fn publish_json<T: Serialize>(output: &Path, value: T) -> Result<Published<T>> {
    let (sha256, encoded_bytes) = publish_json_identity(output, &value)?;
    Ok(Published {
        value,
        sha256,
        encoded_bytes,
    })
}

fn publish_json_identity(output: &Path, value: &impl Serialize) -> Result<(String, u64)> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .context("aic-learning.hierarchical-output: output needs a UTF-8 name")?;
    let temporary = output.with_file_name(format!(".{name}.tmp"));
    let file = fs::File::create(&temporary)?;
    let mut writer = BufWriter::new(DigestWriter::new(file));
    if let Err(error) = serde_json::to_writer_pretty(&mut writer, value) {
        drop(writer);
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    writer.flush()?;
    let digested = writer.into_inner().map_err(|error| error.into_error())?;
    digested.inner.sync_all()?;
    let identity = digested.finish();
    fs::rename(temporary, output)?;
    Ok(identity)
}

pub(crate) fn load_published_panel(path: &Path) -> Result<Published<Panel>> {
    let metadata = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    ensure!(
        metadata.len() <= MAX_INPUT_BYTES,
        "aic-learning.hierarchical-input: {} exceeds {} bytes",
        path.display(),
        MAX_INPUT_BYTES
    );
    load_published_json(path)
}

pub(crate) fn load_published_report(path: &Path) -> Result<Published<Report>> {
    load_published_json(path)
}

fn load_published_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Published<T>> {
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut reader = DigestReader::new(BufReader::new(file));
    let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
    let value =
        T::deserialize(&mut deserializer).with_context(|| format!("parse {}", path.display()))?;
    deserializer
        .end()
        .with_context(|| format!("finish parsing {}", path.display()))?;
    let (sha256, encoded_bytes) = reader.finish();
    Ok(Published {
        value,
        sha256,
        encoded_bytes,
    })
}

struct DigestWriter<W> {
    inner: W,
    digest: Sha256,
    bytes: u64,
}

impl<W> DigestWriter<W> {
    fn new(inner: W) -> Self {
        Self {
            inner,
            digest: Sha256::new(),
            bytes: 0,
        }
    }

    fn finish(self) -> (String, u64) {
        (digest_hex(self.digest), self.bytes)
    }
}

impl<W: Write> Write for DigestWriter<W> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let written = self.inner.write(buffer)?;
        self.digest.update(&buffer[..written]);
        self.bytes += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

struct DigestReader<R> {
    inner: R,
    digest: Sha256,
    bytes: u64,
}

impl<R> DigestReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            digest: Sha256::new(),
            bytes: 0,
        }
    }

    fn finish(self) -> (String, u64) {
        (digest_hex(self.digest), self.bytes)
    }
}

impl<R: Read> Read for DigestReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(buffer)?;
        self.digest.update(&buffer[..read]);
        self.bytes += read as u64;
        Ok(read)
    }
}

fn digest_hex(digest: Sha256) -> String {
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_layout::geometry::Point;

    #[test]
    fn network_clone_page_shrinks_to_fit_live_memory() {
        let mut caps = aic_pilot_native::constructive::routing_candidates::AllowanceCaps::default();
        caps.peak_live_bytes = 15;
        let mut allowance = RequestAllowance::new(caps);
        let mut page_count = 2;
        let bytes = reserve_network_clone_page(&mut allowance, 10, &mut page_count).unwrap();
        assert_eq!((page_count, bytes), (1, 10));
        allowance.release_peak(bytes);
        assert_eq!(allowance.usage().peak_live_bytes, 10);
    }

    #[test]
    fn no_compatible_network_member_keeps_its_specific_diagnostic() {
        let allowance = RequestAllowance::new(Default::default());
        let counters = Counters {
            network_unknown: 1,
            route_no_compatible_endpoints: 1,
            route_no_compatible_connection: Some("output-b".into()),
            route_work_limit: 1,
            request_stops: vec![RequestStopRecord {
                state_revision: 7,
                connection: "output-c".into(),
                stop: allowance.exhausted_stop(WorkKind::Variants),
                usage: allowance.usage(),
            }],
            ..Counters::default()
        };
        let reason = no_macro_reason(&counters, (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0), 7);
        assert_eq!(reason.code, "hierarchical.no-compatible-endpoints");
        assert_eq!(reason.entity, "output-b");
    }

    #[test]
    fn request_stop_reason_survives_capped_telemetry() {
        let mut counters = Counters::default();
        let allowance = RequestAllowance::new(Default::default());
        for index in 0..=ROUTE_ATTEMPT_SAMPLE_LIMIT {
            record_request_stop(
                &mut counters,
                &Request {
                    state_revision: 7,
                    connection: format!("route-{index}"),
                },
                &allowance,
                allowance.exhausted_stop(WorkKind::EndpointVisits),
            );
        }
        assert_eq!(counters.request_stops.len(), ROUTE_ATTEMPT_SAMPLE_LIMIT);
        assert_eq!(counters.request_stops_total, ROUTE_ATTEMPT_SAMPLE_LIMIT + 1);
        let before = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
        let reason = no_macro_reason(&counters, before, 7);
        assert_eq!(reason.code, "hierarchical.request-work-limit");
        assert_eq!(reason.entity, format!("route-{ROUTE_ATTEMPT_SAMPLE_LIMIT}"));
        assert!(reason.message.contains("EndpointVisits"));

        let before = (
            counters.request_stops_total,
            counters.route_deadline,
            counters.route_work_limit,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        );
        record_request_stop(
            &mut counters,
            &Request {
                state_revision: 8,
                connection: "later-route".into(),
            },
            &allowance,
            allowance.exhausted_stop(WorkKind::GraphRows),
        );
        let reason = no_macro_reason(&counters, before, 8);
        assert_eq!(reason.entity, "later-route");
        assert!(reason.message.contains("GraphRows"));
    }

    #[test]
    fn repair_macro_replays_one_complete_native_transaction() -> Result<()> {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-repair/fixture.json"
        ))?;
        let mut session = Session::new(
            serde_json::from_value(fixture["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value(fixture["limits"].clone())?,
        )?;
        for command in fixture["prefix"].as_array().expect("fixture prefix") {
            assert!(
                session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        let original = session.observe();
        let config: CandidateConfig = serde_json::from_value(fixture["candidate"].clone())?;
        let request = session.repair_request(&["second".into()], &["first".into()], 1, &config)?;
        let mut allowance = RequestAllowance::new(request.allowance_caps.clone());
        let (mut draft, reserved_bytes) = session.prepare_repair_draft_with_allowance(
            &request,
            &["first".into()],
            &mut allowance,
        )?;
        allowance.release_peak(reserved_bytes);
        let next = draft
            .route_candidates(
                &Request {
                    state_revision: draft.revision(),
                    connection: "second".into(),
                },
                &config,
            )?
            .candidates
            .remove(0);
        draft.apply_route_candidate(&next)?;
        let restored = draft
            .route_candidates(
                &Request {
                    state_revision: draft.revision(),
                    connection: "first".into(),
                },
                &config,
            )?
            .candidates
            .remove(0);
        let action = MacroAction::Repair {
            request: request.clone(),
            candidate: RepairCandidate {
                schema: aic_pilot_native::constructive::routing_candidates::REPAIR_CANDIDATE_SCHEMA
                    .into(),
                state_revision: request.state_revision,
                removed: vec!["first".into()],
                replacements: vec![next, restored],
                expanded: 0,
            },
        };
        let child = action.apply(&session, &config, 1)?;
        assert!(child.observe().fully_validated);
        let mut tampered = action.clone();
        if let MacroAction::Repair { request, .. } = &mut tampered {
            request.max_removed = 2;
        }
        assert!(tampered.apply(&session, &config, 1).is_err());
        assert!(action.apply(&session, &config, 0).is_err());
        assert_eq!(session.observe(), original);
        let decoded: MacroAction = serde_json::from_value(serde_json::to_value(&action)?)?;
        assert_eq!(
            physical_action_identity(&decoded)?,
            physical_action_identity(&action)?
        );
        assert_eq!(
            action.summary(session.revision()).removed_connections,
            vec!["first"]
        );
        assert_eq!(action.features(&session, &child, config.bounds)[2], 2.0);
        Ok(())
    }

    #[test]
    fn zero_candidate_route_cutoffs_reach_outer_diagnostic() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let panel: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-route-candidates/panel.json"
        ))?;
        let case = &panel["cases"][0];
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value(panel["limits"].clone())?,
        )?;
        for command in case["prefix"].as_array().expect("fixture prefix") {
            assert!(
                session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        let model_config = crate::schema::RouteGuidanceModelConfig {
            schema: crate::schema::RouteGuidanceModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            raster_width: 8,
            heads: 2,
            maximum_edge_cost: 32,
        };
        let (order, state) = GuidanceModel::initial_state(&model_config, 17)?;
        let model = GuidanceModel::load(&model_config, &order, &state, Device::Cpu)?;
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 20,
            max_y: 20,
        };
        for (searches, expanded, expected) in [
            (1, 100, "hierarchical.route-search-limit"),
            (48, 1, "hierarchical.route-work-budget"),
        ] {
            let candidate = CandidateConfig {
                bounds,
                max_candidates: 2,
                max_searches: searches,
                work_per_search: 1,
                max_expanded: expanded,
                overlap_penalty: 8,
                max_crossings_per_route: 0,
                request_allowance: Default::default(),
            };
            let factory = Factory::new(
                &model,
                "test",
                Device::Cpu,
                DomainConfig {
                    placement: bounds,
                    extension_lengths: vec![1, 2, 4],
                },
                &candidate,
                0,
                4,
                1,
                0.0,
                false,
                None,
            )?;
            let mut counters = Counters::default();
            let batch = factory.action_batch(&session, &mut counters, false)?;
            assert!(batch.actions.is_empty());
            assert_eq!(batch.no_macro_diagnostic.unwrap().code, expected);
        }
        Ok(())
    }

    #[test]
    fn variant_limit_keeps_completed_network_candidate_with_a_larger_batch_page() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let panel: Panel = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-network/panel.json"
        ))?;
        let case = &panel.cases[0];
        let mut session = Session::new(
            case.problem.clone(),
            crate::constructive::test_support::catalogs(),
            panel.limits.clone(),
        )?;
        for command in &case.prefix {
            let step = session.step(command.clone());
            assert!(step.accepted, "{step:?}");
        }
        let model_config = crate::schema::RouteGuidanceModelConfig {
            schema: crate::schema::RouteGuidanceModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            raster_width: 8,
            heads: 2,
            maximum_edge_cost: 32,
        };
        let (order, state) = GuidanceModel::initial_state(&model_config, 17)?;
        let model = GuidanceModel::load(&model_config, &order, &state, Device::Cpu)?;
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 20,
            max_y: 20,
        };
        let mut candidate = CandidateConfig {
            bounds,
            max_candidates: 2,
            max_searches: 48,
            work_per_search: 20_000,
            max_expanded: 400_000,
            overlap_penalty: 8,
            max_crossings_per_route: 0,
            request_allowance: Default::default(),
        };
        candidate.request_allowance.max_variants = 1;
        candidate.request_allowance.max_inference_batch = 8;
        let factory = Factory::new(
            &model,
            "test",
            Device::Cpu,
            DomainConfig {
                placement: bounds,
                extension_lengths: vec![1, 2, 4],
            },
            &candidate,
            0,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut counters = Counters::default();
        let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let candidates = factory.network_actions(
            &session,
            &["output-a".into(), "output-b".into()],
            &mut counters,
            &mut allowance,
        )?;
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].routes[0].connection, "output-a");
        assert_eq!(counters.network_unknown, 1);
        assert_eq!(counters.request_stops.len(), 1);
        assert_eq!(
            counters.request_stops[0].stop.category,
            Some(WorkKind::Variants)
        );
        assert!(allowance.usage().peak_live_bytes > 0);
        let mut tiny_identity_caps = candidate.request_allowance.clone();
        tiny_identity_caps.routing_scans = 1;
        let mut tiny_allowance = RequestAllowance::new(tiny_identity_caps);
        let mut tiny_counters = Counters::default();
        let none = factory.network_actions(
            &session,
            &["output-a".into(), "output-b".into()],
            &mut tiny_counters,
            &mut tiny_allowance,
        )?;
        assert!(none.is_empty());
        assert_eq!(tiny_counters.network_unknown, 1);
        assert_eq!(
            tiny_counters.request_stops[0].stop.category,
            Some(WorkKind::RoutingScans)
        );
        let mut multi_caps = candidate.request_allowance.clone();
        multi_caps.max_variants = 2;
        let mut full_allowance = RequestAllowance::new(multi_caps.clone());
        let mut full_counters = Counters::default();
        let full = factory.network_actions(
            &session,
            &["output-a".into(), "output-b".into()],
            &mut full_counters,
            &mut full_allowance,
        )?;
        assert!(!full.is_empty());
        assert!(full_allowance.usage().peak_live_bytes > session.clone_reservation_bytes() * 2);
        multi_caps.peak_live_bytes = full_allowance.usage().peak_live_bytes - 1;
        let mut tight_allowance = RequestAllowance::new(multi_caps);
        let mut tight_counters = Counters::default();
        let tight = factory.network_actions(
            &session,
            &["output-a".into(), "output-b".into()],
            &mut tight_counters,
            &mut tight_allowance,
        )?;
        assert_eq!(
            tight, full,
            "smaller pages must preserve completed variants"
        );
        assert!(tight_counters.guidance_batches > full_counters.guidance_batches);
        assert_eq!(tight_counters.network_unknown, 0);
        assert!(tight_counters.request_stops.is_empty());
        assert_eq!(tight_counters.request_stops_total, 0);
        assert!(tight_counters.last_request_stop.is_none());
        assert!(tight_allowance.usage().peak_live_bytes <= tight_allowance.caps().peak_live_bytes);

        // The same network can be replayed while another obligation is still
        // open. No final layout is built in that case, so copied-byte work
        // must exclude the final layout copy.
        let mut incomplete = Session::new(
            case.problem.clone(),
            crate::constructive::test_support::catalogs(),
            panel.limits.clone(),
        )?;
        for command in case.prefix.iter().take(4) {
            assert!(incomplete.step(command.clone()).accepted);
        }
        let mut incomplete_counters = Counters::default();
        let mut incomplete_allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let incomplete_candidates = factory.network_actions(
            &incomplete,
            &["output-a".into(), "output-b".into()],
            &mut incomplete_counters,
            &mut incomplete_allowance,
        )?;
        let witness = &incomplete_candidates[0];
        let mut draft = incomplete.clone();
        let mut copied = incomplete.clone_reservation_bytes().saturating_mul(2);
        for route in &witness.routes {
            copied = copied.saturating_add(draft.clone_reservation_bytes());
            draft.apply_route_candidate(route)?;
        }
        assert!(draft.metrics().is_none());
        let mut exact_caps = candidate.request_allowance.clone();
        exact_caps.copied_bytes = copied;
        let mut exact_allowance = RequestAllowance::new(exact_caps);
        let mut replay = incomplete.clone();
        replay
            .apply_network_candidate_with_allowance(
                witness,
                draft.clone_reservation_bytes(),
                copied,
                false,
                &mut exact_allowance,
            )
            .expect("incomplete network replay fits exactly measured copy work");
        assert_eq!(
            exact_allowance.usage().categories[&WorkKind::CopiedBytes],
            copied
        );
        assert!(replay.metrics().is_none());

        let base_catalogs = crate::constructive::test_support::catalogs();
        let mut facility_catalog = base_catalogs.facilities.catalog().clone();
        let grinder = facility_catalog
            .facilities
            .iter_mut()
            .find(|facility| facility.id == "grinder-1-mode-normal")
            .expect("fixture grinder exists");
        let mut kept_output = false;
        grinder.ports.retain(|port| {
            if port.direction == aic_layout::facilities::FacilityPortDirection::Output
                && port.transport == aic_layout::logistics::TransportKind::Belt
            {
                if kept_output {
                    return false;
                }
                kept_output = true;
            }
            true
        });
        let constrained_catalogs = Arc::new(Catalogs {
            identity: "2".repeat(64),
            facilities: aic_layout::facilities::ValidatedFacilityCatalog::try_from_catalog(
                facility_catalog,
            )
            .expect("single output port remains valid"),
            transports: base_catalogs.transports.clone(),
            components: base_catalogs.components.clone(),
        });
        let mut incompatible_problem = case.problem.clone();
        let output_a = incompatible_problem
            .connections
            .iter_mut()
            .find(|connection| connection.id == "output-a")
            .expect("network member exists");
        output_a.rate = aic_layout::rate::Rate {
            numerator: 1,
            denominator: 2,
        };
        let mut output_c = incompatible_problem
            .connections
            .iter()
            .find(|connection| connection.id == "output-b")
            .expect("network member exists")
            .clone();
        output_c.id = "output-c".into();
        incompatible_problem.connections.push(output_c);
        let mut incompatible = Session::new(
            incompatible_problem,
            constrained_catalogs,
            panel.limits.clone(),
        )?;
        for command in &case.prefix {
            assert!(incompatible.step(command.clone()).accepted);
        }
        let prefill = incompatible.route_candidates(
            &Request {
                state_revision: incompatible.revision(),
                connection: "output-a".into(),
            },
            &candidate,
        )?;
        incompatible.apply_route_candidate(&prefill.candidates[0])?;
        let mut incompatible_counters = Counters::default();
        let batch = factory.action_batch(&incompatible, &mut incompatible_counters, false)?;
        assert!(batch.actions.is_empty());
        assert_eq!(
            incompatible_counters
                .route_no_compatible_connection
                .as_deref(),
            Some("output-b")
        );
        let diagnostic = batch.no_macro_diagnostic.expect("no macro reason");
        assert_eq!(diagnostic.code, "hierarchical.no-compatible-endpoints");
        assert_eq!(diagnostic.entity, "output-b");
        Ok(())
    }

    #[test]
    fn published_json_round_trips_with_exact_file_identity() {
        let path = std::env::temp_dir().join(format!(
            "aic-hierarchical-published-json-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let value = json!({"schema":"test-v1","values":[1,2,3]});
        let published = publish_json(&path, value.clone()).unwrap();
        let encoded = fs::read(&path).unwrap();
        assert_eq!(encoded, serde_json::to_vec_pretty(&value).unwrap());
        assert_eq!(published.sha256, bytes_sha256(&encoded));
        assert_eq!(published.encoded_bytes, encoded.len() as u64);

        let recovered: Published<Value> = load_published_json(&path).unwrap();
        assert_eq!(recovered.value, value);
        assert_eq!(recovered.sha256, published.sha256);
        assert_eq!(recovered.encoded_bytes, published.encoded_bytes);
        fs::remove_file(path).unwrap();
    }

    fn value(area: u64, transport: u64, turns: u64) -> f64 {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 20,
            max_y: 20,
        };
        let normalized = normalized_layout_quality(area, transport, turns, 441, 8);
        completed_quality_value(lexicographic_quality_penalty(
            f64::from(normalized[0]),
            f64::from(normalized[1]),
            f64::from(normalized[2]),
            bounds,
            8,
        ))
    }

    #[test]
    fn completed_quality_uses_area_transport_turn_order() {
        assert!(value(40, 3_528, 3_528) > value(41, 0, 0));
        assert!(value(40, 90, 3_528) > value(40, 91, 0));
        assert!(value(40, 90, 8) > value(40, 90, 9));
        assert!(value(441, 3_528, 3_528) > 1.0);
    }

    fn edge(prior: f64, anchor_prior: f64) -> Edge {
        Edge {
            action: MacroAction::Command(Command::Connect),
            prior,
            anchor_prior,
            visits: 0,
            value_sum: 0.0,
            child: None,
        }
    }

    #[test]
    fn adapted_gumbel_candidates_always_include_anchor_quota() {
        let actions = vec![
            edge(0.70, 0.01),
            edge(0.20, 0.02),
            edge(0.05, 0.50),
            edge(0.03, 0.30),
            edge(0.01, 0.10),
            edge(0.01, 0.07),
        ];
        let gumbels = vec![4.0, 3.0, -4.0, -3.0, 2.0, 1.0];
        let considered = considered_root_actions(&actions, &gumbels, 4, 2, 0.5);
        assert_eq!(considered.len(), 4);
        assert!(considered.contains(&2));
        assert!(considered.contains(&3));
    }

    #[test]
    fn gumbels_and_improved_policy_are_deterministic_and_normalized() {
        let first = deterministic_gumbels(7, "case-a", "state-a", 8);
        assert_eq!(first, deterministic_gumbels(7, "case-a", "state-a", 8));
        assert_ne!(first, deterministic_gumbels(7, "case-b", "state-a", 8));
        assert!(first.iter().all(|value| value.is_finite()));

        let command_edge = |command| Edge {
            action: MacroAction::Command(command),
            prior: 0.5,
            anchor_prior: 0.5,
            visits: 0,
            value_sum: 0.0,
            child: None,
        };
        let forward = [
            command_edge(Command::Connect),
            command_edge(Command::AbortRoute),
        ];
        let reverse = [
            command_edge(Command::AbortRoute),
            command_edge(Command::Connect),
        ];
        let keyed_forward = deterministic_action_gumbels(7, "state-a", &forward).unwrap();
        let keyed_reverse = deterministic_action_gumbels(7, "state-a", &reverse).unwrap();
        assert_eq!(keyed_forward, [keyed_reverse[1], keyed_reverse[0]]);
        assert_ne!(keyed_forward[0], keyed_forward[1]);

        let route = Candidate {
            id: 0,
            state_revision: 0,
            connection: "route-a".into(),
            source_port: None,
            target_port: None,
            cells: Vec::new(),
            commands: vec![Command::Connect],
            crossings: vec![],
            steps: 1,
            turns: 0,
            expanded: 10,
            overlap_with_previous: 0,
            learned_cost: 1,
            predicted_success: Some(0.25),
            guidance_uncertainty: Some(0.1),
            provenance: Provenance::Anchor,
        };
        let mut crossing_route = route.clone();
        crossing_route.commands.insert(
            0,
            Command::ReserveBridge {
                position: Point { x: 6, y: 6 },
                component: "log-connector".into(),
                rotation: 0,
                crossed_axis: BridgeAxis::Horizontal,
            },
        );
        assert_ne!(
            network_witness_key(&[route.clone(), route.clone()]),
            network_witness_key(&[route.clone(), crossing_route]),
        );
        let mut other_member = route.clone();
        other_member.connection = "route-b".into();
        assert_eq!(
            network_witness_key(&[route.clone(), other_member.clone()]),
            network_witness_key(&[other_member, route.clone()]),
        );
        let mut other_port = route.clone();
        other_port.source_port = Some("output-b".into());
        assert_ne!(
            network_witness_key(&[route.clone()]),
            network_witness_key(&[other_port]),
        );
        let mut same_physical_route = route.clone();
        same_physical_route.id = 2;
        same_physical_route.expanded = 100;
        same_physical_route.learned_cost = 5;
        same_physical_route.predicted_success = Some(0.75);
        same_physical_route.guidance_uncertainty = Some(0.9);
        assert_eq!(
            physical_action_identity(&MacroAction::Route(route.clone())).unwrap(),
            physical_action_identity(&MacroAction::Route(same_physical_route)).unwrap()
        );
        let mut different_route = route.clone();
        different_route.commands.push(Command::AbortRoute);
        assert_ne!(
            physical_action_identity(&MacroAction::Route(route)).unwrap(),
            physical_action_identity(&MacroAction::Route(different_route)).unwrap()
        );

        let actions = vec![edge(0.8, 0.5), edge(0.2, 0.5)];
        let policy = improved_policy(&actions, &[0.0, 5.0], 0.5);
        assert!((policy.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(policy[1] > policy[0]);
    }

    #[test]
    fn legacy_gumbel_config_keeps_rank_assignment_and_round_trips() {
        let original = json!({
            "algorithm": "gumbel-sequential-halving",
            "candidates": 16,
            "anchor_candidates": 8,
            "anchor_policy_mass": 0.5,
            "seed": 20260922,
            "value_scale": 0.1,
            "max_visit_init": 50.0
        });
        let parsed: RootSelection = serde_json::from_value(original.clone()).unwrap();
        assert!(matches!(
            parsed,
            RootSelection::GumbelSequentialHalving {
                gumbel_assignment: GumbelAssignment::Rank,
                ..
            }
        ));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), original);
    }

    #[test]
    fn sequential_halving_minimum_covers_non_power_of_two_rounds() {
        assert_eq!(minimum_sequential_halving_simulations(2), 2);
        assert_eq!(minimum_sequential_halving_simulations(9), 19);
        assert_eq!(minimum_sequential_halving_simulations(16), 30);
    }

    #[test]
    fn simulation_revisits_retain_completed_layout_quality() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 15,
            max_y: 15,
        };
        let candidate = CandidateConfig {
            bounds,
            max_candidates: 3,
            max_searches: 12,
            work_per_search: 10_000,
            max_expanded: 100_000,
            overlap_penalty: 8,
            max_crossings_per_route: 0,
            request_allowance: Default::default(),
        };
        let config = Config {
            schema: CONFIG_SCHEMA.into(),
            bounds,
            extension_lengths: vec![1, 2, 4],
            candidate: candidate.clone(),
            repair_max_removed: 0,
            cpu_threads: 1,
            placement_page: 4,
            beam_width: 1,
            run_beam: false,
            simulations_per_move: 2,
            horizon: 4,
            max_moves: 4,
            exploration: 1.0,
            root_selection: RootSelection::Puct,
            anchor_prior_mass: 0.0,
            compiled_inference: false,
        };
        let mut session = crate::constructive::test_support::session(64);
        for command in [
            Command::Place {
                block: "source".into(),
                position: Point { x: 5, y: 12 },
                rotation: 0,
            },
            Command::Place {
                block: "target".into(),
                position: Point { x: 5, y: 2 },
                rotation: 0,
            },
        ] {
            assert!(session.step(command).accepted);
        }
        let report = session
            .route_candidates(
                &Request {
                    state_revision: session.revision(),
                    connection: "connection-0".into(),
                },
                &candidate,
            )
            .unwrap();
        session
            .apply_route_candidate(&report.candidates[0])
            .unwrap();

        let first = simulation_leaf_value(&session, 0, &config).unwrap();
        let revisit = simulation_leaf_value(&session, 3, &config).unwrap();
        assert_eq!(first, state_value(&session, bounds));
        assert_eq!(revisit, first);
        assert!(first > 1.0);
    }
}
