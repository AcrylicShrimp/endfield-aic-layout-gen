use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{ExactObjectiveValue, IntegratedLayoutReport, exact};
use super::coordinate_partition::{invalid_input, millis, model_scale, prepare_target_input};
use super::rotation_partition::diagnose_cumulative_facility_rotation_partitions;
use super::{ExactDimensionCaseOutcome, PartitionCaseModelScale};

pub const PHASE2_REFERENCE_ABLATION_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Phase2ReferenceAblationKind {
    Placements,
    PlacementsAndFacilityPorts,
    PlacementsAndAllTerminals,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Phase2ReferenceAblationCaseReport {
    pub kind: Phase2ReferenceAblationKind,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub model_scale: PartitionCaseModelScale,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Phase2ReferenceAblationReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub reference_search_ms: u64,
    pub reference_objective: Option<ExactObjectiveValue>,
    pub case_search_budget_ms: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<Phase2ReferenceAblationCaseReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_phase2_reference_ablation(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    fixed_width: i32,
    fixed_height: i32,
    fixed_x: i32,
    fixed_y: i32,
    port_assignment_index: usize,
    prefix_search_budget: Duration,
    reference_search_budget: Duration,
    case_search_budget: Duration,
) -> Result<Phase2ReferenceAblationReport, IntegratedLayoutReport> {
    if case_search_budget.is_zero() {
        return Err(invalid_input(
            "/case_search_budget",
            "reference ablation requires a positive per-case budget",
        ));
    }
    let reference_run = diagnose_cumulative_facility_rotation_partitions(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        target_phase_index,
        fixed_width,
        fixed_height,
        fixed_x,
        fixed_y,
        port_assignment_index,
        prefix_search_budget,
        reference_search_budget,
    )?;
    let reference = reference_run.selected_witness.ok_or_else(|| {
        invalid_input(
            "/reference",
            "rotation partition did not reproduce a validated reference witness",
        )
    })?;
    let reference_exact = reference
        .exact
        .as_ref()
        .expect("validated reference has exact metrics");
    let reference_search_ms = reference_exact.search_ms;
    let reference_objective = reference_exact.objective;
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    let input = prepare_target_input(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        &growth,
        target_phase_index,
    )?;
    let bounds = reference
        .bounds
        .as_ref()
        .expect("reference has exact bounds");
    let fixed_dimensions = exact::shared_layer::FixedUsedDimensions {
        width: i32::try_from(bounds.width).expect("reference width fits i32"),
        height: i32::try_from(bounds.height).expect("reference height fits i32"),
    };
    let started = Instant::now();
    let kinds = [
        (
            Phase2ReferenceAblationKind::Placements,
            exact::shared_layer::ReferenceAblationFixation::Placements,
        ),
        (
            Phase2ReferenceAblationKind::PlacementsAndFacilityPorts,
            exact::shared_layer::ReferenceAblationFixation::PlacementsAndFacilityPorts,
        ),
        (
            Phase2ReferenceAblationKind::PlacementsAndAllTerminals,
            exact::shared_layer::ReferenceAblationFixation::PlacementsAndAllTerminals,
        ),
    ];
    let mut completed = Vec::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for (kind, fixation) in kinds {
            let input = input.clone();
            let reference = &reference;
            handles.push((
                kind,
                scope.spawn(move || {
                    exact::shared_layer::solve_factored_endpoints_fixed_dimensions_reference_ablation(
                        input,
                        logistics_components,
                        Some(case_search_budget),
                        fixed_dimensions,
                        reference,
                        fixation,
                    )
                }),
            ));
        }
        for (kind, handle) in handles {
            completed.push((
                kind,
                handle.join().expect("reference ablation worker panicked"),
            ));
        }
    });
    let cases = completed
        .into_iter()
        .map(|(kind, layout)| {
            let outcome = super::coordinate_partition::classify_outcome(&layout);
            let exact = layout
                .exact
                .as_ref()
                .expect("executed reference ablation has exact metrics");
            Phase2ReferenceAblationCaseReport {
                kind,
                outcome,
                construction_ms: exact.construction_ms,
                search_ms: exact.search_ms,
                first_incumbent_ms: exact.first_incumbent_ms,
                model_scale: model_scale(exact),
                layout,
            }
        })
        .collect();

    Ok(Phase2ReferenceAblationReport {
        schema_version: PHASE2_REFERENCE_ABLATION_SCHEMA_VERSION,
        target_phase_index,
        reference_search_ms,
        reference_objective,
        case_search_budget_ms: millis(case_search_budget),
        outer_wall_ms: millis(started.elapsed()),
        cases,
        diagnostic_only: true,
    })
}
