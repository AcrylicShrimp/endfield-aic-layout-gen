//! Stable-state macro search over placement commands and complete route witnesses.

use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::Path,
    sync::Arc,
    time::Instant,
};

use aic_layout::{geometry::Bounds, physical::Problem, validation::Catalogs};
use aic_pilot_native::constructive::{
    Command, Limits, Session,
    learning::{CommandKind, Config as DomainConfig},
    routing_candidates::{Candidate, Config as CandidateConfig, Provenance, Request},
};
use aic_torch::tch::{self, Device, Tensor};
use anyhow::{Context, Result, ensure};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    input::Tensors,
    outer_model::{
        ACTION_FEATURES, Batch as OuterBatch, Model as OuterModel, ParameterOrder as OuterOrder,
    },
    outer_training::Config as OuterTrainingConfig,
    route_guidance::{Model as GuidanceModel, ParameterOrder},
    route_guidance_training::Config as GuidanceTrainingConfig,
};

const CONFIG_SCHEMA: &str = "aic-hierarchical-search-config-v1";
const PANEL_SCHEMA: &str = "aic-hierarchical-search-panel-v1";
const REPORT_SCHEMA: &str = "aic-hierarchical-search-report-v1";
const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;

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

#[derive(Clone)]
pub(crate) enum MacroAction {
    Command(Command),
    Route(Candidate),
}

#[derive(Clone, Debug, Serialize)]
struct ActionSummary {
    kind: &'static str,
    entity: String,
    state_revision: u64,
    steps: Option<i64>,
    provenance: Option<Provenance>,
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
}

#[derive(Clone, Debug, Serialize)]
struct SearchResult {
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
}

#[derive(Debug, Serialize)]
struct CaseReport {
    id: String,
    prefix_commands: usize,
    beam: SearchResult,
    mcts: SearchResult,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    status: &'static str,
    partition: String,
    device: String,
    catalog_sha256: String,
    lower_checkpoint_sha256: String,
    outer_checkpoint_sha256: String,
    cases: Vec<CaseReport>,
}

pub(crate) struct Factory<'a> {
    guidance: &'a GuidanceModel,
    guidance_version: &'a str,
    device: Device,
    domain: DomainConfig,
    candidate: &'a CandidateConfig,
    placement_page: usize,
    pool: rayon::ThreadPool,
    outer: Option<&'a OuterModel>,
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
        outer: Option<&'a OuterModel>,
    ) -> Result<Factory<'a>> {
        Ok(Factory {
            guidance,
            guidance_version,
            device,
            domain,
            candidate,
            placement_page,
            pool: rayon::ThreadPoolBuilder::new()
                .num_threads(cpu_threads)
                .thread_name(|index| format!("hierarchical-route-{index}"))
                .build()?,
            outer,
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
        let ready = session
            .problem()
            .connections
            .iter()
            .enumerate()
            .filter(|(index, _)| observation.routes[*index].is_none())
            .filter_map(|(_, connection)| {
                let request = Request {
                    state_revision: observation.revision,
                    connection: connection.id.clone(),
                };
                session
                    .route_guidance_input(&request, self.candidate, &self.domain)
                    .ok()
                    .map(|input| (request, input))
            })
            .collect::<Vec<_>>();
        if !ready.is_empty() {
            let inputs = ready.iter().map(|(_, input)| input).collect::<Vec<_>>();
            let guidance =
                self.guidance
                    .predict_native(&inputs, self.guidance_version, self.device)?;
            counters.guidance_batches += 1;
            counters.guidance_inputs += inputs.len();
            let reports = self.pool.install(|| {
                ready
                    .par_iter()
                    .zip(guidance.par_iter())
                    .map(|((request, _), guidance)| {
                        session.route_candidates_with_guidance(request, self.candidate, guidance)
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
        Ok(actions)
    }

    fn score_actions(
        &self,
        session: &Session,
        actions: &[MacroAction],
        children: &[Session],
    ) -> Result<(Vec<f64>, f64)> {
        let Some(model) = self.outer else {
            let scores = actions
                .iter()
                .zip(children)
                .map(|(action, child)| action.prior_score(session, child))
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
        let priors = Vec::<f32>::try_from(prediction.logp.exp().to_device(Device::Cpu))?
            .into_iter()
            .map(f64::from)
            .collect();
        let completion = prediction.values.double_value(&[0, 0]);
        Ok((priors, completion))
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
        }
    }

    fn prior_score(&self, before: &Session, after: &Session) -> f64 {
        let before = construction_progress(before);
        let after_progress = construction_progress(after);
        let progress = (after_progress - before) * 8.0;
        match self {
            Self::Command(_) => progress,
            Self::Route(candidate) => {
                progress + f64::from(candidate.predicted_success.unwrap_or(0.5))
                    - candidate.steps as f64 * 0.002
                    - candidate.learned_cost as f64 * 0.0001
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
        }
        row
    }
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
        Some(&outer),
    )?;
    // Warm the complete lower-tool path once outside both measurements. This
    // includes graph/raster inference, exact route search and native replay.
    let warmup = prefix_session(&panel.cases[0], &panel, catalogs.clone())?;
    let _ = factory.actions(&warmup, &mut Counters::default())?;
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
    Ok(finish_result("beam", best.0, best.1, 0, counters, started))
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
        working = action.apply(&working)?;
        trace.push(action.summary(revision));
    }
    Ok(finish_result(
        "mcts",
        working,
        trace,
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
        let edge = actions
            .iter()
            .enumerate()
            .max_by(|left, right| {
                let score = |edge: &Edge| {
                    let q = if edge.visits == 0 {
                        0.0
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
    let (priors, value) = factory.score_actions(&nodes[node].session, &actions, &children)?;
    nodes[node].actions = Some(
        actions
            .into_iter()
            .zip(priors)
            .map(|(action, prior)| Edge {
                action,
                prior,
                visits: 0,
                value_sum: 0.0,
                child: None,
            })
            .collect(),
    );
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
    }
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
            && config.exploration > 0.0,
        "aic-learning.hierarchical-config: invalid schema or bounded search controls"
    );
    Ok(())
}

fn validate_panel(panel: &Panel) -> Result<()> {
    ensure!(
        panel.schema == PANEL_SCHEMA
            && panel.partition == "held-out"
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
