use std::collections::{BTreeMap, BTreeSet};

use pumpkin_solver::core::results::ProblemSolution;

use super::super::{
    FacilityPlacement, INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic,
    IntegratedLayoutReport, IntegratedLayoutStatus, ModelInput, PlacedLogisticsComponent,
    TransportKind, TransportNetwork, TransportNetworkEndpoint, TransportNetworkSegment,
    TransportNetworkTerminal, WorldGridPosition, canonicalize_report_geometry, world_position,
};
use super::{Arc, EndpointOption, ModelBridge, ModelInstance, ModelRoute};
use crate::facilities::FacilityPortDirection;
use crate::logistics::LogisticsComponentKind;
use crate::recipes::Rate;

struct ExtractedPath {
    requirement_id: String,
    source: TransportNetworkEndpoint,
    target: TransportNetworkEndpoint,
    item: String,
    rate: Rate,
    transport: TransportKind,
    cells: Vec<WorldGridPosition>,
}

struct NetworkBuilder {
    id: String,
    requirement_ids: BTreeSet<String>,
    item: String,
    transport: TransportKind,
    cells: BTreeSet<(i64, i64)>,
    segments: BTreeMap<((i64, i64), (i64, i64)), Rate>,
    terminals: BTreeMap<TerminalKey, (TransportNetworkEndpoint, Rate)>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct TerminalKey {
    node: String,
    direction_rank: u8,
    endpoint_key: String,
    x: i64,
    y: i64,
}

pub(in crate::layouts::integrated) fn extract_report(
    solution: &impl ProblemSolution,
    status: IntegratedLayoutStatus,
    input: &ModelInput,
    instances: &[ModelInstance],
    model_routes: &[ModelRoute],
    model_bridges: &[ModelBridge],
) -> IntegratedLayoutReport {
    let mut placements = Vec::new();
    for instance in instances {
        let candidate = instance
            .candidates
            .iter()
            .find(|candidate| solution.get_integer_value(candidate.selected) == 1)
            .expect("exactly one placement candidate is selected");
        placements.push(FacilityPlacement {
            instance: instance.input.id.clone(),
            recipe: instance.input.recipe.clone(),
            facility: instance.input.facility.clone(),
            x: i64::from(candidate.x),
            y: i64::from(candidate.y),
            width: i64::from(candidate.width),
            height: i64::from(candidate.height),
            rotation: candidate.rotation,
        });
    }
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));

    let paths = input
        .edges
        .iter()
        .zip(model_routes)
        .map(|(edge, model_route)| {
            let source = selected_endpoint(solution, &model_route.source_options);
            let target = selected_endpoint(solution, &model_route.target_options);
            let cells = extract_path(
                solution,
                source.cell,
                target.cell,
                &model_route.arcs,
                input.width,
            );
            ExtractedPath {
                requirement_id: edge.requirement_id.clone(),
                source: source.endpoint.clone(),
                target: target.endpoint.clone(),
                item: edge.edge.item.clone(),
                rate: edge.edge.rate,
                transport: edge.transport,
                cells,
            }
        })
        .collect::<Vec<_>>();

    let logistics_components = model_bridges
        .iter()
        .filter(|bridge| solution.get_integer_value(bridge.selected) == 1)
        .map(|bridge| {
            let position = world_position(bridge.cell, input.width);
            let owners = paths
                .iter()
                .filter(|path| path.transport == bridge.transport && path.cells.contains(&position))
                .map(|path| path.requirement_id.clone())
                .collect::<BTreeSet<_>>();
            let rotation = bridge
                .rotations
                .iter()
                .find(|(_, selected)| solution.get_integer_value(*selected) == 1)
                .map(|(rotation, _)| *rotation)
                .expect("selected bridge has exactly one selected rotation");
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
        })
        .collect::<Vec<_>>();

    let mut transport_networks = project_paths_to_networks(&paths);
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
                if status == IntegratedLayoutStatus::Optimal {
                    "integrated-layout-optimal"
                } else {
                    "integrated-layout-feasible"
                },
                if status == IntegratedLayoutStatus::Optimal {
                    "facility placement, port selection, and routes are solved with proven minimum total route length"
                } else {
                    "facility placement, port selection, and routing are feasible but not proven optimal"
                },
            ),
            IntegratedLayoutDiagnostic::info(
                "transport-networks-projected-from-route-baseline",
                "physical transport networks are projected from the temporary route-per-requirement exact formulation; shared network flow is not implemented yet",
            ),
        ],
    };
    canonicalize_report_geometry(&mut report);
    report
}

fn project_paths_to_networks(paths: &[ExtractedPath]) -> Vec<TransportNetwork> {
    let mut builders = BTreeMap::<String, NetworkBuilder>::new();
    for path in paths {
        let id = network_id(path.transport, &path.item);
        let builder = builders
            .entry(id.clone())
            .or_insert_with(|| NetworkBuilder {
                id,
                requirement_ids: BTreeSet::new(),
                item: path.item.clone(),
                transport: path.transport,
                cells: BTreeSet::new(),
                segments: BTreeMap::new(),
                terminals: BTreeMap::new(),
            });
        builder.requirement_ids.insert(path.requirement_id.clone());
        builder
            .cells
            .extend(path.cells.iter().map(|cell| (cell.x, cell.y)));
        for cells in path.cells.windows(2) {
            let key = ((cells[0].x, cells[0].y), (cells[1].x, cells[1].y));
            let rate = builder.segments.entry(key).or_insert(Rate::zero());
            *rate = rate
                .checked_add(path.rate)
                .expect("validated route rates remain representable when projected");
        }
        add_terminal(
            builder,
            &path.source,
            FacilityPortDirection::Output,
            path.cells.first().expect("path is non-empty"),
            path.rate,
        );
        add_terminal(
            builder,
            &path.target,
            FacilityPortDirection::Input,
            path.cells.last().expect("path is non-empty"),
            path.rate,
        );
    }

    builders
        .into_values()
        .map(|builder| {
            let id = builder.id;
            let terminals = builder
                .terminals
                .into_iter()
                .enumerate()
                .map(
                    |(index, (key, (endpoint, rate)))| TransportNetworkTerminal {
                        id: format!("{id}:terminal:{index:04}"),
                        node: key.node,
                        direction: if key.direction_rank == 0 {
                            FacilityPortDirection::Input
                        } else {
                            FacilityPortDirection::Output
                        },
                        endpoint,
                        position: WorldGridPosition { x: key.x, y: key.y },
                        rate,
                    },
                )
                .collect();
            TransportNetwork {
                id,
                requirement_ids: builder.requirement_ids.into_iter().collect(),
                item: builder.item,
                transport: builder.transport,
                cells: builder
                    .cells
                    .into_iter()
                    .map(|(x, y)| WorldGridPosition { x, y })
                    .collect(),
                segments: builder
                    .segments
                    .into_iter()
                    .map(|((from, to), rate)| TransportNetworkSegment {
                        from: WorldGridPosition {
                            x: from.0,
                            y: from.1,
                        },
                        to: WorldGridPosition { x: to.0, y: to.1 },
                        rate,
                    })
                    .collect(),
                terminals,
                component_ids: Vec::new(),
            }
        })
        .collect()
}

fn add_terminal(
    builder: &mut NetworkBuilder,
    endpoint: &TransportNetworkEndpoint,
    direction: FacilityPortDirection,
    position: &WorldGridPosition,
    path_rate: Rate,
) {
    let key = TerminalKey {
        node: endpoint_node(endpoint).to_string(),
        direction_rank: u8::from(direction == FacilityPortDirection::Output),
        endpoint_key: endpoint_key(endpoint),
        x: position.x,
        y: position.y,
    };
    let (_, rate) = builder
        .terminals
        .entry(key)
        .or_insert_with(|| (endpoint.clone(), Rate::zero()));
    *rate = rate
        .checked_add(path_rate)
        .expect("validated terminal rates remain representable when projected");
}

fn endpoint_node(endpoint: &TransportNetworkEndpoint) -> &str {
    match endpoint {
        TransportNetworkEndpoint::Facility { instance, .. } => instance,
        TransportNetworkEndpoint::External { node, .. } => node,
    }
}

fn endpoint_key(endpoint: &TransportNetworkEndpoint) -> String {
    match endpoint {
        TransportNetworkEndpoint::Facility { instance, port } => {
            format!("facility:{instance}:{port}")
        }
        TransportNetworkEndpoint::External { node, side } => {
            format!("external:{node}:{side:?}")
        }
    }
}

fn network_id(transport: TransportKind, item: &str) -> String {
    let transport = match transport {
        TransportKind::Belt => "belt",
        TransportKind::Pipe => "pipe",
    };
    format!("network:{transport}:{item}")
}

fn selected_endpoint<'a>(
    solution: &impl ProblemSolution,
    options: &'a [EndpointOption],
) -> &'a EndpointOption {
    options
        .iter()
        .find(|option| solution.get_integer_value(option.selected) == 1)
        .expect("exactly one endpoint option is selected")
}

fn extract_path(
    solution: &impl ProblemSolution,
    source: usize,
    target: usize,
    arcs: &[Arc],
    width: i32,
) -> Vec<WorldGridPosition> {
    let mut next_by_cell = BTreeMap::new();
    for arc in arcs {
        if solution.get_integer_value(arc.selected) == 1 {
            next_by_cell.insert(arc.from, arc.to);
        }
    }
    let mut cells = vec![world_position(source, width)];
    let mut current = source;
    let mut seen = BTreeSet::from([source]);
    while current != target {
        current = *next_by_cell.get(&current).unwrap_or_else(|| {
            panic!(
                "solver route stops before target: source={source}, target={target}, current={current}, arcs={next_by_cell:?}"
            )
        });
        assert!(seen.insert(current), "solver route contains a cycle");
        cells.push(world_position(current, width));
    }
    cells
}
