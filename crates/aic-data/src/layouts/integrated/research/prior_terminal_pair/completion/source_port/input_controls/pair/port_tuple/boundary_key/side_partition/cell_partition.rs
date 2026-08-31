use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;

pub const EXTERNAL_BOUNDARY_CELL_PARTITION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExternalBoundaryCellCaseReport {
    pub case_index: usize,
    pub key: i32,
    pub solve: ExternalBoundaryKeySolveReport,
    pub authoritative_certificate_satisfied: bool,
    pub observation_certificate_satisfied: bool,
    pub certificate_copies_equal: bool,
    pub root_restriction_observed: bool,
    pub root_restriction_satisfied: bool,
    pub facility_fixation_satisfied: bool,
    pub interpretation_blocked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExternalBoundaryCellPartitionReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: ExternalBoundarySidePartitionReport,
    pub selected_side_case_index: usize,
    pub selected_side: String,
    pub selected_terminal: String,
    pub parent_side_keys: Vec<i32>,
    pub execution_order: Vec<i32>,
    pub partition_non_empty: bool,
    pub partition_pairwise_disjoint: bool,
    pub partition_exact_cover: bool,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub construction_times_instrumented: bool,
    pub cases: Vec<ExternalBoundaryCellCaseReport>,
    pub common_static_certificates_satisfied: bool,
    pub controlled_model_contract_satisfied: bool,
    pub validated_feasible_count: usize,
    pub proven_infeasible_count: usize,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub selected_side_witness_found: bool,
    pub selected_side_infeasibility_proven: bool,
    pub unresolved_keys: Vec<i32>,
    pub selected_side_outcome: ExactDimensionCaseOutcome,
    pub interpretation_blocked: bool,
    pub experiment_ms: u64,
    pub total_wall_ms: u64,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_external_boundary_cell_partition(
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
) -> Result<ExternalBoundaryCellPartitionReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if cell_authoritative_search_budget.is_zero() || cell_observation_search_budget.is_zero() {
        return Err(invalid_input(
            "/external_boundary_cell_partition_budget",
            "external boundary-cell budgets must be positive",
        ));
    }
    let parent = diagnose_external_boundary_side_partition(
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
    )?;
    if parent.interpretation_blocked
        || parent.combined_outcome != ExactDimensionCaseOutcome::Unknown
    {
        return Err(invalid_input(
            "/parent",
            "boundary-cell partition requires an unblocked unresolved side parent",
        ));
    }
    let selected = parent
        .cases
        .iter()
        .filter(|case| case.solve.combined_outcome == ExactDimensionCaseOutcome::Unknown)
        .min_by_key(|case| case.case_index)
        .ok_or_else(|| {
            invalid_input(
                "/parent/cases",
                "boundary-cell partition requires one unresolved side",
            )
        })?;
    if selected.allowed_keys.len() < 2 {
        return Err(invalid_input(
            "/parent/cases/allowed_keys",
            "boundary-cell partition requires at least two keys",
        ));
    }
    let selected_side_case_index = selected.case_index;
    let selected_side = selected.side.clone();
    let mut parent_side_keys = selected.allowed_keys.clone();
    parent_side_keys.sort_unstable();
    parent_side_keys.dedup();
    let execution_order = parent_side_keys.clone();
    let partition_non_empty = !parent_side_keys.is_empty();
    let partition_pairwise_disjoint = parent_side_keys.len() == selected.allowed_keys.len();
    let partition_exact_cover = parent_side_keys == selected.allowed_keys;
    if !partition_non_empty || !partition_pairwise_disjoint || !partition_exact_cover {
        return Err(invalid_input(
            "/partition",
            "boundary-cell singleton domains must exactly cover the selected side",
        ));
    }

    let boundary_parent = &parent.parent;
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
        "/selected_case/assignments",
    )?;
    let fixed_ports = exact_ports(&parent_assignments, &boundary_parent.selected_assignments);
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
    let fixation = exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements;
    let selected_terminal = parent.selected_terminal.clone();
    let experiment_started = Instant::now();
    let mut cases = Vec::with_capacity(parent_side_keys.len());
    let mut static_certificate_shapes = Vec::with_capacity(parent_side_keys.len() * 2);
    for (case_index, key) in parent_side_keys.iter().copied().enumerate() {
        let allowed_keys = vec![key];
        let (authoritative_layout, authoritative_certificates) =
            exact::shared_layer::solve_sparse_support_endpoints_boundary_key_restricted_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                input.clone(),
                logistics_components,
                Some(cell_authoritative_search_budget),
                dimensions,
                coordinate.clone(),
                fixed_ports.clone(),
                prior_reference,
                fixation,
                selected_terminal.clone(),
                allowed_keys.clone(),
            );
        let (observation_layout, root_snapshot, observation_certificates) =
            exact::shared_layer::solve_sparse_support_endpoints_boundary_key_restricted_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
                input.clone(),
                logistics_components,
                Some(cell_observation_search_budget),
                dimensions,
                coordinate.clone(),
                fixed_ports.clone(),
                prior_reference,
                fixation,
                selected_terminal.clone(),
                allowed_keys.clone(),
            );
        let root_snapshot = root_snapshot.ok_or_else(|| {
            invalid_input(
                "/cases/root_snapshot",
                format!("boundary key {key} observation did not return a root snapshot"),
            )
        })?;
        let fixation_observation = assess_fixation(&root_snapshot, &requested);
        let solve = solve_report(
            &format!("sparse-boundary-cell-{key}"),
            authoritative_layout,
            observation_layout,
            root_snapshot,
            fixation_observation,
        );
        let authoritative_certificate_satisfied = restriction_certificates_satisfied(
            &authoritative_certificates,
            &boundary_parent.static_certificates,
            &selected_terminal,
            &allowed_keys,
        );
        let observation_certificate_satisfied = restriction_certificates_satisfied(
            &observation_certificates,
            &boundary_parent.static_certificates,
            &selected_terminal,
            &allowed_keys,
        );
        let certificate_copies_equal = authoritative_certificates == observation_certificates;
        static_certificate_shapes.push(normalized_certificates(
            &authoritative_certificates,
            &selected_terminal,
        ));
        static_certificate_shapes.push(normalized_certificates(
            &observation_certificates,
            &selected_terminal,
        ));
        let (root_restriction_observed, root_restriction_satisfied) =
            singleton_root_restriction_status(&solve.root_snapshot, &selected_terminal, key);
        let facility_fixation_satisfied =
            root_facility_fixation_satisfied(&solve.root_snapshot, input.instances.len());
        let interpretation_blocked = solve.evidence_conflict
            || solve.combined_outcome == ExactDimensionCaseOutcome::InvalidWitness
            || !authoritative_certificate_satisfied
            || !observation_certificate_satisfied
            || !certificate_copies_equal
            || !root_restriction_satisfied
            || !facility_fixation_satisfied
            || (solve.fixation_observation.assertion_applies
                && !solve.fixation_observation.assertion_satisfied);
        cases.push(ExternalBoundaryCellCaseReport {
            case_index,
            key,
            solve,
            authoritative_certificate_satisfied,
            observation_certificate_satisfied,
            certificate_copies_equal,
            root_restriction_observed,
            root_restriction_satisfied,
            facility_fixation_satisfied,
            interpretation_blocked,
        });
    }

    let common_static_certificates_satisfied = static_certificate_shapes
        .first()
        .is_some_and(|first| static_certificate_shapes.iter().all(|shape| shape == first));
    let controlled_model_contract_satisfied = cases.first().is_some_and(|first| {
        cases.iter().all(|case| {
            side_model_contract_equal(
                &first.solve.authoritative_layout,
                1,
                &case.solve.authoritative_layout,
                1,
            ) && side_model_contract_equal(
                &first.solve.observation_layout,
                1,
                &case.solve.observation_layout,
                1,
            )
        })
    });
    let validated_feasible_count =
        count_cell_outcomes(&cases, ExactDimensionCaseOutcome::ValidatedFeasible);
    let proven_infeasible_count =
        count_cell_outcomes(&cases, ExactDimensionCaseOutcome::ProvenInfeasible);
    let unknown_count = count_cell_outcomes(&cases, ExactDimensionCaseOutcome::Unknown);
    let invalid_witness_count =
        count_cell_outcomes(&cases, ExactDimensionCaseOutcome::InvalidWitness);
    let selected_side_witness_found = validated_feasible_count > 0;
    let selected_side_infeasibility_proven = proven_infeasible_count == cases.len();
    let interpretation_blocked = !common_static_certificates_satisfied
        || !controlled_model_contract_satisfied
        || cases.iter().any(|case| case.interpretation_blocked)
        || invalid_witness_count > 0;
    let selected_side_outcome = if interpretation_blocked {
        ExactDimensionCaseOutcome::InvalidWitness
    } else if selected_side_witness_found {
        ExactDimensionCaseOutcome::ValidatedFeasible
    } else if selected_side_infeasibility_proven {
        ExactDimensionCaseOutcome::ProvenInfeasible
    } else {
        ExactDimensionCaseOutcome::Unknown
    };
    let unresolved_keys = cases
        .iter()
        .filter(|case| case.solve.combined_outcome == ExactDimensionCaseOutcome::Unknown)
        .map(|case| case.key)
        .collect();

    Ok(ExternalBoundaryCellPartitionReport {
        schema_version: EXTERNAL_BOUNDARY_CELL_PARTITION_SCHEMA_VERSION,
        target_phase_index,
        parent,
        selected_side_case_index,
        selected_side,
        selected_terminal,
        parent_side_keys,
        execution_order,
        partition_non_empty,
        partition_pairwise_disjoint,
        partition_exact_cover,
        authoritative_case_search_budget_ms: millis(cell_authoritative_search_budget),
        observation_case_search_budget_ms: millis(cell_observation_search_budget),
        construction_times_instrumented: true,
        cases,
        common_static_certificates_satisfied,
        controlled_model_contract_satisfied,
        validated_feasible_count,
        proven_infeasible_count,
        unknown_count,
        invalid_witness_count,
        selected_side_witness_found,
        selected_side_infeasibility_proven,
        unresolved_keys,
        selected_side_outcome,
        interpretation_blocked,
        experiment_ms: millis(experiment_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        diagnostic_only: true,
    })
}

fn count_cell_outcomes(
    cases: &[ExternalBoundaryCellCaseReport],
    outcome: ExactDimensionCaseOutcome,
) -> usize {
    cases
        .iter()
        .filter(|case| case.solve.combined_outcome == outcome)
        .count()
}

fn singleton_root_restriction_status(
    snapshot: &RootDomainSnapshot,
    selected_terminal: &str,
    expected_key: i32,
) -> (bool, bool) {
    if snapshot.capture_status == "root-infeasible" {
        return (false, true);
    }
    let Some(terminal) = snapshot
        .terminals
        .iter()
        .find(|terminal| terminal.terminal == selected_terminal)
    else {
        return (false, false);
    };
    (
        true,
        singleton_root_values_match(&terminal.root_geometry_values, expected_key),
    )
}

fn singleton_root_values_match(values: &[i32], expected_key: i32) -> bool {
    values == [expected_key]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singleton_partition_preserves_sorted_parent_keys() {
        let parent = vec![0, 4, 8];
        let children = parent.iter().map(|key| vec![*key]).collect::<Vec<_>>();
        assert!(children.iter().all(|child| child.len() == 1));
        assert_eq!(children.into_iter().flatten().collect::<Vec<_>>(), parent);
    }

    #[test]
    fn singleton_root_values_require_exactly_the_selected_key() {
        assert!(singleton_root_values_match(&[7], 7));
        assert!(!singleton_root_values_match(&[], 7));
        assert!(!singleton_root_values_match(&[8], 7));
        assert!(!singleton_root_values_match(&[7, 8], 7));
    }

    #[test]
    fn singleton_root_restriction_accepts_root_infeasibility_without_observation() {
        let snapshot = RootDomainSnapshot::root_infeasible_without_brancher_call();
        assert_eq!(
            singleton_root_restriction_status(&snapshot, "terminal", 7),
            (false, true)
        );
    }
}
