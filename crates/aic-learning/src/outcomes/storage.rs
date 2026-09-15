use super::{config::RootSlot, sample::SelectedAction};
use aic_pilot_native::physical_editor::{
    EditWork, PhysicalEditorDiagnostic, PhysicalSessionOutcome, RouteWork, learning::Input,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

pub const DATASET_SCHEMA: &str = "aic-physical-outcome-dataset-v1";
pub const SPLIT_SCHEMA: &str = "aic-physical-outcome-splits-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticRecord {
    pub code: String,
    pub severity: String,
    pub stage: String,
    pub entity: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkRecord {
    pub expanded: usize,
    pub preflight_expanded: usize,
    pub route_calls: usize,
    pub pending: usize,
    pub routes: Vec<RouteRecord>,
    pub committed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RouteRecord {
    pub edge: usize,
    pub status: String,
    pub expanded: usize,
    pub preflight_expanded: usize,
    pub preflight_status: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedOutcome {
    pub status: String,
    pub charged: bool,
    pub censored: bool,
    pub attempts: u64,
    pub revision: u64,
    pub done: bool,
    pub work: WorkRecord,
    pub best_metrics: aic_layout::physical::Metrics,
    pub current_pending: Option<usize>,
    pub current_conflicts: Option<usize>,
    pub current_metrics: Option<aic_layout::physical::Metrics>,
    pub route_budget_exhausted: Option<bool>,
    pub diagnostics: Vec<DiagnosticRecord>,
}

impl From<PhysicalSessionOutcome> for ObservedOutcome {
    fn from(value: PhysicalSessionOutcome) -> Self {
        let censored = value.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "physical-editor.elapsed"
                || diagnostic.code == "physical-editor.budget-exhausted"
                || diagnostic.stage == "compiled-boundary"
                || diagnostic.code == "physical-editor.export-validation"
        });
        let observed =
            matches!(value.status, "draft_accepted" | "validated" | "improved") && !censored;
        let route_budget_exhausted = observed.then(|| {
            value.work.expanded >= 50_000
                || value
                    .work
                    .routes
                    .iter()
                    .any(|route| route.status == "work-budget")
        });
        Self {
            status: value.status.into(),
            charged: value.charged,
            censored,
            attempts: value.attempts,
            revision: value.revision,
            done: value.done,
            work: value.work.into(),
            best_metrics: value.best_metrics,
            current_pending: value.current_pending,
            current_conflicts: value.current_conflicts,
            current_metrics: value.current_metrics,
            route_budget_exhausted,
            diagnostics: value.diagnostics.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<PhysicalEditorDiagnostic> for DiagnosticRecord {
    fn from(value: PhysicalEditorDiagnostic) -> Self {
        Self {
            code: value.code,
            severity: value.severity.into(),
            stage: value.stage.into(),
            entity: value.entity,
            message: value.message,
        }
    }
}

impl From<EditWork> for WorkRecord {
    fn from(value: EditWork) -> Self {
        Self {
            expanded: value.expanded,
            preflight_expanded: value.preflight_expanded,
            route_calls: value.route_calls,
            pending: value.pending,
            routes: value.routes.into_iter().map(Into::into).collect(),
            committed: value.committed,
        }
    }
}

impl From<RouteWork> for RouteRecord {
    fn from(value: RouteWork) -> Self {
        Self {
            edge: value.edge,
            status: value.status,
            expanded: value.expanded,
            preflight_expanded: value.preflight_expanded,
            preflight_status: value.preflight_status,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub root_id: String,
    pub step: u64,
    pub pre_action: Input,
    pub selected: SelectedAction,
    pub outcome: ObservedOutcome,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RootManifest {
    pub slot: RootSlot,
    pub status: String,
    pub source_file: Option<String>,
    pub source_sha256: Option<String>,
    pub structural_identity: Option<String>,
    pub exact_identity: Option<String>,
    pub draft_sha256: Option<String>,
    pub identity_work: Option<u64>,
    pub diagnostic: Option<serde_json::Value>,
    pub rows: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkManifest {
    pub root_id: String,
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub rows: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SplitManifest {
    pub schema: String,
    pub config_sha256: String,
    pub catalog_sha256: String,
    pub roots: Vec<RootManifest>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetManifest {
    pub schema: String,
    pub status: String,
    pub config_sha256: String,
    pub config_file: String,
    pub catalog_sha256: String,
    pub elapsed_ms: u64,
    pub configured_roots: usize,
    pub admitted_roots: usize,
    pub collected_roots: usize,
    pub rows: usize,
    pub output_bytes: u64,
    pub roots: Vec<RootManifest>,
    pub chunks: Vec<ChunkManifest>,
    pub coverage: BTreeMap<String, u64>,
    pub failure: Option<String>,
}

/// Dataset bindings verified once, with rows decoded one root chunk at a time.
pub struct VerifiedIndex {
    root: PathBuf,
    pub manifest: DatasetManifest,
    pub split: SplitManifest,
    chunks: BTreeMap<String, ChunkManifest>,
}

impl VerifiedIndex {
    pub fn read_root(&self, root_id: &str) -> Result<Vec<Row>> {
        let chunk = self.chunks.get(root_id).ok_or_else(|| {
            anyhow::anyhow!("aic-learning.outcomes-load: missing chunk for {root_id}")
        })?;
        decode_chunk(&self.root.join(&chunk.path), chunk)
    }
}

pub struct Stage {
    output: PathBuf,
    root: PathBuf,
    max_bytes: u64,
    metadata_reserve: u64,
    data_bytes: u64,
    metadata_bytes: u64,
    published: bool,
}

impl Stage {
    pub fn new(output: &Path, max_bytes: u64, metadata_reserve: u64) -> Result<Self> {
        ensure!(
            !output.exists(),
            "aic-learning.outcomes-output: output already exists: {}",
            output.display()
        );
        let name = output
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("aic-learning.outcomes-output: invalid output path"))?;
        let root = output.with_file_name(format!(".{name}.stage-{}", std::process::id()));
        ensure!(
            !root.exists(),
            "aic-learning.outcomes-output: staging path already exists: {}",
            root.display()
        );
        fs::create_dir_all(root.join("sources"))?;
        fs::create_dir(root.join("chunks"))?;
        Ok(Self {
            output: output.to_owned(),
            root,
            max_bytes,
            metadata_reserve,
            data_bytes: 0,
            metadata_bytes: 0,
            published: false,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn data_available(&self) -> u64 {
        self.max_bytes
            .saturating_sub(self.metadata_reserve)
            .saturating_sub(self.data_bytes)
    }

    pub fn write_source(
        &mut self,
        root_id: &str,
        bytes: &[u8],
    ) -> Result<Option<(String, String)>> {
        if bytes.len() as u64 > self.data_available() {
            return Ok(None);
        }
        let relative = format!("sources/{root_id}.json");
        write_new(&self.root.join(&relative), bytes)?;
        self.data_bytes += bytes.len() as u64;
        Ok(Some((relative, hash(bytes))))
    }

    pub fn write_chunk(&mut self, root_id: &str, rows: &[Row]) -> Result<Option<ChunkManifest>> {
        let relative = format!("chunks/{root_id}.postcard");
        let path = self.root.join(&relative);
        let mut writer = BufWriter::new(
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?,
        );
        let mut digest = Sha256::new();
        let mut bytes = 0u64;
        let mut count = 0usize;
        for row in rows {
            let encoded = postcard::to_stdvec(row)?;
            let frame = 8u64.saturating_add(encoded.len() as u64);
            if frame > self.data_available().saturating_sub(bytes) {
                break;
            }
            let length = (encoded.len() as u64).to_le_bytes();
            writer.write_all(&length)?;
            writer.write_all(&encoded)?;
            digest.update(length);
            digest.update(&encoded);
            bytes += frame;
            count += 1;
        }
        writer.flush()?;
        drop(writer);
        if count == 0 {
            fs::remove_file(path)?;
            return Ok(None);
        }
        self.data_bytes += bytes;
        Ok(Some(ChunkManifest {
            root_id: root_id.into(),
            path: relative,
            sha256: hex(&digest.finalize()),
            bytes,
            rows: count,
        }))
    }

    pub fn write_split(&mut self, split: &SplitManifest) -> Result<()> {
        self.write_metadata("split-manifest.json", split)
    }

    pub fn write_config(&mut self, bytes: &[u8]) -> Result<()> {
        self.write_metadata_bytes("config.json", bytes)
    }

    pub fn publish(
        mut self,
        manifest: &mut DatasetManifest,
        report: &serde_json::Value,
    ) -> Result<PathBuf> {
        let report_bytes = pretty(report)?;
        let manifest_bytes = loop {
            let bytes = pretty(manifest)?;
            let total = self.data_bytes
                + self.metadata_bytes
                + bytes.len() as u64
                + report_bytes.len() as u64;
            if manifest.output_bytes == total {
                break bytes;
            }
            manifest.output_bytes = total;
        };
        let added = manifest_bytes.len() as u64 + report_bytes.len() as u64;
        ensure!(
            self.metadata_bytes + added <= self.metadata_reserve,
            "aic-learning.outcomes-output: final metadata exceeds reserved bytes"
        );
        ensure!(
            self.data_bytes + self.metadata_bytes + added <= self.max_bytes,
            "aic-learning.outcomes-output: aggregate output exceeds configured ceiling"
        );
        write_new(&self.root.join("manifest.json"), &manifest_bytes)?;
        write_new(&self.root.join("report.json"), &report_bytes)?;
        fs::rename(&self.root, &self.output)
            .with_context(|| format!("publish {}", self.output.display()))?;
        self.published = true;
        Ok(self.output.clone())
    }

    fn write_metadata(&mut self, name: &str, value: &impl Serialize) -> Result<()> {
        let bytes = pretty(value)?;
        self.write_metadata_bytes(name, &bytes)
    }

    fn write_metadata_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        ensure!(
            self.metadata_bytes + bytes.len() as u64 <= self.metadata_reserve,
            "aic-learning.outcomes-output: split metadata exceeds reserved bytes"
        );
        write_new(&self.root.join(name), bytes)?;
        self.metadata_bytes += bytes.len() as u64;
        Ok(())
    }
}

impl Drop for Stage {
    fn drop(&mut self) {
        // A failed run intentionally leaves its create-new stage as cold evidence.
        let _ = self.published;
    }
}

pub fn verified_index(path: &Path) -> Result<VerifiedIndex> {
    let manifest: DatasetManifest = read_json(&path.join("manifest.json"))?;
    ensure!(
        manifest.schema == DATASET_SCHEMA,
        "aic-learning.outcomes-load: unsupported dataset schema"
    );
    let config = fs::read(path.join(&manifest.config_file))?;
    ensure!(
        hash(&config) == manifest.config_sha256,
        "aic-learning.outcomes-load: config hash mismatch"
    );
    let split: SplitManifest = read_json(&path.join("split-manifest.json"))?;
    ensure!(
        split.schema == SPLIT_SCHEMA
            && split.config_sha256 == manifest.config_sha256
            && split.catalog_sha256 == manifest.catalog_sha256,
        "aic-learning.outcomes-load: split manifest binding mismatch"
    );
    for root in &split.roots {
        let final_root = manifest
            .roots
            .iter()
            .find(|candidate| candidate.slot.root_id == root.slot.root_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "aic-learning.outcomes-load: split root missing from final manifest"
                )
            })?;
        ensure!(
            final_root.slot == root.slot
                && final_root.source_file == root.source_file
                && final_root.source_sha256 == root.source_sha256
                && final_root.structural_identity == root.structural_identity
                && final_root.exact_identity == root.exact_identity
                && final_root.draft_sha256 == root.draft_sha256,
            "aic-learning.outcomes-load: root admission binding mismatch for {}",
            root.slot.root_id
        );
        if let (Some(relative), Some(expected)) = (&root.source_file, &root.source_sha256) {
            ensure!(
                hash_file(&path.join(relative))? == *expected,
                "aic-learning.outcomes-load: source hash mismatch for {}",
                root.slot.root_id
            );
        }
    }
    let mut chunks = BTreeMap::new();
    for chunk in &manifest.chunks {
        ensure!(
            fs::metadata(path.join(&chunk.path))?.len() == chunk.bytes
                && hash_file(&path.join(&chunk.path))? == chunk.sha256,
            "aic-learning.outcomes-load: chunk binding failed for {}",
            chunk.path
        );
        ensure!(
            chunks
                .insert(chunk.root_id.clone(), chunk.clone())
                .is_none(),
            "aic-learning.outcomes-load: duplicate chunk for {}",
            chunk.root_id
        );
    }
    ensure!(
        manifest
            .roots
            .iter()
            .filter(|root| root.rows > 0)
            .all(|root| chunks.contains_key(&root.slot.root_id)),
        "aic-learning.outcomes-load: root manifest lacks chunk binding"
    );
    Ok(VerifiedIndex {
        root: path.to_owned(),
        manifest,
        split,
        chunks,
    })
}

pub fn load(path: &Path) -> Result<(DatasetManifest, Vec<Row>)> {
    let index = verified_index(path)?;
    let mut rows = Vec::new();
    for chunk in &index.manifest.chunks {
        rows.extend(index.read_root(&chunk.root_id)?);
    }
    ensure!(
        rows.len() == index.manifest.rows,
        "aic-learning.outcomes-load: manifest row total mismatch"
    );
    Ok((index.manifest, rows))
}

fn decode_chunk(path: &Path, chunk: &ChunkManifest) -> Result<Vec<Row>> {
    let bytes = fs::read(path)?;
    let mut cursor = std::io::Cursor::new(&bytes);
    let mut rows = Vec::with_capacity(chunk.rows);
    while cursor.position() < bytes.len() as u64 {
        let mut length = [0u8; 8];
        cursor.read_exact(&mut length)?;
        let length = usize::try_from(u64::from_le_bytes(length))?;
        ensure!(
            length <= bytes.len().saturating_sub(cursor.position() as usize),
            "aic-learning.outcomes-load: truncated frame in {}",
            chunk.path
        );
        let mut encoded = vec![0; length];
        cursor.read_exact(&mut encoded)?;
        let row: Row = postcard::from_bytes(&encoded)?;
        ensure!(
            row.root_id == chunk.root_id,
            "aic-learning.outcomes-load: row owner mismatch in {}",
            chunk.path
        );
        rows.push(row);
    }
    ensure!(
        rows.len() == chunk.rows,
        "aic-learning.outcomes-load: row count mismatch in {}",
        chunk.path
    );
    Ok(rows)
}

fn pretty(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_reader(BufReader::new(File::open(path)?))
        .with_context(|| format!("parse {}", path.display()))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = BufWriter::new(OpenOptions::new().write(true).create_new(true).open(path)?);
    file.write_all(bytes)?;
    file.flush()?;
    Ok(())
}

pub fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = BufReader::new(File::open(path)?);
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex(&digest.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
