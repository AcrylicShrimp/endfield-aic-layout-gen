//! Minimal PPO update kernel for direct constructive command trajectories.

use aic_pilot_native::constructive::learning::Observation;
use aic_torch::{
    Adam, AdamBackend, Generator,
    tch::{Device, Kind, Tensor},
};
use anyhow::{Result, ensure};
use serde::Serialize;

use crate::{
    collection::credit,
    input::Tensors,
    schema::{ConstructiveModelConfig, UpdateConfig},
};

use super::{Model, policy};

pub struct Sample<'a> {
    pub observation: &'a Observation,
    pub choice: &'a policy::Choice,
    pub advantage: f32,
    pub target: f32,
}

pub struct Learner {
    pub model: Model,
    actor: Adam,
    critic: Adam,
    pub sampler: Generator,
    pub config: UpdateConfig,
    pub device: Device,
    pub completed_steps: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct UpdateReport {
    pub step: u64,
    pub loss: f64,
    pub policy_loss: f64,
    pub value_loss: f64,
    pub entropy: f64,
    pub approximate_kl: f64,
    pub clip_fraction: f64,
    pub actor_gradient_norm: f64,
    pub critic_gradient_norm: f64,
}

impl Learner {
    pub fn cold(
        model_config: &ConstructiveModelConfig,
        update: &UpdateConfig,
        initialization_seed: u64,
        device: Device,
    ) -> Result<(Vec<String>, Self)> {
        update.validate()?;
        ensure!(
            update.imitation_coefficient == 0.,
            "aic-learning.constructive-ppo: imitation is outside the first pilot"
        );
        let (order, state) = Model::initial_state(model_config, initialization_seed)?;
        let model = Model::load(model_config, &order, &state, device)?;
        let backend = if update.adam_backend == "fused" {
            AdamBackend::Fused
        } else {
            AdamBackend::Foreach
        };
        let optimizer = |critic: bool| {
            Adam::new(
                model
                    .parameters
                    .iter()
                    .filter(|(name, _)| name.starts_with("critic.") == critic)
                    .map(|(name, value)| (name.clone(), value.shallow_clone()))
                    .collect(),
                update.learning_rate,
                update.beta1,
                update.beta2,
                update.epsilon,
                backend,
            )
        };
        Ok((
            order,
            Self {
                actor: optimizer(false)?,
                critic: optimizer(true)?,
                sampler: Generator::new(initialization_seed.checked_add(1_000_000).ok_or_else(
                    || anyhow::anyhow!("aic-learning.constructive-ppo: sampler seed overflow"),
                )?)?,
                model,
                config: update.clone(),
                device,
                completed_steps: 0,
            },
        ))
    }

    pub fn sample(&mut self, rows: &[&Observation]) -> Result<Vec<policy::Choice>> {
        policy::sample(&self.model, rows, &mut self.sampler, self.device)
    }

    pub fn greedy(&self, rows: &[&Observation]) -> Result<Vec<policy::Choice>> {
        policy::greedy(&self.model, rows, self.device)
    }

    pub fn update(&mut self, samples: &[Sample<'_>], normalization: &str) -> Result<UpdateReport> {
        ensure!(
            !samples.is_empty() && self.completed_steps < u64::MAX,
            "aic-learning.constructive-ppo: empty batch or exhausted step counter"
        );
        let decisions = samples
            .iter()
            .map(|sample| policy::TrainingDecision {
                observation: sample.observation,
                choice: sample.choice,
            })
            .collect::<Vec<_>>();
        let (logp, entropy, values) =
            policy::selected_likelihood(&self.model, &decisions, self.device)?;
        let old_logp = Tensor::from_slice(
            &samples
                .iter()
                .map(|sample| sample.choice.old_logp as f32)
                .collect::<Vec<_>>(),
        )
        .to_device(self.device);
        let raw_advantage = Tensor::from_slice(
            &samples
                .iter()
                .map(|sample| sample.advantage)
                .collect::<Vec<_>>(),
        )
        .to_device(self.device);
        let advantage = credit::normalize(&raw_advantage, normalization)?;
        let targets = Tensor::from_slice(
            &samples
                .iter()
                .map(|sample| sample.target)
                .collect::<Vec<_>>(),
        )
        .to_device(self.device);
        self.actor.zero_grad()?;
        self.critic.zero_grad()?;
        let ratio = (&logp - &old_logp).exp();
        let clipped = ratio.clamp(1. - self.config.clip_epsilon, 1. + self.config.clip_epsilon);
        let policy_loss = -(&ratio * &advantage)
            .minimum(&(&clipped * &advantage))
            .mean(Kind::Float);
        let value_loss = (&values - targets).square().mean(Kind::Float) * 0.5;
        let entropy_mean = entropy.mean(Kind::Float);
        let loss = &policy_loss + &value_loss - self.config.entropy_coefficient * &entropy_mean;
        let scalar = f64::try_from(&loss)?;
        ensure!(
            scalar.is_finite(),
            "aic-learning.constructive-ppo: nonfinite objective"
        );
        loss.backward();
        let actor_gradient_norm = self.actor.clip_grad_norm(self.config.gradient_clip)?;
        let critic_gradient_norm = self.critic.clip_grad_norm(self.config.gradient_clip)?;
        self.actor.step()?;
        self.critic.step()?;
        self.completed_steps += 1;
        let approximate_kl = f64::try_from((&old_logp - &logp).mean(Kind::Float))?;
        let clip_fraction = f64::try_from(
            (&ratio - 1.)
                .abs()
                .gt(self.config.clip_epsilon)
                .to_kind(Kind::Float)
                .mean(Kind::Float),
        )?;
        Ok(UpdateReport {
            step: self.completed_steps,
            loss: scalar,
            policy_loss: f64::try_from(policy_loss)?,
            value_loss: f64::try_from(value_loss)?,
            entropy: f64::try_from(entropy_mean)?,
            approximate_kl,
            clip_fraction,
            actor_gradient_norm,
            critic_gradient_norm,
        })
    }

    pub fn snapshot(&mut self) -> Result<Tensors> {
        let mut state: Tensors = self
            .model
            .parameters
            .iter()
            .map(|(name, value)| {
                (
                    format!("constructive.model.{name}"),
                    value.detach().to_device(Device::Cpu).copy(),
                )
            })
            .collect();
        for (group, optimizer) in [("actor", &mut self.actor), ("critic", &mut self.critic)] {
            for (name, adam) in optimizer.snapshot()? {
                let prefix = format!("constructive.adam.{group}.{name}");
                state.insert(
                    format!("{prefix}.step"),
                    Tensor::from_slice(&[adam.as_ref().map_or(0, |state| state.step)]),
                );
                if let Some(adam) = adam {
                    state.insert(format!("{prefix}.mean"), adam.mean.to_device(Device::Cpu));
                    state.insert(
                        format!("{prefix}.square"),
                        adam.square.to_device(Device::Cpu),
                    );
                }
            }
        }
        state.insert("constructive.rng.sampler".into(), self.sampler.state()?);
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_pilot_native::constructive::{Command, learning::Cursor};

    #[test]
    fn sampled_joint_command_survives_one_ppo_update() {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let model = ConstructiveModelConfig {
            schema: ConstructiveModelConfig::SCHEMA.into(),
            width: 16,
            message_rounds: 1,
        };
        let update = UpdateConfig {
            adam_backend: "foreach".into(),
            learning_rate: 0.0003,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            clip_epsilon: 0.2,
            entropy_coefficient: 0.01,
            imitation_coefficient: 0.,
            gradient_clip: 0.5,
        };
        let (_, mut learner) = Learner::cold(&model, &update, 73, Device::Cpu).unwrap();
        let first = super::super::test_support::observation();
        let second = super::super::test_support::observation();
        let choices = learner.sample(&[&first, &second]).unwrap();
        assert!(choices.iter().all(|choice| choice.factors.len() == 3));
        assert!(choices.iter().all(|choice| {
            choice.factors[0].cursor == Cursor::Command
                && matches!(choice.command.command, Command::Place { .. })
                && choice.old_logp.is_finite()
                && choice.old_value.is_finite()
        }));
        let report = learner
            .update(
                &[
                    Sample {
                        observation: &first,
                        choice: &choices[0],
                        advantage: 1.,
                        target: 0.5,
                    },
                    Sample {
                        observation: &second,
                        choice: &choices[1],
                        advantage: -1.,
                        target: -0.5,
                    },
                ],
                "center",
            )
            .unwrap();
        assert_eq!(report.step, 1);
        assert!(report.loss.is_finite() && report.actor_gradient_norm.is_finite());
        assert!(report.approximate_kl.abs() < 1e-5);
        assert_eq!(report.clip_fraction, 0.);
        let snapshot = learner.snapshot().unwrap();
        assert!(snapshot.contains_key("constructive.rng.sampler"));
        assert!(
            snapshot
                .keys()
                .any(|key| key.starts_with("constructive.adam.actor."))
        );
    }
}
