//! Charge-once continuation of exact crossing-option preparation.

use aic_layout::geometry::{Point, direction};
use aic_layout::logistics::{LogisticsComponentKind, ValidatedLogisticsComponentCatalog};
use aic_layout::projection::cardinal_index;
use aic_layout::rate::Rate;
use indexmap::{IndexMap, IndexSet};

use super::{Carrier, Crossing, CrossingVisit, Crossings, RouteJob};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Visits,
    Poses,
    Candidates,
    Done,
}

struct CandidateProgress {
    visit_index: usize,
    direction: Option<usize>,
    network: Option<usize>,
    rate: Rate,
    chain: Option<usize>,
    straight: bool,
    next_index: usize,
    pose_index: usize,
}

impl CandidateProgress {
    fn new() -> Self {
        Self {
            visit_index: 0,
            direction: None,
            network: None,
            rate: Rate::zero(),
            chain: None,
            straight: true,
            next_index: 0,
            pose_index: 0,
        }
    }
}

/// Retains the route-cell, catalog-pose, and crossing-candidate scans across
/// a temporary visitor stop. Inputs must remain the same immutable request.
pub struct CrossingOptionsCursor {
    phase: Phase,
    carrier_index: usize,
    cell_index: usize,
    visits: IndexMap<Point, Vec<(usize, usize)>>,
    component_index: usize,
    component_charged: bool,
    rotation_index: usize,
    poses: Vec<(usize, i64)>,
    candidate_index: usize,
    candidate: Option<CandidateProgress>,
    options: Crossings,
}

impl Default for CrossingOptionsCursor {
    fn default() -> Self {
        Self::new()
    }
}

impl CrossingOptionsCursor {
    pub fn new() -> Self {
        Self {
            phase: Phase::Visits,
            carrier_index: 0,
            cell_index: 0,
            visits: IndexMap::new(),
            component_index: 0,
            component_charged: false,
            rotation_index: 0,
            poses: Vec::new(),
            candidate_index: 0,
            candidate: None,
            options: Crossings::new(),
        }
    }

    /// Returns true when preparation is complete. A visitor stop keeps the
    /// exact next charge position and all constructed options in this cursor.
    pub fn advance<E>(
        &mut self,
        carriers: &[Carrier<'_>],
        job: &RouteJob,
        catalogs: &ValidatedLogisticsComponentCatalog,
        excluded: &IndexSet<Point>,
        visit: impl FnMut(CrossingVisit) -> Result<(), E>,
    ) -> Result<bool, E> {
        self.advance_with(
            carriers.len(),
            |index| {
                let carrier = &carriers[index];
                Carrier {
                    id: carrier.id,
                    network: carrier.network,
                    transport: carrier.transport,
                    rate: carrier.rate,
                    cells: carrier.cells,
                }
            },
            job,
            catalogs,
            excluded,
            visit,
        )
    }

    /// Access immutable carrier geometry by index without copying its cells
    /// or rebuilding a carrier array on each resumed page.
    pub fn advance_with<'a, E>(
        &mut self,
        carrier_count: usize,
        carrier: impl Fn(usize) -> Carrier<'a>,
        job: &RouteJob,
        catalogs: &ValidatedLogisticsComponentCatalog,
        excluded: &IndexSet<Point>,
        mut visit: impl FnMut(CrossingVisit) -> Result<(), E>,
    ) -> Result<bool, E> {
        if self.phase == Phase::Visits {
            while self.carrier_index < carrier_count {
                let carrier = carrier(self.carrier_index);
                if carrier.transport != job.transport || self.cell_index == carrier.cells.len() {
                    self.carrier_index += 1;
                    self.cell_index = 0;
                    continue;
                }
                visit(CrossingVisit::RouteCell)?;
                let point = carrier.cells[self.cell_index];
                self.visits
                    .entry(point)
                    .or_default()
                    .push((self.carrier_index, self.cell_index));
                self.cell_index += 1;
            }
            self.phase = Phase::Poses;
        }

        let definitions = &catalogs.catalog().components;
        let sufficient = |quantity: i64, duration: i64, rate: Rate| {
            aic_layout::validation::within_capacity(rate, quantity, duration)
        };
        if self.phase == Phase::Poses {
            while self.component_index < definitions.len() {
                let definition = &definitions[self.component_index];
                if !self.component_charged {
                    visit(CrossingVisit::CatalogEntry)?;
                    self.component_charged = true;
                }
                let eligible = definition.kind == LogisticsComponentKind::Bridge
                    && definition.transport == job.transport
                    && definition.footprint.width == 1
                    && definition.footprint.height == 1
                    && sufficient(
                        definition.capacity.quantity,
                        definition.capacity.duration_ms,
                        job.rate,
                    );
                if eligible {
                    while self.rotation_index < definition.allowed_rotations.len() {
                        visit(CrossingVisit::CatalogEntry)?;
                        self.poses.push((
                            self.component_index,
                            definition.allowed_rotations[self.rotation_index],
                        ));
                        self.rotation_index += 1;
                    }
                }
                self.component_index += 1;
                self.component_charged = false;
                self.rotation_index = 0;
            }
            self.phase = Phase::Candidates;
        }

        if self.phase == Phase::Candidates {
            while self.candidate_index < self.visits.len() {
                let (point, carriers_at_point) = self
                    .visits
                    .get_index(self.candidate_index)
                    .expect("candidate index is in range");
                let point = *point;
                if self.candidate.is_none() {
                    visit(CrossingVisit::Candidate)?;
                    if excluded.contains(&point) {
                        self.complete_candidate();
                        continue;
                    }
                    self.candidate = Some(CandidateProgress::new());
                }
                let progress = self.candidate.as_mut().expect("candidate was started");
                while progress.visit_index < carriers_at_point.len() {
                    let (carrier_index, cell_index) = carriers_at_point[progress.visit_index];
                    visit(CrossingVisit::RouteCell)?;
                    progress.visit_index += 1;
                    let carrier = carrier(carrier_index);
                    if carrier.id == job.id
                        || cell_index == 0
                        || cell_index + 1 == carrier.cells.len()
                    {
                        progress.straight = false;
                        break;
                    }
                    let Some(heading) = direction(carrier.cells[cell_index - 1], point) else {
                        progress.straight = false;
                        break;
                    };
                    if direction(point, carrier.cells[cell_index + 1]) != Some(heading)
                        || progress.direction.is_some_and(|old| old != heading)
                        || progress.network.is_some_and(|old| old != carrier.network)
                    {
                        progress.straight = false;
                        break;
                    }
                    let Ok(rate) = progress.rate.checked_add(carrier.rate) else {
                        progress.straight = false;
                        break;
                    };
                    progress.rate = rate;
                    progress.direction = Some(heading);
                    progress.network = Some(carrier.network);
                    progress.chain.get_or_insert(carrier.id);
                }
                if !progress.straight || progress.direction.is_none() {
                    self.complete_candidate();
                    continue;
                }
                let heading = progress.direction.expect("checked direction");
                while progress.next_index < 2 {
                    let next = if progress.next_index == 0 {
                        (heading + 1) % 4
                    } else {
                        (heading + 3) % 4
                    };
                    while progress.pose_index < self.poses.len() {
                        let (definition_index, rotation) = self.poses[progress.pose_index];
                        visit(CrossingVisit::CatalogEntry)?;
                        progress.pose_index += 1;
                        let definition = &definitions[definition_index];
                        if sufficient(
                            definition.capacity.quantity,
                            definition.capacity.duration_ms,
                            progress.rate,
                        ) && [heading, next].iter().all(|&axis| {
                            definition.input_directions.iter().any(|&edge| {
                                (cardinal_index(edge) + rotation as usize / 90) % 4
                                    == (axis + 2) % 4
                            }) && definition.output_directions.iter().any(|&edge| {
                                (cardinal_index(edge) + rotation as usize / 90) % 4 == axis
                            })
                        }) {
                            self.options.insert(
                                (point, next),
                                Crossing {
                                    position: point,
                                    direction: next,
                                    component: definition.id.clone(),
                                    rotation,
                                    transport: job.transport,
                                    kind: LogisticsComponentKind::Bridge,
                                    carrier_chain: progress.chain.expect("nonempty carrier visits"),
                                    carrier_network: progress
                                        .network
                                        .expect("nonempty carrier visits"),
                                },
                            );
                            break;
                        }
                    }
                    progress.next_index += 1;
                    progress.pose_index = 0;
                }
                self.complete_candidate();
            }
            self.phase = Phase::Done;
        }
        Ok(self.phase == Phase::Done)
    }

    /// Consume a cursor after `advance` reports completion.
    pub fn finish(self) -> Crossings {
        self.options
    }

    fn complete_candidate(&mut self) {
        *self
            .visits
            .get_index_mut(self.candidate_index)
            .expect("candidate index is in range")
            .1 = Vec::new();
        self.candidate = None;
        self.candidate_index += 1;
    }
}
