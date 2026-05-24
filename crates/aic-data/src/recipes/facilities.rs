use std::collections::BTreeMap;

use serde::Serialize;

use crate::recipes::{Rate, RecipeRunRate, RecipeThroughputReport};

const STAGE: &str = "recipe-facilities";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityRequirementReport {
    pub success: bool,
    pub recipe_requirements: Vec<RecipeFacilityRequirement>,
    pub facility_summaries: Vec<FacilityRequirementSummary>,
    pub diagnostics: Vec<FacilityRequirementDiagnostic>,
}

impl FacilityRequirementReport {
    fn success(
        recipe_requirements: Vec<RecipeFacilityRequirement>,
        facility_summaries: Vec<FacilityRequirementSummary>,
    ) -> Self {
        Self {
            success: true,
            recipe_requirements,
            facility_summaries,
            diagnostics: vec![FacilityRequirementDiagnostic::info(
                "facility-requirements-calculated",
                "/",
                None,
                "facility requirements were calculated",
            )],
        }
    }

    fn failure(diagnostic: FacilityRequirementDiagnostic) -> Self {
        Self {
            success: false,
            recipe_requirements: Vec::new(),
            facility_summaries: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeFacilityRequirement {
    pub recipe: String,
    pub facility: String,
    pub work_seconds_per_second: Rate,
    pub required_facilities: i64,
    pub unused_capacity: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityRequirementSummary {
    pub facility: String,
    pub required_facilities: i64,
    pub unused_capacity: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityRequirementDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl FacilityRequirementDiagnostic {
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

pub fn calculate_facility_requirements(
    throughput: &RecipeThroughputReport,
) -> FacilityRequirementReport {
    if !throughput.success {
        return FacilityRequirementReport::failure(FacilityRequirementDiagnostic::error(
            "upstream-throughput-failed",
            "/",
            None,
            "facility requirements require a successful throughput report",
        ));
    }

    let mut recipe_requirements = Vec::with_capacity(throughput.recipe_rates.len());
    for recipe_rate in &throughput.recipe_rates {
        let requirement = match requirement_from_recipe_rate(recipe_rate) {
            Ok(requirement) => requirement,
            Err(diagnostic) => return FacilityRequirementReport::failure(diagnostic),
        };
        recipe_requirements.push(requirement);
    }

    let facility_summaries = match summarize_by_facility(&recipe_requirements) {
        Ok(summaries) => summaries,
        Err(diagnostic) => return FacilityRequirementReport::failure(diagnostic),
    };

    FacilityRequirementReport::success(recipe_requirements, facility_summaries)
}

fn requirement_from_recipe_rate(
    recipe_rate: &RecipeRunRate,
) -> Result<RecipeFacilityRequirement, FacilityRequirementDiagnostic> {
    let required_facilities = ceil_rate(recipe_rate.work_seconds_per_second)?;
    let unused_capacity =
        unused_capacity(required_facilities, recipe_rate.work_seconds_per_second)?;

    Ok(RecipeFacilityRequirement {
        recipe: recipe_rate.recipe.clone(),
        facility: recipe_rate.facility.clone(),
        work_seconds_per_second: recipe_rate.work_seconds_per_second,
        required_facilities,
        unused_capacity,
    })
}

fn summarize_by_facility(
    recipe_requirements: &[RecipeFacilityRequirement],
) -> Result<Vec<FacilityRequirementSummary>, FacilityRequirementDiagnostic> {
    let mut summaries = BTreeMap::<String, FacilityRequirementSummary>::new();

    for requirement in recipe_requirements {
        let summary = summaries
            .entry(requirement.facility.clone())
            .or_insert_with(|| FacilityRequirementSummary {
                facility: requirement.facility.clone(),
                required_facilities: 0,
                unused_capacity: Rate::zero(),
            });

        summary.required_facilities = summary
            .required_facilities
            .checked_add(requirement.required_facilities)
            .ok_or_else(arithmetic_overflow)?;
        summary.unused_capacity = summary
            .unused_capacity
            .checked_add(requirement.unused_capacity)
            .map_err(map_throughput_arithmetic)?;
    }

    Ok(summaries.into_values().collect())
}

fn ceil_rate(rate: Rate) -> Result<i64, FacilityRequirementDiagnostic> {
    if rate.is_zero() {
        return Ok(0);
    }

    let base = rate.numerator / rate.denominator;
    let remainder = rate.numerator % rate.denominator;

    if remainder == 0 {
        Ok(base)
    } else {
        base.checked_add(1).ok_or_else(arithmetic_overflow)
    }
}

fn unused_capacity(
    required_facilities: i64,
    work_seconds_per_second: Rate,
) -> Result<Rate, FacilityRequirementDiagnostic> {
    Rate {
        numerator: required_facilities,
        denominator: 1,
    }
    .checked_sub(work_seconds_per_second)
    .map_err(map_throughput_arithmetic)
}

fn map_throughput_arithmetic(
    _diagnostic: crate::recipes::ThroughputDiagnostic,
) -> FacilityRequirementDiagnostic {
    arithmetic_overflow()
}

fn arithmetic_overflow() -> FacilityRequirementDiagnostic {
    FacilityRequirementDiagnostic::error(
        "arithmetic-overflow",
        "/",
        None,
        "arithmetic overflow while calculating facility requirements",
    )
}
