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
    logistics::LogisticsComponentKind,
    physical::{BridgeAxis, Problem},
    validation::Catalogs,
};
use aic_pilot_native::constructive::{
    Command, Limits, Session,
    learning::{
        CommandKind, CommandPage, Config as DomainConfig, Domain as LearningDomain,
        FactoredCommand, SelectedFactor,
    },
    routing_candidates::{
        AllowanceStop, AllowanceUsage, Candidate, Config as CandidateConfig, NetworkCandidate,
        PhysicalCost, Provenance, RepairCandidate, RepairRequest, Report as RouteReport, Request,
        RequestAllowance, RequestFailure, RouteReadiness, SearchMemoryBudget,
        Termination as RouteTermination, WorkKind, network_shared_cells_with_allowance,
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
pub const REPORT_SCHEMA: &str = "aic-hierarchical-search-report-v12";
const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PANEL_BYTES: u64 = 256 * 1024 * 1024;
const ROUTE_ATTEMPT_SAMPLE_LIMIT: usize = 32;

mod lower_pager;
mod negotiated;
mod network_pager;
mod placement_scoring;
mod repair_pager;
pub(crate) mod root_catalog;
mod root_scoring;

use lower_pager::{LowerPager, RouteUpdate, TreeReservation, action_bytes, provenance_bytes};
use placement_scoring::{PlacementActionScorer, PreparedPlacement, ScoredPlacement};

fn default_anchor_prior_mass() -> f64 {
    0.25
}

fn default_compiled_inference() -> bool {
    true
}

fn default_max_live_search_bytes() -> usize {
    4 * 1024 * 1024 * 1024
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
    /// Aggregate retained and temporary memory across one search tree.
    #[serde(default = "default_max_live_search_bytes")]
    pub max_live_search_bytes: usize,
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
    pub gumbel_eligible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_target: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RootRecord {
    pub state_sha256: String,
    pub state_revision: u64,
    pub target: RootRecordTarget,
    pub pending_lower_tasks: usize,
    pub loaded_lower_pages: usize,
    pub later_completed_connections: usize,
    pub later_total_connections: usize,
    pub later_footprint_area: u64,
    pub later_transport_cells: u64,
    pub later_turns: u64,
    pub later_fully_validated: bool,
    pub later_termination: SearchTermination,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "data", rename_all = "kebab-case")]
pub(crate) enum RootRecordTarget {
    PuctDense(DenseRootRecord),
    GumbelCatalog {
        descriptor: Arc<root_catalog::publication::RootCatalogDescriptor>,
    },
    #[cfg(test)]
    GumbelDenseOracle(DenseRootRecord),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DenseRootRecord {
    pub actions: Vec<RootActionRecord>,
    pub selected_action: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_physical_id: Option<Vec<u8>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gumbel_shortlist: Vec<RootShortlistEvidence>,
    pub gumbel_shortlist_cap: Option<usize>,
    pub initial_eligible_actions: Option<usize>,
}

impl RootRecord {
    #[cfg(test)]
    pub(crate) fn dense(&self) -> Option<&DenseRootRecord> {
        match &self.target {
            RootRecordTarget::PuctDense(dense) => Some(dense),
            RootRecordTarget::GumbelCatalog { .. } => None,
            #[cfg(test)]
            RootRecordTarget::GumbelDenseOracle(dense) => Some(dense),
        }
    }

    #[cfg(test)]
    pub(crate) fn dense_mut(&mut self) -> Option<&mut DenseRootRecord> {
        match &mut self.target {
            RootRecordTarget::PuctDense(dense) | RootRecordTarget::GumbelDenseOracle(dense) => {
                Some(dense)
            }
            RootRecordTarget::GumbelCatalog { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct RootShortlistEvidence {
    pub physical_id: Vec<u8>,
    pub initial_rank: usize,
    pub gumbel_bits: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SearchTermination {
    FullyValidated,
    NoMacroActions,
    ResourceStopped,
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
    #[serde(default)]
    repair_tasks: usize,
    #[serde(default)]
    repair_attempts: usize,
    #[serde(default)]
    repair_samples: usize,
    #[serde(default)]
    repair_unknown: usize,
    #[serde(default)]
    repair_legalization_failures: usize,
    #[serde(default)]
    repair_guidance_fallbacks: usize,
    #[serde(default)]
    repair_priced_edges: usize,
    #[serde(default, skip_deserializing, skip_serializing_if = "Vec::is_empty")]
    repair_attempt_samples: Vec<negotiated::NegotiationSummary>,
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
    pub(crate) termination: SearchTermination,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
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
    fn config(&self) -> &crate::schema::OuterModelConfig {
        match self {
            Self::Eager(model) => &model.config,
            Self::Compiled(model) => model.config(),
        }
    }

    fn forward(&self, batch: &OuterBatch) -> Result<crate::outer_model::Prediction> {
        match self {
            Self::Eager(model) => model.forward(batch),
            Self::Compiled(model) => model.forward(batch),
        }
    }

    fn encode_one_root(
        &self,
        graph: &crate::constructive::InputBatch,
    ) -> Result<crate::outer_model::EncodedOuterState> {
        match self {
            Self::Eager(model) => Ok(tch::no_grad(|| model.encode_unchecked(graph))),
            Self::Compiled(model) => model.encode_one_root(graph),
        }
    }

    fn score_one_root_page(
        &self,
        encoded: &crate::outer_model::EncodedOuterState,
        rows: &[[f32; ACTION_FEATURES]],
        entities: &[i64],
        device: Device,
    ) -> Result<Tensor> {
        match self {
            Self::Eager(model) => {
                tch::no_grad(|| model.score_one_root_page(encoded, rows, entities, device))
            }
            Self::Compiled(model) => model.score_one_root_page(encoded, rows, entities, device),
        }
    }
}

pub(crate) struct Factory<'a> {
    identity: Arc<()>,
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

struct PlacementPages {
    offsets: [usize; 2],
    exhausted: [bool; 2],
    loaded_pages: usize,
    revision: u64,
    bounds: Bounds,
    domain: Option<Arc<LearningDomain>>,
    domain_memory: Option<TreeReservation>,
    page_memory: Option<TreeReservation>,
    memory_stop: Option<AllowanceStop>,
    retry_after_live: Option<usize>,
}

impl PlacementPages {
    fn ensure_domain(
        &mut self,
        session: &Session,
        config: &DomainConfig,
        budget: Option<&SearchMemoryBudget>,
    ) -> Result<Option<AllowanceStop>> {
        if let (Some(budget), Some(at_stop), Some(stop)) =
            (budget, self.retry_after_live, &self.memory_stop)
        {
            if budget.live_bytes() >= at_stop {
                return Ok(Some(stop.clone()));
            }
        }
        if self.domain.is_some() {
            return Ok(None);
        }
        let mut reservation = if let Some(budget) = budget {
            let peak = session
                .learning_placement_domain_peak_bytes(config)
                .map_err(anyhow::Error::new)?;
            match TreeReservation::new(budget, peak) {
                Ok(reservation) => Some(reservation),
                Err(stop) => {
                    self.memory_stop = Some(stop.clone());
                    self.retry_after_live = Some(budget.live_bytes());
                    return Ok(Some(stop));
                }
            }
        } else {
            None
        };
        let domain = session
            .learning_placement_domain(config)
            .map_err(anyhow::Error::new)?;
        if let Some(reservation) = &mut reservation {
            if let Err(stop) = reservation.resize(domain.retained_bytes()) {
                self.memory_stop = Some(stop.clone());
                self.retry_after_live =
                    budget.map(|budget| budget.live_bytes().saturating_sub(reservation.bytes()));
                return Ok(Some(stop));
            }
        }
        self.domain = Some(domain);
        self.domain_memory = reservation;
        self.memory_stop = None;
        self.retry_after_live = None;
        Ok(None)
    }
}

fn placement_page_peak_bytes(session: &Session, page_size: usize) -> usize {
    let longest_id = session
        .problem()
        .blocks
        .iter()
        .map(|block| block.id.len())
        .chain(
            session
                .problem()
                .terminals
                .iter()
                .map(|terminal| terminal.id.len()),
        )
        .max()
        .unwrap_or(0);
    let per_command = std::mem::size_of::<FactoredCommand>()
        .saturating_add(3 * std::mem::size_of::<SelectedFactor>())
        .saturating_add(std::mem::size_of::<MacroAction>())
        .saturating_add(longest_id)
        .saturating_add(64);
    page_size
        .saturating_mul(2)
        .saturating_mul(per_command)
        .saturating_add(
            session
                .problem()
                .blocks
                .len()
                .saturating_mul(std::mem::size_of::<usize>()),
        )
        .saturating_add(page_size.saturating_mul(std::mem::size_of::<(usize, usize)>()))
        .saturating_add(4096)
}

fn placement_page_allocation_failed(failure: &anyhow::Error) -> bool {
    failure
        .downcast_ref::<aic_pilot_native::constructive::Diagnostic>()
        .is_some_and(|diagnostic| diagnostic.code == "learning-resource")
        || failure
            .downcast_ref::<std::collections::TryReserveError>()
            .is_some()
}

fn placement_host_stop(budget: &SearchMemoryBudget, attempted: usize) -> AllowanceStop {
    AllowanceStop {
        reason: "host-allocation",
        category: None,
        limit: budget.limit_bytes(),
        used: budget.live_bytes(),
        deadline_overshoot_ms: 0,
        attempted_bytes: Some(attempted),
        aggregate_peak_bytes: Some(budget.peak_bytes()),
    }
}

fn load_placement_pair(
    pages: &mut PlacementPages,
    mut load: impl FnMut(CommandKind, usize) -> Result<CommandPage>,
) -> Result<Vec<MacroAction>> {
    let place = (!pages.exhausted[0])
        .then(|| load(CommandKind::Place, pages.offsets[0]))
        .transpose()?;
    let terminal = (!pages.exhausted[1])
        .then(|| load(CommandKind::PlaceTerminal, pages.offsets[1]))
        .transpose()?;
    let command_count = place
        .as_ref()
        .map_or(0, |page| page.commands.len())
        .checked_add(terminal.as_ref().map_or(0, |page| page.commands.len()))
        .ok_or_else(|| anyhow::anyhow!("placement page command count overflow"))?;
    let mut actions = Vec::new();
    actions.try_reserve_exact(command_count)?;
    for (index, page) in [place, terminal].into_iter().enumerate() {
        if let Some(page) = page {
            pages.offsets[index] = page.next_offset;
            pages.exhausted[index] = page.exhausted;
            actions.extend(
                page.commands
                    .into_iter()
                    .map(|factored| MacroAction::Command(factored.command.command)),
            );
        }
    }
    Ok(actions)
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
            identity: Arc::new(()),
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

    fn placement_actions(
        &self,
        session: &Session,
        exhaust_placements: bool,
        tree_memory: Option<&SearchMemoryBudget>,
        previous: Option<PlacementPages>,
    ) -> Result<(Vec<MacroAction>, PlacementPages, Option<AllowanceStop>)> {
        let mut actions = Vec::new();
        let fresh = || PlacementPages {
            offsets: [0; 2],
            exhausted: [true; 2],
            loaded_pages: 1,
            revision: session.revision(),
            bounds: self.domain.placement,
            domain: None,
            domain_memory: None,
            page_memory: None,
            memory_stop: None,
            retry_after_live: None,
        };
        let mut placement_pages = previous
            .filter(|pages| {
                pages.revision == session.revision() && pages.bounds == self.domain.placement
            })
            .unwrap_or_else(fresh);
        if session.has_unplaced_entities() {
            if let Some(stop) = placement_pages.ensure_domain(session, &self.domain, tree_memory)? {
                placement_pages.exhausted = [false; 2];
                placement_pages.loaded_pages = 0;
                return Ok((actions, placement_pages, Some(stop)));
            }
            placement_pages.loaded_pages = 1;
            if !exhaust_placements {
                placement_pages.exhausted = [false; 2];
                let page_memory = if let Some(budget) = tree_memory {
                    match TreeReservation::new(
                        budget,
                        placement_page_peak_bytes(session, self.placement_page),
                    ) {
                        Ok(reservation) => Some(reservation),
                        Err(stop) => {
                            placement_pages.loaded_pages = 0;
                            placement_pages.memory_stop = Some(stop.clone());
                            placement_pages.retry_after_live = Some(budget.live_bytes());
                            return Ok((actions, placement_pages, Some(stop)));
                        }
                    }
                } else {
                    None
                };
                let domain = Arc::clone(
                    placement_pages
                        .domain
                        .as_ref()
                        .expect("placement domain admitted"),
                );
                let loaded = load_placement_pair(&mut placement_pages, |kind, offset| {
                    domain
                        .command_page(kind, offset, self.placement_page)
                        .map_err(anyhow::Error::new)
                });
                actions = match loaded {
                    Ok(actions) => actions,
                    Err(failure)
                        if tree_memory.is_some() && placement_page_allocation_failed(&failure) =>
                    {
                        let budget = tree_memory.expect("guarded by is_some");
                        let stop = placement_host_stop(
                            budget,
                            page_memory.as_ref().map_or(0, TreeReservation::bytes),
                        );
                        placement_pages.loaded_pages = 0;
                        placement_pages.memory_stop = Some(stop.clone());
                        placement_pages.retry_after_live =
                            Some(budget.live_bytes().saturating_sub(
                                page_memory.as_ref().map_or(0, TreeReservation::bytes),
                            ));
                        return Ok((Vec::new(), placement_pages, Some(stop)));
                    }
                    Err(failure) => return Err(failure),
                };
                if !actions.is_empty() {
                    placement_pages.page_memory = page_memory;
                }
                placement_pages.memory_stop = None;
                placement_pages.retry_after_live = None;
                if placement_pages.exhausted.iter().all(|&exhausted| exhausted) {
                    placement_pages.domain = None;
                    placement_pages.domain_memory = None;
                }
                return Ok((actions, placement_pages, None));
            }
            let domain = placement_pages
                .domain
                .as_ref()
                .expect("placement domain admitted");
            for (index, kind) in [CommandKind::Place, CommandKind::PlaceTerminal]
                .into_iter()
                .enumerate()
            {
                loop {
                    let page = domain.command_page(
                        kind,
                        placement_pages.offsets[index],
                        self.placement_page,
                    )?;
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
            if placement_pages.exhausted.iter().all(|&exhausted| exhausted) {
                placement_pages.domain = None;
                placement_pages.domain_memory = None;
            }
        }
        Ok((actions, placement_pages, None))
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
            counters.repair_unknown,
        );
        let (mut actions, placement_pages, _) =
            self.placement_actions(session, exhaust_placements, None, None)?;

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
                    Ok(input) => prepared.push((request.clone(), Some(input), allowance)),
                    Err(RequestFailure::Invalid(diagnostic)) => {
                        if !diagnostic.code.starts_with("constructive.route-guidance") {
                            return Err(anyhow::Error::new(diagnostic));
                        }
                        counters.repair_guidance_fallbacks += 1;
                        prepared.push((request.clone(), None, allowance));
                    }
                    Err(RequestFailure::Stopped(stop)) => {
                        let can_fallback = stop.reason != "deadline";
                        record_request_stop(counters, request, &allowance, stop);
                        if can_fallback {
                            prepared.push((request.clone(), None, allowance));
                        }
                    }
                    Err(RequestFailure::PendingPlacement { .. }) => {
                        counters.pending_route_networks += 1;
                        counters.pending_route_connections += 1;
                    }
                    Err(RequestFailure::NoCompatibleEndpoints { connection }) => {
                        counters.route_no_compatible_endpoints += 1;
                        counters.route_no_compatible_connection = Some(connection);
                        prepared.push((request.clone(), None, allowance));
                    }
                }
            }
            let chunk = prepared.as_mut_slice();
            let mut valid = Vec::new();
            let mut inference_reservations = vec![0usize; chunk.len()];
            for (index, (request, input, allowance)) in chunk.iter_mut().enumerate() {
                let Some(input) = input.as_ref() else {
                    continue;
                };
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
            let mut guidance_by_index = vec![None; chunk.len()];
            let mut guidance_reservations = vec![0usize; chunk.len()];
            if !valid.is_empty() {
                let inputs = valid
                    .iter()
                    .map(|&index| chunk[index].1.as_ref().expect("prepared input"))
                    .collect::<Vec<_>>();
                let guidance =
                    self.guidance
                        .predict_native(&inputs, self.guidance_version, self.device);
                drop(inputs);
                for (index, (_, input, allowance)) in chunk.iter_mut().enumerate() {
                    allowance.release_peak(inference_reservations[index]);
                    if let Some(input) = input.take() {
                        allowance.release_peak(input.reserved_bytes);
                    }
                }
                match guidance {
                    Ok(guidance) => {
                        counters.guidance_batches += 1;
                        counters.guidance_inputs += valid.len();
                        for (index, guide) in valid.iter().copied().zip(guidance) {
                            let (request, _, allowance) = &mut chunk[index];
                            let bytes = guide.heads.iter().fold(0usize, |total, head| {
                                total.saturating_add(head.directed_costs.len().saturating_mul(32))
                            });
                            match allowance.reserve_peak(bytes) {
                                Ok(()) => {
                                    guidance_reservations[index] = bytes;
                                    guidance_by_index[index] = Some(guide);
                                }
                                Err(stop) => {
                                    record_request_stop(counters, request, allowance, stop)
                                }
                            }
                        }
                    }
                    Err(_) => counters.repair_guidance_fallbacks += valid.len(),
                }
            }
            if valid.is_empty() {
                for (_, input, allowance) in chunk.iter_mut() {
                    if let Some(input) = input.take() {
                        allowance.release_peak(input.reserved_bytes);
                    }
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
            let mut direct_counts = vec![0usize; chunk.len()];
            let mut guided_searched = vec![false; chunk.len()];
            for (index, ((request, _, allowance), report)) in
                chunk.iter_mut().zip(reports).enumerate()
            {
                match report {
                    Some(Ok(report)) => {
                        guided_searched[index] = true;
                        direct_counts[index] = report.candidates.len();
                        record_route_report(counters, &report);
                        actions.extend(report.candidates.into_iter().map(MacroAction::Route));
                    }
                    Some(Err(RequestFailure::Invalid(diagnostic))) => {
                        if !diagnostic.code.starts_with("constructive.route-guidance") {
                            return Err(anyhow::Error::new(diagnostic));
                        }
                        counters.repair_guidance_fallbacks += 1;
                        guidance_by_index[index] = None;
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
                guidance_by_index[index] = None;
                allowance.release_peak(guidance_reservations[index]);
            }
            for (index, (request, _, allowance)) in chunk.iter_mut().enumerate() {
                if guided_searched[index] {
                    continue;
                }
                match session.route_candidates_with_allowance(request, self.candidate, allowance) {
                    Ok(report) => {
                        direct_counts[index] = report.candidates.len();
                        record_route_report(counters, &report);
                        actions.extend(report.candidates.into_iter().map(MacroAction::Route));
                    }
                    Err(diagnostic)
                        if diagnostic.code.starts_with("constructive.route-guidance") =>
                    {
                        counters.repair_guidance_fallbacks += 1;
                    }
                    Err(diagnostic) => return Err(anyhow::Error::new(diagnostic)),
                }
            }
            if self.repair_max_removed > 0 {
                for (index, (request, _, allowance)) in chunk.iter_mut().enumerate() {
                    if direct_counts[index] >= self.candidate.max_candidates {
                        continue;
                    }
                    let report = self.negotiated_repair_actions(
                        session,
                        std::slice::from_ref(&request.connection),
                        self.candidate.max_candidates - direct_counts[index],
                        counters,
                        allowance,
                    )?;
                    negotiated::record_negotiation_report(counters, &report);
                    if let Some(repair_request) = report.request {
                        actions.extend(report.actions.into_iter().map(|candidate| {
                            MacroAction::Repair {
                                request: repair_request.clone(),
                                candidate,
                            }
                        }));
                    }
                }
            }
        }
        for (requests, allowance) in &mut ready_networks {
            let connections = requests
                .iter()
                .map(|request| request.connection.clone())
                .collect::<Vec<_>>();
            let network = self.network_actions(session, &connections, counters, allowance)?;
            let direct_count = network.len();
            actions.extend(network.into_iter().map(MacroAction::Network));
            if self.repair_max_removed > 0 && direct_count < self.candidate.max_candidates {
                let report = self.negotiated_repair_actions(
                    session,
                    &connections,
                    self.candidate.max_candidates - direct_count,
                    counters,
                    allowance,
                )?;
                negotiated::record_negotiation_report(counters, &report);
                if let Some(repair_request) = report.request {
                    actions.extend(report.actions.into_iter().map(|candidate| {
                        MacroAction::Repair {
                            request: repair_request.clone(),
                            candidate,
                        }
                    }));
                }
            }
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
        tree_memory: &SearchMemoryBudget,
    ) -> Result<Vec<MacroAction>> {
        if pages.revision != session.revision() || pages.bounds != self.domain.placement {
            pages.domain = None;
            pages.domain_memory = None;
            pages.page_memory = None;
            pages.offsets = [0; 2];
            pages.exhausted = [false; 2];
            pages.loaded_pages = 0;
            pages.revision = session.revision();
            pages.bounds = self.domain.placement;
            pages.memory_stop = None;
            pages.retry_after_live = None;
        }
        if let Some(stop) = pages.ensure_domain(session, &self.domain, Some(tree_memory))? {
            capture_tree_memory_stop(counters, session.revision(), tree_memory, stop);
            return Ok(Vec::new());
        }
        let domain = Arc::clone(pages.domain.as_ref().expect("placement domain admitted"));
        let page_memory = match TreeReservation::new(
            tree_memory,
            placement_page_peak_bytes(session, self.placement_page),
        ) {
            Ok(reservation) => reservation,
            Err(stop) => {
                pages.memory_stop = Some(stop.clone());
                pages.retry_after_live = Some(tree_memory.live_bytes());
                capture_tree_memory_stop(counters, session.revision(), tree_memory, stop);
                return Ok(Vec::new());
            }
        };
        // The pair commits only after both native page calls and output
        // admission succeed.
        let actions = load_placement_pair(pages, |kind, offset| {
            domain
                .command_page(kind, offset, self.placement_page)
                .map_err(anyhow::Error::new)
        });
        let actions = match actions {
            Ok(actions) => actions,
            Err(failure) if placement_page_allocation_failed(&failure) => {
                let stop = placement_host_stop(tree_memory, page_memory.bytes());
                pages.memory_stop = Some(stop.clone());
                pages.retry_after_live =
                    Some(tree_memory.live_bytes().saturating_sub(page_memory.bytes()));
                capture_tree_memory_stop(counters, session.revision(), tree_memory, stop);
                return Ok(Vec::new());
            }
            Err(failure) => return Err(failure),
        };
        pages.memory_stop = None;
        pages.retry_after_live = None;
        if !actions.is_empty() {
            pages.page_memory = Some(page_memory);
        }
        pages.loaded_pages += 1;
        if pages.exhausted.iter().all(|&exhausted| exhausted) {
            pages.domain = None;
            drop(domain);
            pages.domain_memory = None;
        }
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
                    if partial.variant == 0 {
                        prepared.push(Some((member, None)));
                        continue;
                    }
                    match partial.session.route_guidance_input_with_allowance(
                        &member,
                        self.candidate,
                        &self.domain,
                        allowance,
                    ) {
                        Ok(input) => {
                            reserved = reserved.saturating_add(input.reserved_bytes);
                            prepared.push(Some((member, Some(input))));
                        }
                        Err(RequestFailure::Invalid(diagnostic)) => {
                            if !diagnostic.code.starts_with("constructive.route-guidance") {
                                return Err(anyhow::Error::new(diagnostic));
                            }
                            counters.repair_guidance_fallbacks += 1;
                            prepared.push(Some((member, None)));
                        }
                        Err(RequestFailure::Stopped(stop)) => {
                            if stop.reason == "deadline" {
                                record_page_stop!(&member, stop);
                                page_stopped = true;
                                break;
                            }
                            record_page_stop!(&member, stop);
                            prepared.push(Some((member, None)));
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
                                    if !diagnostic.code.starts_with("constructive.route-guidance") {
                                        return Err(anyhow::Error::new(diagnostic));
                                    }
                                    counters.repair_guidance_fallbacks += 1;
                                    prepared.push(None);
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
                let mut inference_unavailable = false;
                for (member, input) in prepared.iter().flatten() {
                    let Some(input) = input.as_ref() else {
                        continue;
                    };
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
                        let deadline = stop.reason == "deadline";
                        record_page_stop!(member, stop);
                        page_stopped = deadline;
                        inference_unavailable = true;
                        break;
                    }
                }
                if page_stopped {
                    break;
                }
                if prepared.iter().all(Option::is_none) {
                    for partial in partials.drain(..) {
                        let bytes = clone_bytes.saturating_add(partial.retained_bytes);
                        allowance.release_peak(bytes);
                        reserved -= bytes;
                    }
                    partials.clear();
                    break;
                }
                let had_input = prepared
                    .iter()
                    .map(|item| item.as_ref().is_some_and(|(_, input)| input.is_some()))
                    .collect::<Vec<_>>();
                let mut guidance = None;
                if !inference_unavailable && had_input.iter().any(|&present| present) {
                    let inputs = prepared
                        .iter()
                        .flatten()
                        .filter_map(|(_, input)| input.as_ref())
                        .collect::<Vec<_>>();
                    let inference_bytes = inputs.iter().fold(0usize, |total, input| {
                        total
                            .saturating_add(inference_reservation_bytes(input, self.guidance_heads))
                    });
                    match allowance.reserve_peak(inference_bytes) {
                        Ok(()) => {
                            reserved = reserved.saturating_add(inference_bytes);
                            let predicted = self.guidance.predict_native(
                                &inputs,
                                self.guidance_version,
                                self.device,
                            );
                            allowance.release_peak(inference_bytes);
                            reserved -= inference_bytes;
                            match predicted {
                                Ok(guides) => {
                                    counters.guidance_batches += 1;
                                    counters.guidance_inputs += inputs.len();
                                    guidance = Some(guides);
                                }
                                Err(_) => counters.repair_guidance_fallbacks += inputs.len(),
                            }
                        }
                        Err(stop) => {
                            let deadline = stop.reason == "deadline";
                            record_page_stop!(&request, stop);
                            page_stopped = deadline;
                        }
                    }
                }
                if page_stopped {
                    break;
                }
                let input_bytes = prepared
                    .iter_mut()
                    .flatten()
                    .filter_map(|(_, input)| input.take())
                    .map(|input| input.reserved_bytes)
                    .sum::<usize>();
                allowance.release_peak(input_bytes);
                reserved = reserved.saturating_sub(input_bytes);
                let mut guidance_bytes = guidance.as_ref().map_or(0, |guides| {
                    guides.iter().fold(0usize, |total, guide| {
                        total.saturating_add(guide.heads.iter().fold(0usize, |head_total, head| {
                            head_total.saturating_add(head.directed_costs.len().saturating_mul(32))
                        }))
                    })
                });
                if let Err(stop) = allowance.reserve_peak(guidance_bytes) {
                    let deadline = stop.reason == "deadline";
                    record_page_stop!(&request, stop);
                    page_stopped = deadline;
                    guidance = None;
                    guidance_bytes = 0;
                } else {
                    reserved = reserved.saturating_add(guidance_bytes);
                }
                if page_stopped {
                    break;
                }
                if let Err(stop) = allowance.check_deadline() {
                    record_page_stop!(&request, stop);
                    page_stopped = true;
                    break;
                }
                let mut next = Vec::with_capacity(partials.len());
                let mut guide_index = 0usize;
                for (index, (mut partial, prepared)) in std::mem::take(&mut partials)
                    .into_iter()
                    .zip(&prepared)
                    .enumerate()
                {
                    let Some((member, _)) = prepared else {
                        let bytes = clone_bytes.saturating_add(partial.retained_bytes);
                        allowance.release_peak(bytes);
                        reserved -= bytes;
                        continue;
                    };
                    let guide = if had_input[index] {
                        let selected = guidance.as_ref().and_then(|guides| guides.get(guide_index));
                        guide_index += 1;
                        selected
                    } else {
                        None
                    };
                    let routed = if let Some(guide) = guide {
                        partial.session.route_candidates_for_network_with_allowance(
                            member,
                            self.candidate,
                            guide,
                            allowance,
                        )
                    } else {
                        partial
                            .session
                            .route_candidates_for_network_anchor_with_allowance(
                                member,
                                self.candidate,
                                allowance,
                            )
                    };
                    let report = match routed {
                        Ok(report) if guide.is_some() && report.candidates.is_empty() => partial
                            .session
                            .route_candidates_for_network_anchor_with_allowance(
                                member,
                                self.candidate,
                                allowance,
                            )
                            .map_err(anyhow::Error::new)?,
                        Ok(report) => report,
                        Err(diagnostic)
                            if diagnostic.code.starts_with("constructive.route-guidance") =>
                        {
                            counters.repair_guidance_fallbacks += 1;
                            partial
                                .session
                                .route_candidates_for_network_anchor_with_allowance(
                                    member,
                                    self.candidate,
                                    allowance,
                                )
                                .map_err(anyhow::Error::new)?
                        }
                        Err(diagnostic) => return Err(anyhow::Error::new(diagnostic)),
                    };
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
                    let mut candidate = selected.clone();
                    match partial
                        .session
                        .apply_unpublished_route_candidate_with_cost_allowance(
                            &candidate, allowance,
                        ) {
                        Ok((_, cost)) => candidate.physical_cost = cost,
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
                let key_bytes = network_witness_key_reservation_bytes(&partial.routes);
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
                let physical_cost = root
                    .physical_cost_between(&partial.session, connections)
                    .map_err(anyhow::Error::new)?;
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
                    physical_cost,
                    charged_work: allowance.usage(),
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

    fn score_action_rows(
        &self,
        session: &Session,
        features: &[[f32; ACTION_FEATURES]],
        entity_references: &[i64],
        anchor_scores: &[f64],
    ) -> Result<ActionScores> {
        ensure!(
            features.len() == entity_references.len() && features.len() == anchor_scores.len(),
            "action score rows have mismatched lengths"
        );
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
        let Some(model) = self.outer.as_ref() else {
            return Ok(ActionScores {
                priors: anchor_priors.clone(),
                anchor_priors,
                model_value: state_value(session, self.domain.placement),
            });
        };
        let input = session
            .macro_learning_input(&self.domain)
            .map_err(anyhow::Error::new)?;
        let batch = OuterBatch::from_rows(
            &[input.as_ref()],
            &[features],
            &[entity_references],
            self.device,
        )?;
        let prediction = tch::no_grad(|| model.forward(&batch))?;
        let learned = Vec::<f32>::try_from(prediction.logp.exp().to_device(Device::Cpu))?
            .into_iter()
            .map(f64::from)
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
        // The sparse native output remains live after the tensor scratch is
        // released. Include its strings and head headers in the reservation
        // before inference so ownership can transfer without a second peak.
        .saturating_add(input.connection.len())
        .saturating_add(heads.saturating_mul(128))
        .saturating_add(1024)
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

fn capture_tree_memory_stop(
    counters: &mut Counters,
    revision: u64,
    budget: &SearchMemoryBudget,
    stop: AllowanceStop,
) {
    capture_request_stop(
        counters,
        RequestStopRecord {
            state_revision: revision,
            connection: "tree-memory".into(),
            stop,
            usage: AllowanceUsage {
                peak_live_bytes: budget.peak_bytes(),
                ..AllowanceUsage::default()
            },
        },
    );
}

fn tree_memory_diagnostic(stop: &AllowanceStop, revision: u64) -> MacroDiagnostic {
    if stop.reason == "host-allocation" {
        return MacroDiagnostic {
            code: "hierarchical.resource-limit".into(),
            severity: "warning".into(),
            stage: "hierarchical-search".into(),
            message: format!("placement page host allocation failed at revision {revision}"),
            entity: "placement-page".into(),
        };
    }
    MacroDiagnostic {
        code: "hierarchical.aggregate-memory-limit".into(),
        severity: "warning".into(),
        stage: "hierarchical-search".into(),
        message: format!(
            "aggregate search memory reached {} of {} bytes; attempted {} more bytes, charged peak {} bytes at revision {revision}",
            stop.used,
            stop.limit,
            stop.attempted_bytes.unwrap_or(0),
            stop.aggregate_peak_bytes.unwrap_or(stop.used),
        ),
        entity: "tree-memory".into(),
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
        usize,
    ),
    revision: u64,
) -> MacroDiagnostic {
    let (code, entity, message) = if counters.request_stops_total > before.0 {
        let (connection, stop) = counters
            .last_request_stop
            .as_ref()
            .expect("new stop record");
        (
            if stop.reason == "aggregate-memory-limit" {
                "hierarchical.aggregate-memory-limit"
            } else if stop.reason == "deadline" {
                "hierarchical.request-deadline"
            } else if stop.reason == "peak-memory-limit" || stop.reason == "repair-resource-size" {
                "hierarchical.resource-limit"
            } else {
                "hierarchical.request-work-limit"
            },
            connection.clone(),
            if stop.reason == "aggregate-memory-limit" {
                format!(
                    "aggregate search memory reached {} of {} bytes; attempted {} more bytes, charged peak {} bytes",
                    stop.used,
                    stop.limit,
                    stop.attempted_bytes.unwrap_or(0),
                    stop.aggregate_peak_bytes.unwrap_or(stop.used),
                )
            } else if stop.reason == "peak-memory-limit" || stop.reason == "repair-resource-size" {
                format!(
                    "request exceeded the representable or configured memory limit: {}",
                    stop.reason
                )
            } else {
                format!(
                    "request stopped at {:?}: used {}, limit {}",
                    stop.category, stop.used, stop.limit
                )
            },
        )
    } else if counters.route_no_compatible_endpoints > before.4 {
        (
            "hierarchical.no-compatible-endpoints",
            counters
                .route_no_compatible_connection
                .clone()
                .unwrap_or_else(|| "macro-state".into()),
            "placed endpoints have no compatible port pair".into(),
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
    } else if counters.repair_unknown > before.11 {
        (
            "hierarchical.repair-unknown",
            "macro-state".into(),
            "bounded repair search stopped without a complete legal candidate".into(),
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
            !fork.has_active_route(),
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
        let physical_cost = match self {
            Self::Route(candidate) => Some(&candidate.physical_cost),
            Self::Network(candidate) => Some(&candidate.physical_cost),
            Self::Repair { candidate, .. } => Some(&candidate.physical_cost),
            Self::Command(_) => None,
        };
        let charged_work = match self {
            Self::Route(candidate) => Some(&candidate.charged_work),
            Self::Network(candidate) => Some(&candidate.charged_work),
            Self::Repair { candidate, .. } => Some(&candidate.charged_work),
            Self::Command(_) => None,
        };
        let scale = bounds.width().max(bounds.height()).max(1) as f32;
        row[3] = construction_progress(before) as f32;
        row[4] = construction_progress(after) as f32;
        row[5] = row[4] - row[3];
        row[23] = after.metrics().is_some() as u8 as f32;
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
                mark_route_provenance(&mut row, candidate);
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
                    mark_route_provenance(&mut row, route);
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
                    mark_route_provenance(&mut row, route);
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
        if let Some(cost) = physical_cost {
            row[30..45].copy_from_slice(&physical_cost_features(cost, bounds));
        }
        if let Some(work) = charged_work {
            row[45..49].copy_from_slice(&charged_work_features(work));
        }
        row
    }
}

fn mark_route_provenance(row: &mut [f32; ACTION_FEATURES], candidate: &Candidate) {
    mark_route_provenance_values(row, &candidate.provenance, &candidate.additional_provenance);
}

fn mark_route_provenance_values(
    row: &mut [f32; ACTION_FEATURES],
    primary: &Provenance,
    additional: &[Provenance],
) {
    for provenance in std::iter::once(primary).chain(additional.iter()) {
        match provenance {
            Provenance::Anchor => row[13] = 1.0,
            Provenance::Guided { .. } => row[14] = 1.0,
            Provenance::Diversified { .. } => row[15] = 1.0,
        }
    }
}

/// Version 5 outer action slots 30..45. Counts and rational utilization are
/// derived from the exact native replay delta; no catalog currency is assumed.
fn physical_cost_features(cost: &PhysicalCost, bounds: Bounds) -> [f32; 15] {
    let scale = bounds.width().max(bounds.height()).max(1) as f32;
    let area = (bounds.width() * bounds.height()).max(1) as f32;
    let mut installed = 0f32;
    let mut reused = 0f32;
    let mut removed = 0f32;
    let mut component_delta = [0f32; 3];
    for item in &cost.component_uses {
        match item.delta {
            1 => installed += 1.0,
            -1 => removed += 1.0,
            _ => reused += 1.0,
        }
        let kind = match item.kind {
            LogisticsComponentKind::Splitter => 0,
            LogisticsComponentKind::Converger => 1,
            LogisticsComponentKind::Bridge => 2,
        };
        component_delta[kind] += f32::from(item.delta);
    }
    let utilization =
        |rate: aic_layout::rate::Rate| rate.numerator as f32 / rate.denominator.max(1) as f32;
    [
        cost.route_steps_delta as f32 / scale,
        cost.route_turns_delta as f32 / scale,
        cost.occupied_transport_cells_delta as f32 / area,
        cost.component_count_delta as f32,
        cost.component_footprint_cells_delta as f32 / area,
        installed,
        reused,
        removed,
        component_delta[0],
        component_delta[1],
        component_delta[2],
        utilization(cost.max_touched_utilization_before),
        utilization(cost.max_touched_utilization_after),
        cost.touched_resource_count as f32 / area,
        cost.component_uses.len() as f32,
    ]
}

/// Version 5 slots 45..49 expose the native work already charged when the
/// witness was published. The logarithm keeps upper-scale counts finite.
fn charged_work_features(work: &AllowanceUsage) -> [f32; 4] {
    let amount = |kind| work.categories.get(&kind).copied().unwrap_or(0) as f32;
    [
        amount(WorkKind::Expansions).ln_1p() / 16.0,
        amount(WorkKind::RoutingScans).ln_1p() / 20.0,
        amount(WorkKind::CopiedBytes).ln_1p() / 30.0,
        (work.peak_live_bytes as f32).ln_1p() / 30.0,
    ]
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
    let endpoint = |id: &str| session.placed_entity_position(id);
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

fn network_witness_key_reservation_bytes(routes: &[Candidate]) -> usize {
    let route_cells = routes.iter().map(|route| route.cells.len()).sum::<usize>();
    let route_commands = routes
        .iter()
        .map(|route| route.commands.len())
        .sum::<usize>();
    route_cells
        .saturating_mul(64)
        .saturating_add(route_commands.saturating_mul(64))
        .saturating_add(routes.iter().fold(0usize, |total, route| {
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
        }))
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
    let catalog_binding = root_catalog::case::CatalogRunBinding {
        limits_sha256: catalog_limits_sha256(&panel.value.limits)?,
        effective_config_sha256: catalog_effective_config_sha256(
            &config,
            &bytes_sha256(&lower_config_bytes),
            &bytes_sha256(&lower_order_bytes),
            &checkpoint_sha256,
            &bytes_sha256(&outer_config_bytes),
            &bytes_sha256(&outer_order_bytes),
            &outer_checkpoint_sha256,
        )?,
        catalog_sha256: catalogs.identity.clone(),
        outer_checkpoint_sha256: outer_checkpoint_sha256.clone(),
    };
    let mut staged_catalogs = Vec::new();
    let catalog_memory = SearchMemoryBudget::new(config.max_live_search_bytes);
    if matches!(
        config.root_selection,
        RootSelection::GumbelSequentialHalving { .. }
    ) {
        let parent = output
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let prefixes = panel
            .value
            .cases
            .iter()
            .flat_map(|case| {
                (0..config.max_moves).map(move |move_index| {
                    root_catalog::case::root_prefix(output, &case.id, move_index)
                })
            })
            .collect::<Vec<_>>();
        root_catalog::publication::recover_unpublished(output, &prefixes)?;
    }
    let mut cases = Vec::with_capacity(panel.value.cases.len());
    for case in &panel.value.cases {
        let root = prefix_session(case, &panel.value, catalogs.clone())?;
        let beam = config
            .run_beam
            .then(|| beam_search(root.clone(), &factory, &config))
            .transpose()?;
        let mut mcts = match config.root_selection {
            RootSelection::Puct => mcts_search(root, &factory, &config, &case.id)?,
            RootSelection::GumbelSequentialHalving { .. } => {
                let collected = root_catalog::case::search_case(
                    root,
                    &factory,
                    &config,
                    &case.id,
                    output,
                    retain_root_records,
                    &catalog_binding,
                    &catalog_memory,
                )?;
                let mut mcts = collected.result;
                if retain_root_records && mcts.termination != SearchTermination::ResourceStopped {
                    mcts.root_records = collected
                        .roots
                        .into_iter()
                        .map(|root| RootRecord {
                            state_sha256: root.state_sha256,
                            state_revision: root.state_revision,
                            target: RootRecordTarget::GumbelCatalog {
                                descriptor: root.descriptor,
                            },
                            pending_lower_tasks: root.pending_lower_tasks,
                            loaded_lower_pages: root.loaded_lower_pages,
                            later_completed_connections: mcts.completed_connections,
                            later_total_connections: mcts.total_connections,
                            later_footprint_area: mcts.footprint_area.unwrap_or(0),
                            later_transport_cells: mcts.transport_cells.unwrap_or(0),
                            later_turns: mcts.turns.unwrap_or(0),
                            later_fully_validated: mcts.fully_validated,
                            later_termination: mcts.termination,
                        })
                        .collect();
                    staged_catalogs.extend(collected.stages);
                }
                mcts
            }
        };
        if !retain_root_records || mcts.termination == SearchTermination::ResourceStopped {
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
    if staged_catalogs.is_empty() {
        publish_json(output, report)
    } else {
        let (sha256, encoded_bytes) = root_catalog::publication::publish_embedded_report(
            output,
            &report,
            &mut staged_catalogs,
        )?
        .map_err(|stop| anyhow::anyhow!("aic-learning.hierarchical-catalog-publication: {stop}"))?;
        Ok(Published {
            value: report,
            sha256,
            encoded_bytes,
        })
    }
}

#[derive(Serialize)]
struct VersionedCatalogLimits<'a> {
    schema: &'static str,
    limits: &'a Limits,
}

pub(crate) fn catalog_limits_sha256(limits: &Limits) -> Result<String> {
    semantic_sha256(&VersionedCatalogLimits {
        schema: "aic-hierarchical-catalog-limits-v1",
        limits,
    })
}

#[derive(Serialize)]
struct CatalogEffectiveConfig<'a> {
    schema: &'static str,
    search: &'a Config,
    lower_config_sha256: &'a str,
    lower_order_sha256: &'a str,
    lower_checkpoint_sha256: &'a str,
    outer_config_sha256: &'a str,
    outer_order_sha256: &'a str,
    outer_checkpoint_sha256: &'a str,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn catalog_effective_config_sha256(
    search: &Config,
    lower_config_sha256: &str,
    lower_order_sha256: &str,
    lower_checkpoint_sha256: &str,
    outer_config_sha256: &str,
    outer_order_sha256: &str,
    outer_checkpoint_sha256: &str,
) -> Result<String> {
    semantic_sha256(&CatalogEffectiveConfig {
        schema: "aic-hierarchical-catalog-effective-config-v1",
        search,
        lower_config_sha256,
        lower_order_sha256,
        lower_checkpoint_sha256,
        outer_config_sha256,
        outer_order_sha256,
        outer_checkpoint_sha256,
    })
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
    publish_panel_json(output, panel)
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
            if session.metrics().is_some() {
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
            .is_some_and(|(session, _)| session.metrics().is_some())
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
        if let Some(completed) = beam.iter().find(|(session, _)| session.metrics().is_some()) {
            best = completed.clone();
            termination = SearchTermination::FullyValidated;
            break;
        }
    }
    if let Some(completed) = beam.iter().find(|(session, _)| session.metrics().is_some()) {
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
    identity: Vec<u8>,
    features: [f32; ACTION_FEATURES],
    entity_index: i64,
    anchor_score: f64,
    prior: f64,
    anchor_prior: f64,
    visits: u32,
    value_sum: f64,
    child: Option<usize>,
    blocked_value: Option<f64>,
}

struct Node {
    session: Arc<Session>,
    memory: TreeReservation,
    visits: u32,
    value: Option<f64>,
    actions: Option<Vec<Edge>>,
    placement_pages: Option<PlacementPages>,
    lower: Option<LowerPager>,
    lower_loaded_pages: usize,
    pending_actions: Vec<MacroAction>,
    pending_updates: Vec<RouteUpdate>,
    pending_retry_live: Option<usize>,
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
    let tree_memory = SearchMemoryBudget::new(config.max_live_search_bytes);
    let mut root_record_reservations = Vec::new();
    for _ in 0..config.max_moves {
        if working.metrics().is_some() {
            termination = SearchTermination::FullyValidated;
            break;
        }
        let root_stops_before = counters.request_stops_total;
        let root_bytes = working
            .clone_reservation_bytes()
            .saturating_add(std::mem::size_of::<Node>());
        let root_memory = match TreeReservation::new(&tree_memory, root_bytes) {
            Ok(reservation) => reservation,
            Err(stop) => {
                capture_tree_memory_stop(
                    &mut counters,
                    working.revision(),
                    &tree_memory,
                    stop.clone(),
                );
                no_macro_diagnostic = Some(tree_memory_diagnostic(&stop, working.revision()));
                termination = SearchTermination::ResourceStopped;
                break;
            }
        };
        let mut nodes = vec![Node {
            session: Arc::new(working.clone()),
            memory: root_memory,
            visits: 0,
            value: None,
            actions: None,
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        let hash_memory = match TreeReservation::new(
            &tree_memory,
            working.hierarchical_state_hash_workspace_bytes(),
        ) {
            Ok(reservation) => reservation,
            Err(stop) => {
                capture_tree_memory_stop(
                    &mut counters,
                    working.revision(),
                    &tree_memory,
                    stop.clone(),
                );
                no_macro_diagnostic = Some(tree_memory_diagnostic(&stop, working.revision()));
                termination = SearchTermination::ResourceStopped;
                break;
            }
        };
        let state_identity = state_sha256(&working)?;
        drop(hash_memory);
        let (selected, policy_targets, gumbel_eligible, gumbel_shortlist): (
            Option<usize>,
            Option<Vec<f64>>,
            Option<HashSet<Vec<u8>>>,
            Vec<RootShortlistEvidence>,
        ) = match &config.root_selection {
            RootSelection::Puct => {
                for _ in 0..config.simulations_per_move {
                    simulate(
                        &mut nodes,
                        0,
                        None,
                        factory,
                        config,
                        &mut counters,
                        &tree_memory,
                    )?;
                    simulations += 1;
                }
                ensure_node_expanded(&mut nodes, 0, factory, &mut counters, false, &tree_memory)?;
                let selected = nodes[0]
                    .actions
                    .as_ref()
                    .expect("expanded root")
                    .iter()
                    .enumerate()
                    .filter(|(_, edge)| edge.blocked_value.is_none())
                    .max_by(|left, right| {
                        left.1
                            .visits
                            .cmp(&right.1.visits)
                            .then_with(|| left.1.prior.total_cmp(&right.1.prior))
                    })
                    .map(|(index, _)| index);
                (selected, None, None, Vec::new())
            }
            #[cfg(test)]
            RootSelection::GumbelSequentialHalving {
                candidates,
                anchor_candidates,
                anchor_policy_mass,
                seed,
                value_scale,
                max_visit_init,
                gumbel_assignment,
            } => {
                ensure_node_expanded(&mut nodes, 0, factory, &mut counters, true, &tree_memory)?;
                let decision = gumbel_sequential_halving(
                    &mut nodes,
                    0,
                    factory,
                    config,
                    &mut counters,
                    &tree_memory,
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
                (
                    decision.selected,
                    decision.policy_targets,
                    decision.eligible_ids,
                    decision.shortlist,
                )
            }
            #[cfg(not(test))]
            RootSelection::GumbelSequentialHalving { .. } => {
                unreachable!("production Gumbel roots use the catalog case runner")
            }
        };
        let Some(selected) = selected else {
            termination = if counters.request_stops_total > root_stops_before {
                SearchTermination::ResourceStopped
            } else {
                SearchTermination::NoMacroActions
            };
            no_macro_diagnostic = nodes[0].no_macro_diagnostic.clone();
            if let Some(diagnostic) = &mut no_macro_diagnostic {
                if termination == SearchTermination::ResourceStopped {
                    diagnostic.message.push_str(&format!(
                        "; pending lower tasks {}, retained unscored witnesses {}",
                        nodes[0].lower.as_ref().map_or(0, LowerPager::pending),
                        nodes[0].pending_actions.len(),
                    ));
                }
            }
            break;
        };
        let actions = nodes[0].actions.as_ref().expect("expanded root");
        let record_bytes = actions.iter().fold(4096usize, |bytes, edge| {
            bytes
                .saturating_add(action_bytes(&edge.action))
                .saturating_add(std::mem::size_of::<RootActionRecord>())
        });
        let record_reservation = match TreeReservation::new(&tree_memory, record_bytes) {
            Ok(reservation) => reservation,
            Err(stop) => {
                capture_tree_memory_stop(
                    &mut counters,
                    working.revision(),
                    &tree_memory,
                    stop.clone(),
                );
                no_macro_diagnostic = Some(tree_memory_diagnostic(&stop, working.revision()));
                termination = SearchTermination::ResourceStopped;
                break;
            }
        };
        root_record_reservations.push(record_reservation);
        let action = actions[selected].action.clone();
        let revision = working.revision();
        root_records.push(root_record_from_decision(
            &nodes[0],
            state_identity,
            revision,
            selected,
            policy_targets.as_deref(),
            gumbel_eligible.as_ref(),
            gumbel_shortlist,
            match &config.root_selection {
                RootSelection::Puct => None,
                RootSelection::GumbelSequentialHalving { candidates, .. } => Some(*candidates),
            },
        ));
        working = action.apply(&working, &config.candidate, config.repair_max_removed)?;
        trace.push(action.summary(revision));
    }
    let (final_completed, final_total) = working.connection_progress_counts();
    let final_validated = working.metrics().is_some();
    if final_validated {
        termination = SearchTermination::FullyValidated;
    }
    for record in &mut root_records {
        record.later_completed_connections = final_completed;
        record.later_total_connections = final_total;
        record.later_footprint_area = working.metrics().map_or(0, |metrics| metrics.area);
        record.later_transport_cells = working
            .metrics()
            .map_or(0, |metrics| metrics.transport_cells);
        record.later_turns = working.metrics().map_or(0, |metrics| metrics.turns);
        record.later_fully_validated = final_validated;
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

#[allow(clippy::too_many_arguments)]
fn root_record_from_decision(
    node: &Node,
    state_sha256: String,
    state_revision: u64,
    selected_action: usize,
    policy_targets: Option<&[f64]>,
    eligible_ids: Option<&HashSet<Vec<u8>>>,
    gumbel_shortlist: Vec<RootShortlistEvidence>,
    gumbel_shortlist_cap: Option<usize>,
) -> RootRecord {
    let actions = node.actions.as_ref().expect("expanded root");
    RootRecord {
        state_sha256,
        state_revision,
        target: {
            let dense_actions = actions
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
                    gumbel_eligible: eligible_ids.map(|eligible| eligible.contains(&edge.identity)),
                    policy_target: policy_targets.map(|targets| targets[index]),
                })
                .collect();
            let dense = DenseRootRecord {
                actions: dense_actions,
                selected_action,
                selected_physical_id: Some(actions[selected_action].identity.clone()),
                gumbel_shortlist,
                gumbel_shortlist_cap,
                initial_eligible_actions: eligible_ids.map(HashSet::len),
            };
            #[cfg(test)]
            if gumbel_shortlist_cap.is_some() {
                RootRecordTarget::GumbelDenseOracle(dense)
            } else {
                RootRecordTarget::PuctDense(dense)
            }
            #[cfg(not(test))]
            {
                assert!(
                    gumbel_shortlist_cap.is_none(),
                    "production Gumbel must use catalog roots"
                );
                RootRecordTarget::PuctDense(dense)
            }
        },
        pending_lower_tasks: node.lower.as_ref().map_or(0, LowerPager::pending),
        loaded_lower_pages: node.lower_loaded_pages,
        later_completed_connections: 0,
        later_total_connections: 0,
        later_footprint_area: 0,
        later_transport_cells: 0,
        later_turns: 0,
        later_fully_validated: false,
        later_termination: SearchTermination::MoveBudgetExhausted,
    }
}

#[cfg(test)]
pub(crate) fn controlled_network_pair(
    session: &Session,
    candidate: &CandidateConfig,
) -> Result<(MacroAction, MacroAction)> {
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
    let factory = Factory::new(
        &model,
        "checkpoint",
        Device::Cpu,
        DomainConfig {
            placement: candidate.bounds,
            extension_lengths: vec![1, 2, 4],
        },
        candidate,
        0,
        4,
        1,
        0.0,
        false,
        None,
    )?;
    let mut counters = Counters::default();
    let mut allowance = RequestAllowance::new(candidate.request_allowance.clone());
    let networks = factory.network_actions(
        session,
        &["output-a".into(), "output-b".into()],
        &mut counters,
        &mut allowance,
    )?;
    ensure!(networks.len() >= 2, "controlled fixture needs two networks");
    Ok((
        MacroAction::Network(networks[0].clone()),
        MacroAction::Network(networks[1].clone()),
    ))
}

#[cfg(test)]
pub(crate) fn controlled_gumbel_record(
    source: Session,
    candidate: CandidateConfig,
    initial: Vec<MacroAction>,
    later: Vec<MacroAction>,
    updates: Vec<(String, usize, Vec<Provenance>)>,
) -> Result<RootRecord> {
    let lower_config: serde_json::Value = serde_json::from_str(include_str!(
        "../../../docs/benchmarks/hierarchical-route-guidance/config.json"
    ))?;
    let model_config = serde_json::from_value(lower_config["model"].clone())?;
    let domain = serde_json::from_value(lower_config["learning_domain"].clone())?;
    let (order, state) = GuidanceModel::initial_state(&model_config, 17)?;
    let model = GuidanceModel::load(&model_config, &order, &state, Device::Cpu)?;
    let factory = Factory::new(
        &model,
        "checkpoint",
        Device::Cpu,
        domain,
        &candidate,
        1,
        4,
        1,
        0.0,
        false,
        None,
    )?;
    let mut config: Config = serde_json::from_str(include_str!(
        "../../../docs/benchmarks/hierarchical-curriculum/search.json"
    ))?;
    config.candidate = candidate.clone();
    config.simulations_per_move = 2;
    config.horizon = 1;
    config.repair_max_removed = 1;
    let budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
    let state_identity = state_sha256(&source)?;
    let revision = source.revision();
    let mut nodes = vec![Node {
        session: Arc::new(source),
        memory: TreeReservation::new(&budget, 0)
            .map_err(|stop| anyhow::anyhow!("controlled root reservation: {stop:?}"))?,
        visits: 0,
        value: None,
        actions: Some(Vec::new()),
        placement_pages: None,
        lower: None,
        lower_loaded_pages: 0,
        pending_actions: Vec::new(),
        pending_updates: Vec::new(),
        pending_retry_live: None,
        no_macro_diagnostic: None,
    }];
    let mut counters = Counters::default();
    append_node_actions(
        &mut nodes,
        0,
        &factory,
        &mut counters,
        &budget,
        initial,
        Vec::new(),
    )?;
    let controls = GumbelControls {
        candidates: 2,
        anchor_candidates: 0,
        anchor_policy_mass: 0.0,
        seed: 7,
        value_scale: 0.1,
        max_visit_init: 50.0,
        gumbel_assignment: GumbelAssignment::ActionIdentity,
    };
    let frozen = freeze_gumbel_root(&nodes[0], controls, "controlled", &state_identity)?
        .ok_or_else(|| anyhow::anyhow!("controlled root has no initial action"))?;
    let updates = updates
        .into_iter()
        .map(|(connection, id, additional_provenance)| {
            Ok(RouteUpdate {
                connection,
                id,
                additional_provenance,
                _memory: TreeReservation::new(&budget, 0)
                    .map_err(|stop| anyhow::anyhow!("controlled update reservation: {stop:?}"))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    append_node_actions(
        &mut nodes,
        0,
        &factory,
        &mut counters,
        &budget,
        later,
        updates,
    )?;
    let decision = finish_gumbel_root(
        &mut nodes,
        0,
        &factory,
        &config,
        &mut counters,
        &budget,
        controls,
        frozen,
    )?;
    let selected = decision
        .selected
        .ok_or_else(|| anyhow::anyhow!("controlled root has no selected action"))?;
    let record = root_record_from_decision(
        &nodes[0],
        state_identity,
        revision,
        selected,
        decision.policy_targets.as_deref(),
        decision.eligible_ids.as_ref(),
        decision.shortlist,
        Some(2),
    );
    let report = Report {
        schema: REPORT_SCHEMA.into(),
        status: "complete".into(),
        partition: "test".into(),
        device: "cpu".into(),
        search_config_sha256: String::new(),
        panel_sha256: String::new(),
        catalog_sha256: String::new(),
        lower_config_sha256: String::new(),
        lower_parameter_order_sha256: String::new(),
        lower_checkpoint_sha256: String::new(),
        outer_config_sha256: String::new(),
        outer_parameter_order_sha256: String::new(),
        outer_checkpoint_sha256: String::new(),
        root_records_retained: true,
        cases: vec![CaseReport {
            id: "controlled".into(),
            prefix_commands: 0,
            beam: None,
            mcts: SearchResult {
                algorithm: "mcts".into(),
                status: "complete".into(),
                fully_validated: false,
                termination: SearchTermination::MoveBudgetExhausted,
                no_macro_diagnostic: None,
                completed_connections: 0,
                total_connections: 1,
                footprint_area: None,
                transport_cells: None,
                turns: None,
                moves: 1,
                simulations: decision.simulations,
                elapsed_seconds: 0.0,
                samples_per_minute: 0.0,
                counters,
                trace: Vec::new(),
                root_records: vec![record],
            },
        }],
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SERIAL: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "aic-controlled-gumbel-{}-{}.json",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    publish_json(&path, report)?;
    let loaded = load_published_report(&path);
    fs::remove_file(&path)?;
    Ok(loaded?.value.cases.remove(0).mcts.root_records.remove(0))
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

#[cfg(test)]
struct RootDecision {
    selected: Option<usize>,
    policy_targets: Option<Vec<f64>>,
    eligible_ids: Option<HashSet<Vec<u8>>>,
    shortlist: Vec<RootShortlistEvidence>,
    simulations: usize,
}

#[cfg(test)]
struct RootChoice {
    identity: Vec<u8>,
    gumbel: f64,
    initial_rank: usize,
}

#[cfg(test)]
struct FrozenGumbelRoot {
    eligible_ids: HashSet<Vec<u8>>,
    initial_priors: BTreeMap<Vec<u8>, f64>,
    initial_root_value: f64,
    remaining: Vec<RootChoice>,
    shortlist: Vec<RootShortlistEvidence>,
}

#[cfg(test)]
fn resolve_root_choice(actions: &[Edge], choice: &RootChoice) -> usize {
    actions
        .iter()
        .position(|edge| edge.identity == choice.identity)
        .expect("frozen Gumbel witness remains exposed during the root budget")
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn gumbel_sequential_halving(
    nodes: &mut Vec<Node>,
    root: usize,
    factory: &Factory<'_>,
    config: &Config,
    counters: &mut Counters,
    tree_memory: &SearchMemoryBudget,
    controls: GumbelControls,
    case_id: &str,
    state_identity: &str,
) -> Result<RootDecision> {
    let frozen = freeze_gumbel_root(&nodes[root], controls, case_id, state_identity)?;
    let Some(frozen) = frozen else {
        return Ok(RootDecision {
            selected: None,
            policy_targets: None,
            eligible_ids: None,
            shortlist: Vec::new(),
            simulations: 0,
        });
    };
    finish_gumbel_root(
        nodes,
        root,
        factory,
        config,
        counters,
        tree_memory,
        controls,
        frozen,
    )
}

#[cfg(test)]
fn freeze_gumbel_root(
    node: &Node,
    controls: GumbelControls,
    case_id: &str,
    state_identity: &str,
) -> Result<Option<FrozenGumbelRoot>> {
    let actions = node.actions.as_ref().expect("expanded root");
    if actions.is_empty() {
        return Ok(None);
    }
    let eligible_ids = actions
        .iter()
        .map(|edge| edge.identity.clone())
        .collect::<HashSet<_>>();
    ensure!(
        eligible_ids.len() == actions.len(),
        "aic-learning.hierarchical-root-identity: initial root has duplicate physical action IDs"
    );
    // Widening renormalizes the live edge priors. Keep the mixture used to
    // select this root budget fixed for its ranking and training target.
    let initial_priors = actions
        .iter()
        .map(|edge| {
            (
                edge.identity.clone(),
                root_policy_prior(edge, controls.anchor_policy_mass),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let initial_root_value = node.value.unwrap_or_default();
    let gumbels = match controls.gumbel_assignment {
        GumbelAssignment::Rank => {
            deterministic_gumbels(controls.seed, case_id, state_identity, actions.len())
        }
        GumbelAssignment::ActionIdentity => {
            deterministic_action_gumbels(controls.seed, state_identity, actions)?
        }
    };
    let selected_indices = considered_root_actions(
        actions,
        &gumbels,
        controls.candidates,
        controls.anchor_candidates,
        controls.anchor_policy_mass,
    );
    let remaining = selected_indices
        .into_iter()
        .map(|index| RootChoice {
            identity: actions[index].identity.clone(),
            gumbel: gumbels[index],
            initial_rank: index,
        })
        .collect::<Vec<_>>();
    let shortlist = remaining
        .iter()
        .map(|choice| RootShortlistEvidence {
            physical_id: choice.identity.clone(),
            initial_rank: choice.initial_rank,
            gumbel_bits: choice.gumbel.to_bits(),
        })
        .collect();
    Ok(Some(FrozenGumbelRoot {
        eligible_ids,
        initial_priors,
        initial_root_value,
        remaining,
        shortlist,
    }))
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
fn finish_gumbel_root(
    nodes: &mut Vec<Node>,
    root: usize,
    factory: &Factory<'_>,
    config: &Config,
    counters: &mut Counters,
    tree_memory: &SearchMemoryBudget,
    controls: GumbelControls,
    frozen: FrozenGumbelRoot,
) -> Result<RootDecision> {
    let FrozenGumbelRoot {
        eligible_ids,
        initial_priors,
        initial_root_value,
        mut remaining,
        shortlist,
    } = frozen;
    let mut simulations = 0usize;
    while remaining.len() > 1 {
        let rounds_left =
            remaining.len().ilog2() as usize + usize::from(!remaining.len().is_power_of_two());
        let available = config.simulations_per_move - simulations;
        let visits_each = (available / (remaining.len() * rounds_left)).max(1);
        for choice in &remaining {
            for _ in 0..visits_each {
                simulate(
                    nodes,
                    root,
                    Some(&choice.identity),
                    factory,
                    config,
                    counters,
                    tree_memory,
                )?;
                simulations += 1;
            }
        }
        let transformed = completed_q_transform(
            nodes[root].actions.as_ref().expect("expanded root"),
            initial_root_value,
            controls.value_scale,
            controls.max_visit_init,
            Some(&eligible_ids),
        );
        remaining.sort_by(|left, right| {
            let actions = nodes[root].actions.as_ref().unwrap();
            let right_index = resolve_root_choice(actions, right);
            let left_index = resolve_root_choice(actions, left);
            let right_prior = initial_priors[&right.identity];
            let left_prior = initial_priors[&left.identity];
            (right.gumbel + right_prior.max(f64::MIN_POSITIVE).ln() + transformed[right_index])
                .total_cmp(
                    &(left.gumbel
                        + left_prior.max(f64::MIN_POSITIVE).ln()
                        + transformed[left_index]),
                )
                .then_with(|| left.initial_rank.cmp(&right.initial_rank))
        });
        remaining.truncate(remaining.len().div_ceil(2));
    }
    let selected = &remaining[0];
    while simulations < config.simulations_per_move {
        simulate(
            nodes,
            root,
            Some(&selected.identity),
            factory,
            config,
            counters,
            tree_memory,
        )?;
        simulations += 1;
    }
    let transformed = completed_q_transform(
        nodes[root].actions.as_ref().expect("expanded root"),
        initial_root_value,
        controls.value_scale,
        controls.max_visit_init,
        Some(&eligible_ids),
    );
    let policy_targets = improved_policy_with_priors(
        nodes[root].actions.as_ref().unwrap(),
        &transformed,
        controls.anchor_policy_mass,
        Some(&eligible_ids),
        Some(&initial_priors),
    );
    Ok(RootDecision {
        selected: {
            let index = resolve_root_choice(nodes[root].actions.as_ref().unwrap(), selected);
            nodes[root].actions.as_ref().unwrap()[index]
                .blocked_value
                .is_none()
                .then_some(index)
        },
        policy_targets: Some(policy_targets),
        eligible_ids: Some(eligible_ids),
        shortlist,
        simulations,
    })
}

#[cfg(test)]
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

#[cfg(test)]
fn root_policy_prior(edge: &Edge, anchor_policy_mass: f64) -> f64 {
    edge.prior * (1.0 - anchor_policy_mass) + edge.anchor_prior * anchor_policy_mass
}

#[cfg(test)]
fn root_prior_score(edge: &Edge, gumbel: f64, anchor_policy_mass: f64) -> f64 {
    gumbel
        + root_policy_prior(edge, anchor_policy_mass)
            .max(f64::MIN_POSITIVE)
            .ln()
}

#[cfg(test)]
fn completed_q_transform(
    actions: &[Edge],
    root_value: f64,
    value_scale: f64,
    max_visit_init: f64,
    eligible_ids: Option<&HashSet<Vec<u8>>>,
) -> Vec<f64> {
    let eligible = |edge: &Edge| eligible_ids.is_none_or(|ids| ids.contains(&edge.identity));
    let total_visits = actions
        .iter()
        .filter(|edge| eligible(edge))
        .map(|edge| u64::from(edge.visits))
        .sum::<u64>();
    let total_value = actions
        .iter()
        .filter(|edge| eligible(edge))
        .map(|edge| edge.value_sum)
        .sum::<f64>();
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
    let minimum = completed
        .iter()
        .zip(actions)
        .filter(|(_, edge)| eligible(edge))
        .map(|(q, _)| *q)
        .fold(root_value, f64::min);
    let maximum = completed
        .iter()
        .zip(actions)
        .filter(|(_, edge)| eligible(edge))
        .map(|(q, _)| *q)
        .fold(root_value, f64::max);
    let denominator = maximum - minimum;
    let max_visits = actions
        .iter()
        .filter(|edge| eligible(edge))
        .map(|edge| edge.visits)
        .max()
        .unwrap_or(0);
    let scale = finite_root_q_scale(max_visit_init, max_visits, value_scale);
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

fn finite_root_q_scale(max_visit_init: f64, max_visits: u32, value_scale: f64) -> f64 {
    ((max_visit_init + f64::from(max_visits)) * value_scale).min(f64::MAX)
}

#[cfg(test)]
fn improved_policy(
    actions: &[Edge],
    transformed_q: &[f64],
    anchor_policy_mass: f64,
    eligible_ids: Option<&HashSet<Vec<u8>>>,
) -> Vec<f64> {
    improved_policy_with_priors(
        actions,
        transformed_q,
        anchor_policy_mass,
        eligible_ids,
        None,
    )
}

#[cfg(test)]
fn improved_policy_with_priors(
    actions: &[Edge],
    transformed_q: &[f64],
    anchor_policy_mass: f64,
    eligible_ids: Option<&HashSet<Vec<u8>>>,
    initial_priors: Option<&BTreeMap<Vec<u8>, f64>>,
) -> Vec<f64> {
    let logits = actions
        .iter()
        .zip(transformed_q)
        .map(|(edge, q)| {
            if eligible_ids.is_some_and(|eligible| !eligible.contains(&edge.identity)) {
                return f64::NEG_INFINITY;
            }
            initial_priors
                .and_then(|priors| priors.get(&edge.identity).copied())
                .unwrap_or_else(|| root_policy_prior(edge, anchor_policy_mass))
                .max(f64::MIN_POSITIVE)
                .ln()
                + q
        })
        .collect::<Vec<_>>();
    let maximum = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if maximum == f64::NEG_INFINITY {
        return vec![0.0; actions.len()];
    }
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

#[cfg(test)]
fn deterministic_gumbels(seed: u64, case_id: &str, state_identity: &str, count: usize) -> Vec<f64> {
    let mut state = rank_gumbel_state(seed, case_id, state_identity);
    (0..count)
        .map(|_| gumbel_from_bits(splitmix64(&mut state)))
        .collect()
}

fn rank_gumbel_state(seed: u64, case_id: &str, state_identity: &str) -> u64 {
    let mut digest = Sha256::new();
    digest.update(seed.to_le_bytes());
    digest.update([0]);
    digest.update(case_id.as_bytes());
    digest.update([0]);
    digest.update(state_identity.as_bytes());
    let bytes = digest.finalize();
    u64::from_le_bytes(bytes[..8].try_into().expect("SHA-256 prefix"))
}

#[cfg(test)]
fn deterministic_action_gumbels(
    seed: u64,
    state_identity: &str,
    actions: &[Edge],
) -> Result<Vec<f64>> {
    actions
        .iter()
        .map(|edge| deterministic_action_gumbel(seed, state_identity, &edge.identity))
        .collect()
}

fn deterministic_action_gumbel(seed: u64, state_identity: &str, physical_id: &[u8]) -> Result<f64> {
    let mut prefix = Sha256::new();
    prefix.update(seed.to_le_bytes());
    prefix.update([0]);
    // State identity already hashes the problem and observation. Case IDs are
    // sampling metadata, so an identical physical state keeps the same noise.
    prefix.update(state_identity.as_bytes());
    prefix.update([0]);
    prefix.update(physical_id);
    let bytes = prefix.finalize();
    let bits = u64::from_le_bytes(bytes[..8].try_into().expect("SHA-256 prefix"));
    Ok(gumbel_from_bits(bits))
}

pub(crate) fn physical_action_identity(action: &MacroAction) -> Result<Vec<u8>> {
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
    forced_root_edge: Option<&[u8]>,
    factory: &Factory<'_>,
    config: &Config,
    counters: &mut Counters,
    tree_memory: &SearchMemoryBudget,
) -> Result<()> {
    let mut current = root;
    let mut path = Vec::<(usize, usize)>::new();
    let value = loop {
        if let Some(value) = simulation_leaf_value(&nodes[current].session, path.len(), config) {
            break value;
        }
        if nodes[current].actions.is_none() {
            break ensure_node_expanded(nodes, current, factory, counters, false, tree_memory)?;
        }
        if !nodes[current].pending_actions.is_empty() || !nodes[current].pending_updates.is_empty()
        {
            append_node_actions(
                nodes,
                current,
                factory,
                counters,
                tree_memory,
                Vec::new(),
                Vec::new(),
            )?;
        }
        maybe_widen_placements(nodes, current, factory, counters, tree_memory)?;
        maybe_widen_lower(nodes, current, factory, counters, tree_memory)?;
        let Some(actions) = nodes[current]
            .actions
            .as_ref()
            .filter(|actions| !actions.is_empty())
        else {
            break state_value(&nodes[current].session, config.bounds);
        };
        if actions.iter().all(|edge| edge.blocked_value.is_some()) {
            break state_value(&nodes[current].session, config.bounds);
        }
        let edge = if current == root && path.is_empty() {
            forced_root_edge
                .map(|identity| {
                    actions
                        .iter()
                        .position(|edge| edge.identity == identity)
                        .expect("frozen Gumbel witness remains exposed")
                })
                .unwrap_or_else(|| select_puct_edge(nodes, current, actions, config))
        } else {
            select_puct_edge(nodes, current, actions, config)
        };
        if let Some(blocked_value) = nodes[current].actions.as_ref().unwrap()[edge].blocked_value {
            break blocked_value;
        }
        path.push((current, edge));
        if let Some(child) = nodes[current].actions.as_ref().unwrap()[edge].child {
            current = child;
            continue;
        }
        let estimate = nodes[current]
            .session
            .clone_reservation_bytes()
            .saturating_mul(2)
            .saturating_add(std::mem::size_of::<Node>())
            .saturating_add(action_bytes(
                &nodes[current].actions.as_ref().unwrap()[edge].action,
            ));
        let mut child_memory = match TreeReservation::new(tree_memory, estimate) {
            Ok(reservation) => reservation,
            Err(stop) => {
                capture_tree_memory_stop(
                    counters,
                    nodes[current].session.revision(),
                    tree_memory,
                    stop.clone(),
                );
                nodes[current].no_macro_diagnostic = Some(tree_memory_diagnostic(
                    &stop,
                    nodes[current].session.revision(),
                ));
                let blocked_value = state_value(&nodes[current].session, config.bounds);
                nodes[current].actions.as_mut().unwrap()[edge].blocked_value = Some(blocked_value);
                path.pop();
                break blocked_value;
            }
        };
        let child_session = nodes[current].actions.as_ref().unwrap()[edge]
            .action
            .apply(
                &nodes[current].session,
                &config.candidate,
                config.repair_max_removed,
            )?;
        if let Err(stop) = child_memory.resize(
            child_session
                .clone_reservation_bytes()
                .saturating_add(std::mem::size_of::<Node>()),
        ) {
            capture_tree_memory_stop(
                counters,
                nodes[current].session.revision(),
                tree_memory,
                stop.clone(),
            );
            nodes[current].no_macro_diagnostic = Some(tree_memory_diagnostic(
                &stop,
                nodes[current].session.revision(),
            ));
            let blocked_value = state_value(&nodes[current].session, config.bounds);
            nodes[current].actions.as_mut().unwrap()[edge].blocked_value = Some(blocked_value);
            path.pop();
            break blocked_value;
        }
        let child = nodes.len();
        nodes.push(Node {
            session: Arc::new(child_session),
            memory: child_memory,
            visits: 0,
            value: None,
            actions: None,
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        });
        nodes[current].actions.as_mut().unwrap()[edge].child = Some(child);
        current = child;
        break ensure_node_expanded(nodes, current, factory, counters, false, tree_memory)?;
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
        .filter(|(_, edge)| edge.blocked_value.is_none())
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
        .or_else(|| actions.iter().position(|edge| edge.blocked_value.is_none()))
        .expect("unblocked action")
}

fn simulation_leaf_value(session: &Session, depth: usize, config: &Config) -> Option<f64> {
    (session.metrics().is_some() || depth == config.horizon)
        .then(|| state_value(session, config.bounds))
}

enum ScoredChildren {
    Ready(ScoredActionRows),
    MemoryStop(AllowanceStop),
}

struct ScoredActionRows {
    features: Vec<[f32; ACTION_FEATURES]>,
    entity_indices: Vec<i64>,
    anchor_scores: Vec<f64>,
    exact_value: f64,
    _memory: TreeReservation,
}

fn apply_scoring_children(
    session: &Session,
    actions: &[MacroAction],
    factory: &Factory<'_>,
    tree_memory: &SearchMemoryBudget,
) -> Result<ScoredChildren> {
    let row_bytes = actions
        .len()
        .saturating_mul(
            std::mem::size_of::<[f32; ACTION_FEATURES]>()
                + std::mem::size_of::<i64>()
                + std::mem::size_of::<f64>(),
        )
        .saturating_add(3 * std::mem::size_of::<Vec<u8>>());
    let row_memory = match TreeReservation::new(tree_memory, row_bytes) {
        Ok(reservation) => reservation,
        Err(stop) => return Ok(ScoredChildren::MemoryStop(stop)),
    };
    let mut features = Vec::with_capacity(actions.len());
    let mut entity_indices = Vec::with_capacity(actions.len());
    let mut anchor_scores = Vec::with_capacity(actions.len());
    let mut exact_value = state_value(session, factory.domain.placement);
    let placement_scorer = if actions
        .iter()
        .any(|action| matches!(action, MacroAction::Command(_)))
    {
        match PlacementActionScorer::prepare(session, factory.domain.placement, tree_memory)? {
            PreparedPlacement::Ready(scorer) => Some(scorer),
            PreparedPlacement::MemoryStop(stop) => return Ok(ScoredChildren::MemoryStop(stop)),
        }
    } else {
        None
    };
    let placement_page = placement_scorer
        .as_ref()
        .map(|scorer| scorer.begin_page(session))
        .transpose()?;
    for action in actions {
        if let MacroAction::Command(command) = action {
            let page = placement_page.as_ref().expect("placement scorer prepared");
            match page.score(command, tree_memory)? {
                ScoredPlacement::Ready(scored) => {
                    features.push(scored.features);
                    entity_indices.push(scored.entity_index);
                    anchor_scores.push(scored.anchor_score);
                    exact_value = exact_value.max(scored.child_value);
                }
                ScoredPlacement::MemoryStop(stop) => {
                    return Ok(ScoredChildren::MemoryStop(stop));
                }
            }
            continue;
        }
        let estimate = session
            .clone_reservation_bytes()
            .saturating_add(action_bytes(action))
            .saturating_add(8192);
        let mut reservation = match TreeReservation::new(tree_memory, estimate) {
            Ok(reservation) => reservation,
            Err(stop) => return Ok(ScoredChildren::MemoryStop(stop)),
        };
        let child = factory.apply_action(action, session)?;
        if let Err(stop) = reservation.resize(child.clone_reservation_bytes()) {
            return Ok(ScoredChildren::MemoryStop(stop));
        }
        features.push(action.features(session, &child, factory.domain.placement));
        entity_indices.push(action.entity_index(session.problem())?);
        anchor_scores.push(action.prior_score(session, &child, factory.domain.placement));
        exact_value = exact_value.max(state_value(&child, factory.domain.placement));
        drop(child);
        drop(reservation);
    }
    Ok(ScoredChildren::Ready(ScoredActionRows {
        features,
        entity_indices,
        anchor_scores,
        exact_value,
        _memory: row_memory,
    }))
}

fn ensure_node_expanded(
    nodes: &mut [Node],
    node: usize,
    factory: &Factory<'_>,
    counters: &mut Counters,
    exhaust_placements: bool,
    tree_memory: &SearchMemoryBudget,
) -> Result<f64> {
    let placement_retry_ready = nodes[node]
        .placement_pages
        .as_ref()
        .and_then(|pages| pages.retry_after_live)
        .is_some_and(|at_stop| tree_memory.live_bytes() < at_stop);
    if placement_retry_ready && nodes[node].actions.as_ref().is_some_and(Vec::is_empty) {
        nodes[node].actions = None;
        nodes[node].value = None;
    }
    if nodes[node].actions.is_some() {
        return Ok(nodes[node]
            .value
            .unwrap_or_else(|| state_value(&nodes[node].session, factory.domain.placement)));
    }
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
        counters.repair_unknown,
    );
    let previous_pages = nodes[node].placement_pages.take();
    let (mut actions, placement_pages, placement_stop) = factory.placement_actions(
        &nodes[node].session,
        exhaust_placements,
        Some(tree_memory),
        previous_pages,
    )?;
    counters.expansions += 1;
    if let Some(stop) = placement_stop {
        capture_tree_memory_stop(
            counters,
            nodes[node].session.revision(),
            tree_memory,
            stop.clone(),
        );
        nodes[node].no_macro_diagnostic = Some(tree_memory_diagnostic(
            &stop,
            nodes[node].session.revision(),
        ));
        nodes[node].placement_pages = Some(placement_pages);
        let value = state_value(&nodes[node].session, factory.domain.placement);
        nodes[node].value = Some(value);
        nodes[node].actions = Some(Vec::new());
        return Ok(value);
    }
    counters.placement_samples += actions.len();
    counters.generated_macro_samples += actions.len();
    let mut lower = LowerPager::new(
        Arc::clone(&nodes[node].session),
        factory,
        counters,
        tree_memory,
        nodes[node].memory.bytes(),
    )?;
    actions.extend(lower.first_actions(factory, counters)?);
    while actions.is_empty() && lower.pending() > 0 && !lower.all_pending_blocked_on_memory() {
        actions.extend(lower.next_actions(factory, counters)?);
    }
    // Only close a zero-action wait when no other tree branch can run an
    // owner or expose a distinct edge that might free shared memory.
    let other_release_path =
        nodes.iter().enumerate().any(|(other, peer)| {
            other != node
                && (peer.lower.as_ref().is_some_and(|pager| {
                    pager.pending() > 0 && !pager.all_pending_blocked_on_memory()
                }) || peer.actions.as_ref().is_some_and(|edges| {
                    edges
                        .iter()
                        .any(|edge| edge.blocked_value.is_none() && edge.child != Some(node))
                }))
        });
    let terminal_memory_wait =
        actions.is_empty() && lower.all_pending_blocked_on_memory() && !other_release_path;
    if terminal_memory_wait {
        if let Some(stop) = lower.pending_memory_stop() {
            capture_tree_memory_stop(counters, nodes[node].session.revision(), tree_memory, stop);
        }
    }
    nodes[node].lower_loaded_pages = 1;
    nodes[node].lower = if terminal_memory_wait {
        lower.release();
        None
    } else {
        Some(lower)
    };
    nodes[node].placement_pages = Some(placement_pages);
    nodes[node].no_macro_diagnostic = actions
        .is_empty()
        .then(|| no_macro_reason(counters, before, nodes[node].session.revision()));
    if actions.is_empty() {
        let value = state_value(&nodes[node].session, factory.domain.placement);
        nodes[node].value = Some(value);
        nodes[node].actions = Some(vec![]);
        return Ok(value);
    }
    let rows = match apply_scoring_children(&nodes[node].session, &actions, factory, tree_memory)? {
        ScoredChildren::Ready(rows) => rows,
        ScoredChildren::MemoryStop(stop) => {
            capture_tree_memory_stop(
                counters,
                nodes[node].session.revision(),
                tree_memory,
                stop.clone(),
            );
            nodes[node].no_macro_diagnostic = Some(tree_memory_diagnostic(
                &stop,
                nodes[node].session.revision(),
            ));
            let value = state_value(&nodes[node].session, factory.domain.placement);
            nodes[node].pending_actions = actions;
            nodes[node].pending_retry_live = Some(tree_memory.live_bytes());
            nodes[node].value = Some(value);
            nodes[node].actions = Some(Vec::new());
            return Ok(value);
        }
    };
    let ScoredActionRows {
        features,
        entity_indices,
        anchor_scores,
        exact_value,
        _memory: _row_memory_guard,
    } = rows;
    let scores = factory.score_action_rows(
        &nodes[node].session,
        &features,
        &entity_indices,
        &anchor_scores,
    )?;
    // Expansion already paid for and natively applied every exposed macro.
    // Preserve that exact one-step evidence. The current outer checkpoint was
    // trained on fully placed routing states, so its completion head is not a
    // valid value estimate while any facility or terminal remains unplaced.
    let all_entities_placed = !nodes[node].session.has_unplaced_entities();
    let value = if all_entities_placed {
        scores.model_value.max(exact_value)
    } else {
        exact_value
    };
    let edge_bytes = actions.iter().fold(0usize, |total, action| {
        total
            // LowerPager keeps the transferred witness reservation for the
            // node lifetime; placements have no pager owner.
            .saturating_add(if matches!(action, MacroAction::Command(_)) {
                action_bytes(action)
            } else {
                0
            })
            .saturating_add(std::mem::size_of::<Edge>())
            .saturating_add(256)
    });
    if let Err(stop) = nodes[node]
        .memory
        .resize(nodes[node].memory.bytes().saturating_add(edge_bytes))
    {
        capture_tree_memory_stop(
            counters,
            nodes[node].session.revision(),
            tree_memory,
            stop.clone(),
        );
        nodes[node].no_macro_diagnostic = Some(tree_memory_diagnostic(
            &stop,
            nodes[node].session.revision(),
        ));
        nodes[node].pending_actions = actions;
        nodes[node].pending_retry_live = Some(tree_memory.live_bytes());
        nodes[node].value = Some(value);
        nodes[node].actions = Some(Vec::new());
        return Ok(value);
    }
    let mut edges = actions
        .into_iter()
        .zip(features)
        .zip(entity_indices)
        .zip(anchor_scores)
        .zip(scores.priors)
        .zip(scores.anchor_priors)
        .map(
            |(((((action, features), entity_index), anchor_score), prior), anchor_prior)| {
                Ok(Edge {
                    identity: physical_action_identity(&action)?,
                    action,
                    features,
                    entity_index,
                    anchor_score,
                    prior,
                    anchor_prior,
                    visits: 0,
                    value_sum: 0.0,
                    child: None,
                    blocked_value: None,
                })
            },
        )
        .collect::<Result<Vec<_>>>()?;
    edges.sort_by(|left, right| right.prior.total_cmp(&left.prior));
    nodes[node].actions = Some(edges);
    if let Some(pages) = &mut nodes[node].placement_pages {
        pages.page_memory = None;
    }
    nodes[node].value = Some(value);
    Ok(value)
}

fn maybe_widen_placements(
    nodes: &mut [Node],
    node: usize,
    factory: &Factory<'_>,
    counters: &mut Counters,
    tree_memory: &SearchMemoryBudget,
) -> Result<()> {
    if !nodes[node].pending_actions.is_empty() || !nodes[node].pending_updates.is_empty() {
        return Ok(());
    }
    let desired_pages = (f64::from(nodes[node].visits.max(1)).sqrt().floor() as usize).max(1);
    let Some(pages) = nodes[node].placement_pages.as_ref() else {
        return Ok(());
    };
    if desired_pages <= pages.loaded_pages || pages.exhausted.into_iter().all(|exhausted| exhausted)
    {
        return Ok(());
    }
    let mut pages = nodes[node]
        .placement_pages
        .take()
        .expect("placement pages were present");
    let session = nodes[node].session.clone();
    let additional_result =
        factory.next_placement_page(&session, &mut pages, counters, tree_memory);
    let page_stop = pages.memory_stop.clone();
    nodes[node].placement_pages = Some(pages);
    let additional = additional_result?;
    if let Some(stop) = page_stop {
        nodes[node].no_macro_diagnostic = Some(tree_memory_diagnostic(&stop, session.revision()));
        return Ok(());
    }
    let result = append_node_actions(
        nodes,
        node,
        factory,
        counters,
        tree_memory,
        additional,
        Vec::new(),
    );
    if result.is_ok() && nodes[node].pending_actions.is_empty() {
        if let Some(pages) = &mut nodes[node].placement_pages {
            pages.page_memory = None;
        }
    }
    result
}

fn maybe_widen_lower(
    nodes: &mut [Node],
    node: usize,
    factory: &Factory<'_>,
    counters: &mut Counters,
    tree_memory: &SearchMemoryBudget,
) -> Result<()> {
    if !nodes[node].pending_actions.is_empty() || !nodes[node].pending_updates.is_empty() {
        return Ok(());
    }
    let desired_pages = (f64::from(nodes[node].visits.max(1)).sqrt().floor() as usize).max(1);
    let memory_retry_ready = nodes[node].lower.as_ref().is_some_and(|lower| {
        lower.hash_memory_retry_ready()
            || lower.publication_memory_retry_ready()
            || lower.scope_memory_retry_ready()
    });
    if desired_pages <= nodes[node].lower_loaded_pages && !memory_retry_ready {
        return Ok(());
    }
    let Some(lower) = nodes[node].lower.as_mut() else {
        return Ok(());
    };
    if lower.pending() == 0 {
        return Ok(());
    }
    let additional = lower.next_actions(factory, counters)?;
    let updates = lower.take_updated_routes();
    if !memory_retry_ready
        && !lower.hash_memory_pending()
        && !lower.publication_memory_retry_ready()
        && !lower.scope_memory_pending()
    {
        nodes[node].lower_loaded_pages += 1;
    }
    append_node_actions(
        nodes,
        node,
        factory,
        counters,
        tree_memory,
        additional,
        updates,
    )
}

fn append_node_actions(
    nodes: &mut [Node],
    node: usize,
    factory: &Factory<'_>,
    counters: &mut Counters,
    tree_memory: &SearchMemoryBudget,
    additional: Vec<MacroAction>,
    updates: Vec<RouteUpdate>,
) -> Result<()> {
    nodes[node].pending_actions.extend(additional);
    nodes[node].pending_updates.extend(updates);
    if nodes[node].pending_actions.is_empty() && nodes[node].pending_updates.is_empty() {
        return Ok(());
    }
    if nodes[node]
        .pending_retry_live
        .is_some_and(|at_stop| tree_memory.live_bytes() >= at_stop)
    {
        return Ok(());
    }
    let new_ids = nodes[node]
        .pending_actions
        .iter()
        .map(physical_action_identity)
        .collect::<Result<Vec<_>>>()?;
    let existing = nodes[node].actions.as_ref().expect("expanded node");
    let mut ids = HashSet::new();
    ids.try_reserve(existing.len().saturating_add(new_ids.len()))?;
    ids.extend(existing.iter().map(|edge| edge.identity.as_slice()));
    for id in &new_ids {
        ensure!(
            ids.insert(id.as_slice()),
            "aic-learning.hierarchical-root-identity: appended physical action ID duplicates an exposed witness"
        );
    }
    let session = Arc::clone(&nodes[node].session);
    let additional = &nodes[node].pending_actions;
    let rows = match apply_scoring_children(&session, &additional, factory, tree_memory)? {
        ScoredChildren::Ready(rows) => rows,
        ScoredChildren::MemoryStop(stop) => {
            capture_tree_memory_stop(counters, session.revision(), tree_memory, stop.clone());
            nodes[node].no_macro_diagnostic =
                Some(tree_memory_diagnostic(&stop, session.revision()));
            nodes[node].pending_retry_live = Some(tree_memory.live_bytes());
            return Ok(());
        }
    };
    let ScoredActionRows {
        features: new_features,
        entity_indices: new_entity_indices,
        anchor_scores: new_anchor_scores,
        exact_value,
        _memory: _row_memory_guard,
    } = rows;
    let existing_count = nodes[node].actions.as_ref().expect("expanded node").len();
    let mut features = Vec::with_capacity(existing_count + additional.len());
    let mut entity_indices = Vec::with_capacity(existing_count + additional.len());
    let mut anchor_scores = Vec::with_capacity(existing_count + additional.len());
    for edge in nodes[node].actions.as_ref().expect("expanded node") {
        features.push(edge.features);
        entity_indices.push(edge.entity_index);
        anchor_scores.push(edge.anchor_score);
    }
    for update in &nodes[node].pending_updates {
        let index = nodes[node]
            .actions
            .as_ref()
            .expect("expanded node")
            .iter()
            .position(|edge| matches!(&edge.action, MacroAction::Route(candidate) if candidate.connection == update.connection && candidate.id == update.id))
            .ok_or_else(|| anyhow::anyhow!("updated route witness was not published"))?;
        let MacroAction::Route(candidate) =
            &nodes[node].actions.as_ref().expect("expanded node")[index].action
        else {
            unreachable!("matching route update")
        };
        features[index][13..16].fill(0.0);
        mark_route_provenance_values(
            &mut features[index],
            &candidate.provenance,
            &update.additional_provenance,
        );
    }
    features.extend(&new_features);
    entity_indices.extend(&new_entity_indices);
    anchor_scores.extend(&new_anchor_scores);
    let scores = factory.score_action_rows(&session, &features, &entity_indices, &anchor_scores)?;
    let edge_bytes = additional.iter().fold(0usize, |total, action| {
        total
            .saturating_add(if matches!(action, MacroAction::Command(_)) {
                action_bytes(action)
            } else {
                0
            })
            .saturating_add(std::mem::size_of::<Edge>())
            .saturating_add(256)
    });
    if let Err(stop) = nodes[node]
        .memory
        .resize(nodes[node].memory.bytes().saturating_add(edge_bytes))
    {
        capture_tree_memory_stop(counters, session.revision(), tree_memory, stop.clone());
        nodes[node].no_macro_diagnostic = Some(tree_memory_diagnostic(&stop, session.revision()));
        nodes[node].pending_retry_live = Some(tree_memory.live_bytes());
        return Ok(());
    }
    let additional = std::mem::take(&mut nodes[node].pending_actions);
    let updates = std::mem::take(&mut nodes[node].pending_updates);
    nodes[node].pending_retry_live = None;
    nodes[node].no_macro_diagnostic = None;
    let edges = nodes[node].actions.as_mut().expect("expanded node");
    for update in updates {
        let edge = edges
            .iter_mut()
            .find(|edge| matches!(&edge.action, MacroAction::Route(candidate) if candidate.connection == update.connection && candidate.id == update.id))
            .expect("pending update was validated before scoring");
        let MacroAction::Route(candidate) = &mut edge.action else {
            unreachable!("matching route update")
        };
        candidate.additional_provenance = update.additional_provenance;
    }
    for (index, ((edge, prior), anchor_prior)) in edges
        .iter_mut()
        .zip(&scores.priors)
        .zip(&scores.anchor_priors)
        .enumerate()
    {
        edge.features = features[index];
        edge.prior = *prior;
        edge.anchor_prior = *anchor_prior;
    }
    edges.extend(
        additional
            .into_iter()
            .zip(new_ids)
            .zip(new_features)
            .zip(new_entity_indices)
            .zip(new_anchor_scores)
            .zip(scores.priors.into_iter().skip(existing_count))
            .zip(scores.anchor_priors.into_iter().skip(existing_count))
            .map(
                |(
                    (((((action, identity), features), entity_index), anchor_score), prior),
                    anchor_prior,
                )| {
                    Ok(Edge {
                        identity,
                        action,
                        features,
                        entity_index,
                        anchor_score,
                        prior,
                        anchor_prior,
                        visits: 0,
                        value_sum: 0.0,
                        child: None,
                        blocked_value: None,
                    })
                },
            )
            .collect::<Result<Vec<_>>>()?,
    );
    edges.sort_by(|left, right| right.prior.total_cmp(&left.prior));
    let all_entities_placed = !session.has_unplaced_entities();
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
        !session.has_active_route(),
        "aic-learning.hierarchical-prefix: case {} left an active route",
        case.id
    );
    Ok(session)
}

fn construction_progress(session: &Session) -> f64 {
    session.construction_progress()
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
    let (completed_connections, total_connections) = session.connection_progress_counts();
    let fully_validated = session.metrics().is_some();
    let metrics = session.metrics();
    SearchResult {
        algorithm: algorithm.into(),
        status: if fully_validated {
            "complete"
        } else {
            "incomplete"
        }
        .into(),
        fully_validated,
        termination,
        no_macro_diagnostic: matches!(
            termination,
            SearchTermination::NoMacroActions | SearchTermination::ResourceStopped
        )
        .then_some(no_macro_diagnostic)
        .flatten(),
        completed_connections,
        total_connections,
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
    session
        .hierarchical_state_sha256()
        .map_err(anyhow::Error::new)
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
            && config.max_live_search_bytes > 0
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
    let (sha256, encoded_bytes) = publish_json_identity(output, &value, None)?;
    Ok(Published {
        value,
        sha256,
        encoded_bytes,
    })
}

fn publish_panel_json(output: &Path, panel: Panel) -> Result<Published<Panel>> {
    let (sha256, encoded_bytes) = publish_json_identity(output, &panel, Some(MAX_PANEL_BYTES))?;
    Ok(Published {
        value: panel,
        sha256,
        encoded_bytes,
    })
}

fn publish_json_identity(
    output: &Path,
    value: &impl Serialize,
    byte_limit: Option<u64>,
) -> Result<(String, u64)> {
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
    if let Some(limit) = byte_limit.filter(|limit| digested.bytes > *limit) {
        let bytes = digested.bytes;
        drop(digested);
        let _ = fs::remove_file(&temporary);
        anyhow::bail!(
            "aic-learning.hierarchical-panel-resource: {} has {} bytes; limit is {} bytes",
            output.display(),
            bytes,
            limit
        );
    }
    digested.inner.sync_all()?;
    let identity = digested.finish();
    fs::rename(temporary, output)?;
    Ok(identity)
}

pub(crate) fn load_published_panel(path: &Path) -> Result<Published<Panel>> {
    load_published_panel_with_limit(path, MAX_PANEL_BYTES)
}

fn load_published_panel_with_limit(path: &Path, limit: u64) -> Result<Published<Panel>> {
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("stat {}", path.display()))?;
    ensure!(
        metadata.len() <= limit,
        "aic-learning.hierarchical-panel-resource: {} has {} bytes; limit is {} bytes",
        path.display(),
        metadata.len(),
        limit
    );
    load_published_json_reader(path, BufReader::new(file.take(limit + 1)), Some(limit))
}

pub(crate) fn load_published_report(path: &Path) -> Result<Published<Report>> {
    let published: Published<Report> =
        load_published_json(path).map_err(|error: anyhow::Error| {
            if error
                .chain()
                .any(|cause| cause.downcast_ref::<serde_json::Error>().is_some())
            {
                error.context(format!(
                "aic-learning.hierarchical-report-schema: {} cannot decode tagged v12 root records",
                path.display()
            ))
            } else {
                error
            }
        })?;
    ensure!(
        published.value.schema == REPORT_SCHEMA,
        "aic-learning.hierarchical-report-schema: {} uses unsupported schema {} (expected {})",
        path.display(),
        published.value.schema,
        REPORT_SCHEMA
    );
    for case in &published.value.cases {
        for record in &case.mcts.root_records {
            validate_root_record_lineage(record).with_context(|| {
                format!(
                    "aic-learning.hierarchical-root-lineage: {} case {} revision {}",
                    path.display(),
                    case.id,
                    record.state_revision
                )
            })?;
        }
    }
    Ok(published)
}

pub(crate) fn retire_replay_data(path: &Path) -> Result<()> {
    root_catalog::publication::retire_replay_data(path)
}

pub(crate) fn validate_root_record_lineage(record: &RootRecord) -> Result<()> {
    match &record.target {
        RootRecordTarget::PuctDense(dense) => validate_dense_root_lineage(dense, false),
        RootRecordTarget::GumbelCatalog { descriptor } => {
            root_catalog::publication::validate_descriptor_lineage(
                descriptor,
                &record.state_sha256,
                record.state_revision,
            )
        }
        #[cfg(test)]
        RootRecordTarget::GumbelDenseOracle(dense) => validate_dense_root_lineage(dense, true),
    }
}

fn validate_dense_root_lineage(dense: &DenseRootRecord, gumbel_oracle: bool) -> Result<()> {
    let selected = dense
        .actions
        .get(dense.selected_action)
        .ok_or_else(|| anyhow::anyhow!("selected action is outside the recorded root"))?;
    ensure!(
        dense.selected_physical_id.as_deref()
            == Some(physical_action_identity(&selected.action)?.as_slice()),
        "selected physical action ID differs from its witness"
    );
    let mut ids = HashSet::new();
    ids.try_reserve(dense.actions.len())?;
    let mut eligible = HashSet::new();
    eligible.try_reserve(dense.actions.len())?;
    for action in &dense.actions {
        let id = physical_action_identity(&action.action)?;
        ensure!(
            ids.insert(id.clone()),
            "duplicate physical action ID in root"
        );
        if gumbel_oracle {
            let is_eligible = action
                .gumbel_eligible
                .ok_or_else(|| anyhow::anyhow!("Gumbel action lacks initial eligibility"))?;
            let target = action
                .policy_target
                .ok_or_else(|| anyhow::anyhow!("Gumbel action lacks frozen policy target"))?;
            ensure!(
                target.is_finite() && target >= 0.0,
                "invalid frozen policy target"
            );
            if is_eligible {
                eligible.insert(id);
            } else {
                ensure!(target == 0.0, "late Gumbel action has nonzero target");
            }
        }
    }
    if gumbel_oracle {
        let cap = dense
            .gumbel_shortlist_cap
            .ok_or_else(|| anyhow::anyhow!("Gumbel dense oracle lacks shortlist limit"))?;
        ensure!(
            dense.initial_eligible_actions == Some(eligible.len())
                && eligible.contains(dense.selected_physical_id.as_ref().unwrap())
                && !dense.gumbel_shortlist.is_empty()
                && dense.gumbel_shortlist.len() <= cap,
            "Gumbel initial eligibility or shortlist differs from record"
        );
        let mut ranks = HashSet::new();
        let mut shortlist_ids = HashSet::new();
        for entry in &dense.gumbel_shortlist {
            ensure!(
                eligible.contains(&entry.physical_id)
                    && ranks.insert(entry.initial_rank)
                    && entry.initial_rank < eligible.len()
                    && shortlist_ids.insert(&entry.physical_id)
                    && f64::from_bits(entry.gumbel_bits).is_finite(),
                "Gumbel shortlist contains ambiguous ID, rank or noise"
            );
        }
        ensure!(
            shortlist_ids.contains(dense.selected_physical_id.as_ref().unwrap()),
            "selected Gumbel action is absent from shortlist"
        );
    } else {
        ensure!(
            dense.gumbel_shortlist.is_empty()
                && dense.gumbel_shortlist_cap.is_none()
                && dense.initial_eligible_actions.is_none()
                && dense.actions.iter().all(|action| {
                    action.gumbel_eligible.is_none() && action.policy_target.is_none()
                }),
            "PUCT root contains Gumbel-only lineage"
        );
    }
    Ok(())
}

fn load_published_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Published<T>> {
    let file = fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    load_published_json_reader(path, BufReader::new(file), None)
}

fn load_published_json_reader<T: for<'de> Deserialize<'de>, R: Read>(
    path: &Path,
    source: R,
    byte_limit: Option<u64>,
) -> Result<Published<T>> {
    let mut reader = DigestReader::new(source);
    let parsed = {
        let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
        T::deserialize(&mut deserializer).and_then(|value| {
            deserializer.end()?;
            Ok(value)
        })
    };
    let (sha256, encoded_bytes) = reader.finish();
    if let Some(limit) = byte_limit {
        ensure!(
            encoded_bytes <= limit,
            "aic-learning.hierarchical-panel-resource: {} has at least {} bytes; limit is {} bytes",
            path.display(),
            encoded_bytes,
            limit
        );
    }
    let value = parsed.with_context(|| format!("parse {}", path.display()))?;
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
    #[ignore = "local root-action layout probe"]
    fn plan00_root_layout_probe() {
        eprintln!(
            "plan00 sizes macro_action={} edge={} root_action_record={} root_record={} feature_row={} identity_vec={}",
            std::mem::size_of::<MacroAction>(),
            std::mem::size_of::<Edge>(),
            std::mem::size_of::<RootActionRecord>(),
            std::mem::size_of::<RootRecord>(),
            std::mem::size_of::<[f32; ACTION_FEATURES]>(),
            std::mem::size_of::<Vec<u8>>()
        );
    }

    #[test]
    #[ignore = "set AIC_ROOT_INTEGRATED_PANEL for a bounded local root cost probe"]
    fn plan00_integrated_root_cost_probe() -> Result<()> {
        use std::{fs, io::Write, time::Instant};

        let _guard = crate::TORCH_TEST.lock().unwrap();
        let panel_path = std::env::var("AIC_ROOT_INTEGRATED_PANEL")?;
        let panel: serde_json::Value = serde_json::from_slice(&fs::read(panel_path)?)?;
        let case = &panel["cases"][0];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let loaded = aic_layout_gen::io::load_catalogs(&root.join("data/game/normalized"))?;
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            Arc::new(loaded.catalogs),
            serde_json::from_value(panel["limits"].clone())?,
        )?;
        let prefix = std::env::var("AIC_ROOT_INTEGRATED_PREFIX")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        for command in case["prefix"].as_array().unwrap().iter().take(prefix) {
            let step = session.step(serde_json::from_value(command.clone())?);
            ensure!(step.accepted && step.diagnostic.is_none(), "{step:?}");
        }
        let candidate: CandidateConfig = serde_json::from_value(panel["candidate"].clone())?;
        let domain_config = DomainConfig {
            placement: candidate.bounds,
            extension_lengths: vec![1, 2, 4],
        };
        let domain = session.learning_placement_domain(&domain_config)?;
        let input = session.macro_learning_input(&domain_config)?;
        let model_config = crate::schema::OuterModelConfig {
            schema: crate::schema::OuterModelConfig::SCHEMA.into(),
            encoder: crate::schema::ConstructiveModelConfig {
                schema: crate::schema::ConstructiveModelConfig::SCHEMA.into(),
                width: 32,
                message_rounds: 1,
            },
            width: 32,
        };
        let (order, state) = OuterModel::initial_state(&model_config, 51)?;
        let model = OuterModel::load(&model_config, &order, &state, Device::Cpu)?;
        let output = root.join("artifacts/plan00-integrated-root-scores.bin");
        let mut writer = std::io::BufWriter::new(fs::File::create(&output)?);
        let page_size = 256usize;
        let batch_size = 65_536usize;
        let max_pages = std::env::var("AIC_ROOT_INTEGRATED_MAX_PAGES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        let mut rows = Vec::with_capacity(batch_size);
        let mut references = Vec::with_capacity(batch_size);
        let mut anchors = Vec::with_capacity(batch_size);
        let mut count = 0usize;
        let mut pages = 0usize;
        let mut child_seconds = 0.0;
        let mut batch_seconds = 0.0;
        let mut forward_seconds = 0.0;
        let started = Instant::now();
        let mut flush = |rows: &mut Vec<[f32; ACTION_FEATURES]>,
                         references: &mut Vec<i64>,
                         anchors: &mut Vec<f64>|
         -> Result<()> {
            if rows.is_empty() {
                return Ok(());
            }
            let now = Instant::now();
            let batch =
                OuterBatch::from_rows(&[input.as_ref()], &[rows], &[references], Device::Cpu)?;
            batch_seconds += now.elapsed().as_secs_f64();
            let now = Instant::now();
            let prediction = tch::no_grad(|| model.forward(&batch))?;
            let logits = Vec::<f32>::try_from(prediction.logits.to_device(Device::Cpu))?;
            forward_seconds += now.elapsed().as_secs_f64();
            for (logit, anchor) in logits.into_iter().zip(anchors.iter().copied()) {
                writer.write_all(&logit.to_le_bytes())?;
                writer.write_all(&anchor.to_le_bytes())?;
            }
            rows.clear();
            references.clear();
            anchors.clear();
            Ok(())
        };
        for kind in [CommandKind::Place, CommandKind::PlaceTerminal] {
            let mut offset = 0usize;
            loop {
                let page = domain.command_page(kind, offset, page_size)?;
                for factored in &page.commands {
                    let action = MacroAction::Command(factored.command.command.clone());
                    let now = Instant::now();
                    let child = action.apply(&session, &candidate, 0)?;
                    rows.push(action.features(&session, &child, candidate.bounds));
                    references.push(action.entity_index(session.problem())?);
                    anchors.push(action.prior_score(&session, &child, candidate.bounds));
                    child_seconds += now.elapsed().as_secs_f64();
                    count += 1;
                    if rows.len() == batch_size {
                        flush(&mut rows, &mut references, &mut anchors)?;
                    }
                }
                offset = page.next_offset;
                pages += 1;
                if page.exhausted || pages >= max_pages {
                    break;
                }
            }
            if pages >= max_pages {
                break;
            }
        }
        flush(&mut rows, &mut references, &mut anchors)?;
        drop(flush);
        writer.flush()?;
        drop(writer);
        let bytes = fs::metadata(&output)?.len();
        ensure!(
            bytes == count as u64 * 12,
            "score probe file length differs"
        );
        fs::remove_file(output)?;
        eprintln!(
            "plan00 integrated count={count} pages={pages} seconds={:.3} child_seconds={child_seconds:.3} batch_seconds={batch_seconds:.3} forward_seconds={forward_seconds:.3} score_bytes={bytes}",
            started.elapsed().as_secs_f64()
        );
        Ok(())
    }

    #[test]
    #[ignore = "set AIC_ROOT_INTEGRATED_PANEL for local analytic placement parity"]
    fn plan00_analytic_placement_parity_probe() -> Result<()> {
        use std::{fs, time::Instant};

        let panel_path = std::env::var("AIC_ROOT_INTEGRATED_PANEL")?;
        let panel: serde_json::Value = serde_json::from_slice(&fs::read(panel_path)?)?;
        let case = &panel["cases"][0];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let loaded = aic_layout_gen::io::load_catalogs(&root.join("data/game/normalized"))?;
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            Arc::new(loaded.catalogs),
            serde_json::from_value(panel["limits"].clone())?,
        )?;
        let prefix = std::env::var("AIC_ROOT_INTEGRATED_PREFIX")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        for command in case["prefix"].as_array().unwrap().iter().take(prefix) {
            let step = session.step(serde_json::from_value(command.clone())?);
            ensure!(step.accepted && step.diagnostic.is_none(), "{step:?}");
        }
        let candidate: CandidateConfig = serde_json::from_value(panel["candidate"].clone())?;
        let domain_config = DomainConfig {
            placement: candidate.bounds,
            extension_lengths: vec![1, 2, 4],
        };
        let domain = session.learning_placement_domain(&domain_config)?;
        let problem = session.problem();
        let total = problem.blocks.len() + problem.terminals.len() + problem.connections.len();
        let completed = problem
            .blocks
            .iter()
            .filter(|block| session.placed_entity_position(&block.id).is_some())
            .count()
            + problem
                .terminals
                .iter()
                .filter(|terminal| session.placed_entity_position(&terminal.id).is_some())
                .count()
            + session.connection_progress_counts().0;
        let before_progress = construction_progress(&session);
        let after_progress = (completed + 1) as f64 / total.max(1) as f64;
        let bounds = candidate.bounds;
        let span = bounds.width().max(bounds.height()).max(1) as f64;
        let scale = span as f32;
        let center_x = (bounds.min_x + bounds.max_x) as f64 * 0.5;
        let center_y = (bounds.min_y + bounds.max_y) as f64 * 0.5;
        let fixed_distance = problem
            .connections
            .iter()
            .filter_map(|connection| {
                session
                    .placed_entity_position(&connection.source)
                    .zip(session.placed_entity_position(&connection.target))
                    .map(|(source, target)| source.distance(target) as f64 / span)
            })
            .sum::<f64>();
        let fixed_terms = problem
            .connections
            .iter()
            .map(|connection| {
                session
                    .placed_entity_position(&connection.source)
                    .zip(session.placed_entity_position(&connection.target))
                    .map(|(source, target)| source.distance(target) as f64 / span)
            })
            .collect::<Vec<_>>();
        let mut incident_by_entity =
            std::collections::HashMap::<String, Vec<(usize, Option<Point>, bool)>>::new();
        for (index, connection) in problem.connections.iter().enumerate() {
            if connection.source == connection.target {
                incident_by_entity
                    .entry(connection.source.clone())
                    .or_default()
                    .push((index, None, true));
            } else {
                incident_by_entity
                    .entry(connection.source.clone())
                    .or_default()
                    .push((
                        index,
                        session.placed_entity_position(&connection.target),
                        false,
                    ));
                incident_by_entity
                    .entry(connection.target.clone())
                    .or_default()
                    .push((
                        index,
                        session.placed_entity_position(&connection.source),
                        false,
                    ));
            }
        }
        let scorer_budget = SearchMemoryBudget::new(64 * 1024 * 1024);
        let production_scorer =
            match PlacementActionScorer::prepare(&session, candidate.bounds, &scorer_budget)? {
                PreparedPlacement::Ready(scorer) => scorer,
                PreparedPlacement::MemoryStop(stop) => {
                    anyhow::bail!("production placement scorer stopped: {stop:?}")
                }
            };
        let production_page = production_scorer.begin_page(&session)?;
        let offsets = [0, 1_000, 1_000_000, 5_000_000];
        let mut checked = 0usize;
        let mut max_anchor_error = 0.0f64;
        let analytic_started = Instant::now();
        let mut analytic_seconds = 0.0;
        let mut ordered_seconds = 0.0;
        let mut prepared_seconds = 0.0;
        let mut child_seconds = 0.0;
        for offset in offsets {
            let page = domain.command_page(CommandKind::Place, offset, 256)?;
            for factored in &page.commands {
                let command = &factored.command.command;
                let (entity, position, rotation, kind) = match command {
                    Command::Place {
                        block,
                        position,
                        rotation,
                    } => (block.as_str(), *position, Some(*rotation), 0),
                    Command::PlaceTerminal { terminal, position } => {
                        (terminal.as_str(), *position, None, 1)
                    }
                    _ => unreachable!("placement page returned another command"),
                };
                let now = Instant::now();
                let mut row = [0.0f32; ACTION_FEATURES];
                row[kind] = 1.0;
                row[3] = before_progress as f32;
                row[4] = after_progress as f32;
                row[5] = row[4] - row[3];
                row[6] = position.x as f32 / scale;
                row[7] = position.y as f32 / scale;
                if let Some(rotation) = rotation {
                    row[8] = (rotation as f32).to_radians().sin();
                    row[9] = (rotation as f32).to_radians().cos();
                }
                let incident_distance = problem
                    .connections
                    .iter()
                    .filter(|connection| connection.source == entity || connection.target == entity)
                    .filter_map(|connection| {
                        let source = if connection.source == entity {
                            Some(position)
                        } else {
                            session.placed_entity_position(&connection.source)
                        };
                        let target = if connection.target == entity {
                            Some(position)
                        } else {
                            session.placed_entity_position(&connection.target)
                        };
                        source
                            .zip(target)
                            .map(|(source, target)| source.distance(target) as f64 / span)
                    })
                    .sum::<f64>();
                let center_distance = ((position.x as f64 - center_x).abs()
                    + (position.y as f64 - center_y).abs())
                    / span;
                let anchor = (after_progress - before_progress) * 8.0
                    + (-center_distance * 0.08 - (fixed_distance + incident_distance) * 0.12);
                analytic_seconds += now.elapsed().as_secs_f64();

                let now = Instant::now();
                let ordered_distance = problem
                    .connections
                    .iter()
                    .filter_map(|connection| {
                        let source = if connection.source == entity {
                            Some(position)
                        } else {
                            session.placed_entity_position(&connection.source)
                        };
                        let target = if connection.target == entity {
                            Some(position)
                        } else {
                            session.placed_entity_position(&connection.target)
                        };
                        source
                            .zip(target)
                            .map(|(source, target)| source.distance(target) as f64 / span)
                    })
                    .sum::<f64>();
                let ordered_anchor = (after_progress - before_progress) * 8.0
                    + (-center_distance * 0.08 - ordered_distance * 0.12);
                ordered_seconds += now.elapsed().as_secs_f64();

                let now = Instant::now();
                let incidents = incident_by_entity
                    .get(entity)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]);
                let mut incident_index = 0usize;
                let prepared_distance = fixed_terms
                    .iter()
                    .enumerate()
                    .filter_map(|(index, fixed)| {
                        if incidents.get(incident_index).map(|incident| incident.0) != Some(index) {
                            return *fixed;
                        }
                        let (_, peer, self_connection) = incidents[incident_index];
                        incident_index += 1;
                        if self_connection {
                            Some(0.0)
                        } else {
                            peer.map(|peer| position.distance(peer) as f64 / span)
                        }
                    })
                    .sum::<f64>();
                let prepared_anchor = (after_progress - before_progress) * 8.0
                    + (-center_distance * 0.08 - prepared_distance * 0.12);
                prepared_seconds += now.elapsed().as_secs_f64();

                let action = MacroAction::Command(command.clone());
                let now = Instant::now();
                let child = action.apply(&session, &candidate, 0)?;
                let actual_row = action.features(&session, &child, bounds);
                let actual_anchor = action.prior_score(&session, &child, bounds);
                child_seconds += now.elapsed().as_secs_f64();
                let production = match production_page.score(command, &scorer_budget)? {
                    ScoredPlacement::Ready(score) => score,
                    ScoredPlacement::MemoryStop(stop) => {
                        anyhow::bail!("production placement scorer stopped: {stop:?}")
                    }
                };
                ensure!(
                    production.features == actual_row,
                    "production feature mismatch"
                );
                ensure!(production.entity_index == action.entity_index(problem)?);
                ensure!(production.anchor_score.to_bits() == actual_anchor.to_bits());
                ensure!(production.child_value == state_value(&child, bounds));
                ensure!(
                    ordered_anchor.to_bits() == actual_anchor.to_bits(),
                    "ordered analytic anchor differs at offset {offset}: {ordered_anchor} versus {actual_anchor}"
                );
                ensure!(
                    prepared_anchor.to_bits() == actual_anchor.to_bits(),
                    "prepared analytic anchor differs at offset {offset}: {prepared_anchor} versus {actual_anchor}"
                );
                ensure!(
                    child.metrics().is_none(),
                    "sample includes completion transition"
                );
                ensure!(
                    row == actual_row,
                    "analytic feature mismatch at offset {offset}"
                );
                ensure!(
                    after_progress == state_value(&child, bounds),
                    "analytic child value mismatch at offset {offset}"
                );
                max_anchor_error = max_anchor_error.max((anchor - actual_anchor).abs());
                checked += 1;
            }
        }
        ensure!(max_anchor_error <= 1e-12, "anchor error {max_anchor_error}");
        eprintln!(
            "plan00 analytic parity checked={checked} anchor_max_error={max_anchor_error:.3e} analytic_seconds={analytic_seconds:.6} ordered_seconds={ordered_seconds:.6} prepared_seconds={prepared_seconds:.6} child_seconds={child_seconds:.6} elapsed={:.6}",
            analytic_started.elapsed().as_secs_f64()
        );
        if std::env::var("AIC_ROOT_ANALYTIC_ALL").as_deref() == Ok("1") {
            let started = Instant::now();
            let mut offset = 0usize;
            let mut count = 0usize;
            let mut checksum = 0.0f64;
            loop {
                let page = domain.command_page(CommandKind::Place, offset, 256)?;
                for factored in &page.commands {
                    let score =
                        match production_page.score(&factored.command.command, &scorer_budget)? {
                            ScoredPlacement::Ready(score) => score,
                            ScoredPlacement::MemoryStop(stop) => {
                                anyhow::bail!("production placement scorer stopped: {stop:?}")
                            }
                        };
                    checksum += score.anchor_score;
                    count += 1;
                }
                offset = page.next_offset;
                if page.exhausted {
                    break;
                }
            }
            eprintln!(
                "plan01 production analytic full count={count} seconds={:.3} checksum={checksum:.9}",
                started.elapsed().as_secs_f64()
            );
        }
        Ok(())
    }

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
    fn placement_domain_refusal_preserves_source_and_retries_after_release() -> Result<()> {
        let panel: Panel = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-search/panel.json"
        ))?;
        let case = &panel.cases[0];
        let session = Session::new(
            case.problem.clone(),
            crate::constructive::test_support::catalogs(),
            panel.limits,
        )?;
        let config = DomainConfig {
            placement: Bounds {
                min_x: 0,
                min_y: 0,
                max_x: 20,
                max_y: 20,
            },
            extension_lengths: vec![1, 2, 4],
        };
        let peak = session.learning_placement_domain_peak_bytes(&config)?;
        let budget = SearchMemoryBudget::new(peak + 1024);
        let pressure = TreeReservation::new(&budget, 1025).expect("pressure fits");
        let mut pages = PlacementPages {
            offsets: [3, 5],
            exhausted: [false; 2],
            loaded_pages: 1,
            revision: session.revision(),
            bounds: config.placement,
            domain: None,
            domain_memory: None,
            page_memory: None,
            memory_stop: None,
            retry_after_live: None,
        };
        let stop = pages.ensure_domain(&session, &config, Some(&budget))?;
        assert!(matches!(stop, Some(stop) if stop.reason == "aggregate-memory-limit"));
        assert!(pages.domain.is_none());
        assert_eq!(pages.offsets, [3, 5]);
        assert_eq!(budget.live_bytes(), 1025);
        assert!(
            pages
                .ensure_domain(&session, &config, Some(&budget))?
                .is_some()
        );
        drop(pressure);
        assert!(
            pages
                .ensure_domain(&session, &config, Some(&budget))?
                .is_none()
        );
        let domain = pages.domain.as_ref().expect("admitted domain");
        assert_eq!(budget.live_bytes(), domain.retained_bytes());
        assert_eq!(pages.offsets, [3, 5]);
        let domain = Arc::clone(domain);
        let before = (pages.offsets, pages.exhausted);
        let failed = load_placement_pair(&mut pages, |kind, offset| {
            if kind == CommandKind::PlaceTerminal {
                Err(anyhow::anyhow!("injected second-family failure"))
            } else {
                domain
                    .command_page(kind, offset, 4)
                    .map_err(anyhow::Error::new)
            }
        });
        assert!(failed.is_err());
        assert_eq!((pages.offsets, pages.exhausted), before);
        let retried = load_placement_pair(&mut pages, |kind, offset| {
            domain
                .command_page(kind, offset, 4)
                .map_err(anyhow::Error::new)
        })?;
        let mut reference = PlacementPages {
            offsets: before.0,
            exhausted: before.1,
            loaded_pages: 1,
            revision: session.revision(),
            bounds: config.placement,
            domain: None,
            domain_memory: None,
            page_memory: None,
            memory_stop: None,
            retry_after_live: None,
        };
        let uninterrupted = load_placement_pair(&mut reference, |kind, offset| {
            domain
                .command_page(kind, offset, 4)
                .map_err(anyhow::Error::new)
        })?;
        assert_eq!(
            (pages.offsets, pages.exhausted),
            (reference.offsets, reference.exhausted)
        );
        assert_eq!(
            retried
                .iter()
                .map(physical_action_identity)
                .collect::<Result<Vec<_>>>()?,
            uninterrupted
                .iter()
                .map(physical_action_identity)
                .collect::<Result<Vec<_>>>()?
        );
        drop(domain);
        drop(pages);
        assert_eq!(budget.live_bytes(), 0);
        Ok(())
    }

    #[test]
    fn placement_page_staging_refusal_keeps_both_offsets() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let panel: Panel = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-search/panel.json"
        ))?;
        let session = Session::new(
            panel.cases[0].problem.clone(),
            crate::constructive::test_support::catalogs(),
            panel.limits,
        )?;
        let search: Config = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-curriculum/search.json"
        ))?;
        let domain_config = DomainConfig {
            placement: search.bounds,
            extension_lengths: search.extension_lengths.clone(),
        };
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
        let factory = Factory::new(
            &model,
            "test",
            Device::Cpu,
            domain_config.clone(),
            &search.candidate,
            0,
            64,
            1,
            0.0,
            false,
            None,
        )?;
        let domain_peak = session.learning_placement_domain_peak_bytes(&domain_config)?;
        let page_peak = placement_page_peak_bytes(&session, 64);
        let retained = session
            .learning_placement_domain(&domain_config)?
            .retained_bytes();
        assert!(page_peak > domain_peak - retained);
        let first_budget = SearchMemoryBudget::new(domain_peak + page_peak);
        let first_pressure =
            TreeReservation::new(&first_budget, page_peak).expect("domain still fits");
        let (first_actions, first_pages, first_stop) =
            factory.placement_actions(&session, false, Some(&first_budget), None)?;
        assert!(first_actions.is_empty());
        assert!(first_stop.is_some());
        assert!(first_pages.domain.is_some());
        assert_eq!(first_pages.offsets, [0; 2]);
        drop(first_pressure);
        let (first_actions, first_pages, first_stop) =
            factory.placement_actions(&session, false, Some(&first_budget), Some(first_pages))?;
        assert!(first_stop.is_none());
        assert!(!first_actions.is_empty());
        assert!(first_pages.page_memory.is_some());
        let scoring_budget = SearchMemoryBudget::new(64 * 1024 * 1024);
        let scored =
            match apply_scoring_children(&session, &first_actions, &factory, &scoring_budget)? {
                ScoredChildren::Ready(rows) => rows,
                ScoredChildren::MemoryStop(stop) => {
                    anyhow::bail!("integration scoring stopped: {stop:?}")
                }
            };
        let mut expected_value = state_value(&session, search.bounds);
        for (index, action) in first_actions.iter().enumerate() {
            let child = factory.apply_action(action, &session)?;
            assert_eq!(
                scored.features[index],
                action.features(&session, &child, search.bounds)
            );
            assert_eq!(
                scored.entity_indices[index],
                action.entity_index(session.problem())?
            );
            assert_eq!(
                scored.anchor_scores[index].to_bits(),
                action
                    .prior_score(&session, &child, search.bounds)
                    .to_bits()
            );
            expected_value = expected_value.max(state_value(&child, search.bounds));
        }
        assert_eq!(scored.exact_value, expected_value);
        drop(scored);
        assert_eq!(scoring_budget.live_bytes(), 0);
        drop(first_actions);
        drop(first_pages);
        assert_eq!(first_budget.live_bytes(), 0);
        let budget = SearchMemoryBudget::new(domain_peak + page_peak + 1024);
        let mut pages = PlacementPages {
            offsets: [0; 2],
            exhausted: [false; 2],
            loaded_pages: 0,
            revision: session.revision(),
            bounds: domain_config.placement,
            domain: None,
            domain_memory: None,
            page_memory: None,
            memory_stop: None,
            retry_after_live: None,
        };
        assert!(
            pages
                .ensure_domain(&session, &domain_config, Some(&budget))?
                .is_none()
        );
        let pressure_bytes = budget.limit_bytes() - budget.live_bytes() - page_peak + 1;
        let pressure = TreeReservation::new(&budget, pressure_bytes).expect("pressure fits");
        let mut counters = Counters::default();
        assert!(
            factory
                .next_placement_page(&session, &mut pages, &mut counters, &budget)?
                .is_empty()
        );
        assert_eq!(pages.offsets, [0; 2]);
        assert_eq!(pages.loaded_pages, 0);
        assert_eq!(counters.request_stops_total, 1);
        assert!(pages.page_memory.is_none());
        drop(pressure);
        let actions = factory.next_placement_page(&session, &mut pages, &mut counters, &budget)?;
        assert!(!actions.is_empty());
        assert!(pages.offsets[0] > 0);
        assert!(pages.page_memory.is_some());
        drop(actions);
        drop(pages);
        assert_eq!(budget.live_bytes(), 0);
        Ok(())
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
        let reason = no_macro_reason(&counters, (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0), 7);
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
        let before = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
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
        let mut next = draft
            .route_candidates(
                &Request {
                    state_revision: draft.revision(),
                    connection: "second".into(),
                },
                &config,
            )?
            .candidates
            .remove(0);
        let before_next = draft.clone();
        draft.apply_route_candidate(&next)?;
        next.physical_cost = before_next
            .physical_cost_between(&draft, std::slice::from_ref(&next.connection))
            .map_err(anyhow::Error::new)?;
        let mut restored = draft
            .route_candidates(
                &Request {
                    state_revision: draft.revision(),
                    connection: "first".into(),
                },
                &config,
            )?
            .candidates
            .remove(0);
        let before_restored = draft.clone();
        draft.apply_route_candidate(&restored)?;
        restored.physical_cost = before_restored
            .physical_cost_between(&draft, std::slice::from_ref(&restored.connection))
            .map_err(anyhow::Error::new)?;
        let expanded = next.expanded + restored.expanded;
        let action = MacroAction::Repair {
            request: request.clone(),
            candidate: RepairCandidate {
                schema: aic_pilot_native::constructive::routing_candidates::REPAIR_CANDIDATE_SCHEMA
                    .into(),
                state_revision: request.state_revision,
                removed: vec!["first".into()],
                replacements: vec![next, restored],
                expanded,
                physical_cost: session
                    .physical_cost_between(&draft, &["first".into(), "second".into()])
                    .map_err(anyhow::Error::new)?,
                charged_work: Default::default(),
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
        let features = action.features(&session, &child, config.bounds);
        assert_eq!(features[2], 2.0);
        let MacroAction::Repair { candidate, .. } = &action else {
            unreachable!("repair action fixture")
        };
        assert_eq!(
            features[30],
            candidate.physical_cost.route_steps_delta as f32
                / config.bounds.width().max(config.bounds.height()) as f32
        );
        assert_eq!(
            features[33],
            candidate.physical_cost.component_count_delta as f32
        );
        Ok(())
    }

    #[test]
    fn negotiated_repair_reaches_the_outer_action_batch() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
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
        let candidate: CandidateConfig = serde_json::from_value(fixture["candidate"].clone())?;
        let bounds = candidate.bounds;
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
        let domain = DomainConfig {
            placement: bounds,
            extension_lengths: vec![1, 2, 4],
        };
        let forbidden = Factory::new(
            &model,
            "test",
            Device::Cpu,
            domain.clone(),
            &candidate,
            0,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut forbidden_counters = Counters::default();
        let baseline_start = std::time::Instant::now();
        let forbidden_batch = forbidden.action_batch(&session, &mut forbidden_counters, false)?;
        let baseline_elapsed = baseline_start.elapsed();
        assert!(forbidden_batch.actions.is_empty());
        assert_eq!(forbidden_counters.repair_tasks, 0);
        let factory = Factory::new(
            &model,
            "test",
            Device::Cpu,
            domain.clone(),
            &candidate,
            1,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut counters = Counters::default();
        let negotiated_start = std::time::Instant::now();
        let batch = factory.action_batch(&session, &mut counters, false)?;
        let negotiated_elapsed = negotiated_start.elapsed();
        let repairs = batch
            .actions
            .iter()
            .filter_map(|action| match action {
                MacroAction::Repair { request, candidate } => Some((request, candidate)),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(
            !repairs.is_empty(),
            "repair report: {:?}",
            counters.repair_attempt_samples
        );
        assert!(
            repairs.len() >= 2,
            "priced guided variants should survive semantic dedup"
        );
        assert!(repairs.iter().any(|(_, repair)| {
            repair
                .replacements
                .iter()
                .any(|route| matches!(route.provenance, Provenance::Guided { .. }))
        }));
        assert_eq!(repairs[0].0.allowed_removals, vec!["first"]);
        assert_eq!(repairs[0].1.removed, vec!["first"]);
        let tree_memory =
            aic_pilot_native::constructive::routing_candidates::SearchMemoryBudget::new(
                1024 * 1024 * 1024,
            );
        let mut page_allowance = RequestAllowance::with_memory_budget(
            candidate.request_allowance.clone(),
            Some(tree_memory.clone()),
        );
        let mut repair_pager = repair_pager::RepairPager::new(
            Arc::new(session.clone()),
            vec!["second".into()],
            &factory,
            &mut page_allowance,
        )?
        .expect("one committed route permits repair");
        let mut page_counters = Counters::default();
        let mut saw_page_yield = false;
        for _ in 0..200 {
            let page = repair_pager.next_page(
                &factory,
                &mut page_counters,
                &mut page_allowance,
                1,
                1_000,
                100_000,
                Instant::now() + std::time::Duration::from_secs(5),
            )?;
            saw_page_yield |= matches!(page.status, repair_pager::RepairPageStatus::Pending);
            if !repair_pager.candidates().is_empty() {
                break;
            }
            if let repair_pager::RepairPageStatus::Unknown(stop) = page.status {
                return Err(anyhow::anyhow!(
                    "repair page stopped before a candidate: {stop:?}"
                ));
            }
        }
        assert!(
            saw_page_yield,
            "repair did not retain a route page boundary"
        );
        assert!(!repair_pager.candidates().is_empty());
        session.repaired_candidate(repair_pager.request(), &repair_pager.candidates()[0])?;
        repair_pager.release(&mut page_allowance);
        assert_eq!(tree_memory.live_bytes(), 0);
        let mut wider_candidate = candidate.clone();
        wider_candidate.max_candidates = 6;
        let wider_factory = Factory::new(
            &model,
            "test",
            Device::Cpu,
            domain.clone(),
            &wider_candidate,
            1,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut lower_counters = Counters::default();
        let mut lower = LowerPager::new(
            Arc::new(session.clone()),
            &wider_factory,
            &mut lower_counters,
            &tree_memory,
            0,
        )?;
        let hash_workspace = session.hierarchical_state_hash_workspace_bytes();
        let pressure = lower_pager::TreeReservation::new(
            &tree_memory,
            1024 * 1024 * 1024 - hash_workspace + 1,
        )
        .expect("pre-hash pressure fits the aggregate budget");
        assert!(
            lower
                .first_actions(&wider_factory, &mut lower_counters)?
                .is_empty()
        );
        assert!(lower.hash_memory_pending());
        drop(pressure);
        let first_actions = lower.first_actions(&wider_factory, &mut lower_counters)?;
        assert!(
            first_actions
                .iter()
                .any(|action| matches!(action, MacroAction::Repair { .. }))
        );
        lower.release();
        assert_eq!(tree_memory.live_bytes(), 0);
        let mut one_expansion = wider_candidate.clone();
        one_expansion.request_allowance.expansions = 1;
        let one_expansion_factory = Factory::new(
            &model,
            "test",
            Device::Cpu,
            domain.clone(),
            &one_expansion,
            1,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut zero_half_counters = Counters::default();
        let mut zero_half = LowerPager::new(
            Arc::new(session.clone()),
            &one_expansion_factory,
            &mut zero_half_counters,
            &tree_memory,
            0,
        )?;
        assert!(
            zero_half
                .first_actions(&wider_factory, &mut zero_half_counters)
                .is_err(),
            "a pager must reject a different factory before doing work"
        );
        assert!(
            zero_half
                .first_actions(&one_expansion_factory, &mut zero_half_counters)?
                .is_empty()
        );
        assert!(
            zero_half.pending() > 0,
            "zero temporary half must retain the task"
        );
        assert_eq!(zero_half_counters.request_stops_total, 0);
        zero_half.release();
        assert_eq!(tree_memory.live_bytes(), 0);
        let integrated = Config {
            schema: CONFIG_SCHEMA.into(),
            bounds,
            extension_lengths: vec![1, 2, 4],
            candidate: wider_candidate.clone(),
            max_live_search_bytes: 1024 * 1024 * 1024,
            repair_max_removed: 1,
            cpu_threads: 1,
            placement_page: 4,
            beam_width: 1,
            run_beam: false,
            simulations_per_move: 2,
            horizon: 1,
            max_moves: 1,
            exploration: 1.0,
            root_selection: RootSelection::Puct,
            anchor_prior_mass: 0.0,
            compiled_inference: false,
        };
        let search = mcts_search(session.clone(), &wider_factory, &integrated, "repair-page")?;
        assert!(
            search.root_records[0]
                .dense()
                .unwrap()
                .actions
                .iter()
                .any(|record| { matches!(record.action, MacroAction::Repair { .. }) })
        );
        let mut tiny_memory = integrated.clone();
        tiny_memory.max_live_search_bytes = 1;
        let stopped = mcts_search(session.clone(), &wider_factory, &tiny_memory, "memory-stop")?;
        assert_eq!(stopped.termination, SearchTermination::ResourceStopped);
        let diagnostic = stopped.no_macro_diagnostic.expect("typed memory stop");
        assert_eq!(diagnostic.code, "hierarchical.aggregate-memory-limit");
        assert!(diagnostic.message.contains("attempted"));
        let commit_start = std::time::Instant::now();
        let child = batch
            .actions
            .iter()
            .find_map(|action| match action {
                MacroAction::Repair { .. } => Some(action.apply(&session, &candidate, 1)),
                _ => None,
            })
            .expect("repair action")?;
        let commit_elapsed = commit_start.elapsed();
        if std::env::var_os("AIC_PLAN04_MEASURE").is_some() {
            let storage_start = std::time::Instant::now();
            let stored = serde_json::to_vec(&batch.actions)?;
            eprintln!(
                "plan04-local baseline_ms={} negotiated_ms={} commit_ms={} storage_ms={} stored_bytes={} actions={} request_peak_bytes={}",
                baseline_elapsed.as_millis(),
                negotiated_elapsed.as_millis(),
                commit_elapsed.as_millis(),
                storage_start.elapsed().as_millis(),
                stored.len(),
                batch.actions.len(),
                counters
                    .repair_attempt_samples
                    .first()
                    .map_or(0, |sample| sample.allowance.peak_live_bytes)
            );
        }
        assert!(child.observe().fully_validated);
        assert_eq!(counters.repair_samples, repairs.len());
        assert!(counters.repair_priced_edges > 0);

        let mut capped = candidate.clone();
        capped.request_allowance.max_variants = 1;
        let capped_factory = Factory::new(
            &model,
            "test",
            Device::Cpu,
            domain,
            &capped,
            1,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut capped_counters = Counters::default();
        let mut allowance = RequestAllowance::new(capped.request_allowance.clone());
        let report = capped_factory.negotiated_repair_actions(
            &session,
            &["second".into()],
            capped.max_candidates,
            &mut capped_counters,
            &mut allowance,
        )?;
        assert_eq!(report.summary.attempts, 1);
        assert_eq!(
            report.summary.termination,
            negotiated::NegotiationTermination::Unknown
        );
        assert!(
            !report.actions.is_empty(),
            "later limit must retain legal candidates"
        );
        assert_eq!(
            report.summary.stop.as_ref().and_then(|stop| stop.category),
            Some(WorkKind::Variants)
        );

        let mut anchor_only = capped.clone();
        anchor_only.request_allowance.inference_elements = 1;
        let anchor_factory = Factory::new(
            &model,
            "test",
            Device::Cpu,
            DomainConfig {
                placement: bounds,
                extension_lengths: vec![1, 2, 4],
            },
            &anchor_only,
            1,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut anchor_counters = Counters::default();
        let mut anchor_allowance = RequestAllowance::new(anchor_only.request_allowance.clone());
        let anchor_report = anchor_factory.negotiated_repair_actions(
            &session,
            &["second".into()],
            anchor_only.max_candidates,
            &mut anchor_counters,
            &mut anchor_allowance,
        )?;
        assert!(!anchor_report.actions.is_empty());
        assert!(anchor_counters.repair_guidance_fallbacks >= 2);
        let mut outer_fallback_counters = Counters::default();
        let outer_fallback =
            anchor_factory.action_batch(&session, &mut outer_fallback_counters, false)?;
        assert!(
            outer_fallback
                .actions
                .iter()
                .any(|action| matches!(action, MacroAction::Repair { .. }))
        );
        assert!(outer_fallback_counters.repair_guidance_fallbacks > 0);
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
        let tree_memory =
            aic_pilot_native::constructive::routing_candidates::SearchMemoryBudget::new(
                1024 * 1024 * 1024,
            );
        let mut page_allowance = RequestAllowance::with_memory_budget(
            candidate.request_allowance.clone(),
            Some(tree_memory.clone()),
        );
        let mut network_pager = network_pager::NetworkPager::new(
            &session,
            &state_sha256(&session)?,
            vec!["output-a".into(), "output-b".into()],
            &mut page_allowance,
        )?;
        let mut page_counters = Counters::default();
        let mut saw_yield = false;
        let mut saw_variant_stop = false;
        for page_number in 0..100 {
            let page = network_pager.next_page(
                &session,
                &factory,
                &mut page_counters,
                &mut page_allowance,
                1,
                0,
                if page_number == 0 { 100 } else { 10_000 },
                10_000,
                Instant::now() + std::time::Duration::from_secs(5),
            )?;
            assert!(page.candidate_ids.len() <= 1);
            assert!(page.expanded <= if page_number == 0 { 100 } else { 10_000 });
            assert!(page.scans <= 10_000);
            saw_yield |= matches!(page.status, network_pager::NetworkPageStatus::Pending);
            if let network_pager::NetworkPageStatus::Unknown(stop) = page.status {
                saw_variant_stop = stop.category == Some(WorkKind::Variants);
                break;
            }
        }
        assert!(
            saw_yield,
            "network member route did not retain a page boundary"
        );
        assert!(
            saw_variant_stop,
            "completed first variant did not retain its typed stop"
        );
        assert_eq!(network_pager.candidates().len(), 1);
        assert_eq!(
            network_pager.candidates()[0].routes[0].connection,
            "output-a"
        );
        network_pager.release(&session, &mut page_allowance);
        assert_eq!(tree_memory.live_bytes(), 0);
        let mut held_allowance = RequestAllowance::new(candidate.request_allowance.clone());
        let mut held_pager = network_pager::NetworkPager::new(
            &session,
            &state_sha256(&session)?,
            vec!["output-a".into(), "output-b".into()],
            &mut held_allowance,
        )?;
        let held_page = held_pager.next_page(
            &session,
            &factory,
            &mut Counters::default(),
            &mut held_allowance,
            1,
            1,
            100,
            10_000,
            Instant::now() + std::time::Duration::from_secs(5),
        )?;
        assert!(matches!(
            held_page.status,
            network_pager::NetworkPageStatus::Pending
        ));
        assert_eq!(
            held_allowance.usage().categories.get(&WorkKind::Variants),
            None
        );
        let released_page = held_pager.next_page(
            &session,
            &factory,
            &mut Counters::default(),
            &mut held_allowance,
            1,
            0,
            100,
            10_000,
            Instant::now() + std::time::Duration::from_secs(5),
        )?;
        assert!(matches!(
            released_page.status,
            network_pager::NetworkPageStatus::Pending
        ));
        assert_eq!(
            held_allowance.usage().categories.get(&WorkKind::Variants),
            Some(&1)
        );
        held_pager.release(&session, &mut held_allowance);
        let mut anchor_candidate = candidate.clone();
        anchor_candidate.request_allowance.inference_elements = 1;
        anchor_candidate.request_allowance.max_variants = 2;
        let anchor_factory = Factory::new(
            &model,
            "test",
            Device::Cpu,
            DomainConfig {
                placement: bounds,
                extension_lengths: vec![1, 2, 4],
            },
            &anchor_candidate,
            0,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut outer_counters = Counters::default();
        let outer = anchor_factory.action_batch(&session, &mut outer_counters, false)?;
        assert!(
            outer
                .actions
                .iter()
                .any(|action| matches!(action, MacroAction::Network(_)))
        );
        assert!(
            outer_counters
                .request_stops
                .iter()
                .any(|sample| sample.stop.category == Some(WorkKind::InferenceElements))
        );
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
        let without_work = |mut candidates: Vec<NetworkCandidate>| {
            for candidate in &mut candidates {
                candidate.charged_work = AllowanceUsage::default();
                for route in &mut candidate.routes {
                    route.charged_work = AllowanceUsage::default();
                }
            }
            candidates
        };
        assert_eq!(
            without_work(tight),
            without_work(full),
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
        assert!(incompatible_counters.request_stops_total > 0);
        assert_eq!(diagnostic.code, "hierarchical.request-work-limit");
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

    #[test]
    fn panel_publication_and_loading_share_a_bounded_byte_identity() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let fixture = root.join("docs/benchmarks/hierarchical-search/panel.json");
        let panel: Panel = serde_json::from_slice(&fs::read(fixture).unwrap()).unwrap();
        let encoded = serde_json::to_vec_pretty(&panel).unwrap();
        let limit = encoded.len() as u64;
        let path = std::env::temp_dir().join(format!(
            "aic-hierarchical-panel-size-{}.json",
            std::process::id()
        ));
        let refused = path.with_file_name(format!(
            "aic-hierarchical-panel-refused-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&refused);

        let (sha256, bytes) = publish_json_identity(&path, &panel, Some(limit)).unwrap();
        let loaded = load_published_panel_with_limit(&path, limit).unwrap();
        assert_eq!(bytes, limit);
        assert_eq!(loaded.encoded_bytes, limit);
        assert_eq!(loaded.sha256, sha256);
        assert_eq!(loaded.value.cases.len(), panel.cases.len());

        let failure = publish_json_identity(&refused, &panel, Some(limit - 1)).unwrap_err();
        assert!(failure.to_string().contains("hierarchical-panel-resource"));
        assert!(failure.to_string().contains(&limit.to_string()));
        assert!(failure.to_string().contains(&(limit - 1).to_string()));
        assert!(!refused.exists());
        assert!(
            !refused
                .with_file_name(format!(
                    ".aic-hierarchical-panel-refused-{}.json.tmp",
                    std::process::id()
                ))
                .exists()
        );

        // The descriptor was admitted at exactly the limit. Simulate a file
        // growing before deserialization reads it, without a timing race.
        let file = fs::File::open(&path).unwrap();
        assert_eq!(file.metadata().unwrap().len(), limit);
        let mut append = fs::OpenOptions::new().append(true).open(&path).unwrap();
        append.write_all(b" ").unwrap();
        let failure = load_published_json_reader::<Panel, _>(
            &path,
            BufReader::new(file.take(limit + 1)),
            Some(limit),
        )
        .err()
        .expect("oversized panel must be rejected");
        assert!(failure.to_string().contains("hierarchical-panel-resource"));
        assert!(failure.to_string().contains(&(limit + 1).to_string()));
        assert!(load_published_panel_with_limit(&path, limit).is_err());
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
        let action = MacroAction::Command(Command::Connect);
        Edge {
            identity: physical_action_identity(&action).unwrap(),
            action,
            features: [0.0; ACTION_FEATURES],
            entity_index: 0,
            anchor_score: 0.0,
            prior,
            anchor_prior,
            visits: 0,
            value_sum: 0.0,
            child: None,
            blocked_value: None,
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

        let command_edge = |command| {
            let action = MacroAction::Command(command);
            Edge {
                identity: physical_action_identity(&action).unwrap(),
                action,
                features: [0.0; ACTION_FEATURES],
                entity_index: 0,
                anchor_score: 0.0,
                prior: 0.5,
                anchor_prior: 0.5,
                visits: 0,
                value_sum: 0.0,
                child: None,
                blocked_value: None,
            }
        };
        let forward = [
            command_edge(Command::Connect),
            command_edge(Command::AbortRoute),
        ];
        let frozen = RootChoice {
            identity: forward[0].identity.clone(),
            gumbel: 1.25,
            initial_rank: 0,
        };
        let mut sorted = [
            command_edge(Command::Connect),
            command_edge(Command::AbortRoute),
        ];
        sorted.swap(0, 1);
        assert_eq!(resolve_root_choice(&sorted, &frozen), 1);
        assert_eq!(frozen.gumbel, 1.25);
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
            additional_provenance: Vec::new(),
            physical_cost: Default::default(),
            charged_work: Default::default(),
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
        let late_action = MacroAction::Route(route.clone());
        let mut different_route = route.clone();
        different_route.commands.push(Command::AbortRoute);
        assert_ne!(
            physical_action_identity(&MacroAction::Route(route)).unwrap(),
            physical_action_identity(&MacroAction::Route(different_route)).unwrap()
        );

        let actions = vec![edge(0.8, 0.5), edge(0.2, 0.5)];
        let policy = improved_policy(&actions, &[0.0, 5.0], 0.5, None);
        assert!((policy.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(policy[1] > policy[0]);

        let initial = [
            command_edge(Command::Connect),
            command_edge(Command::AbortRoute),
        ];
        let eligible = initial
            .iter()
            .map(|edge| edge.identity.clone())
            .collect::<HashSet<_>>();
        let late = late_action;
        let late_edge = Edge {
            identity: physical_action_identity(&late).unwrap(),
            action: late,
            features: [0.0; ACTION_FEATURES],
            entity_index: 0,
            anchor_score: 0.0,
            prior: 0.8,
            anchor_prior: 0.8,
            visits: 0,
            value_sum: 0.0,
            child: None,
            blocked_value: None,
        };
        let reordered = [
            late_edge,
            command_edge(Command::AbortRoute),
            command_edge(Command::Connect),
        ];
        let masked = improved_policy(&reordered, &[10.0, 1.0, 0.0], 0.5, Some(&eligible));
        assert_eq!(masked[0], 0.0);
        assert!((masked.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!(masked[1] > masked[2]);
    }

    #[test]
    fn gumbel_eligible_q_transform_ignores_late_root_edges() {
        let make = |command, visits, value_sum| {
            let action = MacroAction::Command(command);
            Edge {
                identity: physical_action_identity(&action).unwrap(),
                action,
                features: [0.0; ACTION_FEATURES],
                entity_index: 0,
                anchor_score: 0.0,
                prior: 0.5,
                anchor_prior: 0.5,
                visits,
                value_sum,
                child: None,
                blocked_value: None,
            }
        };
        let initial = [
            make(Command::Connect, 4, 2.0),
            make(Command::AbortRoute, 0, 0.0),
        ];
        let eligible = initial
            .iter()
            .map(|edge| edge.identity.clone())
            .collect::<HashSet<_>>();
        let baseline = completed_q_transform(&initial, 0.25, 0.5, 1.0, Some(&eligible));
        let late = [
            make(
                Command::Place {
                    block: "late".into(),
                    position: Point { x: 0, y: 0 },
                    rotation: 0,
                },
                1000,
                -1000.0,
            ),
            make(Command::AbortRoute, 0, 0.0),
            make(Command::Connect, 4, 2.0),
        ];
        let after = completed_q_transform(&late, 0.25, 0.5, 1.0, Some(&eligible));
        assert_eq!(after[1], baseline[1]);
        assert_eq!(after[2], baseline[0]);
    }

    #[test]
    fn gumbel_target_keeps_initial_eligible_priors_after_widening() {
        let mut second = edge(0.2, 0.3);
        second.action = MacroAction::Command(Command::AbortRoute);
        second.identity = physical_action_identity(&second.action).unwrap();
        let mut initial = [edge(0.8, 0.7), second];
        initial[0].prior = 0.8;
        initial[0].anchor_prior = 0.7;
        initial[1].prior = 0.2;
        initial[1].anchor_prior = 0.3;
        let eligible = initial
            .iter()
            .map(|edge| edge.identity.clone())
            .collect::<HashSet<_>>();
        let priors = initial
            .iter()
            .map(|edge| (edge.identity.clone(), root_policy_prior(edge, 0.25)))
            .collect::<BTreeMap<_, _>>();
        let before = improved_policy_with_priors(
            &initial,
            &[0.0, 0.0],
            0.25,
            Some(&eligible),
            Some(&priors),
        );
        initial[0].prior = 0.1;
        initial[0].anchor_prior = 0.1;
        initial[1].prior = 0.9;
        initial[1].anchor_prior = 0.9;
        let after = improved_policy_with_priors(
            &initial,
            &[0.0, 0.0],
            0.25,
            Some(&eligible),
            Some(&priors),
        );
        assert_eq!(before, after);
        assert!(after[0] > after[1]);
    }

    #[test]
    fn versioned_root_report_admits_identity_and_rejects_ambiguity() -> Result<()> {
        let action = |command, eligible, target| RootActionRecord {
            action: MacroAction::Command(command),
            prior: 0.5,
            anchor_prior: 0.5,
            visits: 1,
            q: 0.25,
            gumbel_eligible: Some(eligible),
            policy_target: Some(target),
        };
        let actions = vec![
            action(Command::Connect, true, 1.0),
            action(Command::AbortRoute, true, 0.0),
            action(
                Command::Place {
                    block: "late".into(),
                    position: Point { x: 0, y: 0 },
                    rotation: 0,
                },
                false,
                0.0,
            ),
        ];
        let selected_id = physical_action_identity(&actions[0].action)?;
        let record = RootRecord {
            state_sha256: "state".into(),
            state_revision: 0,
            target: RootRecordTarget::GumbelDenseOracle(DenseRootRecord {
                actions,
                selected_action: 0,
                selected_physical_id: Some(selected_id.clone()),
                gumbel_shortlist: vec![RootShortlistEvidence {
                    physical_id: selected_id,
                    initial_rank: 0,
                    gumbel_bits: 1.25f64.to_bits(),
                }],
                gumbel_shortlist_cap: Some(2),
                initial_eligible_actions: Some(2),
            }),
            pending_lower_tasks: 1,
            loaded_lower_pages: 1,
            later_completed_connections: 0,
            later_total_connections: 1,
            later_footprint_area: 0,
            later_transport_cells: 0,
            later_turns: 0,
            later_fully_validated: false,
            later_termination: SearchTermination::MoveBudgetExhausted,
        };
        let make_report = |record: RootRecord, schema: &str| Report {
            schema: schema.into(),
            status: "ok".into(),
            partition: "test".into(),
            device: "cpu".into(),
            search_config_sha256: String::new(),
            panel_sha256: String::new(),
            catalog_sha256: String::new(),
            lower_config_sha256: String::new(),
            lower_parameter_order_sha256: String::new(),
            lower_checkpoint_sha256: String::new(),
            outer_config_sha256: String::new(),
            outer_parameter_order_sha256: String::new(),
            outer_checkpoint_sha256: String::new(),
            root_records_retained: true,
            cases: vec![CaseReport {
                id: "identity".into(),
                prefix_commands: 0,
                beam: None,
                mcts: SearchResult {
                    algorithm: "mcts".into(),
                    status: "ok".into(),
                    fully_validated: false,
                    termination: SearchTermination::MoveBudgetExhausted,
                    no_macro_diagnostic: None,
                    completed_connections: 0,
                    total_connections: 1,
                    footprint_area: None,
                    transport_cells: None,
                    turns: None,
                    moves: 1,
                    simulations: 1,
                    elapsed_seconds: 0.0,
                    samples_per_minute: 0.0,
                    counters: Counters::default(),
                    trace: Vec::new(),
                    root_records: vec![record],
                },
            }],
        };
        let path = std::env::temp_dir().join(format!(
            "aic-gumbel-root-admission-{}.json",
            std::process::id()
        ));
        let check = |report: Report| -> Result<Result<Published<Report>>> {
            publish_json(&path, report)?;
            let loaded = load_published_report(&path);
            fs::remove_file(&path)?;
            Ok(loaded)
        };
        let loaded = check(make_report(record.clone(), REPORT_SCHEMA))??;
        assert_eq!(
            loaded.value.cases[0].mcts.root_records[0]
                .dense()
                .unwrap()
                .actions
                .len(),
            3
        );
        assert!(
            check(make_report(
                record.clone(),
                "aic-hierarchical-search-report-v10"
            ))?
            .err()
            .expect("old schema must be rejected")
            .to_string()
            .contains("hierarchical-report-schema")
        );
        let mut untagged = serde_json::to_value(make_report(
            record.clone(),
            "aic-hierarchical-search-report-v11",
        ))?;
        let root = untagged["cases"][0]["mcts"]["root_records"][0]
            .as_object_mut()
            .expect("serialized root is an object");
        let target = root.remove("target").expect("tagged target");
        for (key, value) in target["data"].as_object().expect("dense target data") {
            root.insert(key.clone(), value.clone());
        }
        publish_json(&path, untagged)?;
        let untagged_error = load_published_report(&path)
            .err()
            .expect("old untagged report must be rejected");
        fs::remove_file(&path)?;
        assert!(
            untagged_error
                .to_string()
                .contains("hierarchical-report-schema")
        );
        let mut duplicate = record.clone();
        let dense = duplicate.dense_mut().unwrap();
        dense.actions[2].action = dense.actions[1].action.clone();
        assert!(check(make_report(duplicate, REPORT_SCHEMA))?.is_err());
        let mut puct = record;
        let dense = puct.dense_mut().unwrap();
        dense.gumbel_shortlist.clear();
        dense.gumbel_shortlist_cap = None;
        dense.initial_eligible_actions = None;
        for action in &mut dense.actions {
            action.gumbel_eligible = None;
            action.policy_target = None;
        }
        puct.target = RootRecordTarget::PuctDense(puct.dense().unwrap().clone());
        check(make_report(puct, REPORT_SCHEMA))??;
        Ok(())
    }

    #[test]
    fn appended_route_update_keeps_the_exposed_physical_owner() -> Result<()> {
        let _torch_guard = crate::TORCH_TEST.lock().unwrap();
        let panel: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-route-guidance/training-panel.json"
        ))?;
        let route_config: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-route-guidance/config.json"
        ))?;
        let case = &panel["cases"][0];
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value(panel["limits"].clone())?,
        )?;
        for command in case["prefix"].as_array().expect("route prefix") {
            assert!(
                session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        let candidate_config: CandidateConfig = serde_json::from_value(panel["candidate"].clone())?;
        let request = Request {
            state_revision: session.revision(),
            connection: case["request"].as_str().expect("route request").into(),
        };
        let candidates = session
            .route_candidates(&request, &candidate_config)?
            .candidates;
        assert!(candidates.len() >= 2);
        let mut first = candidates[0].clone();
        first.provenance = Provenance::Anchor;
        let mut later = candidates[1].clone();
        later.provenance = Provenance::Guided {
            model_version: "checkpoint".into(),
            head: "head-1".into(),
        };
        let model_config = serde_json::from_value(route_config["model"].clone())?;
        let (order, state) = GuidanceModel::initial_state(&model_config, 17)?;
        let model = GuidanceModel::load(&model_config, &order, &state, Device::Cpu)?;
        let factory = Factory::new(
            &model,
            "checkpoint",
            Device::Cpu,
            serde_json::from_value(route_config["learning_domain"].clone())?,
            &candidate_config,
            0,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let mut nodes = vec![Node {
            session: Arc::new(session),
            memory: TreeReservation::new(&budget, 0)
                .map_err(|stop| anyhow::anyhow!("node reservation: {stop:?}"))?,
            visits: 0,
            value: None,
            actions: Some(Vec::new()),
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        let mut counters = Counters::default();
        append_node_actions(
            &mut nodes,
            0,
            &factory,
            &mut counters,
            &budget,
            vec![MacroAction::Route(first.clone())],
            Vec::new(),
        )?;
        let original_id = physical_action_identity(&MacroAction::Route(first.clone()))?;
        let mut config: Config = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-curriculum/search.json"
        ))?;
        config.candidate = candidate_config.clone();
        config.simulations_per_move = 2;
        config.horizon = 1;
        let controls = GumbelControls {
            candidates: 2,
            anchor_candidates: 0,
            anchor_policy_mass: 0.0,
            seed: 7,
            value_scale: 0.1,
            max_visit_init: 50.0,
            gumbel_assignment: GumbelAssignment::ActionIdentity,
        };
        let state_identity = state_sha256(&nodes[0].session)?;
        let frozen = freeze_gumbel_root(&nodes[0], controls, "route", &state_identity)?
            .expect("initial route root");
        assert_eq!(frozen.shortlist.len(), 1);
        assert_eq!(frozen.shortlist[0].physical_id, original_id);
        let original_prior = nodes[0].actions.as_ref().unwrap()[0].prior;
        nodes[0].actions.as_mut().unwrap()[0].visits = 3;
        nodes[0].actions.as_mut().unwrap()[0].value_sum = 1.5;
        let update = RouteUpdate {
            connection: first.connection.clone(),
            id: first.id,
            additional_provenance: vec![Provenance::Guided {
                model_version: "checkpoint".into(),
                head: "head-0".into(),
            }],
            _memory: TreeReservation::new(&budget, 0)
                .map_err(|stop| anyhow::anyhow!("update reservation: {stop:?}"))?,
        };
        append_node_actions(
            &mut nodes,
            0,
            &factory,
            &mut counters,
            &budget,
            vec![MacroAction::Route(later.clone())],
            vec![update],
        )?;
        let edges = nodes[0].actions.as_ref().unwrap();
        assert_eq!(edges.len(), 2);
        let original = edges
            .iter()
            .find(|edge| edge.identity == original_id)
            .unwrap();
        assert_eq!(original.visits, 3);
        assert_eq!(original.value_sum, 1.5);
        assert_eq!(original.features[13..15], [1.0, 1.0]);
        assert!(original.prior > 0.0 && original.prior < original_prior);
        let MacroAction::Route(updated) = &original.action else {
            unreachable!("original route witness")
        };
        assert_eq!(updated.additional_provenance.len(), 1);
        assert_eq!(physical_action_identity(&original.action)?, original_id);
        assert!(edges.iter().any(|edge| {
            edge.identity == physical_action_identity(&MacroAction::Route(later.clone())).unwrap()
        }));
        assert_eq!(
            resolve_root_choice(
                edges,
                &RootChoice {
                    identity: original_id.clone(),
                    gumbel: 0.25,
                    initial_rank: 0,
                }
            ),
            edges.iter().position(|edge| edge.visits == 3).unwrap()
        );
        let later_id = physical_action_identity(&MacroAction::Route(later))?;
        let decision = finish_gumbel_root(
            &mut nodes,
            0,
            &factory,
            &config,
            &mut counters,
            &budget,
            controls,
            frozen,
        )?;
        assert_eq!(decision.eligible_ids.as_ref().unwrap().len(), 1);
        assert_eq!(decision.shortlist[0].physical_id, original_id);
        assert_eq!(
            nodes[0].actions.as_ref().unwrap()[decision.selected.unwrap()].identity,
            original_id
        );
        let late_index = nodes[0]
            .actions
            .as_ref()
            .unwrap()
            .iter()
            .position(|edge| edge.identity == later_id)
            .unwrap();
        assert_eq!(decision.policy_targets.as_ref().unwrap()[late_index], 0.0);
        let reroot = gumbel_sequential_halving(
            &mut nodes,
            0,
            &factory,
            &config,
            &mut counters,
            &budget,
            controls,
            "reroot",
            &state_identity,
        )?;
        assert!(reroot.eligible_ids.as_ref().unwrap().contains(&later_id));
        assert!(
            reroot
                .shortlist
                .iter()
                .any(|entry| entry.physical_id == later_id)
        );
        let duplicate = append_node_actions(
            &mut nodes,
            0,
            &factory,
            &mut counters,
            &budget,
            vec![MacroAction::Route(first)],
            Vec::new(),
        )
        .err()
        .expect("duplicate physical witness must be refused before scoring");
        assert!(duplicate.to_string().contains("hierarchical-root-identity"));
        Ok(())
    }

    #[test]
    fn appended_network_witness_keeps_the_exposed_physical_owner() -> Result<()> {
        let _torch_guard = crate::TORCH_TEST.lock().unwrap();
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
            assert!(session.step(command.clone()).accepted);
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
        let mut candidate_config = CandidateConfig {
            bounds,
            max_candidates: 4,
            max_searches: 48,
            work_per_search: 20_000,
            max_expanded: 400_000,
            overlap_penalty: 8,
            max_crossings_per_route: 0,
            request_allowance: Default::default(),
        };
        candidate_config.request_allowance.max_variants = 4;
        let factory = Factory::new(
            &model,
            "checkpoint",
            Device::Cpu,
            DomainConfig {
                placement: bounds,
                extension_lengths: vec![1, 2, 4],
            },
            &candidate_config,
            0,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut counters = Counters::default();
        let mut allowance = RequestAllowance::new(candidate_config.request_allowance.clone());
        let networks = factory.network_actions(
            &session,
            &["output-a".into(), "output-b".into()],
            &mut counters,
            &mut allowance,
        )?;
        assert!(networks.len() >= 2, "fixture needs two physical networks");
        let first = MacroAction::Network(networks[0].clone());
        let late = MacroAction::Network(networks[1].clone());
        assert_ne!(
            physical_action_identity(&first)?,
            physical_action_identity(&late)?
        );
        let budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let mut nodes = [Node {
            session: Arc::new(session),
            memory: TreeReservation::new(&budget, 0)
                .map_err(|stop| anyhow::anyhow!("node reservation: {stop:?}"))?,
            visits: 0,
            value: None,
            actions: Some(Vec::new()),
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        append_node_actions(
            &mut nodes,
            0,
            &factory,
            &mut counters,
            &budget,
            vec![first.clone()],
            Vec::new(),
        )?;
        let first_id = physical_action_identity(&first)?;
        nodes[0].actions.as_mut().unwrap()[0].visits = 3;
        nodes[0].actions.as_mut().unwrap()[0].value_sum = 1.5;
        append_node_actions(
            &mut nodes,
            0,
            &factory,
            &mut counters,
            &budget,
            vec![late.clone()],
            Vec::new(),
        )?;
        let edges = nodes[0].actions.as_ref().unwrap();
        assert_eq!(edges.len(), 2);
        let retained = edges.iter().find(|edge| edge.identity == first_id).unwrap();
        assert_eq!(retained.visits, 3);
        assert_eq!(retained.value_sum, 1.5);
        assert_eq!(physical_action_identity(&retained.action)?, first_id);
        assert!(
            edges
                .iter()
                .any(|edge| edge.identity == physical_action_identity(&late).unwrap())
        );
        Ok(())
    }

    #[test]
    fn appended_repair_witness_keeps_the_exposed_physical_owner() -> Result<()> {
        let _torch_guard = crate::TORCH_TEST.lock().unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-repair/fixture.json"
        ))?;
        let mut session = Session::new(
            serde_json::from_value(fixture["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value(fixture["limits"].clone())?,
        )?;
        for command in fixture["prefix"].as_array().expect("repair prefix") {
            assert!(
                session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        let candidate_config: CandidateConfig =
            serde_json::from_value(fixture["candidate"].clone())?;
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
        let factory = Factory::new(
            &model,
            "checkpoint",
            Device::Cpu,
            DomainConfig {
                placement: candidate_config.bounds,
                extension_lengths: vec![1, 2, 4],
            },
            &candidate_config,
            1,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let mut counters = Counters::default();
        let repairs = factory
            .action_batch(&session, &mut counters, false)?
            .actions
            .into_iter()
            .filter(|action| matches!(action, MacroAction::Repair { .. }))
            .collect::<Vec<_>>();
        assert!(repairs.len() >= 2, "fixture needs two repair witnesses");
        let first = repairs[0].clone();
        let late = repairs[1].clone();
        assert_ne!(
            physical_action_identity(&first)?,
            physical_action_identity(&late)?
        );
        let budget = SearchMemoryBudget::new(1024 * 1024 * 1024);
        let mut nodes = [Node {
            session: Arc::new(session),
            memory: TreeReservation::new(&budget, 0)
                .map_err(|stop| anyhow::anyhow!("node reservation: {stop:?}"))?,
            visits: 0,
            value: None,
            actions: Some(Vec::new()),
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        append_node_actions(
            &mut nodes,
            0,
            &factory,
            &mut counters,
            &budget,
            vec![first.clone()],
            Vec::new(),
        )?;
        let first_id = physical_action_identity(&first)?;
        nodes[0].actions.as_mut().unwrap()[0].visits = 3;
        nodes[0].actions.as_mut().unwrap()[0].value_sum = 1.5;
        append_node_actions(
            &mut nodes,
            0,
            &factory,
            &mut counters,
            &budget,
            vec![late.clone()],
            Vec::new(),
        )?;
        let edges = nodes[0].actions.as_ref().unwrap();
        assert_eq!(edges.len(), 2);
        let retained = edges.iter().find(|edge| edge.identity == first_id).unwrap();
        assert_eq!(retained.visits, 3);
        assert_eq!(retained.value_sum, 1.5);
        assert_eq!(physical_action_identity(&retained.action)?, first_id);
        assert!(
            edges
                .iter()
                .any(|edge| edge.identity == physical_action_identity(&late).unwrap())
        );
        Ok(())
    }

    #[test]
    fn gumbel_root_records_late_route_from_live_lower_pages() -> Result<()> {
        let _torch_guard = crate::TORCH_TEST.lock().unwrap();
        let panel: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-route-guidance/training-panel.json"
        ))?;
        let route_config: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-route-guidance/config.json"
        ))?;
        let mut config: Config = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-curriculum/search.json"
        ))?;
        config.candidate = serde_json::from_value(panel["candidate"].clone())?;
        config.simulations_per_move = 16;
        config.max_moves = 1;
        config.horizon = 1;
        config.compiled_inference = false;
        config.cpu_threads = 1;
        config.max_live_search_bytes = 1024 * 1024 * 1024;
        config.root_selection = RootSelection::GumbelSequentialHalving {
            candidates: 2,
            anchor_candidates: 0,
            anchor_policy_mass: 0.0,
            seed: 7,
            value_scale: 0.1,
            max_visit_init: 50.0,
            gumbel_assignment: GumbelAssignment::ActionIdentity,
        };
        validate_config(&config)?;
        let model_config = serde_json::from_value(route_config["model"].clone())?;
        let (order, state) = GuidanceModel::initial_state(&model_config, 17)?;
        let model = GuidanceModel::load(&model_config, &order, &state, Device::Cpu)?;
        let factory = Factory::new(
            &model,
            "checkpoint",
            Device::Cpu,
            serde_json::from_value(route_config["learning_domain"].clone())?,
            &config.candidate,
            0,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let case = &panel["cases"][0];
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value(panel["limits"].clone())?,
        )?;
        for command in case["prefix"].as_array().expect("route prefix") {
            assert!(
                session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        let result = mcts_search(session, &factory, &config, "late-route")?;
        assert_eq!(result.root_records.len(), 1);
        let record = &result.root_records[0];
        validate_root_record_lineage(record)?;
        let dense = record.dense().expect("dense oracle");
        let eligible = dense
            .actions
            .iter()
            .filter(|action| action.gumbel_eligible == Some(true))
            .count();
        let late_routes = dense
            .actions
            .iter()
            .filter(|action| {
                action.gumbel_eligible == Some(false)
                    && matches!(action.action, MacroAction::Route(_))
            })
            .collect::<Vec<_>>();
        assert_eq!(dense.initial_eligible_actions, Some(eligible));
        assert!(
            !late_routes.is_empty(),
            "live lower page never widened root"
        );
        assert!(
            late_routes
                .iter()
                .all(|action| action.policy_target == Some(0.0))
        );
        assert!(
            dense
                .actions
                .iter()
                .map(|action| action.policy_target.unwrap())
                .sum::<f64>()
                > 0.999
        );
        Ok(())
    }

    #[test]
    fn gumbel_root_records_late_repair_from_live_lower_pages() -> Result<()> {
        let _torch_guard = crate::TORCH_TEST.lock().unwrap();
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
        let run = |session: Session, candidate: CandidateConfig| -> Result<()> {
            let mut config: Config = serde_json::from_str(include_str!(
                "../../../docs/benchmarks/hierarchical-curriculum/search.json"
            ))?;
            config.bounds = candidate.bounds;
            config.candidate = candidate;
            config.repair_max_removed = 1;
            config.simulations_per_move = 16;
            config.max_moves = 1;
            config.horizon = 1;
            config.compiled_inference = false;
            config.cpu_threads = 1;
            config.max_live_search_bytes = 1024 * 1024 * 1024;
            config.root_selection = RootSelection::GumbelSequentialHalving {
                candidates: 2,
                anchor_candidates: 0,
                anchor_policy_mass: 0.0,
                seed: 7,
                value_scale: 0.1,
                max_visit_init: 50.0,
                gumbel_assignment: GumbelAssignment::ActionIdentity,
            };
            validate_config(&config)?;
            let factory = Factory::new(
                &model,
                "checkpoint",
                Device::Cpu,
                DomainConfig {
                    placement: config.bounds,
                    extension_lengths: vec![1, 2, 4],
                },
                &config.candidate,
                1,
                4,
                1,
                0.0,
                false,
                None,
            )?;
            let result = mcts_search(session, &factory, &config, "repair")?;
            assert_eq!(result.root_records.len(), 1, "repair root missing");
            let record = &result.root_records[0];
            validate_root_record_lineage(record)?;
            let late = record
                .dense()
                .expect("dense oracle")
                .actions
                .iter()
                .filter(|action| {
                    action.gumbel_eligible == Some(false)
                        && matches!(action.action, MacroAction::Repair { .. })
                })
                .collect::<Vec<_>>();
            assert!(!late.is_empty(), "repair lower page never widened root");
            assert!(late.iter().all(|action| action.policy_target == Some(0.0)));
            Ok(())
        };
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-repair/fixture.json"
        ))?;
        let mut repair_session = Session::new(
            serde_json::from_value(fixture["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value(fixture["limits"].clone())?,
        )?;
        for command in fixture["prefix"].as_array().expect("repair prefix") {
            assert!(
                repair_session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        run(
            repair_session,
            serde_json::from_value(fixture["candidate"].clone())?,
        )?;
        Ok(())
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
            max_live_search_bytes: default_max_live_search_bytes(),
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
