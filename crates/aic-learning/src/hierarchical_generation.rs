//! Alternating-generation distillation from frozen hierarchical MCTS records.

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use aic_layout::{geometry::Point, validation::Catalogs};
#[cfg(test)]
use aic_pilot_native::constructive::Command;
use aic_pilot_native::constructive::{
    Limits, Session,
    learning::{Config as DomainConfig, Input},
    routing_candidates::{Candidate, GuidanceInput, Provenance},
};
use aic_torch::{
    Adam, AdamBackend, AdamState,
    tch::{self, Device, Kind, Tensor},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    hierarchical_search::{
        Case, MacroAction, PANEL_SCHEMA, Panel, Published, REPORT_SCHEMA as SEARCH_REPORT_SCHEMA,
        Report as CollectionReport, RootRecord, load_published_panel, load_published_report,
    },
    input::Tensors,
    outer_model::{
        ACTION_FEATURES, Batch as OuterBatch, Model as OuterModel, ValueTarget,
        normalized_layout_quality, value_loss,
    },
    outer_training::Config as OuterTrainingConfig,
    route_guidance::{
        Batch as LowerBatch, Model as LowerModel, ParameterOrder as LowerOrder,
        Prediction as LowerPrediction,
    },
    route_guidance_training::Config as LowerTrainingConfig,
};

const CONFIG_SCHEMA: &str = "aic-hierarchical-generation-training-v1";
pub(crate) const REPORT_SCHEMA: &str = "aic-hierarchical-generation-training-report-v3";
const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

fn default_checkpoint_interval_epochs() -> usize {
    50
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub epochs: usize,
    #[serde(default = "default_checkpoint_interval_epochs")]
    pub checkpoint_interval_epochs: usize,
    pub batch_states: usize,
    pub search_temperature: f64,
    pub lower_model_temperature: f64,
    pub lower_learning_rate: f64,
    pub outer_learning_rate: f64,
    pub gradient_clip: f64,
    pub seed: u64,
    #[serde(default)]
    pub outer_states_per_epoch: Option<usize>,
    #[serde(default = "default_true")]
    pub train_lower: bool,
    #[serde(default = "default_true")]
    pub train_failed_policy: bool,
}

fn default_true() -> bool {
    true
}

struct OuterExample {
    input: Arc<Input>,
    features: Vec<[f32; ACTION_FEATURES]>,
    entity_references: Vec<i64>,
    target: Vec<f32>,
    values: ValueTarget,
    successful_episode: bool,
}

struct ReplaySource {
    identity: ReplayIdentity,
    examples: Vec<OuterExample>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ReplayIdentity {
    panel_sha256: String,
    collection_sha256: String,
}

pub(crate) struct ReplayWindow {
    capacity: usize,
    sources: VecDeque<ReplaySource>,
}

impl ReplayWindow {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            sources: VecDeque::new(),
        }
    }

    fn identities(&self) -> Vec<ReplayIdentity> {
        self.sources
            .iter()
            .map(|source| source.identity.clone())
            .collect()
    }

    fn examples(&self) -> Vec<&OuterExample> {
        self.sources
            .iter()
            .flat_map(|source| source.examples.iter())
            .collect()
    }

    fn push(&mut self, source: ReplaySource) {
        if self.capacity == 0 {
            return;
        }
        self.sources.push_back(source);
        while self.sources.len() > self.capacity {
            self.sources.pop_front();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn restore_source(
        &mut self,
        panel_path: &Path,
        collection_path: &Path,
        lower_training_output: &Path,
        outer_training_output: &Path,
        catalog_root: &Path,
        generation_config: &Config,
    ) -> Result<()> {
        if self.capacity == 0 {
            return Ok(());
        }
        let panel = load_published_panel(panel_path)?;
        let collection = load_published_report(collection_path)?;
        validate_inputs(&panel.value, &collection.value)?;
        let lower_config_bytes = bounded_read(&lower_training_output.join("config.json"))?;
        let lower_order_bytes = bounded_read(&lower_training_output.join("parameter-order.json"))?;
        let lower_config: LowerTrainingConfig = serde_json::from_slice(&lower_config_bytes)?;
        let outer_config_bytes = bounded_read(&outer_training_output.join("config.json"))?;
        let outer_order_bytes = bounded_read(&outer_training_output.join("parameter-order.json"))?;
        let outer_config: OuterTrainingConfig = serde_json::from_slice(&outer_config_bytes)?;
        ensure!(
            bytes_sha256(&lower_config_bytes) == collection.value.lower_config_sha256
                && bytes_sha256(&lower_order_bytes)
                    == collection.value.lower_parameter_order_sha256
                && crate::input::sha256(&lower_training_output.join("state.safetensors"))?
                    == collection.value.lower_checkpoint_sha256
                && bytes_sha256(&outer_config_bytes) == collection.value.outer_config_sha256
                && bytes_sha256(&outer_order_bytes)
                    == collection.value.outer_parameter_order_sha256
                && crate::input::sha256(&outer_training_output.join("state.safetensors"))?
                    == collection.value.outer_checkpoint_sha256
                && semantic_sha256(&outer_config.search)? == collection.value.search_config_sha256,
            "aic-learning.hierarchical-replay-lineage: replay model or search differs"
        );
        validate_collection_structure(&collection.value, &outer_config.search)?;
        let loaded = aic_layout_gen::io::load_catalogs(catalog_root)?;
        ensure!(
            loaded.catalogs.identity == collection.value.catalog_sha256,
            "aic-learning.hierarchical-replay-catalog: replay catalog differs"
        );
        crate::input::verify_training_artifact(
            lower_training_output,
            &loaded.catalogs.identity,
            &collection.value.lower_checkpoint_sha256,
        )?;
        crate::input::verify_training_artifact(
            outer_training_output,
            &loaded.catalogs.identity,
            &collection.value.outer_checkpoint_sha256,
        )?;
        let domain = DomainConfig {
            placement: outer_config.search.bounds,
            extension_lengths: outer_config.search.extension_lengths.clone(),
        };
        ensure!(
            lower_config.learning_domain == domain,
            "aic-learning.hierarchical-replay-domain: replay lower and outer domains differ"
        );
        let (outer, _) = collect_examples(
            panel.value,
            collection.value,
            &domain,
            &outer_config,
            &lower_config,
            Arc::new(loaded.catalogs),
            generation_config,
            false,
        )?;
        self.push(ReplaySource {
            identity: ReplayIdentity {
                panel_sha256: panel.sha256,
                collection_sha256: collection.sha256,
            },
            examples: outer
                .into_iter()
                .filter(|example| example.successful_episode)
                .collect(),
        });
        Ok(())
    }
}

struct LowerCandidate {
    edge_offsets: Vec<i64>,
    head: i64,
    q: f32,
}

struct LowerExample {
    input: GuidanceInput,
    candidates: Vec<LowerCandidate>,
    target: Vec<f32>,
    success: f32,
}

struct LowerGroup {
    session: Session,
    candidates: Vec<(usize, Candidate, i64)>,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    status: &'static str,
    resumed: bool,
    device: String,
    catalog_sha256: String,
    source_lower_checkpoint_sha256: String,
    source_outer_checkpoint_sha256: String,
    collection_sha256: String,
    panel_sha256: String,
    root_states: usize,
    outer_states_per_epoch: usize,
    replay_root_states: usize,
    train_lower: bool,
    train_failed_policy: bool,
    outer_action_samples: usize,
    lower_route_samples: usize,
    epochs: usize,
    updates: usize,
    elapsed_seconds: f64,
    samples_per_minute: f64,
    initial_outer_loss: f64,
    final_outer_loss: f64,
    initial_lower_loss: f64,
    final_lower_loss: f64,
    maximum_gradient_norm: f64,
    lower_checkpoint_sha256: String,
    outer_checkpoint_sha256: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct RecoveryBinding {
    schema: String,
    config_sha256: String,
    panel_sha256: String,
    collection_sha256: String,
    catalog_sha256: String,
    source_lower_config_sha256: String,
    source_lower_parameter_order_sha256: String,
    source_lower_checkpoint_sha256: String,
    source_outer_config_sha256: String,
    source_outer_parameter_order_sha256: String,
    source_outer_checkpoint_sha256: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    replay_sources: Vec<ReplayIdentity>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoveryState {
    schema: String,
    binding_sha256: String,
    completed_epochs: usize,
    updates: usize,
    processed_samples: usize,
    training_seconds: f64,
    initial_outer_loss: Option<f64>,
    final_outer_loss: f64,
    initial_lower_loss: Option<f64>,
    final_lower_loss: f64,
    maximum_gradient_norm: f64,
    lower_sha256: String,
    outer_sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RecoveryPointer {
    schema: String,
    completed_epochs: usize,
}

const RECOVERY_BINDING_SCHEMA: &str = "aic-hierarchical-generation-recovery-binding-v1";
const RECOVERY_STATE_SCHEMA: &str = "aic-hierarchical-generation-recovery-state-v1";

pub fn run(
    config_path: &Path,
    panel_path: &Path,
    collection_path: &Path,
    lower_training_output: &Path,
    outer_training_output: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
    resume: bool,
) -> Result<Value> {
    validate_output_state(output, resume)?;
    let panel = load_published_panel(panel_path)?;
    let collection = load_published_report(collection_path)?;
    run_published(
        config_path,
        panel,
        collection,
        lower_training_output,
        outer_training_output,
        catalog_root,
        output,
        device,
        resume,
        &mut ReplayWindow::new(0),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_published(
    config_path: &Path,
    panel: Published<Panel>,
    collection: Published<CollectionReport>,
    lower_training_output: &Path,
    outer_training_output: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
    resume: bool,
    replay: &mut ReplayWindow,
) -> Result<Value> {
    validate_output_state(output, resume)?;
    let pending = pending_path(output)?;
    let config_bytes = bounded_read(config_path)?;
    let config: Config = serde_json::from_slice(&config_bytes)?;
    let Published {
        value: panel,
        sha256: panel_sha256,
        encoded_bytes: panel_encoded_bytes,
    } = panel;
    let Published {
        value: collection,
        sha256: collection_sha256,
        encoded_bytes: collection_encoded_bytes,
    } = collection;
    ensure!(
        panel_encoded_bytes > 0 && collection_encoded_bytes > 0,
        "aic-learning.hierarchical-generation-input: empty published panel or collection"
    );
    validate_config(&config)?;
    validate_inputs(&panel, &collection)?;
    ensure!(
        panel_sha256 == collection.panel_sha256,
        "aic-learning.hierarchical-generation-lineage: collection panel differs"
    );

    let lower_config_bytes = bounded_read(&lower_training_output.join("config.json"))?;
    let lower_order_bytes = bounded_read(&lower_training_output.join("parameter-order.json"))?;
    ensure!(
        bytes_sha256(&lower_config_bytes) == collection.lower_config_sha256
            && bytes_sha256(&lower_order_bytes) == collection.lower_parameter_order_sha256,
        "aic-learning.hierarchical-generation-lineage: lower model definition differs"
    );
    let lower_config: LowerTrainingConfig = serde_json::from_slice(&lower_config_bytes)?;
    let lower_order: LowerOrder = serde_json::from_slice(&lower_order_bytes)?;
    let lower_source = lower_training_output.join("state.safetensors");
    let lower_source_sha256 = crate::input::sha256(&lower_source)?;
    ensure!(
        lower_source_sha256 == collection.lower_checkpoint_sha256,
        "aic-learning.hierarchical-generation-lineage: lower collection model differs"
    );
    let mut lower_values: Tensors = Tensor::read_safetensors(&lower_source)?
        .into_iter()
        .collect();

    let outer_config_bytes = bounded_read(&outer_training_output.join("config.json"))?;
    let outer_order_bytes = bounded_read(&outer_training_output.join("parameter-order.json"))?;
    ensure!(
        bytes_sha256(&outer_config_bytes) == collection.outer_config_sha256
            && bytes_sha256(&outer_order_bytes) == collection.outer_parameter_order_sha256,
        "aic-learning.hierarchical-generation-lineage: outer model definition differs"
    );
    let outer_config: OuterTrainingConfig = serde_json::from_slice(&outer_config_bytes)?;
    ensure!(
        lower_config.learning_domain
            == (DomainConfig {
                placement: outer_config.search.bounds,
                extension_lengths: outer_config.search.extension_lengths.clone(),
            }),
        "aic-learning.hierarchical-generation-domain: search and lower model domains differ"
    );
    let outer_order: crate::outer_model::ParameterOrder =
        serde_json::from_slice(&outer_order_bytes)?;
    ensure!(
        semantic_sha256(&outer_config.search)? == collection.search_config_sha256,
        "aic-learning.hierarchical-generation-lineage: collection search config differs"
    );
    validate_collection_structure(&collection, &outer_config.search)?;
    let outer_source = outer_training_output.join("state.safetensors");
    let outer_source_sha256 = crate::input::sha256(&outer_source)?;
    ensure!(
        outer_source_sha256 == collection.outer_checkpoint_sha256,
        "aic-learning.hierarchical-generation-lineage: outer collection model differs"
    );
    let mut outer_values: Tensors = Tensor::read_safetensors(&outer_source)?
        .into_iter()
        .collect();

    let loaded = aic_layout_gen::io::load_catalogs(catalog_root)?;
    let catalogs = Arc::new(loaded.catalogs);
    crate::input::verify_training_artifact(
        lower_training_output,
        &catalogs.identity,
        &lower_source_sha256,
    )?;
    crate::input::verify_training_artifact(
        outer_training_output,
        &catalogs.identity,
        &outer_source_sha256,
    )?;
    ensure!(
        catalogs.identity == collection.catalog_sha256,
        "aic-learning.hierarchical-generation-catalog: collection catalog differs"
    );
    let domain = DomainConfig {
        placement: outer_config.search.bounds,
        extension_lengths: outer_config.search.extension_lengths.clone(),
    };
    let (outer_examples, lower_examples) = collect_examples(
        panel,
        collection,
        &domain,
        &outer_config,
        &lower_config,
        catalogs.clone(),
        &config,
        config.train_lower,
    )?;
    ensure!(
        !outer_examples.is_empty() && (!config.train_lower || !lower_examples.is_empty()),
        "aic-learning.hierarchical-generation-samples: collection has no usable roots or route choices"
    );
    let replay_examples = replay.examples();
    let replay_root_states = replay_examples.len();

    let binding = RecoveryBinding {
        schema: RECOVERY_BINDING_SCHEMA.into(),
        config_sha256: bytes_sha256(&config_bytes),
        panel_sha256: panel_sha256.clone(),
        collection_sha256: collection_sha256.clone(),
        catalog_sha256: catalogs.identity.clone(),
        source_lower_config_sha256: bytes_sha256(&lower_config_bytes),
        source_lower_parameter_order_sha256: bytes_sha256(&lower_order_bytes),
        source_lower_checkpoint_sha256: lower_source_sha256.clone(),
        source_outer_config_sha256: bytes_sha256(&outer_config_bytes),
        source_outer_parameter_order_sha256: bytes_sha256(&outer_order_bytes),
        source_outer_checkpoint_sha256: outer_source_sha256.clone(),
        replay_sources: replay.identities(),
    };
    let binding_bytes = serde_json::to_vec_pretty(&binding)?;
    let binding_sha256 = bytes_sha256(&binding_bytes);
    let resume = if resume {
        let retained: RecoveryBinding = read_json(&pending.join("recovery-binding.json"))?;
        ensure!(
            retained == binding,
            "aic-learning.hierarchical-generation-resume: inputs or source checkpoints changed"
        );
        if ensure_recovery_pointer(&pending)? {
            true
        } else {
            // No atomically published epoch exists. The pending files contain
            // no reusable learner state, so restart this generation from its
            // already-validated immutable inputs.
            fs::remove_dir_all(&pending)?;
            false
        }
    } else {
        false
    };
    let mut recovery = if resume {
        let (state, recovered_lower, recovered_outer) = load_recovery(&pending, &binding_sha256)?;
        lower_values = recovered_lower;
        outer_values = recovered_outer;
        state
    } else {
        fs::create_dir_all(pending.join("checkpoints"))?;
        fs::write(pending.join("recovery-binding.json"), &binding_bytes)?;
        RecoveryState {
            schema: RECOVERY_STATE_SCHEMA.into(),
            binding_sha256: binding_sha256.clone(),
            completed_epochs: 0,
            updates: 0,
            processed_samples: 0,
            training_seconds: 0.0,
            initial_outer_loss: None,
            final_outer_loss: 0.0,
            initial_lower_loss: (!config.train_lower).then_some(0.0),
            final_lower_loss: 0.0,
            maximum_gradient_norm: 0.0,
            lower_sha256: String::new(),
            outer_sha256: String::new(),
        }
    };
    ensure!(
        recovery.completed_epochs < config.epochs,
        "aic-learning.hierarchical-generation-resume: checkpoint already reached configured epochs"
    );
    let lower = LowerModel::load(&lower_config.model, &lower_order, &lower_values, device)?;
    let outer = OuterModel::load(&outer_config.model, &outer_order, &outer_values, device)?;

    let backend = if matches!(device, Device::Cuda(_)) {
        AdamBackend::Fused
    } else {
        AdamBackend::Foreach
    };
    let mut lower_optimizer = Adam::new(
        lower
            .parameters
            .iter()
            .map(|(name, tensor)| (name.clone(), tensor.shallow_clone()))
            .collect(),
        config.lower_learning_rate,
        0.9,
        0.999,
        1e-8,
        backend,
    )?;
    let mut outer_optimizer = Adam::new(
        outer
            .parameters
            .iter()
            .map(|(name, tensor)| (name.clone(), tensor.shallow_clone()))
            .collect(),
        config.outer_learning_rate,
        0.9,
        0.999,
        1e-8,
        backend,
    )?;
    if resume {
        restore_optimizer(&mut lower_optimizer, &lower_values, device)?;
        restore_optimizer(&mut outer_optimizer, &outer_values, device)?;
    }

    synchronize(device);
    let started = Instant::now();
    let initial_training_seconds = recovery.training_seconds;
    for epoch in recovery.completed_epochs..config.epochs {
        let order = outer_epoch_examples(
            &outer_examples,
            &replay_examples,
            config
                .outer_states_per_epoch
                .unwrap_or(outer_examples.len()),
            config.seed,
            epoch,
        );
        for examples in order.chunks(config.batch_states) {
            outer_optimizer.zero_grad()?;
            let loss = outer_loss(&outer, examples, config.train_failed_policy, device)?;
            let scalar = f64::try_from(&loss)?;
            ensure!(
                scalar.is_finite(),
                "aic-learning.hierarchical-generation-outer-loss: nonfinite"
            );
            recovery.initial_outer_loss.get_or_insert(scalar);
            recovery.final_outer_loss = scalar;
            loss.backward();
            recovery.maximum_gradient_norm = recovery
                .maximum_gradient_norm
                .max(outer_optimizer.clip_grad_norm(config.gradient_clip)?);
            outer_optimizer.step()?;
            recovery.updates += 1;
            recovery.processed_samples += examples
                .iter()
                .map(|example| example.features.len())
                .sum::<usize>();
        }

        if config.train_lower {
            let lower_rotation = (config.seed as usize).wrapping_add(epoch) % lower_examples.len();
            let order = (0..lower_examples.len())
                .map(|index| (index + lower_rotation) % lower_examples.len())
                .collect::<Vec<_>>();
            for indices in order.chunks(config.batch_states) {
                let examples = indices
                    .iter()
                    .map(|&index| &lower_examples[index])
                    .collect::<Vec<_>>();
                lower_optimizer.zero_grad()?;
                let loss = lower_loss(&lower, &examples, config.lower_model_temperature, device)?;
                let scalar = f64::try_from(&loss)?;
                ensure!(
                    scalar.is_finite(),
                    "aic-learning.hierarchical-generation-lower-loss: nonfinite"
                );
                recovery.initial_lower_loss.get_or_insert(scalar);
                recovery.final_lower_loss = scalar;
                loss.backward();
                recovery.maximum_gradient_norm = recovery
                    .maximum_gradient_norm
                    .max(lower_optimizer.clip_grad_norm(config.gradient_clip)?);
                lower_optimizer.step()?;
                recovery.updates += 1;
                recovery.processed_samples += examples
                    .iter()
                    .map(|example| example.candidates.len())
                    .sum::<usize>();
            }
        }
        recovery.completed_epochs = epoch + 1;
        recovery.training_seconds = initial_training_seconds + started.elapsed().as_secs_f64();
        if recovery.completed_epochs < config.epochs
            && recovery.completed_epochs % config.checkpoint_interval_epochs == 0
        {
            synchronize(device);
            save_recovery(
                &pending,
                &mut recovery,
                &lower,
                &mut lower_optimizer,
                &outer,
                &mut outer_optimizer,
            )?;
        }
    }
    synchronize(device);
    let elapsed_seconds = initial_training_seconds + started.elapsed().as_secs_f64();

    fs::create_dir_all(pending.join("lower"))?;
    fs::create_dir_all(pending.join("outer"))?;
    fs::copy(
        lower_training_output.join("config.json"),
        pending.join("lower/config.json"),
    )?;
    fs::copy(
        lower_training_output.join("parameter-order.json"),
        pending.join("lower/parameter-order.json"),
    )?;
    if config.train_lower {
        write_tensors(&pending.join("lower/state.safetensors"), lower.snapshot())?;
    } else {
        fs::copy(&lower_source, pending.join("lower/state.safetensors"))?;
    }
    fs::copy(
        outer_training_output.join("config.json"),
        pending.join("outer/config.json"),
    )?;
    fs::copy(
        outer_training_output.join("parameter-order.json"),
        pending.join("outer/parameter-order.json"),
    )?;
    write_tensors(&pending.join("outer/state.safetensors"), outer.snapshot())?;
    fs::write(
        pending.join("config.json"),
        serde_json::to_vec_pretty(&config)?,
    )?;
    let lower_checkpoint_sha256 = crate::input::sha256(&pending.join("lower/state.safetensors"))?;
    let outer_checkpoint_sha256 = crate::input::sha256(&pending.join("outer/state.safetensors"))?;
    let report = Report {
        schema: REPORT_SCHEMA,
        status: "complete",
        resumed: resume,
        device: format!("{device:?}"),
        catalog_sha256: catalogs.identity.clone(),
        source_lower_checkpoint_sha256: lower_source_sha256,
        source_outer_checkpoint_sha256: outer_source_sha256,
        collection_sha256,
        panel_sha256,
        root_states: outer_examples.len(),
        outer_states_per_epoch: config
            .outer_states_per_epoch
            .unwrap_or(outer_examples.len()),
        replay_root_states,
        train_lower: config.train_lower,
        train_failed_policy: config.train_failed_policy,
        outer_action_samples: outer_examples
            .iter()
            .map(|example| example.features.len())
            .sum(),
        lower_route_samples: lower_examples
            .iter()
            .map(|example| example.candidates.len())
            .sum(),
        epochs: config.epochs,
        updates: recovery.updates,
        elapsed_seconds,
        samples_per_minute: if elapsed_seconds == 0.0 {
            0.0
        } else {
            recovery.processed_samples as f64 * 60.0 / elapsed_seconds
        },
        initial_outer_loss: recovery.initial_outer_loss.unwrap_or(0.0),
        final_outer_loss: recovery.final_outer_loss,
        initial_lower_loss: recovery.initial_lower_loss.unwrap_or(0.0),
        final_lower_loss: recovery.final_lower_loss,
        maximum_gradient_norm: recovery.maximum_gradient_norm,
        lower_checkpoint_sha256,
        outer_checkpoint_sha256,
    };
    fs::write(
        pending.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::rename(&pending, output)?;
    drop(replay_examples);
    if replay.capacity > 0 {
        replay.push(ReplaySource {
            identity: ReplayIdentity {
                panel_sha256: report.panel_sha256.clone(),
                collection_sha256: report.collection_sha256.clone(),
            },
            examples: outer_examples
                .into_iter()
                .filter(|example| example.successful_episode)
                .collect(),
        });
    }
    Ok(json!({
        "schema": REPORT_SCHEMA,
        "status": "complete",
        "output": output,
        "root_states": report.root_states,
        "resumed": resume,
        "processed_samples": recovery.processed_samples,
        "samples_per_minute": report.samples_per_minute,
        "lower_checkpoint_sha256": report.lower_checkpoint_sha256,
        "outer_checkpoint_sha256": report.outer_checkpoint_sha256,
    }))
}

fn collect_examples(
    panel: Panel,
    collection: CollectionReport,
    domain: &DomainConfig,
    outer_config: &OuterTrainingConfig,
    lower_config: &LowerTrainingConfig,
    catalogs: Arc<Catalogs>,
    config: &Config,
    collect_lower: bool,
) -> Result<(Vec<OuterExample>, Vec<LowerExample>)> {
    let limits = panel.limits;
    let mut cases = panel
        .cases
        .into_iter()
        .map(|case| (case.id.clone(), case))
        .collect::<BTreeMap<_, _>>();
    let source_lower_checkpoint_sha256 = collection.lower_checkpoint_sha256;
    let mut outer_examples = Vec::new();
    let mut lower_examples = Vec::new();
    for collected in collection.cases {
        let case = cases.remove(&collected.id).context(
            "aic-learning.hierarchical-generation-case: collection case missing from panel",
        )?;
        let mut session = prefix_session(&case, limits, catalogs.clone())?;
        for record in collected.mcts.root_records {
            ensure!(
                state_sha256(&session)? == record.state_sha256
                    && session.observe().revision == record.state_revision,
                "aic-learning.hierarchical-generation-replay: root state differs in {}",
                case.id
            );
            ensure!(
                record.selected_action < record.actions.len() && !record.actions.is_empty(),
                "aic-learning.hierarchical-generation-record: invalid selected action in {}",
                case.id
            );
            let children = record
                .actions
                .iter()
                .map(|action| {
                    action.action.apply(
                        &session,
                        &outer_config.search.candidate,
                        outer_config.search.repair_max_removed,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            let features = record
                .actions
                .iter()
                .zip(&children)
                .map(|(action, child)| {
                    action
                        .action
                        .features(&session, child, outer_config.search.bounds)
                })
                .collect::<Vec<_>>();
            let entity_references = record
                .actions
                .iter()
                .map(|action| action.action.entity_index(session.problem()))
                .collect::<Result<Vec<_>>>()?;
            let target = visit_target(&record, config.search_temperature);
            let observation = session.observe();
            let completed = record.later_completed_connections;
            let total = record.later_total_connections.max(1);
            let additional = completed.saturating_sub(observation.completed);
            let remaining = total.saturating_sub(observation.completed).max(1);
            let area = outer_config.search.bounds.width() * outer_config.search.bounds.height();
            let quality = record.later_fully_validated.then(|| {
                normalized_layout_quality(
                    record.later_footprint_area,
                    record.later_transport_cells,
                    record.later_turns,
                    area,
                    total,
                )
            });
            let values = ValueTarget {
                completion: record.later_fully_validated as u8 as f32,
                additional_fraction: additional as f32 / remaining as f32,
                next_progress: (additional > 0) as u8 as f32,
                blocked_fraction: total.saturating_sub(completed) as f32 / total as f32,
                area: quality.map(|quality| quality[0]),
                transport: quality.map(|quality| quality[1]),
                turns: quality.map(|quality| quality[2]),
            };
            outer_examples.push(OuterExample {
                input: session
                    .macro_learning_input(domain)
                    .map_err(anyhow::Error::new)?,
                features,
                entity_references,
                target,
                values,
                successful_episode: record.later_fully_validated,
            });
            if collect_lower {
                collect_lower_examples(
                    &session,
                    &record,
                    domain,
                    lower_config,
                    &source_lower_checkpoint_sha256,
                    &outer_config.search.candidate,
                    config.search_temperature,
                    &mut lower_examples,
                )?;
            }
            session = children[record.selected_action].clone();
        }
    }
    Ok((outer_examples, lower_examples))
}

fn outer_epoch_examples<'a>(
    current: &'a [OuterExample],
    replay: &[&'a OuterExample],
    slots: usize,
    seed: u64,
    epoch: usize,
) -> Vec<&'a OuterExample> {
    let count = current.len();
    if replay.is_empty() {
        let rotation = if slots == count {
            (seed as usize).wrapping_add(epoch) % count
        } else {
            (seed as usize).wrapping_add(epoch.wrapping_mul(slots)) % count
        };
        return (0..slots)
            .map(|index| &current[(index + rotation) % count])
            .collect();
    }
    let replay_count = slots / 2;
    let current_count = slots - replay_count;
    let current_start = (seed as usize).wrapping_add(epoch.wrapping_mul(current_count)) % count;
    let replay_start =
        (seed as usize).wrapping_add(epoch.wrapping_mul(replay_count)) % replay.len();
    let mut examples = Vec::with_capacity(slots);
    for index in 0..current_count {
        examples.push(&current[(current_start + index) % count]);
        if index < replay_count {
            examples.push(replay[(replay_start + index) % replay.len()]);
        }
    }
    examples
}

fn collect_lower_examples(
    session: &Session,
    record: &RootRecord,
    domain: &DomainConfig,
    lower_config: &LowerTrainingConfig,
    source_lower_checkpoint_sha256: &str,
    candidate_config: &aic_pilot_native::constructive::routing_candidates::Config,
    temperature: f64,
    output: &mut Vec<LowerExample>,
) -> Result<()> {
    let mut groups = BTreeMap::<(String, String), LowerGroup>::new();
    for (index, action) in record.actions.iter().enumerate() {
        match &action.action {
            MacroAction::Route(candidate) => {
                add_lower_candidate(
                    session,
                    index,
                    candidate,
                    lower_config.model.heads,
                    source_lower_checkpoint_sha256,
                    &mut groups,
                )?;
            }
            MacroAction::Network(candidate) => {
                let mut intermediate = session.clone();
                for route in &candidate.routes {
                    add_lower_candidate(
                        &intermediate,
                        index,
                        route,
                        lower_config.model.heads,
                        source_lower_checkpoint_sha256,
                        &mut groups,
                    )?;
                    intermediate
                        .apply_route_candidate(route)
                        .map_err(anyhow::Error::new)?;
                }
            }
            MacroAction::Repair { request, candidate } => {
                let mut allowance =
                    aic_pilot_native::constructive::routing_candidates::RequestAllowance::new(
                        request.allowance_caps.clone(),
                    );
                let (mut intermediate, reserved_bytes) = session
                    .prepare_repair_draft_with_allowance(
                        request,
                        &candidate.removed,
                        &mut allowance,
                    )
                    .map_err(anyhow::Error::new)?;
                allowance.release_peak(reserved_bytes);
                for route in &candidate.replacements {
                    add_lower_candidate(
                        &intermediate,
                        index,
                        route,
                        lower_config.model.heads,
                        source_lower_checkpoint_sha256,
                        &mut groups,
                    )?;
                    intermediate
                        .apply_route_candidate(route)
                        .map_err(anyhow::Error::new)?;
                }
            }
            MacroAction::Command(_) => {}
        }
    }
    for ((_, connection), group) in groups {
        let request = aic_pilot_native::constructive::routing_candidates::Request {
            state_revision: group.session.observe().revision,
            connection: connection.to_owned(),
        };
        let input = group
            .session
            .route_guidance_input(&request, candidate_config, domain)
            .map_err(anyhow::Error::new)?;
        let q_max = group
            .candidates
            .iter()
            .map(|(index, _, _)| record.actions[*index].q)
            .fold(f64::NEG_INFINITY, f64::max);
        // Search values reserve [0, 1) for incomplete progress and [1, 2]
        // for validated completion plus layout quality. Keep the lower-model
        // regret target in its historical [0, 1] range.
        let q_scale = 2.0;
        let candidates = group
            .candidates
            .iter()
            .map(|(index, candidate, head)| {
                Ok(LowerCandidate {
                    edge_offsets: edge_offsets(candidate, input.bounds),
                    head: *head,
                    q: ((q_max - record.actions[*index].q).max(0.0) / q_scale).min(1.0) as f32,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let weights = group
            .candidates
            .iter()
            .map(|(index, _, _)| {
                search_weight(
                    record.actions[*index].visits,
                    record.actions[*index].prior,
                    temperature,
                )
            })
            .collect::<Vec<_>>();
        let denominator = weights.iter().sum::<f64>();
        if denominator == 0.0 && group.candidates.len() > 1 {
            continue;
        }
        let target = if denominator == 0.0 {
            vec![1.0]
        } else {
            weights
                .into_iter()
                .map(|weight| (weight / denominator) as f32)
                .collect()
        };
        output.push(LowerExample {
            input,
            candidates,
            target,
            success: record.later_fully_validated as u8 as f32,
        });
    }
    Ok(())
}

fn add_lower_candidate(
    session: &Session,
    action: usize,
    candidate: &Candidate,
    heads: usize,
    source_lower_checkpoint_sha256: &str,
    groups: &mut BTreeMap<(String, String), LowerGroup>,
) -> Result<()> {
    let head = match &candidate.provenance {
        Provenance::Guided {
            model_version,
            head,
        }
        | Provenance::Diversified {
            model_version: Some(model_version),
            head: Some(head),
            ..
        } => {
            ensure!(
                model_version == source_lower_checkpoint_sha256,
                "aic-learning.hierarchical-generation-model-version: route provenance differs from collection checkpoint"
            );
            let head = head
                .strip_prefix("head-")
                .and_then(|head| head.parse::<usize>().ok())
                .context("aic-learning.hierarchical-generation-head: malformed guidance head")?;
            ensure!(
                head < heads,
                "aic-learning.hierarchical-generation-head: guidance head is outside the model"
            );
            head as i64
        }
        Provenance::Anchor
        | Provenance::Diversified {
            model_version: None,
            ..
        }
        | Provenance::Diversified { head: None, .. } => return Ok(()),
    };
    ensure!(
        candidate.state_revision == session.observe().revision,
        "aic-learning.hierarchical-generation-route-state: candidate belongs to a different intermediate state"
    );
    let key = (state_sha256(session)?, candidate.connection.clone());
    groups
        .entry(key)
        .or_insert_with(|| LowerGroup {
            session: session.clone(),
            candidates: Vec::new(),
        })
        .candidates
        .push((action, candidate.clone(), head));
    Ok(())
}

fn edge_offsets(candidate: &Candidate, bounds: aic_layout::geometry::Bounds) -> Vec<i64> {
    let area = bounds.width() * bounds.height();
    candidate
        .cells
        .windows(2)
        .map(|edge| {
            let direction = direction(edge[0], edge[1]);
            let cell = (edge[1].y - bounds.min_y) * bounds.width() + edge[1].x - bounds.min_x;
            direction * area + cell
        })
        .collect()
}

fn direction(from: Point, to: Point) -> i64 {
    match (to.x - from.x, to.y - from.y) {
        (1, 0) => 0,
        (0, 1) => 1,
        (-1, 0) => 2,
        (0, -1) => 3,
        _ => unreachable!("candidate cells are cardinal neighbors"),
    }
}

fn visit_target(record: &RootRecord, temperature: f64) -> Vec<f32> {
    if record
        .actions
        .iter()
        .all(|action| action.policy_target.is_some())
    {
        let targets = record
            .actions
            .iter()
            .map(|action| action.policy_target.unwrap())
            .collect::<Vec<_>>();
        let denominator = targets.iter().sum::<f64>();
        return targets
            .into_iter()
            .map(|target| (target / denominator.max(f64::MIN_POSITIVE)) as f32)
            .collect();
    }
    let weights = record
        .actions
        .iter()
        .map(|action| search_weight(action.visits, action.prior, temperature))
        .collect::<Vec<_>>();
    let denominator = weights.iter().sum::<f64>();
    weights
        .into_iter()
        .map(|weight| (weight / denominator.max(f64::MIN_POSITIVE)) as f32)
        .collect()
}

fn search_weight(visits: u32, _prior: f64, temperature: f64) -> f64 {
    if visits == 0 {
        0.0
    } else {
        f64::from(visits).powf(1.0 / temperature)
    }
}

fn outer_loss(
    model: &OuterModel,
    examples: &[&OuterExample],
    train_failed_policy: bool,
    device: Device,
) -> Result<Tensor> {
    let inputs = examples
        .iter()
        .map(|example| example.input.as_ref())
        .collect::<Vec<_>>();
    let features = examples
        .iter()
        .map(|example| example.features.as_slice())
        .collect::<Vec<_>>();
    let entity_references = examples
        .iter()
        .map(|example| example.entity_references.as_slice())
        .collect::<Vec<_>>();
    let batch = OuterBatch::from_rows(&inputs, &features, &entity_references, device)?;
    let prediction = model.forward(&batch)?;
    let mut policies = Vec::with_capacity(examples.len());
    for (state, example) in examples.iter().enumerate() {
        let range = batch.action_range(state);
        let target = Tensor::from_slice(&example.target).to_device(device);
        if train_failed_policy || example.successful_episode {
            policies.push(
                -(target
                    * prediction
                        .logp
                        .narrow(0, range.start, range.end - range.start))
                .sum(Kind::Float),
            );
        }
    }
    let targets = examples
        .iter()
        .map(|example| example.values)
        .collect::<Vec<_>>();
    let policy = if policies.is_empty() {
        Tensor::zeros([], (Kind::Float, device))
    } else {
        Tensor::stack(&policies, 0).sum(Kind::Float) / examples.len() as f64
    };
    Ok(policy + value_loss(&prediction.values, &targets, device)?)
}

fn lower_loss(
    model: &LowerModel,
    examples: &[&LowerExample],
    temperature: f64,
    device: Device,
) -> Result<Tensor> {
    let inputs = examples
        .iter()
        .map(|example| &example.input)
        .collect::<Vec<_>>();
    let batch = LowerBatch::from_inputs(&inputs, device)?;
    let prediction = model.forward(&batch)?;
    lower_prediction_loss(&prediction, &batch, examples, temperature, device)
}

fn lower_prediction_loss(
    prediction: &LowerPrediction,
    batch: &LowerBatch,
    examples: &[&LowerExample],
    temperature: f64,
    device: Device,
) -> Result<Tensor> {
    let area = batch.dimensions().0 * batch.dimensions().1;
    let heads = prediction.directed_costs.size()[1];
    let flat = prediction.directed_costs.view([-1]);
    let mut policies = Vec::new();
    let mut regression = Vec::new();
    for (state, example) in examples.iter().enumerate() {
        let energies = example
            .candidates
            .iter()
            .map(|candidate| {
                let base = (state as i64 * heads + candidate.head) * 4 * area;
                let indices = Tensor::from_slice(
                    &candidate
                        .edge_offsets
                        .iter()
                        .map(|offset| base + offset)
                        .collect::<Vec<_>>(),
                )
                .to_device(device);
                let energy = flat.index_select(0, &indices).sum(Kind::Float);
                regression.push((&energy - f64::from(candidate.q) * 4.0).square());
                energy
            })
            .collect::<Vec<_>>();
        let logp = (-Tensor::stack(&energies, 0) / temperature).log_softmax(-1, Kind::Float);
        policies
            .push(-(Tensor::from_slice(&example.target).to_device(device) * logp).sum(Kind::Float));
    }
    let policy = Tensor::stack(&policies, 0).mean(Kind::Float);
    let regression = Tensor::stack(&regression, 0).mean(Kind::Float);
    let target = Tensor::from_slice(
        &examples
            .iter()
            .map(|example| example.success)
            .collect::<Vec<_>>(),
    )
    .to_device(device)
    .unsqueeze(-1)
    .expand([-1, heads], false);
    let success = prediction.success.clamp(1e-5, 1.0 - 1e-5);
    let outcome = -(&target * success.log() + (1.0f64 - &target) * (1.0f64 - success).log())
        .mean(Kind::Float);
    let uncertainty_target = (prediction.success.detach() - &target).abs();
    let uncertainty = (&prediction.uncertainty - uncertainty_target)
        .square()
        .mean(Kind::Float);
    Ok(policy + regression * 0.05 + outcome + uncertainty * 0.1)
}

fn prefix_session(case: &Case, limits: Limits, catalogs: Arc<Catalogs>) -> Result<Session> {
    let mut session =
        Session::new(case.problem.clone(), catalogs, limits).map_err(anyhow::Error::new)?;
    for (index, command) in case.prefix.iter().enumerate() {
        let step = session.step(command.clone());
        ensure!(
            step.accepted && step.diagnostic.is_none(),
            "aic-learning.hierarchical-generation-prefix: {} command {} rejected",
            case.id,
            index
        );
    }
    Ok(session)
}

fn validate_config(config: &Config) -> Result<()> {
    ensure!(
        config.schema == CONFIG_SCHEMA
            && (1..=1_000_000).contains(&config.epochs)
            && (1..=1_000_000).contains(&config.checkpoint_interval_epochs)
            && (1..=4096).contains(&config.batch_states)
            && config.search_temperature.is_finite()
            && config.search_temperature > 0.0
            && config.lower_model_temperature.is_finite()
            && config.lower_model_temperature > 0.0
            && config.lower_learning_rate.is_finite()
            && config.lower_learning_rate > 0.0
            && config.outer_learning_rate.is_finite()
            && config.outer_learning_rate > 0.0
            && config.gradient_clip.is_finite()
            && config.gradient_clip > 0.0
            && config.seed <= i64::MAX as u64
            && config
                .outer_states_per_epoch
                .is_none_or(|states| (1..=4096).contains(&states)),
        "aic-learning.hierarchical-generation-config: invalid training controls"
    );
    Ok(())
}

fn validate_inputs(panel: &Panel, collection: &CollectionReport) -> Result<()> {
    let panel_ids = panel
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let collection_ids = collection
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    ensure!(
        panel.schema == PANEL_SCHEMA
            && panel.partition == "training"
            && panel.partition == collection.partition
            && collection.schema == SEARCH_REPORT_SCHEMA
            && collection.root_records_retained
            && !panel.cases.is_empty()
            && panel.cases.len() == panel_ids.len()
            && panel_ids == collection_ids
            && collection
                .cases
                .iter()
                .all(|case| !case.mcts.root_records.is_empty()),
        "aic-learning.hierarchical-generation-input: panel and collection differ or contain no roots"
    );
    Ok(())
}

fn validate_collection_structure(
    collection: &CollectionReport,
    search: &crate::hierarchical_search::Config,
) -> Result<()> {
    ensure!(
        collection.cases.iter().all(|case| {
            case.mcts.root_records.len() <= search.max_moves
                && case.mcts.root_records.iter().all(|record| {
                    !record.actions.is_empty() && record.selected_action < record.actions.len()
                })
        }),
        "aic-learning.hierarchical-generation-collection: roots or actions exceed the search contract"
    );
    Ok(())
}

fn write_tensors(path: &Path, tensors: Tensors) -> Result<()> {
    Tensor::write_safetensors(&tensors.iter().collect::<Vec<_>>(), path)?;
    Ok(())
}

fn training_snapshot(mut values: Tensors, optimizer: &mut Adam) -> Result<Tensors> {
    for (name, state) in optimizer.snapshot()? {
        let prefix = format!("adam.{name}");
        values.insert(
            format!("{prefix}.step"),
            Tensor::from_slice(&[state.as_ref().map_or(0, |state| state.step)]),
        );
        if let Some(state) = state {
            values.insert(
                format!("{prefix}.mean"),
                state.mean.to_device(Device::Cpu).copy(),
            );
            values.insert(
                format!("{prefix}.square"),
                state.square.to_device(Device::Cpu).copy(),
            );
        }
    }
    Ok(values)
}

fn restore_optimizer(optimizer: &mut Adam, values: &Tensors, device: Device) -> Result<()> {
    let names = optimizer
        .named_parameters()
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    for (index, name) in names.iter().enumerate() {
        let prefix = format!("adam.{name}");
        let step = values
            .get(&format!("{prefix}.step"))
            .with_context(|| format!("missing Adam step for {name}"))?;
        ensure!(
            step.kind() == Kind::Int64 && step.size() == [1],
            "aic-learning.hierarchical-generation-resume: invalid Adam step for {name}"
        );
        let step = i64::try_from(step)?;
        ensure!(
            step >= 0,
            "aic-learning.hierarchical-generation-resume: negative Adam step for {name}"
        );
        if step > 0 {
            let mean = values
                .get(&format!("{prefix}.mean"))
                .with_context(|| format!("missing Adam mean for {name}"))?;
            let square = values
                .get(&format!("{prefix}.square"))
                .with_context(|| format!("missing Adam square for {name}"))?;
            ensure!(
                mean.size() == optimizer.named_parameters()[index].1.size()
                    && square.size() == optimizer.named_parameters()[index].1.size(),
                "aic-learning.hierarchical-generation-resume: invalid Adam shape for {name}"
            );
            optimizer.restore_parameter(
                index,
                &AdamState {
                    step,
                    mean: mean.to_device(device),
                    square: square.to_device(device),
                },
            )?;
        } else {
            ensure!(
                !values.contains_key(&format!("{prefix}.mean"))
                    && !values.contains_key(&format!("{prefix}.square")),
                "aic-learning.hierarchical-generation-resume: Adam moments without a step for {name}"
            );
        }
    }
    Ok(())
}

fn save_recovery(
    pending: &Path,
    state: &mut RecoveryState,
    lower: &LowerModel,
    lower_optimizer: &mut Adam,
    outer: &OuterModel,
    outer_optimizer: &mut Adam,
) -> Result<()> {
    let checkpoints = pending.join("checkpoints");
    let final_path = checkpoints.join(format!("{:08}", state.completed_epochs));
    let stage = final_path.with_extension("pending");
    if stage.exists() {
        fs::remove_dir_all(&stage)?;
    }
    ensure!(
        !final_path.exists(),
        "aic-learning.hierarchical-generation-checkpoint: {} already exists",
        final_path.display()
    );
    fs::create_dir(&stage)?;
    write_tensors(
        &stage.join("lower.safetensors"),
        training_snapshot(lower.snapshot(), lower_optimizer)?,
    )?;
    write_tensors(
        &stage.join("outer.safetensors"),
        training_snapshot(outer.snapshot(), outer_optimizer)?,
    )?;
    state.lower_sha256 = crate::input::sha256(&stage.join("lower.safetensors"))?;
    state.outer_sha256 = crate::input::sha256(&stage.join("outer.safetensors"))?;
    fs::write(stage.join("state.json"), serde_json::to_vec_pretty(state)?)?;
    fs::rename(&stage, &final_path)?;
    let pointer = RecoveryPointer {
        schema: RECOVERY_STATE_SCHEMA.into(),
        completed_epochs: state.completed_epochs,
    };
    let pointer_stage = pending.join("latest.pending");
    fs::write(&pointer_stage, serde_json::to_vec_pretty(&pointer)?)?;
    fs::rename(&pointer_stage, pending.join("latest.json"))?;
    for entry in fs::read_dir(&checkpoints)? {
        let entry = entry?;
        if entry.path() != final_path {
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

fn load_recovery(
    pending: &Path,
    binding_sha256: &str,
) -> Result<(RecoveryState, Tensors, Tensors)> {
    let pointer: RecoveryPointer = read_json(&pending.join("latest.json"))?;
    ensure!(
        pointer.schema == RECOVERY_STATE_SCHEMA && pointer.completed_epochs > 0,
        "aic-learning.hierarchical-generation-resume: invalid latest checkpoint pointer"
    );
    let checkpoint = pending
        .join("checkpoints")
        .join(format!("{:08}", pointer.completed_epochs));
    let state: RecoveryState = read_json(&checkpoint.join("state.json"))?;
    ensure!(
        state.schema == RECOVERY_STATE_SCHEMA
            && state.binding_sha256 == binding_sha256
            && state.completed_epochs == pointer.completed_epochs
            && state.updates > 0
            && state.processed_samples > 0
            && state.training_seconds.is_finite()
            && state.training_seconds >= 0.0
            && state.initial_outer_loss.is_some_and(f64::is_finite)
            && state.final_outer_loss.is_finite()
            && state.initial_lower_loss.is_some_and(f64::is_finite)
            && state.final_lower_loss.is_finite()
            && state.maximum_gradient_norm.is_finite()
            && state.maximum_gradient_norm >= 0.0,
        "aic-learning.hierarchical-generation-resume: invalid recovery state"
    );
    ensure!(
        crate::input::sha256(&checkpoint.join("lower.safetensors"))? == state.lower_sha256
            && crate::input::sha256(&checkpoint.join("outer.safetensors"))? == state.outer_sha256,
        "aic-learning.hierarchical-generation-resume: checkpoint hash differs"
    );
    let lower = Tensor::read_safetensors(checkpoint.join("lower.safetensors"))?
        .into_iter()
        .collect();
    let outer = Tensor::read_safetensors(checkpoint.join("outer.safetensors"))?
        .into_iter()
        .collect();
    for entry in fs::read_dir(pending.join("checkpoints"))? {
        let entry = entry?;
        if entry.path() != checkpoint {
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }
    }
    let pointer_stage = pending.join("latest.pending");
    if pointer_stage.exists() {
        fs::remove_file(pointer_stage)?;
    }
    Ok((state, lower, outer))
}

fn ensure_recovery_pointer(pending: &Path) -> Result<bool> {
    if pending.join("latest.json").is_file() {
        return Ok(true);
    }
    let checkpoints = pending.join("checkpoints");
    if !checkpoints.is_dir() {
        return Ok(false);
    }
    let latest = fs::read_dir(&checkpoints)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let complete = entry.path().join("state.json").is_file()
                && entry.path().join("lower.safetensors").is_file()
                && entry.path().join("outer.safetensors").is_file();
            complete
                .then(|| {
                    entry
                        .file_name()
                        .to_str()
                        .and_then(|name| name.parse::<usize>().ok())
                })
                .flatten()
                .filter(|epoch| *epoch > 0)
        })
        .max();
    let Some(completed_epochs) = latest else {
        return Ok(false);
    };
    let pointer = RecoveryPointer {
        schema: RECOVERY_STATE_SCHEMA.into(),
        completed_epochs,
    };
    let stage = pending.join("latest.pending");
    fs::write(&stage, serde_json::to_vec_pretty(&pointer)?)?;
    fs::rename(stage, pending.join("latest.json"))?;
    Ok(true)
}

fn synchronize(device: Device) {
    if let Device::Cuda(index) = device {
        tch::Cuda::synchronize(index as i64);
    }
}

fn state_sha256(session: &Session) -> Result<String> {
    Ok(bytes_sha256(&serde_json::to_vec(&(
        session.problem(),
        session.observe(),
    ))?))
}

fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    ensure!(
        metadata.len() <= MAX_INPUT_BYTES,
        "aic-learning.hierarchical-generation-input: {} exceeds {} bytes",
        path.display(),
        MAX_INPUT_BYTES
    );
    fs::read(path).with_context(|| format!("read {}", path.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    serde_json::from_slice(&bounded_read(path)?)
        .with_context(|| format!("parse {}", path.display()))
}

fn validate_output_state(output: &Path, resume: bool) -> Result<()> {
    ensure!(
        !output.exists(),
        "aic-learning.hierarchical-generation-output: {} already exists",
        output.display()
    );
    let pending = pending_path(output)?;
    ensure!(
        resume == pending.exists(),
        "aic-learning.hierarchical-generation-output: fresh training requires no pending output; --resume requires {}",
        pending.display()
    );
    Ok(())
}

fn pending_path(output: &Path) -> Result<PathBuf> {
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .context("hierarchical generation output needs a UTF-8 name")?;
    Ok(output.with_file_name(format!(".{name}.pending")))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hierarchical_search::{RootActionRecord, SearchTermination};

    #[test]
    fn distilled_edge_directions_match_native_channel_order() {
        let origin = Point { x: 4, y: 7 };
        assert_eq!(direction(origin, Point { x: 5, y: 7 }), 0);
        assert_eq!(direction(origin, Point { x: 4, y: 8 }), 1);
        assert_eq!(direction(origin, Point { x: 3, y: 7 }), 2);
        assert_eq!(direction(origin, Point { x: 4, y: 6 }), 3);
    }

    #[test]
    fn gumbel_policy_target_replaces_allocator_visit_counts() {
        let actions = [(100, 0.2), (1, 0.8)]
            .into_iter()
            .map(|(visits, policy_target)| RootActionRecord {
                action: MacroAction::Command(Command::Connect),
                prior: 0.5,
                anchor_prior: 0.5,
                visits,
                q: 0.0,
                policy_target: Some(policy_target),
            })
            .collect();
        let record = RootRecord {
            state_sha256: "state".into(),
            state_revision: 0,
            actions,
            selected_action: 1,
            later_completed_connections: 0,
            later_total_connections: 1,
            later_footprint_area: 0,
            later_transport_cells: 0,
            later_turns: 0,
            later_fully_validated: false,
            later_termination: SearchTermination::MoveBudgetExhausted,
        };
        assert_eq!(visit_target(&record, 0.25), vec![0.2, 0.8]);
    }

    #[test]
    fn prior_generation_config_gets_a_bounded_checkpoint_default() -> Result<()> {
        let config: Config = serde_json::from_value(json!({
            "schema": CONFIG_SCHEMA,
            "epochs": 200,
            "batch_states": 8,
            "search_temperature": 1.0,
            "lower_model_temperature": 4.0,
            "lower_learning_rate": 0.0001,
            "outer_learning_rate": 0.0001,
            "gradient_clip": 1.0,
            "seed": 1
        }))?;
        assert_eq!(config.checkpoint_interval_epochs, 50);
        validate_config(&config)
    }

    #[test]
    fn recovery_pointer_distinguishes_precheckpoint_and_published_state() {
        let root = std::env::temp_dir().join(format!(
            "aic-hierarchical-recovery-pointer-{}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        fs::create_dir_all(root.join("checkpoints")).unwrap();
        assert!(!ensure_recovery_pointer(&root).unwrap());
        let checkpoint = root.join("checkpoints/00000020");
        fs::create_dir(&checkpoint).unwrap();
        for name in ["state.json", "lower.safetensors", "outer.safetensors"] {
            fs::write(checkpoint.join(name), []).unwrap();
        }
        assert!(ensure_recovery_pointer(&root).unwrap());
        let pointer: RecoveryPointer = read_json(&root.join("latest.json")).unwrap();
        assert_eq!(pointer.completed_epochs, 20);
        fs::remove_dir_all(root).unwrap();
    }
}
