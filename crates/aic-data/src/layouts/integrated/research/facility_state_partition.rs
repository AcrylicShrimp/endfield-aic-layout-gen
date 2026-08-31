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

use super::super::{ExactSearchStatistics, IntegratedLayoutReport, ModelInput, exact};
use super::coordinate_partition::{
    FacilityCoordinateCaseDisposition, FacilityPortAssignment, FacilityPortDomainReport,
    PartitionCaseModelScale, classify_outcome, enumerate_port_assignments, invalid_input, millis,
    model_scale, prepare_target_input, validate_inputs,
};
use super::{
    ExactDimensionCaseOutcome, ExactDimensionSolverStack, ExactUsedDimensionCandidate,
    sweep_cumulative_integrated_layout_fixed_dimensions_with_local_continuation,
};

pub const CUMULATIVE_FACILITY_STATE_PARTITION_SCHEMA_VERSION: u32 = 2;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FacilityStatePartitionCaseReport {
    pub assignment_index: usize,
    pub rotation: i64,
    pub disposition: FacilityCoordinateCaseDisposition,
    pub worker_index: usize,
    pub completion_order: usize,
    pub outcome: Option<ExactDimensionCaseOutcome>,
    pub construction_ms: Option<u64>,
    pub search_ms: Option<u64>,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: Option<ExactSearchStatistics>,
    pub model_scale: Option<PartitionCaseModelScale>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CumulativeFacilityStatePartitionReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub total_phase_count: usize,
    pub solver_stack: ExactDimensionSolverStack,
    pub fixed_dimensions: ExactUsedDimensionCandidate,
    pub partitioned_facility: String,
    pub fixed_coordinate: [i32; 2],
    pub prior_overlap_facility_state_fixed: bool,
    pub prior_placement_count: usize,
    pub prior_facility_terminal_count: usize,
    pub terminal_domains: Vec<FacilityPortDomainReport>,
    pub port_assignments: Vec<Vec<FacilityPortAssignment>>,
    pub legal_rotations: Vec<i64>,
    pub legal_state_count: usize,
    pub requested_worker_count: usize,
    pub actual_worker_count: usize,
    pub prefix_search_budget_ms_per_case: u64,
    pub state_search_budget_ms_per_case: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<FacilityStatePartitionCaseReport>,
    pub validated_witness_found: bool,
    pub complete_infeasibility_proven: bool,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub selected_witness: Option<IntegratedLayoutReport>,
    pub representative_layout: Option<IntegratedLayoutReport>,
    pub diagnostic_only: bool,
}

#[derive(Clone)]
struct WorkItem {
    assignment_index: usize,
    rotation: i64,
}

struct CompletionEvent {
    work: WorkItem,
    disposition: FacilityCoordinateCaseDisposition,
    worker_index: usize,
    outcome: Option<ExactDimensionCaseOutcome>,
    layout: Option<IntegratedLayoutReport>,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_cumulative_facility_state_partitions_with_local_continuation(
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
    worker_count: usize,
    prefix_search_budget: Duration,
    state_search_budget: Duration,
) -> Result<CumulativeFacilityStatePartitionReport, IntegratedLayoutReport> {
    diagnose_cumulative_facility_state_partitions_impl(
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
        worker_count,
        prefix_search_budget,
        state_search_budget,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_cumulative_facility_state_partitions_with_prior_overlap_facility_state(
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
    worker_count: usize,
    prefix_search_budget: Duration,
    state_search_budget: Duration,
) -> Result<CumulativeFacilityStatePartitionReport, IntegratedLayoutReport> {
    diagnose_cumulative_facility_state_partitions_impl(
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
        worker_count,
        prefix_search_budget,
        state_search_budget,
        true,
    )
}

#[allow(clippy::too_many_arguments)]
fn diagnose_cumulative_facility_state_partitions_impl(
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
    worker_count: usize,
    prefix_search_budget: Duration,
    state_search_budget: Duration,
    prior_overlap_facility_state_fixed: bool,
) -> Result<CumulativeFacilityStatePartitionReport, IntegratedLayoutReport> {
    validate_inputs(
        target_phase_index,
        fixed_width,
        fixed_height,
        worker_count,
        prefix_search_budget,
        state_search_budget,
    )?;
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success || target_phase_index >= growth.phases.len() {
        return Err(invalid_input(
            "/target_phase_index",
            "facility growth planning failed or the target phase is out of range",
        ));
    }
    let introduced = &growth.phases[target_phase_index].facilities;
    if introduced.len() != 1 {
        return Err(invalid_input(
            "/target_phase_index",
            "facility state partition requires exactly one introduced facility",
        ));
    }
    let partitioned_facility = introduced[0].clone();

    let prefix = sweep_cumulative_integrated_layout_fixed_dimensions_with_local_continuation(
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
            "preceding cumulative phase did not produce a validated active-stack hint",
        ));
    }

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
    let legal_rotations = exact::shared_layer::facility_rotations_at_coordinate(
        &input,
        &partitioned_facility,
        fixed_x,
        fixed_y,
    )
    .map_err(IntegratedLayoutReport::invalid)?;
    if legal_rotations.is_empty() {
        return Err(invalid_input(
            "/fixed_coordinate",
            "the fixed coordinate has no legal facility rotation",
        ));
    }
    let terminal_domains =
        exact::shared_layer::facility_port_partition_domains(&input, &partitioned_facility)
            .map_err(IntegratedLayoutReport::invalid)?
            .into_iter()
            .map(|domain| FacilityPortDomainReport {
                terminal: domain.terminal,
                ports: domain.ports,
            })
            .collect::<Vec<_>>();
    let port_assignments = enumerate_port_assignments(&terminal_domains);
    if port_assignments.is_empty() {
        return Err(invalid_input(
            "/terminal_domains",
            "the introduced facility has no complete compatible port assignment",
        ));
    }
    let legal_state_count = port_assignments
        .len()
        .checked_mul(legal_rotations.len())
        .ok_or_else(|| invalid_input("/states", "facility state count overflowed usize"))?;

    let (work_sender, work_receiver) = unbounded::<WorkItem>();
    let (completion_sender, completion_receiver) = unbounded::<CompletionEvent>();
    for assignment_index in 0..port_assignments.len() {
        for rotation in &legal_rotations {
            work_sender
                .send(WorkItem {
                    assignment_index,
                    rotation: *rotation,
                })
                .expect("fresh facility-state work queue remains connected");
        }
    }
    drop(work_sender);

    let actual_worker_count = worker_count.min(legal_state_count);
    let witness_found = AtomicBool::new(false);
    let started = Instant::now();
    let mut worker_failure = None;
    let mut completed = Vec::with_capacity(legal_state_count);
    std::thread::scope(|scope| {
        for worker_index in 0..actual_worker_count {
            let work_receiver = work_receiver.clone();
            let completion_sender = completion_sender.clone();
            let input = &input;
            let prior_solution = &prior_solution;
            let port_assignments = &port_assignments;
            let partitioned_facility = &partitioned_facility;
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
                        fixed_x,
                        fixed_y,
                        partitioned_facility,
                        port_assignments,
                        state_search_budget,
                        prior_solution,
                        prior_overlap_facility_state_fixed,
                        witness_found,
                    );
                }));
                if result.is_err() {
                    let _ = completion_sender.send(CompletionEvent {
                        work: WorkItem {
                            assignment_index: usize::MAX,
                            rotation: 0,
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
        for (completion_order, event) in completion_receiver.into_iter().enumerate() {
            if event.work.assignment_index == usize::MAX {
                worker_failure = Some(event.worker_index);
            } else {
                completed.push((completion_order, event));
            }
        }
    });
    if let Some(worker_index) = worker_failure {
        return Err(invalid_input(
            "/workers",
            format!("facility state partition worker {worker_index} panicked"),
        ));
    }

    completed.sort_by_key(|(_, event)| (event.work.assignment_index, event.work.rotation));
    let mut layouts = Vec::new();
    let cases = completed
        .into_iter()
        .map(|(completion_order, event)| {
            let exact = event
                .layout
                .as_ref()
                .and_then(|layout| layout.exact.as_ref());
            if let Some(layout) = event.layout.clone() {
                layouts.push((event.outcome, layout));
            }
            FacilityStatePartitionCaseReport {
                assignment_index: event.work.assignment_index,
                rotation: event.work.rotation,
                disposition: event.disposition,
                worker_index: event.worker_index,
                completion_order,
                outcome: event.outcome,
                construction_ms: exact.map(|exact| exact.construction_ms),
                search_ms: exact.map(|exact| exact.search_ms),
                first_incumbent_ms: exact.and_then(|exact| exact.first_incumbent_ms),
                search_statistics: exact.map(|exact| exact.search_statistics),
                model_scale: exact.map(model_scale),
            }
        })
        .collect::<Vec<_>>();
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

    Ok(CumulativeFacilityStatePartitionReport {
        schema_version: CUMULATIVE_FACILITY_STATE_PARTITION_SCHEMA_VERSION,
        target_phase_index,
        total_phase_count: growth.phases.len(),
        solver_stack: ExactDimensionSolverStack::WatchedDemandWithLocalContinuation,
        fixed_dimensions: ExactUsedDimensionCandidate {
            width: fixed_width,
            height: fixed_height,
            area: i64::from(fixed_width) * i64::from(fixed_height),
        },
        partitioned_facility,
        fixed_coordinate: [fixed_x, fixed_y],
        prior_overlap_facility_state_fixed,
        prior_placement_count: prior_solution.placements.len(),
        prior_facility_terminal_count: prior_solution
            .transport_networks
            .iter()
            .flat_map(|network| network.terminals.iter())
            .filter(|terminal| {
                matches!(
                    terminal.endpoint,
                    super::super::TransportNetworkEndpoint::Facility { .. }
                )
            })
            .count(),
        terminal_domains,
        port_assignments,
        legal_rotations,
        legal_state_count,
        requested_worker_count: worker_count,
        actual_worker_count,
        prefix_search_budget_ms_per_case: millis(prefix_search_budget),
        state_search_budget_ms_per_case: millis(state_search_budget),
        outer_wall_ms: millis(started.elapsed()),
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

#[allow(clippy::too_many_arguments)]
fn run_worker(
    worker_index: usize,
    work_receiver: Receiver<WorkItem>,
    completion_sender: Sender<CompletionEvent>,
    input: &ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    fixed_width: i32,
    fixed_height: i32,
    fixed_x: i32,
    fixed_y: i32,
    partitioned_facility: &str,
    port_assignments: &[Vec<FacilityPortAssignment>],
    search_budget: Duration,
    prior_solution: &IntegratedLayoutReport,
    prior_overlap_facility_state_fixed: bool,
    witness_found: &AtomicBool,
) {
    while let Ok(work) = work_receiver.recv() {
        if witness_found.load(Ordering::Acquire) {
            if completion_sender
                .send(CompletionEvent {
                    work,
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
        let fixed_ports = port_assignments[work.assignment_index]
            .iter()
            .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
                terminal: assignment.terminal.clone(),
                port: assignment.port.clone(),
            })
            .collect();
        let fixed_dimensions = exact::shared_layer::FixedUsedDimensions {
            width: fixed_width,
            height: fixed_height,
        };
        let fixed_coordinate = exact::shared_layer::FixedFacilityCoordinate {
            instance: partitioned_facility.to_string(),
            x: fixed_x,
            y: fixed_y,
            rotation: Some(work.rotation),
        };
        let layout = if prior_overlap_facility_state_fixed {
            exact::shared_layer::solve_factored_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                input.clone(),
                logistics_components,
                Some(search_budget),
                fixed_dimensions,
                fixed_coordinate,
                fixed_ports,
                prior_solution,
                exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacementsAndFacilityPorts,
            )
        } else {
            exact::shared_layer::solve_factored_endpoints_fixed_dimensions_coordinate_ports_feasibility_only_with_prior_and_local_continuation(
                input.clone(),
                logistics_components,
                Some(search_budget),
                fixed_dimensions,
                fixed_coordinate,
                fixed_ports,
                Some(prior_solution),
            )
        };
        let outcome = classify_outcome(&layout);
        if outcome == ExactDimensionCaseOutcome::ValidatedFeasible {
            witness_found.store(true, Ordering::Release);
        }
        if completion_sender
            .send(CompletionEvent {
                work,
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
