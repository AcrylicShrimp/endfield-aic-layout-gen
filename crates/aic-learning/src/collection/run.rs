//! Bounded native collection and minibatch updates with one boundary checkpoint.
use super::{
    config::{Config, read_bound},
    rollout, storage,
};
use crate::{
    input::{Seed, sha256},
    update::Learner,
};
use aic_pilot_native::learning::pool::{Command, Pool};
use aic_torch::{
    deterministic,
    tch::{self, Device},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Instant};

pub fn run(
    config_path: &Path,
    seed: &Path,
    output: &Path,
    device: Device,
    chunks: usize,
    resume: Option<&Path>,
    workers: Option<usize>,
) -> Result<Value> {
    ensure!(
        !output.exists() && chunks > 0,
        "aic-learning.collection-output: new output and a positive chunk count required"
    );
    let mut config = Config::load(config_path)?;
    if let Some(n) = workers {
        ensure!(
            n > 0 && n <= config.episodes.len(),
            "aic-learning.collection-workers: invalid worker count"
        );
        config.workers = n;
    }
    let config_hash = sha256(config_path)?;
    tch::set_num_threads(1);
    tch::set_num_interop_threads(1);
    deterministic()?;
    let began = Instant::now();
    let input = Seed::load(seed, device)?;
    let catalogs = aic_pilot_native::input::load_catalogs(&config.catalog_root, &config.catalogs)
        .map_err(anyhow::Error::msg)?;
    let mut pool =
        Pool::new(catalogs, config.workers, config.episodes.len()).map_err(anyhow::Error::msg)?;
    let (mut learner, previous, saved) = if let Some(path) = resume {
        let (learner, m, s) = storage::restore(path, &input, &config, &config_hash, device)?;
        (learner, m.completed_chunks, Some(s))
    } else {
        (
            Learner::load(
                &input.manifest.model,
                &input.manifest.update,
                &input.manifest.parameter_order,
                &input.initial_state()?,
                device,
                0,
            )?,
            0,
            None,
        )
    };
    ensure!(
        chunks as u64
            <= (config.limits.max_actions / config.chunk_actions as u64)
                .saturating_sub(previous as u64),
        "aic-learning.collection-horizon: requested chunks exceed episode"
    );
    let root = config_path.parent().unwrap_or(Path::new("."));
    let mut admissions = Vec::new();
    for (i, e) in config.episodes.iter().enumerate() {
        let request: Value = read_bound(root, &e.request)?;
        let original: Value = read_bound(root, &e.original)?;
        let command = if let Some(saved) = &saved {
            ensure!(
                saved[i]["native"]["request"] == request
                    && saved[i]["native"]["original"] == original,
                "aic-learning.collection-input-binding: restored stream {i} differs from actual input"
            );
            Command::Restore {
                checkpoint: serde_json::to_string(&saved[i]["native"])?,
                options: config.options.clone(),
            }
        } else {
            Command::Open {
                request: serde_json::from_value(request)?,
                original: serde_json::from_value(original)?,
                limits: config.limits.clone(),
                options: config.options.clone(),
            }
        };
        admissions.push((i, command));
    }
    fs::create_dir_all(output)?;
    let mut collected = rollout::Rollout::default();
    let result = (|| {
        pool.exchange(admissions).map_err(anyhow::Error::msg)?;
        let admission_seconds = began.elapsed().as_secs_f64();
        let mut records = Vec::new();
        for chunk in previous..previous + chunks {
            let directory = output.join(format!("chunk-{chunk:04}"));
            fs::create_dir(&directory)?;
            rollout::collect(&mut pool, &mut learner, &config, &mut collected)?;
            let batch = rollout::minibatch(&collected, &config, device)?;
            let start = Instant::now();
            let update = learner.apply(&batch, &input.replay, true)?;
            let update_seconds = start.elapsed().as_secs_f64();
            // Native preparation states and positive witnesses remain available
            // for success-buffer admission. Porting tensor/gradient exports are gone.
            storage::write(&directory.join("evidence.json"), &collected.evidence)?;
            records.push(json!({"chunk":chunk,"samples":collected.samples.len(),"collection_seconds":collected.seconds,"update_seconds":update_seconds,
                "observation_bytes":collected.observation_bytes,"true_terminals":collected.samples.iter().filter(|s|s.terminal).count(),"update":update}));
        }
        let results = storage::data(
            pool.exchange(
                (0..config.episodes.len())
                    .map(|i| (i, Command::Result))
                    .collect(),
            )
            .map_err(anyhow::Error::msg)?,
        )?;
        storage::write(&output.join("results.json"), &results)?;
        let save = Instant::now();
        storage::save(
            &output.join("checkpoint"),
            &mut pool,
            &mut learner,
            &input,
            &config,
            &config_hash,
            previous + chunks,
        )?;
        let checkpoint_seconds = save.elapsed().as_secs_f64();
        let report = json!({"schema":"aic-native-collection-report-v2","status":"completed","production_cutover":false,"workers":config.workers,"streams":config.episodes.len(),
            "completed_chunks":previous+chunks,"resumed":resume.is_some(),"admission_seconds":admission_seconds,"checkpoint_seconds":checkpoint_seconds,
            "total_seconds":began.elapsed().as_secs_f64(),"records":records,"all_retained_best_valid":true});
        storage::write(&output.join("report.json"), &report)?;
        Ok(report)
    })();
    if let Err(error) = &result {
        storage::failure(output, &mut pool, &mut learner, &collected, &config, error)?;
    }
    result
}
