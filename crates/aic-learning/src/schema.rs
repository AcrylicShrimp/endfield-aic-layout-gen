//! Versioned interchange and checkpoint metadata; no LibTorch dependency.
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelConfig {
    pub width: i64,
    pub message_rounds: usize,
    pub node_features: i64,
    pub edge_features: i64,
    pub global_features: i64,
    pub action_features: i64,
    pub port_features: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConstructiveModelConfig {
    pub schema: String,
    pub width: i64,
    pub message_rounds: usize,
}

impl ConstructiveModelConfig {
    pub const SCHEMA: &'static str = "aic-constructive-attention-model-v1";

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == Self::SCHEMA
                && (8..=4096).contains(&self.width)
                && self.message_rounds <= 32,
            "aic-learning.constructive-model-config: invalid schema or dimensions"
        );
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfig {
    #[serde(default = "default_adam_backend")]
    pub adam_backend: String,
    pub learning_rate: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub epsilon: f64,
    pub clip_epsilon: f64,
    pub entropy_coefficient: f64,
    pub imitation_coefficient: f64,
    pub gradient_clip: f64,
}
fn default_adam_backend() -> String {
    "foreach".into()
}
impl UpdateConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.adam_backend.as_str(), "foreach" | "fused"),
            "aic-learning.adam-backend: unsupported {}",
            self.adam_backend
        );
        ensure!(
            self.learning_rate.is_finite()
                && self.learning_rate > 0.
                && (0. ..1.).contains(&self.beta1)
                && (0. ..1.).contains(&self.beta2)
                && self.epsilon.is_finite()
                && self.epsilon > 0.
                && (0. ..1.).contains(&self.clip_epsilon)
                && self.entropy_coefficient.is_finite()
                && self.entropy_coefficient >= 0.
                && self.imitation_coefficient.is_finite()
                && self.imitation_coefficient >= 0.
                && self.gradient_clip.is_finite()
                && self.gradient_clip > 0.,
            "aic-learning.config: invalid update parameters"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Archive {
    pub path: String,
    pub sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn update_parameters_reject_nonfinite_and_invalid_bounds() {
        let mut value = UpdateConfig {
            adam_backend: "foreach".into(),
            learning_rate: 0.0003,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            clip_epsilon: 0.2,
            entropy_coefficient: 0.,
            imitation_coefficient: 0.2,
            gradient_clip: 0.5,
        };
        assert!(value.validate().is_ok());
        value.beta1 = 1.;
        assert!(value.validate().is_err());
        value.beta1 = 0.9;
        value.learning_rate = f64::NAN;
        assert!(value.validate().is_err());
    }
}
