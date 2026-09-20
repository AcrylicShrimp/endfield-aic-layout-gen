//! Self-roll-in training for one-connection learned route options.

mod replay;

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
    sync::Arc,
    time::Instant,
};

use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::Point,
    logistics::TransportKind,
    physical::{Block, Connection, Problem},
    projection::rotate,
    rate::Rate,
    validation::Catalogs,
};
use aic_pilot_native::constructive::{
    Command, Limits, Observation as NativeObservation, Session, learning::Config as DomainConfig,
};
use aic_torch::{
    Generator, deterministic,
    tch::{self, Device, Tensor},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    input::sha256,
    schema::{ConstructiveModelConfig, UpdateConfig},
    update::device_name,
};

use super::{
    policy,
    ppo::Learner,
    rollout::{self, Collection, Episode, Sampling, Transition},
    run::{load_catalogs, read_json, write_json},
};

use replay::{Buffer as ReplayBuffer, Config as ReplayConfig};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub model: ConstructiveModelConfig,
    pub optimizer: UpdateConfig,
    pub domain: DomainConfig,
    pub native: Limits,
    pub task: TaskConfig,
    pub reward: RewardConfig,
    #[serde(default)]
    pub replay: ReplayConfig,
    pub updates: u64,
    pub sessions_per_update: usize,
    pub ppo_epochs: usize,
    pub ppo_minibatch_size: usize,
    pub evaluation_sessions: usize,
    pub evaluation_interval: u64,
    pub evaluation_seed: u64,
    pub task_seed: u64,
    pub gamma: f64,
    pub gae_lambda: f64,
    pub advantage_normalization: String,
    pub initialization_seed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskConfig {
    pub facility: String,
    pub transport: TransportKind,
    pub rate: Rate,
    pub rotation: i64,
    pub max_connections: usize,
    #[serde(default)]
    pub aligned_warmup_updates: u64,
    pub route_turns: usize,
    pub allow_abort: bool,
    pub facility_margin: i64,
    pub route_gap: i64,
    pub row_gap: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RewardConfig {
    pub success_bonus: f64,
    pub abort_penalty: f64,
    pub timeout_penalty: f64,
    pub blocked_rollin_penalty: f64,
    pub rejected_penalty: f64,
    pub step_penalty: f64,
    pub potential_scale: f64,
    pub failure_distance_scale: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct TaskSpec {
    connections: usize,
    rows: usize,
    crossed: bool,
    seed: u64,
}

#[derive(Clone, Debug, Serialize)]
struct Summary {
    sessions: usize,
    options: usize,
    blocked_before_action: usize,
    dead_ends: usize,
    route_successes: usize,
    route_success_rate: f64,
    fully_routed_sessions: usize,
    turns: usize,
    accepted: usize,
    mean_reward: f64,
    maximum_rollin_depth: usize,
    by_rollin_depth: BTreeMap<usize, Value>,
    by_task_shape: BTreeMap<String, Value>,
    by_requested_connections: BTreeMap<usize, Value>,
    seconds: f64,
}

struct RouteCollection {
    rollout: Collection,
    sessions: usize,
    fully_routed_sessions: usize,
    blocked: Vec<BlockedOption>,
    dead_ends: usize,
}

struct BlockedOption {
    depth: usize,
    requested: usize,
    crossed: bool,
}

struct TaskState {
    session: Session,
    targets: usize,
    next: usize,
    route_steps: usize,
    current: Episode,
    last_success_episode: Option<usize>,
    successful_episodes: Vec<usize>,
    crossed: bool,
    allow_abort: bool,
    stopped: bool,
}

struct TaskBuilder {
    config: TaskConfig,
    domain: DomainConfig,
    native: Limits,
    catalogs: Arc<Catalogs>,
    width: i64,
    height: i64,
}

impl RewardConfig {
    fn validate(&self) -> Result<()> {
        ensure!(
            self.success_bonus.is_finite()
                && self.success_bonus > 0.
                && self.abort_penalty.is_finite()
                && self.abort_penalty <= 0.
                && self.timeout_penalty.is_finite()
                && self.timeout_penalty <= 0.
                && self.blocked_rollin_penalty.is_finite()
                && self.blocked_rollin_penalty <= 0.
                && self.rejected_penalty.is_finite()
                && self.rejected_penalty <= 0.
                && self.step_penalty.is_finite()
                && self.step_penalty <= 0.
                && self.potential_scale.is_finite()
                && self.potential_scale >= 0.
                && self.failure_distance_scale.is_finite()
                && self.failure_distance_scale <= 0.,
            "aic-learning.constructive-route-reward: invalid coefficients"
        );
        Ok(())
    }
}

impl TaskBuilder {
    fn new(config: &Config, catalogs: Arc<Catalogs>) -> Result<Self> {
        let facility = catalogs
            .facilities
            .facility(&config.task.facility)
            .with_context(|| {
                format!(
                    "aic-learning.constructive-route-task: missing facility {}",
                    config.task.facility
                )
            })?;
        ensure!(
            facility.allowed_rotations.contains(&config.task.rotation),
            "aic-learning.constructive-route-task: configured rotation is unavailable"
        );
        for direction in [FacilityPortDirection::Output, FacilityPortDirection::Input] {
            let count = facility
                .ports
                .iter()
                .filter(|port| {
                    port.direction == direction && port.transport == config.task.transport
                })
                .count();
            ensure!(
                count == 1,
                "aic-learning.constructive-route-task: the first slice requires exactly one compatible {direction:?} port, found {count}"
            );
        }
        let (width, height, _) = rotate(facility, config.task.rotation, Point { x: 0, y: 0 });
        let bounds = config.domain.placement;
        let required_width = config
            .task
            .facility_margin
            .checked_mul(2)
            .and_then(|margin| margin.checked_add(width.checked_mul(2)?))
            .and_then(|value| value.checked_add(config.task.route_gap));
        let required_height = config
            .task
            .facility_margin
            .checked_mul(2)
            .and_then(|margin| {
                margin.checked_add(height.checked_mul(config.task.max_connections as i64)?)
            })
            .and_then(|value| {
                value.checked_add(
                    config
                        .task
                        .row_gap
                        .checked_mul(config.task.max_connections.saturating_sub(1) as i64)?,
                )
            });
        ensure!(
            required_width.is_some_and(|value| value <= bounds.width())
                && required_height.is_some_and(|value| value <= bounds.height()),
            "aic-learning.constructive-route-task: configured facility rows do not fit the placement canvas"
        );
        Ok(Self {
            config: config.task.clone(),
            domain: config.domain.clone(),
            native: config.native,
            catalogs,
            width,
            height,
        })
    }

    fn build(&self, spec: &TaskSpec) -> Result<Session> {
        ensure!(
            (1..=self.config.max_connections).contains(&spec.connections)
                && (spec.connections..=self.config.max_connections).contains(&spec.rows),
            "aic-learning.constructive-route-task: invalid connection or row count"
        );
        let blocks = (0..spec.connections)
            .flat_map(|index| {
                [
                    Block {
                        id: format!("source-{index:03}"),
                        facility: self.config.facility.clone(),
                    },
                    Block {
                        id: format!("target-{index:03}"),
                        facility: self.config.facility.clone(),
                    },
                ]
            })
            .collect::<Vec<_>>();
        let connections = (0..spec.connections)
            .map(|index| Connection {
                id: format!("route-{index:03}"),
                source: format!("source-{index:03}"),
                target: format!("target-{index:03}"),
                commodity: format!("route-item-{index:03}"),
                transport: self.config.transport,
                rate: self.config.rate,
            })
            .collect();
        let problem = Problem {
            blocks,
            terminals: vec![],
            connections,
        };
        let mut session = Session::new(problem, self.catalogs.clone(), self.native)
            .map_err(anyhow::Error::new)?;
        let bounds = self.domain.placement;
        let source_x = bounds.min_x + self.config.facility_margin;
        let target_x = source_x + self.width + self.config.route_gap;
        let row_stride = self.height + self.config.row_gap;
        for index in 0..spec.connections {
            let source_row = if spec.seed & 1 == 0 {
                index
            } else {
                spec.rows - 1 - index
            };
            let target_row = if spec.crossed {
                spec.rows - 1 - source_row
            } else {
                source_row
            };
            for (id, x, row) in [
                (format!("source-{index:03}"), source_x, source_row),
                (format!("target-{index:03}"), target_x, target_row),
            ] {
                let y =
                    bounds.min_y + self.config.facility_margin + i64::try_from(row)? * row_stride;
                let step = session.step(Command::Place {
                    block: id.clone(),
                    position: Point { x, y },
                    rotation: self.config.rotation,
                });
                ensure!(
                    step.accepted,
                    "aic-learning.constructive-route-setup: placement {id} failed: {:?}",
                    step.diagnostic
                );
            }
        }
        Ok(session)
    }
}

pub fn run(
    config_path: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
) -> Result<Value> {
    let config: Config = read_json(config_path)?;
    validate_config(&config)?;
    ensure!(
        !output.exists(),
        "aic-learning.constructive-route-output: {} already exists",
        output.display()
    );
    let catalogs = load_catalogs(catalog_root)?;
    let builder = TaskBuilder::new(&config, catalogs)?;
    let evaluation_specs = evaluation_specs(&config);
    for spec in &evaluation_specs {
        builder.build(spec)?;
    }

    tch::set_num_threads(1);
    tch::set_num_interop_threads(1);
    deterministic()?;
    let (parameter_order, mut learner) = Learner::cold(
        &config.model,
        &config.optimizer,
        config.initialization_seed,
        device,
    )?;
    fs::create_dir_all(output)?;
    write_json(&output.join("config.json"), &config)?;
    write_json(&output.join("evaluation-tasks.json"), &evaluation_specs)?;
    write_json(&output.join("parameter-order.json"), &parameter_order)?;
    let mut metrics = BufWriter::new(File::create(output.join("metrics.jsonl"))?);

    let initial_greedy = evaluate(
        &builder,
        &evaluation_specs,
        &config,
        &mut learner,
        Sampling::GreedyEvaluation,
    )?;
    let initial_stochastic = evaluate(
        &builder,
        &evaluation_specs,
        &config,
        &mut learner,
        Sampling::StochasticEvaluation {
            seed: config.evaluation_seed,
        },
    )?;
    let mut final_greedy = summary(&initial_greedy);
    let mut final_stochastic = summary(&initial_stochastic);
    write_row(
        &mut metrics,
        &json!({
            "update":0,
            "evaluation_greedy":final_greedy,
            "evaluation_stochastic":final_stochastic,
        }),
    )?;

    let mut total_sessions = 0usize;
    let mut total_options = 0usize;
    let mut total_turns = 0usize;
    let mut replay = ReplayBuffer::new(config.replay.clone());
    for update in 1..=config.updates {
        let specs = training_specs(&config, update);
        let sessions = specs
            .iter()
            .map(|spec| Ok((builder.build(spec)?, spec.clone())))
            .collect::<Result<Vec<_>>>()?;
        let collected = collect(
            sessions,
            &builder.domain,
            &config.reward,
            config.task.route_turns,
            config.task.allow_abort,
            config.gamma,
            &mut learner,
            Sampling::Training,
        )?;
        let credit = rollout::gae(&collected.rollout, config.gamma, config.gae_lambda)?;
        let samples = rollout::ppo_samples(&collected.rollout, &credit)?;
        let mut epochs = Vec::with_capacity(config.ppo_epochs);
        for _ in 0..config.ppo_epochs {
            let draw = replay.draw(update);
            let report = {
                let replay_samples = replay.samples(&draw);
                learner.update_epoch(
                    &samples,
                    &replay_samples,
                    &config.advantage_normalization,
                    config.ppo_minibatch_size,
                )?
            };
            replay.observe_utility(&report.replay_utility);
            epochs.push(report);
        }
        // Causal boundary: successes from this collection become eligible only
        // after every learner epoch for the update has finished.
        let admissions = replay.admit(&collected.rollout.episodes, config.gamma, update)?;
        total_sessions += collected.sessions;
        total_options += collected.rollout.episodes.len() + collected.blocked.len();
        total_turns += collected.rollout.turns;
        let mut row = json!({
            "update":update,
            "available_connections":available_connections(&config, update),
            "training":summary(&collected),
            "ppo":epochs,
            "replay":replay.report(admissions),
        });
        if update % config.evaluation_interval == 0 || update == config.updates {
            let greedy = evaluate(
                &builder,
                &evaluation_specs,
                &config,
                &mut learner,
                Sampling::GreedyEvaluation,
            )?;
            let stochastic = evaluate(
                &builder,
                &evaluation_specs,
                &config,
                &mut learner,
                Sampling::StochasticEvaluation {
                    seed: config.evaluation_seed,
                },
            )?;
            final_greedy = summary(&greedy);
            final_stochastic = summary(&stochastic);
            row["evaluation_greedy"] = serde_json::to_value(&final_greedy)?;
            row["evaluation_stochastic"] = serde_json::to_value(&final_stochastic)?;
        }
        write_row(&mut metrics, &row)?;
        metrics.flush()?;
    }
    metrics.flush()?;

    let model_path = output.join("learner.safetensors");
    let snapshot = learner.snapshot()?;
    Tensor::write_safetensors(&snapshot.iter().collect::<Vec<_>>(), &model_path)?;
    let evaluation_path = output.join("evaluation-tasks.json");
    let report = json!({
        "schema":"aic-constructive-route-training-report-v1",
        "status":"completed",
        "updates":config.updates,
        "optimizer_steps":learner.completed_steps,
        "training_sessions":total_sessions,
        "training_options":total_options,
        "training_turns":total_turns,
        "final_evaluation_greedy":final_greedy,
        "final_evaluation_stochastic":final_stochastic,
        "parameter_count":learner.model.parameters.iter().map(|(_, value)| value.numel() as u64).sum::<u64>(),
        "device":device_name(device)?,
        "determinism":"strict",
        "config_sha256":sha256(config_path)?,
        "evaluation_tasks_sha256":sha256(&evaluation_path)?,
        "catalog_sha256":builder.catalogs.identity,
        "learner_sha256":sha256(&model_path)?,
        "files":["config.json","evaluation-tasks.json","parameter-order.json","metrics.jsonl","learner.safetensors","report.json"],
    });
    write_json(&output.join("report.json"), &report)?;
    Ok(report)
}

fn validate_config(config: &Config) -> Result<()> {
    ensure!(
        config.schema == "aic-constructive-route-training-v1"
            && (1..=1024).contains(&config.updates)
            && (2..=4096).contains(&config.sessions_per_update)
            && (1..=16).contains(&config.ppo_epochs)
            && (1..=262_144).contains(&config.ppo_minibatch_size)
            && (2..=4096).contains(&config.evaluation_sessions)
            && (1..=config.updates).contains(&config.evaluation_interval)
            && (1..=8).contains(&config.task.max_connections)
            && config.task.aligned_warmup_updates < config.updates
            && (2..=4096).contains(&config.task.route_turns)
            && config.task.facility_margin >= 1
            && config.task.route_gap >= 1
            && config.task.row_gap >= 1
            && config.task.rate.numerator > 0
            && config.task.rate.denominator > 0
            && (0. ..=1.).contains(&config.gamma)
            && (0. ..=1.).contains(&config.gae_lambda)
            && matches!(
                config.advantage_normalization.as_str(),
                "center" | "standardize"
            ),
        "aic-learning.constructive-route-config: invalid configuration"
    );
    config.model.validate()?;
    config.optimizer.validate()?;
    config.reward.validate()?;
    config
        .replay
        .validate(config.optimizer.imitation_coefficient)?;
    Ok(())
}

fn available_connections(config: &Config, update: u64) -> usize {
    if config.task.max_connections == 1 || config.updates == 1 {
        return 1;
    }
    1 + usize::try_from(
        ((update - 1) * config.task.max_connections as u64 / (config.updates - 1))
            .min(config.task.max_connections as u64 - 1),
    )
    .expect("bounded curriculum depth")
}

fn training_specs(config: &Config, update: u64) -> Vec<TaskSpec> {
    let available = available_connections(config, update);
    let shapes = if update > config.task.aligned_warmup_updates {
        2
    } else {
        1
    };
    (0..config.sessions_per_update)
        .map(|index| {
            let combination = (index + update as usize - 1) % (available * shapes);
            let connections = 1 + combination % available;
            let crossed = combination / available == 1;
            TaskSpec {
                connections,
                rows: if crossed && available == 1 {
                    config.task.max_connections
                } else {
                    available
                },
                crossed,
                seed: config.task_seed ^ update.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ index as u64,
            }
        })
        .collect()
}

fn evaluation_specs(config: &Config) -> Vec<TaskSpec> {
    (0..config.evaluation_sessions)
        .map(|index| TaskSpec {
            connections: 1 + index % config.task.max_connections,
            rows: config.task.max_connections,
            crossed: (index / config.task.max_connections).is_multiple_of(2),
            seed: config.evaluation_seed ^ index as u64,
        })
        .collect()
}

fn evaluate(
    builder: &TaskBuilder,
    specs: &[TaskSpec],
    config: &Config,
    learner: &mut Learner,
    sampling: Sampling,
) -> Result<RouteCollection> {
    let sessions = specs
        .iter()
        .map(|spec| Ok((builder.build(spec)?, spec.clone())))
        .collect::<Result<Vec<_>>>()?;
    collect(
        sessions,
        &builder.domain,
        &config.reward,
        config.task.route_turns,
        config.task.allow_abort,
        config.gamma,
        learner,
        sampling,
    )
}

fn collect(
    sessions: Vec<(Session, TaskSpec)>,
    domain: &DomainConfig,
    reward: &RewardConfig,
    route_turns: usize,
    allow_abort: bool,
    gamma: f64,
    learner: &mut Learner,
    sampling: Sampling,
) -> Result<RouteCollection> {
    ensure!(
        !sessions.is_empty(),
        "aic-learning.constructive-route-collection: no sessions"
    );
    let started = Instant::now();
    let count = sessions.len();
    let mut states = sessions
        .into_iter()
        .map(|(session, spec)| {
            let targets = session.problem().connections.len();
            TaskState {
                session,
                targets,
                next: 0,
                route_steps: 0,
                current: route_episode(0, targets, spec.crossed),
                last_success_episode: None,
                successful_episodes: Vec::new(),
                crossed: spec.crossed,
                allow_abort,
                stopped: false,
            }
        })
        .collect::<Vec<_>>();
    let mut episodes: Vec<Episode> = Vec::new();
    let mut blocked = Vec::new();
    let mut dead_ends = 0;
    let mut evaluation_sampler = match sampling {
        Sampling::StochasticEvaluation { seed } => Some(Generator::new(seed)?),
        _ => None,
    };
    loop {
        let active = states
            .iter()
            .enumerate()
            .filter_map(|(owner, state)| (!state.stopped).then_some(owner))
            .collect::<Vec<_>>();
        if active.is_empty() {
            break;
        }
        let mut owners = Vec::new();
        let mut observations = Vec::new();
        for owner in active {
            match states[owner].session.route_learning_observation_with_abort(
                domain,
                states[owner].next,
                states[owner].allow_abort,
            ) {
                Ok(observation) => {
                    owners.push(owner);
                    observations.push(observation);
                }
                Err(diagnostic)
                    if diagnostic.code == "constructive.empty-route-learning-domain" =>
                {
                    let state = &mut states[owner];
                    if state.current.transitions.is_empty() {
                        blocked.push(BlockedOption {
                            depth: state.next,
                            requested: state.targets,
                            crossed: state.crossed,
                        });
                        if let Some(episode) = state.last_success_episode {
                            episodes[episode]
                                .transitions
                                .last_mut()
                                .expect("successful route option has a terminal transition")
                                .reward += reward.blocked_rollin_penalty;
                            episodes[episode].route_blocked_after = true;
                        }
                    } else {
                        let observation = state.session.observe();
                        let current_potential =
                            potential(&observation, domain, reward.potential_scale);
                        let remaining = normalized_distance(&observation, domain).unwrap_or(1.);
                        let last = state
                            .current
                            .transitions
                            .last_mut()
                            .expect("nonempty dead-end route has a final transition");
                        last.terminal = true;
                        last.reward +=
                            dead_end_adjustment(current_potential, remaining, reward, gamma);
                        state.current.dead_end = true;
                        state.current.fully_validated = observation.fully_validated;
                        state.current.completed_connections = observation.completed;
                        episodes.push(std::mem::take(&mut state.current));
                        dead_ends += 1;
                    }
                    finalize_route_outcomes(&mut episodes, state);
                    state.stopped = true;
                }
                Err(diagnostic) => return Err(diagnostic.into()),
            }
        }
        if owners.is_empty() {
            continue;
        }
        let rows = observations.iter().collect::<Vec<_>>();
        let choices = match sampling {
            Sampling::Training => learner.sample(&rows)?,
            Sampling::GreedyEvaluation => learner.greedy(&rows)?,
            Sampling::StochasticEvaluation { .. } => policy::sample(
                &learner.model,
                &rows,
                evaluation_sampler
                    .as_mut()
                    .expect("stochastic route sampler initialized"),
                learner.device,
            )?,
        };
        for ((owner, observation), choice) in owners.into_iter().zip(observations).zip(choices) {
            let state = &mut states[owner];
            let before = state.session.observe();
            let before_potential = potential(&before, domain, reward.potential_scale);
            let command = choice.command.command.clone();
            let step = state.session.step_selected(choice.command.clone());
            state.route_steps += 1;
            let after = state.session.observe();
            let succeeded = after.routes[state.next].is_some();
            let aborted = step.accepted && matches!(command, Command::AbortRoute);
            let exhausted = state.route_steps >= route_turns;
            let terminal = succeeded || aborted || exhausted || step.terminal;
            let after_potential = potential(&after, domain, reward.potential_scale);
            let mut step_reward =
                shaping(before_potential, after_potential, terminal, gamma) + reward.step_penalty;
            if !step.accepted {
                step_reward += reward.rejected_penalty;
            }
            if succeeded {
                step_reward += reward.success_bonus;
            } else if terminal {
                step_reward += if aborted {
                    reward.abort_penalty
                } else {
                    reward.timeout_penalty
                };
                step_reward += reward.failure_distance_scale
                    * normalized_distance(
                        if after.active.is_some() {
                            &after
                        } else {
                            &before
                        },
                        domain,
                    )
                    .unwrap_or(1.);
            }
            ensure!(
                step_reward.is_finite(),
                "aic-learning.constructive-route-collection: nonfinite reward"
            );
            state.current.transitions.push(Transition {
                observation,
                choice,
                reward: step_reward,
                terminal,
                accepted: step.accepted,
                progress_delta: step.progress_delta,
                fully_validated: step.fully_validated,
            });
            if terminal {
                state.current.route_succeeded = succeeded;
                state.current.fully_validated = after.fully_validated;
                state.current.completed_connections = after.completed;
                let episode = episodes.len();
                episodes.push(std::mem::take(&mut state.current));
                if succeeded && state.next + 1 < state.targets && !state.session.done() {
                    state.last_success_episode = Some(episode);
                    state.successful_episodes.push(episode);
                    state.next += 1;
                    state.route_steps = 0;
                    state.current = route_episode(state.next, state.targets, state.crossed);
                } else {
                    if succeeded {
                        state.successful_episodes.push(episode);
                    }
                    finalize_route_outcomes(&mut episodes, state);
                    state.stopped = true;
                }
            }
        }
    }
    ensure!(
        episodes.iter().all(|episode| {
            !episode.transitions.is_empty()
                && episode.transitions.last().is_some_and(|step| step.terminal)
        }),
        "aic-learning.constructive-route-collection: incomplete option accounting"
    );
    let turns = episodes
        .iter()
        .map(|episode| episode.transitions.len())
        .sum();
    let accepted = episodes
        .iter()
        .flat_map(|episode| &episode.transitions)
        .filter(|transition| transition.accepted)
        .count();
    let fully_routed_sessions = states
        .iter()
        .filter(|state| state.session.observe().fully_validated)
        .count();
    Ok(RouteCollection {
        rollout: Collection {
            episodes,
            turns,
            accepted,
            completed: fully_routed_sessions,
            seconds: started.elapsed().as_secs_f64(),
        },
        sessions: count,
        fully_routed_sessions,
        blocked,
        dead_ends,
    })
}

fn finalize_route_outcomes(episodes: &mut [Episode], state: &TaskState) {
    let observation = state.session.observe();
    for &episode in &state.successful_episodes {
        episodes[episode].route_observed_depth = observation.completed;
        episodes[episode].route_full_session = observation.fully_validated;
    }
}

fn route_episode(depth: usize, total: usize, crossed: bool) -> Episode {
    Episode {
        rollin_depth: depth,
        route_crossed: crossed,
        total_connections: total,
        ..Episode::default()
    }
}

fn potential(observation: &NativeObservation, domain: &DomainConfig, potential_scale: f64) -> f64 {
    -potential_scale * normalized_distance(observation, domain).unwrap_or(0.)
}

fn normalized_distance(observation: &NativeObservation, domain: &DomainConfig) -> Option<f64> {
    let Some(active) = &observation.active else {
        return None;
    };
    let endpoint = active
        .cells
        .last()
        .expect("active route always contains its source");
    let distance = endpoint.x.abs_diff(active.target.x) + endpoint.y.abs_diff(active.target.y);
    let span = domain
        .placement
        .width()
        .max(domain.placement.height())
        .max(1) as f64;
    Some(distance as f64 / span)
}

fn shaping(current: f64, next: f64, terminal: bool, gamma: f64) -> f64 {
    gamma * if terminal { 0. } else { next } - current
}

fn dead_end_adjustment(
    current_potential: f64,
    remaining_distance: f64,
    reward: &RewardConfig,
    gamma: f64,
) -> f64 {
    -gamma * current_potential
        + reward.timeout_penalty
        + reward.failure_distance_scale * remaining_distance
}

fn summary(collection: &RouteCollection) -> Summary {
    let blocked_before_action = collection.blocked.len();
    let options = collection.rollout.episodes.len() + blocked_before_action;
    let route_successes = collection
        .rollout
        .episodes
        .iter()
        .filter(|episode| episode.route_succeeded)
        .count();
    let mut depths = BTreeMap::<usize, (usize, usize)>::new();
    let mut shapes = BTreeMap::<String, (usize, usize)>::new();
    let mut requested = BTreeMap::<usize, (usize, usize)>::new();
    for episode in &collection.rollout.episodes {
        let succeeded = usize::from(episode.route_succeeded);
        for entry in [
            depths.entry(episode.rollin_depth).or_default(),
            requested.entry(episode.total_connections).or_default(),
        ] {
            entry.0 += 1;
            entry.1 += succeeded;
        }
        let shape = if episode.route_crossed {
            "crossed"
        } else {
            "aligned"
        };
        let entry = shapes.entry(shape.into()).or_default();
        entry.0 += 1;
        entry.1 += succeeded;
    }
    for blocked in &collection.blocked {
        depths.entry(blocked.depth).or_default().0 += 1;
        requested.entry(blocked.requested).or_default().0 += 1;
        shapes
            .entry(if blocked.crossed {
                "crossed".into()
            } else {
                "aligned".into()
            })
            .or_default()
            .0 += 1;
    }
    Summary {
        sessions: collection.sessions,
        options,
        blocked_before_action,
        dead_ends: collection.dead_ends,
        route_successes,
        route_success_rate: route_successes as f64 / options as f64,
        fully_routed_sessions: collection.fully_routed_sessions,
        turns: collection.rollout.turns,
        accepted: collection.rollout.accepted,
        mean_reward: collection
            .rollout
            .episodes
            .iter()
            .map(|episode| {
                episode
                    .transitions
                    .iter()
                    .map(|transition| transition.reward)
                    .sum::<f64>()
            })
            .sum::<f64>()
            / options as f64,
        maximum_rollin_depth: collection
            .rollout
            .episodes
            .iter()
            .map(|episode| episode.rollin_depth)
            .chain(collection.blocked.iter().map(|blocked| blocked.depth))
            .max()
            .unwrap_or(0),
        by_rollin_depth: grouped_summary(depths),
        by_task_shape: grouped_summary(shapes),
        by_requested_connections: grouped_summary(requested),
        seconds: collection.rollout.seconds,
    }
}

fn grouped_summary<K: Ord>(groups: BTreeMap<K, (usize, usize)>) -> BTreeMap<K, Value> {
    groups
        .into_iter()
        .map(|(key, (attempts, successes))| {
            (
                key,
                json!({
                    "attempts":attempts,
                    "successes":successes,
                    "success_rate":successes as f64 / attempts as f64,
                }),
            )
        })
        .collect()
}

fn write_row(writer: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curriculum_opens_deeper_self_roll_in_gradually() {
        let mut config: Config = serde_json::from_value(json!({
            "schema":"aic-constructive-route-training-v1",
            "model":{"schema":"aic-constructive-attention-model-v2","width":16,"message_rounds":1},
            "optimizer":{"adam_backend":"foreach","learning_rate":0.001,"beta1":0.9,"beta2":0.999,"epsilon":1e-8,"clip_epsilon":0.2,"entropy_coefficient":0.01,"imitation_coefficient":0.0,"gradient_clip":0.5},
            "domain":{"placement":{"minX":0,"minY":0,"maxX":31,"maxY":31},"extension_lengths":[1,2,4]},
            "native":{"max_turns":128,"max_extension":4},
            "task":{"facility":"fixture","transport":"pipe","rate":{"numerator":1,"denominator":2},"rotation":0,"max_connections":4,"aligned_warmup_updates":1,"route_turns":16,"allow_abort":false,"facility_margin":2,"route_gap":4,"row_gap":2},
            "reward":{"success_bonus":1.0,"abort_penalty":-1.0,"timeout_penalty":-0.25,"blocked_rollin_penalty":-1.0,"rejected_penalty":-0.05,"step_penalty":0.0,"potential_scale":1.0,"failure_distance_scale":-1.0},
            "updates":8,"sessions_per_update":8,"ppo_epochs":1,"ppo_minibatch_size":16,"evaluation_sessions":8,"evaluation_interval":1,"evaluation_seed":7,"task_seed":9,"gamma":0.99,"gae_lambda":0.95,"advantage_normalization":"standardize","initialization_seed":11
        })).unwrap();
        assert_eq!(available_connections(&config, 1), 1);
        assert_eq!(available_connections(&config, 3), 2);
        assert_eq!(available_connections(&config, 5), 3);
        assert_eq!(available_connections(&config, 7), 4);
        assert!(
            training_specs(&config, 1)
                .iter()
                .all(|spec| spec.connections == 1 && spec.rows == 1 && !spec.crossed)
        );
        let offset_specs = training_specs(&config, 2);
        assert!(offset_specs.iter().any(|spec| {
            spec.connections == 1 && spec.rows == config.task.max_connections && spec.crossed
        }));
        let final_specs = training_specs(&config, 7);
        assert_eq!(
            final_specs
                .iter()
                .map(|spec| (spec.connections, spec.crossed))
                .collect::<std::collections::BTreeSet<_>>(),
            (1..=4)
                .flat_map(|connections| { [(connections, false), (connections, true)] })
                .collect()
        );
        config.task.max_connections = 1;
        assert_eq!(available_connections(&config, 7), 1);
    }

    #[test]
    fn terminal_shaping_closes_the_discounted_ledger() {
        let gamma = 0.99;
        let potentials = [0., -0.5, -0.25];
        let rewards = [
            shaping(potentials[0], potentials[1], false, gamma),
            shaping(potentials[1], potentials[2], false, gamma),
            shaping(potentials[2], -0.1, true, gamma),
        ];
        let discounted = rewards[0] + gamma * rewards[1] + gamma * gamma * rewards[2];
        assert!(discounted.abs() < 1e-12);
    }

    #[test]
    fn active_dead_end_closes_shaping_and_keeps_failure_penalty() {
        let gamma = 0.99;
        let reward = RewardConfig {
            success_bonus: 1.,
            abort_penalty: -1.,
            timeout_penalty: -0.25,
            blocked_rollin_penalty: -1.,
            rejected_penalty: -0.05,
            step_penalty: 0.,
            potential_scale: 1.,
            failure_distance_scale: -1.,
        };
        let potentials = [0., -0.5, -0.25];
        let first = shaping(potentials[0], potentials[1], false, gamma);
        let mut second = shaping(potentials[1], potentials[2], false, gamma);
        second += dead_end_adjustment(potentials[2], 0.25, &reward, gamma);
        let discounted = first + gamma * second;
        let expected_penalty = gamma * (reward.timeout_penalty - 0.25);
        assert!((discounted - expected_penalty).abs() < 1e-12);
    }

    #[test]
    fn later_route_observes_policy_committed_occupancy() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config: Config =
            read_json(&root.join("docs/benchmarks/direct-constructive-route-options/config.json"))?;
        let catalogs = load_catalogs(&root.join("data/game/normalized"))?;
        let builder = TaskBuilder::new(&config, catalogs)?;
        let mut session = builder.build(&TaskSpec {
            connections: 2,
            rows: 2,
            crossed: false,
            seed: 0,
        })?;
        for command in [
            Command::BeginRoute {
                connection: "route-000".into(),
                source_port: Some("output-pipe-3".into()),
                target_port: Some("input-pipe-3".into()),
            },
            Command::Extend {
                direction: aic_pilot_native::constructive::Direction::East,
                length: 4,
            },
            Command::Connect,
        ] {
            let step = session.step(command);
            ensure!(
                step.accepted,
                "authored route failed: {:?}",
                step.diagnostic
            );
        }
        let observation = session.route_learning_observation(&builder.domain, 1)?;
        let committed_pipe_cells = observation
            .input
            .spatial_features
            .iter()
            .filter(|features| features[4] == 1.)
            .count();
        assert_eq!(committed_pipe_cells, 5);
        Ok(())
    }

    #[test]
    fn authored_offset_route_is_reachable_with_two_runs() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config: Config =
            read_json(&root.join("docs/benchmarks/direct-constructive-route-options/config.json"))?;
        let catalogs = load_catalogs(&root.join("data/game/normalized"))?;
        let builder = TaskBuilder::new(&config, catalogs)?;
        let mut session = builder.build(&TaskSpec {
            connections: 1,
            rows: 4,
            crossed: true,
            seed: 0,
        })?;
        for command in [
            Command::BeginRoute {
                connection: "route-000".into(),
                source_port: Some("output-pipe-3".into()),
                target_port: Some("input-pipe-3".into()),
            },
            Command::Extend {
                direction: aic_pilot_native::constructive::Direction::South,
                length: 15,
            },
            Command::Extend {
                direction: aic_pilot_native::constructive::Direction::East,
                length: 4,
            },
            Command::Connect,
        ] {
            let step = session.step(command);
            ensure!(
                step.accepted,
                "authored offset route failed: {:?}",
                step.diagnostic
            );
        }
        assert!(session.observe().fully_validated);
        Ok(())
    }
}
