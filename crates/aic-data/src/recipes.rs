use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::BufReader,
    path::Path,
};

use serde::{Deserialize, Serialize};

pub const SUPPORTED_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecipeBook {
    pub schema_version: u32,
    pub external_items: Vec<String>,
    pub recipes: Vec<Recipe>,
}

impl RecipeBook {
    pub fn validate(&self) -> ValidationReport {
        let mut validator = Validator::default();
        validator.validate(self);
        validator.into_report()
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub id: String,
    pub facility: String,
    pub inputs: Vec<ItemAmount>,
    pub outputs: Vec<ItemAmount>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ItemAmount {
    pub item: String,
    pub quantity: i64,
}

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

pub fn load_recipe_book(path: impl AsRef<Path>) -> Result<RecipeBook, LoadRecipeBookError> {
    let path = path.as_ref();
    let file = File::open(path).map_err(|source| LoadRecipeBookError::Open {
        path: path.display().to_string(),
        source,
    })?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(|source| LoadRecipeBookError::Parse {
        path: path.display().to_string(),
        source,
    })
}

#[derive(Debug)]
pub enum LoadRecipeBookError {
    Open {
        path: String,
        source: std::io::Error,
    },
    Parse {
        path: String,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for LoadRecipeBookError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open { path, source } => {
                write!(formatter, "failed to open recipe file '{path}': {source}")
            }
            Self::Parse { path, source } => {
                write!(formatter, "failed to parse recipe file '{path}': {source}")
            }
        }
    }
}

impl std::error::Error for LoadRecipeBookError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } => Some(source),
            Self::Parse { source, .. } => Some(source),
        }
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

        let output_producers = self.build_output_producers(book);
        self.validate_external_output_overlap(book, &output_producers);
        self.validate_input_links(book, &output_producers);
        self.validate_ambiguous_outputs(&output_producers);
        self.validate_cycles(book, &output_producers);
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

    fn build_output_producers<'a>(
        &mut self,
        book: &'a RecipeBook,
    ) -> HashMap<&'a str, Vec<&'a Recipe>> {
        let mut output_producers = HashMap::<&str, Vec<&Recipe>>::new();

        for recipe in &book.recipes {
            for output in &recipe.outputs {
                output_producers
                    .entry(output.item.as_str())
                    .or_default()
                    .push(recipe);
            }
        }

        output_producers
    }

    fn validate_external_output_overlap(
        &mut self,
        book: &RecipeBook,
        output_producers: &HashMap<&str, Vec<&Recipe>>,
    ) {
        for (external_index, external_item) in book.external_items.iter().enumerate() {
            if output_producers.contains_key(external_item.as_str()) {
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

    fn validate_input_links(
        &mut self,
        book: &RecipeBook,
        output_producers: &HashMap<&str, Vec<&Recipe>>,
    ) {
        let external_items = book
            .external_items
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();

        for (recipe_index, recipe) in book.recipes.iter().enumerate() {
            for (input_index, input) in recipe.inputs.iter().enumerate() {
                let item = input.item.as_str();
                if !external_items.contains(item) && !output_producers.contains_key(item) {
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

    fn validate_ambiguous_outputs(&mut self, output_producers: &HashMap<&str, Vec<&Recipe>>) {
        for (item, producers) in output_producers {
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

    fn validate_cycles(
        &mut self,
        book: &RecipeBook,
        output_producers: &HashMap<&str, Vec<&Recipe>>,
    ) {
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
                output_producers,
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
        if !is_kebab_case_id(value) {
            self.push(
                code,
                path,
                format!("'{value}' must match ^[a-z0-9]+(-[a-z0-9]+)*$"),
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

fn is_kebab_case_id(value: &str) -> bool {
    if value.is_empty() || value.starts_with('-') || value.ends_with('-') {
        return false;
    }

    let mut previous_was_hyphen = false;

    for byte in value.bytes() {
        let is_segment_char = byte.is_ascii_lowercase() || byte.is_ascii_digit();
        let is_hyphen = byte == b'-';

        if !is_segment_char && !is_hyphen {
            return false;
        }

        if is_hyphen && previous_was_hyphen {
            return false;
        }

        previous_was_hyphen = is_hyphen;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_book() -> RecipeBook {
        RecipeBook {
            schema_version: 1,
            external_items: vec!["originium-ore".to_string()],
            recipes: vec![Recipe {
                id: "grind-originium-powder".to_string(),
                facility: "grinding-unit".to_string(),
                inputs: vec![ItemAmount {
                    item: "originium-ore".to_string(),
                    quantity: 1,
                }],
                outputs: vec![ItemAmount {
                    item: "originium-powder".to_string(),
                    quantity: 1,
                }],
                duration_ms: 2000,
            }],
        }
    }

    #[test]
    fn accepts_valid_recipe_book() {
        let report = valid_book().validate();

        assert!(report.valid);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn rejects_invalid_ids() {
        let mut book = valid_book();
        book.recipes[0].id = "grind_originium_powder".to_string();
        book.recipes[0].facility = "Grinding Unit".to_string();
        book.recipes[0].outputs[0].item = "originium--powder".to_string();

        let report = book.validate();

        assert_codes(
            &report,
            &[
                "invalid-recipe-id",
                "invalid-facility-id",
                "invalid-item-id",
            ],
        );
    }

    #[test]
    fn rejects_missing_input_links() {
        let mut book = valid_book();
        book.recipes[0].inputs[0].item = "missing-item".to_string();

        let report = book.validate();

        assert_codes(&report, &["missing-input-link"]);
    }

    #[test]
    fn rejects_ambiguous_output_producers() {
        let mut book = valid_book();
        book.recipes.push(Recipe {
            id: "alternate-originium-powder".to_string(),
            facility: "grinding-unit".to_string(),
            inputs: vec![ItemAmount {
                item: "originium-ore".to_string(),
                quantity: 2,
            }],
            outputs: vec![ItemAmount {
                item: "originium-powder".to_string(),
                quantity: 3,
            }],
            duration_ms: 3000,
        });

        let report = book.validate();

        assert_codes(&report, &["ambiguous-output-producer"]);
    }

    #[test]
    fn rejects_cycles() {
        let book = RecipeBook {
            schema_version: 1,
            external_items: vec![],
            recipes: vec![
                Recipe {
                    id: "make-a".to_string(),
                    facility: "assembler".to_string(),
                    inputs: vec![ItemAmount {
                        item: "item-b".to_string(),
                        quantity: 1,
                    }],
                    outputs: vec![ItemAmount {
                        item: "item-a".to_string(),
                        quantity: 1,
                    }],
                    duration_ms: 1000,
                },
                Recipe {
                    id: "make-b".to_string(),
                    facility: "assembler".to_string(),
                    inputs: vec![ItemAmount {
                        item: "item-a".to_string(),
                        quantity: 1,
                    }],
                    outputs: vec![ItemAmount {
                        item: "item-b".to_string(),
                        quantity: 1,
                    }],
                    duration_ms: 1000,
                },
            ],
        };

        let report = book.validate();

        assert_codes(&report, &["recipe-cycle"]);
    }

    #[test]
    fn rejects_non_positive_numbers() {
        let mut book = valid_book();
        book.recipes[0].inputs[0].quantity = 0;
        book.recipes[0].duration_ms = -1;

        let report = book.validate();

        assert_codes(&report, &["non-positive-quantity", "non-positive-duration"]);
    }

    fn assert_codes(report: &ValidationReport, expected_codes: &[&str]) {
        assert!(!report.valid);

        let codes = report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect::<Vec<_>>();

        for expected_code in expected_codes {
            assert!(
                codes.contains(expected_code),
                "expected diagnostic code '{expected_code}', got {codes:?}"
            );
        }
    }
}
