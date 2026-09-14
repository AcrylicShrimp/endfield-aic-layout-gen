mod validation;
// Cold diagnostic interchange. Validation runs once; updates reuse tensors.
use crate::schema::{INPUT_SCHEMA, InputManifest};
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
        ensure!(
            roots.kind() == Kind::Int64
                && roots.size() == [graphs]
                && selected.kind() == Kind::Int64
                && selected.size() == [factors],
            "aic-learning.input-labels: {prefix}"
        );
        ensure!(
            tensor(&graph, "global_features")?.size()[0] == graphs,
            "aic-learning.input-graphs: {prefix}"
        );
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

pub struct Input {
    pub manifest: InputManifest,
    pub identity: String,
    pub tensors: Tensors,
    pub ppo: Packed,
    pub sil: Packed,
    pub targets: Tensors,
}
impl Input {
    pub fn load(path: &Path, device: Device) -> Result<Self> {
        ensure!(
            fs::metadata(path)?.len() <= 1024 * 1024,
            "aic-learning.manifest-size: {}",
            path.display()
        );
        let bytes =
            fs::read(path).with_context(|| format!("aic-learning.manifest: {}", path.display()))?;
        ensure!(
            bytes.len() <= 1024 * 1024,
            "aic-learning.manifest-size: {}",
            path.display()
        );
        let manifest: InputManifest = serde_json::from_slice(&bytes)?;
        ensure!(
            manifest.schema == INPUT_SCHEMA,
            "aic-learning.input-schema: {}",
            manifest.schema
        );
        manifest.update.validate()?;
        ensure!(
            manifest.max_tensor_bytes > 0 && manifest.max_tensor_bytes <= 2 * 1024 * 1024 * 1024,
            "aic-learning.input-budget: invalid tensor byte bound"
        );
        let relative = Path::new(&manifest.tensors);
        ensure!(
            relative.components().count() == 1 && relative.file_name().is_some(),
            "aic-learning.input-path: expected a sibling tensor file"
        );
        let archive = path.parent().unwrap_or(Path::new(".")).join(relative);
        ensure!(
            fs::metadata(&archive)?.len() <= manifest.max_tensor_bytes,
            "aic-learning.input-budget: {}",
            archive.display()
        );
        ensure!(
            sha256(&archive)? == manifest.tensors_sha256,
            "aic-learning.input-hash: {}",
            archive.display()
        );
        let rows = Tensor::read_safetensors(&archive)
            .with_context(|| format!("aic-learning.input-archive: {}", archive.display()))?;
        let length = rows.len();
        let tensors: Tensors = rows.into_iter().collect();
        ensure!(
            tensors.len() == length,
            "aic-learning.input-duplicate: tensor names"
        );
        let decoded_bytes: usize = tensors
            .values()
            .map(|t| t.numel() * t.kind().elt_size_in_bytes())
            .sum();
        ensure!(
            decoded_bytes as u64 <= manifest.max_tensor_bytes,
            "aic-learning.input-budget: decoded tensors"
        );
        let ppo = Packed::load(
            &tensors,
            "ppo",
            manifest.ppo_graphs,
            manifest.ppo_factors,
            device,
            &manifest.model,
        )?;
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
            ("ppo.old_logp", manifest.ppo_graphs),
            ("ppo.advantages", manifest.ppo_graphs),
            ("ppo.targets", manifest.ppo_graphs),
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
            tensors,
            ppo,
            sil,
            targets,
        })
    }
}
