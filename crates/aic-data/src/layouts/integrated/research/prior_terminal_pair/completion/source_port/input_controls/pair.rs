use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::super::super::super::super::{
    ExactSearchStatistics, IntegratedLayoutReport, RootDomainSnapshot, WorldGridPosition, exact,
};
use super::super::super::super::super::coordinate_partition::{
    FacilityPortAssignment, PartitionCaseModelScale, classify_outcome, invalid_input, millis,
    model_scale, prepare_target_input,
};
use super::super::super::super::super::{ExactDimensionCaseOutcome, ExactDimensionSolverStack};
use super::{PriorInputPortControlSuiteReport, PriorInputPortControlsReport};

pub const PRIOR_INPUT_PORT_PAIR_PORTFOLIO_SCHEMA_VERSION: u32 = 1;
pub const PRIOR_INPUT_PAIR_ROOT_SNAPSHOT_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorInputPortProofExclusion {
    pub suite_index: usize,
    pub terminal: String,
    pub port: String,
    pub outcome: ExactDimensionCaseOutcome,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PriorInputPortResidualDomain {
    pub suite_index: usize,
    pub terminal: String,
    pub ports: Vec<String>,
    pub port_positions: Vec<Option<WorldGridPosition>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorInputPortPairCaseReport {
    pub pair_index: usize,
    pub assignments: Vec<FacilityPortAssignment>,
    pub connection_positions: Vec<Option<WorldGridPosition>>,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub model_scale: PartitionCaseModelScale,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorInputPortPairPortfolioReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub solver_stack: ExactDimensionSolverStack,
    pub endpoint_encoding: super::super::super::super::super::EndpointChannelEncoding,
    pub control_stage: PriorInputPortControlsReport,
    pub representative_source_leaf_index: usize,
    pub inherited_assignments: Vec<FacilityPortAssignment>,
    pub inherited_terminal_count: usize,
    pub proof_exclusions: Vec<PriorInputPortProofExclusion>,
    pub residual_domains: Vec<PriorInputPortResidualDomain>,
    pub excluded_atomic_pair_count: usize,
    pub residual_pair_count: usize,
    pub fixed_terminal_count_per_pair: usize,
    pub worker_count: usize,
    pub pair_case_search_budget_ms: u64,
    pub preparation_ms: u64,
    pub pair_wave_wall_ms: u64,
    pub total_wall_ms: u64,
    pub validated_feasible_count: usize,
    pub proven_infeasible_count: usize,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub representative_witness_found: bool,
    pub representative_infeasibility_proven: bool,
    pub invalid_witness_found: bool,
    pub predeclared_observation_pair_index: Option<usize>,
    pub cases: Vec<PriorInputPortPairCaseReport>,
    pub diagnostic_only: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PriorInputPairRootSnapshotReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub solver_stack: ExactDimensionSolverStack,
    pub selection_rule: String,
    pub selected_pair_index: usize,
    pub baseline_outcome: ExactDimensionCaseOutcome,
    pub assignments: Vec<FacilityPortAssignment>,
    pub fixed_terminal_count: usize,
    pub baseline_search_statistics: ExactSearchStatistics,
    pub baseline_model_scale: PartitionCaseModelScale,
    pub observation_search_budget_ms: u64,
    pub observed_outcome: ExactDimensionCaseOutcome,
    pub observed_layout: IntegratedLayoutReport,
    pub root_snapshot: RootDomainSnapshot,
    pub interpretation_blocked: bool,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_prior_input_port_pair_portfolio(
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
    initial_pair_case_search_budget: Duration,
    completion_case_search_budget: Duration,
    source_case_search_budget: Duration,
    control_case_search_budget: Duration,
    residual_pair_case_search_budget: Duration,
) -> Result<PriorInputPortPairPortfolioReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if residual_pair_case_search_budget.is_zero() {
        return Err(invalid_input(
            "/residual_pair_case_search_budget",
            "prior-input pair portfolio requires a positive case search budget",
        ));
    }
    let control_stage = super::diagnose_prior_input_port_controls(
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
        representative_source_leaf_index,
        worker_count,
        prefix_search_budget,
        initial_pair_case_search_budget,
        completion_case_search_budget,
        source_case_search_budget,
        control_case_search_budget,
    )?;
    let preparation_started = Instant::now();
    if control_stage.invalid_witness_found {
        return Err(invalid_input(
            "/control_stage",
            "prior-input pair portfolio cannot interpret a control stage with an invalid witness",
        ));
    }
    if control_stage.suites.len() != 2 {
        return Err(invalid_input(
            "/control_stage/suites",
            format!(
                "prior-input pair portfolio requires exactly two control suites, found {}",
                control_stage.suites.len()
            ),
        ));
    }
    let suite_terminals = control_stage
        .suites
        .iter()
        .map(|suite| suite.terminal.as_str())
        .collect::<BTreeSet<_>>();
    if suite_terminals.len() != control_stage.suites.len() {
        return Err(invalid_input(
            "/control_stage/suites",
            "prior-input pair control suites repeat a terminal",
        ));
    }

    let (residual_domains, proof_exclusions) = derive_residual_domains(&control_stage.suites)?;
    let original_atomic_pair_count =
        control_stage
            .suites
            .iter()
            .try_fold(1_usize, |product, suite| {
                product.checked_mul(suite.ports.len()).ok_or_else(|| {
                    invalid_input(
                        "/control_stage/suites",
                        "original input-port pair count overflowed usize",
                    )
                })
            })?;
    let pair_inputs = enumerate_residual_pairs(&residual_domains)?;
    let excluded_atomic_pair_count = original_atomic_pair_count
        .checked_sub(pair_inputs.len())
        .ok_or_else(|| {
            invalid_input(
                "/residual_domains",
                "residual pair count exceeds the original Cartesian product",
            )
        })?;

    let inherited_assignments = control_stage.inherited_assignments.clone();
    let inherited_terminals = inherited_assignments
        .iter()
        .map(|assignment| assignment.terminal.as_str())
        .collect::<BTreeSet<_>>();
    if inherited_terminals.len() != control_stage.inherited_terminal_count {
        return Err(invalid_input(
            "/inherited_assignments",
            "control-stage inherited assignments are not distinct",
        ));
    }
    for pair in &pair_inputs {
        let mut terminals = inherited_terminals.clone();
        for assignment in &pair.assignments {
            if !terminals.insert(&assignment.terminal) {
                return Err(invalid_input(
                    "/residual_domains",
                    format!(
                        "residual pair repeats fixed terminal {}",
                        assignment.terminal
                    ),
                ));
            }
        }
        if terminals.len() != inherited_assignments.len() + 2 {
            return Err(invalid_input(
                "/residual_domains",
                "residual pair does not add exactly two terminal assignments",
            ));
        }
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
    let pair_stage = &control_stage.source_stage.completion_stage.pair_stage;
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
    let preparation_ms = millis(preparation_started.elapsed());
    let wave_started = Instant::now();
    let mut completed = Vec::with_capacity(pair_inputs.len());
    for chunk in pair_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for pair in chunk {
                let input = input.clone();
                let coordinate = coordinate.clone();
                let prior_reference = &pair_stage.prior_reference;
                let mut exact_ports = inherited_assignments
                    .iter()
                    .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
                        terminal: assignment.terminal.clone(),
                        port: assignment.port.clone(),
                    })
                    .collect::<Vec<_>>();
                exact_ports.extend(pair.assignments.iter().map(|assignment| {
                    exact::shared_layer::FixedTerminalPortChoice {
                        terminal: assignment.terminal.clone(),
                        port: assignment.port.clone(),
                    }
                }));
                let pair = pair.clone();
                handles.push((
                    pair.clone(),
                    scope.spawn(move || {
                        exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                            input,
                            logistics_components,
                            Some(residual_pair_case_search_budget),
                            dimensions,
                            coordinate,
                            exact_ports,
                            prior_reference,
                            exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements,
                        )
                    }),
                ));
            }
            for (pair, handle) in handles {
                completed.push((
                    pair,
                    handle.join().expect("prior-input pair worker panicked"),
                ));
            }
        });
    }
    completed.sort_by_key(|(pair, _)| pair.pair_index);
    let cases = completed
        .into_iter()
        .map(|(pair, layout)| {
            let exact = layout
                .exact
                .as_ref()
                .expect("executed prior-input pair has exact model metrics");
            PriorInputPortPairCaseReport {
                pair_index: pair.pair_index,
                assignments: pair.assignments,
                connection_positions: pair.connection_positions,
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
    let validated_feasible_count =
        count_outcome(&cases, ExactDimensionCaseOutcome::ValidatedFeasible);
    let proven_infeasible_count =
        count_outcome(&cases, ExactDimensionCaseOutcome::ProvenInfeasible);
    let unknown_count = count_outcome(&cases, ExactDimensionCaseOutcome::Unknown);
    let invalid_witness_count = count_outcome(&cases, ExactDimensionCaseOutcome::InvalidWitness);
    let residual_complete_infeasibility =
        !cases.is_empty() && proven_infeasible_count == cases.len();
    let representative_witness_found =
        control_stage.representative_witness_found || validated_feasible_count > 0;
    let representative_infeasibility_proven =
        control_stage.representative_infeasibility_proven || residual_complete_infeasibility;
    let invalid_witness_found = invalid_witness_count > 0;

    Ok(PriorInputPortPairPortfolioReport {
        schema_version: PRIOR_INPUT_PORT_PAIR_PORTFOLIO_SCHEMA_VERSION,
        target_phase_index,
        solver_stack: ExactDimensionSolverStack::WatchedDemandWithLocalContinuation,
        endpoint_encoding:
            super::super::super::super::super::EndpointChannelEncoding::SparseSupport,
        representative_source_leaf_index,
        inherited_terminal_count: inherited_assignments.len(),
        inherited_assignments,
        proof_exclusions,
        residual_domains,
        excluded_atomic_pair_count,
        residual_pair_count: cases.len(),
        fixed_terminal_count_per_pair: control_stage.inherited_terminal_count + 2,
        worker_count,
        pair_case_search_budget_ms: millis(residual_pair_case_search_budget),
        preparation_ms,
        pair_wave_wall_ms: millis(wave_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        validated_feasible_count,
        proven_infeasible_count,
        unknown_count,
        invalid_witness_count,
        representative_witness_found,
        representative_infeasibility_proven,
        invalid_witness_found,
        predeclared_observation_pair_index: (!cases.is_empty()).then_some(0),
        cases,
        control_stage,
        diagnostic_only: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_prior_input_pair_root_snapshot(
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
    initial_pair_case_search_budget: Duration,
    completion_case_search_budget: Duration,
    source_case_search_budget: Duration,
    control_case_search_budget: Duration,
    residual_pair_case_search_budget: Duration,
    observation_search_budget: Duration,
) -> Result<PriorInputPairRootSnapshotReport, IntegratedLayoutReport> {
    if observation_search_budget.is_zero() {
        return Err(invalid_input(
            "/observation_search_budget",
            "root-domain observation requires a positive search budget",
        ));
    }
    let portfolio = diagnose_prior_input_port_pair_portfolio(
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
        representative_source_leaf_index,
        worker_count,
        prefix_search_budget,
        initial_pair_case_search_budget,
        completion_case_search_budget,
        source_case_search_budget,
        control_case_search_budget,
        residual_pair_case_search_budget,
    )?;
    let selected_pair_index = lowest_unknown_pair_index(
        portfolio
            .cases
            .iter()
            .map(|case| (case.pair_index, case.outcome)),
    )
    .ok_or_else(|| {
        invalid_input(
            "/cases",
            "root-domain observation requires at least one completed Unknown pair case",
        )
    })?;
    let selected = portfolio
        .cases
        .iter()
        .find(|case| case.pair_index == selected_pair_index)
        .expect("selected unknown pair is present")
        .clone();

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
    let pair_stage = &portfolio
        .control_stage
        .source_stage
        .completion_stage
        .pair_stage;
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
    let mut exact_ports = portfolio
        .inherited_assignments
        .iter()
        .chain(&selected.assignments)
        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
            terminal: assignment.terminal.clone(),
            port: assignment.port.clone(),
        })
        .collect::<Vec<_>>();
    exact_ports.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    let (observed_layout, root_snapshot) = exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
        input,
        logistics_components,
        Some(observation_search_budget),
        dimensions,
        coordinate,
        exact_ports,
        &pair_stage.prior_reference,
        exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements,
    );
    let root_snapshot = root_snapshot.ok_or_else(|| {
        invalid_input(
            "/root_snapshot",
            "root-domain observer was not called and the solve did not prove root infeasibility",
        )
    })?;
    let observed_outcome = classify_outcome(&observed_layout);
    let interpretation_blocked = observed_outcome == ExactDimensionCaseOutcome::InvalidWitness
        || !root_snapshot.fixed_facility_contract_satisfied
        || !root_snapshot.fixed_terminal_contract_satisfied;

    Ok(PriorInputPairRootSnapshotReport {
        schema_version: PRIOR_INPUT_PAIR_ROOT_SNAPSHOT_SCHEMA_VERSION,
        target_phase_index,
        solver_stack: portfolio.solver_stack,
        selection_rule: "minimum pair_index among completed Unknown cases".to_string(),
        selected_pair_index,
        baseline_outcome: selected.outcome,
        assignments: selected.assignments,
        fixed_terminal_count: portfolio.fixed_terminal_count_per_pair,
        baseline_search_statistics: selected.search_statistics,
        baseline_model_scale: selected.model_scale,
        observation_search_budget_ms: millis(observation_search_budget),
        observed_outcome,
        observed_layout,
        root_snapshot,
        interpretation_blocked,
        diagnostic_only: true,
    })
}

fn lowest_unknown_pair_index(
    cases: impl IntoIterator<Item = (usize, ExactDimensionCaseOutcome)>,
) -> Option<usize> {
    cases
        .into_iter()
        .filter_map(|(index, outcome)| {
            (outcome == ExactDimensionCaseOutcome::Unknown).then_some(index)
        })
        .min()
}

fn derive_residual_domains(
    suites: &[PriorInputPortControlSuiteReport],
) -> Result<
    (
        Vec<PriorInputPortResidualDomain>,
        Vec<PriorInputPortProofExclusion>,
    ),
    IntegratedLayoutReport,
> {
    let mut domains = Vec::with_capacity(suites.len());
    let mut exclusions = Vec::new();
    for suite in suites {
        let mut residual_ports = Vec::new();
        let mut residual_positions = Vec::new();
        for port in &suite.ports {
            let matches = suite
                .cases
                .iter()
                .filter(|case| case.assignment.port == *port)
                .collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(invalid_input(
                    "/control_stage/suites/cases",
                    format!(
                        "control suite {} port {port} must occur exactly once, found {} cases",
                        suite.suite_index,
                        matches.len()
                    ),
                ));
            }
            let case = matches[0];
            if case.assignment.terminal != suite.terminal {
                return Err(invalid_input(
                    "/control_stage/suites/cases",
                    format!(
                        "control suite {} case terminal differs from its suite terminal",
                        suite.suite_index
                    ),
                ));
            }
            match residual_outcome_disposition(case.outcome) {
                Some(false) => {
                    exclusions.push(PriorInputPortProofExclusion {
                        suite_index: suite.suite_index,
                        terminal: suite.terminal.clone(),
                        port: port.clone(),
                        outcome: case.outcome,
                    });
                }
                None => {
                    return Err(invalid_input(
                        "/control_stage/suites/cases",
                        format!(
                            "control suite {} port {port} has an invalid witness",
                            suite.suite_index
                        ),
                    ));
                }
                Some(true) => {
                    residual_ports.push(port.clone());
                    residual_positions.push(case.connection_position.clone());
                }
            }
        }
        if suite.cases.len() != suite.ports.len() {
            return Err(invalid_input(
                "/control_stage/suites/cases",
                format!(
                    "control suite {} contains cases outside its declared port domain",
                    suite.suite_index
                ),
            ));
        }
        domains.push(PriorInputPortResidualDomain {
            suite_index: suite.suite_index,
            terminal: suite.terminal.clone(),
            ports: residual_ports,
            port_positions: residual_positions,
        });
    }
    Ok((domains, exclusions))
}

fn enumerate_residual_pairs(
    domains: &[PriorInputPortResidualDomain],
) -> Result<Vec<PairCaseInput>, IntegratedLayoutReport> {
    if domains.len() != 2 {
        return Err(invalid_input(
            "/residual_domains",
            "prior-input pair enumeration requires exactly two residual domains",
        ));
    }
    let mut pairs = Vec::new();
    for (left_index, left_port) in domains[0].ports.iter().enumerate() {
        for (right_index, right_port) in domains[1].ports.iter().enumerate() {
            pairs.push(PairCaseInput {
                pair_index: pairs.len(),
                assignments: vec![
                    FacilityPortAssignment {
                        terminal: domains[0].terminal.clone(),
                        port: left_port.clone(),
                    },
                    FacilityPortAssignment {
                        terminal: domains[1].terminal.clone(),
                        port: right_port.clone(),
                    },
                ],
                connection_positions: vec![
                    domains[0].port_positions[left_index].clone(),
                    domains[1].port_positions[right_index].clone(),
                ],
            });
        }
    }
    Ok(pairs)
}

fn count_outcome(
    cases: &[PriorInputPortPairCaseReport],
    outcome: ExactDimensionCaseOutcome,
) -> usize {
    cases.iter().filter(|case| case.outcome == outcome).count()
}

fn residual_outcome_disposition(outcome: ExactDimensionCaseOutcome) -> Option<bool> {
    match outcome {
        ExactDimensionCaseOutcome::ProvenInfeasible => Some(false),
        ExactDimensionCaseOutcome::ValidatedFeasible | ExactDimensionCaseOutcome::Unknown => {
            Some(true)
        }
        ExactDimensionCaseOutcome::InvalidWitness => None,
    }
}

#[derive(Debug, Clone)]
struct PairCaseInput {
    pair_index: usize,
    assignments: Vec<FacilityPortAssignment>,
    connection_positions: Vec<Option<WorldGridPosition>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residual_pair_enumeration_is_complete_and_keeps_equal_ports() {
        let domains = vec![
            PriorInputPortResidualDomain {
                suite_index: 0,
                terminal: "left".to_string(),
                ports: vec!["p0".to_string(), "p1".to_string()],
                port_positions: vec![None, None],
            },
            PriorInputPortResidualDomain {
                suite_index: 1,
                terminal: "right".to_string(),
                ports: vec!["p0".to_string(), "p1".to_string()],
                port_positions: vec![None, None],
            },
        ];

        let pairs = enumerate_residual_pairs(&domains).unwrap();

        assert_eq!(pairs.len(), 4);
        assert_eq!(pairs[0].assignments[0].port, "p0");
        assert_eq!(pairs[0].assignments[1].port, "p0");
        assert_eq!(pairs[3].assignments[0].port, "p1");
        assert_eq!(pairs[3].assignments[1].port, "p1");
    }

    #[test]
    fn residual_domains_remove_only_proven_infeasibility() {
        assert_eq!(
            residual_outcome_disposition(ExactDimensionCaseOutcome::ProvenInfeasible),
            Some(false)
        );
        assert_eq!(
            residual_outcome_disposition(ExactDimensionCaseOutcome::ValidatedFeasible),
            Some(true)
        );
        assert_eq!(
            residual_outcome_disposition(ExactDimensionCaseOutcome::Unknown),
            Some(true)
        );
        assert_eq!(
            residual_outcome_disposition(ExactDimensionCaseOutcome::InvalidWitness),
            None
        );
    }

    #[test]
    fn root_snapshot_selects_lowest_completed_unknown_not_predeclared_case() {
        let selected = lowest_unknown_pair_index([
            (0, ExactDimensionCaseOutcome::ProvenInfeasible),
            (3, ExactDimensionCaseOutcome::Unknown),
            (1, ExactDimensionCaseOutcome::Unknown),
            (2, ExactDimensionCaseOutcome::ValidatedFeasible),
        ]);

        assert_eq!(selected, Some(1));
    }
}
