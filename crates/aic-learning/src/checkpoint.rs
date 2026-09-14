//! One complete learner checkpoint, published by directory rename. This does not
//! introduce a per-step journal or claim to include a not-yet-integrated factory pool.
use crate::{
    input::{Input, Tensors, sha256},
    schema::{CHECKPOINT_SCHEMA, CheckpointManifest},
    update::Learner,
};
use aic_torch::tch::{Device, Tensor};
use anyhow::{Context, Result, ensure};
use std::{fs, io::Write, path::Path};

pub fn device_name(device: Device) -> Result<String> {
    match device {
        Device::Cpu => Ok("cpu".into()),
        Device::Cuda(n) => Ok(format!("cuda:{n}")),
        _ => anyhow::bail!("aic-learning.device: CPU/CUDA required"),
    }
}

pub fn save(learner: &mut Learner, input: &Input, directory: &Path) -> Result<CheckpointManifest> {
    ensure!(
        !directory.exists(),
        "aic-learning.checkpoint-exists: {}",
        directory.display()
    );
    let parent = directory.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let name = directory
        .file_name()
        .context("aic-learning.checkpoint-path: expected a directory name")?
        .to_string_lossy();
    let staging = parent.join(format!(".{name}.pending-{}", std::process::id()));
    fs::create_dir(&staging)
        .with_context(|| format!("aic-learning.checkpoint-stage: {}", staging.display()))?;
    let result = (|| {
        let tensors = learner.snapshot()?;
        let archive = staging.join("state.safetensors");
        Tensor::write_safetensors(&tensors.iter().collect::<Vec<_>>(), &archive)?;
        fs::File::open(&archive)?.sync_all()?;
        let manifest = CheckpointManifest {
            schema: CHECKPOINT_SCHEMA.into(),
            input_sha256: input.identity.clone(),
            tensors_sha256: sha256(&archive)?,
            model: learner.model.config.clone(),
            update: learner.config.clone(),
            parameter_order: learner
                .model
                .parameters
                .iter()
                .map(|p| p.0.clone())
                .collect(),
            completed_steps: learner.completed_steps,
            device: device_name(learner.device)?,
        };
        let mut file = fs::File::create(staging.join("manifest.json"))?;
        file.write_all(&serde_json::to_vec_pretty(&manifest)?)?;
        file.sync_all()?;
        fs::File::open(&staging)?.sync_all()?;
        fs::rename(&staging, directory)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(manifest)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn read(directory: &Path, max_bytes: u64) -> Result<(CheckpointManifest, Tensors)> {
    let manifest_path = directory.join("manifest.json");
    ensure!(
        fs::metadata(&manifest_path)?.len() <= 1024 * 1024,
        "aic-learning.checkpoint-size: {}",
        manifest_path.display()
    );
    let manifest: CheckpointManifest = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    ensure!(
        manifest.schema == CHECKPOINT_SCHEMA,
        "aic-learning.checkpoint-schema: {}",
        directory.display()
    );
    let archive = directory.join("state.safetensors");
    ensure!(
        fs::metadata(&archive)?.len() <= max_bytes,
        "aic-learning.checkpoint-size: {}",
        archive.display()
    );
    ensure!(
        sha256(&archive)? == manifest.tensors_sha256,
        "aic-learning.checkpoint-hash: {}",
        archive.display()
    );
    let rows = Tensor::read_safetensors(&archive)?;
    let count = rows.len();
    let tensors: Tensors = rows.into_iter().collect();
    ensure!(
        tensors.len() == count,
        "aic-learning.checkpoint-duplicate: {}",
        directory.display()
    );
    Ok((manifest, tensors))
}

pub fn restore(directory: &Path, input: &Input, device: Device) -> Result<Learner> {
    let (manifest, tensors) = read(directory, input.manifest.max_tensor_bytes)?;
    ensure!(
        manifest.input_sha256 == input.identity
            && manifest.model == input.manifest.model
            && manifest.update == input.manifest.update
            && manifest.parameter_order == input.manifest.parameter_order
            && manifest.device == device_name(device)?,
        "aic-learning.checkpoint-binding: {}",
        directory.display()
    );
    let learner = Learner::load(
        &manifest.model,
        &manifest.update,
        &manifest.parameter_order,
        &tensors,
        device,
        manifest.completed_steps,
    )
    .with_context(|| format!("aic-learning.checkpoint-restore: {}", directory.display()))?;
    // Restore already validates each required field. Reject unknown fields by name
    // without copying model/Adam state back from the GPU for a second validation.
    let mut expected = std::collections::BTreeSet::new();
    for name in &manifest.parameter_order {
        expected.insert(format!("model.{name}"));
        let label = if name.starts_with("critic.") {
            "critic"
        } else {
            "actor"
        };
        let prefix = format!("adam.{label}.{name}");
        expected.insert(format!("{prefix}.step"));
        if i64::try_from(&tensors[&format!("{prefix}.step")])? > 0 {
            expected.insert(format!("{prefix}.mean"));
            expected.insert(format!("{prefix}.square"));
        }
    }
    for name in ["sampler", "shuffle", "replay", "replay_order"] {
        expected.insert(format!("rng.{name}"));
    }
    ensure!(
        expected == tensors.keys().cloned().collect(),
        "aic-learning.checkpoint-state: unknown state fields at {}",
        directory.display()
    );
    Ok(learner)
}
