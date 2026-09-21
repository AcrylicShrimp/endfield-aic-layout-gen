//! Fixed-bound policy/value guided Monte Carlo tree search.
//!
//! The native domain enumerates every locally valid command into progressively
//! widened tree nodes. The checkpoint policy supplies only priors and values.
//! Commands are applied through forked constructive sessions, and a committed
//! success is published only after native and independent physical validation.

use std::{collections::VecDeque, fs, path::Path, sync::Arc, time::Instant};

use aic_layout::{
    geometry::Bounds,
    physical::{Layout, Problem},
    projection::rotate,
    validation::{Catalogs, validate},
};
use aic_pilot_native::constructive::{
    Command, Session,
    learning::{CommandKind, Config as DomainConfig, FactoredCommand, Observation},
};
use aic_torch::{
    Generator, deterministic,
    tch::{self, Device, Tensor},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    constructive_problems::ProblemGenerator,
    input::{Tensors, sha256},
    update::device_name,
};

use super::{
    Model, policy,
    rollout::RewardConfig,
    run::{Config as TrainingConfig, load_catalogs, read_json, validate_config, write_json},
};

#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub width: i64,
    pub height: i64,
    pub simulations_per_move: usize,
    pub proof_simulations_per_move: usize,
    pub horizon: usize,
    pub max_actions_per_widening: usize,
    pub initial_children: usize,
    pub widening: f64,
    pub exploration: f64,
    pub route_potential_scale: f64,
    pub max_policy_evaluations: usize,
    pub baseline_seed: u64,
    pub cpu_threads: usize,
}

impl Config {
    fn validate(&self) -> Result<()> {
        let area = self.width.checked_mul(self.height);
        ensure!(
            self.width > 0
                && self.height > 0
                && area.is_some_and(|area| area <= 65_536)
                && (2..=65_536).contains(&self.simulations_per_move)
                && (self.simulations_per_move..=65_536).contains(&self.proof_simulations_per_move)
                && (1..=4096).contains(&self.horizon)
                && (1..=4096).contains(&self.max_actions_per_widening)
                && (1..=4096).contains(&self.initial_children)
                && self.widening.is_finite()
                && self.widening >= 0.
                && self.exploration.is_finite()
                && self.exploration > 0.
                && self.route_potential_scale.is_finite()
                && self.route_potential_scale >= 0.
                && (2..=10_000_000).contains(&self.max_policy_evaluations)
                && (1..=64).contains(&self.cpu_threads),
            "aic-learning.constructive-search-config: invalid fixed-bound search configuration"
        );
        Ok(())
    }

    fn domain(&self, training: &TrainingConfig) -> DomainConfig {
        DomainConfig {
            placement: Bounds {
                min_x: 0,
                min_y: 0,
                max_x: self.width - 1,
                max_y: self.height - 1,
            },
            extension_lengths: training.domain.extension_lengths.clone(),
        }
    }
}

#[derive(Default, Serialize)]
struct Counters {
    simulations: usize,
    policy_evaluations: usize,
    actor_scoring_batches: usize,
    actor_choices_scored: usize,
    session_forks: usize,
    nodes: usize,
    dead_end_leaves: usize,
    terminal_leaves: usize,
    value_clamps: usize,
    dominated_aborts_filtered: usize,
    valid_actions_enumerated: usize,
    widening_batches: usize,
    begin_routes_applied: usize,
    extensions_applied: usize,
    connects_applied: usize,
    complete_route_witnesses: usize,
    maximum_simulation_depth: usize,
}

#[derive(Serialize)]
struct TraceEntry {
    decision: usize,
    command: Command,
    root_visits: u32,
    selected_visits: u32,
    selected_mean_return: f64,
    selected_prior: f64,
    progress_before: f64,
    progress_after: f64,
    completed_connections: usize,
    total_connections: usize,
}

struct Edge {
    command: Command,
    joint_logp: f64,
    prior: f64,
    visits: u32,
    value_sum: f64,
    child: Option<usize>,
    progress_witness: Option<Vec<Command>>,
}

struct KindPage {
    kind: CommandKind,
    admitted: usize,
    next_offset: usize,
    exhausted: bool,
    pending: VecDeque<FactoredCommand>,
}

struct PageState {
    kinds: Vec<KindPage>,
    next_kind: usize,
}

#[derive(Serialize)]
struct KindFrontier {
    kind: CommandKind,
    admitted: usize,
    generated_through_offset: usize,
    buffered: usize,
    exhausted: bool,
}

#[derive(Serialize)]
struct ValidActionFrontier {
    admitted_children: usize,
    all_native_actions_admitted: bool,
    kinds: Vec<KindFrontier>,
}

#[derive(Default)]
struct Node {
    visits: u32,
    value_sum: f64,
    value: Option<f64>,
    children: Option<Vec<Edge>>,
    pages: Option<PageState>,
    dead_end: bool,
}

struct PathStep {
    node: usize,
    edge: usize,
    command: Command,
    reward: f64,
    progress_delta: f64,
}

struct Search<'a> {
    model: &'a Model,
    device: Device,
    domain: &'a DomainConfig,
    reward: &'a RewardConfig,
    gamma: f64,
    config: &'a Config,
    nodes: Vec<Node>,
    root: usize,
    counters: Counters,
}

impl<'a> Search<'a> {
    fn new(
        model: &'a Model,
        device: Device,
        domain: &'a DomainConfig,
        reward: &'a RewardConfig,
        gamma: f64,
        config: &'a Config,
    ) -> Self {
        Self {
            model,
            device,
            domain,
            reward,
            gamma,
            config,
            nodes: vec![Node::default()],
            root: 0,
            counters: Counters {
                nodes: 1,
                ..Counters::default()
            },
        }
    }

    fn clamp_value(&mut self, value: f64) -> f64 {
        let lower = self.reward.unfinished_penalty.min(0.);
        let upper = self.reward.progress_scale + self.reward.full_validation_bonus;
        let clamped = value.clamp(lower, upper);
        self.counters.value_clamps += usize::from(clamped != value);
        clamped
    }

    fn route_potential(&self, session: &Session) -> f64 {
        -session
            .active_route_manhattan_distance()
            .map_or(0., |distance| distance as f64)
            / self.config.width.max(self.config.height) as f64
    }

    fn gather_valid_actions(
        &mut self,
        node: usize,
        observation: &Observation,
        maximum: usize,
    ) -> Vec<FactoredCommand> {
        let pages = self.nodes[node]
            .pages
            .as_mut()
            .expect("expanded node has command pages");
        let mut gathered = Vec::with_capacity(maximum);
        let mut empty_scans = 0usize;
        while gathered.len() < maximum && empty_scans < pages.kinds.len() {
            let index = pages.next_kind;
            pages.next_kind = (pages.next_kind + 1) % pages.kinds.len();
            let state = &mut pages.kinds[index];
            if state.pending.is_empty() && !state.exhausted {
                let page = observation.domain.command_page(
                    state.kind,
                    state.next_offset,
                    self.config.max_actions_per_widening,
                );
                state.next_offset = page.next_offset;
                state.exhausted = page.exhausted;
                state.pending.extend(page.commands);
            }
            if let Some(command) = state.pending.pop_front() {
                state.admitted += 1;
                gathered.push(command);
                empty_scans = 0;
            } else {
                empty_scans += 1;
            }
        }
        gathered
    }

    fn add_valid_actions(
        &mut self,
        node: usize,
        observation: &Observation,
        target: usize,
    ) -> Result<Option<f64>> {
        let current = self.nodes[node]
            .children
            .as_ref()
            .expect("expanded node has children")
            .len();
        let maximum = target
            .saturating_sub(current)
            .min(self.config.max_actions_per_widening);
        if maximum == 0 || self.counters.policy_evaluations >= self.config.max_policy_evaluations {
            return Ok(None);
        }
        let commands = self.gather_valid_actions(node, observation, maximum);
        if commands.is_empty() {
            return Ok(None);
        }
        let scored =
            policy::score_factored_commands(self.model, observation, &commands, self.device)?;
        self.counters.policy_evaluations += 1;
        self.counters.actor_scoring_batches += scored.scoring_batches;
        self.counters.actor_choices_scored += scored.scored_choices;
        self.counters.valid_actions_enumerated += scored.choices.len();
        self.counters.widening_batches += 1;
        let value = self.clamp_value(scored.value);
        let children = self.nodes[node].children.as_mut().unwrap();
        children.extend(scored.choices.into_iter().map(|choice| Edge {
            command: choice.command.command,
            joint_logp: choice.old_logp,
            prior: 0.,
            visits: 0,
            value_sum: 0.,
            child: None,
            progress_witness: None,
        }));
        let maximum_logp = children
            .iter()
            .map(|edge| edge.joint_logp)
            .fold(f64::NEG_INFINITY, f64::max);
        let total = children
            .iter()
            .map(|edge| (edge.joint_logp - maximum_logp).exp())
            .sum::<f64>();
        ensure!(
            total.is_finite() && total > 0.,
            "aic-learning.constructive-search-prior: invalid native action mass"
        );
        for edge in children {
            edge.prior = (edge.joint_logp - maximum_logp).exp() / total;
        }
        Ok(Some(value))
    }

    fn expand(&mut self, node: usize, session: &Session) -> Result<f64> {
        if let Some(value) = self.nodes[node].value {
            return Ok(value);
        }
        let observation = match session.learning_observation(self.domain) {
            Ok(observation) => observation,
            Err(diagnostic) if diagnostic.code == "constructive.empty-learning-domain" => {
                self.nodes[node].dead_end = true;
                self.nodes[node].value = Some(self.reward.unfinished_penalty);
                self.nodes[node].children = Some(vec![]);
                self.counters.dead_end_leaves += 1;
                return Ok(self.reward.unfinished_penalty);
            }
            Err(diagnostic) => return Err(diagnostic.into()),
        };
        let mut kinds = observation.domain.available_command_kinds();
        if let Some(abort) = kinds
            .iter()
            .position(|kind| *kind == CommandKind::AbortRoute)
        {
            kinds.remove(abort);
            self.counters.dominated_aborts_filtered += 1;
        }
        if kinds.is_empty() {
            self.nodes[node].dead_end = true;
            self.nodes[node].value = Some(self.reward.unfinished_penalty);
            self.nodes[node].children = Some(vec![]);
            self.counters.dead_end_leaves += 1;
            return Ok(self.reward.unfinished_penalty);
        }
        ensure!(
            self.counters.policy_evaluations < self.config.max_policy_evaluations,
            "aic-learning.constructive-search-budget: policy evaluation budget exhausted"
        );
        let kind_count = kinds.len();
        self.nodes[node].children = Some(vec![]);
        self.nodes[node].pages = Some(PageState {
            kinds: kinds
                .into_iter()
                .map(|kind| KindPage {
                    kind,
                    admitted: 0,
                    next_offset: 0,
                    exhausted: false,
                    pending: VecDeque::new(),
                })
                .collect(),
            next_kind: 0,
        });
        let initial = self.config.initial_children.max(kind_count);
        let mut value = None;
        while self.nodes[node].children.as_ref().unwrap().len() < initial
            && self.counters.policy_evaluations < self.config.max_policy_evaluations
        {
            let Some(scored_value) = self.add_valid_actions(node, &observation, initial)? else {
                break;
            };
            value.get_or_insert(scored_value);
        }
        let Some(value) = value else {
            self.nodes[node].dead_end = true;
            self.nodes[node].value = Some(self.reward.unfinished_penalty);
            self.counters.dead_end_leaves += 1;
            return Ok(self.reward.unfinished_penalty);
        };
        self.nodes[node].value = Some(value);
        Ok(value)
    }

    fn widen(&mut self, node: usize, session: &Session) -> Result<()> {
        if self.nodes[node].dead_end
            || self.counters.policy_evaluations >= self.config.max_policy_evaluations
            || self.nodes[node].pages.as_ref().is_some_and(|pages| {
                pages
                    .kinds
                    .iter()
                    .all(|kind| kind.exhausted && kind.pending.is_empty())
            })
        {
            return Ok(());
        }
        let base = self.config.initial_children.max(
            self.nodes[node]
                .pages
                .as_ref()
                .map_or(0, |pages| pages.kinds.len()),
        );
        let target = base
            + (self.config.widening * f64::from(self.nodes[node].visits).sqrt()).floor() as usize;
        let current = self.nodes[node]
            .children
            .as_ref()
            .expect("expanded node has children")
            .len();
        if current >= target {
            return Ok(());
        }
        let observation = session
            .learning_observation(self.domain)
            .map_err(anyhow::Error::new)?;
        self.add_valid_actions(node, &observation, target)?;
        Ok(())
    }

    fn select(&self, node: usize) -> Option<usize> {
        let current = &self.nodes[node];
        let children = current.children.as_ref()?;
        if children.is_empty() {
            return None;
        }
        if let Some(unvisited) = children.iter().position(|edge| edge.visits == 0) {
            return Some(unvisited);
        }
        let scale = f64::from(current.visits.max(1)).sqrt();
        (0..children.len()).max_by(|&left, &right| {
            let score = |edge: &Edge| {
                let mean = edge.value_sum / f64::from(edge.visits);
                mean + self.config.exploration * edge.prior * scale / (1. + f64::from(edge.visits))
            };
            score(&children[left])
                .total_cmp(&score(&children[right]))
                .then_with(|| right.cmp(&left))
        })
    }

    fn simulate(&mut self, committed: &Session) -> Result<()> {
        let mut session = committed.clone();
        self.counters.session_forks += 1;
        self.counters.simulations += 1;
        let mut node = self.root;
        let mut path = Vec::new();
        let leaf_value;
        loop {
            if session.done() {
                self.counters.terminal_leaves += 1;
                leaf_value = 0.;
                break;
            }
            if self.nodes[node].children.is_none() {
                leaf_value = self.expand(node, &session)?;
                break;
            }
            if path.len() >= self.config.horizon || self.nodes[node].dead_end {
                leaf_value = self.nodes[node].value.unwrap_or(0.);
                break;
            }
            self.widen(node, &session)?;
            let Some(edge) = self.select(node) else {
                leaf_value = self.nodes[node]
                    .value
                    .unwrap_or(self.reward.unfinished_penalty);
                break;
            };
            let command = self.nodes[node].children.as_ref().unwrap()[edge]
                .command
                .clone();
            let potential_before = self.route_potential(&session);
            let step = session.step(command.clone());
            ensure!(
                step.accepted,
                "aic-learning.constructive-search-replay: proposed tree command was rejected: {:?}",
                step.diagnostic
            );
            match &command {
                Command::BeginRoute { .. } => self.counters.begin_routes_applied += 1,
                Command::Extend { .. } => self.counters.extensions_applied += 1,
                Command::Connect => self.counters.connects_applied += 1,
                _ => {}
            }
            let potential_after = self.route_potential(&session);
            let reward = self.reward.reward(&step)
                + self.config.route_potential_scale
                    * (self.gamma * potential_after - potential_before);
            path.push(PathStep {
                node,
                edge,
                command,
                reward,
                progress_delta: step.progress_delta,
            });
            let child = if let Some(child) = self.nodes[node].children.as_ref().unwrap()[edge].child
            {
                child
            } else {
                let child = self.nodes.len();
                self.nodes.push(Node::default());
                self.nodes[node].children.as_mut().unwrap()[edge].child = Some(child);
                self.counters.nodes += 1;
                child
            };
            node = child;
            if step.terminal {
                self.counters.terminal_leaves += 1;
                leaf_value = 0.;
                break;
            }
            if self.nodes[node].children.is_none() {
                leaf_value = self.expand(node, &session)?;
                break;
            }
        }
        self.nodes[node].visits += 1;
        self.nodes[node].value_sum += leaf_value;
        self.counters.maximum_simulation_depth =
            self.counters.maximum_simulation_depth.max(path.len());
        let mut next_progress = None;
        for start in (0..path.len()).rev() {
            if path[start].progress_delta > 0. {
                next_progress = Some(start);
            }
            if let Some(progress) = next_progress {
                let witness = path[start..=progress]
                    .iter()
                    .map(|step| step.command.clone())
                    .collect::<Vec<_>>();
                let edge =
                    &mut self.nodes[path[start].node].children.as_mut().unwrap()[path[start].edge];
                if edge
                    .progress_witness
                    .as_ref()
                    .is_none_or(|existing| witness.len() < existing.len())
                {
                    if matches!(path[start].command, Command::BeginRoute { .. })
                        && witness
                            .last()
                            .is_some_and(|command| command == &Command::Connect)
                    {
                        self.counters.complete_route_witnesses += 1;
                    }
                    edge.progress_witness = Some(witness);
                }
            }
        }
        let mut backed = leaf_value;
        for step in path.into_iter().rev() {
            backed = step.reward + self.gamma * backed;
            let edge = &mut self.nodes[step.node].children.as_mut().unwrap()[step.edge];
            edge.visits += 1;
            edge.value_sum += backed;
            self.nodes[step.node].visits += 1;
            self.nodes[step.node].value_sum += backed;
        }
        Ok(())
    }

    fn best_root_edge(&self) -> Option<usize> {
        self.nodes[self.root]
            .children
            .as_ref()?
            .iter()
            .enumerate()
            .filter(|(_, edge)| {
                edge.visits > 0 && edge.child.is_some() && edge.progress_witness.is_some()
            })
            .max_by(|(left_index, left), (right_index, right)| {
                left.visits
                    .cmp(&right.visits)
                    .then_with(|| {
                        (left.value_sum / f64::from(left.visits))
                            .total_cmp(&(right.value_sum / f64::from(right.visits)))
                    })
                    .then_with(|| left.prior.total_cmp(&right.prior))
                    .then_with(|| right_index.cmp(left_index))
            })
            .map(|(index, _)| index)
    }

    fn root_frontier(&self) -> Option<ValidActionFrontier> {
        let node = &self.nodes[self.root];
        let pages = node.pages.as_ref()?;
        Some(ValidActionFrontier {
            admitted_children: node.children.as_ref().map_or(0, Vec::len),
            all_native_actions_admitted: pages
                .kinds
                .iter()
                .all(|kind| kind.exhausted && kind.pending.is_empty()),
            kinds: pages
                .kinds
                .iter()
                .map(|kind| KindFrontier {
                    kind: kind.kind,
                    admitted: kind.admitted,
                    generated_through_offset: kind.next_offset,
                    buffered: kind.pending.len(),
                    exhausted: kind.exhausted,
                })
                .collect(),
        })
    }

    fn commit(&mut self, session: &mut Session, decision: usize) -> Result<Vec<TraceEntry>> {
        let Some(edge_index) = self.best_root_edge() else {
            return Ok(vec![]);
        };
        let witness = self.nodes[self.root].children.as_ref().unwrap()[edge_index]
            .progress_witness
            .clone()
            .expect("selected edge has a progress witness");
        let mut committed = Vec::with_capacity(witness.len());
        for command in witness {
            let root = self.root;
            let edge_index = self.nodes[root]
                .children
                .as_ref()
                .and_then(|children| {
                    children.iter().position(|edge| {
                        edge.command == command && edge.visits > 0 && edge.child.is_some()
                    })
                })
                .context(
                    "aic-learning.constructive-search-commit: witness edge is missing from tree",
                )?;
            let edge = &self.nodes[root].children.as_ref().unwrap()[edge_index];
            let selected_visits = edge.visits;
            let selected_mean_return = edge.value_sum / f64::from(edge.visits);
            let selected_prior = edge.prior;
            let child = edge.child.expect("visited edge has a child");
            let root_visits = self.nodes[root].visits;
            let before = construction_progress(session);
            let step = session.step(command.clone());
            ensure!(
                step.accepted,
                "aic-learning.constructive-search-commit: witnessed command was rejected: {:?}",
                step.diagnostic
            );
            self.root = child;
            let observed = session.observe();
            committed.push(TraceEntry {
                decision: decision + committed.len(),
                command,
                root_visits,
                selected_visits,
                selected_mean_return,
                selected_prior,
                progress_before: before,
                progress_after: construction_progress(session),
                completed_connections: observed.completed,
                total_connections: observed.total,
            });
        }
        Ok(committed)
    }
}

#[derive(Serialize)]
struct BaselineSummary {
    seed: u64,
    policy_evaluations: usize,
    actor_scoring_batches: usize,
    actor_choices_scored: usize,
    episodes_started: usize,
    terminal_episodes: usize,
    fully_validated: usize,
    dead_ends: usize,
    best_progress: f64,
    best_completed_connections: usize,
    total_connections: usize,
    seconds: f64,
}

fn policy_baseline(
    problem: &Problem,
    catalogs: Arc<Catalogs>,
    native: aic_pilot_native::constructive::Limits,
    domain: &DomainConfig,
    model: &Model,
    device: Device,
    evaluations: usize,
    seed: u64,
) -> Result<(BaselineSummary, Option<Layout>)> {
    let started = Instant::now();
    let mut sampler = Generator::new(seed)?;
    let mut used = 0usize;
    let mut actor_scoring_batches = 0usize;
    let mut actor_choices_scored = 0usize;
    let mut episodes_started = 0usize;
    let mut terminal_episodes = 0usize;
    let mut fully_validated = 0usize;
    let mut dead_ends = 0usize;
    let mut best_progress = 0.;
    let mut best_completed_connections = 0usize;
    let total_connections = problem.connections.len();
    let mut successful = None;
    while used < evaluations {
        episodes_started += 1;
        let mut session =
            Session::new(problem.clone(), catalogs.clone(), native).map_err(anyhow::Error::new)?;
        let mut dead_end = false;
        while used < evaluations && !session.done() {
            let observation = match session.learning_observation(domain) {
                Ok(observation) => observation,
                Err(diagnostic) if diagnostic.code == "constructive.empty-learning-domain" => {
                    dead_end = true;
                    dead_ends += 1;
                    break;
                }
                Err(diagnostic) => return Err(diagnostic.into()),
            };
            let choice = policy::sample(model, &[&observation], &mut sampler, device)?
                .pop()
                .expect("one observation produces one choice");
            actor_scoring_batches += choice.factors.len();
            actor_choices_scored += choice
                .factors
                .iter()
                .map(|factor| observation.domain.choices(&factor.cursor).len())
                .sum::<usize>();
            let step = session.step_selected(choice.command);
            ensure!(
                step.accepted,
                "aic-learning.constructive-baseline: sampled command was rejected"
            );
            used += 1;
            let observed = session.observe();
            let progress = construction_progress(&session);
            if progress > best_progress {
                best_progress = progress;
                best_completed_connections = observed.completed;
            }
        }
        if session.done() {
            terminal_episodes += 1;
        }
        if let Some(layout) = session.layout() {
            fully_validated += 1;
            successful.get_or_insert(layout);
        }
        if !dead_end {
            let observed = session.observe();
            let progress = construction_progress(&session);
            if progress > best_progress {
                best_progress = progress;
                best_completed_connections = observed.completed;
            }
        }
    }
    Ok((
        BaselineSummary {
            seed,
            policy_evaluations: used,
            actor_scoring_batches,
            actor_choices_scored,
            episodes_started,
            terminal_episodes,
            fully_validated,
            dead_ends,
            best_progress,
            best_completed_connections,
            total_connections,
            seconds: started.elapsed().as_secs_f64(),
        },
        successful,
    ))
}

fn construction_progress(session: &Session) -> f64 {
    let observed = session.observe();
    let complete = observed.poses.iter().filter(|pose| pose.is_some()).count()
        + observed
            .terminal_poses
            .iter()
            .filter(|pose| pose.is_some())
            .count()
        + observed
            .routes
            .iter()
            .filter(|route| route.is_some())
            .count();
    let total = observed.poses.len() + observed.terminal_poses.len() + observed.routes.len();
    complete as f64 / total as f64
}

fn validate_bounded_layout(layout: &Layout, catalogs: &Catalogs, bounds: Bounds) -> Result<Value> {
    ensure!(
        layout.solution.bridges.is_empty(),
        "aic-learning.constructive-search-bound: native result unexpectedly contains bridges"
    );
    for pose in &layout.solution.poses {
        let block = layout
            .problem
            .blocks
            .iter()
            .find(|block| block.id == pose.block)
            .context("aic-learning.constructive-search-bound: pose block is missing")?;
        let definition = catalogs
            .facilities
            .facility(&block.facility)
            .context("aic-learning.constructive-search-bound: facility is missing")?;
        let (width, height, _) = rotate(definition, pose.rotation, pose.position);
        ensure!(
            bounds.contains(pose.position)
                && bounds.contains(aic_layout::geometry::Point {
                    x: pose.position.x + width - 1,
                    y: pose.position.y + height - 1,
                }),
            "aic-learning.constructive-search-bound: facility {} leaves the fixed bound",
            pose.block
        );
    }
    ensure!(
        layout
            .solution
            .terminal_poses
            .iter()
            .all(|pose| bounds.contains(pose.position))
            && layout
                .solution
                .routes
                .iter()
                .flat_map(|route| &route.cells)
                .all(|&point| bounds.contains(point)),
        "aic-learning.constructive-search-bound: terminal or route leaves the fixed bound"
    );
    let metrics = validate(layout, catalogs)
        .map_err(|diagnostic| anyhow::anyhow!("{}: {}", diagnostic.code, diagnostic.message))?;
    Ok(serde_json::to_value(metrics)?)
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    training_output: &Path,
    catalog_root: &Path,
    source_plan: &Path,
    output: &Path,
    config: Config,
    device: Device,
) -> Result<Value> {
    config.validate()?;
    ensure!(
        training_output.is_dir(),
        "aic-learning.constructive-search-input: missing {}",
        training_output.display()
    );
    ensure!(
        !output.exists(),
        "aic-learning.constructive-search-output: {} already exists",
        output.display()
    );
    let training: TrainingConfig = read_json(&training_output.join("config.json"))?;
    validate_config(&training)?;
    let parameter_order: Vec<String> = read_json(&training_output.join("parameter-order.json"))?;
    let training_report: Value = read_json(&training_output.join("report.json"))?;
    let checkpoint = training_output.join("learner.safetensors");
    let learner_sha256 = sha256(&checkpoint)?;
    let catalogs = load_catalogs(catalog_root)?;
    let generator = ProblemGenerator::load(&training_output.join("sampling.json"), catalog_root)?;
    ensure!(
        training_report["schema"] == "aic-constructive-training-report-v2"
            && training_report["status"] == "completed"
            && training_report["catalog_sha256"].as_str() == Some(&catalogs.identity)
            && training_report["sampling_sha256"].as_str()
                == Some(generator.sampling_sha256.as_str())
            && training_report["learner_sha256"].as_str() == Some(learner_sha256.as_str()),
        "aic-learning.constructive-search-identity: training report, sampling, catalogs or checkpoint do not match"
    );
    let prepared = generator.prepare_source_plan(source_plan, "explicit-source-plan")?;
    let domain = config.domain(&training);
    Session::new(prepared.problem.clone(), catalogs.clone(), training.native)
        .map_err(anyhow::Error::new)?
        .learning_observation(&domain)
        .map_err(anyhow::Error::new)?;

    rayon::ThreadPoolBuilder::new()
        .num_threads(config.cpu_threads)
        .build_global()
        .context("aic-learning.constructive-search-threads: initialize CPU worker pool")?;
    tch::set_num_threads(config.cpu_threads as i32);
    tch::set_num_interop_threads(1);
    deterministic()?;
    let values: Tensors = Tensor::read_safetensors(&checkpoint)?.into_iter().collect();
    let model = Model::load(&training.model, &parameter_order, &values, device)?;

    let started = Instant::now();
    let mut session = Session::new(prepared.problem.clone(), catalogs.clone(), training.native)
        .map_err(anyhow::Error::new)?;
    let mut search = Search::new(
        &model,
        device,
        &domain,
        &training.reward,
        training.gamma,
        &config,
    );
    let mut trace = Vec::new();
    let mut stop_reason = "native-terminal";
    while !session.done() {
        let mut simulations = 0usize;
        while simulations < config.simulations_per_move
            && search.counters.policy_evaluations < config.max_policy_evaluations
        {
            search.simulate(&session)?;
            simulations += 1;
        }
        while search.best_root_edge().is_none()
            && simulations < config.proof_simulations_per_move
            && search.counters.policy_evaluations < config.max_policy_evaluations
        {
            search.simulate(&session)?;
            simulations += 1;
        }
        let committed = search.commit(&mut session, trace.len())?;
        if !committed.is_empty() {
            trace.extend(committed);
        } else {
            stop_reason = if search.counters.policy_evaluations >= config.max_policy_evaluations {
                "policy-evaluation-budget"
            } else {
                "no-complete-progress-witness"
            };
            break;
        }
        if search.counters.policy_evaluations >= config.max_policy_evaluations && !session.done() {
            stop_reason = "policy-evaluation-budget";
            break;
        }
    }
    let search_seconds = started.elapsed().as_secs_f64();
    let search_layout = session.layout();
    let bounds = domain.placement;
    let search_metrics = search_layout
        .as_ref()
        .map(|layout| validate_bounded_layout(layout, &catalogs, bounds))
        .transpose()?;
    let search_status = if search_layout.is_some() {
        "feasible"
    } else {
        "unknown"
    };
    let observed = session.observe();
    let search_progress = construction_progress(&session);
    let valid_action_frontier = search.root_frontier();

    let baseline_budget = search.counters.policy_evaluations;
    let (baseline, baseline_layout) = policy_baseline(
        &prepared.problem,
        catalogs.clone(),
        training.native,
        &domain,
        &model,
        device,
        baseline_budget,
        config.baseline_seed,
    )?;
    let baseline_metrics = baseline_layout
        .as_ref()
        .map(|layout| validate_bounded_layout(layout, &catalogs, bounds))
        .transpose()?;

    fs::create_dir_all(output)?;
    write_json(&output.join("problem.json"), &prepared)?;
    write_json(&output.join("search-trace.json"), &trace)?;
    if let Some(layout) = &search_layout {
        write_json(&output.join("search-layout.json"), layout)?;
    }
    if let Some(layout) = &baseline_layout {
        write_json(&output.join("baseline-layout.json"), layout)?;
    }
    let report = json!({
        "schema":"aic-constructive-fixed-bound-search-v1",
        "status":"completed",
        "classification":search_status,
        "stop_reason":if search_layout.is_some() { "independently-validated" } else { stop_reason },
        "bound":{"width":config.width,"height":config.height,"area":config.width * config.height},
        "problem":{
            "id":prepared.id,
            "target_item":prepared.target_item,
            "facility_count":prepared.facility_count,
            "terminal_count":prepared.problem.terminals.len(),
            "connection_count":prepared.problem.connections.len(),
            "source_plan":source_plan,
        },
        "search":{
            "fully_validated":search_layout.is_some(),
            "decisions":trace.len(),
            "construction_progress":search_progress,
            "completed_connections":observed.completed,
            "total_connections":observed.total,
            "metrics":search_metrics,
            "seconds":search_seconds,
            "counters":search.counters,
            "valid_action_frontier":valid_action_frontier,
            "trace":"search-trace.json",
            "layout":search_layout.as_ref().map(|_| "search-layout.json"),
        },
        "policy_sampling_control":{
            "budget_contract":"same encoded-observation count as MCTS node evaluations",
            "summary":baseline,
            "metrics":baseline_metrics,
            "layout":baseline_layout.as_ref().map(|_| "baseline-layout.json"),
        },
        "search_config":config,
        "checkpoint":{
            "training_output":training_output,
            "learner_sha256":learner_sha256,
            "catalog_sha256":catalogs.identity,
            "device":device_name(device)?,
        },
        "files":["problem.json","search-trace.json","report.json"],
    });
    write_json(&output.join("report.json"), &report)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ConstructiveModelConfig;
    use aic_layout::geometry::Point;
    use aic_pilot_native::constructive::Direction;

    fn model() -> Model {
        let config = ConstructiveModelConfig {
            schema: ConstructiveModelConfig::SCHEMA.into(),
            width: 16,
            message_rounds: 1,
        };
        let (order, values) = Model::initial_state(&config, 17).unwrap();
        Model::load(&config, &order, &values, Device::Cpu).unwrap()
    }

    #[test]
    fn native_pages_replay_and_puct_commits_a_visited_edge() {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let model = model();
        let domain = super::super::test_support::domain();
        let session = super::super::test_support::session(32);
        let observation = session.learning_observation(&domain).unwrap();
        let commands = observation
            .domain
            .available_command_kinds()
            .into_iter()
            .flat_map(|kind| observation.domain.command_page(kind, 0, 2).commands)
            .collect::<Vec<_>>();
        let scored =
            policy::score_factored_commands(&model, &observation, &commands, Device::Cpu).unwrap();
        assert_eq!(scored.choices.len(), commands.len());
        for choice in scored.choices {
            let mut fork = session.clone();
            assert!(fork.step_selected(choice.command).accepted);
        }

        let reward = RewardConfig {
            progress_scale: 1.,
            full_validation_bonus: 5.,
            rejected_penalty: -0.05,
            unfinished_penalty: -1.,
        };
        let config = Config {
            width: 10,
            height: 10,
            simulations_per_move: 8,
            proof_simulations_per_move: 32,
            horizon: 4,
            max_actions_per_widening: 8,
            initial_children: 2,
            widening: 1.,
            exploration: 1.25,
            route_potential_scale: 1.,
            max_policy_evaluations: 64,
            baseline_seed: 1,
            cpu_threads: 4,
        };
        let mut search = Search::new(&model, Device::Cpu, &domain, &reward, 0.99, &config);
        for _ in 0..config.simulations_per_move {
            search.simulate(&session).unwrap();
        }
        let mut committed = session;
        let entries = search.commit(&mut committed, 0).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].selected_visits > 0);
        assert_eq!(committed.observe().turns, 1);
    }

    #[test]
    fn route_progress_is_committed_as_one_complete_witness() {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let model = model();
        let domain = super::super::test_support::domain();
        let mut session = super::super::test_support::session(32);
        for command in [
            Command::Place {
                block: "source".into(),
                position: Point { x: 5, y: 8 },
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
        let witness = vec![
            Command::BeginRoute {
                connection: "connection-0".into(),
                source_port: Some("output-belt-1".into()),
                target_port: Some("input-belt-1".into()),
            },
            Command::Extend {
                direction: Direction::North,
                length: 2,
            },
            Command::Connect,
        ];
        let reward = RewardConfig {
            progress_scale: 1.,
            full_validation_bonus: 5.,
            rejected_penalty: -0.05,
            unfinished_penalty: -1.,
        };
        let config = Config {
            width: 10,
            height: 10,
            simulations_per_move: 8,
            proof_simulations_per_move: 32,
            horizon: 4,
            max_actions_per_widening: 8,
            initial_children: 2,
            widening: 1.,
            exploration: 1.25,
            route_potential_scale: 1.,
            max_policy_evaluations: 64,
            baseline_seed: 1,
            cpu_threads: 4,
        };
        let edge = |command: Command, child, progress_witness| Edge {
            command,
            joint_logp: 0.,
            prior: 1.,
            visits: 4,
            value_sum: 2.,
            child: Some(child),
            progress_witness,
        };
        let mut search = Search::new(&model, Device::Cpu, &domain, &reward, 0.99, &config);
        search.nodes = vec![
            Node {
                visits: 4,
                value: Some(0.),
                children: Some(vec![edge(witness[0].clone(), 1, Some(witness.clone()))]),
                ..Node::default()
            },
            Node {
                visits: 4,
                value: Some(0.),
                children: Some(vec![edge(witness[1].clone(), 2, None)]),
                ..Node::default()
            },
            Node {
                visits: 4,
                value: Some(0.),
                children: Some(vec![edge(witness[2].clone(), 3, None)]),
                ..Node::default()
            },
            Node::default(),
        ];
        search.counters.nodes = 4;

        let entries = search.commit(&mut session, 0).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            entries
                .iter()
                .map(|entry| &entry.command)
                .collect::<Vec<_>>(),
            witness.iter().collect::<Vec<_>>()
        );
        assert!(session.done());
        assert!(session.layout().is_some());
        assert_eq!(search.root, 3);
    }
}
