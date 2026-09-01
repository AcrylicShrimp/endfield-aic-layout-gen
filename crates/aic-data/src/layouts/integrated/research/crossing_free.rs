use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    LogisticsComponentKind, ValidatedItemCatalog, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;
use crate::research::ModelComplexityMetrics;

use super::coordinate_partition::{classify_outcome, invalid_input, prepare_target_input};
use super::{
    EndpointSupportPropagationStatistics, ExactDimensionCaseOutcome, LayerGridAnalyzerRuntime,
};
use crate::layouts::integrated::{
    ExactModelMetrics, ExactSearchStatistics, ExactValidationStatus, IntegratedLayoutReport, exact,
};

pub const CROSSING_FREE_RESTRICTION_EXPERIMENT_SCHEMA_VERSION: u32 = 1;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;
const RUN_ORDER: [CrossingRestrictionCaseKind; 8] = [
    CrossingRestrictionCaseKind::Unrestricted,
    CrossingRestrictionCaseKind::CrossingFree,
    CrossingRestrictionCaseKind::CrossingFree,
    CrossingRestrictionCaseKind::Unrestricted,
    CrossingRestrictionCaseKind::CrossingFree,
    CrossingRestrictionCaseKind::Unrestricted,
    CrossingRestrictionCaseKind::Unrestricted,
    CrossingRestrictionCaseKind::CrossingFree,
];

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CrossingRestrictionCaseKind {
    Unrestricted,
    CrossingFree,
}

impl CrossingRestrictionCaseKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unrestricted => "A",
            Self::CrossingFree => "B",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CrossingRestrictionCaseReport {
    pub run_index: usize,
    pub label: String,
    pub kind: CrossingRestrictionCaseKind,
    pub outcome: ExactDimensionCaseOutcome,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub validation: ExactValidationStatus,
    pub model: ExactModelMetrics,
    pub model_complexity: ModelComplexityMetrics,
    pub search_statistics: ExactSearchStatistics,
    pub grid_propagation: LayerGridAnalyzerRuntime,
    pub endpoint_support_propagation: EndpointSupportPropagationStatistics,
    pub crossing_free_certificate: Option<exact::shared_layer::CrossingFreeBuildCertificate>,
    pub bridge_component_count: usize,
    pub layout: IntegratedLayoutReport,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CrossingFreeRestrictionExperimentReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub fixed_dimensions: [i32; 2],
    pub run_order: Vec<String>,
    pub case_search_budget_ms: u64,
    pub observation_budget_ms: u64,
    pub cases: Vec<CrossingRestrictionCaseReport>,
    pub crossing_free_observation: Option<CrossingRestrictionCaseReport>,
    pub unrestricted_case_count: usize,
    pub crossing_free_case_count: usize,
    pub unrestricted_witness_count: usize,
    pub crossing_free_witness_count: usize,
    pub all_crossing_free_cases_unknown: bool,
    pub crossing_free_witness_found: bool,
    pub selected_crossing_free_witness: Option<IntegratedLayoutReport>,
    pub model_identity_satisfied: bool,
    pub unrestricted_models_identical: bool,
    pub crossing_free_models_identical: bool,
    pub exact_restriction_delta_satisfied: bool,
    pub crossing_free_certificates_satisfied: bool,
    pub all_found_crossing_free_witnesses_valid: bool,
    pub outcome_consistency_satisfied: bool,
    pub hint_progression_authorized: bool,
    pub interpretation_blocked: bool,
    pub outer_wall_ms: u64,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_crossing_free_restriction(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    fixed_width: i32,
    fixed_height: i32,
    case_search_budget: Duration,
    observation_budget: Duration,
) -> Result<CrossingFreeRestrictionExperimentReport, IntegratedLayoutReport> {
    if fixed_width <= 0 || fixed_height <= 0 {
        return Err(invalid_input(
            "/fixed_dimensions",
            "crossing-free restriction experiment requires positive fixed dimensions",
        ));
    }
    if case_search_budget.is_zero() || observation_budget.is_zero() {
        return Err(invalid_input(
            "/search_budget",
            "crossing-free restriction experiment requires positive case and observation budgets",
        ));
    }
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success || target_phase_index >= growth.phases.len() {
        return Err(invalid_input(
            "/target_phase_index",
            "facility growth planning failed or target phase is out of range",
        ));
    }
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
    let fixed_dimensions = exact::shared_layer::FixedUsedDimensions {
        width: fixed_width,
        height: fixed_height,
    };
    let started = Instant::now();
    let mut cases = Vec::with_capacity(RUN_ORDER.len());
    for (run_index, kind) in RUN_ORDER.into_iter().enumerate() {
        cases.push(run_case(
            run_index,
            kind,
            input.clone(),
            logistics_components,
            fixed_dimensions,
            case_search_budget,
        ));
    }
    let all_crossing_free_cases_unknown = cases
        .iter()
        .filter(|case| case.kind == CrossingRestrictionCaseKind::CrossingFree)
        .all(|case| case.outcome == ExactDimensionCaseOutcome::Unknown);
    let crossing_free_observation = all_crossing_free_cases_unknown.then(|| {
        run_case(
            RUN_ORDER.len(),
            CrossingRestrictionCaseKind::CrossingFree,
            input,
            logistics_components,
            fixed_dimensions,
            observation_budget,
        )
    });

    let unrestricted = cases
        .iter()
        .filter(|case| case.kind == CrossingRestrictionCaseKind::Unrestricted)
        .collect::<Vec<_>>();
    let mut crossing_free = cases
        .iter()
        .filter(|case| case.kind == CrossingRestrictionCaseKind::CrossingFree)
        .collect::<Vec<_>>();
    let primary_crossing_free_case_count = crossing_free.len();
    crossing_free.extend(crossing_free_observation.iter());
    let unrestricted_models_identical = identical_models(&unrestricted);
    let crossing_free_models_identical = identical_models(&crossing_free);
    let exact_restriction_delta_satisfied = unrestricted.first().is_some_and(|control| {
        crossing_free.iter().all(|restricted| {
            restriction_delta_satisfied(
                &control.model_complexity,
                &restricted.model_complexity,
                restricted.crossing_free_certificate.as_ref(),
            )
        })
    });
    let model_identity_satisfied = unrestricted_models_identical
        && crossing_free_models_identical
        && unrestricted.first().is_some_and(|control| {
            crossing_free.iter().all(|restricted| {
                control.model == restricted.model
                    && control.layout.exact.as_ref().map(|exact| exact.formulation)
                        == restricted
                            .layout
                            .exact
                            .as_ref()
                            .map(|exact| exact.formulation)
            })
        });
    let crossing_free_certificates_satisfied = crossing_free
        .iter()
        .copied()
        .all(|case| certificate_satisfied(case.crossing_free_certificate.as_ref()));
    let all_found_crossing_free_witnesses_valid = crossing_free
        .iter()
        .copied()
        .filter(|case| case.layout.success)
        .all(|case| {
            case.validation == ExactValidationStatus::Passed && case.bridge_component_count == 0
        });
    let raw_crossing_free_witness = crossing_free
        .iter()
        .copied()
        .find(|case| case.layout.success)
        .map(|case| case.layout.clone());
    let unrestricted_witness_count = unrestricted
        .iter()
        .filter(|case| case.layout.success)
        .count();
    let crossing_free_witness_count = crossing_free
        .iter()
        .copied()
        .filter(|case| case.layout.success)
        .count();
    let crossing_free_witness_found = raw_crossing_free_witness.is_some();
    let outcome_consistency_satisfied =
        outcomes_are_consistent(&unrestricted) && outcomes_are_consistent(&crossing_free);
    let interpretation_blocked = !model_identity_satisfied
        || !exact_restriction_delta_satisfied
        || !crossing_free_certificates_satisfied
        || !all_found_crossing_free_witnesses_valid
        || !outcome_consistency_satisfied;
    let hint_progression_authorized = crossing_free_witness_found
        && all_found_crossing_free_witnesses_valid
        && !interpretation_blocked;
    let selected_crossing_free_witness = hint_progression_authorized
        .then_some(raw_crossing_free_witness)
        .flatten();
    let unrestricted_case_count = unrestricted.len();
    let crossing_free_case_count = primary_crossing_free_case_count;
    drop(unrestricted);
    drop(crossing_free);

    Ok(CrossingFreeRestrictionExperimentReport {
        schema_version: CROSSING_FREE_RESTRICTION_EXPERIMENT_SCHEMA_VERSION,
        target_phase_index,
        fixed_dimensions: [fixed_width, fixed_height],
        run_order: RUN_ORDER
            .iter()
            .map(|kind| kind.label().to_string())
            .collect(),
        case_search_budget_ms: millis(case_search_budget),
        observation_budget_ms: millis(observation_budget),
        cases,
        crossing_free_observation,
        unrestricted_case_count,
        crossing_free_case_count,
        unrestricted_witness_count,
        crossing_free_witness_count,
        all_crossing_free_cases_unknown,
        crossing_free_witness_found,
        selected_crossing_free_witness,
        model_identity_satisfied,
        unrestricted_models_identical,
        crossing_free_models_identical,
        exact_restriction_delta_satisfied,
        crossing_free_certificates_satisfied,
        all_found_crossing_free_witnesses_valid,
        outcome_consistency_satisfied,
        hint_progression_authorized,
        interpretation_blocked,
        outer_wall_ms: millis(started.elapsed()),
        diagnostic_only: true,
    })
}

fn run_case(
    run_index: usize,
    kind: CrossingRestrictionCaseKind,
    input: crate::layouts::integrated::ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    fixed_dimensions: exact::shared_layer::FixedUsedDimensions,
    budget: Duration,
) -> CrossingRestrictionCaseReport {
    let (layout, grid, endpoint, certificate) = match kind {
        CrossingRestrictionCaseKind::Unrestricted => {
            let (layout, grid, endpoint) = exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_feasibility_only_with_local_continuation_guarded_intersection_propagation(
                input,
                logistics_components,
                Some(budget),
                fixed_dimensions,
            );
            (layout, grid, endpoint, None)
        }
        CrossingRestrictionCaseKind::CrossingFree => {
            exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_crossing_free_feasibility_only_with_local_continuation_guarded_intersection_propagation(
                input,
                logistics_components,
                Some(budget),
                fixed_dimensions,
            )
        }
    };
    let exact = layout
        .exact
        .as_ref()
        .expect("constructed crossing restriction case has exact metrics");
    CrossingRestrictionCaseReport {
        run_index,
        label: kind.label().to_string(),
        kind,
        outcome: classify_outcome(&layout),
        construction_ms: exact.construction_ms,
        search_ms: exact.search_ms,
        first_incumbent_ms: exact.first_incumbent_ms,
        validation: exact.validation,
        model: exact.model.clone(),
        model_complexity: exact.model_complexity.clone(),
        search_statistics: exact.search_statistics.clone(),
        grid_propagation: super::possible_graph_connectivity::grid_analyzer_runtime(grid),
        endpoint_support_propagation: endpoint,
        crossing_free_certificate: certificate,
        bridge_component_count: layout
            .logistics_components
            .iter()
            .filter(|component| component.kind == LogisticsComponentKind::Bridge)
            .count(),
        layout,
    }
}

fn identical_models(cases: &[&CrossingRestrictionCaseReport]) -> bool {
    cases.first().is_some_and(|first| {
        cases.iter().all(|case| {
            case.model == first.model
                && case.model_complexity == first.model_complexity
                && case.layout.exact.as_ref().map(|exact| exact.formulation)
                    == first.layout.exact.as_ref().map(|exact| exact.formulation)
        })
    })
}

fn outcomes_are_consistent(cases: &[&CrossingRestrictionCaseReport]) -> bool {
    outcome_values_are_consistent(cases.iter().map(|case| case.outcome))
}

fn outcome_values_are_consistent(
    outcomes: impl IntoIterator<Item = ExactDimensionCaseOutcome>,
) -> bool {
    let outcomes = outcomes.into_iter().collect::<Vec<_>>();
    if outcomes.contains(&ExactDimensionCaseOutcome::InvalidWitness) {
        return false;
    }
    let has_feasible = outcomes
        .iter()
        .any(|outcome| *outcome == ExactDimensionCaseOutcome::ValidatedFeasible);
    let has_infeasible = outcomes
        .iter()
        .any(|outcome| *outcome == ExactDimensionCaseOutcome::ProvenInfeasible);
    !(has_feasible && has_infeasible)
}

fn restriction_delta_satisfied(
    control: &ModelComplexityMetrics,
    restricted: &ModelComplexityMetrics,
    certificate: Option<&exact::shared_layer::CrossingFreeBuildCertificate>,
) -> bool {
    let Some(certificate) =
        certificate.filter(|certificate| certificate_satisfied(Some(certificate)))
    else {
        return false;
    };
    let (Some(control_constraints), Some(restricted_constraints)) =
        (&control.constraints, &restricted.constraints)
    else {
        return false;
    };
    let (Some(control_graph), Some(restricted_graph)) =
        (&control.factor_graph, &restricted.factor_graph)
    else {
        return false;
    };
    control.variables == restricted.variables
        && control.coupling == restricted.coupling
        && control.symmetry == restricted.symmetry
        && control.estimated_bytes == restricted.estimated_bytes
        && restricted_constraints.total_constraints
            == control_constraints.total_constraints + certificate.posted_constraint_count as u64
        && restricted_constraints.total_terms
            == control_constraints.total_terms + certificate.posted_term_count as u64
        && restricted_graph.variable_vertices == control_graph.variable_vertices
        && restricted_graph.constraint_vertices
            == control_graph.constraint_vertices + certificate.posted_constraint_count as u64
        && restricted_graph.incidences
            == control_graph.incidences + certificate.posted_term_count as u64
        && restricted_graph.maximum_variable_degree == control_graph.maximum_variable_degree
        && restricted_graph.p95_variable_degree == control_graph.p95_variable_degree
        && restricted_graph.maximum_constraint_degree == control_graph.maximum_constraint_degree
        && restricted_graph.p95_constraint_degree == control_graph.p95_constraint_degree
        && restricted_graph.connected_components == control_graph.connected_components
        && restricted_graph.articulation_points == control_graph.articulation_points
        && restricted_graph.retained_full_graph == control_graph.retained_full_graph
        && restricted_constraints.by_family.iter().any(|family| {
            family.family == "crossing-restriction"
                && family.constraints == certificate.posted_constraint_count as u64
                && family.terms == certificate.posted_term_count as u64
                && family.relation == crate::research::ConstraintRelation::Equality
                && family.maximum_arity == certificate.posted_term_count as u64
                && family.p95_arity == certificate.posted_term_count as u64
                && family.maximum_absolute_coefficient == 1
        })
        && restricted_constraints
            .by_family
            .iter()
            .filter(|family| family.family != "crossing-restriction")
            .cloned()
            .collect::<Vec<_>>()
            == control_constraints.by_family
        && restricted_graph
            .family_incidences
            .iter()
            .filter(|incidence| incidence.constraint_family == "crossing-restriction")
            .map(|incidence| incidence.incidences)
            .sum::<u64>()
            == certificate.posted_term_count as u64
        && restricted_graph
            .family_incidences
            .iter()
            .filter(|incidence| incidence.constraint_family != "crossing-restriction")
            .cloned()
            .collect::<Vec<_>>()
            == control_graph.family_incidences
}

fn certificate_satisfied(
    certificate: Option<&exact::shared_layer::CrossingFreeBuildCertificate>,
) -> bool {
    certificate.is_some_and(|certificate| {
        let transports = certificate
            .bridges
            .iter()
            .map(|bridge| bridge.transport)
            .collect::<std::collections::BTreeSet<_>>();
        let coordinates = certificate
            .bridges
            .iter()
            .map(|bridge| (bridge.transport, bridge.cell))
            .collect::<std::collections::BTreeSet<_>>();
        let cartesian_coverage_complete = transports.len()
            == certificate.active_transport_layer_count
            && certificate
                .bridges
                .iter()
                .all(|bridge| bridge.cell < certificate.grid_cell_count)
            && transports.iter().all(|transport| {
                (0..certificate.grid_cell_count)
                    .all(|cell| coordinates.contains(&(*transport, cell)))
            });
        certificate.schema_version
            == exact::shared_layer::CROSSING_FREE_BUILD_CERTIFICATE_SCHEMA_VERSION
            && certificate.mode == "all-bridge-selected-zero"
            && certificate.complete
            && certificate.bridge_count == certificate.expected_bridge_count
            && certificate.bridges.len() == certificate.bridge_count
            && certificate.expected_bridge_count
                == certificate.grid_cell_count * certificate.active_transport_layer_count
            && cartesian_coverage_complete
            && certificate.posted_constraint_count == usize::from(certificate.bridge_count > 0)
            && certificate.posted_term_count == certificate.bridge_count
            && certificate.new_variable_count == 0
    })
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_is_counterbalanced() {
        assert_eq!(
            RUN_ORDER.map(CrossingRestrictionCaseKind::label),
            ["A", "B", "B", "A", "B", "A", "A", "B"]
        );
    }

    #[test]
    fn certificate_rejects_an_incomplete_bridge_census() {
        let certificate = exact::shared_layer::CrossingFreeBuildCertificate {
            schema_version: exact::shared_layer::CROSSING_FREE_BUILD_CERTIFICATE_SCHEMA_VERSION,
            mode: "all-bridge-selected-zero".to_string(),
            bridges: Vec::new(),
            bridge_count: 1,
            expected_bridge_count: 1,
            active_transport_layer_count: 1,
            grid_cell_count: 1,
            posted_constraint_count: 1,
            posted_term_count: 1,
            new_variable_count: 0,
            complete: true,
        };
        assert!(!certificate_satisfied(Some(&certificate)));
    }

    #[test]
    fn invalid_or_contradictory_outcomes_fail_closed() {
        assert!(!outcome_values_are_consistent([
            ExactDimensionCaseOutcome::InvalidWitness,
            ExactDimensionCaseOutcome::Unknown,
        ]));
        assert!(!outcome_values_are_consistent([
            ExactDimensionCaseOutcome::ValidatedFeasible,
            ExactDimensionCaseOutcome::ProvenInfeasible,
        ]));
        assert!(outcome_values_are_consistent([
            ExactDimensionCaseOutcome::Unknown,
            ExactDimensionCaseOutcome::ValidatedFeasible,
        ]));
    }
}
