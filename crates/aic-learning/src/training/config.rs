//! Versioned external training configuration; no game constants in runtime code.
use super::{curriculum, evaluation, replay, schedule};
use crate::{
    collection::rollout::Settings,
    schema::{ModelConfig, UpdateConfig},
};
use aic_pilot_native::{
    generation::source,
    input::CatalogPaths,
    learning::{Limits, Options},
};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retention {
    pub checkpoint_interval_updates: u64,
    pub recovery_checkpoints: usize,
    pub keep_updates: Vec<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    pub max_observation_bytes: u64,
    pub max_rss_bytes: u64,
    pub max_output_bytes: u64,
    pub min_free_bytes: u64,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub purpose: String,
    pub catalog_root: PathBuf,
    pub catalogs: CatalogPaths,
    pub initialization_seed: u64,
    pub model: ModelConfig,
    pub optimizer: UpdateConfig,
    pub ppo: schedule::Config,
    pub streams: usize,
    pub workers: usize,
    pub chunk_actions: usize,
    pub minibatch_size: usize,
    pub native: Limits,
    pub actions: Options,
    pub gamma: f64,
    pub gae_lambda: f64,
    pub advantage_normalization: String,
    pub generation: source::Config,
    pub strata: BTreeMap<String, Vec<String>>,
    pub success: replay::Config,
    pub curriculum: curriculum::Config,
    pub evaluation: evaluation::Config,
    pub retention: Retention,
    pub resources: Resources,
    pub duration_seconds: u64,
    pub finish_grace_seconds: u64,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        ensure!(
            fs::metadata(path)?.len() <= 1024 * 1024,
            "aic-learning.training-config: oversized {}",
            path.display()
        );
        let mut config: Self = serde_json::from_slice(&fs::read(path)?)?;
        config.validate()?;
        let root = path.parent().unwrap_or(Path::new("."));
        config.catalog_root = root.join(&config.catalog_root).canonicalize()?;
        config.evaluation.panel = root.join(&config.evaluation.panel);
        Ok(config)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "aic-training-v1"
                && matches!(self.purpose.as_str(), "training" | "bounded-comparison")
                && (1..=4096).contains(&self.streams)
                && self.workers > 0
                && self.workers <= self.streams
                && (1..=4096).contains(&self.chunk_actions)
                && (1..=4096).contains(&self.native.max_actions)
                && self.native.max_actions % self.chunk_actions as u64 == 0
                && self.minibatch_size > 0
                && self.minibatch_size <= self.streams * self.chunk_actions
                && self.initialization_seed <= (u64::MAX - 10_000_000) / 1000
                && (0. ..=1.).contains(&self.gamma)
                && (0. ..=1.).contains(&self.gae_lambda)
                && matches!(
                    self.advantage_normalization.as_str(),
                    "center" | "standardize"
                )
                && self.retention.checkpoint_interval_updates > 0
                && self.retention.recovery_checkpoints > 0
                && self.retention.recovery_checkpoints <= 1024
                && self.duration_seconds > 0
                && self.finish_grace_seconds > 0
                && self.resources.max_observation_bytes > 0
                && self.resources.max_rss_bytes > 0
                && self.resources.max_output_bytes > 0,
            "aic-learning.training-config: invalid schema, execution bounds or diagnostic prefix"
        );
        ensure!(
            self.model.node_features
                == (aic_pilot_native::learning::schema::NODE_STATIC
                    + aic_pilot_native::learning::schema::NODE_STATE) as i64
                && self.model.edge_features
                    == (aic_pilot_native::learning::schema::EDGE_STATIC
                        + aic_pilot_native::learning::schema::EDGE_STATE)
                        as i64
                && self.model.global_features == aic_pilot_native::learning::GLOBAL_FEATURES as i64
                && self.model.action_features
                    == aic_pilot_native::learning::schema::ACTION_FEATURES as i64
                && self.model.port_features
                    == aic_pilot_native::learning::schema::PORT_FEATURES as i64,
            "aic-learning.model-schema: model dimensions differ from native graph feature schema"
        );
        ensure!(
            self.strata.len() == 2
                && self.strata.values().all(|p| !p.is_empty())
                && self.generation.products.iter().all(|p| self
                    .strata
                    .values()
                    .filter(|ps| ps.contains(&p.id))
                    .count()
                    == 1),
            "aic-learning.training-strata: every generated product needs one of two nonempty strata"
        );
        self.optimizer.validate()?;
        self.ppo.validate()?;
        self.success.validate()?;
        self.curriculum.validate()?;
        self.evaluation.validate()?;
        Ok(())
    }
    pub(crate) fn collection(&self, wave: u64) -> Settings {
        Settings {
            episode_ids: (0..self.streams)
                .map(|s| format!("{}/{wave}/{s}", self.initialization_seed))
                .collect(),
            episode_actions: self.native.max_actions,
            chunk_actions: self.chunk_actions,
            minibatch_size: self.minibatch_size,
            gamma: self.gamma,
            gae_lambda: self.gae_lambda,
            advantage_normalization: self.advantage_normalization.clone(),
            max_observation_bytes: self.resources.max_observation_bytes,
        }
    }
}
