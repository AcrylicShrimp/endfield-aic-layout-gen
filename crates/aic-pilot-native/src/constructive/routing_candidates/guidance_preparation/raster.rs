//! Resumable channel-major raster construction for one route-guidance input.

use aic_layout::{
    geometry::{Bounds, Point, layer},
    logistics::TransportKind,
    physical::Bridge,
};

use super::super::{
    AllowanceStop, EndpointPair, GUIDANCE_GRID_CHANNELS, RequestAllowance, Session, WorkKind,
    bridge_copy_bytes,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Bodies,
    Terminals,
    OccupiedSameLayer,
    OccupiedOtherLayer,
    Bridges,
    SelectedPairs,
    RemainingPairs,
    Cells,
    Done,
}

/// The graph's occupied-union scan reuses the raster's already copied keys.
pub(super) struct RasterKeys {
    pub(super) bodies: Vec<Point>,
    pub(super) occupied_same: Vec<Point>,
    pub(super) occupied_other: Vec<Point>,
    pub(super) transport_layer: usize,
    reserved_bytes: usize,
}

impl RasterKeys {
    pub(super) fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }
}

pub(super) struct RasterCursor {
    bounds: Bounds,
    width: usize,
    area: usize,
    transport_layer: usize,
    remaining: usize,
    grid: Vec<f32>,
    bridge_resources: Vec<Bridge>,
    bodies: Vec<Point>,
    occupied_same: Vec<Point>,
    occupied_other: Vec<Point>,
    bridge_keys: Vec<(TransportKind, Point)>,
    phase: Phase,
    index: usize,
    remaining_outer: usize,
    remaining_inner: usize,
    output_bytes: usize,
    scratch_bytes: usize,
}

impl RasterCursor {
    pub(super) fn new(
        source: &Session,
        connection: usize,
        bounds: Bounds,
        remaining: usize,
        allowance: &mut RequestAllowance,
    ) -> Result<Self, AllowanceStop> {
        let width = usize::try_from(bounds.width()).expect("validated positive width");
        let height = usize::try_from(bounds.height()).expect("validated positive height");
        let area = width.saturating_mul(height);
        let transport_layer = layer(source.problem.connections[connection].transport);
        let scratch_bytes = source
            .bodies
            .len()
            .saturating_add(source.occupied[0].len())
            .saturating_add(source.occupied[1].len())
            .saturating_mul(std::mem::size_of::<Point>())
            .saturating_add(
                source
                    .bridges
                    .len()
                    .saturating_mul(std::mem::size_of::<(TransportKind, Point)>()),
            );
        let output_bytes = area
            .saturating_mul(GUIDANCE_GRID_CHANNELS)
            .saturating_mul(std::mem::size_of::<f32>())
            .saturating_add(source.bridge_copy_bytes());
        allowance.reserve_peak(output_bytes.saturating_add(scratch_bytes))?;
        if let Err(stop) = allowance.charge(WorkKind::CopiedBytes, scratch_bytes) {
            allowance.release_peak(output_bytes.saturating_add(scratch_bytes));
            return Err(stop);
        }
        let mut bodies = Vec::with_capacity(source.bodies.len());
        bodies.extend(source.bodies.iter().copied());
        let mut occupied_same = Vec::with_capacity(source.occupied[transport_layer].len());
        occupied_same.extend(source.occupied[transport_layer].keys().copied());
        let mut occupied_other = Vec::with_capacity(source.occupied[1 - transport_layer].len());
        occupied_other.extend(source.occupied[1 - transport_layer].keys().copied());
        let mut bridge_keys = Vec::with_capacity(source.bridges.len());
        bridge_keys.extend(source.bridges.keys().copied());
        bridge_keys
            .sort_unstable_by(|left, right| source.bridges[left].id.cmp(&source.bridges[right].id));
        Ok(Self {
            bounds,
            width,
            area,
            transport_layer,
            remaining,
            grid: vec![0.; GUIDANCE_GRID_CHANNELS * area],
            bridge_resources: Vec::with_capacity(source.bridges.len()),
            bodies,
            occupied_same,
            occupied_other,
            bridge_keys,
            phase: Phase::Bodies,
            index: 0,
            remaining_outer: 0,
            remaining_inner: 0,
            output_bytes,
            scratch_bytes,
        })
    }

    fn mark(&mut self, channel: usize, point: Point, value: f32) {
        if self.bounds.contains(point) {
            let index = usize::try_from(point.y - self.bounds.min_y).expect("point in bounds")
                * self.width
                + usize::try_from(point.x - self.bounds.min_x).expect("point in bounds");
            let offset = channel * self.area + index;
            self.grid[offset] = self.grid[offset].max(value);
        }
    }

    pub(super) fn advance(
        &mut self,
        source: &Session,
        connection: usize,
        selected_pairs: &[EndpointPair],
        remaining_pairs: &[Vec<EndpointPair>],
        allowance: &mut RequestAllowance,
    ) -> Result<(), AllowanceStop> {
        loop {
            match self.phase {
                Phase::Bodies => {
                    while self.index < self.bodies.len() {
                        allowance.charge(WorkKind::BlockedVisits, 1)?;
                        self.mark(0, self.bodies[self.index], 1.);
                        self.index += 1;
                    }
                    self.phase = Phase::Terminals;
                    self.index = 0;
                }
                Phase::Terminals => {
                    while self.index < source.terminal_poses.len() {
                        if let Some(pose) = &source.terminal_poses[self.index] {
                            allowance.charge(WorkKind::BlockedVisits, 1)?;
                            self.mark(1, pose.position, 1.);
                        }
                        self.index += 1;
                    }
                    self.phase = Phase::OccupiedSameLayer;
                    self.index = 0;
                }
                Phase::OccupiedSameLayer => {
                    while self.index < self.occupied_same.len() {
                        let point = self.occupied_same[self.index];
                        let routes = &source.occupied[self.transport_layer][&point];
                        allowance.charge(WorkKind::RouteCells, routes.len())?;
                        let channel = if source.shared_cell_available(point, connection) {
                            2
                        } else {
                            3
                        };
                        self.mark(channel, point, 1.);
                        self.mark(13, point, (routes.len() as f32 / 8.).min(1.));
                        self.index += 1;
                    }
                    self.phase = Phase::OccupiedOtherLayer;
                    self.index = 0;
                }
                Phase::OccupiedOtherLayer => {
                    while self.index < self.occupied_other.len() {
                        allowance.charge(WorkKind::RouteCells, 1)?;
                        self.mark(4, self.occupied_other[self.index], 1.);
                        self.index += 1;
                    }
                    self.phase = Phase::Bridges;
                    self.index = 0;
                }
                Phase::Bridges => {
                    while self.index < self.bridge_keys.len() {
                        let bridge = &source.bridges[&self.bridge_keys[self.index]];
                        if self.bridge_resources.len() == self.index {
                            allowance.charge(WorkKind::CopiedBytes, bridge_copy_bytes(bridge))?;
                            self.bridge_resources.push(bridge.clone());
                        }
                        allowance.charge(WorkKind::ValidationRecords, 1)?;
                        let transport = source
                            .catalogs
                            .components
                            .component(&bridge.component)
                            .expect("installed bridge has a validated catalog component")
                            .transport;
                        if layer(transport) == self.transport_layer {
                            for (axis, channel) in bridge.channels.iter().enumerate() {
                                self.mark(
                                    14 + axis,
                                    bridge.position,
                                    (channel.connections.len() as f32 / 8.).min(1.),
                                );
                            }
                        }
                        self.index += 1;
                    }
                    self.phase = Phase::SelectedPairs;
                    self.index = 0;
                }
                Phase::SelectedPairs => {
                    while self.index < selected_pairs.len() {
                        allowance.charge(WorkKind::EndpointVisits, 1)?;
                        let pair = &selected_pairs[self.index];
                        self.mark(5, pair.source.point, 1.);
                        self.mark(6, pair.target.point, 1.);
                        self.index += 1;
                    }
                    self.phase = Phase::RemainingPairs;
                    self.index = 0;
                }
                Phase::RemainingPairs => {
                    while self.remaining_outer < remaining_pairs.len() {
                        let pairs = &remaining_pairs[self.remaining_outer];
                        while self.remaining_inner < pairs.len() {
                            allowance.charge(WorkKind::EndpointVisits, 1)?;
                            let pair = &pairs[self.remaining_inner];
                            self.mark(7, pair.source.point, 1.);
                            self.mark(8, pair.target.point, 1.);
                            self.remaining_inner += 1;
                        }
                        self.remaining_outer += 1;
                        self.remaining_inner = 0;
                    }
                    self.phase = Phase::Cells;
                    self.index = 0;
                }
                Phase::Cells => {
                    while self.index < self.area {
                        allowance.charge(WorkKind::RasterCells, GUIDANCE_GRID_CHANNELS)?;
                        let x = self.bounds.min_x + (self.index % self.width) as i64;
                        let y = self.bounds.min_y + (self.index / self.width) as i64;
                        let point = Point { x, y };
                        if x == self.bounds.min_x
                            || x == self.bounds.max_x
                            || y == self.bounds.min_y
                            || y == self.bounds.max_y
                        {
                            self.mark(9, point, 1.);
                        }
                        self.grid[(10 + self.transport_layer) * self.area + self.index] = 1.;
                        let rate = &source.problem.connections[connection].rate;
                        self.grid[12 * self.area + self.index] =
                            (rate.numerator as f32 / rate.denominator as f32).ln_1p();
                        let density = (self.remaining as f32
                            / source.problem.connections.len() as f32)
                            .min(1.);
                        let offset = 13 * self.area + self.index;
                        self.grid[offset] = self.grid[offset].max(density);
                        self.index += 1;
                    }
                    allowance.check_deadline()?;
                    self.phase = Phase::Done;
                }
                Phase::Done => return Ok(()),
            }
        }
    }

    pub(super) fn finish(
        mut self,
        allowance: &mut RequestAllowance,
    ) -> (Vec<f32>, Vec<Bridge>, RasterKeys, usize) {
        let grid = std::mem::take(&mut self.grid);
        let bridges = std::mem::take(&mut self.bridge_resources);
        let keys = RasterKeys {
            bodies: std::mem::take(&mut self.bodies),
            occupied_same: std::mem::take(&mut self.occupied_same),
            occupied_other: std::mem::take(&mut self.occupied_other),
            transport_layer: self.transport_layer,
            reserved_bytes: self.scratch_bytes.saturating_sub(
                self.bridge_keys
                    .len()
                    .saturating_mul(std::mem::size_of::<(TransportKind, Point)>()),
            ),
        };
        let output_bytes = self.output_bytes;
        let released_scratch = self.scratch_bytes - keys.reserved_bytes;
        drop(self);
        allowance.release_peak(released_scratch);
        (grid, bridges, keys, output_bytes)
    }

    pub(super) fn reserved_bytes(&self) -> usize {
        self.output_bytes.saturating_add(self.scratch_bytes)
    }
}
