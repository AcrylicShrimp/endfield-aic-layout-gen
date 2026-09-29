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
    routing_candidates::{Candidate, GuidanceInput, Provenance, SearchMemoryBudget},
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
        Case, DenseRootRecord, MacroAction, PANEL_SCHEMA, Panel, Published,
        REPORT_SCHEMA as SEARCH_REPORT_SCHEMA, Report as CollectionReport, RootRecord,
        RootRecordTarget, RootSelection, SearchTermination, catalog_effective_config_sha256,
        catalog_limits_sha256, load_published_panel, load_published_report,
        root_catalog::{
            publication::CatalogBinding,
            replay::{RootReplaySource, RootReplayStart, RootReplayStep, VerifiedRootLedger},
        },
    },
    input::Tensors,
    outer_model::{
        ACTION_FEATURES, Batch as OuterBatch, Model as OuterModel, StreamedPolicyRoot,
        StreamedPolicyStatistics, ValueTarget, normalized_layout_quality, value_loss,
    },
    outer_training::Config as OuterTrainingConfig,
    route_guidance::{
        Batch as LowerBatch, Model as LowerModel, ParameterOrder as LowerOrder,
        Prediction as LowerPrediction,
    },
    route_guidance_training::Config as LowerTrainingConfig,
};

const CONFIG_SCHEMA: &str = "aic-hierarchical-generation-training-v1";
pub(crate) const REPORT_SCHEMA: &str = "aic-hierarchical-generation-training-report-v4";
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
    policy: OuterPolicy,
    values: ValueTarget,
    successful_episode: bool,
}

enum OuterPolicy {
    Dense {
        features: Vec<[f32; ACTION_FEATURES]>,
        entity_references: Vec<i64>,
        target: Vec<f32>,
    },
    Catalog {
        source: Arc<Session>,
        ledger: VerifiedRootLedger,
        domain: DomainConfig,
        page_size: usize,
        anchor_policy_mass: f64,
        max_live_bytes: usize,
    },
}

impl OuterPolicy {
    fn action_count(&self) -> usize {
        match self {
            Self::Dense { features, .. } => features.len(),
            Self::Catalog { ledger, .. } => ledger.descriptor().policy_action_count(),
        }
    }
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
        ensure!(
            panel.sha256 == collection.value.panel_sha256,
            "aic-learning.hierarchical-replay-lineage: {} differs from collection panel identity in {}",
            panel_path.display(),
            collection_path.display()
        );
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
            collection_path,
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
    policy_supervised: bool,
    success: f32,
}

struct LowerGroup {
    session: Session,
    candidates: Vec<(LowerRootScore, Candidate, Vec<i64>)>,
}

#[derive(Clone, Copy)]
struct LowerRootScore {
    visits: u32,
    q: f64,
    prior: f64,
}

struct LowerRootWitness<'a> {
    action: &'a MacroAction,
    score: LowerRootScore,
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

const RECOVERY_BINDING_SCHEMA: &str = "aic-hierarchical-generation-recovery-binding-v2";
const RECOVERY_STATE_SCHEMA: &str = "aic-hierarchical-generation-recovery-state-v2";

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
        collection_path,
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
    collection_path: &Path,
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
        collection_path,
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
            let scalar = outer_backward(&outer, examples, config.train_failed_policy, device)?;
            ensure!(
                scalar.is_finite(),
                "aic-learning.hierarchical-generation-outer-loss: nonfinite"
            );
            recovery.initial_outer_loss.get_or_insert(scalar);
            recovery.final_outer_loss = scalar;
            recovery.maximum_gradient_norm = recovery
                .maximum_gradient_norm
                .max(outer_optimizer.clip_grad_norm(config.gradient_clip)?);
            outer_optimizer.step()?;
            recovery.updates += 1;
            recovery.processed_samples += examples
                .iter()
                .map(|example| example.policy.action_count())
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
            .map(|example| example.policy.action_count())
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
    collection_path: &Path,
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
    let limits_sha256 = catalog_limits_sha256(&limits)?;
    let effective_config_sha256 = catalog_effective_config_sha256(
        &outer_config.search,
        &collection.lower_config_sha256,
        &collection.lower_parameter_order_sha256,
        &collection.lower_checkpoint_sha256,
        &collection.outer_config_sha256,
        &collection.outer_parameter_order_sha256,
        &collection.outer_checkpoint_sha256,
    )?;
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
                    && session.revision() == record.state_revision,
                "aic-learning.hierarchical-generation-replay: root state differs in {}",
                case.id
            );
            let (policy, selected_child) = match &record.target {
                RootRecordTarget::PuctDense(dense) => dense_outer_policy(
                    dense,
                    &session,
                    &outer_config.search,
                    config.search_temperature,
                )?,
                #[cfg(test)]
                RootRecordTarget::GumbelDenseOracle(dense) => dense_outer_policy(
                    dense,
                    &session,
                    &outer_config.search,
                    config.search_temperature,
                )?,
                RootRecordTarget::GumbelCatalog { descriptor } => {
                    let expected = CatalogBinding {
                        source_sha256: record.state_sha256.clone(),
                        source_revision: record.state_revision,
                        limits_sha256: limits_sha256.clone(),
                        effective_config_sha256: effective_config_sha256.clone(),
                        catalog_sha256: collection.catalog_sha256.clone(),
                        outer_checkpoint_sha256: collection.outer_checkpoint_sha256.clone(),
                    };
                    let source = Arc::new(session.clone());
                    let ledger = VerifiedRootLedger::open(
                        collection_path,
                        Arc::clone(descriptor),
                        &expected,
                        source.as_ref(),
                    )?;
                    let RootSelection::GumbelSequentialHalving {
                        anchor_policy_mass, ..
                    } = outer_config.search.root_selection
                    else {
                        anyhow::bail!(
                            "aic-learning.hierarchical-generation-catalog: search selection differs from catalog record"
                        )
                    };
                    let selected_child = descriptor.selected_action().apply(
                        &session,
                        &outer_config.search.candidate,
                        outer_config.search.repair_max_removed,
                    )?;
                    (
                        OuterPolicy::Catalog {
                            source,
                            ledger,
                            domain: domain.clone(),
                            page_size: outer_config.search.placement_page,
                            anchor_policy_mass,
                            max_live_bytes: outer_config.search.max_live_search_bytes,
                        },
                        selected_child,
                    )
                }
            };
            let (completed_before, _) = session.connection_progress_counts();
            let completed = record.later_completed_connections;
            let total = record.later_total_connections.max(1);
            let additional = completed.saturating_sub(completed_before);
            let remaining = total.saturating_sub(completed_before).max(1);
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
                policy,
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
            session = selected_child;
        }
    }
    Ok((outer_examples, lower_examples))
}

fn dense_outer_policy(
    dense: &DenseRootRecord,
    session: &Session,
    search: &crate::hierarchical_search::Config,
    temperature: f64,
) -> Result<(OuterPolicy, Session)> {
    ensure!(
        dense.selected_action < dense.actions.len() && !dense.actions.is_empty(),
        "aic-learning.hierarchical-generation-record: invalid dense selected action"
    );
    let mut features = Vec::with_capacity(dense.actions.len());
    let mut selected_child = None;
    for (index, action) in dense.actions.iter().enumerate() {
        let child = action
            .action
            .apply(session, &search.candidate, search.repair_max_removed)?;
        features.push(action.action.features(session, &child, search.bounds));
        if index == dense.selected_action {
            selected_child = Some(child);
        }
    }
    let entity_references = dense
        .actions
        .iter()
        .map(|action| action.action.entity_index(session.problem()))
        .collect::<Result<Vec<_>>>()?;
    Ok((
        OuterPolicy::Dense {
            features,
            entity_references,
            target: visit_target(dense, temperature),
        },
        selected_child.expect("selected action validated above"),
    ))
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
    let witnesses = match &record.target {
        RootRecordTarget::PuctDense(dense) => dense
            .actions
            .iter()
            .map(|action| LowerRootWitness {
                action: &action.action,
                score: LowerRootScore {
                    visits: action.visits,
                    q: action.q,
                    prior: action.prior,
                },
            })
            .collect::<Vec<_>>(),
        #[cfg(test)]
        RootRecordTarget::GumbelDenseOracle(dense) => dense
            .actions
            .iter()
            .map(|action| LowerRootWitness {
                action: &action.action,
                score: LowerRootScore {
                    visits: action.visits,
                    q: action.q,
                    prior: action.prior,
                },
            })
            .collect::<Vec<_>>(),
        RootRecordTarget::GumbelCatalog { descriptor } => descriptor
            .lower_witnesses()
            .map(|witness| LowerRootWitness {
                action: witness.action,
                score: LowerRootScore {
                    visits: witness.visits,
                    q: witness.q,
                    prior: 0.0,
                },
            })
            .collect::<Vec<_>>(),
    };
    let mut groups = BTreeMap::<(String, String), LowerGroup>::new();
    for witness in witnesses {
        match witness.action {
            MacroAction::Route(candidate) => {
                add_lower_candidate(
                    session,
                    witness.score,
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
                        witness.score,
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
                        witness.score,
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
            .map(|(score, _, _)| score.q)
            .fold(f64::NEG_INFINITY, f64::max);
        // Search values reserve [0, 1) for incomplete progress and [1, 2]
        // for validated completion plus layout quality. Keep the lower-model
        // regret target in its historical [0, 1] range.
        let q_scale = 2.0;
        let expanded = group
            .candidates
            .iter()
            .flat_map(|(score, candidate, heads)| {
                heads
                    .iter()
                    .map(move |head| (*score, candidate, *head, heads.len()))
            })
            .collect::<Vec<_>>();
        let candidates = expanded
            .iter()
            .map(|(score, candidate, head, _)| LowerCandidate {
                edge_offsets: edge_offsets(candidate, input.bounds),
                head: *head,
                q: ((q_max - score.q).max(0.0) / q_scale).min(1.0) as f32,
            })
            .collect::<Vec<_>>();
        let weights = expanded
            .iter()
            .map(|(score, _, _, head_count)| {
                search_weight(score.visits, score.prior, temperature) / *head_count as f64
            })
            .collect::<Vec<_>>();
        let denominator = weights.iter().sum::<f64>();
        let target = if denominator == 0.0 {
            vec![0.0; candidates.len()]
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
            policy_supervised: denominator > 0.0,
            success: record.later_fully_validated as u8 as f32,
        });
    }
    Ok(())
}

fn add_lower_candidate(
    session: &Session,
    score: LowerRootScore,
    candidate: &Candidate,
    heads: usize,
    source_lower_checkpoint_sha256: &str,
    groups: &mut BTreeMap<(String, String), LowerGroup>,
) -> Result<()> {
    let mut guided_heads = BTreeSet::new();
    for provenance in
        std::iter::once(&candidate.provenance).chain(candidate.additional_provenance.iter())
    {
        let (model_version, head) = match provenance {
            Provenance::Guided {
                model_version,
                head,
            }
            | Provenance::Diversified {
                model_version: Some(model_version),
                head: Some(head),
                ..
            } => (model_version, head),
            Provenance::Diversified {
                model_version: Some(model_version),
                head: None,
            } => {
                ensure!(
                    model_version == source_lower_checkpoint_sha256,
                    "aic-learning.hierarchical-generation-model-version: route provenance differs from collection checkpoint"
                );
                continue;
            }
            Provenance::Diversified {
                model_version: None,
                head: Some(_),
            } => anyhow::bail!(
                "aic-learning.hierarchical-generation-head: guidance head lacks model version"
            ),
            Provenance::Anchor
            | Provenance::Diversified {
                model_version: None,
                head: None,
            } => continue,
        };
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
        guided_heads.insert(head as i64);
    }
    if guided_heads.is_empty() {
        return Ok(());
    }
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
        .push((score, candidate.clone(), guided_heads.into_iter().collect()));
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

fn visit_target(record: &DenseRootRecord, temperature: f64) -> Vec<f32> {
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

fn outer_backward(
    model: &OuterModel,
    examples: &[&OuterExample],
    train_failed_policy: bool,
    device: Device,
) -> Result<f64> {
    if examples
        .iter()
        .all(|example| matches!(example.policy, OuterPolicy::Dense { .. }))
    {
        let loss = outer_loss(model, examples, train_failed_policy, device)?;
        let scalar = f64::try_from(&loss)?;
        loss.backward();
        return Ok(scalar);
    }
    let roots = examples
        .iter()
        .map(|example| StreamedPolicyRoot::new(model, example.input.as_ref(), device))
        .collect::<Result<Vec<_>>>()?;
    let values = Tensor::cat(
        &roots
            .iter()
            .map(|root| root.values().shallow_clone())
            .collect::<Vec<_>>(),
        0,
    );
    let targets = examples
        .iter()
        .map(|example| example.values)
        .collect::<Vec<_>>();
    let value = value_loss(&values, &targets, device)?;
    let value_scalar = f64::try_from(&value)?;
    let mut policy_scalar = 0.0;
    let mut encoder_terms = Vec::new();
    for (root, example) in roots.iter().zip(examples) {
        if !train_failed_policy && !example.successful_episode {
            continue;
        }
        let mut statistics = StreamedPolicyStatistics::new(device);
        match &example.policy {
            OuterPolicy::Dense {
                features,
                entity_references,
                target,
            } => {
                statistics.observe(&root.frozen_logits(features, entity_references)?, target)?;
                root.backward_page(
                    features,
                    entity_references,
                    target,
                    &statistics,
                    examples.len(),
                )?;
            }
            OuterPolicy::Catalog {
                source,
                ledger,
                domain,
                page_size,
                anchor_policy_mass,
                max_live_bytes,
            } => {
                let budget = SearchMemoryBudget::new(*max_live_bytes);
                let RootReplayStart::Ready(mut cursor) = RootReplaySource::from_verified(
                    ledger,
                    source.as_ref(),
                    domain,
                    *page_size,
                    *anchor_policy_mass,
                    &budget,
                )
                .map_err(|error| classify_catalog_replay_io(error, "open"))?
                else {
                    anyhow::bail!(
                        "aic-learning.hierarchical-generation-resource-unknown: replay preparation exhausted memory"
                    )
                };
                loop {
                    match cursor
                        .next_page(&budget)
                        .map_err(|error| classify_catalog_replay_io(error, "normalizer"))?
                    {
                        RootReplayStep::Page(page) => statistics.observe(
                            &root.frozen_logits(&page.features, &page.entity_indices)?,
                            &page.targets,
                        )?,
                        RootReplayStep::Complete => break,
                        RootReplayStep::MemoryStop(stop) => anyhow::bail!(
                            "aic-learning.hierarchical-generation-resource-unknown: replay normalizer stopped: {stop:?}"
                        ),
                    }
                }
                cursor
                    .rewind()
                    .map_err(|error| classify_catalog_replay_io(error, "rewind"))?;
                loop {
                    match cursor
                        .next_page(&budget)
                        .map_err(|error| classify_catalog_replay_io(error, "gradient"))?
                    {
                        RootReplayStep::Page(page) => root.backward_page(
                            &page.features,
                            &page.entity_indices,
                            &page.targets,
                            &statistics,
                            examples.len(),
                        )?,
                        RootReplayStep::Complete => break,
                        RootReplayStep::MemoryStop(stop) => anyhow::bail!(
                            "aic-learning.hierarchical-generation-resource-unknown: replay gradient stopped: {stop:?}"
                        ),
                    }
                }
            }
        }
        policy_scalar += f64::try_from(statistics.true_cross_entropy()?)? / examples.len() as f64;
        encoder_terms.push(root.encoder_surrogate()?);
    }
    let mut loss = value;
    for term in encoder_terms {
        loss += term;
    }
    loss.backward();
    Ok(value_scalar + policy_scalar)
}

fn classify_catalog_replay_io(error: anyhow::Error, stage: &str) -> anyhow::Error {
    if error
        .chain()
        .any(|cause| cause.downcast_ref::<std::io::Error>().is_some())
    {
        anyhow::anyhow!(
            "aic-learning.hierarchical-generation-resource-unknown: catalog replay {stage} I/O failed: {error:#}"
        )
    } else {
        error
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
    let dense = examples
        .iter()
        .map(|example| match &example.policy {
            OuterPolicy::Dense {
                features,
                entity_references,
                target,
            } => Ok((
                features.as_slice(),
                entity_references.as_slice(),
                target.as_slice(),
            )),
            OuterPolicy::Catalog { .. } => anyhow::bail!(
                "aic-learning.hierarchical-generation-loss: catalog root requires streamed backward"
            ),
        })
        .collect::<Result<Vec<_>>>()?;
    let features = dense
        .iter()
        .map(|(features, _, _)| *features)
        .collect::<Vec<_>>();
    let entity_references = dense
        .iter()
        .map(|(_, entities, _)| *entities)
        .collect::<Vec<_>>();
    let batch = OuterBatch::from_rows(&inputs, &features, &entity_references, device)?;
    let prediction = model.forward(&batch)?;
    let mut policies = Vec::with_capacity(examples.len());
    for (state, example) in examples.iter().enumerate() {
        let range = batch.action_range(state);
        let target = Tensor::from_slice(dense[state].2).to_device(device);
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
        if example.policy_supervised {
            let logp = (-Tensor::stack(&energies, 0) / temperature).log_softmax(-1, Kind::Float);
            policies.push(
                -(Tensor::from_slice(&example.target).to_device(device) * logp).sum(Kind::Float),
            );
        }
    }
    let policy = if policies.is_empty() {
        Tensor::zeros([], (Kind::Float, device))
    } else {
        Tensor::stack(&policies, 0).mean(Kind::Float)
    };
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
    ensure!(
        collection
            .cases
            .iter()
            .all(|case| case.mcts.termination != SearchTermination::ResourceStopped),
        "aic-learning.hierarchical-generation-resource-unknown: collection contains a resource-stopped case"
    );
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
                    match (&search.root_selection, &record.target) {
                        (RootSelection::Puct, RootRecordTarget::PuctDense(dense)) => {
                            !dense.actions.is_empty() && dense.selected_action < dense.actions.len()
                        }
                        (
                            RootSelection::GumbelSequentialHalving { .. },
                            RootRecordTarget::GumbelCatalog { descriptor },
                        ) => descriptor.case_id() == case.id,
                        #[cfg(test)]
                        (
                            RootSelection::GumbelSequentialHalving { .. },
                            RootRecordTarget::GumbelDenseOracle(dense),
                        ) => {
                            !dense.actions.is_empty() && dense.selected_action < dense.actions.len()
                        }
                        _ => false,
                    }
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
    session
        .hierarchical_state_sha256()
        .map_err(anyhow::Error::new)
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
    use crate::hierarchical_search::{
        RootActionRecord, SearchTermination, validate_root_record_lineage,
    };

    #[test]
    fn lower_examples_keep_updated_heads_and_zero_visit_rows() -> Result<()> {
        let _torch_guard = crate::TORCH_TEST.lock().unwrap();
        let panel: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-route-guidance/training-panel.json"
        ))?;
        let lower_config: LowerTrainingConfig = serde_json::from_str(include_str!(
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
        let candidate_config = serde_json::from_value(panel["candidate"].clone())?;
        let request = aic_pilot_native::constructive::routing_candidates::Request {
            state_revision: session.revision(),
            connection: case["request"].as_str().expect("route request").into(),
        };
        let native = session.route_candidates(&request, &candidate_config)?;
        assert!(
            native.candidates.len() >= 2,
            "fixture needs two route witnesses"
        );
        let mut first = native.candidates[0].clone();
        first.provenance = Provenance::Anchor;
        first.additional_provenance = vec![
            Provenance::Guided {
                model_version: "checkpoint".into(),
                head: "head-0".into(),
            },
            Provenance::Guided {
                model_version: "checkpoint".into(),
                head: "head-1".into(),
            },
            Provenance::Guided {
                model_version: "checkpoint".into(),
                head: "head-1".into(),
            },
        ];
        let mut second = native.candidates[1].clone();
        second.provenance = Provenance::Guided {
            model_version: "checkpoint".into(),
            head: "head-2".into(),
        };
        let mut initially_exposed = first.clone();
        initially_exposed.additional_provenance.clear();
        let controlled = crate::hierarchical_search::controlled_gumbel_record(
            session.clone(),
            candidate_config.clone(),
            vec![MacroAction::Route(initially_exposed.clone())],
            vec![MacroAction::Route(second.clone())],
            vec![(
                initially_exposed.connection.clone(),
                initially_exposed.id,
                first.additional_provenance.clone(),
            )],
        )?;
        let dense = controlled.dense().expect("dense oracle");
        assert_eq!(dense.actions.len(), 2);
        assert_eq!(dense.initial_eligible_actions, Some(1));
        assert_eq!(dense.gumbel_shortlist.len(), 1);
        assert_eq!(dense.gumbel_shortlist[0].initial_rank, 0);
        assert!(f64::from_bits(dense.gumbel_shortlist[0].gumbel_bits).is_finite());
        assert_eq!(dense.actions[dense.selected_action].visits, 2);
        let late = dense
            .actions
            .iter()
            .find(|action| action.gumbel_eligible == Some(false))
            .expect("late route witness");
        assert_eq!(late.policy_target, Some(0.0));
        assert_eq!(late.visits, 0);
        let selected = &dense.actions[dense.selected_action];
        let MacroAction::Route(updated) = &selected.action else {
            unreachable!("initial route witness")
        };
        assert_eq!(updated.provenance, Provenance::Anchor);
        assert_eq!(updated.additional_provenance, first.additional_provenance);
        let mut integrated_lower = Vec::new();
        collect_lower_examples(
            &session,
            &controlled,
            &lower_config.learning_domain,
            &lower_config,
            "checkpoint",
            &candidate_config,
            1.0,
            &mut integrated_lower,
        )?;
        assert_eq!(integrated_lower.len(), 1);
        assert_eq!(integrated_lower[0].candidates.len(), 3);
        assert_eq!(integrated_lower[0].target, [0.5, 0.5, 0.0]);
        let input = session.macro_learning_input(&lower_config.learning_domain)?;
        let mut features = Vec::new();
        let mut entity_references = Vec::new();
        for action in &dense.actions {
            let child = action.action.apply(&session, &candidate_config, 1)?;
            features.push(
                action
                    .action
                    .features(&session, &child, candidate_config.bounds),
            );
            entity_references.push(action.action.entity_index(session.problem())?);
        }
        let outer_batch = OuterBatch::from_rows(
            &[input.as_ref()],
            &[features.as_slice()],
            &[entity_references.as_slice()],
            Device::Cpu,
        )?;
        let action_range = outer_batch.action_range(0);
        assert_eq!(action_range.end - action_range.start, 2);
        let actions = [first, second]
            .into_iter()
            .map(|candidate| RootActionRecord {
                action: MacroAction::Route(candidate),
                prior: 0.5,
                anchor_prior: 0.5,
                visits: 0,
                q: 0.25,
                gumbel_eligible: Some(false),
                policy_target: Some(0.0),
            })
            .collect();
        let mut record = RootRecord {
            state_sha256: state_sha256(&session)?,
            state_revision: session.revision(),
            target: RootRecordTarget::GumbelDenseOracle(DenseRootRecord {
                actions,
                selected_action: 0,
                selected_physical_id: None,
                gumbel_shortlist: Vec::new(),
                gumbel_shortlist_cap: Some(1),
                initial_eligible_actions: Some(0),
            }),
            pending_lower_tasks: 0,
            loaded_lower_pages: 1,
            later_completed_connections: 0,
            later_total_connections: 1,
            later_footprint_area: 0,
            later_transport_cells: 0,
            later_turns: 0,
            later_fully_validated: false,
            later_termination: SearchTermination::MoveBudgetExhausted,
        };
        let mut examples = Vec::new();
        collect_lower_examples(
            &session,
            &record,
            &lower_config.learning_domain,
            &lower_config,
            "checkpoint",
            &candidate_config,
            1.0,
            &mut examples,
        )?;
        assert_eq!(examples.len(), 1);
        assert_eq!(examples[0].candidates.len(), 3);
        assert_eq!(
            examples[0]
                .candidates
                .iter()
                .map(|candidate| candidate.head)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert!(!examples[0].policy_supervised);
        assert_eq!(examples[0].target, vec![0.0; 3]);
        let (order, state) = LowerModel::initial_state(&lower_config.model, 17)?;
        let model = LowerModel::load(&lower_config.model, &order, &state, Device::Cpu)?;
        let baseline = lower_loss(&model, &[&examples[0]], 4.0, Device::Cpu)?.double_value(&[]);
        examples[0].target = vec![1.0, 0.0, 0.0];
        let with_ignored_target =
            lower_loss(&model, &[&examples[0]], 4.0, Device::Cpu)?.double_value(&[]);
        assert_eq!(baseline, with_ignored_target);

        record.dense_mut().unwrap().actions[0].visits = 4;
        record.dense_mut().unwrap().actions[1].visits = 2;
        examples.clear();
        collect_lower_examples(
            &session,
            &record,
            &lower_config.learning_domain,
            &lower_config,
            "checkpoint",
            &candidate_config,
            1.0,
            &mut examples,
        )?;
        assert!(examples[0].policy_supervised);
        assert_eq!(examples[0].target, vec![1.0 / 3.0; 3]);
        let MacroAction::Route(first) = &mut record.dense_mut().unwrap().actions[0].action else {
            unreachable!("first route witness")
        };
        first.additional_provenance.push(Provenance::Diversified {
            model_version: Some("wrong-checkpoint".into()),
            head: None,
        });
        let error = collect_lower_examples(
            &session,
            &record,
            &lower_config.learning_domain,
            &lower_config,
            "checkpoint",
            &candidate_config,
            1.0,
            &mut Vec::new(),
        )
        .err()
        .expect("headless diversified provenance must still check its model version");
        assert!(
            error
                .to_string()
                .contains("hierarchical-generation-model-version")
        );
        Ok(())
    }

    #[test]
    fn network_and_repair_witnesses_reach_lower_training() -> Result<()> {
        let _torch_guard = crate::TORCH_TEST.lock().unwrap();
        let lower_config: LowerTrainingConfig = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-route-guidance/config.json"
        ))?;
        let network_panel: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-network/panel.json"
        ))?;
        let search_config: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-curriculum/search.json"
        ))?;
        let network_case = &network_panel["cases"][0];
        let mut network_session = Session::new(
            serde_json::from_value(network_case["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value(network_panel["limits"].clone())?,
        )?;
        for command in network_case["prefix"].as_array().expect("network prefix") {
            assert!(
                network_session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        let mut network_config = serde_json::from_value::<
            aic_pilot_native::constructive::routing_candidates::Config,
        >(search_config["candidate"].clone())?;
        network_config.request_allowance.max_variants = 4;
        let (mut selected_network, mut network) =
            crate::hierarchical_search::controlled_network_pair(&network_session, &network_config)?;
        for (action, head) in [(&mut selected_network, "head-0"), (&mut network, "head-1")] {
            let MacroAction::Network(candidate) = action else {
                unreachable!("controlled network witness")
            };
            for route in &mut candidate.routes {
                route.provenance = Provenance::Guided {
                    model_version: "checkpoint".into(),
                    head: head.into(),
                };
                route.additional_provenance.clear();
            }
        }
        let repair_fixture: Value = serde_json::from_str(include_str!(
            "../../../docs/benchmarks/hierarchical-repair/fixture.json"
        ))?;
        let mut repair_session = Session::new(
            serde_json::from_value(repair_fixture["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value(repair_fixture["limits"].clone())?,
        )?;
        for command in repair_fixture["prefix"].as_array().expect("repair prefix") {
            assert!(
                repair_session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        let repair_config = serde_json::from_value(repair_fixture["candidate"].clone())?;
        let repair_request = repair_session.repair_request(
            &["second".into()],
            &["first".into()],
            1,
            &repair_config,
        )?;
        let build_repair = |second_choice: usize| -> Result<MacroAction> {
            let mut allowance =
                aic_pilot_native::constructive::routing_candidates::RequestAllowance::new(
                    repair_request.allowance_caps.clone(),
                );
            let (mut draft, reserved_bytes) = repair_session.prepare_repair_draft_with_allowance(
                &repair_request,
                &["first".into()],
                &mut allowance,
            )?;
            allowance.release_peak(reserved_bytes);
            let mut replacements = Vec::new();
            for (index, connection) in ["second", "first"].into_iter().enumerate() {
                let request = aic_pilot_native::constructive::routing_candidates::Request {
                    state_revision: draft.revision(),
                    connection: connection.into(),
                };
                let mut route = draft
                    .route_candidates(&request, &repair_config)?
                    .candidates
                    .into_iter()
                    .nth(if index == 0 { second_choice } else { 0 })
                    .with_context(|| {
                        format!("no repair route {connection} choice {second_choice}")
                    })?;
                route.provenance = Provenance::Guided {
                    model_version: "checkpoint".into(),
                    head: format!("head-{index}"),
                };
                let before = draft.clone();
                draft.apply_route_candidate(&route)?;
                route.physical_cost = before
                    .physical_cost_between(&draft, std::slice::from_ref(&route.connection))?;
                replacements.push(route);
            }
            Ok(MacroAction::Repair {
                request: repair_request.clone(),
                candidate: aic_pilot_native::constructive::routing_candidates::RepairCandidate {
                    schema:
                        aic_pilot_native::constructive::routing_candidates::REPAIR_CANDIDATE_SCHEMA
                            .into(),
                    state_revision: repair_request.state_revision,
                    removed: vec!["first".into()],
                    expanded: replacements.iter().map(|route| route.expanded).sum(),
                    replacements,
                    physical_cost: repair_session
                        .physical_cost_between(&draft, &["first".into(), "second".into()])?,
                    charged_work: Default::default(),
                },
            })
        };
        let mut selected_repair = build_repair(1)?;
        if let MacroAction::Repair { candidate, .. } = &mut selected_repair {
            for route in &mut candidate.replacements {
                route.provenance = Provenance::Anchor;
            }
        }
        let repair = build_repair(0)?;
        for (is_network, session, selected, action, config) in [
            (
                true,
                &network_session,
                selected_network,
                network,
                &network_config,
            ),
            (
                false,
                &repair_session,
                selected_repair,
                repair,
                &repair_config,
            ),
        ] {
            let record = crate::hierarchical_search::controlled_gumbel_record(
                session.clone(),
                config.clone(),
                vec![selected],
                vec![action],
                Vec::new(),
            )?;
            validate_root_record_lineage(&record)?;
            let dense = record.dense().expect("dense oracle");
            assert_eq!(
                dense.actions[dense.selected_action].policy_target,
                Some(1.0)
            );
            assert_eq!(visit_target(dense, 1.0).iter().sum::<f32>(), 1.0);
            assert!(dense.actions.iter().any(|action| {
                action.gumbel_eligible == Some(false) && action.policy_target == Some(0.0)
            }));
            for witness in &dense.actions {
                witness.action.apply(session, config, 1)?;
            }
            let mut examples = Vec::new();
            collect_lower_examples(
                session,
                &record,
                &lower_config.learning_domain,
                &lower_config,
                "checkpoint",
                config,
                1.0,
                &mut examples,
            )?;
            assert!(!examples.is_empty());
            if is_network {
                for (eligible, supervised) in [(true, true), (false, false)] {
                    let mut isolated = record.clone();
                    isolated
                        .dense_mut()
                        .unwrap()
                        .actions
                        .retain(|witness| witness.gumbel_eligible == Some(eligible));
                    let mut contributions = Vec::new();
                    collect_lower_examples(
                        session,
                        &isolated,
                        &lower_config.learning_domain,
                        &lower_config,
                        "checkpoint",
                        config,
                        1.0,
                        &mut contributions,
                    )?;
                    assert!(!contributions.is_empty());
                    assert!(contributions.iter().all(|example| {
                        example.policy_supervised == supervised
                            && (supervised || example.target.iter().all(|target| *target == 0.0))
                    }));
                }
            } else {
                assert_eq!(examples.len(), 2);
                assert!(examples.iter().all(|example| {
                    example.candidates.len() == 1
                        && !example.policy_supervised
                        && example.target == [0.0]
                }));
            }
        }
        Ok(())
    }

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
                gumbel_eligible: Some(policy_target > 0.0),
                policy_target: Some(policy_target),
            })
            .collect();
        let record = RootRecord {
            state_sha256: "state".into(),
            state_revision: 0,
            target: RootRecordTarget::GumbelDenseOracle(DenseRootRecord {
                actions,
                selected_action: 1,
                selected_physical_id: None,
                gumbel_shortlist: Vec::new(),
                gumbel_shortlist_cap: Some(2),
                initial_eligible_actions: Some(2),
            }),
            pending_lower_tasks: 0,
            loaded_lower_pages: 1,
            later_completed_connections: 0,
            later_total_connections: 1,
            later_footprint_area: 0,
            later_transport_cells: 0,
            later_turns: 0,
            later_fully_validated: false,
            later_termination: SearchTermination::MoveBudgetExhausted,
        };
        assert_eq!(visit_target(record.dense().unwrap(), 0.25), vec![0.2, 0.8]);
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
