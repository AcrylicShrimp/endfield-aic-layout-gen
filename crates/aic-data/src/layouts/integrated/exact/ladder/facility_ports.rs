use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::results::{ProblemSolution, SatisfactionResult};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, Literal, TransformableVariable};

use super::super::endpoint_support_propagator::{
    EndpointSupportPropagationCounters, SparseEndpointSupportPropagatorArgs,
};
use super::super::metrics::elapsed_millis;
use super::super::recorder::{ConstraintFamily, VariableFamily};
use super::super::search_statistics::{
    MeteredBrancher, SearchEventCounters, capture_search_statistics,
};
use super::{
    BOTTOM_UP_RUNG_SCHEMA_VERSION, BottomUpRungKind, BottomUpRungOutcome, BottomUpRungReport,
    BottomUpRungWitness, BottomUpSearchSpaceProfile, BottomUpSemanticCertificate,
    BottomUpTerminationReason, FacilityEndpointPlacement, FacilityPortPlacement,
    FacilityPortsWitness, ModelInstance, PlacementModel, build_model,
    facility_geometry_search_space_profile, oriented_dimensions_i64,
};
use crate::facilities::{FacilityPortDefinition, FacilityPortDirection, FacilityPortEdge};
use crate::layouts::FacilityPlacementBounds;
use crate::layouts::integrated::geometry::rotate_port;
use crate::layouts::integrated::{
    EndpointInput, ExactSearchStatistics, ExactValidationStatus, IntegratedLayoutDiagnostic,
    ModelInput,
};
use crate::logistics::{CardinalDirection, TransportKind};
use crate::research::ModelComplexityMetrics;

const FORMULATION: &str = "factorized-coordinate-geometry-rotation-port-support-v1";

struct PortModel {
    placement: PlacementModel,
    rotations: BTreeMap<String, DomainId>,
    endpoints: Vec<ModelEndpoint>,
    support_counters: Arc<EndpointSupportPropagationCounters>,
}

struct ModelEndpoint {
    terminal: String,
    instance: String,
    direction: FacilityPortDirection,
    transport: TransportKind,
    ports: Vec<FacilityPortDefinition>,
    port_choice: DomainId,
    local_key: DomainId,
    local_connections: Vec<LocalConnection>,
    connection_x: DomainId,
    connection_y: DomainId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct LocalConnection {
    dx: i32,
    dy: i32,
    arm_direction: CardinalDirection,
}

pub(super) fn solve(input: ModelInput, time_limit: Duration) -> BottomUpRungReport {
    let ceiling = [input.width, input.height];
    let facility_count = input.instances.len();
    let endpoint_descriptors = facility_endpoint_descriptors(&input);
    let facility_terminal_count = endpoint_descriptors.len();
    let facility_terminal_ids = endpoint_descriptors
        .iter()
        .map(|descriptor| descriptor.0.clone())
        .collect::<Vec<_>>();
    let search_space = search_space_profile(&input);
    let construction_started = Instant::now();
    let mut port_model = match build_port_model(&input) {
        Ok(model) => model,
        Err(diagnostic) => {
            return BottomUpRungReport {
                schema_version: BOTTOM_UP_RUNG_SCHEMA_VERSION,
                rung: BottomUpRungKind::FacilityPorts,
                formulation: FORMULATION,
                ceiling,
                facility_count,
                facility_terminal_count,
                facility_terminal_ids,
                semantic_certificate: semantic_certificate(),
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
                witness: None,
                diagnostics: vec![diagnostic],
            };
        }
    };
    let construction_ms = elapsed_millis(construction_started.elapsed());
    let model_complexity = port_model.placement.model.metrics();

    let search_started = Instant::now();
    let search_event_counters = Arc::new(Mutex::new(SearchEventCounters::default()));
    let default_brancher = port_model.placement.model.solver_mut().default_brancher();
    let mut brancher = MeteredBrancher::new(default_brancher, Arc::clone(&search_event_counters));
    let mut resolver = ResolutionResolver::default();
    let mut termination = TimeBudget::starting_now(time_limit);
    let result = port_model.placement.model.solver_mut().satisfy(
        &mut brancher,
        &mut termination,
        &mut resolver,
    );
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
            let extracted = extract_witness(
                &solution,
                &port_model.placement.instances,
                &port_model.rotations,
                &port_model.endpoints,
            );
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
                Some(BottomUpRungWitness::FacilityPorts { witness: extracted }),
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
        rung: BottomUpRungKind::FacilityPorts,
        formulation: FORMULATION,
        ceiling,
        facility_count,
        facility_terminal_count,
        facility_terminal_ids,
        semantic_certificate: semantic_certificate(),
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
        endpoint_support_statistics: Some(port_model.support_counters.snapshot()),
        witness,
        diagnostics,
    }
}

fn semantic_certificate() -> BottomUpSemanticCertificate {
    BottomUpSemanticCertificate {
        facility_geometry: true,
        facility_ports: true,
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

fn search_space_profile(input: &ModelInput) -> BottomUpSearchSpaceProfile {
    let geometry = facility_geometry_search_space_profile(input);
    let mut port_log2 = 0.0;
    let mut empty = false;
    let mut port_domain_histogram = BTreeMap::new();
    for (_, _, _, _, ports) in facility_endpoint_descriptors(input) {
        if ports.is_empty() {
            empty = true;
        } else {
            port_log2 += (ports.len() as f64).log2();
            *port_domain_histogram.entry(ports.len()).or_insert(0) += 1;
        }
    }
    let directional = geometry.directional_rotation_upper_bound_log2;
    if empty || directional.is_none() {
        return BottomUpSearchSpaceProfile {
            semantic_assignment_upper_bound_log2: None,
            semantic_assignment_upper_bound_log10: None,
            directional_rotation_upper_bound_log2: directional,
            directional_rotation_upper_bound_log10: geometry.directional_rotation_upper_bound_log10,
            rotation_equivalence_reduction_log2: None,
            facility_port_choice_upper_bound_log2: None,
            facility_port_choice_upper_bound_log10: None,
            facility_port_domain_histogram: Some(port_domain_histogram),
        };
    }
    let total_log2 = directional.expect("checked directional volume") + port_log2;
    BottomUpSearchSpaceProfile {
        semantic_assignment_upper_bound_log2: Some(total_log2),
        semantic_assignment_upper_bound_log10: Some(total_log2 * std::f64::consts::LOG10_2),
        directional_rotation_upper_bound_log2: directional,
        directional_rotation_upper_bound_log10: geometry.directional_rotation_upper_bound_log10,
        rotation_equivalence_reduction_log2: Some(0.0),
        facility_port_choice_upper_bound_log2: Some(port_log2),
        facility_port_choice_upper_bound_log10: Some(port_log2 * std::f64::consts::LOG10_2),
        facility_port_domain_histogram: Some(port_domain_histogram),
    }
}

fn build_port_model(input: &ModelInput) -> Result<PortModel, IntegratedLayoutDiagnostic> {
    let mut placement = build_model(input)?;
    let tag = placement.model.new_constraint_tag();
    let rotations = build_rotation_channels(&mut placement, tag);
    let support_counters = Arc::new(EndpointSupportPropagationCounters::default());
    let endpoints = build_endpoints(
        &mut placement,
        input,
        &rotations,
        Arc::clone(&support_counters),
        tag,
    )?;
    Ok(PortModel {
        placement,
        rotations,
        endpoints,
        support_counters,
    })
}

fn build_rotation_channels(
    placement: &mut PlacementModel,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> BTreeMap<String, DomainId> {
    let mut rotations = BTreeMap::new();
    for instance in &placement.instances {
        let mut values = instance
            .orientations
            .iter()
            .flat_map(|orientation| orientation.equivalent_rotations.iter().copied())
            .map(|rotation| i32::try_from(rotation).expect("validated rotation fits i32"))
            .collect::<Vec<_>>();
        values.sort_unstable();
        values.dedup();
        let rotation = placement.model.new_sparse_variable(
            VariableFamily::Placement,
            values,
            format!("facility:{}:directional-rotation", instance.id),
        );
        for orientation in &instance.orientations {
            let mut variables = vec![orientation.selected_parent];
            variables.extend(orientation.equivalent_rotations.iter().map(|_| rotation));
            let mut predicates = vec![orientation.selected.get_false_predicate()];
            predicates.extend(orientation.equivalent_rotations.iter().map(|value| {
                rotation
                    .equality_predicate(i32::try_from(*value).expect("validated rotation fits i32"))
            }));
            placement.model.post_predicate_clause(
                ConstraintFamily::PlacementChoice,
                &variables,
                predicates,
                tag,
            );
            for value in &orientation.equivalent_rotations {
                placement.model.post_predicate_clause(
                    ConstraintFamily::PlacementChoice,
                    &[rotation, orientation.selected_parent],
                    vec![
                        rotation.disequality_predicate(
                            i32::try_from(*value).expect("validated rotation fits i32"),
                        ),
                        orientation.selected.get_true_predicate(),
                    ],
                    tag,
                );
            }
        }
        rotations.insert(instance.id.clone(), rotation);
    }
    rotations
}

fn build_endpoints(
    placement: &mut PlacementModel,
    input: &ModelInput,
    rotations: &BTreeMap<String, DomainId>,
    counters: Arc<EndpointSupportPropagationCounters>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<Vec<ModelEndpoint>, IntegratedLayoutDiagnostic> {
    let instances = input
        .instances
        .iter()
        .map(|instance| (instance.id.as_str(), instance))
        .collect::<BTreeMap<_, _>>();
    let mut endpoints = Vec::new();
    for (terminal, instance_id, direction, transport, ports) in facility_endpoint_descriptors(input)
    {
        let instance = instances[instance_id.as_str()];
        let rotation = rotations[instance_id.as_str()];
        let rotation_values = placement
            .instances
            .iter()
            .find(|candidate| candidate.id == instance_id)
            .expect("prepared endpoint instance is modeled")
            .orientations
            .iter()
            .flat_map(|orientation| orientation.equivalent_rotations.iter().copied())
            .map(|value| i32::try_from(value).expect("validated rotation fits i32"))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut connections = BTreeSet::new();
        let mut raw_rows = Vec::new();
        for rotation_value in &rotation_values {
            for (port_index, port) in ports.iter().enumerate() {
                let connection =
                    local_connection(&instance.definition, port, i64::from(*rotation_value));
                connections.insert(connection);
                raw_rows.push((*rotation_value, port_index, connection));
            }
        }
        if raw_rows.is_empty() {
            return Err(IntegratedLayoutDiagnostic::error(
                "bottom-up-terminal-has-no-port-support",
                "/networks/terminals",
                Some(terminal.clone()),
                "facility terminal has no compatible directional port support",
            ));
        }
        let local_connections = connections.into_iter().collect::<Vec<_>>();
        let connection_keys = local_connections
            .iter()
            .enumerate()
            .map(|(index, connection)| {
                (
                    *connection,
                    i32::try_from(index).expect("local connection count fits i32"),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let rows = raw_rows
            .into_iter()
            .map(|(rotation_value, port_index, connection)| {
                [
                    rotation_value,
                    i32::try_from(port_index).expect("port count fits i32"),
                    connection_keys[&connection],
                ]
            })
            .collect::<Vec<_>>();
        let port_values = (0..ports.len())
            .map(|index| i32::try_from(index).expect("port count fits i32"))
            .collect::<Vec<_>>();
        let local_values = (0..local_connections.len())
            .map(|index| i32::try_from(index).expect("local connection count fits i32"))
            .collect::<Vec<_>>();
        let port_choice = placement.model.new_sparse_variable(
            VariableFamily::Endpoint,
            port_values.clone(),
            format!("terminal:{terminal}:port"),
        );
        let local_key = placement.model.new_sparse_variable(
            VariableFamily::EndpointGeometry,
            local_values.clone(),
            format!("terminal:{terminal}:local-connection"),
        );
        let connection_x = placement.model.new_variable(
            VariableFamily::EndpointGeometry,
            0,
            input.width - 1,
            format!("terminal:{terminal}:connection-x"),
        );
        let connection_y = placement.model.new_variable(
            VariableFamily::EndpointGeometry,
            0,
            input.height - 1,
            format!("terminal:{terminal}:connection-y"),
        );
        placement.model.record_global_constraint(
            ConstraintFamily::EndpointLink,
            [rotation, port_choice, local_key],
        );
        let _ = placement
            .model
            .solver_mut()
            .add_propagator(SparseEndpointSupportPropagatorArgs {
                name: format!("terminal:{terminal}:rotation-port-local-support"),
                variables: [rotation, port_choice, local_key],
                domain_values: [rotation_values, port_values, local_values],
                rows,
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            });

        let modeled_instance = placement
            .instances
            .iter()
            .find(|candidate| candidate.id == instance_id)
            .expect("prepared endpoint instance is modeled");
        for (key, connection) in local_connections.iter().enumerate() {
            let key_value = i32::try_from(key).expect("local connection count fits i32");
            let selected = placement.model.new_named_literal_for_predicate(
                VariableFamily::EndpointGeometry,
                local_key.equality_predicate(key_value),
                tag,
                format!("terminal:{terminal}:local-connection:{key_value}"),
            );
            let selected_parent = *selected.get_integer_variable().inner();
            placement.model.post_implied_equals(
                ConstraintFamily::EndpointLink,
                vec![connection_x.scaled(1), modeled_instance.x.scaled(-1)],
                connection.dx,
                1,
                selected,
                selected_parent,
                tag,
            );
            placement.model.post_implied_equals(
                ConstraintFamily::EndpointLink,
                vec![connection_y.scaled(1), modeled_instance.y.scaled(-1)],
                connection.dy,
                1,
                selected,
                selected_parent,
                tag,
            );
        }
        post_connection_clearance(
            &mut placement.model,
            &terminal,
            &instance_id,
            connection_x,
            connection_y,
            &placement.instances,
            tag,
        );
        endpoints.push(ModelEndpoint {
            terminal,
            instance: instance_id,
            direction,
            transport,
            ports,
            port_choice,
            local_key,
            local_connections,
            connection_x,
            connection_y,
        });
    }
    endpoints.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    Ok(endpoints)
}

fn post_connection_clearance(
    model: &mut super::super::recorder::RecordedModel,
    terminal: &str,
    owner: &str,
    connection_x: DomainId,
    connection_y: DomainId,
    instances: &[ModelInstance],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for instance in instances.iter().filter(|instance| instance.id != owner) {
        for orientation in &instance.orientations {
            let left = reify_endpoint_clearance(
                model,
                &format!(
                    "terminal:{terminal}:left-of:{}:{}x{}",
                    instance.id, orientation.width, orientation.height
                ),
                vec![connection_x.scaled(1), instance.x.scaled(-1)],
                -1,
                tag,
            );
            let right = reify_endpoint_clearance(
                model,
                &format!(
                    "terminal:{terminal}:right-of:{}:{}x{}",
                    instance.id, orientation.width, orientation.height
                ),
                vec![instance.x.scaled(1), connection_x.scaled(-1)],
                -orientation.width,
                tag,
            );
            let above = reify_endpoint_clearance(
                model,
                &format!(
                    "terminal:{terminal}:above:{}:{}x{}",
                    instance.id, orientation.width, orientation.height
                ),
                vec![connection_y.scaled(1), instance.y.scaled(-1)],
                -1,
                tag,
            );
            let below = reify_endpoint_clearance(
                model,
                &format!(
                    "terminal:{terminal}:below:{}:{}x{}",
                    instance.id, orientation.width, orientation.height
                ),
                vec![instance.y.scaled(1), connection_y.scaled(-1)],
                -orientation.height,
                tag,
            );
            let separations = [left, right, above, below];
            let mut variables = vec![orientation.selected_parent];
            variables.extend(separations.iter().map(|(_, parent)| *parent));
            let mut predicates = vec![orientation.selected.get_false_predicate()];
            predicates.extend(
                separations
                    .iter()
                    .map(|(literal, _)| literal.get_true_predicate()),
            );
            model.post_predicate_clause(
                ConstraintFamily::EndpointClearance,
                &variables,
                predicates,
                tag,
            );
        }
    }
}

fn reify_endpoint_clearance(
    model: &mut super::super::recorder::RecordedModel,
    name: &str,
    terms: Vec<pumpkin_solver::core::variables::AffineView<DomainId>>,
    rhs: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (Literal, DomainId) {
    let literal = model.new_named_literal(VariableFamily::EndpointGeometry, name);
    let parent = *literal.get_integer_variable().inner();
    model.post_reified_less_than_or_equals(
        ConstraintFamily::EndpointClearance,
        terms,
        rhs,
        1,
        literal,
        parent,
        tag,
    );
    (literal, parent)
}

fn facility_endpoint_descriptors(
    input: &ModelInput,
) -> Vec<(
    String,
    String,
    FacilityPortDirection,
    TransportKind,
    Vec<FacilityPortDefinition>,
)> {
    let mut descriptors = Vec::new();
    for network in &input.networks {
        for terminal in network.terminals() {
            let EndpointInput::Facility { instance, ports } = terminal.endpoint() else {
                continue;
            };
            descriptors.push((
                terminal.id().to_string(),
                instance.clone(),
                terminal.direction(),
                network.transport(),
                ports.clone(),
            ));
        }
    }
    descriptors.sort_by(|left, right| left.0.cmp(&right.0));
    descriptors
}

fn local_connection(
    definition: &crate::facilities::FacilityDefinition,
    port: &FacilityPortDefinition,
    rotation: i64,
) -> LocalConnection {
    let (position, edge) = rotate_port(
        &port.position,
        port.edge,
        rotation,
        definition.footprint.width,
        definition.footprint.height,
    );
    let (delta_x, delta_y) = edge_delta(edge);
    LocalConnection {
        dx: i32::try_from(position.x).expect("validated port x fits i32") + delta_x,
        dy: i32::try_from(position.y).expect("validated port y fits i32") + delta_y,
        arm_direction: opposite_direction(edge_direction(edge)),
    }
}

fn edge_delta(edge: FacilityPortEdge) -> (i32, i32) {
    match edge {
        FacilityPortEdge::North => (0, -1),
        FacilityPortEdge::East => (1, 0),
        FacilityPortEdge::South => (0, 1),
        FacilityPortEdge::West => (-1, 0),
    }
}

fn edge_direction(edge: FacilityPortEdge) -> CardinalDirection {
    match edge {
        FacilityPortEdge::North => CardinalDirection::North,
        FacilityPortEdge::East => CardinalDirection::East,
        FacilityPortEdge::South => CardinalDirection::South,
        FacilityPortEdge::West => CardinalDirection::West,
    }
}

fn opposite_direction(direction: CardinalDirection) -> CardinalDirection {
    match direction {
        CardinalDirection::North => CardinalDirection::South,
        CardinalDirection::East => CardinalDirection::West,
        CardinalDirection::South => CardinalDirection::North,
        CardinalDirection::West => CardinalDirection::East,
    }
}

fn extract_witness(
    solution: &impl ProblemSolution,
    instances: &[ModelInstance],
    rotations: &BTreeMap<String, DomainId>,
    model_endpoints: &[ModelEndpoint],
) -> FacilityPortsWitness {
    let mut placements = instances
        .iter()
        .map(|instance| {
            let rotation = i64::from(solution.get_integer_value(rotations[&instance.id]));
            let orientation = instance
                .orientations
                .iter()
                .find(|orientation| orientation.equivalent_rotations.contains(&rotation))
                .expect("directional rotation belongs to one geometry class");
            FacilityPortPlacement {
                instance: instance.id.clone(),
                recipe: instance.recipe.clone(),
                facility: instance.facility.clone(),
                x: i64::from(solution.get_integer_value(instance.x)),
                y: i64::from(solution.get_integer_value(instance.y)),
                width: i64::from(orientation.width),
                height: i64::from(orientation.height),
                rotation,
            }
        })
        .collect::<Vec<_>>();
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));

    let mut endpoints = model_endpoints
        .iter()
        .map(|endpoint| {
            let port_index = usize::try_from(solution.get_integer_value(endpoint.port_choice))
                .expect("port choice is non-negative");
            let local_index = usize::try_from(solution.get_integer_value(endpoint.local_key))
                .expect("local connection key is non-negative");
            FacilityEndpointPlacement {
                terminal: endpoint.terminal.clone(),
                instance: endpoint.instance.clone(),
                port: endpoint.ports[port_index].id.clone(),
                direction: endpoint.direction,
                transport: endpoint.transport,
                connection_x: i64::from(solution.get_integer_value(endpoint.connection_x)),
                connection_y: i64::from(solution.get_integer_value(endpoint.connection_y)),
                arm_direction: endpoint.local_connections[local_index].arm_direction,
            }
        })
        .collect::<Vec<_>>();
    endpoints.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    let bounds = used_bounds(&placements, &endpoints);
    FacilityPortsWitness {
        bounds,
        placements,
        endpoints,
    }
}

fn validate_witness(
    input: &ModelInput,
    witness: &FacilityPortsWitness,
) -> Vec<IntegratedLayoutDiagnostic> {
    let mut diagnostics = Vec::new();
    let expected_instances = input
        .instances
        .iter()
        .map(|instance| (instance.id.as_str(), instance))
        .collect::<BTreeMap<_, _>>();
    let mut placements = BTreeMap::new();
    for placement in &witness.placements {
        if placements
            .insert(placement.instance.as_str(), placement)
            .is_some()
        {
            diagnostics.push(diagnostic(
                "bottom-up-duplicate-facility-placement",
                &placement.instance,
                "facility-port witness contains a duplicate facility placement",
            ));
            continue;
        }
        let Some(instance) = expected_instances.get(placement.instance.as_str()) else {
            diagnostics.push(diagnostic(
                "bottom-up-unexpected-facility-placement",
                &placement.instance,
                "facility-port witness contains an unexpected facility placement",
            ));
            continue;
        };
        if placement.recipe != instance.recipe
            || placement.facility != instance.facility
            || !instance
                .definition
                .allowed_rotations
                .contains(&placement.rotation)
        {
            diagnostics.push(diagnostic(
                "bottom-up-invalid-directional-facility-placement",
                &placement.instance,
                "facility-port witness changed facility identity or selected an invalid rotation",
            ));
        }
        let expected_dimensions = oriented_dimensions_i64(
            instance.definition.footprint.width,
            instance.definition.footprint.height,
            placement.rotation,
        );
        if (placement.width, placement.height) != expected_dimensions
            || placement.x < 0
            || placement.y < 0
            || placement.x + placement.width > i64::from(input.width)
            || placement.y + placement.height > i64::from(input.height)
        {
            diagnostics.push(diagnostic(
                "bottom-up-invalid-directional-facility-geometry",
                &placement.instance,
                "facility-port witness dimensions or origin are inconsistent with the ceiling",
            ));
        }
    }
    for instance in &input.instances {
        if !placements.contains_key(instance.id.as_str()) {
            diagnostics.push(diagnostic(
                "bottom-up-missing-facility-placement",
                &instance.id,
                "facility-port witness omitted a modeled facility",
            ));
        }
    }
    for left in 0..witness.placements.len() {
        for right in (left + 1)..witness.placements.len() {
            if port_placements_overlap(&witness.placements[left], &witness.placements[right]) {
                diagnostics.push(diagnostic(
                    "bottom-up-overlapping-facilities",
                    &format!(
                        "{}:{}",
                        witness.placements[left].instance, witness.placements[right].instance
                    ),
                    "facility-port witness contains overlapping facility footprints",
                ));
            }
        }
    }

    let expected_endpoints = facility_endpoint_descriptors(input)
        .into_iter()
        .map(|descriptor| (descriptor.0.clone(), descriptor))
        .collect::<BTreeMap<_, _>>();
    let mut seen_endpoints = BTreeSet::new();
    for endpoint in &witness.endpoints {
        if !seen_endpoints.insert(endpoint.terminal.as_str()) {
            diagnostics.push(diagnostic(
                "bottom-up-duplicate-facility-endpoint",
                &endpoint.terminal,
                "facility-port witness contains a duplicate facility endpoint",
            ));
            continue;
        }
        let Some((_, instance_id, direction, transport, ports)) =
            expected_endpoints.get(&endpoint.terminal)
        else {
            diagnostics.push(diagnostic(
                "bottom-up-unexpected-facility-endpoint",
                &endpoint.terminal,
                "facility-port witness contains an unexpected facility endpoint",
            ));
            continue;
        };
        let Some(placement) = placements.get(instance_id.as_str()) else {
            continue;
        };
        let Some(port) = ports.iter().find(|port| port.id == endpoint.port) else {
            diagnostics.push(diagnostic(
                "bottom-up-invalid-facility-port",
                &endpoint.terminal,
                "facility-port witness selected a port outside the compatible endpoint domain",
            ));
            continue;
        };
        if endpoint.instance != *instance_id
            || endpoint.direction != *direction
            || endpoint.transport != *transport
        {
            diagnostics.push(diagnostic(
                "bottom-up-facility-endpoint-identity-mismatch",
                &endpoint.terminal,
                "facility-port witness changed endpoint ownership, direction, or transport",
            ));
        }
        let definition = &expected_instances[instance_id.as_str()].definition;
        let expected_local = local_connection(definition, port, placement.rotation);
        let expected_x = placement.x + i64::from(expected_local.dx);
        let expected_y = placement.y + i64::from(expected_local.dy);
        if endpoint.connection_x != expected_x
            || endpoint.connection_y != expected_y
            || endpoint.arm_direction != expected_local.arm_direction
            || endpoint.connection_x < 0
            || endpoint.connection_y < 0
            || endpoint.connection_x >= i64::from(input.width)
            || endpoint.connection_y >= i64::from(input.height)
        {
            diagnostics.push(diagnostic(
                "bottom-up-invalid-facility-endpoint-geometry",
                &endpoint.terminal,
                "facility-port witness connection cell or arm does not match placement, rotation, and port",
            ));
        }
        if witness.placements.iter().any(|facility| {
            endpoint.connection_x >= facility.x
                && endpoint.connection_x < facility.x + facility.width
                && endpoint.connection_y >= facility.y
                && endpoint.connection_y < facility.y + facility.height
        }) {
            diagnostics.push(diagnostic(
                "bottom-up-blocked-facility-endpoint",
                &endpoint.terminal,
                "facility-port witness connection cell is blocked by a facility footprint",
            ));
        }
    }
    for terminal in expected_endpoints.keys() {
        if !seen_endpoints.contains(terminal.as_str()) {
            diagnostics.push(diagnostic(
                "bottom-up-missing-facility-endpoint",
                terminal,
                "facility-port witness omitted a modeled facility endpoint",
            ));
        }
    }
    if witness.bounds != used_bounds(&witness.placements, &witness.endpoints) {
        diagnostics.push(IntegratedLayoutDiagnostic::error(
            "bottom-up-facility-port-bounds-mismatch",
            "/witness/bounds",
            None,
            "facility-port witness bounds do not equal its modeled facility and connection geometry",
        ));
    }
    diagnostics
}

fn used_bounds(
    placements: &[FacilityPortPlacement],
    endpoints: &[FacilityEndpointPlacement],
) -> FacilityPlacementBounds {
    let mut min_x = i64::MAX;
    let mut min_y = i64::MAX;
    let mut max_x = i64::MIN;
    let mut max_y = i64::MIN;
    for placement in placements {
        min_x = min_x.min(placement.x);
        min_y = min_y.min(placement.y);
        max_x = max_x.max(placement.x + placement.width);
        max_y = max_y.max(placement.y + placement.height);
    }
    for endpoint in endpoints {
        min_x = min_x.min(endpoint.connection_x);
        min_y = min_y.min(endpoint.connection_y);
        max_x = max_x.max(endpoint.connection_x + 1);
        max_y = max_y.max(endpoint.connection_y + 1);
    }
    if min_x == i64::MAX {
        FacilityPlacementBounds {
            width: 0,
            height: 0,
        }
    } else {
        FacilityPlacementBounds {
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }
}

fn port_placements_overlap(left: &FacilityPortPlacement, right: &FacilityPortPlacement) -> bool {
    left.x < right.x + right.width
        && right.x < left.x + left.width
        && left.y < right.y + right.height
        && right.y < left.y + left.height
}

fn diagnostic(
    code: &'static str,
    entity: &str,
    message: &'static str,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(code, "/witness", Some(entity.to_string()), message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityDefinition, FacilityFootprint, FacilityPortPosition};
    use crate::layouts::integrated::{ComponentCapacityRates, EdgeInput, InstanceInput, networks};
    use crate::recipes::{FacilityInstanceWiringEdge, Rate};

    fn east_port_fixture() -> (FacilityDefinition, FacilityPortDefinition) {
        let port = FacilityPortDefinition {
            id: "output".to_string(),
            direction: FacilityPortDirection::Output,
            transport: TransportKind::Belt,
            position: FacilityPortPosition { x: 2, y: 0 },
            edge: FacilityPortEdge::East,
        };
        let definition = FacilityDefinition {
            id: "fixture".to_string(),
            footprint: FacilityFootprint {
                width: 3,
                height: 2,
            },
            allowed_rotations: vec![0, 90, 180, 270],
            ports: vec![port.clone()],
        };
        (definition, port)
    }

    #[test]
    fn rotates_the_selected_port_to_the_exact_outside_connection_cell() {
        let (definition, port) = east_port_fixture();

        assert_eq!(
            local_connection(&definition, &port, 0),
            LocalConnection {
                dx: 3,
                dy: 0,
                arm_direction: CardinalDirection::West,
            }
        );
        assert_eq!(
            local_connection(&definition, &port, 90),
            LocalConnection {
                dx: 1,
                dy: 3,
                arm_direction: CardinalDirection::North,
            }
        );
        assert_eq!(
            local_connection(&definition, &port, 180),
            LocalConnection {
                dx: -1,
                dy: 1,
                arm_direction: CardinalDirection::East,
            }
        );
        assert_eq!(
            local_connection(&definition, &port, 270),
            LocalConnection {
                dx: 0,
                dy: -1,
                arm_direction: CardinalDirection::South,
            }
        );
    }

    #[test]
    fn every_rotated_connection_cell_is_outside_its_owner_footprint() {
        let (definition, port) = east_port_fixture();

        for rotation in definition.allowed_rotations.iter().copied() {
            let connection = local_connection(&definition, &port, rotation);
            let (width, height) = oriented_dimensions_i64(
                definition.footprint.width,
                definition.footprint.height,
                rotation,
            );
            assert!(
                connection.dx < 0
                    || connection.dy < 0
                    || i64::from(connection.dx) >= width
                    || i64::from(connection.dy) >= height
            );
        }
    }

    #[test]
    fn solves_and_validates_a_facility_owned_port_without_routing_state() {
        let (definition, port) = east_port_fixture();
        let rate = Rate {
            numerator: 1,
            denominator: 1,
        };
        let edge = EdgeInput {
            requirement_id: "requirement".to_string(),
            edge: FacilityInstanceWiringEdge::original(
                "fixture-instance",
                "external-output",
                "surplus",
                "item",
                rate,
            ),
            source: EndpointInput::Facility {
                instance: "fixture-instance".to_string(),
                ports: vec![port],
            },
            target: EndpointInput::External {
                node: "external-output".to_string(),
            },
            transport: TransportKind::Belt,
            capacity_rate: rate,
            component_capacity_rates: ComponentCapacityRates {
                splitter: rate,
                converger: rate,
                bridge: rate,
            },
        };
        let edges = vec![edge];
        let input = ModelInput {
            width: 4,
            height: 3,
            cell_count: 12,
            instances: vec![InstanceInput {
                id: "fixture-instance".to_string(),
                recipe: "fixture-recipe".to_string(),
                facility: definition.id.clone(),
                definition,
            }],
            networks: networks::normalize(&edges).expect("fixture network should normalize"),
            edges,
        };

        let report = solve(input, Duration::from_secs(1));

        assert_eq!(report.outcome, BottomUpRungOutcome::Feasible);
        assert_eq!(report.validation, ExactValidationStatus::Passed);
        assert!(!report.semantic_certificate.pipe_routing);
        assert!(!report.semantic_certificate.belt_routing);
        let BottomUpRungWitness::FacilityPorts { witness } = report.witness.unwrap() else {
            panic!("facility-port rung should return its dedicated witness");
        };
        assert_eq!(witness.placements.len(), 1);
        assert_eq!(witness.endpoints.len(), 1);
        assert_eq!(witness.endpoints[0].port, "output");
    }

    #[test]
    fn rejects_a_port_connection_blocked_by_the_only_non_overlapping_placement() {
        let (definition, port) = east_port_fixture();
        let blocker = FacilityDefinition {
            id: "blocker".to_string(),
            footprint: FacilityFootprint {
                width: 1,
                height: 2,
            },
            allowed_rotations: vec![0],
            ports: Vec::new(),
        };
        let rate = Rate {
            numerator: 1,
            denominator: 1,
        };
        let edge = EdgeInput {
            requirement_id: "requirement".to_string(),
            edge: FacilityInstanceWiringEdge::original(
                "fixture-instance",
                "external-output",
                "surplus",
                "item",
                rate,
            ),
            source: EndpointInput::Facility {
                instance: "fixture-instance".to_string(),
                ports: vec![port],
            },
            target: EndpointInput::External {
                node: "external-output".to_string(),
            },
            transport: TransportKind::Belt,
            capacity_rate: rate,
            component_capacity_rates: ComponentCapacityRates {
                splitter: rate,
                converger: rate,
                bridge: rate,
            },
        };
        let edges = vec![edge];
        let input = ModelInput {
            width: 4,
            height: 2,
            cell_count: 8,
            instances: vec![
                InstanceInput {
                    id: "fixture-instance".to_string(),
                    recipe: "fixture-recipe".to_string(),
                    facility: definition.id.clone(),
                    definition,
                },
                InstanceInput {
                    id: "blocker-instance".to_string(),
                    recipe: "blocker-recipe".to_string(),
                    facility: blocker.id.clone(),
                    definition: blocker,
                },
            ],
            networks: networks::normalize(&edges).expect("fixture network should normalize"),
            edges,
        };

        let report = solve(input, Duration::from_secs(1));

        assert_eq!(report.outcome, BottomUpRungOutcome::Infeasible);
        assert_eq!(report.validation, ExactValidationStatus::NotAttempted);
        assert!(report.witness.is_none());
    }
}
