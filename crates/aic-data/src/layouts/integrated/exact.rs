use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::sync::Arc as Shared;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pumpkin_solver::Solver;
use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::DefaultBrancher;
use pumpkin_solver::core::optimisation::OptimisationDirection;
use pumpkin_solver::core::optimisation::linear_sat_unsat::LinearSatUnsat;
use pumpkin_solver::core::results::{OptimisationResult, SolutionReference};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::{
    EndpointInput, ExactModelMetrics, FacilityPortEdge, InstanceInput, IntegratedLayoutDiagnostic,
    IntegratedLayoutReport, IntegratedLayoutStatus, ModelInput, TransportKind,
    TransportNetworkEndpoint, ValidatedLogisticsComponentCatalog, witness,
};
use crate::facilities::FacilityPortDirection;
use crate::logistics::LogisticsComponentKind;

mod extract;
mod formulation;
mod metrics;

use extract::extract_report;
use formulation::{
    external_endpoint_options, generate_candidates, grid_arcs, incident_arcs_by_axis,
    model_facility_endpoint_options, post_acyclic_network_ordering, post_at_most_one,
    post_bridge_crossing, post_equals_one,
};
use metrics::{elapsed_millis, finish_report};

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

pub(super) fn solve(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
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
    let mut route_arc_variables = Vec::new();
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
        let mut supply_by_cell = vec![Vec::<(DomainId, i32)>::new(); cell_count];
        let mut demand_by_cell = vec![Vec::<(DomainId, i32)>::new(); cell_count];
        for terminal in &terminals {
            let destination = if terminal.direction == FacilityPortDirection::Output {
                &mut supply_by_cell
            } else {
                &mut demand_by_cell
            };
            for option in &terminal.options {
                destination[option.cell].push((option.selected, terminal.flow_units));
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
                    .map(|(variable, units)| variable.scaled(-*units)),
            );
            conservation.extend(
                demand_by_cell[cell]
                    .iter()
                    .map(|(variable, units)| variable.scaled(*units)),
            );
            solver
                .add_constraint(pumpkin_solver::equals(conservation, 0, tag))
                .post();

            let supply_present = post_presence(
                &mut solver,
                format!("network-{network_index}-cell-{cell}-supply"),
                supply_by_cell[cell].iter().map(|(variable, _)| *variable),
                tag,
            );
            let demand_present = post_presence(
                &mut solver,
                format!("network-{network_index}-cell-{cell}-demand"),
                demand_by_cell[cell].iter().map(|(variable, _)| *variable),
                tag,
            );

            let mut incoming_arms = incoming[cell]
                .iter()
                .map(|arc| arc.selected.scaled(1))
                .collect::<Vec<_>>();
            incoming_arms.push(supply_present.scaled(1));
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(incoming_arms, 1, tag))
                .post();
            let mut outgoing_arms = outgoing[cell]
                .iter()
                .map(|arc| arc.selected.scaled(1))
                .collect::<Vec<_>>();
            outgoing_arms.push(demand_present.scaled(1));
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(outgoing_arms, 1, tag))
                .post();

            let mut supply_capacity = supply_by_cell[cell]
                .iter()
                .map(|(variable, units)| variable.scaled(*units))
                .collect::<Vec<_>>();
            supply_capacity.push(supply_present.scaled(-network.line_capacity_units()));
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(supply_capacity, 0, tag))
                .post();
            let mut demand_capacity = demand_by_cell[cell]
                .iter()
                .map(|(variable, units)| variable.scaled(*units))
                .collect::<Vec<_>>();
            demand_capacity.push(demand_present.scaled(-network.line_capacity_units()));
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(demand_capacity, 0, tag))
                .post();

            let active_variables = incoming[cell]
                .iter()
                .chain(&outgoing[cell])
                .map(|arc| arc.selected)
                .chain(supply_by_cell[cell].iter().map(|(variable, _)| *variable))
                .chain(demand_by_cell[cell].iter().map(|(variable, _)| *variable))
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
        route_arc_variables.extend(arcs.iter().map(|arc| arc.selected));
        model_networks.push(ModelNetwork {
            input_index: network_index,
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

    let route_length =
        solver.new_named_bounded_integer(0, route_arc_variables.len() as i32, "total-route-length");
    let mut route_length_definition = route_arc_variables
        .iter()
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    route_length_definition.push(route_length.scaled(-1));
    solver
        .add_constraint(pumpkin_solver::equals(route_length_definition, 0, tag))
        .post();

    let construction_ms = elapsed_millis(construction_started.elapsed());
    let mut brancher = solver.default_brancher();
    let mut resolver = ResolutionResolver::default();
    let incumbent_count = Shared::new(AtomicUsize::new(0));
    let callback_incumbent_count = Shared::clone(&incumbent_count);
    let callback = move |_: &Solver,
                         _: SolutionReference,
                         _: &DefaultBrancher,
                         _: &ResolutionResolver|
          -> ControlFlow<()> {
        callback_incumbent_count.fetch_add(1, Ordering::Relaxed);
        ControlFlow::Continue(())
    };
    let mut termination = time_limit.map(TimeBudget::starting_now);
    let search_started = Instant::now();
    let result = solver.optimise(
        &mut brancher,
        &mut termination,
        &mut resolver,
        LinearSatUnsat::new(OptimisationDirection::Minimise, route_length, callback),
    );
    let search_ms = elapsed_millis(search_started.elapsed());

    let mut report = match result {
        OptimisationResult::Optimal(solution) => extract_report(
            &solution,
            IntegratedLayoutStatus::Optimal,
            &input,
            &model_instances,
            &model_networks,
            &model_bridges,
        ),
        OptimisationResult::Satisfiable(solution) | OptimisationResult::Stopped(solution, ()) => {
            extract_report(
                &solution,
                IntegratedLayoutStatus::Feasible,
                &input,
                &model_instances,
                &model_networks,
                &model_bridges,
            )
        }
        OptimisationResult::Unsatisfiable => IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::Infeasible,
            IntegratedLayoutDiagnostic::error(
                "integrated-layout-infeasible",
                "/",
                None,
                "facility placement, port selection, and route constraints are infeasible",
            ),
        ),
        OptimisationResult::Unknown => IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::Unknown,
            IntegratedLayoutDiagnostic::error(
                "integrated-layout-unknown",
                "/",
                None,
                "solver terminated without a solution or proof",
            ),
        ),
    };
    let validation = if report.success {
        match witness::validate(&input, logistics_components, &report) {
            Ok(()) => super::ExactValidationStatus::Passed,
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
    let observed_incumbents = incumbent_count.load(Ordering::Relaxed);
    finish_report(
        report,
        model_metrics,
        construction_ms,
        search_ms,
        observed_incumbents,
        validation,
    )
}
