use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{
    localization::{LocalizationTextSource, ValidatedLocalizationCatalog},
    recipes::{
        ItemAmount, Recipe, ThroughputTarget, ValidatedRecipeBook,
        id::{STABLE_ID_PATTERN, is_stable_id},
    },
};

const STAGE: &str = "recipe-source-check";
pub const SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecipeSourcePlanRequest {
    pub schema_version: u32,
    pub target: ThroughputTarget,
    pub source_selections: Vec<RecipeSourceSelection>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecipeSourceSelection {
    pub path: String,
    pub source: RecipeSource,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RecipeSource {
    ExternalInput,
    Recipe { recipe: String },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RecipeSourceCheckStatus {
    Ready,
    SelectionRequired,
    InvalidInput,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RecipeSourceResolution {
    Automatic,
    Explicit,
    SelectionRequired,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeSourceCheckReport {
    pub ready: bool,
    pub status: RecipeSourceCheckStatus,
    pub root: Option<RecipeSourceNode>,
    pub source_catalog: Vec<RecipeSourceGroup>,
    pub required_selection_paths: Vec<String>,
    pub diagnostics: Vec<RecipeSourceDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeSourceNode {
    pub path: String,
    pub item: String,
    pub external_input_allowed: bool,
    pub resolution: RecipeSourceResolution,
    pub selected_source: Option<RecipeSource>,
    pub children: Vec<RecipeSourceNode>,
    pub cycle_to_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeSourceGroup {
    pub item: String,
    pub external_input_supported: bool,
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeSourceDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalizedRecipeSourceCheckReport {
    pub locale: String,
    pub ready: bool,
    pub status: RecipeSourceCheckStatus,
    pub root: Option<LocalizedRecipeSourceNode>,
    pub source_catalog: Vec<LocalizedRecipeSourceGroup>,
    pub required_selection_paths: Vec<String>,
    pub diagnostics: Vec<RecipeSourceDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalizedRecipeSourceNode {
    pub path: String,
    pub item: String,
    pub item_name: String,
    pub item_name_source: LocalizationTextSource,
    pub external_input_allowed: bool,
    pub resolution: RecipeSourceResolution,
    pub selected_source: Option<RecipeSource>,
    pub children: Vec<LocalizedRecipeSourceNode>,
    pub cycle_to_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalizedRecipeSourceGroup {
    pub item: String,
    pub item_name: String,
    pub item_name_source: LocalizationTextSource,
    pub external_input_supported: bool,
    pub recipes: Vec<LocalizedRecipeSourceOption>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalizedRecipeSourceOption {
    pub id: String,
    pub description: String,
    pub description_source: LocalizationTextSource,
    pub facility: LocalizedRecipeFacility,
    pub inputs: Vec<LocalizedItemAmount>,
    pub outputs: Vec<LocalizedItemAmount>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalizedRecipeFacility {
    pub id: String,
    pub base_facility: String,
    pub facility_name: String,
    pub facility_name_source: LocalizationTextSource,
    pub mode: String,
    pub mode_name: String,
    pub mode_name_source: LocalizationTextSource,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LocalizedItemAmount {
    pub item: String,
    pub item_name: String,
    pub item_name_source: LocalizationTextSource,
    pub quantity: i64,
}

impl RecipeSourceCheckReport {
    pub fn invalid(diagnostics: Vec<RecipeSourceDiagnostic>) -> Self {
        Self {
            ready: false,
            status: RecipeSourceCheckStatus::InvalidInput,
            root: None,
            source_catalog: Vec::new(),
            required_selection_paths: Vec::new(),
            diagnostics,
        }
    }
}

impl RecipeSourceDiagnostic {
    pub fn error(
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

    fn info(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }
}

pub fn check_recipe_source_plan(
    book: &ValidatedRecipeBook,
    request: &RecipeSourcePlanRequest,
) -> RecipeSourceCheckReport {
    let mut diagnostics = validate_request(request);
    if book
        .index()
        .producer_ids_for(&request.target.item)
        .is_none()
    {
        diagnostics.push(RecipeSourceDiagnostic::error(
            "unknown-target-item",
            "/target/item",
            Some(request.target.item.clone()),
            format!(
                "target item '{}' is not produced by any recipe",
                request.target.item
            ),
        ));
    }
    if !diagnostics.is_empty() {
        return RecipeSourceCheckReport {
            ready: false,
            status: RecipeSourceCheckStatus::InvalidInput,
            root: None,
            source_catalog: Vec::new(),
            required_selection_paths: Vec::new(),
            diagnostics,
        };
    }

    let source_catalog = build_source_catalog(book, &request.target.item);
    let selections = request
        .source_selections
        .iter()
        .map(|selection| (selection.path.clone(), selection.source.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut builder = SourceHierarchyBuilder {
        book,
        selections: &selections,
        known_paths: BTreeSet::new(),
        required_paths: BTreeSet::new(),
        diagnostics: Vec::new(),
    };
    let root = builder.build_node(
        &request.target.item,
        "/target".to_string(),
        true,
        &BTreeMap::new(),
    );
    for selection in &request.source_selections {
        if !builder.known_paths.contains(&selection.path) {
            builder.diagnostics.push(RecipeSourceDiagnostic::error(
                "unknown-source-selection-path",
                "/source_selections",
                Some(selection.path.clone()),
                format!(
                    "source selection path '{}' does not exist in the target hierarchy",
                    selection.path
                ),
            ));
        }
    }

    if !builder.diagnostics.is_empty() {
        return RecipeSourceCheckReport {
            ready: false,
            status: RecipeSourceCheckStatus::InvalidInput,
            root: Some(root),
            source_catalog,
            required_selection_paths: builder.required_paths.into_iter().collect(),
            diagnostics: builder.diagnostics,
        };
    }

    let required_selection_paths = builder.required_paths.into_iter().collect::<Vec<_>>();
    if required_selection_paths.is_empty() {
        RecipeSourceCheckReport {
            ready: true,
            status: RecipeSourceCheckStatus::Ready,
            root: Some(root),
            source_catalog,
            required_selection_paths,
            diagnostics: vec![RecipeSourceDiagnostic::info(
                "recipe-source-ready",
                "/",
                Some(request.target.item.clone()),
                "every active material demand has an automatic or explicit source",
            )],
        }
    } else {
        RecipeSourceCheckReport {
            ready: false,
            status: RecipeSourceCheckStatus::SelectionRequired,
            root: Some(root),
            source_catalog,
            diagnostics: required_selection_paths
                .iter()
                .map(|path| {
                    RecipeSourceDiagnostic::info(
                        "source-selection-required",
                        path,
                        None,
                        format!("material demand at '{path}' requires a source selection"),
                    )
                })
                .collect(),
            required_selection_paths,
        }
    }
}

pub fn localize_recipe_source_check_report(
    report: RecipeSourceCheckReport,
    localization: &ValidatedLocalizationCatalog,
) -> LocalizedRecipeSourceCheckReport {
    LocalizedRecipeSourceCheckReport {
        locale: localization.catalog().locale.clone(),
        ready: report.ready,
        status: report.status,
        root: report
            .root
            .map(|node| localize_source_node(node, localization)),
        source_catalog: report
            .source_catalog
            .into_iter()
            .map(|group| localize_source_group(group, localization))
            .collect(),
        required_selection_paths: report.required_selection_paths,
        diagnostics: report.diagnostics,
    }
}

fn localize_source_node(
    node: RecipeSourceNode,
    localization: &ValidatedLocalizationCatalog,
) -> LocalizedRecipeSourceNode {
    let (item_name, item_name_source) = localized_item_name(&node.item, localization);
    LocalizedRecipeSourceNode {
        path: node.path,
        item: node.item,
        item_name,
        item_name_source,
        external_input_allowed: node.external_input_allowed,
        resolution: node.resolution,
        selected_source: node.selected_source,
        children: node
            .children
            .into_iter()
            .map(|child| localize_source_node(child, localization))
            .collect(),
        cycle_to_path: node.cycle_to_path,
    }
}

fn localize_source_group(
    group: RecipeSourceGroup,
    localization: &ValidatedLocalizationCatalog,
) -> LocalizedRecipeSourceGroup {
    let (item_name, item_name_source) = localized_item_name(&group.item, localization);
    LocalizedRecipeSourceGroup {
        item: group.item,
        item_name,
        item_name_source,
        external_input_supported: group.external_input_supported,
        recipes: group
            .recipes
            .into_iter()
            .map(|recipe| localize_recipe_option(recipe, localization))
            .collect(),
    }
}

fn localize_recipe_option(
    recipe: Recipe,
    localization: &ValidatedLocalizationCatalog,
) -> LocalizedRecipeSourceOption {
    let description = localization.recipe_description(&recipe.id);
    let facility = localization.facility(&recipe.facility);
    LocalizedRecipeSourceOption {
        id: recipe.id.clone(),
        description: description
            .map(|description| description.description.clone())
            .unwrap_or_else(|| recipe.id.clone()),
        description_source: description
            .map(|description| description.description_source)
            .unwrap_or(LocalizationTextSource::IdFallback),
        facility: LocalizedRecipeFacility {
            id: recipe.facility.clone(),
            base_facility: facility
                .map(|facility| facility.base_facility.clone())
                .unwrap_or_else(|| recipe.facility.clone()),
            facility_name: facility
                .map(|facility| facility.facility_name.clone())
                .unwrap_or_else(|| recipe.facility.clone()),
            facility_name_source: facility
                .map(|facility| facility.facility_name_source)
                .unwrap_or(LocalizationTextSource::IdFallback),
            mode: facility
                .map(|facility| facility.mode.clone())
                .unwrap_or_else(|| recipe.facility.clone()),
            mode_name: facility
                .map(|facility| facility.mode_name.clone())
                .unwrap_or_else(|| recipe.facility.clone()),
            mode_name_source: facility
                .map(|facility| facility.mode_name_source)
                .unwrap_or(LocalizationTextSource::IdFallback),
        },
        inputs: localize_item_amounts(recipe.inputs, localization),
        outputs: localize_item_amounts(recipe.outputs, localization),
        duration_ms: recipe.duration_ms,
    }
}

fn localize_item_amounts(
    amounts: Vec<ItemAmount>,
    localization: &ValidatedLocalizationCatalog,
) -> Vec<LocalizedItemAmount> {
    amounts
        .into_iter()
        .map(|amount| {
            let (item_name, item_name_source) = localized_item_name(&amount.item, localization);
            LocalizedItemAmount {
                item: amount.item,
                item_name,
                item_name_source,
                quantity: amount.quantity,
            }
        })
        .collect()
}

fn localized_item_name(
    item_id: &str,
    localization: &ValidatedLocalizationCatalog,
) -> (String, LocalizationTextSource) {
    localization.item(item_id).map_or_else(
        || (item_id.to_string(), LocalizationTextSource::IdFallback),
        |item| (item.display_name.clone(), item.display_name_source),
    )
}

fn validate_request(request: &RecipeSourcePlanRequest) -> Vec<RecipeSourceDiagnostic> {
    let mut diagnostics = Vec::new();
    if request.schema_version != SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION {
        diagnostics.push(RecipeSourceDiagnostic::error(
            "unsupported-recipe-source-plan-schema-version",
            "/schema_version",
            None,
            format!(
                "schema_version must be {SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION}, found {}",
                request.schema_version
            ),
        ));
    }
    if !is_stable_id(&request.target.item) {
        diagnostics.push(RecipeSourceDiagnostic::error(
            "invalid-target-id",
            "/target/item",
            Some(request.target.item.clone()),
            format!(
                "target item '{}' must match {STABLE_ID_PATTERN}",
                request.target.item
            ),
        ));
    }
    if request.target.quantity <= 0 {
        diagnostics.push(RecipeSourceDiagnostic::error(
            "non-positive-target-quantity",
            "/target/quantity",
            None,
            format!(
                "target quantity must be positive, found {}",
                request.target.quantity
            ),
        ));
    }
    if request.target.duration_ms <= 0 {
        diagnostics.push(RecipeSourceDiagnostic::error(
            "non-positive-target-duration",
            "/target/duration_ms",
            None,
            format!(
                "target duration_ms must be positive, found {}",
                request.target.duration_ms
            ),
        ));
    }

    let mut seen_paths = BTreeSet::new();
    for (index, selection) in request.source_selections.iter().enumerate() {
        if !selection.path.starts_with('/') {
            diagnostics.push(RecipeSourceDiagnostic::error(
                "invalid-source-selection-path",
                format!("/source_selections/{index}/path"),
                Some(selection.path.clone()),
                "source selection path must start with '/'",
            ));
        }
        if !seen_paths.insert(selection.path.as_str()) {
            diagnostics.push(RecipeSourceDiagnostic::error(
                "duplicate-source-selection-path",
                format!("/source_selections/{index}/path"),
                Some(selection.path.clone()),
                format!(
                    "source selection path '{}' appears more than once",
                    selection.path
                ),
            ));
        }
        if let RecipeSource::Recipe { recipe } = &selection.source
            && !is_stable_id(recipe)
        {
            diagnostics.push(RecipeSourceDiagnostic::error(
                "invalid-source-selection-recipe-id",
                format!("/source_selections/{index}/source/recipe"),
                Some(recipe.clone()),
                format!("selected recipe '{recipe}' must match {STABLE_ID_PATTERN}"),
            ));
        }
    }
    diagnostics
}

fn build_source_catalog(book: &ValidatedRecipeBook, target_item: &str) -> Vec<RecipeSourceGroup> {
    fn visit(
        book: &ValidatedRecipeBook,
        item: &str,
        groups: &mut BTreeMap<String, RecipeSourceGroup>,
    ) {
        if groups.contains_key(item) {
            return;
        }
        let recipes = book
            .index()
            .producer_ids_for(item)
            .unwrap_or_default()
            .iter()
            .map(|recipe_id| {
                book.index()
                    .recipe(recipe_id)
                    .expect("producer IDs come from the validated recipe index")
                    .clone()
            })
            .collect::<Vec<_>>();
        groups.insert(
            item.to_string(),
            RecipeSourceGroup {
                item: item.to_string(),
                external_input_supported: true,
                recipes: recipes.clone(),
            },
        );
        for recipe in recipes {
            for input in recipe.inputs {
                visit(book, &input.item, groups);
            }
        }
    }

    let mut groups = BTreeMap::new();
    visit(book, target_item, &mut groups);
    groups.into_values().collect()
}

struct SourceHierarchyBuilder<'a> {
    book: &'a ValidatedRecipeBook,
    selections: &'a BTreeMap<String, RecipeSource>,
    known_paths: BTreeSet<String>,
    required_paths: BTreeSet<String>,
    diagnostics: Vec<RecipeSourceDiagnostic>,
}

impl SourceHierarchyBuilder<'_> {
    fn build_node(
        &mut self,
        item: &str,
        path: String,
        is_root: bool,
        ancestor_recipes: &BTreeMap<String, String>,
    ) -> RecipeSourceNode {
        self.known_paths.insert(path.clone());
        let producer_ids = self
            .book
            .index()
            .producer_ids_for(item)
            .map(<[String]>::to_vec)
            .unwrap_or_default();
        let explicit_source = self.selections.get(&path).cloned();
        let valid_explicit_source = match explicit_source {
            Some(RecipeSource::ExternalInput) if is_root => {
                self.diagnostics.push(RecipeSourceDiagnostic::error(
                    "external-target-selection",
                    "/source_selections",
                    Some(path.clone()),
                    "the target material cannot be selected as an external input",
                ));
                None
            }
            Some(RecipeSource::Recipe { recipe }) if !producer_ids.contains(&recipe) => {
                self.diagnostics.push(RecipeSourceDiagnostic::error(
                    "selected-recipe-output-mismatch",
                    "/source_selections",
                    Some(path.clone()),
                    format!("selected recipe '{recipe}' does not produce item '{item}'"),
                ));
                None
            }
            source => source,
        };

        let (resolution, selected_source) = if let Some(source) = valid_explicit_source {
            (RecipeSourceResolution::Explicit, Some(source))
        } else if producer_ids.len() == 1 {
            (
                RecipeSourceResolution::Automatic,
                Some(RecipeSource::Recipe {
                    recipe: producer_ids[0].clone(),
                }),
            )
        } else if producer_ids.is_empty() && !is_root {
            (
                RecipeSourceResolution::Automatic,
                Some(RecipeSource::ExternalInput),
            )
        } else {
            self.required_paths.insert(path.clone());
            (RecipeSourceResolution::SelectionRequired, None)
        };

        let (children, cycle_to_path) = match &selected_source {
            Some(RecipeSource::Recipe { recipe: recipe_id }) => {
                if let Some(ancestor_path) = ancestor_recipes.get(recipe_id) {
                    (Vec::new(), Some(ancestor_path.clone()))
                } else {
                    let recipe = self
                        .book
                        .index()
                        .recipe(recipe_id)
                        .expect("selected recipe was validated against producer IDs")
                        .clone();
                    let mut descendants = ancestor_recipes.clone();
                    descendants.insert(recipe_id.clone(), path.clone());
                    (
                        recipe
                            .inputs
                            .iter()
                            .map(|input| {
                                self.build_node(
                                    &input.item,
                                    format!("{path}/recipe:{recipe_id}/input:{}", input.item),
                                    false,
                                    &descendants,
                                )
                            })
                            .collect(),
                        None,
                    )
                }
            }
            _ => (Vec::new(), None),
        };

        RecipeSourceNode {
            path,
            item: item.to_string(),
            external_input_allowed: !is_root,
            resolution,
            selected_source,
            children,
            cycle_to_path,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        localization::{
            LocalizationCatalog, LocalizationTextSource, LocalizedFacility, LocalizedItem,
            LocalizedName, LocalizedRecipeDescription, ValidatedLocalizationCatalog,
        },
        recipes::{ItemAmount, RecipeBook},
    };

    fn recipe(id: &str, input: &str, output: &str) -> Recipe {
        Recipe {
            id: id.to_string(),
            facility: format!("{id}-facility"),
            inputs: vec![ItemAmount {
                item: input.to_string(),
                quantity: 1,
            }],
            outputs: vec![ItemAmount {
                item: output.to_string(),
                quantity: 1,
            }],
            duration_ms: 1000,
        }
    }

    fn contextual_book() -> ValidatedRecipeBook {
        ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: vec!["raw-material".to_string()],
            recipes: vec![
                recipe("make-shared-a", "raw-material", "shared-material"),
                recipe("make-shared-b", "raw-material", "shared-material"),
                recipe("make-left", "shared-material", "left-material"),
                recipe("make-right", "shared-material", "right-material"),
                Recipe {
                    id: "assemble-target".to_string(),
                    facility: "assembler".to_string(),
                    inputs: vec![
                        ItemAmount {
                            item: "left-material".to_string(),
                            quantity: 1,
                        },
                        ItemAmount {
                            item: "right-material".to_string(),
                            quantity: 1,
                        },
                    ],
                    outputs: vec![ItemAmount {
                        item: "target-material".to_string(),
                        quantity: 1,
                    }],
                    duration_ms: 1000,
                },
            ],
        })
        .expect("contextual source book should validate")
    }

    fn localization(book: &ValidatedRecipeBook) -> ValidatedLocalizationCatalog {
        let item_ids = book
            .recipe_book()
            .recipes
            .iter()
            .flat_map(|recipe| recipe.inputs.iter().chain(&recipe.outputs))
            .map(|amount| amount.item.clone())
            .collect::<BTreeSet<_>>();
        let facility_ids = book
            .recipe_book()
            .recipes
            .iter()
            .map(|recipe| recipe.facility.clone())
            .collect::<BTreeSet<_>>();
        let recipe_descriptions = book
            .recipe_book()
            .recipes
            .iter()
            .map(|recipe| LocalizedRecipeDescription {
                id: recipe.id.clone(),
                description: format!("{} 설명", recipe.id),
                description_source: LocalizationTextSource::Official,
            })
            .collect();
        ValidatedLocalizationCatalog::try_from_catalog(LocalizationCatalog {
            schema_version: 1,
            locale: "ko-KR".to_string(),
            items: item_ids
                .into_iter()
                .map(|id| LocalizedItem {
                    display_name: if id == "shared-material" {
                        "공유 재료".to_string()
                    } else {
                        format!("{id} 이름")
                    },
                    id,
                    display_name_source: LocalizationTextSource::Official,
                })
                .collect(),
            facilities: facility_ids
                .into_iter()
                .map(|id| LocalizedFacility {
                    base_facility: id.clone(),
                    facility_name: format!("{id} 설비"),
                    facility_name_source: LocalizationTextSource::Official,
                    mode: "normal".to_string(),
                    mode_name: "기본 모드".to_string(),
                    mode_name_source: LocalizationTextSource::Official,
                    id,
                })
                .collect(),
            modes: vec![LocalizedName {
                id: "normal".to_string(),
                display_name: "기본 모드".to_string(),
                display_name_source: LocalizationTextSource::Official,
            }],
            recipe_descriptions,
        })
        .expect("test localization should validate")
    }

    fn request(source_selections: Vec<RecipeSourceSelection>) -> RecipeSourcePlanRequest {
        RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "target-material".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections,
        }
    }

    fn find_node<'a>(node: &'a RecipeSourceNode, path: &str) -> Option<&'a RecipeSourceNode> {
        if node.path == path {
            return Some(node);
        }
        node.children
            .iter()
            .find_map(|child| find_node(child, path))
    }

    #[test]
    fn returns_all_context_paths_in_one_hierarchy() {
        let left_path = "/target/recipe:assemble-target/input:left-material/recipe:make-left/input:shared-material";
        let right_path = "/target/recipe:assemble-target/input:right-material/recipe:make-right/input:shared-material";

        let report = check_recipe_source_plan(&contextual_book(), &request(Vec::new()));

        assert_eq!(report.status, RecipeSourceCheckStatus::SelectionRequired);
        assert_eq!(
            report.required_selection_paths,
            vec![left_path.to_string(), right_path.to_string()]
        );
        let root = report.root.as_ref().expect("hierarchy should have a root");
        for path in [left_path, right_path] {
            let node = find_node(root, path).expect("both shared-material contexts should exist");
            assert_eq!(node.item, "shared-material");
            assert!(node.external_input_allowed);
            assert_eq!(node.resolution, RecipeSourceResolution::SelectionRequired);
        }
        let shared_group = report
            .source_catalog
            .iter()
            .find(|group| group.item == "shared-material")
            .expect("shared-material choices should be present in the catalog");
        assert!(shared_group.external_input_supported);
        assert_eq!(
            shared_group
                .recipes
                .iter()
                .map(|recipe| recipe.id.as_str())
                .collect::<Vec<_>>(),
            vec!["make-shared-a", "make-shared-b"]
        );
    }

    #[test]
    fn localizes_hierarchy_groups_and_recipe_details() {
        let book = contextual_book();
        let report = check_recipe_source_plan(&book, &request(Vec::new()));
        let report = localize_recipe_source_check_report(report, &localization(&book));
        let shared_group = report
            .source_catalog
            .iter()
            .find(|group| group.item == "shared-material")
            .expect("shared-material choices should exist");

        assert_eq!(report.locale, "ko-KR");
        assert_eq!(shared_group.item_name, "공유 재료");
        assert_eq!(shared_group.recipes[0].description, "make-shared-a 설명");
        assert_eq!(shared_group.recipes[0].facility.mode_name, "기본 모드");
        assert_eq!(shared_group.recipes[0].outputs[0].item_name, "공유 재료");
    }

    #[test]
    fn allows_the_same_material_to_use_different_sources_by_context() {
        let left_path = "/target/recipe:assemble-target/input:left-material/recipe:make-left/input:shared-material";
        let right_path = "/target/recipe:assemble-target/input:right-material/recipe:make-right/input:shared-material";
        let report = check_recipe_source_plan(
            &contextual_book(),
            &request(vec![
                RecipeSourceSelection {
                    path: left_path.to_string(),
                    source: RecipeSource::ExternalInput,
                },
                RecipeSourceSelection {
                    path: right_path.to_string(),
                    source: RecipeSource::Recipe {
                        recipe: "make-shared-a".to_string(),
                    },
                },
            ]),
        );

        assert!(report.ready, "{:#?}", report.diagnostics);
        let root = report.root.as_ref().expect("hierarchy should have a root");
        assert_eq!(
            find_node(root, left_path).unwrap().selected_source,
            Some(RecipeSource::ExternalInput)
        );
        assert_eq!(
            find_node(root, right_path).unwrap().selected_source,
            Some(RecipeSource::Recipe {
                recipe: "make-shared-a".to_string()
            })
        );
    }

    #[test]
    fn represents_recipe_cycles_with_finite_ancestor_references() {
        let book = ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: Vec::new(),
            recipes: vec![
                recipe("grow-crop", "seed", "crop"),
                recipe("collect-seed", "crop", "seed"),
            ],
        })
        .expect("cyclic source book should validate");
        let request = RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "crop".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections: Vec::new(),
        };

        let report = check_recipe_source_plan(&book, &request);

        assert!(report.ready, "{:#?}", report.diagnostics);
        let json = serde_json::to_value(&report).expect("report should serialize");
        assert_eq!(
            json.pointer("/root/children/0/children/0/cycle_to_path"),
            Some(&serde_json::Value::String("/target".to_string()))
        );
    }

    #[test]
    fn rejects_external_input_for_the_target_context() {
        let report = check_recipe_source_plan(
            &contextual_book(),
            &request(vec![RecipeSourceSelection {
                path: "/target".to_string(),
                source: RecipeSource::ExternalInput,
            }]),
        );

        assert_eq!(report.status, RecipeSourceCheckStatus::InvalidInput);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, "external-target-selection");
        assert_eq!(report.diagnostics[0].entity.as_deref(), Some("/target"));
    }

    #[test]
    fn rejects_selection_paths_outside_the_selected_hierarchy() {
        let report = check_recipe_source_plan(
            &contextual_book(),
            &request(vec![RecipeSourceSelection {
                path: "/target/not-a-material-context".to_string(),
                source: RecipeSource::ExternalInput,
            }]),
        );

        assert_eq!(report.status, RecipeSourceCheckStatus::InvalidInput);
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, "unknown-source-selection-path");
        assert_eq!(
            report.diagnostics[0].entity.as_deref(),
            Some("/target/not-a-material-context")
        );
    }
}
