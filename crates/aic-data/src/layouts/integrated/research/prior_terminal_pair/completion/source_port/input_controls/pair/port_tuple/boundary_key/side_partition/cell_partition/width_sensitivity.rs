use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use serde::Serialize;

use super::*;
use crate::layouts::ExactModelMetrics;

pub const BOUNDARY_CELL_WIDTH_SENSITIVITY_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoundaryCellWidthCaseReport {
    pub case_index: usize,
    pub width: i32,
    pub height: i32,
    pub encoded_key: i32,
    pub fixed_facilities_fit: bool,
    pub solve: ExternalBoundaryKeySolveReport,
    pub authoritative_certificate_satisfied: bool,
    pub observation_certificate_satisfied: bool,
    pub certificate_copies_equal: bool,
    pub certificate_identity_satisfied: bool,
    pub root_restriction_observed: bool,
    pub root_restriction_satisfied: bool,
    pub facility_fixation_satisfied: bool,
    pub grid_cell_count_satisfied: bool,
    pub semantic_model_contract_satisfied: bool,
    pub interpretation_blocked: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoundaryCellWidthSensitivityReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub parent: ExternalBoundaryCellPartitionReport,
    pub selected_parent_key: i32,
    pub semantic_side: String,
    pub semantic_x: i32,
    pub semantic_y: i32,
    pub semantic_direction_index: i32,
    pub fixed_height: i32,
    pub requested_widths: Vec<i32>,
    pub widths_positive: bool,
    pub widths_strictly_increasing: bool,
    pub widths_within_request_ceiling: bool,
    pub includes_parent_width: bool,
    pub authoritative_case_search_budget_ms: u64,
    pub observation_case_search_budget_ms: u64,
    pub construction_times_instrumented: bool,
    pub cases: Vec<BoundaryCellWidthCaseReport>,
    pub common_logical_input_identity_satisfied: bool,
    pub common_certificate_identity_satisfied: bool,
    pub common_semantic_model_contract_satisfied: bool,
    pub validated_feasible_count: usize,
    pub proven_infeasible_count: usize,
    pub unknown_count: usize,
    pub invalid_witness_count: usize,
    pub witness_found: bool,
    pub interpretation_blocked: bool,
    pub experiment_ms: u64,
    pub total_wall_ms: u64,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_boundary_cell_width_sensitivity(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    parent_width: i32,
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
    requested_widths: Vec<i32>,
    width_authoritative_search_budget: Duration,
    width_observation_search_budget: Duration,
) -> Result<BoundaryCellWidthSensitivityReport, IntegratedLayoutReport> {
    let total_started = Instant::now();
    if width_authoritative_search_budget.is_zero() || width_observation_search_budget.is_zero() {
        return Err(invalid_input(
            "/boundary_cell_width_sensitivity_budget",
            "boundary-cell width-sensitivity budgets must be positive",
        ));
    }
    let parent = diagnose_external_boundary_cell_partition(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        target_phase_index,
        parent_width,
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
    )?;
    if parent.interpretation_blocked
        || parent.selected_side_outcome != ExactDimensionCaseOutcome::Unknown
    {
        return Err(invalid_input(
            "/parent",
            "width sensitivity requires an unblocked unresolved boundary-cell parent",
        ));
    }
    let selected_parent_key = *parent.unresolved_keys.first().ok_or_else(|| {
        invalid_input(
            "/parent/unresolved_keys",
            "width sensitivity requires one unresolved singleton key",
        )
    })?;
    let (semantic_x, semantic_y, semantic_direction_index) =
        decode_geometry_key(selected_parent_key, parent_width)?;
    if semantic_direction_index != 0 || semantic_y != 0 {
        return Err(invalid_input(
            "/parent/unresolved_keys/0",
            "the selected representative must be a north-boundary endpoint",
        ));
    }

    let widths_positive = !requested_widths.is_empty()
        && requested_widths
            .iter()
            .all(|width| *width > semantic_x && *width > 0);
    let widths_strictly_increasing = requested_widths.windows(2).all(|pair| pair[0] < pair[1]);
    let widths_within_request_ceiling = requested_widths
        .iter()
        .all(|width| i64::from(*width) <= request.max_width);
    let includes_parent_width = requested_widths.contains(&parent_width);
    if !widths_positive
        || !widths_strictly_increasing
        || !widths_within_request_ceiling
        || !includes_parent_width
    {
        return Err(invalid_input(
            "/requested_widths",
            "widths must be positive, strictly increasing, within the request ceiling, contain the semantic endpoint, and include the parent width",
        ));
    }

    let side_parent = &parent.parent;
    let boundary_parent = &side_parent.parent;
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
    let coordinate = exact::shared_layer::FixedFacilityCoordinate {
        instance: tuple_parent.parent.partitioned_facility.clone(),
        x: tuple_parent.parent.fixed_coordinate[0],
        y: tuple_parent.parent.fixed_coordinate[1],
        rotation: Some(tuple_parent.parent.fixed_rotation),
    };
    let prior_reference = &tuple_parent.parent.prior_reference;
    let fixation = exact::shared_layer::ReferenceAblationFixation::PriorOverlapPlacements;
    let selected_terminal = parent.selected_terminal.clone();
    let expected_external_terminal_count = boundary_parent.static_certificates.len();
    let experiment_started = Instant::now();
    let mut cases = Vec::with_capacity(requested_widths.len());
    let mut logical_input_identities = Vec::with_capacity(requested_widths.len());
    let mut certificate_identities = Vec::with_capacity(requested_widths.len() * 2);
    for (case_index, width) in requested_widths.iter().copied().enumerate() {
        let case_request = FacilityPlacementRequest {
            schema_version: request.schema_version,
            max_width: i64::from(width),
            max_height: i64::from(fixed_height),
        };
        let input = prepare_target_input(
            instance_wiring,
            facilities,
            items,
            transports,
            logistics_components,
            &case_request,
            &growth,
            target_phase_index,
        )?;
        logical_input_identities.push(logical_input_identity(&input));
        let encoded_key =
            encode_geometry_key(semantic_x, semantic_y, width, semantic_direction_index)?;
        let allowed_keys = vec![encoded_key];
        let dimensions = exact::shared_layer::FixedUsedDimensions {
            width,
            height: fixed_height,
        };
        let fixed_facilities_fit =
            fixed_facilities_fit(prior_reference, &input, &coordinate, width, fixed_height);
        let (authoritative_layout, authoritative_certificates) =
            exact::shared_layer::solve_sparse_support_endpoints_boundary_key_restricted_fixed_dimensions_coordinate_ports_prior_overlap_ablation(
                input.clone(),
                logistics_components,
                Some(width_authoritative_search_budget),
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
                Some(width_observation_search_budget),
                dimensions,
                coordinate.clone(),
                fixed_ports.clone(),
                prior_reference,
                fixation,
                selected_terminal.clone(),
                allowed_keys,
            );
        let root_snapshot = root_snapshot.ok_or_else(|| {
            invalid_input(
                "/cases/root_snapshot",
                format!("width {width} observation did not return a root snapshot"),
            )
        })?;
        let fixation_observation = assess_fixation(&root_snapshot, &requested);
        let solve = solve_report(
            &format!("sparse-boundary-cell-width-{width}"),
            authoritative_layout,
            observation_layout,
            root_snapshot,
            fixation_observation,
        );
        let legal_keys = exact::reachable_boundary_keys(width, fixed_height);
        let authoritative_certificate_satisfied = width_certificates_satisfied(
            &authoritative_certificates,
            expected_external_terminal_count,
            &selected_terminal,
            encoded_key,
            &legal_keys,
        );
        let observation_certificate_satisfied = width_certificates_satisfied(
            &observation_certificates,
            expected_external_terminal_count,
            &selected_terminal,
            encoded_key,
            &legal_keys,
        );
        let certificate_copies_equal = authoritative_certificates == observation_certificates;
        let authoritative_identity = certificate_identity(&authoritative_certificates);
        let observation_identity = certificate_identity(&observation_certificates);
        let certificate_identity_satisfied = authoritative_identity == observation_identity;
        certificate_identities.push(authoritative_identity);
        certificate_identities.push(observation_identity);
        let (root_restriction_observed, root_restriction_satisfied) =
            singleton_root_restriction_status(
                &solve.root_snapshot,
                &selected_terminal,
                encoded_key,
            );
        let facility_fixation_satisfied =
            root_facility_fixation_satisfied(&solve.root_snapshot, input.instances.len());
        let grid_cell_count_satisfied = solve
            .authoritative_layout
            .exact
            .as_ref()
            .is_some_and(|exact| exact.model.grid_cell_count == (width * fixed_height) as usize)
            && solve
                .observation_layout
                .exact
                .as_ref()
                .is_some_and(|exact| {
                    exact.model.grid_cell_count == (width * fixed_height) as usize
                });
        let semantic_model_contract_satisfied =
            width_semantic_model_contract(&solve.authoritative_layout, &solve.observation_layout);
        let interpretation_blocked = !fixed_facilities_fit
            || solve.evidence_conflict
            || solve.combined_outcome == ExactDimensionCaseOutcome::InvalidWitness
            || !authoritative_certificate_satisfied
            || !observation_certificate_satisfied
            || !certificate_copies_equal
            || !certificate_identity_satisfied
            || !root_restriction_satisfied
            || !facility_fixation_satisfied
            || !grid_cell_count_satisfied
            || !semantic_model_contract_satisfied
            || (solve.fixation_observation.assertion_applies
                && !solve.fixation_observation.assertion_satisfied);
        cases.push(BoundaryCellWidthCaseReport {
            case_index,
            width,
            height: fixed_height,
            encoded_key,
            fixed_facilities_fit,
            solve,
            authoritative_certificate_satisfied,
            observation_certificate_satisfied,
            certificate_copies_equal,
            certificate_identity_satisfied,
            root_restriction_observed,
            root_restriction_satisfied,
            facility_fixation_satisfied,
            grid_cell_count_satisfied,
            semantic_model_contract_satisfied,
            interpretation_blocked,
        });
    }

    let common_logical_input_identity_satisfied =
        logical_input_identities.first().is_some_and(|first| {
            logical_input_identities
                .iter()
                .all(|identity| identity == first)
        });
    let common_certificate_identity_satisfied =
        certificate_identities.first().is_some_and(|first| {
            certificate_identities
                .iter()
                .all(|identity| identity == first)
        });
    let common_semantic_model_contract_satisfied = cases.first().is_some_and(|first| {
        cases.iter().all(|case| {
            cross_width_semantic_model_contract(
                &first.solve.authoritative_layout,
                &case.solve.authoritative_layout,
            ) && cross_width_semantic_model_contract(
                &first.solve.observation_layout,
                &case.solve.observation_layout,
            )
        })
    });
    let validated_feasible_count =
        count_width_outcomes(&cases, ExactDimensionCaseOutcome::ValidatedFeasible);
    let proven_infeasible_count =
        count_width_outcomes(&cases, ExactDimensionCaseOutcome::ProvenInfeasible);
    let unknown_count = count_width_outcomes(&cases, ExactDimensionCaseOutcome::Unknown);
    let invalid_witness_count =
        count_width_outcomes(&cases, ExactDimensionCaseOutcome::InvalidWitness);
    let witness_found = validated_feasible_count > 0;
    let interpretation_blocked = !common_logical_input_identity_satisfied
        || !common_certificate_identity_satisfied
        || !common_semantic_model_contract_satisfied
        || cases.iter().any(|case| case.interpretation_blocked)
        || invalid_witness_count > 0;

    Ok(BoundaryCellWidthSensitivityReport {
        schema_version: BOUNDARY_CELL_WIDTH_SENSITIVITY_SCHEMA_VERSION,
        target_phase_index,
        parent,
        selected_parent_key,
        semantic_side: "north".to_string(),
        semantic_x,
        semantic_y,
        semantic_direction_index,
        fixed_height,
        requested_widths,
        widths_positive,
        widths_strictly_increasing,
        widths_within_request_ceiling,
        includes_parent_width,
        authoritative_case_search_budget_ms: millis(width_authoritative_search_budget),
        observation_case_search_budget_ms: millis(width_observation_search_budget),
        construction_times_instrumented: true,
        cases,
        common_logical_input_identity_satisfied,
        common_certificate_identity_satisfied,
        common_semantic_model_contract_satisfied,
        validated_feasible_count,
        proven_infeasible_count,
        unknown_count,
        invalid_witness_count,
        witness_found,
        interpretation_blocked,
        experiment_ms: millis(experiment_started.elapsed()),
        total_wall_ms: millis(total_started.elapsed()),
        diagnostic_only: true,
    })
}

fn decode_geometry_key(key: i32, width: i32) -> Result<(i32, i32, i32), IntegratedLayoutReport> {
    if key < 0 || width <= 0 {
        return Err(invalid_input(
            "/selected_parent_key",
            "boundary geometry key and width must be non-negative and positive",
        ));
    }
    let cell = key / 4;
    Ok((cell % width, cell / width, key % 4))
}

fn encode_geometry_key(
    x: i32,
    y: i32,
    width: i32,
    direction_index: i32,
) -> Result<i32, IntegratedLayoutReport> {
    if x < 0 || y < 0 || width <= x || !matches!(direction_index, 0..=3) {
        return Err(invalid_input(
            "/semantic_endpoint",
            "semantic endpoint is outside the requested width or has an invalid direction",
        ));
    }
    y.checked_mul(width)
        .and_then(|cell| cell.checked_add(x))
        .and_then(|cell| cell.checked_mul(4))
        .and_then(|key| key.checked_add(direction_index))
        .ok_or_else(|| invalid_input("/semantic_endpoint", "boundary geometry key overflow"))
}

fn width_certificates_satisfied(
    certificates: &[exact::shared_layer::BoundaryKeyBuildCertificate],
    expected_count: usize,
    selected_terminal: &str,
    selected_key: i32,
    legal_keys: &[i32],
) -> bool {
    let selected_keys = [selected_key];
    certificates.len() == expected_count
        && certificates
            .iter()
            .map(|certificate| certificate.terminal.as_str())
            .collect::<BTreeSet<_>>()
            .len()
            == expected_count
        && certificates.iter().all(|certificate| {
            let selected = certificate.terminal == selected_terminal;
            let expected_keys = if selected {
                selected_keys.as_slice()
            } else {
                legal_keys
            };
            certificate.declared_domain_kind == "sparse-legal"
                && certificate.declared_lower_bound == expected_keys[0]
                && certificate.declared_upper_bound == *expected_keys.last().expect("non-empty")
                && certificate.declared_values == expected_keys
                && certificate.unary_table_projection == expected_keys
                && certificate.routing_option_keys == expected_keys
                && if selected {
                    certificate.restriction_values.as_deref() == Some(selected_keys.as_slice())
                } else {
                    certificate.restriction_values.is_none()
                }
        })
}

fn certificate_identity(
    certificates: &[exact::shared_layer::BoundaryKeyBuildCertificate],
) -> Vec<(String, usize, String)> {
    let mut identity = certificates
        .iter()
        .map(|certificate| {
            (
                certificate.terminal.clone(),
                certificate.network_index,
                certificate.network_id.clone(),
            )
        })
        .collect::<Vec<_>>();
    identity.sort();
    identity
}

fn logical_input_identity(
    input: &ModelInput,
) -> (Vec<(String, String, String)>, Vec<String>, Vec<String>) {
    let mut instances = input
        .instances
        .iter()
        .map(|instance| {
            (
                instance.id.clone(),
                instance.recipe.clone(),
                instance.facility.clone(),
            )
        })
        .collect::<Vec<_>>();
    instances.sort();
    let mut requirements = input
        .edges
        .iter()
        .map(|edge| edge.requirement_id.clone())
        .collect::<Vec<_>>();
    requirements.sort();
    let mut networks = input
        .networks
        .iter()
        .map(|network| network.id().to_string())
        .collect::<Vec<_>>();
    networks.sort();
    (instances, requirements, networks)
}

fn fixed_facilities_fit(
    prior_reference: &IntegratedLayoutReport,
    input: &ModelInput,
    coordinate: &exact::shared_layer::FixedFacilityCoordinate,
    width: i32,
    height: i32,
) -> bool {
    if prior_reference.placements.len() + 1 != input.instances.len()
        || prior_reference.placements.iter().any(|placement| {
            placement.x < 0
                || placement.y < 0
                || placement.x + placement.width > i64::from(width)
                || placement.y + placement.height > i64::from(height)
        })
    {
        return false;
    }
    let Some(instance) = input
        .instances
        .iter()
        .find(|instance| instance.id == coordinate.instance)
    else {
        return false;
    };
    let rotation = coordinate.rotation.unwrap_or(0);
    let Ok(base_width) = i32::try_from(instance.definition.footprint.width) else {
        return false;
    };
    let Ok(base_height) = i32::try_from(instance.definition.footprint.height) else {
        return false;
    };
    let (facility_width, facility_height) = if matches!(rotation, 90 | 270) {
        (base_height, base_width)
    } else {
        (base_width, base_height)
    };
    coordinate.x >= 0
        && coordinate.y >= 0
        && coordinate.x + facility_width <= width
        && coordinate.y + facility_height <= height
}

fn width_semantic_model_contract(
    authoritative: &IntegratedLayoutReport,
    observation: &IntegratedLayoutReport,
) -> bool {
    let (Some(authoritative), Some(observation)) = (&authoritative.exact, &observation.exact)
    else {
        return false;
    };
    authoritative.formulation == observation.formulation
        && authoritative.model == observation.model
        && authoritative.model_complexity == observation.model_complexity
}

fn cross_width_semantic_model_contract(
    left: &IntegratedLayoutReport,
    right: &IntegratedLayoutReport,
) -> bool {
    let (Some(left), Some(right)) = (&left.exact, &right.exact) else {
        return false;
    };
    left.formulation == right.formulation
        && semantic_model_signature(left.model) == semantic_model_signature(right.model)
}

fn semantic_model_signature(model: ExactModelMetrics) -> [i64; 18] {
    [
        model.facility_count as i64,
        model.route_requirement_count as i64,
        model.commodity_network_count as i64,
        model.commodity_item_count as i64,
        model.belt_network_count as i64,
        model.pipe_network_count as i64,
        model.network_requirement_reference_count as i64,
        model.network_terminal_count as i64,
        model.external_terminal_count as i64,
        model.boundary_terminal_count as i64,
        model.maximum_network_flow_scale,
        i64::from(model.maximum_line_capacity_units),
        model.total_terminal_flow_units,
        model.hinted_placements as i64,
        model.hinted_terminals as i64,
        model.hinted_networks as i64,
        model.hinted_components as i64,
        model.crossing_owner_variables as i64,
    ]
}

fn count_width_outcomes(
    cases: &[BoundaryCellWidthCaseReport],
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
    fn semantic_north_endpoint_is_reencoded_per_width() {
        let (x, y, direction) = decode_geometry_key(24, 16).unwrap();
        assert_eq!((x, y, direction), (6, 0, 0));
        assert_eq!(encode_geometry_key(x, y, 13, direction).unwrap(), 24);
        assert_eq!(encode_geometry_key(x, y, 16, direction).unwrap(), 24);
    }

    #[test]
    fn legal_boundary_key_count_matches_variable_used_bounds() {
        assert_eq!(exact::reachable_boundary_keys(13, 16).len(), 445);
        assert_eq!(exact::reachable_boundary_keys(16, 16).len(), 544);
    }

    #[test]
    fn width_order_requires_a_strict_sequence() {
        let ordered = [13, 14, 15, 16];
        let repeated = [13, 14, 14, 16];
        assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(!repeated.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
