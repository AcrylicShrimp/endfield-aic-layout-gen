//! Direct held-out evaluation of the disposable physical-outcome heads.

use super::{
    config::{Losses, Optimizer},
    model::{Batch, HEADS, Predictor, labels_for, sigmoid},
};
use crate::{
    input::{Tensors, sha256},
    outcomes::{
        sample::choose_random,
        storage::{ObservedOutcome, Row, WorkRecord},
    },
    schema::ModelConfig,
    training::evaluation::Panel,
};
use aic_layout::{physical::Metrics, rng::Rng};
use aic_pilot_native::{
    generation::physical_identity::Limits as IdentityLimits,
    input::{CatalogPaths, load_catalogs},
    learning::{
        Limits, Observation, Options,
        pool::{Command, Pool, Reply},
    },
};
use aic_torch::tch::{Device, Tensor};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path, time::Instant};

const SCHEMA: &str = "aic-physical-outcome-head-evaluation-v1";
const BINARY: [usize; 7] = [0, 1, 2, 3, 4, 5, 11];

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema: String,
    catalogs: CatalogPaths,
    model: ModelConfig,
    initialization_seed: u64,
    optimizer: Optimizer,
    losses: Losses,
    count_scale: f64,
    native: Limits,
    actions: Options,
    identity_limits: IdentityLimits,
    workers: usize,
    replicates: usize,
    actions_per_episode: u64,
    max_input_bytes: u64,
    prediction_batch_rows: usize,
}

impl Config {
    fn load(path: &Path) -> Result<(Self, Vec<u8>)> {
        ensure!(
            fs::metadata(path)?.len() <= 1024 * 1024,
            "aic-learning.head-evaluation-config: oversized {}",
            path.display()
        );
        let bytes = fs::read(path)?;
        let value: Self = serde_json::from_slice(&bytes)?;
        ensure!(
            value.schema == SCHEMA
                && value.model
                    == ModelConfig {
                        width: 40,
                        message_rounds: 2,
                        node_features: 41,
                        edge_features: 37,
                        global_features: 15,
                        action_features: 12,
                        port_features: 11,
                    }
                && value.count_scale > 0.0
                && value.workers > 0
                && (1..=64).contains(&value.replicates)
                && value.actions_per_episode > 0
                && value.actions_per_episode <= value.native.max_actions
                && value.max_input_bytes > 0
                && (1..=4096).contains(&value.prediction_batch_rows),
            "aic-learning.head-evaluation-config: invalid schema, model or bounds"
        );
        Ok((value, bytes))
    }
}

#[derive(Clone)]
struct Scored {
    case: String,
    product: String,
    stratum: String,
    replicate: usize,
    step: u64,
    family: usize,
    state: &'static str,
    status: String,
    target: [f32; 12],
    mask: [f32; 12],
    prediction: [f32; 12],
    initial: [f64; 3],
}

pub fn run(
    config_path: &Path,
    catalog_root: &Path,
    panel_path: &Path,
    checkpoint: &Path,
    output: &Path,
    device: Device,
) -> Result<Value> {
    ensure!(
        !output.exists(),
        "aic-learning.head-evaluation-output: output already exists: {}",
        output.display()
    );
    let began = Instant::now();
    let (config, config_bytes) = Config::load(config_path)?;
    let catalogs = load_catalogs(catalog_root, &config.catalogs).map_err(anyhow::Error::msg)?;
    let catalog_identity = catalogs.identity.clone();
    let panel = Panel::load(
        panel_path,
        config.max_input_bytes,
        &catalogs,
        &config.identity_limits,
    )?;
    ensure!(
        checkpoint.is_file(),
        "aic-learning.head-evaluation-checkpoint: missing {}",
        checkpoint.display()
    );
    aic_torch::deterministic()?;
    let values: Tensors = Tensor::read_safetensors(checkpoint)?.into_iter().collect();
    let mut predictor = Predictor::new_parts(
        &config.model,
        &config.optimizer,
        &config.losses,
        config.initialization_seed,
        device,
    )?;
    predictor.restore_parameters(&values)?;
    let rows = collect_rows(&config, &panel, catalogs)?;
    let mut scored = Vec::with_capacity(rows.len());
    for chunk in rows.chunks(config.prediction_batch_rows) {
        let references = chunk.iter().map(|row| &row.0).collect::<Vec<_>>();
        let batch = Batch::from_rows(&references, config.count_scale, device)?;
        let predictions = predictor.predictions(&batch)?;
        for ((row, meta), prediction) in chunk.iter().zip(predictions) {
            let (target, mask) = labels_for(row, config.count_scale)?;
            let initial = row.pre_action.global_features[3..6]
                .iter()
                .map(|value| f64::from(*value).exp_m1().round().max(1.0))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            scored.push(Scored {
                case: meta.case.clone(),
                product: meta.product.clone(),
                stratum: meta.stratum.clone(),
                replicate: meta.replicate,
                step: row.step,
                family: row.selected.family,
                state: if row.pre_action.pending > 0 || row.pre_action.conflicts > 0 {
                    "pending"
                } else {
                    "complete"
                },
                status: row.outcome.status.clone(),
                target,
                mask,
                prediction,
                initial,
            });
        }
    }
    let report = json!({
        "schema": SCHEMA,
        "config_sha256": crate::outcomes::storage::hash(&config_bytes),
        "catalog_sha256": catalog_identity,
        "panel_sha256": panel.identity,
        "checkpoint_sha256": sha256(checkpoint)?,
        "cases": panel.inputs.len(),
        "replicates": config.replicates,
        "episodes": panel.inputs.len() * config.replicates,
        "rows": scored.len(),
        "complete_episodes": complete_episodes(&scored, config.actions_per_episode),
        "action_sampling": "family-balanced-random-compatible-v1",
        "metrics": summarize(&scored, config.count_scale),
        "groups": group_summaries(&scored, config.count_scale),
        "largest_errors": largest_errors(&scored),
        "seconds": began.elapsed().as_secs_f64(),
    });
    write_json(output, &report)?;
    Ok(report)
}

#[derive(Clone)]
struct Meta {
    case: String,
    product: String,
    stratum: String,
    replicate: usize,
}

fn collect_rows(
    config: &Config,
    panel: &Panel,
    catalogs: std::sync::Arc<aic_pilot_native::input::Catalogs>,
) -> Result<Vec<(Row, Meta)>> {
    let count = panel.inputs.len() * config.replicates;
    let mut pool =
        Pool::new(catalogs, config.workers.min(count), count).map_err(anyhow::Error::msg)?;
    let mut limits = config.native.clone();
    limits.max_actions = config.actions_per_episode;
    let mut commands = Vec::with_capacity(count);
    for (case, input) in panel.inputs.iter().enumerate() {
        for replicate in 0..config.replicates {
            commands.push((
                case * config.replicates + replicate,
                Command::Open {
                    request: input.request.clone(),
                    original: input.original.clone(),
                    limits: limits.clone(),
                    options: config.actions.clone(),
                },
            ));
        }
    }
    ensure_ready(pool.exchange(commands).map_err(anyhow::Error::msg)?)?;
    let replies = pool
        .exchange((0..count).map(|id| (id, Command::Observe)).collect())
        .map_err(anyhow::Error::msg)?;
    let mut current: Vec<Option<Observation>> = (0..count).map(|_| None).collect();
    for (id, reply) in replies {
        let Reply::Observation(observation) = reply else {
            anyhow::bail!("aic-learning.head-evaluation-protocol: observation required")
        };
        current[id] = Some(observation);
    }
    let mut rngs = panel
        .manifest
        .cases
        .iter()
        .flat_map(|case| {
            (0..config.replicates).map(move |replicate| {
                case.action_seed
                    .checked_add(replicate as u64)
                    .map(Rng::new)
                    .context("aic-learning.head-evaluation-seed: overflow")
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut rows = Vec::with_capacity(count * config.actions_per_episode as usize);
    let mut charged = vec![0u64; count];
    for _ in 0..config.actions_per_episode {
        let active = current
            .iter()
            .enumerate()
            .filter_map(|(id, value)| (!value.as_ref().unwrap().done).then_some(id))
            .collect::<Vec<_>>();
        if active.is_empty() {
            break;
        }
        let mut pending = BTreeMap::new();
        let mut commands = Vec::with_capacity(active.len());
        for id in active {
            let observation = current[id].as_ref().unwrap();
            let selected = choose_random(&observation.input, &observation.domain, &mut rngs[id])?;
            commands.push((
                id,
                Command::Step {
                    handle: observation.handle,
                    root: selected.root,
                    ports: selected.ports.clone(),
                },
            ));
            pending.insert(id, (selected, observation.input.as_ref().clone()));
        }
        for (id, reply) in pool.exchange(commands).map_err(anyhow::Error::msg)? {
            let Reply::Step { result, next } = reply else {
                anyhow::bail!("aic-learning.head-evaluation-protocol: step required")
            };
            let (selected, pre_action) = pending.remove(&id).unwrap();
            let case = id / config.replicates;
            let replicate = id % config.replicates;
            if result["outcome"]["charged"] == true {
                let step = charged[id];
                charged[id] += 1;
                rows.push((
                    Row {
                        root_id: format!("{}/{}", panel.manifest.cases[case].id, replicate),
                        step,
                        pre_action,
                        selected,
                        outcome: observed_outcome(&result["outcome"], &next, &config.native)?,
                    },
                    Meta {
                        case: panel.manifest.cases[case].id.clone(),
                        product: panel.manifest.cases[case].product.clone(),
                        stratum: panel.manifest.cases[case].stratum.clone(),
                        replicate,
                    },
                ));
            }
            current[id] = Some(next);
        }
    }
    ensure!(
        !rows.is_empty(),
        "aic-learning.head-evaluation: no charged rows"
    );
    Ok(rows)
}

fn ensure_ready(replies: Vec<(usize, Reply)>) -> Result<()> {
    ensure!(
        replies
            .into_iter()
            .all(|(_, reply)| matches!(reply, Reply::Ready)),
        "aic-learning.head-evaluation-protocol: ready required"
    );
    Ok(())
}

fn observed_outcome(value: &Value, next: &Observation, limits: &Limits) -> Result<ObservedOutcome> {
    let status = value["status"]
        .as_str()
        .context("aic-learning.head-evaluation-outcome: missing status")?
        .to_owned();
    let diagnostics = value["diagnostics"].as_array().cloned().unwrap_or_default();
    let censored = diagnostics.iter().any(|diagnostic| {
        diagnostic["code"]
            .as_str()
            .is_some_and(|code| code.ends_with(".elapsed") || code.ends_with(".budget-exhausted"))
    });
    let changed = matches!(status.as_str(), "draft_accepted" | "validated" | "improved");
    let expanded = value["details"]["route_expansions"].as_u64().unwrap_or(0) as usize;
    let current_metrics = matches!(status.as_str(), "validated" | "improved")
        .then(|| metric(&value["details"]["native_acceptance"]["metrics"]))
        .transpose()?;
    Ok(ObservedOutcome {
        status,
        charged: value["charged"] == true,
        censored,
        attempts: value["attempts"].as_u64().unwrap_or(next.attempts),
        revision: value["revision"].as_u64().unwrap_or(next.revision),
        done: value["done"] == true,
        work: WorkRecord {
            expanded,
            preflight_expanded: value["details"]["preflight_expansions"]
                .as_u64()
                .unwrap_or(0) as usize,
            route_calls: value["details"]["route_calls"].as_u64().unwrap_or(0) as usize,
            pending: next.input.pending,
            routes: vec![],
            committed: changed,
        },
        best_metrics: metric(&value["best_metrics"])
            .context("aic-learning.head-evaluation-metrics: invalid retained best")?,
        current_pending: changed.then_some(next.input.pending),
        current_conflicts: changed.then_some(next.input.conflicts),
        current_metrics,
        route_budget_exhausted: changed.then_some(expanded >= limits.route_expansions),
        diagnostics: vec![],
    })
}

fn metric(value: &Value) -> Result<Metrics> {
    let score = value["score"]
        .as_array()
        .context("aic-learning.head-evaluation-metrics: missing score")?;
    ensure!(
        score.len() == 3,
        "aic-learning.head-evaluation-metrics: score length"
    );
    Ok(Metrics {
        width: value["width"].as_i64().unwrap_or(0),
        height: value["height"].as_i64().unwrap_or(0),
        area: score[0].as_u64().context("area score")?,
        body_cells: 0,
        transport_cells: score[1].as_u64().context("transport score")?,
        turns: score[2].as_u64().context("turn score")?,
        occupancy: 0.0,
        weak_components: 0,
        branch_blocks: 0,
        merge_blocks: 0,
        max_degree: 0,
        mean_port_distance: 0.0,
        mean_detour: 0.0,
        belt_connections: 0,
        pipe_connections: 0,
        bridges: 0,
    })
}

fn summarize(rows: &[Scored], count_scale: f64) -> Value {
    Value::Object(
        HEADS
            .iter()
            .enumerate()
            .map(|(head, name)| {
                let selected = rows
                    .iter()
                    .filter(|row| row.mask[head] > 0.0)
                    .collect::<Vec<_>>();
                let value = if BINARY.contains(&head) {
                    binary(head, &selected)
                } else {
                    regression(head, &selected, count_scale)
                };
                ((*name).to_owned(), value)
            })
            .collect(),
    )
}

fn binary(head: usize, rows: &[&Scored]) -> Value {
    let positives = rows.iter().filter(|row| row.target[head] > 0.5).count();
    if rows.is_empty() {
        return json!({"labels": 0});
    }
    let prevalence = positives as f64 / rows.len() as f64;
    let mut brier = 0.0;
    let mut log_loss = 0.0;
    let mut correct = 0usize;
    for row in rows {
        let probability = sigmoid(row.prediction[head]).clamp(1e-7, 1.0 - 1e-7);
        let target = f64::from(row.target[head]);
        brier += (probability - target).powi(2);
        log_loss -= target * probability.ln() + (1.0 - target) * (1.0 - probability).ln();
        correct += usize::from((probability >= 0.5) == (target >= 0.5));
    }
    json!({
        "labels": rows.len(),
        "positives": positives,
        "negatives": rows.len() - positives,
        "prevalence": prevalence,
        "accuracy": correct as f64 / rows.len() as f64,
        "brier": brier / rows.len() as f64,
        "log_loss": log_loss / rows.len() as f64,
        "auroc": auroc(head, rows),
        "evaluation_fitted_constant": {
            "accuracy": prevalence.max(1.0 - prevalence),
            "brier": prevalence * (1.0 - prevalence),
            "log_loss": if prevalence == 0.0 || prevalence == 1.0 { 0.0 } else {
                -(prevalence * prevalence.ln() + (1.0 - prevalence) * (1.0 - prevalence).ln())
            }
        }
    })
}

fn auroc(head: usize, rows: &[&Scored]) -> Option<f64> {
    let positives = rows
        .iter()
        .filter(|row| row.target[head] > 0.5)
        .collect::<Vec<_>>();
    let negatives = rows
        .iter()
        .filter(|row| row.target[head] <= 0.5)
        .collect::<Vec<_>>();
    if positives.is_empty() || negatives.is_empty() {
        return None;
    }
    let mut score = 0.0;
    for positive in &positives {
        for negative in &negatives {
            score += if positive.prediction[head] > negative.prediction[head] {
                1.0
            } else if positive.prediction[head] == negative.prediction[head] {
                0.5
            } else {
                0.0
            };
        }
    }
    Some(score / (positives.len() * negatives.len()) as f64)
}

fn regression(head: usize, rows: &[&Scored], count_scale: f64) -> Value {
    if rows.is_empty() {
        return json!({"labels": 0});
    }
    let mean = rows
        .iter()
        .map(|row| f64::from(row.target[head]))
        .sum::<f64>()
        / rows.len() as f64;
    let mut mae = 0.0;
    let mut squared = 0.0;
    let mut baseline = 0.0;
    let mut raw = 0.0;
    for row in rows {
        let prediction = f64::from(row.prediction[head]);
        let target = f64::from(row.target[head]);
        mae += (prediction - target).abs();
        squared += (prediction - target).powi(2);
        baseline += (mean - target).abs();
        raw += match head {
            6 | 7 => ((prediction * count_scale.ln_1p()).exp_m1()
                - (target * count_scale.ln_1p()).exp_m1())
            .abs(),
            8..=10 => (prediction - target).abs() * row.initial[head - 8],
            _ => 0.0,
        };
    }
    json!({
        "labels": rows.len(),
        "mae": mae / rows.len() as f64,
        "rmse": (squared / rows.len() as f64).sqrt(),
        "raw_mae": raw / rows.len() as f64,
        "evaluation_fitted_constant_mae": baseline / rows.len() as f64,
    })
}

fn group_summaries(rows: &[Scored], count_scale: f64) -> Value {
    let mut groups = BTreeMap::<String, Vec<Scored>>::new();
    for row in rows {
        groups
            .entry(format!("stratum={}", row.stratum))
            .or_default()
            .push(row.clone());
        groups
            .entry(format!("product={}", row.product))
            .or_default()
            .push(row.clone());
    }
    Value::Object(
        groups
            .into_iter()
            .map(|(name, rows)| {
                (
                    name,
                    json!({"rows": rows.len(), "metrics": summarize(&rows, count_scale)}),
                )
            })
            .collect(),
    )
}

fn largest_errors(rows: &[Scored]) -> Value {
    Value::Object(
        HEADS
            .iter()
            .enumerate()
            .map(|(head, name)| {
                let mut selected = rows
                    .iter()
                    .filter(|row| row.mask[head] > 0.0)
                    .map(|row| {
                        let prediction = if BINARY.contains(&head) {
                            sigmoid(row.prediction[head])
                        } else {
                            f64::from(row.prediction[head])
                        };
                        let target = f64::from(row.target[head]);
                        ((prediction - target).abs(), row, prediction, target)
                    })
                    .collect::<Vec<_>>();
                selected.sort_by(|a, b| b.0.total_cmp(&a.0));
                (
                    (*name).to_owned(),
                    Value::Array(
                        selected
                            .into_iter()
                            .take(3)
                            .map(|(error, row, prediction, target)| {
                                json!({"case":row.case,"product":row.product,"stratum":row.stratum,
                                    "replicate":row.replicate,"step":row.step,"family":row.family,
                                    "state":row.state,"status":row.status,"prediction":prediction,
                                    "target":target,"absolute_error":error})
                            })
                            .collect(),
                    ),
                )
            })
            .collect(),
    )
}

fn complete_episodes(rows: &[Scored], actions: u64) -> usize {
    let mut counts = BTreeMap::new();
    for row in rows {
        *counts.entry((&row.case, row.replicate)).or_insert(0u64) += 1;
    }
    counts.values().filter(|&&count| count == actions).count()
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
