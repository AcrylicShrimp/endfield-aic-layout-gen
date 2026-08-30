use std::collections::{BTreeMap, BTreeSet};

use crate::layouts::FacilityPlacement;
use crate::logistics::{LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog};

use super::{
    EndpointInput, INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic,
    IntegratedLayoutReport, IntegratedRouteEndpoint, ModelInput, candidate_port_connections,
    grid_index,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SegmentShape {
    Horizontal,
    Vertical,
    Other,
}

pub(super) fn validate(
    input: &ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    report: &IntegratedLayoutReport,
) -> Result<(), IntegratedLayoutDiagnostic> {
    if report.schema_version != INTEGRATED_LAYOUT_SCHEMA_VERSION {
        return Err(invalid(
            "/schema_version",
            format!(
                "witness schema version {} does not match supported version {}",
                report.schema_version, INTEGRATED_LAYOUT_SCHEMA_VERSION
            ),
        ));
    }
    if report.placements.len() != input.instances.len() {
        return Err(invalid(
            "/placements",
            format!(
                "witness has {} placements for {} facility instances",
                report.placements.len(),
                input.instances.len()
            ),
        ));
    }
    let cell_count = input.width as usize * input.height as usize;
    let placements = validate_placements(input, report, cell_count)?;
    let mut layer_cells = [vec![Vec::new(); cell_count], vec![Vec::new(); cell_count]];
    let mut expected_by_id = BTreeMap::new();
    for expected in &input.edges {
        if expected_by_id
            .insert(expected.requirement_id.as_str(), expected)
            .is_some()
        {
            return Err(invalid(
                "/routes",
                format!(
                    "prepared route requirement ID '{}' appears more than once",
                    expected.requirement_id
                ),
            ));
        }
    }
    let mut actual_ids = BTreeSet::new();
    for (route_index, route) in report.routes.iter().enumerate() {
        if !actual_ids.insert(route.requirement_id.as_str()) {
            return Err(invalid(
                format!("/routes/{route_index}/requirement_id"),
                format!(
                    "route requirement ID '{}' appears more than once",
                    route.requirement_id
                ),
            ));
        }
        let Some(expected) = expected_by_id.get(route.requirement_id.as_str()).copied() else {
            return Err(invalid(
                format!("/routes/{route_index}/requirement_id"),
                format!(
                    "route requirement ID '{}' is not expected by the prepared model",
                    route.requirement_id
                ),
            ));
        };
        if route.requirement_fingerprint != expected.requirement_fingerprint {
            return Err(invalid(
                format!("/routes/{route_index}/requirement_fingerprint"),
                "route requirement fingerprint does not match the prepared capacity-split flow",
            ));
        }
        if route.item != expected.edge.item
            || route.rate != expected.edge.rate
            || route.transport != expected.transport
        {
            return Err(invalid(
                format!("/routes/{route_index}"),
                "route material, rate, or transport does not match the prepared capacity-split flow",
            ));
        }
        if route.cells.is_empty() {
            return Err(invalid(
                format!("/routes/{route_index}/cells"),
                "route must contain at least one cell",
            ));
        }
        let cells = route
            .cells
            .iter()
            .enumerate()
            .map(|(cell_index, cell)| {
                if cell.x < 0
                    || cell.y < 0
                    || cell.x >= i64::from(input.width)
                    || cell.y >= i64::from(input.height)
                {
                    return Err(invalid(
                        format!("/routes/{route_index}/cells/{cell_index}"),
                        "route cell is outside the hard layout bounds",
                    ));
                }
                Ok(grid_index(cell.x as i32, cell.y as i32, input.width))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let source_cell = endpoint_cell(
            input,
            &placements,
            &expected.source,
            &route.source,
            cells[0],
            route_index,
            "source",
        )?;
        let target_terminal = *cells.last().expect("non-empty route");
        let target_cell = endpoint_cell(
            input,
            &placements,
            &expected.target,
            &route.target,
            target_terminal,
            route_index,
            "target",
        )?;
        validate_external_side(
            input,
            &placements,
            &route.source,
            &route.target,
            route_index,
        )?;
        if cells[0] != source_cell || *cells.last().expect("non-empty route") != target_cell {
            return Err(invalid(
                format!("/routes/{route_index}/cells"),
                "route endpoints do not match the selected facility ports or external connections",
            ));
        }
        if cells.iter().copied().collect::<BTreeSet<_>>().len() != cells.len() {
            return Err(invalid(
                format!("/routes/{route_index}/cells"),
                "route contains a repeated cell",
            ));
        }
        for pair in cells.windows(2) {
            let left_x = pair[0] % input.width as usize;
            let left_y = pair[0] / input.width as usize;
            let right_x = pair[1] % input.width as usize;
            let right_y = pair[1] / input.width as usize;
            if left_x.abs_diff(right_x) + left_y.abs_diff(right_y) != 1 {
                return Err(invalid(
                    format!("/routes/{route_index}/cells"),
                    "consecutive route cells must be orthogonally adjacent",
                ));
            }
        }
        let layer = layer_index(route.transport);
        for (path_index, cell) in cells.iter().enumerate() {
            if placements.occupied[*cell] {
                return Err(invalid(
                    format!("/routes/{route_index}/cells/{path_index}"),
                    "route occupies a production-facility footprint cell",
                ));
            }
            layer_cells[layer][*cell].push(segment_shape(&cells, path_index, input.width));
        }
    }
    if let Some(missing) = expected_by_id
        .keys()
        .find(|requirement_id| !actual_ids.contains(**requirement_id))
    {
        return Err(invalid(
            "/routes",
            format!("required route '{missing}' is missing from the witness"),
        ));
    }

    validate_crossings(input, components, report, &placements, &layer_cells)?;
    let (minimum_x, minimum_y, used_width, used_height) = used_geometry_bounds(report);
    if minimum_x != 0 || minimum_y != 0 {
        return Err(invalid(
            "/bounds",
            format!(
                "used geometry must be canonicalized to origin (0, 0), found ({minimum_x}, {minimum_y})"
            ),
        ));
    }
    let bounds = report.bounds.as_ref().ok_or_else(|| {
        invalid(
            "/bounds",
            "successful witness must report its exact used bounds",
        )
    })?;
    if bounds.width != used_width || bounds.height != used_height {
        return Err(invalid(
            "/bounds",
            format!(
                "reported bounds {}x{} do not equal used bounds {used_width}x{used_height}",
                bounds.width, bounds.height
            ),
        ));
    }
    Ok(())
}

struct ValidatedPlacements<'a> {
    by_instance: BTreeMap<&'a str, &'a FacilityPlacement>,
    occupied: Vec<bool>,
}

fn validate_placements<'a>(
    input: &ModelInput,
    report: &'a IntegratedLayoutReport,
    cell_count: usize,
) -> Result<ValidatedPlacements<'a>, IntegratedLayoutDiagnostic> {
    let mut by_instance = BTreeMap::new();
    let mut occupied = vec![false; cell_count];
    for (index, placement) in report.placements.iter().enumerate() {
        let instance = input
            .instances
            .iter()
            .find(|instance| instance.id == placement.instance)
            .ok_or_else(|| {
                invalid(
                    format!("/placements/{index}/instance"),
                    format!("unknown facility instance '{}'", placement.instance),
                )
            })?;
        if by_instance
            .insert(placement.instance.as_str(), placement)
            .is_some()
        {
            return Err(invalid(
                format!("/placements/{index}/instance"),
                format!("duplicate facility instance '{}'", placement.instance),
            ));
        }
        if placement.recipe != instance.recipe
            || placement.facility != instance.facility
            || !instance
                .definition
                .allowed_rotations
                .contains(&placement.rotation)
        {
            return Err(invalid(
                format!("/placements/{index}"),
                "placement identity or rotation does not match its prepared facility instance",
            ));
        }
        let source_width = instance.definition.footprint.width;
        let source_height = instance.definition.footprint.height;
        let (width, height) = if matches!(placement.rotation, 90 | 270) {
            (source_height, source_width)
        } else {
            (source_width, source_height)
        };
        if placement.width != width
            || placement.height != height
            || placement.x < 0
            || placement.y < 0
            || placement.x + placement.width > i64::from(input.width)
            || placement.y + placement.height > i64::from(input.height)
        {
            return Err(invalid(
                format!("/placements/{index}"),
                "placement dimensions or origin violate its facility definition or hard bounds",
            ));
        }
        for y in placement.y..(placement.y + placement.height) {
            for x in placement.x..(placement.x + placement.width) {
                let cell = grid_index(x as i32, y as i32, input.width);
                if occupied[cell] {
                    return Err(invalid(
                        format!("/placements/{index}"),
                        "production-facility footprints overlap",
                    ));
                }
                occupied[cell] = true;
            }
        }
    }
    Ok(ValidatedPlacements {
        by_instance,
        occupied,
    })
}

#[allow(clippy::too_many_arguments)]
fn endpoint_cell(
    input: &ModelInput,
    placements: &ValidatedPlacements<'_>,
    expected: &EndpointInput,
    actual: &IntegratedRouteEndpoint,
    terminal_cell: usize,
    route_index: usize,
    endpoint_kind: &str,
) -> Result<usize, IntegratedLayoutDiagnostic> {
    match (expected, actual) {
        (
            EndpointInput::Facility { instance, ports },
            IntegratedRouteEndpoint::Facility {
                instance: actual_instance,
                port,
            },
        ) if instance == actual_instance && ports.iter().any(|candidate| candidate.id == *port) => {
            let placement = placements.by_instance[instance.as_str()];
            let definition = &input
                .instances
                .iter()
                .find(|candidate| candidate.id == *instance)
                .expect("prepared endpoint instance exists")
                .definition;
            let connections = candidate_port_connections(
                definition,
                placement.rotation,
                placement.x as i32,
                placement.y as i32,
                input.width,
                input.height,
            );
            connections.get(port).copied().ok_or_else(|| {
                invalid(
                    format!("/routes/{route_index}/{endpoint_kind}"),
                    "selected facility port connection is outside the hard search domain",
                )
            })
        }
        (
            EndpointInput::External { node },
            IntegratedRouteEndpoint::External {
                node: actual_node, ..
            },
        ) if node == actual_node => Ok(terminal_cell),
        _ => Err(invalid(
            format!("/routes/{route_index}/{endpoint_kind}"),
            "route endpoint does not match the prepared endpoint and compatible port set",
        )),
    }
}

fn validate_external_side(
    input: &ModelInput,
    placements: &ValidatedPlacements<'_>,
    source: &IntegratedRouteEndpoint,
    target: &IntegratedRouteEndpoint,
    route_index: usize,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let pair = match (source, target) {
        (
            IntegratedRouteEndpoint::External { side, .. },
            facility @ IntegratedRouteEndpoint::Facility { .. },
        )
        | (
            facility @ IntegratedRouteEndpoint::Facility { .. },
            IntegratedRouteEndpoint::External { side, .. },
        ) => Some((*side, facility)),
        _ => None,
    };
    let Some((external_side, facility)) = pair else {
        return Ok(());
    };
    let IntegratedRouteEndpoint::Facility { instance, port } = facility else {
        unreachable!()
    };
    let placement = placements.by_instance[instance.as_str()];
    let definition = &input
        .instances
        .iter()
        .find(|candidate| candidate.id == *instance)
        .expect("prepared endpoint instance exists")
        .definition;
    let facility_side = definition
        .ports
        .iter()
        .find(|candidate| candidate.id == *port)
        .map(|candidate| candidate.edge.rotated_clockwise(placement.rotation))
        .ok_or_else(|| {
            invalid(
                format!("/routes/{route_index}"),
                "selected facility port is missing",
            )
        })?;
    if external_side != facility_side {
        return Err(invalid(
            format!("/routes/{route_index}"),
            "external connection side does not match the selected facility port side",
        ));
    }
    Ok(())
}

fn used_geometry_bounds(report: &IntegratedLayoutReport) -> (i64, i64, i64, i64) {
    let mut minimum_x = i64::MAX;
    let mut minimum_y = i64::MAX;
    let mut maximum_x = i64::MIN;
    let mut maximum_y = i64::MIN;
    for placement in &report.placements {
        minimum_x = minimum_x.min(placement.x);
        minimum_y = minimum_y.min(placement.y);
        maximum_x = maximum_x.max(placement.x + placement.width - 1);
        maximum_y = maximum_y.max(placement.y + placement.height - 1);
    }
    for position in report
        .routes
        .iter()
        .flat_map(|route| route.cells.iter())
        .chain(
            report
                .logistics_components
                .iter()
                .map(|component| &component.position),
        )
    {
        minimum_x = minimum_x.min(position.x);
        minimum_y = minimum_y.min(position.y);
        maximum_x = maximum_x.max(position.x);
        maximum_y = maximum_y.max(position.y);
    }
    if minimum_x == i64::MAX {
        (0, 0, 0, 0)
    } else {
        (
            minimum_x,
            minimum_y,
            maximum_x - minimum_x + 1,
            maximum_y - minimum_y + 1,
        )
    }
}

fn validate_crossings(
    input: &ModelInput,
    components: &ValidatedLogisticsComponentCatalog,
    report: &IntegratedLayoutReport,
    placements: &ValidatedPlacements<'_>,
    layer_cells: &[Vec<Vec<SegmentShape>>; 2],
) -> Result<(), IntegratedLayoutDiagnostic> {
    let mut expected_bridges = BTreeSet::new();
    for transport in [TransportKind::Belt, TransportKind::Pipe] {
        for (cell, shapes) in layer_cells[layer_index(transport)].iter().enumerate() {
            match shapes.as_slice() {
                [] | [_] => {}
                [first, second]
                    if matches!(
                        (*first, *second),
                        (SegmentShape::Horizontal, SegmentShape::Vertical)
                            | (SegmentShape::Vertical, SegmentShape::Horizontal)
                    ) =>
                {
                    expected_bridges.insert((transport, cell));
                }
                _ => {
                    return Err(invalid(
                        "/routes",
                        format!(
                            "same-layer cell {cell} has {} channels without a valid perpendicular bridge crossing",
                            shapes.len()
                        ),
                    ));
                }
            }
        }
    }

    let mut actual_bridges = BTreeSet::new();
    for (index, component) in report.logistics_components.iter().enumerate() {
        if component.kind != LogisticsComponentKind::Bridge {
            return Err(invalid(
                format!("/logistics_components/{index}/kind"),
                "sparse witness currently emits only bridge logistics components",
            ));
        }
        let definition = components
            .component_by_kind(component.transport, component.kind)
            .expect("validated catalog has every bridge capability");
        if component.component != definition.id
            || !definition.allowed_rotations.contains(&component.rotation)
            || component.position.x < 0
            || component.position.y < 0
            || component.position.x >= i64::from(input.width)
            || component.position.y >= i64::from(input.height)
        {
            return Err(invalid(
                format!("/logistics_components/{index}"),
                "placed logistics bridge does not match its catalog definition or hard bounds",
            ));
        }
        let cell = grid_index(
            component.position.x as i32,
            component.position.y as i32,
            input.width,
        );
        if placements.occupied[cell] || !actual_bridges.insert((component.transport, cell)) {
            return Err(invalid(
                format!("/logistics_components/{index}"),
                "placed logistics bridge overlaps a facility or duplicates another bridge",
            ));
        }
    }
    if actual_bridges != expected_bridges {
        return Err(invalid(
            "/logistics_components",
            "placed logistics bridges do not exactly cover same-layer perpendicular crossings",
        ));
    }
    Ok(())
}

fn segment_shape(path: &[usize], index: usize, width: i32) -> SegmentShape {
    if index == 0 || index + 1 == path.len() {
        return SegmentShape::Other;
    }
    let previous = path[index - 1];
    let current = path[index];
    let next = path[index + 1];
    if previous % width as usize == current % width as usize
        && current % width as usize == next % width as usize
    {
        SegmentShape::Vertical
    } else if previous / width as usize == current / width as usize
        && current / width as usize == next / width as usize
    {
        SegmentShape::Horizontal
    } else {
        SegmentShape::Other
    }
}

fn layer_index(transport: TransportKind) -> usize {
    match transport {
        TransportKind::Belt => 0,
        TransportKind::Pipe => 1,
    }
}

fn invalid(path: impl Into<String>, message: impl Into<String>) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error("constructed-layout-witness-invalid", path, None, message)
}
