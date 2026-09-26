//! Atomic removal and replacement of committed routes.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use aic_layout::{
    geometry::{Point, layer},
    logistics::TransportKind,
    physical::{Junction, Layout, Problem, SCHEMA, Solution},
    rate::Rate,
    validation::{junction_options_with_visit, validate_with_visit},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{AllowanceCaps, AllowanceStop, Candidate, Config, RequestAllowance, WorkKind};
use crate::constructive::{Diagnostic, Session, Step, UsedPort, error};

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

impl Session {
    /// Hash the complete stable state, including immutable problem, catalog and
    /// command limits. This is deliberately computed only at a repair boundary.
    pub fn repair_state_sha256(&self) -> Result<String, Diagnostic> {
        let encoded = serde_json::to_vec(&(
            &self.problem,
            &self.catalogs.identity,
            &self.limits,
            &self.final_failure,
            self.observe(),
        ))
        .map_err(|failure| {
            error(
                "repair-state-identity",
                "repair",
                format!("cannot encode exact native state: {failure}"),
            )
        })?;
        Ok(Sha256::digest(encoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
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
            state_sha256: self.repair_state_sha256()?,
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

    fn validate_repair_request(&self, request: &RepairRequest) -> Result<(), Diagnostic> {
        if request.schema != REPAIR_REQUEST_SCHEMA
            || request.state_revision != self.revision
            || request.state_sha256 != self.repair_state_sha256()?
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
        self.validate_repair_request(request)?;
        self.validate_repair_removals(request, removed)?;
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

    /// Project only committed obligations into a complete physical layout.
    /// All placed bodies and terminals remain so they are checked as obstacles.
    fn committed_repair_layout(&self) -> Layout {
        Layout {
            schema: SCHEMA.into(),
            catalog_sha256: self.catalogs.identity.clone(),
            problem: Problem {
                blocks: self
                    .problem
                    .blocks
                    .iter()
                    .zip(&self.poses)
                    .filter_map(|(block, pose)| pose.as_ref().map(|_| block.clone()))
                    .collect(),
                terminals: self
                    .problem
                    .terminals
                    .iter()
                    .zip(&self.terminal_poses)
                    .filter_map(|(terminal, pose)| pose.as_ref().map(|_| terminal.clone()))
                    .collect(),
                connections: self
                    .problem
                    .connections
                    .iter()
                    .zip(&self.routes)
                    .filter_map(|(connection, route)| route.as_ref().map(|_| connection.clone()))
                    .collect(),
            },
            solution: Solution {
                poses: self.poses.iter().flatten().cloned().collect(),
                terminal_poses: self.terminal_poses.iter().flatten().cloned().collect(),
                routes: self.routes.iter().flatten().cloned().collect(),
                bridges: {
                    let mut values = self.bridges.values().cloned().collect::<Vec<_>>();
                    values.sort_by(|left, right| left.id.cmp(&right.id));
                    values
                },
                junctions: {
                    let mut values = self.junctions.values().cloned().collect::<Vec<_>>();
                    values.sort_by(|left, right| left.id.cmp(&right.id));
                    values
                },
            },
        }
    }

    fn validate_committed_repair_layout(
        &self,
        allowance: &mut RequestAllowance,
    ) -> Result<(), Diagnostic> {
        let projected_bytes = self.clone_reservation_bytes();
        allowance.reserve_peak(projected_bytes).map_err(stopped)?;
        let result = (|| {
            allowance
                .charge(WorkKind::CopiedBytes, projected_bytes)
                .map_err(stopped)?;
            let layout = self.committed_repair_layout();
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
            })?;
            Ok(())
        })();
        allowance.release_peak(projected_bytes);
        result
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
                channel.connections.retain(|id| surviving.contains(id));
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
        self.validate_repair_request(request)?;
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
        let mut allowance = RequestAllowance::new(request.allowance_caps.clone());
        let (mut draft, mut reserved_bytes) =
            self.prepare_repair_draft_with_allowance(request, &candidate.removed, &mut allowance)?;
        // Each member returns one Step per command. Earlier Steps remain live
        // while the next member's return vector and the accumulating output
        // coexist; reserve both vectors before starting replay.
        let steps_bytes = candidate
            .replacements
            .iter()
            .map(|replacement| replacement.commands.len())
            .sum::<usize>()
            .saturating_mul(std::mem::size_of::<Step>().saturating_add(128))
            .saturating_mul(2);
        if let Err(stop) = allowance.reserve_peak(steps_bytes) {
            allowance.release_peak(reserved_bytes);
            return Err(stopped(stop));
        }
        reserved_bytes = reserved_bytes.saturating_add(steps_bytes);
        let result = (|| {
            let mut steps = Vec::new();
            for replacement in &candidate.replacements {
                if replacement.state_revision != draft.revision {
                    return Err(error(
                        "repair-revision",
                        &replacement.connection,
                        "replacement revision does not match the evolving repair draft",
                    ));
                }
                let applied = draft
                    .apply_route_candidate_with_allowance(replacement, &mut allowance)
                    .map_err(|failure| failure.into_diagnostic(&replacement.connection))?;
                allowance
                    .charge(
                        WorkKind::CopiedBytes,
                        applied.len().saturating_mul(std::mem::size_of::<Step>()),
                    )
                    .map_err(stopped)?;
                steps.extend(applied);
                let live_bytes = self
                    .clone_reservation_bytes()
                    .saturating_add(draft.clone_reservation_bytes())
                    .saturating_add(steps_bytes);
                if live_bytes > reserved_bytes {
                    allowance
                        .reserve_peak(live_bytes - reserved_bytes)
                        .map_err(stopped)?;
                } else {
                    allowance.release_peak(reserved_bytes - live_bytes);
                }
                reserved_bytes = live_bytes;
            }
            allowance.check_deadline().map_err(stopped)?;
            if draft.has_active_route()
                || draft.final_failure.is_some()
                || draft.is_complete() && draft.metrics.is_none()
            {
                return Err(error(
                    "repair-final-state",
                    "repair",
                    "repaired native state is not stable and independently validated",
                ));
            }
            draft.validate_committed_repair_layout(&mut allowance)?;
            for (index, route) in self.routes.iter().enumerate() {
                let id = &self.problem.connections[index].id;
                if !expected.contains(id) && draft.routes[index] != *route {
                    return Err(error(
                        "repair-unrelated-route",
                        id,
                        "repair changed a route outside selected and removed obligations",
                    ));
                }
            }
            Ok(steps)
        })();
        allowance.release_peak(reserved_bytes);
        result.map(|steps| (draft, steps))
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
