//! Actual episode ownership at a completed optimizer boundary.
use super::curriculum::{Plan, Root};
use aic_pilot_native::generation::source::Generated;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct Episode {
    pub input: Generated,
    pub root: Root,
    pub depth: u8,
    pub start_area: u64,
    pub product: String,
}
#[derive(Serialize, Deserialize)]
pub struct State {
    pub update: u64,
    pub samples: u64,
    pub wave: u64,
    pub actions: u64,
    pub episodes: Vec<Episode>,
    /// Present only after a fully completed wave, before admission.
    pub next: Option<Plan>,
    pub best_evaluation: Option<f64>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            update: 0,
            samples: 0,
            wave: 0,
            actions: 0,
            episodes: vec![],
            next: None,
            best_evaluation: None,
        }
    }
}

impl State {
    pub fn validate(&self, config: &super::config::Config) -> anyhow::Result<()> {
        use anyhow::ensure;
        ensure!(
            self.update > 0
                && self
                    .update
                    .checked_mul((config.streams * config.chunk_actions) as u64)
                    == Some(self.samples)
                && self.actions > 0
                && self.actions <= config.native.max_actions
                && self.actions % config.chunk_actions as u64 == 0
                && self.best_evaluation.is_none_or(f64::is_finite),
            "aic-learning.checkpoint-state: inconsistent update/sample/action boundary"
        );
        if let Some(plan) = &self.next {
            ensure!(
                self.episodes.is_empty()
                    && self.actions == config.native.max_actions
                    && self.wave.checked_add(1) == Some(plan.wave)
                    && plan.slots.len() == config.streams,
                "aic-learning.checkpoint-plan: incomplete next wave"
            );
            let mut actual = [0usize; 3];
            for (i, slot) in plan.slots.iter().enumerate() {
                ensure!(
                    slot.stream == i
                        && slot.wave == plan.wave
                        && slot.depth <= 2
                        && slot.requested_depth <= 2
                        && (slot.depth > 0) == slot.inherited.is_some()
                        && (slot.depth > 0) == slot.parent.is_some()
                        && if slot.depth == 0 {
                            slot.root.is_none()
                        } else {
                            slot.root.as_ref().is_some_and(|r| {
                                r.wave.checked_add(slot.depth as u64) == Some(slot.wave)
                            })
                        },
                    "aic-learning.checkpoint-plan: invalid logical slot {i}"
                );
                actual[slot.depth as usize] += 1;
            }
            ensure!(
                actual == plan.actual
                    && plan.requested == super::curriculum::quotas(config.streams, plan.wave)?,
                "aic-learning.checkpoint-plan: quota mismatch"
            );
        } else {
            ensure!(
                self.actions < config.native.max_actions
                    && self.episodes.len() == config.streams
                    && self.episodes.iter().all(|e| e.depth <= 2
                        && e.root.wave.checked_add(e.depth as u64) == Some(self.wave)
                        && e.start_area > 0),
                "aic-learning.checkpoint-episodes: inconsistent live episode ownership"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_rejects_missing_slots_and_wrong_sample_identity() {
        let config: super::super::config::Config = serde_json::from_str(include_str!(
            "../../../../tools/native-training/training.json"
        ))
        .unwrap();
        let plan = super::super::curriculum::choose(
            &config.curriculum,
            config.initialization_seed,
            1,
            config.streams,
            &[],
            &Default::default(),
        )
        .unwrap();
        let mut state = State {
            update: 2,
            samples: 8192,
            wave: 0,
            actions: 64,
            episodes: vec![],
            next: Some(plan),
            best_evaluation: None,
        };
        state.validate(&config).unwrap();
        state.samples -= 1;
        assert!(state.validate(&config).is_err());
        state.samples += 1;
        state.next.as_mut().unwrap().slots.pop();
        assert!(state.validate(&config).is_err());
    }
}
