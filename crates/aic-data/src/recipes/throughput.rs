use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::recipes::{
    ItemAmount, Recipe, RecipeGraphError, ValidatedRecipeBook,
    id::{STABLE_ID_PATTERN, is_stable_id},
};

const STAGE: &str = "recipe-throughput";

pub const SUPPORTED_THROUGHPUT_REQUEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecipeThroughputRequest {
    pub schema_version: u32,
    pub target: ThroughputTarget,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ThroughputTarget {
    pub item: String,
    pub quantity: i64,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Rate {
    pub numerator: i64,
    pub denominator: i64,
}

impl Rate {
    pub fn zero() -> Self {
        Self {
            numerator: 0,
            denominator: 1,
        }
    }

    pub fn from_quantity_per_duration_ms(
        quantity: i64,
        duration_ms: i64,
    ) -> Result<Self, ThroughputDiagnostic> {
        if quantity <= 0 {
            return Err(ThroughputDiagnostic::error(
                "non-positive-target-quantity",
                "/target/quantity",
                None,
                format!("target quantity must be positive, found {quantity}"),
            ));
        }

        if duration_ms <= 0 {
            return Err(ThroughputDiagnostic::error(
                "non-positive-target-duration",
                "/target/duration_ms",
                None,
                format!("target duration_ms must be positive, found {duration_ms}"),
            ));
        }

        let numerator = checked_mul_i128(quantity as i128, 1000, "rate-normalization")?;
        Self::normalize(numerator, duration_ms as i128, "rate-normalization")
    }

    pub fn checked_add(self, other: Self) -> Result<Self, ThroughputDiagnostic> {
        let left = checked_mul_i128(
            self.numerator as i128,
            other.denominator as i128,
            "rate-addition",
        )?;
        let right = checked_mul_i128(
            other.numerator as i128,
            self.denominator as i128,
            "rate-addition",
        )?;
        let numerator = checked_add_i128(left, right, "rate-addition")?;
        let denominator = checked_mul_i128(
            self.denominator as i128,
            other.denominator as i128,
            "rate-addition",
        )?;

        Self::normalize(numerator, denominator, "rate-addition")
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, ThroughputDiagnostic> {
        let left = checked_mul_i128(
            self.numerator as i128,
            other.denominator as i128,
            "rate-subtraction",
        )?;
        let right = checked_mul_i128(
            other.numerator as i128,
            self.denominator as i128,
            "rate-subtraction",
        )?;
        let numerator = checked_sub_i128(left, right, "rate-subtraction")?;
        let denominator = checked_mul_i128(
            self.denominator as i128,
            other.denominator as i128,
            "rate-subtraction",
        )?;

        Self::normalize(numerator, denominator, "rate-subtraction")
    }

    pub fn checked_mul_i64(self, rhs: i64) -> Result<Self, ThroughputDiagnostic> {
        if rhs < 0 {
            return Err(arithmetic_overflow("rate-multiplication"));
        }

        let numerator =
            checked_mul_i128(self.numerator as i128, rhs as i128, "rate-multiplication")?;
        Self::normalize(numerator, self.denominator as i128, "rate-multiplication")
    }

    pub fn checked_div_i64(self, rhs: i64) -> Result<Self, ThroughputDiagnostic> {
        if rhs <= 0 {
            return Err(arithmetic_overflow("rate-division"));
        }

        let denominator = checked_mul_i128(self.denominator as i128, rhs as i128, "rate-division")?;
        Self::normalize(self.numerator as i128, denominator, "rate-division")
    }

    pub fn checked_work_seconds_per_second(
        self,
        duration_ms: i64,
    ) -> Result<Self, ThroughputDiagnostic> {
        if duration_ms <= 0 {
            return Err(arithmetic_overflow("work-rate-calculation"));
        }

        let numerator = checked_mul_i128(
            self.numerator as i128,
            duration_ms as i128,
            "work-rate-calculation",
        )?;
        let denominator =
            checked_mul_i128(self.denominator as i128, 1000, "work-rate-calculation")?;
        Self::normalize(numerator, denominator, "work-rate-calculation")
    }

    pub fn max(self, other: Self) -> Self {
        if self >= other { self } else { other }
    }

    pub fn is_zero(self) -> bool {
        self.numerator == 0
    }

    fn normalize(
        numerator: i128,
        denominator: i128,
        operation: &'static str,
    ) -> Result<Self, ThroughputDiagnostic> {
        if numerator < 0 || denominator <= 0 {
            return Err(arithmetic_overflow(operation));
        }

        if numerator == 0 {
            return Ok(Self::zero());
        }

        let divisor = gcd(numerator, denominator);
        let numerator = numerator / divisor;
        let denominator = denominator / divisor;

        if numerator > i64::MAX as i128 || denominator > i64::MAX as i128 {
            return Err(arithmetic_overflow(operation));
        }

        Ok(Self {
            numerator: numerator as i64,
            denominator: denominator as i64,
        })
    }
}

impl PartialOrd for Rate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Rate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let left = (self.numerator as i128) * (other.denominator as i128);
        let right = (other.numerator as i128) * (self.denominator as i128);
        left.cmp(&right)
    }
}

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
pub struct RecipeThroughputReport {
    pub success: bool,
    pub target: Option<ItemRate>,
    pub recipe_rates: Vec<RecipeRunRate>,
    pub external_input_rates: Vec<ItemRate>,
    pub item_demand_rates: Vec<ItemRate>,
    pub surplus_rates: Vec<ItemRate>,
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
            diagnostics,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ThroughputDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl ThroughputDiagnostic {
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

        let graph = match self.resolve_graph(&request.target.item) {
            Ok(graph) => graph,
            Err(error) => return RecipeThroughputReport::failure(map_graph_error(error)),
        };

        let target_rate = match Rate::from_quantity_per_duration_ms(
            request.target.quantity,
            request.target.duration_ms,
        ) {
            Ok(rate) => rate,
            Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
        };

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

fn map_graph_error(error: RecipeGraphError) -> ThroughputDiagnostic {
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

fn checked_add_i128(
    left: i128,
    right: i128,
    operation: &'static str,
) -> Result<i128, ThroughputDiagnostic> {
    left.checked_add(right)
        .ok_or_else(|| arithmetic_overflow(operation))
}

fn checked_sub_i128(
    left: i128,
    right: i128,
    operation: &'static str,
) -> Result<i128, ThroughputDiagnostic> {
    left.checked_sub(right)
        .ok_or_else(|| arithmetic_overflow(operation))
}

fn checked_mul_i128(
    left: i128,
    right: i128,
    operation: &'static str,
) -> Result<i128, ThroughputDiagnostic> {
    left.checked_mul(right)
        .ok_or_else(|| arithmetic_overflow(operation))
}

fn arithmetic_overflow(operation: &'static str) -> ThroughputDiagnostic {
    ThroughputDiagnostic::error(
        "arithmetic-overflow",
        "/",
        None,
        format!("arithmetic overflow while performing {operation}"),
    )
}

fn gcd(mut left: i128, mut right: i128) -> i128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }

    left
}
