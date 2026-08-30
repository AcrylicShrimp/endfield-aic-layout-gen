use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::facilities::FacilityFootprint;
use crate::stable_id::{STABLE_ID_PATTERN, is_stable_id};

use super::{TransportCapacity, TransportKind};

const STAGE: &str = "logistics-component-catalog-validation";

pub const SUPPORTED_LOGISTICS_COMPONENT_CATALOG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LogisticsComponentCatalog {
    pub schema_version: u32,
    pub components: Vec<LogisticsComponentDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LogisticsComponentDefinition {
    pub id: String,
    pub transport: TransportKind,
    pub kind: LogisticsComponentKind,
    pub footprint: FacilityFootprint,
    pub allowed_rotations: Vec<i64>,
    pub input_directions: Vec<CardinalDirection>,
    pub output_directions: Vec<CardinalDirection>,
    pub capacity: TransportCapacity,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum LogisticsComponentKind {
    Splitter,
    Converger,
    Bridge,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum CardinalDirection {
    North,
    East,
    South,
    West,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LogisticsComponentCatalogValidationReport {
    pub valid: bool,
    pub diagnostics: Vec<LogisticsComponentCatalogDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LogisticsComponentCatalogDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl LogisticsComponentCatalogDiagnostic {
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
pub struct ValidatedLogisticsComponentCatalog {
    catalog: LogisticsComponentCatalog,
    component_index: BTreeMap<String, usize>,
}

impl ValidatedLogisticsComponentCatalog {
    pub fn try_from_catalog(
        catalog: LogisticsComponentCatalog,
    ) -> Result<Self, LogisticsComponentCatalogValidationReport> {
        let report = validate_logistics_component_catalog(&catalog);
        if !report.valid {
            return Err(report);
        }
        let component_index = catalog
            .components
            .iter()
            .enumerate()
            .map(|(index, component)| (component.id.clone(), index))
            .collect();
        Ok(Self {
            catalog,
            component_index,
        })
    }

    pub fn catalog(&self) -> &LogisticsComponentCatalog {
        &self.catalog
    }

    pub fn component(&self, id: &str) -> Option<&LogisticsComponentDefinition> {
        self.component_index
            .get(id)
            .map(|index| &self.catalog.components[*index])
    }

    pub fn component_by_kind(
        &self,
        transport: TransportKind,
        kind: LogisticsComponentKind,
    ) -> Option<&LogisticsComponentDefinition> {
        self.catalog
            .components
            .iter()
            .find(|component| component.transport == transport && component.kind == kind)
    }
}

pub fn load_logistics_component_catalog(
    path: impl AsRef<Path>,
) -> Result<LogisticsComponentCatalog, LoadLogisticsComponentCatalogError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| LoadLogisticsComponentCatalogError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_reader(BufReader::new(file)).map_err(|source| {
        LoadLogisticsComponentCatalogError::Parse {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[derive(Debug)]
pub enum LoadLogisticsComponentCatalogError {
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for LoadLogisticsComponentCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, source } => write!(
                formatter,
                "failed to open logistics component catalog file '{}': {source}",
                path.display()
            ),
            Self::Parse { path, source } => write!(
                formatter,
                "failed to parse logistics component catalog file '{}': {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LoadLogisticsComponentCatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

pub fn validate_logistics_component_catalog(
    catalog: &LogisticsComponentCatalog,
) -> LogisticsComponentCatalogValidationReport {
    let mut diagnostics = Vec::new();
    if catalog.schema_version != SUPPORTED_LOGISTICS_COMPONENT_CATALOG_SCHEMA_VERSION {
        diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
            "unsupported-logistics-component-catalog-schema-version",
            "/schema_version",
            None,
            format!(
                "logistics component catalog schema_version {} is unsupported; expected {}",
                catalog.schema_version, SUPPORTED_LOGISTICS_COMPONENT_CATALOG_SCHEMA_VERSION
            ),
        ));
    }

    let mut ids = BTreeSet::new();
    let mut capabilities = BTreeSet::new();
    for (index, component) in catalog.components.iter().enumerate() {
        validate_component(index, component, &mut diagnostics);
        if !ids.insert(component.id.as_str()) {
            diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
                "duplicate-logistics-component-id",
                format!("/components/{index}/id"),
                Some(component.id.clone()),
                format!(
                    "logistics component id '{}' appears more than once",
                    component.id
                ),
            ));
        }
        if !capabilities.insert((component.transport, component.kind)) {
            diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
                "duplicate-logistics-component-capability",
                format!("/components/{index}"),
                Some(component.id.clone()),
                format!(
                    "transport {:?} component kind {:?} appears more than once",
                    component.transport, component.kind
                ),
            ));
        }
    }

    for transport in [TransportKind::Belt, TransportKind::Pipe] {
        for kind in [
            LogisticsComponentKind::Splitter,
            LogisticsComponentKind::Converger,
            LogisticsComponentKind::Bridge,
        ] {
            if !capabilities.contains(&(transport, kind)) {
                diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
                    "missing-logistics-component-capability",
                    "/components",
                    None,
                    format!("catalog has no {transport:?} {kind:?} component"),
                ));
            }
        }
    }

    LogisticsComponentCatalogValidationReport {
        valid: diagnostics.is_empty(),
        diagnostics,
    }
}

fn validate_component(
    index: usize,
    component: &LogisticsComponentDefinition,
    diagnostics: &mut Vec<LogisticsComponentCatalogDiagnostic>,
) {
    if !is_stable_id(&component.id) {
        diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
            "invalid-logistics-component-id",
            format!("/components/{index}/id"),
            Some(component.id.clone()),
            format!(
                "component id '{}' must match {STABLE_ID_PATTERN}",
                component.id
            ),
        ));
    }
    if component.footprint.width != 1 || component.footprint.height != 1 {
        diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
            "unsupported-logistics-component-footprint",
            format!("/components/{index}/footprint"),
            Some(component.id.clone()),
            "current logistics components must have a one-by-one horizontal footprint",
        ));
    }
    validate_rotations(index, component, diagnostics);
    validate_directions(index, component, diagnostics);
    if component.capacity.quantity <= 0 || component.capacity.duration_ms <= 0 {
        diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
            "non-positive-logistics-component-capacity",
            format!("/components/{index}/capacity"),
            Some(component.id.clone()),
            "logistics component capacity quantity and duration_ms must be positive",
        ));
    }
}

fn validate_rotations(
    index: usize,
    component: &LogisticsComponentDefinition,
    diagnostics: &mut Vec<LogisticsComponentCatalogDiagnostic>,
) {
    let mut rotations = BTreeSet::new();
    for (rotation_index, rotation) in component.allowed_rotations.iter().enumerate() {
        if !matches!(rotation, 0 | 90 | 180 | 270) || !rotations.insert(*rotation) {
            diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
                "invalid-logistics-component-rotation",
                format!("/components/{index}/allowed_rotations/{rotation_index}"),
                Some(component.id.clone()),
                format!("rotation {rotation} must be a unique quarter turn"),
            ));
        }
    }
    if rotations.is_empty() {
        diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
            "empty-logistics-component-rotations",
            format!("/components/{index}/allowed_rotations"),
            Some(component.id.clone()),
            "logistics component must allow at least one rotation",
        ));
    }
}

fn validate_directions(
    index: usize,
    component: &LogisticsComponentDefinition,
    diagnostics: &mut Vec<LogisticsComponentCatalogDiagnostic>,
) {
    let inputs = component
        .input_directions
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let outputs = component
        .output_directions
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if inputs.len() != component.input_directions.len()
        || outputs.len() != component.output_directions.len()
    {
        diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
            "duplicate-logistics-component-direction",
            format!("/components/{index}"),
            Some(component.id.clone()),
            "input and output direction lists must not contain duplicates",
        ));
    }
    let topology_valid = match component.kind {
        LogisticsComponentKind::Splitter => inputs.len() == 1 && outputs.len() == 3,
        LogisticsComponentKind::Converger => inputs.len() == 3 && outputs.len() == 1,
        LogisticsComponentKind::Bridge => inputs.len() == 4 && outputs.len() == 4,
    };
    if !topology_valid {
        diagnostics.push(LogisticsComponentCatalogDiagnostic::error(
            "invalid-logistics-component-topology",
            format!("/components/{index}"),
            Some(component.id.clone()),
            format!(
                "component kind {:?} has {} input and {} output directions",
                component.kind,
                inputs.len(),
                outputs.len()
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(
        id: &str,
        transport: TransportKind,
        kind: LogisticsComponentKind,
    ) -> LogisticsComponentDefinition {
        let (inputs, outputs) = match kind {
            LogisticsComponentKind::Splitter => (
                vec![CardinalDirection::North],
                vec![
                    CardinalDirection::East,
                    CardinalDirection::South,
                    CardinalDirection::West,
                ],
            ),
            LogisticsComponentKind::Converger => (
                vec![
                    CardinalDirection::North,
                    CardinalDirection::East,
                    CardinalDirection::West,
                ],
                vec![CardinalDirection::South],
            ),
            LogisticsComponentKind::Bridge => (
                vec![
                    CardinalDirection::North,
                    CardinalDirection::East,
                    CardinalDirection::South,
                    CardinalDirection::West,
                ],
                vec![
                    CardinalDirection::North,
                    CardinalDirection::East,
                    CardinalDirection::South,
                    CardinalDirection::West,
                ],
            ),
        };
        LogisticsComponentDefinition {
            id: id.to_string(),
            transport,
            kind,
            footprint: FacilityFootprint {
                width: 1,
                height: 1,
            },
            allowed_rotations: vec![0, 90, 180, 270],
            input_directions: inputs,
            output_directions: outputs,
            capacity: TransportCapacity {
                quantity: 1,
                duration_ms: 1000,
            },
        }
    }

    fn catalog() -> LogisticsComponentCatalog {
        let mut components = Vec::new();
        for transport in [TransportKind::Belt, TransportKind::Pipe] {
            for kind in [
                LogisticsComponentKind::Splitter,
                LogisticsComponentKind::Converger,
                LogisticsComponentKind::Bridge,
            ] {
                components.push(component(
                    &format!("{transport:?}-{kind:?}").to_lowercase(),
                    transport,
                    kind,
                ));
            }
        }
        LogisticsComponentCatalog {
            schema_version: SUPPORTED_LOGISTICS_COMPONENT_CATALOG_SCHEMA_VERSION,
            components,
        }
    }

    #[test]
    fn validates_and_indexes_complete_component_capabilities() {
        let validated = ValidatedLogisticsComponentCatalog::try_from_catalog(catalog())
            .expect("catalog should validate");
        assert!(
            validated
                .component_by_kind(TransportKind::Pipe, LogisticsComponentKind::Bridge)
                .is_some()
        );
    }

    #[test]
    fn rejects_missing_capability_and_invalid_topology() {
        let mut catalog = catalog();
        catalog.components.pop();
        catalog.components[0].output_directions.clear();
        let report = validate_logistics_component_catalog(&catalog);
        assert!(!report.valid);
        let codes = report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<BTreeSet<_>>();
        assert!(codes.contains("invalid-logistics-component-topology"));
        assert!(codes.contains("missing-logistics-component-capability"));
    }

    #[test]
    fn rejects_unknown_json_fields() {
        let error = serde_json::from_str::<LogisticsComponentCatalog>(
            r#"{"schema_version":1,"components":[],"unknown":true}"#,
        )
        .expect_err("unknown fields must be rejected");
        assert!(error.to_string().contains("unknown field"));
    }
}
