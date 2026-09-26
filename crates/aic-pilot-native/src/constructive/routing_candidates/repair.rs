//! Atomic removal and replacement of committed routes.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;

use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::{Point, direction, layer},
    logistics::{LogisticsComponentKind, TransportKind},
    physical::{Bridge, BridgeAxis, Junction, Metrics, Pose, Route, TerminalPose},
    projection::{cardinal_index, project_port},
    rate::Rate,
    validation::{SiteArmUse, junction_options_with_visit, validate_with_visit, within_capacity},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    AllowanceCaps, AllowanceStop, Candidate, Config, Guidance, Report, Request, RequestAllowance,
    WorkKind, json_size_with_allowance,
};
use crate::constructive::{
    ActiveRoute, Command, Diagnostic, Focus, ReplayMeter, ReplayWork, Session, Step, UsedPort,
    error,
};

pub const REPAIR_REQUEST_SCHEMA: &str = "aic-constructive-repair-request-v1";
pub const REPAIR_CANDIDATE_SCHEMA: &str = "aic-constructive-repair-candidate-v1";

/// Authorization is made from a live state and caller policy, independently
/// of any proposed replacement witness.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepairRequest {
    pub schema: String,
    pub state_revision: u64,
    pub state_sha256: String,
    pub selected_connections: Vec<String>,
    pub allowed_removals: Vec<String>,
    pub max_removed: usize,
    pub allowance_caps: AllowanceCaps,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct RepairCandidate {
    pub schema: String,
    pub state_revision: u64,
    pub removed: Vec<String>,
    pub replacements: Vec<Candidate>,
    pub expanded: usize,
}

/// One screened repair owns exactly one cloned native draft. Its allowance is
/// shared with candidate search, member replay, validation and publication.
pub struct RepairDraft<'a> {
    source: &'a Session,
    request: &'a RepairRequest,
    removed: Vec<String>,
    draft: Option<Session>,
    allowance: &'a mut RequestAllowance,
    reserved_bytes: usize,
    retained_bytes: usize,
    replacements: Vec<Candidate>,
    steps: Vec<Step>,
    progress_placed: usize,
    progress_committed: usize,
    progress_total: usize,
}

/// Binds one trusted request to an immutable live state. Repeated local
/// neighborhoods use this authorization without rehashing that same state.
pub struct RepairScope<'a> {
    source: &'a Session,
    request: RepairRequest,
}

#[derive(Serialize)]
struct BorrowedRepairObservation<'a> {
    revision: u64,
    turns: u64,
    max_turns: u64,
    poses: &'a [Option<Pose>],
    terminal_poses: &'a [Option<TerminalPose>],
    routes: &'a [Option<Route>],
    junctions: Vec<&'a Junction>,
    bridges: Vec<&'a Bridge>,
    active: &'a Option<ActiveRoute>,
    focus: &'a Option<Focus>,
    completed: usize,
    total: usize,
    completion_ratio: f64,
    fully_validated: bool,
}

struct RepairHashWriter<'a> {
    digest: Sha256,
    allowance: Option<&'a mut RequestAllowance>,
    pending_bytes: usize,
    stop: Option<AllowanceStop>,
}

impl Write for RepairHashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.digest.update(bytes);
        self.pending_bytes = self.pending_bytes.saturating_add(bytes.len());
        if self.pending_bytes >= 4096 {
            if let Some(allowance) = self.allowance.as_deref_mut() {
                if let Err(stop) =
                    allowance.record_completed(WorkKind::CopiedBytes, self.pending_bytes)
                {
                    self.stop = Some(stop);
                    return Err(std::io::Error::other("repair identity allowance stopped"));
                }
            }
            self.pending_bytes = 0;
        }
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl RepairScope<'_> {
    pub fn request(&self) -> &RepairRequest {
        &self.request
    }

    pub fn conflict_order(
        &self,
        allowance: &mut RequestAllowance,
    ) -> Result<
        (
            Vec<(String, u64)>,
            BTreeMap<String, Vec<(TransportKind, Point)>>,
            usize,
        ),
        Diagnostic,
    > {
        self.source
            .repair_conflict_order_checked(&self.request, allowance)
    }

    pub fn start_draft<'a>(
        &'a self,
        removed: &[String],
        allowance: &'a mut RequestAllowance,
    ) -> Result<RepairDraft<'a>, Diagnostic> {
        self.source
            .start_repair_draft_validated(&self.request, removed, allowance)
    }
}

pub fn publish_repair_candidate(
    request: &RepairRequest,
    candidate: &RepairCandidate,
    allowance: &mut RequestAllowance,
) -> Result<usize, Diagnostic> {
    allowance
        .preflight(WorkKind::Candidates, 1)
        .map_err(stopped)?;
    let bytes = candidate
        .replacements
        .iter()
        .map(|route| route.cells.len().saturating_mul(128))
        .sum::<usize>()
        .saturating_add(
            request
                .selected_connections
                .iter()
                .chain(&request.allowed_removals)
                .map(|id| id.len().saturating_add(128))
                .sum::<usize>(),
        )
        .saturating_add(8192);
    allowance.reserve_peak(bytes).map_err(stopped)?;
    let result = json_size_with_allowance(&(request, candidate), allowance)
        .and_then(|size| {
            allowance.charge(WorkKind::Candidates, 1)?;
            Ok(size.saturating_add(512))
        })
        .map_err(|failure| failure.into_diagnostic("repair"));
    allowance.release_peak(bytes);
    result
}

impl Drop for RepairDraft<'_> {
    fn drop(&mut self) {
        self.allowance.release_peak(self.reserved_bytes);
    }
}

impl RepairDraft<'_> {
    pub fn session(&self) -> &Session {
        self.draft.as_ref().expect("repair draft is live")
    }

    pub fn session_and_allowance(&mut self) -> (&Session, &mut RequestAllowance) {
        (
            self.draft.as_ref().expect("repair draft is live"),
            self.allowance,
        )
    }

    pub fn route_candidates(
        &mut self,
        connection: &str,
        config: &Config,
        guidance: Option<&Guidance>,
    ) -> Result<Report, Diagnostic> {
        let draft = self.draft.as_ref().expect("repair draft is live");
        draft.route_candidates_inner(
            &Request {
                state_revision: draft.revision,
                connection: connection.to_owned(),
            },
            config,
            guidance,
            self.allowance,
            false,
            false,
        )
    }

    pub fn apply_member(&mut self, candidate: Candidate) -> Result<(), Diagnostic> {
        let candidate_bytes = candidate_reservation_bytes(&candidate);
        let step_bytes = candidate
            .commands
            .len()
            .saturating_mul(std::mem::size_of::<Step>().saturating_add(128))
            .saturating_mul(2);
        let growth_bytes = candidate
            .cells
            .len()
            .saturating_mul(4096)
            .saturating_add(candidate.crossings.len().saturating_mul(4096))
            .saturating_add(candidate.commands.len().saturating_mul(128))
            .saturating_add(4096);
        let reserve = candidate_bytes
            .saturating_add(step_bytes)
            .saturating_add(growth_bytes);
        self.allowance.reserve_peak(reserve).map_err(stopped)?;
        self.reserved_bytes = self.reserved_bytes.saturating_add(reserve);
        let result = (|| {
            self.allowance
                .charge(
                    WorkKind::CopiedBytes,
                    candidate.cells.len().saturating_mul(32),
                )
                .map_err(stopped)?;
            let draft = self.draft.as_mut().expect("repair draft is live");
            draft.install_repair_meter(self.allowance);
            let replay = draft.replay_repair_member_in_place(
                &candidate,
                self.allowance,
                self.progress_placed,
                &mut self.progress_committed,
                self.progress_total,
            );
            let metered = draft.finish_repair_meter(self.allowance);
            let steps = metered.and(replay)?;
            self.allowance
                .charge(
                    WorkKind::CopiedBytes,
                    steps.len().saturating_mul(std::mem::size_of::<Step>()),
                )
                .map_err(stopped)?;
            self.steps.extend(steps);
            self.replacements.push(candidate);
            self.retained_bytes = self
                .retained_bytes
                .saturating_add(candidate_bytes)
                .saturating_add(step_bytes);
            let live_bytes = self
                .source
                .clone_reservation_bytes()
                .saturating_add(draft.clone_reservation_bytes())
                .saturating_add(self.retained_bytes);
            if live_bytes > self.reserved_bytes {
                self.allowance
                    .reserve_peak(live_bytes - self.reserved_bytes)
                    .map_err(stopped)?;
            } else {
                self.allowance
                    .release_peak(self.reserved_bytes - live_bytes);
            }
            self.reserved_bytes = live_bytes;
            Ok(())
        })();
        result
    }

    pub fn apply_member_borrowed(&mut self, candidate: &Candidate) -> Result<(), Diagnostic> {
        let bytes = candidate_reservation_bytes(candidate);
        self.allowance.reserve_peak(bytes).map_err(stopped)?;
        self.reserved_bytes = self.reserved_bytes.saturating_add(bytes);
        self.retained_bytes = self.retained_bytes.saturating_add(bytes);
        let result = (|| {
            self.allowance
                .charge(WorkKind::CopiedBytes, bytes)
                .map_err(stopped)?;
            self.apply_member(candidate.clone())
        })();
        self.retained_bytes -= bytes;
        self.reserved_bytes -= bytes;
        self.allowance.release_peak(bytes);
        result
    }

    pub fn finish(mut self) -> Result<(Session, RepairCandidate, Vec<Step>), Diagnostic> {
        let expected = self
            .request
            .selected_connections
            .iter()
            .chain(&self.removed)
            .cloned()
            .collect::<BTreeSet<_>>();
        let actual = self
            .replacements
            .iter()
            .map(|route| route.connection.clone())
            .collect::<BTreeSet<_>>();
        if actual != expected || actual.len() != self.replacements.len() {
            return Err(error(
                "repair-coverage",
                "repair",
                "screened repair does not cover every selected and removed obligation once",
            ));
        }
        let draft = self.draft.as_mut().expect("repair draft is live");
        if draft.has_active_route() || draft.final_failure.is_some() {
            return Err(error(
                "repair-final-state",
                "repair",
                "screened repair ends in an unstable native state",
            ));
        }
        if draft.is_complete() {
            draft.metrics = Some(draft.validate_complete_repair_layout(self.allowance)?);
        } else {
            draft.validate_incomplete_repair_state(self.source, self.allowance)?;
        }
        for (index, route) in self.source.routes.iter().enumerate() {
            let id = &self.source.problem.connections[index].id;
            self.allowance
                .charge(WorkKind::ValidationRecords, 1)
                .map_err(stopped)?;
            if !expected.contains(id) && draft.routes[index] != *route {
                return Err(error(
                    "repair-unrelated-route",
                    id,
                    "screened repair changed an unrelated committed route",
                ));
            }
        }
        let expanded = self.replacements.iter().try_fold(0usize, |total, route| {
            total.checked_add(route.expanded).ok_or_else(|| {
                error(
                    "repair-expanded",
                    "repair",
                    "repair expansion count overflow",
                )
            })
        })?;
        let candidate = RepairCandidate {
            schema: REPAIR_CANDIDATE_SCHEMA.into(),
            state_revision: self.request.state_revision,
            removed: self.removed.clone(),
            expanded,
            replacements: std::mem::take(&mut self.replacements),
        };
        let publication_bytes = candidate
            .replacements
            .iter()
            .map(|route| route.cells.len().saturating_mul(128))
            .sum::<usize>()
            .saturating_add(4096);
        self.allowance
            .reserve_peak(publication_bytes)
            .map_err(stopped)?;
        self.reserved_bytes = self.reserved_bytes.saturating_add(publication_bytes);
        self.allowance.check_deadline().map_err(stopped)?;
        if let Some(last) = self.steps.last_mut() {
            last.terminal = draft.done();
            last.fully_validated = draft.metrics.is_some();
        }
        let steps = std::mem::take(&mut self.steps);
        Ok((
            self.draft.take().expect("repair draft is live"),
            candidate,
            steps,
        ))
    }
}

fn stopped(stop: AllowanceStop) -> Diagnostic {
    error(
        if stop.reason == "deadline" {
            "repair-deadline"
        } else {
            "repair-work-limit"
        },
        "repair",
        format!(
            "repair stopped at {:?}: used {}, limit {}",
            stop.category, stop.used, stop.limit
        ),
    )
}

fn sorted_unique(ids: &[String]) -> bool {
    !ids.is_empty() && ids.windows(2).all(|pair| pair[0] < pair[1])
}

fn candidate_reservation_bytes(candidate: &Candidate) -> usize {
    candidate
        .cells
        .len()
        .saturating_mul(128)
        .saturating_add(candidate.crossings.len().saturating_mul(512))
        .saturating_add(candidate.commands.len().saturating_mul(256))
        .saturating_add(2048)
}

impl Session {
    /// Price committed resources that obstruct the selected task. This is an
    /// ordering heuristic; only replay and independent validation admit a
    /// candidate. Present capacity checks use exact rates and bridge channels.
    /// The caller releases the returned reservation after the order is dropped.
    pub fn repair_conflict_order(
        &self,
        request: &RepairRequest,
        allowance: &mut RequestAllowance,
    ) -> Result<
        (
            Vec<(String, u64)>,
            BTreeMap<String, Vec<(TransportKind, Point)>>,
            usize,
        ),
        Diagnostic,
    > {
        self.validate_repair_request_inner(request, Some(allowance))?;
        self.repair_conflict_order_checked(request, allowance)
    }

    fn repair_conflict_order_checked(
        &self,
        request: &RepairRequest,
        allowance: &mut RequestAllowance,
    ) -> Result<
        (
            Vec<(String, u64)>,
            BTreeMap<String, Vec<(TransportKind, Point)>>,
            usize,
        ),
        Diagnostic,
    > {
        if allowance.caps() != &request.allowance_caps {
            return Err(error(
                "repair-allowance",
                "repair",
                "conflict ordering allowance differs from the trusted request",
            ));
        }
        let endpoint_capacity = request
            .selected_connections
            .iter()
            .filter_map(|id| self.connections.get(id).copied())
            .map(|index| {
                let connection = &self.problem.connections[index];
                [&connection.source, &connection.target]
                    .into_iter()
                    .map(|id| {
                        self.blocks.get(id).map_or(1, |&block| {
                            self.catalogs
                                .facilities
                                .facility(&self.problem.blocks[block].facility)
                                .map_or(1, |facility| facility.ports.len())
                        })
                    })
                    .sum::<usize>()
            })
            .sum::<usize>();
        let route_cells = request
            .allowed_removals
            .iter()
            .filter_map(|id| {
                self.connections
                    .get(id)
                    .and_then(|&index| self.routes[index].as_ref())
            })
            .map(|route| route.cells.len())
            .sum::<usize>();
        let reservation = endpoint_capacity
            .saturating_mul(128)
            .saturating_add(request.allowed_removals.len().saturating_mul(256))
            .saturating_add(route_cells.saturating_mul(96))
            .saturating_add(4096);
        allowance.reserve_peak(reservation).map_err(stopped)?;
        let result = (|| {
            let mut endpoints = HashSet::<(TransportKind, Point)>::new();
            let mut selected = Vec::new();
            for id in &request.selected_connections {
                allowance
                    .charge(WorkKind::ValidationRecords, 1)
                    .map_err(stopped)?;
                let index = self.connections[id];
                let connection = &self.problem.connections[index];
                selected.push(index);
                for (endpoint, direction) in [
                    (&connection.source, FacilityPortDirection::Output),
                    (&connection.target, FacilityPortDirection::Input),
                ] {
                    if let Some(&block) = self.blocks.get(endpoint) {
                        let pose = self.poses[block].as_ref().ok_or_else(|| {
                            error("repair-endpoint", endpoint, "selected facility is unplaced")
                        })?;
                        let facility = self
                            .catalogs
                            .facilities
                            .facility(&self.problem.blocks[block].facility)
                            .expect("admitted facility exists");
                        for port in &facility.ports {
                            allowance
                                .charge(WorkKind::EndpointVisits, 1)
                                .map_err(stopped)?;
                            if port.direction == direction && port.transport == connection.transport
                            {
                                endpoints.insert((
                                    connection.transport,
                                    project_port(facility, port, pose.rotation, pose.position)
                                        .connection,
                                ));
                            }
                        }
                    } else {
                        let &terminal = self.terminals.get(endpoint).ok_or_else(|| {
                            error("repair-endpoint", endpoint, "selected terminal is unknown")
                        })?;
                        let pose = self.terminal_poses[terminal].as_ref().ok_or_else(|| {
                            error("repair-endpoint", endpoint, "selected terminal is unplaced")
                        })?;
                        allowance
                            .charge(WorkKind::EndpointVisits, 1)
                            .map_err(stopped)?;
                        endpoints.insert((connection.transport, pose.position));
                    }
                }
            }
            let mut priced = Vec::with_capacity(request.allowed_removals.len());
            let mut conflict_cells = BTreeMap::new();
            for id in &request.allowed_removals {
                allowance
                    .charge(WorkKind::ValidationRecords, 1)
                    .map_err(stopped)?;
                let index = self.connections[id];
                let connection = &self.problem.connections[index];
                let route = self.routes[index]
                    .as_ref()
                    .expect("trusted committed route");
                let mut price = 0u64;
                let mut contested = Vec::new();
                for &point in &route.cells {
                    allowance.charge(WorkKind::RouteCells, 1).map_err(stopped)?;
                    let mut cell_price = if endpoints.contains(&(connection.transport, point)) {
                        1024u32
                    } else {
                        1u32
                    };
                    for &selected_index in &selected {
                        allowance
                            .charge(WorkKind::RoutingScans, 1)
                            .map_err(stopped)?;
                        let selected_connection = &self.problem.connections[selected_index];
                        if selected_connection.transport == connection.transport {
                            if !self.shared_cell_available_checked(point, selected_index, || {
                                allowance.charge(WorkKind::ReplayUsers, 1).map_err(stopped)
                            })? {
                                cell_price = cell_price.saturating_add(4);
                            }
                        }
                    }
                    if let Some(bridge) = self.bridges.get(&(connection.transport, point)) {
                        for channel in &bridge.channels {
                            let mut owned = false;
                            for owner in &channel.connections {
                                allowance
                                    .charge(WorkKind::ReplayUsers, 1)
                                    .map_err(stopped)?;
                                if owner == id {
                                    owned = true;
                                    break;
                                }
                            }
                            if !owned {
                                continue;
                            }
                            let mut load = Rate::zero();
                            for owner in &channel.connections {
                                allowance
                                    .charge(WorkKind::ReplayUsers, 1)
                                    .map_err(stopped)?;
                                load = load
                                    .checked_add(
                                        self.problem.connections[self.connections[owner]].rate,
                                    )
                                    .map_err(|_| {
                                        error(
                                            "repair-bridge-price",
                                            id,
                                            "bridge axis rate overflow",
                                        )
                                    })?;
                            }
                            let definition = self
                                .catalogs
                                .components
                                .component(&bridge.component)
                                .expect("committed bridge definition exists");
                            allowance
                                .charge(WorkKind::CatalogVisits, 1)
                                .map_err(stopped)?;
                            for &selected_index in &selected {
                                allowance
                                    .charge(WorkKind::RoutingScans, 1)
                                    .map_err(stopped)?;
                                let other = &self.problem.connections[selected_index];
                                if other.transport != connection.transport {
                                    continue;
                                }
                                let residual =
                                    load.checked_add(other.rate).ok().is_some_and(|sum| {
                                        within_capacity(
                                            sum,
                                            definition.capacity.quantity,
                                            definition.capacity.duration_ms,
                                        )
                                    });
                                if channel.commodity != other.commodity || !residual {
                                    cell_price = cell_price.saturating_add(16);
                                }
                            }
                        }
                    }
                    if cell_price > 1 {
                        contested.push((connection.transport, point));
                    }
                    price = price.saturating_add(u64::from(cell_price));
                }
                priced.push((std::cmp::Reverse(price), id.clone()));
                conflict_cells.insert(id.clone(), contested);
            }
            priced.sort();
            Ok((
                priced
                    .into_iter()
                    .map(|(price, id)| (id, price.0))
                    .collect(),
                conflict_cells,
            ))
        })();
        match result {
            Ok((order, conflict_cells)) => Ok((order, conflict_cells, reservation)),
            Err(failure) => {
                allowance.release_peak(reservation);
                Err(failure)
            }
        }
    }

    pub fn committed_repair_cells(&self, id: &str) -> Option<(TransportKind, &[Point])> {
        let &index = self.connections.get(id)?;
        let route = self.routes[index].as_ref()?;
        Some((self.problem.connections[index].transport, &route.cells))
    }

    /// Price currently occupied cells for a selected draft obligation. The
    /// draft already excludes the removed routes, so its prices do not
    /// penalize space that rip-up made free. The caller releases the returned
    /// map reservation after it has amended guidance costs.
    pub fn repair_live_cell_prices(
        &self,
        id: &str,
        allowance: &mut RequestAllowance,
    ) -> Result<(TransportKind, BTreeMap<Point, u32>, usize), Diagnostic> {
        let &index = self.connections.get(id).ok_or_else(|| {
            error(
                "repair-price",
                id,
                "unknown connection for current-state pricing",
            )
        })?;
        let connection = &self.problem.connections[index];
        let occupied = &self.occupied[layer(connection.transport)];
        let reserved = occupied.len().saturating_mul(128).saturating_add(4096);
        allowance.reserve_peak(reserved).map_err(stopped)?;
        let result = (|| {
            let mut prices = BTreeMap::new();
            for (&point, _) in occupied {
                allowance
                    .charge(WorkKind::RoutingScans, 1)
                    .map_err(stopped)?;
                let mut price = if self.shared_cell_available_checked(point, index, || {
                    allowance.charge(WorkKind::ReplayUsers, 1).map_err(stopped)
                })? {
                    1u32
                } else {
                    4u32
                };
                if let Some(bridge) = self.bridges.get(&(connection.transport, point)) {
                    allowance
                        .charge(WorkKind::CatalogVisits, 1)
                        .map_err(stopped)?;
                    let component = self
                        .catalogs
                        .components
                        .component(&bridge.component)
                        .expect("committed bridge definition exists");
                    let mut compatible = false;
                    for channel in &bridge.channels {
                        let mut load = Rate::zero();
                        for owner in &channel.connections {
                            allowance
                                .charge(WorkKind::ReplayUsers, 1)
                                .map_err(stopped)?;
                            let other = &self.problem.connections[self.connections[owner]];
                            load = load.checked_add(other.rate).map_err(|_| {
                                error("repair-price", id, "bridge axis rate overflow")
                            })?;
                        }
                        if channel.commodity == connection.commodity
                            && load.checked_add(connection.rate).ok().is_some_and(|sum| {
                                within_capacity(
                                    sum,
                                    component.capacity.quantity,
                                    component.capacity.duration_ms,
                                )
                            })
                        {
                            compatible = true;
                        }
                    }
                    price = price.saturating_add(if compatible { 2 } else { 16 });
                }
                prices.insert(point, price);
            }
            Ok((connection.transport, prices, reserved))
        })();
        if result.is_err() {
            allowance.release_peak(reserved);
        }
        result
    }

    pub fn start_repair_draft<'a>(
        &'a self,
        request: &'a RepairRequest,
        removed: &[String],
        allowance: &'a mut RequestAllowance,
    ) -> Result<RepairDraft<'a>, Diagnostic> {
        self.validate_repair_request_inner(request, Some(allowance))?;
        self.start_repair_draft_validated(request, removed, allowance)
    }

    fn start_repair_draft_validated<'a>(
        &'a self,
        request: &'a RepairRequest,
        removed: &[String],
        allowance: &'a mut RequestAllowance,
    ) -> Result<RepairDraft<'a>, Diagnostic> {
        self.validate_repair_removals(request, removed)?;
        let (draft, reserved_bytes) =
            self.prepare_repair_draft_checked(request, removed, allowance)?;
        let progress_total = draft.poses.len() + draft.terminal_poses.len() + draft.routes.len();
        if let Err(stop) = allowance.charge(WorkKind::ValidationRecords, progress_total) {
            allowance.release_peak(reserved_bytes);
            return Err(stopped(stop));
        }
        let progress_placed = draft.poses.iter().filter(|pose| pose.is_some()).count()
            + draft
                .terminal_poses
                .iter()
                .filter(|pose| pose.is_some())
                .count();
        let progress_committed = draft.routes.iter().filter(|route| route.is_some()).count();
        Ok(RepairDraft {
            source: self,
            request,
            removed: removed.to_vec(),
            draft: Some(draft),
            allowance,
            reserved_bytes,
            retained_bytes: 0,
            replacements: Vec::new(),
            steps: Vec::new(),
            progress_placed,
            progress_committed,
            progress_total,
        })
    }

    /// Hash the complete stable state, including immutable problem, catalog and
    /// command limits. This is deliberately computed only at a repair boundary.
    pub fn repair_state_sha256(&self) -> Result<String, Diagnostic> {
        self.repair_state_sha256_inner(None)
    }

    fn repair_state_sha256_inner(
        &self,
        mut allowance: Option<&mut RequestAllowance>,
    ) -> Result<String, Diagnostic> {
        let component_count = self.junctions.len().saturating_add(self.bridges.len());
        let workspace_bytes = component_count.saturating_mul(32).saturating_add(4096);
        if let Some(allowance) = allowance.as_deref_mut() {
            allowance.reserve_peak(workspace_bytes).map_err(stopped)?;
        }
        let result = (|| {
            if let Some(allowance) = allowance.as_deref_mut() {
                allowance
                    .charge(
                        WorkKind::ValidationRecords,
                        self.problem
                            .blocks
                            .len()
                            .saturating_add(self.problem.terminals.len())
                            .saturating_add(self.problem.connections.len())
                            .saturating_add(component_count),
                    )
                    .map_err(stopped)?;
            }
            let mut junctions = self.junctions.values().collect::<Vec<_>>();
            junctions.sort_by(|a, b| a.id.cmp(&b.id));
            let mut bridges = self.bridges.values().collect::<Vec<_>>();
            bridges.sort_by(|a, b| a.id.cmp(&b.id));
            let observation = BorrowedRepairObservation {
                revision: self.revision,
                turns: self.turns,
                max_turns: self.limits.max_turns,
                poses: &self.poses,
                terminal_poses: &self.terminal_poses,
                routes: &self.routes,
                junctions,
                bridges,
                active: &self.active,
                focus: &self.focus,
                completed: self.completed(),
                total: self.routes.len(),
                completion_ratio: self.progress(),
                fully_validated: self.metrics.is_some(),
            };
            let mut writer = RepairHashWriter {
                digest: Sha256::new(),
                allowance: allowance.as_deref_mut(),
                pending_bytes: 0,
                stop: None,
            };
            let encoded = serde_json::to_writer(
                &mut writer,
                &(
                    &self.problem,
                    &self.catalogs.identity,
                    &self.limits,
                    &self.final_failure,
                    observation,
                ),
            );
            if let Err(failure) = encoded {
                if let Some(stop) = writer.stop.take() {
                    return Err(stopped(stop));
                }
                return Err(error(
                    "repair-state-identity",
                    "repair",
                    format!("cannot encode exact native state: {failure}"),
                ));
            }
            if let Some(allowance) = writer.allowance.as_deref_mut() {
                allowance
                    .record_completed(WorkKind::CopiedBytes, writer.pending_bytes)
                    .map_err(stopped)?;
            }
            Ok(std::mem::take(&mut writer.digest)
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect())
        })();
        if let Some(allowance) = allowance.as_deref_mut() {
            allowance.release_peak(workspace_bytes);
        }
        result
    }

    /// Build the trusted scope from the current committed state and caller's
    /// selected task, removal policy and finite route allowance.
    pub fn repair_request(
        &self,
        selected_connections: &[String],
        allowed_removals: &[String],
        max_removed: usize,
        config: &Config,
    ) -> Result<RepairRequest, Diagnostic> {
        self.repair_request_inner(
            selected_connections,
            allowed_removals,
            max_removed,
            config,
            None,
        )
    }

    fn repair_request_inner(
        &self,
        selected_connections: &[String],
        allowed_removals: &[String],
        max_removed: usize,
        config: &Config,
        allowance: Option<&mut RequestAllowance>,
    ) -> Result<RepairRequest, Diagnostic> {
        config.validate()?;
        if self.has_active_route() || self.done() {
            return Err(error(
                "repair-state",
                "repair",
                "repair requires an unfinished stable native state",
            ));
        }
        let selected = selected_connections
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let allowed = allowed_removals.iter().cloned().collect::<BTreeSet<_>>();
        if selected.is_empty()
            || allowed.is_empty()
            || selected.len() != selected_connections.len()
            || allowed.len() != allowed_removals.len()
            || max_removed == 0
            || max_removed > allowed.len()
        {
            return Err(error(
                "repair-scope",
                "repair",
                "selected task and permitted removals must be nonempty, distinct and bounded",
            ));
        }
        for id in &selected {
            let Some(&index) = self.connections.get(id) else {
                return Err(error("repair-selected", id, "unknown selected obligation"));
            };
            if self.routes[index].is_some() {
                return Err(error(
                    "repair-selected",
                    id,
                    "selected obligation is already complete",
                ));
            }
        }
        for id in &allowed {
            let Some(&index) = self.connections.get(id) else {
                return Err(error("repair-scope", id, "unknown permitted removal"));
            };
            if self.routes[index].is_none() || selected.contains(id) {
                return Err(error(
                    "repair-scope",
                    id,
                    "permitted removal must be a distinct committed route",
                ));
            }
        }
        Ok(RepairRequest {
            schema: REPAIR_REQUEST_SCHEMA.into(),
            state_revision: self.revision,
            state_sha256: self.repair_state_sha256_inner(allowance)?,
            selected_connections: selected.into_iter().collect(),
            allowed_removals: allowed.into_iter().collect(),
            max_removed,
            allowance_caps: config.request_allowance.clone(),
        })
    }

    /// Resolve a configured repair policy from the live state. The candidate
    /// and any serialized action record are deliberately absent from this API.
    pub fn repair_request_for_policy(
        &self,
        selected_connections: &[String],
        max_removed: usize,
        config: &Config,
    ) -> Result<RepairRequest, Diagnostic> {
        let allowed = self
            .problem
            .connections
            .iter()
            .zip(&self.routes)
            .filter_map(|(connection, route)| route.as_ref().map(|_| connection.id.clone()))
            .collect::<Vec<_>>();
        self.repair_request(
            selected_connections,
            &allowed,
            max_removed.min(allowed.len()),
            config,
        )
    }

    /// Construct a trusted, budgeted scope once for all local neighborhoods
    /// of one immutable macro state.
    pub fn repair_scope_for_policy(
        &self,
        selected_connections: &[String],
        max_removed: usize,
        config: &Config,
        allowance: &mut RequestAllowance,
    ) -> Result<(RepairScope<'_>, usize), Diagnostic> {
        let scope_bytes = self
            .problem
            .connections
            .len()
            .saturating_mul(256)
            .saturating_add(4096);
        allowance.reserve_peak(scope_bytes).map_err(stopped)?;
        let result = (|| {
            allowance
                .charge(WorkKind::ValidationRecords, self.problem.connections.len())
                .map_err(stopped)?;
            let allowed = self
                .problem
                .connections
                .iter()
                .zip(&self.routes)
                .filter_map(|(connection, route)| route.as_ref().map(|_| connection.id.clone()))
                .collect::<Vec<_>>();
            let request = self.repair_request_inner(
                selected_connections,
                &allowed,
                max_removed.min(allowed.len()),
                config,
                Some(allowance),
            )?;
            Ok((
                RepairScope {
                    source: self,
                    request,
                },
                scope_bytes,
            ))
        })();
        if result.is_err() {
            allowance.release_peak(scope_bytes);
        }
        result
    }

    fn validate_repair_request_inner(
        &self,
        request: &RepairRequest,
        allowance: Option<&mut RequestAllowance>,
    ) -> Result<(), Diagnostic> {
        if request.schema != REPAIR_REQUEST_SCHEMA
            || request.state_revision != self.revision
            || request.state_sha256 != self.repair_state_sha256_inner(allowance)?
            || self.has_active_route()
            || self.done()
        {
            return Err(error(
                "repair-stale-request",
                "repair",
                "repair request does not match this exact unfinished stable state",
            ));
        }
        if !sorted_unique(&request.selected_connections)
            || !sorted_unique(&request.allowed_removals)
            || request.max_removed == 0
            || request.max_removed > request.allowed_removals.len()
        {
            return Err(error(
                "repair-scope",
                "repair",
                "repair request has malformed selected or permitted scope",
            ));
        }
        for id in &request.selected_connections {
            let Some(&index) = self.connections.get(id) else {
                return Err(error("repair-selected", id, "unknown selected obligation"));
            };
            if self.routes[index].is_some() {
                return Err(error("repair-selected", id, "selected route is complete"));
            }
        }
        for id in &request.allowed_removals {
            let Some(&index) = self.connections.get(id) else {
                return Err(error("repair-scope", id, "unknown permitted route"));
            };
            if self.routes[index].is_none()
                || request.selected_connections.binary_search(id).is_ok()
            {
                return Err(error(
                    "repair-scope",
                    id,
                    "permitted route is not committed",
                ));
            }
        }
        Ok(())
    }

    fn validate_repair_removals(
        &self,
        request: &RepairRequest,
        removed: &[String],
    ) -> Result<(), Diagnostic> {
        if !sorted_unique(removed) || removed.len() > request.max_removed {
            return Err(error(
                "repair-removals",
                "repair",
                "removed routes must be a nonempty sorted set within the trusted limit",
            ));
        }
        for id in removed {
            if request.allowed_removals.binary_search(id).is_err() {
                return Err(error(
                    "repair-removals",
                    id,
                    "candidate removal is outside the trusted scope",
                ));
            }
        }
        Ok(())
    }

    /// Return a lower-tool draft with a live clone reservation. The caller must
    /// release `reserved_bytes` on its shared allowance when done with the draft.
    pub fn prepare_repair_draft_with_allowance(
        &self,
        request: &RepairRequest,
        removed: &[String],
        allowance: &mut RequestAllowance,
    ) -> Result<(Session, usize), Diagnostic> {
        self.validate_repair_request_inner(request, Some(allowance))?;
        self.validate_repair_removals(request, removed)?;
        self.prepare_repair_draft_checked(request, removed, allowance)
    }

    fn prepare_repair_draft_checked(
        &self,
        request: &RepairRequest,
        removed: &[String],
        allowance: &mut RequestAllowance,
    ) -> Result<(Session, usize), Diagnostic> {
        if allowance.caps() != &request.allowance_caps {
            return Err(error(
                "repair-allowance",
                "repair",
                "draft allowance differs from the trusted request",
            ));
        }
        let source_bytes = self.clone_reservation_bytes();
        let rebuild_bytes = self
            .routes
            .iter()
            .flatten()
            .map(|route| route.cells.len())
            .sum::<usize>()
            .saturating_mul(256)
            .saturating_add(self.junctions.len().saturating_mul(1024))
            .saturating_add(self.bridges.len().saturating_mul(2048))
            .saturating_add(self.used_ports.len().saturating_mul(512))
            .saturating_add(4096);
        // The source, draft, and old/new derived maps coexist during rebuild.
        let reserved_bytes = source_bytes.saturating_mul(2).saturating_add(rebuild_bytes);
        allowance.reserve_peak(reserved_bytes).map_err(stopped)?;
        let result =
            (|| {
                allowance
                    .charge(
                        WorkKind::CopiedBytes,
                        source_bytes.saturating_add(rebuild_bytes),
                    )
                    .map_err(stopped)?;
                let mut draft = self.clone();
                for id in removed {
                    allowance
                        .charge(WorkKind::ValidationRecords, 1)
                        .map_err(stopped)?;
                    let index = draft.connections[id];
                    draft.routes[index] = None;
                }
                draft.active = None;
                draft.active_cells.clear();
                draft.focus = None;
                draft.metrics = None;
                draft.final_failure = None;
                draft.turns = draft.turns.checked_add(1).ok_or_else(|| {
                    error("repair-turns", "repair", "repair turn counter overflow")
                })?;
                draft.revision = draft.revision.checked_add(1).ok_or_else(|| {
                    error("repair-revision", "repair", "repair revision overflow")
                })?;
                draft.rebuild_after_route_removal(allowance)?;
                Ok(draft)
            })();
        if result.is_err() {
            allowance.release_peak(reserved_bytes);
        }
        result.and_then(|draft| {
            let live_bytes = source_bytes.saturating_add(draft.clone_reservation_bytes());
            if live_bytes > reserved_bytes {
                if let Err(stop) = allowance.reserve_peak(live_bytes - reserved_bytes) {
                    allowance.release_peak(reserved_bytes);
                    return Err(stopped(stop));
                }
            } else {
                allowance.release_peak(reserved_bytes - live_bytes);
            }
            Ok((draft, live_bytes))
        })
    }

    /// The draft only mutates routes. Check the native indexes and component
    /// ownership of every committed route without manufacturing a complete
    /// problem from an incomplete one. Native Connect and survivor rebuild
    /// already enforce geometry, topology and exact component capacity.
    pub(in crate::constructive) fn validate_incomplete_repair_state(
        &self,
        source: &Session,
        allowance: &mut RequestAllowance,
    ) -> Result<(), Diagnostic> {
        let route_cells = self
            .routes
            .iter()
            .flatten()
            .map(|route| route.cells.len())
            .sum::<usize>();
        let workspace_bytes = route_cells
            .saturating_mul(256)
            .saturating_add(self.used_ports.len().saturating_mul(512))
            .saturating_add(4096);
        allowance.reserve_peak(workspace_bytes).map_err(stopped)?;
        let result = (|| {
            allowance
                .charge(WorkKind::CopiedBytes, workspace_bytes)
                .map_err(stopped)?;
            allowance
                .charge(
                    WorkKind::ValidationRecords,
                    self.poses
                        .len()
                        .saturating_add(self.terminal_poses.len())
                        .saturating_add(self.bodies.len())
                        .saturating_add(self.terminal_at.len()),
                )
                .map_err(stopped)?;
            if self.poses != source.poses
                || self.terminal_poses != source.terminal_poses
                || self.bodies != source.bodies
                || self.terminal_at != source.terminal_at
            {
                return Err(error(
                    "repair-stable-state",
                    "placement",
                    "repair changed a placed body or terminal",
                ));
            }
            let mut occupied: [HashMap<Point, Vec<usize>>; 2] =
                std::array::from_fn(|_| HashMap::new());
            let mut visits: HashMap<(TransportKind, Point), Vec<(usize, usize)>> = HashMap::new();
            let mut ports: HashMap<(usize, String), UsedPort> = HashMap::new();
            let mut port_points: HashMap<(usize, String), Point> = HashMap::new();
            for (index, route) in self.routes.iter().enumerate() {
                allowance
                    .charge(WorkKind::ValidationRecords, 1)
                    .map_err(stopped)?;
                let Some(route) = route else { continue };
                let obligation = &self.problem.connections[index];
                if route.connection != obligation.id {
                    return Err(error(
                        "repair-stable-state",
                        &route.connection,
                        "route ID differs from its obligation",
                    ));
                }
                let capacity = self.catalogs.transports.capacity(obligation.transport);
                if route.cells.is_empty()
                    || route.cells.len() > 250_000
                    || !within_capacity(obligation.rate, capacity.quantity, capacity.duration_ms)
                {
                    return Err(error(
                        "repair-stable-state",
                        &route.connection,
                        "route length or line rate is invalid",
                    ));
                }
                for (endpoint, port, direction, cell) in [
                    (
                        &obligation.source,
                        route.source_port.as_ref(),
                        FacilityPortDirection::Output,
                        route.cells[0],
                    ),
                    (
                        &obligation.target,
                        route.target_port.as_ref(),
                        FacilityPortDirection::Input,
                        *route.cells.last().unwrap(),
                    ),
                ] {
                    allowance
                        .charge(WorkKind::ValidationRecords, 1)
                        .map_err(stopped)?;
                    if let Some(&block) = self.blocks.get(endpoint) {
                        let Some(port) = port else {
                            return Err(error(
                                "repair-stable-state",
                                &route.connection,
                                "facility route port is missing",
                            ));
                        };
                        let pose = self.poses[block].as_ref().ok_or_else(|| {
                            error(
                                "repair-stable-state",
                                &route.connection,
                                "facility endpoint is unplaced",
                            )
                        })?;
                        let definition = self
                            .catalogs
                            .facilities
                            .facility(&self.problem.blocks[block].facility)
                            .expect("admitted facility has a catalog definition");
                        let mut selected = None;
                        for definition_port in &definition.ports {
                            allowance
                                .charge(WorkKind::CatalogVisits, 1)
                                .map_err(stopped)?;
                            let candidate = project_port(
                                definition,
                                definition_port,
                                pose.rotation,
                                pose.position,
                            );
                            if candidate.id == *port
                                && candidate.direction == direction
                                && candidate.transport == obligation.transport
                                && candidate.connection == cell
                            {
                                selected = Some(candidate.position);
                            }
                        }
                        let Some(position) = selected else {
                            return Err(error(
                                "repair-stable-state",
                                &route.connection,
                                "route endpoint does not match its facility port",
                            ));
                        };
                        if port_points
                            .insert((block, port.clone()), position)
                            .is_some_and(|old| old != position)
                        {
                            return Err(error(
                                "repair-stable-state",
                                &route.connection,
                                "shared port projects to inconsistent positions",
                            ));
                        }
                    } else {
                        let Some(&terminal) = self.terminals.get(endpoint) else {
                            return Err(error(
                                "repair-stable-state",
                                &route.connection,
                                "route endpoint is unknown",
                            ));
                        };
                        if port.is_some()
                            || self.terminal_poses[terminal]
                                .as_ref()
                                .map(|pose| pose.position)
                                != Some(cell)
                        {
                            return Err(error(
                                "repair-stable-state",
                                &route.connection,
                                "route misses its boundary terminal",
                            ));
                        }
                    }
                }
                let mut local = HashSet::new();
                for (pi, &cell) in route.cells.iter().enumerate() {
                    allowance
                        .charge(WorkKind::ValidationRecords, 1)
                        .map_err(stopped)?;
                    if !(0..=1_000_000).contains(&cell.x)
                        || !(0..=1_000_000).contains(&cell.y)
                        || self.bodies.contains(&cell)
                        || !local.insert(cell)
                        || pi > 0 && direction(route.cells[pi - 1], cell).is_none()
                        || self.terminal_at.get(&cell).is_some_and(|&terminal| {
                            let id = &self.problem.terminals[terminal].id;
                            id != &obligation.source && id != &obligation.target
                        })
                    {
                        return Err(error(
                            "repair-stable-state",
                            &route.connection,
                            "route path crosses an obstacle, repeats, skips, or leaves bounds",
                        ));
                    }
                    occupied[layer(obligation.transport)]
                        .entry(cell)
                        .or_default()
                        .push(index);
                    visits
                        .entry((obligation.transport, cell))
                        .or_default()
                        .push((index, pi));
                }
                for (endpoint, port, direction) in [
                    (
                        &obligation.source,
                        route.source_port.as_ref(),
                        FacilityPortDirection::Output,
                    ),
                    (
                        &obligation.target,
                        route.target_port.as_ref(),
                        FacilityPortDirection::Input,
                    ),
                ] {
                    allowance
                        .charge(WorkKind::ValidationRecords, 1)
                        .map_err(stopped)?;
                    let (Some(&block), Some(port)) = (self.blocks.get(endpoint), port) else {
                        continue;
                    };
                    let entry = ports
                        .entry((block, port.clone()))
                        .or_insert_with(|| UsedPort {
                            commodity: obligation.commodity.clone(),
                            transport: obligation.transport,
                            direction,
                            rate: Rate::zero(),
                        });
                    if entry.commodity != obligation.commodity
                        || entry.transport != obligation.transport
                        || entry.direction != direction
                    {
                        return Err(error(
                            "repair-stable-state",
                            &obligation.id,
                            "port owners disagree",
                        ));
                    }
                    entry.rate = entry.rate.checked_add(obligation.rate).map_err(|_| {
                        error("repair-stable-state", &obligation.id, "port rate overflow")
                    })?;
                    if !within_capacity(entry.rate, capacity.quantity, capacity.duration_ms) {
                        return Err(error(
                            "repair-stable-state",
                            &obligation.id,
                            "aggregate facility port rate exceeds line capacity",
                        ));
                    }
                }
            }
            for transport in 0..2 {
                allowance
                    .charge(
                        WorkKind::ValidationRecords,
                        occupied[transport]
                            .len()
                            .saturating_add(self.occupied[transport].len()),
                    )
                    .map_err(stopped)?;
                if occupied[transport].len() != self.occupied[transport].len() {
                    return Err(error(
                        "repair-stable-state",
                        "occupancy",
                        "route occupancy index has a different site count",
                    ));
                }
                for (point, users) in &occupied[transport] {
                    allowance
                        .charge(WorkKind::ValidationRecords, users.len().saturating_mul(2))
                        .map_err(stopped)?;
                    let Some(actual) = self.occupied[transport].get(point) else {
                        return Err(error(
                            "repair-stable-state",
                            "occupancy",
                            "committed route cell is missing from occupancy",
                        ));
                    };
                    let mut expected_owners = users.clone();
                    let mut actual_owners = actual.clone();
                    expected_owners.sort_unstable();
                    actual_owners.sort_unstable();
                    if actual_owners != expected_owners {
                        return Err(error(
                            "repair-stable-state",
                            "occupancy",
                            "route occupancy owners disagree",
                        ));
                    }
                }
            }
            allowance
                .charge(
                    WorkKind::ValidationRecords,
                    ports.len().saturating_add(self.used_ports.len()),
                )
                .map_err(stopped)?;
            if ports.len() != self.used_ports.len() {
                return Err(error(
                    "repair-stable-state",
                    "ports",
                    "used port count differs from committed routes",
                ));
            }
            for (key, expected) in &ports {
                allowance
                    .charge(WorkKind::ValidationRecords, 1)
                    .map_err(stopped)?;
                let Some(actual) = self.used_ports.get(key) else {
                    return Err(error(
                        "repair-stable-state",
                        "ports",
                        "committed route port is missing",
                    ));
                };
                if actual.commodity != expected.commodity
                    || actual.transport != expected.transport
                    || actual.direction != expected.direction
                    || actual.rate != expected.rate
                {
                    return Err(error(
                        "repair-stable-state",
                        "ports",
                        "committed port rate or identity disagrees",
                    ));
                }
            }
            self.validate_repair_bridges(&visits, allowance)?;
            self.validate_repair_junctions(&visits, &port_points, allowance)?;
            Ok(())
        })();
        allowance.release_peak(workspace_bytes);
        result
    }

    fn validate_repair_bridges(
        &self,
        visits: &HashMap<(TransportKind, Point), Vec<(usize, usize)>>,
        allowance: &mut RequestAllowance,
    ) -> Result<(), Diagnostic> {
        let mut ids = HashSet::new();
        for (&(transport, point), bridge) in &self.bridges {
            allowance
                .charge(WorkKind::ValidationRecords, 1)
                .map_err(stopped)?;
            let Some(definition) = self.catalogs.components.component(&bridge.component) else {
                return Err(error(
                    "repair-stable-state",
                    &bridge.id,
                    "unknown bridge component",
                ));
            };
            let Some(users) = visits.get(&(transport, point)) else {
                return Err(error(
                    "repair-stable-state",
                    &bridge.id,
                    "bridge has no committed route",
                ));
            };
            if !aic_layout::stable_id::is_stable_id(&bridge.id)
                || !ids.insert(&bridge.id)
                || bridge.position != point
                || definition.transport != transport
                || definition.kind != LogisticsComponentKind::Bridge
                || definition.footprint.width != 1
                || definition.footprint.height != 1
                || !definition.allowed_rotations.contains(&bridge.rotation)
                || bridge.channels[0].axis != BridgeAxis::Horizontal
                || bridge.channels[1].axis != BridgeAxis::Vertical
                || self.bodies.contains(&point)
                || self.terminal_at.contains_key(&point)
                || self.junctions.contains_key(&(transport, point))
            {
                return Err(error(
                    "repair-stable-state",
                    &bridge.id,
                    "bridge geometry, component, or axes are invalid",
                ));
            }
            let mut actual = [BTreeSet::new(), BTreeSet::new()];
            let mut load = [Rate::zero(), Rate::zero()];
            let mut directions = [None, None];
            for &(ci, pi) in users {
                allowance
                    .charge(WorkKind::ValidationRecords, 1)
                    .map_err(stopped)?;
                let route = self.routes[ci]
                    .as_ref()
                    .expect("visited route is committed");
                if pi == 0 || pi + 1 == route.cells.len() {
                    return Err(error(
                        "repair-stable-state",
                        &bridge.id,
                        "bridge terminates a route",
                    ));
                }
                let Some(heading) = direction(route.cells[pi - 1], point) else {
                    return Err(error(
                        "repair-stable-state",
                        &bridge.id,
                        "bridge route has no incoming arm",
                    ));
                };
                let axis = heading % 2;
                let obligation = &self.problem.connections[ci];
                let channel = &bridge.channels[axis];
                let rotated = |arm| (cardinal_index(arm) + bridge.rotation as usize / 90) % 4;
                if direction(point, route.cells[pi + 1]) != Some(heading)
                    || channel.commodity != obligation.commodity
                    || directions[axis].is_some_and(|previous| previous != heading)
                    || !definition
                        .input_directions
                        .iter()
                        .any(|&arm| rotated(arm) == (heading + 2) % 4)
                    || !definition
                        .output_directions
                        .iter()
                        .any(|&arm| rotated(arm) == heading)
                {
                    return Err(error(
                        "repair-stable-state",
                        &bridge.id,
                        "bridge arm, commodity, or flow direction is invalid",
                    ));
                }
                directions[axis] = Some(heading);
                actual[axis].insert(obligation.id.as_str());
                load[axis] = load[axis].checked_add(obligation.rate).map_err(|_| {
                    error(
                        "repair-stable-state",
                        &bridge.id,
                        "bridge axis rate overflow",
                    )
                })?;
            }
            for axis in 0..2 {
                let channel = &bridge.channels[axis];
                allowance
                    .charge(WorkKind::ValidationRecords, channel.connections.len())
                    .map_err(stopped)?;
                let named = channel
                    .connections
                    .iter()
                    .map(String::as_str)
                    .collect::<BTreeSet<_>>();
                let line = self.catalogs.transports.capacity(transport);
                if actual[axis].is_empty()
                    || named.len() != channel.connections.len()
                    || named != actual[axis]
                    || !within_capacity(
                        load[axis],
                        definition.capacity.quantity,
                        definition.capacity.duration_ms,
                    )
                    || !within_capacity(load[axis], line.quantity, line.duration_ms)
                {
                    return Err(error(
                        "repair-stable-state",
                        &bridge.id,
                        "bridge axis ownership or capacity is invalid",
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_repair_junctions(
        &self,
        visits: &HashMap<(TransportKind, Point), Vec<(usize, usize)>>,
        port_points: &HashMap<(usize, String), Point>,
        allowance: &mut RequestAllowance,
    ) -> Result<(), Diagnostic> {
        let mut ids = HashSet::new();
        for (&(transport, point), junction) in &self.junctions {
            allowance
                .charge(WorkKind::ValidationRecords, 1)
                .map_err(stopped)?;
            let Some(definition) = self.catalogs.components.component(&junction.component) else {
                return Err(error(
                    "repair-stable-state",
                    &junction.id,
                    "unknown junction component",
                ));
            };
            if !aic_layout::stable_id::is_stable_id(&junction.id)
                || !ids.insert(&junction.id)
                || junction.position != point
                || definition.transport != transport
                || !matches!(
                    definition.kind,
                    LogisticsComponentKind::Splitter | LogisticsComponentKind::Converger
                )
                || definition.footprint.width != 1
                || definition.footprint.height != 1
                || !definition.allowed_rotations.contains(&junction.rotation)
                || self.bodies.contains(&point)
                || self.terminal_at.contains_key(&point)
                || self.bridges.contains_key(&(transport, point))
                || !visits.contains_key(&(transport, point))
            {
                return Err(error(
                    "repair-stable-state",
                    &junction.id,
                    "junction geometry or component is invalid",
                ));
            }
        }
        for (&(transport, point), users) in visits {
            allowance
                .charge(WorkKind::ValidationRecords, 1)
                .map_err(stopped)?;
            if self.bridges.contains_key(&(transport, point)) {
                continue;
            }
            let mut uses = Vec::with_capacity(users.len());
            for &(ci, pi) in users {
                allowance
                    .charge(WorkKind::ValidationRecords, 1)
                    .map_err(stopped)?;
                let route = self.routes[ci]
                    .as_ref()
                    .expect("visited route is committed");
                uses.push(self.repair_site_arm_use(ci, route, pi, port_points)?);
            }
            let mut interrupted = None;
            let required =
                junction_options_with_visit(transport, point, &uses, &self.catalogs, || {
                    match allowance.charge(WorkKind::CatalogVisits, 1) {
                        Ok(()) => true,
                        Err(stop) => {
                            interrupted = Some(stop);
                            false
                        }
                    }
                });
            if let Some(stop) = interrupted {
                return Err(stopped(stop));
            }
            let required = required.map_err(|failure| {
                error(
                    "repair-stable-state",
                    failure.entity.to_string(),
                    failure.message,
                )
            })?;
            let selected = self.junctions.get(&(transport, point));
            match (required, selected) {
                (None, None) => {}
                (Some(options), Some(junction)) => {
                    allowance
                        .charge(WorkKind::ValidationRecords, junction.connections.len())
                        .map_err(stopped)?;
                    let named = junction
                        .connections
                        .iter()
                        .map(String::as_str)
                        .collect::<BTreeSet<_>>();
                    let actual = users
                        .iter()
                        .map(|&(ci, _)| self.problem.connections[ci].id.as_str())
                        .collect::<BTreeSet<_>>();
                    if named.len() != junction.connections.len()
                        || named != actual
                        || !options.contains(&(junction.component.clone(), junction.rotation))
                    {
                        return Err(error(
                            "repair-stable-state",
                            &junction.id,
                            "junction arms, rate, rotation, or owners are invalid",
                        ));
                    }
                }
                (Some(_), None) => {
                    return Err(error(
                        "repair-stable-state",
                        "junction",
                        "committed branch or merge lacks a junction",
                    ));
                }
                (None, Some(junction)) => {
                    return Err(error(
                        "repair-stable-state",
                        &junction.id,
                        "unneeded junction occupies a route",
                    ));
                }
            }
        }
        Ok(())
    }

    fn repair_site_arm_use<'a>(
        &'a self,
        connection_index: usize,
        route: &'a Route,
        path_index: usize,
        port_points: &HashMap<(usize, String), Point>,
    ) -> Result<SiteArmUse<'a>, Diagnostic> {
        let connection = &self.problem.connections[connection_index];
        let point = route.cells[path_index];
        let mut external_incoming = false;
        let mut external_outgoing = false;
        let incoming = if path_index > 0 {
            direction(point, route.cells[path_index - 1])
        } else if let Some(&block) = self.blocks.get(&connection.source) {
            let port = route.source_port.as_ref().ok_or_else(|| {
                error(
                    "repair-stable-state",
                    &connection.id,
                    "source route port is missing",
                )
            })?;
            let position = port_points.get(&(block, port.clone())).ok_or_else(|| {
                error(
                    "repair-stable-state",
                    &connection.id,
                    "source route port is not indexed",
                )
            })?;
            direction(point, *position)
        } else {
            external_incoming = true;
            None
        };
        let outgoing = if path_index + 1 < route.cells.len() {
            direction(point, route.cells[path_index + 1])
        } else if let Some(&block) = self.blocks.get(&connection.target) {
            let port = route.target_port.as_ref().ok_or_else(|| {
                error(
                    "repair-stable-state",
                    &connection.id,
                    "target route port is missing",
                )
            })?;
            let position = port_points.get(&(block, port.clone())).ok_or_else(|| {
                error(
                    "repair-stable-state",
                    &connection.id,
                    "target route port is not indexed",
                )
            })?;
            direction(point, *position)
        } else {
            external_outgoing = true;
            None
        };
        if incoming.is_none() && !external_incoming || outgoing.is_none() && !external_outgoing {
            return Err(error(
                "repair-stable-state",
                &connection.id,
                "route endpoint arm is not adjacent",
            ));
        }
        Ok(SiteArmUse {
            connection: &connection.id,
            commodity: &connection.commodity,
            rate: connection.rate,
            incoming,
            outgoing,
            external_incoming,
            external_outgoing,
        })
    }

    fn validate_complete_repair_layout(
        &self,
        allowance: &mut RequestAllowance,
    ) -> Result<Metrics, Diagnostic> {
        let layout_bytes = self.clone_reservation_bytes();
        allowance.reserve_peak(layout_bytes).map_err(stopped)?;
        let result = (|| {
            allowance
                .charge(WorkKind::CopiedBytes, layout_bytes)
                .map_err(stopped)?;
            let layout = self.complete_layout();
            let mut interrupted = None;
            let validation = validate_with_visit(&layout, &self.catalogs, || {
                match allowance.charge(WorkKind::ValidationRecords, 1) {
                    Ok(()) => Ok(()),
                    Err(stop) => {
                        interrupted = Some(stop);
                        Err(aic_layout::validation::error(
                            "request-work-limit",
                            "repair-validation",
                            "repair validation allowance stopped",
                        ))
                    }
                }
            });
            if let Some(stop) = interrupted {
                return Err(stopped(stop));
            }
            validation.map_err(|failure| {
                error(
                    "repair-final-validation",
                    failure.entity.to_string(),
                    failure.message,
                )
            })
        })();
        allowance.release_peak(layout_bytes);
        result
    }

    fn replay_repair_member_in_place(
        &mut self,
        candidate: &Candidate,
        allowance: &mut RequestAllowance,
        placed: usize,
        committed: &mut usize,
        total: usize,
    ) -> Result<Vec<Step>, Diagnostic> {
        if self.revision != candidate.state_revision {
            return Err(error(
                "repair-revision",
                &candidate.connection,
                "replacement revision does not match the evolving repair draft",
            ));
        }
        let connection = self.validate_route_candidate_before_replay(candidate)?;
        let mut steps = Vec::with_capacity(candidate.commands.len());
        for command in &candidate.commands {
            allowance
                .charge(WorkKind::ValidationRecords, 1)
                .map_err(stopped)?;
            if self.done() {
                return Err(error(
                    "repair-turns",
                    &candidate.connection,
                    "repair witness exceeds the native turn limit",
                ));
            }
            self.check_replay_deadline()?;
            let before = (placed + *committed) as f64 / total as f64;
            self.turns += 1;
            match command {
                Command::BeginRoute {
                    connection,
                    source_port,
                    target_port,
                } => {
                    self.begin_route(connection, source_port.as_deref(), target_port.as_deref())?
                }
                Command::Extend { direction, length } => self.extend(*direction, *length)?,
                Command::ReserveBridge {
                    position,
                    component,
                    rotation,
                    crossed_axis,
                } => self.reserve_bridge(*position, component, *rotation, *crossed_axis)?,
                Command::Connect => {
                    self.connect()?;
                    *committed += 1;
                }
                _ => {
                    return Err(error(
                        "repair-witness",
                        &candidate.connection,
                        "repair replacement contains a non-route command",
                    ));
                }
            }
            self.revision += 1;
            let after = (placed + *committed) as f64 / total as f64;
            steps.push(Step {
                accepted: true,
                turn: self.turns,
                revision: self.revision,
                progress_before: before,
                progress_after: after,
                progress_delta: after - before,
                terminal: self.done(),
                fully_validated: false,
                diagnostic: None,
            });
        }
        self.validate_route_candidate_after_replay(candidate, connection, self)?;
        Ok(steps)
    }

    fn install_repair_meter(&mut self, allowance: &RequestAllowance) {
        self.replay_deadline = Some(allowance.deadline());
        self.replay_meter = Some(Arc::new(ReplayMeter::new([
            allowance.remaining(WorkKind::ReplayRouteCells),
            allowance.remaining(WorkKind::ReplayUsers),
            allowance.remaining(WorkKind::CatalogVisits),
            allowance.remaining(WorkKind::ValidationRecords),
        ])));
    }

    fn finish_repair_meter(&mut self, allowance: &mut RequestAllowance) -> Result<(), Diagnostic> {
        let meter = self
            .replay_meter
            .take()
            .expect("repair replay meter installed");
        self.replay_deadline = None;
        let mut stopped_on = None;
        for (kind, used) in [
            WorkKind::ReplayRouteCells,
            WorkKind::ReplayUsers,
            WorkKind::CatalogVisits,
            WorkKind::ValidationRecords,
        ]
        .into_iter()
        .zip(meter.usage())
        {
            if let Err(stop) = allowance.record_completed(kind, used) {
                stopped_on.get_or_insert(stop);
            }
        }
        if let Some(kind) = meter.stopped() {
            stopped_on = Some(allowance.exhausted_stop(match kind {
                ReplayWork::RouteCells => WorkKind::ReplayRouteCells,
                ReplayWork::Users => WorkKind::ReplayUsers,
                ReplayWork::Catalog => WorkKind::CatalogVisits,
                ReplayWork::Validation => WorkKind::ValidationRecords,
            }));
        }
        if let Some(stop) = stopped_on {
            return Err(stopped(stop));
        }
        allowance.check_deadline().map_err(stopped)
    }

    pub(crate) fn rebuild_after_route_removal(
        &mut self,
        allowance: &mut RequestAllowance,
    ) -> Result<(), Diagnostic> {
        let old_junctions = std::mem::take(&mut self.junctions);
        let old_bridges = std::mem::take(&mut self.bridges);
        self.occupied = std::array::from_fn(|_| HashMap::new());
        self.used_ports.clear();
        let mut sites = BTreeMap::<(TransportKind, Point), Vec<(usize, usize)>>::new();
        let mut surviving = HashSet::new();
        for (index, route) in self.routes.iter().enumerate() {
            let Some(route) = route else { continue };
            let connection = &self.problem.connections[index];
            surviving.insert(connection.id.clone());
            for (path_index, &point) in route.cells.iter().enumerate() {
                allowance
                    .charge(WorkKind::ReplayRouteCells, 1)
                    .map_err(stopped)?;
                self.occupied[layer(connection.transport)]
                    .entry(point)
                    .or_default()
                    .push(index);
                sites
                    .entry((connection.transport, point))
                    .or_default()
                    .push((index, path_index));
            }
            for (endpoint, port, direction) in [
                (
                    &connection.source,
                    route.source_port.as_ref(),
                    aic_layout::facilities::FacilityPortDirection::Output,
                ),
                (
                    &connection.target,
                    route.target_port.as_ref(),
                    aic_layout::facilities::FacilityPortDirection::Input,
                ),
            ] {
                let (Some(&block), Some(port)) = (self.blocks.get(endpoint), port) else {
                    continue;
                };
                allowance
                    .charge(WorkKind::ReplayUsers, 1)
                    .map_err(stopped)?;
                let entry = self
                    .used_ports
                    .entry((block, port.clone()))
                    .or_insert_with(|| UsedPort {
                        commodity: connection.commodity.clone(),
                        transport: connection.transport,
                        direction,
                        rate: Rate::zero(),
                    });
                entry.rate = entry.rate.checked_add(connection.rate).map_err(|_| {
                    error(
                        "repair-port-rate",
                        &connection.id,
                        "surviving port rate overflow",
                    )
                })?;
            }
        }
        for (key, mut bridge) in old_bridges {
            allowance
                .charge(WorkKind::ValidationRecords, 1)
                .map_err(stopped)?;
            for channel in &mut bridge.channels {
                let owners = std::mem::take(&mut channel.connections);
                let bytes = owners.len().saturating_mul(std::mem::size_of::<String>());
                allowance.reserve_peak(bytes).map_err(stopped)?;
                let result = (|| {
                    let mut kept = Vec::with_capacity(owners.len());
                    for id in owners {
                        allowance
                            .charge(WorkKind::ReplayUsers, 1)
                            .map_err(stopped)?;
                        if surviving.contains(&id) {
                            kept.push(id);
                        }
                    }
                    Ok(kept)
                })();
                allowance.release_peak(bytes);
                channel.connections = result?;
            }
            if bridge
                .channels
                .iter()
                .all(|channel| !channel.connections.is_empty())
            {
                self.bridges.insert(key, bridge);
            }
        }
        for (key, users) in sites {
            allowance
                .charge(WorkKind::ValidationRecords, 1)
                .map_err(stopped)?;
            if self.bridges.contains_key(&key) {
                continue;
            }
            let mut arm_uses = Vec::with_capacity(users.len());
            for (index, path_index) in &users {
                allowance
                    .charge(WorkKind::ReplayUsers, 1)
                    .map_err(stopped)?;
                let route = self.routes[*index].as_ref().expect("survivor exists");
                arm_uses.push(self.site_arm_use(*index, route, *path_index)?);
            }
            let mut interrupted = None;
            let options =
                junction_options_with_visit(key.0, key.1, &arm_uses, &self.catalogs, || {
                    match allowance.charge(WorkKind::CatalogVisits, 1) {
                        Ok(()) => true,
                        Err(stop) => {
                            interrupted = Some(stop);
                            false
                        }
                    }
                });
            if let Some(stop) = interrupted {
                return Err(stopped(stop));
            }
            let options = options.map_err(|failure| {
                error(
                    "repair-topology",
                    failure.entity.to_string(),
                    failure.message,
                )
            })?;
            let Some(options) = options else { continue };
            let selected = old_junctions
                .get(&key)
                .filter(|prior| options.contains(&(prior.component.clone(), prior.rotation)))
                .map(|prior| (prior.component.clone(), prior.rotation))
                .unwrap_or_else(|| options[0].clone());
            let owners = users
                .iter()
                .map(|(index, _)| self.problem.connections[*index].id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let transport = match key.0 {
                TransportKind::Belt => "belt",
                TransportKind::Pipe => "pipe",
            };
            self.junctions.insert(
                key,
                Junction {
                    id: format!("junction-{transport}-{}-{}", key.1.x, key.1.y),
                    component: selected.0,
                    position: key.1,
                    rotation: selected.1,
                    connections: owners,
                },
            );
        }
        Ok(())
    }

    /// Commit a complete repair only after every replacement replays on the
    /// evolving draft and the final native state passes its stable checks.
    pub fn repaired_candidate(
        &self,
        request: &RepairRequest,
        candidate: &RepairCandidate,
    ) -> Result<(Session, Vec<Step>), Diagnostic> {
        let mut allowance = RequestAllowance::new(request.allowance_caps.clone());
        self.repaired_candidate_with_allowance(request, candidate, &mut allowance)
    }

    /// Replay a repair under the same allowance used to prepare and search
    /// its witness, so screening cannot reset work or deadline counters.
    pub fn repaired_candidate_with_allowance(
        &self,
        request: &RepairRequest,
        candidate: &RepairCandidate,
        allowance: &mut RequestAllowance,
    ) -> Result<(Session, Vec<Step>), Diagnostic> {
        self.validate_repair_request_inner(request, Some(allowance))?;
        self.validate_repair_removals(request, &candidate.removed)?;
        if candidate.schema != REPAIR_CANDIDATE_SCHEMA
            || candidate.state_revision != request.state_revision
        {
            return Err(error(
                "repair-candidate",
                "repair",
                "candidate schema or originating revision is invalid",
            ));
        }
        let expected = request
            .selected_connections
            .iter()
            .chain(candidate.removed.iter())
            .cloned()
            .collect::<BTreeSet<_>>();
        let actual = candidate
            .replacements
            .iter()
            .map(|route| route.connection.clone())
            .collect::<BTreeSet<_>>();
        if actual != expected || actual.len() != candidate.replacements.len() {
            return Err(error(
                "repair-coverage",
                "repair",
                "replacements must cover removed and selected obligations exactly once",
            ));
        }
        let expanded = candidate
            .replacements
            .iter()
            .try_fold(0usize, |total, route| {
                total.checked_add(route.expanded).ok_or_else(|| {
                    error(
                        "repair-expanded",
                        "repair",
                        "repair expansion count overflow",
                    )
                })
            })?;
        if candidate.expanded != expanded {
            return Err(error(
                "repair-expanded",
                "repair",
                "repair expansion count differs from its replacement witnesses",
            ));
        }
        let mut prepared =
            self.start_repair_draft_validated(request, &candidate.removed, allowance)?;
        for replacement in &candidate.replacements {
            prepared.apply_member_borrowed(replacement)?;
        }
        let (draft, replayed, steps) = prepared.finish()?;
        if replayed != *candidate {
            return Err(error(
                "repair-witness",
                "repair",
                "screened repair differs from the submitted witness",
            ));
        }
        Ok((draft, steps))
    }

    pub fn apply_repair_candidate(
        &mut self,
        request: &RepairRequest,
        candidate: &RepairCandidate,
    ) -> Result<Vec<Step>, Diagnostic> {
        let (draft, steps) = self.repaired_candidate(request, candidate)?;
        *self = draft;
        Ok(steps)
    }
}
