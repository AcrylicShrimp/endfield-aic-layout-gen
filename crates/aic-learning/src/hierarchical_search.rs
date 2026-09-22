//! Stable-state macro search over placement commands and complete route witnesses.

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::Path,
    sync::Arc,
    time::Instant,
};

use aic_layout::{geometry::Bounds, physical::Problem, validation::Catalogs};
use aic_pilot_native::constructive::{
    Command, Limits, Session,
    learning::{CommandKind, Config as DomainConfig},
    routing_candidates::{
        Candidate, Config as CandidateConfig, NetworkCandidate, Provenance, Request,
        network_shared_cells,
    },
};
use aic_torch::tch::{self, Device, Tensor};
use anyhow::{Context, Result, ensure};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    input::Tensors,
    outer_model::{
        ACTION_FEATURES, Batch as OuterBatch, CompiledModel as CompiledOuterModel,
        Model as OuterModel, ParameterOrder as OuterOrder,
    },
    outer_training::Config as OuterTrainingConfig,
    route_guidance::{
        CompiledModel as CompiledGuidanceModel, Model as GuidanceModel, ParameterOrder,
    },
    route_guidance_training::Config as GuidanceTrainingConfig,
};

const CONFIG_SCHEMA: &str = "aic-hierarchical-search-config-v1";
const PANEL_SCHEMA: &str = "aic-hierarchical-search-panel-v1";
pub const REPORT_SCHEMA: &str = "aic-hierarchical-search-report-v2";
const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;

fn default_anchor_prior_mass() -> f64 {
    0.25
}

fn default_compiled_inference() -> bool {
    true
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
    pub simulations_per_move: usize,
    pub horizon: usize,
    pub max_moves: usize,
    pub exploration: f64,
    #[serde(default = "default_anchor_prior_mass")]
    pub anchor_prior_mass: f64,
    #[serde(default = "default_compiled_inference")]
    pub compiled_inference: bool,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Panel {
    schema: String,
    partition: String,
    limits: Limits,
    cases: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    problem: Problem,
    prefix: Vec<Command>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
pub(crate) enum MacroAction {
    Command(Command),
    Route(Candidate),
    Network(NetworkCandidate),
}

#[derive(Clone, Debug, Serialize)]
struct ActionSummary {
    kind: &'static str,
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
    pub visits: u32,
    pub q: f64,
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
    pub later_fully_validated: bool,
}

#[derive(Default, Clone, Debug, Serialize)]
pub(crate) struct Counters {
    expansions: usize,
    generated_macro_samples: usize,
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
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct SearchResult {
    algorithm: &'static str,
    status: &'static str,
    fully_validated: bool,
    completed_connections: usize,
    total_connections: usize,
    moves: usize,
    simulations: usize,
    elapsed_seconds: f64,
    samples_per_minute: f64,
    counters: Counters,
    trace: Vec<ActionSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    root_records: Vec<RootRecord>,
}

#[derive(Debug, Serialize)]
pub(crate) struct CaseReport {
    id: String,
    prefix_commands: usize,
    beam: SearchResult,
    mcts: SearchResult,
}

#[derive(Debug, Serialize)]
pub(crate) struct Report {
    schema: &'static str,
    status: &'static str,
    partition: String,
    device: String,
    catalog_sha256: String,
    lower_checkpoint_sha256: String,
    outer_checkpoint_sha256: String,
    cases: Vec<CaseReport>,
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

    pub(crate) fn actions(
        &self,
        session: &Session,
        counters: &mut Counters,
    ) -> Result<Vec<MacroAction>> {
        ensure!(
            session.observe().active.is_none(),
            "aic-learning.hierarchical-state: macro state contains an active partial route"
        );
        let mut actions = Vec::new();
        if let Ok(observation) = session.learning_observation(&self.domain) {
            for kind in [CommandKind::Place, CommandKind::PlaceTerminal] {
                let page = observation
                    .domain
                    .command_page(kind, 0, self.placement_page);
                actions.extend(
                    page.commands
                        .into_iter()
                        .map(|factored| MacroAction::Command(factored.command.command)),
                );
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
                counters.route_searches += report.searches;
                counters.route_expanded += report.expanded;
                counters.route_preflight_expanded += report.preflight_expanded;
                counters.native_rejections += report.native_rejections.len();
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
                counters.route_searches += report.searches;
                counters.route_expanded += report.expanded;
                counters.route_preflight_expanded += report.preflight_expanded;
                counters.native_rejections += report.native_rejections.len();
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
    ) -> Result<(Vec<f64>, f64)> {
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
            return Ok((
                weights
                    .into_iter()
                    .map(|weight| weight / denominator.max(f64::MIN_POSITIVE))
                    .collect(),
                state_value(session),
            ));
        };
        let input = session
            .macro_learning_input(&self.domain)
            .map_err(anyhow::Error::new)?;
        let features = actions
            .iter()
            .zip(children)
            .map(|(action, child)| action.features(session, child, self.domain.placement))
            .collect::<Vec<_>>();
        let batch = OuterBatch::from_rows(&[input.as_ref()], &[features.as_slice()], self.device)?;
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
        let priors = learned
            .into_iter()
            .zip(anchor_weights)
            .map(|(learned, anchor)| {
                learned * (1.0 - self.anchor_prior_mass())
                    + anchor / denominator.max(f64::MIN_POSITIVE) * self.anchor_prior_mass()
            })
            .collect();
        let completion = prediction.values.double_value(&[0, 0]);
        Ok((priors, completion))
    }

    fn anchor_prior_mass(&self) -> f64 {
        // Config validation owns the range. Keeping this accessor local makes
        // it explicit that the anchor participates only in action ordering.
        self.anchor_prior_mass
    }
}

impl MacroAction {
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
                kind: "place",
                entity: block.clone(),
                state_revision: revision,
                steps: None,
                provenance: None,
            },
            Self::Command(Command::PlaceTerminal { terminal, .. }) => ActionSummary {
                kind: "place-terminal",
                entity: terminal.clone(),
                state_revision: revision,
                steps: None,
                provenance: None,
            },
            Self::Command(_) => unreachable!("macro command generator exposes only placements"),
            Self::Route(candidate) => ActionSummary {
                kind: "route",
                entity: candidate.connection.clone(),
                state_revision: candidate.state_revision,
                steps: Some(candidate.steps),
                provenance: Some(candidate.provenance.clone()),
            },
            Self::Network(candidate) => ActionSummary {
                kind: "network",
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
    ensure!(
        !output.exists(),
        "aic-learning.hierarchical-output: {} already exists",
        output.display()
    );
    let config: Config = read_json(config_path)?;
    let panel: Panel = read_json(panel_path)?;
    validate_config(&config)?;
    validate_panel(&panel)?;
    let lower_config: GuidanceTrainingConfig =
        read_json(&lower_training_output.join("config.json"))?;
    let order: ParameterOrder = read_json(&lower_training_output.join("parameter-order.json"))?;
    let checkpoint = lower_training_output.join("state.safetensors");
    let values: Tensors = Tensor::read_safetensors(&checkpoint)?.into_iter().collect();
    let guidance = GuidanceModel::load(&lower_config.model, &order, &values, device)?;
    let checkpoint_sha256 = crate::input::sha256(&checkpoint)?;
    let outer_config: OuterTrainingConfig = read_json(&outer_training_output.join("config.json"))?;
    let outer_order: OuterOrder = read_json(&outer_training_output.join("parameter-order.json"))?;
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
    // Warm the complete integrated path once outside both measurements. This
    // builds every compiled trace needed by the panel and includes graph/raster
    // inference, exact route search and native replay.
    let warmup = prefix_session(&panel.cases[0], &panel, catalogs.clone())?;
    let _ = beam_search(warmup, &factory, &config)?;
    let mut cases = Vec::with_capacity(panel.cases.len());
    for case in &panel.cases {
        let root = prefix_session(case, &panel, catalogs.clone())?;
        let beam = beam_search(root.clone(), &factory, &config)?;
        let mcts = mcts_search(root, &factory, &config)?;
        cases.push(CaseReport {
            id: case.id.clone(),
            prefix_commands: case.prefix.len(),
            beam,
            mcts,
        });
    }
    let report = Report {
        schema: REPORT_SCHEMA,
        status: "complete",
        partition: panel.partition,
        device: format!("{device:?}"),
        catalog_sha256: catalogs.identity.clone(),
        lower_checkpoint_sha256: checkpoint_sha256,
        outer_checkpoint_sha256,
        cases,
    };
    write_report(output, &report)?;
    Ok(json!({
        "schema": REPORT_SCHEMA,
        "status": "complete",
        "output": output,
        "cases": report.cases.len(),
        "beam_completed": report.cases.iter().filter(|case| case.beam.fully_validated).count(),
        "mcts_completed": report.cases.iter().filter(|case| case.mcts.fully_validated).count(),
        "beam_samples_per_minute": aggregate_throughput(report.cases.iter().map(|case| &case.beam)),
        "mcts_samples_per_minute": aggregate_throughput(report.cases.iter().map(|case| &case.mcts)),
    }))
}

fn beam_search(root: Session, factory: &Factory<'_>, config: &Config) -> Result<SearchResult> {
    let started = Instant::now();
    let mut counters = Counters::default();
    let mut beam = vec![(root, Vec::<ActionSummary>::new())];
    let mut best = beam[0].clone();
    for _ in 0..config.max_moves {
        let mut next = Vec::new();
        for (session, trace) in &beam {
            if session.layout().is_some() {
                best = (session.clone(), trace.clone());
                beam = vec![best.clone()];
                break;
            }
            for action in factory.actions(session, &mut counters)? {
                let child = action.apply(session)?;
                let mut child_trace = trace.clone();
                child_trace.push(action.summary(session.observe().revision));
                if state_value(&child) > state_value(&best.0) {
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
            break;
        }
        next.sort_by(|left, right| state_value(&right.0).total_cmp(&state_value(&left.0)));
        let mut identities = HashSet::new();
        next.retain(|(session, _)| {
            identities.insert(serde_json::to_string(&session.observe()).unwrap())
        });
        next.truncate(config.beam_width);
        beam = next;
        if let Some(completed) = beam.iter().find(|(session, _)| session.layout().is_some()) {
            best = completed.clone();
            break;
        }
    }
    if let Some(completed) = beam.iter().find(|(session, _)| session.layout().is_some()) {
        best = completed.clone();
    }
    Ok(finish_result(
        "beam",
        best.0,
        best.1,
        Vec::new(),
        0,
        counters,
        started,
    ))
}

struct Edge {
    action: MacroAction,
    prior: f64,
    visits: u32,
    value_sum: f64,
    child: Option<usize>,
}

struct Node {
    session: Session,
    visits: u32,
    value: Option<f64>,
    actions: Option<Vec<Edge>>,
}

fn mcts_search(
    mut working: Session,
    factory: &Factory<'_>,
    config: &Config,
) -> Result<SearchResult> {
    let started = Instant::now();
    let mut counters = Counters::default();
    let mut trace = Vec::new();
    let mut root_records = Vec::new();
    let mut simulations = 0usize;
    for _ in 0..config.max_moves {
        if working.layout().is_some() {
            break;
        }
        let mut nodes = vec![Node {
            session: working.clone(),
            visits: 0,
            value: None,
            actions: None,
        }];
        for _ in 0..config.simulations_per_move {
            simulate(&mut nodes, 0, factory, config, &mut counters)?;
            simulations += 1;
        }
        ensure_node_expanded(&mut nodes, 0, factory, &mut counters)?;
        let actions = nodes[0].actions.as_ref().expect("expanded root");
        let Some((selected, _)) = actions.iter().enumerate().max_by(|left, right| {
            left.1
                .visits
                .cmp(&right.1.visits)
                .then_with(|| left.1.prior.total_cmp(&right.1.prior))
        }) else {
            break;
        };
        let action = actions[selected].action.clone();
        let revision = working.observe().revision;
        root_records.push(RootRecord {
            state_sha256: state_sha256(&working)?,
            state_revision: revision,
            actions: actions
                .iter()
                .map(|edge| RootActionRecord {
                    action: edge.action.clone(),
                    prior: edge.prior,
                    visits: edge.visits,
                    q: if edge.visits == 0 {
                        0.0
                    } else {
                        edge.value_sum / f64::from(edge.visits)
                    },
                })
                .collect(),
            selected_action: selected,
            later_completed_connections: 0,
            later_total_connections: 0,
            later_fully_validated: false,
        });
        working = action.apply(&working)?;
        trace.push(action.summary(revision));
    }
    let final_observation = working.observe();
    for record in &mut root_records {
        record.later_completed_connections = final_observation.completed;
        record.later_total_connections = final_observation.total;
        record.later_fully_validated = final_observation.fully_validated;
    }
    Ok(finish_result(
        "mcts",
        working,
        trace,
        root_records,
        simulations,
        counters,
        started,
    ))
}

fn simulate(
    nodes: &mut Vec<Node>,
    root: usize,
    factory: &Factory<'_>,
    config: &Config,
    counters: &mut Counters,
) -> Result<()> {
    let mut current = root;
    let mut path = Vec::<(usize, usize)>::new();
    let value = loop {
        if nodes[current].session.layout().is_some() {
            break 1.0;
        }
        if path.len() == config.horizon {
            break state_value(&nodes[current].session);
        }
        if nodes[current].actions.is_none() {
            break ensure_node_expanded(nodes, current, factory, counters)?;
        }
        let Some(actions) = nodes[current]
            .actions
            .as_ref()
            .filter(|actions| !actions.is_empty())
        else {
            break state_value(&nodes[current].session);
        };
        let parent_visits = nodes[current].visits.max(1) as f64;
        // Actions are stored in descending prior order. Progressive widening
        // keeps a large placement page from consuming every simulation at the
        // root before any promising branch reaches routing.
        let widened = (parent_visits.sqrt().ceil() as usize).clamp(1, actions.len());
        let edge = actions[..widened]
            .iter()
            .enumerate()
            .max_by(|left, right| {
                let score = |edge: &Edge| {
                    let q = if edge.visits == 0 {
                        nodes[current].value.unwrap_or_default()
                    } else {
                        edge.value_sum / f64::from(edge.visits)
                    };
                    q + config.exploration * edge.prior * parent_visits.sqrt()
                        / (1.0 + f64::from(edge.visits))
                };
                score(left.1).total_cmp(&score(right.1))
            })
            .map(|(index, _)| index)
            .expect("nonempty actions");
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
        });
        nodes[current].actions.as_mut().unwrap()[edge].child = Some(child);
        current = child;
        break ensure_node_expanded(nodes, current, factory, counters)?;
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

fn ensure_node_expanded(
    nodes: &mut [Node],
    node: usize,
    factory: &Factory<'_>,
    counters: &mut Counters,
) -> Result<f64> {
    if nodes[node].actions.is_some() {
        return Ok(nodes[node]
            .value
            .unwrap_or_else(|| state_value(&nodes[node].session)));
    }
    let actions = factory.actions(&nodes[node].session, counters)?;
    if actions.is_empty() {
        let value = state_value(&nodes[node].session);
        nodes[node].value = Some(value);
        nodes[node].actions = Some(vec![]);
        return Ok(value);
    }
    let children = actions
        .iter()
        .map(|action| action.apply(&nodes[node].session))
        .collect::<Result<Vec<_>>>()?;
    let (priors, model_value) = factory.score_actions(&nodes[node].session, &actions, &children)?;
    // Expansion already paid for and natively applied every exposed macro.
    // Preserve that exact one-step evidence. The current outer checkpoint was
    // trained on fully placed routing states, so its completion head is not a
    // valid value estimate while any facility or terminal remains unplaced.
    let exact_value = children
        .iter()
        .map(state_value)
        .fold(state_value(&nodes[node].session), f64::max);
    let observation = nodes[node].session.observe();
    let all_entities_placed = observation.poses.iter().all(Option::is_some)
        && observation.terminal_poses.iter().all(Option::is_some);
    let value = if all_entities_placed {
        model_value.max(exact_value)
    } else {
        exact_value
    };
    let mut edges = actions
        .into_iter()
        .zip(priors)
        .map(|(action, prior)| Edge {
            action,
            prior,
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

fn state_value(session: &Session) -> f64 {
    if session.layout().is_some() {
        1.0
    } else {
        construction_progress(session) * 0.9
    }
}

fn finish_result(
    algorithm: &'static str,
    session: Session,
    trace: Vec<ActionSummary>,
    root_records: Vec<RootRecord>,
    simulations: usize,
    counters: Counters,
    started: Instant,
) -> SearchResult {
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let observation = session.observe();
    SearchResult {
        algorithm,
        status: if observation.fully_validated {
            "complete"
        } else {
            "incomplete"
        },
        fully_validated: observation.fully_validated,
        completed_connections: observation.completed,
        total_connections: observation.total,
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
    let bytes = serde_json::to_vec(&session.observe())?;
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

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let metadata = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    ensure!(
        metadata.len() <= MAX_INPUT_BYTES,
        "aic-learning.hierarchical-input: {} exceeds {} bytes",
        path.display(),
        MAX_INPUT_BYTES
    );
    serde_json::from_slice(&fs::read(path)?)
        .with_context(|| format!("parse hierarchical input {}", path.display()))
}

fn write_report(output: &Path, report: &Report) -> Result<()> {
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
    fs::write(&temporary, serde_json::to_vec_pretty(report)?)?;
    fs::rename(temporary, output)?;
    Ok(())
}
