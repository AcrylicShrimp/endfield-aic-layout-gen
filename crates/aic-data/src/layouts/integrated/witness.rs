use std::collections::{BTreeMap, BTreeSet};

use crate::layouts::FacilityPlacement;
use crate::logistics::{LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog};

use super::{
    BoundarySide, EndpointInput, IntegratedLayoutDiagnostic, IntegratedLayoutReport,
    IntegratedRouteEndpoint, ModelInput, candidate_port_connections, grid_index,
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
    if report.routes.len() != input.edges.len() {
        return Err(invalid(
            "/routes",
            format!(
                "witness has {} routes for {} capacity-split flows",
                report.routes.len(),
                input.edges.len()
            ),
        ));
    }

    let cell_count = input.width as usize * input.height as usize;
    let placements = validate_placements(input, report, cell_count)?;
    let mut layer_cells = [vec![Vec::new(); cell_count], vec![Vec::new(); cell_count]];
    let mut used_width = report
        .placements
        .iter()
        .map(|placement| placement.x + placement.width)
        .max()
        .unwrap_or(0);
    let mut used_height = report
        .placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .max()
        .unwrap_or(0);

    for (route_index, (expected, route)) in input.edges.iter().zip(&report.routes).enumerate() {
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
                used_width = used_width.max(cell.x + 1);
                used_height = used_height.max(cell.y + 1);
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
        if cells[0] != source_cell || *cells.last().expect("non-empty route") != target_cell {
            return Err(invalid(
                format!("/routes/{route_index}/cells"),
                "route endpoints do not match the selected facility ports or boundary terminals",
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

    validate_crossings(input, components, report, &placements, &layer_cells)?;
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
            )
            .expect("validated placement keeps port connections in bounds");
            Ok(connections[port])
        }
        (
            EndpointInput::Boundary { node },
            IntegratedRouteEndpoint::Boundary {
                node: actual_node,
                side,
            },
        ) if node == actual_node => {
            boundary_cell(input, *side, terminal_cell, route_index, endpoint_kind)
        }
        _ => Err(invalid(
            format!("/routes/{route_index}/{endpoint_kind}"),
            "route endpoint does not match the prepared endpoint and compatible port set",
        )),
    }
}

fn boundary_cell(
    input: &ModelInput,
    side: BoundarySide,
    terminal_cell: usize,
    route_index: usize,
    endpoint_kind: &str,
) -> Result<usize, IntegratedLayoutDiagnostic> {
    let x = terminal_cell % input.width as usize;
    let y = terminal_cell / input.width as usize;
    let on_side = match side {
        BoundarySide::North => y == 0,
        BoundarySide::East => x == input.width as usize - 1,
        BoundarySide::South => y == input.height as usize - 1,
        BoundarySide::West => x == 0,
    };
    if on_side {
        Ok(terminal_cell)
    } else {
        Err(invalid(
            format!("/routes/{route_index}/{endpoint_kind}"),
            "boundary endpoint side does not match its terminal route cell",
        ))
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
