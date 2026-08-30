use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use pumpkin_solver::Solver;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::{
    EndpointInput, ExactModelMetrics, ExactObjectiveKind, ExactObjectiveStageReport,
    FacilityPortEdge, InstanceInput, IntegratedLayoutDiagnostic, IntegratedLayoutReport,
    IntegratedLayoutStatus, LayoutScore, ModelInput, TransportKind, TransportNetworkEndpoint,
    ValidatedLogisticsComponentCatalog, witness,
};
use crate::facilities::FacilityPortDirection;
use crate::logistics::CardinalDirection;
use crate::logistics::LogisticsComponentKind;

mod extract;
mod formulation;
mod hint;
mod metrics;
mod objective;

use extract::extract_report;
use formulation::{
    DIRECTIONS, FlowTerms, direction_between, direction_index, external_endpoint_options,
    generate_candidates, grid_arcs, incident_arcs_by_axis, model_facility_endpoint_options,
    post_acyclic_network_ordering, post_at_most_one, post_branch_component_topology,
    post_bridge_crossing, post_equals_one,
};
use hint::build_solver_hint;
use metrics::{elapsed_millis, finish_report};
use objective::{build_objectives, optimise_lexicographically};

struct Candidate {
    rotation: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    occupied_cells: Vec<usize>,
    port_connections: BTreeMap<String, usize>,
    selected: DomainId,
}

struct ModelInstance {
    input: InstanceInput,
    candidates: Vec<Candidate>,
}

#[derive(Clone)]
struct EndpointOption {
    endpoint: TransportNetworkEndpoint,
    cell: usize,
    selected: DomainId,
    external_side: Option<FacilityPortEdge>,
    arm_direction: CardinalDirection,
}

#[derive(Debug, Clone, Copy)]
struct Arc {
    from: usize,
    to: usize,
    flow: DomainId,
    selected: DomainId,
}

struct ModelTerminal {
    id: String,
    direction: FacilityPortDirection,
    rate: crate::recipes::Rate,
    flow_units: i32,
    options: Vec<EndpointOption>,
}

struct ModelNetwork {
    input_index: usize,
    line_capacity_units: i32,
    bridge_capacity_units: i32,
    terminals: Vec<ModelTerminal>,
    arcs: Vec<Arc>,
    route_cells: Vec<DomainId>,
    horizontal_incident: Vec<Vec<DomainId>>,
    vertical_incident: Vec<Vec<DomainId>>,
}

struct EdgeEndpointOptions {
    source: Vec<EndpointOption>,
    target: Vec<EndpointOption>,
}

struct ModelBridge {
    transport: TransportKind,
    cell: usize,
    component: String,
    selected: DomainId,
    rotations: Vec<(i64, DomainId)>,
}

struct ModelBranchComponent {
    network_index: usize,
    transport: TransportKind,
    cell: usize,
    component: String,
    kind: LogisticsComponentKind,
    rotation: i64,
    selected: DomainId,
}

fn post_presence(
    solver: &mut Solver,
    name: String,
    variables: impl Iterator<Item = DomainId>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> DomainId {
    let variables = variables.collect::<Vec<_>>();
    let presence = solver.new_named_bounded_integer(0, 1, name);
    for variable in &variables {
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                [variable.scaled(1), presence.scaled(-1)],
                0,
                tag,
            ))
            .post();
    }
    let mut definition = vec![presence.scaled(1)];
    definition.extend(variables.iter().map(|variable| variable.scaled(-1)));
    solver
        .add_constraint(pumpkin_solver::less_than_or_equals(definition, 0, tag))
        .post();
    presence
}

fn post_arm(
    solver: &mut Solver,
    name: String,
    terminal_presence: DomainId,
    grid_arcs: &[DomainId],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> DomainId {
    let arm = solver.new_named_bounded_integer(0, 1, name);
    let mut definition = vec![arm.scaled(1), terminal_presence.scaled(-1)];
    definition.extend(grid_arcs.iter().map(|arc| arc.scaled(-1)));
    solver
        .add_constraint(pumpkin_solver::equals(definition, 0, tag))
        .post();
    arm
}

fn undirected_arc_pairs(arcs: &[Arc]) -> Vec<[Arc; 2]> {
    let mut by_edge = BTreeMap::<(usize, usize), Vec<Arc>>::new();
    for arc in arcs {
        by_edge
            .entry((arc.from.min(arc.to), arc.from.max(arc.to)))
            .or_default()
            .push(*arc);
    }
    by_edge
        .into_values()
        .map(|pair| {
            pair.try_into()
                .expect("every orthogonal grid edge has two directed arcs")
        })
        .collect()
}

pub(super) fn solve_with_prior_solution(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    prior_solution: Option<&IntegratedLayoutReport>,
) -> IntegratedLayoutReport {
    let construction_started = Instant::now();
    let mut model_metrics = ExactModelMetrics {
        facility_count: input.instances.len(),
        route_requirement_count: input.edges.len(),
        commodity_network_count: input.networks.len(),
        commodity_item_count: input
            .networks
            .iter()
            .map(|network| network.item())
            .collect::<BTreeSet<_>>()
            .len(),
        belt_network_count: input
            .networks
            .iter()
            .filter(|network| network.transport() == TransportKind::Belt)
            .count(),
        pipe_network_count: input
            .networks
            .iter()
            .filter(|network| network.transport() == TransportKind::Pipe)
            .count(),
        network_requirement_reference_count: input
            .networks
            .iter()
            .map(|network| network.route_indices().len())
            .sum(),
        network_terminal_count: input
            .networks
            .iter()
            .map(|network| network.terminal_count())
            .sum(),
        external_terminal_count: input
            .networks
            .iter()
            .map(|network| network.external_terminal_count())
            .sum(),
        maximum_network_flow_scale: input
            .networks
            .iter()
            .map(|network| network.flow_scale())
            .max()
            .unwrap_or(0),
        maximum_line_capacity_units: input
            .networks
            .iter()
            .map(|network| network.line_capacity_units())
            .max()
            .unwrap_or(0),
        total_terminal_flow_units: input
            .networks
            .iter()
            .map(|network| network.total_terminal_flow_units())
            .sum(),
        grid_cell_count: input.cell_count as usize,
        ..ExactModelMetrics::default()
    };
    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let cell_count = input.cell_count as usize;
    let mut occupancy = vec![Vec::<DomainId>::new(); cell_count];
    let mut model_instances = Vec::with_capacity(input.instances.len());

    for instance in &input.instances {
        let candidates = generate_candidates(&mut solver, &instance, input.width, input.height);
        model_metrics.placement_variables += candidates.len();
        if candidates.is_empty() {
            return IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Infeasible,
                IntegratedLayoutDiagnostic::error(
                    "facility-has-no-placement-candidate",
                    "/",
                    Some(instance.id.clone()),
                    "facility has no rotation and origin within the hard layout bounds",
                ),
            );
        }
        post_equals_one(
            &mut solver,
            candidates.iter().map(|candidate| candidate.selected),
            tag,
        );
        for candidate in &candidates {
            for cell in &candidate.occupied_cells {
                occupancy[*cell].push(candidate.selected);
            }
        }
        model_instances.push(ModelInstance {
            input: instance.clone(),
            candidates,
        });
    }

    for cell_candidates in &occupancy {
        post_at_most_one(&mut solver, cell_candidates.iter().copied(), tag);
    }

    let mut edge_endpoint_options = Vec::with_capacity(input.edges.len());
    for (edge_index, edge) in input.edges.iter().enumerate() {
        let (source_options, target_options) = match (&edge.source, &edge.target) {
            (EndpointInput::Facility { .. }, EndpointInput::Facility { .. }) => (
                model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "source",
                    &edge.source,
                    &model_instances,
                    tag,
                ),
                model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "target",
                    &edge.target,
                    &model_instances,
                    tag,
                ),
            ),
            (EndpointInput::External { node }, EndpointInput::Facility { .. }) => {
                let target = model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "target",
                    &edge.target,
                    &model_instances,
                    tag,
                );
                (external_endpoint_options(node, &target), target)
            }
            (EndpointInput::Facility { .. }, EndpointInput::External { node }) => {
                let source = model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "source",
                    &edge.source,
                    &model_instances,
                    tag,
                );
                let target = external_endpoint_options(node, &source);
                (source, target)
            }
            (EndpointInput::External { .. }, EndpointInput::External { .. }) => unreachable!(
                "external-to-external requirements are rejected during model preparation"
            ),
        };
        model_metrics.endpoint_variables += match (&edge.source, &edge.target) {
            (EndpointInput::Facility { .. }, EndpointInput::Facility { .. }) => {
                source_options.len() + target_options.len()
            }
            (EndpointInput::External { .. }, EndpointInput::Facility { .. }) => {
                target_options.len()
            }
            (EndpointInput::Facility { .. }, EndpointInput::External { .. }) => {
                source_options.len()
            }
            (EndpointInput::External { .. }, EndpointInput::External { .. }) => 0,
        };
        edge_endpoint_options.push(EdgeEndpointOptions {
            source: source_options,
            target: target_options,
        });
    }

    let mut model_networks = Vec::with_capacity(input.networks.len());
    let mut model_branch_components = Vec::new();
    for (network_index, network) in input.networks.iter().enumerate() {
        let terminals = network
            .terminals()
            .iter()
            .map(|terminal| {
                let edge_options = &edge_endpoint_options[terminal.route_index()];
                ModelTerminal {
                    id: terminal.id().to_string(),
                    direction: terminal.direction(),
                    rate: terminal.rate(),
                    flow_units: terminal.flow_units(),
                    options: if terminal.direction() == FacilityPortDirection::Output {
                        edge_options.source.clone()
                    } else {
                        edge_options.target.clone()
                    },
                }
            })
            .collect::<Vec<_>>();

        let (arcs, incoming, outgoing) = grid_arcs(
            &mut solver,
            network_index,
            input.width,
            input.height,
            network.line_capacity_units(),
            tag,
        );
        let (horizontal_incident, vertical_incident) =
            incident_arcs_by_axis(&arcs, cell_count, input.width);
        model_metrics.route_arc_variables += arcs.len();
        model_metrics.network_flow_variables += arcs.len();
        model_metrics.route_cell_variables += cell_count;
        model_metrics.route_order_variables += cell_count;
        model_metrics.acyclicity_constraints += arcs.len();
        post_acyclic_network_ordering(&mut solver, network_index, &arcs, input.cell_count, tag);
        let mut supply_by_cell: Vec<[FlowTerms; 4]> = (0..cell_count)
            .map(|_| std::array::from_fn(|_| FlowTerms::new()))
            .collect::<Vec<_>>();
        let mut demand_by_cell: Vec<[FlowTerms; 4]> = (0..cell_count)
            .map(|_| std::array::from_fn(|_| FlowTerms::new()))
            .collect::<Vec<_>>();
        for terminal in &terminals {
            let destination = if terminal.direction == FacilityPortDirection::Output {
                &mut supply_by_cell
            } else {
                &mut demand_by_cell
            };
            for option in &terminal.options {
                destination[option.cell][direction_index(option.arm_direction)]
                    .push((option.selected, terminal.flow_units));
            }
        }

        let mut route_cells = Vec::with_capacity(cell_count);
        for cell in 0..cell_count {
            let route_cell = solver.new_named_bounded_integer(
                0,
                1,
                format!("network-{network_index}-cell-{cell}"),
            );
            route_cells.push(route_cell);

            let mut conservation = Vec::new();
            conservation.extend(outgoing[cell].iter().map(|arc| arc.flow.scaled(1)));
            conservation.extend(incoming[cell].iter().map(|arc| arc.flow.scaled(-1)));
            conservation.extend(
                supply_by_cell[cell]
                    .iter()
                    .flatten()
                    .map(|(variable, units)| variable.scaled(-*units)),
            );
            conservation.extend(
                demand_by_cell[cell]
                    .iter()
                    .flatten()
                    .map(|(variable, units)| variable.scaled(*units)),
            );
            solver
                .add_constraint(pumpkin_solver::equals(conservation, 0, tag))
                .post();

            let mut incoming_flow: [FlowTerms; 4] =
                std::array::from_fn(|direction| supply_by_cell[cell][direction].clone());
            let mut outgoing_flow: [FlowTerms; 4] =
                std::array::from_fn(|direction| demand_by_cell[cell][direction].clone());
            for arc in &incoming[cell] {
                incoming_flow[direction_index(direction_between(cell, arc.from, input.width))]
                    .push((arc.flow, 1));
            }
            for arc in &outgoing[cell] {
                outgoing_flow[direction_index(direction_between(cell, arc.to, input.width))]
                    .push((arc.flow, 1));
            }

            let incoming_arms: [DomainId; 4] = std::array::from_fn(|direction| {
                let terminal_presence = post_presence(
                    &mut solver,
                    format!(
                        "network-{network_index}-cell-{cell}-{:?}-supply",
                        DIRECTIONS[direction]
                    )
                    .to_lowercase(),
                    supply_by_cell[cell][direction]
                        .iter()
                        .map(|(variable, _)| *variable),
                    tag,
                );
                let grid_arcs = incoming[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.from, input.width)) == direction
                    })
                    .map(|arc| arc.selected)
                    .collect::<Vec<_>>();
                post_arm(
                    &mut solver,
                    format!(
                        "network-{network_index}-cell-{cell}-{:?}-incoming",
                        DIRECTIONS[direction]
                    )
                    .to_lowercase(),
                    terminal_presence,
                    &grid_arcs,
                    tag,
                )
            });
            let outgoing_arms: [DomainId; 4] = std::array::from_fn(|direction| {
                let terminal_presence = post_presence(
                    &mut solver,
                    format!(
                        "network-{network_index}-cell-{cell}-{:?}-demand",
                        DIRECTIONS[direction]
                    )
                    .to_lowercase(),
                    demand_by_cell[cell][direction]
                        .iter()
                        .map(|(variable, _)| *variable),
                    tag,
                );
                let grid_arcs = outgoing[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.to, input.width)) == direction
                    })
                    .map(|arc| arc.selected)
                    .collect::<Vec<_>>();
                post_arm(
                    &mut solver,
                    format!(
                        "network-{network_index}-cell-{cell}-{:?}-outgoing",
                        DIRECTIONS[direction]
                    )
                    .to_lowercase(),
                    terminal_presence,
                    &grid_arcs,
                    tag,
                )
            });
            for direction in 0..4 {
                solver
                    .add_constraint(pumpkin_solver::less_than_or_equals(
                        [
                            incoming_arms[direction].scaled(1),
                            outgoing_arms[direction].scaled(1),
                        ],
                        1,
                        tag,
                    ))
                    .post();
                for flow in [&incoming_flow[direction], &outgoing_flow[direction]] {
                    if flow.is_empty() {
                        continue;
                    }
                    solver
                        .add_constraint(pumpkin_solver::less_than_or_equals(
                            flow.iter()
                                .map(|(variable, coefficient)| variable.scaled(*coefficient))
                                .collect::<Vec<_>>(),
                            network.line_capacity_units(),
                            tag,
                        ))
                        .post();
                }
            }

            let branch_components = post_branch_component_topology(
                &mut solver,
                network_index,
                cell,
                network.transport(),
                &incoming_arms,
                &outgoing_arms,
                &incoming_flow,
                network.line_capacity_units(),
                network.component_capacity_units(LogisticsComponentKind::Splitter),
                network.component_capacity_units(LogisticsComponentKind::Converger),
                logistics_components,
                tag,
            );
            model_metrics.branch_component_variables += branch_components.len();
            model_branch_components.extend(branch_components);

            let active_variables = incoming[cell]
                .iter()
                .chain(&outgoing[cell])
                .map(|arc| arc.selected)
                .chain(
                    supply_by_cell[cell]
                        .iter()
                        .flatten()
                        .map(|(variable, _)| *variable),
                )
                .chain(
                    demand_by_cell[cell]
                        .iter()
                        .flatten()
                        .map(|(variable, _)| *variable),
                )
                .collect::<Vec<_>>();
            for active in &active_variables {
                solver
                    .add_constraint(pumpkin_solver::less_than_or_equals(
                        [active.scaled(1), route_cell.scaled(-1)],
                        0,
                        tag,
                    ))
                    .post();
            }
            let mut route_definition = vec![route_cell.scaled(1)];
            route_definition.extend(active_variables.iter().map(|variable| variable.scaled(-1)));
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    route_definition,
                    0,
                    tag,
                ))
                .post();
        }

        for pair in undirected_arc_pairs(&arcs) {
            post_at_most_one(&mut solver, pair.into_iter().map(|arc| arc.selected), tag);
        }
        model_networks.push(ModelNetwork {
            input_index: network_index,
            line_capacity_units: network.line_capacity_units(),
            bridge_capacity_units: network.component_capacity_units(LogisticsComponentKind::Bridge),
            terminals,
            arcs,
            route_cells,
            horizontal_incident,
            vertical_incident,
        });
    }

    let mut model_bridges = Vec::with_capacity(cell_count * 2);
    for (transport, transport_name) in
        [(TransportKind::Belt, "belt"), (TransportKind::Pipe, "pipe")]
    {
        let definition = logistics_components
            .component_by_kind(transport, LogisticsComponentKind::Bridge)
            .expect("validated catalog has every bridge capability");
        let networks = model_networks
            .iter()
            .enumerate()
            .filter(|(_, network)| input.networks[network.input_index].transport() == transport)
            .collect::<Vec<_>>();
        for cell in 0..cell_count {
            let selected =
                solver.new_named_bounded_integer(0, 1, format!("{transport_name}-bridge-{cell}"));
            let rotations = definition
                .allowed_rotations
                .iter()
                .map(|rotation| {
                    (
                        *rotation,
                        solver.new_named_bounded_integer(
                            0,
                            1,
                            format!("{transport_name}-bridge-{cell}-rotation-{rotation}"),
                        ),
                    )
                })
                .collect::<Vec<_>>();
            let mut rotation_definition = rotations
                .iter()
                .map(|(_, variable)| variable.scaled(1))
                .collect::<Vec<_>>();
            rotation_definition.push(selected.scaled(-1));
            solver
                .add_constraint(pumpkin_solver::equals(rotation_definition, 0, tag))
                .post();
            let (owner_variables, crossing_constraints) = post_bridge_crossing(
                &mut solver,
                transport_name,
                cell,
                selected,
                &occupancy[cell],
                &networks,
                &model_branch_components
                    .iter()
                    .filter(|component| component.transport == transport && component.cell == cell)
                    .map(|component| component.selected)
                    .collect::<Vec<_>>(),
                tag,
            );
            model_metrics.bridge_variables += 1;
            model_metrics.bridge_rotation_variables += rotations.len();
            model_metrics.crossing_owner_variables += owner_variables;
            model_metrics.crossing_constraints += crossing_constraints + 1;
            model_bridges.push(ModelBridge {
                transport,
                cell,
                component: definition.id.clone(),
                selected,
                rotations,
            });
        }
    }

    let solver_hint = build_solver_hint(
        prior_solution,
        &input,
        &model_instances,
        &model_networks,
        &model_branch_components,
        &model_bridges,
        &mut model_metrics,
    );
    let objectives = match build_objectives(
        &mut solver,
        &input,
        &occupancy,
        &model_networks,
        &model_branch_components,
        &model_bridges,
        &mut model_metrics,
        tag,
    ) {
        Ok(objectives) => objectives,
        Err(diagnostic) => {
            return IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::InvalidInput,
                diagnostic,
            );
        }
    };

    let construction_ms = elapsed_millis(construction_started.elapsed());
    let search = optimise_lexicographically(
        &mut solver,
        objectives,
        &solver_hint,
        time_limit,
        tag,
        |solution, status| {
            extract_report(
                solution,
                status,
                &input,
                &model_instances,
                &model_networks,
                &model_branch_components,
                &model_bridges,
            )
        },
    );
    let mut report = search.report;
    let objective_stages = search.stages;
    let search_ms = search.search_ms;
    let first_incumbent_ms = search.first_incumbent_ms;
    let observed_incumbents = search.incumbent_count;
    let validation = if report.success {
        match witness::validate(&input, logistics_components, &report) {
            Ok(()) => match validate_objective_witness(&report, &objective_stages) {
                Ok(()) => super::ExactValidationStatus::Passed,
                Err(diagnostic) => {
                    report.success = false;
                    report.status = IntegratedLayoutStatus::Unknown;
                    report.diagnostics.push(diagnostic);
                    super::ExactValidationStatus::Failed
                }
            },
            Err(diagnostic) => {
                report.success = false;
                report.status = IntegratedLayoutStatus::Unknown;
                report.diagnostics.push(diagnostic);
                super::ExactValidationStatus::Failed
            }
        }
    } else {
        super::ExactValidationStatus::NotAttempted
    };
    finish_report(
        report,
        model_metrics,
        construction_ms,
        search_ms,
        first_incumbent_ms,
        observed_incumbents,
        validation,
        objective_stages,
    )
}

fn validate_objective_witness(
    report: &IntegratedLayoutReport,
    stages: &[ExactObjectiveStageReport],
) -> Result<(), IntegratedLayoutDiagnostic> {
    let score = LayoutScore::from_report(report, &[]).ok_or_else(|| {
        IntegratedLayoutDiagnostic::error(
            "invalid-exact-objective-witness",
            "/exact/objective",
            None,
            "successful exact witness has no scoreable used geometry",
        )
    })?;
    for stage in stages {
        let Some(incumbent) = stage.incumbent else {
            continue;
        };
        let observed = match stage.objective {
            ExactObjectiveKind::UsedBoundingBoxArea => {
                i64::try_from(score.used_bounding_box_area).ok()
            }
            ExactObjectiveKind::PhysicalTransportTiles => {
                i64::try_from(score.physical_transport_tiles).ok()
            }
            ExactObjectiveKind::TotalRouteTurns => i64::try_from(score.total_route_turns).ok(),
            ExactObjectiveKind::MaximumUsedSide => Some(score.maximum_used_side),
            ExactObjectiveKind::LogisticsComponentCount => {
                i64::try_from(score.logistics_component_count).ok()
            }
        };
        if observed != Some(incumbent) {
            return Err(IntegratedLayoutDiagnostic::error(
                "invalid-exact-objective-witness",
                "/exact/objective",
                None,
                format!(
                    "solver objective {:?} is {incumbent}, but the extracted witness reports {:?}",
                    stage.objective, observed
                ),
            ));
        }
    }
    Ok(())
}
