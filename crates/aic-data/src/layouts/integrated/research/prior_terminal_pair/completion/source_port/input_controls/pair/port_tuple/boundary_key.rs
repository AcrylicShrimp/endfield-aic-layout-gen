use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;
use crate::layouts::integrated::ModelInput;

mod side_partition;

pub use side_partition::*;

pub const EXTERNAL_BOUNDARY_KEY_LEGAL_SUPPORT_AB_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalBoundaryKeyStaticCertificate {
    pub terminal: String,
    pub network_index: usize,
    pub network_id: String,
    pub bounded_declared_count: usize,
    pub sparse_declared_count: usize,
    pub legal_key_count: usize,
    pub bounded_table_count: usize,
    pub sparse_table_count: usize,
    pub bounded_option_count: usize,
    pub sparse_option_count: usize,
    pub bounded_declared_is_full_expected_range: bool,
    pub bounded_declared_contains_legal: bool,
    pub exact_legal_set_equality: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalBoundaryKeyRootComparison {
    pub terminal: String,
    pub legal_key_count: usize,
    pub bounded_root_observed: bool,
    pub sparse_root_observed: bool,
    pub bounded_root_values: Vec<i32>,
    pub sparse_root_values: Vec<i32>,
    pub bounded_root_absent_from_legal: Vec<i32>,
    pub sparse_root_absent_from_legal: Vec<i32>,
    pub legal_values_pruned_only_by_sparse: Vec<i32>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct ExternalBoundaryKeyRootTotals {
    pub bounded_observed_terminal_count: usize,
    pub sparse_observed_terminal_count: usize,
    pub bounded_root_absent_from_legal: usize,
    pub sparse_root_absent_from_legal: usize,
    pub legal_values_pruned_only_by_sparse: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalBoundaryKeyNetworkContract {
    pub network_index: usize,
    pub network_id: String,
    pub item: String,
    pub transport: String,
    pub terminal_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalBoundaryKeyCommonModelContract {
    pub model_ceiling: [i32; 2],
    pub fixed_dimensions: [i32; 2],
    pub fixed_facility: String,
    pub fixed_coordinate: [i32; 2],
    pub fixed_rotation: i64,
    pub fixed_terminal_assignments: Vec<FacilityPortAssignment>,
    pub facility_instances: Vec<String>,
    pub logical_requirement_ids: Vec<String>,
    pub networks: Vec<ExternalBoundaryKeyNetworkContract>,
    pub single_prepared_input_cloned_for_all_four_builds: bool,
    pub single_fixed_port_vector_cloned_for_all_four_builds: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExternalBoundaryKeySolveReport {
    pub encoding: String,
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
    pub root_snapshot: RootDomainSnapshot,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExternalBoundaryKeyLegalSupportAbReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: ResidualFacilityPortTuplePortfolioReport,
    pub selected_case_index: usize,
    pub selected_assignments: Vec<FacilityPortAssignment>,
    pub execution_order: Vec<String>,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub construction_times_instrumented: bool,
    pub common_model_contract: ExternalBoundaryKeyCommonModelContract,
    pub bounded: ExternalBoundaryKeySolveReport,
    pub sparse: ExternalBoundaryKeySolveReport,
    pub static_certificates: Vec<ExternalBoundaryKeyStaticCertificate>,
    pub root_comparisons: Vec<ExternalBoundaryKeyRootComparison>,
    pub root_totals: ExternalBoundaryKeyRootTotals,
    pub static_equivalence_satisfied: bool,
    pub model_structure_equivalence_satisfied: bool,
    pub root_semantic_identity_observed: bool,
    pub root_semantic_identity_satisfied: bool,
    pub root_observation_coverage_satisfied: bool,
    pub sparse_root_support_satisfied: bool,
    pub combined_outcome: ExactDimensionCaseOutcome,
    pub evidence_conflict: bool,
    pub performance_classification: String,
    pub selected_next_case_index: Option<usize>,
    pub interpretation_blocked: bool,
    pub experiment_ms: u64,
    pub total_wall_ms: u64,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_external_boundary_key_legal_support_ab(
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
) -> Result<ExternalBoundaryKeyLegalSupportAbReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if ab_authoritative_search_budget.is_zero() || ab_observation_search_budget.is_zero() {
        return Err(invalid_input(
            "/external_boundary_key_legal_support_ab_budget",
            "external boundary-key A/B budgets must be positive",
        ));
    }
    let parent = diagnose_residual_facility_port_tuple_portfolio(
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
    )?;
    if parent.interpretation_blocked
        || parent.parent_witness_found
        || parent.parent_infeasibility_proven
    {
        return Err(invalid_input(
            "/parent",
            "boundary-key A/B requires an unresolved, unblocked residual tuple portfolio",
        ));
    }
    let selected_case_index = parent.selected_next_unknown_case_index.ok_or_else(|| {
        invalid_input(
            "/parent/selected_next_unknown_case_index",
            "boundary-key A/B requires a selected unknown tuple",
        )
    })?;
    let selected = parent
        .cases
        .iter()
        .find(|case| case.case_index == selected_case_index)
        .ok_or_else(|| {
            invalid_input(
                "/parent/cases",
                "selected unknown tuple is absent from the parent portfolio",
            )
        })?;
    if selected.combined_outcome != ExactDimensionCaseOutcome::Unknown {
        return Err(invalid_input(
            "/parent/cases",
            "selected boundary-key A/B tuple must remain unknown",
        ));
    }
    let selected_assignments = selected.assignments.clone();

    let parent_assignments = parent
        .parent
        .inherited_assignments
        .iter()
        .chain(&parent.parent.assignments)
        .cloned()
        .collect::<Vec<_>>();
    let requested = parent_assignments
        .iter()
        .chain(&selected_assignments)
        .cloned()
        .collect::<Vec<_>>();
    assert_distinct_assignments(
        &requested,
        EXPECTED_TOTAL_FIXED_TERMINALS,
        "/selected_case/assignments",
    )?;
    let fixed_ports = exact_ports(&parent_assignments, &selected_assignments);
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
        width: parent.parent.fixed_dimensions[0],
        height: parent.parent.fixed_dimensions[1],
    };
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: parent.parent.partitioned_facility.clone(),
        x: parent.parent.fixed_coordinate[0],
        y: parent.parent.fixed_coordinate[1],
        rotation: Some(parent.parent.fixed_rotation),
    };
    let prior_reference = &parent.parent.prior_reference;
    let fixation = exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements;
    let common_model_contract = common_model_contract(&input, dimensions, &coordinate, &requested);
    let experiment_started = Instant::now();
    let execution_order = vec![
        "bounded-authoritative".to_string(),
        "sparse-authoritative".to_string(),
        "bounded-observation".to_string(),
        "sparse-observation".to_string(),
    ];

    let (bounded_authoritative, bounded_authoritative_certificates) =
        exact::shared_layer::solve_sparse_support_endpoints_boundary_key_audit_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
            input.clone(),
            logistics_components,
            Some(ab_authoritative_search_budget),
            dimensions,
            coordinate.clone(),
            fixed_ports.clone(),
            prior_reference,
            fixation,
            false,
        );
    let (sparse_authoritative, sparse_authoritative_certificates) =
        exact::shared_layer::solve_sparse_support_endpoints_boundary_key_audit_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
            input.clone(),
            logistics_components,
            Some(ab_authoritative_search_budget),
            dimensions,
            coordinate.clone(),
            fixed_ports.clone(),
            prior_reference,
            fixation,
            true,
        );
    let (
        bounded_observation,
        bounded_snapshot,
        bounded_observation_certificates,
        _bounded_build_certificate,
    ) =
        exact::shared_layer::solve_sparse_support_endpoints_boundary_key_audit_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
            input.clone(),
            logistics_components,
            Some(ab_observation_search_budget),
            dimensions,
            coordinate.clone(),
            fixed_ports.clone(),
            prior_reference,
            fixation,
            false,
        );
    let (
        sparse_observation,
        sparse_snapshot,
        sparse_observation_certificates,
        _sparse_build_certificate,
    ) =
        exact::shared_layer::solve_sparse_support_endpoints_boundary_key_audit_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
            input,
            logistics_components,
            Some(ab_observation_search_budget),
            dimensions,
            coordinate,
            fixed_ports,
            prior_reference,
            fixation,
            true,
        );

    let bounded_snapshot = bounded_snapshot.ok_or_else(|| {
        invalid_input(
            "/bounded/root_snapshot",
            "bounded observation did not return a root snapshot",
        )
    })?;
    let sparse_snapshot = sparse_snapshot.ok_or_else(|| {
        invalid_input(
            "/sparse/root_snapshot",
            "sparse observation did not return a root snapshot",
        )
    })?;
    let bounded_fixation = assess_fixation(&bounded_snapshot, &requested);
    let sparse_fixation = assess_fixation(&sparse_snapshot, &requested);
    let bounded = solve_report(
        "bounded-positive-table",
        bounded_authoritative,
        bounded_observation,
        bounded_snapshot,
        bounded_fixation,
    );
    let sparse = solve_report(
        "sparse-legal-support",
        sparse_authoritative,
        sparse_observation,
        sparse_snapshot,
        sparse_fixation,
    );

    let authoritative_certificate_match = bounded_authoritative_certificates
        == bounded_observation_certificates
        && sparse_authoritative_certificates == sparse_observation_certificates;
    let (static_certificates, static_certificate_comparison_satisfied) =
        compare_static_certificates(
            &bounded_authoritative_certificates,
            &sparse_authoritative_certificates,
            0,
            common_model_contract.model_ceiling[0]
                .checked_mul(common_model_contract.model_ceiling[1])
                .and_then(|cell_count| cell_count.checked_mul(4))
                .and_then(|value| value.checked_sub(1))
                .expect("validated model ceiling has a bounded boundary-key range"),
        );
    let static_equivalence_satisfied = authoritative_certificate_match
        && static_certificate_comparison_satisfied
        && bounded_authoritative_certificates.len() == sparse_authoritative_certificates.len()
        && static_certificates.len()
            == bounded
                .authoritative_layout
                .exact
                .as_ref()
                .expect("executed exact solve has metrics")
                .model
                .external_terminal_count
        && static_certificates
            .iter()
            .all(|certificate| certificate.exact_legal_set_equality);
    let root_comparisons = compare_root_values(
        &bounded.root_snapshot,
        &sparse.root_snapshot,
        &bounded_authoritative_certificates,
    );
    let root_totals = root_totals(&root_comparisons);
    let bounded_root_infeasible = bounded.root_snapshot.capture_status == "root-infeasible";
    let sparse_root_infeasible = sparse.root_snapshot.capture_status == "root-infeasible";
    let (root_observation_coverage_satisfied, sparse_root_support_satisfied) =
        root_observation_checks(
            &root_comparisons,
            static_certificates.len(),
            bounded_root_infeasible,
            sparse_root_infeasible,
        );
    let root_semantic_identity_observed = bounded.root_snapshot.capture_status != "root-infeasible"
        && sparse.root_snapshot.capture_status != "root-infeasible";
    let root_semantic_identity_satisfied = !root_semantic_identity_observed
        || root_semantic_identity_equal(&bounded.root_snapshot, &sparse.root_snapshot);
    let model_structure_equivalence_satisfied =
        model_structure_equal(&bounded.authoritative_layout, &sparse.authoritative_layout)
            && model_structure_equal(&bounded.observation_layout, &sparse.observation_layout);
    let (combined_outcome, evidence_conflict) =
        combine_outcomes(bounded.combined_outcome, sparse.combined_outcome);
    let performance_classification =
        performance_classification(bounded.authoritative_outcome, sparse.authoritative_outcome);
    let interpretation_blocked = evidence_conflict
        || bounded.combined_outcome == ExactDimensionCaseOutcome::InvalidWitness
        || sparse.combined_outcome == ExactDimensionCaseOutcome::InvalidWitness
        || !static_equivalence_satisfied
        || !model_structure_equivalence_satisfied
        || !root_semantic_identity_satisfied
        || !root_observation_coverage_satisfied
        || !sparse_root_support_satisfied
        || (bounded.fixation_observation.assertion_applies
            && !bounded.fixation_observation.assertion_satisfied)
        || (sparse.fixation_observation.assertion_applies
            && !sparse.fixation_observation.assertion_satisfied);
    let selected_next_case_index = (!interpretation_blocked
        && combined_outcome == ExactDimensionCaseOutcome::ProvenInfeasible)
        .then(|| {
            parent
                .cases
                .iter()
                .filter(|case| {
                    case.case_index != selected_case_index
                        && case.combined_outcome == ExactDimensionCaseOutcome::Unknown
                })
                .map(|case| case.case_index)
                .min()
        })
        .flatten();

    Ok(ExternalBoundaryKeyLegalSupportAbReport {
        schema_version: EXTERNAL_BOUNDARY_KEY_LEGAL_SUPPORT_AB_SCHEMA_VERSION,
        target_phase_index,
        parent,
        selected_case_index,
        selected_assignments,
        execution_order,
        authoritative_case_search_budget_ms: millis(ab_authoritative_search_budget),
        observation_case_search_budget_ms: millis(ab_observation_search_budget),
        construction_times_instrumented: true,
        common_model_contract,
        bounded,
        sparse,
        static_certificates,
        root_comparisons,
        root_totals,
        static_equivalence_satisfied,
        model_structure_equivalence_satisfied,
        root_semantic_identity_observed,
        root_semantic_identity_satisfied,
        root_observation_coverage_satisfied,
        sparse_root_support_satisfied,
        combined_outcome,
        evidence_conflict,
        performance_classification,
        selected_next_case_index,
        interpretation_blocked,
        experiment_ms: millis(experiment_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        diagnostic_only: true,
    })
}

fn solve_report(
    encoding: &str,
    authoritative_layout: IntegratedLayoutReport,
    observation_layout: IntegratedLayoutReport,
    root_snapshot: RootDomainSnapshot,
    fixation_observation: ResidualFacilityPortFixationObservation,
) -> ExternalBoundaryKeySolveReport {
    let authoritative_outcome = classify_outcome(&authoritative_layout);
    let observation_outcome = classify_outcome(&observation_layout);
    let (combined_outcome, evidence_conflict) =
        combine_outcomes(authoritative_outcome, observation_outcome);
    let exact = authoritative_layout
        .exact
        .as_ref()
        .expect("executed boundary-key A/B solve has exact metrics");
    ExternalBoundaryKeySolveReport {
        encoding: encoding.to_string(),
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
        root_snapshot,
    }
}

fn common_model_contract(
    input: &ModelInput,
    dimensions: exact::shared_layer::FixedUsedDimensions,
    coordinate: &exact::shared_layer::FixedFacilityCoordinate,
    requested: &[FacilityPortAssignment],
) -> ExternalBoundaryKeyCommonModelContract {
    let mut fixed_terminal_assignments = requested.to_vec();
    fixed_terminal_assignments
        .sort_by(|left, right| (&left.terminal, &left.port).cmp(&(&right.terminal, &right.port)));
    ExternalBoundaryKeyCommonModelContract {
        model_ceiling: [input.width, input.height],
        fixed_dimensions: [dimensions.width, dimensions.height],
        fixed_facility: coordinate.instance.clone(),
        fixed_coordinate: [coordinate.x, coordinate.y],
        fixed_rotation: coordinate
            .rotation
            .expect("boundary-key A/B fixes the selected facility rotation"),
        fixed_terminal_assignments,
        facility_instances: input
            .instances
            .iter()
            .map(|instance| instance.id.clone())
            .collect(),
        logical_requirement_ids: input
            .edges
            .iter()
            .map(|edge| edge.requirement_id.clone())
            .collect(),
        networks: input
            .networks
            .iter()
            .enumerate()
            .map(
                |(network_index, network)| ExternalBoundaryKeyNetworkContract {
                    network_index,
                    network_id: network.id().to_string(),
                    item: network.item().to_string(),
                    transport: format!("{:?}", network.transport()),
                    terminal_ids: network
                        .terminals()
                        .iter()
                        .map(|terminal| terminal.id().to_string())
                        .collect(),
                },
            )
            .collect(),
        single_prepared_input_cloned_for_all_four_builds: true,
        single_fixed_port_vector_cloned_for_all_four_builds: true,
    }
}

fn root_semantic_identity_equal(left: &RootDomainSnapshot, right: &RootDomainSnapshot) -> bool {
    let terminal_signatures = |snapshot: &RootDomainSnapshot| {
        let mut signatures = snapshot
            .terminals
            .iter()
            .map(|terminal| {
                format!(
                    "{}|{}|{}|{:?}|{:?}|{}|{:?}|{:?}",
                    terminal.terminal,
                    terminal.network_index,
                    terminal.network_id,
                    terminal.transport,
                    terminal.direction,
                    terminal.endpoint_kind,
                    terminal.facility_instance,
                    terminal.external_node,
                )
            })
            .collect::<Vec<_>>();
        signatures.sort();
        signatures
    };
    let network_signatures = |snapshot: &RootDomainSnapshot| {
        let mut signatures = snapshot
            .networks
            .iter()
            .map(|network| {
                format!(
                    "{}|{}|{:?}|{}",
                    network.network_index, network.network_id, network.transport, network.item,
                )
            })
            .collect::<Vec<_>>();
        signatures.sort();
        signatures
    };
    terminal_signatures(left) == terminal_signatures(right)
        && network_signatures(left) == network_signatures(right)
}

fn compare_static_certificates(
    bounded: &[exact::shared_layer::BoundaryKeyBuildCertificate],
    sparse: &[exact::shared_layer::BoundaryKeyBuildCertificate],
    expected_bounded_lower_bound: i32,
    expected_bounded_upper_bound: i32,
) -> (Vec<ExternalBoundaryKeyStaticCertificate>, bool) {
    let Some(bounded_by_terminal) = unique_certificates_by_terminal(bounded) else {
        return (Vec::new(), false);
    };
    let Some(sparse_by_terminal) = unique_certificates_by_terminal(sparse) else {
        return (Vec::new(), false);
    };
    if bounded_by_terminal.keys().collect::<BTreeSet<_>>()
        != sparse_by_terminal.keys().collect::<BTreeSet<_>>()
    {
        return (Vec::new(), false);
    }
    let rows = bounded_by_terminal
        .iter()
        .filter_map(|(terminal, bounded)| {
            let sparse = sparse_by_terminal.get(terminal)?;
            let bounded_expected_values =
                (expected_bounded_lower_bound..=expected_bounded_upper_bound).collect::<Vec<_>>();
            let bounded_declared = bounded
                .declared_values
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            let legal = bounded
                .unary_table_projection
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            let bounded_declared_contains_legal = legal.is_subset(&bounded_declared);
            let sparse_bounds_match = sparse.declared_values.first().copied()
                == Some(sparse.declared_lower_bound)
                && sparse.declared_values.last().copied() == Some(sparse.declared_upper_bound);
            let exact_legal_set_equality = bounded.network_index == sparse.network_index
                && bounded.network_id == sparse.network_id
                && sparse.declared_domain_kind == "sparse-legal"
                && bounded.declared_domain_kind == "bounded"
                && bounded.declared_lower_bound == expected_bounded_lower_bound
                && bounded.declared_upper_bound == expected_bounded_upper_bound
                && bounded.declared_values == bounded_expected_values
                && bounded_declared_contains_legal
                && sparse_bounds_match
                && sparse.declared_values == bounded.unary_table_projection
                && bounded.unary_table_projection == sparse.unary_table_projection
                && bounded.unary_table_projection == bounded.routing_option_keys
                && sparse.unary_table_projection == sparse.routing_option_keys
                && bounded.restriction_values.is_none()
                && sparse.restriction_values.is_none();
            Some(ExternalBoundaryKeyStaticCertificate {
                terminal: terminal.clone(),
                network_index: bounded.network_index,
                network_id: bounded.network_id.clone(),
                bounded_declared_count: bounded.declared_values.len(),
                sparse_declared_count: sparse.declared_values.len(),
                legal_key_count: bounded.unary_table_projection.len(),
                bounded_table_count: bounded.unary_table_projection.len(),
                sparse_table_count: sparse.unary_table_projection.len(),
                bounded_option_count: bounded.routing_option_keys.len(),
                sparse_option_count: sparse.routing_option_keys.len(),
                bounded_declared_is_full_expected_range: bounded.declared_lower_bound
                    == expected_bounded_lower_bound
                    && bounded.declared_upper_bound == expected_bounded_upper_bound
                    && bounded.declared_values == bounded_expected_values,
                bounded_declared_contains_legal,
                exact_legal_set_equality,
            })
        })
        .collect::<Vec<_>>();
    let satisfied = rows.len() == bounded.len()
        && rows.len() == sparse.len()
        && rows
            .iter()
            .all(|certificate| certificate.exact_legal_set_equality);
    (rows, satisfied)
}

fn unique_certificates_by_terminal(
    certificates: &[exact::shared_layer::BoundaryKeyBuildCertificate],
) -> Option<BTreeMap<String, &exact::shared_layer::BoundaryKeyBuildCertificate>> {
    let mut by_terminal = BTreeMap::new();
    for certificate in certificates {
        if by_terminal
            .insert(certificate.terminal.clone(), certificate)
            .is_some()
        {
            return None;
        }
    }
    Some(by_terminal)
}

fn compare_root_values(
    bounded: &RootDomainSnapshot,
    sparse: &RootDomainSnapshot,
    legal_certificates: &[exact::shared_layer::BoundaryKeyBuildCertificate],
) -> Vec<ExternalBoundaryKeyRootComparison> {
    let terminal_values = |snapshot: &RootDomainSnapshot| {
        snapshot
            .terminals
            .iter()
            .filter(|terminal| terminal.endpoint_kind == "external")
            .map(|terminal| {
                (
                    terminal.terminal.clone(),
                    terminal
                        .root_geometry_values
                        .iter()
                        .copied()
                        .collect::<BTreeSet<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>()
    };
    let bounded_observed = bounded.capture_status != "root-infeasible";
    let sparse_observed = sparse.capture_status != "root-infeasible";
    let bounded_terminals = bounded_observed.then(|| terminal_values(bounded));
    let sparse_terminals = sparse_observed.then(|| terminal_values(sparse));
    let Some(legal) = unique_certificates_by_terminal(legal_certificates) else {
        return Vec::new();
    };
    legal
        .into_iter()
        .map(|(terminal, certificate)| {
            let legal = certificate
                .unary_table_projection
                .iter()
                .copied()
                .collect::<BTreeSet<_>>();
            let bounded_root = bounded_terminals
                .as_ref()
                .and_then(|terminals| terminals.get(&terminal));
            let sparse_root = sparse_terminals
                .as_ref()
                .and_then(|terminals| terminals.get(&terminal));
            compare_root_sets(terminal, &legal, bounded_root, sparse_root)
        })
        .collect()
}

fn compare_root_sets(
    terminal: String,
    legal: &BTreeSet<i32>,
    bounded_root: Option<&BTreeSet<i32>>,
    sparse_root: Option<&BTreeSet<i32>>,
) -> ExternalBoundaryKeyRootComparison {
    let empty = BTreeSet::new();
    let bounded_values = bounded_root.unwrap_or(&empty);
    let sparse_values = sparse_root.unwrap_or(&empty);
    let legal_values_pruned_only_by_sparse = match (bounded_root, sparse_root) {
        (Some(bounded_root), Some(sparse_root)) => bounded_root
            .intersection(legal)
            .copied()
            .collect::<BTreeSet<_>>()
            .difference(sparse_root)
            .copied()
            .collect(),
        _ => Vec::new(),
    };
    ExternalBoundaryKeyRootComparison {
        terminal,
        legal_key_count: legal.len(),
        bounded_root_observed: bounded_root.is_some(),
        sparse_root_observed: sparse_root.is_some(),
        bounded_root_values: bounded_values.iter().copied().collect(),
        sparse_root_values: sparse_values.iter().copied().collect(),
        bounded_root_absent_from_legal: bounded_values.difference(legal).copied().collect(),
        sparse_root_absent_from_legal: sparse_values.difference(legal).copied().collect(),
        legal_values_pruned_only_by_sparse,
    }
}

fn root_totals(comparisons: &[ExternalBoundaryKeyRootComparison]) -> ExternalBoundaryKeyRootTotals {
    ExternalBoundaryKeyRootTotals {
        bounded_observed_terminal_count: comparisons
            .iter()
            .filter(|comparison| comparison.bounded_root_observed)
            .count(),
        sparse_observed_terminal_count: comparisons
            .iter()
            .filter(|comparison| comparison.sparse_root_observed)
            .count(),
        bounded_root_absent_from_legal: comparisons
            .iter()
            .map(|comparison| comparison.bounded_root_absent_from_legal.len())
            .sum(),
        sparse_root_absent_from_legal: comparisons
            .iter()
            .map(|comparison| comparison.sparse_root_absent_from_legal.len())
            .sum(),
        legal_values_pruned_only_by_sparse: comparisons
            .iter()
            .map(|comparison| comparison.legal_values_pruned_only_by_sparse.len())
            .sum(),
    }
}

fn root_observation_checks(
    comparisons: &[ExternalBoundaryKeyRootComparison],
    expected_terminal_count: usize,
    bounded_root_infeasible: bool,
    sparse_root_infeasible: bool,
) -> (bool, bool) {
    let coverage = comparisons.len() == expected_terminal_count
        && comparisons.iter().all(|comparison| {
            (bounded_root_infeasible || comparison.bounded_root_observed)
                && (sparse_root_infeasible || comparison.sparse_root_observed)
        });
    let sparse_support = sparse_root_infeasible
        || (coverage
            && comparisons.iter().all(|comparison| {
                comparison.sparse_root_observed
                    && comparison.sparse_root_absent_from_legal.is_empty()
            }));
    (coverage, sparse_support)
}

fn model_structure_equal(left: &IntegratedLayoutReport, right: &IntegratedLayoutReport) -> bool {
    let (Some(left), Some(right)) = (&left.exact, &right.exact) else {
        return false;
    };
    left.model == right.model
        && left.model_complexity.constraints == right.model_complexity.constraints
        && left.model_complexity.factor_graph == right.model_complexity.factor_graph
        && left.model_complexity.coupling == right.model_complexity.coupling
        && variable_family_shapes(&left.model_complexity.variables.by_family)
            == variable_family_shapes(&right.model_complexity.variables.by_family)
}

fn variable_family_shapes(
    families: &[crate::research::VariableFamilyMetrics],
) -> Vec<(String, u64, u64, u64)> {
    families
        .iter()
        .map(|family| {
            (
                family.family.clone(),
                family.total_variables,
                family.boolean_variables,
                family.integer_variables,
            )
        })
        .collect()
}

fn performance_classification(
    bounded: ExactDimensionCaseOutcome,
    sparse: ExactDimensionCaseOutcome,
) -> String {
    let resolved = |outcome| {
        matches!(
            outcome,
            ExactDimensionCaseOutcome::ValidatedFeasible
                | ExactDimensionCaseOutcome::ProvenInfeasible
        )
    };
    if bounded == ExactDimensionCaseOutcome::InvalidWitness
        || sparse == ExactDimensionCaseOutcome::InvalidWitness
    {
        "blocked-invalid".to_string()
    } else if !resolved(bounded) && resolved(sparse) {
        "sparse-crossed-cutoff".to_string()
    } else if resolved(bounded) && !resolved(sparse) {
        "sparse-regressed-at-cutoff".to_string()
    } else if resolved(bounded) && resolved(sparse) {
        "both-resolved".to_string()
    } else {
        "both-unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn certificate(
        terminal: &str,
        network_index: usize,
        kind: &str,
        declared_values: Vec<i32>,
        legal_values: Vec<i32>,
        option_values: Vec<i32>,
    ) -> exact::shared_layer::BoundaryKeyBuildCertificate {
        exact::shared_layer::BoundaryKeyBuildCertificate {
            terminal: terminal.to_string(),
            network_index,
            network_id: format!("network-{network_index}"),
            declared_domain_kind: kind.to_string(),
            declared_lower_bound: *declared_values.first().expect("non-empty test domain"),
            declared_upper_bound: *declared_values.last().expect("non-empty test domain"),
            declared_values,
            unary_table_projection: legal_values,
            routing_option_keys: option_values,
            restriction_values: None,
        }
    }

    fn valid_certificate_pair() -> (
        exact::shared_layer::BoundaryKeyBuildCertificate,
        exact::shared_layer::BoundaryKeyBuildCertificate,
    ) {
        (
            certificate(
                "terminal-a",
                0,
                "bounded",
                (0..=5).collect(),
                vec![1, 3, 5],
                vec![1, 3, 5],
            ),
            certificate(
                "terminal-a",
                0,
                "sparse-legal",
                vec![1, 3, 5],
                vec![1, 3, 5],
                vec![1, 3, 5],
            ),
        )
    }

    #[test]
    fn performance_classification_is_symmetric() {
        assert_eq!(
            performance_classification(
                ExactDimensionCaseOutcome::Unknown,
                ExactDimensionCaseOutcome::ProvenInfeasible,
            ),
            "sparse-crossed-cutoff"
        );
        assert_eq!(
            performance_classification(
                ExactDimensionCaseOutcome::ProvenInfeasible,
                ExactDimensionCaseOutcome::Unknown,
            ),
            "sparse-regressed-at-cutoff"
        );
    }

    #[test]
    fn static_certificate_requires_exact_terminal_bijection() {
        let (bounded, sparse) = valid_certificate_pair();
        assert!(compare_static_certificates(&[bounded.clone()], &[sparse.clone()], 0, 5).1);
        assert!(
            !compare_static_certificates(&[bounded.clone(), bounded], &[sparse.clone()], 0, 5).1
        );
        let mut extra = sparse.clone();
        extra.terminal = "terminal-extra".to_string();
        assert!(
            !compare_static_certificates(&[valid_certificate_pair().0], &[sparse, extra], 0, 5).1
        );
    }

    #[test]
    fn static_certificate_rejects_wrong_bounded_declaration() {
        let (mut bounded, sparse) = valid_certificate_pair();
        bounded.declared_values.remove(2);
        assert!(!compare_static_certificates(&[bounded], &[sparse], 0, 5).1);

        let (bounded, sparse) = valid_certificate_pair();
        assert!(!compare_static_certificates(&[bounded], &[sparse], 0, 6).1);
    }

    #[test]
    fn static_certificate_rejects_missing_or_mismatched_legal_support() {
        let (bounded, mut sparse) = valid_certificate_pair();
        sparse.declared_values.remove(1);
        assert!(!compare_static_certificates(&[bounded.clone()], &[sparse], 0, 5).1);

        let (_, mut sparse) = valid_certificate_pair();
        sparse.routing_option_keys.pop();
        assert!(!compare_static_certificates(&[bounded], &[sparse], 0, 5).1);
    }

    #[test]
    fn root_comparison_preserves_sparse_observation_when_bounded_is_unavailable() {
        let legal = [1, 3, 5].into_iter().collect::<BTreeSet<_>>();
        let sparse = [1, 5].into_iter().collect::<BTreeSet<_>>();
        let comparison = compare_root_sets("terminal-a".to_string(), &legal, None, Some(&sparse));
        assert!(!comparison.bounded_root_observed);
        assert!(comparison.sparse_root_observed);
        assert_eq!(comparison.sparse_root_values, vec![1, 5]);
        assert!(comparison.sparse_root_absent_from_legal.is_empty());
    }

    #[test]
    fn static_certificate_rejects_network_or_sparse_bound_mismatch() {
        let (bounded, mut sparse) = valid_certificate_pair();
        sparse.network_id = "different-network".to_string();
        assert!(!compare_static_certificates(&[bounded.clone()], &[sparse], 0, 5).1);

        let (_, mut sparse) = valid_certificate_pair();
        sparse.declared_upper_bound = 4;
        assert!(!compare_static_certificates(&[bounded], &[sparse], 0, 5).1);
    }

    #[test]
    fn root_observation_checks_each_available_side() {
        let legal = [1, 3, 5].into_iter().collect::<BTreeSet<_>>();
        let sparse = [1, 5].into_iter().collect::<BTreeSet<_>>();
        let sparse_only = compare_root_sets("terminal-a".to_string(), &legal, None, Some(&sparse));
        assert_eq!(
            root_observation_checks(&[sparse_only], 1, true, false),
            (true, true)
        );

        let missing_sparse = compare_root_sets("terminal-a".to_string(), &legal, None, None);
        assert_eq!(
            root_observation_checks(&[missing_sparse], 1, true, false),
            (false, false)
        );
        assert_eq!(root_observation_checks(&[], 1, true, true), (false, true));
    }

    #[test]
    fn performance_classification_covers_all_resolution_shapes() {
        use ExactDimensionCaseOutcome::{
            InvalidWitness, ProvenInfeasible, Unknown, ValidatedFeasible,
        };

        assert_eq!(performance_classification(Unknown, Unknown), "both-unknown");
        assert_eq!(
            performance_classification(ValidatedFeasible, ProvenInfeasible),
            "both-resolved"
        );
        assert_eq!(
            performance_classification(InvalidWitness, Unknown),
            "blocked-invalid"
        );
    }
}
