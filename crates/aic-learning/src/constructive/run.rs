//! Bounded direct-construction training pilot over logical problem sets.

use std::{
    fs::{self, File},
    io::{BufWriter, Read, Write},
    path::Path,
    sync::Arc,
};

use aic_layout::{
    facilities::{FacilityCatalog, ValidatedFacilityCatalog},
    geometry::Point,
    logistics::{
        LogisticsComponentCatalog, TransportCatalog, TransportKind,
        ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    },
    physical::Layout,
    projection::rotate,
    validation::Catalogs,
};
use aic_pilot_native::constructive::{Command, Limits, Session, learning::Config as DomainConfig};
use aic_torch::{
    deterministic,
    tch::{self, Device, Tensor},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    constructive_problems::{PreparedProblem, ProblemSet},
    input::{Tensors, sha256},
    schema::{ConstructiveModelConfig, UpdateConfig},
    update::device_name,
};

use super::{
    Model, policy,
    ppo::Learner,
    rollout::{self, Collection, RewardConfig, Sampling},
};

const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub model: ConstructiveModelConfig,
    pub optimizer: UpdateConfig,
    pub domain: DomainConfig,
    pub native: Limits,
    pub reward: RewardConfig,
    pub updates: u64,
    pub episodes_per_update: usize,
    pub ppo_epochs: usize,
    pub ppo_minibatch_size: usize,
    pub evaluation_episodes: usize,
    pub evaluation_seed: u64,
    pub gamma: f64,
    pub gae_lambda: f64,
    pub advantage_normalization: String,
    pub initialization_seed: u64,
}

#[derive(Serialize)]
struct Summary {
    episodes: usize,
    turns: usize,
    accepted: usize,
    fully_validated: usize,
    dead_ends: usize,
    mean_completion_ratio: f64,
    mean_reward: f64,
    seconds: f64,
}

#[derive(Serialize)]
struct EvaluationCase {
    problem_index: usize,
    problem_id: String,
    target_item: String,
    size_band: String,
    facility_count: usize,
    fully_validated: bool,
    turns: u64,
    completed_connections: usize,
    total_connections: usize,
    terminal_reason: String,
    layout_file: Option<String>,
    visualization_file: Option<String>,
    commands: Vec<Command>,
}

struct EvaluatedCase {
    summary: EvaluationCase,
    layout: Option<Layout>,
}

pub fn run(
    config_path: &Path,
    catalog_root: &Path,
    problem_path: &Path,
    output: &Path,
    device: Device,
) -> Result<Value> {
    let config: Config = read_json(config_path)?;
    let problems: ProblemSet = read_json(problem_path)?;
    validate(&config, &problems)?;
    ensure!(
        !output.exists(),
        "aic-learning.constructive-output: {} already exists",
        output.display()
    );
    let catalogs = load_catalogs(catalog_root)?;
    // Admit every immutable problem before creating output or initializing Torch.
    for problem in problems.training.iter().chain(&problems.evaluation) {
        Session::new(problem.problem.clone(), catalogs.clone(), config.native)
            .map_err(anyhow::Error::new)?;
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
    write_json(&output.join("problems.json"), &problems)?;
    write_json(&output.join("parameter-order.json"), &parameter_order)?;
    let mut metrics = BufWriter::new(File::create(output.join("metrics.jsonl"))?);
    let initial_greedy = evaluate(
        &problems.evaluation,
        catalogs.clone(),
        &config,
        &mut learner,
        problems.evaluation.len(),
        Sampling::GreedyEvaluation,
    )?;
    let initial_stochastic = evaluate(
        &problems.evaluation,
        catalogs.clone(),
        &config,
        &mut learner,
        config.evaluation_episodes,
        Sampling::StochasticEvaluation {
            seed: config.evaluation_seed,
        },
    )?;
    write_row(
        &mut metrics,
        &json!({
            "update":0,
            "evaluation_greedy":summary(&initial_greedy),
            "evaluation_stochastic":summary(&initial_stochastic),
        }),
    )?;
    let mut total_episodes = 0u64;
    let mut total_turns = 0u64;
    let mut final_greedy = summary(&initial_greedy);
    let mut final_stochastic = summary(&initial_stochastic);
    for update in 1..=config.updates {
        let offset = (update as usize - 1)
            .checked_mul(config.episodes_per_update)
            .ok_or_else(|| anyhow::anyhow!("aic-learning.constructive-schedule: overflow"))?;
        let mut sessions = (0..config.episodes_per_update)
            .map(|index| {
                let problem = &problems.training[(offset + index) % problems.training.len()];
                Session::new(problem.problem.clone(), catalogs.clone(), config.native)
                    .map_err(anyhow::Error::new)
            })
            .collect::<Result<Vec<_>>>()?;
        let collection = rollout::collect(
            &mut sessions,
            &config.domain,
            &config.reward,
            &mut learner,
            Sampling::Training,
        )?;
        let credit = rollout::gae(&collection, config.gamma, config.gae_lambda)?;
        let samples = rollout::ppo_samples(&collection, &credit)?;
        let mut epochs = Vec::with_capacity(config.ppo_epochs);
        for _ in 0..config.ppo_epochs {
            epochs.push(learner.update_epoch(
                &samples,
                &config.advantage_normalization,
                config.ppo_minibatch_size,
            )?);
        }
        let evaluation_greedy = evaluate(
            &problems.evaluation,
            catalogs.clone(),
            &config,
            &mut learner,
            problems.evaluation.len(),
            Sampling::GreedyEvaluation,
        )?;
        let evaluation_stochastic = evaluate(
            &problems.evaluation,
            catalogs.clone(),
            &config,
            &mut learner,
            config.evaluation_episodes,
            Sampling::StochasticEvaluation {
                seed: config.evaluation_seed,
            },
        )?;
        total_episodes += collection.episodes.len() as u64;
        total_turns += collection.turns as u64;
        final_greedy = summary(&evaluation_greedy);
        final_stochastic = summary(&evaluation_stochastic);
        write_row(
            &mut metrics,
            &json!({
                "update":update,
                "training":summary(&collection),
                "ppo":epochs,
                "evaluation_greedy":final_greedy,
                "evaluation_stochastic":final_stochastic,
            }),
        )?;
        metrics.flush()?;
    }
    metrics.flush()?;
    let model_path = output.join("learner.safetensors");
    let snapshot = learner.snapshot()?;
    Tensor::write_safetensors(&snapshot.iter().collect::<Vec<_>>(), &model_path)?;
    let report = json!({
        "schema":"aic-constructive-training-report-v1",
        "status":"completed",
        "updates":config.updates,
        "optimizer_steps":learner.completed_steps,
        "training_episodes":total_episodes,
        "training_turns":total_turns,
        "final_evaluation_greedy":final_greedy,
        "final_evaluation_stochastic":final_stochastic,
        "parameter_count":learner.model.parameters.iter().map(|(_, value)| value.numel() as u64).sum::<u64>(),
        "device":device_name(device)?,
        "determinism":"strict",
        "config_sha256":sha256(config_path)?,
        "problems_sha256":sha256(problem_path)?,
        "catalog_sha256":catalogs.identity,
        "learner_sha256":sha256(&model_path)?,
        "files":["config.json","problems.json","parameter-order.json","metrics.jsonl","learner.safetensors","report.json"],
    });
    write_json(&output.join("report.json"), &report)?;
    Ok(report)
}

pub fn evaluate_checkpoint(
    training_output: &Path,
    catalog_root: &Path,
    output: &Path,
    cases: usize,
    device: Device,
) -> Result<Value> {
    ensure!(
        training_output.is_dir(),
        "aic-learning.constructive-evaluation-input: missing {}",
        training_output.display()
    );
    ensure!(
        !output.exists(),
        "aic-learning.constructive-evaluation-output: {} already exists",
        output.display()
    );
    let config: Config = read_json(&training_output.join("config.json"))?;
    let problems: ProblemSet = read_json(&training_output.join("problems.json"))?;
    let parameter_order: Vec<String> = read_json(&training_output.join("parameter-order.json"))?;
    let training_report: Value = read_json(&training_output.join("report.json"))?;
    let checkpoint = training_output.join("learner.safetensors");
    let learner_sha256 = sha256(&checkpoint)?;
    validate(&config, &problems)?;
    ensure!(
        (1..=problems.evaluation.len()).contains(&cases),
        "aic-learning.constructive-evaluation-cases: requested {cases}, available {}",
        problems.evaluation.len()
    );
    let catalogs = load_catalogs(catalog_root)?;
    ensure!(
        training_report["schema"] == "aic-constructive-training-report-v1"
            && training_report["status"] == "completed"
            && training_report["catalog_sha256"].as_str() == Some(&catalogs.identity)
            && training_report["learner_sha256"].as_str() == Some(learner_sha256.as_str()),
        "aic-learning.constructive-evaluation-identity: training report, catalogs or checkpoint do not match"
    );
    for problem in problems.evaluation.iter().take(cases) {
        Session::new(problem.problem.clone(), catalogs.clone(), config.native)
            .map_err(anyhow::Error::new)?;
    }
    tch::set_num_threads(1);
    tch::set_num_interop_threads(1);
    deterministic()?;
    let values: Tensors = Tensor::read_safetensors(&checkpoint)?.into_iter().collect();
    let model = Model::load(&config.model, &parameter_order, &values, device)?;
    let evaluated = greedy_cases(
        &problems.evaluation[..cases],
        catalogs.clone(),
        &config,
        &model,
        device,
    )?;
    fs::create_dir_all(output)?;
    for case in &evaluated {
        if let (Some(file), Some(layout)) = (&case.summary.layout_file, &case.layout) {
            write_json(&output.join(file), layout)?;
        }
        if let (Some(file), Some(layout)) = (&case.summary.visualization_file, &case.layout) {
            fs::write(output.join(file), render_layout_svg(layout, &catalogs)?)?;
        }
    }
    let summaries = evaluated
        .iter()
        .map(|case| &case.summary)
        .collect::<Vec<_>>();
    let successful = evaluated
        .iter()
        .filter(|case| case.summary.fully_validated)
        .count();
    let report = json!({
        "schema":"aic-constructive-checkpoint-evaluation-v1",
        "status":"completed",
        "sampling":"greedy",
        "cases":summaries,
        "requested_cases":cases,
        "fully_validated":successful,
        "device":device_name(device)?,
        "catalog_sha256":catalogs.identity,
        "learner_sha256":learner_sha256,
        "training_output":training_output,
        "gallery":"index.html",
    });
    write_json(&output.join("report.json"), &report)?;
    fs::write(
        output.join("index.html"),
        render_gallery(&evaluated, &catalogs, &report)?,
    )?;
    Ok(report)
}

fn greedy_cases(
    problems: &[PreparedProblem],
    catalogs: Arc<Catalogs>,
    config: &Config,
    model: &Model,
    device: Device,
) -> Result<Vec<EvaluatedCase>> {
    let mut sessions = problems
        .iter()
        .map(|problem| {
            Session::new(problem.problem.clone(), catalogs.clone(), config.native)
                .map_err(anyhow::Error::new)
        })
        .collect::<Result<Vec<_>>>()?;
    let mut commands = vec![Vec::new(); sessions.len()];
    let mut terminal_reasons = vec![None; sessions.len()];
    let mut active = (0..sessions.len()).collect::<Vec<_>>();
    while !active.is_empty() {
        let mut owners = Vec::new();
        let mut observations = Vec::new();
        for &owner in &active {
            match sessions[owner].learning_observation(&config.domain) {
                Ok(observation) => {
                    owners.push(owner);
                    observations.push(observation);
                }
                Err(diagnostic) if diagnostic.code == "constructive.empty-learning-domain" => {
                    terminal_reasons[owner] = Some("empty-learning-domain".to_string());
                }
                Err(diagnostic) => return Err(diagnostic.into()),
            }
        }
        if owners.is_empty() {
            break;
        }
        let rows = observations.iter().collect::<Vec<_>>();
        let choices = policy::greedy(model, &rows, device)?;
        for (owner, choice) in owners.into_iter().zip(choices) {
            commands[owner].push(choice.command.command.clone());
            sessions[owner].step_selected(choice.command);
        }
        active.retain(|&owner| !sessions[owner].done() && terminal_reasons[owner].is_none());
    }
    Ok(sessions
        .into_iter()
        .enumerate()
        .map(|(problem_index, session)| {
            let observation = session.observe();
            let layout = session.layout();
            let terminal_reason = terminal_reasons[problem_index].take().unwrap_or_else(|| {
                if observation.fully_validated {
                    "fully-validated".to_string()
                } else if observation.turns >= observation.max_turns {
                    "turn-limit".to_string()
                } else {
                    "terminal-failure".to_string()
                }
            });
            EvaluatedCase {
                summary: EvaluationCase {
                    problem_index,
                    problem_id: problems[problem_index].id.clone(),
                    target_item: problems[problem_index].target_item.clone(),
                    size_band: problems[problem_index].size_band.clone(),
                    facility_count: problems[problem_index].facility_count,
                    fully_validated: observation.fully_validated,
                    turns: observation.turns,
                    completed_connections: observation.completed,
                    total_connections: observation.total,
                    terminal_reason,
                    layout_file: layout
                        .as_ref()
                        .map(|_| format!("case-{problem_index:03}.layout.json")),
                    visualization_file: layout
                        .as_ref()
                        .map(|_| format!("case-{problem_index:03}.svg")),
                    commands: std::mem::take(&mut commands[problem_index]),
                },
                layout,
            }
        })
        .collect())
}

fn render_gallery(cases: &[EvaluatedCase], catalogs: &Catalogs, report: &Value) -> Result<String> {
    let mut cards = String::new();
    for case in cases {
        let status = if case.summary.fully_validated {
            "validated"
        } else {
            "unfinished"
        };
        let drawing = match &case.layout {
            Some(layout) => render_layout_svg(layout, catalogs)?,
            None => format!(
                "<div class=\"empty\">No validated layout ({})</div>",
                escape_html(&case.summary.terminal_reason)
            ),
        };
        cards.push_str(&format!(
            "<section><header><h2>Held-out case {}</h2><span class=\"{}\">{}</span></header><p>{} turns · {}/{} connections</p>{}</section>",
            case.summary.problem_index,
            status,
            status,
            case.summary.turns,
            case.summary.completed_connections,
            case.summary.total_connections,
            drawing,
        ));
    }
    let report_json = escape_html(&serde_json::to_string_pretty(report)?);
    Ok(format!(
        r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Constructive checkpoint layouts</title><style>
:root{{color-scheme:dark}}body{{margin:0;padding:28px;background:#07121d;color:#dceaf5;font:14px ui-monospace,SFMono-Regular,Menlo,monospace}}main{{max-width:1200px;margin:auto}}h1{{font:700 26px system-ui,sans-serif;margin:0 0 8px}}.lede{{color:#8facbf;margin:0 0 24px}}section{{background:#0c1b29;border:1px solid #29465b;border-radius:12px;padding:18px;margin:0 0 20px;box-shadow:0 10px 30px #0005}}header{{display:flex;align-items:center;gap:12px}}h2{{font:650 18px system-ui,sans-serif;margin:0;flex:1}}header span{{padding:4px 8px;border-radius:999px;font-weight:700}}.validated{{background:#143d32;color:#7ff0bd}}.unfinished{{background:#452629;color:#ff9da4}}section p{{color:#92adbf}}svg{{display:block;width:100%;height:auto;max-height:680px;background:#061019;border-radius:8px}}.empty{{padding:50px;text-align:center;color:#d58c93;background:#190f15;border-radius:8px}}details{{color:#9cb5c6}}pre{{white-space:pre-wrap;overflow:auto;background:#061019;padding:14px;border-radius:8px}}
</style></head><body><main><h1>Greedy layouts from the trained constructive policy</h1><p class="lede">Every displayed layout was produced from an empty held-out problem and passed the independent physical validator.</p>{cards}<details><summary>Evaluation report</summary><pre>{report_json}</pre></details></main></body></html>"#
    ))
}

fn render_layout_svg(layout: &Layout, catalogs: &Catalogs) -> Result<String> {
    let mut min_x = i64::MAX;
    let mut min_y = i64::MAX;
    let mut max_x = i64::MIN;
    let mut max_y = i64::MIN;
    let mut facilities = Vec::new();
    for pose in &layout.solution.poses {
        let block = layout
            .problem
            .blocks
            .iter()
            .find(|block| block.id == pose.block)
            .context("layout pose references missing block")?;
        let definition = catalogs
            .facilities
            .facility(&block.facility)
            .context("layout block references missing facility")?;
        let (width, height, _) = rotate(definition, pose.rotation, pose.position);
        min_x = min_x.min(pose.position.x);
        min_y = min_y.min(pose.position.y);
        max_x = max_x.max(pose.position.x + width - 1);
        max_y = max_y.max(pose.position.y + height - 1);
        facilities.push((pose, block, width, height));
    }
    for route in &layout.solution.routes {
        for point in &route.cells {
            min_x = min_x.min(point.x);
            min_y = min_y.min(point.y);
            max_x = max_x.max(point.x);
            max_y = max_y.max(point.y);
        }
    }
    ensure!(
        min_x <= max_x && min_y <= max_y,
        "aic-learning.constructive-render: empty layout"
    );
    const CELL: i64 = 32;
    const PAD: i64 = 2;
    let origin = Point {
        x: min_x - PAD,
        y: min_y - PAD,
    };
    let width = (max_x - min_x + 1 + 2 * PAD) * CELL;
    let height = (max_y - min_y + 1 + 2 * PAD) * CELL;
    let coordinate = |point: Point| {
        (
            (point.x - origin.x) * CELL + CELL / 2,
            (point.y - origin.y) * CELL + CELL / 2,
        )
    };
    let mut drawing = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"Validated factory layout\"><defs><pattern id=\"grid\" width=\"{CELL}\" height=\"{CELL}\" patternUnits=\"userSpaceOnUse\"><path d=\"M {CELL} 0 L 0 0 0 {CELL}\" fill=\"none\" stroke=\"#183044\" stroke-width=\"1\"/></pattern></defs><rect width=\"100%\" height=\"100%\" fill=\"#061019\"/><rect width=\"100%\" height=\"100%\" fill=\"url(#grid)\"/>"
    );
    for route in &layout.solution.routes {
        let connection = layout
            .problem
            .connections
            .iter()
            .find(|connection| connection.id == route.connection)
            .context("layout route references missing connection")?;
        let color = match connection.transport {
            TransportKind::Belt => "#ffb454",
            TransportKind::Pipe => "#5bd6ff",
        };
        let points = route
            .cells
            .iter()
            .map(|&point| {
                let (x, y) = coordinate(point);
                format!("{x},{y}")
            })
            .collect::<Vec<_>>()
            .join(" ");
        drawing.push_str(&format!(
            "<polyline points=\"{points}\" fill=\"none\" stroke=\"#02070b\" stroke-width=\"14\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/><polyline points=\"{points}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"8\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"
        ));
    }
    for (pose, block, facility_width, facility_height) in facilities {
        let x = (pose.position.x - origin.x) * CELL + 2;
        let y = (pose.position.y - origin.y) * CELL + 2;
        let rect_width = facility_width * CELL - 4;
        let rect_height = facility_height * CELL - 4;
        let role = if block.id.ends_with("-source") {
            "SOURCE"
        } else if block.id.ends_with("-target") {
            "TARGET"
        } else {
            "FACILITY"
        };
        let label_chars = usize::try_from(rect_width / 7).unwrap_or(8).clamp(8, 28);
        let facility_label = escape_html(&short_label(&block.facility, label_chars));
        let full_label = escape_html(&format!("{} · {}", block.id, block.facility));
        drawing.push_str(&format!(
            "<g><title>{full_label}</title><rect x=\"{x}\" y=\"{y}\" width=\"{rect_width}\" height=\"{rect_height}\" rx=\"5\" fill=\"#3c5b90\" stroke=\"#9ebeff\" stroke-width=\"2\"/><text x=\"{}\" y=\"{}\" text-anchor=\"middle\" fill=\"#f5f8ff\" font-family=\"ui-monospace,monospace\"><tspan x=\"{}\" font-size=\"13\" font-weight=\"700\">{role}</tspan><tspan x=\"{}\" dy=\"17\" font-size=\"10\">{facility_label}</tspan></text></g>",
            x + rect_width / 2,
            y + rect_height / 2 - 7,
            x + rect_width / 2,
            x + rect_width / 2,
        ));
    }
    drawing.push_str("</svg>");
    Ok(drawing)
}

fn short_label(value: &str, max_chars: usize) -> String {
    let mut label = value.chars().take(max_chars).collect::<String>();
    if value.chars().count() > max_chars {
        label.push('…');
    }
    label
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn validate(config: &Config, problems: &ProblemSet) -> Result<()> {
    ensure!(
        config.schema == "aic-constructive-training-v3"
            && problems.schema == "aic-constructive-problem-set-v2"
            && !problems.training.is_empty()
            && problems.training.len() <= 4096
            && !problems.evaluation.is_empty()
            && problems.evaluation.len() <= 4096
            && problems
                .evaluation
                .iter()
                .all(|problem| !problems.training.contains(problem))
            && (1..=1024).contains(&config.updates)
            && (2..=4096).contains(&config.episodes_per_update)
            && (1..=16).contains(&config.ppo_epochs)
            && (1..=262_144).contains(&config.ppo_minibatch_size)
            && (problems.evaluation.len()..=4096).contains(&config.evaluation_episodes)
            && (0. ..=1.).contains(&config.gamma)
            && (0. ..=1.).contains(&config.gae_lambda)
            && matches!(
                config.advantage_normalization.as_str(),
                "center" | "standardize"
            ),
        "aic-learning.constructive-config: invalid pilot configuration"
    );
    config.model.validate()?;
    config.optimizer.validate()?;
    config.reward.validate()?;
    Ok(())
}

fn evaluate(
    problems: &[PreparedProblem],
    catalogs: Arc<Catalogs>,
    config: &Config,
    learner: &mut Learner,
    episodes: usize,
    sampling: Sampling,
) -> Result<Collection> {
    let mut sessions = (0..episodes)
        .map(|index| {
            let problem = &problems[index % problems.len()];
            Session::new(problem.problem.clone(), catalogs.clone(), config.native)
                .map_err(anyhow::Error::new)
        })
        .collect::<Result<Vec<_>>>()?;
    rollout::collect(
        &mut sessions,
        &config.domain,
        &config.reward,
        learner,
        sampling,
    )
}

fn summary(collection: &Collection) -> Summary {
    Summary {
        episodes: collection.episodes.len(),
        turns: collection.turns,
        accepted: collection.accepted,
        fully_validated: collection.completed,
        dead_ends: collection
            .episodes
            .iter()
            .filter(|episode| episode.dead_end)
            .count(),
        mean_completion_ratio: collection
            .episodes
            .iter()
            .map(|episode| episode.completed_connections as f64 / episode.total_connections as f64)
            .sum::<f64>()
            / collection.episodes.len() as f64,
        mean_reward: collection
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
            / collection.episodes.len() as f64,
        seconds: collection.seconds,
    }
}

pub(super) fn load_catalogs(root: &Path) -> Result<Arc<Catalogs>> {
    let facilities: FacilityCatalog = read_json(&root.join("facilities.json"))?;
    let transports: TransportCatalog = read_json(&root.join("transports.json"))?;
    let components: LogisticsComponentCatalog = read_json(&root.join("logistics-components.json"))?;
    let identity_bytes = serde_json::to_vec(&(&facilities, &transports, &components))?;
    let identity = Sha256::digest(identity_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(Arc::new(Catalogs {
        identity,
        facilities: ValidatedFacilityCatalog::try_from_catalog(facilities)
            .map_err(|report| anyhow::anyhow!("aic-learning.constructive-catalog: {report:?}"))?,
        transports: ValidatedTransportCatalog::try_from_catalog(transports)
            .map_err(|report| anyhow::anyhow!("aic-learning.constructive-catalog: {report:?}"))?,
        components: ValidatedLogisticsComponentCatalog::try_from_catalog(components)
            .map_err(|report| anyhow::anyhow!("aic-learning.constructive-catalog: {report:?}"))?,
    }))
}

pub(super) fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    ensure!(
        file.metadata()?.len() <= MAX_INPUT_BYTES,
        "aic-learning.constructive-input: {} exceeds 16 MiB",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_INPUT_BYTES,
        "aic-learning.constructive-input: {} exceeds 16 MiB",
        path.display()
    );
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

pub(super) fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn write_row(writer: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *writer, value)?;
    writer.write_all(b"\n")?;
    Ok(())
}
