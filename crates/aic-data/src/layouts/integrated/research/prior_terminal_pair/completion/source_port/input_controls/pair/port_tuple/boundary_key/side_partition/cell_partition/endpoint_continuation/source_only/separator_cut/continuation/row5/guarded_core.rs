use std::collections::BTreeSet;
use std::fmt::Write;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::*;
use crate::layouts::{TransportNetworkEndpoint, WorldGridPosition};

pub const GUARDED_CORE_INITIAL_GATE_SCHEMA_VERSION: u32 = 1;
const EXPECTED_ATOM_COUNT: usize = 30;
const EXPECTED_FORMULATION: &str =
    "joint-shared-v4-unrestricted-sparse-boundary-guarded-core-assumptions";
const EXPECTED_OBSERVE_FORMULATION: &str =
    "joint-shared-v4-unrestricted-sparse-boundary-guarded-core-observe";
const EXPECTED_REPLAY_FORMULATION: &str =
    "joint-shared-v4-unrestricted-sparse-boundary-guarded-core-replay";
fn expected_solver_signature() -> String {
    format!("pumpkin-{}|{EXPECTED_FORMULATION}", pumpkin_solver::VERSION)
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct GuardedCoreAcceptedFixture {
    pub schema_version: u32,
    pub fixture_id: String,
    pub source_terminal: String,
    pub demand_terminal: String,
    pub selected_item_code: i32,
    pub solver_signature: String,
    pub workload_wiring_sha256: String,
    pub phase_model_semantics_sha256: String,
    pub external_terminal_ids: Vec<String>,
    pub network_item_codes: Vec<String>,
    pub atom_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GuardedCorePhaseIdentity {
    pub solver_signature: String,
    pub workload_wiring_sha256: String,
    pub phase_model_semantics_sha256: String,
    pub external_terminal_ids: Vec<String>,
    pub network_item_codes: Vec<String>,
}

pub fn accepted_guarded_core_fixture() -> GuardedCoreAcceptedFixture {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/benchmarks/heavy-xiranite-phase3-row5-case0.guarded-core.json"
    )))
    .expect("committed guarded-core fixture must parse")
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GuardedCoreInitialGateStatus {
    Go,
    Stop,
    Blocked,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GuardedCoreInitialGateReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: MaterialRow5SeparatorReport,
    pub search_ceiling: [i32; 2],
    pub search_budget_ms: u64,
    pub atom_ids: Vec<String>,
    pub fixture_id: String,
    pub accepted_semantic_fixture_satisfied: bool,
    pub expected_atom_count: usize,
    pub atom_count_satisfied: bool,
    pub atom_ids_unique: bool,
    pub placement_atom_count: usize,
    pub facility_port_atom_count: usize,
    pub route_atom_count: usize,
    pub full_core_layout: IntegratedLayoutReport,
    pub full_core_outcome: ExactDimensionCaseOutcome,
    pub observation_layout: IntegratedLayoutReport,
    pub observation_outcome: ExactDimensionCaseOutcome,
    pub root_snapshot: Option<exact::shared_layer::RootDomainSnapshot>,
    pub control_layout: IntegratedLayoutReport,
    pub control_outcome: ExactDimensionCaseOutcome,
    pub(in crate::layouts::integrated) guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) observation_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) observation_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) control_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) control_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub guarded_core_certificate_satisfied: bool,
    pub unrestricted_boundary_certificate_satisfied: bool,
    pub root_predicates_satisfied: bool,
    pub model_identity_satisfied: bool,
    pub guarded_core_delta_satisfied: bool,
    pub observation_evidence_compatible: bool,
    pub control_evidence_valid: bool,
    pub full_core_proven_infeasible: bool,
    pub gate_status: GuardedCoreInitialGateStatus,
    pub interpretation_blocked: bool,
    pub experiment_ms: u64,
    pub total_wall_ms: u64,
    pub diagnostic_only: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GuardedCoreSequentialShrinkStatus {
    Completed,
    StoppedEmptyCore,
    Blocked,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuardedCoreShrinkAttempt {
    pub attempt_index: usize,
    pub attempted_atom_index: usize,
    pub attempted_atom_id: String,
    pub prior_core_size: usize,
    pub candidate_core_size: usize,
    pub candidate_atom_ids: Vec<String>,
    pub outcome: ExactDimensionCaseOutcome,
    pub removed: bool,
    pub removal_authorized_by_proof: bool,
    pub certificate_satisfied: bool,
    pub unrestricted_boundary_satisfied: bool,
    pub exact_model_delta_satisfied: bool,
    pub interpretation_blocked: bool,
    pub wall_ms: u64,
    pub construction_ms: Option<u64>,
    pub search_ms: Option<u64>,
    pub first_incumbent_ms: Option<u64>,
    pub branch_decisions: Option<u64>,
    pub backtracks: Option<u64>,
    pub conflicts: Option<u64>,
    pub learned_clauses: Option<u64>,
    pub solver_propagations: Option<u64>,
    pub variables: Option<u64>,
    pub constraints: Option<u64>,
    pub incidences: Option<u64>,
    pub termination: Option<String>,
    pub proof: Option<String>,
    pub validation: Option<String>,
    pub model_complexity: Option<crate::research::ModelComplexityMetrics>,
    pub(in crate::layouts::integrated) guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    #[serde(skip_serializing)]
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuardedCoreSequentialShrinkReport {
    pub schema_version: u32,
    pub initial_gate: GuardedCoreInitialGateReport,
    pub search_budget_ms: u64,
    pub initial_core_size: usize,
    pub attempts: Vec<GuardedCoreShrinkAttempt>,
    pub final_atom_ids: Vec<String>,
    pub removed_atom_ids: Vec<String>,
    pub final_core_size: usize,
    pub final_authoritative_outcome: Option<ExactDimensionCaseOutcome>,
    pub final_observation_outcome: Option<ExactDimensionCaseOutcome>,
    pub final_certificate_satisfied: bool,
    pub final_unrestricted_boundary_satisfied: bool,
    pub final_exact_model_delta_satisfied: bool,
    pub final_root_predicates_satisfied: bool,
    pub final_model_identity_satisfied: bool,
    pub final_proven_infeasible: bool,
    pub(in crate::layouts::integrated) final_authoritative_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) final_observation_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) final_authoritative_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) final_observation_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub final_root_snapshot: Option<exact::shared_layer::RootDomainSnapshot>,
    pub status: GuardedCoreSequentialShrinkStatus,
    pub interpretation_blocked: bool,
    pub shrinking_ms: u64,
    pub total_wall_ms: u64,
    pub final_authoritative_layout: Option<IntegratedLayoutReport>,
    pub final_observation_layout: Option<IntegratedLayoutReport>,
    pub diagnostic_only: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GuardedCoreReplayStatus {
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum GuardedCoreReplayPerformanceClassification {
    ObservedReplayOutcomeImprovement,
    ObservedReplayOutcomeRegression,
    NoOutcomeWinner,
    InconclusiveRepeatedOutcomes,
    InconclusiveInvalidExperiment,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GuardedCoreRootAtomDelta {
    pub stable_id: String,
    pub baseline_domain: exact::shared_layer::RootDomainCardinality,
    pub replay_domain: exact::shared_layer::RootDomainCardinality,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuardedCoreReplayReport {
    pub schema_version: u32,
    pub shrinking: GuardedCoreSequentialShrinkReport,
    pub search_budget_ms: u64,
    pub replay_atom_ids: Vec<String>,
    pub baseline_authoritative_outcome: ExactDimensionCaseOutcome,
    pub reverse_baseline_authoritative_outcome: ExactDimensionCaseOutcome,
    pub baseline_observation_outcome: ExactDimensionCaseOutcome,
    pub replay_authoritative_outcome: ExactDimensionCaseOutcome,
    pub reverse_replay_authoritative_outcome: ExactDimensionCaseOutcome,
    pub replay_observation_outcome: ExactDimensionCaseOutcome,
    pub execution_order: Vec<String>,
    pub process_id: u32,
    pub operating_system: String,
    pub architecture: String,
    pub hint_sha256: String,
    pub hint_atom_matches: Vec<Option<bool>>,
    pub hint_matches_complete_replay_conjunction: Option<bool>,
    pub source_proof_satisfied: bool,
    pub baseline_certificate_satisfied: bool,
    pub replay_clause_certificate_satisfied: bool,
    pub unrestricted_boundary_satisfied: bool,
    pub baseline_model_identity_satisfied: bool,
    pub baseline_matches_accepted_control: bool,
    pub replay_model_identity_satisfied: bool,
    pub exact_clause_delta_satisfied: bool,
    pub root_snapshot_contract_satisfied: bool,
    pub root_core_domain_deltas: Vec<GuardedCoreRootAtomDelta>,
    pub root_changed_atom_count: usize,
    pub baseline_root_infeasible: bool,
    pub replay_root_infeasible: bool,
    pub replay_newly_root_eliminated: bool,
    pub evidence_valid: bool,
    pub repeated_outcomes_consistent: bool,
    pub performance_comparison_allowed: bool,
    pub performance_classification: GuardedCoreReplayPerformanceClassification,
    pub status: GuardedCoreReplayStatus,
    pub interpretation_blocked: bool,
    pub experiment_ms: u64,
    pub total_wall_ms: u64,
    pub baseline_authoritative_layout: IntegratedLayoutReport,
    pub reverse_baseline_authoritative_layout: IntegratedLayoutReport,
    pub baseline_observation_layout: IntegratedLayoutReport,
    pub replay_authoritative_layout: IntegratedLayoutReport,
    pub reverse_replay_authoritative_layout: IntegratedLayoutReport,
    pub replay_observation_layout: IntegratedLayoutReport,
    pub baseline_root_snapshot: Option<exact::shared_layer::RootDomainSnapshot>,
    pub replay_root_snapshot: Option<exact::shared_layer::RootDomainSnapshot>,
    pub(in crate::layouts::integrated) baseline_authoritative_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) baseline_observation_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) reverse_baseline_authoritative_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) replay_authoritative_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) replay_observation_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) reverse_replay_authoritative_guarded_core_certificates:
        Vec<exact::shared_layer::GuardedCoreBuildCertificate>,
    pub(in crate::layouts::integrated) baseline_authoritative_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) baseline_observation_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) reverse_baseline_authoritative_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) replay_authoritative_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) replay_observation_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub(in crate::layouts::integrated) reverse_replay_authoritative_boundary_certificates:
        Vec<exact::shared_layer::BoundaryKeyBuildCertificate>,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_guarded_core_initial_gate(
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
    tuple_authoritative_search_budget: Duration,
    tuple_observation_search_budget: Duration,
    ab_authoritative_search_budget: Duration,
    ab_observation_search_budget: Duration,
    side_authoritative_search_budget: Duration,
    side_observation_search_budget: Duration,
    cell_authoritative_search_budget: Duration,
    cell_observation_search_budget: Duration,
    selected_network_id: String,
    endpoint_authoritative_search_budget: Duration,
    endpoint_observation_search_budget: Duration,
    source_only_authoritative_search_budget: Duration,
    source_only_observation_search_budget: Duration,
    row4_separator_authoritative_search_budget: Duration,
    row4_separator_observation_search_budget: Duration,
    junction_authoritative_search_budget: Duration,
    junction_observation_search_budget: Duration,
    row5_authoritative_search_budget: Duration,
    row5_observation_search_budget: Duration,
    full_core_search_budget: Duration,
) -> Result<GuardedCoreInitialGateReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if full_core_search_budget.is_zero() {
        return Err(invalid_input(
            "/guarded_core/full_core_search_budget",
            "guarded-core full-model search budget must be positive",
        ));
    }
    if target_phase_index != 3
        || [fixed_width, fixed_height] != [16, 16]
        || [fixed_x, fixed_y] != [8, 5]
        || port_assignment_index != 5
        || fixed_rotation != 0
        || prior_facility_bit_index != 2
        || terminal_bit_indices != [2, 3]
        || representative_source_leaf_index != 0
        || selected_network_id != "network:pipe:item-liquid-xiranite-poly"
    {
        return Err(invalid_input(
            "/guarded_core/accepted_fixture/caller_contract",
            "guarded-core initial gate accepts only the committed Phase 3 row-5 case-zero caller contract",
        ));
    }
    let accepted_fixture = accepted_guarded_core_fixture();
    if accepted_fixture.schema_version != 1
        || accepted_fixture.fixture_id.is_empty()
        || accepted_fixture.solver_signature != expected_solver_signature()
        || accepted_fixture.workload_wiring_sha256.len() != 64
        || accepted_fixture.phase_model_semantics_sha256.len() != 64
        || accepted_fixture.external_terminal_ids.len() != 10
        || accepted_fixture.network_item_codes.is_empty()
        || accepted_fixture.atom_ids.len() != EXPECTED_ATOM_COUNT
        || accepted_fixture
            .atom_ids
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != EXPECTED_ATOM_COUNT
    {
        return Err(invalid_input(
            "/guarded_core/accepted_fixture",
            "guarded-core accepted fixture must be schema 1 with a non-empty ID and 30 distinct ordered atoms",
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
    let phase_identity = build_phase_identity(instance_wiring, &input, logistics_components);
    if phase_identity.solver_signature != accepted_fixture.solver_signature
        || phase_identity.workload_wiring_sha256 != accepted_fixture.workload_wiring_sha256
        || phase_identity.phase_model_semantics_sha256
            != accepted_fixture.phase_model_semantics_sha256
        || phase_identity.external_terminal_ids != accepted_fixture.external_terminal_ids
        || phase_identity.network_item_codes != accepted_fixture.network_item_codes
    {
        return Err(invalid_input(
            "/guarded_core/accepted_fixture/phase_identity",
            format!(
                "current Phase 3 semantics do not match the committed guarded-core fixture; observed identity: {}",
                serde_json::to_string(&phase_identity)
                    .expect("guarded-core phase identity is serializable")
            ),
        ));
    }
    let parent = diagnose_material_row5_separator(
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
        tuple_authoritative_search_budget,
        tuple_observation_search_budget,
        ab_authoritative_search_budget,
        ab_observation_search_budget,
        side_authoritative_search_budget,
        side_observation_search_budget,
        cell_authoritative_search_budget,
        cell_observation_search_budget,
        selected_network_id,
        endpoint_authoritative_search_budget,
        endpoint_observation_search_budget,
        source_only_authoritative_search_budget,
        source_only_observation_search_budget,
        row4_separator_authoritative_search_budget,
        row4_separator_observation_search_budget,
        junction_authoritative_search_budget,
        junction_observation_search_budget,
        row5_authoritative_search_budget,
        row5_observation_search_budget,
    )?;
    let case_zero = parent.cases.first().ok_or_else(|| {
        invalid_input(
            "/parent/cases/0",
            "guarded-core extraction requires accepted row-5 case zero",
        )
    })?;
    if parent.interpretation_blocked
        || parent.target_phase_index != 3
        || parent.fixed_dimensions != [16, 16]
        || parent.selected_network_id != "network:pipe:item-liquid-xiranite-poly"
        || parent.selected_item != "item-liquid-xiranite-poly"
        || parent.source_cell != 48
        || parent.source_continuation_cell != 81
        || parent.demand_cell != 113
        || case_zero.case_index != Some(0)
        || case_zero.selected_arc != Some([80, 96])
        || case_zero.solve.combined_outcome != ExactDimensionCaseOutcome::ProvenInfeasible
        || case_zero.interpretation_blocked
    {
        return Err(invalid_input(
            "/parent/accepted_fixture",
            "guarded-core extraction requires the accepted Phase 3 row-5 case-zero proof",
        ));
    }

    let reconstructed = reconstruct_guarded_core(&parent, &input)?;
    let atoms = reconstructed.atoms;
    let prior_reference = &reconstructed.prior_reference;

    let atom_ids = atoms
        .iter()
        .map(exact::shared_layer::GuardedCoreAtom::stable_id)
        .collect::<Vec<_>>();
    let atom_ids_unique = atom_ids.iter().collect::<BTreeSet<_>>().len() == atom_ids.len();
    let atom_count_satisfied = atoms.len() == EXPECTED_ATOM_COUNT;
    let placement_atom_count = atoms
        .iter()
        .filter(|atom| matches!(atom, exact::shared_layer::GuardedCoreAtom::Placement { .. }))
        .count();
    let facility_port_atom_count = atoms
        .iter()
        .filter(|atom| {
            matches!(
                atom,
                exact::shared_layer::GuardedCoreAtom::FacilityPort { .. }
            )
        })
        .count();
    let route_atom_count = atoms.len() - 2 - placement_atom_count - facility_port_atom_count - 1;

    let accepted_semantic_fixture_satisfied = atom_ids == accepted_fixture.atom_ids
        && parent.source_terminal == accepted_fixture.source_terminal
        && parent.demand_terminal == accepted_fixture.demand_terminal
        && parent.selected_item_code == accepted_fixture.selected_item_code;

    if !atom_count_satisfied
        || !atom_ids_unique
        || !accepted_semantic_fixture_satisfied
        || placement_atom_count != 4
        || facility_port_atom_count != 15
        || route_atom_count != 8
    {
        return Err(invalid_input(
            "/guarded_core/accepted_fixture/atoms",
            "reconstructed native premises do not match the committed 30-atom semantic fixture",
        ));
    }

    let experiment_started = Instant::now();
    let (full_core_layout, guarded_core_certificates, boundary_certificates, _, _) =
        exact::shared_layer::solve_sparse_support_guarded_core_feasibility(
            input.clone(),
            logistics_components,
            Some(full_core_search_budget),
            Some(prior_reference),
            atoms.clone(),
            exact::shared_layer::GuardedCorePosting::Assumptions,
        );
    let (
        observation_layout,
        observation_guarded_core_certificates,
        observation_boundary_certificates,
        _,
        _,
        root_snapshot,
    ) = exact::shared_layer::solve_sparse_support_guarded_core_root_snapshot(
        input.clone(),
        logistics_components,
        Some(full_core_search_budget),
        Some(prior_reference),
        atoms.clone(),
        exact::shared_layer::GuardedCorePosting::Assumptions,
    );
    let (control_layout, control_guarded_core_certificates, control_boundary_certificates, _, _, _) =
        exact::shared_layer::solve_sparse_support_guarded_core_root_snapshot(
            input,
            logistics_components,
            Some(full_core_search_budget),
            Some(prior_reference),
            Vec::new(),
            exact::shared_layer::GuardedCorePosting::Assumptions,
        );
    let full_core_outcome = classify_outcome(&full_core_layout);
    let observation_outcome = classify_outcome(&observation_layout);
    let control_outcome = classify_outcome(&control_layout);
    let guarded_core_certificate_satisfied =
        assumption_certificate_matches(&guarded_core_certificates, &atom_ids)
            && observation_guarded_core_certificates == guarded_core_certificates
            && assumption_certificate_matches(&control_guarded_core_certificates, &[]);
    let legal_boundary_keys = exact::reachable_boundary_keys(16, 16);
    let unrestricted_boundary_certificate_satisfied =
        complete_unrestricted_boundary_certificate(
            &full_core_layout,
            &boundary_certificates,
            &accepted_fixture.external_terminal_ids,
            &legal_boundary_keys,
        ) && complete_unrestricted_boundary_certificate(
            &observation_layout,
            &observation_boundary_certificates,
            &accepted_fixture.external_terminal_ids,
            &legal_boundary_keys,
        ) && complete_unrestricted_boundary_certificate(
            &control_layout,
            &control_boundary_certificates,
            &accepted_fixture.external_terminal_ids,
            &legal_boundary_keys,
        );
    let root_predicates_satisfied = root_snapshot.as_ref().is_some_and(|snapshot| {
        snapshot.guarded_core_atoms.len() == EXPECTED_ATOM_COUNT
            && snapshot
                .guarded_core_atoms
                .iter()
                .map(|atom| atom.stable_id.as_str())
                .eq(atom_ids.iter().map(String::as_str))
            && snapshot
                .guarded_core_atoms
                .iter()
                .all(|atom| atom.predicate_forced_true)
    });
    let model_identity_satisfied = same_exact_model(&full_core_layout, &observation_layout);
    let guarded_core_delta_satisfied =
        guarded_core_delta_is_exact(&control_layout, &full_core_layout, EXPECTED_ATOM_COUNT);
    let observation_evidence_compatible = !matches!(
        observation_outcome,
        ExactDimensionCaseOutcome::ValidatedFeasible | ExactDimensionCaseOutcome::InvalidWitness
    );
    let control_evidence_valid = control_outcome != ExactDimensionCaseOutcome::InvalidWitness;
    let full_core_proven_infeasible =
        full_core_outcome == ExactDimensionCaseOutcome::ProvenInfeasible;
    let interpretation_blocked = initial_gate_is_blocked(
        &[
            atom_count_satisfied,
            atom_ids_unique,
            accepted_semantic_fixture_satisfied,
            placement_atom_count == 4,
            facility_port_atom_count == 15,
            route_atom_count == 8,
            guarded_core_certificate_satisfied,
            unrestricted_boundary_certificate_satisfied,
            root_predicates_satisfied,
            model_identity_satisfied,
            guarded_core_delta_satisfied,
            observation_evidence_compatible,
            control_evidence_valid,
        ],
        full_core_proven_infeasible,
    );
    let gate_status = if interpretation_blocked {
        GuardedCoreInitialGateStatus::Blocked
    } else if control_outcome == ExactDimensionCaseOutcome::ProvenInfeasible {
        GuardedCoreInitialGateStatus::Stop
    } else {
        GuardedCoreInitialGateStatus::Go
    };
    Ok(GuardedCoreInitialGateReport {
        schema_version: GUARDED_CORE_INITIAL_GATE_SCHEMA_VERSION,
        target_phase_index,
        parent,
        search_ceiling: [fixed_width, fixed_height],
        search_budget_ms: millis(full_core_search_budget),
        atom_ids,
        fixture_id: accepted_fixture.fixture_id.clone(),
        accepted_semantic_fixture_satisfied,
        expected_atom_count: EXPECTED_ATOM_COUNT,
        atom_count_satisfied,
        atom_ids_unique,
        placement_atom_count,
        facility_port_atom_count,
        route_atom_count,
        full_core_layout,
        full_core_outcome,
        observation_layout,
        observation_outcome,
        root_snapshot,
        control_layout,
        control_outcome,
        guarded_core_certificates,
        boundary_certificates,
        observation_guarded_core_certificates,
        observation_boundary_certificates,
        control_guarded_core_certificates,
        control_boundary_certificates,
        guarded_core_certificate_satisfied,
        unrestricted_boundary_certificate_satisfied,
        root_predicates_satisfied,
        model_identity_satisfied,
        guarded_core_delta_satisfied,
        observation_evidence_compatible,
        control_evidence_valid,
        full_core_proven_infeasible,
        gate_status,
        interpretation_blocked,
        experiment_ms: millis(experiment_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        diagnostic_only: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_guarded_core_sequential_shrinking(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    initial_gate: GuardedCoreInitialGateReport,
    search_budget: Duration,
) -> Result<GuardedCoreSequentialShrinkReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if search_budget.is_zero() {
        return Err(invalid_input(
            "/guarded_core/shrinking/search_budget",
            "guarded-core shrinking search budget must be positive",
        ));
    }
    if initial_gate.gate_status != GuardedCoreInitialGateStatus::Go
        || initial_gate.interpretation_blocked
        || !initial_gate.full_core_proven_infeasible
        || target_phase_index != initial_gate.target_phase_index
    {
        return Err(invalid_input(
            "/guarded_core/shrinking/initial_gate",
            "sequential shrinking requires a non-blocked Go initial gate for the same phase",
        ));
    }

    let fixture = accepted_guarded_core_fixture();
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
    let identity = build_phase_identity(instance_wiring, &input, logistics_components);
    if identity.solver_signature != fixture.solver_signature
        || identity.workload_wiring_sha256 != fixture.workload_wiring_sha256
        || identity.phase_model_semantics_sha256 != fixture.phase_model_semantics_sha256
        || identity.external_terminal_ids != fixture.external_terminal_ids
        || identity.network_item_codes != fixture.network_item_codes
    {
        return Err(invalid_input(
            "/guarded_core/shrinking/phase_identity",
            "sequential shrinking Phase 3 semantics do not match the committed fixture",
        ));
    }

    let reconstructed = reconstruct_guarded_core(&initial_gate.parent, &input)?;
    let original_atoms = reconstructed.atoms;
    let original_atom_ids = original_atoms
        .iter()
        .map(exact::shared_layer::GuardedCoreAtom::stable_id)
        .collect::<Vec<_>>();
    if original_atom_ids != initial_gate.atom_ids || original_atom_ids != fixture.atom_ids {
        return Err(invalid_input(
            "/guarded_core/shrinking/atoms",
            "sequential shrinking reconstructed atoms do not match the accepted initial gate",
        ));
    }

    let shrinking_started = Instant::now();
    let legal_boundary_keys = exact::reachable_boundary_keys(16, 16);
    let initial_atom_certificates = initial_gate
        .guarded_core_certificates
        .first()
        .filter(|_| initial_gate.guarded_core_certificates.len() == 1)
        .map(|certificate| certificate.atoms.clone())
        .ok_or_else(|| {
            invalid_input(
                "/guarded_core/shrinking/initial_certificates",
                "sequential shrinking requires exactly one accepted initial atom certificate",
            )
        })?;
    let mut current_atoms = original_atoms.clone();
    let mut attempts = Vec::with_capacity(original_atoms.len());
    let mut removed_atom_ids = Vec::new();
    let mut blocked = false;

    for (attempt_index, attempted_atom) in original_atoms.iter().enumerate() {
        let attempted_atom_id = attempted_atom.stable_id();
        let prior_core_size = current_atoms.len();
        let candidate_atoms = current_atoms
            .iter()
            .filter(|atom| atom.stable_id() != attempted_atom_id)
            .cloned()
            .collect::<Vec<_>>();
        let candidate_atom_ids = candidate_atoms
            .iter()
            .map(exact::shared_layer::GuardedCoreAtom::stable_id)
            .collect::<Vec<_>>();
        if candidate_atoms.len() + 1 != prior_core_size {
            return Err(invalid_input(
                "/guarded_core/shrinking/order",
                "each accepted atom must occur exactly once in the current core",
            ));
        }

        let expected_certificates = expected_atom_certificate_subset(
            &initial_atom_certificates,
            &candidate_atom_ids,
        )
        .ok_or_else(|| {
            invalid_input(
                "/guarded_core/shrinking/candidate_certificates",
                "candidate atom IDs must map bijectively to the accepted initial certificates",
            )
        })?;
        let attempt_started = Instant::now();
        let (layout, certificates, boundary_certificates, _, _) =
            exact::shared_layer::solve_sparse_support_guarded_core_feasibility(
                input.clone(),
                logistics_components,
                Some(search_budget),
                Some(&reconstructed.prior_reference),
                candidate_atoms.clone(),
                exact::shared_layer::GuardedCorePosting::Assumptions,
            );
        let outcome = classify_outcome(&layout);
        let certificate_satisfied = assumption_certificate_exactly_matches(
            &certificates,
            &candidate_atom_ids,
            &expected_certificates,
        );
        let unrestricted_boundary_satisfied = complete_unrestricted_boundary_certificate(
            &layout,
            &boundary_certificates,
            &fixture.external_terminal_ids,
            &legal_boundary_keys,
        );
        let exact_model_delta_satisfied = guarded_core_delta_is_exact(
            &initial_gate.control_layout,
            &layout,
            candidate_atoms.len(),
        );
        let evidence_conflict = candidate_atoms.is_empty()
            && initial_gate.control_outcome == ExactDimensionCaseOutcome::ValidatedFeasible
            && outcome == ExactDimensionCaseOutcome::ProvenInfeasible;
        let (removed, interpretation_blocked) = classify_shrink_attempt(
            outcome,
            certificate_satisfied,
            unrestricted_boundary_satisfied,
            exact_model_delta_satisfied,
            evidence_conflict,
        );
        let metrics = compact_solve_metrics(&layout);
        attempts.push(GuardedCoreShrinkAttempt {
            attempt_index,
            attempted_atom_index: attempt_index,
            attempted_atom_id: attempted_atom_id.clone(),
            prior_core_size,
            candidate_core_size: candidate_atoms.len(),
            candidate_atom_ids,
            outcome,
            removed,
            removal_authorized_by_proof: removed,
            certificate_satisfied,
            unrestricted_boundary_satisfied,
            exact_model_delta_satisfied,
            interpretation_blocked,
            wall_ms: millis(attempt_started.elapsed()),
            construction_ms: metrics.construction_ms,
            search_ms: metrics.search_ms,
            first_incumbent_ms: metrics.first_incumbent_ms,
            branch_decisions: metrics.branch_decisions,
            backtracks: metrics.backtracks,
            conflicts: metrics.conflicts,
            learned_clauses: metrics.learned_clauses,
            solver_propagations: metrics.solver_propagations,
            variables: metrics.variables,
            constraints: metrics.constraints,
            incidences: metrics.incidences,
            termination: metrics.termination,
            proof: metrics.proof,
            validation: metrics.validation,
            model_complexity: metrics.model_complexity,
            guarded_core_certificates: certificates,
            boundary_certificates,
            layout,
        });
        if interpretation_blocked {
            blocked = true;
            break;
        }
        if removed {
            current_atoms = candidate_atoms;
            removed_atom_ids.push(attempted_atom_id);
        }
    }

    let final_atom_ids = current_atoms
        .iter()
        .map(exact::shared_layer::GuardedCoreAtom::stable_id)
        .collect::<Vec<_>>();
    let mut final_authoritative_layout = None;
    let mut final_observation_layout = None;
    let mut final_authoritative_outcome = None;
    let mut final_observation_outcome = None;
    let mut final_certificate_satisfied = false;
    let mut final_unrestricted_boundary_satisfied = false;
    let mut final_exact_model_delta_satisfied = false;
    let mut final_root_predicates_satisfied = false;
    let mut final_model_identity_satisfied = false;
    let mut final_proven_infeasible = false;
    let mut final_authoritative_guarded_core_certificates = Vec::new();
    let mut final_observation_guarded_core_certificates = Vec::new();
    let mut final_authoritative_boundary_certificates = Vec::new();
    let mut final_observation_boundary_certificates = Vec::new();
    let mut final_root_snapshot = None;

    if !blocked {
        let (authoritative, certificates, boundary_certificates, _, _) =
            exact::shared_layer::solve_sparse_support_guarded_core_feasibility(
                input.clone(),
                logistics_components,
                Some(search_budget),
                Some(&reconstructed.prior_reference),
                current_atoms.clone(),
                exact::shared_layer::GuardedCorePosting::Assumptions,
            );
        let (observation, observed_certificates, observed_boundary_certificates, _, _, snapshot) =
            exact::shared_layer::solve_sparse_support_guarded_core_root_snapshot(
                input,
                logistics_components,
                Some(search_budget),
                Some(&reconstructed.prior_reference),
                current_atoms,
                exact::shared_layer::GuardedCorePosting::Assumptions,
            );
        let authoritative_outcome = classify_outcome(&authoritative);
        let observation_outcome = classify_outcome(&observation);
        let expected_certificates =
            expected_atom_certificate_subset(&initial_atom_certificates, &final_atom_ids)
                .ok_or_else(|| {
                    invalid_input(
                        "/guarded_core/shrinking/final_certificates",
                        "final atom IDs must map bijectively to the accepted initial certificates",
                    )
                })?;
        final_certificate_satisfied = assumption_certificate_exactly_matches(
            &certificates,
            &final_atom_ids,
            &expected_certificates,
        ) && certificates == observed_certificates;
        final_unrestricted_boundary_satisfied = complete_unrestricted_boundary_certificate(
            &authoritative,
            &boundary_certificates,
            &fixture.external_terminal_ids,
            &legal_boundary_keys,
        ) && complete_unrestricted_boundary_certificate(
            &observation,
            &observed_boundary_certificates,
            &fixture.external_terminal_ids,
            &legal_boundary_keys,
        );
        final_exact_model_delta_satisfied = guarded_core_delta_is_exact(
            &initial_gate.control_layout,
            &authoritative,
            final_atom_ids.len(),
        );
        final_root_predicates_satisfied = snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.guarded_core_atoms.len() == final_atom_ids.len()
                && snapshot
                    .guarded_core_atoms
                    .iter()
                    .map(|atom| atom.stable_id.as_str())
                    .eq(final_atom_ids.iter().map(String::as_str))
                && snapshot
                    .guarded_core_atoms
                    .iter()
                    .all(|atom| atom.predicate_forced_true)
        });
        final_model_identity_satisfied = same_exact_model(&authoritative, &observation);
        final_proven_infeasible = authoritative_outcome
            == ExactDimensionCaseOutcome::ProvenInfeasible
            && !matches!(
                observation_outcome,
                ExactDimensionCaseOutcome::ValidatedFeasible
                    | ExactDimensionCaseOutcome::InvalidWitness
            );
        blocked = !final_certificate_satisfied
            || !final_unrestricted_boundary_satisfied
            || !final_exact_model_delta_satisfied
            || !final_root_predicates_satisfied
            || !final_model_identity_satisfied
            || !final_proven_infeasible;
        final_authoritative_outcome = Some(authoritative_outcome);
        final_observation_outcome = Some(observation_outcome);
        final_authoritative_guarded_core_certificates = certificates;
        final_observation_guarded_core_certificates = observed_certificates;
        final_authoritative_boundary_certificates = boundary_certificates;
        final_observation_boundary_certificates = observed_boundary_certificates;
        final_root_snapshot = snapshot;
        final_authoritative_layout = Some(authoritative);
        final_observation_layout = Some(observation);
    }

    let status = if blocked {
        GuardedCoreSequentialShrinkStatus::Blocked
    } else if final_atom_ids.is_empty() {
        GuardedCoreSequentialShrinkStatus::StoppedEmptyCore
    } else {
        GuardedCoreSequentialShrinkStatus::Completed
    };
    let total_wall_ms = initial_gate
        .total_wall_ms
        .saturating_add(millis(total_started.elapsed()));
    Ok(GuardedCoreSequentialShrinkReport {
        schema_version: 1,
        search_budget_ms: millis(search_budget),
        initial_core_size: original_atom_ids.len(),
        attempts,
        final_core_size: final_atom_ids.len(),
        final_atom_ids,
        removed_atom_ids,
        final_authoritative_outcome,
        final_observation_outcome,
        final_certificate_satisfied,
        final_unrestricted_boundary_satisfied,
        final_exact_model_delta_satisfied,
        final_root_predicates_satisfied,
        final_model_identity_satisfied,
        final_proven_infeasible,
        final_authoritative_guarded_core_certificates,
        final_observation_guarded_core_certificates,
        final_authoritative_boundary_certificates,
        final_observation_boundary_certificates,
        final_root_snapshot,
        status,
        interpretation_blocked: blocked,
        shrinking_ms: millis(shrinking_started.elapsed()),
        total_wall_ms,
        initial_gate,
        final_authoritative_layout,
        final_observation_layout,
        diagnostic_only: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_guarded_core_replay(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    shrinking: GuardedCoreSequentialShrinkReport,
    search_budget: Duration,
) -> Result<GuardedCoreReplayReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if search_budget.is_zero() {
        return Err(invalid_input(
            "/guarded_core/replay/search_budget",
            "guarded-core replay search budget must be positive",
        ));
    }
    let source_proof_satisfied = shrinking.status == GuardedCoreSequentialShrinkStatus::Completed
        && !shrinking.interpretation_blocked
        && shrinking.final_core_size == shrinking.final_atom_ids.len()
        && shrinking.final_core_size == 9
        && shrinking.attempts.len() == EXPECTED_ATOM_COUNT
        && shrinking
            .attempts
            .iter()
            .all(|attempt| !attempt.interpretation_blocked)
        && shrinking.final_authoritative_outcome
            == Some(ExactDimensionCaseOutcome::ProvenInfeasible)
        && shrinking
            .final_authoritative_layout
            .as_ref()
            .is_some_and(|layout| {
                classify_outcome(layout) == ExactDimensionCaseOutcome::ProvenInfeasible
            })
        && shrinking
            .final_observation_layout
            .as_ref()
            .is_some_and(|layout| {
                classify_outcome(layout) != ExactDimensionCaseOutcome::InvalidWitness
            })
        && shrinking.final_proven_infeasible
        && shrinking.final_certificate_satisfied
        && shrinking.final_unrestricted_boundary_satisfied
        && shrinking.final_exact_model_delta_satisfied
        && shrinking.final_root_predicates_satisfied
        && shrinking.final_model_identity_satisfied
        && shrinking
            .final_atom_ids
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            == 9
        && shrinking
            .removed_atom_ids
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            == EXPECTED_ATOM_COUNT - 9
        && target_phase_index == shrinking.initial_gate.target_phase_index;
    if !source_proof_satisfied {
        return Err(invalid_input(
            "/guarded_core/replay/source_proof",
            "guarded-core replay requires a non-empty independently proven shrinking result for the same phase",
        ));
    }

    let fixture = accepted_guarded_core_fixture();
    let retained_ids = shrinking
        .final_atom_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let removed_ids = shrinking
        .removed_atom_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let canonical_ids = fixture.atom_ids.iter().cloned().collect::<BTreeSet<_>>();
    if !retained_ids.is_disjoint(&removed_ids)
        || retained_ids
            .union(&removed_ids)
            .cloned()
            .collect::<BTreeSet<_>>()
            != canonical_ids
    {
        return Err(invalid_input(
            "/guarded_core/replay/atom_partition",
            "retained and removed atoms must form an exact disjoint partition of the canonical fixture",
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
    let identity = build_phase_identity(instance_wiring, &input, logistics_components);
    if identity.solver_signature != fixture.solver_signature
        || identity.workload_wiring_sha256 != fixture.workload_wiring_sha256
        || identity.phase_model_semantics_sha256 != fixture.phase_model_semantics_sha256
        || identity.external_terminal_ids != fixture.external_terminal_ids
        || identity.network_item_codes != fixture.network_item_codes
    {
        return Err(invalid_input(
            "/guarded_core/replay/phase_identity",
            "guarded-core replay Phase 3 semantics do not match the committed fixture",
        ));
    }

    let reconstructed = reconstruct_guarded_core(&shrinking.initial_gate.parent, &input)?;
    let original_by_id = reconstructed
        .atoms
        .into_iter()
        .map(|atom| (atom.stable_id(), atom))
        .collect::<std::collections::BTreeMap<_, _>>();
    if original_by_id.len() != EXPECTED_ATOM_COUNT {
        return Err(invalid_input(
            "/guarded_core/replay/atoms",
            "reconstructed guarded-core atoms must remain distinct and complete",
        ));
    }
    let replay_atoms = shrinking
        .final_atom_ids
        .iter()
        .map(|atom_id| original_by_id.get(atom_id).cloned())
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            invalid_input(
                "/guarded_core/replay/atoms",
                "every retained atom must resolve in the accepted semantic fixture",
            )
        })?;
    let replay_atom_ids = replay_atoms
        .iter()
        .map(exact::shared_layer::GuardedCoreAtom::stable_id)
        .collect::<Vec<_>>();
    if replay_atom_ids != shrinking.final_atom_ids {
        return Err(invalid_input(
            "/guarded_core/replay/atoms",
            "replay atom order must match the independently proven retained conjunction",
        ));
    }
    let initial_atom_certificates = shrinking
        .initial_gate
        .guarded_core_certificates
        .first()
        .filter(|_| shrinking.initial_gate.guarded_core_certificates.len() == 1)
        .map(|certificate| certificate.atoms.clone())
        .ok_or_else(|| {
            invalid_input(
                "/guarded_core/replay/initial_certificates",
                "guarded-core replay requires exactly one accepted initial atom certificate",
            )
        })?;
    let expected_replay_certificates =
        expected_atom_certificate_subset(&initial_atom_certificates, &replay_atom_ids).ok_or_else(
            || {
                invalid_input(
                    "/guarded_core/replay/certificates",
                    "replay atoms must map bijectively to the accepted initial certificates",
                )
            },
        )?;
    if expected_replay_certificates
        .iter()
        .map(|certificate| certificate.domain_id)
        .collect::<BTreeSet<_>>()
        .len()
        != replay_atom_ids.len()
        || !assumption_certificate_exactly_matches(
            &shrinking.final_authoritative_guarded_core_certificates,
            &replay_atom_ids,
            &expected_replay_certificates,
        )
        || shrinking.final_authoritative_guarded_core_certificates
            != shrinking.final_observation_guarded_core_certificates
    {
        return Err(invalid_input(
            "/guarded_core/replay/source_certificates",
            "retained replay atoms do not match the independently proven source certificates",
        ));
    }

    let experiment_started = Instant::now();
    let prior_reference = &reconstructed.prior_reference;
    let hint_sha256 = sha256_json(prior_reference);
    let hint_atom_matches = replay_atoms
        .iter()
        .map(|atom| hint_atom_match(prior_reference, atom, 16))
        .collect::<Vec<_>>();
    let hint_matches_complete_replay_conjunction =
        if hint_atom_matches.iter().any(|value| *value == Some(false)) {
            Some(false)
        } else if hint_atom_matches.iter().all(|value| *value == Some(true)) {
            Some(true)
        } else {
            None
        };
    // Counterbalanced authoritative wave: A, B, B, A. Every call constructs a fresh solver.
    let (
        baseline_authoritative_layout,
        baseline_authoritative_guarded_core_certificates,
        baseline_authoritative_boundary_certificates,
        _,
        _,
    ) = exact::shared_layer::solve_sparse_support_guarded_core_feasibility(
        input.clone(),
        logistics_components,
        Some(search_budget),
        Some(prior_reference),
        Vec::new(),
        exact::shared_layer::GuardedCorePosting::Assumptions,
    );
    let (
        replay_authoritative_layout,
        replay_authoritative_guarded_core_certificates,
        replay_authoritative_boundary_certificates,
        _,
        _,
    ) = exact::shared_layer::solve_sparse_support_guarded_core_feasibility(
        input.clone(),
        logistics_components,
        Some(search_budget),
        Some(prior_reference),
        replay_atoms.clone(),
        exact::shared_layer::GuardedCorePosting::ReplayClause,
    );
    let (
        reverse_replay_authoritative_layout,
        reverse_replay_authoritative_guarded_core_certificates,
        reverse_replay_authoritative_boundary_certificates,
        _,
        _,
    ) = exact::shared_layer::solve_sparse_support_guarded_core_feasibility(
        input.clone(),
        logistics_components,
        Some(search_budget),
        Some(prior_reference),
        replay_atoms.clone(),
        exact::shared_layer::GuardedCorePosting::ReplayClause,
    );
    let (
        reverse_baseline_authoritative_layout,
        reverse_baseline_authoritative_guarded_core_certificates,
        reverse_baseline_authoritative_boundary_certificates,
        _,
        _,
    ) = exact::shared_layer::solve_sparse_support_guarded_core_feasibility(
        input.clone(),
        logistics_components,
        Some(search_budget),
        Some(prior_reference),
        Vec::new(),
        exact::shared_layer::GuardedCorePosting::Assumptions,
    );
    // Observation wave is separate and never supplies the authoritative outcome.
    let (
        baseline_observation_layout,
        baseline_observation_guarded_core_certificates,
        baseline_observation_boundary_certificates,
        _,
        _,
        baseline_root_snapshot,
    ) = exact::shared_layer::solve_sparse_support_guarded_core_root_snapshot(
        input.clone(),
        logistics_components,
        Some(search_budget),
        Some(prior_reference),
        replay_atoms.clone(),
        exact::shared_layer::GuardedCorePosting::ObserveOnly,
    );
    let (
        replay_observation_layout,
        replay_observation_guarded_core_certificates,
        replay_observation_boundary_certificates,
        _,
        _,
        replay_root_snapshot,
    ) = exact::shared_layer::solve_sparse_support_guarded_core_root_snapshot(
        input,
        logistics_components,
        Some(search_budget),
        Some(prior_reference),
        replay_atoms,
        exact::shared_layer::GuardedCorePosting::ReplayClause,
    );

    let baseline_authoritative_outcome = classify_outcome(&baseline_authoritative_layout);
    let reverse_baseline_authoritative_outcome =
        classify_outcome(&reverse_baseline_authoritative_layout);
    let baseline_observation_outcome = classify_outcome(&baseline_observation_layout);
    let replay_authoritative_outcome = classify_outcome(&replay_authoritative_layout);
    let reverse_replay_authoritative_outcome =
        classify_outcome(&reverse_replay_authoritative_layout);
    let replay_observation_outcome = classify_outcome(&replay_observation_layout);
    let baseline_certificate_satisfied = assumption_certificate_exactly_matches(
        &baseline_authoritative_guarded_core_certificates,
        &[],
        &[],
    ) && baseline_authoritative_guarded_core_certificates
        == reverse_baseline_authoritative_guarded_core_certificates
        && observe_certificate_exactly_matches(
            &baseline_observation_guarded_core_certificates,
            &replay_atom_ids,
            &expected_replay_certificates,
        );
    let replay_clause_certificate_satisfied = replay_clause_certificate_exactly_matches(
        &replay_authoritative_guarded_core_certificates,
        &replay_atom_ids,
        &expected_replay_certificates,
    ) && replay_authoritative_guarded_core_certificates
        == reverse_replay_authoritative_guarded_core_certificates
        && replay_authoritative_guarded_core_certificates
            == replay_observation_guarded_core_certificates;
    let legal_boundary_keys = exact::reachable_boundary_keys(16, 16);
    let unrestricted_boundary_satisfied = [
        (
            &baseline_authoritative_layout,
            &baseline_authoritative_boundary_certificates,
        ),
        (
            &baseline_observation_layout,
            &baseline_observation_boundary_certificates,
        ),
        (
            &reverse_baseline_authoritative_layout,
            &reverse_baseline_authoritative_boundary_certificates,
        ),
        (
            &replay_authoritative_layout,
            &replay_authoritative_boundary_certificates,
        ),
        (
            &reverse_replay_authoritative_layout,
            &reverse_replay_authoritative_boundary_certificates,
        ),
        (
            &replay_observation_layout,
            &replay_observation_boundary_certificates,
        ),
    ]
    .iter()
    .all(|(layout, certificates)| {
        complete_unrestricted_boundary_certificate(
            layout,
            certificates,
            &fixture.external_terminal_ids,
            &legal_boundary_keys,
        )
    });
    let baseline_model_identity_satisfied = same_exact_model_across_formulations(
        &baseline_authoritative_layout,
        &baseline_observation_layout,
        EXPECTED_FORMULATION,
        EXPECTED_OBSERVE_FORMULATION,
    ) && same_exact_model_for_formulation(
        &baseline_authoritative_layout,
        &reverse_baseline_authoritative_layout,
        EXPECTED_FORMULATION,
    );
    let baseline_matches_accepted_control = same_exact_model(
        &shrinking.initial_gate.control_layout,
        &baseline_authoritative_layout,
    );
    let replay_model_identity_satisfied = same_exact_model_for_formulation(
        &replay_authoritative_layout,
        &replay_observation_layout,
        EXPECTED_REPLAY_FORMULATION,
    ) && same_exact_model_for_formulation(
        &replay_authoritative_layout,
        &reverse_replay_authoritative_layout,
        EXPECTED_REPLAY_FORMULATION,
    );
    let exact_clause_delta_satisfied = guarded_replay_delta_is_exact(
        &baseline_authoritative_layout,
        &replay_authoritative_layout,
        &expected_replay_certificates,
    ) && guarded_replay_delta_is_exact(
        &reverse_baseline_authoritative_layout,
        &reverse_replay_authoritative_layout,
        &expected_replay_certificates,
    );
    let baseline_root_infeasible = baseline_root_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.capture_status == "root-infeasible");
    let replay_root_infeasible = replay_root_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.capture_status == "root-infeasible");
    let replay_newly_root_eliminated = !baseline_root_infeasible && replay_root_infeasible;
    let root_core_domain_deltas = baseline_root_snapshot
        .as_ref()
        .zip(replay_root_snapshot.as_ref())
        .filter(|_| !baseline_root_infeasible && !replay_root_infeasible)
        .map(|(baseline, replay)| {
            baseline
                .guarded_core_atoms
                .iter()
                .zip(&replay.guarded_core_atoms)
                .map(|(baseline, replay)| GuardedCoreRootAtomDelta {
                    stable_id: baseline.stable_id.clone(),
                    baseline_domain: baseline.domain.clone(),
                    replay_domain: replay.domain.clone(),
                    changed: baseline.domain != replay.domain,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let root_changed_atom_count = root_core_domain_deltas
        .iter()
        .filter(|delta| delta.changed)
        .count();
    let captured_atom_contract = |snapshot: &exact::shared_layer::RootDomainSnapshot| {
        snapshot.capture_status == "captured-before-first-decision"
            && snapshot.guarded_core_atoms.len() == replay_atom_ids.len()
            && snapshot
                .guarded_core_atoms
                .iter()
                .zip(&expected_replay_certificates)
                .all(|(atom, certificate)| {
                    atom.stable_id == certificate.stable_id
                        && atom.domain_id == certificate.domain_id
                })
    };
    let root_snapshot_contract_satisfied = baseline_root_snapshot
        .as_ref()
        .zip(replay_root_snapshot.as_ref())
        .is_some_and(|(baseline, replay)| {
            match (baseline_root_infeasible, replay_root_infeasible) {
                (false, false) => {
                    captured_atom_contract(baseline)
                        && captured_atom_contract(replay)
                        && root_core_domain_deltas.len() == replay_atom_ids.len()
                }
                (false, true) => {
                    captured_atom_contract(baseline)
                        && replay.guarded_core_atoms.is_empty()
                        && replay_observation_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
                }
                (true, true) => {
                    baseline.guarded_core_atoms.is_empty()
                        && replay.guarded_core_atoms.is_empty()
                        && baseline_observation_outcome
                            == ExactDimensionCaseOutcome::ProvenInfeasible
                        && replay_observation_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
                }
                (true, false) => false,
            }
        })
        && root_capture_transition_is_valid(
            baseline_root_infeasible,
            replay_root_infeasible,
            baseline_observation_outcome,
            replay_observation_outcome,
        );
    let outcomes = [
        baseline_authoritative_outcome,
        reverse_baseline_authoritative_outcome,
        baseline_observation_outcome,
        replay_authoritative_outcome,
        reverse_replay_authoritative_outcome,
        replay_observation_outcome,
    ];
    let evidence_valid = outcomes
        .iter()
        .all(|outcome| *outcome != ExactDimensionCaseOutcome::InvalidWitness)
        && outcomes_are_satisfiability_consistent(
            outcomes
                .into_iter()
                .chain(std::iter::once(shrinking.initial_gate.control_outcome)),
        );
    let repeated_outcomes_consistent = baseline_authoritative_outcome
        == reverse_baseline_authoritative_outcome
        && replay_authoritative_outcome == reverse_replay_authoritative_outcome;
    let interpretation_blocked = !baseline_certificate_satisfied
        || !replay_clause_certificate_satisfied
        || !unrestricted_boundary_satisfied
        || !baseline_model_identity_satisfied
        || !baseline_matches_accepted_control
        || !replay_model_identity_satisfied
        || !exact_clause_delta_satisfied
        || !root_snapshot_contract_satisfied
        || !evidence_valid;
    let performance_comparison_allowed = !interpretation_blocked && repeated_outcomes_consistent;
    let performance_classification = classify_replay_performance(
        performance_comparison_allowed,
        repeated_outcomes_consistent,
        baseline_authoritative_outcome,
        replay_authoritative_outcome,
    );
    let status = if interpretation_blocked {
        GuardedCoreReplayStatus::Blocked
    } else {
        GuardedCoreReplayStatus::Completed
    };
    let total_wall_ms = shrinking
        .total_wall_ms
        .saturating_add(millis(total_started.elapsed()));

    Ok(GuardedCoreReplayReport {
        schema_version: 1,
        search_budget_ms: millis(search_budget),
        replay_atom_ids,
        baseline_authoritative_outcome,
        reverse_baseline_authoritative_outcome,
        baseline_observation_outcome,
        replay_authoritative_outcome,
        reverse_replay_authoritative_outcome,
        replay_observation_outcome,
        execution_order: vec![
            "ab:baseline-authoritative".to_string(),
            "ab:replay-authoritative".to_string(),
            "ba:replay-authoritative".to_string(),
            "ba:baseline-authoritative".to_string(),
            "observation:baseline".to_string(),
            "observation:replay".to_string(),
        ],
        process_id: std::process::id(),
        operating_system: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        hint_sha256,
        hint_atom_matches,
        hint_matches_complete_replay_conjunction,
        source_proof_satisfied,
        baseline_certificate_satisfied,
        replay_clause_certificate_satisfied,
        unrestricted_boundary_satisfied,
        baseline_model_identity_satisfied,
        baseline_matches_accepted_control,
        replay_model_identity_satisfied,
        exact_clause_delta_satisfied,
        root_snapshot_contract_satisfied,
        root_core_domain_deltas,
        root_changed_atom_count,
        baseline_root_infeasible,
        replay_root_infeasible,
        replay_newly_root_eliminated,
        evidence_valid,
        repeated_outcomes_consistent,
        performance_comparison_allowed,
        performance_classification,
        status,
        interpretation_blocked,
        experiment_ms: millis(experiment_started.elapsed()),
        total_wall_ms,
        baseline_authoritative_layout,
        reverse_baseline_authoritative_layout,
        baseline_observation_layout,
        replay_authoritative_layout,
        reverse_replay_authoritative_layout,
        replay_observation_layout,
        baseline_root_snapshot,
        replay_root_snapshot,
        baseline_authoritative_guarded_core_certificates,
        baseline_observation_guarded_core_certificates,
        reverse_baseline_authoritative_guarded_core_certificates,
        replay_authoritative_guarded_core_certificates,
        replay_observation_guarded_core_certificates,
        reverse_replay_authoritative_guarded_core_certificates,
        baseline_authoritative_boundary_certificates,
        baseline_observation_boundary_certificates,
        reverse_baseline_authoritative_boundary_certificates,
        replay_authoritative_boundary_certificates,
        replay_observation_boundary_certificates,
        reverse_replay_authoritative_boundary_certificates,
        shrinking,
        diagnostic_only: true,
    })
}

struct CompactSolveMetrics {
    construction_ms: Option<u64>,
    search_ms: Option<u64>,
    first_incumbent_ms: Option<u64>,
    branch_decisions: Option<u64>,
    backtracks: Option<u64>,
    conflicts: Option<u64>,
    learned_clauses: Option<u64>,
    solver_propagations: Option<u64>,
    variables: Option<u64>,
    constraints: Option<u64>,
    incidences: Option<u64>,
    termination: Option<String>,
    proof: Option<String>,
    validation: Option<String>,
    model_complexity: Option<crate::research::ModelComplexityMetrics>,
}

fn compact_solve_metrics(report: &IntegratedLayoutReport) -> CompactSolveMetrics {
    let exact = report.exact.as_ref();
    let statistics = exact.map(|exact| &exact.search_statistics);
    CompactSolveMetrics {
        construction_ms: exact.map(|exact| exact.construction_ms),
        search_ms: exact.map(|exact| exact.search_ms),
        first_incumbent_ms: exact.and_then(|exact| exact.first_incumbent_ms),
        branch_decisions: statistics.and_then(|statistics| statistics.branch_decisions),
        backtracks: statistics.and_then(|statistics| statistics.backtracks),
        conflicts: statistics.and_then(|statistics| statistics.conflicts),
        learned_clauses: statistics.and_then(|statistics| statistics.learned_clauses),
        solver_propagations: statistics.and_then(|statistics| statistics.solver_propagations),
        variables: exact.map(|exact| exact.model_complexity.variables.total_variables),
        constraints: exact.and_then(|exact| {
            exact
                .model_complexity
                .constraints
                .as_ref()
                .map(|constraints| constraints.total_constraints)
        }),
        incidences: exact.and_then(|exact| {
            exact
                .model_complexity
                .factor_graph
                .as_ref()
                .map(|graph| graph.incidences)
        }),
        termination: exact.map(|exact| format!("{:?}", exact.termination)),
        proof: exact.map(|exact| format!("{:?}", exact.proof)),
        validation: exact.map(|exact| format!("{:?}", exact.validation)),
        model_complexity: exact.map(|exact| exact.model_complexity.clone()),
    }
}

fn classify_shrink_attempt(
    outcome: ExactDimensionCaseOutcome,
    certificate_satisfied: bool,
    unrestricted_boundary_satisfied: bool,
    exact_model_delta_satisfied: bool,
    evidence_conflict: bool,
) -> (bool, bool) {
    let blocked = outcome == ExactDimensionCaseOutcome::InvalidWitness
        || !certificate_satisfied
        || !unrestricted_boundary_satisfied
        || !exact_model_delta_satisfied;
    let blocked = blocked || evidence_conflict;
    let removed = !blocked && outcome == ExactDimensionCaseOutcome::ProvenInfeasible;
    (removed, blocked)
}

struct ReconstructedGuardedCore {
    atoms: Vec<exact::shared_layer::GuardedCoreAtom>,
    prior_reference: IntegratedLayoutReport,
}

fn reconstruct_guarded_core(
    parent: &MaterialRow5SeparatorReport,
    input: &crate::layouts::integrated::model::ModelInput,
) -> Result<ReconstructedGuardedCore, IntegratedLayoutReport> {
    let row4_parent = &parent.parent.parent;
    let source_parent = &row4_parent.parent;
    let endpoint_parent = &source_parent.parent;
    let cell_parent = &endpoint_parent.parent;
    let boundary_parent = &cell_parent.parent.parent;
    let tuple_parent = &boundary_parent.parent;
    let parent_assignments = tuple_parent
        .parent
        .inherited_assignments
        .iter()
        .chain(&tuple_parent.parent.assignments)
        .cloned()
        .collect::<Vec<_>>();
    let requested = parent_assignments
        .iter()
        .chain(&boundary_parent.selected_assignments)
        .cloned()
        .collect::<Vec<_>>();
    assert_distinct_assignments(
        &requested,
        EXPECTED_TOTAL_FIXED_TERMINALS,
        "/guarded_core/facility_ports",
    )?;

    let prior_reference = &tuple_parent.parent.prior_reference;
    if input.instances.len() != 4 || prior_reference.placements.len() != 3 {
        return Err(invalid_input(
            "/guarded_core/placements",
            "accepted guarded-core fixture requires three inherited and one new placement",
        ));
    }

    let mut atoms = vec![
        exact::shared_layer::GuardedCoreAtom::UsedWidth { value: 16 },
        exact::shared_layer::GuardedCoreAtom::UsedHeight { value: 16 },
    ];
    for placement in &prior_reference.placements {
        atoms.push(exact::shared_layer::GuardedCoreAtom::Placement {
            instance: placement.instance.clone(),
            x: i32::try_from(placement.x).map_err(|_| {
                invalid_input(
                    "/guarded_core/placements/x",
                    "prior placement x does not fit i32",
                )
            })?,
            y: i32::try_from(placement.y).map_err(|_| {
                invalid_input(
                    "/guarded_core/placements/y",
                    "prior placement y does not fit i32",
                )
            })?,
            rotation: placement.rotation,
        });
    }
    atoms.push(exact::shared_layer::GuardedCoreAtom::Placement {
        instance: tuple_parent.parent.partitioned_facility.clone(),
        x: tuple_parent.parent.fixed_coordinate[0],
        y: tuple_parent.parent.fixed_coordinate[1],
        rotation: tuple_parent.parent.fixed_rotation,
    });
    atoms.extend(requested.iter().map(|assignment| {
        exact::shared_layer::GuardedCoreAtom::FacilityPort {
            terminal: assignment.terminal.clone(),
            port: assignment.port.clone(),
        }
    }));
    atoms.push(exact::shared_layer::GuardedCoreAtom::ExternalBoundaryKey {
        terminal: cell_parent.selected_terminal.clone(),
        key: endpoint_parent.selected_boundary_key,
    });
    atoms.extend([
        exact::shared_layer::GuardedCoreAtom::MaterialArcFlowAtLeast {
            network: parent.selected_network_id.clone(),
            from: 48,
            to: 64,
            minimum: 1,
        },
        exact::shared_layer::GuardedCoreAtom::MaterialArcFlowEquals {
            network: parent.selected_network_id.clone(),
            from: 48,
            to: 32,
            value: 0,
        },
    ]);
    for (from, to) in [(64, 80), (80, 81), (80, 96)] {
        atoms.push(exact::shared_layer::GuardedCoreAtom::MaterialArcSelected {
            network: parent.selected_network_id.clone(),
            from,
            to,
        });
        atoms.push(exact::shared_layer::GuardedCoreAtom::MaterialArcItem {
            network: parent.selected_network_id.clone(),
            from,
            to,
            item: parent.selected_item.clone(),
        });
    }
    Ok(ReconstructedGuardedCore {
        atoms,
        prior_reference: prior_reference.clone(),
    })
}

fn build_phase_identity(
    instance_wiring: &FacilityInstanceWiringReport,
    input: &crate::layouts::integrated::model::ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
) -> GuardedCorePhaseIdentity {
    let mut belt_code = 0_i32;
    let mut pipe_code = 0_i32;
    let mut external_terminal_ids = Vec::new();
    let mut network_item_codes = Vec::new();
    let networks = input
        .networks
        .iter()
        .enumerate()
        .map(|(network_index, network)| {
            let item_code = match network.transport() {
                TransportKind::Belt => {
                    belt_code += 1;
                    belt_code
                }
                TransportKind::Pipe => {
                    pipe_code += 1;
                    pipe_code
                }
            };
            network_item_codes.push(format!("{}={item_code}", network.id()));
            let terminals = network
                .terminals()
                .iter()
                .map(|terminal| {
                    if matches!(
                        terminal.endpoint(),
                        crate::layouts::integrated::model::EndpointInput::External { .. }
                    ) {
                        external_terminal_ids.push(terminal.id().to_string());
                    }
                    serde_json::json!({
                        "id": terminal.id(),
                        "route_index": terminal.route_index(),
                        "direction": terminal.direction(),
                        "endpoint": endpoint_semantics(terminal.endpoint()),
                        "rate": terminal.rate(),
                        "flow_units": terminal.flow_units(),
                    })
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "index": network_index,
                "id": network.id(),
                "item": network.item(),
                "transport": network.transport(),
                "item_code": item_code,
                "line_capacity_rate": network.line_capacity_rate(),
                "flow_scale": network.flow_scale(),
                "line_capacity_units": network.line_capacity_units(),
                "component_capacity_units": {
                    "splitter": network.component_capacity_units(crate::logistics::LogisticsComponentKind::Splitter),
                    "converger": network.component_capacity_units(crate::logistics::LogisticsComponentKind::Converger),
                    "bridge": network.component_capacity_units(crate::logistics::LogisticsComponentKind::Bridge),
                },
                "route_indices": network.route_indices(),
                "terminals": terminals,
            })
        })
        .collect::<Vec<_>>();
    external_terminal_ids.sort();

    let instances = input
        .instances
        .iter()
        .map(|instance| {
            serde_json::json!({
                "id": instance.id,
                "recipe": instance.recipe,
                "facility": instance.facility,
                "definition": {
                    "id": instance.definition.id,
                    "footprint": {
                        "width": instance.definition.footprint.width,
                        "height": instance.definition.footprint.height,
                    },
                    "allowed_rotations": instance.definition.allowed_rotations,
                    "ports": instance.definition.ports,
                },
            })
        })
        .collect::<Vec<_>>();
    let edges = input
        .edges
        .iter()
        .map(|edge| {
            serde_json::json!({
                "requirement_id": edge.requirement_id,
                "edge": edge.edge,
                "source": endpoint_semantics(&edge.source),
                "target": endpoint_semantics(&edge.target),
                "transport": edge.transport,
                "capacity_rate": edge.capacity_rate,
                "component_capacity_rates": edge.component_capacity_rates.values(),
            })
        })
        .collect::<Vec<_>>();
    let phase_semantics = serde_json::json!({
        "schema": "guarded-core-phase-semantics-v1",
        "width": input.width,
        "height": input.height,
        "cell_count": input.cell_count,
        "instances": instances,
        "edges": edges,
        "networks": networks,
        "logistics_components": {
            "schema_version": logistics_components.catalog().schema_version,
            "components": &logistics_components.catalog().components,
        },
    });
    GuardedCorePhaseIdentity {
        solver_signature: expected_solver_signature(),
        workload_wiring_sha256: sha256_json(instance_wiring),
        phase_model_semantics_sha256: sha256_json(&phase_semantics),
        external_terminal_ids,
        network_item_codes,
    }
}

fn endpoint_semantics(
    endpoint: &crate::layouts::integrated::model::EndpointInput,
) -> serde_json::Value {
    match endpoint {
        crate::layouts::integrated::model::EndpointInput::Facility { instance, ports } => {
            serde_json::json!({
                "kind": "facility",
                "instance": instance,
                "ports": ports,
            })
        }
        crate::layouts::integrated::model::EndpointInput::External { node } => serde_json::json!({
            "kind": "external",
            "node": node,
        }),
    }
}

fn sha256_json(value: &impl Serialize) -> String {
    let encoded = serde_json::to_vec(value).expect("canonical guarded-core semantics serialize");
    let mut digest = Sha256::new();
    digest.update((encoded.len() as u64).to_be_bytes());
    digest.update(encoded);
    digest
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        })
}

fn hint_atom_match(
    hint: &IntegratedLayoutReport,
    atom: &exact::shared_layer::GuardedCoreAtom,
    grid_width: usize,
) -> Option<bool> {
    match atom {
        exact::shared_layer::GuardedCoreAtom::UsedWidth { value } => hint
            .bounds
            .as_ref()
            .map(|bounds| bounds.width == i64::from(*value)),
        exact::shared_layer::GuardedCoreAtom::UsedHeight { value } => hint
            .bounds
            .as_ref()
            .map(|bounds| bounds.height == i64::from(*value)),
        exact::shared_layer::GuardedCoreAtom::Placement {
            instance,
            x,
            y,
            rotation,
        } => hint
            .placements
            .iter()
            .find(|placement| placement.instance == *instance)
            .map(|placement| {
                placement.x == i64::from(*x)
                    && placement.y == i64::from(*y)
                    && placement.rotation == *rotation
            }),
        exact::shared_layer::GuardedCoreAtom::FacilityPort { terminal, port } => hint
            .transport_networks
            .iter()
            .flat_map(|network| network.terminals.iter())
            .find(|candidate| candidate.id == *terminal)
            .and_then(|candidate| match &candidate.endpoint {
                TransportNetworkEndpoint::Facility {
                    port: hinted_port, ..
                } => Some(hinted_port == port),
                TransportNetworkEndpoint::External { .. } => None,
            }),
        exact::shared_layer::GuardedCoreAtom::MaterialArcSelected { network, from, to } => hint
            .transport_networks
            .iter()
            .find(|candidate| candidate.id == *network)
            .map(|candidate| {
                candidate.segments.iter().any(|segment| {
                    world_grid_index(&segment.from, grid_width) == Some(*from)
                        && world_grid_index(&segment.to, grid_width) == Some(*to)
                })
            }),
        exact::shared_layer::GuardedCoreAtom::ExternalBoundaryKey { .. }
        | exact::shared_layer::GuardedCoreAtom::MaterialArcItem { .. }
        | exact::shared_layer::GuardedCoreAtom::MaterialArcFlowAtLeast { .. }
        | exact::shared_layer::GuardedCoreAtom::MaterialArcFlowEquals { .. } => None,
    }
}

fn world_grid_index(position: &WorldGridPosition, grid_width: usize) -> Option<usize> {
    let x = usize::try_from(position.x).ok()?;
    let y = usize::try_from(position.y).ok()?;
    y.checked_mul(grid_width)?.checked_add(x)
}

fn assumption_certificate_matches(
    certificates: &[exact::shared_layer::GuardedCoreBuildCertificate],
    atom_ids: &[String],
) -> bool {
    certificates.len() == 1
        && certificates[0].posting == exact::shared_layer::GuardedCorePosting::Assumptions
        && certificates[0].clause.is_none()
        && certificates[0].atoms.len() == atom_ids.len()
        && certificates[0]
            .atoms
            .iter()
            .map(|atom| atom.stable_id.as_str())
            .eq(atom_ids.iter().map(String::as_str))
}

fn expected_atom_certificate_subset(
    initial: &[exact::shared_layer::GuardedCoreAtomCertificate],
    retained_atom_ids: &[String],
) -> Option<Vec<exact::shared_layer::GuardedCoreAtomCertificate>> {
    let by_id = initial
        .iter()
        .map(|certificate| (certificate.stable_id.as_str(), certificate))
        .collect::<std::collections::BTreeMap<_, _>>();
    if by_id.len() != initial.len()
        || retained_atom_ids.iter().collect::<BTreeSet<_>>().len() != retained_atom_ids.len()
    {
        return None;
    }
    retained_atom_ids
        .iter()
        .enumerate()
        .map(|(atom_index, atom_id)| {
            let mut certificate = (*by_id.get(atom_id.as_str())?).clone();
            certificate.atom_index = atom_index;
            Some(certificate)
        })
        .collect()
}

fn assumption_certificate_exactly_matches(
    certificates: &[exact::shared_layer::GuardedCoreBuildCertificate],
    atom_ids: &[String],
    expected_atoms: &[exact::shared_layer::GuardedCoreAtomCertificate],
) -> bool {
    assumption_certificate_matches(certificates, atom_ids)
        && certificates[0].atoms == expected_atoms
}

fn replay_clause_certificate_exactly_matches(
    certificates: &[exact::shared_layer::GuardedCoreBuildCertificate],
    atom_ids: &[String],
    expected_atoms: &[exact::shared_layer::GuardedCoreAtomCertificate],
) -> bool {
    certificates.len() == 1
        && !atom_ids.is_empty()
        && atom_ids.len() == expected_atoms.len()
        && certificates[0].posting == exact::shared_layer::GuardedCorePosting::ReplayClause
        && certificates[0].atoms == expected_atoms
        && certificates[0]
            .atoms
            .iter()
            .map(|atom| atom.stable_id.as_str())
            .eq(atom_ids.iter().map(String::as_str))
        && certificates[0].clause.as_ref().is_some_and(|clause| {
            clause.variable_count_delta == 0
                && clause.clause_count_delta == 1
                && clause.clause_arity == atom_ids.len()
                && clause.atoms == expected_atoms
        })
}

fn observe_certificate_exactly_matches(
    certificates: &[exact::shared_layer::GuardedCoreBuildCertificate],
    atom_ids: &[String],
    expected_atoms: &[exact::shared_layer::GuardedCoreAtomCertificate],
) -> bool {
    certificates.len() == 1
        && certificates[0].posting == exact::shared_layer::GuardedCorePosting::ObserveOnly
        && certificates[0].clause.is_none()
        && certificates[0].atoms == expected_atoms
        && certificates[0]
            .atoms
            .iter()
            .map(|atom| atom.stable_id.as_str())
            .eq(atom_ids.iter().map(String::as_str))
}

fn complete_unrestricted_boundary_certificate(
    report: &IntegratedLayoutReport,
    certificates: &[exact::shared_layer::BoundaryKeyBuildCertificate],
    expected_terminal_ids: &[String],
    legal_boundary_keys: &[i32],
) -> bool {
    let Some(exact) = report.exact.as_ref() else {
        return false;
    };
    boundary_certificates_are_complete(expected_terminal_ids, certificates, legal_boundary_keys)
        && exact.model.external_terminal_count == expected_terminal_ids.len()
}

fn boundary_certificates_are_complete(
    expected_terminal_ids: &[String],
    certificates: &[exact::shared_layer::BoundaryKeyBuildCertificate],
    legal_boundary_keys: &[i32],
) -> bool {
    let expected_terminal_ids = expected_terminal_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let certificate_terminal_ids = certificates
        .iter()
        .map(|certificate| certificate.terminal.clone())
        .collect::<BTreeSet<_>>();
    certificates.len() == expected_terminal_ids.len()
        && certificate_terminal_ids == expected_terminal_ids
        && certificates.iter().all(|certificate| {
            certificate.declared_domain_kind == "sparse-legal"
                && certificate.declared_values == legal_boundary_keys
                && certificate.unary_table_projection == legal_boundary_keys
                && certificate.routing_option_keys == legal_boundary_keys
                && certificate.restriction_values.is_none()
        })
}

fn initial_gate_is_blocked(required_gates: &[bool], full_core_proven_infeasible: bool) -> bool {
    !full_core_proven_infeasible || required_gates.iter().any(|gate| !gate)
}

fn same_exact_model(left: &IntegratedLayoutReport, right: &IntegratedLayoutReport) -> bool {
    same_exact_model_for_formulation(left, right, EXPECTED_FORMULATION)
}

fn same_exact_model_for_formulation(
    left: &IntegratedLayoutReport,
    right: &IntegratedLayoutReport,
    expected_formulation: &str,
) -> bool {
    let (Some(left), Some(right)) = (left.exact.as_ref(), right.exact.as_ref()) else {
        return false;
    };
    left.formulation == expected_formulation
        && right.formulation == expected_formulation
        && left.formulation == right.formulation
        && left.model == right.model
        && left.model_complexity == right.model_complexity
}

fn same_exact_model_across_formulations(
    left: &IntegratedLayoutReport,
    right: &IntegratedLayoutReport,
    expected_left_formulation: &str,
    expected_right_formulation: &str,
) -> bool {
    let (Some(left), Some(right)) = (left.exact.as_ref(), right.exact.as_ref()) else {
        return false;
    };
    left.formulation == expected_left_formulation
        && right.formulation == expected_right_formulation
        && left.model == right.model
        && left.model_complexity == right.model_complexity
}

fn guarded_replay_delta_is_exact(
    baseline: &IntegratedLayoutReport,
    replay: &IntegratedLayoutReport,
    atoms: &[exact::shared_layer::GuardedCoreAtomCertificate],
) -> bool {
    let (Some(baseline), Some(replay)) = (baseline.exact.as_ref(), replay.exact.as_ref()) else {
        return false;
    };
    let arity = u64::try_from(atoms.len()).expect("guarded replay arity fits u64");
    if arity == 0
        || baseline.formulation != EXPECTED_FORMULATION
        || replay.formulation != EXPECTED_REPLAY_FORMULATION
        || baseline.model != replay.model
        || baseline.model_complexity.variables != replay.model_complexity.variables
        || baseline.model_complexity.symmetry != replay.model_complexity.symmetry
        || baseline.model_complexity.estimated_bytes != replay.model_complexity.estimated_bytes
    {
        return false;
    }
    let (Some(baseline_constraints), Some(replay_constraints)) = (
        baseline.model_complexity.constraints.as_ref(),
        replay.model_complexity.constraints.as_ref(),
    ) else {
        return false;
    };
    let guarded_families = replay_constraints
        .by_family
        .iter()
        .filter(|family| family.family == "guarded-core")
        .collect::<Vec<_>>();
    let non_guarded_replay_families = replay_constraints
        .by_family
        .iter()
        .filter(|family| family.family != "guarded-core")
        .cloned()
        .collect::<Vec<_>>();
    if baseline_constraints.total_constraints.checked_add(1)
        != Some(replay_constraints.total_constraints)
        || baseline_constraints.total_terms.checked_add(arity)
            != Some(replay_constraints.total_terms)
        || guarded_families.len() != 1
        || guarded_families[0].constraints != 1
        || guarded_families[0].terms != arity
        || guarded_families[0].relation != crate::research::ConstraintRelation::Other
        || guarded_families[0].maximum_arity != arity
        || guarded_families[0].p95_arity != arity
        || guarded_families[0].maximum_absolute_coefficient != 1
        || non_guarded_replay_families != baseline_constraints.by_family
    {
        return false;
    }
    let (Some(baseline_graph), Some(replay_graph)) = (
        baseline.model_complexity.factor_graph.as_ref(),
        replay.model_complexity.factor_graph.as_ref(),
    ) else {
        return false;
    };
    let guarded_incidences = replay_graph
        .family_incidences
        .iter()
        .filter(|incidence| incidence.constraint_family == "guarded-core")
        .map(|incidence| incidence.incidences)
        .sum::<u64>();
    let non_guarded_replay_incidences = replay_graph
        .family_incidences
        .iter()
        .filter(|incidence| incidence.constraint_family != "guarded-core")
        .cloned()
        .collect::<Vec<_>>();
    if baseline_graph.variable_vertices != replay_graph.variable_vertices
        || baseline_graph.constraint_vertices.checked_add(1)
            != Some(replay_graph.constraint_vertices)
        || baseline_graph.incidences.checked_add(arity) != Some(replay_graph.incidences)
        || guarded_incidences != arity
        || non_guarded_replay_incidences != baseline_graph.family_incidences
    {
        return false;
    }
    let (Some(baseline_coupling), Some(replay_coupling)) = (
        baseline.model_complexity.coupling.as_ref(),
        replay.model_complexity.coupling.as_ref(),
    ) else {
        return false;
    };
    let variable_families = atoms
        .iter()
        .map(|atom| atom.variable_family.as_str())
        .collect::<BTreeSet<_>>();
    let cross_family_delta = u64::from(variable_families.len() > 1);
    let placement_routing = variable_families.contains("placement")
        && variable_families
            .iter()
            .any(|family| matches!(*family, "route-arc" | "flow" | "route-cell" | "arm-item"));
    baseline_coupling.facility_network_incidences == replay_coupling.facility_network_incidences
        && baseline_coupling.shared_network_facility_pairs
            == replay_coupling.shared_network_facility_pairs
        && baseline_coupling
            .cross_family_constraints
            .checked_add(cross_family_delta)
            == Some(replay_coupling.cross_family_constraints)
        && baseline_coupling
            .placement_routing_constraints
            .checked_add(u64::from(placement_routing))
            == Some(replay_coupling.placement_routing_constraints)
        && baseline_coupling
            .placement_routing_incidences
            .checked_add(if placement_routing { arity } else { 0 })
            == Some(replay_coupling.placement_routing_incidences)
        && baseline_coupling.network_collision_constraints
            == replay_coupling.network_collision_constraints
        && baseline_coupling.objective_incidences == replay_coupling.objective_incidences
}

fn outcomes_are_satisfiability_consistent(
    outcomes: impl IntoIterator<Item = ExactDimensionCaseOutcome>,
) -> bool {
    let mut has_feasible = false;
    let mut has_proven_infeasible = false;
    for outcome in outcomes {
        has_feasible |= outcome == ExactDimensionCaseOutcome::ValidatedFeasible;
        has_proven_infeasible |= outcome == ExactDimensionCaseOutcome::ProvenInfeasible;
    }
    !(has_feasible && has_proven_infeasible)
}

fn root_capture_transition_is_valid(
    baseline_root_infeasible: bool,
    replay_root_infeasible: bool,
    baseline_outcome: ExactDimensionCaseOutcome,
    replay_outcome: ExactDimensionCaseOutcome,
) -> bool {
    match (baseline_root_infeasible, replay_root_infeasible) {
        (false, false) => true,
        (false, true) => replay_outcome == ExactDimensionCaseOutcome::ProvenInfeasible,
        (true, true) => {
            baseline_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
                && replay_outcome == ExactDimensionCaseOutcome::ProvenInfeasible
        }
        (true, false) => false,
    }
}

fn classify_replay_performance(
    performance_comparison_allowed: bool,
    repeated_outcomes_consistent: bool,
    baseline: ExactDimensionCaseOutcome,
    replay: ExactDimensionCaseOutcome,
) -> GuardedCoreReplayPerformanceClassification {
    if !performance_comparison_allowed {
        return if repeated_outcomes_consistent {
            GuardedCoreReplayPerformanceClassification::InconclusiveInvalidExperiment
        } else {
            GuardedCoreReplayPerformanceClassification::InconclusiveRepeatedOutcomes
        };
    }
    if !repeated_outcomes_consistent {
        return GuardedCoreReplayPerformanceClassification::InconclusiveRepeatedOutcomes;
    }
    let baseline_terminal = matches!(
        baseline,
        ExactDimensionCaseOutcome::ValidatedFeasible | ExactDimensionCaseOutcome::ProvenInfeasible
    );
    let replay_terminal = matches!(
        replay,
        ExactDimensionCaseOutcome::ValidatedFeasible | ExactDimensionCaseOutcome::ProvenInfeasible
    );
    match (baseline_terminal, replay_terminal) {
        (false, true) => {
            GuardedCoreReplayPerformanceClassification::ObservedReplayOutcomeImprovement
        }
        (true, false) => {
            GuardedCoreReplayPerformanceClassification::ObservedReplayOutcomeRegression
        }
        _ => GuardedCoreReplayPerformanceClassification::NoOutcomeWinner,
    }
}

fn guarded_core_delta_is_exact(
    control: &IntegratedLayoutReport,
    assumptions: &IntegratedLayoutReport,
    expected_arity: usize,
) -> bool {
    let (Some(control), Some(assumptions)) = (control.exact.as_ref(), assumptions.exact.as_ref())
    else {
        return false;
    };
    let expected = u64::try_from(expected_arity).expect("guarded-core arity fits u64");
    if control.formulation != EXPECTED_FORMULATION
        || assumptions.formulation != EXPECTED_FORMULATION
        || control.formulation != assumptions.formulation
        || control.model != assumptions.model
        || control.model_complexity.variables != assumptions.model_complexity.variables
        || control.model_complexity.coupling != assumptions.model_complexity.coupling
        || control.model_complexity.symmetry != assumptions.model_complexity.symmetry
    {
        return false;
    }
    if expected == 0 {
        return zero_arity_guarded_core_delta_is_exact(
            &control.model_complexity,
            &assumptions.model_complexity,
        );
    }
    let (Some(control_constraints), Some(assumption_constraints)) = (
        control.model_complexity.constraints.as_ref(),
        assumptions.model_complexity.constraints.as_ref(),
    ) else {
        return false;
    };
    let guarded_families = assumption_constraints
        .by_family
        .iter()
        .filter(|family| family.family == "guarded-core")
        .collect::<Vec<_>>();
    let non_guarded_assumption_families = assumption_constraints
        .by_family
        .iter()
        .filter(|family| family.family != "guarded-core")
        .cloned()
        .collect::<Vec<_>>();
    if control_constraints.total_constraints.checked_add(expected)
        != Some(assumption_constraints.total_constraints)
        || control_constraints.total_terms.checked_add(expected)
            != Some(assumption_constraints.total_terms)
        || guarded_families.len() != 1
        || guarded_families[0].constraints != expected
        || guarded_families[0].terms != expected
        || guarded_families[0].relation != crate::research::ConstraintRelation::Other
        || guarded_families[0].maximum_arity != 1
        || guarded_families[0].p95_arity != 1
        || guarded_families[0].maximum_absolute_coefficient != 1
        || non_guarded_assumption_families != control_constraints.by_family
    {
        return false;
    }
    let (Some(control_graph), Some(assumption_graph)) = (
        control.model_complexity.factor_graph.as_ref(),
        assumptions.model_complexity.factor_graph.as_ref(),
    ) else {
        return false;
    };
    let guarded_incidences = assumption_graph
        .family_incidences
        .iter()
        .filter(|incidence| incidence.constraint_family == "guarded-core")
        .map(|incidence| incidence.incidences)
        .sum::<u64>();
    let non_guarded_assumption_incidences = assumption_graph
        .family_incidences
        .iter()
        .filter(|incidence| incidence.constraint_family != "guarded-core")
        .cloned()
        .collect::<Vec<_>>();
    control_graph.variable_vertices == assumption_graph.variable_vertices
        && control_graph.constraint_vertices.checked_add(expected)
            == Some(assumption_graph.constraint_vertices)
        && control_graph.incidences.checked_add(expected) == Some(assumption_graph.incidences)
        && guarded_incidences == expected
        && non_guarded_assumption_incidences == control_graph.family_incidences
}

fn zero_arity_guarded_core_delta_is_exact(
    control: &crate::research::ModelComplexityMetrics,
    assumptions: &crate::research::ModelComplexityMetrics,
) -> bool {
    control == assumptions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boundary_certificate(
        terminal: &str,
        keys: &[i32],
    ) -> exact::shared_layer::BoundaryKeyBuildCertificate {
        exact::shared_layer::BoundaryKeyBuildCertificate {
            terminal: terminal.to_string(),
            network_index: 0,
            network_id: "network:test".to_string(),
            declared_domain_kind: "sparse-legal".to_string(),
            declared_lower_bound: *keys.first().expect("test keys are non-empty"),
            declared_upper_bound: *keys.last().expect("test keys are non-empty"),
            declared_values: keys.to_vec(),
            unary_table_projection: keys.to_vec(),
            routing_option_keys: keys.to_vec(),
            restriction_values: None,
        }
    }

    fn atom_certificate(
        atom_index: usize,
        stable_id: &str,
        domain_id: u32,
    ) -> exact::shared_layer::GuardedCoreAtomCertificate {
        exact::shared_layer::GuardedCoreAtomCertificate {
            atom_index,
            stable_id: stable_id.to_string(),
            domain_id,
            variable_family: "test".to_string(),
            variable_name: format!("domain-{domain_id}"),
            declared_lower_bound: 0,
            declared_upper_bound: 1,
            declared_cardinality: 2,
            relation: exact::shared_layer::NativePredicateRelation::Equal,
            value: 1,
            complement_relation: exact::shared_layer::NativePredicateRelation::NotEqual,
            complement_value: 1,
        }
    }

    #[test]
    fn initial_gate_blocks_every_non_proof_outcome() {
        assert!(initial_gate_is_blocked(&[true, true], false));
        assert!(initial_gate_is_blocked(&[true, false], true));
        assert!(!initial_gate_is_blocked(&[true, true], true));
    }

    #[test]
    fn unknown_cannot_mask_a_cross_run_satisfiability_conflict() {
        assert!(!outcomes_are_satisfiability_consistent([
            ExactDimensionCaseOutcome::Unknown,
            ExactDimensionCaseOutcome::ProvenInfeasible,
            ExactDimensionCaseOutcome::ValidatedFeasible,
            ExactDimensionCaseOutcome::Unknown,
        ]));
        assert!(outcomes_are_satisfiability_consistent([
            ExactDimensionCaseOutcome::Unknown,
            ExactDimensionCaseOutcome::ProvenInfeasible,
            ExactDimensionCaseOutcome::Unknown,
        ]));
        assert!(outcomes_are_satisfiability_consistent([
            ExactDimensionCaseOutcome::Unknown,
            ExactDimensionCaseOutcome::ValidatedFeasible,
        ]));
    }

    #[test]
    fn replay_root_conflict_is_a_valid_new_elimination() {
        assert!(root_capture_transition_is_valid(
            false,
            true,
            ExactDimensionCaseOutcome::Unknown,
            ExactDimensionCaseOutcome::ProvenInfeasible,
        ));
        assert!(!root_capture_transition_is_valid(
            false,
            true,
            ExactDimensionCaseOutcome::Unknown,
            ExactDimensionCaseOutcome::Unknown,
        ));
        assert!(!root_capture_transition_is_valid(
            true,
            false,
            ExactDimensionCaseOutcome::ProvenInfeasible,
            ExactDimensionCaseOutcome::Unknown,
        ));
    }

    #[test]
    fn inconsistent_abba_outcomes_never_select_a_performance_winner() {
        assert_eq!(
            classify_replay_performance(
                false,
                false,
                ExactDimensionCaseOutcome::Unknown,
                ExactDimensionCaseOutcome::ValidatedFeasible,
            ),
            GuardedCoreReplayPerformanceClassification::InconclusiveRepeatedOutcomes
        );
        assert_eq!(
            classify_replay_performance(
                true,
                true,
                ExactDimensionCaseOutcome::Unknown,
                ExactDimensionCaseOutcome::ValidatedFeasible,
            ),
            GuardedCoreReplayPerformanceClassification::ObservedReplayOutcomeImprovement
        );
    }

    #[test]
    fn invalid_experiment_never_selects_a_performance_winner() {
        assert_eq!(
            classify_replay_performance(
                false,
                true,
                ExactDimensionCaseOutcome::Unknown,
                ExactDimensionCaseOutcome::ValidatedFeasible,
            ),
            GuardedCoreReplayPerformanceClassification::InconclusiveInvalidExperiment
        );
    }

    #[test]
    fn shrinking_removes_only_a_valid_infeasibility_proof() {
        assert_eq!(
            classify_shrink_attempt(
                ExactDimensionCaseOutcome::ProvenInfeasible,
                true,
                true,
                true,
                false
            ),
            (true, false)
        );
        for outcome in [
            ExactDimensionCaseOutcome::ValidatedFeasible,
            ExactDimensionCaseOutcome::Unknown,
        ] {
            assert_eq!(
                classify_shrink_attempt(outcome, true, true, true, false),
                (false, false)
            );
        }
        assert_eq!(
            classify_shrink_attempt(
                ExactDimensionCaseOutcome::InvalidWitness,
                true,
                true,
                true,
                false
            ),
            (false, true)
        );
        for gates in [
            (false, true, true),
            (true, false, true),
            (true, true, false),
        ] {
            assert_eq!(
                classify_shrink_attempt(
                    ExactDimensionCaseOutcome::ProvenInfeasible,
                    gates.0,
                    gates.1,
                    gates.2,
                    false,
                ),
                (false, true)
            );
        }
        assert_eq!(
            classify_shrink_attempt(
                ExactDimensionCaseOutcome::ProvenInfeasible,
                true,
                true,
                true,
                true,
            ),
            (false, true)
        );
    }

    #[test]
    fn zero_atom_delta_requires_exact_atom_free_model_identity() {
        let control = crate::research::ModelComplexityMetrics::unavailable();
        assert!(zero_arity_guarded_core_delta_is_exact(&control, &control));
        let mut drifted = control.clone();
        drifted.estimated_bytes = Some(1);
        assert!(!zero_arity_guarded_core_delta_is_exact(&control, &drifted));
    }

    #[test]
    fn deletion_certificate_is_the_exact_reindexed_initial_subset() {
        let initial = vec![
            atom_certificate(0, "atom:a", 10),
            atom_certificate(1, "atom:b", 20),
            atom_certificate(2, "atom:c", 30),
        ];
        let retained = vec!["atom:a".to_string(), "atom:c".to_string()];
        let subset = expected_atom_certificate_subset(&initial, &retained)
            .expect("accepted subset should resolve");
        assert_eq!(subset[0], initial[0]);
        assert_eq!(subset[1].stable_id, "atom:c");
        assert_eq!(subset[1].domain_id, 30);
        assert_eq!(subset[1].atom_index, 1);
        assert!(
            expected_atom_certificate_subset(
                &initial,
                &["atom:a".to_string(), "atom:a".to_string()]
            )
            .is_none()
        );
        assert!(
            expected_atom_certificate_subset(&initial, &["atom:missing".to_string()]).is_none()
        );
    }

    #[test]
    fn unrestricted_boundary_gate_requires_every_distinct_terminal() {
        let keys = vec![1, 4, 7];
        let terminals = vec!["terminal:a".to_string(), "terminal:b".to_string()];
        let complete = vec![
            boundary_certificate("terminal:a", &keys),
            boundary_certificate("terminal:b", &keys),
        ];
        assert!(boundary_certificates_are_complete(
            &terminals, &complete, &keys
        ));
        assert!(!boundary_certificates_are_complete(
            &["terminal:a".to_string()],
            &complete,
            &keys
        ));
        assert!(!boundary_certificates_are_complete(
            &terminals,
            &[complete[0].clone(), complete[0].clone()],
            &keys,
        ));
        let mut restricted = complete;
        restricted[1].restriction_values = Some(vec![1]);
        assert!(!boundary_certificates_are_complete(
            &terminals,
            &restricted,
            &keys
        ));
    }

    #[test]
    fn committed_fixture_is_complete_and_not_a_placeholder() {
        let fixture = accepted_guarded_core_fixture();
        assert_eq!(fixture.schema_version, 1);
        assert_eq!(fixture.solver_signature, expected_solver_signature());
        assert_eq!(fixture.atom_ids.len(), EXPECTED_ATOM_COUNT);
        assert_eq!(
            fixture.atom_ids.iter().collect::<BTreeSet<_>>().len(),
            EXPECTED_ATOM_COUNT
        );
        assert_eq!(fixture.external_terminal_ids.len(), 10);
        assert!(
            fixture
                .external_terminal_ids
                .windows(2)
                .all(|ids| ids[0] < ids[1])
        );
        assert_eq!(fixture.network_item_codes.len(), 8);
        assert_eq!(fixture.workload_wiring_sha256.len(), 64);
        assert_eq!(fixture.phase_model_semantics_sha256.len(), 64);
        assert_ne!(fixture.workload_wiring_sha256, "0".repeat(64));
        assert_ne!(fixture.phase_model_semantics_sha256, "0".repeat(64));
    }
}
