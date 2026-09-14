//! One complete boundary checkpoint; diagnostic exports are separate artifacts.
use super::{config::Config, policy::Choice, rollout::Rollout};
use crate::{
    input::{Seed, sha256},
    update::Learner,
};
use aic_pilot_native::learning::pool::{Command, Pool, Reply};
use aic_torch::tch::{Device, Tensor};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    schema: String,
    config_sha256: String,
    seed_sha256: String,
    pub completed_chunks: usize,
    pub completed_steps: u64,
    pub device: String,
    tensors_sha256: String,
    sessions_sha256: String,
}
pub fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = std::io::BufWriter::new(fs::File::create(path)?);
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}
pub fn data(replies: Vec<(usize, Reply)>) -> Result<Vec<Value>> {
    replies
        .into_iter()
        .map(|(id, r)| match r {
            Reply::Data(v) => Ok(json!({"stream":id,"native":v})),
            _ => anyhow::bail!("aic-learning.collection-protocol: expected native data"),
        })
        .collect()
}
pub fn save(
    path: &Path,
    pool: &mut Pool,
    learner: &mut Learner,
    input: &Seed,
    config: &Config,
    config_hash: &str,
    chunks: usize,
) -> Result<()> {
    ensure!(
        !path.exists(),
        "aic-learning.collection-checkpoint-exists: {}",
        path.display()
    );
    let stage = path.with_extension(format!("pending-{}", std::process::id()));
    fs::create_dir(&stage)?;
    let result = (|| {
        let sessions = data(
            pool.exchange(
                (0..config.episodes.len())
                    .map(|i| (i, Command::Snapshot))
                    .collect(),
            )
            .map_err(anyhow::Error::msg)?,
        )?;
        write(&stage.join("sessions.json"), &sessions)?;
        Tensor::write_safetensors(
            &learner.snapshot()?.iter().collect::<Vec<_>>(),
            stage.join("learner.safetensors"),
        )?;
        let manifest = Manifest {
            schema: "aic-native-collection-checkpoint-v2".into(),
            config_sha256: config_hash.into(),
            seed_sha256: input.identity.clone(),
            completed_chunks: chunks,
            completed_steps: learner.completed_steps,
            device: crate::update::device_name(learner.device)?,
            tensors_sha256: sha256(&stage.join("learner.safetensors"))?,
            sessions_sha256: sha256(&stage.join("sessions.json"))?,
        };
        write(&stage.join("manifest.json"), &manifest)?;
        for name in ["sessions.json", "learner.safetensors", "manifest.json"] {
            fs::File::open(stage.join(name))?.sync_all()?;
        }
        fs::File::open(&stage)?.sync_all()?;
        fs::rename(&stage, path)?;
        fs::File::open(path.parent().unwrap_or(Path::new(".")))?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&stage);
    }
    result
}
pub fn restore(
    path: &Path,
    input: &Seed,
    config: &Config,
    hash: &str,
    device: Device,
) -> Result<(Learner, Manifest, Vec<Value>)> {
    ensure!(
        fs::metadata(path.join("manifest.json"))?.len() <= 1024 * 1024,
        "aic-learning.collection-checkpoint-size: manifest"
    );
    let m: Manifest = serde_json::from_slice(&fs::read(path.join("manifest.json"))?)?;
    ensure!(
        m.schema == "aic-native-collection-checkpoint-v2"
            && m.config_sha256 == hash
            && m.seed_sha256 == input.identity
            && m.device == crate::update::device_name(device)?
            && m.completed_chunks as u64 <= config.limits.max_actions / config.chunk_actions as u64,
        "aic-learning.collection-checkpoint-binding: {}",
        path.display()
    );
    for (name, expected, limit) in [
        (
            "sessions.json",
            &m.sessions_sha256,
            config.episodes.len() as u64 * 16 * 1024 * 1024,
        ),
        (
            "learner.safetensors",
            &m.tensors_sha256,
            input.manifest.max_tensor_bytes,
        ),
    ] {
        ensure!(
            fs::metadata(path.join(name))?.len() <= limit && sha256(&path.join(name))? == *expected,
            "aic-learning.collection-checkpoint-hash: {}",
            path.join(name).display()
        );
    }
    let states = Tensor::read_safetensors(path.join("learner.safetensors"))?
        .into_iter()
        .collect();
    let learner = Learner::load(
        &input.manifest.model,
        &input.manifest.update,
        &input.manifest.parameter_order,
        &states,
        device,
        m.completed_steps,
    )?;
    let sessions: Vec<Value> = serde_json::from_slice(&fs::read(path.join("sessions.json"))?)?;
    ensure!(
        sessions.len() == config.episodes.len()
            && sessions
                .iter()
                .enumerate()
                .all(|(i, s)| s["stream"].as_u64() == Some(i as u64)),
        "aic-learning.collection-checkpoint-ownership: {}",
        path.display()
    );
    Ok((learner, m, sessions))
}
/// Durable partial evidence is explicitly not a resumable completed boundary.
pub fn failure(
    path: &Path,
    pool: &mut Pool,
    learner: &mut Learner,
    rollout: &Rollout,
    config: &Config,
    error: &anyhow::Error,
) -> Result<()> {
    let mut snapshots = Vec::new();
    let mut evidence = Vec::new();
    for id in 0..config.episodes.len() {
        if let Ok(rows) = pool.exchange(vec![(id, Command::Snapshot)]) {
            snapshots.extend(data(rows)?);
        }
        if let Ok(rows) = pool.exchange(vec![(id, Command::Drain)]) {
            evidence.extend(data(rows)?);
        }
    }
    // Stream available numerical inputs without constructing a second full
    // JSON tree. This is a cold failure artifact, never per-action persistence.
    let mut rows = std::io::BufWriter::new(fs::File::create(path.join("partial.jsonl"))?);
    for r in &rollout.samples {
        partial_row(
            &mut rows,
            "sample",
            r.stream,
            &r.observation,
            Some(&r.choice),
            Some(&r.outcome),
        )?;
    }
    for (stream, observation) in rollout.current.iter().enumerate() {
        partial_row(
            &mut rows,
            "current",
            stream,
            observation,
            rollout.choices.get(stream),
            None,
        )?;
    }
    let failed_replies = pool.take_failed_replies();
    for (stream, reply) in rollout.replies.iter().chain(&failed_replies) {
        match reply {
            Reply::Ready => {}
            Reply::Step { result, next } => {
                partial_row(&mut rows, "reply", *stream, next, None, Some(result))?
            }
            Reply::Data(v) => evidence.push(json!({"stream":stream,"native":v})),
            Reply::Observation(o) => partial_row(&mut rows, "reply", *stream, o, None, None)?,
        }
    }
    rows.flush()?;
    Tensor::write_safetensors(
        &learner.snapshot()?.iter().collect::<Vec<_>>(),
        path.join("partial-learner.safetensors"),
    )?;
    write(
        &path.join("failure.json"),
        &json!({"schema":"aic-native-collection-failure-v1", "completed":false,
        "error":format!("{error:#}"),"snapshots":snapshots,"evidence":evidence,"drained_evidence":rollout.evidence,
        "samples":rollout.samples.len(),"pending_choices":rollout.choices.len(),"completed_updates":learner.completed_steps,
        "numerical_inputs":"partial.jsonl","learner":"partial-learner.safetensors"}),
    )
    .context("aic-learning.collection-failure-save")
}

fn partial_row(
    writer: &mut impl Write,
    kind: &str,
    stream: usize,
    o: &aic_pilot_native::learning::Observation,
    choice: Option<&Choice>,
    outcome: Option<&Value>,
) -> Result<()> {
    #[derive(Serialize)]
    struct Row<'a> {
        kind: &'a str,
        stream: usize,
        input: &'a aic_pilot_native::learning::Input,
        domain: &'a aic_pilot_native::learning::Domain,
        handle: u64,
        blueprint_sha256: &'a str,
        revision: u64,
        attempts: u64,
        done: bool,
        choice: Option<&'a Choice>,
        outcome: Option<&'a Value>,
    }
    serde_json::to_writer(
        &mut *writer,
        &Row {
            kind,
            stream,
            input: &o.input,
            domain: &o.domain,
            handle: o.handle,
            blueprint_sha256: &o.blueprint_sha256,
            revision: o.revision,
            attempts: o.attempts,
            done: o.done,
            choice,
            outcome,
        },
    )?;
    writer.write_all(b"\n")?;
    Ok(())
}
