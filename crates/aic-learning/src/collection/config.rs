use aic_pilot_native::learning::{Limits, Options};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileRef {
    pub path: PathBuf,
    pub sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Episode {
    pub id: String,
    pub source: Value,
    pub geometry_sha256: String,
    pub request: FileRef,
    pub original: FileRef,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub catalog_root: PathBuf,
    pub catalogs: Value,
    pub episodes: Vec<Episode>,
    pub reserved_geometry: Vec<String>,
    pub options: Options,
    pub limits: Limits,
    pub workers: usize,
    pub chunk_actions: usize,
    pub gamma: f64,
    pub gae_lambda: f64,
    pub advantage_normalization: String,
    pub minibatch_size: usize,
    pub max_observation_bytes: u64,
}
impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        ensure!(
            fs::metadata(path)?.len() <= 1024 * 1024,
            "aic-learning.collection-config: manifest too large at {}",
            path.display()
        );
        let c: Self = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(
            c.schema == "aic-native-collection-input-v1"
                && !c.episodes.is_empty()
                && c.episodes.len() <= 4096
                && c.workers > 0
                && c.workers <= c.episodes.len()
                && c.chunk_actions > 0
                && c.chunk_actions as u64 <= c.limits.max_actions
                && c.minibatch_size > 0
                && c.chunk_actions
                    .checked_mul(c.episodes.len())
                    .is_some_and(|n| c.minibatch_size <= n)
                && c.max_observation_bytes > 0
                && c.max_observation_bytes <= 8 * 1024 * 1024 * 1024
                && (0. ..=1.).contains(&c.gamma)
                && (0. ..=1.).contains(&c.gae_lambda)
                && matches!(c.advantage_normalization.as_str(), "center" | "standardize"),
            "aic-learning.collection-config: invalid bounds/schema at {}",
            path.display()
        );
        let mut ids = BTreeSet::new();
        for e in &c.episodes {
            ensure!(
                !e.id.is_empty()
                    && ids.insert(&e.id)
                    && e.geometry_sha256.len() == 64
                    && !c.reserved_geometry.contains(&e.geometry_sha256),
                "aic-learning.collection-reservation: duplicate identity or reserved geometry for {}",
                e.id
            );
        }
        Ok(c)
    }
}
pub fn read_bound<T: serde::de::DeserializeOwned>(root: &Path, reference: &FileRef) -> Result<T> {
    let path = root.join(&reference.path);
    use std::io::Read;
    let mut bytes = Vec::new();
    fs::File::open(&path)
        .with_context(|| format!("aic-learning.collection-input: {}", path.display()))?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "aic-learning.collection-input-size: {}",
        path.display()
    );
    ensure!(
        aic_pilot_native::input::hash(&bytes) == reference.sha256,
        "aic-learning.collection-input-hash: {}",
        path.display()
    );
    serde_json::from_slice(&bytes)
        .with_context(|| format!("aic-learning.collection-input: {}", path.display()))
}
