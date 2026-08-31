use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;
use crate::facilities::FacilityPortDirection;

pub const EXTERNAL_BOUNDARY_SIDE_PARTITION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExternalBoundarySideDomain {
    pub case_index: usize,
    pub side: String,
    pub direction_index: i32,
    pub keys: Vec<i32>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExternalBoundarySideCaseReport {
    pub case_index: usize,
    pub side: String,
    pub direction_index: i32,
    pub allowed_keys: Vec<i32>,
    pub solve: ExternalBoundaryKeySolveReport,
    pub authoritative_certificate_satisfied: bool,
    pub observation_certificate_satisfied: bool,
    pub certificate_copies_equal: bool,
    pub facility_fixation_satisfied: bool,
    pub root_restriction_observed: bool,
    pub root_restriction_satisfied: bool,
    pub interpretation_blocked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExternalBoundarySidePartitionReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: ExternalBoundaryKeyLegalSupportAbReport,
    pub selected_network_index: usize,
    pub selected_network_id: String,
    pub selected_terminal: String,
    pub selected_terminal_rule: String,
    pub parent_root_keys: Vec<i32>,
    pub sides: Vec<ExternalBoundarySideDomain>,
    pub partition_non_empty: bool,
    pub partition_pairwise_disjoint: bool,
    pub partition_exact_cover: bool,
    pub execution_order: Vec<String>,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub construction_times_instrumented: bool,
    pub cases: Vec<ExternalBoundarySideCaseReport>,
    pub common_static_certificates_satisfied: bool,
    pub controlled_model_contract_satisfied: bool,
    pub validated_feasible_count: usize,
    pub proven_infeasible_count: usize,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub selected_case_witness_found: bool,
    pub selected_case_infeasibility_proven: bool,
    pub selected_next_parent_case_index: Option<usize>,
    pub combined_outcome: ExactDimensionCaseOutcome,
    pub interpretation_blocked: bool,
    pub experiment_ms: u64,
    pub total_wall_ms: u64,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_external_boundary_side_partition(
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
) -> Result<ExternalBoundarySidePartitionReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if side_authoritative_search_budget.is_zero() || side_observation_search_budget.is_zero() {
        return Err(invalid_input(
            "/external_boundary_side_partition_budget",
            "external boundary-side budgets must be positive",
        ));
    }
    let parent = diagnose_external_boundary_key_legal_support_ab(
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
    )?;
    if parent.interpretation_blocked
        || parent.sparse.combined_outcome != ExactDimensionCaseOutcome::Unknown
    {
        return Err(invalid_input(
            "/parent",
            "boundary-side partition requires an unblocked sparse parent that remains unknown",
        ));
    }

    let (selected_network_index, selected_network_id, selected_terminal, parent_root_keys) =
        select_external_demand(&parent.sparse.root_snapshot)?;
    let sides = partition_by_side(&parent_root_keys)?;
    let partition_non_empty = sides.iter().all(|side| !side.keys.is_empty());
    let partition_pairwise_disjoint = side_sets_pairwise_disjoint(&sides);
    let partition_exact_cover = side_union(&sides) == parent_root_keys;
    if !partition_non_empty || !partition_pairwise_disjoint || !partition_exact_cover {
        return Err(invalid_input(
            "/partition",
            "boundary-side domains must be non-empty, disjoint, and exactly cover the parent root domain",
        ));
    }

    let tuple_parent = &parent.parent;
    let parent_assignments = tuple_parent
        .parent
        .inherited_assignments
        .iter()
        .chain(&tuple_parent.parent.assignments)
        .cloned()
        .collect::<Vec<_>>();
    let requested = parent_assignments
        .iter()
        .chain(&parent.selected_assignments)
        .cloned()
        .collect::<Vec<_>>();
    assert_distinct_assignments(
        &requested,
        EXPECTED_TOTAL_FIXED_TERMINALS,
        "/selected_case/assignments",
    )?;
    let fixed_ports = exact_ports(&parent_assignments, &parent.selected_assignments);
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
    let expected_external_terminal_count = parent.static_certificates.len();
    let execution_order = sides
        .iter()
        .flat_map(|side| {
            [
                format!("{}-authoritative", side.side),
                format!("{}-observation", side.side),
            ]
        })
        .collect::<Vec<_>>();
    let experiment_started = Instant::now();
    let mut cases = Vec::with_capacity(sides.len());
    let mut static_certificate_shapes = Vec::with_capacity(sides.len() * 2);
    for side in &sides {
        let (authoritative_layout, authoritative_certificates) =
            exact::shared_layer::solve_sparse_support_endpoints_boundary_key_restricted_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                input.clone(),
                logistics_components,
                Some(side_authoritative_search_budget),
                dimensions,
                coordinate.clone(),
                fixed_ports.clone(),
                prior_reference,
                fixation,
                selected_terminal.clone(),
                side.keys.clone(),
            );
        let (observation_layout, root_snapshot, observation_certificates) =
            exact::shared_layer::solve_sparse_support_endpoints_boundary_key_restricted_fixed_dimensions_coordinate_ports_prior_overlap_root_snapshot(
                input.clone(),
                logistics_components,
                Some(side_observation_search_budget),
                dimensions,
                coordinate.clone(),
                fixed_ports.clone(),
                prior_reference,
                fixation,
                selected_terminal.clone(),
                side.keys.clone(),
            );
        let root_snapshot = root_snapshot.ok_or_else(|| {
            invalid_input(
                "/cases/root_snapshot",
                format!("{} observation did not return a root snapshot", side.side),
            )
        })?;
        let fixation_observation = assess_fixation(&root_snapshot, &requested);
        let solve = solve_report(
            &format!("sparse-boundary-side-{}", side.side),
            authoritative_layout,
            observation_layout,
            root_snapshot,
            fixation_observation,
        );
        let authoritative_certificate_satisfied = restriction_certificates_satisfied(
            &authoritative_certificates,
            &parent.static_certificates,
            &selected_terminal,
            &side.keys,
        );
        let observation_certificate_satisfied = restriction_certificates_satisfied(
            &observation_certificates,
            &parent.static_certificates,
            &selected_terminal,
            &side.keys,
        );
        debug_assert_eq!(
            parent.static_certificates.len(),
            expected_external_terminal_count
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
            root_restriction_status(&solve.root_snapshot, &selected_terminal, &side.keys);
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
        cases.push(ExternalBoundarySideCaseReport {
            case_index: side.case_index,
            side: side.side.clone(),
            direction_index: side.direction_index,
            allowed_keys: side.keys.clone(),
            solve,
            authoritative_certificate_satisfied,
            observation_certificate_satisfied,
            certificate_copies_equal,
            facility_fixation_satisfied,
            root_restriction_observed,
            root_restriction_satisfied,
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
                first.allowed_keys.len(),
                &case.solve.authoritative_layout,
                case.allowed_keys.len(),
            ) && side_model_contract_equal(
                &first.solve.observation_layout,
                first.allowed_keys.len(),
                &case.solve.observation_layout,
                case.allowed_keys.len(),
            )
        })
    });
    let validated_feasible_count =
        count_outcomes(&cases, ExactDimensionCaseOutcome::ValidatedFeasible);
    let proven_infeasible_count =
        count_outcomes(&cases, ExactDimensionCaseOutcome::ProvenInfeasible);
    let unknown_count = count_outcomes(&cases, ExactDimensionCaseOutcome::Unknown);
    let invalid_witness_count = count_outcomes(&cases, ExactDimensionCaseOutcome::InvalidWitness);
    let selected_case_witness_found = validated_feasible_count > 0;
    let selected_case_infeasibility_proven = proven_infeasible_count == sides.len();
    let interpretation_blocked = !common_static_certificates_satisfied
        || !controlled_model_contract_satisfied
        || cases.iter().any(|case| case.interpretation_blocked)
        || invalid_witness_count > 0;
    let combined_outcome = if interpretation_blocked {
        ExactDimensionCaseOutcome::InvalidWitness
    } else if selected_case_witness_found {
        ExactDimensionCaseOutcome::ValidatedFeasible
    } else if selected_case_infeasibility_proven {
        ExactDimensionCaseOutcome::ProvenInfeasible
    } else {
        ExactDimensionCaseOutcome::Unknown
    };
    let selected_next_parent_case_index = (!interpretation_blocked
        && selected_case_infeasibility_proven)
        .then(|| {
            tuple_parent
                .cases
                .iter()
                .filter(|case| {
                    case.case_index != parent.selected_case_index
                        && case.combined_outcome == ExactDimensionCaseOutcome::Unknown
                })
                .map(|case| case.case_index)
                .min()
        })
        .flatten();

    Ok(ExternalBoundarySidePartitionReport {
        schema_version: EXTERNAL_BOUNDARY_SIDE_PARTITION_SCHEMA_VERSION,
        target_phase_index,
        parent,
        selected_network_index,
        selected_network_id,
        selected_terminal,
        selected_terminal_rule:
            "lowest network index with one possible internal supply and one external demand whose root key count equals possible demand options"
                .to_string(),
        parent_root_keys,
        sides,
        partition_non_empty,
        partition_pairwise_disjoint,
        partition_exact_cover,
        execution_order,
        authoritative_case_search_budget_ms: millis(side_authoritative_search_budget),
        observation_case_search_budget_ms: millis(side_observation_search_budget),
        construction_times_instrumented: true,
        cases,
        common_static_certificates_satisfied,
        controlled_model_contract_satisfied,
        validated_feasible_count,
        proven_infeasible_count,
        unknown_count,
        invalid_witness_count,
        selected_case_witness_found,
        selected_case_infeasibility_proven,
        selected_next_parent_case_index,
        combined_outcome,
        interpretation_blocked,
        experiment_ms: millis(experiment_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        diagnostic_only: true,
    })
}

fn select_external_demand(
    snapshot: &RootDomainSnapshot,
) -> Result<(usize, String, String, Vec<i32>), IntegratedLayoutReport> {
    let mut candidates = snapshot
        .terminals
        .iter()
        .filter(|terminal| {
            terminal.endpoint_kind == "external"
                && terminal.direction == FacilityPortDirection::Input
                && terminal.root_geometry_values.len() > 1
        })
        .filter_map(|terminal| {
            let network = snapshot.networks.iter().find(|network| {
                network.network_index == terminal.network_index
                    && network.network_id == terminal.network_id
            })?;
            (network.possible_supply_options == 1
                && unique_possible_supply_is_internal(snapshot, network.network_index)
                && network.possible_demand_options == terminal.root_geometry_values.len())
            .then(|| {
                (
                    terminal.network_index,
                    terminal.network_id.clone(),
                    terminal.terminal.clone(),
                    terminal.root_geometry_values.clone(),
                )
            })
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| (&left.0, &left.2).cmp(&(&right.0, &right.2)));
    let Some(mut selected) = candidates.first().cloned() else {
        return Err(invalid_input(
            "/parent/sparse/root_snapshot/terminals",
            "no external demand satisfies the side-partition selection rule",
        ));
    };
    if candidates
        .iter()
        .filter(|candidate| candidate.0 == selected.0)
        .count()
        != 1
    {
        return Err(invalid_input(
            "/parent/sparse/root_snapshot/terminals",
            "the selected network has more than one matching external demand",
        ));
    }
    selected.3.sort_unstable();
    selected.3.dedup();
    Ok(selected)
}

fn partition_by_side(
    parent_keys: &[i32],
) -> Result<Vec<ExternalBoundarySideDomain>, IntegratedLayoutReport> {
    let definitions = [(0, "north"), (1, "east"), (2, "south"), (3, "west")];
    let sides = definitions
        .into_iter()
        .enumerate()
        .map(
            |(case_index, (direction_index, side))| ExternalBoundarySideDomain {
                case_index,
                side: side.to_string(),
                direction_index,
                keys: parent_keys
                    .iter()
                    .copied()
                    .filter(|key| key.rem_euclid(4) == direction_index)
                    .collect(),
            },
        )
        .collect::<Vec<_>>();
    if sides.iter().any(|side| side.keys.is_empty()) {
        return Err(invalid_input(
            "/partition/sides",
            "the controlled four-side partition requires every side to be present",
        ));
    }
    Ok(sides)
}

fn side_sets_pairwise_disjoint(sides: &[ExternalBoundarySideDomain]) -> bool {
    let mut seen = BTreeSet::new();
    sides
        .iter()
        .flat_map(|side| side.keys.iter().copied())
        .all(|key| seen.insert(key))
}

fn side_union(sides: &[ExternalBoundarySideDomain]) -> Vec<i32> {
    sides
        .iter()
        .flat_map(|side| side.keys.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn restriction_certificates_satisfied(
    certificates: &[exact::shared_layer::BoundaryKeyBuildCertificate],
    parent_certificates: &[ExternalBoundaryKeyStaticCertificate],
    selected_terminal: &str,
    expected_keys: &[i32],
) -> bool {
    if certificates.len() != parent_certificates.len() {
        return false;
    }
    let mut by_terminal = BTreeMap::new();
    for certificate in certificates {
        if by_terminal
            .insert(certificate.terminal.as_str(), certificate)
            .is_some()
        {
            return false;
        }
        let Some(parent) = parent_certificates
            .iter()
            .find(|parent| parent.terminal == certificate.terminal)
        else {
            return false;
        };
        if certificate.network_index != parent.network_index
            || certificate.network_id != parent.network_id
            || certificate.declared_domain_kind != "sparse-legal"
            || certificate.declared_values != certificate.unary_table_projection
            || certificate.declared_values != certificate.routing_option_keys
        {
            return false;
        }
        if certificate.terminal == selected_terminal
            && !selected_certificate_matches(certificate, expected_keys)
        {
            return false;
        }
        if certificate.terminal != selected_terminal
            && (certificate.declared_values.len() != parent.sparse_declared_count
                || certificate.unary_table_projection.len() != parent.sparse_table_count
                || certificate.routing_option_keys.len() != parent.sparse_option_count)
        {
            return false;
        }
    }
    certificates.iter().all(|certificate| {
        if certificate.terminal == selected_terminal {
            certificate.restriction_values.as_deref() == Some(expected_keys)
        } else {
            certificate.restriction_values.is_none()
        }
    }) && by_terminal.contains_key(selected_terminal)
}

fn selected_certificate_matches(
    certificate: &exact::shared_layer::BoundaryKeyBuildCertificate,
    expected_keys: &[i32],
) -> bool {
    let (Some(expected_lower), Some(expected_upper)) = (
        expected_keys.first().copied(),
        expected_keys.last().copied(),
    ) else {
        return false;
    };
    certificate.declared_lower_bound == expected_lower
        && certificate.declared_upper_bound == expected_upper
        && certificate.declared_values == expected_keys
        && certificate.unary_table_projection == expected_keys
        && certificate.routing_option_keys == expected_keys
        && certificate.restriction_values.as_deref() == Some(expected_keys)
}

fn normalized_certificates(
    certificates: &[exact::shared_layer::BoundaryKeyBuildCertificate],
    selected_terminal: &str,
) -> Vec<exact::shared_layer::BoundaryKeyBuildCertificate> {
    let mut normalized = certificates.to_vec();
    for certificate in &mut normalized {
        certificate.restriction_values = None;
        if certificate.terminal == selected_terminal {
            certificate.declared_lower_bound = 0;
            certificate.declared_upper_bound = 0;
            certificate.declared_values.clear();
            certificate.unary_table_projection.clear();
            certificate.routing_option_keys.clear();
        }
    }
    normalized.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    normalized
}

fn side_model_contract_equal(
    left: &IntegratedLayoutReport,
    left_selected_key_count: usize,
    right: &IntegratedLayoutReport,
    right_selected_key_count: usize,
) -> bool {
    let (Some(left), Some(right)) = (&left.exact, &right.exact) else {
        return false;
    };
    let mut left_model = left.model;
    let mut right_model = right.model;
    let Some(left_uncontrolled_boundary_variables) = left_model
        .boundary_terminal_variables
        .checked_sub(left_selected_key_count)
    else {
        return false;
    };
    let Some(right_uncontrolled_boundary_variables) = right_model
        .boundary_terminal_variables
        .checked_sub(right_selected_key_count)
    else {
        return false;
    };
    // Every retained boundary key creates one routing option, and post_cell_topology records one
    // terminal-option crossing guard for it. Normalize only that certified selected-domain delta.
    let Some(left_uncontrolled_crossing_constraints) = left_model
        .crossing_constraints
        .checked_sub(left_selected_key_count)
    else {
        return false;
    };
    let Some(right_uncontrolled_crossing_constraints) = right_model
        .crossing_constraints
        .checked_sub(right_selected_key_count)
    else {
        return false;
    };
    left_model.boundary_terminal_variables = left_uncontrolled_boundary_variables;
    right_model.boundary_terminal_variables = right_uncontrolled_boundary_variables;
    left_model.crossing_constraints = left_uncontrolled_crossing_constraints;
    right_model.crossing_constraints = right_uncontrolled_crossing_constraints;
    left.formulation == right.formulation && left_model == right_model
}

fn root_facility_fixation_satisfied(snapshot: &RootDomainSnapshot, expected_count: usize) -> bool {
    snapshot.capture_status == "root-infeasible"
        || (snapshot.fixed_facility_contract_satisfied
            && snapshot.facilities.len() == expected_count
            && snapshot.facilities.iter().all(|facility| {
                facility.expected_fixed
                    && facility.fixed_contract_satisfied
                    && facility.placement_choice.cardinality == 1
                    && facility.possible_x_values.len() == 1
                    && facility.possible_y_values.len() == 1
                    && facility.possible_rotations.len() == 1
            }))
}

fn unique_possible_supply_is_internal(snapshot: &RootDomainSnapshot, network_index: usize) -> bool {
    let supplies = snapshot
        .terminals
        .iter()
        .filter(|terminal| {
            terminal.network_index == network_index
                && terminal.direction == FacilityPortDirection::Output
        })
        .map(|terminal| {
            (
                terminal.endpoint_kind.as_str(),
                terminal.routing_options.fixed_true + terminal.routing_options.unresolved,
            )
        })
        .collect::<Vec<_>>();
    supply_options_have_one_internal_owner(&supplies)
}

fn supply_options_have_one_internal_owner(supplies: &[(&str, usize)]) -> bool {
    supplies.iter().map(|(_, count)| count).sum::<usize>() == 1
        && supplies
            .iter()
            .any(|(kind, count)| *kind == "facility" && *count == 1)
}

fn root_restriction_status(
    snapshot: &RootDomainSnapshot,
    selected_terminal: &str,
    expected_keys: &[i32],
) -> (bool, bool) {
    if snapshot.capture_status == "root-infeasible" {
        return (false, true);
    }
    let expected = expected_keys.iter().copied().collect::<BTreeSet<_>>();
    let Some(terminal) = snapshot
        .terminals
        .iter()
        .find(|terminal| terminal.terminal == selected_terminal)
    else {
        return (false, false);
    };
    (
        true,
        terminal
            .root_geometry_values
            .iter()
            .all(|key| expected.contains(key)),
    )
}

fn count_outcomes(
    cases: &[ExternalBoundarySideCaseReport],
    outcome: ExactDimensionCaseOutcome,
) -> usize {
    cases
        .iter()
        .filter(|case| case.solve.combined_outcome == outcome)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_partition_is_non_empty_disjoint_and_complete() {
        let keys = vec![0, 1, 2, 3, 4, 5, 6, 7];
        let sides = partition_by_side(&keys).unwrap();
        assert!(sides.iter().all(|side| side.keys.len() == 2));
        assert!(side_sets_pairwise_disjoint(&sides));
        assert_eq!(side_union(&sides), keys);
    }

    #[test]
    fn side_partition_rejects_a_missing_side() {
        assert!(partition_by_side(&[0, 1, 2]).is_err());
    }

    #[test]
    fn root_restriction_requires_the_observed_terminal_and_subset() {
        let root_infeasible = RootDomainSnapshot::root_infeasible_without_brancher_call();
        assert_eq!(
            root_restriction_status(&root_infeasible, "terminal", &[0]),
            (false, true)
        );
    }

    #[test]
    fn supply_selection_requires_one_facility_owned_option() {
        assert!(supply_options_have_one_internal_owner(&[
            ("facility", 1),
            ("external", 0),
        ]));
        assert!(!supply_options_have_one_internal_owner(&[("external", 1)]));
        assert!(!supply_options_have_one_internal_owner(&[("facility", 2)]));
    }

    #[test]
    fn selected_certificate_requires_the_actual_side_domain() {
        let certificate = exact::shared_layer::BoundaryKeyBuildCertificate {
            terminal: "selected".to_string(),
            network_index: 1,
            network_id: "network".to_string(),
            declared_domain_kind: "sparse-legal".to_string(),
            declared_lower_bound: 0,
            declared_upper_bound: 4,
            declared_values: vec![0, 4],
            unary_table_projection: vec![0, 4],
            routing_option_keys: vec![0, 4],
            restriction_values: Some(vec![0, 4]),
        };
        assert!(selected_certificate_matches(&certificate, &[0, 4]));
        let mut mismatched = certificate;
        mismatched.declared_values.push(8);
        assert!(!selected_certificate_matches(&mismatched, &[0, 4]));
    }
}
