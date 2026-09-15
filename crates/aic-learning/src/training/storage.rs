//! Atomic boundary publication with immutable sparse replay chunks.
use super::{
    config::Config,
    frozen::Frozen,
    replay::{Buffer, Reference, State as ReplayState},
    resources::Ledger,
    state::State,
};
use crate::{
    collection::storage::{data, write},
    input::sha256,
    schema::Archive,
    update::{Learner, device_name},
};
use aic_pilot_native::learning::pool::{Command, Pool};
use aic_torch::tch::{Device, Tensor};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

const CHECKPOINT_SCHEMA: &str = "aic-training-checkpoint-v4";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub binding: String,
    pub device: String,
    pub completed_steps: u64,
    pub parameter_order: Vec<String>,
    pub files: BTreeMap<String, String>,
    pub replay: BTreeMap<u64, Archive>,
}
#[derive(Default)]
pub struct Store {
    pub best_model: Option<crate::input::Tensors>,
    pub chunks: BTreeMap<u64, Archive>,
    pub checkpoints: BTreeMap<u64, Manifest>,
    pub ledger: Ledger,
}
impl Store {
    fn persist_inputs(&mut self, root: &Path, buffer: &Buffer, ordinal: u64) -> Result<()> {
        let rows: Vec<_> = buffer
            .inputs
            .iter()
            .filter(|(r, _)| r[0] == ordinal)
            .map(|(r, v)| (*r, v.as_ref()))
            .collect();
        if rows.is_empty() {
            return Ok(());
        }
        let name = format!("replay/{ordinal:08}.bin");
        let path = root.join(&name);
        ensure!(
            !path.exists(),
            "aic-learning.replay-file-exists: {}",
            path.display()
        );
        let mut writer = BufWriter::new(fs::File::create(&path)?);
        postcard::to_io(&rows, &mut writer)?;
        writer.flush()?;
        writer.get_ref().sync_all()?;
        self.ledger.track(&path)?;
        self.chunks.insert(
            ordinal,
            Archive {
                path: name.into(),
                sha256: sha256(&path)?,
            },
        );
        Ok(())
    }
    pub fn save(
        &mut self,
        root: &Path,
        binding: &str,
        state: &State,
        buffer: &Buffer,
        learner: &mut Learner,
        pool: &mut Pool,
        order: &[String],
        config: &Config,
    ) -> Result<PathBuf> {
        // Save newly retained inputs at the checkpoint cadence. A terminal
        // boundary discards unsuccessful preparation before any normal disk I/O.
        let pending: BTreeSet<_> = buffer
            .inputs
            .keys()
            .map(|r| r[0])
            .filter(|n| !self.chunks.contains_key(n))
            .collect();
        for ordinal in pending {
            self.persist_inputs(root, buffer, ordinal)?;
        }
        let path = root.join(format!("checkpoints/{:08}", state.update));
        ensure!(
            !path.exists(),
            "aic-learning.checkpoint-exists: {}",
            path.display()
        );
        let stage = path.with_extension("pending");
        fs::create_dir(&stage)?;
        write(&stage.join("state.json"), state)?;
        write(&stage.join("replay.json"), &buffer.state)?;
        // Finished episodes are represented by the actual selected next starts.
        if state.next.is_none() && !state.episodes.is_empty() {
            let sessions = data(
                pool.exchange(
                    (0..config.streams)
                        .map(|i| (i, Command::Snapshot))
                        .collect(),
                )
                .map_err(anyhow::Error::msg)?,
            )?;
            write(&stage.join("sessions.json"), &sessions)?;
        }
        Tensor::write_safetensors(
            &learner.snapshot()?.iter().collect::<Vec<_>>(),
            stage.join("learner.safetensors"),
        )?;
        if let Some(best) = &self.best_model {
            Tensor::write_safetensors(
                &best.iter().collect::<Vec<_>>(),
                stage.join("best.safetensors"),
            )?;
        }
        let mut files = BTreeMap::new();
        for name in [
            "state.json",
            "replay.json",
            "sessions.json",
            "learner.safetensors",
            "best.safetensors",
        ] {
            let file = stage.join(name);
            if file.exists() {
                fs::File::open(&file)?.sync_all()?;
                files.insert(name.into(), sha256(&file)?);
            }
        }
        let needed: BTreeSet<_> = buffer.references().iter().map(|r| r[0]).collect();
        let replay = needed
            .into_iter()
            .map(|i| {
                self.chunks.get(&i).cloned().map(|a| (i, a)).ok_or_else(|| {
                    anyhow::anyhow!("aic-learning.replay-checkpoint: missing chunk {i}")
                })
            })
            .collect::<Result<_>>()?;
        let manifest = Manifest {
            schema: CHECKPOINT_SCHEMA.into(),
            binding: binding.into(),
            device: device_name(learner.device)?,
            completed_steps: learner.completed_steps,
            parameter_order: order.into(),
            files,
            replay,
        };
        write(&stage.join("manifest.json"), &manifest)?;
        fs::File::open(stage.join("manifest.json"))?.sync_all()?;
        fs::File::open(&stage)?.sync_all()?;
        fs::rename(&stage, &path)?;
        fs::File::open(path.parent().unwrap())?.sync_all()?;
        for name in manifest
            .files
            .keys()
            .map(String::as_str)
            .chain(["manifest.json"])
        {
            self.ledger.track(&path.join(name))?;
        }
        self.ledger.enforce(config.resources.max_output_bytes)?;
        let pointer = root.join("latest.pending");
        write(&pointer, &format!("checkpoints/{:08}", state.update))?;
        fs::File::open(&pointer)?.sync_all()?;
        fs::rename(&pointer, root.join("latest.json"))?;
        fs::File::open(root)?.sync_all()?;
        self.ledger.track(&root.join("latest.json"))?;
        self.checkpoints.insert(state.update, manifest);
        self.prune(root, state, buffer, config)?;
        Ok(path)
    }
    fn prune(
        &mut self,
        root: &Path,
        state: &State,
        buffer: &Buffer,
        config: &Config,
    ) -> Result<()> {
        let recent: BTreeSet<_> = self
            .checkpoints
            .keys()
            .rev()
            .take(config.retention.recovery_checkpoints)
            .copied()
            .chain(config.retention.keep_updates.iter().copied())
            .collect();
        let remove: Vec<_> = self
            .checkpoints
            .keys()
            .filter(|n| !recent.contains(n))
            .copied()
            .collect();
        for n in remove {
            let m = self.checkpoints.remove(&n).unwrap();
            let path = root.join(format!("checkpoints/{n:08}"));
            for name in m.files.keys().map(String::as_str).chain(["manifest.json"]) {
                self.ledger.remove(&path.join(name))?;
            }
            fs::remove_dir(path)?;
        }
        let keep: BTreeSet<_> = self
            .checkpoints
            .values()
            .flat_map(|m| m.replay.keys().copied())
            .chain(buffer.references().iter().map(|r| r[0]))
            .collect();
        let remove: Vec<_> = self
            .chunks
            .keys()
            .filter(|n| !keep.contains(n))
            .copied()
            .collect();
        for n in remove {
            let a = self.chunks.remove(&n).unwrap();
            self.ledger.remove(&root.join(a.path))?;
        }
        // Each checkpoint owns its actual starts; completed generation slots no longer need duplicate files.
        let current = root.join(format!("inputs/{:08}", state.wave));
        let remove: Vec<_> = self
            .ledger
            .files
            .keys()
            .filter(|p| p.starts_with(root.join("inputs")) && !p.starts_with(&current))
            .cloned()
            .collect();
        let parents: BTreeSet<_> = remove
            .iter()
            .filter_map(|p| p.parent().map(Path::to_path_buf))
            .collect();
        for path in remove {
            self.ledger.remove(&path)?;
        }
        for path in parents {
            let _ = fs::remove_dir(&path);
            if let Some(parent) = path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
        Ok(())
    }
}

pub fn restore(
    root: &Path,
    path: &Path,
    binding: &str,
    config: &Config,
    device: Device,
) -> Result<(Store, State, Buffer, Learner, Vec<String>, Vec<Value>)> {
    ensure!(
        path.parent() == Some(root.join("checkpoints").as_path()),
        "aic-learning.resume-location: resume checkpoint must belong to this output"
    );
    let manifest = read_manifest(&path.join("manifest.json"))?;
    ensure!(
        manifest.binding == binding && manifest.device == device_name(device)?,
        "aic-learning.checkpoint-binding: {}",
        path.display()
    );
    for (name, hash) in &manifest.files {
        ensure!(
            [
                "state.json",
                "replay.json",
                "sessions.json",
                "learner.safetensors",
                "best.safetensors"
            ]
            .contains(&name.as_str()),
            "aic-learning.checkpoint-file: unexpected {name}"
        );
        verify(&path.join(name), hash, config.resources.max_output_bytes)?;
    }
    ensure!(
        ["state.json", "replay.json", "learner.safetensors"]
            .iter()
            .all(|n| manifest.files.contains_key(*n)),
        "aic-learning.checkpoint-files: missing required state"
    );
    let state: State = read(&path.join("state.json"), config.resources.max_output_bytes)?;
    ensure!(
        state.best_evaluation.is_some() == manifest.files.contains_key("best.safetensors"),
        "aic-learning.checkpoint-best: model/score presence differs"
    );
    state.validate(config)?;
    let replay: ReplayState = read(&path.join("replay.json"), config.resources.max_output_bytes)?;
    let mut buffer = Buffer {
        labels: replay.entries.values().map(|e| e.labels.len()).sum(),
        state: replay,
        ..Buffer::default()
    };
    let needed = buffer.references();
    for (ordinal, archive) in &manifest.replay {
        ensure!(
            archive.path == PathBuf::from(format!("replay/{ordinal:08}.bin")),
            "aic-learning.replay-path: noncanonical chunk"
        );
        let file = root.join(&archive.path);
        verify(
            &file,
            &archive.sha256,
            config.success.max_bytes.saturating_mul(4),
        )?;
        let bytes = fs::read(&file)?;
        let rows: Vec<(Reference, Frozen)> = postcard::from_bytes(&bytes)?;
        for (reference, input) in rows {
            ensure!(
                reference[0] == *ordinal,
                "aic-learning.replay-owner: inconsistent chunk"
            );
            if needed.contains(&reference) {
                ensure!(
                    buffer.inputs.insert(reference, Arc::new(input)).is_none(),
                    "aic-learning.replay-owner: duplicate reference"
                );
            }
        }
    }
    ensure!(
        buffer.inputs.len() == needed.len()
            && buffer.labels <= config.success.max_labels
            && buffer.state.entries.len() <= config.success.max_sequences,
        "aic-learning.replay-restore: missing input or exceeded capacity"
    );
    buffer.prune_inputs();
    ensure!(
        buffer.bytes <= config.success.max_bytes,
        "aic-learning.replay-restore: byte capacity"
    );
    let tensors = Tensor::read_safetensors(path.join("learner.safetensors"))?
        .into_iter()
        .collect();
    let learner = Learner::load(
        &config.model,
        &config.optimizer,
        &manifest.parameter_order,
        &tensors,
        device,
        manifest.completed_steps,
    )?;
    let sessions: Vec<Value> = if manifest.files.contains_key("sessions.json") {
        read(
            &path.join("sessions.json"),
            config.resources.max_output_bytes,
        )?
    } else {
        vec![]
    };
    ensure!(
        (state.next.is_some() && sessions.is_empty())
            || (sessions.len() == config.streams
                && sessions.iter().enumerate().all(|(i, s)| s["stream"] == i)),
        "aic-learning.resume-ownership: incomplete native sessions"
    );
    preserve_uncommitted(root, state.update)?;
    let mut store = Store {
        best_model: if manifest.files.contains_key("best.safetensors") {
            Some(
                Tensor::read_safetensors(path.join("best.safetensors"))?
                    .into_iter()
                    .collect(),
            )
        } else {
            None
        },
        chunks: manifest.replay.clone(),
        ledger: Ledger::scan(root)?,
        ..Store::default()
    };
    // Retention discovers only small checkpoint manifests, never historical trajectories.
    for entry in fs::read_dir(root.join("checkpoints"))? {
        let entry = entry?;
        if let Ok(n) = entry.file_name().to_string_lossy().parse::<u64>() {
            ensure!(
                n <= state.update,
                "aic-learning.resume-stale: newer checkpoint exists; use latest.json"
            );
            let m = read_manifest(&entry.path().join("manifest.json"))?;
            for (n, a) in &m.replay {
                store.chunks.entry(*n).or_insert_with(|| a.clone());
            }
            store.checkpoints.insert(n, m);
        }
    }
    store.ledger.enforce(config.resources.max_output_bytes)?;
    Ok((
        store,
        state,
        buffer,
        learner,
        manifest.parameter_order,
        sessions,
    ))
}
fn verify(path: &Path, hash: &str, limit: u64) -> Result<()> {
    ensure!(
        fs::metadata(path)?.len() <= limit && sha256(path)? == hash,
        "aic-learning.checkpoint-hash: {}",
        path.display()
    );
    Ok(())
}

/// Recovery rolls back to the selected boundary without overwriting actual
/// later work. Only the few output directories are inspected, once on resume.
fn preserve_uncommitted(root: &Path, update: u64) -> Result<()> {
    let destination = root.join(format!("interrupted-{}", std::process::id()));
    for directory in ["updates", "evaluations", "replay", "checkpoints"] {
        for entry in fs::read_dir(root.join(directory))? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let ordinal = name.split('.').next().unwrap_or("").parse::<u64>().ok();
            if name.ends_with(".pending") || ordinal.is_some_and(|n| n > update) {
                fs::create_dir_all(destination.join(directory))?;
                fs::rename(
                    entry.path(),
                    destination.join(directory).join(name.as_ref()),
                )?;
            }
        }
    }
    // An interrupted constructor may have irreplaceable partial geometry. Keep
    // it outside the normal input retention tree before retrying that slot.
    for wave in fs::read_dir(root.join("inputs"))? {
        let wave = wave?;
        if !wave.file_type()?.is_dir() {
            continue;
        }
        for slot in fs::read_dir(wave.path())? {
            let slot = slot?;
            if slot.file_type()?.is_dir() && !slot.path().join("manifest.json").exists() {
                let target = destination.join("inputs").join(wave.file_name());
                fs::create_dir_all(&target)?;
                fs::rename(slot.path(), target.join(slot.file_name()))?;
            }
        }
    }
    Ok(())
}
pub fn read<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T> {
    ensure!(
        fs::metadata(path)?.len() <= limit,
        "aic-learning.input-size: {}",
        path.display()
    );
    serde_json::from_reader(std::io::BufReader::new(fs::File::open(path)?))
        .map_err(|e| anyhow::anyhow!("aic-learning.input-json: {}: {e}", path.display()))
}

/// One bounded header read, shared by restore and retention discovery.
fn read_manifest(path: &Path) -> Result<Manifest> {
    let header: Value = read(path, 4 * 1024 * 1024)?;
    ensure!(
        header["schema"] == CHECKPOINT_SCHEMA,
        "aic-learning.checkpoint-version: {}: expected {}; actual {}",
        path.display(),
        CHECKPOINT_SCHEMA,
        header["schema"]
    );
    Ok(serde_json::from_value(header)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recovery_preserves_later_outputs_and_interrupted_geometry() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("aic-recovery-preservation-{}", std::process::id()));
        fs::create_dir(&root)?;
        for name in [
            "updates",
            "evaluations",
            "replay",
            "checkpoints",
            "inputs/00000001/0000",
            "inputs/00000001/0001",
        ] {
            fs::create_dir_all(root.join(name))?;
        }
        for (name, bytes) in [
            ("updates/00000002.json", "committed"),
            ("updates/00000003.json", "later-update"),
            ("replay/00000003.bin", "later-replay"),
            ("inputs/00000001/0000/setup-partial.json", "actual-partial"),
            ("inputs/00000001/0001/manifest.json", "actual-complete"),
        ] {
            fs::write(root.join(name), bytes)?;
        }
        fs::create_dir(root.join("checkpoints/00000003.pending"))?;
        preserve_uncommitted(&root, 2)?;
        let saved = root.join(format!("interrupted-{}", std::process::id()));
        assert_eq!(
            fs::read_to_string(saved.join("updates/00000003.json"))?,
            "later-update"
        );
        assert_eq!(
            fs::read_to_string(saved.join("inputs/00000001/0000/setup-partial.json"))?,
            "actual-partial"
        );
        assert_eq!(
            fs::read_to_string(root.join("updates/00000002.json"))?,
            "committed"
        );
        assert!(root.join("inputs/00000001/0001/manifest.json").exists());
        assert!(!root.join("replay/00000003.bin").exists());
        assert!(saved.join("checkpoints/00000003.pending").exists());
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn restore_rejects_old_versions_before_touching_missing_payloads() -> Result<()> {
        let root = std::env::temp_dir().join(format!("aic-contract-{}", std::process::id()));
        let path = root.join("checkpoints/00000001");
        fs::create_dir_all(&path)?;
        let config: Config =
            serde_json::from_str(include_str!("../../../../tools/native-training/smoke.json"))?;
        let valid = serde_json::json!({"schema":CHECKPOINT_SCHEMA,
            "binding":"test", "device":"cpu", "completed_steps":1, "parameter_order":[], "files":{}, "replay":{}});
        for version in [
            None,
            Some("aic-training-checkpoint-v1"),
            Some("aic-training-checkpoint-v2"),
        ] {
            let mut value = valid.clone();
            if let Some(version) = version {
                value["schema"] = version.into();
                // Obsolete fields must not hide the version diagnostic.
                value["contract"] = serde_json::json!({"observation":"historical"});
            } else {
                value.as_object_mut().unwrap().remove("schema");
            }
            write(&path.join("manifest.json"), &value)?;
            let error = restore(&root, &path, "test", &config, Device::Cpu)
                .err()
                .expect("must reject");
            let text = error.to_string();
            assert!(
                text.starts_with("aic-learning.checkpoint-version:"),
                "{text}"
            );
            assert!(text.contains(&path.display().to_string()));
            assert!(!root.join("latest.json").exists());
        }
        write(&path.join("manifest.json"), &valid)?;
        let error = restore(&root, &path, "test", &config, Device::Cpu)
            .err()
            .expect("missing payloads");
        assert!(
            error
                .to_string()
                .starts_with("aic-learning.checkpoint-files:")
        );
        fs::remove_dir_all(root)?;
        Ok(())
    }
}
