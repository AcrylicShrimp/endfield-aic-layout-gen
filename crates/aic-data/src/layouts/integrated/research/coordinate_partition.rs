use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;
use crate::research::ModelComplexityMetrics;

use super::super::{
    ExactModelMetrics, IntegratedLayoutDiagnostic, IntegratedLayoutReport, IntegratedLayoutStatus,
    ModelInput, exact, harness, prepare_exact_model,
};
use super::{
    ExactDimensionCaseOutcome, ExactUsedDimensionCandidate,
    sweep_cumulative_integrated_layout_fixed_dimensions,
};

pub const CUMULATIVE_FACILITY_COORDINATE_PARTITION_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FacilityCoordinateCaseDisposition {
    Executed,
    SkippedAfterWitness,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FacilityCoordinateCaseReport {
    pub coordinate_index: usize,
    pub x: i32,
    pub y: i32,
    pub disposition: FacilityCoordinateCaseDisposition,
    pub worker_index: usize,
    pub completion_order: usize,
    pub outcome: Option<ExactDimensionCaseOutcome>,
    pub construction_ms: Option<u64>,
    pub search_ms: Option<u64>,
    pub first_incumbent_ms: Option<u64>,
    pub model: Option<ExactModelMetrics>,
    pub model_complexity: Option<ModelComplexityMetrics>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CumulativeFacilityCoordinatePartitionReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub total_phase_count: usize,
    pub fixed_dimensions: ExactUsedDimensionCandidate,
    pub partitioned_facility: String,
    pub legal_coordinate_count: usize,
    pub requested_worker_count: usize,
    pub actual_worker_count: usize,
    pub prefix_search_budget_ms_per_case: u64,
    pub coordinate_search_budget_ms_per_case: u64,
    pub outer_wall_ms: u64,
    pub prefix_primary_area_optimum_proven: bool,
    pub prefix_hint_bounds: Option<[i64; 2]>,
    pub cases: Vec<FacilityCoordinateCaseReport>,
    pub validated_witness_found: bool,
    pub complete_infeasibility_proven: bool,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub selected_witness: Option<IntegratedLayoutReport>,
    pub representative_layout: Option<IntegratedLayoutReport>,
    pub diagnostic_only: bool,
}

struct WorkItem {
    coordinate_index: usize,
    coordinate: exact::shared_layer::FixedFacilityCoordinate,
}

struct CompletionEvent {
    coordinate_index: usize,
    coordinate: exact::shared_layer::FixedFacilityCoordinate,
    disposition: FacilityCoordinateCaseDisposition,
    worker_index: usize,
    outcome: Option<ExactDimensionCaseOutcome>,
    layout: Option<IntegratedLayoutReport>,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_cumulative_facility_coordinate_partitions(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    fixed_width: i32,
    fixed_height: i32,
    worker_count: usize,
    prefix_search_budget: Duration,
    coordinate_search_budget: Duration,
) -> Result<CumulativeFacilityCoordinatePartitionReport, IntegratedLayoutReport> {
    validate_inputs(
        target_phase_index,
        fixed_width,
        fixed_height,
        worker_count,
        prefix_search_budget,
        coordinate_search_budget,
    )?;
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success {
        return Err(invalid_input(
            "/target_phase_index",
            "facility growth planning failed",
        ));
    }
    if target_phase_index >= growth.phases.len() {
        return Err(invalid_input(
            "/target_phase_index",
            format!(
                "target phase {target_phase_index} is outside the cumulative phase range 0..{}",
                growth.phases.len()
            ),
        ));
    }
    let introduced = &growth.phases[target_phase_index].facilities;
    if introduced.len() != 1 {
        return Err(invalid_input(
            "/target_phase_index",
            format!(
                "coordinate partition requires exactly one facility introduced in the target phase, found {}",
                introduced.len()
            ),
        ));
    }
    let partitioned_facility = introduced[0].clone();

    let prefix = sweep_cumulative_integrated_layout_fixed_dimensions(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        target_phase_index - 1,
        worker_count,
        prefix_search_budget,
    )?;
    let prior_solution = prefix.layout;
    if !prefix.completed_target_phase || !prior_solution.success {
        return Err(invalid_input(
            "/prefix",
            "preceding cumulative phase did not produce a validated hint",
        ));
    }
    let prefix_primary_area_optimum_proven = prefix
        .phase_sweeps
        .last()
        .is_some_and(|phase| phase.primary_area_optimum_proven);
    let prefix_hint_bounds = prior_solution
        .bounds
        .as_ref()
        .map(|bounds| [bounds.width, bounds.height]);

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
    let fixed_dimensions = ExactUsedDimensionCandidate {
        width: fixed_width,
        height: fixed_height,
        area: i64::from(fixed_width) * i64::from(fixed_height),
    };
    let coordinates =
        exact::shared_layer::facility_coordinate_partitions(&input, &partitioned_facility)
            .map_err(IntegratedLayoutReport::invalid)?;
    if coordinates.is_empty() {
        return Err(invalid_input(
            "/partitioned_facility",
            "the introduced facility has no legal coordinate under the hard layout ceiling",
        ));
    }

    let actual_worker_count = worker_count.min(coordinates.len());
    let (work_sender, work_receiver) = unbounded::<WorkItem>();
    let (completion_sender, completion_receiver) = unbounded::<CompletionEvent>();
    for (coordinate_index, coordinate) in coordinates.iter().cloned().enumerate() {
        work_sender
            .send(WorkItem {
                coordinate_index,
                coordinate,
            })
            .expect("fresh coordinate work queue remains connected");
    }
    drop(work_sender);

    let witness_found = AtomicBool::new(false);
    let started = Instant::now();
    let mut cases = Vec::with_capacity(coordinates.len());
    let mut layouts = Vec::new();
    let mut worker_failure = None;
    std::thread::scope(|scope| {
        for worker_index in 0..actual_worker_count {
            let work_receiver = work_receiver.clone();
            let completion_sender = completion_sender.clone();
            let input = &input;
            let prior_solution = &prior_solution;
            let witness_found = &witness_found;
            scope.spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    run_worker(
                        worker_index,
                        work_receiver,
                        completion_sender.clone(),
                        input,
                        logistics_components,
                        fixed_width,
                        fixed_height,
                        coordinate_search_budget,
                        prior_solution,
                        witness_found,
                    );
                }));
                if result.is_err() {
                    let _ = completion_sender.send(CompletionEvent {
                        coordinate_index: usize::MAX,
                        coordinate: exact::shared_layer::FixedFacilityCoordinate {
                            instance: String::new(),
                            x: 0,
                            y: 0,
                        },
                        disposition: FacilityCoordinateCaseDisposition::Executed,
                        worker_index,
                        outcome: None,
                        layout: None,
                    });
                }
            });
        }
        drop(completion_sender);
        for (completion_order, event) in completion_receiver.iter().enumerate() {
            if event.coordinate_index == usize::MAX {
                worker_failure = Some(event.worker_index);
                continue;
            }
            let exact = event
                .layout
                .as_ref()
                .and_then(|layout| layout.exact.as_ref());
            cases.push(FacilityCoordinateCaseReport {
                coordinate_index: event.coordinate_index,
                x: event.coordinate.x,
                y: event.coordinate.y,
                disposition: event.disposition,
                worker_index: event.worker_index,
                completion_order,
                outcome: event.outcome,
                construction_ms: exact.map(|exact| exact.construction_ms),
                search_ms: exact.map(|exact| exact.search_ms),
                first_incumbent_ms: exact.and_then(|exact| exact.first_incumbent_ms),
                model: exact.map(|exact| exact.model),
                model_complexity: exact.map(|exact| exact.model_complexity.clone()),
            });
            if let Some(layout) = event.layout {
                layouts.push((event.outcome, layout));
            }
        }
    });
    if let Some(worker_index) = worker_failure {
        return Err(invalid_input(
            "/workers",
            format!("coordinate partition worker {worker_index} panicked"),
        ));
    }
    cases.sort_by_key(|case| case.coordinate_index);

    let selected_witness = layouts
        .iter()
        .find(|(outcome, _)| *outcome == Some(ExactDimensionCaseOutcome::ValidatedFeasible))
        .map(|(_, layout)| layout.clone());
    let representative_layout = selected_witness.clone().or_else(|| {
        layouts
            .iter()
            .find(|(outcome, _)| *outcome == Some(ExactDimensionCaseOutcome::Unknown))
            .or_else(|| layouts.first())
            .map(|(_, layout)| layout.clone())
    });
    let unknown_count = cases
        .iter()
        .filter(|case| case.outcome == Some(ExactDimensionCaseOutcome::Unknown))
        .count();
    let invalid_witness_count = cases
        .iter()
        .filter(|case| case.outcome == Some(ExactDimensionCaseOutcome::InvalidWitness))
        .count();
    let complete_infeasibility_proven = selected_witness.is_none()
        && cases
            .iter()
            .all(|case| case.outcome == Some(ExactDimensionCaseOutcome::ProvenInfeasible));

    Ok(CumulativeFacilityCoordinatePartitionReport {
        schema_version: CUMULATIVE_FACILITY_COORDINATE_PARTITION_SCHEMA_VERSION,
        target_phase_index,
        total_phase_count: growth.phases.len(),
        fixed_dimensions,
        partitioned_facility,
        legal_coordinate_count: coordinates.len(),
        requested_worker_count: worker_count,
        actual_worker_count,
        prefix_search_budget_ms_per_case: millis(prefix_search_budget),
        coordinate_search_budget_ms_per_case: millis(coordinate_search_budget),
        outer_wall_ms: millis(started.elapsed()),
        prefix_primary_area_optimum_proven,
        prefix_hint_bounds,
        cases,
        validated_witness_found: selected_witness.is_some(),
        complete_infeasibility_proven,
        unknown_count,
        invalid_witness_count,
        selected_witness,
        representative_layout,
        diagnostic_only: true,
    })
}

fn prepare_target_input(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    growth: &crate::layouts::FacilityGrowthPlanReport,
    target_phase_index: usize,
) -> Result<ModelInput, IntegratedLayoutReport> {
    let total_facilities = growth
        .components
        .iter()
        .map(|component| component.facilities.len())
        .sum();
    let cumulative = growth
        .phases
        .iter()
        .take(target_phase_index + 1)
        .flat_map(|phase| phase.facilities.iter().cloned())
        .collect();
    let partial =
        harness::project_cumulative_wiring(instance_wiring, &cumulative, total_facilities)
            .map_err(IntegratedLayoutReport::invalid)?;
    prepare_exact_model(
        &partial,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_worker(
    worker_index: usize,
    work_receiver: Receiver<WorkItem>,
    completion_sender: Sender<CompletionEvent>,
    input: &ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    fixed_width: i32,
    fixed_height: i32,
    search_budget: Duration,
    prior_solution: &IntegratedLayoutReport,
    witness_found: &AtomicBool,
) {
    while let Ok(work) = work_receiver.recv() {
        if witness_found.load(Ordering::Acquire) {
            if completion_sender
                .send(CompletionEvent {
                    coordinate_index: work.coordinate_index,
                    coordinate: work.coordinate,
                    disposition: FacilityCoordinateCaseDisposition::SkippedAfterWitness,
                    worker_index,
                    outcome: None,
                    layout: None,
                })
                .is_err()
            {
                return;
            }
            continue;
        }
        let layout = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_coordinate_feasibility_only_with_prior(
            input.clone(),
            logistics_components,
            Some(search_budget),
            exact::shared_layer::FixedUsedDimensions {
                width: fixed_width,
                height: fixed_height,
            },
            work.coordinate.clone(),
            Some(prior_solution),
        );
        let outcome = classify_outcome(&layout);
        if outcome == ExactDimensionCaseOutcome::ValidatedFeasible {
            witness_found.store(true, Ordering::Release);
        }
        if completion_sender
            .send(CompletionEvent {
                coordinate_index: work.coordinate_index,
                coordinate: work.coordinate,
                disposition: FacilityCoordinateCaseDisposition::Executed,
                worker_index,
                outcome: Some(outcome),
                layout: Some(layout),
            })
            .is_err()
        {
            return;
        }
    }
}

fn classify_outcome(layout: &IntegratedLayoutReport) -> ExactDimensionCaseOutcome {
    use super::super::{ExactProofStatus, ExactValidationStatus};
    if layout.success
        && layout
            .exact
            .as_ref()
            .is_some_and(|exact| exact.validation == ExactValidationStatus::Passed)
    {
        ExactDimensionCaseOutcome::ValidatedFeasible
    } else if layout.status == IntegratedLayoutStatus::Infeasible
        && layout
            .exact
            .as_ref()
            .is_some_and(|exact| exact.proof == ExactProofStatus::ProvenInfeasible)
    {
        ExactDimensionCaseOutcome::ProvenInfeasible
    } else if layout
        .exact
        .as_ref()
        .is_some_and(|exact| exact.validation == ExactValidationStatus::Failed)
    {
        ExactDimensionCaseOutcome::InvalidWitness
    } else {
        ExactDimensionCaseOutcome::Unknown
    }
}

fn validate_inputs(
    target_phase_index: usize,
    fixed_width: i32,
    fixed_height: i32,
    worker_count: usize,
    prefix_search_budget: Duration,
    coordinate_search_budget: Duration,
) -> Result<(), IntegratedLayoutReport> {
    if target_phase_index == 0 {
        return Err(invalid_input(
            "/target_phase_index",
            "coordinate partition diagnosis requires a preceding cumulative phase",
        ));
    }
    if fixed_width <= 0 || fixed_height <= 0 {
        return Err(invalid_input(
            "/fixed_dimensions",
            "fixed dimensions must both be positive",
        ));
    }
    if worker_count == 0 {
        return Err(invalid_input(
            "/worker_count",
            "coordinate partition diagnosis requires at least one worker",
        ));
    }
    if prefix_search_budget.is_zero() || coordinate_search_budget.is_zero() {
        return Err(invalid_input(
            "/search_budget",
            "coordinate partition diagnosis requires positive search budgets",
        ));
    }
    Ok(())
}

fn invalid_input(path: impl Into<String>, message: impl Into<String>) -> IntegratedLayoutReport {
    IntegratedLayoutReport::invalid(IntegratedLayoutDiagnostic::error(
        "invalid-cumulative-facility-coordinate-partition",
        path,
        None,
        message,
    ))
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}
