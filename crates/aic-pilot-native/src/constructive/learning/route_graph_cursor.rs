//! Retained graph-input construction for a stable route-guidance request.

use std::{collections::HashSet, sync::Arc};

use aic_layout::{
    geometry::{Point, layer},
    physical::{Bridge, BridgeAxis},
};

use super::{
    Config, ENTITY_FEATURES, FOCUS_FEATURES, GLOBAL_FEATURES, Input, RELATION_FEATURES, SCHEMA,
    endpoint_entity, host_allocation_stop, push_spatial, reserve_graph, try_clone_bridge,
};
use crate::constructive::{
    Focus, Session, error,
    routing_candidates::{AllowanceStop, RequestAllowance, RequestFailure, WorkKind},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Blocks,
    Terminals,
    Connections,
    BlockSpatial,
    RouteSpatial,
    Bridges,
    TerminalSpatial,
    Focus,
    OccupiedBodies,
    OccupiedLayer0,
    OccupiedLayer1,
    OccupiedTerminals,
    RouteFocus,
    Done,
}

pub(crate) struct RouteGraphCursor {
    input: Input,
    phase: Phase,
    index: usize,
    inner: usize,
    route_cell_graph_charged: bool,
    bridge_catalog_charged: bool,
    bridge_validation_charged: bool,
    bridge_features: Option<(f32, [f32; 2])>,
    bridge_axis: usize,
    bridge_owner: usize,
    occupied: HashSet<Point>,
    scale: f32,
    output_bytes: usize,
    scratch_bytes: usize,
}

impl RouteGraphCursor {
    pub(crate) fn new(
        source: &Session,
        config: &Config,
        connection: usize,
        occupied_sites: usize,
        allowance: &mut RequestAllowance,
    ) -> Result<Self, RequestFailure> {
        config.validate(source)?;
        if source.active.is_some()
            || connection >= source.problem.connections.len()
            || source.routes[connection].is_some()
        {
            return Err(error(
                "route-guidance-target",
                "learning-domain",
                "route guidance requires an unresolved connection in a stable state",
            )
            .into());
        }
        let blocks = source.problem.blocks.len();
        let terminals = source.problem.terminals.len();
        let connections = source.problem.connections.len();
        let entities = blocks.saturating_add(terminals).saturating_add(connections);
        let relations = connections.saturating_mul(2);
        let (rows, _, graph_bytes) = source.guidance_graph_bounds();
        let spatial = rows.saturating_sub(entities + relations);
        let output_bytes = graph_bytes
            .saturating_add(source.bridge_copy_bytes())
            .saturating_add(std::mem::size_of::<Input>())
            .saturating_add(2 * std::mem::size_of::<usize>());
        // HashSet's table can use roughly twice the entry count. Four Point
        // widths per possible entry also cover control bytes and spare slots.
        let scratch_bytes = occupied_sites.saturating_mul(4 * std::mem::size_of::<Point>());
        let held_bytes = output_bytes.saturating_add(scratch_bytes);
        allowance.reserve_peak(held_bytes)?;
        let allocations = (|| -> Result<_, RequestFailure> {
            let mut entity_features = Vec::new();
            reserve_graph(&mut entity_features, entities, true)?;
            let mut relation_index = Vec::new();
            reserve_graph(&mut relation_index, relations, true)?;
            let mut relation_features = Vec::new();
            reserve_graph(&mut relation_features, relations, true)?;
            let mut spatial_features = Vec::new();
            reserve_graph(&mut spatial_features, spatial, true)?;
            let mut spatial_owners = Vec::new();
            reserve_graph(&mut spatial_owners, spatial, true)?;
            let mut bridge_resources = Vec::new();
            reserve_graph(&mut bridge_resources, source.bridges.len(), true)?;
            let mut occupied = HashSet::new();
            occupied.try_reserve(occupied_sites).map_err(|_| {
                RequestFailure::Stopped(AllowanceStop {
                    reason: "host-allocation",
                    category: None,
                    limit: 0,
                    used: 0,
                    deadline_overshoot_ms: 0,
                    attempted_bytes: Some(scratch_bytes),
                    aggregate_peak_bytes: None,
                })
            })?;
            Ok((
                entity_features,
                relation_index,
                relation_features,
                spatial_features,
                spatial_owners,
                bridge_resources,
                occupied,
            ))
        })();
        let (
            entity_features,
            relation_index,
            relation_features,
            spatial_features,
            spatial_owners,
            bridge_resources,
            occupied,
        ) = match allocations {
            Ok(allocations) => allocations,
            Err(failure) => {
                allowance.release_peak(held_bytes);
                return Err(failure);
            }
        };
        Ok(Self {
            input: Input {
                schema: SCHEMA,
                entity_features,
                relation_index,
                relation_features,
                spatial_features,
                spatial_owners,
                global_features: [0.; GLOBAL_FEATURES],
                focus_features: [0.; FOCUS_FEATURES],
                focus_entity: -1,
                bridge_resources,
            },
            phase: Phase::Blocks,
            index: 0,
            inner: 0,
            route_cell_graph_charged: false,
            bridge_catalog_charged: false,
            bridge_validation_charged: false,
            bridge_features: None,
            bridge_axis: 0,
            bridge_owner: 0,
            occupied,
            scale: config
                .placement
                .width()
                .max(config.placement.height())
                .max(1) as f32,
            output_bytes,
            scratch_bytes,
        })
    }

    fn push_spatial(&mut self, point: Point, kind: usize, owner: i64) {
        push_spatial(
            &mut self.input.spatial_features,
            &mut self.input.spatial_owners,
            point,
            self.scale,
            kind,
            owner,
        );
    }

    pub(crate) fn advance(
        &mut self,
        source: &Session,
        config: &Config,
        connection: usize,
        bridges: &[Bridge],
        bodies: &[Point],
        occupied_layers: [&[Point]; 2],
        allowance: &mut RequestAllowance,
    ) -> Result<(), RequestFailure> {
        let blocks = source.problem.blocks.len();
        let terminals = source.problem.terminals.len();
        let connection_offset = blocks + terminals;
        loop {
            match self.phase {
                Phase::Blocks => {
                    while self.index < blocks {
                        allowance.charge(WorkKind::GraphRows, 1)?;
                        let block = &source.problem.blocks[self.index];
                        let definition = source
                            .catalogs
                            .facilities
                            .facility(&block.facility)
                            .expect("constructive problem admitted facilities");
                        let mut row = [0.; ENTITY_FEATURES];
                        row[0] = 1.;
                        row[8] = definition.footprint.width as f32 / self.scale;
                        row[9] = definition.footprint.height as f32 / self.scale;
                        row[10] = definition.allowed_rotations.len() as f32;
                        if let Some(pose) = &source.poses[self.index] {
                            row[2] = 1.;
                            row[11] = pose.position.x as f32 / self.scale;
                            row[12] = pose.position.y as f32 / self.scale;
                            row[13] = (pose.rotation as f32).to_radians().sin();
                            row[14] = (pose.rotation as f32).to_radians().cos();
                        }
                        if matches!(source.focus, Some(Focus::Block { block, .. }) if block == self.index)
                        {
                            row[7] = 1.;
                        }
                        self.input.entity_features.push(row);
                        self.index += 1;
                    }
                    self.phase = Phase::Terminals;
                    self.index = 0;
                }
                Phase::Terminals => {
                    while self.index < terminals {
                        allowance.charge(WorkKind::GraphRows, 1)?;
                        let terminal = &source.problem.terminals[self.index];
                        let mut row = [0.; ENTITY_FEATURES];
                        row[16
                            + matches!(
                                terminal.direction,
                                aic_layout::physical::TerminalDirection::Output
                            ) as usize] = 1.;
                        row[5 + layer(terminal.transport)] = 1.;
                        if let Some(pose) = &source.terminal_poses[self.index] {
                            row[2] = 1.;
                            row[11] = pose.position.x as f32 / self.scale;
                            row[12] = pose.position.y as f32 / self.scale;
                        }
                        if matches!(source.focus, Some(Focus::Terminal { terminal, .. }) if terminal == self.index)
                        {
                            row[7] = 1.;
                        }
                        self.input.entity_features.push(row);
                        self.index += 1;
                    }
                    self.phase = Phase::Connections;
                    self.index = 0;
                }
                Phase::Connections => {
                    while self.index < source.problem.connections.len() {
                        allowance.charge(WorkKind::GraphRows, 3)?;
                        let item = &source.problem.connections[self.index];
                        let entity = connection_offset + self.index;
                        let mut row = [0.; ENTITY_FEATURES];
                        row[1] = 1.;
                        row[3] = source.routes[self.index].is_some() as u8 as f32;
                        row[4] = 0.;
                        row[5 + layer(item.transport)] = 1.;
                        row[15] =
                            (item.rate.numerator as f32 / item.rate.denominator as f32).ln_1p();
                        if matches!(source.focus, Some(Focus::Route { connection, .. }) if connection == self.index)
                        {
                            row[7] = 1.;
                        }
                        self.input.entity_features.push(row);
                        let source_entity = endpoint_entity(source, &item.source);
                        let target_entity = endpoint_entity(source, &item.target);
                        self.input.relation_index.extend([
                            [source_entity, entity as i64],
                            [entity as i64, target_entity],
                        ]);
                        let mut source_row = [0.; RELATION_FEATURES];
                        source_row[0] = 1.;
                        source_row[2 + layer(item.transport)] = 1.;
                        source_row[4] = row[15];
                        let mut target_row = source_row;
                        target_row[0] = 0.;
                        target_row[1] = 1.;
                        self.input
                            .relation_features
                            .extend([source_row, target_row]);
                        self.index += 1;
                    }
                    self.phase = Phase::BlockSpatial;
                    self.index = 0;
                }
                Phase::BlockSpatial => {
                    while self.index < source.poses.len() {
                        let Some(pose) = &source.poses[self.index] else {
                            self.index += 1;
                            continue;
                        };
                        let definition = source
                            .catalogs
                            .facilities
                            .facility(&source.problem.blocks[self.index].facility)
                            .expect("constructive problem admitted facilities");
                        let (width, height) = if pose.rotation % 180 == 0 {
                            (definition.footprint.width, definition.footprint.height)
                        } else {
                            (definition.footprint.height, definition.footprint.width)
                        };
                        let area = usize::try_from(width * height).expect("admitted footprint");
                        while self.inner < area {
                            allowance.charge(WorkKind::GraphRows, 1)?;
                            let point = Point {
                                x: pose.position.x + (self.inner as i64 % width),
                                y: pose.position.y + (self.inner as i64 / width),
                            };
                            self.push_spatial(point, 2, self.index as i64);
                            self.inner += 1;
                        }
                        self.index += 1;
                        self.inner = 0;
                    }
                    self.phase = Phase::RouteSpatial;
                    self.index = 0;
                }
                Phase::RouteSpatial => {
                    while self.index < source.routes.len() {
                        let Some(route) = &source.routes[self.index] else {
                            self.index += 1;
                            continue;
                        };
                        let kind = 3 + layer(source.problem.connections[self.index].transport);
                        while self.inner < route.cells.len() {
                            if !self.route_cell_graph_charged {
                                allowance.charge(WorkKind::GraphRows, 1)?;
                                self.route_cell_graph_charged = true;
                            }
                            allowance.charge(WorkKind::RouteCells, 1)?;
                            self.push_spatial(
                                route.cells[self.inner],
                                kind,
                                (connection_offset + self.index) as i64,
                            );
                            self.route_cell_graph_charged = false;
                            self.inner += 1;
                        }
                        self.index += 1;
                        self.inner = 0;
                    }
                    self.phase = Phase::Bridges;
                    self.index = 0;
                }
                Phase::Bridges => {
                    while self.index < bridges.len() {
                        let bridge = &bridges[self.index];
                        if self.input.bridge_resources.len() == self.index {
                            allowance.charge(
                                WorkKind::CopiedBytes,
                                crate::constructive::routing_candidates::bridge_copy_bytes(bridge),
                            )?;
                            self.input
                                .bridge_resources
                                .push(try_clone_bridge(bridge).map_err(|_| {
                                    host_allocation_stop(
                                        crate::constructive::routing_candidates::bridge_copy_bytes(
                                            bridge,
                                        ),
                                    )
                                })?);
                        }
                        if !self.bridge_catalog_charged {
                            allowance.charge(
                                WorkKind::CatalogVisits,
                                source.catalogs.components.catalog().components.len(),
                            )?;
                            self.bridge_catalog_charged = true;
                        }
                        if !self.bridge_validation_charged {
                            allowance.charge(
                                WorkKind::ValidationRecords,
                                bridge
                                    .channels
                                    .iter()
                                    .map(|channel| channel.connections.len())
                                    .sum(),
                            )?;
                            self.bridge_validation_charged = true;
                        }
                        if self.bridge_features.is_none() {
                            self.bridge_features = Some(source.bridge_component_features(bridge));
                        }
                        let (component_code, loads) =
                            self.bridge_features.expect("cached bridge features");
                        while self.bridge_axis < bridge.channels.len() {
                            let channel = &bridge.channels[self.bridge_axis];
                            let kind = match channel.axis {
                                BridgeAxis::Horizontal => 9,
                                BridgeAxis::Vertical => 10,
                            };
                            while self.bridge_owner < channel.connections.len() {
                                allowance.charge(WorkKind::GraphRows, 1)?;
                                let owner = source.connections
                                    [&channel.connections[self.bridge_owner]]
                                    + connection_offset;
                                self.push_spatial(bridge.position, kind, owner as i64);
                                let row =
                                    self.input.spatial_features.last_mut().expect("bridge row");
                                row[11] = component_code;
                                row[12] = loads[self.bridge_axis];
                                row[13] = bridge.rotation as f32 / 270.;
                                self.bridge_owner += 1;
                            }
                            self.bridge_axis += 1;
                            self.bridge_owner = 0;
                        }
                        self.index += 1;
                        self.bridge_axis = 0;
                        self.bridge_catalog_charged = false;
                        self.bridge_validation_charged = false;
                        self.bridge_features = None;
                    }
                    self.phase = Phase::TerminalSpatial;
                    self.index = 0;
                }
                Phase::TerminalSpatial => {
                    while self.index < source.terminal_poses.len() {
                        if let Some(pose) = &source.terminal_poses[self.index] {
                            allowance.charge(WorkKind::GraphRows, 1)?;
                            self.push_spatial(pose.position, 8, (blocks + self.index) as i64);
                        }
                        self.index += 1;
                    }
                    self.phase = Phase::Focus;
                }
                Phase::Focus => {
                    match &source.focus {
                        None => self.input.focus_features[0] = 1.,
                        Some(Focus::Block { block, position }) => {
                            self.input.focus_features[1] = 1.;
                            self.input.focus_features[4] = position.x as f32 / self.scale;
                            self.input.focus_features[5] = position.y as f32 / self.scale;
                            self.input.focus_entity = *block as i64;
                        }
                        Some(Focus::Terminal { terminal, position }) => {
                            self.input.focus_features[11] = 1.;
                            self.input.focus_features[4] = position.x as f32 / self.scale;
                            self.input.focus_features[5] = position.y as f32 / self.scale;
                            self.input.focus_entity = (blocks + terminal) as i64;
                        }
                        Some(Focus::Route {
                            connection,
                            endpoint,
                            target,
                            active,
                        }) => {
                            self.input.focus_features[2] = 1.;
                            self.input.focus_features[3] = *active as u8 as f32;
                            self.input.focus_features[4] = endpoint.x as f32 / self.scale;
                            self.input.focus_features[5] = endpoint.y as f32 / self.scale;
                            self.input.focus_features[6] = target.x as f32 / self.scale;
                            self.input.focus_features[7] = target.y as f32 / self.scale;
                            self.input.focus_features[8] =
                                (target.x - endpoint.x) as f32 / self.scale;
                            self.input.focus_features[9] =
                                (target.y - endpoint.y) as f32 / self.scale;
                            self.input.focus_features[10] =
                                endpoint.distance(*target) as f32 / self.scale;
                            self.input.focus_entity = (connection_offset + connection) as i64;
                        }
                    }
                    self.phase = Phase::OccupiedBodies;
                    self.index = 0;
                }
                Phase::OccupiedBodies => {
                    while self.index < bodies.len() {
                        allowance.charge(WorkKind::ValidationRecords, 1)?;
                        self.occupied.insert(bodies[self.index]);
                        self.index += 1;
                    }
                    self.phase = Phase::OccupiedLayer0;
                    self.index = 0;
                }
                Phase::OccupiedLayer0 => {
                    while self.index < occupied_layers[0].len() {
                        allowance.charge(WorkKind::ValidationRecords, 1)?;
                        self.occupied.insert(occupied_layers[0][self.index]);
                        self.index += 1;
                    }
                    self.phase = Phase::OccupiedLayer1;
                    self.index = 0;
                }
                Phase::OccupiedLayer1 => {
                    while self.index < occupied_layers[1].len() {
                        allowance.charge(WorkKind::ValidationRecords, 1)?;
                        self.occupied.insert(occupied_layers[1][self.index]);
                        self.index += 1;
                    }
                    self.phase = Phase::OccupiedTerminals;
                    self.index = 0;
                }
                Phase::OccupiedTerminals => {
                    while self.index < source.terminal_poses.len() {
                        if let Some(pose) = &source.terminal_poses[self.index] {
                            allowance.charge(WorkKind::ValidationRecords, 1)?;
                            self.occupied.insert(pose.position);
                        }
                        self.index += 1;
                    }
                    let placed = source.poses.iter().filter(|pose| pose.is_some()).count()
                        + source
                            .terminal_poses
                            .iter()
                            .filter(|pose| pose.is_some())
                            .count();
                    let placeable = source.poses.len() + source.terminal_poses.len();
                    self.input.global_features = [
                        source.turns as f32 / source.limits.max_turns as f32,
                        (source.limits.max_turns - source.turns) as f32
                            / source.limits.max_turns as f32,
                        source.completed() as f32 / source.routes.len() as f32,
                        placed as f32 / placeable as f32,
                        0.,
                        self.occupied.len() as f32
                            / (config.placement.width() * config.placement.height()) as f32,
                        config.placement.width() as f32 / self.scale,
                        config.placement.height() as f32 / self.scale,
                    ];
                    self.phase = Phase::RouteFocus;
                    self.index = 0;
                }
                Phase::RouteFocus => {
                    while self.index < self.input.entity_features.len() {
                        allowance.charge(WorkKind::ValidationRecords, 1)?;
                        self.input.entity_features[self.index][7] = 0.;
                        self.index += 1;
                    }
                    let entity = connection_offset + connection;
                    self.input.entity_features[entity][7] = 1.;
                    self.input.focus_features = [0.; FOCUS_FEATURES];
                    self.input.focus_features[2] = 1.;
                    self.input.focus_entity = entity as i64;
                    self.phase = Phase::Done;
                }
                Phase::Done => return Ok(()),
            }
        }
    }

    pub(crate) fn reserved_bytes(&self) -> usize {
        self.output_bytes.saturating_add(self.scratch_bytes)
    }

    pub(crate) fn finish(mut self, allowance: &mut RequestAllowance) -> (Arc<Input>, usize) {
        let input = std::mem::replace(
            &mut self.input,
            Input {
                schema: SCHEMA,
                entity_features: Vec::new(),
                relation_index: Vec::new(),
                relation_features: Vec::new(),
                spatial_features: Vec::new(),
                spatial_owners: Vec::new(),
                global_features: [0.; GLOBAL_FEATURES],
                focus_features: [0.; FOCUS_FEATURES],
                focus_entity: -1,
                bridge_resources: Vec::new(),
            },
        );
        let output_bytes = self.output_bytes;
        let scratch_bytes = self.scratch_bytes;
        drop(self);
        allowance.release_peak(scratch_bytes);
        (Arc::new(input), output_bytes)
    }
}
