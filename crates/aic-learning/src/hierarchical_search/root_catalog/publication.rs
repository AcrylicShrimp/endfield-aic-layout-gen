//! One immutable prior sidecar and its versioned, source-bound root descriptor.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    CatalogStop, CatalogVisit, FrozenTargetScalars, LEDGER_ROW_BYTES, NonplacementWitness,
    collector::InitialRootCatalog, search::RootCatalogSimulation,
};
use crate::hierarchical_search::{
    MacroAction, Report, RootRecordTarget, RootShortlistEvidence, SearchMemoryBudget,
    TreeReservation, physical_action_identity, publish_json_identity, root_scoring::RootNoiseMode,
};

pub(super) const DESCRIPTOR_SCHEMA: &str = "aic-hierarchical-root-catalog-v1";
#[cfg(test)]
pub(super) const REPORT_SCHEMA: &str = "aic-hierarchical-catalog-report-v1";
const ACTION_FEATURE_SCHEMA: &str = "aic-hierarchical-action-features-v1";

/// The caller computes these once from the complete effective run inputs.
/// In particular, `limits_sha256` is the versioned per-case native Limits,
/// which is not included in the source observation hash.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CatalogBinding {
    pub(crate) source_sha256: String,
    pub(crate) source_revision: u64,
    pub(crate) limits_sha256: String,
    pub(crate) effective_config_sha256: String,
    pub(crate) catalog_sha256: String,
    pub(crate) outer_checkpoint_sha256: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RootCatalogDescriptor {
    pub(super) schema: String,
    pub(super) case_id: String,
    pub(super) action_feature_schema: String,
    pub(super) source_order_version: u32,
    pub(super) binding: CatalogBinding,
    pub(super) sidecar_name: String,
    pub(super) sidecar_bytes: u64,
    pub(super) sidecar_sha256: String,
    pub(super) initial_eligible_count: u64,
    pub(super) placement_count: u64,
    pub(super) noise_mode: RootNoiseMode,
    pub(super) selected_action: MacroAction,
    pub(super) selected_physical_id: Vec<u8>,
    pub(super) selected_rank: u64,
    pub(super) selected_ordinal: u64,
    pub(super) shortlist: Vec<RootShortlistEvidence>,
    pub(super) visits: Vec<CatalogVisit>,
    pub(super) target: FrozenTargetScalars,
    pub(super) initial_root_value: f64,
    pub(super) simulations: usize,
    pub(super) nonplacement_witnesses: Vec<NonplacementWitness>,
    pub(super) pending_lower_tasks: usize,
    pub(super) loaded_lower_pages: usize,
}

impl std::fmt::Debug for RootCatalogDescriptor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RootCatalogDescriptor")
            .field("schema", &self.schema)
            .field("case_id", &self.case_id)
            .field("sidecar_name", &self.sidecar_name)
            .field("initial_eligible_count", &self.initial_eligible_count)
            .field("placement_count", &self.placement_count)
            .field("nonplacement_witnesses", &self.nonplacement_witnesses.len())
            .finish_non_exhaustive()
    }
}

pub(crate) struct LowerCatalogWitness<'a> {
    pub(crate) action: &'a MacroAction,
    pub(crate) visits: u32,
    pub(crate) q: f64,
}

impl RootCatalogDescriptor {
    pub(crate) fn case_id(&self) -> &str {
        &self.case_id
    }

    pub(crate) fn binding(&self) -> &CatalogBinding {
        &self.binding
    }

    pub(crate) fn selected_action(&self) -> &MacroAction {
        &self.selected_action
    }

    pub(crate) fn policy_action_count(&self) -> usize {
        self.initial_eligible_count as usize
            + self
                .nonplacement_witnesses
                .iter()
                .filter(|witness| witness.initial_ordinal.is_none())
                .count()
    }

    pub(crate) fn lower_witnesses(&self) -> impl Iterator<Item = LowerCatalogWitness<'_>> {
        self.nonplacement_witnesses.iter().map(|witness| {
            let visit = witness
                .initial_ordinal
                .and_then(|ordinal| self.visits.iter().find(|visit| visit.ordinal == ordinal));
            LowerCatalogWitness {
                action: &witness.action,
                visits: visit.map_or(0, |visit| visit.visits),
                q: visit.map_or(0.0, |visit| visit.value_sum / f64::from(visit.visits)),
            }
        })
    }
}

pub(in crate::hierarchical_search) struct StagedRootCatalog {
    pub(in crate::hierarchical_search) descriptor: Arc<RootCatalogDescriptor>,
    sidecar_path: PathBuf,
    report_parent: PathBuf,
    committed: bool,
    _witness_memory: TreeReservation,
    _simulation_edges_memory: TreeReservation,
    _simulation_choice_memory: TreeReservation,
}

#[cfg(test)]
#[derive(Serialize)]
struct CatalogReportEnvelope<'a, T> {
    schema: &'static str,
    report: &'a T,
    root_catalogs: Vec<&'a RootCatalogDescriptor>,
}

#[derive(Deserialize)]
struct RecoveryReport {
    schema: String,
    cases: Vec<RecoveryCase>,
}

#[derive(Deserialize)]
struct RecoveryCase {
    mcts: RecoverySearch,
}

#[derive(Deserialize)]
struct RecoverySearch {
    #[serde(default)]
    root_records: Vec<RecoveryRoot>,
}

#[derive(Deserialize)]
struct RecoveryRoot {
    target: RecoveryTarget,
}

#[derive(Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "kebab-case")]
enum RecoveryTarget {
    PuctDense(serde::de::IgnoredAny),
    GumbelCatalog { descriptor: RecoveryDescriptor },
}

#[derive(Deserialize)]
struct RecoveryDescriptor {
    sidecar_name: String,
}

impl StagedRootCatalog {
    /// The source-order ledger is verified before it is renamed into the
    /// report directory. Dropping an uncommitted stage removes that sidecar.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        catalog: &mut Option<InitialRootCatalog<'_>>,
        simulation: &mut Option<RootCatalogSimulation>,
        binding: &CatalogBinding,
        output_report: &Path,
        case_id: &str,
        value_scale: f64,
        max_visit_init: f64,
        anchor_policy_mass: f64,
        budget: &SearchMemoryBudget,
    ) -> Result<Result<Self, CatalogStop>> {
        let catalog_ref = catalog.as_ref().expect("prepared root catalog is retained");
        let simulation_ref = simulation
            .as_ref()
            .expect("completed root search is retained");
        ensure!(
            binding.source_sha256 == catalog_ref.state_identity
                && binding.source_revision == catalog_ref.source.revision(),
            "aic-learning.root-catalog-binding: source identity or revision differs"
        );
        simulation_ref.selected.as_ref().ok_or_else(|| {
            anyhow::anyhow!("aic-learning.root-catalog-publication: no selected action")
        })?;
        simulation_ref.selected_id.as_ref().ok_or_else(|| {
            anyhow::anyhow!("aic-learning.root-catalog-publication: missing selected ID")
        })?;
        simulation_ref.selected_rank.ok_or_else(|| {
            anyhow::anyhow!("aic-learning.root-catalog-publication: missing selected rank")
        })?;
        simulation_ref.selected_ordinal.ok_or_else(|| {
            anyhow::anyhow!("aic-learning.root-catalog-publication: missing selected ordinal")
        })?;
        let target = match catalog_ref.files.target_scalars(
            catalog_ref.initial_root_value,
            &simulation_ref.visits,
            value_scale,
            max_visit_init,
            anchor_policy_mass,
            budget,
        ) {
            Ok(target) => target,
            Err(stop) => return Ok(Err(stop)),
        };
        let report_parent = output_report
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_owned();
        ensure!(
            catalog_ref.files.ledger_path.parent() == Some(report_parent.as_path()),
            "aic-learning.root-catalog-publication: ledger and report must share a directory"
        );
        let expected_bytes = catalog_ref
            .files
            .count
            .checked_mul(LEDGER_ROW_BYTES as u64)
            .ok_or_else(|| {
                anyhow::anyhow!("aic-learning.root-catalog-publication: ledger length overflow")
            })?;
        let (digest, bytes) = match file_digest_and_sync(&catalog_ref.files.ledger_path) {
            Ok(value) => value,
            Err(source) => {
                return Ok(Err(CatalogStop::Io {
                    path: catalog_ref.files.ledger_path.clone(),
                    operation: "verify",
                    source,
                }));
            }
        };
        ensure!(
            bytes == expected_bytes,
            "aic-learning.root-catalog-publication: ledger has {bytes} bytes; expected {expected_bytes}"
        );
        let report_name = output_report
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                anyhow::anyhow!("aic-learning.root-catalog-publication: report name is not UTF-8")
            })?;
        let case_digest = hex_digest(case_id.as_bytes());
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let sidecar_name = format!(
            ".{report_name}.root-{}-{}-{nonce}.priors",
            &case_digest[..16],
            binding.source_revision
        );
        let sidecar_path = report_parent.join(&sidecar_name);
        ensure!(
            !sidecar_path.exists(),
            "aic-learning.root-catalog-publication: sidecar already exists"
        );
        if let Err(source) = fs::rename(&catalog_ref.files.ledger_path, &sidecar_path) {
            return Ok(Err(CatalogStop::Io {
                path: catalog_ref.files.ledger_path.clone(),
                operation: "rename",
                source,
            }));
        }
        if let Err(source) = File::open(&report_parent).and_then(|directory| directory.sync_all()) {
            fs::rename(&sidecar_path, &catalog_ref.files.ledger_path).map_err(|rollback| {
                anyhow::anyhow!(
                    "aic-learning.root-catalog-publication: directory sync failed ({source}); sidecar rollback also failed ({rollback})"
                )
            })?;
            return Ok(Err(CatalogStop::Io {
                path: report_parent,
                operation: "sync",
                source,
            }));
        }
        let catalog = catalog.take().expect("prepared root catalog is retained");
        let simulation = simulation
            .take()
            .expect("completed root search is retained");
        let selected_action = simulation
            .selected
            .expect("selected action checked before rename");
        let selected_physical_id = simulation
            .selected_id
            .expect("selected ID checked before rename");
        let selected_rank = simulation
            .selected_rank
            .expect("selected rank checked before rename");
        let selected_ordinal = simulation
            .selected_ordinal
            .expect("selected ordinal checked before rename");
        let InitialRootCatalog {
            mut files,
            witnesses,
            lower,
            lower_loaded_pages,
            placement_count,
            initial_root_value,
            ..
        } = catalog;
        files.ledger_path.clear();
        let (nonplacement_witnesses, witness_memory) = witnesses.into_frozen();
        let descriptor = Arc::new(RootCatalogDescriptor {
            schema: DESCRIPTOR_SCHEMA.into(),
            case_id: case_id.into(),
            action_feature_schema: ACTION_FEATURE_SCHEMA.into(),
            source_order_version: 1,
            binding: binding.clone(),
            sidecar_name,
            sidecar_bytes: bytes,
            sidecar_sha256: digest,
            initial_eligible_count: files.count,
            placement_count,
            noise_mode: files.mode,
            selected_action,
            selected_physical_id,
            selected_rank,
            selected_ordinal,
            shortlist: simulation.shortlist,
            visits: simulation.visits,
            target,
            initial_root_value,
            simulations: simulation.simulations,
            nonplacement_witnesses,
            pending_lower_tasks: lower.pending(),
            loaded_lower_pages: lower_loaded_pages,
        });
        Ok(Ok(Self {
            descriptor,
            sidecar_path,
            report_parent,
            committed: false,
            _witness_memory: witness_memory,
            _simulation_edges_memory: simulation._edges_memory,
            _simulation_choice_memory: simulation._choice_memory,
        }))
    }
}

impl Drop for StagedRootCatalog {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.sidecar_path);
        }
    }
}

/// Commit the report only after all referenced sidecars are staged. The
/// existing report publisher writes a temporary JSON file and renames it last.
#[cfg(test)]
pub(in crate::hierarchical_search) fn publish_with_catalogs<T: Serialize>(
    output: &Path,
    report: &T,
    stages: &mut [StagedRootCatalog],
) -> Result<Result<(String, u64), CatalogStop>> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    ensure!(
        stages.iter().all(|stage| stage.report_parent == parent),
        "aic-learning.root-catalog-publication: report directory differs from staged sidecar"
    );
    ensure!(
        !stages.is_empty(),
        "aic-learning.root-catalog-publication: no catalog descriptors"
    );
    let envelope = CatalogReportEnvelope {
        schema: REPORT_SCHEMA,
        report,
        root_catalogs: stages
            .iter()
            .map(|stage| stage.descriptor.as_ref())
            .collect(),
    };
    let published = match publish_json_identity(output, &envelope, None) {
        Ok(published) => published,
        Err(error) => {
            if let Some(stop) = report_disk_stop(output, &error) {
                return Ok(Err(stop));
            }
            return Err(error);
        }
    };
    for stage in stages {
        stage.committed = true;
    }
    Ok(Ok(published))
}

/// Commit the canonical search report, whose Gumbel records own the exact
/// descriptors. Publication requires a one-to-one match with staged sidecars.
pub(in crate::hierarchical_search) fn publish_embedded_report(
    output: &Path,
    report: &Report,
    stages: &mut [StagedRootCatalog],
) -> Result<Result<(String, u64), CatalogStop>> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    ensure!(
        stages.iter().all(|stage| stage.report_parent == parent),
        "aic-learning.root-catalog-publication: report directory differs from staged sidecar"
    );
    let embedded = report
        .cases
        .iter()
        .flat_map(|case| case.mcts.root_records.iter())
        .filter_map(|record| match &record.target {
            RootRecordTarget::GumbelCatalog { descriptor } => Some(descriptor),
            _ => None,
        })
        .collect::<Vec<_>>();
    ensure!(
        embedded.len() == stages.len()
            && embedded
                .iter()
                .zip(stages.iter())
                .all(|(descriptor, stage)| Arc::ptr_eq(descriptor, &stage.descriptor)),
        "aic-learning.root-catalog-publication: report references differ from staged catalogs"
    );
    let published = match publish_json_identity(output, report, None) {
        Ok(published) => published,
        Err(error) => {
            if let Some(stop) = report_disk_stop(output, &error) {
                return Ok(Err(stop));
            }
            return Err(error);
        }
    };
    for stage in stages {
        stage.committed = true;
    }
    Ok(Ok(published))
}

fn report_disk_stop(output: &Path, error: &anyhow::Error) -> Option<CatalogStop> {
    let kind = error.chain().find_map(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .map(std::io::Error::kind)
            .or_else(|| {
                cause
                    .downcast_ref::<serde_json::Error>()
                    .and_then(serde_json::Error::io_error_kind)
            })
    })?;
    matches!(
        kind,
        std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded
    )
    .then(|| CatalogStop::Io {
        path: output.to_owned(),
        operation: "publish",
        source: std::io::Error::new(kind, error.to_string()),
    })
}

pub(in crate::hierarchical_search) fn validate_descriptor_lineage(
    descriptor: &RootCatalogDescriptor,
    source_sha256: &str,
    source_revision: u64,
) -> Result<()> {
    ensure!(
        descriptor.schema == DESCRIPTOR_SCHEMA
            && descriptor.action_feature_schema == ACTION_FEATURE_SCHEMA
            && descriptor.source_order_version == 1,
        "aic-learning.root-catalog-schema: unsupported descriptor"
    );
    ensure!(
        descriptor.binding.source_sha256 == source_sha256
            && descriptor.binding.source_revision == source_revision,
        "aic-learning.root-catalog-lineage: root source differs from descriptor"
    );
    ensure!(
        descriptor.initial_eligible_count > 0
            && descriptor.placement_count <= descriptor.initial_eligible_count
            && descriptor.selected_ordinal < descriptor.initial_eligible_count
            && descriptor.selected_rank < descriptor.initial_eligible_count
            && descriptor.selected_physical_id
                == physical_action_identity(&descriptor.selected_action)?,
        "aic-learning.root-catalog-lineage: selected action or count differs"
    );
    ensure!(
        descriptor
            .shortlist
            .iter()
            .any(|entry| entry.physical_id == descriptor.selected_physical_id),
        "aic-learning.root-catalog-lineage: selected action is absent from shortlist"
    );
    let mut visits = HashSet::new();
    for visit in &descriptor.visits {
        ensure!(
            visit.ordinal < descriptor.initial_eligible_count
                && visit.visits > 0
                && visit.value_sum.is_finite()
                && visits.insert(visit.ordinal),
            "aic-learning.root-catalog-lineage: invalid visited action"
        );
    }
    Ok(())
}

pub(super) fn open_replay(
    report_path: &Path,
    descriptor: &RootCatalogDescriptor,
    expected: &CatalogBinding,
) -> Result<BufReader<File>> {
    ensure!(
        descriptor.schema == DESCRIPTOR_SCHEMA
            && descriptor.action_feature_schema == ACTION_FEATURE_SCHEMA
            && descriptor.source_order_version == 1,
        "aic-learning.root-catalog-schema: unsupported descriptor"
    );
    ensure!(
        &descriptor.binding == expected,
        "aic-learning.root-catalog-binding: replay identity differs"
    );
    ensure!(
        Path::new(&descriptor.sidecar_name)
            .file_name()
            .is_some_and(|name| name == descriptor.sidecar_name.as_str())
            && Path::new(&descriptor.sidecar_name).components().count() == 1,
        "aic-learning.root-catalog-sidecar: sidecar name is not a basename"
    );
    let expected_bytes = descriptor
        .initial_eligible_count
        .checked_mul(LEDGER_ROW_BYTES as u64)
        .ok_or_else(|| anyhow::anyhow!("aic-learning.root-catalog-sidecar: length overflow"))?;
    ensure!(
        descriptor.sidecar_bytes == expected_bytes,
        "aic-learning.root-catalog-sidecar: descriptor length differs from action count"
    );
    let path = report_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join(&descriptor.sidecar_name);
    let mut file = File::open(&path)?;
    let (digest, bytes) = file_digest_open(&mut file)?;
    ensure!(
        bytes == descriptor.sidecar_bytes && digest == descriptor.sidecar_sha256,
        "aic-learning.root-catalog-sidecar: {} length or checksum differs",
        path.display()
    );
    file.seek(SeekFrom::Start(0))?;
    Ok(BufReader::new(file))
}

/// Call once when recovering an output, before creating any new catalog
/// owner. It preserves only sidecars named by the committed report and clears
/// that output's unpublished sidecars and known root sort/raw temporaries.
pub(in crate::hierarchical_search) fn recover_unpublished(
    report_path: &Path,
    root_prefixes: &[PathBuf],
) -> Result<()> {
    let parent = report_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let report_name = report_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            anyhow::anyhow!("aic-learning.root-catalog-recovery: report name is not UTF-8")
        })?;
    let retained = if report_path.exists() {
        let recovered: RecoveryReport =
            serde_json::from_reader(BufReader::new(File::open(report_path)?))?;
        ensure!(
            recovered.schema == crate::hierarchical_search::REPORT_SCHEMA,
            "aic-learning.root-catalog-recovery: report schema differs"
        );
        recovered
            .cases
            .into_iter()
            .flat_map(|case| case.mcts.root_records)
            .filter_map(|record| match record.target {
                RecoveryTarget::GumbelCatalog { descriptor } => Some(descriptor.sidecar_name),
                RecoveryTarget::PuctDense(_) => None,
            })
            .collect::<HashSet<_>>()
    } else {
        HashSet::new()
    };
    let mut temporary_stems = Vec::new();
    for prefix in root_prefixes {
        ensure!(
            prefix.parent() == Some(parent),
            "aic-learning.root-catalog-recovery: root prefix and report directory differ"
        );
        temporary_stems.push(
            prefix
                .with_extension("")
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    anyhow::anyhow!("aic-learning.root-catalog-recovery: root prefix is not UTF-8")
                })?
                .to_owned(),
        );
    }
    let sidecar_prefix = format!(".{report_name}.root-");
    let report_temp = format!(".{report_name}.tmp");
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let unpublished_sidecar = name.starts_with(&sidecar_prefix)
            && name.ends_with(".priors")
            && !retained.contains(name);
        let root_temporary = temporary_stems.iter().any(|stem| {
            name == format!("{stem}.root-score.tmp")
                || name == format!("{stem}.prior-ledger.tmp")
                || name == format!("{stem}.prior-sorted.tmp")
                || (name.starts_with(&format!("{stem}.prior-run-")) && name.ends_with(".tmp"))
                || (name.starts_with(&format!("{stem}.prior-merge-")) && name.ends_with(".tmp"))
        });
        if unpublished_sidecar || root_temporary || name == report_temp {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

/// Retire a completed curriculum collection after it leaves the replay window.
/// Publish the compact report first so interruption cannot leave a report that
/// claims replayable roots while their sidecars are missing. A retry also
/// removes sidecars left behind if interruption happened after that publication.
pub(in crate::hierarchical_search) fn retire_replay_data(report_path: &Path) -> Result<()> {
    let mut published = crate::hierarchical_search::load_published_report(report_path)?;
    if published.value.root_records_retained {
        for case in &mut published.value.cases {
            case.mcts.root_records.clear();
        }
        published.value.root_records_retained = false;
        crate::hierarchical_search::publish_json_identity(report_path, &published.value, None)?;
    }

    let parent = report_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let report_name = report_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            anyhow::anyhow!("aic-learning.root-catalog-retirement: report name is not UTF-8")
        })?;
    let sidecar_prefix = format!(".{report_name}.root-");
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&sidecar_prefix) && name.ends_with(".priors"))
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

fn file_digest_and_sync(path: &Path) -> std::io::Result<(String, u64)> {
    let mut file = File::open(path)?;
    file.sync_all()?;
    file_digest_open(&mut file)
}

fn file_digest_open(file: &mut File) -> std::io::Result<(String, u64)> {
    let mut input = BufReader::new(file);
    let mut buffer = [0u8; 64 * 1024];
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        bytes += read as u64;
    }
    Ok((hex_bytes(&hasher.finalize()), bytes))
}

fn hex_digest(bytes: &[u8]) -> String {
    hex_bytes(&Sha256::digest(bytes))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_disk_refusal_remains_a_typed_catalog_stop() {
        let error = anyhow::Error::new(std::io::Error::from(std::io::ErrorKind::StorageFull));
        let Some(CatalogStop::Io {
            path,
            operation,
            source,
        }) = report_disk_stop(Path::new("report.json"), &error)
        else {
            panic!("disk exhaustion must remain a typed publication stop")
        };
        assert_eq!(path, Path::new("report.json"));
        assert_eq!(operation, "publish");
        assert_eq!(source.kind(), std::io::ErrorKind::StorageFull);
        assert!(report_disk_stop(Path::new("report.json"), &anyhow::anyhow!("schema")).is_none());
    }
}
