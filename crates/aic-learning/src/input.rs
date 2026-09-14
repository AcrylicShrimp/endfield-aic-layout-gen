#[cfg(test)]
mod tests;
mod validation;
// Initial learner/replay seed. External tensors are validated once on admission.
use crate::schema::{INPUT_SCHEMA, SeedManifest};
use aic_torch::tch::{Device, Kind, Tensor};
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

pub type Tensors = BTreeMap<String, Tensor>;
pub fn sha256(path: &Path) -> Result<String> {
    let mut file =
        fs::File::open(path).with_context(|| format!("aic-learning.input: {}", path.display()))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}
pub fn tensor<'a>(values: &'a Tensors, name: &str) -> Result<&'a Tensor> {
    values
        .get(name)
        .with_context(|| format!("aic-learning.tensor-missing: {name}"))
}

pub struct Packed {
    pub graph: Tensors,
    pub branches: Option<Tensors>,
    pub roots: Tensor,
    pub selected: Tensor,
    pub graphs: i64,
    pub factors: i64,
}
impl Packed {
    fn load(
        values: &Tensors,
        prefix: &str,
        graphs: i64,
        factors: i64,
        device: Device,
        config: &crate::schema::ModelConfig,
    ) -> Result<Self> {
        ensure!(
            graphs > 0 && graphs <= 4096 && factors >= 0 && factors <= 262144,
            "aic-learning.input-size: {prefix}"
        );
        validation::packed(values, prefix, graphs, factors, config)?;
        let copy = |key: &str| -> Tensors {
            values
                .iter()
                .filter_map(|(name, t)| {
                    name.strip_prefix(key)
                        .map(|n| (n.to_owned(), t.to_device(device)))
                })
                .collect()
        };
        let graph = copy(&format!("{prefix}.batch."));
        let branches = if factors > 0 {
            Some(copy(&format!("{prefix}.branches.")))
        } else {
            None
        };
        let roots = tensor(values, &format!("{prefix}.roots"))?.to_device(device);
        let selected = tensor(values, &format!("{prefix}.selected"))?.to_device(device);
        Ok(Self {
            graph,
            branches,
            roots,
            selected,
            graphs,
            factors,
        })
    }
}

pub struct Seed {
    pub manifest: SeedManifest,
    pub identity: String,
    directory: std::path::PathBuf,
    pub replay: ReplayBatch,
}
impl Seed {
    pub fn load(path: &Path, device: Device) -> Result<Self> {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .with_context(|| format!("aic-learning.manifest: {}", path.display()))?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 1024 * 1024,
            "aic-learning.manifest-size: {}",
            path.display()
        );
        let manifest: SeedManifest = serde_json::from_slice(&bytes)?;
        ensure!(
            manifest.schema == INPUT_SCHEMA,
            "aic-learning.input-schema: {}",
            manifest.schema
        );
        ensure!(
            manifest.max_tensor_bytes > 0 && manifest.max_tensor_bytes <= 2 * 1024 * 1024 * 1024,
            "aic-learning.input-budget: invalid tensor byte bound"
        );
        let directory = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let tensors = archive(&directory, &manifest.replay, manifest.max_tensor_bytes)?;
        let sil = Packed::load(
            &tensors,
            "sil",
            manifest.sil_graphs,
            manifest.sil_factors,
            device,
            &manifest.model,
        )?;
        let mut targets = Tensors::new();
        for (name, size) in [
            ("sil.returns", manifest.sil_graphs),
            ("sil.weights", manifest.sil_graphs),
        ] {
            let t = tensor(&tensors, name)?;
            ensure!(
                t.kind() == Kind::Float
                    && t.size() == [size]
                    && bool::try_from(t.isfinite().all())?,
                "aic-learning.input-target: {name}"
            );
            targets.insert(name.into(), t.to_device(device));
        }
        // Bind the checkpoint to the exact validated manifest, including archive hash.
        let identity = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        Ok(Self {
            manifest,
            identity,
            directory,
            replay: ReplayBatch {
                packed: sil,
                returns: targets.remove("sil.returns").unwrap(),
                weights: targets.remove("sil.weights").unwrap(),
            },
        })
    }
    pub fn initial_state(&self) -> Result<Tensors> {
        archive(
            &self.directory,
            &self.manifest.state,
            self.manifest.max_tensor_bytes,
        )
    }
}

/// Frozen actual behavior rows. Constructed directly by the rollout owner.
pub struct PpoBatch {
    pub packed: Packed,
    pub old_logp: Tensor,
    pub advantages: Tensor,
    pub targets: Tensor,
}
pub struct ReplayBatch {
    pub packed: Packed,
    pub returns: Tensor,
    pub weights: Tensor,
}

fn archive(
    directory: &Path,
    reference: &crate::schema::Archive,
    max_bytes: u64,
) -> Result<Tensors> {
    let relative = Path::new(&reference.path);
    ensure!(
        relative.components().count() == 1 && relative.file_name().is_some(),
        "aic-learning.input-path: expected a sibling tensor file"
    );
    let archive = directory.join(relative);
    ensure!(
        fs::metadata(&archive)?.len() <= max_bytes,
        "aic-learning.input-budget: {}",
        archive.display()
    );
    ensure!(
        sha256(&archive)? == reference.sha256,
        "aic-learning.input-hash: {}",
        archive.display()
    );
    let rows = Tensor::read_safetensors(&archive)
        .with_context(|| format!("aic-learning.input-archive: {}", archive.display()))?;
    let tensors = rows.into_iter().collect();
    Ok(tensors)
}
