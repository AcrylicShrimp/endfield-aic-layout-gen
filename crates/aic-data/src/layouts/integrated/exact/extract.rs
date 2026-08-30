use std::collections::BTreeSet;

use pumpkin_solver::core::results::ProblemSolution;

use super::super::{
    FacilityPlacement, INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic,
    IntegratedLayoutReport, IntegratedLayoutStatus, ModelInput, PlacedLogisticsComponent,
    TransportNetwork, TransportNetworkEndpoint, TransportNetworkSegment, TransportNetworkTerminal,
    canonicalize_report_geometry, world_position,
};
use super::{EndpointOption, ModelBranchComponent, ModelBridge, ModelInstance, ModelNetwork};
use crate::logistics::LogisticsComponentKind;
use crate::recipes::Rate;

pub(in crate::layouts::integrated) fn extract_report(
    solution: &impl ProblemSolution,
    status: IntegratedLayoutStatus,
    input: &ModelInput,
    instances: &[ModelInstance],
    model_networks: &[ModelNetwork],
    model_branch_components: &[ModelBranchComponent],
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

    let mut transport_networks = model_networks
        .iter()
        .map(|model_network| {
            let network = &input.networks[model_network.input_index];
            let cells = model_network
                .route_cells
                .iter()
                .enumerate()
                .filter(|(_, selected)| solution.get_integer_value(**selected) == 1)
                .map(|(cell, _)| world_position(cell, input.width))
                .collect::<Vec<_>>();
            let segments = model_network
                .arcs
                .iter()
                .filter(|arc| solution.get_integer_value(arc.selected) == 1)
                .map(|arc| {
                    let flow_units = solution.get_integer_value(arc.flow);
                    assert!(flow_units > 0, "selected network arc carries positive flow");
                    TransportNetworkSegment {
                        from: world_position(arc.from, input.width),
                        to: world_position(arc.to, input.width),
                        rate: rate_from_flow_units(flow_units, network.flow_scale()),
                    }
                })
                .collect::<Vec<_>>();
            let terminals = model_network
                .terminals
                .iter()
                .map(|terminal| {
                    let option = selected_endpoint(solution, &terminal.options);
                    TransportNetworkTerminal {
                        id: terminal.id.clone(),
                        node: endpoint_node(&option.endpoint).to_string(),
                        direction: terminal.direction,
                        endpoint: option.endpoint.clone(),
                        position: world_position(option.cell, input.width),
                        rate: terminal.rate,
                    }
                })
                .collect::<Vec<_>>();
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
                terminals,
                component_ids: Vec::new(),
            }
        })
        .collect::<Vec<_>>();

    let mut logistics_components = model_branch_components
        .iter()
        .filter(|component| solution.get_integer_value(component.selected) == 1)
        .map(|component| {
            let position = world_position(component.cell, input.width);
            let network_id = input.networks[component.network_index].id().to_string();
            PlacedLogisticsComponent {
                id: super::super::identity::logistics_component_id(
                    component.kind,
                    component.transport,
                    position.x,
                    position.y,
                    &BTreeSet::from([network_id]),
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
        model_bridges
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
            .collect::<Vec<_>>(),
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
                if status == IntegratedLayoutStatus::Optimal {
                    "integrated-layout-optimal"
                } else {
                    "integrated-layout-feasible"
                },
                if status == IntegratedLayoutStatus::Optimal {
                    "facility placement, port selection, and solver-selected commodity flow are solved with a proven lexicographic layout optimum"
                } else {
                    "facility placement, port selection, and solver-selected commodity flow are feasible but not proven optimal"
                },
            ),
            IntegratedLayoutDiagnostic::info(
                "solver-selected-logistics-components",
                "same-item flow, splitter positions, converger positions, component rotations, and bridge crossings are selected inside the joint solver model",
            ),
        ],
    };
    canonicalize_report_geometry(&mut report);
    report
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

fn endpoint_node(endpoint: &TransportNetworkEndpoint) -> &str {
    match endpoint {
        TransportNetworkEndpoint::Facility { instance, .. } => instance,
        TransportNetworkEndpoint::External { node, .. } => node,
    }
}

fn rate_from_flow_units(flow_units: i32, flow_scale: i64) -> Rate {
    let divisor = gcd(i64::from(flow_units), flow_scale);
    Rate {
        numerator: i64::from(flow_units) / divisor,
        denominator: flow_scale / divisor,
    }
}

fn gcd(mut left: i64, mut right: i64) -> i64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}
