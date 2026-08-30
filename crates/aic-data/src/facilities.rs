use std::{
    collections::BTreeSet,
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::stable_id::{STABLE_ID_PATTERN, is_stable_id};

mod validated;

pub use validated::ValidatedFacilityCatalog;

const STAGE: &str = "facility-catalog-validation";

pub const SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacilityCatalog {
    pub schema_version: u32,
    pub facilities: Vec<FacilityDefinition>,
}

impl FacilityCatalog {
    pub fn validate(&self) -> FacilityCatalogValidationReport {
        validate_facility_catalog(self)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacilityDefinition {
    pub id: String,
    pub footprint: FacilityFootprint,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacilityFootprint {
    pub width: i64,
    pub height: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityCatalogValidationReport {
    pub valid: bool,
    pub diagnostics: Vec<FacilityCatalogDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityCatalogDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl FacilityCatalogDiagnostic {
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

pub fn load_facility_catalog(
    path: impl AsRef<Path>,
) -> Result<FacilityCatalog, LoadFacilityCatalogError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| LoadFacilityCatalogError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|source| LoadFacilityCatalogError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug)]
pub enum LoadFacilityCatalogError {
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for LoadFacilityCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, source } => {
                write!(
                    formatter,
                    "failed to open facility catalog file '{}': {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(
                    formatter,
                    "failed to parse facility catalog file '{}': {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for LoadFacilityCatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

pub fn validate_facility_catalog(catalog: &FacilityCatalog) -> FacilityCatalogValidationReport {
    let mut diagnostics = Vec::new();

    validate_schema_version(catalog, &mut diagnostics);
    validate_facilities(catalog, &mut diagnostics);

    FacilityCatalogValidationReport {
        valid: diagnostics.is_empty(),
        diagnostics,
    }
}

fn validate_schema_version(
    catalog: &FacilityCatalog,
    diagnostics: &mut Vec<FacilityCatalogDiagnostic>,
) {
    if catalog.schema_version != SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION {
        diagnostics.push(FacilityCatalogDiagnostic::error(
            "unsupported-facility-catalog-schema-version",
            "/schema_version",
            None,
            format!(
                "facility catalog schema_version {} is unsupported; expected {}",
                catalog.schema_version, SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION
            ),
        ));
    }
}

fn validate_facilities(
    catalog: &FacilityCatalog,
    diagnostics: &mut Vec<FacilityCatalogDiagnostic>,
) {
    let mut seen_ids = BTreeSet::new();

    for (index, facility) in catalog.facilities.iter().enumerate() {
        if !is_stable_id(&facility.id) {
            diagnostics.push(FacilityCatalogDiagnostic::error(
                "invalid-facility-id",
                format!("/facilities/{index}/id"),
                Some(facility.id.clone()),
                format!(
                    "facility id '{}' must match {STABLE_ID_PATTERN}",
                    facility.id
                ),
            ));
        }

        if !seen_ids.insert(facility.id.as_str()) {
            diagnostics.push(FacilityCatalogDiagnostic::error(
                "duplicate-facility-id",
                format!("/facilities/{index}/id"),
                Some(facility.id.clone()),
                format!("facility id '{}' appears more than once", facility.id),
            ));
        }

        if facility.footprint.width <= 0 {
            diagnostics.push(FacilityCatalogDiagnostic::error(
                "non-positive-footprint-width",
                format!("/facilities/{index}/footprint/width"),
                Some(facility.id.clone()),
                format!(
                    "facility '{}' footprint width must be positive, found {}",
                    facility.id, facility.footprint.width
                ),
            ));
        }

        if facility.footprint.height <= 0 {
            diagnostics.push(FacilityCatalogDiagnostic::error(
                "non-positive-footprint-height",
                format!("/facilities/{index}/footprint/height"),
                Some(facility.id.clone()),
                format!(
                    "facility '{}' footprint height must be positive, found {}",
                    facility.id, facility.footprint.height
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_catalog() -> FacilityCatalog {
        FacilityCatalog {
            schema_version: 1,
            facilities: vec![FacilityDefinition {
                id: "grinding-unit".to_string(),
                footprint: FacilityFootprint {
                    width: 3,
                    height: 2,
                },
            }],
        }
    }

    #[test]
    fn accepts_valid_facility_catalog() {
        let report = valid_catalog().validate();

        assert!(report.valid);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn accepts_empty_facility_catalog() {
        let report = FacilityCatalog {
            schema_version: 1,
            facilities: Vec::new(),
        }
        .validate();

        assert!(report.valid);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn rejects_unknown_facility_catalog_fields_on_parse() {
        let error = serde_json::from_str::<FacilityCatalog>(
            r#"{
              "schema_version": 1,
              "facilities": [],
              "extra": true
            }"#,
        )
        .expect_err("unknown facility catalog fields should be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_facility_definition_fields_on_parse() {
        let error = serde_json::from_str::<FacilityCatalog>(
            r#"{
              "schema_version": 1,
              "facilities": [
                {
                  "id": "grinding-unit",
                  "footprint": { "width": 3, "height": 2 },
                  "extra": true
                }
              ]
            }"#,
        )
        .expect_err("unknown facility definition fields should be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_unknown_facility_footprint_fields_on_parse() {
        let error = serde_json::from_str::<FacilityCatalog>(
            r#"{
              "schema_version": 1,
              "facilities": [
                {
                  "id": "grinding-unit",
                  "footprint": { "width": 3, "height": 2, "extra": true }
                }
              ]
            }"#,
        )
        .expect_err("unknown facility footprint fields should be rejected");

        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn rejects_unsupported_facility_catalog_schema_version() {
        let mut catalog = valid_catalog();
        catalog.schema_version = SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION + 1;

        let report = catalog.validate();

        assert!(!report.valid);
        assert_facility_catalog_diagnostics(
            &report.diagnostics,
            &[(
                "unsupported-facility-catalog-schema-version",
                "/schema_version",
            )],
        );
    }

    #[test]
    fn rejects_invalid_and_duplicate_facility_ids() {
        let catalog = FacilityCatalog {
            schema_version: 1,
            facilities: vec![
                FacilityDefinition {
                    id: "Grinding Unit".to_string(),
                    footprint: FacilityFootprint {
                        width: 3,
                        height: 2,
                    },
                },
                FacilityDefinition {
                    id: "grinding-unit".to_string(),
                    footprint: FacilityFootprint {
                        width: 3,
                        height: 2,
                    },
                },
                FacilityDefinition {
                    id: "grinding-unit".to_string(),
                    footprint: FacilityFootprint {
                        width: 3,
                        height: 2,
                    },
                },
            ],
        };

        let report = catalog.validate();

        assert!(!report.valid);
        assert_facility_catalog_diagnostics(
            &report.diagnostics,
            &[
                ("invalid-facility-id", "/facilities/0/id"),
                ("duplicate-facility-id", "/facilities/2/id"),
            ],
        );
    }

    #[test]
    fn rejects_non_positive_footprint_dimensions() {
        let catalog = FacilityCatalog {
            schema_version: 1,
            facilities: vec![FacilityDefinition {
                id: "grinding-unit".to_string(),
                footprint: FacilityFootprint {
                    width: 0,
                    height: -1,
                },
            }],
        };

        let report = catalog.validate();

        assert!(!report.valid);
        assert_facility_catalog_diagnostics(
            &report.diagnostics,
            &[
                (
                    "non-positive-footprint-width",
                    "/facilities/0/footprint/width",
                ),
                (
                    "non-positive-footprint-height",
                    "/facilities/0/footprint/height",
                ),
            ],
        );
    }

    #[test]
    fn serializes_entity_as_null_when_unavailable() {
        let mut catalog = valid_catalog();
        catalog.schema_version = SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION + 1;
        let report = catalog.validate();

        let json = serde_json::to_value(&report).expect("facility catalog report should serialize");

        assert_eq!(json["diagnostics"][0]["entity"], serde_json::Value::Null);
    }

    #[test]
    fn missing_facility_catalog_file_returns_loader_error() {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("aic-missing-facility-catalog-file-{suffix}.json"));
        let error = load_facility_catalog(&path).expect_err("missing file should fail to load");

        assert!(matches!(error, LoadFacilityCatalogError::Open { .. }));
    }

    fn assert_facility_catalog_diagnostics(
        diagnostics: &[FacilityCatalogDiagnostic],
        expected: &[(&str, &str)],
    ) {
        let actual = diagnostics
            .iter()
            .map(|diagnostic| (diagnostic.code, diagnostic.path.as_str()))
            .collect::<Vec<_>>();

        for expected_diagnostic in expected {
            assert!(
                actual.contains(expected_diagnostic),
                "expected diagnostic {expected_diagnostic:?}, got {actual:?}"
            );
        }
    }
}
