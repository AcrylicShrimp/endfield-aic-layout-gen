use crate::bridge;
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use tch::Tensor;

pub struct AdamState {
    pub step: i64,
    pub mean: Tensor,
    pub square: Tensor,
}

#[derive(Clone, Copy, Debug)]
pub enum AdamBackend {
    Foreach,
    Fused,
}

/// Explicit, ordered parameter ownership. Actor and critic do not accidentally
/// acquire every parameter from a shared VarStore. State snapshots are detached.
pub struct Adam {
    native: bridge::AdamHandle,
    parameters: Vec<(String, Tensor)>,
}
impl Adam {
    pub fn new(
        parameters: Vec<(String, Tensor)>,
        lr: f64,
        beta1: f64,
        beta2: f64,
        eps: f64,
        backend: AdamBackend,
    ) -> Result<Self> {
        ensure!(
            !parameters.is_empty()
                && parameters
                    .iter()
                    .map(|p| &p.0)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == parameters.len(),
            "aic-torch.adam-parameters: nonempty unique parameter names required"
        );
        ensure!(
            lr.is_finite()
                && lr >= 0.
                && (0. ..1.).contains(&beta1)
                && (0. ..1.).contains(&beta2)
                && eps.is_finite()
                && eps > 0.,
            "aic-torch.adam-config: invalid learning rate, betas or epsilon"
        );
        let tensors = parameters
            .iter()
            .map(|p| p.1.shallow_clone())
            .collect::<Vec<_>>();
        let mode = match backend {
            AdamBackend::Foreach => 0,
            AdamBackend::Fused => 1,
        };
        let native = bridge::AdamHandle::new(&tensors, lr, beta1, beta2, eps, mode)?;
        Ok(Self { native, parameters })
    }
    pub fn zero_grad(&mut self) -> Result<()> {
        self.native.zero_grad()
    }
    pub fn step(&mut self) -> Result<()> {
        self.native.step()
    }
    pub fn named_parameters(&self) -> &[(String, Tensor)] {
        &self.parameters
    }
    pub fn freeze(&self, frozen: bool) {
        for (_, p) in &self.parameters {
            let _ = p.set_requires_grad(!frozen);
        }
    }
    pub fn clip_grad_norm(&self, limit: f64) -> Result<f64> {
        ensure!(
            limit.is_finite() && limit > 0.,
            "aic-torch.gradient-limit: expected positive finite bound"
        );
        self.native.clip_grad_norm(limit)
    }

    pub fn snapshot(&mut self) -> Result<Vec<(String, Option<AdamState>)>> {
        self.parameters
            .iter()
            .enumerate()
            .map(|(i, (name, _))| {
                Ok((
                    name.clone(),
                    self.native.state(i)?.map(|(step, mean, square)| AdamState {
                        step,
                        mean,
                        square,
                    }),
                ))
            })
            .collect()
    }
    pub fn restore_parameter(&mut self, index: usize, state: &AdamState) -> Result<()> {
        ensure!(
            index < self.parameters.len(),
            "aic-torch.adam-state: parameter index out of range"
        );
        self.native
            .restore(index, state.step, &state.mean, &state.square)
    }
}
