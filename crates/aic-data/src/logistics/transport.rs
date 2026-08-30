use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::TransportKind;

const STAGE: &str = "transport-catalog-validation";

pub const SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransportCatalog {
    pub schema_version: u32,
    pub transports: Vec<TransportDefinition>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransportDefinition {
    pub kind: TransportKind,
    pub capacity: TransportCapacity,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TransportCapacity {
    pub quantity: i64,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TransportCatalogValidationReport {
    pub valid: bool,
    pub diagnostics: Vec<TransportCatalogDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TransportCatalogDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl TransportCatalogDiagnostic {
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
pub struct ValidatedTransportCatalog {
    catalog: TransportCatalog,
    index: BTreeMap<TransportKind, usize>,
}

impl ValidatedTransportCatalog {
    pub fn try_from_catalog(
        catalog: TransportCatalog,
    ) -> Result<Self, TransportCatalogValidationReport> {
        let report = validate_transport_catalog(&catalog);
        if !report.valid {
            return Err(report);
        }
        let index = catalog
            .transports
            .iter()
            .enumerate()
            .map(|(index, transport)| (transport.kind, index))
            .collect();
        Ok(Self { catalog, index })
    }

    pub fn catalog(&self) -> &TransportCatalog {
        &self.catalog
    }

    pub fn capacity(&self, kind: TransportKind) -> &TransportCapacity {
        &self.catalog.transports[self.index[&kind]].capacity
    }
}

pub fn load_transport_catalog(
    path: impl AsRef<Path>,
) -> Result<TransportCatalog, LoadTransportCatalogError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| LoadTransportCatalogError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|source| LoadTransportCatalogError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug)]
pub enum LoadTransportCatalogError {
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for LoadTransportCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, source } => write!(
                formatter,
                "failed to open transport catalog file '{}': {source}",
                path.display()
            ),
            Self::Parse { path, source } => write!(
                formatter,
                "failed to parse transport catalog file '{}': {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LoadTransportCatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

pub fn validate_transport_catalog(catalog: &TransportCatalog) -> TransportCatalogValidationReport {
    let mut diagnostics = Vec::new();
    if catalog.schema_version != SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION {
        diagnostics.push(TransportCatalogDiagnostic::error(
            "unsupported-transport-catalog-schema-version",
            "/schema_version",
            None,
            format!(
                "transport catalog schema_version {} is unsupported; expected {}",
                catalog.schema_version, SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION
            ),
        ));
    }

    let mut seen = BTreeSet::new();
    for (index, transport) in catalog.transports.iter().enumerate() {
        let entity = format!("{:?}", transport.kind).to_lowercase();
        if !seen.insert(transport.kind) {
            diagnostics.push(TransportCatalogDiagnostic::error(
                "duplicate-transport-kind",
                format!("/transports/{index}/kind"),
                Some(entity.clone()),
                format!("transport kind '{entity}' appears more than once"),
            ));
        }
        if transport.capacity.quantity <= 0 {
            diagnostics.push(TransportCatalogDiagnostic::error(
                "non-positive-transport-capacity-quantity",
                format!("/transports/{index}/capacity/quantity"),
                Some(entity.clone()),
                format!(
                    "transport capacity quantity must be positive, found {}",
                    transport.capacity.quantity
                ),
            ));
        }
        if transport.capacity.duration_ms <= 0 {
            diagnostics.push(TransportCatalogDiagnostic::error(
                "non-positive-transport-capacity-duration",
                format!("/transports/{index}/capacity/duration_ms"),
                Some(entity),
                format!(
                    "transport capacity duration_ms must be positive, found {}",
                    transport.capacity.duration_ms
                ),
            ));
        }
    }
    for kind in [TransportKind::Belt, TransportKind::Pipe] {
        if !seen.contains(&kind) {
            let entity = format!("{kind:?}").to_lowercase();
            diagnostics.push(TransportCatalogDiagnostic::error(
                "missing-transport-kind",
                "/transports",
                Some(entity.clone()),
                format!("transport catalog is missing '{entity}'"),
            ));
        }
    }

    TransportCatalogValidationReport {
        valid: diagnostics.is_empty(),
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> TransportCatalog {
        TransportCatalog {
            schema_version: SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION,
            transports: vec![
                TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 2000,
                    },
                },
                TransportDefinition {
                    kind: TransportKind::Pipe,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 500,
                    },
                },
            ],
        }
    }

    #[test]
    fn validates_and_promotes_transport_capacities() {
        let validated = ValidatedTransportCatalog::try_from_catalog(catalog())
            .expect("transport catalog should validate");

        assert_eq!(
            validated.capacity(TransportKind::Belt),
            &TransportCapacity {
                quantity: 1,
                duration_ms: 2000,
            }
        );
        assert_eq!(validated.capacity(TransportKind::Pipe).duration_ms, 500);
    }

    #[test]
    fn rejects_duplicate_missing_and_non_positive_capacities() {
        let mut invalid = catalog();
        invalid.schema_version += 1;
        invalid.transports[0].capacity.quantity = 0;
        invalid.transports[0].capacity.duration_ms = -1;
        invalid.transports[1].kind = TransportKind::Belt;

        let report = validate_transport_catalog(&invalid);
        assert!(!report.valid);
        assert_eq!(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![
                "unsupported-transport-catalog-schema-version",
                "non-positive-transport-capacity-quantity",
                "non-positive-transport-capacity-duration",
                "duplicate-transport-kind",
                "missing-transport-kind",
            ]
        );
    }

    #[test]
    fn rejects_unknown_transport_fields() {
        let error = serde_json::from_str::<TransportCatalog>(
            r#"{
              "schema_version": 1,
              "transports": [
                {
                  "kind": "belt",
                  "capacity": { "quantity": 1, "duration_ms": 2000, "extra": true }
                }
              ]
            }"#,
        )
        .expect_err("unknown fields should fail parsing");

        assert!(error.to_string().contains("unknown field `extra`"));
    }
}
