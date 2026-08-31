use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use crossbeam_channel::unbounded;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{IntegratedLayoutReport, exact};
use super::coordinate_partition::{
    CUMULATIVE_FACILITY_ROTATION_PARTITION_SCHEMA_VERSION,
    CumulativeFacilityRotationPartitionReport, FacilityPortDomainReport,
    FacilityRotationPartitionCaseReport, classify_outcome, enumerate_port_assignments,
    invalid_input, millis, model_scale, prepare_target_input, validate_inputs,
};
use super::{
    ExactDimensionCaseOutcome, ExactUsedDimensionCandidate,
    sweep_cumulative_integrated_layout_fixed_dimensions,
};

const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

struct Completion {
    rotation: i64,
    worker_index: usize,
    layout: IntegratedLayoutReport,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_cumulative_facility_rotation_partitions(
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
    rotation_search_budget: Duration,
) -> Result<CumulativeFacilityRotationPartitionReport, IntegratedLayoutReport> {
    validate_inputs(
        target_phase_index,
        fixed_width,
        fixed_height,
        1,
        prefix_search_budget,
        rotation_search_budget,
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
            "rotation partition requires exactly one introduced facility",
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
        4,
        prefix_search_budget,
    )?;
    let prior_solution = prefix.layout;
    if !prefix.completed_target_phase || !prior_solution.success {
        return Err(invalid_input(
            "/prefix",
            "preceding cumulative phase did not produce a validated hint",
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
    let domains =
        exact::shared_layer::facility_port_partition_domains(&input, &partitioned_facility)
            .map_err(IntegratedLayoutReport::invalid)?
            .into_iter()
            .map(|domain| FacilityPortDomainReport {
                terminal: domain.terminal,
                ports: domain.ports,
            })
            .collect::<Vec<_>>();
    let assignments = enumerate_port_assignments(&domains);
    let fixed_port_assignments = assignments.get(port_assignment_index).cloned().ok_or_else(
        || {
            invalid_input(
                "/port_assignment_index",
                format!(
                    "port assignment {port_assignment_index} is outside the complete range 0..{}",
                    assignments.len()
                ),
            )
        },
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

    let started = Instant::now();
    let (sender, receiver) = unbounded::<Completion>();
    let mut completions = Vec::with_capacity(legal_rotations.len());
    std::thread::scope(|scope| {
        for (worker_index, rotation) in legal_rotations.iter().copied().enumerate() {
            let sender = sender.clone();
            let input = &input;
            let prior_solution = &prior_solution;
            let partitioned_facility = &partitioned_facility;
            let fixed_port_assignments = &fixed_port_assignments;
            scope.spawn(move || {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    let fixed_ports = fixed_port_assignments
                        .iter()
                        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
                            terminal: assignment.terminal.clone(),
                            port: assignment.port.clone(),
                        })
                        .collect();
                    let layout = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_coordinate_ports_feasibility_only_with_prior(
                        input.clone(),
                        logistics_components,
                        Some(rotation_search_budget),
                        exact::shared_layer::FixedUsedDimensions {
                            width: fixed_width,
                            height: fixed_height,
                        },
                        exact::shared_layer::FixedFacilityCoordinate {
                            instance: partitioned_facility.clone(),
                            x: fixed_x,
                            y: fixed_y,
                            rotation: Some(rotation),
                        },
                        fixed_ports,
                        Some(prior_solution),
                    );
                    let _ = sender.send(Completion {
                        rotation,
                        worker_index,
                        layout,
                    });
                }));
            });
        }
        drop(sender);
        completions.extend(receiver.iter());
    });
    if completions.len() != legal_rotations.len() {
        return Err(invalid_input(
            "/workers",
            "one rotation partition worker panicked",
        ));
    }
    completions.sort_by_key(|completion| completion.rotation);

    let mut cases = Vec::with_capacity(completions.len());
    let mut selected_witness = None;
    let mut representative_layout = None;
    for (completion_order, completion) in completions.into_iter().enumerate() {
        let outcome = classify_outcome(&completion.layout);
        let exact = completion
            .layout
            .exact
            .as_ref()
            .expect("executed exact rotation case has metrics");
        if outcome == ExactDimensionCaseOutcome::ValidatedFeasible && selected_witness.is_none() {
            selected_witness = Some(completion.layout.clone());
        }
        representative_layout.get_or_insert_with(|| completion.layout.clone());
        cases.push(FacilityRotationPartitionCaseReport {
            rotation: completion.rotation,
            worker_index: completion.worker_index,
            completion_order,
            outcome,
            construction_ms: exact.construction_ms,
            search_ms: exact.search_ms,
            first_incumbent_ms: exact.first_incumbent_ms,
            model_scale: model_scale(exact),
        });
    }
    let unknown_count = cases
        .iter()
        .filter(|case| case.outcome == ExactDimensionCaseOutcome::Unknown)
        .count();
    let complete_infeasibility_proven = cases
        .iter()
        .all(|case| case.outcome == ExactDimensionCaseOutcome::ProvenInfeasible);
    let representative_layout = selected_witness
        .clone()
        .or(representative_layout)
        .expect("at least one legal rotation was executed");

    Ok(CumulativeFacilityRotationPartitionReport {
        schema_version: CUMULATIVE_FACILITY_ROTATION_PARTITION_SCHEMA_VERSION,
        target_phase_index,
        total_phase_count: growth.phases.len(),
        fixed_dimensions: ExactUsedDimensionCandidate {
            width: fixed_width,
            height: fixed_height,
            area: i64::from(fixed_width) * i64::from(fixed_height),
        },
        partitioned_facility,
        fixed_coordinate: [fixed_x, fixed_y],
        fixed_port_assignment_index: port_assignment_index,
        fixed_port_assignments,
        legal_rotations,
        search_budget_ms_per_rotation: millis(rotation_search_budget),
        outer_wall_ms: millis(started.elapsed()),
        cases,
        validated_witness_found: selected_witness.is_some(),
        complete_infeasibility_proven,
        unknown_count,
        selected_witness,
        representative_layout,
        diagnostic_only: true,
    })
}
