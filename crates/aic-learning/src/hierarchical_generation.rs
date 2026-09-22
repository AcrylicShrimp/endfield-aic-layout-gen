//! Alternating-generation distillation from frozen hierarchical MCTS records.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use aic_layout::{geometry::Point, physical::Problem, validation::Catalogs};
use aic_pilot_native::constructive::{
    Command, Limits, Session,
    learning::{Config as DomainConfig, Input},
    routing_candidates::{Candidate, GuidanceInput},
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
    hierarchical_search::{MacroAction, REPORT_SCHEMA as SEARCH_REPORT_SCHEMA, RootRecord},
    input::Tensors,
    outer_model::{ACTION_FEATURES, Batch as OuterBatch, Model as OuterModel, VALUE_HEADS},
    outer_training::Config as OuterTrainingConfig,
    route_guidance::{
        Batch as LowerBatch, Model as LowerModel, ParameterOrder as LowerOrder,
        Prediction as LowerPrediction,
    },
    route_guidance_training::Config as LowerTrainingConfig,
};

const CONFIG_SCHEMA: &str = "aic-hierarchical-generation-training-v1";
const PANEL_SCHEMA: &str = "aic-hierarchical-search-panel-v1";
const REPORT_SCHEMA: &str = "aic-hierarchical-generation-training-report-v2";
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
}

#[derive(Debug, Deserialize)]
struct Panel {
    schema: String,
    partition: String,
    limits: Limits,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
struct Case {
    id: String,
    problem: Problem,
    prefix: Vec<Command>,
}

#[derive(Debug, Deserialize)]
struct CollectionReport {
    schema: String,
    partition: String,
    catalog_sha256: String,
    lower_checkpoint_sha256: String,
    outer_checkpoint_sha256: String,
    cases: Vec<CollectionCase>,
}

#[derive(Debug, Deserialize)]
struct CollectionCase {
    id: String,
    mcts: CollectionSearch,
}

#[derive(Debug, Deserialize)]
struct CollectionSearch {
    root_records: Vec<RootRecord>,
}

struct OuterExample {
    input: Arc<Input>,
    features: Vec<[f32; ACTION_FEATURES]>,
    target: Vec<f32>,
    values: [f32; VALUE_HEADS],
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
    source_lower_checkpoint_sha256: String,
    source_outer_checkpoint_sha256: String,
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
    let config_bytes = bounded_read(config_path)?;
    let panel_bytes = bounded_read(panel_path)?;
    let collection_bytes = bounded_read(collection_path)?;
    let config: Config = serde_json::from_slice(&config_bytes)?;
    let panel: Panel = serde_json::from_slice(&panel_bytes)?;
    let collection: CollectionReport = serde_json::from_slice(&collection_bytes)?;
    validate_config(&config)?;
    validate_inputs(&panel, &collection)?;

    let lower_config: LowerTrainingConfig = read_json(&lower_training_output.join("config.json"))?;
    let lower_order: LowerOrder = read_json(&lower_training_output.join("parameter-order.json"))?;
    let lower_source = lower_training_output.join("state.safetensors");
    let lower_source_sha256 = crate::input::sha256(&lower_source)?;
    ensure!(
        lower_source_sha256 == collection.lower_checkpoint_sha256,
        "aic-learning.hierarchical-generation-lineage: lower collection model differs"
    );
    let mut lower_values: Tensors = Tensor::read_safetensors(&lower_source)?
        .into_iter()
        .collect();

    let outer_config: OuterTrainingConfig = read_json(&outer_training_output.join("config.json"))?;
    let outer_order: crate::outer_model::ParameterOrder =
        read_json(&outer_training_output.join("parameter-order.json"))?;
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
    ensure!(
        catalogs.identity == collection.catalog_sha256,
        "aic-learning.hierarchical-generation-catalog: collection catalog differs"
    );
    let domain = DomainConfig {
        placement: outer_config.search.bounds,
        extension_lengths: outer_config.search.extension_lengths.clone(),
    };
    let (outer_examples, lower_examples) = collect_examples(
        &panel,
        &collection,
        &domain,
        &outer_config,
        &lower_config,
        catalogs.clone(),
        &config,
    )?;
    ensure!(
        !outer_examples.is_empty() && !lower_examples.is_empty(),
        "aic-learning.hierarchical-generation-samples: collection has no usable roots or route choices"
    );

    let binding = RecoveryBinding {
        schema: RECOVERY_BINDING_SCHEMA.into(),
        config_sha256: bytes_sha256(&config_bytes),
        panel_sha256: bytes_sha256(&panel_bytes),
        collection_sha256: bytes_sha256(&collection_bytes),
        catalog_sha256: catalogs.identity.clone(),
        source_lower_checkpoint_sha256: lower_source_sha256.clone(),
        source_outer_checkpoint_sha256: outer_source_sha256.clone(),
    };
    let binding_bytes = serde_json::to_vec_pretty(&binding)?;
    let binding_sha256 = bytes_sha256(&binding_bytes);
    let mut recovery = if resume {
        let retained: RecoveryBinding = read_json(&pending.join("recovery-binding.json"))?;
        ensure!(
            retained == binding,
            "aic-learning.hierarchical-generation-resume: inputs or source checkpoints changed"
        );
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
            initial_lower_loss: None,
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
        let rotation = (config.seed as usize).wrapping_add(epoch) % outer_examples.len();
        let order = (0..outer_examples.len())
            .map(|index| (index + rotation) % outer_examples.len())
            .collect::<Vec<_>>();
        for indices in order.chunks(config.batch_states) {
            let examples = indices
                .iter()
                .map(|&index| &outer_examples[index])
                .collect::<Vec<_>>();
            outer_optimizer.zero_grad()?;
            let loss = outer_loss(&outer, &examples, device)?;
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
    write_tensors(&pending.join("lower/state.safetensors"), lower.snapshot())?;
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
        collection_sha256: bytes_sha256(&collection_bytes),
        panel_sha256: bytes_sha256(&panel_bytes),
        root_states: outer_examples.len(),
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
    panel: &Panel,
    collection: &CollectionReport,
    domain: &DomainConfig,
    outer_config: &OuterTrainingConfig,
    lower_config: &LowerTrainingConfig,
    catalogs: Arc<Catalogs>,
    config: &Config,
) -> Result<(Vec<OuterExample>, Vec<LowerExample>)> {
    let cases = panel
        .cases
        .iter()
        .map(|case| (case.id.as_str(), case))
        .collect::<BTreeMap<_, _>>();
    let mut outer_examples = Vec::new();
    let mut lower_examples = Vec::new();
    for collected in &collection.cases {
        let case = cases[collected.id.as_str()];
        let mut session = prefix_session(case, panel, catalogs.clone())?;
        for record in &collected.mcts.root_records {
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
                .map(|action| action.action.apply(&session))
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
            let target = visit_target(record, config.search_temperature);
            let observation = session.observe();
            let completed = record.later_completed_connections;
            let total = record.later_total_connections.max(1);
            let additional = completed.saturating_sub(observation.completed);
            let values = [
                record.later_fully_validated as u8 as f32,
                completed as f32 / total as f32,
                (additional > 0) as u8 as f32,
                1.0 - completed as f32 / total as f32,
                1.0 / (1.0 + record.actions.len() as f32),
                if record.later_fully_validated {
                    0.0
                } else {
                    1.0
                },
            ];
            outer_examples.push(OuterExample {
                input: session
                    .macro_learning_input(domain)
                    .map_err(anyhow::Error::new)?,
                features,
                target,
                values,
            });
            collect_lower_examples(
                &session,
                record,
                domain,
                lower_config,
                config.search_temperature,
                &mut lower_examples,
            )?;
            session = children[record.selected_action].clone();
        }
    }
    Ok((outer_examples, lower_examples))
}

fn collect_lower_examples(
    session: &Session,
    record: &RootRecord,
    domain: &DomainConfig,
    lower_config: &LowerTrainingConfig,
    temperature: f64,
    output: &mut Vec<LowerExample>,
) -> Result<()> {
    let mut groups = BTreeMap::<&str, Vec<(usize, &Candidate)>>::new();
    for (index, action) in record.actions.iter().enumerate() {
        match &action.action {
            MacroAction::Route(candidate) => groups
                .entry(candidate.connection.as_str())
                .or_default()
                .push((index, candidate)),
            MacroAction::Network(candidate) => {
                for route in &candidate.routes {
                    groups
                        .entry(route.connection.as_str())
                        .or_default()
                        .push((index, route));
                }
            }
            MacroAction::Command(_) => {}
        }
    }
    for (connection, indices) in groups {
        if indices.len() < 2 {
            continue;
        }
        let request = aic_pilot_native::constructive::routing_candidates::Request {
            state_revision: session.observe().revision,
            connection: connection.to_owned(),
        };
        let input = session
            .route_guidance_input(&request, &lower_candidate_config(lower_config), domain)
            .map_err(anyhow::Error::new)?;
        let q_max = indices
            .iter()
            .map(|&(index, _)| record.actions[index].q)
            .fold(f64::NEG_INFINITY, f64::max);
        let candidates = indices
            .iter()
            .map(|&(index, candidate)| {
                Ok(LowerCandidate {
                    edge_offsets: edge_offsets(candidate, input.bounds),
                    head: (candidate.id % lower_config.model.heads) as i64,
                    q: (q_max - record.actions[index].q).max(0.0) as f32,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let weights = indices
            .iter()
            .map(|&(index, _)| {
                search_weight(
                    record.actions[index].visits,
                    record.actions[index].prior,
                    temperature,
                )
            })
            .collect::<Vec<_>>();
        let denominator = weights.iter().sum::<f64>();
        let target = weights
            .into_iter()
            .map(|weight| (weight / denominator.max(f64::MIN_POSITIVE)) as f32)
            .collect();
        output.push(LowerExample {
            input,
            candidates,
            target,
            success: record.later_fully_validated as u8 as f32,
        });
    }
    Ok(())
}

fn lower_candidate_config(
    config: &LowerTrainingConfig,
) -> aic_pilot_native::constructive::routing_candidates::Config {
    // Collection candidates already passed this exact boundary. Only bounds are
    // needed to reconstruct the model input; conservative valid limits avoid a
    // second search during distillation.
    aic_pilot_native::constructive::routing_candidates::Config {
        bounds: config.learning_domain.placement,
        max_candidates: 1,
        max_searches: 1,
        work_per_search: 1,
        max_expanded: 1,
        overlap_penalty: 1,
    }
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
        (-1, 0) => 1,
        (0, 1) => 2,
        (0, -1) => 3,
        _ => unreachable!("candidate cells are cardinal neighbors"),
    }
}

fn visit_target(record: &RootRecord, temperature: f64) -> Vec<f32> {
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

fn search_weight(visits: u32, prior: f64, temperature: f64) -> f64 {
    if visits == 0 {
        prior.max(1e-12) * 1e-3
    } else {
        f64::from(visits).powf(1.0 / temperature)
    }
}

fn outer_loss(model: &OuterModel, examples: &[&OuterExample], device: Device) -> Result<Tensor> {
    let inputs = examples
        .iter()
        .map(|example| example.input.as_ref())
        .collect::<Vec<_>>();
    let features = examples
        .iter()
        .map(|example| example.features.as_slice())
        .collect::<Vec<_>>();
    let batch = OuterBatch::from_rows(&inputs, &features, device)?;
    let prediction = model.forward(&batch)?;
    let mut policies = Vec::with_capacity(examples.len());
    for (state, example) in examples.iter().enumerate() {
        let range = batch.action_range(state);
        let target = Tensor::from_slice(&example.target).to_device(device);
        policies.push(
            -(target
                * prediction
                    .logp
                    .narrow(0, range.start, range.end - range.start))
            .sum(Kind::Float),
        );
    }
    let target = Tensor::from_slice(
        &examples
            .iter()
            .flat_map(|example| example.values)
            .collect::<Vec<_>>(),
    )
    .view([examples.len() as i64, VALUE_HEADS as i64])
    .to_device(device);
    Ok(Tensor::stack(&policies, 0).mean(Kind::Float)
        + (&prediction.values - target).square().mean(Kind::Float))
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
                let energy = flat.index_select(0, &indices).mean(Kind::Float);
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

fn prefix_session(case: &Case, panel: &Panel, catalogs: Arc<Catalogs>) -> Result<Session> {
    let mut session =
        Session::new(case.problem.clone(), catalogs, panel.limits).map_err(anyhow::Error::new)?;
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
            && config.seed <= i64::MAX as u64,
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
            && panel.partition == collection.partition
            && collection.schema == SEARCH_REPORT_SCHEMA
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

fn synchronize(device: Device) {
    if let Device::Cuda(index) = device {
        tch::Cuda::synchronize(index as i64);
    }
}

fn state_sha256(session: &Session) -> Result<String> {
    Ok(bytes_sha256(&serde_json::to_vec(&session.observe())?))
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
