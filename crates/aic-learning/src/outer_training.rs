//! Search-supervised warm start for the separate outer macro policy/value model.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use aic_layout::{geometry::Bounds, physical::Problem, validation::Catalogs};
use aic_pilot_native::constructive::{Command, Limits, Session, learning::Input};
use aic_torch::{
    Adam, AdamBackend,
    tch::{self, Device, Kind, Tensor},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    hierarchical_search::{Config as SearchConfig, Counters, Factory},
    input::Tensors,
    outer_model::{
        ACTION_FEATURES, Batch, Model, ValueTarget, normalized_layout_quality, value_loss,
    },
    route_guidance::{Model as GuidanceModel, ParameterOrder as GuidanceOrder},
    route_guidance_training::Config as GuidanceTrainingConfig,
    schema::OuterModelConfig,
};

const CONFIG_SCHEMA: &str = "aic-hierarchical-outer-training-v1";
const PANEL_SCHEMA: &str = "aic-hierarchical-search-panel-v1";
const REPORT_SCHEMA: &str = "aic-hierarchical-outer-training-report-v3";
const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub model: OuterModelConfig,
    pub search: SearchConfig,
    pub epochs: usize,
    pub batch_states: usize,
    pub label_horizon: usize,
    pub target_temperature: f64,
    pub learning_rate: f64,
    pub gradient_clip: f64,
    pub seed: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Panel {
    schema: String,
    partition: String,
    limits: Limits,
    #[serde(default, rename = "preparation")]
    _preparation: Option<serde_json::Value>,
    cases: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    problem: Problem,
    prefix: Vec<Command>,
}

struct Example {
    input: Arc<Input>,
    features: Vec<[f32; ACTION_FEATURES]>,
    entity_references: Vec<i64>,
    policy_target: Vec<f32>,
    completion_labels: Vec<f32>,
    utility_labels: Vec<f32>,
    value_target: ValueTarget,
}

#[derive(Clone, Copy)]
struct Outcome {
    full: bool,
    completed: usize,
    total: usize,
    moves: usize,
    footprint_area: u64,
    transport_cells: u64,
    turns: u64,
}

#[derive(Clone, Debug, Serialize)]
struct Evaluation {
    states: usize,
    macro_action_samples: usize,
    states_with_completing_action: usize,
    selected_completing_actions: usize,
    exact_best_actions: usize,
    mean_selected_completion: f64,
    mean_oracle_completion: f64,
    mean_selected_utility: f64,
    mean_oracle_utility: f64,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    status: &'static str,
    device: String,
    catalog_sha256: String,
    lower_checkpoint_sha256: String,
    config_sha256: String,
    training_panel_sha256: String,
    evaluation_panel_sha256: String,
    training_states: usize,
    training_macro_actions: usize,
    processed_macro_action_samples: usize,
    sample_definition: &'static str,
    epochs: usize,
    updates: usize,
    elapsed_seconds: f64,
    samples_per_minute: f64,
    initial_loss: f64,
    final_loss: f64,
    maximum_gradient_norm: f64,
    evaluation_before: Evaluation,
    evaluation_after: Evaluation,
    checkpoint_sha256: String,
}

pub fn run(
    config_path: &Path,
    training_panel_path: &Path,
    evaluation_panel_path: &Path,
    lower_training_output: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
) -> Result<Value> {
    ensure!(
        !output.exists(),
        "aic-learning.outer-training-output: {} already exists",
        output.display()
    );
    let pending = pending_path(output)?;
    ensure!(
        !pending.exists(),
        "aic-learning.outer-training-output: pending path exists {}",
        pending.display()
    );
    let config_bytes = bounded_read(config_path)?;
    let training_bytes = bounded_read(training_panel_path)?;
    let evaluation_bytes = bounded_read(evaluation_panel_path)?;
    let config: Config = serde_json::from_slice(&config_bytes)?;
    let training_panel: Panel = serde_json::from_slice(&training_bytes)?;
    let evaluation_panel: Panel = serde_json::from_slice(&evaluation_bytes)?;
    validate_config(&config)?;
    validate_panel(&training_panel, "training")?;
    validate_panel(&evaluation_panel, "held-out")?;
    validate_panel_separation(&training_panel, &evaluation_panel)?;

    let lower_config: GuidanceTrainingConfig =
        read_json(&lower_training_output.join("config.json"))?;
    let lower_order: GuidanceOrder =
        read_json(&lower_training_output.join("parameter-order.json"))?;
    let lower_checkpoint = lower_training_output.join("state.safetensors");
    let lower_values: Tensors = Tensor::read_safetensors(&lower_checkpoint)?
        .into_iter()
        .collect();
    let guidance = GuidanceModel::load(&lower_config.model, &lower_order, &lower_values, device)?;
    let lower_checkpoint_sha256 = crate::input::sha256(&lower_checkpoint)?;
    let loaded = aic_layout_gen::io::load_catalogs(catalog_root)?;
    let catalogs = Arc::new(loaded.catalogs);
    crate::input::verify_training_artifact(
        lower_training_output,
        &catalogs.identity,
        &lower_checkpoint_sha256,
    )?;
    let domain = aic_pilot_native::constructive::learning::Config {
        placement: config.search.bounds,
        extension_lengths: config.search.extension_lengths.clone(),
    };
    ensure!(
        lower_config.learning_domain == domain,
        "aic-learning.outer-training-domain: search and lower model domains differ"
    );
    let factory = Factory::new(
        &guidance,
        &lower_checkpoint_sha256,
        device,
        domain.clone(),
        &config.search.candidate,
        config.search.repair_max_removed,
        config.search.placement_page,
        config.search.cpu_threads,
        config.search.anchor_prior_mass,
        config.search.compiled_inference,
        None,
    )?;
    let training = collect_panel(
        &training_panel,
        &factory,
        &domain,
        &config,
        catalogs.clone(),
    )?;
    let evaluation = collect_panel(
        &evaluation_panel,
        &factory,
        &domain,
        &config,
        catalogs.clone(),
    )?;
    ensure!(
        training.iter().any(|example| {
            example.completion_labels.iter().any(|label| *label == 1.0)
                && example.completion_labels.iter().any(|label| *label == 0.0)
        }) && evaluation.iter().any(|example| {
            example.completion_labels.iter().any(|label| *label == 1.0)
                && example.completion_labels.iter().any(|label| *label == 0.0)
        }),
        "aic-learning.outer-training-labels: both partitions need discriminating actions"
    );

    let (order, state) = Model::initial_state(&config.model, config.seed)?;
    let model = Model::load(&config.model, &order, &state, device)?;
    let mut optimizer = Adam::new(
        model
            .parameters
            .iter()
            .map(|(name, value)| (name.clone(), value.shallow_clone()))
            .collect(),
        config.learning_rate,
        0.9,
        0.999,
        1e-8,
        if matches!(device, Device::Cuda(_)) {
            AdamBackend::Fused
        } else {
            AdamBackend::Foreach
        },
    )?;
    let before = evaluate(&model, &evaluation, device)?;
    synchronize(device);
    let started = Instant::now();
    let mut initial_loss = None;
    let mut final_loss = 0.0;
    let mut maximum_gradient_norm = 0.0f64;
    let mut updates = 0usize;
    let mut processed = 0usize;
    for epoch in 0..config.epochs {
        let rotation = (config.seed as usize).wrapping_add(epoch) % training.len();
        let indices = (0..training.len())
            .map(|index| (index + rotation) % training.len())
            .collect::<Vec<_>>();
        for indices in indices.chunks(config.batch_states) {
            let examples = indices
                .iter()
                .map(|&index| &training[index])
                .collect::<Vec<_>>();
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
            let batch = Batch::from_rows(&inputs, &features, &entity_references, device)?;
            optimizer.zero_grad()?;
            let prediction = model.forward(&batch)?;
            let loss = training_loss(&prediction, &batch, &examples, device)?;
            let scalar = f64::try_from(&loss)?;
            ensure!(
                scalar.is_finite(),
                "aic-learning.outer-training-loss: nonfinite"
            );
            initial_loss.get_or_insert(scalar);
            final_loss = scalar;
            loss.backward();
            maximum_gradient_norm =
                maximum_gradient_norm.max(optimizer.clip_grad_norm(config.gradient_clip)?);
            optimizer.step()?;
            updates += 1;
            processed += examples
                .iter()
                .map(|example| example.features.len())
                .sum::<usize>();
        }
    }
    synchronize(device);
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let after = evaluate(&model, &evaluation, device)?;

    fs::create_dir_all(&pending)?;
    write_json(&pending.join("config.json"), &config)?;
    write_json(&pending.join("parameter-order.json"), &order)?;
    Tensor::write_safetensors(
        &model.snapshot().iter().collect::<Vec<_>>(),
        pending.join("state.safetensors"),
    )?;
    let checkpoint_sha256 = crate::input::sha256(&pending.join("state.safetensors"))?;
    let report = Report {
        schema: REPORT_SCHEMA,
        status: "complete",
        device: format!("{device:?}"),
        catalog_sha256: catalogs.identity.clone(),
        lower_checkpoint_sha256,
        config_sha256: bytes_sha256(&config_bytes),
        training_panel_sha256: bytes_sha256(&training_bytes),
        evaluation_panel_sha256: bytes_sha256(&evaluation_bytes),
        training_states: training.len(),
        training_macro_actions: training.iter().map(|example| example.features.len()).sum(),
        processed_macro_action_samples: processed,
        sample_definition: "native-valid macro actions with fixed-budget exact continuation targets",
        epochs: config.epochs,
        updates,
        elapsed_seconds,
        samples_per_minute: if elapsed_seconds == 0.0 {
            0.0
        } else {
            processed as f64 * 60.0 / elapsed_seconds
        },
        initial_loss: initial_loss.unwrap_or(0.0),
        final_loss,
        maximum_gradient_norm,
        evaluation_before: before,
        evaluation_after: after,
        checkpoint_sha256,
    };
    write_json(&pending.join("report.json"), &report)?;
    fs::rename(&pending, output)?;
    Ok(json!({
        "schema": REPORT_SCHEMA,
        "status": "complete",
        "output": output,
        "processed_macro_action_samples": report.processed_macro_action_samples,
        "samples_per_minute": report.samples_per_minute,
        "selected_completing_before": report.evaluation_before.selected_completing_actions,
        "selected_completing_after": report.evaluation_after.selected_completing_actions,
        "checkpoint_sha256": report.checkpoint_sha256,
    }))
}

fn collect_panel(
    panel: &Panel,
    factory: &Factory<'_>,
    domain: &aic_pilot_native::constructive::learning::Config,
    config: &Config,
    catalogs: Arc<Catalogs>,
) -> Result<Vec<Example>> {
    panel
        .cases
        .iter()
        .map(|case| {
            let session = prefix_session(case, panel, catalogs.clone())?;
            let input = session
                .macro_learning_input(domain)
                .map_err(anyhow::Error::new)?;
            let mut counters = Counters::default();
            let actions = factory.actions(&session, &mut counters)?;
            ensure!(
                !actions.is_empty(),
                "aic-learning.outer-training-actions: no actions in {}",
                case.id
            );
            let mut outcomes = Vec::with_capacity(actions.len());
            let mut features = Vec::with_capacity(actions.len());
            let entity_references = actions
                .iter()
                .map(|action| action.entity_index(session.problem()))
                .collect::<Result<Vec<_>>>()?;
            for action in &actions {
                let child = factory.apply_action(action, &session)?;
                features.push(action.features(&session, &child, config.search.bounds));
                outcomes.push(best_outcome(
                    &child,
                    factory,
                    config.search.bounds,
                    config.label_horizon.saturating_sub(1),
                    1,
                )?);
            }
            let utilities = outcomes
                .iter()
                .map(|outcome| utility(*outcome, config.search.bounds))
                .collect::<Vec<_>>();
            let maximum = utilities.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let weights = utilities
                .iter()
                .map(|utility| {
                    ((*utility - maximum) as f64 / config.target_temperature).exp() as f32
                })
                .collect::<Vec<_>>();
            let denominator = weights.iter().sum::<f32>();
            let policy_target = weights
                .into_iter()
                .map(|weight| weight / denominator)
                .collect::<Vec<_>>();
            let completion_labels = outcomes
                .iter()
                .map(|outcome| outcome.full as u8 as f32)
                .collect::<Vec<_>>();
            let best = outcomes
                .iter()
                .copied()
                .max_by(|left, right| {
                    utility(*left, config.search.bounds)
                        .total_cmp(&utility(*right, config.search.bounds))
                })
                .expect("actions are nonempty");
            let current_completed = session.observe().completed;
            let additional = best.completed.saturating_sub(current_completed);
            let remaining = best.total.saturating_sub(current_completed).max(1);
            let area = config.search.bounds.width() * config.search.bounds.height();
            let quality = best.full.then(|| {
                normalized_layout_quality(
                    best.footprint_area,
                    best.transport_cells,
                    best.turns,
                    area,
                    best.total,
                )
            });
            let value_target = ValueTarget {
                completion: best.full as u8 as f32,
                additional_fraction: additional as f32 / remaining as f32,
                next_progress: (additional > 0) as u8 as f32,
                blocked_fraction: best.total.saturating_sub(best.completed) as f32
                    / best.total.max(1) as f32,
                area: quality.map(|quality| quality[0]),
                transport: quality.map(|quality| quality[1]),
                turns: quality.map(|quality| quality[2]),
            };
            Ok(Example {
                input,
                features,
                entity_references,
                policy_target,
                completion_labels,
                utility_labels: utilities,
                value_target,
            })
        })
        .collect()
}

fn best_outcome(
    session: &Session,
    factory: &Factory<'_>,
    bounds: Bounds,
    horizon: usize,
    moves: usize,
) -> Result<Outcome> {
    let observation = session.observe();
    if observation.fully_validated || horizon == 0 {
        return Ok(Outcome {
            full: observation.fully_validated,
            completed: observation.completed,
            total: observation.total,
            moves,
            footprint_area: session.metrics().map_or(0, |metrics| metrics.area),
            transport_cells: session
                .metrics()
                .map_or(0, |metrics| metrics.transport_cells),
            turns: session.metrics().map_or(0, |metrics| metrics.turns),
        });
    }
    let mut counters = Counters::default();
    let actions = factory.actions(session, &mut counters)?;
    let mut best = Outcome {
        full: false,
        completed: observation.completed,
        total: observation.total,
        moves,
        footprint_area: session.metrics().map_or(0, |metrics| metrics.area),
        transport_cells: session
            .metrics()
            .map_or(0, |metrics| metrics.transport_cells),
        turns: session.metrics().map_or(0, |metrics| metrics.turns),
    };
    for action in actions.into_iter().take(64) {
        let child = factory.apply_action(&action, session)?;
        let outcome = best_outcome(&child, factory, bounds, horizon - 1, moves + 1)?;
        if utility(outcome, bounds) > utility(best, bounds) {
            best = outcome;
        }
    }
    Ok(best)
}

fn utility(outcome: Outcome, bounds: Bounds) -> f32 {
    if outcome.full {
        let canvas = bounds.width() * bounds.height();
        let [area, transport, turns] = normalized_layout_quality(
            outcome.footprint_area,
            outcome.transport_cells,
            outcome.turns,
            canvas,
            outcome.total,
        );
        let canvas = canvas.max(1) as f32;
        let routed = canvas * outcome.total.max(1) as f32;
        let penalty = area + transport / (canvas + 1.0) + turns / ((canvas + 1.0) * (routed + 1.0));
        4.0 + canvas * (1.0 - penalty)
    } else {
        outcome.completed as f32 / outcome.total.max(1) as f32 - outcome.moves as f32 * 0.001
    }
}

fn training_loss(
    prediction: &crate::outer_model::Prediction,
    batch: &Batch,
    examples: &[&Example],
    device: Device,
) -> Result<Tensor> {
    let mut policy_losses = Vec::with_capacity(examples.len());
    for (state, example) in examples.iter().enumerate() {
        let range = batch.action_range(state);
        let target = Tensor::from_slice(&example.policy_target).to_device(device);
        let logp = prediction
            .logp
            .narrow(0, range.start, range.end - range.start);
        policy_losses.push(-(target * logp).sum(Kind::Float));
    }
    let policy = Tensor::stack(&policy_losses, 0).mean(Kind::Float);
    let targets = examples
        .iter()
        .map(|example| example.value_target)
        .collect::<Vec<_>>();
    let value = value_loss(&prediction.values, &targets, device)?;
    Ok(policy + value)
}

fn evaluate(model: &Model, examples: &[Example], device: Device) -> Result<Evaluation> {
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
    let batch = Batch::from_rows(&inputs, &features, &entity_references, device)?;
    let prediction = tch::no_grad(|| model.forward(&batch))?;
    let logits = Vec::<f32>::try_from(prediction.logits.to_device(Device::Cpu))?;
    let mut selected_completing_actions = 0;
    let mut exact_best_actions = 0;
    let mut selected_quality = 0.0;
    let mut oracle_quality = 0.0;
    let mut selected_utility = 0.0;
    let mut oracle_utility = 0.0;
    for (state, example) in examples.iter().enumerate() {
        let range = batch.action_range(state);
        let start = range.start as usize;
        let end = range.end as usize;
        let selected = logits[start..end]
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .expect("nonempty action domain");
        let best = example
            .completion_labels
            .iter()
            .copied()
            .fold(0.0f32, f32::max);
        let exact_best = example
            .utility_labels
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .expect("nonempty policy target");
        let selected_label = example.completion_labels[selected];
        selected_completing_actions += usize::from(selected_label == 1.0);
        exact_best_actions += usize::from(selected == exact_best);
        selected_quality += f64::from(selected_label);
        oracle_quality += f64::from(best);
        selected_utility += f64::from(example.utility_labels[selected]);
        oracle_utility += f64::from(example.utility_labels[exact_best]);
    }
    Ok(Evaluation {
        states: examples.len(),
        macro_action_samples: examples.iter().map(|example| example.features.len()).sum(),
        states_with_completing_action: examples
            .iter()
            .filter(|example| example.completion_labels.contains(&1.0))
            .count(),
        selected_completing_actions,
        exact_best_actions,
        mean_selected_completion: selected_quality / examples.len() as f64,
        mean_oracle_completion: oracle_quality / examples.len() as f64,
        mean_selected_utility: selected_utility / examples.len() as f64,
        mean_oracle_utility: oracle_utility / examples.len() as f64,
    })
}

fn prefix_session(case: &Case, panel: &Panel, catalogs: Arc<Catalogs>) -> Result<Session> {
    let mut session =
        Session::new(case.problem.clone(), catalogs, panel.limits).map_err(anyhow::Error::new)?;
    for command in &case.prefix {
        let step = session.step(command.clone());
        ensure!(
            step.accepted && step.diagnostic.is_none(),
            "invalid prefix in {}",
            case.id
        );
    }
    Ok(session)
}

fn validate_config(config: &Config) -> Result<()> {
    config.model.validate()?;
    ensure!(
        config.schema == CONFIG_SCHEMA
            && (1..=1_000_000).contains(&config.epochs)
            && (1..=4096).contains(&config.batch_states)
            && (1..=16).contains(&config.label_horizon)
            && config.target_temperature.is_finite()
            && config.target_temperature > 0.0
            && config.learning_rate.is_finite()
            && config.learning_rate > 0.0
            && config.gradient_clip.is_finite()
            && config.gradient_clip > 0.0
            && config.seed <= i64::MAX as u64,
        "aic-learning.outer-training-config: invalid training controls"
    );
    Ok(())
}

fn validate_panel(panel: &Panel, partition: &str) -> Result<()> {
    let mut ids = BTreeSet::new();
    ensure!(
        panel.schema == PANEL_SCHEMA
            && panel.partition == partition
            && !panel.cases.is_empty()
            && panel.cases.len() <= 4096
            && panel
                .cases
                .iter()
                .all(|case| !case.id.is_empty() && ids.insert(case.id.as_str())),
        "aic-learning.outer-training-panel: invalid schema, partition or cases"
    );
    Ok(())
}

fn validate_panel_separation(training: &Panel, evaluation: &Panel) -> Result<()> {
    let training_ids = training
        .cases
        .iter()
        .map(|case| case.id.as_str())
        .collect::<BTreeSet<_>>();
    let training_problems = training
        .cases
        .iter()
        .map(|case| serde_json::to_vec(&case.problem))
        .collect::<Result<BTreeSet<_>, _>>()?;
    ensure!(
        evaluation.cases.iter().all(|case| {
            !training_ids.contains(case.id.as_str())
                && serde_json::to_vec(&case.problem)
                    .is_ok_and(|identity| !training_problems.contains(&identity))
        }),
        "aic-learning.outer-training-panels: training and held-out problem identities overlap"
    );
    Ok(())
}

fn synchronize(device: Device) {
    if let Device::Cuda(index) = device {
        tch::Cuda::synchronize(index as i64);
    }
}

fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    ensure!(
        metadata.len() <= MAX_INPUT_BYTES,
        "input too large: {}",
        path.display()
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
        .context("output needs UTF-8 name")?;
    Ok(output.with_file_name(format!(".{name}.pending")))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
}

fn bytes_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
