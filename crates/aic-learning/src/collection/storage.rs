//! Buffered JSON primitives and cold partial-failure evidence.
use super::{policy::Choice, rollout::Rollout};
use crate::update::Learner;
use aic_pilot_native::learning::pool::{Command, Pool, Reply};
use aic_torch::tch::Tensor;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::{fs, io::Write, path::Path};

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
/// Durable partial evidence is explicitly not a resumable completed boundary.
pub fn failure(
    path: &Path,
    pool: &mut Pool,
    learner: &mut Learner,
    rollout: &Rollout,
    streams: usize,
    error: &anyhow::Error,
) -> Result<()> {
    let mut snapshots = Vec::new();
    let mut evidence = Vec::new();
    for id in 0..streams {
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
        &json!({"schema":"aic-native-collection-failure-v2", "contract":aic_pilot_native::learning::current_contract(), "completed":false,
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
