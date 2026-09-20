//! Frozen-policy probe at the first crossed self-roll-in frontier.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    time::Instant,
};

use aic_pilot_native::constructive::{Command, Session};
use aic_torch::{
    Generator, deterministic,
    tch::{self, Device, Tensor},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    constructive::{Model, policy},
    input::{Tensors, sha256},
};

use super::{Config, TaskBuilder, TaskSpec, read_json, write_json};

const SCHEMA: &str = "aic-constructive-route-frontier-probe-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProbeConfig {
    schema: String,
    prefix_target: usize,
    prefix_discovery_attempts: usize,
    discovery_batch_size: usize,
    attempts_per_prefix_per_temperature: usize,
    temperatures: Vec<f64>,
    seed: u64,
}

#[derive(Clone, Debug, Serialize)]
struct Prefix {
    id: usize,
    digest: String,
    task: TaskSpec,
    commands: Vec<Command>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Outcome {
    Success,
    BlockedStart,
    DeadEnd,
    Aborted,
    RouteBudget,
    SessionTerminal,
}

struct AttemptState {
    session: Session,
    task: TaskSpec,
    prefix: Option<usize>,
    commands: Vec<Command>,
    accepted: usize,
    entropy: f64,
    factors: usize,
    outcome: Option<Outcome>,
}

#[derive(Debug)]
struct AttemptResult {
    task: TaskSpec,
    prefix: Option<usize>,
    commands: Vec<Command>,
    accepted: usize,
    entropy: f64,
    factors: usize,
    outcome: Outcome,
}

#[derive(Clone, Debug, Serialize)]
struct CellReport {
    temperature: f64,
    attempts: usize,
    actionable_prefixes: usize,
    actionable_attempts: usize,
    successes: usize,
    unique_successes: usize,
    blocked_starts: usize,
    dead_ends: usize,
    aborted: usize,
    route_budget_failures: usize,
    session_terminals: usize,
    accepted_commands: usize,
    mean_commands: f64,
    mean_factor_entropy: f64,
    success_rate: f64,
    wilson_95_low: f64,
    wilson_95_high: f64,
    actionable_success_rate: f64,
    actionable_wilson_95_low: f64,
    actionable_wilson_95_high: f64,
    by_prefix: BTreeMap<usize, Value>,
    seconds: f64,
}

pub fn probe_frontier(
    training_output: &Path,
    probe_config_path: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
) -> Result<Value> {
    ensure!(
        training_output.is_dir(),
        "aic-learning.constructive-route-frontier-input: missing {}",
        training_output.display()
    );
    ensure!(
        !output.exists(),
        "aic-learning.constructive-route-frontier-output: {} already exists",
        output.display()
    );
    let training_config: Config = read_json(&training_output.join("config.json"))?;
    super::validate_config(&training_config)?;
    let probe: ProbeConfig = read_json(probe_config_path)?;
    validate_probe(&probe)?;
    let training_report: Value = read_json(&training_output.join("report.json"))?;
    let parameter_order: Vec<String> = read_json(&training_output.join("parameter-order.json"))?;
    let checkpoint = training_output.join("learner.safetensors");
    let checkpoint_sha256 = sha256(&checkpoint)?;
    let catalogs = super::load_catalogs(catalog_root)?;
    ensure!(
        training_report["schema"] == "aic-constructive-route-training-report-v1"
            && training_report["status"] == "completed"
            && training_report["catalog_sha256"].as_str() == Some(&catalogs.identity)
            && training_report["learner_sha256"].as_str() == Some(checkpoint_sha256.as_str()),
        "aic-learning.constructive-route-frontier-identity: training report, catalogs or checkpoint do not match"
    );

    tch::set_num_threads(1);
    tch::set_num_interop_threads(1);
    deterministic()?;
    let values: Tensors = Tensor::read_safetensors(&checkpoint)?.into_iter().collect();
    let model = Model::load(&training_config.model, &parameter_order, &values, device)?;
    let builder = TaskBuilder::new(&training_config, catalogs)?;

    let started = Instant::now();
    let (prefixes, discovery_attempts) =
        discover_prefixes(&builder, &training_config, &probe, &model, device)?;
    let mut cells = Vec::with_capacity(probe.temperatures.len());
    for (temperature_index, &temperature) in probe.temperatures.iter().enumerate() {
        cells.push(probe_temperature(
            &builder,
            &training_config,
            &probe,
            &prefixes,
            &model,
            device,
            temperature,
            temperature_index,
        )?);
    }
    let total_successes: usize = cells.iter().map(|cell| cell.successes).sum();
    let status = if prefixes.len() < 4 {
        "insufficient_prefixes"
    } else if total_successes > 0 {
        "success_observed"
    } else {
        "no_success"
    };

    fs::create_dir_all(output)?;
    write_json(&output.join("config.json"), &probe)?;
    write_json(&output.join("prefixes.json"), &prefixes)?;
    let report = json!({
        "schema":"aic-constructive-route-frontier-report-v1",
        "status":status,
        "question":"Can a frozen route policy continue from its own crossed depth-zero prefix when only sampling temperature changes?",
        "training_updates":0,
        "prefixes_discovered":prefixes.len(),
        "prefix_discovery_attempts":discovery_attempts,
        "cells":cells,
        "total_depth_one_successes":total_successes,
        "device":format!("{device:?}"),
        "seconds":started.elapsed().as_secs_f64(),
        "source":{
            "training_output":training_output,
            "checkpoint_sha256":checkpoint_sha256,
            "training_report_sha256":sha256(&training_output.join("report.json"))?,
            "training_config_sha256":sha256(&training_output.join("config.json"))?,
            "probe_config_sha256":sha256(probe_config_path)?,
            "catalog_sha256":builder.catalogs.identity,
        },
        "files":["config.json","prefixes.json","report.json","report.html","identity.json"],
    });
    write_json(&output.join("report.json"), &report)?;
    fs::write(output.join("report.html"), render_html(&report, &cells))?;
    let identity = json!({
        "schema":"aic-constructive-route-frontier-identity-v1",
        "config_sha256":sha256(&output.join("config.json"))?,
        "prefixes_sha256":sha256(&output.join("prefixes.json"))?,
        "report_sha256":sha256(&output.join("report.json"))?,
        "html_sha256":sha256(&output.join("report.html"))?,
    });
    write_json(&output.join("identity.json"), &identity)?;
    Ok(json!({
        "status":status,
        "prefixes_discovered":prefixes.len(),
        "total_depth_one_successes":total_successes,
        "report":output.join("report.json"),
        "report_sha256":identity["report_sha256"],
    }))
}

fn validate_probe(config: &ProbeConfig) -> Result<()> {
    ensure!(
        config.schema == SCHEMA
            && (1..=64).contains(&config.prefix_target)
            && (config.prefix_target..=65_536).contains(&config.prefix_discovery_attempts)
            && (1..=4096).contains(&config.discovery_batch_size)
            && (1..=4096).contains(&config.attempts_per_prefix_per_temperature)
            && (1..=16).contains(&config.temperatures.len())
            && config
                .temperatures
                .iter()
                .all(|temperature| temperature.is_finite() && (0.05..=20.).contains(temperature)),
        "aic-learning.constructive-route-frontier-config: invalid configuration"
    );
    let bits = config
        .temperatures
        .iter()
        .map(|temperature| temperature.to_bits())
        .collect::<BTreeSet<_>>();
    ensure!(
        bits.len() == config.temperatures.len(),
        "aic-learning.constructive-route-frontier-config: duplicate temperature"
    );
    Ok(())
}

fn discover_prefixes(
    builder: &TaskBuilder,
    training: &Config,
    probe: &ProbeConfig,
    model: &Model,
    device: Device,
) -> Result<(Vec<Prefix>, usize)> {
    let mut prefixes = Vec::new();
    let mut identities = BTreeSet::new();
    let mut attempts = 0usize;
    let mut sampler = Generator::new(probe.seed)?;
    while attempts < probe.prefix_discovery_attempts && prefixes.len() < probe.prefix_target {
        let batch = probe
            .discovery_batch_size
            .min(probe.prefix_discovery_attempts - attempts);
        let mut states = Vec::with_capacity(batch);
        for local in 0..batch {
            let attempt = attempts + local;
            let spec = TaskSpec {
                connections: 2,
                rows: training.task.max_connections,
                crossed: true,
                seed: probe.seed ^ attempt as u64,
            };
            states.push(AttemptState {
                session: builder.build(&spec)?,
                task: spec,
                prefix: None,
                commands: Vec::new(),
                accepted: 0,
                entropy: 0.,
                factors: 0,
                outcome: None,
            });
        }
        attempts += batch;
        for result in run_option(
            states,
            0,
            &training.domain,
            training.task.route_turns,
            training.task.allow_abort,
            model,
            device,
            1.,
            &mut sampler,
        )? {
            if result.outcome != Outcome::Success {
                continue;
            }
            let encoded = serde_json::to_vec(&(result.task.seed & 1, &result.commands))?;
            let digest = hex_digest(&encoded);
            if identities.insert(digest.clone()) {
                prefixes.push(Prefix {
                    id: prefixes.len(),
                    digest,
                    task: result.task,
                    commands: result.commands,
                });
                if prefixes.len() == probe.prefix_target {
                    break;
                }
            }
        }
    }
    Ok((prefixes, attempts))
}

#[allow(clippy::too_many_arguments)]
fn probe_temperature(
    builder: &TaskBuilder,
    training: &Config,
    probe: &ProbeConfig,
    prefixes: &[Prefix],
    model: &Model,
    device: Device,
    temperature: f64,
    temperature_index: usize,
) -> Result<CellReport> {
    let started = Instant::now();
    let mut states = Vec::with_capacity(prefixes.len() * probe.attempts_per_prefix_per_temperature);
    for prefix in prefixes {
        for _ in 0..probe.attempts_per_prefix_per_temperature {
            states.push(AttemptState {
                session: reconstruct_prefix(builder, prefix)?,
                task: prefix.task.clone(),
                prefix: Some(prefix.id),
                commands: Vec::new(),
                accepted: 0,
                entropy: 0.,
                factors: 0,
                outcome: None,
            });
        }
    }
    let seed = probe
        .seed
        .checked_add(1_000_000)
        .and_then(|seed| seed.checked_add(temperature_index as u64 * 100_000))
        .context("aic-learning.constructive-route-frontier: sampler seed overflow")?;
    let mut sampler = Generator::new(seed)?;
    let results = run_option(
        states,
        1,
        &training.domain,
        training.task.route_turns,
        training.task.allow_abort,
        model,
        device,
        temperature,
        &mut sampler,
    )?;
    Ok(summarize_cell(
        temperature,
        &results,
        started.elapsed().as_secs_f64(),
    ))
}

fn reconstruct_prefix(builder: &TaskBuilder, prefix: &Prefix) -> Result<Session> {
    let mut session = builder.build(&prefix.task)?;
    for command in &prefix.commands {
        let step = session.step(command.clone());
        ensure!(
            step.accepted,
            "aic-learning.constructive-route-frontier-prefix: rejected command in {}",
            prefix.digest
        );
    }
    let observed = session.observe();
    ensure!(
        observed.completed == 1
            && observed.active.is_none()
            && observed.routes.first().is_some_and(Option::is_some)
            && observed.routes.get(1).is_some_and(Option::is_none),
        "aic-learning.constructive-route-frontier-prefix: reconstructed state mismatch for {}",
        prefix.digest
    );
    Ok(session)
}

#[allow(clippy::too_many_arguments)]
fn run_option(
    mut states: Vec<AttemptState>,
    route: usize,
    domain: &aic_pilot_native::constructive::learning::Config,
    route_turns: usize,
    allow_abort: bool,
    model: &Model,
    device: Device,
    temperature: f64,
    sampler: &mut Generator,
) -> Result<Vec<AttemptResult>> {
    loop {
        let active = states
            .iter()
            .enumerate()
            .filter_map(|(owner, state)| state.outcome.is_none().then_some(owner))
            .collect::<Vec<_>>();
        if active.is_empty() {
            break;
        }
        let mut owners = Vec::new();
        let mut observations = Vec::new();
        for owner in active {
            match states[owner].session.route_learning_observation_with_abort(
                domain,
                route,
                allow_abort,
            ) {
                Ok(observation) => {
                    owners.push(owner);
                    observations.push(observation);
                }
                Err(diagnostic)
                    if diagnostic.code == "constructive.empty-route-learning-domain" =>
                {
                    states[owner].outcome = Some(if states[owner].commands.is_empty() {
                        Outcome::BlockedStart
                    } else {
                        Outcome::DeadEnd
                    });
                }
                Err(diagnostic) => return Err(diagnostic.into()),
            }
        }
        if owners.is_empty() {
            continue;
        }
        let rows = observations.iter().collect::<Vec<_>>();
        let choices = policy::sample_temperature(model, &rows, sampler, device, temperature)?;
        for ((owner, _observation), choice) in owners.into_iter().zip(observations).zip(choices) {
            let state = &mut states[owner];
            let command = choice.command.command.clone();
            let step = state.session.step_selected(choice.command);
            state.commands.push(command.clone());
            state.accepted += usize::from(step.accepted);
            state.entropy += choice.entropy;
            state.factors += choice.factors.len();
            let observed = state.session.observe();
            let succeeded = observed.routes.get(route).is_some_and(Option::is_some);
            let aborted = step.accepted && matches!(command, Command::AbortRoute);
            state.outcome = if succeeded {
                Some(Outcome::Success)
            } else if aborted {
                Some(Outcome::Aborted)
            } else if state.commands.len() >= route_turns {
                Some(Outcome::RouteBudget)
            } else if step.terminal {
                Some(Outcome::SessionTerminal)
            } else {
                None
            };
        }
    }
    Ok(states
        .into_iter()
        .map(|state| AttemptResult {
            task: state.task,
            prefix: state.prefix,
            commands: state.commands,
            accepted: state.accepted,
            entropy: state.entropy,
            factors: state.factors,
            outcome: state.outcome.expect("all frontier attempts terminate"),
        })
        .collect())
}

fn summarize_cell(temperature: f64, results: &[AttemptResult], seconds: f64) -> CellReport {
    let attempts = results.len();
    let successes = count(results, Outcome::Success);
    let unique_successes = results
        .iter()
        .filter(|result| result.outcome == Outcome::Success)
        .map(|result| hex_digest(&serde_json::to_vec(&(result.prefix, &result.commands)).unwrap()))
        .collect::<BTreeSet<_>>()
        .len();
    let accepted_commands = results.iter().map(|result| result.accepted).sum();
    let commands: usize = results.iter().map(|result| result.commands.len()).sum();
    let factors: usize = results.iter().map(|result| result.factors).sum();
    let entropy: f64 = results.iter().map(|result| result.entropy).sum();
    let (wilson_95_low, wilson_95_high) = wilson(successes, attempts);
    let actionable_attempts = attempts - count(results, Outcome::BlockedStart);
    let (actionable_wilson_95_low, actionable_wilson_95_high) =
        wilson(successes, actionable_attempts);
    let mut by_prefix = BTreeMap::new();
    for prefix in results
        .iter()
        .filter_map(|result| result.prefix)
        .collect::<BTreeSet<_>>()
    {
        let rows = results
            .iter()
            .filter(|result| result.prefix == Some(prefix))
            .collect::<Vec<_>>();
        let prefix_successes = rows
            .iter()
            .filter(|result| result.outcome == Outcome::Success)
            .count();
        by_prefix.insert(
            prefix,
            json!({
                "attempts":rows.len(),
                "successes":prefix_successes,
                "success_rate":ratio(prefix_successes, rows.len()),
                "blocked_starts":rows.iter().filter(|result| result.outcome == Outcome::BlockedStart).count(),
                "dead_ends":rows.iter().filter(|result| result.outcome == Outcome::DeadEnd).count(),
            }),
        );
    }
    CellReport {
        temperature,
        attempts,
        actionable_prefixes: by_prefix
            .values()
            .filter(|value| value["blocked_starts"].as_u64() == Some(0))
            .count(),
        actionable_attempts,
        successes,
        unique_successes,
        blocked_starts: count(results, Outcome::BlockedStart),
        dead_ends: count(results, Outcome::DeadEnd),
        aborted: count(results, Outcome::Aborted),
        route_budget_failures: count(results, Outcome::RouteBudget),
        session_terminals: count(results, Outcome::SessionTerminal),
        accepted_commands,
        mean_commands: ratio(commands, attempts),
        mean_factor_entropy: if factors == 0 {
            0.
        } else {
            entropy / factors as f64
        },
        success_rate: ratio(successes, attempts),
        wilson_95_low,
        wilson_95_high,
        actionable_success_rate: ratio(successes, actionable_attempts),
        actionable_wilson_95_low,
        actionable_wilson_95_high,
        by_prefix,
        seconds,
    }
}

fn count(results: &[AttemptResult], outcome: Outcome) -> usize {
    results
        .iter()
        .filter(|result| result.outcome == outcome)
        .count()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.
    } else {
        numerator as f64 / denominator as f64
    }
}

fn wilson(successes: usize, attempts: usize) -> (f64, f64) {
    if attempts == 0 {
        return (0., 1.);
    }
    let z = 1.959_963_984_540_054f64;
    let n = attempts as f64;
    let p = successes as f64 / n;
    let denominator = 1. + z * z / n;
    let center = (p + z * z / (2. * n)) / denominator;
    let margin = z * ((p * (1. - p) / n + z * z / (4. * n * n)).sqrt()) / denominator;
    ((center - margin).max(0.), (center + margin).min(1.))
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn render_html(report: &Value, cells: &[CellReport]) -> String {
    let rows = cells
        .iter()
        .map(|cell| {
            format!(
                "<tr><td>{:.2}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.6}</td><td>[{:.6}, {:.6}]</td><td>{}</td><td>{}</td><td>{:.4}</td></tr>",
                cell.temperature,
                cell.attempts,
                cell.actionable_attempts,
                cell.successes,
                cell.unique_successes,
                cell.actionable_success_rate,
                cell.actionable_wilson_95_low,
                cell.actionable_wilson_95_high,
                cell.blocked_starts,
                cell.dead_ends,
                cell.mean_factor_entropy,
            )
        })
        .collect::<String>();
    let json = escape_html(&serde_json::to_string_pretty(report).expect("report serializes"));
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Route frontier temperature probe</title><style>body{{font-family:system-ui;margin:2rem;max-width:90rem}}table{{border-collapse:collapse}}th,td{{border:1px solid #bbb;padding:.4rem;text-align:right}}th:first-child,td:first-child{{text-align:left}}pre{{background:#f5f5f5;padding:1rem;overflow:auto}}</style></head><body><h1>Route frontier temperature probe</h1><p>Status: <strong>{}</strong>. Prefixes: {}. Depth-one successes: {}.</p><table><thead><tr><th>Temperature</th><th>All attempts</th><th>Actionable attempts</th><th>Successes</th><th>Unique</th><th>Actionable rate</th><th>Actionable Wilson 95%</th><th>Blocked</th><th>Dead ends</th><th>Mean factor entropy</th></tr></thead><tbody>{rows}</tbody></table><h2>Report JSON</h2><pre>{json}</pre></body></html>",
        report["status"].as_str().unwrap_or("unknown"),
        report["prefixes_discovered"],
        report["total_depth_one_successes"],
    )
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilson_zero_success_upper_bound_is_bounded() {
        let (low, high) = wilson(0, 512);
        assert_eq!(low, 0.);
        assert!(high > 0. && high < 0.01);
    }
}
