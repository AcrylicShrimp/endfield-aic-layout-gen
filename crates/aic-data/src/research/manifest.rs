use std::{
    collections::BTreeMap,
    fmt::Write,
    fs::File,
    io::BufReader,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::stable_id::{STABLE_ID_PATTERN, is_stable_id};

const STAGE: &str = "benchmark-workload-validation";
pub const SUPPORTED_BENCHMARK_WORKLOAD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkWorkloadManifest {
    pub schema_version: u32,
    pub id: String,
    pub description: String,
    pub kind: BenchmarkWorkloadKind,
    pub inputs: BenchmarkWorkloadInputs,
    pub expected_target: BenchmarkTargetIdentity,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BenchmarkWorkloadKind {
    ContextualSourcePlan,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkWorkloadInputs {
    pub recipes: String,
    pub source_plan: String,
    pub facility_catalog: String,
    pub item_catalog: String,
    pub transport_catalog: String,
    pub logistics_component_catalog: String,
    pub localization_catalog: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkTargetIdentity {
    pub item: String,
    pub quantity: i64,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BenchmarkWorkloadManifestValidationReport {
    pub valid: bool,
    pub manifest_sha256: Option<String>,
    pub diagnostics: Vec<BenchmarkWorkloadManifestDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BenchmarkWorkloadManifestDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl BenchmarkWorkloadManifestDiagnostic {
    fn error(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "error",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedBenchmarkWorkloadManifest {
    manifest: BenchmarkWorkloadManifest,
    manifest_sha256: String,
}

impl ValidatedBenchmarkWorkloadManifest {
    pub fn try_from_manifest(
        manifest: BenchmarkWorkloadManifest,
    ) -> Result<Self, BenchmarkWorkloadManifestValidationReport> {
        let report = validate_benchmark_workload_manifest(&manifest);
        if !report.valid {
            return Err(report);
        }
        let manifest_sha256 = report
            .manifest_sha256
            .expect("a valid benchmark workload must have an identity hash");
        Ok(Self {
            manifest,
            manifest_sha256,
        })
    }

    pub fn manifest(&self) -> &BenchmarkWorkloadManifest {
        &self.manifest
    }

    pub fn manifest_sha256(&self) -> &str {
        &self.manifest_sha256
    }
}

pub fn load_benchmark_workload_manifest(
    path: impl AsRef<Path>,
) -> Result<BenchmarkWorkloadManifest, LoadBenchmarkWorkloadManifestError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| LoadBenchmarkWorkloadManifestError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_reader(BufReader::new(file)).map_err(|source| {
        LoadBenchmarkWorkloadManifestError::Parse {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[derive(Debug)]
pub enum LoadBenchmarkWorkloadManifestError {
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for LoadBenchmarkWorkloadManifestError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, source } => write!(
                formatter,
                "failed to open benchmark workload manifest '{}': {source}",
                path.display()
            ),
            Self::Parse { path, source } => write!(
                formatter,
                "failed to parse benchmark workload manifest '{}': {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LoadBenchmarkWorkloadManifestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

pub fn validate_benchmark_workload_manifest(
    manifest: &BenchmarkWorkloadManifest,
) -> BenchmarkWorkloadManifestValidationReport {
    let mut diagnostics = Vec::new();
    if manifest.schema_version != SUPPORTED_BENCHMARK_WORKLOAD_SCHEMA_VERSION {
        diagnostics.push(BenchmarkWorkloadManifestDiagnostic::error(
            "unsupported-benchmark-workload-schema-version",
            "/schema_version",
            None,
            format!(
                "benchmark workload schema_version {} is unsupported; expected {}",
                manifest.schema_version, SUPPORTED_BENCHMARK_WORKLOAD_SCHEMA_VERSION
            ),
        ));
    }
    if !is_stable_id(&manifest.id) {
        diagnostics.push(BenchmarkWorkloadManifestDiagnostic::error(
            "invalid-benchmark-workload-id",
            "/id",
            Some(manifest.id.clone()),
            format!(
                "benchmark workload id '{}' must match stable ID pattern {STABLE_ID_PATTERN}",
                manifest.id
            ),
        ));
    }
    if manifest.description.trim().is_empty() {
        diagnostics.push(BenchmarkWorkloadManifestDiagnostic::error(
            "empty-benchmark-workload-description",
            "/description",
            Some(manifest.id.clone()),
            "benchmark workload description must not be empty",
        ));
    }
    if !is_stable_id(&manifest.expected_target.item) {
        diagnostics.push(BenchmarkWorkloadManifestDiagnostic::error(
            "invalid-benchmark-target-item",
            "/expected_target/item",
            Some(manifest.expected_target.item.clone()),
            format!(
                "benchmark target item '{}' must match stable ID pattern {STABLE_ID_PATTERN}",
                manifest.expected_target.item
            ),
        ));
    }
    if manifest.expected_target.quantity <= 0 {
        diagnostics.push(BenchmarkWorkloadManifestDiagnostic::error(
            "invalid-benchmark-target-quantity",
            "/expected_target/quantity",
            Some(manifest.expected_target.item.clone()),
            "benchmark target quantity must be greater than zero",
        ));
    }
    if manifest.expected_target.duration_ms <= 0 {
        diagnostics.push(BenchmarkWorkloadManifestDiagnostic::error(
            "invalid-benchmark-target-duration",
            "/expected_target/duration_ms",
            Some(manifest.expected_target.item.clone()),
            "benchmark target duration_ms must be greater than zero",
        ));
    }

    let mut paths = BTreeMap::<&str, Vec<&str>>::new();
    for (field, path) in input_paths(&manifest.inputs) {
        validate_relative_input_path(field, path, &mut diagnostics);
        paths.entry(path).or_default().push(field);
    }
    for (path, fields) in paths {
        if fields.len() > 1 {
            diagnostics.push(BenchmarkWorkloadManifestDiagnostic::error(
                "duplicate-benchmark-input-path",
                fields[0],
                Some(path.to_string()),
                format!(
                    "benchmark input path '{path}' is used by multiple roles: {}",
                    fields.join(", ")
                ),
            ));
        }
    }

    let valid = diagnostics.is_empty();
    BenchmarkWorkloadManifestValidationReport {
        valid,
        manifest_sha256: valid.then(|| manifest_identity_sha256(manifest)),
        diagnostics,
    }
}

fn input_paths(inputs: &BenchmarkWorkloadInputs) -> Vec<(&'static str, &str)> {
    let mut paths = vec![
        ("/inputs/recipes", inputs.recipes.as_str()),
        ("/inputs/source_plan", inputs.source_plan.as_str()),
        ("/inputs/facility_catalog", inputs.facility_catalog.as_str()),
        ("/inputs/item_catalog", inputs.item_catalog.as_str()),
        (
            "/inputs/transport_catalog",
            inputs.transport_catalog.as_str(),
        ),
        (
            "/inputs/logistics_component_catalog",
            inputs.logistics_component_catalog.as_str(),
        ),
    ];
    if let Some(localization) = &inputs.localization_catalog {
        paths.push(("/inputs/localization_catalog", localization));
    }
    paths
}

fn validate_relative_input_path(
    field: &'static str,
    value: &str,
    diagnostics: &mut Vec<BenchmarkWorkloadManifestDiagnostic>,
) {
    let path = Path::new(value);
    let valid = !value.is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)));
    if !valid {
        diagnostics.push(BenchmarkWorkloadManifestDiagnostic::error(
            "invalid-benchmark-input-path",
            field,
            Some(value.to_string()),
            format!(
                "benchmark input path '{value}' must be a non-empty portable relative path without '.' or '..' components"
            ),
        ));
    }
}

fn manifest_identity_sha256(manifest: &BenchmarkWorkloadManifest) -> String {
    let mut digest = Sha256::new();
    hash_text(&mut digest, "aic-benchmark-workload-v1");
    digest.update(manifest.schema_version.to_be_bytes());
    hash_text(&mut digest, &manifest.id);
    hash_text(
        &mut digest,
        match manifest.kind {
            BenchmarkWorkloadKind::ContextualSourcePlan => "contextual-source-plan",
        },
    );
    for (field, path) in input_paths(&manifest.inputs) {
        hash_text(&mut digest, field);
        hash_text(&mut digest, path);
    }
    hash_text(&mut digest, &manifest.expected_target.item);
    digest.update(manifest.expected_target.quantity.to_be_bytes());
    digest.update(manifest.expected_target.duration_ms.to_be_bytes());
    hex_digest(digest)
}

fn hash_text(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

fn hex_digest(digest: Sha256) -> String {
    digest
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> BenchmarkWorkloadManifest {
        BenchmarkWorkloadManifest {
            schema_version: SUPPORTED_BENCHMARK_WORKLOAD_SCHEMA_VERSION,
            id: "heavy-xiranite-minimum-rate".to_string(),
            description: "Heavy Xiranite full contextual graph".to_string(),
            kind: BenchmarkWorkloadKind::ContextualSourcePlan,
            inputs: BenchmarkWorkloadInputs {
                recipes: "data/game/normalized/recipes.json".to_string(),
                source_plan: "data/examples/source-plan.game-heavy-xiranite-forge.request.json"
                    .to_string(),
                facility_catalog: "data/game/normalized/facilities.json".to_string(),
                item_catalog: "data/game/normalized/items.json".to_string(),
                transport_catalog: "data/game/normalized/transports.json".to_string(),
                logistics_component_catalog: "data/game/normalized/logistics-components.json"
                    .to_string(),
                localization_catalog: Some(
                    "data/game/normalized/localization.ko-KR.json".to_string(),
                ),
            },
            expected_target: BenchmarkTargetIdentity {
                item: "item-xiranite-enr-powder".to_string(),
                quantity: 1,
                duration_ms: 10_000,
            },
        }
    }

    #[test]
    fn validates_and_hashes_workload_identity_without_description() {
        let first = manifest();
        let mut renamed = first.clone();
        renamed.description = "Display text does not change identity".to_string();

        let first = ValidatedBenchmarkWorkloadManifest::try_from_manifest(first)
            .expect("workload should validate");
        let renamed = ValidatedBenchmarkWorkloadManifest::try_from_manifest(renamed)
            .expect("renamed workload should validate");

        assert_eq!(first.manifest_sha256(), renamed.manifest_sha256());
        assert_eq!(first.manifest_sha256().len(), 64);
    }

    #[test]
    fn rejects_bounds_as_workload_fields() {
        let mut value = serde_json::to_value(manifest()).expect("manifest should serialize");
        value["max_width"] = 50.into();

        let error = serde_json::from_value::<BenchmarkWorkloadManifest>(value)
            .expect_err("workload identity must reject scenario bounds");

        assert!(error.to_string().contains("unknown field `max_width`"));
    }

    #[test]
    fn rejects_parent_paths_and_invalid_target_rate() {
        let mut manifest = manifest();
        manifest.inputs.recipes = "../recipes.json".to_string();
        manifest.expected_target.duration_ms = 0;

        let report = validate_benchmark_workload_manifest(&manifest);

        assert!(!report.valid);
        assert!(report.manifest_sha256.is_none());
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "invalid-benchmark-input-path")
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "invalid-benchmark-target-duration")
        );
    }
}
