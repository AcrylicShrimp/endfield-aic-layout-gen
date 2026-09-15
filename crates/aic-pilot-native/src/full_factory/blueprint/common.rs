//! Factory admission into the source-independent physical editor boundary.

use std::collections::{BTreeMap, BTreeSet};

use aic_data::layouts::{FactoryEditCatalogs, TransportNetworkEndpoint};
use aic_layout::editor::{
    COMPILED_SCHEMA, CompiledBlock, CompiledComponent, CompiledEdge, CompiledNetwork, CompiledNode,
    CompiledProblemV1, CompiledTerminal, DRAFT_SCHEMA, PhysicalDraftV1, PhysicalPose,
    UsedDefinitionCatalog,
};

use super::*;

pub(super) fn problem(
    blueprint: &Blueprint,
    catalogs: &FactoryEditCatalogs,
    catalog_sha256: &str,
) -> Result<CompiledProblemV1> {
    let mut used_facilities = BTreeSet::new();
    let facilities = blueprint
        .facilities
        .iter()
        .filter(|block| used_facilities.insert(block.facility.as_str()))
        .map(|block| {
            catalogs
                .facilities
                .facility(&block.facility)
                .unwrap()
                .clone()
        })
        .collect();
    let mut used_components = BTreeSet::new();
    let components = blueprint
        .components
        .iter()
        .filter(|component| used_components.insert(component.component.as_str()))
        .map(|component| {
            catalogs
                .components
                .component(&component.component)
                .unwrap()
                .clone()
        })
        .collect();
    let transports = catalogs.transports.catalog().transports.clone();
    let block_indices: BTreeMap<_, _> = blueprint
        .facilities
        .iter()
        .enumerate()
        .map(|(index, block)| (block.instance.as_str(), index))
        .collect();
    let component_indices: BTreeMap<_, _> = blueprint
        .components
        .iter()
        .enumerate()
        .map(|(index, component)| (component.id.as_str(), index))
        .collect();

    let result = CompiledProblemV1 {
        schema: COMPILED_SCHEMA.into(),
        catalog_sha256: catalog_sha256.into(),
        used_catalog: UsedDefinitionCatalog {
            facilities,
            components,
            transports,
        },
        blocks: blueprint
            .facilities
            .iter()
            .map(|block| CompiledBlock {
                id: block.instance.clone(),
                facility: block.facility.clone(),
                baseline_rotation: block.rotation,
            })
            .collect(),
        networks: blueprint
            .networks
            .iter()
            .map(|network| {
                Ok(CompiledNetwork {
                    id: network["id"]
                        .as_str()
                        .ok_or_else(|| fail("network", "Compiled network has no ID", "network"))?
                        .into(),
                    commodity: network["item"]
                        .as_str()
                        .ok_or_else(|| {
                            fail("network", "Compiled network has no commodity", "network")
                        })?
                        .into(),
                    transport: serde_json::from_value(network["transport"].clone()).map_err(
                        |_| {
                            fail(
                                "network",
                                "Compiled network has invalid transport",
                                "network",
                            )
                        },
                    )?,
                })
            })
            .collect::<Result<_>>()?,
        nodes: blueprint
            .nodes
            .iter()
            .map(|node| {
                let terminal = node
                    .terminal
                    .as_ref()
                    .map(|terminal| match &terminal.endpoint {
                        TransportNetworkEndpoint::Facility { instance, port } => {
                            Ok(CompiledTerminal::Facility {
                                block: *block_indices.get(instance.as_str()).ok_or_else(|| {
                                    fail("terminal", "Terminal owner is absent", instance.clone())
                                })?,
                                source_terminal: terminal.id.clone(),
                                original_port: port.clone(),
                                direction: terminal.direction,
                                rate: terminal.rate,
                            })
                        }
                        TransportNetworkEndpoint::External { side, .. } => {
                            Ok(CompiledTerminal::External {
                                source_terminal: terminal.id.clone(),
                                direction: terminal.direction,
                                rate: terminal.rate,
                                side: Some(*side),
                            })
                        }
                    })
                    .transpose()?;
                Ok(CompiledNode {
                    id: node.id,
                    source_id: node.terminal.as_ref().map_or_else(
                        || format!("node-{}", node.id),
                        |terminal| terminal.id.clone(),
                    ),
                    network: node.network,
                    baseline_position: node.initial_position,
                    terminal,
                    components: node
                        .components
                        .iter()
                        .map(|id| {
                            component_indices.get(id.as_str()).copied().ok_or_else(|| {
                                fail("component", "Node component is absent", id.clone())
                            })
                        })
                        .collect::<Result<_>>()?,
                })
            })
            .collect::<Result<_>>()?,
        edges: blueprint
            .edges
            .iter()
            .map(|edge| CompiledEdge {
                id: edge.id,
                source_id: format!("edge-{}", edge.id),
                network: edge.network,
                source: edge.source,
                target: edge.target,
                rate: edge.rate,
                first: edge.first,
                last: edge.last,
                zero_length: edge.zero_length,
            })
            .collect(),
        components: blueprint
            .components
            .iter()
            .map(|component| CompiledComponent {
                id: component.id.clone(),
                definition: component.component.clone(),
                kind: component.kind,
                transport: component.transport,
                rotation: component.rotation,
                channels: blueprint
                    .nodes
                    .iter()
                    .filter(|node| node.components.iter().any(|id| id == &component.id))
                    .map(|node| node.id)
                    .collect(),
            })
            .collect(),
    };
    Ok(result)
}

pub(super) fn draft(state: &declarative::State) -> PhysicalDraftV1 {
    PhysicalDraftV1 {
        schema: DRAFT_SCHEMA.into(),
        poses: state
            .geometry
            .placements
            .iter()
            .map(|placement| PhysicalPose {
                position: Point {
                    x: placement.x,
                    y: placement.y,
                },
                rotation: placement.rotation,
            })
            .collect(),
        selected_ports: state.ports.clone(),
        anchors: state.anchors.clone(),
        component_positions: state.components.iter().map(Point::from).collect(),
        paths: state.geometry.paths.clone(),
    }
}

pub(super) fn project(
    problem: &CompiledProblemV1,
    source: &PhysicalDraftV1,
    blueprint: &Blueprint,
) -> declarative::State {
    let mut placements = blueprint.facilities.clone();
    for ((placement, pose), _block, definition) in placements
        .iter_mut()
        .zip(&source.poses)
        .zip(&problem.blocks)
        .map(|((placement, pose), block)| {
            let definition = problem
                .used_catalog
                .facilities
                .iter()
                .find(|definition| definition.id == block.facility)
                .expect("validated common facility");
            ((placement, pose), block, definition)
        })
    {
        let (width, height, _) =
            aic_layout::projection::rotate(definition, pose.rotation, pose.position);
        placement.x = pose.position.x;
        placement.y = pose.position.y;
        placement.width = width;
        placement.height = height;
        placement.rotation = pose.rotation;
    }
    declarative::State {
        geometry: Draft {
            placements,
            paths: source.paths.clone(),
        },
        ports: source.selected_ports.clone(),
        anchors: source.anchors.clone(),
        components: source
            .component_positions
            .iter()
            .copied()
            .map(Into::into)
            .collect(),
    }
}
