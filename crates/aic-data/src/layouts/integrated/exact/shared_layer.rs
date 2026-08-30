use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::results::ProblemSolution;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::extract::rate_from_flow_units;
use super::formulation::{
    DIRECTIONS, direction_between, direction_index, external_endpoint_options, generate_candidates,
    grid_arcs, model_facility_endpoint_options, post_at_most_one, post_equals_one,
    rotate_direction,
};
use super::hint::SolverHint;
use super::metrics::{elapsed_millis, finish_report_with_formulation};
use super::objective::{
    ExactObjectives, optimise_lexicographically, post_and, post_count, post_exactly_one_indicator,
    post_sum_variable, require_canonical_origin,
};
use super::recorder::{ConstraintFamily, RecordedModel, VariableFamily};
use super::{
    Arc, Candidate, EdgeEndpointOptions, ModelBridge, ModelInstance, ModelTerminal, post_arm,
    post_presence,
};
use crate::facilities::FacilityPortDirection;
use crate::layouts::integrated::{
    ExactModelMetrics, ExactValidationStatus, FacilityPlacement, INTEGRATED_LAYOUT_SCHEMA_VERSION,
    IntegratedLayoutDiagnostic, IntegratedLayoutReport, IntegratedLayoutStatus, ModelInput,
    PlacedLogisticsComponent, TransportKind, TransportNetwork, TransportNetworkEndpoint,
    TransportNetworkSegment, TransportNetworkTerminal, canonicalize_report_geometry,
    world_position,
};
use crate::logistics::{
    CardinalDirection, LogisticsComponentKind, ValidatedLogisticsComponentCatalog,
};

#[derive(Debug)]
struct SharedBranchComponent {
    transport: TransportKind,
    cell: usize,
    component: String,
    kind: LogisticsComponentKind,
    rotation: i64,
    selected: DomainId,
}

struct SharedLayer {
    transport: TransportKind,
    network_indices: Vec<usize>,
    arcs: Vec<Arc>,
    route_cells: Vec<DomainId>,
    arm_items: Vec<[DomainId; 4]>,
}

pub(in crate::layouts::integrated) fn solve(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    let construction_started = Instant::now();
    let mut model_metrics = initial_metrics(&input);
    let cell_count = input.cell_count as usize;
    let mut solver = RecordedModel::default();
    let tag = solver.new_constraint_tag();

    let (model_instances, occupancy) =
        build_placements(&mut solver, &input, &mut model_metrics, cell_count, tag);
    if model_instances.is_empty() && !input.instances.is_empty() {
        return IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::Infeasible,
            IntegratedLayoutDiagnostic::error(
                "facility-has-no-placement-candidate",
                "/",
                None,
                "a facility has no rotation and origin within the hard layout bounds",
            ),
        );
    }
    let edge_endpoint_options = build_endpoint_options(
        &mut solver,
        &input,
        &model_instances,
        &mut model_metrics,
        tag,
    );
    let model_terminals = build_terminals(&input, &edge_endpoint_options);

    let mut layers = Vec::new();
    let mut branch_components = Vec::new();
    let mut bridges = Vec::new();
    for transport in [TransportKind::Belt, TransportKind::Pipe] {
        let network_indices = input
            .networks
            .iter()
            .enumerate()
            .filter_map(|(index, network)| (network.transport() == transport).then_some(index))
            .collect::<Vec<_>>();
        if network_indices.is_empty() {
            continue;
        }
        let layer = build_layer(
            &mut solver,
            &input,
            &model_terminals,
            &occupancy,
            transport,
            network_indices,
            logistics_components,
            &mut model_metrics,
            &mut branch_components,
            &mut bridges,
            tag,
        );
        layers.push(layer);
    }

    let objectives = match build_objectives(
        &mut solver,
        &input,
        &occupancy,
        &layers,
        &branch_components,
        &bridges,
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

    let (facility_network_incidences, shared_network_facility_pairs) =
        super::logical_coupling_metrics(&input);
    solver.set_logical_coupling(facility_network_incidences, shared_network_facility_pairs);
    let model_complexity = solver.metrics();
    let construction_ms = elapsed_millis(construction_started.elapsed());
    let search = optimise_lexicographically(
        solver.solver_mut(),
        objectives,
        &SolverHint::default(),
        time_limit,
        tag,
        |solution, status| {
            extract_report(
                solution,
                status,
                &input,
                &model_instances,
                &model_terminals,
                &layers,
                &branch_components,
                &bridges,
            )
        },
    );
    let mut report = search.report;
    let validation = if report.success {
        match crate::layouts::integrated::witness::validate(&input, logistics_components, &report) {
            Ok(()) => match super::validate_objective_witness(&report, &search.stages) {
                Ok(()) => ExactValidationStatus::Passed,
                Err(diagnostic) => {
                    report.success = false;
                    report.status = IntegratedLayoutStatus::Unknown;
                    report.diagnostics.push(diagnostic);
                    ExactValidationStatus::Failed
                }
            },
            Err(diagnostic) => {
                report.success = false;
                report.status = IntegratedLayoutStatus::Unknown;
                report.diagnostics.push(diagnostic);
                ExactValidationStatus::Failed
            }
        }
    } else {
        ExactValidationStatus::NotAttempted
    };
    finish_report_with_formulation(
        report,
        "joint-shared-transport-layer-v1",
        model_metrics,
        model_complexity,
        construction_ms,
        search.search_ms,
        search.first_incumbent_ms,
        search.incumbent_count,
        validation,
        search.stages,
    )
}

fn initial_metrics(input: &ModelInput) -> ExactModelMetrics {
    ExactModelMetrics {
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
    }
}

fn build_placements(
    solver: &mut RecordedModel,
    input: &ModelInput,
    metrics: &mut ExactModelMetrics,
    cell_count: usize,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (Vec<ModelInstance>, Vec<Vec<DomainId>>) {
    let mut occupancy = vec![Vec::new(); cell_count];
    let mut instances = Vec::with_capacity(input.instances.len());
    for instance in &input.instances {
        let candidates = generate_candidates(solver, instance, input.width, input.height);
        metrics.placement_variables += candidates.len();
        if candidates.is_empty() {
            return (Vec::new(), occupancy);
        }
        post_equals_one(
            solver,
            ConstraintFamily::PlacementChoice,
            candidates.iter().map(|candidate| candidate.selected),
            tag,
        );
        for candidate in &candidates {
            for cell in &candidate.occupied_cells {
                occupancy[*cell].push(candidate.selected);
            }
        }
        instances.push(ModelInstance {
            input: instance.clone(),
            candidates,
        });
    }
    for candidates in &occupancy {
        post_at_most_one(
            solver,
            ConstraintFamily::FacilityNonOverlap,
            candidates.iter().copied(),
            tag,
        );
    }
    (instances, occupancy)
}

fn build_endpoint_options(
    solver: &mut RecordedModel,
    input: &ModelInput,
    instances: &[ModelInstance],
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<EdgeEndpointOptions> {
    input
        .edges
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| {
            let (source, target) = match (&edge.source, &edge.target) {
                (
                    super::super::EndpointInput::Facility { .. },
                    super::super::EndpointInput::Facility { .. },
                ) => (
                    model_facility_endpoint_options(
                        solver,
                        edge_index,
                        "source",
                        &edge.source,
                        instances,
                        tag,
                    ),
                    model_facility_endpoint_options(
                        solver,
                        edge_index,
                        "target",
                        &edge.target,
                        instances,
                        tag,
                    ),
                ),
                (
                    super::super::EndpointInput::External { node },
                    super::super::EndpointInput::Facility { .. },
                ) => {
                    let target = model_facility_endpoint_options(
                        solver,
                        edge_index,
                        "target",
                        &edge.target,
                        instances,
                        tag,
                    );
                    (external_endpoint_options(node, &target), target)
                }
                (
                    super::super::EndpointInput::Facility { .. },
                    super::super::EndpointInput::External { node },
                ) => {
                    let source = model_facility_endpoint_options(
                        solver,
                        edge_index,
                        "source",
                        &edge.source,
                        instances,
                        tag,
                    );
                    let target = external_endpoint_options(node, &source);
                    (source, target)
                }
                (
                    super::super::EndpointInput::External { .. },
                    super::super::EndpointInput::External { .. },
                ) => unreachable!(),
            };
            metrics.endpoint_variables += match (&edge.source, &edge.target) {
                (
                    super::super::EndpointInput::Facility { .. },
                    super::super::EndpointInput::Facility { .. },
                ) => source.len() + target.len(),
                (
                    super::super::EndpointInput::External { .. },
                    super::super::EndpointInput::Facility { .. },
                ) => target.len(),
                (
                    super::super::EndpointInput::Facility { .. },
                    super::super::EndpointInput::External { .. },
                ) => source.len(),
                (
                    super::super::EndpointInput::External { .. },
                    super::super::EndpointInput::External { .. },
                ) => 0,
            };
            EdgeEndpointOptions { source, target }
        })
        .collect()
}

fn build_terminals(
    input: &ModelInput,
    edge_options: &[EdgeEndpointOptions],
) -> Vec<Vec<ModelTerminal>> {
    input
        .networks
        .iter()
        .map(|network| {
            network
                .terminals()
                .iter()
                .map(|terminal| {
                    let options = &edge_options[terminal.route_index()];
                    ModelTerminal {
                        id: terminal.id().to_string(),
                        direction: terminal.direction(),
                        rate: terminal.rate(),
                        flow_units: terminal.flow_units(),
                        options: if terminal.direction() == FacilityPortDirection::Output {
                            options.source.clone()
                        } else {
                            options.target.clone()
                        },
                    }
                })
                .collect()
        })
        .collect()
}

type TerminalContribution = (DomainId, i32, usize);

#[allow(clippy::too_many_arguments)]
fn build_layer(
    solver: &mut RecordedModel,
    input: &ModelInput,
    terminals: &[Vec<ModelTerminal>],
    facility_occupancy: &[Vec<DomainId>],
    transport: TransportKind,
    network_indices: Vec<usize>,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    metrics: &mut ExactModelMetrics,
    all_branch_components: &mut Vec<SharedBranchComponent>,
    all_bridges: &mut Vec<ModelBridge>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> SharedLayer {
    let cell_count = input.cell_count as usize;
    let transport_name = format!("{transport:?}").to_lowercase();
    let item_codes = network_indices
        .iter()
        .enumerate()
        .map(|(local_index, network_index)| (*network_index, local_index as i32 + 1))
        .collect::<BTreeMap<_, _>>();
    let item_count = i32::try_from(network_indices.len()).expect("network count fits i32");
    let maximum_capacity = network_indices
        .iter()
        .map(|network_index| input.networks[*network_index].line_capacity_units())
        .max()
        .expect("a shared layer has at least one network");
    let layer_index = usize::from(transport == TransportKind::Pipe);
    let (arcs, incoming, outgoing) = grid_arcs(
        solver,
        layer_index,
        input.width,
        input.height,
        maximum_capacity,
        tag,
    );
    metrics.route_arc_variables += arcs.len();
    metrics.network_flow_variables += arcs.len();

    let mut supply_by_cell: Vec<[Vec<TerminalContribution>; 4]> = (0..cell_count)
        .map(|_| std::array::from_fn(|_| Vec::new()))
        .collect();
    let mut demand_by_cell: Vec<[Vec<TerminalContribution>; 4]> = (0..cell_count)
        .map(|_| std::array::from_fn(|_| Vec::new()))
        .collect();
    for network_index in &network_indices {
        for terminal in &terminals[*network_index] {
            let destination = if terminal.direction == FacilityPortDirection::Output {
                &mut supply_by_cell
            } else {
                &mut demand_by_cell
            };
            for option in &terminal.options {
                destination[option.cell][direction_index(option.arm_direction)].push((
                    option.selected,
                    terminal.flow_units,
                    *network_index,
                ));
            }
        }
    }

    let mut incoming_arms_by_cell = Vec::with_capacity(cell_count);
    let mut outgoing_arms_by_cell = Vec::with_capacity(cell_count);
    let mut arm_items = Vec::with_capacity(cell_count);
    let mut route_cells = Vec::with_capacity(cell_count);
    let mut incoming_flow_by_cell = Vec::with_capacity(cell_count);
    let mut outgoing_flow_by_cell = Vec::with_capacity(cell_count);
    let item_rows = std::iter::once(vec![0, 0, 0])
        .chain((1..=item_count).flat_map(|item| [vec![1, 0, item], vec![0, 1, item]]))
        .collect::<Vec<_>>();

    for cell in 0..cell_count {
        let incoming_flow: [Vec<(DomainId, i32)>; 4] = std::array::from_fn(|direction| {
            let mut terms = supply_by_cell[cell][direction]
                .iter()
                .map(|(variable, units, _)| (*variable, *units))
                .collect::<Vec<_>>();
            terms.extend(
                incoming[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.from, input.width)) == direction
                    })
                    .map(|arc| (arc.flow, 1)),
            );
            terms
        });
        let outgoing_flow: [Vec<(DomainId, i32)>; 4] = std::array::from_fn(|direction| {
            let mut terms = demand_by_cell[cell][direction]
                .iter()
                .map(|(variable, units, _)| (*variable, *units))
                .collect::<Vec<_>>();
            terms.extend(
                outgoing[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.to, input.width)) == direction
                    })
                    .map(|arc| (arc.flow, 1)),
            );
            terms
        });

        let incoming_arms: [DomainId; 4] = std::array::from_fn(|direction| {
            let terminal_presence = post_presence(
                solver,
                VariableFamily::TerminalPresence,
                ConstraintFamily::TerminalPresence,
                format!(
                    "{transport_name}-cell-{cell}-{}-supply",
                    direction_name(direction)
                ),
                unique_variables(
                    supply_by_cell[cell][direction]
                        .iter()
                        .map(|(variable, _, _)| *variable),
                ),
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
                solver,
                format!(
                    "{transport_name}-cell-{cell}-{}-incoming",
                    direction_name(direction)
                ),
                terminal_presence,
                &grid_arcs,
                tag,
            )
        });
        let outgoing_arms: [DomainId; 4] = std::array::from_fn(|direction| {
            let terminal_presence = post_presence(
                solver,
                VariableFamily::TerminalPresence,
                ConstraintFamily::TerminalPresence,
                format!(
                    "{transport_name}-cell-{cell}-{}-demand",
                    direction_name(direction)
                ),
                unique_variables(
                    demand_by_cell[cell][direction]
                        .iter()
                        .map(|(variable, _, _)| *variable),
                ),
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
                solver,
                format!(
                    "{transport_name}-cell-{cell}-{}-outgoing",
                    direction_name(direction)
                ),
                terminal_presence,
                &grid_arcs,
                tag,
            )
        });
        let cell_arm_items: [DomainId; 4] = std::array::from_fn(|direction| {
            let item = solver.new_variable(
                VariableFamily::ArmItem,
                0,
                item_count,
                format!(
                    "{transport_name}-cell-{cell}-{}-item",
                    direction_name(direction)
                ),
            );
            solver.post_table(
                ConstraintFamily::ItemAssignment,
                vec![incoming_arms[direction], outgoing_arms[direction], item],
                item_rows.clone(),
                tag,
            );
            for (selected, _, network_index) in supply_by_cell[cell][direction]
                .iter()
                .chain(&demand_by_cell[cell][direction])
            {
                post_selected_item(
                    solver,
                    *selected,
                    item,
                    item_codes[network_index],
                    item_count,
                    tag,
                );
            }
            for network_index in &network_indices {
                let condition = solver.solver_mut().new_named_literal_for_predicate(
                    item.equality_predicate(item_codes[network_index]),
                    tag,
                    format!(
                        "{transport_name}-cell-{cell}-{}-is-item-{}",
                        direction_name(direction),
                        item_codes[network_index]
                    ),
                );
                let incoming_capacity = incoming[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.from, input.width)) == direction
                    })
                    .map(|arc| (arc.flow, 1))
                    .chain(
                        supply_by_cell[cell][direction]
                            .iter()
                            .filter(|(_, _, candidate)| candidate == network_index)
                            .map(|(variable, units, _)| (*variable, *units)),
                    )
                    .collect::<Vec<_>>();
                let outgoing_capacity = outgoing[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.to, input.width)) == direction
                    })
                    .map(|arc| (arc.flow, 1))
                    .chain(
                        demand_by_cell[cell][direction]
                            .iter()
                            .filter(|(_, _, candidate)| candidate == network_index)
                            .map(|(variable, units, _)| (*variable, *units)),
                    )
                    .collect::<Vec<_>>();
                for flow in [&incoming_capacity, &outgoing_capacity] {
                    if flow.is_empty() {
                        continue;
                    }
                    solver.post_implied_less_than_or_equals(
                        ConstraintFamily::LineCapacity,
                        flow.iter()
                            .map(|(variable, coefficient)| variable.scaled(*coefficient))
                            .collect(),
                        input.networks[*network_index].line_capacity_units(),
                        maximum_capacity.unsigned_abs() as u64,
                        condition,
                        item,
                        tag,
                    );
                }
            }
            item
        });

        for direction in 0..4 {
            solver.post_less_than_or_equals(
                ConstraintFamily::OpposingArms,
                vec![
                    incoming_arms[direction].scaled(1),
                    outgoing_arms[direction].scaled(1),
                ],
                1,
                1,
                tag,
            );
        }
        let mut conservation = outgoing_flow
            .iter()
            .flatten()
            .map(|(variable, coefficient)| variable.scaled(*coefficient))
            .collect::<Vec<_>>();
        conservation.extend(
            incoming_flow
                .iter()
                .flatten()
                .map(|(variable, coefficient)| variable.scaled(-*coefficient)),
        );
        solver.post_equals(
            ConstraintFamily::FlowConservation,
            conservation,
            0,
            maximum_capacity.unsigned_abs() as u64,
            tag,
        );
        let route_cell = post_presence(
            solver,
            VariableFamily::RouteCell,
            ConstraintFamily::RouteCellActivation,
            format!("{transport_name}-cell-{cell}-occupied"),
            incoming_arms
                .iter()
                .copied()
                .chain(outgoing_arms.iter().copied()),
            tag,
        );
        route_cells.push(route_cell);
        incoming_arms_by_cell.push(incoming_arms);
        outgoing_arms_by_cell.push(outgoing_arms);
        arm_items.push(cell_arm_items);
        incoming_flow_by_cell.push(incoming_flow);
        outgoing_flow_by_cell.push(outgoing_flow);
    }
    metrics.route_cell_variables += route_cells.len();

    for arc in &arcs {
        let from_direction = direction_index(direction_between(arc.from, arc.to, input.width));
        let to_direction = direction_index(direction_between(arc.to, arc.from, input.width));
        let condition = solver.solver_mut().new_named_literal_for_predicate(
            arc.selected.equality_predicate(1),
            tag,
            format!("{transport_name}-arc-{}-{}-selected", arc.from, arc.to),
        );
        solver.post_implied_binary_equals(
            ConstraintFamily::ItemAssignment,
            arm_items[arc.from][from_direction],
            arm_items[arc.to][to_direction],
            condition,
            arc.selected,
            tag,
        );
    }

    for cell in 0..cell_count {
        let (branches, bridge) = post_cell_topology(
            solver,
            input,
            transport,
            &network_indices,
            &item_codes,
            cell,
            &incoming[cell],
            &outgoing[cell],
            &incoming_arms_by_cell[cell],
            &outgoing_arms_by_cell[cell],
            &arm_items[cell],
            &incoming_flow_by_cell[cell],
            &outgoing_flow_by_cell[cell],
            route_cells[cell],
            &supply_by_cell[cell],
            &demand_by_cell[cell],
            logistics_components,
            metrics,
            tag,
        );
        all_branch_components.extend(branches);
        all_bridges.push(bridge);
        solver.post_less_than_or_equals(
            ConstraintFamily::TransportCollision,
            facility_occupancy[cell]
                .iter()
                .map(|variable| variable.scaled(1))
                .chain(std::iter::once(route_cells[cell].scaled(1)))
                .collect(),
            1,
            1,
            tag,
        );
    }

    SharedLayer {
        transport,
        network_indices,
        arcs,
        route_cells,
        arm_items,
    }
}

fn unique_variables(variables: impl Iterator<Item = DomainId>) -> impl Iterator<Item = DomainId> {
    variables.collect::<BTreeSet<_>>().into_iter()
}

fn direction_name(direction: usize) -> &'static str {
    match direction {
        0 => "north",
        1 => "east",
        2 => "south",
        3 => "west",
        _ => unreachable!(),
    }
}

fn post_selected_item(
    solver: &mut RecordedModel,
    selected: DomainId,
    item: DomainId,
    item_code: i32,
    _maximum_item_code: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let condition = solver
        .solver_mut()
        .new_literal_for_predicate(selected.equality_predicate(1), tag);
    solver.post_implied_equals(
        ConstraintFamily::ItemAssignment,
        vec![item.scaled(1)],
        item_code,
        1,
        condition,
        selected,
        tag,
    );
}

#[allow(clippy::too_many_arguments)]
fn post_cell_topology(
    solver: &mut RecordedModel,
    input: &ModelInput,
    transport: TransportKind,
    network_indices: &[usize],
    item_codes: &BTreeMap<usize, i32>,
    cell: usize,
    incoming_arcs: &[Arc],
    outgoing_arcs: &[Arc],
    incoming_arms: &[DomainId; 4],
    outgoing_arms: &[DomainId; 4],
    arm_items: &[DomainId; 4],
    _incoming_flow: &[Vec<(DomainId, i32)>; 4],
    _outgoing_flow: &[Vec<(DomainId, i32)>; 4],
    route_cell: DomainId,
    supply: &[Vec<TerminalContribution>; 4],
    demand: &[Vec<TerminalContribution>; 4],
    components: &ValidatedLogisticsComponentCatalog,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (Vec<SharedBranchComponent>, ModelBridge) {
    let transport_name = format!("{transport:?}").to_lowercase();
    let item_count = i32::try_from(network_indices.len()).expect("network count fits i32");
    let maximum_capacity = network_indices
        .iter()
        .map(|network_index| input.networks[*network_index].line_capacity_units())
        .max()
        .expect("a shared layer has at least one network");
    let maximum_total_flow = maximum_capacity
        .checked_mul(4)
        .expect("validated flow bounds fit shared topology constraints");

    let mut branches = Vec::new();
    for kind in [
        LogisticsComponentKind::Splitter,
        LogisticsComponentKind::Converger,
    ] {
        let definition = components
            .component_by_kind(transport, kind)
            .expect("validated catalog contains branch capabilities");
        for rotation in &definition.allowed_rotations {
            let selected = solver.new_variable(
                VariableFamily::BranchComponent,
                0,
                1,
                format!(
                    "{transport_name}-cell-{cell}-{:?}-rotation-{rotation}",
                    kind
                )
                .to_lowercase(),
            );
            let allowed_inputs = definition
                .input_directions
                .iter()
                .map(|direction| rotate_direction(*direction, *rotation))
                .collect::<Vec<_>>();
            let allowed_outputs = definition
                .output_directions
                .iter()
                .map(|direction| rotate_direction(*direction, *rotation))
                .collect::<Vec<_>>();
            for (index, direction) in DIRECTIONS.iter().enumerate() {
                if !allowed_inputs.contains(direction) {
                    solver.post_less_than_or_equals(
                        ConstraintFamily::BranchTopology,
                        vec![incoming_arms[index].scaled(1), selected.scaled(1)],
                        1,
                        1,
                        tag,
                    );
                }
                if !allowed_outputs.contains(direction) {
                    solver.post_less_than_or_equals(
                        ConstraintFamily::BranchTopology,
                        vec![outgoing_arms[index].scaled(1), selected.scaled(1)],
                        1,
                        1,
                        tag,
                    );
                }
            }
            branches.push(SharedBranchComponent {
                transport,
                cell,
                component: definition.id.clone(),
                kind,
                rotation: *rotation,
                selected,
            });
        }
    }
    metrics.branch_component_variables += branches.len();

    let bridge_definition = components
        .component_by_kind(transport, LogisticsComponentKind::Bridge)
        .expect("validated catalog contains bridge capabilities");
    let bridge_selected = solver.new_variable(
        VariableFamily::Bridge,
        0,
        1,
        format!("{transport_name}-bridge-{cell}"),
    );
    let bridge_rotations = bridge_definition
        .allowed_rotations
        .iter()
        .map(|rotation| {
            (
                *rotation,
                solver.new_variable(
                    VariableFamily::BridgeRotation,
                    0,
                    1,
                    format!("{transport_name}-bridge-{cell}-rotation-{rotation}"),
                ),
            )
        })
        .collect::<Vec<_>>();
    let mut rotation_definition = bridge_rotations
        .iter()
        .map(|(_, variable)| variable.scaled(1))
        .collect::<Vec<_>>();
    rotation_definition.push(bridge_selected.scaled(-1));
    solver.post_equals(
        ConstraintFamily::BridgeRotation,
        rotation_definition,
        0,
        1,
        tag,
    );
    metrics.bridge_variables += 1;
    metrics.bridge_rotation_variables += bridge_rotations.len();

    post_at_most_one(
        solver,
        ConstraintFamily::BranchTopology,
        branches
            .iter()
            .map(|component| component.selected)
            .chain(std::iter::once(bridge_selected)),
        tag,
    );

    let cell_item = solver.new_variable(
        VariableFamily::ArmItem,
        0,
        item_count,
        format!("{transport_name}-cell-{cell}-non-bridge-item"),
    );
    let cell_item_rows = std::iter::once(vec![0, 0, 0])
        .chain((1..=item_count).map(|item| vec![1, 0, item]))
        .chain(std::iter::once(vec![1, 1, 0]))
        .collect::<Vec<_>>();
    solver.post_table(
        ConstraintFamily::ItemAssignment,
        vec![route_cell, bridge_selected, cell_item],
        cell_item_rows,
        tag,
    );
    for direction in 0..4 {
        let arm_presence_terms = [incoming_arms[direction], outgoing_arms[direction]];
        let mut upper = vec![
            arm_items[direction].scaled(1),
            cell_item.scaled(-1),
            bridge_selected.scaled(-item_count),
        ];
        upper.extend(
            arm_presence_terms
                .iter()
                .map(|presence| presence.scaled(item_count)),
        );
        solver.post_less_than_or_equals(
            ConstraintFamily::ItemAssignment,
            upper,
            item_count,
            item_count.unsigned_abs() as u64,
            tag,
        );
        let mut lower = vec![
            cell_item.scaled(1),
            arm_items[direction].scaled(-1),
            bridge_selected.scaled(-item_count),
        ];
        lower.extend(
            arm_presence_terms
                .iter()
                .map(|presence| presence.scaled(item_count)),
        );
        solver.post_less_than_or_equals(
            ConstraintFamily::ItemAssignment,
            lower,
            item_count,
            item_count.unsigned_abs() as u64,
            tag,
        );
    }

    let splitters = branches
        .iter()
        .filter(|component| component.kind == LogisticsComponentKind::Splitter)
        .map(|component| component.selected)
        .collect::<Vec<_>>();
    let convergers = branches
        .iter()
        .filter(|component| component.kind == LogisticsComponentKind::Converger)
        .map(|component| component.selected)
        .collect::<Vec<_>>();
    let incoming_count = incoming_arms
        .iter()
        .map(|arm| arm.scaled(1))
        .collect::<Vec<_>>();
    let outgoing_count = outgoing_arms
        .iter()
        .map(|arm| arm.scaled(1))
        .collect::<Vec<_>>();
    let mut incoming_maximum = incoming_count.clone();
    incoming_maximum.extend(convergers.iter().map(|selected| selected.scaled(-2)));
    incoming_maximum.push(bridge_selected.scaled(-1));
    solver.post_less_than_or_equals(
        ConstraintFamily::BranchTopology,
        incoming_maximum,
        1,
        2,
        tag,
    );
    let mut outgoing_maximum = outgoing_count.clone();
    outgoing_maximum.extend(splitters.iter().map(|selected| selected.scaled(-2)));
    outgoing_maximum.push(bridge_selected.scaled(-1));
    solver.post_less_than_or_equals(
        ConstraintFamily::BranchTopology,
        outgoing_maximum,
        1,
        2,
        tag,
    );
    let mut splitter_minimum = outgoing_count.clone();
    splitter_minimum.extend(splitters.iter().map(|selected| selected.scaled(-2)));
    solver.post_greater_than_or_equals(
        ConstraintFamily::BranchTopology,
        splitter_minimum,
        0,
        2,
        tag,
    );
    let mut splitter_input = incoming_count.clone();
    splitter_input.extend(splitters.iter().map(|selected| selected.scaled(-1)));
    solver.post_greater_than_or_equals(ConstraintFamily::BranchTopology, splitter_input, 0, 1, tag);
    let mut converger_minimum = incoming_count;
    converger_minimum.extend(convergers.iter().map(|selected| selected.scaled(-2)));
    solver.post_greater_than_or_equals(
        ConstraintFamily::BranchTopology,
        converger_minimum,
        0,
        2,
        tag,
    );
    let mut converger_output = outgoing_count;
    converger_output.extend(convergers.iter().map(|selected| selected.scaled(-1)));
    solver.post_greater_than_or_equals(
        ConstraintFamily::BranchTopology,
        converger_output,
        0,
        1,
        tag,
    );

    for branch in &branches {
        for network_index in network_indices {
            let condition = solver.solver_mut().new_named_literal_for_predicate(
                cell_item.equality_predicate(item_codes[network_index]),
                tag,
                format!(
                    "{transport_name}-cell-{cell}-branch-item-{}",
                    item_codes[network_index]
                ),
            );
            let capacity = input.networks[*network_index].component_capacity_units(branch.kind);
            let mut terms = incoming_arcs
                .iter()
                .map(|arc| arc.flow.scaled(1))
                .chain(
                    supply
                        .iter()
                        .flatten()
                        .filter(|(_, _, candidate)| candidate == network_index)
                        .map(|(variable, coefficient, _)| variable.scaled(*coefficient)),
                )
                .collect::<Vec<_>>();
            terms.push(branch.selected.scaled(maximum_total_flow));
            solver.post_implied_less_than_or_equals(
                ConstraintFamily::BranchTopology,
                terms,
                capacity + maximum_total_flow,
                maximum_total_flow.unsigned_abs() as u64,
                condition,
                cell_item,
                tag,
            );
        }
    }

    let horizontal_incoming = incoming_arcs
        .iter()
        .filter(|arc| same_axis(cell, arc.from, input.width, true))
        .collect::<Vec<_>>();
    let horizontal_outgoing = outgoing_arcs
        .iter()
        .filter(|arc| same_axis(cell, arc.to, input.width, true))
        .collect::<Vec<_>>();
    let vertical_incoming = incoming_arcs
        .iter()
        .filter(|arc| same_axis(cell, arc.from, input.width, false))
        .collect::<Vec<_>>();
    let vertical_outgoing = outgoing_arcs
        .iter()
        .filter(|arc| same_axis(cell, arc.to, input.width, false))
        .collect::<Vec<_>>();
    let bridge_condition = solver.solver_mut().new_named_literal_for_predicate(
        bridge_selected.equality_predicate(1),
        tag,
        format!("{transport_name}-bridge-{cell}-selected-condition"),
    );
    let bridge_possible = [
        &horizontal_incoming,
        &horizontal_outgoing,
        &vertical_incoming,
        &vertical_outgoing,
    ]
    .iter()
    .all(|group| !group.is_empty());
    if !bridge_possible {
        solver.post_equals(
            ConstraintFamily::BridgeCrossing,
            vec![bridge_selected.scaled(1)],
            0,
            1,
            tag,
        );
    }
    for group in [
        &horizontal_incoming,
        &horizontal_outgoing,
        &vertical_incoming,
        &vertical_outgoing,
    ] {
        if group.is_empty() {
            continue;
        }
        solver.post_implied_equals(
            ConstraintFamily::BridgeCrossing,
            group.iter().map(|arc| arc.selected.scaled(1)).collect(),
            1,
            1,
            bridge_condition,
            bridge_selected,
            tag,
        );
    }
    for (incoming_axis, outgoing_axis) in [
        (&horizontal_incoming, &horizontal_outgoing),
        (&vertical_incoming, &vertical_outgoing),
    ] {
        let balance = incoming_axis
            .iter()
            .map(|arc| arc.flow.scaled(1))
            .chain(outgoing_axis.iter().map(|arc| arc.flow.scaled(-1)))
            .collect::<Vec<_>>();
        if balance.is_empty() {
            continue;
        }
        solver.post_implied_equals(
            ConstraintFamily::BridgeCrossing,
            balance,
            0,
            maximum_capacity.unsigned_abs() as u64,
            bridge_condition,
            bridge_selected,
            tag,
        );
    }
    solver.post_implied_binary_equals(
        ConstraintFamily::BridgeCrossing,
        arm_items[direction_index(CardinalDirection::West)],
        arm_items[direction_index(CardinalDirection::East)],
        bridge_condition,
        bridge_selected,
        tag,
    );
    solver.post_implied_binary_equals(
        ConstraintFamily::BridgeCrossing,
        arm_items[direction_index(CardinalDirection::North)],
        arm_items[direction_index(CardinalDirection::South)],
        bridge_condition,
        bridge_selected,
        tag,
    );
    for selected in unique_variables(
        supply
            .iter()
            .chain(demand)
            .flatten()
            .map(|(selected, _, _)| *selected),
    ) {
        solver.post_less_than_or_equals(
            ConstraintFamily::BridgeCrossing,
            vec![bridge_selected.scaled(1), selected.scaled(1)],
            1,
            1,
            tag,
        );
    }
    for (axis_item, incoming_axis) in [
        (
            arm_items[direction_index(CardinalDirection::West)],
            &horizontal_incoming,
        ),
        (
            arm_items[direction_index(CardinalDirection::North)],
            &vertical_incoming,
        ),
    ] {
        for network_index in network_indices {
            let condition = solver.solver_mut().new_named_literal_for_predicate(
                axis_item.equality_predicate(item_codes[network_index]),
                tag,
                format!(
                    "{transport_name}-bridge-{cell}-axis-item-{}",
                    item_codes[network_index]
                ),
            );
            let mut terms = incoming_axis
                .iter()
                .map(|arc| arc.flow.scaled(1))
                .collect::<Vec<_>>();
            terms.push(bridge_selected.scaled(maximum_total_flow));
            solver.post_implied_less_than_or_equals(
                ConstraintFamily::BridgeCrossing,
                terms,
                input.networks[*network_index]
                    .component_capacity_units(LogisticsComponentKind::Bridge)
                    + maximum_total_flow,
                maximum_total_flow.unsigned_abs() as u64,
                condition,
                axis_item,
                tag,
            );
        }
    }
    metrics.crossing_constraints += 13 + supply.iter().chain(demand).flatten().count();

    (
        branches,
        ModelBridge {
            transport,
            cell,
            component: bridge_definition.id.clone(),
            selected: bridge_selected,
            rotations: bridge_rotations,
        },
    )
}

fn same_axis(cell: usize, neighbor: usize, width: i32, horizontal: bool) -> bool {
    let width = usize::try_from(width).expect("validated width is positive");
    (cell / width == neighbor / width) == horizontal
}

#[allow(clippy::too_many_arguments)]
fn build_objectives(
    solver: &mut RecordedModel,
    input: &ModelInput,
    facility_occupancy: &[Vec<DomainId>],
    layers: &[SharedLayer],
    branches: &[SharedBranchComponent],
    bridges: &[ModelBridge],
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<ExactObjectives, IntegratedLayoutDiagnostic> {
    let cell_count = input.cell_count as usize;
    let used_cells = (0..cell_count)
        .map(|cell| {
            post_presence(
                solver,
                VariableFamily::Objective,
                ConstraintFamily::UsedGeometry,
                format!("used-geometry-cell-{cell}"),
                facility_occupancy[cell]
                    .iter()
                    .copied()
                    .chain(layers.iter().map(|layer| layer.route_cells[cell])),
                tag,
            )
        })
        .collect::<Vec<_>>();
    metrics.objective_variables += used_cells.len() + 2;
    require_canonical_origin(solver, input, &used_cells, tag);

    let used_width = solver.new_variable(
        VariableFamily::Objective,
        1,
        input.width,
        "used-bounding-box-width",
    );
    let used_height = solver.new_variable(
        VariableFamily::Objective,
        1,
        input.height,
        "used-bounding-box-height",
    );
    solver.post_maximum(
        ConstraintFamily::BoundingBox,
        used_cells
            .iter()
            .enumerate()
            .map(|(cell, used)| {
                let x = i32::try_from(cell).expect("grid index fits i32") % input.width;
                used.scaled(x + 1)
            })
            .collect(),
        used_width,
        input.width as u64,
        tag,
    );
    solver.post_maximum(
        ConstraintFamily::BoundingBox,
        used_cells
            .iter()
            .enumerate()
            .map(|(cell, used)| {
                let y = i32::try_from(cell).expect("grid index fits i32") / input.width;
                used.scaled(y + 1)
            })
            .collect(),
        used_height,
        input.height as u64,
        tag,
    );
    let used_bounding_box_area = solver.new_variable(
        VariableFamily::Objective,
        1,
        input.cell_count,
        "used-bounding-box-area",
    );
    solver.post_times(
        ConstraintFamily::BoundingBox,
        used_width,
        used_height,
        used_bounding_box_area,
        tag,
    );
    let maximum_used_side = solver.new_variable(
        VariableFamily::Objective,
        1,
        input.width.max(input.height),
        "maximum-used-side",
    );
    solver.post_maximum(
        ConstraintFamily::BoundingBox,
        vec![used_width.scaled(1), used_height.scaled(1)],
        maximum_used_side,
        1,
        tag,
    );
    metrics.objective_variables += 4;

    let physical_tiles = layers
        .iter()
        .flat_map(|layer| layer.route_cells.iter().copied())
        .collect::<Vec<_>>();
    let physical_transport_tiles =
        post_sum_variable(solver, "physical-transport-tiles", &physical_tiles, tag)?;
    metrics.objective_variables += 1;

    let mut turns = Vec::with_capacity(layers.len() * cell_count);
    for (layer_index, layer) in layers.iter().enumerate() {
        for cell in 0..cell_count {
            turns.push(post_shared_turn(
                solver,
                layer_index,
                cell,
                input.width,
                &layer.arcs,
                tag,
                metrics,
            ));
        }
    }
    let total_route_turns = post_sum_variable(solver, "total-route-turns", &turns, tag)?;
    metrics.objective_variables += 1;

    let component_variables = branches
        .iter()
        .map(|component| component.selected)
        .chain(bridges.iter().map(|bridge| bridge.selected))
        .collect::<Vec<_>>();
    let logistics_component_count = post_sum_variable(
        solver,
        "logistics-component-count",
        &component_variables,
        tag,
    )?;
    metrics.objective_variables += 1;

    Ok(ExactObjectives {
        used_bounding_box_area,
        physical_transport_tiles,
        total_route_turns,
        maximum_used_side,
        logistics_component_count,
    })
}

fn post_shared_turn(
    solver: &mut RecordedModel,
    layer_index: usize,
    cell: usize,
    width: i32,
    arcs: &[Arc],
    tag: pumpkin_solver::core::proof::ConstraintTag,
    metrics: &mut ExactModelMetrics,
) -> DomainId {
    let incoming = arcs.iter().filter(|arc| arc.to == cell).collect::<Vec<_>>();
    let outgoing = arcs
        .iter()
        .filter(|arc| arc.from == cell)
        .collect::<Vec<_>>();
    let incoming_count = post_count(
        solver,
        format!("layer-{layer_index}-cell-{cell}-incoming-segment-count"),
        incoming.iter().map(|arc| arc.selected),
        tag,
    );
    let outgoing_count = post_count(
        solver,
        format!("layer-{layer_index}-cell-{cell}-outgoing-segment-count"),
        outgoing.iter().map(|arc| arc.selected),
        tag,
    );
    let exactly_one_incoming = post_exactly_one_indicator(
        solver,
        format!("layer-{layer_index}-cell-{cell}-exactly-one-incoming"),
        incoming_count,
        incoming.len(),
        tag,
    );
    let exactly_one_outgoing = post_exactly_one_indicator(
        solver,
        format!("layer-{layer_index}-cell-{cell}-exactly-one-outgoing"),
        outgoing_count,
        outgoing.len(),
        tag,
    );
    let mut orthogonal_pairs = Vec::new();
    let width_usize = usize::try_from(width).expect("validated width is positive");
    for incoming_arc in &incoming {
        for outgoing_arc in &outgoing {
            let incoming_horizontal = cell / width_usize == incoming_arc.from / width_usize;
            let outgoing_horizontal = cell / width_usize == outgoing_arc.to / width_usize;
            if incoming_horizontal == outgoing_horizontal {
                continue;
            }
            orthogonal_pairs.push(post_and(
                solver,
                format!(
                    "layer-{layer_index}-cell-{cell}-turn-pair-{}-{}",
                    incoming_arc.from, outgoing_arc.to
                ),
                incoming_arc.selected,
                outgoing_arc.selected,
                tag,
            ));
        }
    }
    let has_orthogonal_pair = post_presence(
        solver,
        VariableFamily::Objective,
        ConstraintFamily::TurnDefinition,
        format!("layer-{layer_index}-cell-{cell}-has-orthogonal-pair"),
        orthogonal_pairs.iter().copied(),
        tag,
    );
    let exact_segments = post_and(
        solver,
        format!("layer-{layer_index}-cell-{cell}-exact-segments"),
        exactly_one_incoming,
        exactly_one_outgoing,
        tag,
    );
    metrics.objective_variables += 7 + orthogonal_pairs.len();
    post_and(
        solver,
        format!("layer-{layer_index}-cell-{cell}-turn"),
        exact_segments,
        has_orthogonal_pair,
        tag,
    )
}

#[allow(clippy::too_many_arguments)]
fn extract_report(
    solution: &impl ProblemSolution,
    status: IntegratedLayoutStatus,
    input: &ModelInput,
    instances: &[ModelInstance],
    terminals: &[Vec<ModelTerminal>],
    layers: &[SharedLayer],
    branches: &[SharedBranchComponent],
    bridges: &[ModelBridge],
) -> IntegratedLayoutReport {
    let mut placements = instances
        .iter()
        .map(|instance| {
            let candidate = selected_candidate(solution, &instance.candidates);
            FacilityPlacement {
                instance: instance.input.id.clone(),
                recipe: instance.input.recipe.clone(),
                facility: instance.input.facility.clone(),
                x: i64::from(candidate.x),
                y: i64::from(candidate.y),
                width: i64::from(candidate.width),
                height: i64::from(candidate.height),
                rotation: candidate.rotation,
            }
        })
        .collect::<Vec<_>>();
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));

    let mut transport_networks = input
        .networks
        .iter()
        .enumerate()
        .map(|(network_index, network)| {
            let layer = layers
                .iter()
                .find(|layer| layer.transport == network.transport())
                .expect("every network belongs to a shared layer");
            let code = layer
                .network_indices
                .iter()
                .position(|candidate| *candidate == network_index)
                .expect("shared layer contains the network") as i32
                + 1;
            let mut cells = layer
                .arm_items
                .iter()
                .enumerate()
                .filter(|(_, items)| {
                    items
                        .iter()
                        .any(|item| solution.get_integer_value(*item) == code)
                })
                .map(|(cell, _)| world_position(cell, input.width))
                .collect::<Vec<_>>();
            cells.sort_by_key(|position| (position.y, position.x));
            cells.dedup();
            let segments = layer
                .arcs
                .iter()
                .filter(|arc| solution.get_integer_value(arc.selected) == 1)
                .filter(|arc| {
                    let direction =
                        direction_index(direction_between(arc.from, arc.to, input.width));
                    solution.get_integer_value(layer.arm_items[arc.from][direction]) == code
                })
                .map(|arc| TransportNetworkSegment {
                    from: world_position(arc.from, input.width),
                    to: world_position(arc.to, input.width),
                    rate: rate_from_flow_units(
                        solution.get_integer_value(arc.flow),
                        network.flow_scale(),
                    ),
                })
                .collect::<Vec<_>>();
            let network_terminals = terminals[network_index]
                .iter()
                .map(|terminal| {
                    let option = terminal
                        .options
                        .iter()
                        .find(|option| solution.get_integer_value(option.selected) == 1)
                        .expect("exactly one endpoint option is selected");
                    TransportNetworkTerminal {
                        id: terminal.id.clone(),
                        node: endpoint_node(&option.endpoint).to_string(),
                        direction: terminal.direction,
                        endpoint: option.endpoint.clone(),
                        position: world_position(option.cell, input.width),
                        rate: terminal.rate,
                    }
                })
                .collect();
            TransportNetwork {
                id: network.id().to_string(),
                requirement_ids: network
                    .route_indices()
                    .iter()
                    .map(|route_index| input.edges[*route_index].requirement_id.clone())
                    .collect(),
                item: network.item().to_string(),
                transport: network.transport(),
                cells,
                segments,
                terminals: network_terminals,
                component_ids: Vec::new(),
            }
        })
        .collect::<Vec<_>>();

    let mut logistics_components = branches
        .iter()
        .filter(|component| solution.get_integer_value(component.selected) == 1)
        .map(|component| {
            let position = world_position(component.cell, input.width);
            let owners = transport_networks
                .iter()
                .filter(|network| {
                    network.transport == component.transport && network.cells.contains(&position)
                })
                .map(|network| network.id.clone())
                .collect::<BTreeSet<_>>();
            PlacedLogisticsComponent {
                id: super::super::identity::logistics_component_id(
                    component.kind,
                    component.transport,
                    position.x,
                    position.y,
                    &owners,
                ),
                component: component.component.clone(),
                kind: component.kind,
                transport: component.transport,
                position,
                rotation: component.rotation,
            }
        })
        .collect::<Vec<_>>();
    logistics_components.extend(
        bridges
            .iter()
            .filter(|bridge| solution.get_integer_value(bridge.selected) == 1)
            .map(|bridge| {
                let position = world_position(bridge.cell, input.width);
                let owners = transport_networks
                    .iter()
                    .filter(|network| {
                        network.transport == bridge.transport && network.cells.contains(&position)
                    })
                    .map(|network| network.id.clone())
                    .collect::<BTreeSet<_>>();
                let rotation = bridge
                    .rotations
                    .iter()
                    .find(|(_, selected)| solution.get_integer_value(*selected) == 1)
                    .map(|(rotation, _)| *rotation)
                    .expect("selected bridge has one rotation");
                PlacedLogisticsComponent {
                    id: super::super::identity::logistics_component_id(
                        LogisticsComponentKind::Bridge,
                        bridge.transport,
                        position.x,
                        position.y,
                        &owners,
                    ),
                    component: bridge.component.clone(),
                    kind: LogisticsComponentKind::Bridge,
                    transport: bridge.transport,
                    position,
                    rotation,
                }
            }),
    );
    for network in &mut transport_networks {
        network.component_ids = logistics_components
            .iter()
            .filter(|component| {
                component.transport == network.transport
                    && network.cells.contains(&component.position)
            })
            .map(|component| component.id.clone())
            .collect();
    }

    let mut report = IntegratedLayoutReport {
        schema_version: INTEGRATED_LAYOUT_SCHEMA_VERSION,
        success: true,
        status,
        bounds: None,
        placements,
        logistics_components,
        transport_networks,
        phases: Vec::new(),
        exact: None,
        diagnostics: vec![
            IntegratedLayoutDiagnostic::info(
                "experimental-shared-transport-layer",
                "facility placement, port assignment, and item-labelled flow were solved jointly on one physical grid per transport layer",
            ),
            IntegratedLayoutDiagnostic::info(
                if status == IntegratedLayoutStatus::Optimal {
                    "integrated-layout-optimal"
                } else {
                    "integrated-layout-feasible"
                },
                "the experimental shared-layer model produced a complete solver witness",
            ),
        ],
    };
    canonicalize_report_geometry(&mut report);
    report
}

fn selected_candidate<'a>(
    solution: &impl ProblemSolution,
    candidates: &'a [Candidate],
) -> &'a Candidate {
    candidates
        .iter()
        .find(|candidate| solution.get_integer_value(candidate.selected) == 1)
        .expect("exactly one placement candidate is selected")
}

fn endpoint_node(endpoint: &TransportNetworkEndpoint) -> &str {
    match endpoint {
        TransportNetworkEndpoint::Facility { instance, .. } => instance,
        TransportNetworkEndpoint::External { node, .. } => node,
    }
}
