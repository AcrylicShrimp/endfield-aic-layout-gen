use super::{
    batch, credit,
    policy::{self, Choice},
};
use crate::update::Learner;
use aic_pilot_native::learning::{
    Observation,
    pool::{Command, Pool, Reply},
};
use aic_torch::tch::{Device, no_grad};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{collections::BTreeSet, sync::Arc, time::Instant};

pub struct Sample {
    pub stream: usize,
    pub observation: Observation,
    pub choice: Choice,
    pub reward: f64,
    pub terminal: bool,
    pub outcome: Value,
}
pub struct Settings {
    pub episode_ids: Vec<String>,
    pub episode_actions: u64,
    pub chunk_actions: usize,
    pub minibatch_size: usize,
    pub gamma: f64,
    pub gae_lambda: f64,
    pub advantage_normalization: String,
    pub max_observation_bytes: u64,
}
#[derive(Default)]
pub struct Rollout {
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
    pub family_draws: [u64; 3],
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
fn account<'a>(
    rows: impl IntoIterator<Item = &'a Observation>,
    owners: &mut BTreeSet<usize>,
    bytes: &mut u64,
    limit: u64,
) -> Result<()> {
    for row in rows {
        if owners.insert(Arc::as_ptr(&row.input) as usize) {
            *bytes += row.input.bytes(true);
        }
    }
    ensure!(
        *bytes <= limit,
        "aic-learning.collection-memory: retained numerical input budget exceeded before collection"
    );
    Ok(())
}
pub fn collect(
    pool: &mut Pool,
    learner: &mut Learner,
    config: &Settings,
    retained: &mut Rollout,
    mut guard: impl FnMut() -> Result<()>,
) -> Result<()> {
    *retained = Rollout::default();
    let streams = config.episode_ids.len();
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
            .all(|o| !o.done && o.attempts + config.chunk_actions as u64 <= config.episode_actions),
        "aic-learning.collection-horizon: chunk would cross terminal"
    );
    retained.samples = Vec::with_capacity(
        (streams * config.chunk_actions)
            .min(config.max_observation_bytes as usize / std::mem::size_of::<Sample>()),
    );
    let mut owners = BTreeSet::new();
    account(
        &retained.current,
        &mut owners,
        &mut retained.observation_bytes,
        config.max_observation_bytes,
    )?;
    for step in 0..config.chunk_actions {
        guard()?;
        retained.choices = policy::sample(
            &learner.model,
            &retained.current.iter().collect::<Vec<_>>(),
            &mut learner.sampler,
            learner.device,
        )?;
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
            let terminal = first[*stream] + step as u64 + 1 == config.episode_actions;
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
            retained.family_draws[observation.input.root_meta[choice.root][0] as usize] += 1;
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
        account(
            &retained.current,
            &mut owners,
            &mut retained.observation_bytes,
            config.max_observation_bytes,
        )?;
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
    retained.evidence = retained
        .replies
        .drain(..)
        .map(|(stream, r)| {
            let Reply::Data(data) = r else { unreachable!() };
            json!({"stream":stream,"episode_id":config.episode_ids[stream],"rows":data})
        })
        .collect();
    retained.seconds = started.elapsed().as_secs_f64();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_pilot_native::learning::{Domain, Input, schema::*};
    #[test]
    fn guard_counts_actual_capacity_once_per_shared_owner_before_work() {
        let mut input = Input::default();
        input.node_static.reserve_exact(50);
        input.node_static.push([0.; NODE_STATIC]);
        input.edge_static.reserve_exact(60);
        input.edge_static.push([0.; EDGE_STATIC]);
        input.path_cells.reserve_exact(70);
        input.path_cells.push([0.; PATH_FEATURES]);
        let old_estimate = (input.node_static.capacity() * 56
            + input.edge_static.capacity() * 40
            + input.path_cells.capacity() * 8) as u64;
        let actual = input.bytes(true);
        assert!(actual > old_estimate);
        assert!(actual > input.bytes(false));
        let observation = Observation {
            input: Arc::new(input),
            domain: Arc::new(Domain { groups: vec![] }),
            handle: 1,
            blueprint_sha256: String::new(),
            revision: 0,
            attempts: 0,
            done: false,
        };
        let mut owners = BTreeSet::new();
        let mut bytes = 0;
        let copies = [observation.clone(), observation.clone()];
        account(&copies, &mut owners, &mut bytes, actual).unwrap();
        assert_eq!(bytes, actual);
        let failure = account(
            [&observation],
            &mut BTreeSet::new(),
            &mut 0,
            (actual + old_estimate) / 2,
        )
        .unwrap_err();
        assert!(failure.to_string().contains("collection-memory"));
    }
}
