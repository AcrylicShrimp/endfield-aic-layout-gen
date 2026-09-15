use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::recipes::{
    ItemAmount, Recipe, RecipeGraphError, RecipeSelectionCheckStatus, ValidatedRecipeBook,
    check_recipe_selections,
    id::{STABLE_ID_PATTERN, is_stable_id},
};

mod contextual;
mod cyclic;

pub use contextual::{
    ContextualExternalInputRate, ContextualMaterialFlowRate, ContextualRecipeRunRate,
    ContextualSurplusRate, ContextualThroughputDiagnostic, ContextualThroughputReport,
};

pub const SUPPORTED_THROUGHPUT_REQUEST_SCHEMA_VERSION: u32 = 3;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecipeThroughputRequest {
    pub schema_version: u32,
    pub target: ThroughputTarget,
    pub external_inputs: Vec<String>,
    pub producer_selections: Vec<RecipeProducerSelection>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecipeProducerSelection {
    pub item: String,
    pub recipe: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ThroughputTarget {
    pub item: String,
    pub quantity: i64,
    pub duration_ms: i64,
}

pub use aic_layout::rate::{Rate, ThroughputDiagnostic};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ItemRate {
    pub item: String,
    pub rate: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeRunRate {
    pub recipe: String,
    pub facility: String,
    pub runs_per_second: Rate,
    pub work_seconds_per_second: Rate,
    pub limiting_outputs: Vec<String>,
    pub input_rates: Vec<ItemRate>,
    pub output_rates: Vec<ItemRate>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeProducerSelectionGroup {
    pub item: String,
    pub options: Vec<Recipe>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeThroughputReport {
    pub success: bool,
    pub target: Option<ItemRate>,
    pub recipe_rates: Vec<RecipeRunRate>,
    pub external_input_rates: Vec<ItemRate>,
    pub item_demand_rates: Vec<ItemRate>,
    pub surplus_rates: Vec<ItemRate>,
    pub bootstrap_item_options: Vec<String>,
    pub producer_selection_groups: Vec<RecipeProducerSelectionGroup>,
    pub diagnostics: Vec<ThroughputDiagnostic>,
}

impl RecipeThroughputReport {
    pub fn failure(diagnostic: ThroughputDiagnostic) -> Self {
        Self::failure_many(vec![diagnostic])
    }

    pub fn failure_many(diagnostics: Vec<ThroughputDiagnostic>) -> Self {
        Self {
            success: false,
            target: None,
            recipe_rates: Vec::new(),
            external_input_rates: Vec::new(),
            item_demand_rates: Vec::new(),
            surplus_rates: Vec::new(),
            bootstrap_item_options: Vec::new(),
            producer_selection_groups: Vec::new(),
            diagnostics,
        }
    }

    fn failure_with_producer_selection_group(
        diagnostic: ThroughputDiagnostic,
        group: RecipeProducerSelectionGroup,
    ) -> Self {
        let mut report = Self::failure(diagnostic);
        report.producer_selection_groups.push(group);
        report
    }

    fn failure_with_producer_selection_groups(
        diagnostics: Vec<ThroughputDiagnostic>,
        groups: Vec<RecipeProducerSelectionGroup>,
    ) -> Self {
        let mut report = Self::failure_many(diagnostics);
        report.producer_selection_groups = groups;
        report
    }
}

pub fn validate_throughput_request(request: &RecipeThroughputRequest) -> Vec<ThroughputDiagnostic> {
    let mut diagnostics = Vec::new();

    if request.schema_version != SUPPORTED_THROUGHPUT_REQUEST_SCHEMA_VERSION {
        diagnostics.push(ThroughputDiagnostic::error(
            "unsupported-throughput-request-schema-version",
            "/schema_version",
            None,
            format!(
                "schema_version must be {SUPPORTED_THROUGHPUT_REQUEST_SCHEMA_VERSION}, found {}",
                request.schema_version
            ),
        ));
    }

    if !is_stable_id(&request.target.item) {
        diagnostics.push(ThroughputDiagnostic::error(
            "invalid-target-id",
            "/target/item",
            Some(request.target.item.clone()),
            format!(
                "target item '{}' must match {STABLE_ID_PATTERN}",
                request.target.item
            ),
        ));
    }

    let mut seen_external_inputs = BTreeSet::new();
    for (index, item) in request.external_inputs.iter().enumerate() {
        if !is_stable_id(item) {
            diagnostics.push(ThroughputDiagnostic::error(
                "invalid-external-input-id",
                format!("/external_inputs/{index}"),
                Some(item.clone()),
                format!("external input item '{item}' must match {STABLE_ID_PATTERN}"),
            ));
        }
        if !seen_external_inputs.insert(item.as_str()) {
            diagnostics.push(ThroughputDiagnostic::error(
                "duplicate-external-input",
                format!("/external_inputs/{index}"),
                Some(item.clone()),
                format!("external input item '{item}' appears more than once"),
            ));
        }
    }

    let mut seen_selection_items = BTreeSet::new();
    for (index, selection) in request.producer_selections.iter().enumerate() {
        if !is_stable_id(&selection.item) {
            diagnostics.push(ThroughputDiagnostic::error(
                "invalid-producer-selection-item-id",
                format!("/producer_selections/{index}/item"),
                Some(selection.item.clone()),
                format!(
                    "producer selection item '{}' must match {STABLE_ID_PATTERN}",
                    selection.item
                ),
            ));
        }
        if !is_stable_id(&selection.recipe) {
            diagnostics.push(ThroughputDiagnostic::error(
                "invalid-producer-selection-recipe-id",
                format!("/producer_selections/{index}/recipe"),
                Some(selection.recipe.clone()),
                format!(
                    "selected recipe '{}' must match {STABLE_ID_PATTERN}",
                    selection.recipe
                ),
            ));
        }
        if !seen_selection_items.insert(selection.item.as_str()) {
            diagnostics.push(ThroughputDiagnostic::error(
                "duplicate-producer-selection",
                format!("/producer_selections/{index}/item"),
                Some(selection.item.clone()),
                format!(
                    "producer selection for item '{}' appears more than once",
                    selection.item
                ),
            ));
        }
        if seen_external_inputs.contains(selection.item.as_str()) {
            diagnostics.push(ThroughputDiagnostic::error(
                "external-input-producer-selection-conflict",
                format!("/producer_selections/{index}/item"),
                Some(selection.item.clone()),
                format!(
                    "item '{}' cannot be both an external input and have a selected producer",
                    selection.item
                ),
            ));
        }
    }

    if request.target.quantity <= 0 {
        diagnostics.push(ThroughputDiagnostic::error(
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
        diagnostics.push(ThroughputDiagnostic::error(
            "non-positive-target-duration",
            "/target/duration_ms",
            None,
            format!(
                "target duration_ms must be positive, found {}",
                request.target.duration_ms
            ),
        ));
    }

    diagnostics
}

impl ValidatedRecipeBook {
    pub fn calculate_throughput(
        &self,
        request: &RecipeThroughputRequest,
    ) -> RecipeThroughputReport {
        let request_diagnostics = validate_throughput_request(request);
        if !request_diagnostics.is_empty() {
            return RecipeThroughputReport::failure_many(request_diagnostics);
        }

        let selection_check = check_recipe_selections(self, request);
        if selection_check.status != RecipeSelectionCheckStatus::Ready {
            return RecipeThroughputReport::failure_with_producer_selection_groups(
                selection_check
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| ThroughputDiagnostic {
                        stage: diagnostic.stage,
                        severity: diagnostic.severity,
                        code: diagnostic.code,
                        path: diagnostic.path,
                        entity: diagnostic.entity,
                        message: diagnostic.message,
                    })
                    .collect(),
                selection_check.producer_selection_groups,
            );
        }

        let producer_selections = request
            .producer_selections
            .iter()
            .map(|selection| (selection.item.clone(), selection.recipe.clone()))
            .collect::<BTreeMap<_, _>>();
        let graph = match self.resolve_graph_with_options(
            &request.target.item,
            &request.external_inputs,
            &producer_selections,
        ) {
            Ok(graph) => graph,
            Err(error) => {
                let diagnostic = map_graph_error(&error);
                if let Some(group) = producer_selection_group(self, &error) {
                    return RecipeThroughputReport::failure_with_producer_selection_group(
                        diagnostic, group,
                    );
                }
                return RecipeThroughputReport::failure(diagnostic);
            }
        };

        let target_rate = match Rate::from_quantity_per_duration_ms(
            request.target.quantity,
            request.target.duration_ms,
        ) {
            Ok(rate) => rate,
            Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
        };

        if let Some(bootstrap_item_options) = cyclic::bootstrap_items(&graph) {
            return cyclic::calculate_cyclic_throughput(
                &graph,
                request,
                target_rate,
                bootstrap_item_options,
            );
        }

        let mut item_demands = BTreeMap::<String, Rate>::new();
        item_demands.insert(request.target.item.clone(), target_rate);

        let mut recipe_rate_by_id = BTreeMap::<String, RecipeRunRate>::new();
        let mut surplus_by_item = BTreeMap::<String, Rate>::new();

        for recipe in graph.recipes.iter().rev() {
            let (runs_per_second, limiting_outputs) =
                match required_runs_from_output_demands(recipe, &item_demands) {
                    Ok(value) => value,
                    Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
                };
            let work_seconds_per_second =
                match runs_per_second.checked_work_seconds_per_second(recipe.duration_ms) {
                    Ok(rate) => rate,
                    Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
                };
            let input_rates = match multiply_amounts(&recipe.inputs, runs_per_second) {
                Ok(rates) => rates,
                Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
            };
            let output_rates = match multiply_amounts(&recipe.outputs, runs_per_second) {
                Ok(rates) => rates,
                Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
            };

            if let Err(diagnostic) = add_input_demands(&mut item_demands, &input_rates) {
                return RecipeThroughputReport::failure(diagnostic);
            }

            if let Err(diagnostic) =
                add_surplus_rates(&mut surplus_by_item, &output_rates, &item_demands)
            {
                return RecipeThroughputReport::failure(diagnostic);
            }

            recipe_rate_by_id.insert(
                recipe.id.clone(),
                RecipeRunRate {
                    recipe: recipe.id.clone(),
                    facility: recipe.facility.clone(),
                    runs_per_second,
                    work_seconds_per_second,
                    limiting_outputs,
                    input_rates,
                    output_rates,
                },
            );
        }

        RecipeThroughputReport {
            success: true,
            target: Some(ItemRate {
                item: request.target.item.clone(),
                rate: target_rate,
            }),
            recipe_rates: graph
                .recipes
                .iter()
                .filter_map(|recipe| recipe_rate_by_id.remove(&recipe.id))
                .collect(),
            external_input_rates: graph
                .external_items
                .iter()
                .filter_map(|item| {
                    item_demands.get(item).map(|rate| ItemRate {
                        item: item.clone(),
                        rate: *rate,
                    })
                })
                .collect(),
            item_demand_rates: item_rates_from_map(&item_demands),
            surplus_rates: surplus_by_item
                .into_iter()
                .filter(|(_, rate)| !rate.is_zero())
                .map(|(item, rate)| ItemRate { item, rate })
                .collect(),
            bootstrap_item_options: Vec::new(),
            producer_selection_groups: Vec::new(),
            diagnostics: build_success_diagnostics(&request.target.item),
        }
    }
}

fn required_runs_from_output_demands(
    recipe: &Recipe,
    item_demands: &BTreeMap<String, Rate>,
) -> Result<(Rate, Vec<String>), ThroughputDiagnostic> {
    let mut runs_per_second = Rate::zero();
    let mut limiting_outputs = BTreeSet::<String>::new();

    for output in &recipe.outputs {
        let Some(demand_rate) = item_demands.get(&output.item) else {
            continue;
        };

        let required_runs = demand_rate.checked_div_i64(output.quantity)?;
        match required_runs.cmp(&runs_per_second) {
            std::cmp::Ordering::Greater => {
                runs_per_second = required_runs;
                limiting_outputs.clear();
                limiting_outputs.insert(output.item.clone());
            }
            std::cmp::Ordering::Equal => {
                limiting_outputs.insert(output.item.clone());
            }
            std::cmp::Ordering::Less => {}
        }
    }

    Ok((runs_per_second, limiting_outputs.into_iter().collect()))
}

fn multiply_amounts(
    amounts: &[ItemAmount],
    runs_per_second: Rate,
) -> Result<Vec<ItemRate>, ThroughputDiagnostic> {
    let mut rates = Vec::with_capacity(amounts.len());

    for amount in amounts {
        rates.push(ItemRate {
            item: amount.item.clone(),
            rate: runs_per_second.checked_mul_i64(amount.quantity)?,
        });
    }

    rates.sort_by(|left, right| left.item.cmp(&right.item));
    Ok(rates)
}

fn add_input_demands(
    item_demands: &mut BTreeMap<String, Rate>,
    input_rates: &[ItemRate],
) -> Result<(), ThroughputDiagnostic> {
    for input_rate in input_rates {
        let current_rate = item_demands
            .get(&input_rate.item)
            .copied()
            .unwrap_or_else(Rate::zero);
        let next_rate = current_rate.checked_add(input_rate.rate)?;
        item_demands.insert(input_rate.item.clone(), next_rate);
    }

    Ok(())
}

fn add_surplus_rates(
    surplus_by_item: &mut BTreeMap<String, Rate>,
    output_rates: &[ItemRate],
    item_demands: &BTreeMap<String, Rate>,
) -> Result<(), ThroughputDiagnostic> {
    for output_rate in output_rates {
        let demanded_rate = item_demands
            .get(&output_rate.item)
            .copied()
            .unwrap_or_else(Rate::zero);
        let surplus_rate = output_rate.rate.checked_sub(demanded_rate)?;
        let current_surplus = surplus_by_item
            .get(&output_rate.item)
            .copied()
            .unwrap_or_else(Rate::zero);
        let next_surplus = current_surplus.checked_add(surplus_rate)?;
        surplus_by_item.insert(output_rate.item.clone(), next_surplus);
    }

    Ok(())
}

fn item_rates_from_map(rates_by_item: &BTreeMap<String, Rate>) -> Vec<ItemRate> {
    rates_by_item
        .iter()
        .map(|(item, rate)| ItemRate {
            item: item.clone(),
            rate: *rate,
        })
        .collect()
}

fn producer_selection_group(
    book: &ValidatedRecipeBook,
    error: &RecipeGraphError,
) -> Option<RecipeProducerSelectionGroup> {
    let RecipeGraphError::AmbiguousProducer { item, recipes } = error else {
        return None;
    };
    Some(RecipeProducerSelectionGroup {
        item: item.clone(),
        options: recipes
            .iter()
            .map(|recipe| {
                book.index()
                    .recipe(recipe)
                    .expect("ambiguous producer IDs come from the validated recipe index")
                    .clone()
            })
            .collect(),
    })
}

fn map_graph_error(error: &RecipeGraphError) -> ThroughputDiagnostic {
    match error {
        RecipeGraphError::InvalidTargetId { target_item } => ThroughputDiagnostic::error(
            "invalid-target-id",
            "/target/item",
            Some(target_item.clone()),
            format!("target item '{target_item}' must match {STABLE_ID_PATTERN}"),
        ),
        RecipeGraphError::UnknownTargetItem { target_item } => ThroughputDiagnostic::error(
            "unknown-target-item",
            "/target/item",
            Some(target_item.clone()),
            format!("target item '{target_item}' is neither external nor recipe-produced"),
        ),
        RecipeGraphError::UnknownExternalInput { item } => ThroughputDiagnostic::error(
            "unknown-external-input",
            "/external_inputs",
            Some(item.clone()),
            format!("external input item '{item}' is neither external nor recipe-produced"),
        ),
        RecipeGraphError::UnknownProducerSelectionItem { item } => ThroughputDiagnostic::error(
            "unknown-producer-selection-item",
            "/producer_selections",
            Some(item.clone()),
            format!("producer selection item '{item}' is not produced by any recipe"),
        ),
        RecipeGraphError::UnknownSelectedRecipe { item, recipe } => ThroughputDiagnostic::error(
            "unknown-selected-recipe",
            "/producer_selections",
            Some(recipe.clone()),
            format!("producer selection for item '{item}' references unknown recipe '{recipe}'"),
        ),
        RecipeGraphError::SelectedRecipeDoesNotProduceItem { item, recipe } => {
            ThroughputDiagnostic::error(
                "selected-recipe-output-mismatch",
                "/producer_selections",
                Some(recipe.clone()),
                format!("selected recipe '{recipe}' does not produce item '{item}'"),
            )
        }
        RecipeGraphError::AmbiguousProducer { item, recipes } => ThroughputDiagnostic::error(
            "ambiguous-recipe-producer",
            "/recipes",
            Some(item.clone()),
            format!(
                "item '{item}' is produced by multiple recipes: {}",
                recipes.join(", ")
            ),
        ),
    }
}

fn build_success_diagnostics(target_item: &str) -> Vec<ThroughputDiagnostic> {
    vec![ThroughputDiagnostic::info(
        "throughput-calculated",
        "/",
        Some(target_item.to_string()),
        format!("throughput was calculated for target item '{target_item}'"),
    )]
}
