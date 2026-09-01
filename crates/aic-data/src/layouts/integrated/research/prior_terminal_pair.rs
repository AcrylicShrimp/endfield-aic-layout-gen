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

mod completion;

pub use completion::{
    BOUNDARY_CELL_WIDTH_SENSITIVITY_SCHEMA_VERSION, BoundaryCellWidthCaseReport,
    BoundaryCellWidthSensitivityReport, ENDPOINT_CONTINUATION_PARTITION_SCHEMA_VERSION,
    ENDPOINT_SOURCE_ONLY_CONTROL_SCHEMA_VERSION, EXTERNAL_BOUNDARY_CELL_PARTITION_SCHEMA_VERSION,
    EXTERNAL_BOUNDARY_KEY_LEGAL_SUPPORT_AB_SCHEMA_VERSION,
    EXTERNAL_BOUNDARY_SIDE_PARTITION_SCHEMA_VERSION, EndpointContinuationCandidate,
    EndpointContinuationCaseReport, EndpointContinuationPartitionReport,
    EndpointSourceOnlyCaseReport, EndpointSourceOnlyControlReport, EndpointSourceRegionEvidence,
    ExternalBoundaryCellCaseReport, ExternalBoundaryCellPartitionReport,
    ExternalBoundaryKeyCommonModelContract, ExternalBoundaryKeyLegalSupportAbReport,
    ExternalBoundaryKeyNetworkContract, ExternalBoundaryKeyRootComparison,
    ExternalBoundaryKeyRootTotals, ExternalBoundaryKeySolveReport,
    ExternalBoundaryKeyStaticCertificate, ExternalBoundarySideCaseReport,
    ExternalBoundarySideDomain, ExternalBoundarySidePartitionReport,
    PRIOR_INPUT_PAIR_ROOT_SNAPSHOT_SCHEMA_VERSION, PRIOR_INPUT_PORT_CONTROLS_SCHEMA_VERSION,
    PRIOR_INPUT_PORT_PAIR_PORTFOLIO_SCHEMA_VERSION, PRIOR_SOURCE_PORT_PORTFOLIO_SCHEMA_VERSION,
    PRIOR_TERMINAL_COMPLETION_PORTFOLIO_SCHEMA_VERSION, PriorInputPairRootSnapshotReport,
    PriorInputPortControlCaseReport, PriorInputPortControlSuiteReport,
    PriorInputPortControlsReport, PriorInputPortPairCaseReport, PriorInputPortPairPortfolioReport,
    PriorInputPortProofExclusion, PriorInputPortResidualDomain, PriorSourcePortCaseReport,
    PriorSourcePortParentReport, PriorSourcePortPortfolioReport, PriorTerminalCompletionCaseReport,
    PriorTerminalCompletionDomain, PriorTerminalCompletionParentReport,
    PriorTerminalCompletionPortfolioReport, RESIDUAL_FACILITY_PORT_TUPLE_PORTFOLIO_SCHEMA_VERSION,
    ResidualFacilityPortDomain, ResidualFacilityPortFixationObservation,
    ResidualFacilityPortTupleCaseReport, ResidualFacilityPortTuplePortfolioReport,
    diagnose_boundary_cell_width_sensitivity, diagnose_endpoint_continuation_partition,
    diagnose_endpoint_source_only_control, diagnose_external_boundary_cell_partition,
    diagnose_external_boundary_key_legal_support_ab, diagnose_external_boundary_side_partition,
    diagnose_prior_input_pair_root_snapshot, diagnose_prior_input_port_controls,
    diagnose_prior_input_port_pair_portfolio, diagnose_prior_source_port_portfolio,
    diagnose_prior_terminal_completion_portfolio, diagnose_residual_facility_port_tuple_portfolio,
};

pub const PRIOR_TERMINAL_PAIR_VALUE_PORTFOLIO_SCHEMA_VERSION: u32 = 2;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorTerminalPairDomain {
    pub terminal_bit_index: usize,
    pub terminal: String,
    pub reference_port: String,
    pub ports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorTerminalPairValueCaseReport {
    pub pair_index: usize,
    pub assignments: Vec<FacilityPortAssignment>,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub model_scale: PartitionCaseModelScale,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorTerminalPairValuePortfolioReport {
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
    pub prior_facility_bit_index: usize,
    pub prior_facility: String,
    pub prior_reference: IntegratedLayoutReport,
    pub terminal_domains: Vec<PriorTerminalPairDomain>,
    pub legal_pair_count: usize,
    pub worker_count: usize,
    pub prefix_search_budget_ms_per_case: u64,
    pub case_search_budget_ms: u64,
    pub preparation_ms: u64,
    pub portfolio_wall_ms: u64,
    pub total_wall_ms: u64,
    pub validated_feasible_count: usize,
    pub proven_infeasible_count: usize,
    pub validated_witness_found: bool,
    pub complete_infeasibility_proven: bool,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub cases: Vec<PriorTerminalPairValueCaseReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_prior_terminal_pair_value_portfolio(
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
    prior_facility_bit_index: usize,
    terminal_bit_indices: [usize; 2],
    worker_count: usize,
    prefix_search_budget: Duration,
    case_search_budget: Duration,
) -> Result<PriorTerminalPairValuePortfolioReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    validate_inputs(
        target_phase_index,
        fixed_width,
        fixed_height,
        worker_count,
        prefix_search_budget,
        case_search_budget,
    )?;
    if terminal_bit_indices[0] == terminal_bit_indices[1] {
        return Err(invalid_input(
            "/terminal_bit_indices",
            "terminal pair requires two distinct terminal bit indices",
        ));
    }

    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success || target_phase_index >= growth.phases.len() {
        return Err(invalid_input(
            "/target_phase_index",
            "prior-terminal pair portfolio requires a valid cumulative target phase",
        ));
    }
    let introduced = &growth.phases[target_phase_index].facilities;
    if introduced.len() != 1 {
        return Err(invalid_input(
            "/target_phase_index",
            "prior-terminal pair portfolio requires exactly one introduced facility",
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

    let prior_facilities = prior_solution
        .placements
        .iter()
        .map(|placement| placement.instance.clone())
        .collect::<BTreeSet<_>>();
    let prior_facility = prior_facilities
        .iter()
        .nth(prior_facility_bit_index)
        .cloned()
        .ok_or_else(|| {
            invalid_input(
                "/prior_facility_bit_index",
                format!(
                    "prior facility bit {prior_facility_bit_index} is outside 0..{}",
                    prior_facilities.len()
                ),
            )
        })?;
    let reference_terminals = prior_solution
        .transport_networks
        .iter()
        .flat_map(|network| network.terminals.iter())
        .filter_map(|terminal| match &terminal.endpoint {
            TransportNetworkEndpoint::Facility { instance, port }
                if instance == &prior_facility =>
            {
                Some((terminal.id.clone(), port.clone()))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();

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
    let introduced_domains =
        exact::shared_layer::facility_port_partition_domains(&input, &partitioned_facility)
            .map_err(IntegratedLayoutReport::invalid)?
            .into_iter()
            .map(|domain| FacilityPortDomainReport {
                terminal: domain.terminal,
                ports: domain.ports,
            })
            .collect::<Vec<_>>();
    let introduced_assignments = enumerate_port_assignments(&introduced_domains);
    let fixed_ports = introduced_assignments
        .get(port_assignment_index)
        .cloned()
        .ok_or_else(|| {
            invalid_input(
                "/port_assignment_index",
                format!(
                    "port assignment index {port_assignment_index} is outside 0..{}",
                    introduced_assignments.len()
                ),
            )
        })?;

    let prior_domains =
        exact::shared_layer::facility_port_partition_domains(&input, &prior_facility)
            .map_err(IntegratedLayoutReport::invalid)?
            .into_iter()
            .map(|domain| (domain.terminal, domain.ports))
            .collect::<BTreeMap<_, _>>();
    let reference_terminal_ids = reference_terminals.keys().cloned().collect::<Vec<_>>();
    let terminal_domains = terminal_bit_indices
        .into_iter()
        .map(|terminal_bit_index| {
            let terminal = reference_terminal_ids
                .get(terminal_bit_index)
                .cloned()
                .ok_or_else(|| {
                    invalid_input(
                        "/terminal_bit_indices",
                        format!(
                            "terminal bit {terminal_bit_index} is outside 0..{}",
                            reference_terminal_ids.len()
                        ),
                    )
                })?;
            let ports = prior_domains.get(&terminal).cloned().ok_or_else(|| {
                invalid_input(
                    "/terminal_bit_indices",
                    format!("terminal {terminal} has no cumulative exact-model port domain"),
                )
            })?;
            if ports.is_empty() {
                return Err(invalid_input(
                    "/terminal_bit_indices",
                    format!("terminal {terminal} has an empty compatible port domain"),
                ));
            }
            Ok(PriorTerminalPairDomain {
                terminal_bit_index,
                reference_port: reference_terminals
                    .get(&terminal)
                    .expect("selected reference terminal exists")
                    .clone(),
                terminal,
                ports,
            })
        })
        .collect::<Result<Vec<_>, IntegratedLayoutReport>>()?;
    let pairs = enumerate_terminal_pairs(&terminal_domains);

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
    let introduced_exact_ports = fixed_ports
        .iter()
        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
            terminal: assignment.terminal.clone(),
            port: assignment.port.clone(),
        })
        .collect::<Vec<_>>();

    let preparation_ms = millis(total_started.elapsed());
    let portfolio_started = Instant::now();
    let mut completed = Vec::with_capacity(pairs.len());
    for chunk in pairs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for (pair_index, assignments) in chunk {
                let input = input.clone();
                let coordinate = coordinate.clone();
                let prior_solution = &prior_solution;
                let mut exact_ports = introduced_exact_ports.clone();
                exact_ports.extend(assignments.iter().map(|assignment| {
                    exact::shared_layer::FixedTerminalPortChoice {
                        terminal: assignment.terminal.clone(),
                        port: assignment.port.clone(),
                    }
                }));
                let pair_index = *pair_index;
                let assignments = assignments.clone();
                handles.push((
                    pair_index,
                    assignments,
                    scope.spawn(move || {
                        exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                            input,
                            logistics_components,
                            Some(case_search_budget),
                            dimensions,
                            coordinate,
                            exact_ports,
                            prior_solution,
                            exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements,
                        )
                    }),
                ));
            }
            for (pair_index, assignments, handle) in handles {
                completed.push((
                    pair_index,
                    assignments,
                    handle
                        .join()
                        .expect("prior-terminal pair portfolio worker panicked"),
                ));
            }
        });
    }
    completed.sort_by_key(|(pair_index, _, _)| *pair_index);
    let cases = completed
        .into_iter()
        .map(|(pair_index, assignments, layout)| {
            let exact = layout
                .exact
                .as_ref()
                .expect("executed pair has exact model metrics");
            PriorTerminalPairValueCaseReport {
                pair_index,
                assignments,
                outcome: classify_outcome(&layout),
                construction_ms: exact.construction_ms,
                search_ms: exact.search_ms,
                first_incumbent_ms: exact.first_incumbent_ms,
                search_statistics: exact.search_statistics,
                model_scale: model_scale(exact),
                layout,
            }
        })
        .collect::<Vec<_>>();
    let validated_witness_found = cases
        .iter()
        .any(|case| case.outcome == ExactDimensionCaseOutcome::ValidatedFeasible);
    let validated_feasible_count = cases
        .iter()
        .filter(|case| case.outcome == ExactDimensionCaseOutcome::ValidatedFeasible)
        .count();
    let proven_infeasible_count = cases
        .iter()
        .filter(|case| case.outcome == ExactDimensionCaseOutcome::ProvenInfeasible)
        .count();
    let complete_infeasibility_proven = !validated_witness_found
        && cases
            .iter()
            .all(|case| case.outcome == ExactDimensionCaseOutcome::ProvenInfeasible);
    let unknown_count = cases
        .iter()
        .filter(|case| case.outcome == ExactDimensionCaseOutcome::Unknown)
        .count();
    let invalid_witness_count = cases
        .iter()
        .filter(|case| case.outcome == ExactDimensionCaseOutcome::InvalidWitness)
        .count();

    Ok(PriorTerminalPairValuePortfolioReport {
        schema_version: PRIOR_TERMINAL_PAIR_VALUE_PORTFOLIO_SCHEMA_VERSION,
        target_phase_index,
        solver_stack: ExactDimensionSolverStack::WatchedDemandWithLocalContinuation,
        endpoint_encoding: super::EndpointChannelEncoding::SparseSupport,
        fixed_dimensions: [fixed_width, fixed_height],
        partitioned_facility,
        fixed_coordinate: [fixed_x, fixed_y],
        port_assignment_index,
        fixed_ports,
        fixed_rotation,
        prior_facility_bit_index,
        prior_facility,
        prior_reference: prior_solution,
        legal_pair_count: pairs.len(),
        terminal_domains,
        worker_count,
        prefix_search_budget_ms_per_case: millis(prefix_search_budget),
        case_search_budget_ms: millis(case_search_budget),
        preparation_ms,
        portfolio_wall_ms: millis(portfolio_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        validated_feasible_count,
        proven_infeasible_count,
        validated_witness_found,
        complete_infeasibility_proven,
        unknown_count,
        invalid_witness_count,
        cases,
        diagnostic_only: true,
    })
}

fn enumerate_terminal_pairs(
    domains: &[PriorTerminalPairDomain],
) -> Vec<(usize, Vec<FacilityPortAssignment>)> {
    assert_eq!(domains.len(), 2, "terminal pair has exactly two domains");
    let mut pairs = Vec::with_capacity(domains[0].ports.len() * domains[1].ports.len());
    for left in &domains[0].ports {
        for right in &domains[1].ports {
            pairs.push((
                pairs.len(),
                vec![
                    FacilityPortAssignment {
                        terminal: domains[0].terminal.clone(),
                        port: left.clone(),
                    },
                    FacilityPortAssignment {
                        terminal: domains[1].terminal.clone(),
                        port: right.clone(),
                    },
                ],
            ));
        }
    }
    pairs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_pair_enumeration_is_complete_and_keeps_equal_port_values() {
        let domains = vec![
            PriorTerminalPairDomain {
                terminal_bit_index: 2,
                terminal: "left".to_string(),
                reference_port: "b".to_string(),
                ports: vec!["a".to_string(), "b".to_string()],
            },
            PriorTerminalPairDomain {
                terminal_bit_index: 3,
                terminal: "right".to_string(),
                reference_port: "a".to_string(),
                ports: vec!["a".to_string(), "c".to_string()],
            },
        ];

        let pairs = enumerate_terminal_pairs(&domains);

        assert_eq!(pairs.len(), 4);
        assert_eq!(pairs[0].1[0].port, "a");
        assert_eq!(pairs[0].1[1].port, "a");
        assert_eq!(pairs[3].1[0].port, "b");
        assert_eq!(pairs[3].1[1].port, "c");
    }
}
