//! Numerical PPO/SIL minibatch kernel. Epoch scheduling and native collection
//! remain separate; this kernel consumes already-frozen targets and branches.
use crate::{
    input::{Input, Tensors, tensor},
    model::Model,
    schema::{ModelConfig, UpdateConfig},
};
use aic_torch::{
    Adam, AdamState, Generator,
    tch::{Device, Kind, Tensor, no_grad},
};
use anyhow::{Result, ensure};
use serde::Serialize;

pub fn parse_device(value: &str) -> Result<Device> {
    if value == "cpu" {
        return Ok(Device::Cpu);
    }
    if let Some(index) = value.strip_prefix("cuda:") {
        let index = index
            .parse::<usize>()
            .map_err(|_| anyhow::anyhow!("aic-learning.device: invalid CUDA device {value}"))?;
        ensure!(
            aic_torch::tch::Cuda::is_available()
                && index < (aic_torch::tch::Cuda::device_count() as usize),
            "aic-learning.device: unavailable {value}"
        );
        return Ok(Device::Cuda(index));
    }
    anyhow::bail!("aic-learning.device: expected cpu or cuda:N, got {value}")
}

pub struct Learner {
    pub model: Model,
    pub actor: Adam,
    pub critic: Adam,
    pub sampler: Generator,
    pub shuffle: Generator,
    pub replay: Generator,
    pub replay_order: Generator,
    pub config: UpdateConfig,
    pub completed_steps: u64,
    pub device: Device,
}

#[derive(Serialize)]
pub struct UpdateReport {
    pub step: u64,
    pub actor_updated: bool,
    pub loss: f64,
    pub value_loss: f64,
    pub imitation_loss: f64,
    pub actor_gradient_norm: f64,
    pub critic_gradient_norm: f64,
    pub positive_replay_labels: i64,
    pub permutation: Vec<i64>,
}
#[derive(Serialize)]
pub struct StepReport {
    #[serde(flatten)]
    pub update: UpdateReport,
    pub sample_root: i64,
    pub replay_draws: Vec<i64>,
    pub replay_order: Vec<i64>,
}

impl Learner {
    pub fn load(
        model_config: &ModelConfig,
        config: &UpdateConfig,
        order: &[String],
        tensors: &Tensors,
        device: Device,
        completed_steps: u64,
    ) -> Result<Self> {
        config.validate()?;
        let model = Model::load(model_config, order, tensors, device)?;
        let build = |critic: bool| {
            Adam::new(
                model
                    .parameters
                    .iter()
                    .filter(|(n, _)| n.starts_with("critic.") == critic)
                    .map(|(n, t)| (n.clone(), t.shallow_clone()))
                    .collect(),
                config.learning_rate,
                config.beta1,
                config.beta2,
                config.epsilon,
                if config.adam_backend == "fused" {
                    aic_torch::AdamBackend::Fused
                } else {
                    aic_torch::AdamBackend::Foreach
                },
            )
        };
        let mut actor = build(false)?;
        let mut critic = build(true)?;
        restore_adam(&mut actor, "actor", tensors, device)?;
        restore_adam(&mut critic, "critic", tensors, device)?;
        let rng = |name: &str| -> Result<Generator> {
            let mut g = Generator::new(0)?;
            g.restore(tensor(tensors, &format!("rng.{name}"))?)?;
            Ok(g)
        };
        Ok(Self {
            model,
            actor,
            critic,
            sampler: rng("sampler")?,
            shuffle: rng("shuffle")?,
            replay: rng("replay")?,
            replay_order: rng("replay_order")?,
            config: config.clone(),
            completed_steps,
            device,
        })
    }
    /// Actual minibatch update; diagnostic sampler/replay draws stay in step().
    pub fn apply(&mut self, input: &Input, actor_updated: bool) -> Result<UpdateReport> {
        self.apply_inner(input, actor_updated)
            .map(|(report, _)| report)
    }
    pub fn step(&mut self, input: &Input, actor_updated: bool) -> Result<StepReport> {
        let (update, root_logp) = self.apply_inner(input, actor_updated)?;
        // These diagnostic draws verify independent streams without replacing
        // the recorded conditional choices used by PPO/SIL likelihood.
        let first_root_count =
            i64::try_from(input.ppo.graph["action_graph"].eq(0).sum(Kind::Int64))?;
        let sample_root = i64::try_from(
            self.sampler.categorical(
                &root_logp
                    .narrow(0, 0, first_root_count)
                    .exp()
                    .to_device(Device::Cpu),
            )?,
        )?;
        let replay_draws =
            Vec::<i64>::try_from(self.replay.integers(input.manifest.sil_graphs, 8)?)?;
        let replay_order = Vec::<i64>::try_from(self.replay_order.permutation(8)?)?;
        Ok(StepReport {
            update,
            sample_root,
            replay_draws,
            replay_order,
        })
    }
    fn apply_inner(
        &mut self,
        input: &Input,
        actor_updated: bool,
    ) -> Result<(UpdateReport, Tensor)> {
        ensure!(
            self.completed_steps < u64::MAX,
            "aic-learning.update-counter: exhausted counter"
        );
        ensure!(
            self.config == input.manifest.update && self.model.config == input.manifest.model,
            "aic-learning.update-binding: incompatible input configuration"
        );
        let order = self.shuffle.permutation(input.manifest.ppo_graphs)?;
        let permutation = Vec::<i64>::try_from(&order)?;
        let order = order.to_device(self.device);
        self.actor.zero_grad()?;
        self.critic.zero_grad()?;
        self.actor.freeze(!actor_updated);
        // Always restore trainability on a recoverable error.
        let result = self.compute(input, &order, actor_updated);
        self.actor.freeze(false);
        let (loss, value_loss, imitation_loss, positive, actor_norm, critic_norm, root_logp) =
            result?;
        self.completed_steps += 1;
        Ok((
            UpdateReport {
                step: self.completed_steps,
                actor_updated,
                loss,
                value_loss,
                imitation_loss,
                actor_gradient_norm: actor_norm,
                critic_gradient_norm: critic_norm,
                positive_replay_labels: positive,
                permutation,
            },
            root_logp,
        ))
    }
    fn compute(
        &mut self,
        input: &Input,
        order: &Tensor,
        actor_updated: bool,
    ) -> Result<(f64, f64, f64, i64, f64, f64, Tensor)> {
        let predicted = self.model.predict(&input.ppo)?;
        let target = tensor(&input.targets, "ppo.targets")?.index_select(0, order);
        let value_loss = (predicted.values.index_select(0, order) - target)
            .square()
            .mean(Kind::Float)
            * 0.5;
        let mut loss = value_loss.shallow_clone();
        let mut teacher = Tensor::zeros([], (Kind::Float, self.device));
        let mut positive = 0;
        if actor_updated {
            let old = tensor(&input.targets, "ppo.old_logp")?.index_select(0, order);
            let advantages = tensor(&input.targets, "ppo.advantages")?.index_select(0, order);
            let ratio = (predicted.logp.index_select(0, order) - old).exp();
            let clipped = ratio.clamp(1. - self.config.clip_epsilon, 1. + self.config.clip_epsilon);
            loss = loss
                - (&ratio * &advantages)
                    .minimum(&(&clipped * &advantages))
                    .mean(Kind::Float)
                - self.config.entropy_coefficient
                    * predicted.entropy.index_select(0, order).mean(Kind::Float);
            if self.config.imitation_coefficient > 0. {
                let sil = self.model.predict(&input.sil)?;
                let returns = tensor(&input.targets, "sil.returns")?;
                let weights = tensor(&input.targets, "sil.weights")?;
                let advantage = (returns - sil.values.detach()).clamp_min(0.);
                positive = i64::try_from(advantage.gt(0).sum(Kind::Int64))?;
                teacher = (-sil.logp * advantage * weights).sum(Kind::Float);
                loss += self.config.imitation_coefficient * &teacher;
            }
        }
        let loss_scalar = f64::try_from(&loss)?;
        ensure!(
            loss_scalar.is_finite(),
            "aic-learning.loss: nonfinite objective"
        );
        loss.backward();
        let critic_norm = self.critic.clip_grad_norm(self.config.gradient_clip)?;
        let actor_norm = if actor_updated {
            self.actor.clip_grad_norm(self.config.gradient_clip)?
        } else {
            0.
        };
        if actor_updated {
            self.actor.step()?;
        }
        self.critic.step()?;
        Ok((
            loss_scalar,
            f64::try_from(&value_loss)?,
            f64::try_from(&teacher)?,
            positive,
            actor_norm,
            critic_norm,
            predicted.root_logp.detach(),
        ))
    }
    pub fn snapshot(&mut self) -> Result<Tensors> {
        no_grad(|| {
            let mut values: Tensors = self
                .model
                .parameters
                .iter()
                .map(|(n, t)| {
                    (
                        format!("model.{n}"),
                        t.detach().to_device(Device::Cpu).copy(),
                    )
                })
                .collect();
            for (label, optimizer) in [("actor", &mut self.actor), ("critic", &mut self.critic)] {
                for (name, state) in optimizer.snapshot()? {
                    let prefix = format!("adam.{label}.{name}");
                    let step = state.as_ref().map_or(0, |s| s.step);
                    values.insert(format!("{prefix}.step"), Tensor::from_slice(&[step]));
                    if let Some(state) = state {
                        values.insert(format!("{prefix}.mean"), state.mean.to_device(Device::Cpu));
                        values.insert(
                            format!("{prefix}.square"),
                            state.square.to_device(Device::Cpu),
                        );
                    }
                }
            }
            for (name, generator) in [
                ("sampler", &mut self.sampler),
                ("shuffle", &mut self.shuffle),
                ("replay", &mut self.replay),
                ("replay_order", &mut self.replay_order),
            ] {
                values.insert(format!("rng.{name}"), generator.state()?);
            }
            Ok(values)
        })
    }
}

fn restore_adam(
    optimizer: &mut Adam,
    label: &str,
    tensors: &Tensors,
    device: Device,
) -> Result<()> {
    let names = optimizer
        .named_parameters()
        .iter()
        .map(|p| p.0.clone())
        .collect::<Vec<_>>();
    for (i, name) in names.iter().enumerate() {
        let prefix = format!("adam.{label}.{name}");
        let counter = tensor(tensors, &format!("{prefix}.step"))?;
        ensure!(
            counter.kind() == Kind::Int64 && counter.size() == [1],
            "aic-learning.adam-state: invalid step for {name}"
        );
        let step = i64::try_from(counter)?;
        ensure!(
            step >= 0,
            "aic-learning.adam-state: negative step for {name}"
        );
        if step > 0 {
            optimizer.restore_parameter(
                i,
                &AdamState {
                    step,
                    mean: tensor(tensors, &format!("{prefix}.mean"))?.to_device(device),
                    square: tensor(tensors, &format!("{prefix}.square"))?.to_device(device),
                },
            )?;
        } else {
            ensure!(
                !tensors.contains_key(&format!("{prefix}.mean"))
                    && !tensors.contains_key(&format!("{prefix}.square")),
                "aic-learning.adam-state: accumulators without a positive step for {name}"
            );
        }
    }
    Ok(())
}
