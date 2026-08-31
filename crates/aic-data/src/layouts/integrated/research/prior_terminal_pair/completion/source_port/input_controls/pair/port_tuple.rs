use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::{
    ExactDimensionCaseOutcome, ExactDimensionSolverStack, ExactSearchStatistics,
    FacilityPlacementRequest, FacilityPortAssignment, IntegratedLayoutReport,
    PartitionCaseModelScale, RootDomainSnapshot, ValidatedFacilityCatalog, ValidatedItemCatalog,
    ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog, classify_outcome,
    diagnose_prior_input_pair_root_snapshot, exact, invalid_input, millis, model_scale,
    plan_facility_growth, prepare_target_input,
};
use crate::recipes::FacilityInstanceWiringReport;

pub const RESIDUAL_FACILITY_PORT_TUPLE_PORTFOLIO_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;
const EXPECTED_PARENT_FIXED_TERMINALS: usize = 11;
const EXPECTED_RESIDUAL_TERMINALS: usize = 4;
const EXPECTED_TOTAL_FIXED_TERMINALS: usize =
    EXPECTED_PARENT_FIXED_TERMINALS + EXPECTED_RESIDUAL_TERMINALS;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResidualFacilityPortDomain {
    pub terminal: String,
    pub ports: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ResidualFacilityPortFixationObservation {
    pub capture_status: String,
    pub expected_fixed_terminal_count: usize,
    pub observed_fixed_terminal_count: usize,
    pub assertion_applies: bool,
    pub assertion_satisfied: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResidualFacilityPortTupleCaseReport {
    pub case_index: usize,
    pub assignments: Vec<FacilityPortAssignment>,
    pub authoritative_outcome: ExactDimensionCaseOutcome,
    pub observation_outcome: ExactDimensionCaseOutcome,
    pub combined_outcome: ExactDimensionCaseOutcome,
    pub evidence_conflict: bool,
    pub fixation_observation: ResidualFacilityPortFixationObservation,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub model_scale: PartitionCaseModelScale,
    pub authoritative_layout: IntegratedLayoutReport,
    pub observation_layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResidualFacilityPortTuplePortfolioReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub solver_stack: ExactDimensionSolverStack,
    pub parent: super::PriorInputPairRootSnapshotReport,
    pub residual_domains: Vec<ResidualFacilityPortDomain>,
    pub tuple_count: usize,
    pub fixed_terminal_count_per_case: usize,
    pub worker_count: usize,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub preparation_ms: u64,
    pub authoritative_wave_wall_ms: u64,
    pub observation_wave_wall_ms: u64,
    pub total_wall_ms: u64,
    pub authoritative_validated_feasible_count: usize,
    pub authoritative_proven_infeasible_count: usize,
    pub authoritative_unknown_count: usize,
    pub authoritative_invalid_witness_count: usize,
    pub combined_validated_feasible_count: usize,
    pub combined_proven_infeasible_count: usize,
    pub combined_unknown_count: usize,
    pub combined_invalid_witness_count: usize,
    pub parent_witness_evidence: bool,
    pub parent_infeasibility_evidence: bool,
    pub global_evidence_conflict: bool,
    pub parent_witness_found: bool,
    pub parent_infeasibility_proven: bool,
    pub selected_next_unknown_case_index: Option<usize>,
    pub interpretation_blocked: bool,
    pub cases: Vec<ResidualFacilityPortTupleCaseReport>,
    pub diagnostic_only: bool,
}

#[derive(Debug, Clone)]
struct TupleCaseInput {
    case_index: usize,
    assignments: Vec<FacilityPortAssignment>,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_residual_facility_port_tuple_portfolio(
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
    parent_observation_search_budget: Duration,
    authoritative_case_search_budget: Duration,
    observation_case_search_budget: Duration,
) -> Result<ResidualFacilityPortTuplePortfolioReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if authoritative_case_search_budget.is_zero() || observation_case_search_budget.is_zero() {
        return Err(invalid_input(
            "/residual_facility_port_tuple_budget",
            "residual facility-port tuple budgets must be positive",
        ));
    }
    let parent = diagnose_prior_input_pair_root_snapshot(
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
        parent_observation_search_budget,
    )?;
    if parent.interpretation_blocked {
        return Err(invalid_input(
            "/parent",
            "residual facility-port tuple portfolio cannot use a blocked parent snapshot",
        ));
    }
    let preparation_started = Instant::now();
    let residual_domains = derive_residual_domains(&parent.root_snapshot)?;
    let tuple_inputs = enumerate_tuples(&residual_domains)?;
    if tuple_inputs.len() != 1 << EXPECTED_RESIDUAL_TERMINALS {
        return Err(invalid_input(
            "/residual_domains",
            format!(
                "expected sixteen residual facility-port tuples, found {}",
                tuple_inputs.len()
            ),
        ));
    }
    let parent_assignments = parent
        .inherited_assignments
        .iter()
        .chain(&parent.assignments)
        .cloned()
        .collect::<Vec<_>>();
    assert_distinct_assignments(
        &parent_assignments,
        EXPECTED_PARENT_FIXED_TERMINALS,
        "/parent/assignments",
    )?;

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
    let dimensions = exact::shared_layer::FixedUsedDimensions {
        width: parent.fixed_dimensions[0],
        height: parent.fixed_dimensions[1],
    };
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: parent.partitioned_facility.clone(),
        x: parent.fixed_coordinate[0],
        y: parent.fixed_coordinate[1],
        rotation: Some(parent.fixed_rotation),
    };
    let prior_reference = &parent.prior_reference;
    let preparation_ms = millis(preparation_started.elapsed());

    let authoritative_wave_started = Instant::now();
    let mut authoritative = Vec::with_capacity(tuple_inputs.len());
    for chunk in tuple_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for tuple in chunk {
                let input = input.clone();
                let coordinate = coordinate.clone();
                let exact_ports = exact_ports(&parent_assignments, &tuple.assignments);
                let tuple = tuple.clone();
                handles.push((
                    tuple,
                    scope.spawn(move || {
                        exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                            input,
                            logistics_components,
                            Some(authoritative_case_search_budget),
                            dimensions,
                            coordinate,
                            exact_ports,
                            prior_reference,
                            exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements,
                        )
                    }),
                ));
            }
            for (tuple, handle) in handles {
                authoritative.push((
                    tuple,
                    handle
                        .join()
                        .expect("authoritative facility-port tuple worker panicked"),
                ));
            }
        });
    }
    authoritative.sort_by_key(|(tuple, _)| tuple.case_index);
    let authoritative_wave_wall_ms = millis(authoritative_wave_started.elapsed());

    let observation_wave_started = Instant::now();
    let mut observations = Vec::with_capacity(tuple_inputs.len());
    for chunk in tuple_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for tuple in chunk {
                let input = input.clone();
                let coordinate = coordinate.clone();
                let exact_ports = exact_ports(&parent_assignments, &tuple.assignments);
                let tuple = tuple.clone();
                handles.push((
                    tuple,
                    scope.spawn(move || {
                        exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
                            input,
                            logistics_components,
                            Some(observation_case_search_budget),
                            dimensions,
                            coordinate,
                            exact_ports,
                            prior_reference,
                            exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements,
                        )
                    }),
                ));
            }
            for (tuple, handle) in handles {
                let (layout, snapshot) = handle
                    .join()
                    .expect("observed facility-port tuple worker panicked");
                observations.push((tuple, layout, snapshot));
            }
        });
    }
    observations.sort_by_key(|(tuple, _, _)| tuple.case_index);
    let observation_wave_wall_ms = millis(observation_wave_started.elapsed());

    if authoritative.len() != tuple_inputs.len() || observations.len() != tuple_inputs.len() {
        return Err(invalid_input(
            "/cases",
            "residual facility-port tuple portfolio did not complete every declared child",
        ));
    }
    let mut cases = Vec::with_capacity(tuple_inputs.len());
    for ((tuple, authoritative_layout), (observed_tuple, observation_layout, snapshot)) in
        authoritative.into_iter().zip(observations)
    {
        if tuple.case_index != observed_tuple.case_index
            || tuple.assignments != observed_tuple.assignments
        {
            return Err(invalid_input(
                "/cases",
                "authoritative and observation tuple provenance differ",
            ));
        }
        let snapshot = snapshot.ok_or_else(|| {
            invalid_input(
                "/cases/root_snapshot",
                format!("case {} did not return a root snapshot", tuple.case_index),
            )
        })?;
        let authoritative_outcome = classify_outcome(&authoritative_layout);
        let observation_outcome = classify_outcome(&observation_layout);
        let (combined_outcome, evidence_conflict) =
            combine_outcomes(authoritative_outcome, observation_outcome);
        let requested = parent_assignments
            .iter()
            .chain(&tuple.assignments)
            .cloned()
            .collect::<Vec<_>>();
        let fixation_observation = assess_fixation(&snapshot, &requested);
        let exact = authoritative_layout
            .exact
            .as_ref()
            .expect("executed facility-port tuple has exact model metrics");
        cases.push(ResidualFacilityPortTupleCaseReport {
            case_index: tuple.case_index,
            assignments: tuple.assignments,
            authoritative_outcome,
            observation_outcome,
            combined_outcome,
            evidence_conflict,
            fixation_observation,
            construction_ms: exact.construction_ms,
            search_ms: exact.search_ms,
            first_incumbent_ms: exact.first_incumbent_ms,
            search_statistics: exact.search_statistics,
            model_scale: model_scale(exact),
            authoritative_layout,
            observation_layout,
        });
    }

    let authoritative_validated_feasible_count = count(
        &cases,
        |case| case.authoritative_outcome,
        ExactDimensionCaseOutcome::ValidatedFeasible,
    );
    let authoritative_proven_infeasible_count = count(
        &cases,
        |case| case.authoritative_outcome,
        ExactDimensionCaseOutcome::ProvenInfeasible,
    );
    let authoritative_unknown_count = count(
        &cases,
        |case| case.authoritative_outcome,
        ExactDimensionCaseOutcome::Unknown,
    );
    let authoritative_invalid_witness_count = count(
        &cases,
        |case| case.authoritative_outcome,
        ExactDimensionCaseOutcome::InvalidWitness,
    );
    let combined_validated_feasible_count = count(
        &cases,
        |case| case.combined_outcome,
        ExactDimensionCaseOutcome::ValidatedFeasible,
    );
    let combined_proven_infeasible_count = count(
        &cases,
        |case| case.combined_outcome,
        ExactDimensionCaseOutcome::ProvenInfeasible,
    );
    let combined_unknown_count = count(
        &cases,
        |case| case.combined_outcome,
        ExactDimensionCaseOutcome::Unknown,
    );
    let combined_invalid_witness_count = count(
        &cases,
        |case| case.combined_outcome,
        ExactDimensionCaseOutcome::InvalidWitness,
    );

    let parent_witness_evidence = parent.observed_outcome
        == ExactDimensionCaseOutcome::ValidatedFeasible
        || combined_validated_feasible_count > 0;
    let parent_infeasibility_evidence = parent.observed_outcome
        == ExactDimensionCaseOutcome::ProvenInfeasible
        || combined_proven_infeasible_count == cases.len();
    let global_evidence_conflict = parent_witness_evidence && parent_infeasibility_evidence;
    let interpretation_blocked = parent.interpretation_blocked
        || parent.observed_outcome == ExactDimensionCaseOutcome::InvalidWitness
        || combined_invalid_witness_count > 0
        || cases.iter().any(|case| {
            case.evidence_conflict
                || (case.fixation_observation.assertion_applies
                    && !case.fixation_observation.assertion_satisfied)
        })
        || global_evidence_conflict;
    let parent_witness_found = !interpretation_blocked && parent_witness_evidence;
    let parent_infeasibility_proven = !interpretation_blocked && parent_infeasibility_evidence;
    let selected_next_unknown_case_index =
        (!interpretation_blocked && !parent_witness_found && !parent_infeasibility_proven)
            .then(|| {
                cases
                    .iter()
                    .filter(|case| case.combined_outcome == ExactDimensionCaseOutcome::Unknown)
                    .map(|case| case.case_index)
                    .min()
            })
            .flatten();

    Ok(ResidualFacilityPortTuplePortfolioReport {
        schema_version: RESIDUAL_FACILITY_PORT_TUPLE_PORTFOLIO_SCHEMA_VERSION,
        target_phase_index,
        solver_stack: parent.solver_stack,
        parent,
        residual_domains,
        tuple_count: cases.len(),
        fixed_terminal_count_per_case: EXPECTED_TOTAL_FIXED_TERMINALS,
        worker_count,
        authoritative_case_search_budget_ms: millis(authoritative_case_search_budget),
        observation_case_search_budget_ms: millis(observation_case_search_budget),
        preparation_ms,
        authoritative_wave_wall_ms,
        observation_wave_wall_ms,
        total_wall_ms: millis(total_started.elapsed()),
        authoritative_validated_feasible_count,
        authoritative_proven_infeasible_count,
        authoritative_unknown_count,
        authoritative_invalid_witness_count,
        combined_validated_feasible_count,
        combined_proven_infeasible_count,
        combined_unknown_count,
        combined_invalid_witness_count,
        parent_witness_evidence,
        parent_infeasibility_evidence,
        global_evidence_conflict,
        parent_witness_found,
        parent_infeasibility_proven,
        selected_next_unknown_case_index,
        interpretation_blocked,
        cases,
        diagnostic_only: true,
    })
}

fn derive_residual_domains(
    snapshot: &RootDomainSnapshot,
) -> Result<Vec<ResidualFacilityPortDomain>, IntegratedLayoutReport> {
    if !snapshot.fixed_facility_contract_satisfied || !snapshot.fixed_terminal_contract_satisfied {
        return Err(invalid_input(
            "/parent/root_snapshot",
            "parent fixed-state assertions must pass before residual port enumeration",
        ));
    }
    let mut domains = snapshot
        .terminals
        .iter()
        .filter(|terminal| {
            terminal.endpoint_kind == "facility"
                && terminal
                    .port_choice
                    .as_ref()
                    .is_some_and(|domain| domain.cardinality > 1)
        })
        .map(|terminal| ResidualFacilityPortDomain {
            terminal: terminal.terminal.clone(),
            ports: terminal.root_surviving_port_ids.clone(),
        })
        .collect::<Vec<_>>();
    domains.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    validate_residual_domains(&domains)?;
    Ok(domains)
}

fn validate_residual_domains(
    domains: &[ResidualFacilityPortDomain],
) -> Result<(), IntegratedLayoutReport> {
    if domains.len() != EXPECTED_RESIDUAL_TERMINALS {
        return Err(invalid_input(
            "/parent/root_snapshot/terminals",
            format!(
                "expected four residual facility terminals, found {}",
                domains.len()
            ),
        ));
    }
    let mut terminals = BTreeSet::new();
    for domain in domains {
        if !terminals.insert(domain.terminal.as_str()) || domain.ports.len() != 2 {
            return Err(invalid_input(
                "/parent/root_snapshot/terminals",
                "residual facility terminals must be distinct binary port domains",
            ));
        }
        if domain.ports[0] == domain.ports[1] {
            return Err(invalid_input(
                "/parent/root_snapshot/terminals",
                format!("terminal {} repeats a surviving port ID", domain.terminal),
            ));
        }
    }
    Ok(())
}

fn enumerate_tuples(
    domains: &[ResidualFacilityPortDomain],
) -> Result<Vec<TupleCaseInput>, IntegratedLayoutReport> {
    if domains.len() != EXPECTED_RESIDUAL_TERMINALS
        || domains.iter().any(|domain| domain.ports.len() != 2)
    {
        return Err(invalid_input(
            "/residual_domains",
            "facility-port tuple enumeration requires four binary domains",
        ));
    }
    let tuple_count = 1_usize << domains.len();
    Ok((0..tuple_count)
        .map(|case_index| TupleCaseInput {
            case_index,
            assignments: domains
                .iter()
                .enumerate()
                .map(|(domain_index, domain)| FacilityPortAssignment {
                    terminal: domain.terminal.clone(),
                    port: domain.ports[(case_index >> domain_index) & 1].clone(),
                })
                .collect(),
        })
        .collect())
}

fn exact_ports(
    parent: &[FacilityPortAssignment],
    tuple: &[FacilityPortAssignment],
) -> Vec<exact::shared_layer::FixedTerminalPortChoice> {
    let mut ports = parent
        .iter()
        .chain(tuple)
        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
            terminal: assignment.terminal.clone(),
            port: assignment.port.clone(),
        })
        .collect::<Vec<_>>();
    ports.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    ports
}

fn assert_distinct_assignments(
    assignments: &[FacilityPortAssignment],
    expected: usize,
    path: &str,
) -> Result<(), IntegratedLayoutReport> {
    let terminals = assignments
        .iter()
        .map(|assignment| assignment.terminal.as_str())
        .collect::<BTreeSet<_>>();
    if assignments.len() != expected || terminals.len() != expected {
        return Err(invalid_input(
            path,
            format!("expected {expected} distinct terminal assignments"),
        ));
    }
    Ok(())
}

fn assess_fixation(
    snapshot: &RootDomainSnapshot,
    requested: &[FacilityPortAssignment],
) -> ResidualFacilityPortFixationObservation {
    let assertion_applies = snapshot.capture_status != "root-infeasible";
    let requested_distinct = requested
        .iter()
        .map(|assignment| assignment.terminal.as_str())
        .collect::<BTreeSet<_>>();
    let assertion_satisfied = !assertion_applies
        || (requested.len() == EXPECTED_TOTAL_FIXED_TERMINALS
            && requested_distinct.len() == EXPECTED_TOTAL_FIXED_TERMINALS
            && snapshot.explicitly_fixed_facility_terminal_count == EXPECTED_TOTAL_FIXED_TERMINALS
            && snapshot.fixed_terminal_contract_satisfied
            && requested.iter().all(|assignment| {
                snapshot.terminals.iter().any(|terminal| {
                    terminal.terminal == assignment.terminal
                        && terminal.requested_fixed_port.as_deref() == Some(&assignment.port)
                        && terminal.root_surviving_port_ids == [assignment.port.clone()]
                        && terminal.singleton_geometry_key.is_some()
                        && terminal.expected_geometry_keys
                            == [terminal.singleton_geometry_key.expect("checked singleton")]
                })
            }));
    ResidualFacilityPortFixationObservation {
        capture_status: snapshot.capture_status.clone(),
        expected_fixed_terminal_count: EXPECTED_TOTAL_FIXED_TERMINALS,
        observed_fixed_terminal_count: snapshot.explicitly_fixed_facility_terminal_count,
        assertion_applies,
        assertion_satisfied,
    }
}

fn combine_outcomes(
    authoritative: ExactDimensionCaseOutcome,
    observation: ExactDimensionCaseOutcome,
) -> (ExactDimensionCaseOutcome, bool) {
    if authoritative == ExactDimensionCaseOutcome::InvalidWitness
        || observation == ExactDimensionCaseOutcome::InvalidWitness
    {
        return (ExactDimensionCaseOutcome::InvalidWitness, false);
    }
    let witness = authoritative == ExactDimensionCaseOutcome::ValidatedFeasible
        || observation == ExactDimensionCaseOutcome::ValidatedFeasible;
    let proof = authoritative == ExactDimensionCaseOutcome::ProvenInfeasible
        || observation == ExactDimensionCaseOutcome::ProvenInfeasible;
    if witness && proof {
        return (ExactDimensionCaseOutcome::InvalidWitness, true);
    }
    if witness {
        (ExactDimensionCaseOutcome::ValidatedFeasible, false)
    } else if proof {
        (ExactDimensionCaseOutcome::ProvenInfeasible, false)
    } else {
        (ExactDimensionCaseOutcome::Unknown, false)
    }
}

fn count(
    cases: &[ResidualFacilityPortTupleCaseReport],
    select: impl Fn(&ResidualFacilityPortTupleCaseReport) -> ExactDimensionCaseOutcome,
    outcome: ExactDimensionCaseOutcome,
) -> usize {
    cases.iter().filter(|case| select(case) == outcome).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn domains() -> Vec<ResidualFacilityPortDomain> {
        (0..4)
            .map(|index| ResidualFacilityPortDomain {
                terminal: format!("terminal-{index}"),
                ports: vec![format!("p{index}-0"), format!("p{index}-1")],
            })
            .collect()
    }

    #[test]
    fn enumerates_every_binary_tuple_without_excluding_equal_physical_ports() {
        let tuples = enumerate_tuples(&domains()).unwrap();

        assert_eq!(tuples.len(), 16);
        assert_eq!(tuples[0].assignments[0].port, "p0-0");
        assert_eq!(tuples[0].assignments[3].port, "p3-0");
        assert_eq!(tuples[15].assignments[0].port, "p0-1");
        assert_eq!(tuples[15].assignments[3].port, "p3-1");
        let unique = tuples
            .iter()
            .map(|tuple| {
                tuple
                    .assignments
                    .iter()
                    .map(|assignment| assignment.port.as_str())
                    .collect::<Vec<_>>()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), 16);
    }

    #[test]
    fn combined_outcome_preserves_proofs_and_blocks_conflicts() {
        assert_eq!(
            combine_outcomes(
                ExactDimensionCaseOutcome::Unknown,
                ExactDimensionCaseOutcome::ProvenInfeasible,
            ),
            (ExactDimensionCaseOutcome::ProvenInfeasible, false)
        );
        assert_eq!(
            combine_outcomes(
                ExactDimensionCaseOutcome::ValidatedFeasible,
                ExactDimensionCaseOutcome::ProvenInfeasible,
            ),
            (ExactDimensionCaseOutcome::InvalidWitness, true)
        );
    }

    #[test]
    fn rejects_incomplete_and_non_binary_residual_domains() {
        let mut incomplete = domains();
        incomplete.pop();
        assert!(validate_residual_domains(&incomplete).is_err());

        let mut non_binary = domains();
        non_binary[0].ports.push("p0-2".to_string());
        assert!(validate_residual_domains(&non_binary).is_err());
    }

    #[test]
    fn rejects_duplicate_terminals_and_port_ids() {
        let mut duplicate_terminal = domains();
        duplicate_terminal[1].terminal = duplicate_terminal[0].terminal.clone();
        assert!(validate_residual_domains(&duplicate_terminal).is_err());

        let mut duplicate_port = domains();
        duplicate_port[0].ports[1] = duplicate_port[0].ports[0].clone();
        assert!(validate_residual_domains(&duplicate_port).is_err());
    }
}
