use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::results::{ProblemSolution, SatisfactionResult};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, Literal, TransformableVariable};
use serde::Serialize;

use super::super::{ExactSearchStatistics, ExactValidationStatus};
use super::metrics::elapsed_millis;
use super::recorder::{ConstraintFamily, RecordedModel, VariableFamily};
use super::search_statistics::{MeteredBrancher, SearchEventCounters, capture_search_statistics};
use super::{IntegratedLayoutDiagnostic, ModelInput};
use crate::layouts::{FacilityPlacement, FacilityPlacementBounds};
use crate::research::ModelComplexityMetrics;

pub const BOTTOM_UP_RUNG_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BottomUpRungKind {
    FacilityGeometry,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BottomUpRungOutcome {
    Feasible,
    Infeasible,
    Unknown,
    InvalidWitness,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BottomUpSemanticCertificate {
    pub facility_geometry: bool,
    pub facility_ports: bool,
    pub boundary_terminals: bool,
    pub pipe_routing: bool,
    pub belt_routing: bool,
    pub item_flow: bool,
    pub logistics_components: bool,
    pub objective: bool,
    pub hints: bool,
    pub transferred_learned_state: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityGeometryWitness {
    pub bounds: FacilityPlacementBounds,
    pub placements: Vec<FacilityPlacement>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpRungReport {
    pub schema_version: u32,
    pub rung: BottomUpRungKind,
    pub formulation: &'static str,
    pub ceiling: [i32; 2],
    pub facility_count: usize,
    pub semantic_certificate: BottomUpSemanticCertificate,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_witness_ms: Option<u64>,
    pub outcome: BottomUpRungOutcome,
    pub validation: ExactValidationStatus,
    pub model_complexity: ModelComplexityMetrics,
    pub search_statistics: ExactSearchStatistics,
    pub witness: Option<FacilityGeometryWitness>,
    pub diagnostics: Vec<IntegratedLayoutDiagnostic>,
}

struct PlacementModel {
    model: RecordedModel,
    instances: Vec<ModelInstance>,
}

struct ModelInstance {
    id: String,
    recipe: String,
    facility: String,
    x: DomainId,
    y: DomainId,
    rotation: DomainId,
    orientations: Vec<ModelOrientation>,
}

#[derive(Clone, Copy)]
struct ModelOrientation {
    rotation: i64,
    width: i32,
    height: i32,
    selected: Literal,
    selected_parent: DomainId,
}

pub(in crate::layouts::integrated) fn solve_facility_geometry_rung(
    input: ModelInput,
    time_limit: Duration,
) -> BottomUpRungReport {
    let ceiling = [input.width, input.height];
    let facility_count = input.instances.len();
    let construction_started = Instant::now();
    let mut placement_model = match build_model(&input) {
        Ok(model) => model,
        Err(diagnostic) => {
            return BottomUpRungReport {
                schema_version: BOTTOM_UP_RUNG_SCHEMA_VERSION,
                rung: BottomUpRungKind::FacilityGeometry,
                formulation: "coordinate-rotation-disjunctive-non-overlap-v1",
                ceiling,
                facility_count,
                semantic_certificate: facility_geometry_certificate(),
                construction_ms: elapsed_millis(construction_started.elapsed()),
                search_ms: 0,
                first_witness_ms: None,
                outcome: BottomUpRungOutcome::Infeasible,
                validation: ExactValidationStatus::NotAttempted,
                model_complexity: ModelComplexityMetrics::unavailable(),
                search_statistics: ExactSearchStatistics::default(),
                witness: None,
                diagnostics: vec![diagnostic],
            };
        }
    };
    let construction_ms = elapsed_millis(construction_started.elapsed());
    let model_complexity = placement_model.model.metrics();

    let search_started = Instant::now();
    let search_event_counters = Arc::new(Mutex::new(SearchEventCounters::default()));
    let default_brancher = placement_model.model.solver_mut().default_brancher();
    let mut brancher = MeteredBrancher::new(default_brancher, Arc::clone(&search_event_counters));
    let mut resolver = ResolutionResolver::default();
    let mut termination = TimeBudget::starting_now(time_limit);
    let result =
        placement_model
            .model
            .solver_mut()
            .satisfy(&mut brancher, &mut termination, &mut resolver);
    let search_ms = elapsed_millis(search_started.elapsed());

    let (outcome, validation, first_witness_ms, witness, diagnostics, search_statistics) =
        match result {
            SatisfactionResult::Satisfiable(satisfiable) => {
                let solution = satisfiable.solution();
                let extracted = extract_witness(&solution, &placement_model.instances);
                let validation_diagnostics = validate_witness(&input, &extracted);
                let validation = if validation_diagnostics.is_empty() {
                    ExactValidationStatus::Passed
                } else {
                    ExactValidationStatus::Failed
                };
                let outcome = if validation == ExactValidationStatus::Passed {
                    BottomUpRungOutcome::Feasible
                } else {
                    BottomUpRungOutcome::InvalidWitness
                };
                let statistics = capture_search_statistics(
                    satisfiable.solver(),
                    satisfiable.brancher(),
                    satisfiable.conflict_resolver(),
                    &search_event_counters,
                );
                (
                    outcome,
                    validation,
                    Some(search_ms),
                    Some(extracted),
                    validation_diagnostics,
                    statistics,
                )
            }
            SatisfactionResult::Unsatisfiable(solver, brancher, resolver) => (
                BottomUpRungOutcome::Infeasible,
                ExactValidationStatus::NotAttempted,
                None,
                None,
                Vec::new(),
                capture_search_statistics(solver, brancher, resolver, &search_event_counters),
            ),
            SatisfactionResult::Unknown(solver, brancher, resolver) => (
                BottomUpRungOutcome::Unknown,
                ExactValidationStatus::NotAttempted,
                None,
                None,
                Vec::new(),
                capture_search_statistics(solver, brancher, resolver, &search_event_counters),
            ),
        };

    BottomUpRungReport {
        schema_version: BOTTOM_UP_RUNG_SCHEMA_VERSION,
        rung: BottomUpRungKind::FacilityGeometry,
        formulation: "coordinate-rotation-disjunctive-non-overlap-v1",
        ceiling,
        facility_count,
        semantic_certificate: facility_geometry_certificate(),
        construction_ms,
        search_ms,
        first_witness_ms,
        outcome,
        validation,
        model_complexity,
        search_statistics,
        witness,
        diagnostics,
    }
}

fn facility_geometry_certificate() -> BottomUpSemanticCertificate {
    BottomUpSemanticCertificate {
        facility_geometry: true,
        facility_ports: false,
        boundary_terminals: false,
        pipe_routing: false,
        belt_routing: false,
        item_flow: false,
        logistics_components: false,
        objective: false,
        hints: false,
        transferred_learned_state: false,
    }
}

fn build_model(input: &ModelInput) -> Result<PlacementModel, IntegratedLayoutDiagnostic> {
    let mut model = RecordedModel::default();
    let tag = model.new_constraint_tag();
    let mut instances = Vec::with_capacity(input.instances.len());

    for instance in &input.instances {
        let base_width = i32::try_from(instance.definition.footprint.width)
            .expect("validated facility width fits the solver domain");
        let base_height = i32::try_from(instance.definition.footprint.height)
            .expect("validated facility height fits the solver domain");
        let mut rotations = instance.definition.allowed_rotations.clone();
        rotations.sort_unstable();
        rotations.dedup();
        let fitting_rotations = rotations
            .into_iter()
            .filter_map(|rotation| {
                let (width, height) = oriented_dimensions(base_width, base_height, rotation);
                (width <= input.width && height <= input.height)
                    .then_some((rotation, width, height))
            })
            .collect::<Vec<_>>();
        if fitting_rotations.is_empty() {
            return Err(IntegratedLayoutDiagnostic::error(
                "bottom-up-facility-does-not-fit-ceiling",
                "/ceiling",
                Some(instance.id.clone()),
                format!(
                    "facility instance '{}' has no allowed rotation that fits the {}x{} request ceiling",
                    instance.id, input.width, input.height
                ),
            ));
        }

        let x = model.new_variable(
            VariableFamily::Placement,
            0,
            input.width - 1,
            format!("facility:{}:x", instance.id),
        );
        let y = model.new_variable(
            VariableFamily::Placement,
            0,
            input.height - 1,
            format!("facility:{}:y", instance.id),
        );
        let rotation = model.new_sparse_variable(
            VariableFamily::Placement,
            fitting_rotations
                .iter()
                .map(|(rotation, _, _)| i32::try_from(*rotation).expect("rotation fits i32"))
                .collect::<Vec<_>>(),
            format!("facility:{}:rotation", instance.id),
        );
        let orientations = fitting_rotations
            .into_iter()
            .map(|(rotation_value, width, height)| {
                let selected = model.new_named_literal(
                    VariableFamily::Placement,
                    format!(
                        "facility:{}:rotation:{rotation_value}:selected",
                        instance.id
                    ),
                );
                let selected_parent = *selected.get_integer_variable().inner();
                model.post_predicate_clause(
                    ConstraintFamily::PlacementChoice,
                    &[selected_parent, rotation],
                    vec![
                        selected.get_false_predicate(),
                        rotation.equality_predicate(
                            i32::try_from(rotation_value).expect("rotation fits i32"),
                        ),
                    ],
                    tag,
                );
                model.post_implied_less_than_or_equals(
                    ConstraintFamily::PlacementChoice,
                    vec![x.scaled(1)],
                    input.width - width,
                    1,
                    selected,
                    selected_parent,
                    tag,
                );
                model.post_implied_less_than_or_equals(
                    ConstraintFamily::PlacementChoice,
                    vec![y.scaled(1)],
                    input.height - height,
                    1,
                    selected,
                    selected_parent,
                    tag,
                );
                ModelOrientation {
                    rotation: rotation_value,
                    width,
                    height,
                    selected,
                    selected_parent,
                }
            })
            .collect::<Vec<_>>();
        post_exactly_one_orientation(&mut model, &orientations, tag);

        instances.push(ModelInstance {
            id: instance.id.clone(),
            recipe: instance.recipe.clone(),
            facility: instance.facility.clone(),
            x,
            y,
            rotation,
            orientations,
        });
    }

    post_pairwise_non_overlap(&mut model, &instances, tag);
    Ok(PlacementModel { model, instances })
}

fn post_exactly_one_orientation(
    model: &mut RecordedModel,
    orientations: &[ModelOrientation],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let variables = orientations
        .iter()
        .map(|orientation| orientation.selected_parent)
        .collect::<Vec<_>>();
    model.post_predicate_clause(
        ConstraintFamily::PlacementChoice,
        &variables,
        orientations
            .iter()
            .map(|orientation| orientation.selected.get_true_predicate())
            .collect(),
        tag,
    );
    for left in 0..orientations.len() {
        for right in (left + 1)..orientations.len() {
            model.post_predicate_clause(
                ConstraintFamily::PlacementChoice,
                &[
                    orientations[left].selected_parent,
                    orientations[right].selected_parent,
                ],
                vec![
                    orientations[left].selected.get_false_predicate(),
                    orientations[right].selected.get_false_predicate(),
                ],
                tag,
            );
        }
    }
}

fn post_pairwise_non_overlap(
    model: &mut RecordedModel,
    instances: &[ModelInstance],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for left_index in 0..instances.len() {
        for right_index in (left_index + 1)..instances.len() {
            let left = &instances[left_index];
            let right = &instances[right_index];
            for left_orientation in &left.orientations {
                for right_orientation in &right.orientations {
                    let left_of = reify_separation(
                        model,
                        &format!("{}:left-of:{}", left.id, right.id),
                        vec![left.x.scaled(1), right.x.scaled(-1)],
                        -left_orientation.width,
                        tag,
                    );
                    let right_of = reify_separation(
                        model,
                        &format!("{}:right-of:{}", left.id, right.id),
                        vec![right.x.scaled(1), left.x.scaled(-1)],
                        -right_orientation.width,
                        tag,
                    );
                    let below = reify_separation(
                        model,
                        &format!("{}:below:{}", left.id, right.id),
                        vec![left.y.scaled(1), right.y.scaled(-1)],
                        -left_orientation.height,
                        tag,
                    );
                    let above = reify_separation(
                        model,
                        &format!("{}:above:{}", left.id, right.id),
                        vec![right.y.scaled(1), left.y.scaled(-1)],
                        -right_orientation.height,
                        tag,
                    );
                    let separation_literals = [left_of, right_of, below, above];
                    let mut variables = vec![
                        left_orientation.selected_parent,
                        right_orientation.selected_parent,
                    ];
                    variables.extend(separation_literals.iter().map(|(_, parent)| *parent));
                    let mut predicates = vec![
                        left_orientation.selected.get_false_predicate(),
                        right_orientation.selected.get_false_predicate(),
                    ];
                    predicates.extend(
                        separation_literals
                            .iter()
                            .map(|(literal, _)| literal.get_true_predicate()),
                    );
                    model.post_predicate_clause(
                        ConstraintFamily::FacilityNonOverlap,
                        &variables,
                        predicates,
                        tag,
                    );
                }
            }
        }
    }
}

fn reify_separation(
    model: &mut RecordedModel,
    name: &str,
    terms: Vec<pumpkin_solver::core::variables::AffineView<DomainId>>,
    rhs: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (Literal, DomainId) {
    let literal = model.new_named_literal(VariableFamily::Placement, name);
    let parent = *literal.get_integer_variable().inner();
    model.post_reified_less_than_or_equals(
        ConstraintFamily::FacilityNonOverlap,
        terms,
        rhs,
        1,
        literal,
        parent,
        tag,
    );
    (literal, parent)
}

fn extract_witness(
    solution: &impl ProblemSolution,
    instances: &[ModelInstance],
) -> FacilityGeometryWitness {
    let mut bounds = FacilityPlacementBounds {
        width: 0,
        height: 0,
    };
    let mut placements = Vec::with_capacity(instances.len());
    for instance in instances {
        let orientation = instance
            .orientations
            .iter()
            .find(|orientation| solution.get_literal_value(orientation.selected))
            .expect("exactly one orientation is selected");
        debug_assert_eq!(
            i64::from(solution.get_integer_value(instance.rotation)),
            orientation.rotation
        );
        let x = i64::from(solution.get_integer_value(instance.x));
        let y = i64::from(solution.get_integer_value(instance.y));
        let width = i64::from(orientation.width);
        let height = i64::from(orientation.height);
        bounds.width = bounds.width.max(x + width);
        bounds.height = bounds.height.max(y + height);
        placements.push(FacilityPlacement {
            instance: instance.id.clone(),
            recipe: instance.recipe.clone(),
            facility: instance.facility.clone(),
            x,
            y,
            width,
            height,
            rotation: orientation.rotation,
        });
    }
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));
    FacilityGeometryWitness { bounds, placements }
}

fn validate_witness(
    input: &ModelInput,
    witness: &FacilityGeometryWitness,
) -> Vec<IntegratedLayoutDiagnostic> {
    let mut diagnostics = Vec::new();
    let expected = input
        .instances
        .iter()
        .map(|instance| (instance.id.as_str(), instance))
        .collect::<BTreeMap<_, _>>();
    let mut seen = BTreeSet::new();
    let mut calculated_bounds = FacilityPlacementBounds {
        width: 0,
        height: 0,
    };

    for placement in &witness.placements {
        if !seen.insert(placement.instance.as_str()) {
            diagnostics.push(IntegratedLayoutDiagnostic::error(
                "bottom-up-duplicate-facility-placement",
                "/witness/placements",
                Some(placement.instance.clone()),
                "facility geometry witness contains the same instance more than once",
            ));
            continue;
        }
        let Some(instance) = expected.get(placement.instance.as_str()) else {
            diagnostics.push(IntegratedLayoutDiagnostic::error(
                "bottom-up-unexpected-facility-placement",
                "/witness/placements",
                Some(placement.instance.clone()),
                "facility geometry witness contains an instance absent from the model input",
            ));
            continue;
        };
        if placement.recipe != instance.recipe || placement.facility != instance.facility {
            diagnostics.push(IntegratedLayoutDiagnostic::error(
                "bottom-up-facility-identity-mismatch",
                "/witness/placements",
                Some(placement.instance.clone()),
                "facility geometry witness changed the instance recipe or facility identity",
            ));
        }
        if !instance
            .definition
            .allowed_rotations
            .contains(&placement.rotation)
        {
            diagnostics.push(IntegratedLayoutDiagnostic::error(
                "bottom-up-invalid-facility-rotation",
                "/witness/placements",
                Some(placement.instance.clone()),
                "facility geometry witness selected a rotation not allowed by the facility catalog",
            ));
        }
        let (expected_width, expected_height) = oriented_dimensions_i64(
            instance.definition.footprint.width,
            instance.definition.footprint.height,
            placement.rotation,
        );
        if placement.width != expected_width || placement.height != expected_height {
            diagnostics.push(IntegratedLayoutDiagnostic::error(
                "bottom-up-facility-footprint-mismatch",
                "/witness/placements",
                Some(placement.instance.clone()),
                "facility geometry witness dimensions do not match its selected rotation",
            ));
        }
        if placement.x < 0
            || placement.y < 0
            || placement.x + placement.width > i64::from(input.width)
            || placement.y + placement.height > i64::from(input.height)
        {
            diagnostics.push(IntegratedLayoutDiagnostic::error(
                "bottom-up-facility-outside-ceiling",
                "/witness/placements",
                Some(placement.instance.clone()),
                "facility geometry witness extends outside the request ceiling",
            ));
        }
        calculated_bounds.width = calculated_bounds.width.max(placement.x + placement.width);
        calculated_bounds.height = calculated_bounds.height.max(placement.y + placement.height);
    }

    for missing in expected.keys().filter(|id| !seen.contains(**id)) {
        diagnostics.push(IntegratedLayoutDiagnostic::error(
            "bottom-up-missing-facility-placement",
            "/witness/placements",
            Some((*missing).to_string()),
            "facility geometry witness omitted a modeled facility instance",
        ));
    }
    for left in 0..witness.placements.len() {
        for right in (left + 1)..witness.placements.len() {
            if rectangles_overlap(&witness.placements[left], &witness.placements[right]) {
                diagnostics.push(IntegratedLayoutDiagnostic::error(
                    "bottom-up-overlapping-facilities",
                    "/witness/placements",
                    Some(format!(
                        "{}:{}",
                        witness.placements[left].instance, witness.placements[right].instance
                    )),
                    "facility geometry witness contains overlapping facility footprints",
                ));
            }
        }
    }
    if witness.bounds != calculated_bounds {
        diagnostics.push(IntegratedLayoutDiagnostic::error(
            "bottom-up-facility-bounds-mismatch",
            "/witness/bounds",
            None,
            "facility geometry witness bounds do not equal the bounds of its placements",
        ));
    }
    diagnostics
}

fn oriented_dimensions(width: i32, height: i32, rotation: i64) -> (i32, i32) {
    if matches!(rotation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    }
}

fn oriented_dimensions_i64(width: i64, height: i64, rotation: i64) -> (i64, i64) {
    if matches!(rotation, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    }
}

fn rectangles_overlap(left: &FacilityPlacement, right: &FacilityPlacement) -> bool {
    left.x < right.x + right.width
        && right.x < left.x + left.width
        && left.y < right.y + right.height
        && right.y < left.y + left.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityDefinition, FacilityFootprint};

    fn instance(
        id: &str,
        width: i64,
        height: i64,
        rotations: Vec<i64>,
    ) -> super::super::InstanceInput {
        super::super::InstanceInput {
            id: id.to_string(),
            recipe: format!("recipe-{id}"),
            facility: format!("facility-{id}"),
            definition: FacilityDefinition {
                id: format!("facility-{id}"),
                footprint: FacilityFootprint { width, height },
                allowed_rotations: rotations,
                ports: Vec::new(),
            },
        }
    }

    fn input(width: i32, height: i32, instances: Vec<super::super::InstanceInput>) -> ModelInput {
        ModelInput {
            width,
            height,
            cell_count: width * height,
            instances,
            edges: Vec::new(),
            networks: Vec::new(),
        }
    }

    #[test]
    fn solves_non_overlapping_facility_geometry_without_future_state() {
        let report = solve_facility_geometry_rung(
            input(
                4,
                2,
                vec![instance("a", 2, 2, vec![0]), instance("b", 2, 2, vec![0])],
            ),
            Duration::from_secs(1),
        );

        assert_eq!(report.outcome, BottomUpRungOutcome::Feasible);
        assert_eq!(report.validation, ExactValidationStatus::Passed);
        assert_eq!(report.witness.as_ref().unwrap().placements.len(), 2);
        assert!(report.semantic_certificate.facility_geometry);
        assert!(!report.semantic_certificate.facility_ports);
        assert!(!report.semantic_certificate.pipe_routing);
        assert!(!report.semantic_certificate.belt_routing);
        assert!(!report.semantic_certificate.objective);
        assert!(!report.semantic_certificate.hints);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn proves_that_two_large_facilities_do_not_fit() {
        let report = solve_facility_geometry_rung(
            input(
                3,
                2,
                vec![instance("a", 2, 2, vec![0]), instance("b", 2, 2, vec![0])],
            ),
            Duration::from_secs(1),
        );

        assert_eq!(report.outcome, BottomUpRungOutcome::Infeasible);
        assert_eq!(report.validation, ExactValidationStatus::NotAttempted);
        assert!(report.witness.is_none());
    }

    #[test]
    fn keeps_rotation_as_a_solver_decision() {
        let report = solve_facility_geometry_rung(
            input(2, 3, vec![instance("a", 3, 2, vec![0, 90])]),
            Duration::from_secs(1),
        );

        assert_eq!(report.outcome, BottomUpRungOutcome::Feasible);
        let placement = &report.witness.as_ref().unwrap().placements[0];
        assert_eq!(placement.rotation, 90);
        assert_eq!([placement.width, placement.height], [2, 3]);
    }

    #[test]
    fn reports_a_facility_that_has_no_fitting_rotation() {
        let report = solve_facility_geometry_rung(
            input(2, 2, vec![instance("a", 3, 2, vec![0])]),
            Duration::from_secs(1),
        );

        assert_eq!(report.outcome, BottomUpRungOutcome::Infeasible);
        assert_eq!(report.search_ms, 0);
        assert_eq!(
            report.diagnostics[0].code,
            "bottom-up-facility-does-not-fit-ceiling"
        );
    }
}
