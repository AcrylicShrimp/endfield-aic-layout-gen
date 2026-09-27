#[cfg(test)]
mod tests;
#[cfg(test)]
mod validation;
// Native numerical batches; retained interchange validation is test-only.
use aic_torch::tch::{Device, Tensor};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

pub type Tensors = BTreeMap<String, Tensor>;

#[derive(Deserialize)]
struct TrainingArtifactIdentity {
    status: String,
    catalog_sha256: String,
    checkpoint_sha256: String,
}

#[derive(Deserialize)]
struct GenerationArtifactIdentity {
    schema: String,
    status: String,
    catalog_sha256: String,
    lower_checkpoint_sha256: String,
    outer_checkpoint_sha256: String,
}

fn read_artifact_report<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let size = fs::metadata(path)
        .with_context(|| format!("aic-learning.artifact-report: {}", path.display()))?
        .len();
    ensure!(
        size <= 16 * 1024 * 1024,
        "aic-learning.artifact-report: report exceeds input limit: {}",
        path.display()
    );
    let bytes = fs::read(path)
        .with_context(|| format!("aic-learning.artifact-report: {}", path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("aic-learning.artifact-report: {}", path.display()))
}

pub(crate) fn verify_training_artifact(
    output: &Path,
    catalog_sha256: &str,
    checkpoint_sha256: &str,
) -> Result<()> {
    let direct_report = output.join("report.json");
    match fs::symlink_metadata(&direct_report) {
        Ok(_) => {
            let identity: TrainingArtifactIdentity = read_artifact_report(&direct_report)?;
            ensure!(
                identity.status == "complete"
                    && identity.catalog_sha256 == catalog_sha256
                    && identity.checkpoint_sha256 == checkpoint_sha256,
                "aic-learning.artifact-identity: training artifact catalog or checkpoint differs: {}",
                direct_report.display()
            );
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let kind = fs::symlink_metadata(output)
                .with_context(|| format!("aic-learning.artifact-directory: {}", output.display()))?
                .file_type();
            ensure!(
                kind.is_dir(),
                "aic-learning.artifact-directory: expected a real directory: {}",
                output.display()
            );
            let role = output.file_name().and_then(|name| name.to_str());
            ensure!(
                matches!(role, Some("lower" | "outer")),
                "aic-learning.artifact-role: expected lower or outer directory: {}",
                output.display()
            );
            let parent_report = output
                .parent()
                .context("aic-learning.artifact-directory: model output has no parent")?
                .join("report.json");
            let identity: GenerationArtifactIdentity = read_artifact_report(&parent_report)?;
            let child_checkpoint = match role {
                Some("lower") => &identity.lower_checkpoint_sha256,
                Some("outer") => &identity.outer_checkpoint_sha256,
                _ => unreachable!(),
            };
            ensure!(
                identity.schema == crate::hierarchical_generation::REPORT_SCHEMA
                    && identity.status == "complete"
                    && identity.catalog_sha256 == catalog_sha256
                    && child_checkpoint == checkpoint_sha256,
                "aic-learning.artifact-identity: generation artifact schema, status, catalog or checkpoint differs: {}",
                parent_report.display()
            );
        }
        Err(error) => {
            return Err(error).with_context(|| {
                format!("aic-learning.artifact-report: {}", direct_report.display())
            });
        }
    }
    Ok(())
}

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
    pub fn to_device(&self, device: Device) -> Self {
        let copy = |rows: &Tensors| {
            rows.iter()
                .map(|(n, t)| (n.clone(), t.to_device(device)))
                .collect()
        };
        Self {
            graph: copy(&self.graph),
            branches: self.branches.as_ref().map(copy),
            roots: self.roots.to_device(device),
            selected: self.selected.to_device(device),
            graphs: self.graphs,
            factors: self.factors,
        }
    }
    #[cfg(test)]
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
impl PpoBatch {
    pub fn to_device(&self, device: Device) -> Self {
        Self {
            packed: self.packed.to_device(device),
            old_logp: self.old_logp.to_device(device),
            advantages: self.advantages.to_device(device),
            targets: self.targets.to_device(device),
        }
    }
}
impl ReplayBatch {
    pub fn to_device(&self, device: Device) -> Self {
        Self {
            packed: self.packed.to_device(device),
            returns: self.returns.to_device(device),
            weights: self.weights.to_device(device),
        }
    }
}
