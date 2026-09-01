use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::results::{ProblemSolution, SatisfactionResult};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, Literal, TransformableVariable};
use serde::Serialize;

use super::super::{ExactSearchStatistics, ExactValidationStatus};
use super::metrics::elapsed_millis;
use super::recorder::{ConstraintFamily, RecordedModel, VariableFamily};
use super::search_statistics::{MeteredBrancher, SearchEventCounters, capture_search_statistics};
use super::{IntegratedLayoutDiagnostic, ModelInput};
use crate::layouts::FacilityPlacementBounds;
use crate::logistics::{CardinalDirection, TransportKind};
use crate::research::ModelComplexityMetrics;

use super::super::research::EndpointSupportPropagationStatistics;

mod facility_ports;

pub const BOTTOM_UP_RUNG_SCHEMA_VERSION: u32 = 8;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BottomUpRungKind {
    FacilityGeometry,
    FacilityPortGeometry,
    FacilityPorts,
    FacilityPortsPropagated,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum EndpointClearanceSchedulingPriority {
    High,
    Medium,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct BottomUpSearchProfile {
    pub endpoint_clearance_priority: Option<EndpointClearanceSchedulingPriority>,
    pub endpoint_clearance_counters_enabled: Option<bool>,
    pub endpoint_clearance_false_event_filter_enabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct EndpointClearancePropagationStatistics {
    pub relations: u64,
    pub executions: u64,
    pub notifications: u64,
    pub coordinate_notifications: u64,
    pub orientation_notifications: u64,
    pub skipped_false_orientation_notifications: u64,
    pub enqueued_notifications: u64,
    pub orientation_checks: u64,
    pub rejected_orientations: u64,
    pub forced_separation_detections: u64,
    pub bound_updates: u64,
    pub conflicts: u64,
    pub maximum_reason_predicates: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BottomUpRungOutcome {
    Feasible,
    Infeasible,
    Unknown,
    InvalidWitness,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BottomUpTerminationReason {
    FirstWitness,
    ProvenInfeasible,
    TimeLimit,
    InvalidWitness,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BottomUpSemanticCertificate {
    pub facility_geometry: bool,
    pub facility_ports: bool,
    pub facility_endpoint_clearance: bool,
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
    pub placements: Vec<FacilityGeometryPlacement>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityGeometryPlacement {
    pub instance: String,
    pub recipe: String,
    pub facility: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub representative_rotation: i64,
    pub equivalent_rotations: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BottomUpRungWitness {
    FacilityGeometry { witness: FacilityGeometryWitness },
    FacilityPorts { witness: FacilityPortsWitness },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityPortsWitness {
    pub bounds: FacilityPlacementBounds,
    pub placements: Vec<FacilityPortPlacement>,
    pub endpoints: Vec<FacilityEndpointPlacement>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityPortPlacement {
    pub instance: String,
    pub recipe: String,
    pub facility: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub rotation: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityEndpointPlacement {
    pub terminal: String,
    pub instance: String,
    pub port: String,
    pub direction: crate::facilities::FacilityPortDirection,
    pub transport: TransportKind,
    pub connection_x: i64,
    pub connection_y: i64,
    pub arm_direction: CardinalDirection,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpSearchSpaceProfile {
    /// Cartesian product of every independent semantic choice enabled by this rung.
    /// Cross-entity hard constraints are deliberately not applied, so this is an upper bound on
    /// legal witnesses rather than a feasible-assignment count.
    pub semantic_assignment_upper_bound_log2: Option<f64>,
    pub semantic_assignment_upper_bound_log10: Option<f64>,
    /// Facility origins multiplied by every fitting full directional rotation, before any Rung 0
    /// occupied-rectangle projection and before port choices from later rungs.
    pub directional_rotation_upper_bound_log2: Option<f64>,
    pub directional_rotation_upper_bound_log10: Option<f64>,
    /// Exact quotient between full directional facility placement and the Rung 0
    /// occupied-rectangle projection, expressed as an equivalent number of binary choices. Later
    /// rungs report zero because directional rotation is observable and no projection is applied.
    pub rotation_equivalence_reduction_log2: Option<f64>,
    /// Independent compatible-port choices. `None` means this rung does not model ports.
    pub facility_port_choice_upper_bound_log2: Option<f64>,
    pub facility_port_choice_upper_bound_log10: Option<f64>,
    pub facility_port_domain_histogram: Option<BTreeMap<usize, usize>>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpRungReport {
    pub schema_version: u32,
    pub rung: BottomUpRungKind,
    pub formulation: &'static str,
    pub search_profile: BottomUpSearchProfile,
    pub ceiling: [i32; 2],
    pub facility_count: usize,
    pub facility_terminal_count: usize,
    pub facility_terminal_ids: Vec<String>,
    pub semantic_certificate: BottomUpSemanticCertificate,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_witness_ms: Option<u64>,
    pub outcome: BottomUpRungOutcome,
    pub termination_reason: BottomUpTerminationReason,
    pub witness_count: u32,
    pub validation: ExactValidationStatus,
    pub search_space: BottomUpSearchSpaceProfile,
    pub model_complexity: ModelComplexityMetrics,
    pub search_statistics: ExactSearchStatistics,
    pub endpoint_support_statistics: Option<EndpointSupportPropagationStatistics>,
    pub endpoint_clearance_statistics: Option<EndpointClearancePropagationStatistics>,
    pub witness: Option<BottomUpRungWitness>,
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
    orientations: Vec<ModelOrientation>,
}

struct ModelOrientation {
    width: i32,
    height: i32,
    equivalent_rotations: Vec<i64>,
    selected: Literal,
    selected_parent: DomainId,
}

pub(in crate::layouts::integrated) fn solve_facility_geometry_rung(
    input: ModelInput,
    time_limit: Duration,
) -> BottomUpRungReport {
    let ceiling = [input.width, input.height];
    let facility_count = input.instances.len();
    let search_space = facility_geometry_search_space_profile(&input);
    let construction_started = Instant::now();
    let mut placement_model = match build_model(&input) {
        Ok(model) => model,
        Err(diagnostic) => {
            return BottomUpRungReport {
                schema_version: BOTTOM_UP_RUNG_SCHEMA_VERSION,
                rung: BottomUpRungKind::FacilityGeometry,
                formulation: "coordinate-geometry-orientation-disjunctive-non-overlap-v2",
                search_profile: BottomUpSearchProfile::default(),
                ceiling,
                facility_count,
                facility_terminal_count: 0,
                facility_terminal_ids: Vec::new(),
                semantic_certificate: facility_geometry_certificate(),
                construction_ms: elapsed_millis(construction_started.elapsed()),
                search_ms: 0,
                first_witness_ms: None,
                outcome: BottomUpRungOutcome::Infeasible,
                termination_reason: BottomUpTerminationReason::ProvenInfeasible,
                witness_count: 0,
                validation: ExactValidationStatus::NotAttempted,
                search_space,
                model_complexity: ModelComplexityMetrics::unavailable(),
                search_statistics: ExactSearchStatistics::default(),
                endpoint_support_statistics: None,
                endpoint_clearance_statistics: None,
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

    let (
        outcome,
        termination_reason,
        validation,
        first_witness_ms,
        witness,
        diagnostics,
        search_statistics,
    ) = match result {
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
                if validation == ExactValidationStatus::Passed {
                    BottomUpTerminationReason::FirstWitness
                } else {
                    BottomUpTerminationReason::InvalidWitness
                },
                validation,
                Some(search_ms),
                Some(BottomUpRungWitness::FacilityGeometry { witness: extracted }),
                validation_diagnostics,
                statistics,
            )
        }
        SatisfactionResult::Unsatisfiable(solver, brancher, resolver) => (
            BottomUpRungOutcome::Infeasible,
            BottomUpTerminationReason::ProvenInfeasible,
            ExactValidationStatus::NotAttempted,
            None,
            None,
            Vec::new(),
            capture_search_statistics(solver, brancher, resolver, &search_event_counters),
        ),
        SatisfactionResult::Unknown(solver, brancher, resolver) => (
            BottomUpRungOutcome::Unknown,
            BottomUpTerminationReason::TimeLimit,
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
        formulation: "coordinate-geometry-orientation-disjunctive-non-overlap-v2",
        search_profile: BottomUpSearchProfile::default(),
        ceiling,
        facility_count,
        facility_terminal_count: 0,
        facility_terminal_ids: Vec::new(),
        semantic_certificate: facility_geometry_certificate(),
        construction_ms,
        search_ms,
        first_witness_ms,
        outcome,
        termination_reason,
        witness_count: u32::from(witness.is_some()),
        validation,
        search_space,
        model_complexity,
        search_statistics,
        endpoint_support_statistics: None,
        endpoint_clearance_statistics: None,
        witness,
        diagnostics,
    }
}

pub(in crate::layouts::integrated) fn solve_facility_ports_rung(
    input: ModelInput,
    time_limit: Duration,
) -> BottomUpRungReport {
    facility_ports::solve_with_clearance(input, time_limit)
}

pub(in crate::layouts::integrated) fn solve_facility_ports_propagated_rung(
    input: ModelInput,
    time_limit: Duration,
    priority: EndpointClearanceSchedulingPriority,
    counters_enabled: bool,
    false_event_filter_enabled: bool,
) -> BottomUpRungReport {
    facility_ports::solve_with_propagated_clearance(
        input,
        time_limit,
        priority,
        counters_enabled,
        false_event_filter_enabled,
    )
}

pub(in crate::layouts::integrated) fn solve_facility_ports_propagated_rung_with_fixed_rotations(
    input: ModelInput,
    time_limit: Duration,
    priority: EndpointClearanceSchedulingPriority,
    counters_enabled: bool,
    false_event_filter_enabled: bool,
    fixed_rotations: &BTreeMap<String, i64>,
) -> BottomUpRungReport {
    facility_ports::solve_with_propagated_clearance_and_fixed_rotations(
        input,
        time_limit,
        priority,
        counters_enabled,
        false_event_filter_enabled,
        fixed_rotations,
    )
}

pub(in crate::layouts::integrated) fn solve_facility_port_geometry_rung(
    input: ModelInput,
    time_limit: Duration,
) -> BottomUpRungReport {
    facility_ports::solve_geometry(input, time_limit)
}

fn facility_geometry_search_space_profile(input: &ModelInput) -> BottomUpSearchSpaceProfile {
    let mut semantic_log2 = 0.0;
    let mut directional_log2 = 0.0;
    let mut empty = false;

    for instance in &input.instances {
        let base_width = i32::try_from(instance.definition.footprint.width)
            .expect("validated facility width fits i32");
        let base_height = i32::try_from(instance.definition.footprint.height)
            .expect("validated facility height fits i32");
        let mut rotations = instance.definition.allowed_rotations.clone();
        rotations.sort_unstable();
        rotations.dedup();

        let mut geometries = BTreeSet::new();
        let mut directional_assignments = 0_u64;
        for rotation in rotations {
            let (width, height) = oriented_dimensions(base_width, base_height, rotation);
            if width <= input.width && height <= input.height {
                geometries.insert((width, height));
                directional_assignments += legal_origin_count(input, width, height);
            }
        }
        let semantic_assignments = geometries
            .into_iter()
            .map(|(width, height)| legal_origin_count(input, width, height))
            .sum::<u64>();

        if semantic_assignments == 0 || directional_assignments == 0 {
            empty = true;
        } else {
            semantic_log2 += (semantic_assignments as f64).log2();
            directional_log2 += (directional_assignments as f64).log2();
        }
    }

    if empty {
        return BottomUpSearchSpaceProfile {
            semantic_assignment_upper_bound_log2: None,
            semantic_assignment_upper_bound_log10: None,
            directional_rotation_upper_bound_log2: None,
            directional_rotation_upper_bound_log10: None,
            rotation_equivalence_reduction_log2: None,
            facility_port_choice_upper_bound_log2: None,
            facility_port_choice_upper_bound_log10: None,
            facility_port_domain_histogram: None,
        };
    }

    BottomUpSearchSpaceProfile {
        semantic_assignment_upper_bound_log2: Some(semantic_log2),
        semantic_assignment_upper_bound_log10: Some(semantic_log2 * std::f64::consts::LOG10_2),
        directional_rotation_upper_bound_log2: Some(directional_log2),
        directional_rotation_upper_bound_log10: Some(directional_log2 * std::f64::consts::LOG10_2),
        rotation_equivalence_reduction_log2: Some(directional_log2 - semantic_log2),
        facility_port_choice_upper_bound_log2: None,
        facility_port_choice_upper_bound_log10: None,
        facility_port_domain_histogram: None,
    }
}

fn legal_origin_count(input: &ModelInput, width: i32, height: i32) -> u64 {
    let x_count = u64::try_from(input.width - width + 1).expect("fitting width has legal origins");
    let y_count =
        u64::try_from(input.height - height + 1).expect("fitting height has legal origins");
    x_count * y_count
}

fn facility_geometry_certificate() -> BottomUpSemanticCertificate {
    BottomUpSemanticCertificate {
        facility_geometry: true,
        facility_ports: false,
        facility_endpoint_clearance: false,
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
        let mut rotations_by_geometry = BTreeMap::<(i32, i32), Vec<i64>>::new();
        for rotation in rotations {
            let geometry = oriented_dimensions(base_width, base_height, rotation);
            if geometry.0 <= input.width && geometry.1 <= input.height {
                rotations_by_geometry
                    .entry(geometry)
                    .or_default()
                    .push(rotation);
            }
        }
        if rotations_by_geometry.is_empty() {
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
        let orientations = rotations_by_geometry
            .into_iter()
            .map(|((width, height), equivalent_rotations)| {
                let selected = model.new_named_literal(
                    VariableFamily::Placement,
                    format!(
                        "facility:{}:geometry:{width}x{height}:selected",
                        instance.id,
                    ),
                );
                let selected_parent = *selected.get_integer_variable().inner();
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
                    width,
                    height,
                    equivalent_rotations,
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
    let mut placements = Vec::with_capacity(instances.len());
    for instance in instances {
        let orientation = instance
            .orientations
            .iter()
            .find(|orientation| solution.get_literal_value(orientation.selected))
            .expect("exactly one orientation is selected");
        let x = i64::from(solution.get_integer_value(instance.x));
        let y = i64::from(solution.get_integer_value(instance.y));
        let width = i64::from(orientation.width);
        let height = i64::from(orientation.height);
        placements.push(FacilityGeometryPlacement {
            instance: instance.id.clone(),
            recipe: instance.recipe.clone(),
            facility: instance.facility.clone(),
            x,
            y,
            width,
            height,
            representative_rotation: orientation.equivalent_rotations[0],
            equivalent_rotations: orientation.equivalent_rotations.clone(),
        });
    }
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));
    let bounds = facility_geometry_bounds(&placements);
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
        let mut expected_equivalent_rotations = instance
            .definition
            .allowed_rotations
            .iter()
            .copied()
            .filter(|rotation| {
                oriented_dimensions_i64(
                    instance.definition.footprint.width,
                    instance.definition.footprint.height,
                    *rotation,
                ) == (placement.width, placement.height)
            })
            .collect::<Vec<_>>();
        expected_equivalent_rotations.sort_unstable();
        expected_equivalent_rotations.dedup();
        if expected_equivalent_rotations.is_empty()
            || placement.equivalent_rotations != expected_equivalent_rotations
            || !placement
                .equivalent_rotations
                .contains(&placement.representative_rotation)
        {
            diagnostics.push(IntegratedLayoutDiagnostic::error(
                "bottom-up-invalid-facility-geometry-orientation",
                "/witness/placements",
                Some(placement.instance.clone()),
                "facility geometry witness does not contain the exact allowed rotations for its occupied rectangle",
            ));
        }
        let (expected_width, expected_height) = oriented_dimensions_i64(
            instance.definition.footprint.width,
            instance.definition.footprint.height,
            placement.representative_rotation,
        );
        if placement.width != expected_width || placement.height != expected_height {
            diagnostics.push(IntegratedLayoutDiagnostic::error(
                "bottom-up-facility-footprint-mismatch",
                "/witness/placements",
                Some(placement.instance.clone()),
                "facility geometry witness dimensions do not match its representative rotation",
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
    if witness.bounds != facility_geometry_bounds(&witness.placements) {
        diagnostics.push(IntegratedLayoutDiagnostic::error(
            "bottom-up-facility-bounds-mismatch",
            "/witness/bounds",
            None,
            "facility geometry witness bounds do not equal the bounds of its placements",
        ));
    }
    diagnostics
}

fn facility_geometry_bounds(placements: &[FacilityGeometryPlacement]) -> FacilityPlacementBounds {
    let Some(first) = placements.first() else {
        return FacilityPlacementBounds {
            width: 0,
            height: 0,
        };
    };
    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + first.width;
    let mut max_y = first.y + first.height;
    for placement in placements.iter().skip(1) {
        min_x = min_x.min(placement.x);
        min_y = min_y.min(placement.y);
        max_x = max_x.max(placement.x + placement.width);
        max_y = max_y.max(placement.y + placement.height);
    }
    FacilityPlacementBounds {
        width: max_x - min_x,
        height: max_y - min_y,
    }
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

fn rectangles_overlap(left: &FacilityGeometryPlacement, right: &FacilityGeometryPlacement) -> bool {
    left.x < right.x + right.width
        && right.x < left.x + left.width
        && left.y < right.y + right.height
        && right.y < left.y + left.height
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityDefinition, FacilityFootprint};

    #[derive(Clone, Copy)]
    struct BrutePlacement {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    }

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

    fn geometry_witness(report: &BottomUpRungReport) -> &FacilityGeometryWitness {
        match report.witness.as_ref().unwrap() {
            BottomUpRungWitness::FacilityGeometry { witness } => witness,
            BottomUpRungWitness::FacilityPorts { .. } => panic!("expected facility geometry"),
        }
    }

    fn brute_directional_geometry_feasible(input: &ModelInput) -> bool {
        let candidates = input
            .instances
            .iter()
            .map(|instance| {
                instance
                    .definition
                    .allowed_rotations
                    .iter()
                    .flat_map(|rotation| {
                        let (width, height) = oriented_dimensions(
                            i32::try_from(instance.definition.footprint.width).unwrap(),
                            i32::try_from(instance.definition.footprint.height).unwrap(),
                            *rotation,
                        );
                        (0..=(input.width - width).max(-1)).flat_map(move |x| {
                            (0..=(input.height - height).max(-1)).map(move |y| BrutePlacement {
                                x,
                                y,
                                width,
                                height,
                            })
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        fn choose(candidates: &[Vec<BrutePlacement>], selected: &mut Vec<BrutePlacement>) -> bool {
            let Some(next) = candidates.get(selected.len()) else {
                return true;
            };
            for candidate in next {
                let overlaps = selected.iter().any(|other| {
                    candidate.x < other.x + other.width
                        && other.x < candidate.x + candidate.width
                        && candidate.y < other.y + other.height
                        && other.y < candidate.y + candidate.height
                });
                if !overlaps {
                    selected.push(*candidate);
                    if choose(candidates, selected) {
                        return true;
                    }
                    selected.pop();
                }
            }
            false
        }
        choose(&candidates, &mut Vec::new())
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
        assert_eq!(geometry_witness(&report).placements.len(), 2);
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
    fn keeps_distinct_footprint_orientations_as_solver_decisions() {
        let report = solve_facility_geometry_rung(
            input(2, 3, vec![instance("a", 3, 2, vec![0, 90])]),
            Duration::from_secs(1),
        );

        assert_eq!(report.outcome, BottomUpRungOutcome::Feasible);
        let placement = &geometry_witness(&report).placements[0];
        assert_eq!(placement.representative_rotation, 90);
        assert_eq!(placement.equivalent_rotations, vec![90]);
        assert_eq!([placement.width, placement.height], [2, 3]);
    }

    #[test]
    fn collapses_rotations_with_identical_geometry() {
        let report = solve_facility_geometry_rung(
            input(3, 3, vec![instance("a", 2, 2, vec![0, 90, 180, 270])]),
            Duration::from_secs(1),
        );

        assert_eq!(report.outcome, BottomUpRungOutcome::Feasible);
        let placement = &geometry_witness(&report).placements[0];
        assert_eq!(placement.representative_rotation, 0);
        assert_eq!(placement.equivalent_rotations, vec![0, 90, 180, 270]);
        assert_eq!(report.model_complexity.variables.total_variables, 3);
    }

    #[test]
    fn reports_semantic_assignment_volume_before_non_overlap() {
        let profile = facility_geometry_search_space_profile(&input(
            3,
            3,
            vec![
                instance("square", 2, 2, vec![0, 90, 180, 270]),
                instance("rectangle", 2, 1, vec![0, 90, 180, 270]),
            ],
        ));

        assert!(
            (profile.semantic_assignment_upper_bound_log2.unwrap() - 48_f64.log2()).abs() < 1e-9
        );
        assert!(
            (profile.directional_rotation_upper_bound_log2.unwrap() - 384_f64.log2()).abs() < 1e-9
        );
        assert!((profile.rotation_equivalence_reduction_log2.unwrap() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn groups_rectangle_rotations_into_two_geometry_classes() {
        let model = build_model(&input(
            4,
            4,
            vec![instance("rectangle", 3, 2, vec![0, 90, 180, 270])],
        ))
        .unwrap();
        let orientations = &model.instances[0].orientations;

        assert_eq!(orientations.len(), 2);
        assert_eq!([orientations[0].width, orientations[0].height], [2, 3]);
        assert_eq!(orientations[0].equivalent_rotations, vec![90, 270]);
        assert_eq!([orientations[1].width, orientations[1].height], [3, 2]);
        assert_eq!(orientations[1].equivalent_rotations, vec![0, 180]);
    }

    #[test]
    fn retains_all_directional_rotations_of_the_only_fitting_geometry() {
        let model = build_model(&input(
            2,
            3,
            vec![instance("rectangle", 3, 2, vec![0, 90, 180, 270])],
        ))
        .unwrap();
        let orientations = &model.instances[0].orientations;

        assert_eq!(orientations.len(), 1);
        assert_eq!([orientations[0].width, orientations[0].height], [2, 3]);
        assert_eq!(orientations[0].equivalent_rotations, vec![90, 270]);
    }

    #[test]
    fn reports_used_bounds_independently_of_canvas_translation() {
        let placements = vec![
            FacilityGeometryPlacement {
                instance: "a".to_string(),
                recipe: "recipe-a".to_string(),
                facility: "facility-a".to_string(),
                x: 7,
                y: 11,
                width: 3,
                height: 2,
                representative_rotation: 0,
                equivalent_rotations: vec![0],
            },
            FacilityGeometryPlacement {
                instance: "b".to_string(),
                recipe: "recipe-b".to_string(),
                facility: "facility-b".to_string(),
                x: 12,
                y: 14,
                width: 2,
                height: 4,
                representative_rotation: 0,
                equivalent_rotations: vec![0],
            },
        ];

        assert_eq!(
            facility_geometry_bounds(&placements),
            FacilityPlacementBounds {
                width: 7,
                height: 7,
            }
        );
    }

    #[test]
    fn projected_satisfiability_matches_directional_brute_force_on_small_canvases() {
        for width in 1..=4 {
            for height in 1..=4 {
                let case = input(
                    width,
                    height,
                    vec![
                        instance("rectangle", 2, 1, vec![0, 90, 180, 270]),
                        instance("square", 2, 2, vec![0, 90, 180, 270]),
                    ],
                );
                let expected = brute_directional_geometry_feasible(&case);
                let report = solve_facility_geometry_rung(case, Duration::from_secs(1));

                assert_eq!(
                    report.outcome == BottomUpRungOutcome::Feasible,
                    expected,
                    "projection mismatch on {width}x{height} canvas"
                );
            }
        }
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
            report.search_space.semantic_assignment_upper_bound_log2,
            None
        );
        assert_eq!(
            report.search_space.directional_rotation_upper_bound_log2,
            None
        );
        assert_eq!(
            report.diagnostics[0].code,
            "bottom-up-facility-does-not-fit-ceiling"
        );
    }
}
