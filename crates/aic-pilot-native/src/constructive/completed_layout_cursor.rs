//! Retained construction of the independent validator's input.

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, hash_map::Values};
use std::iter::Peekable;

use aic_layout::geometry::Point;
use aic_layout::logistics::TransportKind;
use aic_layout::physical::{
    Bridge, BridgeChannel, Junction, Layout, Problem, Route, SCHEMA, Solution,
};

use super::{Diagnostic, Session, error};

fn next_vec_growth_bytes<T>(values: &Vec<T>) -> (usize, usize) {
    if values.len() < values.capacity() {
        return (0, 0);
    }
    let old = values.capacity().saturating_mul(std::mem::size_of::<T>());
    let next = values
        .capacity()
        .saturating_mul(2)
        .max(4)
        .saturating_mul(std::mem::size_of::<T>());
    (next, old)
}

#[derive(Clone, Copy)]
struct BridgeById<'a>(&'a Bridge);

impl PartialEq for BridgeById<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
impl Eq for BridgeById<'_> {}
impl PartialOrd for BridgeById<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for BridgeById<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.id.cmp(&other.0.id)
    }
}

#[derive(Clone, Copy)]
struct JunctionById<'a>(&'a Junction);

impl PartialEq for JunctionById<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}
impl Eq for JunctionById<'_> {}
impl PartialOrd for JunctionById<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for JunctionById<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.id.cmp(&other.0.id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MaterializationReservation {
    Granted,
    Retry,
    Denied,
}

pub(crate) enum MaterializationAdvance {
    Pending,
    Complete {
        layout: Layout,
        reserved_bytes: usize,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Identity,
    Blocks,
    Terminals,
    Connections,
    Poses,
    TerminalPoses,
    Routes,
    BridgeOrder,
    Bridges,
    JunctionOrder,
    Junctions,
    Complete,
}

/// Borrows one immutable session revision; the caller keeps that owner alive
/// until the finished layout has been validated or discarded.
pub(crate) struct CompletedLayoutCursor<'a> {
    source: &'a Session,
    layout: Option<Layout>,
    phase: Phase,
    index: usize,
    member: usize,
    header_done: bool,
    pending_reservation: Option<usize>,
    pending_replacement_bytes: usize,
    reserved_bytes: usize,
    bridge_values: Peekable<Values<'a, (TransportKind, Point), Bridge>>,
    bridge_order: BinaryHeap<Reverse<BridgeById<'a>>>,
    junction_values: Peekable<Values<'a, (TransportKind, Point), Junction>>,
    junction_order: BinaryHeap<Reverse<JunctionById<'a>>>,
}

impl<'a> CompletedLayoutCursor<'a> {
    pub(crate) fn new(source: &'a Session) -> Self {
        Self {
            source,
            layout: Some(Layout {
                schema: String::new(),
                catalog_sha256: String::new(),
                problem: Problem {
                    blocks: Vec::new(),
                    terminals: Vec::new(),
                    connections: Vec::new(),
                },
                solution: Solution {
                    poses: Vec::new(),
                    terminal_poses: Vec::new(),
                    routes: Vec::new(),
                    bridges: Vec::new(),
                    junctions: Vec::new(),
                },
            }),
            phase: Phase::Identity,
            index: 0,
            member: 0,
            header_done: false,
            pending_reservation: None,
            pending_replacement_bytes: 0,
            reserved_bytes: 0,
            bridge_values: source.bridges.values().peekable(),
            bridge_order: BinaryHeap::new(),
            junction_values: source.junctions.values().peekable(),
            junction_order: BinaryHeap::new(),
        }
    }

    pub(crate) fn reserved_bytes(&self) -> usize {
        self.reserved_bytes
    }

    /// Release a pending builder's externally charged reservation after its
    /// partial layout and temporary ordering state have been dropped.
    pub(crate) fn cancel(self, mut release: impl FnMut(usize)) {
        let mut cursor = self;
        cursor.release_owned(&mut release);
    }

    fn release_owned(&mut self, release: &mut impl FnMut(usize)) {
        self.layout = None;
        self.bridge_order = BinaryHeap::new();
        self.junction_order = BinaryHeap::new();
        self.pending_reservation = None;
        self.pending_replacement_bytes = 0;
        let bytes = std::mem::take(&mut self.reserved_bytes);
        if bytes > 0 {
            release(bytes);
        }
    }

    fn admit(
        &mut self,
        bytes: usize,
        replacement_bytes: usize,
        visit: &mut impl FnMut(usize) -> usize,
        reserve: &mut impl FnMut(usize) -> MaterializationReservation,
    ) -> Result<bool, Diagnostic> {
        if self.pending_reservation.is_none() {
            match reserve(bytes) {
                MaterializationReservation::Granted => {
                    self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
                    self.pending_reservation = Some(bytes);
                    self.pending_replacement_bytes = replacement_bytes;
                }
                MaterializationReservation::Retry => return Ok(false),
                MaterializationReservation::Denied => {
                    return Err(error(
                        "layout-materialization-memory",
                        "completed-layout",
                        "completed layout cannot fit the request memory allowance",
                    ));
                }
            }
        }
        Ok(visit(1) == 1)
    }

    fn done_item(&mut self, release: &mut impl FnMut(usize)) {
        self.pending_reservation = None;
        let old = std::mem::take(&mut self.pending_replacement_bytes);
        if old > 0 {
            self.reserved_bytes -= old;
            release(old);
        }
    }

    fn next_phase(&mut self, phase: Phase) {
        self.phase = phase;
        self.index = 0;
        self.member = 0;
        self.header_done = false;
    }

    pub(crate) fn advance(
        &mut self,
        mut visit: impl FnMut(usize) -> usize,
        mut reserve: impl FnMut(usize) -> MaterializationReservation,
        mut release: impl FnMut(usize),
    ) -> Result<MaterializationAdvance, Diagnostic> {
        let result = self.advance_inner(&mut visit, &mut reserve, &mut release);
        if result.is_err() {
            self.release_owned(&mut release);
        }
        result
    }

    fn advance_inner(
        &mut self,
        mut visit: impl FnMut(usize) -> usize,
        mut reserve: impl FnMut(usize) -> MaterializationReservation,
        mut release: impl FnMut(usize),
    ) -> Result<MaterializationAdvance, Diagnostic> {
        loop {
            match self.phase {
                Phase::Identity => {
                    if !self.admit(
                        std::mem::size_of::<Layout>()
                            + SCHEMA.len()
                            + self.source.catalogs.identity.len(),
                        0,
                        &mut visit,
                        &mut reserve,
                    )? {
                        return Ok(MaterializationAdvance::Pending);
                    }
                    let layout = self.layout.as_mut().unwrap();
                    layout.schema = SCHEMA.to_owned();
                    layout.catalog_sha256 = self.source.catalogs.identity.clone();
                    self.done_item(&mut release);
                    self.next_phase(Phase::Blocks);
                    continue;
                }
                Phase::Blocks => {
                    let Some(block) = self.source.problem.blocks.get(self.index) else {
                        self.next_phase(Phase::Terminals);
                        continue;
                    };
                    let (growth, old) =
                        next_vec_growth_bytes(&self.layout.as_ref().unwrap().problem.blocks);
                    let bytes = growth + block.id.len() + block.facility.len();
                    if !self.admit(bytes, old, &mut visit, &mut reserve)? {
                        return Ok(MaterializationAdvance::Pending);
                    }
                    self.layout
                        .as_mut()
                        .unwrap()
                        .problem
                        .blocks
                        .push(block.clone());
                    self.index += 1;
                }
                Phase::Terminals => {
                    let Some(terminal) = self.source.problem.terminals.get(self.index) else {
                        self.next_phase(Phase::Connections);
                        continue;
                    };
                    let (growth, old) =
                        next_vec_growth_bytes(&self.layout.as_ref().unwrap().problem.terminals);
                    let bytes = growth + terminal.id.len() + terminal.commodity.len();
                    if !self.admit(bytes, old, &mut visit, &mut reserve)? {
                        return Ok(MaterializationAdvance::Pending);
                    }
                    self.layout
                        .as_mut()
                        .unwrap()
                        .problem
                        .terminals
                        .push(terminal.clone());
                    self.index += 1;
                }
                Phase::Connections => {
                    let Some(connection) = self.source.problem.connections.get(self.index) else {
                        self.next_phase(Phase::Poses);
                        continue;
                    };
                    let (growth, old) =
                        next_vec_growth_bytes(&self.layout.as_ref().unwrap().problem.connections);
                    let bytes = growth
                        + connection.id.len()
                        + connection.source.len()
                        + connection.target.len()
                        + connection.commodity.len();
                    if !self.admit(bytes, old, &mut visit, &mut reserve)? {
                        return Ok(MaterializationAdvance::Pending);
                    }
                    self.layout
                        .as_mut()
                        .unwrap()
                        .problem
                        .connections
                        .push(connection.clone());
                    self.index += 1;
                }
                Phase::Poses => {
                    let Some(pose) = self.source.poses.get(self.index) else {
                        self.next_phase(Phase::TerminalPoses);
                        continue;
                    };
                    if let Some(pose) = pose {
                        let (growth, old) =
                            next_vec_growth_bytes(&self.layout.as_ref().unwrap().solution.poses);
                        let bytes = growth + pose.block.len();
                        if !self.admit(bytes, old, &mut visit, &mut reserve)? {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        self.layout
                            .as_mut()
                            .unwrap()
                            .solution
                            .poses
                            .push(pose.clone());
                        self.done_item(&mut release);
                    }
                    self.index += 1;
                    continue;
                }
                Phase::TerminalPoses => {
                    let Some(pose) = self.source.terminal_poses.get(self.index) else {
                        self.next_phase(Phase::Routes);
                        continue;
                    };
                    if let Some(pose) = pose {
                        let (growth, old) = next_vec_growth_bytes(
                            &self.layout.as_ref().unwrap().solution.terminal_poses,
                        );
                        let bytes = growth + pose.terminal.len();
                        if !self.admit(bytes, old, &mut visit, &mut reserve)? {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        self.layout
                            .as_mut()
                            .unwrap()
                            .solution
                            .terminal_poses
                            .push(pose.clone());
                        self.done_item(&mut release);
                    }
                    self.index += 1;
                    continue;
                }
                Phase::Routes => {
                    let Some(route) = self.source.routes.get(self.index) else {
                        self.next_phase(Phase::BridgeOrder);
                        continue;
                    };
                    let Some(route) = route else {
                        self.index += 1;
                        continue;
                    };
                    if !self.header_done {
                        let (growth, old) =
                            next_vec_growth_bytes(&self.layout.as_ref().unwrap().solution.routes);
                        let bytes = growth
                            + route.connection.len()
                            + route.source_port.as_ref().map_or(0, String::len)
                            + route.target_port.as_ref().map_or(0, String::len)
                            + route
                                .cells
                                .len()
                                .saturating_mul(std::mem::size_of::<Point>());
                        if !self.admit(bytes, old, &mut visit, &mut reserve)? {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        self.layout.as_mut().unwrap().solution.routes.push(Route {
                            connection: route.connection.clone(),
                            source_port: route.source_port.clone(),
                            target_port: route.target_port.clone(),
                            cells: Vec::with_capacity(route.cells.len()),
                        });
                        self.header_done = true;
                        self.done_item(&mut release);
                    }
                    if self.member < route.cells.len() {
                        let requested = (route.cells.len() - self.member).min(256);
                        let admitted = visit(requested).min(requested);
                        if admitted == 0 {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        let end = self.member + admitted;
                        self.layout
                            .as_mut()
                            .unwrap()
                            .solution
                            .routes
                            .last_mut()
                            .unwrap()
                            .cells
                            .extend_from_slice(&route.cells[self.member..end]);
                        self.member = end;
                    } else {
                        self.index += 1;
                        self.member = 0;
                        self.header_done = false;
                        continue;
                    }
                }
                Phase::BridgeOrder => {
                    if let Some(&bridge) = self.bridge_values.peek() {
                        let bytes = if self.bridge_order.capacity() == 0 {
                            self.source
                                .bridges
                                .len()
                                .saturating_mul(std::mem::size_of::<Reverse<BridgeById<'_>>>())
                        } else {
                            0
                        };
                        if !self.admit(bytes, 0, &mut visit, &mut reserve)? {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        if self.bridge_order.capacity() == 0 {
                            self.bridge_order =
                                BinaryHeap::with_capacity(self.source.bridges.len());
                        }
                        self.bridge_order.push(Reverse(BridgeById(bridge)));
                        self.bridge_values.next();
                    } else {
                        self.next_phase(Phase::Bridges);
                        continue;
                    }
                }
                Phase::Bridges => {
                    let Some(&Reverse(BridgeById(bridge))) = self.bridge_order.peek() else {
                        let temporary = self
                            .bridge_order
                            .capacity()
                            .saturating_mul(std::mem::size_of::<Reverse<BridgeById<'_>>>());
                        self.bridge_order = BinaryHeap::new();
                        self.reserved_bytes -= temporary;
                        release(temporary);
                        self.next_phase(Phase::JunctionOrder);
                        continue;
                    };
                    if !self.header_done {
                        let (growth, old) =
                            next_vec_growth_bytes(&self.layout.as_ref().unwrap().solution.bridges);
                        let bytes = growth
                            + bridge.id.len()
                            + bridge.component.len()
                            + bridge
                                .channels
                                .iter()
                                .map(|c| c.commodity.len())
                                .sum::<usize>();
                        if !self.admit(bytes, old, &mut visit, &mut reserve)? {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        let copy = Bridge {
                            id: bridge.id.clone(),
                            component: bridge.component.clone(),
                            position: bridge.position,
                            rotation: bridge.rotation,
                            channels: std::array::from_fn(|axis| BridgeChannel {
                                axis: bridge.channels[axis].axis,
                                commodity: bridge.channels[axis].commodity.clone(),
                                connections: Vec::new(),
                            }),
                        };
                        self.layout.as_mut().unwrap().solution.bridges.push(copy);
                        self.header_done = true;
                        self.done_item(&mut release);
                    }
                    let first_len = bridge.channels[0].connections.len();
                    let (axis, owner_index) = if self.member < first_len {
                        (0, self.member)
                    } else {
                        (1, self.member - first_len)
                    };
                    if let Some(owner) = bridge.channels[axis].connections.get(owner_index) {
                        let (growth, old) = next_vec_growth_bytes(
                            &self
                                .layout
                                .as_ref()
                                .unwrap()
                                .solution
                                .bridges
                                .last()
                                .unwrap()
                                .channels[axis]
                                .connections,
                        );
                        let bytes = growth + owner.len();
                        if !self.admit(bytes, old, &mut visit, &mut reserve)? {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        self.layout
                            .as_mut()
                            .unwrap()
                            .solution
                            .bridges
                            .last_mut()
                            .unwrap()
                            .channels[axis]
                            .connections
                            .push(owner.clone());
                        self.member += 1;
                    } else {
                        self.bridge_order.pop();
                        self.member = 0;
                        self.header_done = false;
                        continue;
                    }
                }
                Phase::JunctionOrder => {
                    if let Some(&junction) = self.junction_values.peek() {
                        let bytes = if self.junction_order.capacity() == 0 {
                            self.source
                                .junctions
                                .len()
                                .saturating_mul(std::mem::size_of::<Reverse<JunctionById<'_>>>())
                        } else {
                            0
                        };
                        if !self.admit(bytes, 0, &mut visit, &mut reserve)? {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        if self.junction_order.capacity() == 0 {
                            self.junction_order =
                                BinaryHeap::with_capacity(self.source.junctions.len());
                        }
                        self.junction_order.push(Reverse(JunctionById(junction)));
                        self.junction_values.next();
                    } else {
                        self.next_phase(Phase::Junctions);
                        continue;
                    }
                }
                Phase::Junctions => {
                    let Some(&Reverse(JunctionById(junction))) = self.junction_order.peek() else {
                        let temporary = self
                            .junction_order
                            .capacity()
                            .saturating_mul(std::mem::size_of::<Reverse<JunctionById<'_>>>());
                        self.junction_order = BinaryHeap::new();
                        self.reserved_bytes -= temporary;
                        release(temporary);
                        self.next_phase(Phase::Complete);
                        continue;
                    };
                    if !self.header_done {
                        let (growth, old) = next_vec_growth_bytes(
                            &self.layout.as_ref().unwrap().solution.junctions,
                        );
                        if !self.admit(
                            growth + junction.id.len() + junction.component.len(),
                            old,
                            &mut visit,
                            &mut reserve,
                        )? {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        let copy = Junction {
                            id: junction.id.clone(),
                            component: junction.component.clone(),
                            position: junction.position,
                            rotation: junction.rotation,
                            connections: Vec::new(),
                        };
                        self.layout.as_mut().unwrap().solution.junctions.push(copy);
                        self.header_done = true;
                        self.done_item(&mut release);
                    }
                    if let Some(owner) = junction.connections.get(self.member) {
                        let (growth, old) = next_vec_growth_bytes(
                            &self
                                .layout
                                .as_ref()
                                .unwrap()
                                .solution
                                .junctions
                                .last()
                                .unwrap()
                                .connections,
                        );
                        let bytes = growth + owner.len();
                        if !self.admit(bytes, old, &mut visit, &mut reserve)? {
                            return Ok(MaterializationAdvance::Pending);
                        }
                        self.layout
                            .as_mut()
                            .unwrap()
                            .solution
                            .junctions
                            .last_mut()
                            .unwrap()
                            .connections
                            .push(owner.clone());
                        self.member += 1;
                    } else {
                        self.junction_order.pop();
                        self.member = 0;
                        self.header_done = false;
                        continue;
                    }
                }
                Phase::Complete => {
                    return Ok(MaterializationAdvance::Complete {
                        layout: self.layout.take().expect("completed layout held"),
                        reserved_bytes: std::mem::take(&mut self.reserved_bytes),
                    });
                }
            }
            self.done_item(&mut release);
        }
    }
}
