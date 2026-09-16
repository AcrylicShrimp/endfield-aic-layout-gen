//! Complete native training lifecycle: admission, PPO, replay, evaluation and recovery.
use super::{
    config::Config,
    curriculum::{self, Parent, Root},
    evaluation::{Evaluator, Panel},
    replay::Buffer,
    resources::Guard,
    schedule,
    state::{Episode, State},
    storage::{self, Store},
};
use crate::{
    collection::{
        rollout::{self, Rollout},
        storage::write,
    },
    input::{Tensors, sha256},
    model::{ENCODER_PREFIXES, Model},
    update::{Learner, device_name},
};
use aic_pilot_native::{
    generation::source::{Generated, Source},
    input::{hash, load_catalogs},
    learning::pool::{Command, Pool, Reply},
};
use aic_torch::{
    deterministic,
    tch::{self, Device, Kind, Tensor, no_grad},
};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{fs, path::Path, sync::Arc, time::Instant};

pub fn run(
    config_path: &Path,
    output: &Path,
    device: Device,
    updates: Option<u64>,
    resume: bool,
    workers: Option<usize>,
) -> Result<Value> {
    let began = Instant::now();
    let mut config = Config::load(config_path)?;
    let catalogs =
        load_catalogs(&config.catalog_root, &config.catalogs).map_err(anyhow::Error::msg)?;
    let panel = Panel::load(
        &config.evaluation.panel,
        config.resources.max_rss_bytes / 4,
        &catalogs,
        &config.generation.identity_limits,
    )?;
    ensure!(
        config.strata.iter().all(|(stratum, products)| panel
            .manifest
            .cases
            .iter()
            .filter(|c| &c.stratum == stratum && products.contains(&c.product))
            .count()
            * 2
            == panel.manifest.cases.len()),
        "aic-learning.panel-strata: evaluation requires equal nonempty coverage of the two configured strata"
    );
    let source = Source::load(
        &config.catalog_root,
        catalogs.clone(),
        config.generation.clone(),
        panel.reserved.clone(),
    )
    .map_err(anyhow::Error::msg)?;
    // Worker count is a resource choice, not logical stream identity.
    let mut identity = serde_json::to_value(&config)?;
    for field in ["workers", "catalog_root"] {
        identity.as_object_mut().unwrap().remove(field);
    }
    if let Some(encoder) = identity["initial_encoder"].as_object_mut() {
        encoder.insert("path".into(), "<bound-by-sha256>".into());
    }
    let binding = hash(&serde_json::to_vec(
        &json!({"config":identity,"source":source.identity,"catalogs":catalogs.identity,"panel":panel.identity,"device":device_name(device)?}),
    )?);
    if let Some(n) = workers {
        config.workers = n;
        config.validate()?;
    }
    ensure!(
        updates.is_none_or(|n| n > 0),
        "aic-learning.training-updates: positive total update target required"
    );
    ensure!(
        resume == output.exists(),
        "aic-learning.training-output: new output required for fresh training; --resume requires existing output {}",
        output.display()
    );
    if !resume {
        fs::create_dir_all(output)?;
        for name in ["inputs", "replay", "checkpoints", "evaluations", "updates"] {
            fs::create_dir(output.join(name))?;
        }
    }
    let output = output.canonicalize()?;
    let _ownership = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(output.join("run.lock"))?;
    _ownership
        .try_lock()
        .map_err(|e| anyhow::anyhow!("aic-learning.run-busy: {}: {e}", output.display()))?;
    let guard = Guard::new(
        &output,
        &config.resources,
        config
            .duration_seconds
            .checked_add(config.finish_grace_seconds)
            .ok_or_else(|| anyhow::anyhow!("aic-learning.elapsed-budget: overflow"))?,
    )?;
    let check = guard.check();
    let native_check = check.clone();
    let source = Arc::new(source.with_guard(Arc::new(move || native_check.native())));
    tch::set_num_threads(1);
    tch::set_num_interop_threads(1);
    deterministic()?;
    let mut pool =
        Pool::new(catalogs.clone(), config.workers, config.streams).map_err(anyhow::Error::msg)?;
    let mut evaluator = Evaluator::new(catalogs, &panel, &config.evaluation, config.workers)?;
    let (mut store, mut state, mut buffer, mut learner, order, sessions) = if resume {
        let latest: String = storage::read(&output.join("latest.json"), 4096)?;
        ensure!(
            latest.starts_with("checkpoints/")
                && latest.split('/').count() == 2
                && latest.split('/').nth(1).unwrap().parse::<u64>().is_ok(),
            "aic-learning.resume-pointer: invalid latest checkpoint"
        );
        storage::restore(&output, &output.join(latest), &binding, &config, device)?
    } else {
        let (order, mut initial) = Model::initial_state(&config.model, config.initialization_seed)?;
        if let Some(encoder) = &config.initial_encoder {
            apply_initial_encoder(&mut initial, Path::new(&encoder.path), &encoder.sha256)?;
        }
        let learner = Learner::load(
            &config.model,
            &config.optimizer,
            &order,
            &initial,
            device,
            0,
        )?;
        (
            Store::default(),
            State::default(),
            Buffer::default(),
            learner,
            order,
            vec![],
        )
    };
    ensure!(
        updates.is_none_or(|n| n > state.update),
        "aic-learning.training-updates: target must exceed completed update {}",
        state.update
    );
    for (stream, episode) in state.episodes.iter().enumerate() {
        source
            .check_reservation(&episode.input, &format!("restore/{}/{stream}", state.wave))
            .map_err(anyhow::Error::msg)?;
    }
    if let Some(plan) = &state.next {
        for slot in &plan.slots {
            if let Some(input) = &slot.inherited {
                source
                    .check_reservation(
                        input,
                        &format!("restore-next/{}/{}", plan.wave, slot.stream),
                    )
                    .map_err(anyhow::Error::msg)?;
            }
        }
    }
    let previous_samples = state.samples;
    let previous_update = state.update;
    let mut collected = Rollout::default();
    let result = (|| -> Result<Value> {
        if !sessions.is_empty() {
            pool.exchange(
                sessions
                    .into_iter()
                    .enumerate()
                    .map(|(i, s)| {
                        Ok((
                            i,
                            Command::Restore {
                                checkpoint: serde_json::to_string(&s["native"])?,
                                options: config.actions.clone(),
                            },
                        ))
                    })
                    .collect::<Result<_>>()?,
            )
            .map_err(anyhow::Error::msg)?;
        }
        let mut last_saved = if resume { state.update } else { 0 };
        let mut generation_seconds = 0.;
        let mut collection_seconds = 0.;
        let mut optimization_seconds = 0.;
        let mut checkpoint_seconds = 0.;
        let mut evaluation_seconds = 0.;
        loop {
            check.run()?;
            let admission = Instant::now();
            if state.episodes.is_empty() || state.next.is_some() {
                admit(&mut state, &mut pool, &source, &config, &output, &mut store)?;
            }
            let admission_seconds = admission.elapsed().as_secs_f64();
            generation_seconds += admission_seconds;
            let ordinal = state.update + 1;
            let settings = config.collection(state.wave);
            rollout::collect(&mut pool, &mut learner, &settings, &mut collected, || {
                check.run()
            })?;
            let (ppo, frozen) = schedule::update(
                &mut learner,
                &collected,
                &settings,
                &config.ppo,
                &mut buffer,
                &config.success,
                config.initialization_seed,
                ordinal,
                || check.run(),
            )?;
            collection_seconds += collected.seconds;
            optimization_seconds += ppo.update_seconds;
            let products: Vec<_> = state.episodes.iter().map(|e| e.product.clone()).collect();
            let events = buffer.ingest(
                &collected,
                &frozen,
                ordinal,
                state.wave,
                &products,
                &config.strata,
                config.gamma,
                &config.success,
            )?;
            state.update = ordinal;
            state.samples += collected.samples.len() as u64;
            state.actions += config.chunk_actions as u64;
            let wave_complete = state.actions == config.native.max_actions;
            let mut quality = Value::Null;
            if wave_complete {
                let parents = finish(&state, &mut pool, &source)?;
                quality = json!({"episodes":parents.len(),"improved":parents.iter().filter(|p|p.best_area<p.start_area).count(),
                    "mean_area_reduction_percent":parents.iter().map(|p|100.*(1.-p.best_area as f64/p.start_area as f64)).sum::<f64>()/parents.len() as f64});
                state.next = Some(curriculum::choose(
                    &config.curriculum,
                    config.initialization_seed,
                    state.wave + 1,
                    config.streams,
                    &parents,
                    &panel.reserved,
                )?);
                state.episodes.clear();
            }
            let mut evaluation = Value::Null;
            if ordinal % config.evaluation.interval_updates == 0 {
                evaluation = evaluator.run(
                    &panel,
                    &config.evaluation,
                    &config.native,
                    &config.actions,
                    &learner.model,
                    device,
                    ordinal,
                    || check.run(),
                )?;
                evaluation_seconds += evaluation["seconds"].as_f64().unwrap();
                let path = output.join(format!("evaluations/{ordinal:08}.json"));
                write(&path, &evaluation)?;
                store.ledger.track(&path)?;
                let score = evaluation["mean_area_reduction_percent"].as_f64().unwrap();
                if evaluation["complete"] == true
                    && state.best_evaluation.is_none_or(|old| score > old)
                {
                    store.best_model = Some(
                        learner
                            .model
                            .parameters
                            .iter()
                            .map(|(name, value)| {
                                (
                                    format!("model.{name}"),
                                    value.detach().to_device(Device::Cpu).copy(),
                                )
                            })
                            .collect(),
                    );
                    state.best_evaluation = Some(score);
                }
                evaluation.as_object_mut().unwrap().remove("rows");
            }
            let stopping = updates.is_some_and(|n| ordinal >= n)
                || ((began.elapsed().as_secs() >= config.duration_seconds
                    || output.join("STOP").exists())
                    && wave_complete);
            let save = Instant::now();
            if stopping || ordinal % config.retention.checkpoint_interval_updates == 0 {
                store.save(
                    &output,
                    &binding,
                    &state,
                    &buffer,
                    &mut learner,
                    &mut pool,
                    &order,
                    &config,
                )?;
                last_saved = ordinal;
            }
            let save_seconds = save.elapsed().as_secs_f64();
            checkpoint_seconds += save_seconds;
            let record = json!({"schema":"aic-training-update-v1","update":ordinal,"wave":state.wave,"actions_in_wave":state.actions,"samples":collected.samples.len(),"total_samples":state.samples,
                "admission_seconds":admission_seconds,"collection_seconds":collected.seconds,
                "action_families":{"place":collected.family_draws[0],"insert_band":collected.family_draws[1],"delete_band":collected.family_draws[2]},"ppo":ppo,"replay":buffer.summary(),"replay_events":events,"quality":quality,"evaluation":evaluation,
                "checkpoint_seconds":save_seconds,"checkpoint_update":last_saved,"peak_rss_bytes":guard.peak_rss(),"output_bytes":store.ledger.bytes});
            let path = output.join(format!("updates/{ordinal:08}.json"));
            write(&path, &record)?;
            store.ledger.track(&path)?;
            store.ledger.enforce(config.resources.max_output_bytes)?;
            eprintln!(
                "{}",
                serde_json::to_string(
                    &json!({"schema":"aic-training-progress-v1","update":ordinal,"samples":state.samples,"collection_seconds":collected.seconds,"optimization_seconds":ppo.update_seconds,"replay_sequences":buffer.state.entries.len(),"wave_complete":wave_complete})
                )?
            );
            if stopping {
                break;
            }
        }
        let seconds = began.elapsed().as_secs_f64();
        let samples = state.samples - previous_samples;
        let report = json!({"schema":"aic-training-report-v1","status":"completed","resumed":resume,"workers":config.workers,"streams":config.streams,"previous_update":previous_update,"completed_updates":state.update,
            "initial_encoder_sha256":config.initial_encoder.as_ref().map(|encoder|encoder.sha256.as_str()),
            "new_samples":samples,"total_samples":state.samples,"total_seconds":seconds,"samples_per_minute":samples as f64*60./seconds,
            "admission_seconds":generation_seconds,"collection_seconds":collection_seconds,"optimization_seconds":optimization_seconds,"checkpoint_seconds":checkpoint_seconds,"evaluation_seconds":evaluation_seconds,
            "replay":buffer.summary(),"peak_rss_bytes":guard.peak_rss(),"output_bytes":store.ledger.bytes,"latest_checkpoint":format!("checkpoints/{:08}",last_saved),"best_evaluation":state.best_evaluation});
        write(&output.join("report.json"), &report)?;
        if output.join("STOP").exists() {
            fs::remove_file(output.join("STOP"))?;
        }
        Ok(report)
    })();
    if let Err(error) = &result {
        let path = output.join(format!("failure-{}-{}", state.update, std::process::id()));
        fs::create_dir(&path)?;
        // A partial optimizer is evidence, never advertised as a completed boundary.
        crate::collection::storage::failure(
            &path,
            &mut pool,
            &mut learner,
            &collected,
            config.streams,
            error,
        )?;
        write(&path.join("state.json"), &state)?;
        write(&path.join("replay.json"), &buffer.state)?;
        let mut writer = std::io::BufWriter::new(fs::File::create(path.join("replay-inputs.bin"))?);
        postcard::to_io(
            &buffer
                .inputs
                .iter()
                .map(|(r, v)| (*r, v.as_ref()))
                .collect::<Vec<_>>(),
            &mut writer,
        )?;
        std::io::Write::flush(&mut writer)?;
    }
    result
}

pub(crate) fn apply_initial_encoder(
    initial: &mut Tensors,
    path: &Path,
    expected_sha256: &str,
) -> Result<()> {
    ensure!(
        sha256(path)? == expected_sha256,
        "aic-learning.initial-encoder-hash: {}",
        path.display()
    );
    let encoder: Tensors = Tensor::read_safetensors(path)?.into_iter().collect();
    let expected = initial
        .keys()
        .filter(|name| {
            name.strip_prefix("model.").is_some_and(|name| {
                ENCODER_PREFIXES
                    .iter()
                    .any(|prefix| name.starts_with(prefix))
            })
        })
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    ensure!(
        encoder
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            == expected,
        "aic-learning.initial-encoder-allowlist: {}",
        path.display()
    );
    no_grad(|| {
        for (name, source) in encoder {
            let target = initial.get_mut(&name).unwrap();
            ensure!(
                source.kind() == Kind::Float
                    && source.size() == target.size()
                    && bool::try_from(source.isfinite().all())?,
                "aic-learning.initial-encoder-tensor: {name}"
            );
            target.copy_(&source);
        }
        Ok(())
    })
}
fn admit(
    state: &mut State,
    pool: &mut Pool,
    source: &Arc<Source>,
    config: &Config,
    root: &Path,
    store: &mut Store,
) -> Result<()> {
    let plan = state.next.take();
    if let Some(p) = &plan {
        state.wave = p.wave;
    }
    state.actions = 0;
    let mut inputs = std::collections::BTreeMap::new();
    let mut commands = vec![];
    for stream in 0..config.streams {
        let inherited = plan
            .as_ref()
            .and_then(|p| p.slots[stream].inherited.as_ref());
        if let Some(input) = inherited {
            source
                .check_reservation(input, &format!("inherited/{}/{stream}", state.wave))
                .map_err(anyhow::Error::msg)?;
            inputs.insert(stream, input.clone());
            commands.push((
                stream,
                Command::Next {
                    request: input.request.clone(),
                    original: input.original.clone(),
                    limits: config.native.clone(),
                    options: config.actions.clone(),
                },
            ));
        } else {
            let slot = state
                .wave
                .checked_mul(config.streams as u64)
                .and_then(|n| n.checked_add(stream as u64))
                .ok_or_else(|| anyhow::anyhow!("aic-learning.input-slot: exhausted identity"))?;
            commands.push((
                stream,
                Command::Generate {
                    source: source.clone(),
                    slot,
                    directory: root.join(format!("inputs/{:08}/{stream:04}", state.wave)),
                    limits: config.native.clone(),
                    options: config.actions.clone(),
                },
            ));
        }
    }
    for (stream, reply) in pool.exchange(commands).map_err(anyhow::Error::msg)? {
        if let Reply::Data(value) = reply {
            let input: Generated = serde_json::from_value(value)?;
            inputs.insert(stream, input);
            for name in ["input.json", "manifest.json"] {
                store
                    .ledger
                    .track(&root.join(format!("inputs/{:08}/{stream:04}/{name}", state.wave)))?;
            }
        }
    }
    let results = pool
        .exchange((0..config.streams).map(|i| (i, Command::Result)).collect())
        .map_err(anyhow::Error::msg)?;
    state.episodes.clear();
    for (stream, reply) in results {
        let Reply::Data(result) = reply else {
            anyhow::bail!("aic-learning.admission: expected metrics")
        };
        let input = inputs.remove(&stream).ok_or_else(|| {
            anyhow::anyhow!("aic-learning.admission: missing actual input {stream}")
        })?;
        let area = result["initial_metrics"]["score"][0]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("aic-learning.admission: missing initial area"))?;
        let slot = plan.as_ref().map(|p| &p.slots[stream]);
        let product = input.metadata["draw"]["product"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("aic-learning.admission: missing product"))?
            .to_owned();
        let lineage = slot.and_then(|s| s.root.clone()).unwrap_or(Root {
            wave: state.wave,
            area,
            request_sha256: hash(&serde_json::to_vec(&input.request)?),
            reservation: input.admitted().map_err(anyhow::Error::msg)?.root.clone(),
        });
        state.episodes.push(Episode {
            input,
            root: lineage,
            depth: slot.map_or(0, |s| s.depth),
            start_area: area,
            product,
        });
    }
    store.ledger.enforce(config.resources.max_output_bytes)?;
    Ok(())
}
fn finish(state: &State, pool: &mut Pool, source: &Source) -> Result<Vec<Parent>> {
    let mut parents = vec![];
    for (stream, reply) in pool
        .exchange(
            (0..state.episodes.len())
                .map(|i| (i, Command::Result))
                .collect(),
        )
        .map_err(anyhow::Error::msg)?
    {
        let Reply::Data(result) = reply else {
            anyhow::bail!("aic-learning.episode-result: expected native data")
        };
        ensure!(
            result["factory_validated"] == true
                && result["attempts"].as_u64() == Some(state.actions),
            "aic-learning.episode-result: incomplete or invalid stream {stream}"
        );
        let e = &state.episodes[stream];
        let best_area = result["best_metrics"]["score"][0]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("aic-learning.episode-result: missing best area"))?;
        let mut best = e.input.clone();
        best.original = serde_json::from_value(result["best"].clone())?;
        if best_area < e.start_area {
            source
                .publish_best(&mut best, &format!("best/{}/{stream}", state.wave))
                .map_err(anyhow::Error::msg)?;
        }
        parents.push(Parent {
            stream,
            wave: state.wave,
            depth: e.depth,
            root: e.root.clone(),
            start_area: e.start_area,
            best_area,
            complete: true,
            validated: true,
            best,
        });
    }
    Ok(parents)
}
