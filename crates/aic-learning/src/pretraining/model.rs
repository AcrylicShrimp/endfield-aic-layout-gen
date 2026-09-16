use super::config::Config;
use crate::{
    collection::batch::graph_inputs,
    input::{Packed, Tensors},
    model::{ENCODER_PREFIXES, Model},
    outcomes::storage::Row,
};
use aic_torch::{
    Adam, AdamBackend, AdamState, Generator,
    tch::{Device, Kind, Tensor, no_grad},
};
use anyhow::{Result, ensure};
#[cfg(test)]
use std::collections::BTreeMap;

pub const HEADS: [&str; 12] = [
    "completion",
    "changed",
    "pending_flag",
    "conflict_flag",
    "unchanged",
    "rejected",
    "pending_count",
    "conflict_count",
    "area",
    "transport",
    "turns",
    "route_budget",
];
const BINARY: [usize; 7] = [0, 1, 2, 3, 4, 5, 11];

pub struct Labels {
    pub targets: Vec<[f32; 12]>,
    pub masks: Vec<[f32; 12]>,
}

pub struct Batch {
    pub packed: Packed,
    pub roots: Tensor,
    pub assignment_terminals: Tensor,
    pub assignment_ports: Tensor,
    pub assignment_owners: Tensor,
    pub assignment_counts: Tensor,
    pub labels: Labels,
}

impl Batch {
    pub fn from_rows(rows: &[&Row], count_scale: f64, device: Device) -> Result<Self> {
        ensure!(
            !rows.is_empty(),
            "aic-learning.pretraining-batch: empty batch"
        );
        let inputs = rows.iter().map(|row| &row.pre_action).collect::<Vec<_>>();
        let packed = graph_inputs(&inputs, device)?;
        let mut node_offset = 0i64;
        let mut action_offset = 0i64;
        let mut roots = Vec::with_capacity(rows.len());
        let mut terminals = Vec::new();
        let mut ports = Vec::new();
        let mut owners = Vec::new();
        let mut counts = Vec::with_capacity(rows.len());
        let mut labels = Labels {
            targets: Vec::with_capacity(rows.len()),
            masks: Vec::with_capacity(rows.len()),
        };
        for (owner, row) in rows.iter().enumerate() {
            ensure!(
                row.selected.root < row.pre_action.action_features.len(),
                "aic-learning.pretraining-action: selected root out of range"
            );
            roots.push(action_offset + row.selected.root as i64);
            counts.push(row.selected.assignments.len() as f32);
            for assignment in &row.selected.assignments {
                ensure!(
                    assignment.terminal >= 0
                        && assignment.terminal < row.pre_action.node_static.len() as i64,
                    "aic-learning.pretraining-action: selected terminal out of range"
                );
                terminals.push(node_offset + assignment.terminal);
                ports.extend_from_slice(&assignment.port);
                owners.push(owner as i64);
            }
            let (target, mask) = labels_for(row, count_scale)?;
            labels.targets.push(target);
            labels.masks.push(mask);
            node_offset += row.pre_action.node_static.len() as i64;
            action_offset += row.pre_action.action_features.len() as i64;
        }
        let terminal_count = terminals.len() as i64;
        Ok(Self {
            packed,
            roots: Tensor::from_slice(&roots).to_device(device),
            assignment_terminals: Tensor::from_slice(&terminals).to_device(device),
            assignment_ports: Tensor::from_slice(&ports)
                .view([terminal_count, 11])
                .to_device(device),
            assignment_owners: Tensor::from_slice(&owners).to_device(device),
            assignment_counts: Tensor::from_slice(&counts).to_device(device),
            labels,
        })
    }
}

pub struct Predictor {
    pub shared: Model,
    heads: Vec<(String, Tensor)>,
    optimizer: Adam,
    weights: [f64; 12],
    gradient_clip: f64,
    device: Device,
}

pub struct StepReport {
    pub loss: f64,
    pub gradient_norm: f64,
}

impl Predictor {
    pub fn new(config: &Config, seed: u64, device: Device) -> Result<Self> {
        Self::new_parts(
            &config.model,
            &config.optimizer,
            &config.losses,
            seed,
            device,
        )
    }

    pub fn new_parts(
        model: &crate::schema::ModelConfig,
        optimizer: &super::config::Optimizer,
        losses: &super::config::Losses,
        seed: u64,
        device: Device,
    ) -> Result<Self> {
        let (order, values) = Model::initial_state(model, seed)?;
        let shared = Model::load(model, &order, &values, device)?;
        let input = model.width * 4 + model.action_features + 1;
        let bound = 1.0 / (input as f64).sqrt();
        let heads: Vec<(String, Tensor)> = vec![
            (
                "heads.weight".into(),
                Tensor::empty([12, input], (Kind::Float, device))
                    .uniform_(-bound, bound)
                    .set_requires_grad(true),
            ),
            (
                "heads.bias".into(),
                Tensor::zeros([12], (Kind::Float, device)).set_requires_grad(true),
            ),
        ];
        let parameters = shared
            .parameters
            .iter()
            .filter(|(name, _)| is_encoder(name))
            .map(|(name, value)| (name.clone(), value.shallow_clone()))
            .chain(
                heads
                    .iter()
                    .map(|(name, value)| (name.clone(), value.shallow_clone())),
            )
            .collect();
        let gradient_clip = optimizer.gradient_clip;
        let optimizer = Adam::new(
            parameters,
            optimizer.learning_rate,
            optimizer.beta1,
            optimizer.beta2,
            optimizer.epsilon,
            AdamBackend::Foreach,
        )?;
        Ok(Self {
            shared,
            heads,
            optimizer,
            weights: losses.values(),
            gradient_clip,
            device,
        })
    }

    pub(crate) fn action_embedding(&self, batch: &Batch) -> Tensor {
        let state = self
            .shared
            .encode_state(&batch.packed.graph, batch.packed.graphs);
        let root = self
            .shared
            .selected_root_embedding(&state, &batch.packed.graph, &batch.roots);
        let count = batch.roots.size()[0];
        let ports = if batch.assignment_terminals.size()[0] == 0 {
            Tensor::zeros(
                [count, self.shared.config.width],
                (Kind::Float, self.device),
            )
        } else {
            let pairs = self.shared.encode_terminal_pairs(
                &state.nodes,
                &batch.assignment_terminals,
                &batch.assignment_ports,
            );
            Tensor::zeros(
                [count, self.shared.config.width],
                (Kind::Float, self.device),
            )
            .index_add_(0, &batch.assignment_owners, &pairs)
                / batch.assignment_counts.clamp_min(1.0).unsqueeze(-1)
        };
        Tensor::cat(
            &[root, ports, batch.assignment_counts.log1p().unsqueeze(-1)],
            -1,
        )
    }

    pub fn logits(&self, batch: &Batch) -> Tensor {
        self.action_embedding(batch)
            .linear(&self.heads[0].1, Some(&self.heads[1].1))
    }

    pub fn evaluate_loss(&self, batch: &Batch) -> Result<(Tensor, [i64; 12])> {
        let logits = self.logits(batch);
        masked_loss(&logits, &batch.labels, &self.weights, self.device)
    }

    pub fn train(&mut self, batch: &Batch) -> Result<StepReport> {
        self.optimizer.zero_grad()?;
        let (loss, _) = self.evaluate_loss(batch)?;
        let scalar = f64::try_from(&loss)?;
        loss.backward();
        let gradient_norm = self.optimizer.clip_grad_norm(self.gradient_clip)?;
        ensure!(
            gradient_norm.is_finite() && gradient_norm > 0.0,
            "aic-learning.pretraining-gradient: missing or nonfinite shared gradient"
        );
        self.optimizer.step()?;
        Ok(StepReport {
            loss: scalar,
            gradient_norm,
        })
    }

    pub fn encoder_snapshot(&self) -> Tensors {
        self.shared
            .parameters
            .iter()
            .filter(|(name, _)| is_encoder(name))
            .map(|(name, value)| {
                (
                    format!("model.{name}"),
                    value.detach().to_device(Device::Cpu),
                )
            })
            .collect()
    }

    pub fn parameter_snapshot(&self) -> Tensors {
        self.optimizer
            .named_parameters()
            .iter()
            .map(|(name, value)| {
                (
                    format!("model.{name}"),
                    value.detach().to_device(Device::Cpu),
                )
            })
            .collect()
    }

    pub fn checkpoint_snapshot(&mut self, shuffle: &mut Generator) -> Result<Tensors> {
        let mut values = self.training_snapshot()?;
        values.insert("rng.shuffle".into(), shuffle.state()?);
        Ok(values)
    }

    pub fn training_snapshot(&mut self) -> Result<Tensors> {
        let mut values = self.parameter_snapshot();
        for (name, state) in self.optimizer.snapshot()? {
            let prefix = format!("adam.{name}");
            values.insert(
                format!("{prefix}.step"),
                Tensor::from_slice(&[state.as_ref().map_or(0, |state| state.step)]),
            );
            if let Some(state) = state {
                values.insert(format!("{prefix}.mean"), state.mean.to_device(Device::Cpu));
                values.insert(
                    format!("{prefix}.square"),
                    state.square.to_device(Device::Cpu),
                );
            }
        }
        Ok(values)
    }

    pub fn restore_parameters(&mut self, values: &Tensors) -> Result<()> {
        no_grad(|| {
            for (name, target) in self
                .shared
                .parameters
                .iter_mut()
                .filter(|(name, _)| is_encoder(name))
                .chain(self.heads.iter_mut())
            {
                let source = values.get(&format!("model.{name}")).ok_or_else(|| {
                    anyhow::anyhow!("aic-learning.pretraining-checkpoint: missing {name}")
                })?;
                ensure!(
                    source.size() == target.size(),
                    "aic-learning.pretraining-checkpoint: shape mismatch for {name}"
                );
                target.copy_(&source.to_device(self.device));
            }
            Ok(())
        })
    }

    pub fn restore_checkpoint(&mut self, values: &Tensors) -> Result<()> {
        self.restore_parameters(values)?;
        for index in 0..self.optimizer.named_parameters().len() {
            let name = self.optimizer.named_parameters()[index].0.clone();
            let prefix = format!("adam.{name}");
            let step = values.get(&format!("{prefix}.step")).ok_or_else(|| {
                anyhow::anyhow!("aic-learning.pretraining-checkpoint: missing Adam step for {name}")
            })?;
            let step = i64::try_from(step)?;
            if step > 0 {
                let mean = values.get(&format!("{prefix}.mean")).ok_or_else(|| {
                    anyhow::anyhow!(
                        "aic-learning.pretraining-checkpoint: missing Adam mean for {name}"
                    )
                })?;
                let square = values.get(&format!("{prefix}.square")).ok_or_else(|| {
                    anyhow::anyhow!(
                        "aic-learning.pretraining-checkpoint: missing Adam square for {name}"
                    )
                })?;
                self.optimizer.restore_parameter(
                    index,
                    &AdamState {
                        step,
                        mean: mean.to_device(self.device),
                        square: square.to_device(self.device),
                    },
                )?;
            }
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn encoder_gradient_families(&self) -> BTreeMap<String, bool> {
        ENCODER_PREFIXES
            .iter()
            .map(|prefix| {
                let present = self
                    .shared
                    .parameters
                    .iter()
                    .filter(|(name, _)| name.starts_with(prefix))
                    .any(|(_, value)| {
                        value.grad().defined()
                            && bool::try_from(value.grad().abs().sum(Kind::Float).gt(0.))
                                .unwrap_or(false)
                    });
                (prefix.trim_end_matches('.').to_string(), present)
            })
            .collect()
    }

    pub fn predictions(&self, batch: &Batch) -> Result<Vec<[f32; 12]>> {
        let values: Vec<f32> =
            Vec::<f32>::try_from(self.logits(batch).to_device(Device::Cpu).view([-1]))?;
        Ok(values
            .chunks_exact(12)
            .map(|row| row.try_into().unwrap())
            .collect())
    }
}

pub fn masked_loss(
    logits: &Tensor,
    labels: &Labels,
    weights: &[f64; 12],
    device: Device,
) -> Result<(Tensor, [i64; 12])> {
    let targets = Tensor::from_slice(labels.targets.as_flattened())
        .view([-1, 12])
        .to_device(device);
    let masks = Tensor::from_slice(labels.masks.as_flattened())
        .view([-1, 12])
        .to_device(device);
    let mut total = Tensor::zeros([], (Kind::Float, device));
    let mut denominators = [0i64; 12];
    for head in 0..12 {
        let mask = masks.select(1, head as i64);
        let denominator = i64::try_from(mask.sum(Kind::Int64))?;
        denominators[head] = denominator;
        if denominator == 0 {
            continue;
        }
        let prediction = logits.select(1, head as i64);
        let target = targets.select(1, head as i64);
        let contribution = if BINARY.contains(&head) {
            prediction.clamp_min(0.) - &prediction * target + (-prediction.abs()).exp().log1p()
        } else {
            (prediction - target).square()
        };
        total +=
            contribution.multiply(&mask).sum(Kind::Float) * (weights[head] / denominator as f64);
    }
    ensure!(
        denominators[0] + denominators[1] + denominators[4] + denominators[5] > 0,
        "aic-learning.pretraining-labels: batch has no primary labels"
    );
    ensure!(
        bool::try_from(total.isfinite())?,
        "aic-learning.pretraining-loss: nonfinite loss"
    );
    Ok((total, denominators))
}

pub fn is_encoder(name: &str) -> bool {
    ENCODER_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

pub fn labels_for(row: &Row, count_scale: f64) -> Result<([f32; 12], [f32; 12])> {
    let mut target = [0f32; 12];
    let mut mask = [0f32; 12];
    if row.outcome.censored || !row.outcome.charged {
        return Ok((target, mask));
    }
    let changed = matches!(
        row.outcome.status.as_str(),
        "draft_accepted" | "validated" | "improved"
    );
    let unchanged = row.outcome.status == "unchanged";
    let rejected = row.outcome.status == "rejected";
    ensure!(
        changed || unchanged || rejected,
        "aic-learning.pretraining-labels: unknown native status"
    );
    for head in [1usize, 4, 5] {
        mask[head] = 1.0;
    }
    target[1] = f32::from(changed);
    target[4] = f32::from(unchanged);
    target[5] = f32::from(rejected);
    if changed {
        let pending = row.outcome.current_pending.ok_or_else(|| {
            anyhow::anyhow!("aic-learning.pretraining-labels: changed row lacks pending count")
        })?;
        let conflicts = row.outcome.current_conflicts.ok_or_else(|| {
            anyhow::anyhow!("aic-learning.pretraining-labels: changed row lacks conflict count")
        })?;
        for head in [0usize, 2, 3, 6, 7] {
            mask[head] = 1.0;
        }
        target[0] = f32::from(matches!(
            row.outcome.status.as_str(),
            "validated" | "improved"
        ));
        target[2] = f32::from(pending > 0);
        target[3] = f32::from(conflicts > 0);
        let scale = count_scale.ln_1p() as f32;
        target[6] = (pending as f32).ln_1p() / scale;
        target[7] = (conflicts as f32).ln_1p() / scale;
        if let Some(exhausted) = row.outcome.route_budget_exhausted {
            mask[11] = 1.0;
            target[11] = f32::from(exhausted);
        }
    }
    if let Some(metrics) = &row.outcome.current_metrics {
        ensure!(
            row.pre_action.global_features.len() >= 6,
            "aic-learning.pretraining-labels: missing initial score globals"
        );
        let initial = row.pre_action.global_features[3..6]
            .iter()
            .map(|value| value.exp_m1().round().max(1.0))
            .collect::<Vec<_>>();
        for head in 8..=10 {
            mask[head] = 1.0;
        }
        target[8] = metrics.area as f32 / initial[0];
        target[9] = metrics.transport_cells as f32 / initial[1];
        target[10] = metrics.turns as f32 / initial[2];
    }
    Ok((target, mask))
}

pub fn sigmoid(value: f32) -> f64 {
    1.0 / (1.0 + (-f64::from(value)).exp())
}

pub fn geometric_features(row: &Row) -> Vec<f32> {
    let mut result = row.pre_action.global_features.clone();
    result.extend_from_slice(&row.pre_action.action_features[row.selected.root]);
    result.extend((0..3).map(|family| f32::from(family == row.selected.family)));
    result.push((row.selected.assignments.len() as f32).ln_1p());
    let mut ports = [0f32; 11];
    for assignment in &row.selected.assignments {
        for (target, value) in ports.iter_mut().zip(assignment.port) {
            *target += value;
        }
    }
    if !row.selected.assignments.is_empty() {
        for value in &mut ports {
            *value /= row.selected.assignments.len() as f32;
        }
    }
    result.extend_from_slice(&ports);
    result
}
