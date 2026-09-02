use std::collections::{BTreeMap, BTreeSet};

use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{
    FacilityPlacementBounds, FacilityPlacementReport, FacilityPlacementRequest,
    FacilityPlacementStatus, SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION, project_facility_ports,
};
use crate::logistics::ValidatedItemCatalog;
use crate::recipes::{FacilityInstanceWiringNode, FacilityInstanceWiringReport, Rate};

use super::first_pipe_frontier::{FacilityInstance, occupied_cells, validate_inputs};
use super::pipe_chain::{GrowthEdge, construct_selected_growth};
use super::{
    CONSTRUCTIVE_FRONTIER_GROWTH_SCHEMA_VERSION, CONSTRUCTIVE_PROCESS_MODULE_SCHEMA_VERSION,
    ConstructiveFrontierDiagnostic, ConstructiveFrontierGrowthReport,
    ConstructiveFrontierGrowthStatistics, ConstructiveFrontierGrowthStatus,
    ConstructiveProcessModuleBoundary, ConstructiveProcessModuleReport,
};

pub fn construct_process_module(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    root_instance: &str,
    internal_item: &str,
) -> ConstructiveProcessModuleReport {
    if let Some(diagnostic) = validate_inputs(wiring) {
        return failure(root_instance, internal_item, diagnostic);
    }
    let instances = wiring
        .nodes
        .iter()
        .filter_map(|node| match node {
            FacilityInstanceWiringNode::Facility {
                id,
                recipe,
                facility,
                ..
            } => Some((
                id.as_str(),
                FacilityInstance {
                    id: id.clone(),
                    recipe: recipe.clone(),
                    facility: facility.clone(),
                },
            )),
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();
    let Some(root) = instances.get(root_instance).cloned() else {
        return failure(
            root_instance,
            internal_item,
            ConstructiveFrontierDiagnostic::error(
                "missing-process-module-root",
                "/root_instance",
                Some(root_instance.to_string()),
                format!("process module root facility instance '{root_instance}' does not exist"),
            ),
        );
    };
    let Some(item) = items.item(internal_item) else {
        return failure(
            root_instance,
            internal_item,
            ConstructiveFrontierDiagnostic::error(
                "missing-process-module-item",
                "/internal_item",
                Some(internal_item.to_string()),
                format!("process module internal item '{internal_item}' does not exist"),
            ),
        );
    };

    let mut selected_edges = wiring
        .edges
        .iter()
        .filter(|edge| edge.target == root_instance && edge.item == internal_item)
        .filter_map(|edge| {
            instances
                .get(edge.source.as_str())
                .map(|source| GrowthEdge {
                    edge,
                    source: source.clone(),
                    target: root.clone(),
                    transport: item.transport,
                })
        })
        .collect::<Vec<_>>();
    selected_edges.sort_by(|left, right| left.edge.id.cmp(&right.edge.id));
    if selected_edges.is_empty() {
        return failure(
            root_instance,
            internal_item,
            ConstructiveFrontierDiagnostic::error(
                "no-process-module-internal-requirement",
                "/edges",
                Some(root_instance.to_string()),
                format!(
                    "root facility instance '{root_instance}' has no facility-supplied '{internal_item}' input"
                ),
            ),
        );
    }

    let internal_requirements = selected_edges
        .iter()
        .map(|edge| edge.edge.id.clone())
        .collect::<Vec<_>>();
    let member_instances = selected_edges
        .iter()
        .flat_map(|edge| [edge.source.id.clone(), edge.target.id.clone()])
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut growth = construct_selected_growth(
        selected_edges,
        facilities,
        0,
        "constructed a process module with every selected internal requirement physically routed",
    );
    let boundary_requirements = if growth.success {
        match project_boundary_requirements(wiring, facilities, items, &member_instances, &growth) {
            Ok(boundary) => boundary,
            Err(diagnostic) => {
                growth.success = false;
                growth.status = ConstructiveFrontierGrowthStatus::Exhausted;
                growth.diagnostics.push(diagnostic);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    ConstructiveProcessModuleReport {
        schema_version: CONSTRUCTIVE_PROCESS_MODULE_SCHEMA_VERSION,
        success: growth.success,
        root_instance: root_instance.to_string(),
        internal_item: internal_item.to_string(),
        member_instances,
        internal_requirements,
        boundary_requirements,
        growth,
    }
}

fn project_boundary_requirements(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    member_instances: &[String],
    growth: &ConstructiveFrontierGrowthReport,
) -> Result<Vec<ConstructiveProcessModuleBoundary>, ConstructiveFrontierDiagnostic> {
    let members = member_instances
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let bounds = growth.bounds.as_ref().ok_or_else(|| {
        ConstructiveFrontierDiagnostic::error(
            "missing-process-module-bounds",
            "/growth/bounds",
            None,
            "constructed process module has no used bounds",
        )
    })?;
    let translated = growth
        .placements
        .iter()
        .cloned()
        .map(|mut placement| {
            placement.x += 1;
            placement.y += 1;
            placement
        })
        .collect::<Vec<_>>();
    let placement_report = FacilityPlacementReport {
        success: true,
        status: FacilityPlacementStatus::Feasible,
        bounds: Some(FacilityPlacementBounds {
            width: bounds.width + 2,
            height: bounds.height + 2,
        }),
        placements: translated,
        diagnostics: Vec::new(),
    };
    let request = FacilityPlacementRequest {
        schema_version: SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
        max_width: bounds.width + 2,
        max_height: bounds.height + 2,
    };
    let mut projection = project_facility_ports(&placement_report, facilities, &request);
    if !projection.success {
        return Err(ConstructiveFrontierDiagnostic::error(
            "process-module-boundary-projection-failed",
            "/growth/placements",
            None,
            "failed to project process-module boundary port options",
        ));
    }
    for port in &mut projection.ports {
        port.position.x -= 1;
        port.position.y -= 1;
        port.connection.x -= 1;
        port.connection.y -= 1;
    }

    let occupied_facilities = occupied_cells(&growth.placements);
    let occupied_transport = growth
        .transport_networks
        .iter()
        .flat_map(|network| {
            network
                .cells
                .iter()
                .map(move |cell| (network.transport, cell.x, cell.y))
        })
        .collect::<BTreeSet<_>>();
    let used_ports = growth
        .transport_networks
        .iter()
        .flat_map(|network| &network.terminals)
        .filter_map(|terminal| match &terminal.endpoint {
            crate::layouts::TransportNetworkEndpoint::Facility { instance, port } => {
                Some((instance.as_str(), port.as_str()))
            }
            crate::layouts::TransportNetworkEndpoint::External { .. } => None,
        })
        .collect::<BTreeSet<_>>();

    let mut boundary = Vec::new();
    for edge in &wiring.edges {
        let source_inside = members.contains(edge.source.as_str());
        let target_inside = members.contains(edge.target.as_str());
        if source_inside == target_inside {
            continue;
        }
        let Some(item) = items.item(&edge.item) else {
            return Err(ConstructiveFrontierDiagnostic::error(
                "missing-process-module-boundary-item",
                "/edges",
                Some(edge.item.clone()),
                format!(
                    "process module boundary references missing item '{}'",
                    edge.item
                ),
            ));
        };
        let (inside_instance, direction) = if source_inside {
            (edge.source.as_str(), FacilityPortDirection::Output)
        } else {
            (edge.target.as_str(), FacilityPortDirection::Input)
        };
        let port_options = projection
            .ports
            .iter()
            .filter(|port| {
                port.instance == inside_instance
                    && port.direction == direction
                    && port.transport == item.transport
                    && !used_ports.contains(&(port.instance.as_str(), port.port.as_str()))
                    && !occupied_facilities.contains(&(port.connection.x, port.connection.y))
                    && !occupied_transport.contains(&(
                        port.transport,
                        port.connection.x,
                        port.connection.y,
                    ))
            })
            .cloned()
            .collect::<Vec<_>>();
        if port_options.is_empty() {
            return Err(ConstructiveFrontierDiagnostic::error(
                "process-module-boundary-blocked",
                "/boundary_requirements",
                Some(edge.id.clone()),
                format!(
                    "process module boundary requirement '{}' has no physically exposed compatible port",
                    edge.id
                ),
            ));
        }
        boundary.push(ConstructiveProcessModuleBoundary {
            requirement: edge.id.clone(),
            item: edge.item.clone(),
            transport: item.transport,
            rate: Rate {
                numerator: edge.rate.numerator,
                denominator: edge.rate.denominator,
            },
            direction,
            inside_instance: inside_instance.to_string(),
            port_options,
        });
    }
    boundary.sort_by(|left, right| left.requirement.cmp(&right.requirement));
    Ok(boundary)
}

fn failure(
    root_instance: &str,
    internal_item: &str,
    diagnostic: ConstructiveFrontierDiagnostic,
) -> ConstructiveProcessModuleReport {
    ConstructiveProcessModuleReport {
        schema_version: CONSTRUCTIVE_PROCESS_MODULE_SCHEMA_VERSION,
        success: false,
        root_instance: root_instance.to_string(),
        internal_item: internal_item.to_string(),
        member_instances: Vec::new(),
        internal_requirements: Vec::new(),
        boundary_requirements: Vec::new(),
        growth: ConstructiveFrontierGrowthReport {
            schema_version: CONSTRUCTIVE_FRONTIER_GROWTH_SCHEMA_VERSION,
            requested_belt_frontier_depth: 0,
            success: false,
            status: ConstructiveFrontierGrowthStatus::InvalidInput,
            bounds: None,
            placements: Vec::new(),
            transport_networks: Vec::new(),
            phases: Vec::new(),
            statistics: ConstructiveFrontierGrowthStatistics::default(),
            diagnostics: vec![diagnostic],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{
        FacilityCatalog, FacilityDefinition, FacilityFootprint, FacilityPortDefinition,
        FacilityPortEdge, FacilityPortPosition, SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
    };
    use crate::logistics::{
        ItemCatalog, ItemDefinition, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION, TransportKind,
    };
    use crate::recipes::{
        FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
        FacilityInstanceWiringProjection,
    };

    fn node(id: &str) -> FacilityInstanceWiringNode {
        let one = Rate {
            numerator: 1,
            denominator: 1,
        };
        FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: format!("{id}-recipe"),
            facility: "machine".to_string(),
            index: 0,
            runs_per_second: one,
            work_seconds_per_second: one,
            unused_capacity: Rate::zero(),
        }
    }

    fn edge(id: &str, source: &str, target: &str, item: &str) -> FacilityInstanceWiringEdge {
        FacilityInstanceWiringEdge {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            kind: "recipe-flow".to_string(),
            item: item.to_string(),
            rate: Rate {
                numerator: 1,
                denominator: 1,
            },
            projection: FacilityInstanceWiringProjection::Original,
        }
    }

    #[test]
    fn constructs_internal_routes_and_exposes_unrouted_boundary_port_options() {
        let wiring = FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![
                node("supplier-a"),
                node("supplier-b"),
                node("root"),
                node("outside-input"),
                node("outside-output"),
            ],
            edges: vec![
                edge("a-root", "supplier-a", "root", "internal"),
                edge("b-root", "supplier-b", "root", "internal"),
                edge("outside-a", "outside-input", "supplier-a", "raw"),
                edge("root-outside", "root", "outside-output", "product"),
            ],
            diagnostics: Vec::new(),
        };
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
            facilities: vec![FacilityDefinition {
                id: "machine".to_string(),
                footprint: FacilityFootprint {
                    width: 2,
                    height: 2,
                },
                allowed_rotations: vec![0],
                ports: vec![
                    FacilityPortDefinition {
                        id: "input-west".to_string(),
                        direction: FacilityPortDirection::Input,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 0, y: 0 },
                        edge: FacilityPortEdge::West,
                    },
                    FacilityPortDefinition {
                        id: "input-north".to_string(),
                        direction: FacilityPortDirection::Input,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 1, y: 0 },
                        edge: FacilityPortEdge::North,
                    },
                    FacilityPortDefinition {
                        id: "output-east".to_string(),
                        direction: FacilityPortDirection::Output,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 1, y: 1 },
                        edge: FacilityPortEdge::East,
                    },
                ],
            }],
        })
        .expect("facility catalog validates");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: ["internal", "raw", "product"]
                .into_iter()
                .map(|id| ItemDefinition {
                    id: id.to_string(),
                    transport: TransportKind::Belt,
                })
                .collect(),
        })
        .expect("item catalog validates");

        let report = construct_process_module(&wiring, &facilities, &items, "root", "internal");
        assert!(report.success, "{:?}", report.growth.diagnostics);
        assert_eq!(report.member_instances.len(), 3);
        assert_eq!(report.internal_requirements.len(), 2);
        assert_eq!(report.growth.transport_networks.len(), 2);
        assert_eq!(report.boundary_requirements.len(), 2);
        assert!(
            report
                .boundary_requirements
                .iter()
                .all(|boundary| !boundary.port_options.is_empty())
        );

        let html = super::super::render_constructive_process_module_html(&report, None)
            .expect("process module should render");
        assert!(html.contains("process-module-boundary"));
    }
}
