//! Frozen-panel warm start and held-out evaluation for route guidance.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use aic_layout::{geometry::Point, physical::Problem, validation::Catalogs};
use aic_pilot_native::constructive::{
    Command, Limits, Session,
    learning::Config as LearningConfig,
    routing_candidates::{
        Candidate, Config as CandidateConfig, GuidanceInput, Provenance, Request,
    },
};
use aic_torch::{
    Adam, AdamBackend,
    tch::{self, Device, Kind, Tensor},
};
use anyhow::{Context, Result, ensure};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    input::sha256 as file_sha256,
    route_guidance::{Batch, Model, Prediction},
    schema::RouteGuidanceModelConfig,
};

const CONFIG_SCHEMA: &str = "aic-route-guidance-training-v1";
const PANEL_SCHEMA: &str = "aic-route-guidance-panel-v1";
const REPORT_SCHEMA: &str = "aic-route-guidance-training-report-v1";
const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub model: RouteGuidanceModelConfig,
    pub learning_domain: LearningConfig,
    pub epochs: usize,
    pub batch_cases: usize,
    pub learning_rate: f64,
    pub gradient_clip: f64,
    pub policy_temperature: f64,
    pub target_temperature: f64,
    pub regret_edge_cost: f64,
    pub seed: u64,
    pub cpu_threads: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Panel {
    schema: String,
    partition: String,
    limits: Limits,
    candidate: CandidateConfig,
    cases: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    problem: Problem,
    prefix: Vec<Command>,
    request: String,
    later_connections: Vec<String>,
}

struct Example {
    input: GuidanceInput,
    candidates: Vec<CandidateLabel>,
    later_requested: usize,
}

struct CandidateLabel {
    edge_offsets: Vec<i64>,
    quality: f32,
    later_completed: usize,
}

#[derive(Clone, Debug, Serialize)]
struct Evaluation {
    cases: usize,
    candidate_samples: usize,
    discriminating_cases: usize,
    anchor_mean_quality: f64,
    selected_mean_quality: f64,
    oracle_mean_quality: f64,
    selected_full_survival: usize,
    oracle_full_survival: usize,
    exact_best_selections: usize,
}

#[derive(Clone, Debug, Serialize)]
struct GuidedEvaluation {
    cases: usize,
    root_candidate_samples: usize,
    guided_candidate_samples: usize,
    native_rejections: usize,
    anchor_full_survival: usize,
    guided_full_survival_samples: usize,
    cases_with_guided_full_survival: usize,
}

#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    status: &'static str,
    device: String,
    catalog_sha256: String,
    config_sha256: String,
    training_panel_sha256: String,
    evaluation_panel_sha256: String,
    epochs: usize,
    batches: usize,
    updates: usize,
    training_cases: usize,
    training_candidate_samples: usize,
    processed_candidate_samples: usize,
    sample_definition: &'static str,
    elapsed_seconds: f64,
    samples_per_minute: f64,
    initial_loss: f64,
    final_loss: f64,
    maximum_gradient_norm: f64,
    evaluation_before: Evaluation,
    evaluation_after: Evaluation,
    guided_evaluation_after: GuidedEvaluation,
    checkpoint_sha256: String,
}

pub fn run(
    config_path: &Path,
    training_panel_path: &Path,
    evaluation_panel_path: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
) -> Result<Value> {
    ensure!(
        !output.exists(),
        "aic-learning.route-guidance-output: {} already exists",
        output.display()
    );
    let pending = pending_path(output)?;
    ensure!(
        !pending.exists(),
        "aic-learning.route-guidance-output: pending path {} already exists",
        pending.display()
    );
    let config_bytes = bounded_read(config_path)?;
    let training_bytes = bounded_read(training_panel_path)?;
    let evaluation_bytes = bounded_read(evaluation_panel_path)?;
    let config: Config = serde_json::from_slice(&config_bytes)
        .with_context(|| format!("parse route guidance config {}", config_path.display()))?;
    let training_panel: Panel = serde_json::from_slice(&training_bytes)
        .with_context(|| format!("parse training panel {}", training_panel_path.display()))?;
    let evaluation_panel: Panel = serde_json::from_slice(&evaluation_bytes)
        .with_context(|| format!("parse evaluation panel {}", evaluation_panel_path.display()))?;
    validate_config(&config)?;
    validate_panel(&training_panel, "training")?;
    validate_panel(&evaluation_panel, "held-out")?;
    ensure!(
        training_panel.candidate.bounds == config.learning_domain.placement
            && evaluation_panel.candidate.bounds == config.learning_domain.placement,
        "aic-learning.route-guidance-bounds: config and panel bounds differ"
    );

    let loaded = aic_layout_gen::io::load_catalogs(catalog_root)?;
    let catalogs = Arc::new(loaded.catalogs);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(config.cpu_threads)
        .thread_name(|index| format!("route-guidance-data-{index}"))
        .build()
        .context("aic-learning.route-guidance-workers: build CPU pool")?;
    let training = collect_panel(
        &pool,
        &training_panel,
        &config.learning_domain,
        catalogs.clone(),
    )?;
    let evaluation = collect_panel(
        &pool,
        &evaluation_panel,
        &config.learning_domain,
        catalogs.clone(),
    )?;
    ensure!(
        training.iter().any(discriminating) && evaluation.iter().any(discriminating),
        "aic-learning.route-guidance-labels: both partitions require a discriminating continuation case"
    );

    let (order, state) = Model::initial_state(&config.model, config.seed)?;
    let model = Model::load(&config.model, &order, &state, device)?;
    let backend = if matches!(device, Device::Cuda(_)) {
        AdamBackend::Fused
    } else {
        AdamBackend::Foreach
    };
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
        backend,
    )?;

    let before = evaluate(&model, &evaluation, &config, device)?;
    synchronize(device);
    let started = Instant::now();
    let mut first_loss = None;
    let mut last_loss = 0.0;
    let mut maximum_gradient_norm = 0.0f64;
    let mut batches = 0usize;
    let mut processed_candidate_samples = 0usize;
    for epoch in 0..config.epochs {
        let rotation = (config.seed as usize).wrapping_add(epoch) % training.len();
        let order_indices = (0..training.len())
            .map(|index| (index + rotation) % training.len())
            .collect::<Vec<_>>();
        for indices in order_indices.chunks(config.batch_cases) {
            let examples = indices
                .iter()
                .map(|&index| &training[index])
                .collect::<Vec<_>>();
            let inputs = examples
                .iter()
                .map(|example| &example.input)
                .collect::<Vec<_>>();
            let batch = Batch::from_inputs(&inputs, device)?;
            optimizer.zero_grad()?;
            let prediction = model.forward(&batch)?;
            let loss = loss(&prediction, &batch, &examples, &config, device)?;
            let scalar = f64::try_from(&loss)?;
            ensure!(
                scalar.is_finite(),
                "aic-learning.route-guidance-loss: nonfinite objective"
            );
            first_loss.get_or_insert(scalar);
            last_loss = scalar;
            loss.backward();
            maximum_gradient_norm =
                maximum_gradient_norm.max(optimizer.clip_grad_norm(config.gradient_clip)?);
            optimizer.step()?;
            batches += 1;
            processed_candidate_samples += examples
                .iter()
                .map(|example| example.candidates.len())
                .sum::<usize>();
        }
    }
    synchronize(device);
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let after = evaluate(&model, &evaluation, &config, device)?;
    let guided_after = evaluate_guided(
        &model,
        &evaluation_panel,
        &config.learning_domain,
        catalogs.clone(),
        device,
    )?;

    fs::create_dir_all(&pending)
        .with_context(|| format!("create route guidance output {}", pending.display()))?;
    write_json(&pending.join("config.json"), &config)?;
    write_json(&pending.join("parameter-order.json"), &order)?;
    Tensor::write_safetensors(
        &model.snapshot().iter().collect::<Vec<_>>(),
        pending.join("state.safetensors"),
    )?;
    let checkpoint_sha256 = file_sha256(&pending.join("state.safetensors"))?;
    let report = Report {
        schema: REPORT_SCHEMA,
        status: "complete",
        device: format!("{device:?}"),
        catalog_sha256: catalogs.identity.clone(),
        config_sha256: bytes_sha256(&config_bytes),
        training_panel_sha256: bytes_sha256(&training_bytes),
        evaluation_panel_sha256: bytes_sha256(&evaluation_bytes),
        epochs: config.epochs,
        batches,
        updates: batches,
        training_cases: training.len(),
        training_candidate_samples: training
            .iter()
            .map(|example| example.candidates.len())
            .sum(),
        processed_candidate_samples,
        sample_definition: "native-valid route candidates with exact downstream continuation labels",
        elapsed_seconds,
        samples_per_minute: if elapsed_seconds == 0.0 {
            0.0
        } else {
            processed_candidate_samples as f64 * 60.0 / elapsed_seconds
        },
        initial_loss: first_loss.unwrap_or(0.0),
        final_loss: last_loss,
        maximum_gradient_norm,
        evaluation_before: before,
        evaluation_after: after,
        guided_evaluation_after: guided_after,
        checkpoint_sha256,
    };
    write_json(&pending.join("report.json"), &report)?;
    fs::rename(&pending, output)
        .with_context(|| format!("publish route guidance output {}", output.display()))?;
    Ok(json!({
        "schema": REPORT_SCHEMA,
        "status": "complete",
        "output": output,
        "processed_candidate_samples": report.processed_candidate_samples,
        "samples_per_minute": report.samples_per_minute,
        "selected_full_survival_before": report.evaluation_before.selected_full_survival,
        "selected_full_survival_after": report.evaluation_after.selected_full_survival,
        "guided_full_survival_after": report.guided_evaluation_after.guided_full_survival_samples,
        "checkpoint_sha256": report.checkpoint_sha256,
    }))
}

fn validate_config(config: &Config) -> Result<()> {
    config.model.validate()?;
    ensure!(
        config.schema == CONFIG_SCHEMA
            && (1..=1_000_000).contains(&config.epochs)
            && (1..=4096).contains(&config.batch_cases)
            && config.learning_rate.is_finite()
            && config.learning_rate > 0.0
            && config.gradient_clip.is_finite()
            && config.gradient_clip > 0.0
            && config.policy_temperature.is_finite()
            && config.policy_temperature > 0.0
            && config.target_temperature.is_finite()
            && config.target_temperature > 0.0
            && config.regret_edge_cost.is_finite()
            && config.regret_edge_cost > 0.0
            && config.seed <= i64::MAX as u64
            && (1..=256).contains(&config.cpu_threads),
        "aic-learning.route-guidance-config: invalid schema or training bounds"
    );
    Ok(())
}

fn validate_panel(panel: &Panel, partition: &str) -> Result<()> {
    ensure!(
        panel.schema == PANEL_SCHEMA
            && panel.partition == partition
            && !panel.cases.is_empty()
            && panel.cases.len() <= 4096,
        "aic-learning.route-guidance-panel: invalid schema, partition or case count"
    );
    let mut ids = BTreeSet::new();
    for case in &panel.cases {
        ensure!(
            !case.id.is_empty()
                && ids.insert(case.id.as_str())
                && !case.request.is_empty()
                && !case.later_connections.is_empty()
                && case.prefix.len() <= 1_000_000
                && case.later_connections.len() <= 4096,
            "aic-learning.route-guidance-panel: malformed case {}",
            case.id
        );
    }
    Ok(())
}

fn collect_panel(
    pool: &rayon::ThreadPool,
    panel: &Panel,
    learning: &LearningConfig,
    catalogs: Arc<Catalogs>,
) -> Result<Vec<Example>> {
    let results = pool.install(|| {
        panel
            .cases
            .par_iter()
            .map(|case| collect_case(case, panel, learning, catalogs.clone()))
            .collect::<Vec<_>>()
    });
    results.into_iter().collect()
}

fn collect_case(
    case: &Case,
    panel: &Panel,
    learning: &LearningConfig,
    catalogs: Arc<Catalogs>,
) -> Result<Example> {
    let mut session = Session::new(case.problem.clone(), catalogs, panel.limits)
        .map_err(anyhow::Error::new)
        .with_context(|| format!("admit route guidance case {}", case.id))?;
    for (index, command) in case.prefix.iter().enumerate() {
        let step = session.step(command.clone());
        ensure!(
            step.accepted && step.diagnostic.is_none(),
            "aic-learning.route-guidance-prefix: case {} command {} rejected: {:?}",
            case.id,
            index,
            step.diagnostic
        );
    }
    let request = Request {
        state_revision: session.observe().revision,
        connection: case.request.clone(),
    };
    let input = session
        .route_guidance_input(&request, &panel.candidate, learning)
        .map_err(anyhow::Error::new)?;
    let report = session
        .route_candidates(&request, &panel.candidate)
        .map_err(anyhow::Error::new)?;
    ensure!(
        report.candidates.len() >= 2,
        "aic-learning.route-guidance-candidates: case {} produced fewer than two candidates",
        case.id
    );
    let width = panel.candidate.bounds.width();
    let area = width * panel.candidate.bounds.height();
    let candidates = report
        .candidates
        .iter()
        .map(|candidate| {
            let later_completed = continuation(&session, candidate, case, &panel.candidate)?;
            let edge_offsets = candidate
                .cells
                .windows(2)
                .map(|edge| {
                    let direction = direction(edge[0], edge[1]);
                    let cell = (edge[1].y - panel.candidate.bounds.min_y) * width + edge[1].x
                        - panel.candidate.bounds.min_x;
                    i64::try_from(direction).expect("cardinal direction") * area + cell
                })
                .collect::<Vec<_>>();
            ensure!(
                !edge_offsets.is_empty(),
                "aic-learning.route-guidance-candidate: zero-edge candidate in {}",
                case.id
            );
            Ok(CandidateLabel {
                edge_offsets,
                quality: later_completed as f32 / case.later_connections.len() as f32,
                later_completed,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Example {
        input,
        candidates,
        later_requested: case.later_connections.len(),
    })
}

fn continuation(
    root: &Session,
    candidate: &Candidate,
    case: &Case,
    config: &CandidateConfig,
) -> Result<usize> {
    let mut session = root.clone();
    session
        .apply_route_candidate(candidate)
        .map_err(anyhow::Error::new)?;
    let mut completed = 0;
    for connection in &case.later_connections {
        let report = session
            .route_candidates(
                &Request {
                    state_revision: session.observe().revision,
                    connection: connection.clone(),
                },
                config,
            )
            .map_err(anyhow::Error::new)?;
        let Some(candidate) = report.candidates.first() else {
            break;
        };
        session
            .apply_route_candidate(candidate)
            .map_err(anyhow::Error::new)?;
        completed += 1;
    }
    Ok(completed)
}

fn loss(
    prediction: &Prediction,
    batch: &Batch,
    examples: &[&Example],
    config: &Config,
    device: Device,
) -> Result<Tensor> {
    let (width, height) = batch.dimensions();
    let area = width * height;
    let heads = config.model.heads as i64;
    let flat = prediction.directed_costs.view([-1]);
    let mut policy_losses = Vec::with_capacity(examples.len());
    let mut regression_losses = Vec::new();
    for (input, example) in examples.iter().enumerate() {
        let best = example
            .candidates
            .iter()
            .map(|candidate| candidate.later_completed)
            .max()
            .expect("case has candidates");
        let mut energies = Vec::with_capacity(example.candidates.len());
        let mut targets = Vec::with_capacity(example.candidates.len());
        for (candidate_index, candidate) in example.candidates.iter().enumerate() {
            let head = candidate_index as i64 % heads;
            let base = (input as i64 * heads + head) * 4 * area;
            let indices = Tensor::from_slice(
                &candidate
                    .edge_offsets
                    .iter()
                    .map(|offset| base + offset)
                    .collect::<Vec<_>>(),
            )
            .to_device(device);
            let energy = flat.index_select(0, &indices).mean(Kind::Float);
            let target = (best - candidate.later_completed) as f64 * config.regret_edge_cost;
            regression_losses.push((&energy - target).square());
            energies.push(energy);
            targets.push(candidate.quality);
        }
        let energies = Tensor::stack(&energies, 0);
        let target = (Tensor::from_slice(&targets).to_device(device) / config.target_temperature)
            .softmax(-1, Kind::Float);
        let logp = (-energies / config.policy_temperature).log_softmax(-1, Kind::Float);
        policy_losses.push(-(target * logp).sum(Kind::Float));
    }
    let policy = Tensor::stack(&policy_losses, 0).mean(Kind::Float);
    let regression = Tensor::stack(&regression_losses, 0).mean(Kind::Float);
    let targets = Tensor::from_slice(
        &examples
            .iter()
            .map(|example| {
                example
                    .candidates
                    .iter()
                    .map(|candidate| candidate.quality)
                    .fold(0.0f32, f32::max)
            })
            .collect::<Vec<_>>(),
    )
    .to_device(device)
    .unsqueeze(-1)
    .expand([-1, config.model.heads as i64], false);
    let success = prediction.success.clamp(1e-5, 1.0 - 1e-5);
    let outcome = -(&targets * success.log() + (1.0f64 - &targets) * (1.0f64 - success).log())
        .mean(Kind::Float);
    let uncertainty_target = (prediction.success.detach() - &targets).abs();
    let uncertainty = (&prediction.uncertainty - uncertainty_target)
        .square()
        .mean(Kind::Float);
    Ok(policy + regression * 0.1 + outcome + uncertainty * 0.1)
}

fn evaluate(
    model: &Model,
    examples: &[Example],
    config: &Config,
    device: Device,
) -> Result<Evaluation> {
    let inputs = examples
        .iter()
        .map(|example| &example.input)
        .collect::<Vec<_>>();
    let batch = Batch::from_inputs(&inputs, device)?;
    let prediction = tch::no_grad(|| model.forward(&batch))?;
    let (width, height) = batch.dimensions();
    let area = width * height;
    let heads = config.model.heads as i64;
    let flat = prediction.directed_costs.to_device(Device::Cpu).view([-1]);
    let mut anchor = 0.0;
    let mut selected = 0.0;
    let mut oracle = 0.0;
    let mut selected_full = 0;
    let mut oracle_full = 0;
    let mut exact_best = 0;
    for (input, example) in examples.iter().enumerate() {
        let energies = example
            .candidates
            .iter()
            .enumerate()
            .map(|(candidate_index, candidate)| {
                let head = candidate_index as i64 % heads;
                let base = (input as i64 * heads + head) * 4 * area;
                candidate
                    .edge_offsets
                    .iter()
                    .map(|offset| flat.double_value(&[base + offset]))
                    .sum::<f64>()
                    / candidate.edge_offsets.len() as f64
            })
            .collect::<Vec<_>>();
        let selected_index = energies
            .iter()
            .enumerate()
            .min_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .expect("case has candidates");
        let best = example
            .candidates
            .iter()
            .map(|candidate| candidate.quality)
            .fold(0.0f32, f32::max);
        let selected_quality = example.candidates[selected_index].quality;
        anchor += f64::from(example.candidates[0].quality);
        selected += f64::from(selected_quality);
        oracle += f64::from(best);
        selected_full += usize::from(
            example.candidates[selected_index].later_completed == example.later_requested,
        );
        oracle_full += usize::from(best == 1.0);
        exact_best += usize::from(selected_quality == best);
    }
    let count = examples.len() as f64;
    Ok(Evaluation {
        cases: examples.len(),
        candidate_samples: examples
            .iter()
            .map(|example| example.candidates.len())
            .sum(),
        discriminating_cases: examples
            .iter()
            .filter(|example| discriminating(example))
            .count(),
        anchor_mean_quality: anchor / count,
        selected_mean_quality: selected / count,
        oracle_mean_quality: oracle / count,
        selected_full_survival: selected_full,
        oracle_full_survival: oracle_full,
        exact_best_selections: exact_best,
    })
}

fn evaluate_guided(
    model: &Model,
    panel: &Panel,
    learning: &LearningConfig,
    catalogs: Arc<Catalogs>,
    device: Device,
) -> Result<GuidedEvaluation> {
    let mut states = Vec::with_capacity(panel.cases.len());
    for case in &panel.cases {
        let mut session = Session::new(case.problem.clone(), catalogs.clone(), panel.limits)
            .map_err(anyhow::Error::new)?;
        for command in &case.prefix {
            let step = session.step(command.clone());
            ensure!(
                step.accepted && step.diagnostic.is_none(),
                "aic-learning.route-guidance-prefix: guided evaluation rejected {}",
                case.id
            );
        }
        let request = Request {
            state_revision: session.observe().revision,
            connection: case.request.clone(),
        };
        let input = session
            .route_guidance_input(&request, &panel.candidate, learning)
            .map_err(anyhow::Error::new)?;
        states.push((session, request, input));
    }
    let input_refs = states.iter().map(|state| &state.2).collect::<Vec<_>>();
    let guidance = model.predict_native(&input_refs, "warm-start-final", device)?;
    let mut root_candidate_samples = 0;
    let mut guided_candidate_samples = 0;
    let mut native_rejections = 0;
    let mut anchor_full_survival = 0;
    let mut guided_full_survival_samples = 0;
    let mut cases_with_guided_full_survival = 0;
    for ((session, request, _), (case, guidance)) in
        states.iter().zip(panel.cases.iter().zip(&guidance))
    {
        let report = session
            .route_candidates_with_guidance(request, &panel.candidate, guidance)
            .map_err(anyhow::Error::new)?;
        root_candidate_samples += report.candidates.len();
        native_rejections += report.native_rejections.len();
        let mut case_guided_full = false;
        for candidate in &report.candidates {
            let completed = continuation(session, candidate, case, &panel.candidate)?;
            let full = completed == case.later_connections.len();
            match &candidate.provenance {
                Provenance::Anchor => anchor_full_survival += usize::from(full),
                Provenance::Guided { .. } | Provenance::Diversified { .. } => {
                    guided_candidate_samples += 1;
                    guided_full_survival_samples += usize::from(full);
                    case_guided_full |= full;
                }
            }
        }
        cases_with_guided_full_survival += usize::from(case_guided_full);
    }
    Ok(GuidedEvaluation {
        cases: panel.cases.len(),
        root_candidate_samples,
        guided_candidate_samples,
        native_rejections,
        anchor_full_survival,
        guided_full_survival_samples,
        cases_with_guided_full_survival,
    })
}

fn discriminating(example: &Example) -> bool {
    let mut values = example
        .candidates
        .iter()
        .map(|candidate| candidate.later_completed);
    let Some(first) = values.next() else {
        return false;
    };
    values.any(|value| value != first)
}

fn direction(from: Point, to: Point) -> usize {
    match (to.x - from.x, to.y - from.y) {
        (1, 0) => 0,
        (0, 1) => 1,
        (-1, 0) => 2,
        (0, -1) => 3,
        _ => unreachable!("native route candidate has cardinal unit edges"),
    }
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
        "aic-learning.route-guidance-input: {} exceeds {} bytes",
        path.display(),
        MAX_INPUT_BYTES
    );
    fs::read(path).with_context(|| format!("read {}", path.display()))
}

fn pending_path(output: &Path) -> Result<PathBuf> {
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .context("aic-learning.route-guidance-output: output needs a UTF-8 name")?;
    Ok(output.with_file_name(format!(".{name}.pending")))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("write {}", path.display()))
}

fn bytes_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
