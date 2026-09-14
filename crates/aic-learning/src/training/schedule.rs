//! Full rollout coverage, epoch-wide KL stopping and actor-safe critic tails.
use super::{
    frozen::{self, Frozen},
    replay::{self, Buffer},
};
use crate::{
    collection::{
        credit,
        rollout::{Rollout, Settings as CollectionConfig},
    },
    input::{Packed, PpoBatch},
    update::Learner,
};
use aic_torch::{
    Generator,
    tch::{Device, Kind, Tensor, no_grad},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::mpsc::sync_channel, time::Instant};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub epochs: usize,
    pub target_kl: f64,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.epochs > 0
                && self.epochs <= 128
                && self.target_kl.is_finite()
                && self.target_kl > 0.,
            "aic-learning.ppo-schedule: positive bounded epochs and target KL required"
        );
        Ok(())
    }
}
#[derive(Serialize)]
pub struct Epoch {
    pub epoch: usize,
    pub samples: usize,
    pub minibatches: usize,
    pub actor_updated: bool,
    pub approximate_kl: f64,
    pub outside_clip_fraction: f64,
    pub value_mae: f64,
    pub value_loss: f64,
}
#[derive(Serialize)]
pub struct Report {
    pub samples: usize,
    pub epochs: Vec<Epoch>,
    pub actor_epochs: usize,
    pub critic_epochs: usize,
    pub minibatch_updates: usize,
    pub replay_labels: usize,
    pub positive_replay_labels: i64,
    pub update_seconds: f64,
    pub prepare_worker_seconds: f64,
}
struct Task {
    indices: Vec<usize>,
    draws: Vec<replay::Draw>,
}
struct Prepared {
    ppo: PpoBatch,
    replay: Option<replay::Prepared>,
    seconds: f64,
}

fn prepare(
    task: Task,
    frozen: &[Frozen],
    advantages: &[f32],
    rollout: &Rollout,
) -> Result<Prepared> {
    let began = Instant::now();
    let rows: Vec<_> = task.indices.iter().map(|&i| &frozen[i]).collect();
    let ppo = PpoBatch {
        packed: frozen::pack(&rows)?,
        old_logp: Tensor::from_slice(
            &task
                .indices
                .iter()
                .map(|&i| rollout.samples[i].choice.old_logp as f32)
                .collect::<Vec<_>>(),
        ),
        advantages: Tensor::from_slice(
            &task
                .indices
                .iter()
                .map(|&i| advantages[i])
                .collect::<Vec<_>>(),
        ),
        targets: Tensor::from_slice(
            &task
                .indices
                .iter()
                .map(|&i| rollout.targets[i])
                .collect::<Vec<_>>(),
        ),
    };
    Ok(Prepared {
        ppo,
        replay: replay::prepare(&task.draws)?,
        seconds: began.elapsed().as_secs_f64(),
    })
}

pub(crate) fn update(
    learner: &mut Learner,
    rollout: &Rollout,
    collection: &CollectionConfig,
    config: &Config,
    buffer: &mut Buffer,
    replay_config: &replay::Config,
    seed: u64,
    ordinal: u64,
    mut guard: impl FnMut() -> Result<()>,
) -> Result<(Report, Vec<Frozen>)> {
    config.validate()?;
    let began = Instant::now();
    let frozen = rollout
        .samples
        .iter()
        .map(|s| Frozen::record(&s.observation, &s.choice))
        .collect::<Result<Vec<_>>>()?;
    let n = frozen.len();
    ensure!(
        n > 0
            && rollout.raw.len() == n
            && rollout.targets.len() == n
            && collection.minibatch_size > 0,
        "aic-learning.ppo-coverage: incomplete frozen rollout"
    );
    let advantages = Vec::<f32>::try_from(credit::normalize(
        &Tensor::from_slice(&rollout.raw),
        &collection.advantage_normalization,
    )?)?;
    let statistics = frozen
        .chunks(collection.minibatch_size)
        .map(|rows| frozen::pack(&rows.iter().collect::<Vec<_>>()))
        .collect::<Result<Vec<_>>>()?;
    let replay_seed = seed
        .checked_mul(1000)
        .and_then(|v| v.checked_add(ordinal))
        .ok_or_else(|| anyhow::anyhow!("aic-learning.replay-seed: exhausted seed namespace"))?;
    learner.replay = Generator::new(
        replay_seed
            .checked_add(5_000_000)
            .ok_or_else(|| anyhow::anyhow!("aic-learning.replay-seed: overflow"))?,
    )?;
    learner.replay_order = Generator::new(
        replay_seed
            .checked_add(6_000_000)
            .ok_or_else(|| anyhow::anyhow!("aic-learning.replay-seed: overflow"))?,
    )?;
    let sampling = replay::Sampling::new(buffer, ordinal)?;
    let mut report = Report {
        samples: n,
        epochs: vec![],
        actor_epochs: 0,
        critic_epochs: 0,
        minibatch_updates: 0,
        replay_labels: 0,
        positive_replay_labels: 0,
        update_seconds: 0.,
        prepare_worker_seconds: 0.,
    };
    let mut stopped = false;
    let mut utility = vec![];
    let mut groups = vec![];
    for epoch in 0..config.epochs {
        let order = Vec::<i64>::try_from(learner.shuffle.permutation(n as i64)?)?;
        let tasks = order
            .chunks(collection.minibatch_size)
            .map(|indices| {
                let draws = if !stopped && learner.config.imitation_coefficient > 0. {
                    buffer.draw(
                        &sampling,
                        replay_config,
                        &mut learner.replay,
                        &mut learner.replay_order,
                    )?
                } else {
                    vec![]
                };
                Ok(Task {
                    indices: indices.iter().map(|&i| i as usize).collect(),
                    draws,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let minibatches = tasks.len();
        std::thread::scope(|scope| -> Result<()> {
            // Rendezvous limits preparation to one batch ahead of device work.
            let (tx, rx) = sync_channel(0);
            let frozen = &frozen;
            let advantages = &advantages;
            let worker = scope.spawn(move || {
                for task in tasks {
                    let prepared = prepare(task, frozen, advantages, rollout);
                    let failed = prepared.is_err();
                    if tx.send(prepared).is_err() || failed {
                        break;
                    }
                }
            });
            let result = (|| {
                for _ in 0..minibatches {
                    guard()?;
                    let prepared = rx.recv().map_err(|_| {
                        anyhow::anyhow!("aic-learning.ppo-prepare: worker disconnected")
                    })??;
                    report.prepare_worker_seconds += prepared.seconds;
                    let ppo = prepared.ppo.to_device(learner.device);
                    let replay = prepared
                        .replay
                        .as_ref()
                        .map(|r| r.batch.to_device(learner.device));
                    let (step, contribution) =
                        learner.apply_minibatch(&ppo, replay.as_ref(), !stopped)?;
                    report.minibatch_updates += 1;
                    report.positive_replay_labels += step.positive_replay_labels;
                    if let (Some(prepared), Some(contribution)) = (prepared.replay, contribution) {
                        report.replay_labels += prepared.groups.len();
                        groups.extend(prepared.groups);
                        utility.push(contribution);
                    }
                }
                Ok(())
            })();
            // On a numerical error unblock the supplier before joining it.
            drop(rx);
            worker
                .join()
                .map_err(|_| anyhow::anyhow!("aic-learning.ppo-prepare: worker panicked"))?;
            result
        })?;
        let metrics = statistics_epoch(learner, &statistics, rollout)?;
        report.actor_epochs += usize::from(!stopped);
        report.critic_epochs += 1;
        report.epochs.push(Epoch {
            epoch: epoch + 1,
            samples: n,
            minibatches,
            actor_updated: !stopped,
            approximate_kl: metrics[0],
            outside_clip_fraction: metrics[1],
            value_mae: metrics[2],
            value_loss: metrics[3],
        });
        if !stopped && metrics[0] > config.target_kl {
            stopped = true;
        }
    }
    let mut observed: BTreeMap<String, (u64, f64)> = BTreeMap::new();
    if !utility.is_empty() {
        let measurements = Vec::<f32>::try_from(Tensor::cat(&utility, 0).to_device(Device::Cpu))?;
        ensure!(
            measurements.len() == groups.len(),
            "aic-learning.replay-utility: label coverage"
        );
        for ((id, weight, first), value) in groups.into_iter().zip(measurements) {
            let row = observed.entry(id).or_default();
            row.0 += u64::from(first);
            row.1 += weight * value as f64;
        }
    }
    buffer.observe(observed, ordinal, replay_config)?;
    report.update_seconds = began.elapsed().as_secs_f64();
    Ok((report, frozen))
}

fn statistics_epoch(learner: &Learner, batches: &[Packed], rollout: &Rollout) -> Result<[f64; 4]> {
    no_grad(|| {
        let mut logs = vec![];
        let mut values = vec![];
        for batch in batches {
            let prediction = learner.model.predict(&batch.to_device(learner.device))?;
            logs.push(prediction.logp);
            values.push(prediction.values);
        }
        let logs = Tensor::cat(&logs, 0).to_kind(Kind::Double);
        let old = Tensor::from_slice(
            &rollout
                .samples
                .iter()
                .map(|s| s.choice.old_logp)
                .collect::<Vec<_>>(),
        )
        .to_device(learner.device);
        let difference = logs - old;
        let ratio = difference.exp();
        let residual = Tensor::cat(&values, 0)
            - Tensor::from_slice(&rollout.targets).to_device(learner.device);
        let result = Tensor::stack(
            &[
                (&ratio - 1. - difference).mean(Kind::Double),
                (&ratio - 1.)
                    .abs()
                    .gt(learner.config.clip_epsilon)
                    .to_kind(Kind::Double)
                    .mean(Kind::Double),
                residual.abs().mean(Kind::Float).to_kind(Kind::Double),
                (residual.square().mean(Kind::Float) * 0.5).to_kind(Kind::Double),
            ],
            0,
        )
        .to_device(Device::Cpu);
        let values = Vec::<f64>::try_from(result)?;
        ensure!(
            values.iter().all(|v| v.is_finite()),
            "aic-learning.ppo-statistics: nonfinite epoch metrics"
        );
        Ok(values.try_into().unwrap())
    })
}
