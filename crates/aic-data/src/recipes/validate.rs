use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::recipes::{
    ItemAmount, Recipe, RecipeBook,
    id::{STABLE_ID_PATTERN, is_stable_id},
    index::RecipeAnalysis,
};

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ValidationReport {
    pub valid: bool,
    pub diagnostics: Vec<ValidationDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ValidationDiagnostic {
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

pub fn validate_recipe_book(book: &RecipeBook) -> ValidationReport {
    let mut validator = Validator::default();
    validator.validate(book);
    validator.into_report()
}

impl RecipeBook {
    pub fn validate(&self) -> ValidationReport {
        validate_recipe_book(self)
    }
}

#[derive(Default)]
struct Validator {
    diagnostics: Vec<ValidationDiagnostic>,
}

impl Validator {
    fn validate(&mut self, book: &RecipeBook) {
        self.validate_schema_version(book);
        self.validate_external_items(book);
        self.validate_recipes(book);

        let analysis = RecipeAnalysis::from_raw(book);
        self.validate_external_output_overlap(book, &analysis);
        self.validate_input_links(book, &analysis);
        self.validate_ambiguous_outputs(&analysis);
        self.validate_cycles(book, &analysis);
    }

    fn into_report(self) -> ValidationReport {
        ValidationReport {
            valid: self.diagnostics.is_empty(),
            diagnostics: self.diagnostics,
        }
    }

    fn validate_schema_version(&mut self, book: &RecipeBook) {
        if book.schema_version != SUPPORTED_SCHEMA_VERSION {
            self.push(
                "unsupported-schema-version",
                "/schema_version",
                format!(
                    "schema_version must be {SUPPORTED_SCHEMA_VERSION}, found {}",
                    book.schema_version
                ),
            );
        }
    }

    fn validate_external_items(&mut self, book: &RecipeBook) {
        let mut seen = HashSet::new();

        for (index, item) in book.external_items.iter().enumerate() {
            let path = format!("/external_items/{index}");
            self.validate_id("invalid-item-id", &path, item);

            if !seen.insert(item) {
                self.push(
                    "duplicate-external-item",
                    path,
                    format!("external item '{item}' appears more than once"),
                );
            }
        }
    }

    fn validate_recipes(&mut self, book: &RecipeBook) {
        let mut seen_recipe_ids = HashSet::new();

        for (recipe_index, recipe) in book.recipes.iter().enumerate() {
            let recipe_path = format!("/recipes/{recipe_index}");

            self.validate_id("invalid-recipe-id", format!("{recipe_path}/id"), &recipe.id);
            self.validate_id(
                "invalid-facility-id",
                format!("{recipe_path}/facility"),
                &recipe.facility,
            );

            if !seen_recipe_ids.insert(&recipe.id) {
                self.push(
                    "duplicate-recipe-id",
                    format!("{recipe_path}/id"),
                    format!("recipe id '{}' appears more than once", recipe.id),
                );
            }

            if recipe.outputs.is_empty() {
                self.push(
                    "empty-outputs",
                    format!("{recipe_path}/outputs"),
                    format!("recipe '{}' must produce at least one item", recipe.id),
                );
            }

            if recipe.duration_ms <= 0 {
                self.push(
                    "non-positive-duration",
                    format!("{recipe_path}/duration_ms"),
                    format!(
                        "recipe '{}' duration_ms must be positive, found {}",
                        recipe.id, recipe.duration_ms
                    ),
                );
            }

            self.validate_item_amounts(&recipe.inputs, format!("{recipe_path}/inputs"));
            self.validate_item_amounts(&recipe.outputs, format!("{recipe_path}/outputs"));
        }
    }

    fn validate_item_amounts(&mut self, amounts: &[ItemAmount], path: String) {
        for (amount_index, amount) in amounts.iter().enumerate() {
            let amount_path = format!("{path}/{amount_index}");
            self.validate_id(
                "invalid-item-id",
                format!("{amount_path}/item"),
                &amount.item,
            );

            if amount.quantity <= 0 {
                self.push(
                    "non-positive-quantity",
                    format!("{amount_path}/quantity"),
                    format!(
                        "item '{}' quantity must be positive, found {}",
                        amount.item, amount.quantity
                    ),
                );
            }
        }
    }

    fn validate_external_output_overlap(&mut self, book: &RecipeBook, analysis: &RecipeAnalysis) {
        for (external_index, external_item) in book.external_items.iter().enumerate() {
            if analysis
                .output_producers
                .contains_key(external_item.as_str())
            {
                self.push(
                    "external-item-produced",
                    format!("/external_items/{external_index}"),
                    format!(
                        "item '{external_item}' is both externally supplied and recipe-produced"
                    ),
                );
            }
        }
    }

    fn validate_input_links(&mut self, book: &RecipeBook, analysis: &RecipeAnalysis) {
        for (recipe_index, recipe) in book.recipes.iter().enumerate() {
            for (input_index, input) in recipe.inputs.iter().enumerate() {
                let item = input.item.as_str();
                if !analysis.external_items.contains(item)
                    && !analysis.output_producers.contains_key(item)
                {
                    self.push(
                        "missing-input-link",
                        format!("/recipes/{recipe_index}/inputs/{input_index}/item"),
                        format!(
                            "input item '{}' for recipe '{}' is neither external nor recipe-produced",
                            input.item, recipe.id
                        ),
                    );
                }
            }
        }
    }

    fn validate_ambiguous_outputs(&mut self, analysis: &RecipeAnalysis) {
        for (item, producers) in &analysis.output_producers {
            let producer_ids = producers
                .iter()
                .map(|recipe| recipe.id.as_str())
                .collect::<HashSet<_>>();

            if producer_ids.len() > 1 {
                let mut sorted_producer_ids = producer_ids.into_iter().collect::<Vec<_>>();
                sorted_producer_ids.sort_unstable();

                self.push(
                    "ambiguous-output-producer",
                    "/recipes",
                    format!(
                        "item '{item}' is produced by multiple recipes: {}",
                        sorted_producer_ids.join(", ")
                    ),
                );
            }
        }
    }

    fn validate_cycles(&mut self, book: &RecipeBook, analysis: &RecipeAnalysis) {
        let recipe_by_id = book
            .recipes
            .iter()
            .map(|recipe| (recipe.id.as_str(), recipe))
            .collect::<HashMap<_, _>>();
        let mut states = HashMap::<&str, VisitState>::new();
        let mut stack = Vec::<&str>::new();

        for recipe in &book.recipes {
            self.visit_recipe_for_cycles(
                recipe.id.as_str(),
                &recipe_by_id,
                &analysis.output_producers,
                &mut states,
                &mut stack,
            );
        }
    }

    fn visit_recipe_for_cycles<'a>(
        &mut self,
        recipe_id: &'a str,
        recipe_by_id: &HashMap<&'a str, &'a Recipe>,
        output_producers: &HashMap<&'a str, Vec<&'a Recipe>>,
        states: &mut HashMap<&'a str, VisitState>,
        stack: &mut Vec<&'a str>,
    ) {
        match states.get(recipe_id) {
            Some(VisitState::Done) => return,
            Some(VisitState::Visiting) => {
                self.report_cycle(recipe_id, stack);
                return;
            }
            None => {}
        }

        states.insert(recipe_id, VisitState::Visiting);
        stack.push(recipe_id);

        let Some(recipe) = recipe_by_id.get(recipe_id) else {
            stack.pop();
            states.insert(recipe_id, VisitState::Done);
            return;
        };

        for input in &recipe.inputs {
            let Some(producers) = output_producers.get(input.item.as_str()) else {
                continue;
            };

            if producers.len() != 1 {
                continue;
            }

            self.visit_recipe_for_cycles(
                producers[0].id.as_str(),
                recipe_by_id,
                output_producers,
                states,
                stack,
            );
        }

        stack.pop();
        states.insert(recipe_id, VisitState::Done);
    }

    fn report_cycle(&mut self, repeated_recipe_id: &str, stack: &[&str]) {
        let start_index = stack
            .iter()
            .position(|recipe_id| *recipe_id == repeated_recipe_id)
            .unwrap_or(0);
        let mut cycle = stack[start_index..].to_vec();
        cycle.push(repeated_recipe_id);

        self.push(
            "recipe-cycle",
            "/recipes",
            format!("cyclic recipe dependency detected: {}", cycle.join(" -> ")),
        );
    }

    fn validate_id(&mut self, code: &'static str, path: impl Into<String>, value: &str) {
        if !is_stable_id(value) {
            self.push(
                code,
                path,
                format!("'{value}' must match {STABLE_ID_PATTERN}"),
            );
        }
    }

    fn push(&mut self, code: &'static str, path: impl Into<String>, message: impl Into<String>) {
        self.diagnostics.push(ValidationDiagnostic {
            code,
            path: path.into(),
            message: message.into(),
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Done,
}
