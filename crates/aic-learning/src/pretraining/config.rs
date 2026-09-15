use crate::schema::ModelConfig;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const SCHEMA: &str = "aic-physical-outcome-pretraining-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetBinding {
    pub manifest_sha256: String,
    pub config_sha256: String,
    pub catalog_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Optimizer {
    pub learning_rate: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub epsilon: f64,
    pub gradient_clip: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Losses {
    pub completion: f64,
    pub changed: f64,
    pub pending_flag: f64,
    pub conflict_flag: f64,
    pub unchanged: f64,
    pub rejected: f64,
    pub pending_count: f64,
    pub conflict_count: f64,
    pub area: f64,
    pub transport: f64,
    pub turns: f64,
    pub route_budget: f64,
}

impl Losses {
    pub fn values(&self) -> [f64; 12] {
        [
            self.completion,
            self.changed,
            self.pending_flag,
            self.conflict_flag,
            self.unchanged,
            self.rejected,
            self.pending_count,
            self.conflict_count,
            self.area,
            self.transport,
            self.turns,
            self.route_budget,
        ]
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub max_elapsed_seconds: u64,
    pub max_rss_bytes: u64,
    pub max_output_bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Gate {
    pub minimum_auroc: f64,
    pub minimum_metric_mae_improvement: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub dataset: DatasetBinding,
    pub model: ModelConfig,
    pub seeds: Vec<u64>,
    pub epochs: usize,
    pub roots_per_batch: usize,
    pub optimizer: Optimizer,
    pub count_scale: f64,
    pub losses: Losses,
    pub gate: Gate,
    pub resources: Resources,
}

impl Config {
    pub fn load(path: &Path) -> Result<(Self, Vec<u8>)> {
        ensure!(
            fs::metadata(path)?.len() <= 1024 * 1024,
            "aic-learning.pretraining-config: oversized {}",
            path.display()
        );
        let bytes = fs::read(path)?;
        let value: Self = serde_json::from_slice(&bytes)?;
        value.validate()?;
        Ok((value, bytes))
    }

    fn validate(&self) -> Result<()> {
        let expected = ModelConfig {
            width: 40,
            message_rounds: 2,
            node_features: 41,
            edge_features: 37,
            global_features: 15,
            action_features: 12,
            port_features: 11,
        };
        ensure!(
            self.schema == SCHEMA
                && self.model == expected
                && self.seeds == [930_000, 930_001]
                && self.epochs == 12
                && self.roots_per_batch == 4,
            "aic-learning.pretraining-config: expected registered model, seeds and schedule"
        );
        ensure!(
            self.dataset.manifest_sha256.len() == 64
                && self.dataset.config_sha256.len() == 64
                && self.dataset.catalog_sha256.len() == 64,
            "aic-learning.pretraining-config: invalid dataset binding"
        );
        ensure!(
            self.optimizer.learning_rate == 0.0003
                && self.optimizer.beta1 == 0.9
                && self.optimizer.beta2 == 0.999
                && self.optimizer.epsilon == 1e-8
                && self.optimizer.gradient_clip == 1.0
                && self.count_scale == 64.0
                && self
                    .losses
                    .values()
                    .iter()
                    .all(|v| v.is_finite() && *v > 0.0),
            "aic-learning.pretraining-config: invalid optimizer, scale or loss weights"
        );
        ensure!(
            self.gate.minimum_auroc == 0.60
                && self.gate.minimum_metric_mae_improvement == 0.05
                && self.resources.max_elapsed_seconds == 1800
                && self.resources.max_rss_bytes == 8 * 1024 * 1024 * 1024
                && self.resources.max_output_bytes == 512 * 1024 * 1024,
            "aic-learning.pretraining-config: expected registered gate and resource bounds"
        );
        Ok(())
    }
}
