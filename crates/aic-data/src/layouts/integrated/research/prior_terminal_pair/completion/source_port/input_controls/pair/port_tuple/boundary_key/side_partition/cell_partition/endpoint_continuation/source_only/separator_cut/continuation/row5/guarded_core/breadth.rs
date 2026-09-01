use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;

pub const GUARDED_CORE_BOUNDARY_CENSUS_SCHEMA_VERSION: u32 = 1;
const EXPECTED_TUPLE_COUNT: usize = 16;
const EXPECTED_RESIDUAL_DOMAIN_COUNT: usize = 4;
const EXPECTED_FIXED_TERMINAL_COUNT: usize = 15;
const EXPECTED_FIXED_FACILITY_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GuardedCoreBoundaryCensusStatus {
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GuardedCoreBoundaryCensusRootStatus {
    Captured,
    ProvenRootInfeasible,
    MissingOrInvalid,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuardedCoreBoundaryCensusCase {
    pub case_index: usize,
    pub assignments: Vec<FacilityPortAssignment>,
    pub requested_fixed_assignments: Vec<FacilityPortAssignment>,
    pub requested_fixed_terminal_count: usize,
    pub root_status: GuardedCoreBoundaryCensusRootStatus,
    pub outcome: ExactDimensionCaseOutcome,
    pub root_live_keys: Vec<i32>,
    pub root_live_key_count: usize,
    pub target_terminal_observed: bool,
    pub fixation_certificate_satisfied: bool,
    pub build_certificate_satisfied: bool,
    pub(in crate::layouts::integrated) build_certificate:
        exact::shared_layer::FixedSolveBuildCertificate,
    pub unrestricted_boundary_certificate_satisfied: bool,
    pub model_identity_satisfied: bool,
    pub actual_model: Option<crate::layouts::ExactModelMetrics>,
    pub actual_model_complexity: Option<crate::research::ModelComplexityMetrics>,
    pub actual_formulation: Option<String>,
    pub evidence_valid: bool,
    pub interpretation_blocked: bool,
    pub construction_ms: Option<u64>,
    pub search_ms: Option<u64>,
    pub branch_decisions: Option<u64>,
    pub backtracks: Option<u64>,
    pub conflicts: Option<u64>,
    pub learned_clauses: Option<u64>,
    pub solver_propagations: Option<u64>,
    pub termination: Option<String>,
    pub proof: Option<String>,
    pub root_snapshot: Option<exact::shared_layer::RootDomainSnapshot>,
    pub(in crate::layouts::integrated) boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    #[serde(skip_serializing)]
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuardedCoreBoundaryCensusReport {
    pub schema_version: u32,
    pub replay: GuardedCoreReplayReport,
    pub target_phase_index: usize,
    pub target_terminal: String,
    pub worker_count: usize,
    pub observation_budget_ms: u64,
    pub unrestricted_legal_key_count: usize,
    pub selected_parent_root_keys: Vec<i32>,
    pub selected_parent_root_key_count: usize,
    pub reference_model: crate::layouts::ExactModelMetrics,
    pub reference_model_complexity: crate::research::ModelComplexityMetrics,
    pub reference_formulation: String,
    pub parent_fixed_assignments: Vec<FacilityPortAssignment>,
    pub parent_fixed_terminal_count: usize,
    pub residual_fixed_terminal_count_per_case: usize,
    pub fixed_terminal_count_per_case: usize,
    pub complete_fixation_request_satisfied: bool,
    pub tuple_count: usize,
    pub cases: Vec<GuardedCoreBoundaryCensusCase>,
    pub captured_case_count: usize,
    pub proven_root_infeasible_case_count: usize,
    pub blocked_case_count: usize,
    pub exact_portfolio_pair_count: usize,
    pub distinct_root_live_key_count: usize,
    pub all_root_live_sets_equal: bool,
    pub all_sets_equal_selected_parent: bool,
    pub fixed_864_case_count_certified: bool,
    pub source_replay_satisfied: bool,
    pub tuple_enumeration_satisfied: bool,
    pub selected_parent_root_set_reproduced: bool,
    pub census_complete: bool,
    pub model_identity_satisfied: bool,
    pub build_certificates_satisfied: bool,
    pub unrestricted_boundary_satisfied: bool,
    pub evidence_valid: bool,
    pub status: GuardedCoreBoundaryCensusStatus,
    pub interpretation_blocked: bool,
    pub census_ms: u64,
    pub total_wall_ms: u64,
    pub diagnostic_only: bool,
}

#[derive(Debug, Clone)]
struct CensusInput {
    case_index: usize,
    assignments: Vec<FacilityPortAssignment>,
}

#[derive(Debug)]
struct RawCensusResult {
    input: CensusInput,
    requested_fixed_assignments: Vec<FacilityPortAssignment>,
    layout: IntegratedLayoutReport,
    snapshot: Option<exact::shared_layer::RootDomainSnapshot>,
    boundary_certificates: Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    build_certificate: exact::shared_layer::FixedSolveBuildCertificate,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_guarded_core_boundary_census(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    replay: GuardedCoreReplayReport,
    worker_count: usize,
    observation_budget: Duration,
) -> Result<GuardedCoreBoundaryCensusReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if worker_count == 0 || observation_budget.is_zero() {
        return Err(invalid_input(
            "/guarded_core/boundary_census",
            "guarded-core boundary census requires positive workers and observation budget",
        ));
    }
    let source_replay_satisfied = replay.status == GuardedCoreReplayStatus::Completed
        && !replay.interpretation_blocked
        && replay.source_proof_satisfied
        && replay.baseline_certificate_satisfied
        && replay.replay_clause_certificate_satisfied
        && replay.unrestricted_boundary_satisfied
        && replay.baseline_model_identity_satisfied
        && replay.baseline_matches_accepted_control
        && replay.replay_model_identity_satisfied
        && replay.exact_clause_delta_satisfied
        && replay.root_snapshot_contract_satisfied
        && replay.evidence_valid
        && target_phase_index == replay.shrinking.initial_gate.target_phase_index;
    if !source_replay_satisfied {
        return Err(invalid_input(
            "/guarded_core/boundary_census/source_replay",
            "guarded-core boundary census requires a completed valid replay for the same phase",
        ));
    }

    let row5_parent = &replay.shrinking.initial_gate.parent;
    let row4_parent = &row5_parent.parent.parent;
    let source_parent = &row4_parent.parent;
    let endpoint_parent = &source_parent.parent;
    let cell_parent = &endpoint_parent.parent;
    let boundary_parent = &cell_parent.parent.parent;
    let tuple_parent = &boundary_parent.parent;
    let target_terminal = cell_parent.selected_terminal.clone();
    let tuple_inputs = certify_tuple_enumeration(tuple_parent)?;
    let tuple_enumeration_satisfied = tuple_inputs.len() == EXPECTED_TUPLE_COUNT;

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
    let fixture = accepted_guarded_core_fixture();
    let identity = build_phase_identity(instance_wiring, &input, logistics_components);
    if identity.solver_signature != fixture.solver_signature
        || identity.workload_wiring_sha256 != fixture.workload_wiring_sha256
        || identity.phase_model_semantics_sha256 != fixture.phase_model_semantics_sha256
        || identity.external_terminal_ids != fixture.external_terminal_ids
        || identity.network_item_codes != fixture.network_item_codes
        || !fixture.external_terminal_ids.contains(&target_terminal)
    {
        return Err(invalid_input(
            "/guarded_core/boundary_census/phase_identity",
            "guarded-core boundary census does not match the accepted Phase 3 semantic fixture",
        ));
    }

    let parent_assignments = tuple_parent
        .parent
        .inherited_assignments
        .iter()
        .chain(&tuple_parent.parent.assignments)
        .cloned()
        .collect::<Vec<_>>();
    assert_distinct_assignments(
        &parent_assignments,
        EXPECTED_PARENT_FIXED_TERMINALS,
        "/guarded_core/boundary_census/parent_assignments",
    )?;
    for tuple_input in &tuple_inputs {
        let complete_assignments = parent_assignments
            .iter()
            .chain(&tuple_input.assignments)
            .cloned()
            .collect::<Vec<_>>();
        assert_distinct_assignments(
            &complete_assignments,
            EXPECTED_FIXED_TERMINAL_COUNT,
            "/guarded_core/boundary_census/complete_assignments",
        )?;
    }
    let complete_fixation_request_satisfied = true;
    let dimensions = exact::shared_layer::FixedUsedDimensions {
        width: tuple_parent.parent.fixed_dimensions[0],
        height: tuple_parent.parent.fixed_dimensions[1],
    };
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: tuple_parent.parent.partitioned_facility.clone(),
        x: tuple_parent.parent.fixed_coordinate[0],
        y: tuple_parent.parent.fixed_coordinate[1],
        rotation: Some(tuple_parent.parent.fixed_rotation),
    };
    let prior_reference = &tuple_parent.parent.prior_reference;
    let legal_boundary_keys = exact::reachable_boundary_keys(dimensions.width, dimensions.height);
    let selected_parent_root_keys = selected_parent_root_keys(
        &boundary_parent.sparse.root_snapshot,
        &target_terminal,
        &legal_boundary_keys,
    )?;
    let reference_exact = boundary_parent
        .sparse
        .observation_layout
        .exact
        .as_ref()
        .ok_or_else(|| {
            invalid_input(
                "/guarded_core/boundary_census/reference_model",
                "selected sparse predecessor observation omits exact model evidence",
            )
        })?;
    let reference_model = (
        reference_exact.model.clone(),
        reference_exact.model_complexity.clone(),
        reference_exact.formulation.to_string(),
    );

    let census_started = Instant::now();
    let mut raw = Vec::with_capacity(tuple_inputs.len());
    for chunk in tuple_inputs.chunks(worker_count) {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(chunk.len());
            for census_input in chunk {
                let model_input = input.clone();
                let coordinate = coordinate.clone();
                let requested_fixed_assignments = parent_assignments
                    .iter()
                    .chain(&census_input.assignments)
                    .cloned()
                    .collect::<Vec<_>>();
                let fixed_ports = fixed_ports(&requested_fixed_assignments);
                let census_input = census_input.clone();
                handles.push((
                    census_input.clone(),
                    scope.spawn(move || {
                        let (layout, snapshot, boundary_certificates, build_certificate) =
                            exact::shared_layer::solve_sparse_support_endpoints_boundary_key_audit_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
                                model_input,
                                logistics_components,
                                Some(observation_budget),
                                dimensions,
                                coordinate,
                                fixed_ports,
                                prior_reference,
                                exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements,
                                true,
                            );
                        RawCensusResult {
                            input: census_input,
                            requested_fixed_assignments,
                            layout,
                            snapshot,
                            boundary_certificates,
                            build_certificate,
                        }
                    }),
                ));
            }
            for (expected, handle) in handles {
                let result = handle
                    .join()
                    .expect("guarded-core boundary census worker panicked");
                assert_eq!(expected.case_index, result.input.case_index);
                assert_eq!(expected.assignments, result.input.assignments);
                raw.push(result);
            }
        });
    }
    raw.sort_by_key(|result| result.input.case_index);

    let mut cases = raw
        .into_iter()
        .map(|raw| {
            build_case(
                raw,
                &target_terminal,
                &fixture.external_terminal_ids,
                &legal_boundary_keys,
                &reference_model,
                dimensions,
                &coordinate,
                prior_reference,
            )
        })
        .collect::<Vec<_>>();
    cases.sort_by_key(|case| case.case_index);

    let captured_case_count = cases
        .iter()
        .filter(|case| case.root_status == GuardedCoreBoundaryCensusRootStatus::Captured)
        .count();
    let proven_root_infeasible_case_count = cases
        .iter()
        .filter(|case| {
            case.root_status == GuardedCoreBoundaryCensusRootStatus::ProvenRootInfeasible
        })
        .count();
    let blocked_case_count = cases
        .iter()
        .filter(|case| case.interpretation_blocked)
        .count();
    let exact_portfolio_pair_count = cases
        .iter()
        .map(|case| case.root_live_key_count)
        .sum::<usize>();
    let distinct_root_live_key_count = cases
        .iter()
        .flat_map(|case| case.root_live_keys.iter().copied())
        .collect::<BTreeSet<_>>()
        .len();
    let all_root_live_sets_equal = cases.first().is_some_and(|first| {
        cases
            .iter()
            .all(|case| case.root_live_keys == first.root_live_keys)
    });
    let all_sets_equal_selected_parent = cases
        .iter()
        .all(|case| case.root_live_keys == selected_parent_root_keys);
    let selected_parent_root_set_reproduced = cases
        .get(boundary_parent.selected_case_index)
        .is_some_and(|case| case.root_live_keys == selected_parent_root_keys);
    let model_identity_satisfied = cases.iter().all(|case| case.model_identity_satisfied);
    let build_certificates_satisfied = cases.iter().all(|case| case.build_certificate_satisfied);
    let unrestricted_boundary_satisfied = cases
        .iter()
        .all(|case| case.unrestricted_boundary_certificate_satisfied);
    let evidence_valid = cases.iter().all(|case| case.evidence_valid);
    let census_complete = cases.len() == EXPECTED_TUPLE_COUNT
        && cases
            .iter()
            .enumerate()
            .all(|(expected, case)| expected == case.case_index)
        && captured_case_count + proven_root_infeasible_case_count == EXPECTED_TUPLE_COUNT;
    let interpretation_blocked = !tuple_enumeration_satisfied
        || !selected_parent_root_set_reproduced
        || !census_complete
        || !model_identity_satisfied
        || !build_certificates_satisfied
        || !unrestricted_boundary_satisfied
        || !evidence_valid
        || blocked_case_count > 0;
    let status = if interpretation_blocked {
        GuardedCoreBoundaryCensusStatus::Blocked
    } else {
        GuardedCoreBoundaryCensusStatus::Completed
    };
    let fixed_864_case_count_certified = certify_fixed_864_case_count(
        interpretation_blocked,
        tuple_enumeration_satisfied,
        census_complete,
        all_sets_equal_selected_parent,
        selected_parent_root_keys.len(),
        exact_portfolio_pair_count,
    );
    let census_ms = millis(census_started.elapsed());
    let total_wall_ms = replay
        .total_wall_ms
        .saturating_add(millis(total_started.elapsed()));

    Ok(GuardedCoreBoundaryCensusReport {
        schema_version: GUARDED_CORE_BOUNDARY_CENSUS_SCHEMA_VERSION,
        target_phase_index,
        target_terminal,
        worker_count,
        observation_budget_ms: millis(observation_budget),
        unrestricted_legal_key_count: legal_boundary_keys.len(),
        selected_parent_root_key_count: selected_parent_root_keys.len(),
        selected_parent_root_keys,
        reference_model: reference_model.0,
        reference_model_complexity: reference_model.1,
        reference_formulation: reference_model.2,
        parent_fixed_assignments: parent_assignments.clone(),
        parent_fixed_terminal_count: parent_assignments.len(),
        residual_fixed_terminal_count_per_case: EXPECTED_RESIDUAL_DOMAIN_COUNT,
        fixed_terminal_count_per_case: EXPECTED_FIXED_TERMINAL_COUNT,
        complete_fixation_request_satisfied,
        tuple_count: cases.len(),
        captured_case_count,
        proven_root_infeasible_case_count,
        blocked_case_count,
        exact_portfolio_pair_count,
        distinct_root_live_key_count,
        all_root_live_sets_equal,
        all_sets_equal_selected_parent,
        fixed_864_case_count_certified,
        source_replay_satisfied,
        tuple_enumeration_satisfied,
        selected_parent_root_set_reproduced,
        census_complete,
        model_identity_satisfied,
        build_certificates_satisfied,
        unrestricted_boundary_satisfied,
        evidence_valid,
        status,
        interpretation_blocked,
        census_ms,
        total_wall_ms,
        cases,
        replay,
        diagnostic_only: true,
    })
}

fn certify_tuple_enumeration(
    parent: &ResidualFacilityPortTuplePortfolioReport,
) -> Result<Vec<CensusInput>, IntegratedLayoutReport> {
    if parent.interpretation_blocked
        || parent.tuple_count != EXPECTED_TUPLE_COUNT
        || parent.cases.len() != EXPECTED_TUPLE_COUNT
        || parent.residual_domains.len() != EXPECTED_RESIDUAL_DOMAIN_COUNT
        || parent
            .residual_domains
            .iter()
            .any(|domain| domain.ports.len() != 2 || domain.ports[0] == domain.ports[1])
    {
        return Err(invalid_input(
            "/guarded_core/boundary_census/tuples",
            "boundary census requires the complete unblocked four-binary-domain tuple portfolio",
        ));
    }
    let expected = (0..EXPECTED_TUPLE_COUNT)
        .map(|case_index| CensusInput {
            case_index,
            assignments: parent
                .residual_domains
                .iter()
                .enumerate()
                .map(|(domain_index, domain)| FacilityPortAssignment {
                    terminal: domain.terminal.clone(),
                    port: domain.ports[(case_index >> domain_index) & 1].clone(),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let by_index = parent
        .cases
        .iter()
        .map(|case| (case.case_index, &case.assignments))
        .collect::<BTreeMap<_, _>>();
    if by_index.len() != EXPECTED_TUPLE_COUNT
        || expected.iter().any(|candidate| {
            by_index.get(&candidate.case_index).copied() != Some(&candidate.assignments)
        })
    {
        return Err(invalid_input(
            "/guarded_core/boundary_census/tuples",
            "stored tuple cases do not equal the exact Cartesian enumeration",
        ));
    }
    Ok(expected)
}

fn fixed_ports(
    assignments: &[FacilityPortAssignment],
) -> Vec<exact::shared_layer::FixedTerminalPortChoice> {
    let mut ports = assignments
        .iter()
        .map(|assignment| exact::shared_layer::FixedTerminalPortChoice {
            terminal: assignment.terminal.clone(),
            port: assignment.port.clone(),
        })
        .collect::<Vec<_>>();
    ports.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    ports
}

fn selected_parent_root_keys(
    snapshot: &exact::shared_layer::RootDomainSnapshot,
    target_terminal: &str,
    legal_keys: &[i32],
) -> Result<Vec<i32>, IntegratedLayoutReport> {
    if snapshot.capture_status == "root-infeasible" {
        return Err(invalid_input(
            "/guarded_core/boundary_census/selected_parent",
            "selected predecessor tuple unexpectedly has no live root",
        ));
    }
    if !is_captured_root_status(&snapshot.capture_status) {
        return Err(invalid_input(
            "/guarded_core/boundary_census/selected_parent",
            "selected predecessor root snapshot has an unrecognized capture status",
        ));
    }
    let terminal = snapshot
        .terminals
        .iter()
        .find(|terminal| {
            terminal.terminal == target_terminal && terminal.endpoint_kind == "external"
        })
        .ok_or_else(|| {
            invalid_input(
                "/guarded_core/boundary_census/selected_parent",
                "selected predecessor root snapshot omits the target external terminal",
            )
        })?;
    certify_live_keys(&terminal.root_geometry_values, legal_keys)
        .map_err(|message| invalid_input("/guarded_core/boundary_census/selected_parent", message))
}

fn build_case(
    raw: RawCensusResult,
    target_terminal: &str,
    expected_terminal_ids: &[String],
    legal_keys: &[i32],
    reference_model: &(
        crate::layouts::ExactModelMetrics,
        crate::research::ModelComplexityMetrics,
        String,
    ),
    expected_dimensions: exact::shared_layer::FixedUsedDimensions,
    expected_coordinate: &exact::shared_layer::FixedFacilityCoordinate,
    prior_reference: &IntegratedLayoutReport,
) -> GuardedCoreBoundaryCensusCase {
    let outcome = classify_outcome(&raw.layout);
    let root_status = classify_root_status(raw.snapshot.as_ref(), outcome);
    let (target_terminal_observed, root_live_keys) = match (root_status, raw.snapshot.as_ref()) {
        (GuardedCoreBoundaryCensusRootStatus::Captured, Some(snapshot)) => snapshot
            .terminals
            .iter()
            .find(|terminal| {
                terminal.terminal == target_terminal && terminal.endpoint_kind == "external"
            })
            .map_or((false, Vec::new()), |terminal| {
                certify_live_keys(&terminal.root_geometry_values, legal_keys)
                    .map_or((false, Vec::new()), |keys| (true, keys))
            }),
        (GuardedCoreBoundaryCensusRootStatus::ProvenRootInfeasible, _) => (true, Vec::new()),
        (GuardedCoreBoundaryCensusRootStatus::MissingOrInvalid, _) => (false, Vec::new()),
        (GuardedCoreBoundaryCensusRootStatus::Captured, None) => (false, Vec::new()),
    };
    let fixation_certificate_satisfied = match (root_status, raw.snapshot.as_ref()) {
        (GuardedCoreBoundaryCensusRootStatus::Captured, Some(snapshot)) => {
            captured_fixation_satisfied(snapshot, &raw.requested_fixed_assignments)
        }
        (GuardedCoreBoundaryCensusRootStatus::ProvenRootInfeasible, Some(snapshot)) => {
            snapshot.capture_status == "root-infeasible"
                && raw.requested_fixed_assignments.len() == EXPECTED_FIXED_TERMINAL_COUNT
                && raw
                    .requested_fixed_assignments
                    .iter()
                    .map(|assignment| assignment.terminal.as_str())
                    .collect::<BTreeSet<_>>()
                    .len()
                    == EXPECTED_FIXED_TERMINAL_COUNT
        }
        _ => false,
    };
    let unrestricted_boundary_certificate_satisfied = boundary_certificates_are_complete(
        expected_terminal_ids,
        &raw.boundary_certificates,
        legal_keys,
    );
    let build_certificate_satisfied = build_certificate_satisfied(
        &raw.build_certificate,
        &raw.requested_fixed_assignments,
        expected_dimensions,
        expected_coordinate,
        prior_reference,
    );
    let model_identity_satisfied = raw.layout.exact.as_ref().is_some_and(|exact| {
        let (model, complexity, formulation) = reference_model;
        exact.model == *model
            && exact.model_complexity == *complexity
            && exact.formulation == *formulation
            && exact
                .model_complexity
                .constraints
                .as_ref()
                .is_some_and(|constraints| {
                    constraints
                        .by_family
                        .iter()
                        .all(|family| family.family != "guarded-core")
                })
    });
    let evidence_valid = outcome != ExactDimensionCaseOutcome::InvalidWitness
        && match root_status {
            GuardedCoreBoundaryCensusRootStatus::Captured => {
                raw.snapshot.as_ref().is_some_and(|snapshot| {
                    is_captured_root_status(&snapshot.capture_status) && target_terminal_observed
                })
            }
            GuardedCoreBoundaryCensusRootStatus::ProvenRootInfeasible => {
                outcome == ExactDimensionCaseOutcome::ProvenInfeasible
                    && raw.snapshot.as_ref().is_some_and(|snapshot| {
                        snapshot.capture_status == "root-infeasible"
                            && snapshot.first_decision.is_none()
                            && snapshot.terminals.is_empty()
                    })
            }
            GuardedCoreBoundaryCensusRootStatus::MissingOrInvalid => false,
        };
    let interpretation_blocked = !fixation_certificate_satisfied
        || !build_certificate_satisfied
        || !unrestricted_boundary_certificate_satisfied
        || !model_identity_satisfied
        || !evidence_valid;
    let exact = raw.layout.exact.as_ref();
    let statistics = exact.map(|exact| &exact.search_statistics);
    GuardedCoreBoundaryCensusCase {
        case_index: raw.input.case_index,
        assignments: raw.input.assignments,
        requested_fixed_assignments: raw.requested_fixed_assignments.clone(),
        requested_fixed_terminal_count: raw.requested_fixed_assignments.len(),
        root_status,
        outcome,
        root_live_key_count: root_live_keys.len(),
        root_live_keys,
        target_terminal_observed,
        fixation_certificate_satisfied,
        build_certificate_satisfied,
        build_certificate: raw.build_certificate,
        unrestricted_boundary_certificate_satisfied,
        model_identity_satisfied,
        actual_model: exact.map(|exact| exact.model.clone()),
        actual_model_complexity: exact.map(|exact| exact.model_complexity.clone()),
        actual_formulation: exact.map(|exact| exact.formulation.to_string()),
        evidence_valid,
        interpretation_blocked,
        construction_ms: exact.map(|exact| exact.construction_ms),
        search_ms: exact.map(|exact| exact.search_ms),
        branch_decisions: statistics.and_then(|statistics| statistics.branch_decisions),
        backtracks: statistics.and_then(|statistics| statistics.backtracks),
        conflicts: statistics.and_then(|statistics| statistics.conflicts),
        learned_clauses: statistics.and_then(|statistics| statistics.learned_clauses),
        solver_propagations: statistics.and_then(|statistics| statistics.solver_propagations),
        termination: exact.map(|exact| format!("{:?}", exact.termination)),
        proof: exact.map(|exact| format!("{:?}", exact.proof)),
        root_snapshot: raw.snapshot,
        boundary_certificates: raw.boundary_certificates,
        layout: raw.layout,
    }
}

fn classify_root_status(
    snapshot: Option<&exact::shared_layer::RootDomainSnapshot>,
    outcome: ExactDimensionCaseOutcome,
) -> GuardedCoreBoundaryCensusRootStatus {
    match snapshot {
        Some(snapshot)
            if snapshot.capture_status == "root-infeasible"
                && outcome == ExactDimensionCaseOutcome::ProvenInfeasible =>
        {
            GuardedCoreBoundaryCensusRootStatus::ProvenRootInfeasible
        }
        Some(snapshot) if is_captured_root_status(&snapshot.capture_status) => {
            GuardedCoreBoundaryCensusRootStatus::Captured
        }
        _ => GuardedCoreBoundaryCensusRootStatus::MissingOrInvalid,
    }
}

fn captured_fixation_satisfied(
    snapshot: &exact::shared_layer::RootDomainSnapshot,
    requested: &[FacilityPortAssignment],
) -> bool {
    is_captured_root_status(&snapshot.capture_status)
        && snapshot.explicitly_fixed_facility_terminal_count == EXPECTED_FIXED_TERMINAL_COUNT
        && snapshot.fixed_terminal_contract_satisfied
        && snapshot.fixed_facility_contract_satisfied
        && requested.len() == EXPECTED_FIXED_TERMINAL_COUNT
        && requested.iter().all(|assignment| {
            snapshot.terminals.iter().any(|terminal| {
                terminal.terminal == assignment.terminal
                    && terminal.requested_fixed_port.as_deref() == Some(&assignment.port)
                    && terminal.root_surviving_port_ids == [assignment.port.clone()]
                    && terminal.singleton_geometry_key.is_some()
            })
        })
}

fn build_certificate_satisfied(
    certificate: &exact::shared_layer::FixedSolveBuildCertificate,
    requested_fixed_assignments: &[FacilityPortAssignment],
    expected_dimensions: exact::shared_layer::FixedUsedDimensions,
    expected_coordinate: &exact::shared_layer::FixedFacilityCoordinate,
    prior_reference: &IntegratedLayoutReport,
) -> bool {
    let mut expected_ports = requested_fixed_assignments
        .iter()
        .map(|assignment| (assignment.terminal.clone(), assignment.port.clone()))
        .collect::<Vec<_>>();
    expected_ports.sort();
    let mut expected_prior_placements = prior_reference.placements.clone();
    expected_prior_placements.sort_by(|left, right| left.instance.cmp(&right.instance));
    certificate.model_build_completed
        && certificate.fixed_dimensions == [expected_dimensions.width, expected_dimensions.height]
        && certificate.fixed_coordinate_instance == expected_coordinate.instance
        && certificate.fixed_coordinate == [expected_coordinate.x, expected_coordinate.y]
        && certificate.fixed_rotation == expected_coordinate.rotation
        && certificate.fixed_terminal_ports == expected_ports
        && certificate.fixed_terminal_ports.len() == EXPECTED_FIXED_TERMINAL_COUNT
        && certificate.prior_placements == expected_prior_placements
        && certificate.prior_placements.len() + 1 == EXPECTED_FIXED_FACILITY_COUNT
        && certificate
            .prior_placements
            .iter()
            .all(|placement| placement.instance != expected_coordinate.instance)
        && certificate.reference_fixation == "prior-overlap-placements"
        && certificate.sparse_legal_boundary_domain
}

fn certify_fixed_864_case_count(
    interpretation_blocked: bool,
    tuple_enumeration_satisfied: bool,
    census_complete: bool,
    all_sets_equal_selected_parent: bool,
    selected_parent_root_key_count: usize,
    exact_portfolio_pair_count: usize,
) -> bool {
    !interpretation_blocked
        && tuple_enumeration_satisfied
        && census_complete
        && all_sets_equal_selected_parent
        && selected_parent_root_key_count == 54
        && exact_portfolio_pair_count == 864
}

fn is_captured_root_status(status: &str) -> bool {
    matches!(
        status,
        "captured-before-first-decision" | "root-solved-without-decision"
    )
}

fn certify_live_keys(values: &[i32], legal_keys: &[i32]) -> Result<Vec<i32>, &'static str> {
    let mut keys = values.to_vec();
    keys.sort_unstable();
    keys.dedup();
    if keys.is_empty() || keys.len() != values.len() {
        return Err("root-live boundary keys must be a non-empty distinct set");
    }
    let legal = legal_keys.iter().copied().collect::<BTreeSet<_>>();
    if keys.iter().any(|key| !legal.contains(key)) {
        return Err(
            "root-live boundary keys contain a value outside the unrestricted legal domain",
        );
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_infeasible_is_the_only_snapshot_free_domain_case() {
        let root_infeasible =
            exact::shared_layer::RootDomainSnapshot::root_infeasible_without_brancher_call();
        assert_eq!(
            classify_root_status(
                Some(&root_infeasible),
                ExactDimensionCaseOutcome::ProvenInfeasible,
            ),
            GuardedCoreBoundaryCensusRootStatus::ProvenRootInfeasible
        );
        assert_eq!(
            classify_root_status(Some(&root_infeasible), ExactDimensionCaseOutcome::Unknown,),
            GuardedCoreBoundaryCensusRootStatus::MissingOrInvalid
        );
        assert_eq!(
            classify_root_status(None, ExactDimensionCaseOutcome::Unknown),
            GuardedCoreBoundaryCensusRootStatus::MissingOrInvalid
        );
        let mut invalid = root_infeasible;
        invalid.capture_status = "unexpected-status".to_string();
        assert_eq!(
            classify_root_status(Some(&invalid), ExactDimensionCaseOutcome::Unknown),
            GuardedCoreBoundaryCensusRootStatus::MissingOrInvalid
        );
    }

    #[test]
    fn live_key_certificate_rejects_duplicates_empty_and_illegal_values() {
        assert_eq!(certify_live_keys(&[1, 3], &[1, 2, 3]), Ok(vec![1, 3]));
        assert!(certify_live_keys(&[], &[1, 2, 3]).is_err());
        assert!(certify_live_keys(&[1, 1], &[1, 2, 3]).is_err());
        assert!(certify_live_keys(&[1, 4], &[1, 2, 3]).is_err());
    }

    #[test]
    fn fixed_864_certificate_is_impossible_for_blocked_evidence() {
        assert!(!certify_fixed_864_case_count(
            true, true, true, true, 54, 864,
        ));
        assert!(certify_fixed_864_case_count(
            false, true, true, true, 54, 864,
        ));
    }

    #[test]
    fn build_certificate_requires_the_complete_posted_contract() {
        let assignments = (0..EXPECTED_FIXED_TERMINAL_COUNT)
            .map(|index| FacilityPortAssignment {
                terminal: format!("terminal-{index:02}"),
                port: format!("port-{index:02}"),
            })
            .collect::<Vec<_>>();
        let mut prior = IntegratedLayoutReport::invalid(
            crate::layouts::IntegratedLayoutDiagnostic::error("test", "/", None, "test fixture"),
        );
        prior.placements = (0..3)
            .map(|index| crate::layouts::FacilityPlacement {
                instance: format!("prior-{index}"),
                recipe: "recipe".to_string(),
                facility: "facility".to_string(),
                x: i64::from(index),
                y: 0,
                width: 1,
                height: 1,
                rotation: 0,
            })
            .collect();
        let dimensions = exact::shared_layer::FixedUsedDimensions {
            width: 16,
            height: 16,
        };
        let coordinate = exact::shared_layer::FixedFacilityCoordinate {
            instance: "new-facility".to_string(),
            x: 4,
            y: 5,
            rotation: Some(90),
        };
        let mut certificate = exact::shared_layer::FixedSolveBuildCertificate {
            fixed_dimensions: [16, 16],
            fixed_coordinate_instance: coordinate.instance.clone(),
            fixed_coordinate: [4, 5],
            fixed_rotation: Some(90),
            fixed_terminal_ports: assignments
                .iter()
                .map(|assignment| (assignment.terminal.clone(), assignment.port.clone()))
                .collect(),
            prior_placements: prior.placements.clone(),
            reference_fixation: "prior-overlap-placements".to_string(),
            sparse_legal_boundary_domain: true,
            model_build_completed: true,
        };
        assert!(build_certificate_satisfied(
            &certificate,
            &assignments,
            dimensions,
            &coordinate,
            &prior,
        ));
        certificate.model_build_completed = false;
        assert!(!build_certificate_satisfied(
            &certificate,
            &assignments,
            dimensions,
            &coordinate,
            &prior,
        ));
    }
}
