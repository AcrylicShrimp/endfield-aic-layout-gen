use crate::bridge;
use anyhow::{Result, ensure};
use tch::{Device, Kind, Tensor};

/// A separately owned CPU Torch generator. No global seed swapping and no sharing
/// between worker threads. Sampling, shuffle and replay use distinct instances.
pub struct Generator(bridge::RngHandle);
impl Generator {
    pub fn new(seed: u64) -> Result<Self> {
        Ok(Self(bridge::RngHandle::new(seed)?))
    }
    pub fn state(&mut self) -> Result<Tensor> {
        self.0.state()
    }
    pub fn restore(&mut self, state: &Tensor) -> Result<()> {
        ensure!(
            state.device() == Device::Cpu && state.kind() == Kind::Uint8 && state.dim() == 1,
            "aic-torch.rng-state: expected a one-dimensional CPU byte tensor"
        );
        self.0.restore(state)
    }
    pub fn permutation(&mut self, count: i64) -> Result<Tensor> {
        ensure!(count >= 0, "aic-torch.rng-draw: negative permutation size");
        self.0.draw(0, count, 0, None)
    }
    pub fn integers(&mut self, high: i64, count: i64) -> Result<Tensor> {
        ensure!(
            high > 0 && count >= 0,
            "aic-torch.rng-draw: invalid integer bounds"
        );
        self.0.draw(1, count, high, None)
    }
    pub fn categorical(&mut self, probabilities: &Tensor) -> Result<Tensor> {
        ensure!(
            probabilities.device() == Device::Cpu
                && probabilities.dim() == 1
                && probabilities.numel() > 0,
            "aic-torch.rng-draw: expected nonempty CPU probabilities"
        );
        self.0.draw(2, 1, 0, Some(probabilities))
    }
}

/// Configure the same deterministic execution requirement as the existing runner.
/// Call once before initializing models or parallel numerical work.
pub fn deterministic() -> Result<()> {
    bridge::deterministic()
}
