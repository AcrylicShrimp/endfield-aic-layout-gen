use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::super::super::{
    ExactSearchStatistics, IntegratedLayoutReport, TransportNetworkEndpoint, WorldGridPosition,
    candidate_port_connections, exact, world_position,
};
use super::super::super::coordinate_partition::{
    FacilityPortAssignment, FacilityPortDomainReport, PartitionCaseModelScale, classify_outcome,
    invalid_input, millis, model_scale, prepare_target_input,
};
use super::super::super::{ExactDimensionCaseOutcome, ExactDimensionSolverStack};
use super::{PriorTerminalCompletionPortfolioReport, diagnose_prior_terminal_completion_portfolio};

mod input_controls;

pub use input_controls::{
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
    MATERIAL_JUNCTION_CONTINUATION_SCHEMA_VERSION, MATERIAL_SEPARATOR_CUT_SCHEMA_VERSION,
    MaterialJunctionCaseReport, MaterialJunctionContinuationReport, MaterialSeparatorCaseReport,
    MaterialSeparatorCutReport, PRIOR_INPUT_PAIR_ROOT_SNAPSHOT_SCHEMA_VERSION,
    PRIOR_INPUT_PORT_CONTROLS_SCHEMA_VERSION, PRIOR_INPUT_PORT_PAIR_PORTFOLIO_SCHEMA_VERSION,
    PriorInputPairRootSnapshotReport, PriorInputPortControlCaseReport,
    PriorInputPortControlSuiteReport, PriorInputPortControlsReport, PriorInputPortPairCaseReport,
    PriorInputPortPairPortfolioReport, PriorInputPortProofExclusion, PriorInputPortResidualDomain,
    RESIDUAL_FACILITY_PORT_TUPLE_PORTFOLIO_SCHEMA_VERSION, ResidualFacilityPortDomain,
    ResidualFacilityPortFixationObservation, ResidualFacilityPortTupleCaseReport,
    ResidualFacilityPortTuplePortfolioReport, diagnose_boundary_cell_width_sensitivity,
    diagnose_endpoint_continuation_partition, diagnose_endpoint_source_only_control,
    diagnose_external_boundary_cell_partition, diagnose_external_boundary_key_legal_support_ab,
    diagnose_external_boundary_side_partition, diagnose_material_junction_continuation,
    diagnose_material_separator_cut, diagnose_prior_input_pair_root_snapshot,
    diagnose_prior_input_port_controls, diagnose_prior_input_port_pair_portfolio,
    diagnose_residual_facility_port_tuple_portfolio,
};

pub const PRIOR_SOURCE_PORT_PORTFOLIO_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorSourcePortParentReport {
    pub completion_leaf_index: usize,
    pub pair_assignments: Vec<FacilityPortAssignment>,
    pub completion_assignments: Vec<FacilityPortAssignment>,
    pub parent_outcome: ExactDimensionCaseOutcome,
    pub expanded: bool,
    pub child_count: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorSourcePortCaseReport {
    pub source_leaf_index: usize,
    pub parent_completion_leaf_index: usize,
    pub pair_assignments: Vec<FacilityPortAssignment>,
    pub completion_assignments: Vec<FacilityPortAssignment>,
    pub source_assignment: FacilityPortAssignment,
    pub source_position: WorldGridPosition,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub model_scale: PartitionCaseModelScale,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorSourcePortPortfolioReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub solver_stack: ExactDimensionSolverStack,
    pub endpoint_encoding: super::super::super::EndpointChannelEncoding,
    pub completion_stage: PriorTerminalCompletionPortfolioReport,
    pub source_terminal: String,
    pub source_facility: String,
    pub source_reference_port: String,
    pub source_reference_position: WorldGridPosition,
    pub source_ports: Vec<String>,
    pub source_port_positions: Vec<WorldGridPosition>,
    pub fixed_terminal_count_per_source_leaf: usize,
    pub selected_lane_terminal_ids: Vec<String>,
    pub selected_lane_terminals_fully_fixed: bool,
    pub unfixed_facility_terminal_domains: Vec<FacilityPortDomainReport>,
    pub source_assignment_count_per_parent: usize,
    pub closed_pair_region_count: usize,
    pub closed_completion_region_count: usize,
    pub expanded_completion_parent_count: usize,
    pub coverage_region_count: usize,
    pub worker_count: usize,
    pub source_case_search_budget_ms: u64,
    pub source_preparation_ms: u64,
    pub source_portfolio_wall_ms: u64,
    pub total_wall_ms: u64,
    pub source_child_validated_feasible_count: usize,
    pub source_child_proven_infeasible_count: usize,
    pub source_child_unknown_count: usize,
    pub source_child_invalid_witness_count: usize,
    pub validated_witness_found: bool,
    pub selected_state_infeasibility_proven: bool,
    pub parents: Vec<PriorSourcePortParentReport>,
    pub cases: Vec<PriorSourcePortCaseReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_prior_source_port_portfolio(
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
    completion_case_search_budget: Duration,
    source_case_search_budget: Duration,
) -> Result<PriorSourcePortPortfolioReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if source_case_search_budget.is_zero() {
        return Err(invalid_input(
            "/source_case_search_budget",
            "prior-source port portfolio requires a positive source-case search budget",
        ));
    }
    let completion_stage = diagnose_prior_terminal_completion_portfolio(
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
        completion_case_search_budget,
    )?;
    let source_preparation_started = Instant::now();

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
    let counterpart_source_ids = completion_stage
        .pair_stage
        .terminal_domains
        .iter()
        .map(|domain| {
            domain
                .terminal
                .strip_suffix(":demand")
                .map(|prefix| format!("{prefix}:supply"))
                .ok_or_else(|| {
                    invalid_input(
                        "/source_terminal",
                        format!(
                            "selected demand terminal {} has no :demand role suffix",
                            domain.terminal
                        ),
                    )
                })
        })
        .collect::<Result<BTreeSet<_>, IntegratedLayoutReport>>()?;
    let source_matches = completion_stage
        .pair_stage
        .prior_reference
        .transport_networks
        .iter()
        .flat_map(|network| network.terminals.iter())
        .filter(|terminal| {
            counterpart_source_ids.contains(&terminal.id)
                && terminal.direction == FacilityPortDirection::Output
                && matches!(terminal.endpoint, TransportNetworkEndpoint::Facility { .. })
        })
        .collect::<Vec<_>>();
    if source_matches.len() != 1 {
        return Err(invalid_input(
            "/source_terminal",
            format!(
                "selected demands have {} facility-backed counterpart supplies in the preceding reference",
                source_matches.len()
            ),
        ));
    }
    let source_reference = source_matches[0];
    let source_terminal = source_reference.id.clone();
    let source_reference_position = source_reference.position.clone();
    let (source_facility, source_reference_port) = match &source_reference.endpoint {
        TransportNetworkEndpoint::Facility { instance, port } => (instance.clone(), port.clone()),
        TransportNetworkEndpoint::External { .. } => {
            return Err(invalid_input(
                "/source_terminal",
                format!("terminal {source_terminal} is not attached to a facility"),
            ));
        }
    };
    if !completion_stage
        .pair_stage
        .prior_reference
        .placements
        .iter()
        .any(|placement| placement.instance == source_facility)
    {
        return Err(invalid_input(
            "/source_terminal",
            format!("source facility {source_facility} is absent from the preceding reference"),
        ));
    }
    let already_fixed_terminals = completion_stage
        .pair_stage
        .fixed_ports
        .iter()
        .map(|assignment| assignment.terminal.clone())
        .chain(
            completion_stage
                .pair_stage
                .terminal_domains
                .iter()
                .map(|domain| domain.terminal.clone()),
        )
        .chain(
            completion_stage
                .completion_domains
                .iter()
                .map(|domain| domain.terminal.clone()),
        )
        .collect::<BTreeSet<_>>();
    if already_fixed_terminals.contains(&source_terminal) {
        return Err(invalid_input(
            "/source_terminal",
            format!("source terminal {source_terminal} is already fixed by the parent portfolio"),
        ));
    }
    let source_ports =
        exact::shared_layer::facility_port_partition_domains(&input, &source_facility)
            .map_err(IntegratedLayoutReport::invalid)?
            .into_iter()
            .find(|domain| domain.terminal == source_terminal)
            .map(|domain| domain.ports)
            .ok_or_else(|| {
                invalid_input(
                    "/source_terminal",
                    format!("terminal {source_terminal} has no cumulative exact-model port domain"),
                )
            })?;
    if source_ports.is_empty() {
        return Err(invalid_input(
            "/source_terminal",
            format!("terminal {source_terminal} has an empty compatible port domain"),
        ));
    }

    let source_placement = completion_stage
        .pair_stage
        .prior_reference
        .placements
        .iter()
        .find(|placement| placement.instance == source_facility)
        .expect("validated source facility placement exists");
    let source_definition = facilities
        .facility(&source_placement.facility)
        .ok_or_else(|| {
            invalid_input(
                "/source_terminal",
                format!(
                    "source placement references unknown facility definition {}",
                    source_placement.facility
                ),
            )
        })?;
    let source_connections = candidate_port_connections(
        source_definition,
        source_placement.rotation,
        i32::try_from(source_placement.x).expect("validated placement x fits solver integer"),
        i32::try_from(source_placement.y).expect("validated placement y fits solver integer"),
        fixed_width,
        fixed_height,
    );
    let source_port_positions = source_ports
        .iter()
        .map(|port| {
            source_connections
                .get(port)
                .copied()
                .map(|cell| world_position(cell, fixed_width))
                .ok_or_else(|| {
                    invalid_input(
                        "/source_terminal",
                        format!(
                            "source port {port} has no in-bounds connection cell at the fixed placement"
                        ),
                    )
                })
        })
        .collect::<Result<Vec<_>, IntegratedLayoutReport>>()?;

    let parents = completion_stage
        .cases
        .iter()
        .map(|case| PriorSourcePortParentReport {
            completion_leaf_index: case.leaf_index,
            pair_assignments: case.pair_assignments.clone(),
            completion_assignments: case.completion_assignments.clone(),
            parent_outcome: case.outcome,
            expanded: case.outcome != ExactDimensionCaseOutcome::ProvenInfeasible,
            child_count: if case.outcome == ExactDimensionCaseOutcome::ProvenInfeasible {
                0
            } else {
                source_ports.len()
            },
        })
        .collect::<Vec<_>>();
    let leaf_inputs = enumerate_source_leaves(
        &parents,
        &source_terminal,
        &source_ports,
        &source_port_positions,
    );
    let closed_pair_region_count = completion_stage.closed_parent_count;
    let closed_completion_region_count = completion_stage.child_proven_infeasible_count;
    let expanded_completion_parent_count = parents.iter().filter(|parent| parent.expanded).count();
    let coverage_region_count =
        closed_pair_region_count + closed_completion_region_count + leaf_inputs.len();

    let dimensions = exact::shared_layer::FixedUsedDimensions {
        width: completion_stage.pair_stage.fixed_dimensions[0],
        height: completion_stage.pair_stage.fixed_dimensions[1],
    };
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: completion_stage.pair_stage.partitioned_facility.clone(),
        x: completion_stage.pair_stage.fixed_coordinate[0],
        y: completion_stage.pair_stage.fixed_coordinate[1],
        rotation: Some(completion_stage.pair_stage.fixed_rotation),
    };
    let introduced_exact_ports = completion_stage
        .pair_stage
        .fixed_ports
        .iter()
        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
            terminal: assignment.terminal.clone(),
            port: assignment.port.clone(),
        })
        .collect::<Vec<_>>();
    let fixed_terminal_ids = leaf_inputs
        .first()
        .map(|leaf| {
            distinct_fixed_terminal_ids(
                completion_stage.pair_stage.fixed_ports.iter().chain(
                    leaf.pair_assignments
                        .iter()
                        .chain(leaf.completion_assignments.iter())
                        .chain(std::iter::once(&leaf.source_assignment)),
                ),
            )
        })
        .transpose()
        .map_err(|terminal| {
            invalid_input(
                "/source_terminal",
                format!("duplicate fixed terminal {terminal} in a source-port leaf"),
            )
        })?
        .unwrap_or_else(|| {
            completion_stage
                .pair_stage
                .fixed_ports
                .iter()
                .map(|assignment| assignment.terminal.clone())
                .collect()
        });
    let fixed_terminal_count_per_source_leaf = fixed_terminal_ids.len();
    for leaf in &leaf_inputs {
        let terminal_ids = distinct_fixed_terminal_ids(
            completion_stage.pair_stage.fixed_ports.iter().chain(
                leaf.pair_assignments
                    .iter()
                    .chain(leaf.completion_assignments.iter())
                    .chain(std::iter::once(&leaf.source_assignment)),
            ),
        )
        .map_err(|terminal| {
            invalid_input(
                "/source_terminal",
                format!("duplicate fixed terminal {terminal} in a source-port leaf"),
            )
        })?;
        if terminal_ids != fixed_terminal_ids {
            return Err(invalid_input(
                "/source_terminal",
                "source-port leaves do not fix the same terminal ID set",
            ));
        }
    }
    let mut selected_lane_terminal_ids = completion_stage
        .pair_stage
        .terminal_domains
        .iter()
        .map(|domain| domain.terminal.clone())
        .chain(counterpart_source_ids.iter().cloned())
        .collect::<Vec<_>>();
    selected_lane_terminal_ids.sort();
    selected_lane_terminal_ids.dedup();
    let selected_lane_terminals_fully_fixed = selected_lane_terminal_ids
        .iter()
        .all(|terminal| fixed_terminal_ids.contains(terminal));
    let facility_instances = completion_stage
        .pair_stage
        .prior_reference
        .placements
        .iter()
        .map(|placement| placement.instance.clone())
        .chain(std::iter::once(
            completion_stage.pair_stage.partitioned_facility.clone(),
        ))
        .collect::<BTreeSet<_>>();
    let mut unfixed_facility_terminal_domains = Vec::new();
    for facility in facility_instances {
        unfixed_facility_terminal_domains.extend(
            exact::shared_layer::facility_port_partition_domains(&input, &facility)
                .map_err(IntegratedLayoutReport::invalid)?
                .into_iter()
                .filter(|domain| !fixed_terminal_ids.contains(&domain.terminal))
                .map(|domain| FacilityPortDomainReport {
                    terminal: domain.terminal,
                    ports: domain.ports,
                }),
        );
    }
    unfixed_facility_terminal_domains.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    let source_preparation_ms = millis(source_preparation_started.elapsed());
    let source_portfolio_started = Instant::now();
    let mut completed = Vec::with_capacity(leaf_inputs.len());
    for chunk in leaf_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for leaf in chunk {
                let input = input.clone();
                let coordinate = coordinate.clone();
                let prior_reference = &completion_stage.pair_stage.prior_reference;
                let mut exact_ports = introduced_exact_ports.clone();
                exact_ports.extend(
                    leaf.pair_assignments
                        .iter()
                        .chain(leaf.completion_assignments.iter())
                        .chain(std::iter::once(&leaf.source_assignment))
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
                            Some(source_case_search_budget),
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
                        .expect("prior-source port portfolio worker panicked"),
                ));
            }
        });
    }
    completed.sort_by_key(|(leaf, _)| leaf.source_leaf_index);
    let cases = completed
        .into_iter()
        .map(|(leaf, layout)| {
            let exact = layout
                .exact
                .as_ref()
                .expect("executed source-port leaf has exact model metrics");
            PriorSourcePortCaseReport {
                source_leaf_index: leaf.source_leaf_index,
                parent_completion_leaf_index: leaf.parent_completion_leaf_index,
                pair_assignments: leaf.pair_assignments,
                completion_assignments: leaf.completion_assignments,
                source_assignment: leaf.source_assignment,
                source_position: leaf.source_position,
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
    let source_child_validated_feasible_count =
        count_outcome(&cases, ExactDimensionCaseOutcome::ValidatedFeasible);
    let source_child_proven_infeasible_count =
        count_outcome(&cases, ExactDimensionCaseOutcome::ProvenInfeasible);
    let source_child_unknown_count = count_outcome(&cases, ExactDimensionCaseOutcome::Unknown);
    let source_child_invalid_witness_count =
        count_outcome(&cases, ExactDimensionCaseOutcome::InvalidWitness);
    let validated_witness_found =
        completion_stage.validated_witness_found || source_child_validated_feasible_count > 0;
    let selected_state_infeasibility_proven = closed_pair_region_count
        + closed_completion_region_count
        + source_child_proven_infeasible_count
        == coverage_region_count;

    Ok(PriorSourcePortPortfolioReport {
        schema_version: PRIOR_SOURCE_PORT_PORTFOLIO_SCHEMA_VERSION,
        target_phase_index,
        solver_stack: ExactDimensionSolverStack::WatchedDemandWithLocalContinuation,
        endpoint_encoding: super::super::super::EndpointChannelEncoding::SparseSupport,
        completion_stage,
        source_terminal,
        source_facility,
        source_reference_port,
        source_reference_position,
        source_assignment_count_per_parent: source_ports.len(),
        source_ports,
        source_port_positions,
        fixed_terminal_count_per_source_leaf,
        selected_lane_terminal_ids,
        selected_lane_terminals_fully_fixed,
        unfixed_facility_terminal_domains,
        closed_pair_region_count,
        closed_completion_region_count,
        expanded_completion_parent_count,
        coverage_region_count,
        worker_count,
        source_case_search_budget_ms: millis(source_case_search_budget),
        source_preparation_ms,
        source_portfolio_wall_ms: millis(source_portfolio_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        source_child_validated_feasible_count,
        source_child_proven_infeasible_count,
        source_child_unknown_count,
        source_child_invalid_witness_count,
        validated_witness_found,
        selected_state_infeasibility_proven,
        parents,
        cases,
        diagnostic_only: true,
    })
}

#[derive(Debug, Clone)]
struct SourceLeafInput {
    source_leaf_index: usize,
    parent_completion_leaf_index: usize,
    pair_assignments: Vec<FacilityPortAssignment>,
    completion_assignments: Vec<FacilityPortAssignment>,
    source_assignment: FacilityPortAssignment,
    source_position: WorldGridPosition,
}

fn enumerate_source_leaves(
    parents: &[PriorSourcePortParentReport],
    source_terminal: &str,
    source_ports: &[String],
    source_port_positions: &[WorldGridPosition],
) -> Vec<SourceLeafInput> {
    let mut leaves = Vec::new();
    for parent in parents.iter().filter(|parent| parent.expanded) {
        for (port, position) in source_ports.iter().zip(source_port_positions) {
            leaves.push(SourceLeafInput {
                source_leaf_index: leaves.len(),
                parent_completion_leaf_index: parent.completion_leaf_index,
                pair_assignments: parent.pair_assignments.clone(),
                completion_assignments: parent.completion_assignments.clone(),
                source_assignment: FacilityPortAssignment {
                    terminal: source_terminal.to_string(),
                    port: port.clone(),
                },
                source_position: position.clone(),
            });
        }
    }
    leaves
}

fn distinct_fixed_terminal_ids<'a>(
    assignments: impl Iterator<Item = &'a FacilityPortAssignment>,
) -> Result<BTreeSet<String>, String> {
    let mut terminals = BTreeSet::new();
    for assignment in assignments {
        if !terminals.insert(assignment.terminal.clone()) {
            return Err(assignment.terminal.clone());
        }
    }
    Ok(terminals)
}

fn count_outcome(cases: &[PriorSourcePortCaseReport], outcome: ExactDimensionCaseOutcome) -> usize {
    cases.iter().filter(|case| case.outcome == outcome).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_every_non_infeasible_parent_by_every_source_port() {
        let parents = vec![
            PriorSourcePortParentReport {
                completion_leaf_index: 0,
                pair_assignments: vec![],
                completion_assignments: vec![],
                parent_outcome: ExactDimensionCaseOutcome::ProvenInfeasible,
                expanded: false,
                child_count: 0,
            },
            PriorSourcePortParentReport {
                completion_leaf_index: 4,
                pair_assignments: vec![],
                completion_assignments: vec![],
                parent_outcome: ExactDimensionCaseOutcome::ValidatedFeasible,
                expanded: true,
                child_count: 2,
            },
            PriorSourcePortParentReport {
                completion_leaf_index: 9,
                pair_assignments: vec![],
                completion_assignments: vec![],
                parent_outcome: ExactDimensionCaseOutcome::Unknown,
                expanded: true,
                child_count: 2,
            },
        ];
        let ports = vec!["output-0".to_string(), "output-1".to_string()];

        let positions = vec![
            WorldGridPosition { x: 1, y: 2 },
            WorldGridPosition { x: 3, y: 4 },
        ];

        let leaves = enumerate_source_leaves(&parents, "source", &ports, &positions);

        assert_eq!(leaves.len(), 4);
        assert_eq!(leaves[0].parent_completion_leaf_index, 4);
        assert_eq!(leaves[1].source_assignment.port, "output-1");
        assert_eq!(leaves[2].parent_completion_leaf_index, 9);
        assert_eq!(leaves[3].source_assignment.terminal, "source");
        assert_eq!(leaves[3].source_position, positions[1]);
    }

    #[test]
    fn rejects_duplicate_fixed_terminal_assignments() {
        let assignments = [
            FacilityPortAssignment {
                terminal: "same".to_string(),
                port: "input-0".to_string(),
            },
            FacilityPortAssignment {
                terminal: "same".to_string(),
                port: "input-1".to_string(),
            },
        ];

        assert_eq!(
            distinct_fixed_terminal_ids(assignments.iter()),
            Err("same".to_string())
        );
    }
}
