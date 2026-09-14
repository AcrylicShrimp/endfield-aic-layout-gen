//! Explicit integration gate; no training launcher or silent curriculum fallback.
use super::{
    config::{Config, read_bound},
    rollout, storage,
};
use crate::{
    input::{Input, sha256},
    update::Learner,
};
use aic_pilot_native::learning::pool::{Command, Pool};
use aic_torch::{
    deterministic,
    tch::{self, Device, Tensor},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Instant};

pub fn run(
    config_path: &Path,
    kernel: &Path,
    output: &Path,
    device: Device,
    chunks: usize,
    resume: Option<&Path>,
    workers: Option<usize>,
) -> Result<Value> {
    ensure!(
        !output.exists() && (1..=2).contains(&chunks),
        "aic-learning.collection-probe: new output and 1..2 chunks required"
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
    let mut input = Input::load(kernel, device)?;
    let (catalogs, _) = aic_pilot_native::input::load_catalogs(
        &config.catalog_root,
        &serde_json::to_string(&config.catalogs)?,
    )
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
                &input.tensors,
                device,
                0,
            )?,
            0,
            None,
        )
    };
    ensure!(
        (previous + chunks) as u64 * config.chunk_actions as u64 <= config.limits.max_actions,
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
        let _ = rollout::observations(pool.exchange(admissions).map_err(anyhow::Error::msg)?)?;
        let admission_seconds = began.elapsed().as_secs_f64();
        let mut records = Vec::new();
        for chunk in previous..previous + chunks {
            let directory = output.join(format!("chunk-{chunk:04}"));
            fs::create_dir(&directory)?;
            // Explicit oracle capture outside collection timing, never a normal
            // per-action snapshot or policy update during behavior collection.
            let before = learner.snapshot()?;
            Tensor::write_safetensors(
                &before.iter().collect::<Vec<_>>(),
                directory.join("before.safetensors"),
            )?;
            rollout::collect(&mut pool, &mut learner, &config, &mut collected)?;
            Tensor::write_safetensors(
                &[("sampler", collected.first_sampler.as_ref().unwrap())],
                directory.join("first-sampler.safetensors"),
            )?;
            rollout::minibatch(&mut input, &collected, &config, device)?;
            let start = Instant::now();
            let update = learner.apply(&input, true)?;
            let update_seconds = start.elapsed().as_secs_f64();
            storage::write(&directory.join("evidence.json"), &collected.evidence)?;
            let transitions:Vec<_>=collected.samples.iter().map(|r|json!({"stream":r.stream,"episode_id":config.episodes[r.stream].id,"attempt":r.observation.attempts,
                "choice":r.choice,"reward":r.reward,"terminal":r.terminal,"outcome":r.outcome})).collect();
            storage::write(&directory.join("transitions.json"), &transitions)?;
            storage::write(
                &directory.join("credit.json"),
                &json!({"raw":collected.raw,"targets":collected.targets,"bootstrap":collected.bootstrap,
                "gamma":config.gamma,"gae_lambda":config.gae_lambda,"advantage_normalization":config.advantage_normalization}),
            )?;
            let first:Vec<_>=collected.samples.iter().take(config.episodes.len()).map(|r|json!({"input":r.observation.input.as_ref(),"domain":r.observation.domain.as_ref(),"choice":r.choice})).collect();
            storage::write(&directory.join("first-observations.json"), &first)?;
            // Pre-update parameters/Adam/RNG plus the actual frozen minibatch
            // make a directly reusable numerical oracle fixture.
            let mut numerical = before;
            for (prefix, packed) in [("ppo", &input.ppo), ("sil", &input.sil)] {
                for (name, t) in &packed.graph {
                    numerical.insert(format!("{prefix}.batch.{name}"), t.to_device(Device::Cpu));
                }
                if let Some(b) = &packed.branches {
                    for (name, t) in b {
                        numerical.insert(
                            format!("{prefix}.branches.{name}"),
                            t.to_device(Device::Cpu),
                        );
                    }
                }
                numerical.insert(
                    format!("{prefix}.roots"),
                    packed.roots.to_device(Device::Cpu),
                );
                numerical.insert(
                    format!("{prefix}.selected"),
                    packed.selected.to_device(Device::Cpu),
                );
            }
            for (name, t) in &input.targets {
                numerical.insert(name.clone(), t.to_device(Device::Cpu));
            }
            Tensor::write_safetensors(
                &numerical.iter().collect::<Vec<_>>(),
                directory.join("input.safetensors"),
            )?;
            let mut manifest = input.manifest.clone();
            manifest.tensors = "input.safetensors".into();
            manifest.tensors_sha256 = sha256(&directory.join("input.safetensors"))?;
            manifest.source = json!({"schema":"aic-actual-rollout-numerical-source-v1","config_sha256":config_hash,"chunk":chunk,"episode_ids":config.episodes.iter().map(|e|&e.id).collect::<Vec<_>>(),"ppo_target_contract":"Actual rewards and true-terminal GAE; first configured minibatch of frozen rollout"});
            storage::write(&directory.join("manifest.json"), &manifest)?;
            Tensor::write_safetensors(
                &learner.snapshot()?.iter().collect::<Vec<_>>(),
                directory.join("after.safetensors"),
            )?;
            let grads: Vec<_> = learner
                .model
                .parameters
                .iter()
                .filter_map(|(n, p)| {
                    let g = p.grad();
                    g.defined().then(|| (n.clone(), g.to_device(Device::Cpu)))
                })
                .collect();
            Tensor::write_safetensors(&grads, directory.join("gradients.safetensors"))?;
            records.push(json!({"chunk":chunk,"samples":collected.samples.len(),"collection_seconds":collected.seconds,"update_seconds":update_seconds,
                "observation_bytes":collected.observation_bytes,"true_terminals":collected.samples.iter().filter(|s|s.terminal).count(),"update":update}));
        }
        let results = storage::data(
            pool.exchange(
                (0..config.episodes.len())
                    .map(|i| (i, Command::ValidateBest))
                    .collect(),
            )
            .map_err(anyhow::Error::msg)?,
        )?;
        ensure!(
            results
                .iter()
                .all(|r| r["native"]["factory_validated"] == true),
            "aic-learning.collection-final: invalid retained native best"
        );
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
        let report = json!({"schema":"aic-native-collection-report-v1","status":"completed","production_cutover":false,"workers":config.workers,"streams":config.episodes.len(),
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
