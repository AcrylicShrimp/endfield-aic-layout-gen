//! True-terminal GAE uses the existing f64 recurrence and f32 output contract.
use aic_torch::tch::{Kind, Tensor};
use anyhow::{Result, ensure};

pub fn gae(
    rewards: &[f64],
    values: &[f64],
    terminals: &[bool],
    bootstrap: f64,
    gamma: f64,
    lambda: f64,
) -> Result<(Vec<f32>, Vec<f32>)> {
    ensure!(
        !rewards.is_empty()
            && rewards.len() == values.len()
            && rewards.len() == terminals.len()
            && rewards
                .iter()
                .chain(values)
                .chain([&bootstrap])
                .all(|v| v.is_finite())
            && (0. ..=1.).contains(&gamma)
            && (0. ..=1.).contains(&lambda),
        "aic-learning.gae-input: invalid credit inputs"
    );
    ensure!(
        terminals[..terminals.len() - 1].iter().all(|t| !*t),
        "aic-learning.gae-terminal: reset inside chunk"
    );
    let mut raw = vec![0.; values.len()];
    let mut targets = raw.clone();
    let (mut next_value, mut next_advantage) = (bootstrap, 0.);
    for i in (0..values.len()).rev() {
        let continuation = if terminals[i] { 0. } else { 1. };
        let delta = rewards[i] + gamma * continuation * next_value - values[i];
        let advantage = delta + gamma * lambda * continuation * next_advantage;
        raw[i] = advantage as f32;
        targets[i] = (advantage + values[i]) as f32;
        next_value = values[i];
        next_advantage = advantage;
    }
    Ok((raw, targets))
}
pub fn normalize(raw: &Tensor, normalization: &str) -> Result<Tensor> {
    let centered = raw - raw.mean(Kind::Float);
    match normalization {
        "center" => Ok(centered),
        "standardize" => Ok(centered / raw.std(false).clamp_min(1e-8)),
        _ => anyhow::bail!("aic-learning.gae-normalization: unknown {normalization}"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unfinished_chunks_bootstrap_but_real_terminal_does_not() {
        let (_, unfinished) = gae(&[0., 1.], &[0.2, 0.3], &[false, false], 2., 1., 1.).unwrap();
        let (_, terminal) = gae(&[0., 1.], &[0.2, 0.3], &[false, true], 2., 1., 1.).unwrap();
        assert_eq!(unfinished, vec![3., 3.]);
        assert_eq!(terminal, vec![1., 1.]);
        assert!(gae(&[0., 0.], &[0., 0.], &[true, false], 0., 1., 1.).is_err());
    }
}
