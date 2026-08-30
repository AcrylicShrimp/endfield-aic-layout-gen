use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::recipes::{
    Recipe, RecipeProducerSelectionGroup, RecipeThroughputRequest, ValidatedRecipeBook,
};

const STAGE: &str = "recipe-selection-check";

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RecipeSelectionCheckStatus {
    Ready,
    SelectionRequired,
    InvalidInput,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeSelectionCheckReport {
    pub ready: bool,
    pub status: RecipeSelectionCheckStatus,
    pub producer_selection_groups: Vec<RecipeProducerSelectionGroup>,
    pub diagnostics: Vec<RecipeSelectionDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeSelectionDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl RecipeSelectionDiagnostic {
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

    fn info(code: &'static str, entity: Option<String>, message: impl Into<String>) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: "/producer_selections".to_string(),
            entity,
            message: message.into(),
        }
    }
}

impl RecipeSelectionCheckReport {
    pub fn invalid(diagnostics: Vec<RecipeSelectionDiagnostic>) -> Self {
        Self {
            ready: false,
            status: RecipeSelectionCheckStatus::InvalidInput,
            producer_selection_groups: Vec::new(),
            diagnostics,
        }
    }
}

pub fn check_recipe_selections(
    book: &ValidatedRecipeBook,
    request: &RecipeThroughputRequest,
) -> RecipeSelectionCheckReport {
    let selections = request
        .producer_selections
        .iter()
        .map(|selection| (selection.item.clone(), selection.recipe.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut diagnostics = validate_selection_references(book, &selections);
    for (index, item) in request.external_inputs.iter().enumerate() {
        if !book.index().is_external_item(item) && book.index().producer_ids_for(item).is_none() {
            diagnostics.push(RecipeSelectionDiagnostic::error(
                "unknown-external-input",
                format!("/external_inputs/{index}"),
                Some(item.clone()),
                format!("external input item '{item}' is neither external nor recipe-produced"),
            ));
        }
    }
    if !book.index().is_external_item(&request.target.item)
        && !request.external_inputs.contains(&request.target.item)
        && book
            .index()
            .producer_ids_for(&request.target.item)
            .is_none()
    {
        diagnostics.push(RecipeSelectionDiagnostic::error(
            "unknown-target-item",
            "/target/item",
            Some(request.target.item.clone()),
            format!(
                "target item '{}' is neither external nor recipe-produced",
                request.target.item
            ),
        ));
    }
    if !diagnostics.is_empty() {
        return RecipeSelectionCheckReport::invalid(diagnostics);
    }

    let external_inputs = request
        .external_inputs
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut checker = SelectionChecker {
        book,
        selections: &selections,
        external_inputs: &external_inputs,
        visited_recipes: BTreeSet::new(),
        visiting_recipes: BTreeSet::new(),
        groups: BTreeMap::new(),
    };
    checker.visit_item(&request.target.item);
    let groups = checker.groups.into_values().collect::<Vec<_>>();
    if groups.is_empty() {
        RecipeSelectionCheckReport {
            ready: true,
            status: RecipeSelectionCheckStatus::Ready,
            producer_selection_groups: Vec::new(),
            diagnostics: vec![RecipeSelectionDiagnostic::info(
                "recipe-selection-ready",
                Some(request.target.item.clone()),
                "every reachable item has a unique or explicitly selected producer",
            )],
        }
    } else {
        RecipeSelectionCheckReport {
            ready: false,
            status: RecipeSelectionCheckStatus::SelectionRequired,
            diagnostics: groups
                .iter()
                .map(|group| {
                    RecipeSelectionDiagnostic::info(
                        "producer-selection-required",
                        Some(group.item.clone()),
                        format!(
                            "item '{}' requires one producer selection from {} options",
                            group.item,
                            group.options.len()
                        ),
                    )
                })
                .collect(),
            producer_selection_groups: groups,
        }
    }
}

fn validate_selection_references(
    book: &ValidatedRecipeBook,
    selections: &BTreeMap<String, String>,
) -> Vec<RecipeSelectionDiagnostic> {
    let mut diagnostics = Vec::new();
    for (item, recipe) in selections {
        let Some(producer_ids) = book.index().producer_ids_for(item) else {
            diagnostics.push(RecipeSelectionDiagnostic::error(
                "unknown-producer-selection-item",
                "/producer_selections",
                Some(item.clone()),
                format!("producer selection item '{item}' is not produced by any recipe"),
            ));
            continue;
        };
        if book.index().recipe(recipe).is_none() {
            diagnostics.push(RecipeSelectionDiagnostic::error(
                "unknown-selected-recipe",
                "/producer_selections",
                Some(recipe.clone()),
                format!(
                    "producer selection for item '{item}' references unknown recipe '{recipe}'"
                ),
            ));
        } else if !producer_ids.contains(recipe) {
            diagnostics.push(RecipeSelectionDiagnostic::error(
                "selected-recipe-output-mismatch",
                "/producer_selections",
                Some(recipe.clone()),
                format!("selected recipe '{recipe}' does not produce item '{item}'"),
            ));
        }
    }
    diagnostics
}

struct SelectionChecker<'a> {
    book: &'a ValidatedRecipeBook,
    selections: &'a BTreeMap<String, String>,
    external_inputs: &'a BTreeSet<String>,
    visited_recipes: BTreeSet<String>,
    visiting_recipes: BTreeSet<String>,
    groups: BTreeMap<String, RecipeProducerSelectionGroup>,
}

impl SelectionChecker<'_> {
    fn visit_item(&mut self, item: &str) {
        if self.book.index().is_external_item(item) || self.external_inputs.contains(item) {
            return;
        }
        let Some(producer_ids) = self.book.index().producer_ids_for(item) else {
            return;
        };
        let recipe_id = match self.selections.get(item) {
            Some(recipe_id) => recipe_id,
            None if producer_ids.len() == 1 => &producer_ids[0],
            None => {
                self.groups.entry(item.to_string()).or_insert_with(|| {
                    RecipeProducerSelectionGroup {
                        item: item.to_string(),
                        options: producer_ids
                            .iter()
                            .map(|recipe_id| {
                                self.book
                                    .index()
                                    .recipe(recipe_id)
                                    .expect("producer IDs come from the validated recipe index")
                                    .clone()
                            })
                            .collect(),
                    }
                });
                return;
            }
        };
        self.visit_recipe(recipe_id);
    }

    fn visit_recipe(&mut self, recipe_id: &str) {
        if self.visited_recipes.contains(recipe_id) || self.visiting_recipes.contains(recipe_id) {
            return;
        }
        self.visiting_recipes.insert(recipe_id.to_string());
        let recipe: Recipe = self
            .book
            .index()
            .recipe(recipe_id)
            .expect("selection references were validated before traversal")
            .clone();
        for input in recipe.inputs {
            self.visit_item(&input.item);
        }
        self.visiting_recipes.remove(recipe_id);
        self.visited_recipes.insert(recipe_id.to_string());
    }
}
