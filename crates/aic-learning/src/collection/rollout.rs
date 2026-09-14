use super::{
    batch,
    config::Config,
    credit,
    policy::{self, Choice},
};
use crate::{input::Input, update::Learner};
use aic_pilot_native::learning::{
    Observation,
    pool::{Command, Pool, Reply},
};
use aic_torch::tch::{Device, Tensor, no_grad};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::time::Instant;

pub struct Sample {
    pub stream: usize,
    pub observation: Observation,
    pub choice: Choice,
    pub reward: f64,
    pub terminal: bool,
    pub outcome: Value,
}
#[derive(Default)]
pub struct Rollout {
    pub first_sampler: Option<Tensor>,
    pub current: Vec<Observation>,
    pub choices: Vec<Choice>,
    pub replies: Vec<(usize, Reply)>,
    pub samples: Vec<Sample>,
    pub bootstrap: Vec<f32>,
    pub raw: Vec<f32>,
    pub targets: Vec<f32>,
    pub evidence: Vec<Value>,
    pub seconds: f64,
    pub observation_bytes: u64,
}
pub fn observations(replies: Vec<(usize, Reply)>) -> Result<Vec<Observation>> {
    replies
        .into_iter()
        .map(|(_, r)| match r {
            Reply::Observation(o) => Ok(o),
            _ => anyhow::bail!("aic-learning.collection-protocol: expected observation"),
        })
        .collect()
}
fn bytes(o: &Observation) -> u64 {
    let v = &o.input;
    (v.node_static.capacity() * 56
        + v.node_state.capacity() * 40
        + v.edge_index.capacity() * 16
        + v.edge_static.capacity() * 40
        + v.edge_state.capacity() * 16
        + v.path_cells.capacity() * 8
        + v.path_edges.capacity() * 8
        + v.global_features.capacity() * 4
        + v.action_features.capacity() * 48
        + v.action_references.capacity() * 16
        + v.root_meta.capacity() * 24
        + v.kind_log_counts.capacity() * 4
        + v.root_invalid.capacity()
        + v.port_features.capacity() * 48) as u64
}
pub fn collect(
    pool: &mut Pool,
    learner: &mut Learner,
    config: &Config,
    retained: &mut Rollout,
) -> Result<()> {
    *retained = Rollout::default();
    let streams = config.episodes.len();
    let started = Instant::now();
    retained.current = observations(
        pool.exchange(
            (0..streams)
                .map(|i| {
                    (
                        i,
                        Command::Begin {
                            actions: config.chunk_actions,
                        },
                    )
                })
                .collect(),
        )
        .map_err(anyhow::Error::msg)?,
    )?;
    let first: Vec<_> = retained.current.iter().map(|o| o.attempts).collect();
    ensure!(
        retained
            .current
            .iter()
            .all(|o| !o.done
                && o.attempts + config.chunk_actions as u64 <= config.limits.max_actions),
        "aic-learning.collection-horizon: chunk would cross terminal"
    );
    retained.samples = Vec::with_capacity(
        (streams * config.chunk_actions)
            .min(config.max_observation_bytes as usize / std::mem::size_of::<Sample>()),
    );
    for step in 0..config.chunk_actions {
        retained.choices = policy::sample(
            &learner.model,
            &retained.current.iter().collect::<Vec<_>>(),
            &mut learner.sampler,
            learner.device,
        )?;
        if step == 0 {
            retained.first_sampler = Some(learner.sampler.state()?);
        }
        let commands = retained
            .current
            .iter()
            .zip(&retained.choices)
            .enumerate()
            .map(|(i, (o, c))| {
                (
                    i,
                    Command::Step {
                        handle: o.handle,
                        root: c.root,
                        ports: c.ports.clone(),
                    },
                )
            })
            .collect();
        retained.replies = pool.exchange(commands).map_err(anyhow::Error::msg)?;
        // Validate the complete reply wave while all inputs, choices and replies
        // remain owned by the caller. Failure must not drop non-replayable input.
        for (stream, reply) in &retained.replies {
            let Reply::Step {
                result,
                next: after,
            } = reply
            else {
                anyhow::bail!("aic-learning.collection-protocol: expected step");
            };
            let outcome = &result["outcome"];
            let terminal = first[*stream] + step as u64 + 1 == config.limits.max_actions;
            ensure!(
                outcome["charged"] == true
                    && after.attempts == first[*stream] + step as u64 + 1
                    && after.done == terminal
                    && !outcome["diagnostics"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|d| matches!(
                            d["code"].as_str(),
                            Some("declarative.elapsed" | "declarative.budget-exhausted")
                        )),
                "aic-learning.collection-native-budget: stream {stream} ended before its episode horizon"
            );
            ensure!(
                outcome["reward"].as_f64().is_some_and(f64::is_finite),
                "aic-learning.collection-reward: missing or nonfinite reward"
            );
        }
        let mut next = Vec::with_capacity(streams);
        for (((stream, reply), observation), choice) in retained
            .replies
            .drain(..)
            .zip(retained.current.drain(..))
            .zip(retained.choices.drain(..))
        {
            let Reply::Step {
                result,
                next: after,
            } = reply
            else {
                unreachable!()
            };
            retained.observation_bytes += bytes(&observation);
            retained.samples.push(Sample {
                stream,
                observation,
                choice,
                reward: result["outcome"]["reward"].as_f64().unwrap(),
                terminal: after.done,
                outcome: result,
            });
            next.push(after);
        }
        retained.current = next;
        ensure!(
            retained.observation_bytes <= config.max_observation_bytes,
            "aic-learning.collection-memory: retained numerical input budget exceeded"
        );
    }
    retained.bootstrap = no_grad(|| -> Result<Vec<f32>> {
        let packed = batch::graph(&retained.current.iter().collect::<Vec<_>>(), learner.device)?;
        Ok(Vec::<f32>::try_from(
            learner.model.roots(&packed).values.to_device(Device::Cpu),
        )?)
    })?;
    let (mut raw, mut targets) = (
        vec![0.; retained.samples.len()],
        vec![0.; retained.samples.len()],
    );
    for stream in 0..streams {
        let rows: Vec<_> = retained
            .samples
            .iter()
            .filter(|s| s.stream == stream)
            .collect();
        let (a, t) = credit::gae(
            &rows.iter().map(|r| r.reward).collect::<Vec<_>>(),
            &rows.iter().map(|r| r.choice.old_value).collect::<Vec<_>>(),
            &rows.iter().map(|r| r.terminal).collect::<Vec<_>>(),
            retained.bootstrap[stream] as f64,
            config.gamma,
            config.gae_lambda,
        )?;
        for (step, (a, t)) in a.into_iter().zip(t).enumerate() {
            raw[step * streams + stream] = a;
            targets[step * streams + stream] = t;
        }
    }
    retained.raw = raw;
    retained.targets = targets;
    retained.replies = pool
        .exchange((0..streams).map(|i| (i, Command::Drain)).collect())
        .map_err(anyhow::Error::msg)?;
    for (stream, reply) in &retained.replies {
        let Reply::Data(data) = reply else {
            anyhow::bail!("aic-learning.collection-protocol: expected chunk");
        };
        let rows = data
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("aic-learning.collection-evidence: invalid rows"))?;
        ensure!(
            rows.len() == config.chunk_actions,
            "aic-learning.collection-evidence: incomplete stream {stream}"
        );
        for (step, row) in rows.iter().enumerate() {
            let original = &retained.samples[step * streams + stream].observation;
            ensure!(
                row["observation"]["revision"].as_u64() == Some(original.revision)
                    && row["observation"]["blueprint_sha256"] == original.blueprint_sha256,
                "aic-learning.collection-ownership: changed historical input for stream {stream}"
            );
        }
    }
    retained.evidence = retained
        .replies
        .drain(..)
        .map(|(stream, r)| {
            let Reply::Data(data) = r else { unreachable!() };
            json!({"stream":stream,"episode_id":config.episodes[stream].id,"rows":data})
        })
        .collect();
    retained.seconds = started.elapsed().as_secs_f64();
    Ok(())
}

pub fn minibatch(
    input: &mut Input,
    rollout: &Rollout,
    config: &Config,
    device: Device,
) -> Result<()> {
    // This gate performs one explicitly selected minibatch, not an epoch/KL
    // scheduler. Normalize over the complete frozen rollout, then select rows.
    let n = config.minibatch_size;
    let rows = &rollout.samples[..n];
    input.ppo = policy::recorded(
        &rows.iter().map(|r| &r.observation).collect::<Vec<_>>(),
        &rows.iter().map(|r| &r.choice).collect::<Vec<_>>(),
        device,
    )?;
    input.manifest.ppo_graphs = n as i64;
    input.manifest.ppo_factors = input.ppo.factors;
    let raw = Tensor::from_slice(&rollout.raw);
    let advantages = credit::normalize(&raw, &config.advantage_normalization)?
        .narrow(0, 0, n as i64)
        .to_device(device);
    input.targets.insert("ppo.advantages".into(), advantages);
    input.targets.insert(
        "ppo.targets".into(),
        Tensor::from_slice(&rollout.targets[..n]).to_device(device),
    );
    input.targets.insert(
        "ppo.old_logp".into(),
        Tensor::from_slice(
            &rows
                .iter()
                .map(|r| r.choice.old_logp as f32)
                .collect::<Vec<_>>(),
        )
        .to_device(device),
    );
    Ok(())
}
