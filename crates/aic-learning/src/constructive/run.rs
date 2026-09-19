//! Bounded direct-construction training pilot over logical problem sets.

use std::{
    fs::{self, File},
    io::{BufWriter, Read, Write},
    path::Path,
    sync::Arc,
};

use aic_layout::{
    facilities::{FacilityCatalog, ValidatedFacilityCatalog},
    logistics::{
        LogisticsComponentCatalog, TransportCatalog, ValidatedLogisticsComponentCatalog,
        ValidatedTransportCatalog,
    },
    physical::Problem,
    validation::Catalogs,
};
use aic_pilot_native::constructive::{Limits, Session, learning::Config as DomainConfig};
use aic_torch::{
    deterministic,
    tch::{self, Device, Tensor},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    input::sha256,
    schema::{ConstructiveModelConfig, UpdateConfig},
    update::device_name,
};

use super::{
    ppo::Learner,
    problems::ProblemSet,
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
        Session::new(problem.clone(), catalogs.clone(), config.native)
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
                Session::new(problem.clone(), catalogs.clone(), config.native)
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

fn validate(config: &Config, problems: &ProblemSet) -> Result<()> {
    ensure!(
        config.schema == "aic-constructive-training-v2"
            && problems.schema == "aic-constructive-problem-set-v1"
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
    problems: &[Problem],
    catalogs: Arc<Catalogs>,
    config: &Config,
    learner: &mut Learner,
    episodes: usize,
    sampling: Sampling,
) -> Result<Collection> {
    let mut sessions = (0..episodes)
        .map(|index| {
            let problem = &problems[index % problems.len()];
            Session::new(problem.clone(), catalogs.clone(), config.native)
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
