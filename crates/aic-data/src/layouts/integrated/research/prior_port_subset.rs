use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{
    ExactSearchStatistics, IntegratedLayoutReport, TransportNetworkEndpoint, exact,
};
use super::coordinate_partition::{
    FacilityPortAssignment, FacilityPortDomainReport, PartitionCaseModelScale, classify_outcome,
    enumerate_port_assignments, invalid_input, millis, model_scale, prepare_target_input,
    validate_inputs,
};
use super::{
    ExactDimensionCaseOutcome, ExactDimensionSolverStack,
    sweep_cumulative_integrated_layout_fixed_dimensions_with_local_continuation,
};

pub const PRIOR_PORT_SUBSET_ABLATION_SCHEMA_VERSION: u32 = 2;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorPortSubsetFacility {
    pub bit_index: usize,
    pub instance: String,
    pub matching_terminal_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorTerminalSubsetTerminal {
    pub bit_index: usize,
    pub terminal: String,
    pub reference_port: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorTerminalSubsetPartition {
    pub facility_bit_index: usize,
    pub facility_instance: String,
    pub terminals: Vec<PriorTerminalSubsetTerminal>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorPortSubsetCaseReport {
    pub facility_mask: u64,
    pub selected_facilities: Vec<String>,
    pub selected_terminals: Vec<String>,
    pub fixed_terminal_count: usize,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub model_scale: PartitionCaseModelScale,
    pub added_constraint_count_from_no_ports: i64,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorPortSubsetAblationReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub solver_stack: ExactDimensionSolverStack,
    pub endpoint_encoding: super::EndpointChannelEncoding,
    pub fixed_dimensions: [i32; 2],
    pub partitioned_facility: String,
    pub fixed_coordinate: [i32; 2],
    pub port_assignment_index: usize,
    pub fixed_ports: Vec<FacilityPortAssignment>,
    pub fixed_rotation: i64,
    pub prior_facilities: Vec<PriorPortSubsetFacility>,
    pub terminal_partition: Option<PriorTerminalSubsetPartition>,
    pub worker_count: usize,
    pub prefix_search_budget_ms_per_case: u64,
    pub case_search_budget_ms: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<PriorPortSubsetCaseReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_prior_port_subset_ablation(
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
    fixed_rotation: i64,
    terminal_subset_facility_bit: Option<usize>,
    worker_count: usize,
    prefix_search_budget: Duration,
    case_search_budget: Duration,
) -> Result<PriorPortSubsetAblationReport, IntegratedLayoutReport> {
    validate_inputs(
        target_phase_index,
        fixed_width,
        fixed_height,
        worker_count,
        prefix_search_budget,
        case_search_budget,
    )?;
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success || target_phase_index >= growth.phases.len() {
        return Err(invalid_input(
            "/target_phase_index",
            "prior-port subset ablation requires a valid cumulative target phase",
        ));
    }
    let introduced = &growth.phases[target_phase_index].facilities;
    if introduced.len() != 1 {
        return Err(invalid_input(
            "/target_phase_index",
            "prior-port subset ablation requires exactly one introduced facility",
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
            "preceding cumulative phase did not produce a validated exact reference",
        ));
    }

    let prior_instance_ids = prior_solution
        .placements
        .iter()
        .map(|placement| placement.instance.clone())
        .collect::<BTreeSet<_>>();
    if prior_instance_ids.len() > 63 {
        return Err(invalid_input(
            "/prefix/placements",
            "prior-port subset ablation supports at most 63 preceding facilities",
        ));
    }
    let terminal_counts = prior_solution
        .transport_networks
        .iter()
        .flat_map(|network| network.terminals.iter())
        .filter_map(|terminal| match &terminal.endpoint {
            TransportNetworkEndpoint::Facility { instance, .. } => Some(instance.clone()),
            TransportNetworkEndpoint::External { .. } => None,
        })
        .fold(BTreeMap::<String, usize>::new(), |mut counts, instance| {
            *counts.entry(instance).or_default() += 1;
            counts
        });
    let prior_facilities = prior_instance_ids
        .into_iter()
        .enumerate()
        .map(|(bit_index, instance)| PriorPortSubsetFacility {
            bit_index,
            matching_terminal_count: terminal_counts.get(&instance).copied().unwrap_or(0),
            instance,
        })
        .collect::<Vec<_>>();
    let terminal_partition = terminal_subset_facility_bit
        .map(|facility_bit_index| {
            let facility = prior_facilities.get(facility_bit_index).ok_or_else(|| {
                invalid_input(
                    "/terminal_subset_facility_bit",
                    format!(
                        "prior facility bit {facility_bit_index} is outside 0..{}",
                        prior_facilities.len()
                    ),
                )
            })?;
            let terminals = prior_solution
                .transport_networks
                .iter()
                .flat_map(|network| network.terminals.iter())
                .filter_map(|terminal| match &terminal.endpoint {
                    TransportNetworkEndpoint::Facility { instance, port }
                        if instance == &facility.instance =>
                    {
                        Some((terminal.id.clone(), port.clone()))
                    }
                    _ => None,
                })
                .collect::<BTreeMap<_, _>>()
                .into_iter()
                .enumerate()
                .map(
                    |(bit_index, (terminal, reference_port))| PriorTerminalSubsetTerminal {
                        bit_index,
                        terminal,
                        reference_port,
                    },
                )
                .collect::<Vec<_>>();
            if terminals.len() > 63 {
                return Err(invalid_input(
                    "/terminal_subset_facility_bit",
                    "prior terminal subset ablation supports at most 63 terminals",
                ));
            }
            Ok(PriorTerminalSubsetPartition {
                facility_bit_index,
                facility_instance: facility.instance.clone(),
                terminals,
            })
        })
        .transpose()?;

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
    if !legal_rotations.contains(&fixed_rotation) {
        return Err(invalid_input(
            "/fixed_rotation",
            "selected rotation is not legal at the fixed coordinate",
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
    let assignments = enumerate_port_assignments(&terminal_domains);
    let fixed_ports = assignments
        .get(port_assignment_index)
        .cloned()
        .ok_or_else(|| {
            invalid_input(
                "/port_assignment_index",
                format!(
                    "port assignment index {port_assignment_index} is outside 0..{}",
                    assignments.len()
                ),
            )
        })?;
    let exact_ports = fixed_ports
        .iter()
        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
            terminal: assignment.terminal.clone(),
            port: assignment.port.clone(),
        })
        .collect::<Vec<_>>();
    let dimensions = exact::shared_layer::FixedUsedDimensions {
        width: fixed_width,
        height: fixed_height,
    };
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: partitioned_facility.clone(),
        x: fixed_x,
        y: fixed_y,
        rotation: Some(fixed_rotation),
    };

    let subset_unit_count = terminal_partition
        .as_ref()
        .map_or(prior_facilities.len(), |partition| {
            partition.terminals.len()
        });
    let subset_count = 1_u64 << subset_unit_count;
    let masks = (0..subset_count).collect::<Vec<_>>();
    let started = Instant::now();
    let mut completed = Vec::with_capacity(masks.len());
    for chunk in masks.chunks(worker_count) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for &mask in chunk {
                let input = input.clone();
                let ports = exact_ports.clone();
                let coordinate = coordinate.clone();
                let prior_solution = &prior_solution;
                let terminal_subset_facility_bit = terminal_subset_facility_bit;
                handles.push((
                    mask,
                    scope.spawn(move || {
                        if let Some(facility_bit_index) = terminal_subset_facility_bit {
                            exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_coordinate_ports_prior_terminal_subset_ablation(
                                input,
                                logistics_components,
                                Some(case_search_budget),
                                dimensions,
                                coordinate,
                                ports,
                                prior_solution,
                                facility_bit_index,
                                mask,
                            )
                        } else {
                            exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_coordinate_ports_prior_port_subset_ablation(
                                input,
                                logistics_components,
                                Some(case_search_budget),
                                dimensions,
                                coordinate,
                                ports,
                                prior_solution,
                                mask,
                            )
                        }
                    }),
                ));
            }
            for (mask, handle) in handles {
                completed.push((
                    mask,
                    handle
                        .join()
                        .expect("prior-port subset ablation worker panicked"),
                ));
            }
        });
    }
    completed.sort_by_key(|(mask, _)| *mask);
    let baseline_constraints = completed
        .iter()
        .find(|(mask, _)| *mask == 0)
        .and_then(|(_, layout)| layout.exact.as_ref())
        .map(model_scale)
        .map(|scale| scale.constraints)
        .expect("zero-mask subset has exact model metrics");
    let cases = completed
        .into_iter()
        .map(|(mask, layout)| {
            let exact = layout
                .exact
                .as_ref()
                .expect("executed subset has exact model metrics");
            let scale = model_scale(exact);
            let selected_facilities = prior_facilities
                .iter()
                .filter(|facility| {
                    terminal_partition.as_ref().map_or_else(
                        || mask & (1_u64 << facility.bit_index) != 0,
                        |partition| facility.bit_index == partition.facility_bit_index && mask != 0,
                    )
                })
                .map(|facility| facility.instance.clone())
                .collect::<Vec<_>>();
            let selected_terminals = terminal_partition
                .as_ref()
                .map(|partition| {
                    partition
                        .terminals
                        .iter()
                        .filter(|terminal| mask & (1_u64 << terminal.bit_index) != 0)
                        .map(|terminal| terminal.terminal.clone())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let fixed_terminal_count = terminal_partition.as_ref().map_or_else(
                || {
                    prior_facilities
                        .iter()
                        .filter(|facility| mask & (1_u64 << facility.bit_index) != 0)
                        .map(|facility| facility.matching_terminal_count)
                        .sum()
                },
                |_| selected_terminals.len(),
            );
            PriorPortSubsetCaseReport {
                facility_mask: mask,
                selected_facilities,
                selected_terminals,
                fixed_terminal_count,
                outcome: classify_outcome(&layout),
                construction_ms: exact.construction_ms,
                search_ms: exact.search_ms,
                first_incumbent_ms: exact.first_incumbent_ms,
                search_statistics: exact.search_statistics,
                model_scale: scale,
                added_constraint_count_from_no_ports: i64::try_from(scale.constraints)
                    .expect("constraint count fits i64")
                    - i64::try_from(baseline_constraints).expect("constraint count fits i64"),
                layout,
            }
        })
        .collect();

    Ok(PriorPortSubsetAblationReport {
        schema_version: PRIOR_PORT_SUBSET_ABLATION_SCHEMA_VERSION,
        target_phase_index,
        solver_stack: ExactDimensionSolverStack::WatchedDemandWithLocalContinuation,
        endpoint_encoding: super::EndpointChannelEncoding::SparseSupport,
        fixed_dimensions: [fixed_width, fixed_height],
        partitioned_facility,
        fixed_coordinate: [fixed_x, fixed_y],
        port_assignment_index,
        fixed_ports,
        fixed_rotation,
        prior_facilities,
        terminal_partition,
        worker_count,
        prefix_search_budget_ms_per_case: millis(prefix_search_budget),
        case_search_budget_ms: millis(case_search_budget),
        outer_wall_ms: millis(started.elapsed()),
        cases,
        diagnostic_only: true,
    })
}
