use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::facilities::{FacilityDefinition, FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementReport, FacilityPlacementRequest,
    FacilityPlacementStatus, PlacedFacilityPort, SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
    WorldGridPosition, project_facility_ports,
};
use crate::logistics::{TransportKind, ValidatedItemCatalog};
use crate::recipes::{
    FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
    FacilityInstanceWiringNode, FacilityInstanceWiringReport,
};

use super::routing::{count_turns, route_shortest_path};
use super::{
    CONSTRUCTIVE_FRONTIER_SCHEMA_VERSION, ConstructiveFrontierDiagnostic,
    ConstructiveFrontierReport, ConstructiveFrontierStatistics, ConstructiveFrontierStatus,
};

#[derive(Clone)]
struct FacilityInstance {
    id: String,
    recipe: String,
    facility: String,
}

struct Frontier<'a> {
    edge: &'a FacilityInstanceWiringEdge,
    source: FacilityInstance,
    target: FacilityInstance,
    source_definition: &'a FacilityDefinition,
    target_definition: &'a FacilityDefinition,
}

pub fn construct_first_pipe_frontier(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
) -> ConstructiveFrontierReport {
    if let Some(diagnostic) = validate_inputs(wiring) {
        return ConstructiveFrontierReport::failure(
            ConstructiveFrontierStatus::InvalidInput,
            diagnostic,
        );
    }
    let frontier = match select_frontier(wiring, facilities, items) {
        Ok(frontier) => frontier,
        Err((status, diagnostic)) => {
            return ConstructiveFrontierReport::failure(status, diagnostic);
        }
    };
    let request = diagnostic_canvas(&frontier);

    let mut statistics = ConstructiveFrontierStatistics::default();
    let seed_candidates =
        placement_candidates(&frontier.target, frontier.target_definition, &request, None);
    for target in seed_candidates {
        statistics.seed_placements_considered += 1;
        let source_candidates = placement_candidates(
            &frontier.source,
            frontier.source_definition,
            &request,
            Some(&target),
        );
        for source in source_candidates {
            statistics.supplier_placements_considered += 1;
            if rectangles_overlap(&source, &target) {
                statistics.overlapping_placements_rejected += 1;
                continue;
            }
            let placements = vec![source.clone(), target.clone()];
            let Some((source_ports, target_ports)) = candidate_ports(
                &placements,
                facilities,
                &request,
                &frontier.source.id,
                &frontier.target.id,
            ) else {
                continue;
            };
            let blocked = occupied_cells(&placements);
            for source_port in &source_ports {
                for target_port in &target_ports {
                    statistics.port_pairs_considered += 1;
                    if blocked.contains(&(source_port.connection.x, source_port.connection.y))
                        || blocked.contains(&(target_port.connection.x, target_port.connection.y))
                    {
                        statistics.blocked_port_pairs_rejected += 1;
                        continue;
                    }
                    statistics.astar_searches += 1;
                    let Some(path) = route_shortest_path(
                        request.max_width,
                        request.max_height,
                        &blocked,
                        &source_port.connection,
                        &target_port.connection,
                    ) else {
                        statistics.astar_failures += 1;
                        continue;
                    };
                    statistics.accepted_path_tiles = path.len();
                    statistics.accepted_path_turns = count_turns(&path);
                    return constructed_report(
                        &frontier,
                        placements,
                        source_port.clone(),
                        target_port.clone(),
                        path,
                        statistics,
                    );
                }
            }
        }
    }

    ConstructiveFrontierReport {
        schema_version: CONSTRUCTIVE_FRONTIER_SCHEMA_VERSION,
        success: false,
        status: ConstructiveFrontierStatus::Exhausted,
        requirement: Some(frontier.edge.id.clone()),
        item: Some(frontier.edge.item.clone()),
        rate: Some(frontier.edge.rate),
        bounds: None,
        placements: Vec::new(),
        source_port: None,
        target_port: None,
        pipe_cells: Vec::new(),
        statistics,
        diagnostics: vec![ConstructiveFrontierDiagnostic::error(
            "pipe-frontier-candidates-exhausted",
            "/edges",
            Some(frontier.edge.id.clone()),
            format!(
                "no placement, port, and A* route candidate constructed pipe frontier '{}' within the derived {}x{} local diagnostic canvas",
                frontier.edge.id, request.max_width, request.max_height
            ),
        )],
    }
}

fn validate_inputs(
    wiring: &FacilityInstanceWiringReport,
) -> Option<ConstructiveFrontierDiagnostic> {
    if !wiring.success {
        return Some(ConstructiveFrontierDiagnostic::error(
            "upstream-instance-wiring-failed",
            "/",
            None,
            "constructive frontier requires successful facility instance wiring",
        ));
    }
    if wiring.schema_version != FACILITY_INSTANCE_WIRING_SCHEMA_VERSION {
        return Some(ConstructiveFrontierDiagnostic::error(
            "unsupported-instance-wiring-schema-version",
            "/schema_version",
            None,
            format!(
                "instance wiring schema version {} is unsupported; expected {}",
                wiring.schema_version, FACILITY_INSTANCE_WIRING_SCHEMA_VERSION
            ),
        ));
    }
    None
}

fn diagnostic_canvas(frontier: &Frontier<'_>) -> FacilityPlacementRequest {
    let source_extent = frontier
        .source_definition
        .footprint
        .width
        .max(frontier.source_definition.footprint.height);
    let target_extent = frontier
        .target_definition
        .footprint
        .width
        .max(frontier.target_definition.footprint.height);
    let extent = source_extent + target_extent + 6;
    FacilityPlacementRequest {
        schema_version: SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
        max_width: extent,
        max_height: extent,
    }
}

fn select_frontier<'a>(
    wiring: &'a FacilityInstanceWiringReport,
    facilities: &'a ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
) -> Result<Frontier<'a>, (ConstructiveFrontierStatus, ConstructiveFrontierDiagnostic)> {
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

    for edge in &wiring.edges {
        let Some(item) = items.item(&edge.item) else {
            return Err((
                ConstructiveFrontierStatus::InvalidInput,
                ConstructiveFrontierDiagnostic::error(
                    "missing-item-definition",
                    "/edges",
                    Some(edge.item.clone()),
                    format!("wiring edge references missing item '{}'", edge.item),
                ),
            ));
        };
        if item.transport != TransportKind::Pipe {
            continue;
        }
        let (Some(source), Some(target)) = (
            instances.get(edge.source.as_str()),
            instances.get(edge.target.as_str()),
        ) else {
            continue;
        };
        let Some(source_definition) = facilities.facility(&source.facility) else {
            return Err(missing_facility(&source.id, &source.facility));
        };
        let Some(target_definition) = facilities.facility(&target.facility) else {
            return Err(missing_facility(&target.id, &target.facility));
        };
        return Ok(Frontier {
            edge,
            source: source.clone(),
            target: target.clone(),
            source_definition,
            target_definition,
        });
    }
    Err((
        ConstructiveFrontierStatus::NoEligibleFrontier,
        ConstructiveFrontierDiagnostic::error(
            "no-internal-pipe-frontier",
            "/edges",
            None,
            "instance wiring contains no facility-to-facility pipe edge for the first constructive slice",
        ),
    ))
}

fn missing_facility(
    instance: &str,
    facility: &str,
) -> (ConstructiveFrontierStatus, ConstructiveFrontierDiagnostic) {
    (
        ConstructiveFrontierStatus::InvalidInput,
        ConstructiveFrontierDiagnostic::error(
            "missing-facility-definition",
            "/nodes",
            Some(instance.to_string()),
            format!("facility instance '{instance}' references missing facility '{facility}'"),
        ),
    )
}

fn placement_candidates(
    instance: &FacilityInstance,
    definition: &FacilityDefinition,
    request: &FacilityPlacementRequest,
    near: Option<&FacilityPlacement>,
) -> Vec<FacilityPlacement> {
    let mut unique_geometry = BTreeSet::new();
    let mut candidates = Vec::new();
    for &rotation in &definition.allowed_rotations {
        let (width, height) = match rotation {
            0 | 180 => (definition.footprint.width, definition.footprint.height),
            90 | 270 => (definition.footprint.height, definition.footprint.width),
            _ => continue,
        };
        if !unique_geometry.insert((width, height, rotation))
            || width + 2 > request.max_width
            || height + 2 > request.max_height
        {
            continue;
        }
        for y in 1..(request.max_height - height) {
            for x in 1..(request.max_width - width) {
                candidates.push(FacilityPlacement {
                    instance: instance.id.clone(),
                    recipe: instance.recipe.clone(),
                    facility: instance.facility.clone(),
                    x,
                    y,
                    width,
                    height,
                    rotation,
                });
            }
        }
    }
    candidates.sort_by(|left, right| {
        placement_distance(left, near)
            .cmp(&placement_distance(right, near))
            .then_with(|| left.rotation.cmp(&right.rotation))
            .then_with(|| left.y.cmp(&right.y))
            .then_with(|| left.x.cmp(&right.x))
    });
    candidates
}

fn placement_distance(placement: &FacilityPlacement, near: Option<&FacilityPlacement>) -> i64 {
    let Some(near) = near else {
        return placement.x + placement.y;
    };
    let placement_center_x = placement.x * 2 + placement.width;
    let placement_center_y = placement.y * 2 + placement.height;
    let near_center_x = near.x * 2 + near.width;
    let near_center_y = near.y * 2 + near.height;
    placement_center_x.abs_diff(near_center_x) as i64
        + placement_center_y.abs_diff(near_center_y) as i64
}

fn rectangles_overlap(left: &FacilityPlacement, right: &FacilityPlacement) -> bool {
    left.x < right.x + right.width
        && right.x < left.x + left.width
        && left.y < right.y + right.height
        && right.y < left.y + left.height
}

fn candidate_ports(
    placements: &[FacilityPlacement],
    facilities: &ValidatedFacilityCatalog,
    request: &FacilityPlacementRequest,
    source_instance: &str,
    target_instance: &str,
) -> Option<(Vec<PlacedFacilityPort>, Vec<PlacedFacilityPort>)> {
    let report = FacilityPlacementReport {
        success: true,
        status: FacilityPlacementStatus::Feasible,
        bounds: Some(bounds_for(placements, &[])),
        placements: placements.to_vec(),
        diagnostics: Vec::new(),
    };
    let projection = project_facility_ports(&report, facilities, request);
    if !projection.success {
        return None;
    }
    let mut source_ports = projection
        .ports
        .iter()
        .filter(|port| {
            port.instance == source_instance
                && port.direction == FacilityPortDirection::Output
                && port.transport == TransportKind::Pipe
        })
        .cloned()
        .collect::<Vec<_>>();
    let mut target_ports = projection
        .ports
        .iter()
        .filter(|port| {
            port.instance == target_instance
                && port.direction == FacilityPortDirection::Input
                && port.transport == TransportKind::Pipe
        })
        .cloned()
        .collect::<Vec<_>>();
    source_ports.sort_by(|left, right| left.port.cmp(&right.port));
    target_ports.sort_by(|left, right| left.port.cmp(&right.port));
    (!source_ports.is_empty() && !target_ports.is_empty()).then_some((source_ports, target_ports))
}

fn occupied_cells(placements: &[FacilityPlacement]) -> HashSet<(i64, i64)> {
    placements
        .iter()
        .flat_map(|placement| {
            (placement.y..placement.y + placement.height).flat_map(move |y| {
                (placement.x..placement.x + placement.width).map(move |x| (x, y))
            })
        })
        .collect()
}

fn constructed_report(
    frontier: &Frontier<'_>,
    mut placements: Vec<FacilityPlacement>,
    mut source_port: PlacedFacilityPort,
    mut target_port: PlacedFacilityPort,
    mut path: Vec<WorldGridPosition>,
    statistics: ConstructiveFrontierStatistics,
) -> ConstructiveFrontierReport {
    canonicalize(
        &mut placements,
        [&mut source_port, &mut target_port],
        &mut path,
    );
    let bounds = bounds_for(&placements, &path);
    ConstructiveFrontierReport {
        schema_version: CONSTRUCTIVE_FRONTIER_SCHEMA_VERSION,
        success: true,
        status: ConstructiveFrontierStatus::Constructed,
        requirement: Some(frontier.edge.id.clone()),
        item: Some(frontier.edge.item.clone()),
        rate: Some(frontier.edge.rate),
        bounds: Some(bounds),
        placements,
        source_port: Some(source_port),
        target_port: Some(target_port),
        pipe_cells: path,
        statistics,
        diagnostics: vec![ConstructiveFrontierDiagnostic::info(
            "pipe-frontier-constructed",
            "constructed and validated one facility-to-facility pipe frontier with local placement, port, and A* route search",
        )],
    }
}

fn canonicalize(
    placements: &mut [FacilityPlacement],
    ports: [&mut PlacedFacilityPort; 2],
    path: &mut [WorldGridPosition],
) {
    let minimum_x = placements
        .iter()
        .map(|placement| placement.x)
        .chain(path.iter().map(|cell| cell.x))
        .min()
        .unwrap_or(0);
    let minimum_y = placements
        .iter()
        .map(|placement| placement.y)
        .chain(path.iter().map(|cell| cell.y))
        .min()
        .unwrap_or(0);
    for placement in placements {
        placement.x -= minimum_x;
        placement.y -= minimum_y;
    }
    for port in ports {
        port.position.x -= minimum_x;
        port.position.y -= minimum_y;
        port.connection.x -= minimum_x;
        port.connection.y -= minimum_y;
    }
    for cell in path {
        cell.x -= minimum_x;
        cell.y -= minimum_y;
    }
}

fn bounds_for(
    placements: &[FacilityPlacement],
    path: &[WorldGridPosition],
) -> FacilityPlacementBounds {
    FacilityPlacementBounds {
        width: placements
            .iter()
            .map(|placement| placement.x + placement.width)
            .chain(path.iter().map(|cell| cell.x + 1))
            .max()
            .unwrap_or(0),
        height: placements
            .iter()
            .map(|placement| placement.y + placement.height)
            .chain(path.iter().map(|cell| cell.y + 1))
            .max()
            .unwrap_or(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{
        FacilityCatalog, FacilityFootprint, FacilityPortDefinition, FacilityPortEdge,
        FacilityPortPosition, SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
    };
    use crate::logistics::{ItemCatalog, ItemDefinition, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION};
    use crate::recipes::{FacilityInstanceWiringProjection, Rate};

    fn facility(
        id: &str,
        direction: FacilityPortDirection,
        edge: FacilityPortEdge,
    ) -> FacilityDefinition {
        let position = match edge {
            FacilityPortEdge::North => FacilityPortPosition { x: 0, y: 0 },
            FacilityPortEdge::East => FacilityPortPosition { x: 1, y: 0 },
            FacilityPortEdge::South => FacilityPortPosition { x: 0, y: 1 },
            FacilityPortEdge::West => FacilityPortPosition { x: 0, y: 0 },
        };
        FacilityDefinition {
            id: id.to_string(),
            footprint: FacilityFootprint {
                width: 2,
                height: 2,
            },
            allowed_rotations: vec![0],
            ports: vec![FacilityPortDefinition {
                id: format!("{id}-port"),
                direction,
                transport: TransportKind::Pipe,
                position,
                edge,
            }],
        }
    }

    fn fixtures() -> (
        FacilityInstanceWiringReport,
        ValidatedFacilityCatalog,
        ValidatedItemCatalog,
    ) {
        let node = |id: &str, facility: &str| FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: format!("{id}-recipe"),
            facility: facility.to_string(),
            index: 0,
            runs_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            work_seconds_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            unused_capacity: Rate::zero(),
        };
        let wiring = FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![
                node("source", "source-machine"),
                node("target", "target-machine"),
            ],
            edges: vec![FacilityInstanceWiringEdge {
                id: "pipe-edge".to_string(),
                source: "source".to_string(),
                target: "target".to_string(),
                kind: "intermediate".to_string(),
                item: "fluid".to_string(),
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
                projection: FacilityInstanceWiringProjection::Original,
            }],
            diagnostics: Vec::new(),
        };
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
            facilities: vec![
                facility(
                    "source-machine",
                    FacilityPortDirection::Output,
                    FacilityPortEdge::East,
                ),
                facility(
                    "target-machine",
                    FacilityPortDirection::Input,
                    FacilityPortEdge::West,
                ),
            ],
        })
        .expect("facility fixture validates");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![ItemDefinition {
                id: "fluid".to_string(),
                transport: TransportKind::Pipe,
            }],
        })
        .expect("item fixture validates");
        (wiring, facilities, items)
    }

    #[test]
    fn constructs_two_facilities_and_one_shortest_pipe() {
        let (wiring, facilities, items) = fixtures();
        let report = construct_first_pipe_frontier(&wiring, &facilities, &items);
        assert!(report.success, "{:?}", report.diagnostics);
        assert_eq!(report.placements.len(), 2);
        assert!(!report.pipe_cells.is_empty());
        assert_eq!(
            report.pipe_cells.first(),
            report.source_port.as_ref().map(|port| &port.connection)
        );
        assert_eq!(
            report.pipe_cells.last(),
            report.target_port.as_ref().map(|port| &port.connection)
        );
        let occupied = occupied_cells(&report.placements);
        assert!(
            report
                .pipe_cells
                .iter()
                .all(|cell| !occupied.contains(&(cell.x, cell.y)))
        );
        assert_eq!(report.statistics.astar_failures, 0);
    }

    #[test]
    fn reports_exhaustion_without_claiming_infeasibility() {
        let (wiring, facilities, items) = fixtures();
        let mut catalog = facilities.catalog().clone();
        catalog.facilities[0].ports[0].direction = FacilityPortDirection::Input;
        let facilities = ValidatedFacilityCatalog::try_from_catalog(catalog)
            .expect("modified facility fixture validates");
        let report = construct_first_pipe_frontier(&wiring, &facilities, &items);
        assert!(!report.success);
        assert_eq!(report.status, ConstructiveFrontierStatus::Exhausted);
    }
}
