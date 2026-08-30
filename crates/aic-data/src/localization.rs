use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    facilities::FacilityCatalog,
    logistics::ItemCatalog,
    recipes::RecipeBook,
    stable_id::{STABLE_ID_PATTERN, is_stable_id},
};

const STAGE: &str = "localization-catalog-validation";
pub const SUPPORTED_LOCALIZATION_CATALOG_SCHEMA_VERSION: u32 = 1;
pub const SUPPORTED_LOCALE: &str = "ko-KR";

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalizationCatalog {
    pub schema_version: u32,
    pub locale: String,
    pub items: Vec<LocalizedItem>,
    pub facilities: Vec<LocalizedFacility>,
    pub modes: Vec<LocalizedName>,
    pub recipe_descriptions: Vec<LocalizedRecipeDescription>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalizedItem {
    pub id: String,
    pub display_name: String,
    pub display_name_source: LocalizationTextSource,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalizedFacility {
    pub id: String,
    pub base_facility: String,
    pub facility_name: String,
    pub facility_name_source: LocalizationTextSource,
    pub mode: String,
    pub mode_name: String,
    pub mode_name_source: LocalizationTextSource,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalizedName {
    pub id: String,
    pub display_name: String,
    pub display_name_source: LocalizationTextSource,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct LocalizedRecipeDescription {
    pub id: String,
    pub description: String,
    pub description_source: LocalizationTextSource,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LocalizationTextSource {
    Official,
    IdFallback,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalizationCatalogValidationReport {
    pub valid: bool,
    pub diagnostics: Vec<LocalizationCatalogDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalizationCatalogDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl LocalizationCatalogDiagnostic {
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
pub struct ValidatedLocalizationCatalog {
    catalog: LocalizationCatalog,
    item_index: BTreeMap<String, usize>,
    facility_index: BTreeMap<String, usize>,
    mode_index: BTreeMap<String, usize>,
    recipe_index: BTreeMap<String, usize>,
}

impl ValidatedLocalizationCatalog {
    pub fn try_from_catalog(
        catalog: LocalizationCatalog,
    ) -> Result<Self, LocalizationCatalogValidationReport> {
        let report = validate_localization_catalog(&catalog);
        if !report.valid {
            return Err(report);
        }

        Ok(Self {
            item_index: index_by_id(&catalog.items, |item| &item.id),
            facility_index: index_by_id(&catalog.facilities, |facility| &facility.id),
            mode_index: index_by_id(&catalog.modes, |mode| &mode.id),
            recipe_index: index_by_id(&catalog.recipe_descriptions, |recipe| &recipe.id),
            catalog,
        })
    }

    pub fn catalog(&self) -> &LocalizationCatalog {
        &self.catalog
    }

    pub fn item(&self, id: &str) -> Option<&LocalizedItem> {
        self.item_index
            .get(id)
            .map(|index| &self.catalog.items[*index])
    }

    pub fn facility(&self, id: &str) -> Option<&LocalizedFacility> {
        self.facility_index
            .get(id)
            .map(|index| &self.catalog.facilities[*index])
    }

    pub fn mode(&self, id: &str) -> Option<&LocalizedName> {
        self.mode_index
            .get(id)
            .map(|index| &self.catalog.modes[*index])
    }

    pub fn recipe_description(&self, id: &str) -> Option<&LocalizedRecipeDescription> {
        self.recipe_index
            .get(id)
            .map(|index| &self.catalog.recipe_descriptions[*index])
    }
}

fn index_by_id<T>(values: &[T], id: impl Fn(&T) -> &String) -> BTreeMap<String, usize> {
    values
        .iter()
        .enumerate()
        .map(|(index, value)| (id(value).clone(), index))
        .collect()
}

pub fn load_localization_catalog(
    path: impl AsRef<Path>,
) -> Result<LocalizationCatalog, LoadLocalizationCatalogError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| LoadLocalizationCatalogError::Open {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_reader(BufReader::new(file)).map_err(|source| {
        LoadLocalizationCatalogError::Parse {
            path: path.to_path_buf(),
            source,
        }
    })
}

#[derive(Debug)]
pub enum LoadLocalizationCatalogError {
    Open {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for LoadLocalizationCatalogError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, source } => write!(
                formatter,
                "failed to open localization catalog file '{}': {source}",
                path.display()
            ),
            Self::Parse { path, source } => write!(
                formatter,
                "failed to parse localization catalog file '{}': {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for LoadLocalizationCatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
    }
}

pub fn validate_localization_catalog(
    catalog: &LocalizationCatalog,
) -> LocalizationCatalogValidationReport {
    let mut diagnostics = Vec::new();
    if catalog.schema_version != SUPPORTED_LOCALIZATION_CATALOG_SCHEMA_VERSION {
        diagnostics.push(LocalizationCatalogDiagnostic::error(
            "unsupported-localization-catalog-schema-version",
            "/schema_version",
            None,
            format!(
                "localization catalog schema_version {} is unsupported; expected {}",
                catalog.schema_version, SUPPORTED_LOCALIZATION_CATALOG_SCHEMA_VERSION
            ),
        ));
    }
    if catalog.locale != SUPPORTED_LOCALE {
        diagnostics.push(LocalizationCatalogDiagnostic::error(
            "unsupported-localization-locale",
            "/locale",
            Some(catalog.locale.clone()),
            format!(
                "localization locale '{}' is unsupported; expected '{SUPPORTED_LOCALE}'",
                catalog.locale
            ),
        ));
    }

    validate_items(catalog, &mut diagnostics);
    validate_facilities(catalog, &mut diagnostics);
    validate_modes(catalog, &mut diagnostics);
    validate_recipes(catalog, &mut diagnostics);

    let known_modes = catalog
        .modes
        .iter()
        .map(|mode| mode.id.as_str())
        .collect::<BTreeSet<_>>();
    for (index, facility) in catalog.facilities.iter().enumerate() {
        if !known_modes.contains(facility.mode.as_str()) {
            diagnostics.push(LocalizationCatalogDiagnostic::error(
                "unknown-localized-facility-mode",
                format!("/facilities/{index}/mode"),
                Some(facility.mode.clone()),
                format!(
                    "localized facility '{}' references missing mode '{}'",
                    facility.id, facility.mode
                ),
            ));
        }
    }

    LocalizationCatalogValidationReport {
        valid: diagnostics.is_empty(),
        diagnostics,
    }
}

pub fn validate_localization_coverage(
    catalog: &LocalizationCatalog,
    items: &ItemCatalog,
    facilities: &FacilityCatalog,
    recipes: &RecipeBook,
) -> LocalizationCatalogValidationReport {
    let mut report = validate_localization_catalog(catalog);
    compare_coverage(
        "item",
        "/items",
        items.items.iter().map(|item| item.id.as_str()),
        catalog.items.iter().map(|item| item.id.as_str()),
        &mut report.diagnostics,
    );
    compare_coverage(
        "facility",
        "/facilities",
        facilities
            .facilities
            .iter()
            .map(|facility| facility.id.as_str()),
        catalog
            .facilities
            .iter()
            .map(|facility| facility.id.as_str()),
        &mut report.diagnostics,
    );
    compare_coverage(
        "recipe-description",
        "/recipe_descriptions",
        recipes.recipes.iter().map(|recipe| recipe.id.as_str()),
        catalog
            .recipe_descriptions
            .iter()
            .map(|recipe| recipe.id.as_str()),
        &mut report.diagnostics,
    );
    report.valid = report.diagnostics.is_empty();
    report
}

fn validate_items(
    catalog: &LocalizationCatalog,
    diagnostics: &mut Vec<LocalizationCatalogDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for (index, item) in catalog.items.iter().enumerate() {
        validate_id("item", &item.id, format!("/items/{index}/id"), diagnostics);
        validate_unique(
            "item",
            &item.id,
            format!("/items/{index}/id"),
            &mut seen,
            diagnostics,
        );
        validate_text(
            &item.id,
            &item.display_name,
            item.display_name_source,
            format!("/items/{index}/display_name"),
            diagnostics,
        );
    }
}

fn validate_facilities(
    catalog: &LocalizationCatalog,
    diagnostics: &mut Vec<LocalizationCatalogDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for (index, facility) in catalog.facilities.iter().enumerate() {
        validate_id(
            "facility",
            &facility.id,
            format!("/facilities/{index}/id"),
            diagnostics,
        );
        validate_unique(
            "facility",
            &facility.id,
            format!("/facilities/{index}/id"),
            &mut seen,
            diagnostics,
        );
        validate_id(
            "base-facility",
            &facility.base_facility,
            format!("/facilities/{index}/base_facility"),
            diagnostics,
        );
        validate_id(
            "mode",
            &facility.mode,
            format!("/facilities/{index}/mode"),
            diagnostics,
        );
        validate_text(
            &facility.base_facility,
            &facility.facility_name,
            facility.facility_name_source,
            format!("/facilities/{index}/facility_name"),
            diagnostics,
        );
        validate_text(
            &facility.mode,
            &facility.mode_name,
            facility.mode_name_source,
            format!("/facilities/{index}/mode_name"),
            diagnostics,
        );
    }
}

fn validate_modes(
    catalog: &LocalizationCatalog,
    diagnostics: &mut Vec<LocalizationCatalogDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for (index, mode) in catalog.modes.iter().enumerate() {
        validate_id("mode", &mode.id, format!("/modes/{index}/id"), diagnostics);
        validate_unique(
            "mode",
            &mode.id,
            format!("/modes/{index}/id"),
            &mut seen,
            diagnostics,
        );
        validate_text(
            &mode.id,
            &mode.display_name,
            mode.display_name_source,
            format!("/modes/{index}/display_name"),
            diagnostics,
        );
    }
}

fn validate_recipes(
    catalog: &LocalizationCatalog,
    diagnostics: &mut Vec<LocalizationCatalogDiagnostic>,
) {
    let mut seen = BTreeSet::new();
    for (index, recipe) in catalog.recipe_descriptions.iter().enumerate() {
        validate_id(
            "recipe-description",
            &recipe.id,
            format!("/recipe_descriptions/{index}/id"),
            diagnostics,
        );
        validate_unique(
            "recipe-description",
            &recipe.id,
            format!("/recipe_descriptions/{index}/id"),
            &mut seen,
            diagnostics,
        );
        validate_text(
            &recipe.id,
            &recipe.description,
            recipe.description_source,
            format!("/recipe_descriptions/{index}/description"),
            diagnostics,
        );
    }
}

fn validate_id(
    kind: &'static str,
    id: &str,
    path: String,
    diagnostics: &mut Vec<LocalizationCatalogDiagnostic>,
) {
    if !is_stable_id(id) {
        diagnostics.push(LocalizationCatalogDiagnostic::error(
            "invalid-localization-id",
            path,
            Some(id.to_string()),
            format!("{kind} id '{id}' must match stable ID pattern {STABLE_ID_PATTERN}"),
        ));
    }
}

fn validate_unique<'a>(
    kind: &'static str,
    id: &'a str,
    path: String,
    seen: &mut BTreeSet<&'a str>,
    diagnostics: &mut Vec<LocalizationCatalogDiagnostic>,
) {
    if !seen.insert(id) {
        diagnostics.push(LocalizationCatalogDiagnostic::error(
            "duplicate-localization-id",
            path,
            Some(id.to_string()),
            format!("{kind} id '{id}' appears more than once"),
        ));
    }
}

fn validate_text(
    fallback_id: &str,
    value: &str,
    source: LocalizationTextSource,
    path: String,
    diagnostics: &mut Vec<LocalizationCatalogDiagnostic>,
) {
    if value.trim().is_empty() {
        diagnostics.push(LocalizationCatalogDiagnostic::error(
            "blank-localization-text",
            path.clone(),
            Some(fallback_id.to_string()),
            "localization text must not be blank",
        ));
    }
    if source == LocalizationTextSource::IdFallback && value != fallback_id {
        diagnostics.push(LocalizationCatalogDiagnostic::error(
            "inconsistent-localization-id-fallback",
            path,
            Some(fallback_id.to_string()),
            format!("ID fallback text must equal '{fallback_id}', found '{value}'"),
        ));
    }
}

fn compare_coverage<'a>(
    kind: &'static str,
    path: &'static str,
    expected: impl Iterator<Item = &'a str>,
    actual: impl Iterator<Item = &'a str>,
    diagnostics: &mut Vec<LocalizationCatalogDiagnostic>,
) {
    let expected = expected.collect::<BTreeSet<_>>();
    let actual = actual.collect::<BTreeSet<_>>();
    for id in expected.difference(&actual) {
        diagnostics.push(LocalizationCatalogDiagnostic::error(
            "missing-localization-coverage",
            path,
            Some((*id).to_string()),
            format!("{kind} '{id}' has no localization record"),
        ));
    }
    for id in actual.difference(&expected) {
        diagnostics.push(LocalizationCatalogDiagnostic::error(
            "unknown-localization-coverage",
            path,
            Some((*id).to_string()),
            format!("localization record '{id}' has no matching {kind}"),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        facilities::{FacilityDefinition, FacilityFootprint},
        logistics::{ItemDefinition, TransportKind},
        recipes::{ItemAmount, Recipe},
    };

    fn catalog() -> LocalizationCatalog {
        LocalizationCatalog {
            schema_version: SUPPORTED_LOCALIZATION_CATALOG_SCHEMA_VERSION,
            locale: SUPPORTED_LOCALE.to_string(),
            items: vec![LocalizedItem {
                id: "water".to_string(),
                display_name: "정제수".to_string(),
                display_name_source: LocalizationTextSource::Official,
            }],
            facilities: vec![LocalizedFacility {
                id: "purifier-mode-liquid".to_string(),
                base_facility: "purifier".to_string(),
                facility_name: "정수기".to_string(),
                facility_name_source: LocalizationTextSource::Official,
                mode: "liquid".to_string(),
                mode_name: "액체 모드".to_string(),
                mode_name_source: LocalizationTextSource::Official,
            }],
            modes: vec![LocalizedName {
                id: "liquid".to_string(),
                display_name: "액체 모드".to_string(),
                display_name_source: LocalizationTextSource::Official,
            }],
            recipe_descriptions: vec![LocalizedRecipeDescription {
                id: "purify-water".to_string(),
                description: "정제수 생산".to_string(),
                description_source: LocalizationTextSource::Official,
            }],
        }
    }

    #[test]
    fn validates_and_indexes_localized_entities() {
        let validated = ValidatedLocalizationCatalog::try_from_catalog(catalog())
            .expect("localization catalog should validate");

        assert_eq!(
            validated
                .item("water")
                .expect("item should exist")
                .display_name,
            "정제수"
        );
        assert_eq!(
            validated
                .facility("purifier-mode-liquid")
                .expect("facility should exist")
                .mode_name,
            "액체 모드"
        );
        assert_eq!(
            validated
                .recipe_description("purify-water")
                .expect("recipe should exist")
                .description,
            "정제수 생산"
        );
    }

    #[test]
    fn rejects_invalid_fallbacks_duplicates_and_unknown_modes() {
        let mut invalid = catalog();
        invalid.schema_version += 1;
        invalid.locale = "en-US".to_string();
        invalid.items[0].display_name = "invented".to_string();
        invalid.items[0].display_name_source = LocalizationTextSource::IdFallback;
        invalid.items.push(invalid.items[0].clone());
        invalid.facilities[0].mode = "missing-mode".to_string();

        let report = validate_localization_catalog(&invalid);
        assert!(!report.valid);
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| { diagnostic.code == "inconsistent-localization-id-fallback" })
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "duplicate-localization-id")
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "unknown-localized-facility-mode")
        );
    }

    #[test]
    fn reports_cross_catalog_coverage_gaps() {
        let items = ItemCatalog {
            schema_version: 1,
            items: vec![ItemDefinition {
                id: "missing-water".to_string(),
                transport: TransportKind::Pipe,
            }],
        };
        let facilities = FacilityCatalog {
            schema_version: 3,
            facilities: vec![FacilityDefinition {
                id: "purifier-mode-liquid".to_string(),
                footprint: FacilityFootprint {
                    width: 1,
                    height: 1,
                },
                allowed_rotations: vec![0],
                ports: Vec::new(),
            }],
        };
        let recipes = RecipeBook {
            schema_version: 1,
            external_items: Vec::new(),
            recipes: vec![Recipe {
                id: "purify-water".to_string(),
                facility: "purifier-mode-liquid".to_string(),
                inputs: Vec::new(),
                outputs: vec![ItemAmount {
                    item: "water".to_string(),
                    quantity: 1,
                }],
                duration_ms: 1000,
            }],
        };

        let report = validate_localization_coverage(&catalog(), &items, &facilities, &recipes);
        assert!(!report.valid);
        assert_eq!(
            report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.code)
                .collect::<Vec<_>>(),
            vec![
                "missing-localization-coverage",
                "unknown-localization-coverage"
            ]
        );
    }

    #[test]
    fn rejects_unknown_json_fields() {
        let error = serde_json::from_str::<LocalizationCatalog>(
            r#"{
              "schema_version": 1,
              "locale": "ko-KR",
              "items": [],
              "facilities": [],
              "modes": [],
              "recipe_descriptions": [],
              "extra": true
            }"#,
        )
        .expect_err("unknown fields should fail parsing");

        assert!(error.to_string().contains("unknown field `extra`"));
    }
}
