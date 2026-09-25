//! Stable-state macro search over placement commands and complete route witnesses.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    io::{BufReader, BufWriter, Read, Write},
    path::Path,
    sync::Arc,
    time::Instant,
};

use aic_layout::{geometry::Bounds, physical::Problem, validation::Catalogs};
use aic_pilot_native::constructive::{
    Command, Limits, Session,
    learning::{CommandKind, Config as DomainConfig},
    routing_candidates::{
        Candidate, Config as CandidateConfig, NetworkCandidate, Provenance, Report as RouteReport,
        Request, Termination as RouteTermination, network_shared_cells,
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
pub const REPORT_SCHEMA: &str = "aic-hierarchical-search-report-v7";
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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ActionSummary {
    kind: String,
    entity: String,
    state_revision: u64,
    steps: Option<i64>,
    provenance: Option<Provenance>,
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
    network_samples: usize,
    shared_network_samples: usize,
    route_candidate_limit: usize,
    route_search_limit: usize,
    route_work_budget: usize,
    route_frontier_exhausted: usize,
    route_no_compatible_endpoints: usize,
    #[serde(default, skip_deserializing, skip_serializing_if = "Vec::is_empty")]
    route_attempt_samples: Vec<RouteAttemptRecord>,
    route_attempt_samples_dropped: usize,
}

#[derive(Clone, Debug, Serialize)]
struct RouteAttemptRecord {
    state_sha256: String,
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
    guidance_version: &'a str,
    device: Device,
    domain: DomainConfig,
    candidate: &'a CandidateConfig,
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
}

struct ActionScores {
    priors: Vec<f64>,
    anchor_priors: Vec<f64>,
    model_value: f64,
}

impl Factory<'_> {
    pub(crate) fn new<'a>(
        guidance: &'a GuidanceModel,
        guidance_version: &'a str,
        device: Device,
        domain: DomainConfig,
        candidate: &'a CandidateConfig,
        placement_page: usize,
        cpu_threads: usize,
        anchor_prior_mass: f64,
        compiled_inference: bool,
        outer: Option<&'a OuterModel>,
    ) -> Result<Factory<'a>> {
        Ok(Factory {
            guidance: if compiled_inference {
                GuidanceRunner::Compiled(CompiledGuidanceModel::new(guidance))
            } else {
                GuidanceRunner::Eager(guidance)
            },
            guidance_version,
            device,
            domain,
            candidate,
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
            session.observe().active.is_none(),
            "aic-learning.hierarchical-state: macro state contains an active partial route"
        );
        let mut actions = Vec::new();
        let mut placement_pages = PlacementPages {
            offsets: [0; 2],
            exhausted: [true; 2],
            loaded_pages: 1,
        };
        if let Ok(observation) = session.learning_observation(&self.domain) {
            for (index, kind) in [CommandKind::Place, CommandKind::PlaceTerminal]
                .into_iter()
                .enumerate()
            {
                loop {
                    let page = observation.domain.command_page(
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

        let observation = session.observe();
        let mut unfinished = BTreeMap::new();
        for (index, connection) in session.problem().connections.iter().enumerate() {
            if observation.routes[index].is_some() {
                continue;
            }
            unfinished
                .entry((connection.commodity.clone(), connection.transport))
                .or_insert_with(Vec::new)
                .push(Request {
                    state_revision: observation.revision,
                    connection: connection.id.clone(),
                });
        }
        // A logical network becomes a macro only when every unfinished member
        // has placed endpoints. Do not silently route one temporarily-ready
        // edge as a singleton and thereby consume the future network's ports.
        let ready = unfinished
            .into_values()
            .filter_map(|requests| {
                let inputs = requests
                    .iter()
                    .map(|request| {
                        session.route_guidance_input(request, self.candidate, &self.domain)
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .ok()?;
                Some((requests, inputs))
            })
            .collect::<Vec<_>>();
        let singletons = ready
            .iter()
            .filter(|(requests, _)| requests.len() == 1)
            .collect::<Vec<_>>();
        if !singletons.is_empty() {
            let inputs = singletons
                .iter()
                .map(|(_, inputs)| &inputs[0])
                .collect::<Vec<_>>();
            let guidance =
                self.guidance
                    .predict_native(&inputs, self.guidance_version, self.device)?;
            counters.guidance_batches += 1;
            counters.guidance_inputs += inputs.len();
            let reports = self.pool.install(|| {
                singletons
                    .par_iter()
                    .zip(guidance.par_iter())
                    .map(|((requests, _), guidance)| {
                        session.route_candidates_with_guidance(
                            &requests[0],
                            self.candidate,
                            guidance,
                        )
                    })
                    .collect::<Vec<_>>()
            });
            for report in reports {
                let report = report.map_err(anyhow::Error::new)?;
                record_route_report(counters, session, &report)?;
                actions.extend(report.candidates.into_iter().map(MacroAction::Route));
            }
        }
        for (requests, _) in ready.iter().filter(|(requests, _)| requests.len() > 1) {
            let connections = requests
                .iter()
                .map(|request| request.connection.clone())
                .collect::<Vec<_>>();
            actions.extend(
                self.network_actions(session, &connections, counters)?
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
    ) -> Result<Vec<NetworkCandidate>> {
        #[derive(Clone)]
        struct Partial {
            variant: usize,
            order: Vec<String>,
            session: Session,
            routes: Vec<Candidate>,
        }

        counters.network_tasks += 1;
        let count = self.candidate.max_candidates;
        let mut partials = (0..count)
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
                }
            })
            .collect::<Vec<_>>();
        for depth in 0..connections.len() {
            let prepared = partials
                .iter()
                .map(|partial| {
                    let request = Request {
                        state_revision: partial.session.observe().revision,
                        connection: partial.order[depth].clone(),
                    };
                    let input = partial
                        .session
                        .route_guidance_input(&request, self.candidate, &self.domain)
                        .map_err(anyhow::Error::new)?;
                    Ok((request, input))
                })
                .collect::<Result<Vec<_>>>()?;
            let inputs = prepared.iter().map(|(_, input)| input).collect::<Vec<_>>();
            let guidance =
                self.guidance
                    .predict_native(&inputs, self.guidance_version, self.device)?;
            counters.guidance_batches += 1;
            counters.guidance_inputs += inputs.len();
            let reports = self.pool.install(|| {
                partials
                    .par_iter()
                    .zip(prepared.par_iter())
                    .zip(guidance.par_iter())
                    .map(|((partial, (request, _)), guidance)| {
                        partial.session.route_candidates_with_guidance(
                            request,
                            self.candidate,
                            guidance,
                        )
                    })
                    .collect::<Vec<_>>()
            });
            let mut next = Vec::with_capacity(partials.len());
            for (mut partial, report) in partials.into_iter().zip(reports) {
                let report = report.map_err(anyhow::Error::new)?;
                record_route_report(counters, &partial.session, &report)?;
                if report.candidates.is_empty() {
                    continue;
                }
                let choice = if partial.variant == 0 {
                    0
                } else {
                    1 + (partial.variant + depth - 1)
                        % report.candidates.len().saturating_sub(1).max(1)
                }
                .min(report.candidates.len() - 1);
                let candidate = report.candidates[choice].clone();
                partial
                    .session
                    .apply_route_candidate(&candidate)
                    .map_err(anyhow::Error::new)?;
                partial.routes.push(candidate);
                next.push(partial);
            }
            partials = next;
            if partials.is_empty() {
                break;
            }
        }
        let first = session_network_identity(root, connections)?;
        let mut semantic = BTreeSet::new();
        let mut candidates = Vec::new();
        for partial in partials
            .into_iter()
            .filter(|partial| partial.routes.len() == connections.len())
        {
            let key = partial
                .routes
                .iter()
                .map(|route| (route.connection.clone(), route.cells.clone()))
                .collect::<Vec<_>>();
            if !semantic.insert(key) {
                continue;
            }
            let mut candidate = NetworkCandidate {
                id: candidates.len(),
                state_revision: root.observe().revision,
                commodity: first.0.clone(),
                transport: first.1,
                steps: partial.routes.iter().map(|route| route.steps).sum(),
                turns: partial.routes.iter().map(|route| route.turns).sum(),
                expanded: partial.routes.iter().map(|route| route.expanded).sum(),
                shared_cells: network_shared_cells(&partial.routes),
                routes: partial.routes,
            };
            let mut replay = root.clone();
            replay
                .apply_network_candidate(&candidate)
                .map_err(anyhow::Error::new)?;
            candidate.id = candidates.len();
            candidates.push(candidate);
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

fn record_route_report(
    counters: &mut Counters,
    session: &Session,
    report: &RouteReport,
) -> Result<()> {
    counters.route_searches += report.searches;
    counters.route_expanded += report.expanded;
    counters.route_preflight_expanded += report.preflight_expanded;
    counters.native_rejections += report.native_rejections.len();
    match report.termination {
        RouteTermination::CandidateLimit => counters.route_candidate_limit += 1,
        RouteTermination::SearchLimit => counters.route_search_limit += 1,
        RouteTermination::WorkBudget => counters.route_work_budget += 1,
        RouteTermination::FrontierExhausted => counters.route_frontier_exhausted += 1,
        RouteTermination::NoCompatibleEndpoints => {
            counters.route_no_compatible_endpoints += 1;
        }
    }
    if counters.route_attempt_samples.len() < ROUTE_ATTEMPT_SAMPLE_LIMIT {
        counters.route_attempt_samples.push(RouteAttemptRecord {
            state_sha256: state_sha256(session)?,
            state_revision: report.state_revision,
            connection: report.connection.clone(),
            termination: report.termination,
            fixed_request_proven_infeasible: report.fixed_request_proven_infeasible,
            candidates: report.candidates.len(),
        });
    } else {
        counters.route_attempt_samples_dropped += 1;
    }
    Ok(())
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
        };
        index.map(|index| index as i64).context(
            "aic-learning.hierarchical-action-entity: action entity is absent from problem",
        )
    }

    pub(crate) fn apply(&self, session: &Session) -> Result<Session> {
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
            },
            Self::Command(Command::PlaceTerminal { terminal, .. }) => ActionSummary {
                kind: "place-terminal".into(),
                entity: terminal.clone(),
                state_revision: revision,
                steps: None,
                provenance: None,
            },
            Self::Command(_) => unreachable!("macro command generator exposes only placements"),
            Self::Route(candidate) => ActionSummary {
                kind: "route".into(),
                entity: candidate.connection.clone(),
                state_revision: candidate.state_revision,
                steps: Some(candidate.steps),
                provenance: Some(candidate.provenance.clone()),
            },
            Self::Network(candidate) => ActionSummary {
                kind: "network".into(),
                entity: candidate.commodity.clone(),
                state_revision: candidate.state_revision,
                steps: Some(candidate.steps),
                provenance: None,
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

fn session_network_identity(
    session: &Session,
    connections: &[String],
) -> Result<(String, aic_layout::logistics::TransportKind)> {
    ensure!(
        connections.len() > 1,
        "aic-learning.hierarchical-network: network task needs multiple connections"
    );
    let mut unique = BTreeSet::new();
    let first = session
        .problem()
        .connections
        .iter()
        .find(|connection| connection.id == connections[0])
        .context("aic-learning.hierarchical-network: unknown first connection")?;
    for id in connections {
        let connection = session
            .problem()
            .connections
            .iter()
            .find(|connection| connection.id == *id)
            .context("aic-learning.hierarchical-network: unknown connection")?;
        ensure!(
            unique.insert(id.as_str())
                && connection.commodity == first.commodity
                && connection.transport == first.transport,
            "aic-learning.hierarchical-network: task mixes duplicate or incompatible connections"
        );
    }
    Ok((first.commodity.clone(), first.transport))
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
    let factory = Factory::new(
        &guidance,
        &checkpoint_sha256,
        device,
        DomainConfig {
            placement: config.bounds,
            extension_lengths: config.extension_lengths.clone(),
        },
        &config.candidate,
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
    for _ in 0..config.max_moves {
        let mut next = Vec::new();
        for (session, trace) in &beam {
            if session.layout().is_some() {
                termination = SearchTermination::FullyValidated;
                best = (session.clone(), trace.clone());
                beam = vec![best.clone()];
                break;
            }
            for action in factory.actions(session, &mut counters)? {
                let child = action.apply(session)?;
                let mut child_trace = trace.clone();
                child_trace.push(action.summary(session.observe().revision));
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
            break;
        };
        let actions = nodes[0].actions.as_ref().expect("expanded root");
        let action = actions[selected].action.clone();
        let revision = working.observe().revision;
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
        working = action.apply(&working)?;
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
            .apply(&nodes[current].session)?;
        let child = nodes.len();
        nodes.push(Node {
            session: child_session,
            visits: 0,
            value: None,
            actions: None,
            placement_pages: None,
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
        .map(|action| action.apply(&nodes[node].session))
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
        .map(|action| action.apply(&session))
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
            steps: 1,
            turns: 0,
            expanded: 10,
            overlap_with_previous: 0,
            learned_cost: 1,
            predicted_success: Some(0.25),
            guidance_uncertainty: Some(0.1),
            provenance: Provenance::Anchor,
        };
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
        };
        let config = Config {
            schema: CONFIG_SCHEMA.into(),
            bounds,
            extension_lengths: vec![1, 2, 4],
            candidate: candidate.clone(),
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
                    state_revision: session.observe().revision,
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
