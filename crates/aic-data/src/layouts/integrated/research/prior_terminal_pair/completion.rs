use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::super::{
    ExactSearchStatistics, IntegratedLayoutReport, TransportNetworkEndpoint, exact,
};
use super::super::coordinate_partition::{
    FacilityPortAssignment, FacilityPortDomainReport, PartitionCaseModelScale, classify_outcome,
    enumerate_port_assignments, invalid_input, millis, model_scale, prepare_target_input,
};
use super::super::{ExactDimensionCaseOutcome, ExactDimensionSolverStack};
use super::{PriorTerminalPairValuePortfolioReport, diagnose_prior_terminal_pair_value_portfolio};

mod source_port;

pub use source_port::{
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
    GUARDED_CORE_BOUNDARY_CENSUS_SCHEMA_VERSION, GUARDED_CORE_INITIAL_GATE_SCHEMA_VERSION,
    GuardedCoreAcceptedFixture, GuardedCoreBoundaryCensusCase, GuardedCoreBoundaryCensusReport,
    GuardedCoreBoundaryCensusRootStatus, GuardedCoreBoundaryCensusStatus,
    GuardedCoreInitialGateReport, GuardedCoreInitialGateStatus, GuardedCoreReplayReport,
    GuardedCoreReplayStatus, GuardedCoreSequentialShrinkReport, GuardedCoreSequentialShrinkStatus,
    GuardedCoreShrinkAttempt, MATERIAL_JUNCTION_CONTINUATION_SCHEMA_VERSION,
    MATERIAL_ROW5_SEPARATOR_SCHEMA_VERSION, MATERIAL_SEPARATOR_CUT_SCHEMA_VERSION,
    MaterialJunctionCaseReport, MaterialJunctionContinuationReport,
    MaterialRow5SeparatorCaseReport, MaterialRow5SeparatorReport, MaterialSeparatorCaseReport,
    MaterialSeparatorCutReport, PRIOR_INPUT_PAIR_ROOT_SNAPSHOT_SCHEMA_VERSION,
    PRIOR_INPUT_PORT_CONTROLS_SCHEMA_VERSION, PRIOR_INPUT_PORT_PAIR_PORTFOLIO_SCHEMA_VERSION,
    PRIOR_SOURCE_PORT_PORTFOLIO_SCHEMA_VERSION, PriorInputPairRootSnapshotReport,
    PriorInputPortControlCaseReport, PriorInputPortControlSuiteReport,
    PriorInputPortControlsReport, PriorInputPortPairCaseReport, PriorInputPortPairPortfolioReport,
    PriorInputPortProofExclusion, PriorInputPortResidualDomain, PriorSourcePortCaseReport,
    PriorSourcePortParentReport, PriorSourcePortPortfolioReport,
    RESIDUAL_FACILITY_PORT_TUPLE_PORTFOLIO_SCHEMA_VERSION, ResidualFacilityPortDomain,
    ResidualFacilityPortFixationObservation, ResidualFacilityPortTupleCaseReport,
    ResidualFacilityPortTuplePortfolioReport, diagnose_boundary_cell_width_sensitivity,
    diagnose_endpoint_continuation_partition, diagnose_endpoint_source_only_control,
    diagnose_external_boundary_cell_partition, diagnose_external_boundary_key_legal_support_ab,
    diagnose_external_boundary_side_partition, diagnose_guarded_core_boundary_census,
    diagnose_guarded_core_initial_gate, diagnose_guarded_core_replay,
    diagnose_guarded_core_sequential_shrinking, diagnose_material_junction_continuation,
    diagnose_material_row5_separator, diagnose_material_separator_cut,
    diagnose_prior_input_pair_root_snapshot, diagnose_prior_input_port_controls,
    diagnose_prior_input_port_pair_portfolio, diagnose_prior_source_port_portfolio,
    diagnose_residual_facility_port_tuple_portfolio,
};

pub const PRIOR_TERMINAL_COMPLETION_PORTFOLIO_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorTerminalCompletionDomain {
    pub terminal_bit_index: usize,
    pub terminal: String,
    pub reference_port: String,
    pub ports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorTerminalCompletionParentReport {
    pub pair_index: usize,
    pub assignments: Vec<FacilityPortAssignment>,
    pub parent_outcome: ExactDimensionCaseOutcome,
    pub expanded: bool,
    pub child_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorTerminalCompletionCaseReport {
    pub leaf_index: usize,
    pub parent_pair_index: usize,
    pub pair_assignments: Vec<FacilityPortAssignment>,
    pub completion_assignments: Vec<FacilityPortAssignment>,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub model_scale: PartitionCaseModelScale,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorTerminalCompletionPortfolioReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub solver_stack: ExactDimensionSolverStack,
    pub endpoint_encoding: super::super::EndpointChannelEncoding,
    pub pair_stage: PriorTerminalPairValuePortfolioReport,
    pub completion_domains: Vec<PriorTerminalCompletionDomain>,
    pub completion_assignment_count_per_parent: usize,
    pub closed_parent_count: usize,
    pub expanded_parent_count: usize,
    pub coverage_region_count: usize,
    pub worker_count: usize,
    pub child_case_search_budget_ms: u64,
    pub child_preparation_ms: u64,
    pub child_portfolio_wall_ms: u64,
    pub total_wall_ms: u64,
    pub child_validated_feasible_count: usize,
    pub child_proven_infeasible_count: usize,
    pub child_unknown_count: usize,
    pub child_invalid_witness_count: usize,
    pub validated_witness_found: bool,
    pub selected_state_infeasibility_proven: bool,
    pub parents: Vec<PriorTerminalCompletionParentReport>,
    pub cases: Vec<PriorTerminalCompletionCaseReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_prior_terminal_completion_portfolio(
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
    pair_case_search_budget: Duration,
    child_case_search_budget: Duration,
) -> Result<PriorTerminalCompletionPortfolioReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if child_case_search_budget.is_zero() {
        return Err(invalid_input(
            "/child_case_search_budget",
            "prior-terminal completion portfolio requires a positive child search budget",
        ));
    }
    let pair_stage = diagnose_prior_terminal_pair_value_portfolio(
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
        fixed_rotation,
        prior_facility_bit_index,
        terminal_bit_indices,
        worker_count,
        prefix_search_budget,
        pair_case_search_budget,
    )?;
    let child_preparation_started = Instant::now();

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
    let reference_terminals = pair_stage
        .prior_reference
        .transport_networks
        .iter()
        .flat_map(|network| network.terminals.iter())
        .filter_map(|terminal| match &terminal.endpoint {
            TransportNetworkEndpoint::Facility { instance, port }
                if instance == &pair_stage.prior_facility =>
            {
                Some((terminal.id.clone(), port.clone()))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let prior_domains =
        exact::shared_layer::facility_port_partition_domains(&input, &pair_stage.prior_facility)
            .map_err(IntegratedLayoutReport::invalid)?
            .into_iter()
            .map(|domain| (domain.terminal, domain.ports))
            .collect::<BTreeMap<_, _>>();
    let selected_terminals = pair_stage
        .terminal_domains
        .iter()
        .map(|domain| domain.terminal.clone())
        .collect::<BTreeSet<_>>();
    let reference_terminal_ids = reference_terminals.keys().cloned().collect::<BTreeSet<_>>();
    let prior_domain_ids = prior_domains.keys().cloned().collect::<BTreeSet<_>>();
    if reference_terminal_ids != prior_domain_ids {
        return Err(invalid_input(
            "/completion_domains",
            "reference and cumulative exact-model terminal sets differ for the prior facility",
        ));
    }
    if !selected_terminals.is_subset(&reference_terminal_ids) {
        return Err(invalid_input(
            "/completion_domains",
            "selected pair contains a terminal outside the prior facility reference",
        ));
    }
    let completion_domains = reference_terminals
        .iter()
        .enumerate()
        .filter(|(_, (terminal, _))| !selected_terminals.contains(*terminal))
        .map(|(terminal_bit_index, (terminal, reference_port))| {
            let ports = prior_domains.get(terminal).cloned().ok_or_else(|| {
                invalid_input(
                    "/completion_domains",
                    format!("terminal {terminal} has no cumulative exact-model port domain"),
                )
            })?;
            if ports.is_empty() {
                return Err(invalid_input(
                    "/completion_domains",
                    format!("terminal {terminal} has an empty compatible port domain"),
                ));
            }
            Ok(PriorTerminalCompletionDomain {
                terminal_bit_index,
                terminal: terminal.clone(),
                reference_port: reference_port.clone(),
                ports,
            })
        })
        .collect::<Result<Vec<_>, IntegratedLayoutReport>>()?;
    if selected_terminals.len() + completion_domains.len() != reference_terminals.len() {
        return Err(invalid_input(
            "/completion_domains",
            "selected and completion terminal domains do not cover the prior facility",
        ));
    }
    let completion_assignment_domains = completion_domains
        .iter()
        .map(|domain| FacilityPortDomainReport {
            terminal: domain.terminal.clone(),
            ports: domain.ports.clone(),
        })
        .collect::<Vec<_>>();
    let completion_assignments = enumerate_port_assignments(&completion_assignment_domains);
    if completion_assignments.is_empty() {
        return Err(invalid_input(
            "/completion_domains",
            "prior-terminal completion portfolio has no complete assignment",
        ));
    }

    let parents = pair_stage
        .cases
        .iter()
        .map(|case| PriorTerminalCompletionParentReport {
            pair_index: case.pair_index,
            assignments: case.assignments.clone(),
            parent_outcome: case.outcome,
            expanded: case.outcome != ExactDimensionCaseOutcome::ProvenInfeasible,
            child_count: if case.outcome == ExactDimensionCaseOutcome::ProvenInfeasible {
                0
            } else {
                completion_assignments.len()
            },
        })
        .collect::<Vec<_>>();
    let leaf_inputs = enumerate_completion_leaves(&parents, &completion_assignments);
    let closed_parent_count = parents.iter().filter(|parent| !parent.expanded).count();
    let expanded_parent_count = parents.len() - closed_parent_count;
    let coverage_region_count = closed_parent_count + leaf_inputs.len();

    let dimensions = exact::shared_layer::FixedUsedDimensions {
        width: pair_stage.fixed_dimensions[0],
        height: pair_stage.fixed_dimensions[1],
    };
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: pair_stage.partitioned_facility.clone(),
        x: pair_stage.fixed_coordinate[0],
        y: pair_stage.fixed_coordinate[1],
        rotation: Some(pair_stage.fixed_rotation),
    };
    let introduced_exact_ports = pair_stage
        .fixed_ports
        .iter()
        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
            terminal: assignment.terminal.clone(),
            port: assignment.port.clone(),
        })
        .collect::<Vec<_>>();
    let child_preparation_ms = millis(child_preparation_started.elapsed());
    let child_portfolio_started = Instant::now();
    let mut completed = Vec::with_capacity(leaf_inputs.len());
    for chunk in leaf_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for leaf in chunk {
                let input = input.clone();
                let coordinate = coordinate.clone();
                let prior_reference = &pair_stage.prior_reference;
                let mut exact_ports = introduced_exact_ports.clone();
                exact_ports.extend(
                    leaf.pair_assignments
                        .iter()
                        .chain(leaf.completion_assignments.iter())
                        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
                            terminal: assignment.terminal.clone(),
                            port: assignment.port.clone(),
                        }),
                );
                let leaf = leaf.clone();
                handles.push((
                    leaf.clone(),
                    scope.spawn(move || {
                        exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                            input,
                            logistics_components,
                            Some(child_case_search_budget),
                            dimensions,
                            coordinate,
                            exact_ports,
                            prior_reference,
                            exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements,
                        )
                    }),
                ));
            }
            for (leaf, handle) in handles {
                completed.push((
                    leaf,
                    handle
                        .join()
                        .expect("prior-terminal completion portfolio worker panicked"),
                ));
            }
        });
    }
    completed.sort_by_key(|(leaf, _)| leaf.leaf_index);
    let cases = completed
        .into_iter()
        .map(|(leaf, layout)| {
            let exact = layout
                .exact
                .as_ref()
                .expect("executed completion leaf has exact model metrics");
            PriorTerminalCompletionCaseReport {
                leaf_index: leaf.leaf_index,
                parent_pair_index: leaf.parent_pair_index,
                pair_assignments: leaf.pair_assignments,
                completion_assignments: leaf.completion_assignments,
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
    let child_validated_feasible_count = cases
        .iter()
        .filter(|case| case.outcome == ExactDimensionCaseOutcome::ValidatedFeasible)
        .count();
    let child_proven_infeasible_count = cases
        .iter()
        .filter(|case| case.outcome == ExactDimensionCaseOutcome::ProvenInfeasible)
        .count();
    let child_unknown_count = cases
        .iter()
        .filter(|case| case.outcome == ExactDimensionCaseOutcome::Unknown)
        .count();
    let child_invalid_witness_count = cases
        .iter()
        .filter(|case| case.outcome == ExactDimensionCaseOutcome::InvalidWitness)
        .count();
    let selected_state_infeasibility_proven =
        pair_stage.proven_infeasible_count + child_proven_infeasible_count == coverage_region_count;
    let validated_witness_found = completion_validated_witness_found(
        pair_stage.validated_witness_found,
        child_validated_feasible_count,
    );

    Ok(PriorTerminalCompletionPortfolioReport {
        schema_version: PRIOR_TERMINAL_COMPLETION_PORTFOLIO_SCHEMA_VERSION,
        target_phase_index,
        solver_stack: ExactDimensionSolverStack::WatchedDemandWithLocalContinuation,
        endpoint_encoding: super::super::EndpointChannelEncoding::SparseSupport,
        completion_assignment_count_per_parent: completion_assignments.len(),
        completion_domains,
        closed_parent_count,
        expanded_parent_count,
        coverage_region_count,
        worker_count,
        child_case_search_budget_ms: millis(child_case_search_budget),
        child_preparation_ms,
        child_portfolio_wall_ms: millis(child_portfolio_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        child_validated_feasible_count,
        child_proven_infeasible_count,
        child_unknown_count,
        child_invalid_witness_count,
        validated_witness_found,
        selected_state_infeasibility_proven,
        parents,
        cases,
        pair_stage,
        diagnostic_only: true,
    })
}

fn completion_validated_witness_found(
    pair_stage_validated_witness_found: bool,
    child_validated_feasible_count: usize,
) -> bool {
    pair_stage_validated_witness_found || child_validated_feasible_count > 0
}

#[derive(Debug, Clone)]
struct CompletionLeafInput {
    leaf_index: usize,
    parent_pair_index: usize,
    pair_assignments: Vec<FacilityPortAssignment>,
    completion_assignments: Vec<FacilityPortAssignment>,
}

fn enumerate_completion_leaves(
    parents: &[PriorTerminalCompletionParentReport],
    completion_assignments: &[Vec<FacilityPortAssignment>],
) -> Vec<CompletionLeafInput> {
    let mut leaves = Vec::new();
    for parent in parents.iter().filter(|parent| parent.expanded) {
        for completion in completion_assignments {
            leaves.push(CompletionLeafInput {
                leaf_index: leaves.len(),
                parent_pair_index: parent.pair_index,
                pair_assignments: parent.assignments.clone(),
                completion_assignments: completion.clone(),
            });
        }
    }
    leaves
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_only_non_infeasible_parents_with_every_completion() {
        let parents = vec![
            PriorTerminalCompletionParentReport {
                pair_index: 0,
                assignments: vec![],
                parent_outcome: ExactDimensionCaseOutcome::ProvenInfeasible,
                expanded: false,
                child_count: 0,
            },
            PriorTerminalCompletionParentReport {
                pair_index: 4,
                assignments: vec![FacilityPortAssignment {
                    terminal: "demand".to_string(),
                    port: "input-belt-4".to_string(),
                }],
                parent_outcome: ExactDimensionCaseOutcome::Unknown,
                expanded: true,
                child_count: 2,
            },
        ];
        let completions = vec![
            vec![FacilityPortAssignment {
                terminal: "supply".to_string(),
                port: "output-belt-0".to_string(),
            }],
            vec![FacilityPortAssignment {
                terminal: "supply".to_string(),
                port: "output-belt-1".to_string(),
            }],
        ];

        let leaves = enumerate_completion_leaves(&parents, &completions);

        assert_eq!(leaves.len(), 2);
        assert!(leaves.iter().all(|leaf| leaf.parent_pair_index == 4));
        assert_eq!(leaves[0].completion_assignments[0].port, "output-belt-0");
        assert_eq!(leaves[1].completion_assignments[0].port, "output-belt-1");
    }

    #[test]
    fn preserves_a_parent_witness_when_all_completion_children_are_unknown() {
        assert!(completion_validated_witness_found(true, 0));
    }
}
