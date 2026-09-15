//! Long-running encoder pretraining from disposable, freshly generated outcomes.

use super::{
    config::{Losses, Optimizer},
    model::{Batch, HEADS, Predictor},
};
use crate::{
    input::{Tensors, sha256},
    outcomes::{
        live::{Collector, RootSpec},
        storage::hash,
    },
    schema::ModelConfig,
    training::{config::Resources as TrainingResources, resources::Guard},
};
use aic_layout_gen::Config as GeneratorConfig;
use aic_pilot_native::physical_editor::{PhysicalCandidateOptions, PhysicalSessionLimits};
use aic_torch::tch::{Device, Tensor, no_grad};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufWriter, Write},
    path::Path,
    process::Command,
    time::Instant,
};

const SCHEMA: &str = "aic-physical-outcome-online-pretraining-v1";
const STATE_SCHEMA: &str = "aic-physical-outcome-online-state-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    schema: String,
    model: ModelConfig,
    initialization_seed: u64,
    generator: GeneratorConfig,
    candidates: PhysicalCandidateOptions,
    session: PhysicalSessionLimits,
    workers: usize,
    roots_per_update: usize,
    roots_per_minibatch: usize,
    development_roots: usize,
    development_interval_updates: u64,
    checkpoint_interval_updates: u64,
    duration_seconds: u64,
    train_generation_seed: u64,
    train_action_seed: u64,
    development_generation_seed: u64,
    development_action_seed: u64,
    optimizer: Optimizer,
    count_scale: f64,
    losses: Losses,
    max_rss_bytes: u64,
    max_output_bytes: u64,
    min_free_bytes: u64,
}

impl Config {
    fn load(path: &Path) -> Result<(Self, Vec<u8>)> {
        ensure!(
            fs::metadata(path)?.len() <= 1024 * 1024,
            "aic-learning.online-config: config exceeds 1 MiB"
        );
        let bytes = fs::read(path)?;
        let value: Self = serde_json::from_slice(&bytes)?;
        value.validate()?;
        Ok((value, bytes))
    }

    fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA,
            "aic-learning.online-config-schema: expected {SCHEMA}"
        );
        ensure!(
            self.model
                == ModelConfig {
                    width: 40,
                    message_rounds: 2,
                    node_features: 41,
                    edge_features: 37,
                    global_features: 15,
                    action_features: 12,
                    port_features: 11
                },
            "aic-learning.online-config-model: expected the established physical graph encoder"
        );
        ensure!(
            self.workers > 0
                && self.roots_per_update == self.workers
                && self.roots_per_update % self.roots_per_minibatch == 0
                && self.development_roots > 0
                && self.development_roots <= self.workers,
            "aic-learning.online-config-batching: invalid worker or root batch sizes"
        );
        ensure!(
            self.session.max_actions == 32
                && self.session.route_expansions == 50_000
                && self.session.max_total_route_expansions == 1_600_000,
            "aic-learning.online-config-session: expected established 32-action native limits"
        );
        ensure!(
            self.checkpoint_interval_updates > 0
                && self.development_interval_updates > 0
                && self.development_interval_updates % self.checkpoint_interval_updates == 0,
            "aic-learning.online-config-cadence: invalid checkpoint or development cadence"
        );
        ensure!(
            self.duration_seconds > 0 && self.duration_seconds <= 24 * 60 * 60,
            "aic-learning.online-config-duration: duration must be at most 24 hours"
        );
        let train_end = self
            .train_generation_seed
            .checked_add(100_000_000_000)
            .unwrap_or(u64::MAX);
        let action_end = self
            .train_action_seed
            .checked_add(100_000_000_000)
            .unwrap_or(u64::MAX);
        ensure!(
            self.development_generation_seed >= train_end
                && self.development_action_seed >= action_end,
            "aic-learning.online-config-seeds: development namespaces must be disjoint from training"
        );
        ensure!(
            self.optimizer.learning_rate.is_finite()
                && self.optimizer.learning_rate > 0.0
                && self.optimizer.gradient_clip.is_finite()
                && self.optimizer.gradient_clip > 0.0
                && self
                    .losses
                    .values()
                    .iter()
                    .all(|v| v.is_finite() && *v > 0.0)
                && self.count_scale > 0.0,
            "aic-learning.online-config-optimization: invalid optimizer or loss settings"
        );
        ensure!(
            self.max_rss_bytes > 0 && self.max_output_bytes >= 1024 * 1024,
            "aic-learning.online-config-resources: invalid resource limits"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct State {
    schema: String,
    status: String,
    config_sha256: String,
    catalog_sha256: String,
    pid: u32,
    update: u64,
    training_roots: u64,
    training_rows: u64,
    development_roots: u64,
    development_rows: u64,
    elapsed_seconds: f64,
    latest_checkpoint: Option<String>,
    stop_reason: Option<String>,
    last_train_loss: Option<f64>,
    last_development_loss: Option<f64>,
}

#[derive(Serialize)]
struct Checkpoint {
    schema: &'static str,
    config_sha256: String,
    catalog_sha256: String,
    update: u64,
    tensors_sha256: String,
}

pub fn run(
    config_path: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
    update_limit: Option<u64>,
    resume: bool,
    workers: Option<usize>,
) -> Result<Value> {
    let (mut config, config_bytes) = Config::load(config_path)?;
    if let Some(workers) = workers {
        ensure!(
            workers == config.workers,
            "aic-learning.online-workers: runtime workers must match the registered config"
        );
        config.workers = workers;
    }
    let config_hash = hash(&config_bytes);
    if resume {
        ensure!(
            output.is_dir(),
            "aic-learning.online-resume: missing output {}",
            output.display()
        );
    } else {
        ensure!(
            !output.exists(),
            "aic-learning.online-output: output already exists: {}",
            output.display()
        );
        fs::create_dir_all(output.join("checkpoints"))?;
        fs::write(output.join("config.json"), &config_bytes)?;
    }
    aic_torch::deterministic()?;
    let (mut collector, catalog_hash) = Collector::new(
        catalog_root,
        config.generator.clone(),
        config.candidates.clone(),
        config.session.clone(),
        config.workers,
    )?;
    let mut predictor = Predictor::new_parts(
        &config.model,
        &config.optimizer,
        &config.losses,
        config.initialization_seed,
        device,
    )?;
    let mut state = if resume {
        let value: State = read_json(&output.join("state.json"))?;
        ensure!(
            value.config_sha256 == config_hash && value.catalog_sha256 == catalog_hash,
            "aic-learning.online-resume: config or catalog binding changed"
        );
        ensure!(
            value.status != "running" || !process_alive(value.pid),
            "aic-learning.online-resume: trainer process {} is still active",
            value.pid
        );
        let checkpoint = value
            .latest_checkpoint
            .as_ref()
            .context("aic-learning.online-resume: no checkpoint")?;
        let tensors: Tensors =
            Tensor::read_safetensors(output.join(checkpoint).join("state.safetensors"))?
                .into_iter()
                .collect();
        predictor.restore_checkpoint(&tensors)?;
        value
    } else {
        State {
            schema: STATE_SCHEMA.into(),
            status: "prepared".into(),
            config_sha256: config_hash.clone(),
            catalog_sha256: catalog_hash.clone(),
            pid: std::process::id(),
            update: 0,
            training_roots: 0,
            training_rows: 0,
            development_roots: 0,
            development_rows: 0,
            elapsed_seconds: 0.0,
            latest_checkpoint: None,
            stop_reason: None,
            last_train_loss: None,
            last_development_loss: None,
        }
    };
    ensure!(
        state.schema == STATE_SCHEMA,
        "aic-learning.online-state: unsupported state schema"
    );
    state.pid = std::process::id();
    let resources = TrainingResources {
        max_observation_bytes: u64::MAX / 2,
        max_rss_bytes: config.max_rss_bytes,
        max_output_bytes: config.max_output_bytes,
        min_free_bytes: config.min_free_bytes,
    };
    let remaining = config
        .duration_seconds
        .saturating_sub(state.elapsed_seconds as u64)
        .max(300);
    let guard = Guard::new(output, &resources, remaining + 300)?;
    let started = Instant::now();
    let base_elapsed = state.elapsed_seconds;
    if state.latest_checkpoint.is_none() {
        checkpoint(
            output,
            &config_hash,
            &catalog_hash,
            config.max_output_bytes,
            &mut state,
            &mut predictor,
        )?;
    }
    state.status = "running".into();
    state.stop_reason = None;
    write_json_atomic(&output.join("state.json"), &state)?;
    let mut window_loss = 0.0;
    let mut window_gradient = 0.0;
    let mut window_updates = 0u64;

    loop {
        guard.check().run()?;
        let elapsed = base_elapsed + started.elapsed().as_secs_f64();
        let reached_time = elapsed >= config.duration_seconds as f64;
        let reached_updates = update_limit.is_some_and(|limit| state.update >= limit);
        if reached_time || reached_updates || output.join("STOP").exists() {
            state.stop_reason = Some(
                if reached_time {
                    "duration"
                } else if reached_updates {
                    "update_limit"
                } else {
                    "requested"
                }
                .into(),
            );
            break;
        }
        let specs = root_specs(
            "train",
            state.update * config.roots_per_update as u64,
            config.roots_per_update,
            config.train_generation_seed,
            config.train_action_seed,
        )?;
        let rows = collector.collect(&specs)?;
        let mut losses = Vec::new();
        let mut gradients = Vec::new();
        for roots in specs.chunks(config.roots_per_minibatch) {
            let ids = roots
                .iter()
                .map(|root| root.root_id.as_str())
                .collect::<std::collections::BTreeSet<_>>();
            let selected = rows
                .iter()
                .filter(|row| ids.contains(row.root_id.as_str()))
                .collect::<Vec<_>>();
            let batch = Batch::from_rows(&selected, config.count_scale, device)?;
            let report = predictor.train(&batch)?;
            losses.push(report.loss);
            gradients.push(report.gradient_norm);
        }
        state.update += 1;
        state.training_roots += specs.len() as u64;
        state.training_rows += rows.len() as u64;
        state.elapsed_seconds = base_elapsed + started.elapsed().as_secs_f64();
        let loss = mean(&losses);
        let gradient = mean(&gradients);
        state.last_train_loss = Some(loss);
        window_loss += loss;
        window_gradient += gradient;
        window_updates += 1;

        if state.update % config.development_interval_updates == 0 {
            let ordinal = (state.update / config.development_interval_updates - 1)
                * config.development_roots as u64;
            let specs = root_specs(
                "dev",
                ordinal,
                config.development_roots,
                config.development_generation_seed,
                config.development_action_seed,
            )?;
            let rows = collector.collect(&specs)?;
            state.last_development_loss = Some(evaluate(
                &predictor,
                &rows,
                config.roots_per_minibatch,
                config.count_scale,
                device,
            )?);
            state.development_roots += specs.len() as u64;
            state.development_rows += rows.len() as u64;
        }
        if state.update % config.checkpoint_interval_updates == 0 {
            checkpoint(
                output,
                &config_hash,
                &catalog_hash,
                config.max_output_bytes,
                &mut state,
                &mut predictor,
            )?;
            append_metric(
                output,
                json!({"schema":"aic-physical-outcome-online-metric-v1",
                "update":state.update,"elapsed_seconds":state.elapsed_seconds,
                "training_roots":state.training_roots,"training_rows":state.training_rows,
                "train_loss":window_loss/window_updates as f64,
                "gradient_norm":window_gradient/window_updates as f64,
                "development_loss":state.last_development_loss,
                "rows_per_second":state.training_rows as f64/state.elapsed_seconds.max(1e-9),
                "labels":HEADS}),
            )?;
            window_loss = 0.0;
            window_gradient = 0.0;
            window_updates = 0;
        }
    }
    state.elapsed_seconds = base_elapsed + started.elapsed().as_secs_f64();
    checkpoint(
        output,
        &config_hash,
        &catalog_hash,
        config.max_output_bytes,
        &mut state,
        &mut predictor,
    )?;
    if window_updates > 0 {
        append_metric(
            output,
            json!({"schema":"aic-physical-outcome-online-metric-v1",
            "update":state.update,"elapsed_seconds":state.elapsed_seconds,
            "training_roots":state.training_roots,"training_rows":state.training_rows,
            "train_loss":window_loss/window_updates as f64,
            "gradient_norm":window_gradient/window_updates as f64,
            "development_loss":state.last_development_loss,
            "rows_per_second":state.training_rows as f64/state.elapsed_seconds.max(1e-9),
            "labels":HEADS}),
        )?;
    }
    let encoder = predictor.encoder_snapshot();
    let encoder_path = output.join("encoder.safetensors");
    let pending = output.join("encoder.pending");
    Tensor::write_safetensors(&encoder.iter().collect::<Vec<_>>(), &pending)?;
    fs::File::open(&pending)?.sync_all()?;
    fs::rename(&pending, &encoder_path)?;
    state.status = "complete".into();
    write_json_atomic(&output.join("state.json"), &state)?;
    ensure!(
        owned_bytes(output, &state)? <= config.max_output_bytes,
        "aic-learning.online-output: retained output exceeds configured limit"
    );
    Ok(
        json!({"schema":STATE_SCHEMA,"status":state.status,"stop_reason":state.stop_reason,
        "update":state.update,"training_roots":state.training_roots,"training_rows":state.training_rows,
        "development_roots":state.development_roots,"development_rows":state.development_rows,
        "elapsed_seconds":state.elapsed_seconds,"peak_rss_bytes":guard.peak_rss(),
        "encoder":encoder_path,"encoder_sha256":sha256(&encoder_path)?}),
    )
}

fn root_specs(
    prefix: &str,
    start: u64,
    count: usize,
    generation: u64,
    action: u64,
) -> Result<Vec<RootSpec>> {
    (0..count)
        .map(|offset| {
            let ordinal = start
                .checked_add(offset as u64)
                .context("aic-learning.online-seed: ordinal overflow")?;
            Ok(RootSpec {
                root_id: format!("{prefix}-{ordinal:012}"),
                blocks: if ordinal % 2 == 0 { 8 } else { 16 },
                generation_seed: generation
                    .checked_add(
                        ordinal
                            .checked_mul(1024)
                            .context("aic-learning.online-seed: generation stride overflow")?,
                    )
                    .context("aic-learning.online-seed: generation overflow")?,
                action_seed: action
                    .checked_add(ordinal)
                    .context("aic-learning.online-seed: action overflow")?,
            })
        })
        .collect()
}

fn evaluate(
    predictor: &Predictor,
    rows: &[crate::outcomes::storage::Row],
    roots_per_batch: usize,
    count_scale: f64,
    device: Device,
) -> Result<f64> {
    let mut by_root = BTreeMap::<&str, Vec<&crate::outcomes::storage::Row>>::new();
    for row in rows {
        by_root.entry(&row.root_id).or_default().push(row);
    }
    let roots = by_root.into_values().collect::<Vec<_>>();
    let mut losses = Vec::new();
    for chunk in roots.chunks(roots_per_batch) {
        let refs = chunk.iter().flatten().copied().collect::<Vec<_>>();
        let batch = Batch::from_rows(&refs, count_scale, device)?;
        losses.push(f64::try_from(no_grad(|| {
            predictor.evaluate_loss(&batch).map(|v| v.0)
        })?)?);
    }
    Ok(mean(&losses))
}

fn checkpoint(
    root: &Path,
    config_hash: &str,
    catalog_hash: &str,
    output_limit: u64,
    state: &mut State,
    predictor: &mut Predictor,
) -> Result<()> {
    if state
        .latest_checkpoint
        .as_deref()
        .is_some_and(|path| path.ends_with(&format!("{:012}", state.update)))
    {
        write_json_atomic(&root.join("state.json"), state)?;
        return Ok(());
    }
    let name = format!("checkpoints/update-{:012}", state.update);
    let final_path = root.join(&name);
    let stage = final_path.with_extension("pending");
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    fs::create_dir(&stage)?;
    let tensors = predictor.training_snapshot()?;
    let tensor_path = stage.join("state.safetensors");
    Tensor::write_safetensors(&tensors.iter().collect::<Vec<_>>(), &tensor_path)?;
    fs::File::open(&tensor_path)?.sync_all()?;
    write_json(
        &stage.join("manifest.json"),
        &Checkpoint {
            schema: STATE_SCHEMA,
            config_sha256: config_hash.into(),
            catalog_sha256: catalog_hash.into(),
            update: state.update,
            tensors_sha256: sha256(&tensor_path)?,
        },
    )?;
    fs::File::open(&stage)?.sync_all()?;
    fs::rename(&stage, &final_path)?;
    let old = state.latest_checkpoint.replace(name);
    write_json_atomic(&root.join("state.json"), state)?;
    if let Some(old) = old {
        let old = root.join(old);
        if old != final_path && old.exists() {
            fs::remove_dir_all(old)?;
        }
    }
    ensure!(
        owned_bytes(root, state)? <= output_limit,
        "aic-learning.online-output: checkpoint output exceeded hard limit"
    );
    Ok(())
}

fn append_metric(root: &Path, value: Value) -> Result<()> {
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("metrics.jsonl"))?;
    serde_json::to_writer(&mut file, &value)?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}
fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}
fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    ensure!(
        fs::metadata(path)?.len() <= 16 * 1024 * 1024,
        "aic-learning.online-state: oversized JSON"
    );
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut writer = BufWriter::new(fs::File::create(path)?);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    Ok(())
}
fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let pending = path.with_extension("pending");
    write_json(&pending, value)?;
    fs::rename(pending, path)?;
    Ok(())
}
fn owned_bytes(root: &Path, state: &State) -> Result<u64> {
    let direct = [
        "config.json",
        "state.json",
        "metrics.jsonl",
        "encoder.safetensors",
    ];
    let checkpoint = state.latest_checkpoint.as_ref().map(|path| root.join(path));
    direct
        .iter()
        .map(|name| root.join(name))
        .chain(
            checkpoint
                .into_iter()
                .flat_map(|path| [path.join("manifest.json"), path.join("state.safetensors")]),
        )
        .filter(|path| path.is_file())
        .try_fold(0u64, |total, path| {
            total
                .checked_add(fs::metadata(path)?.len())
                .context("aic-learning.online-output: byte overflow")
        })
}

fn process_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}
