use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::stable_id::{STABLE_ID_PATTERN, is_stable_id};

mod transport;

pub use transport::{
    LoadTransportCatalogError, SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION, TransportCapacity,
    TransportCatalog, TransportCatalogDiagnostic, TransportCatalogValidationReport,
    TransportDefinition, ValidatedTransportCatalog, load_transport_catalog,
    validate_transport_catalog,
};

const STAGE: &str = "item-catalog-validation";

pub const SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum TransportKind {
    Belt,
    Pipe,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ItemCatalog {
    pub schema_version: u32,
    pub items: Vec<ItemDefinition>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ItemDefinition {
    pub id: String,
    pub transport: TransportKind,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ItemCatalogValidationReport {
    pub valid: bool,
    pub diagnostics: Vec<ItemCatalogDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ItemCatalogDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl ItemCatalogDiagnostic {
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
pub struct ValidatedItemCatalog {
    catalog: ItemCatalog,
    item_index: BTreeMap<String, usize>,
}

impl ValidatedItemCatalog {
    pub fn try_from_catalog(catalog: ItemCatalog) -> Result<Self, ItemCatalogValidationReport> {
        let report = validate_item_catalog(&catalog);
        if !report.valid {
            return Err(report);
        }

        let item_index = catalog
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| (item.id.clone(), index))
            .collect();

        Ok(Self {
            catalog,
            item_index,
        })
    }

    pub fn catalog(&self) -> &ItemCatalog {
        &self.catalog
    }

    pub fn item(&self, item_id: &str) -> Option<&ItemDefinition> {
        self.item_index
            .get(item_id)
            .map(|index| &self.catalog.items[*index])
    }
}

pub fn load_item_catalog(path: impl AsRef<Path>) -> Result<ItemCatalog, LoadItemCatalogError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| LoadItemCatalogError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|source| LoadItemCatalogError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug)]
pub enum LoadItemCatalogError {
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for LoadItemCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, source } => write!(
                formatter,
                "failed to open item catalog file '{}': {source}",
                path.display()
            ),
            Self::Parse { path, source } => write!(
                formatter,
                "failed to parse item catalog file '{}': {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LoadItemCatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

pub fn validate_item_catalog(catalog: &ItemCatalog) -> ItemCatalogValidationReport {
    let mut diagnostics = Vec::new();

    if catalog.schema_version != SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION {
        diagnostics.push(ItemCatalogDiagnostic::error(
            "unsupported-item-catalog-schema-version",
            "/schema_version",
            None,
            format!(
                "item catalog schema_version {} is unsupported; expected {}",
                catalog.schema_version, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION
            ),
        ));
    }

    let mut seen_ids = BTreeSet::new();
    for (index, item) in catalog.items.iter().enumerate() {
        if !is_stable_id(&item.id) {
            diagnostics.push(ItemCatalogDiagnostic::error(
                "invalid-item-id",
                format!("/items/{index}/id"),
                Some(item.id.clone()),
                format!(
                    "item id '{}' must match stable ID pattern {STABLE_ID_PATTERN}",
                    item.id
                ),
            ));
        }
        if !seen_ids.insert(item.id.as_str()) {
            diagnostics.push(ItemCatalogDiagnostic::error(
                "duplicate-item-id",
                format!("/items/{index}/id"),
                Some(item.id.clone()),
                format!("item id '{}' appears more than once", item.id),
            ));
        }
    }

    ItemCatalogValidationReport {
        valid: diagnostics.is_empty(),
        diagnostics,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> ItemCatalog {
        ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "originium-ore".to_string(),
                    transport: TransportKind::Belt,
                },
                ItemDefinition {
                    id: "clean-water".to_string(),
                    transport: TransportKind::Pipe,
                },
            ],
        }
    }

    #[test]
    fn validates_and_promotes_fixed_item_transport_kinds() {
        let validated = ValidatedItemCatalog::try_from_catalog(catalog())
            .expect("item catalog should validate");

        assert_eq!(
            validated
                .item("originium-ore")
                .expect("belt item should exist")
                .transport,
            TransportKind::Belt
        );
        assert_eq!(
            validated
                .item("clean-water")
                .expect("pipe item should exist")
                .transport,
            TransportKind::Pipe
        );
    }

    #[test]
    fn rejects_invalid_duplicate_and_unsupported_catalog_entries() {
        let mut invalid = catalog();
        invalid.schema_version += 1;
        invalid.items[0].id = "Bad Item".to_string();
        invalid.items[1].id = "Bad Item".to_string();

        let report = validate_item_catalog(&invalid);

        assert!(!report.valid);
        assert_eq!(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![
                "unsupported-item-catalog-schema-version",
                "invalid-item-id",
                "invalid-item-id",
                "duplicate-item-id",
            ]
        );
    }

    #[test]
    fn rejects_unknown_json_fields() {
        let error = serde_json::from_str::<ItemCatalog>(
            r#"{
              "schema_version": 1,
              "items": [{ "id": "water", "transport": "pipe", "extra": true }]
            }"#,
        )
        .expect_err("unknown fields should fail parsing");

        assert!(error.to_string().contains("unknown field `extra`"));
    }
}
