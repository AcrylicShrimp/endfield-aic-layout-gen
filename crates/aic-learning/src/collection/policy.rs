//! Central batched heads with the established logical-stream/depth RNG order.
use super::batch;
use crate::model::{Model, distribution};
use aic_pilot_native::learning::Observation;
use aic_torch::{
    Generator,
    tch::{Device, Tensor, no_grad},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Choice {
    pub root: usize,
    pub ports: Vec<usize>,
    pub old_logp: f64,
    pub old_value: f64,
    pub entropy: f64,
}

pub fn sample(
    model: &Model,
    rows: &[&Observation],
    sampler: &mut Generator,
    device: Device,
) -> Result<Vec<Choice>> {
    sample_with(
        model,
        rows,
        |_, probabilities| sampler.categorical(probabilities),
        device,
    )
}

/// Evaluation can give each case its own RNG without losing head batching.
pub fn sample_with(
    model: &Model,
    rows: &[&Observation],
    mut categorical: impl FnMut(usize, &Tensor) -> Result<Tensor>,
    device: Device,
) -> Result<Vec<Choice>> {
    no_grad(|| {
        let packed = batch::graph(rows, device)?;
        let prediction = model.roots(&packed);
        let root_logp = prediction.root_logp.to_device(Device::Cpu);
        let entropies = Vec::<f32>::try_from(prediction.entropy.to_device(Device::Cpu))?;
        let values = Vec::<f32>::try_from(prediction.values.to_device(Device::Cpu))?;
        let mut choices = Vec::new();
        let mut offset = 0;
        for (i, row) in rows.iter().enumerate() {
            let count = row.input.root_meta.len() as i64;
            let probabilities = root_logp.narrow(0, offset, count).exp();
            let selected = i64::try_from(categorical(i, &probabilities)?)?;
            choices.push(Choice {
                root: selected as usize,
                ports: vec![],
                old_logp: root_logp.double_value(&[offset + selected]),
                old_value: values[i] as f64,
                entropy: entropies[i] as f64,
            });
            offset += count;
        }
        let offsets = batch::offsets(rows);
        let mut depth = 0;
        loop {
            let mut branches = Vec::new();
            for (owner, row) in rows.iter().enumerate() {
                let choice = &choices[owner];
                let meta = row.input.root_meta[choice.root];
                if meta[0] == 0 && depth < row.domain.groups[meta[1] as usize].len() {
                    branches.push(batch::branch(
                        row,
                        owner,
                        choice.root,
                        &choice.ports,
                        depth,
                    )?);
                }
            }
            if branches.is_empty() {
                break;
            }
            let packed = batch::branches(&branches, &offsets, device);
            let logits = model.ports(&prediction.nodes, &prediction.pooled, &packed);
            let (logp, entropy) = distribution(&logits, &packed, branches.len() as i64);
            let logp = logp.to_device(Device::Cpu);
            let entropy = Vec::<f32>::try_from(entropy.to_device(Device::Cpu))?;
            let mut offset = 0;
            for (i, branch) in branches.iter().enumerate() {
                let n = branch.keep.len() as i64;
                let selected =
                    i64::try_from(categorical(branch.owner, &logp.narrow(0, offset, n).exp())?)?;
                let choice = &mut choices[branch.owner];
                choice.ports.push(branch.keep[selected as usize]);
                choice.old_logp += logp.double_value(&[offset + selected]);
                choice.entropy += entropy[i] as f64;
                offset += n;
            }
            depth += 1;
        }
        ensure!(
            choices.iter().all(|c| c.old_logp.is_finite()
                && c.old_value.is_finite()
                && c.entropy.is_finite()),
            "aic-learning.collection-forward: nonfinite behavior"
        );
        Ok(choices)
    })
}
