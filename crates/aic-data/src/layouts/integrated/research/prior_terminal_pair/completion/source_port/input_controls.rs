use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    TransportKind, ValidatedItemCatalog, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::super::super::super::{
    ExactSearchStatistics, IntegratedLayoutReport, WorldGridPosition, candidate_port_connections,
    exact, world_position,
};
use super::super::super::super::coordinate_partition::{
    FacilityPortAssignment, FacilityPortDomainReport, PartitionCaseModelScale, classify_outcome,
    invalid_input, millis, model_scale, prepare_target_input,
};
use super::super::super::super::{ExactDimensionCaseOutcome, ExactDimensionSolverStack};
use super::{PriorSourcePortPortfolioReport, diagnose_prior_source_port_portfolio};

mod pair;

pub use pair::{
    EXTERNAL_BOUNDARY_KEY_LEGAL_SUPPORT_AB_SCHEMA_VERSION, ExternalBoundaryKeyCommonModelContract,
    ExternalBoundaryKeyLegalSupportAbReport, ExternalBoundaryKeyNetworkContract,
    ExternalBoundaryKeyRootComparison, ExternalBoundaryKeyRootTotals,
    ExternalBoundaryKeySolveReport, ExternalBoundaryKeyStaticCertificate,
    PRIOR_INPUT_PAIR_ROOT_SNAPSHOT_SCHEMA_VERSION, PRIOR_INPUT_PORT_PAIR_PORTFOLIO_SCHEMA_VERSION,
    PriorInputPairRootSnapshotReport, PriorInputPortPairCaseReport,
    PriorInputPortPairPortfolioReport, PriorInputPortProofExclusion, PriorInputPortResidualDomain,
    RESIDUAL_FACILITY_PORT_TUPLE_PORTFOLIO_SCHEMA_VERSION, ResidualFacilityPortDomain,
    ResidualFacilityPortFixationObservation, ResidualFacilityPortTupleCaseReport,
    ResidualFacilityPortTuplePortfolioReport, diagnose_external_boundary_key_legal_support_ab,
    diagnose_prior_input_pair_root_snapshot, diagnose_prior_input_port_pair_portfolio,
    diagnose_residual_facility_port_tuple_portfolio,
};

pub const PRIOR_INPUT_PORT_CONTROLS_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorInputPortControlCaseReport {
    pub suite_index: usize,
    pub case_index: usize,
    pub assignment: FacilityPortAssignment,
    pub connection_position: Option<WorldGridPosition>,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub model_scale: PartitionCaseModelScale,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorInputPortControlSuiteReport {
    pub suite_index: usize,
    pub terminal: String,
    pub ports: Vec<String>,
    pub port_positions: Vec<Option<WorldGridPosition>>,
    pub validated_feasible_count: usize,
    pub proven_infeasible_count: usize,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub validated_witness_found: bool,
    pub complete_infeasibility_proven: bool,
    pub cases: Vec<PriorInputPortControlCaseReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorInputPortControlsReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub solver_stack: ExactDimensionSolverStack,
    pub endpoint_encoding: super::super::super::super::EndpointChannelEncoding,
    pub source_stage: PriorSourcePortPortfolioReport,
    pub representative_source_leaf_index: usize,
    pub representative_parent_outcome: ExactDimensionCaseOutcome,
    pub inherited_assignments: Vec<FacilityPortAssignment>,
    pub inherited_terminal_count: usize,
    pub controlled_domains: Vec<FacilityPortDomainReport>,
    pub suite_count: usize,
    pub cases_per_suite: usize,
    pub suites_overlap: bool,
    pub worker_count: usize,
    pub case_search_budget_ms: u64,
    pub preparation_ms: u64,
    pub control_wave_wall_ms: u64,
    pub total_wall_ms: u64,
    pub representative_witness_found: bool,
    pub representative_infeasibility_proven: bool,
    pub invalid_witness_found: bool,
    pub suites: Vec<PriorInputPortControlSuiteReport>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_prior_input_port_controls(
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
    representative_source_leaf_index: usize,
    worker_count: usize,
    prefix_search_budget: Duration,
    pair_case_search_budget: Duration,
    completion_case_search_budget: Duration,
    source_case_search_budget: Duration,
    control_case_search_budget: Duration,
) -> Result<PriorInputPortControlsReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if control_case_search_budget.is_zero() {
        return Err(invalid_input(
            "/control_case_search_budget",
            "prior-input controls require a positive case search budget",
        ));
    }
    let source_stage = diagnose_prior_source_port_portfolio(
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
        source_case_search_budget,
    )?;
    let preparation_started = Instant::now();
    let representative_matches = source_stage
        .cases
        .iter()
        .filter(|case| case.source_leaf_index == representative_source_leaf_index)
        .collect::<Vec<_>>();
    if representative_matches.len() != 1 {
        return Err(invalid_input(
            "/representative_source_leaf_index",
            format!(
                "source leaf {representative_source_leaf_index} must occur exactly once, found {} occurrences",
                representative_matches.len()
            ),
        ));
    }
    let representative = representative_matches.first().copied().ok_or_else(|| {
        invalid_input(
            "/representative_source_leaf_index",
            format!(
                "source leaf {representative_source_leaf_index} is absent from the source portfolio"
            ),
        )
    })?;
    if matches!(
        representative.outcome,
        ExactDimensionCaseOutcome::ProvenInfeasible | ExactDimensionCaseOutcome::InvalidWitness
    ) {
        return Err(invalid_input(
            "/representative_source_leaf_index",
            format!(
                "source leaf {representative_source_leaf_index} has non-expandable outcome {:?}",
                representative.outcome
            ),
        ));
    }
    let inherited_assignments = source_stage
        .completion_stage
        .pair_stage
        .fixed_ports
        .iter()
        .cloned()
        .chain(representative.pair_assignments.iter().cloned())
        .chain(representative.completion_assignments.iter().cloned())
        .chain(std::iter::once(representative.source_assignment.clone()))
        .collect::<Vec<_>>();
    let inherited_terminal_ids =
        distinct_terminal_ids(&inherited_assignments).map_err(|terminal| {
            invalid_input(
                "/representative_source_leaf_index",
                format!("representative leaf repeats fixed terminal {terminal}"),
            )
        })?;
    if inherited_terminal_ids.len() != source_stage.fixed_terminal_count_per_source_leaf {
        return Err(invalid_input(
            "/representative_source_leaf_index",
            "representative inherited terminal count differs from the source-stage contract",
        ));
    }

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
    let source_placement = source_stage
        .completion_stage
        .pair_stage
        .prior_reference
        .placements
        .iter()
        .find(|placement| placement.instance == source_stage.source_facility)
        .expect("validated source facility placement exists");
    let source_definition = facilities
        .facility(&source_placement.facility)
        .ok_or_else(|| {
            invalid_input(
                "/controlled_domains",
                format!(
                    "source placement references unknown facility definition {}",
                    source_placement.facility
                ),
            )
        })?;
    let port_definitions = source_definition
        .ports
        .iter()
        .map(|port| (port.id.as_str(), port))
        .collect::<BTreeMap<_, _>>();
    let controlled_domains =
        exact::shared_layer::facility_port_partition_domains(&input, &source_stage.source_facility)
            .map_err(IntegratedLayoutReport::invalid)?
            .into_iter()
            .filter(|domain| {
                !inherited_terminal_ids.contains(&domain.terminal)
                    && domain.ports.len() > 1
                    && domain.direction == FacilityPortDirection::Input
                    && domain.transport == TransportKind::Belt
                    && domain.ports.iter().all(|port| {
                        port_definitions
                            .get(port.as_str())
                            .is_some_and(|definition| {
                                definition.direction == FacilityPortDirection::Input
                                    && definition.transport == TransportKind::Belt
                            })
                    })
            })
            .map(|domain| FacilityPortDomainReport {
                terminal: domain.terminal,
                ports: domain.ports,
            })
            .collect::<Vec<_>>();
    if controlled_domains.len() != 2 {
        return Err(invalid_input(
            "/controlled_domains",
            format!(
                "representative prior-input controls require exactly two non-singleton belt-demand domains, found {}",
                controlled_domains.len()
            ),
        ));
    }
    if controlled_domains
        .iter()
        .any(|domain| domain.ports.len() != 5 || !all_values_distinct(&domain.ports))
    {
        return Err(invalid_input(
            "/controlled_domains",
            "each controlled belt-demand domain must contain five distinct port values",
        ));
    }

    let source_connections = candidate_port_connections(
        source_definition,
        source_placement.rotation,
        i32::try_from(source_placement.x).expect("validated placement x fits solver integer"),
        i32::try_from(source_placement.y).expect("validated placement y fits solver integer"),
        fixed_width,
        fixed_height,
    );
    let suite_inputs = controlled_domains
        .iter()
        .enumerate()
        .map(|(suite_index, domain)| {
            let cases = domain
                .ports
                .iter()
                .enumerate()
                .map(|(case_index, port)| {
                    let connection_position = source_connections
                        .get(port)
                        .copied()
                        .map(|cell| world_position(cell, fixed_width));
                    Ok(ControlCaseInput {
                        suite_index,
                        case_index,
                        assignment: FacilityPortAssignment {
                            terminal: domain.terminal.clone(),
                            port: port.clone(),
                        },
                        connection_position,
                    })
                })
                .collect::<Result<Vec<_>, IntegratedLayoutReport>>()?;
            Ok(ControlSuiteInput {
                suite_index,
                terminal: domain.terminal.clone(),
                ports: domain.ports.clone(),
                cases,
            })
        })
        .collect::<Result<Vec<_>, IntegratedLayoutReport>>()?;
    for suite in &suite_inputs {
        for case in &suite.cases {
            let mut assignments = inherited_assignments.clone();
            assignments.push(case.assignment.clone());
            if distinct_terminal_ids(&assignments).is_err() {
                return Err(invalid_input(
                    "/controlled_domains",
                    format!(
                        "control suite {} repeats terminal {}",
                        suite.suite_index, case.assignment.terminal
                    ),
                ));
            }
        }
    }

    let dimensions = exact::shared_layer::FixedUsedDimensions {
        width: source_stage.completion_stage.pair_stage.fixed_dimensions[0],
        height: source_stage.completion_stage.pair_stage.fixed_dimensions[1],
    };
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: source_stage
            .completion_stage
            .pair_stage
            .partitioned_facility
            .clone(),
        x: source_stage.completion_stage.pair_stage.fixed_coordinate[0],
        y: source_stage.completion_stage.pair_stage.fixed_coordinate[1],
        rotation: Some(source_stage.completion_stage.pair_stage.fixed_rotation),
    };
    let preparation_ms = millis(preparation_started.elapsed());
    let wave_started = Instant::now();
    let case_inputs = suite_inputs
        .iter()
        .flat_map(|suite| suite.cases.iter().cloned())
        .collect::<Vec<_>>();
    let mut completed = Vec::with_capacity(case_inputs.len());
    for chunk in case_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for case in chunk {
                let input = input.clone();
                let coordinate = coordinate.clone();
                let prior_reference = &source_stage.completion_stage.pair_stage.prior_reference;
                let mut exact_ports = inherited_assignments
                    .iter()
                    .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
                        terminal: assignment.terminal.clone(),
                        port: assignment.port.clone(),
                    })
                    .collect::<Vec<_>>();
                exact_ports.push(exact::shared_layer::FixedTerminalPortChoice {
                    terminal: case.assignment.terminal.clone(),
                    port: case.assignment.port.clone(),
                });
                let case = case.clone();
                handles.push((
                    case.clone(),
                    scope.spawn(move || {
                        exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                            input,
                            logistics_components,
                            Some(control_case_search_budget),
                            dimensions,
                            coordinate,
                            exact_ports,
                            prior_reference,
                            exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements,
                        )
                    }),
                ));
            }
            for (case, handle) in handles {
                completed.push((
                    case,
                    handle.join().expect("prior-input control worker panicked"),
                ));
            }
        });
    }
    completed.sort_by_key(|(case, _)| (case.suite_index, case.case_index));
    let mut suites = Vec::with_capacity(suite_inputs.len());
    for suite_input in suite_inputs {
        let cases = completed
            .iter()
            .filter(|(case, _)| case.suite_index == suite_input.suite_index)
            .map(|(case, layout)| {
                let exact = layout
                    .exact
                    .as_ref()
                    .expect("executed prior-input control has exact model metrics");
                PriorInputPortControlCaseReport {
                    suite_index: case.suite_index,
                    case_index: case.case_index,
                    assignment: case.assignment.clone(),
                    connection_position: case.connection_position.clone(),
                    outcome: classify_outcome(layout),
                    construction_ms: exact.construction_ms,
                    search_ms: exact.search_ms,
                    first_incumbent_ms: exact.first_incumbent_ms,
                    search_statistics: exact.search_statistics,
                    model_scale: model_scale(exact),
                    layout: layout.clone(),
                }
            })
            .collect::<Vec<_>>();
        let validated_feasible_count =
            count_outcome(&cases, ExactDimensionCaseOutcome::ValidatedFeasible);
        let proven_infeasible_count =
            count_outcome(&cases, ExactDimensionCaseOutcome::ProvenInfeasible);
        let unknown_count = count_outcome(&cases, ExactDimensionCaseOutcome::Unknown);
        let invalid_witness_count =
            count_outcome(&cases, ExactDimensionCaseOutcome::InvalidWitness);
        suites.push(PriorInputPortControlSuiteReport {
            suite_index: suite_input.suite_index,
            terminal: suite_input.terminal,
            ports: suite_input.ports,
            port_positions: cases
                .iter()
                .map(|case| case.connection_position.clone())
                .collect(),
            validated_feasible_count,
            proven_infeasible_count,
            unknown_count,
            invalid_witness_count,
            validated_witness_found: validated_feasible_count > 0,
            complete_infeasibility_proven: complete_partition_infeasibility(
                proven_infeasible_count,
                cases.len(),
            ),
            cases,
        });
    }
    let representative_witness_found = representative.outcome
        == ExactDimensionCaseOutcome::ValidatedFeasible
        || suites.iter().any(|suite| suite.validated_witness_found);
    let representative_infeasibility_proven = suites
        .iter()
        .any(|suite| suite.complete_infeasibility_proven);
    let invalid_witness_found = suites.iter().any(|suite| suite.invalid_witness_count > 0);

    Ok(PriorInputPortControlsReport {
        schema_version: PRIOR_INPUT_PORT_CONTROLS_SCHEMA_VERSION,
        target_phase_index,
        solver_stack: ExactDimensionSolverStack::WatchedDemandWithLocalContinuation,
        endpoint_encoding: super::super::super::super::EndpointChannelEncoding::SparseSupport,
        representative_source_leaf_index,
        representative_parent_outcome: representative.outcome,
        inherited_terminal_count: inherited_assignments.len(),
        inherited_assignments,
        controlled_domains,
        suite_count: suites.len(),
        cases_per_suite: suites.first().map_or(0, |suite| suite.cases.len()),
        suites_overlap: true,
        worker_count,
        case_search_budget_ms: millis(control_case_search_budget),
        preparation_ms,
        control_wave_wall_ms: millis(wave_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        representative_witness_found,
        representative_infeasibility_proven,
        invalid_witness_found,
        suites,
        source_stage,
        diagnostic_only: true,
    })
}

#[derive(Debug, Clone)]
struct ControlSuiteInput {
    suite_index: usize,
    terminal: String,
    ports: Vec<String>,
    cases: Vec<ControlCaseInput>,
}

#[derive(Debug, Clone)]
struct ControlCaseInput {
    suite_index: usize,
    case_index: usize,
    assignment: FacilityPortAssignment,
    connection_position: Option<WorldGridPosition>,
}

fn distinct_terminal_ids(
    assignments: &[FacilityPortAssignment],
) -> Result<BTreeSet<String>, String> {
    let mut terminals = BTreeSet::new();
    for assignment in assignments {
        if !terminals.insert(assignment.terminal.clone()) {
            return Err(assignment.terminal.clone());
        }
    }
    Ok(terminals)
}

fn count_outcome(
    cases: &[PriorInputPortControlCaseReport],
    outcome: ExactDimensionCaseOutcome,
) -> usize {
    cases.iter().filter(|case| case.outcome == outcome).count()
}

fn all_values_distinct(values: &[String]) -> bool {
    values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

fn complete_partition_infeasibility(proven_infeasible_count: usize, case_count: usize) -> bool {
    case_count > 0 && proven_infeasible_count == case_count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_terminal_ids_rejects_a_repeated_control() {
        let assignments = vec![
            FacilityPortAssignment {
                terminal: "same".to_string(),
                port: "a".to_string(),
            },
            FacilityPortAssignment {
                terminal: "same".to_string(),
                port: "b".to_string(),
            },
        ];

        assert_eq!(distinct_terminal_ids(&assignments), Err("same".to_string()));
    }

    #[test]
    fn partial_proofs_never_close_a_complete_partition() {
        assert!(!complete_partition_infeasibility(0, 5));
        assert!(!complete_partition_infeasibility(1, 5));
        assert!(!complete_partition_infeasibility(4, 5));
        assert!(complete_partition_infeasibility(5, 5));
        assert!(!complete_partition_infeasibility(0, 0));
    }

    #[test]
    fn out_of_bounds_connection_remains_a_control_case_value() {
        let case = ControlCaseInput {
            suite_index: 0,
            case_index: 4,
            assignment: FacilityPortAssignment {
                terminal: "terminal".to_string(),
                port: "input-belt-4".to_string(),
            },
            connection_position: None,
        };

        assert_eq!(case.assignment.port, "input-belt-4");
        assert!(case.connection_position.is_none());
    }
}
