use std::collections::BTreeSet;
use std::fmt::Write;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::*;

pub const GUARDED_CORE_INITIAL_GATE_SCHEMA_VERSION: u32 = 1;
const EXPECTED_ATOM_COUNT: usize = 30;
const EXPECTED_FORMULATION: &str =
    "joint-shared-v4-unrestricted-sparse-boundary-guarded-core-assumptions";
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
    let (Some(left), Some(right)) = (left.exact.as_ref(), right.exact.as_ref()) else {
        return false;
    };
    left.formulation == EXPECTED_FORMULATION
        && right.formulation == EXPECTED_FORMULATION
        && left.formulation == right.formulation
        && left.model == right.model
        && left.model_complexity == right.model_complexity
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
