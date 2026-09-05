//! Natural-footprint first-feasible construction, independent of rigid module composition.
mod audit;
mod boundary;
use boundary::blocked_boundaries;
#[cfg(test)]
mod tests;
use super::composition::{canonicalize_node, merge_nodes, rotate_node, translate_node};
use super::routing::RouteWorkspace;
use super::*;
use crate::facilities::ValidatedFacilityCatalog;
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode, FacilityInstanceWiringReport,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;

pub const CONSTRUCTIVE_PRODUCTION_SCHEMA_VERSION: u32 = 1;
pub const CONSTRUCTIVE_PRODUCTION_REPORT_SCHEMA_VERSION: u32 = 2;
const CONNECTION_ROUTE_LIMIT: u64 = 64;
const ROUTING_WORKSPACE_CELL_LIMIT: usize = 1_000_000;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConstructiveProductionRequest {
    pub schema_version: u32,
    pub target_instance: String,
    pub max_steps: usize,
    pub max_placement_attempts: usize,
    pub max_route_searches: u64,
    #[serde(default = "default_analysis_workers")]
    pub analysis_workers: usize,
}

fn default_analysis_workers() -> usize {
    4
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructiveProductionStep {
    pub index: usize,
    pub stage: &'static str,
    pub added_instances: Vec<String>,
    pub elapsed_ms: u64,
    pub placement_attempts: usize,
    pub statistics: ConstructiveCompositionStatistics,
    pub node: ConstructiveNode,
}

/// A geometrically valid trial rejected for obstructing unfinished connections, never committed.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructiveProductionObstruction {
    pub connection: String,
    pub blocked_requirements: Vec<String>,
    pub candidate: ConstructiveNode,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConstructiveProductionReport {
    pub schema_version: u32,
    pub request: ConstructiveProductionRequest,
    pub per_connection_route_limit: u64,
    pub routing_workspace_cell_limit: usize,
    pub success: bool,
    pub complete: bool,
    pub internal_complete: bool,
    pub status: String,
    pub required_facilities: usize,
    pub required_internal_requirements: usize,
    pub unresolved_internal_requirements: Vec<String>,
    pub unresolved_external_requirements: Vec<String>,
    pub steps: Vec<ConstructiveProductionStep>,
    pub final_node: Option<ConstructiveNode>,
    pub failed_placement_attempts: usize,
    pub failed_statistics: ConstructiveCompositionStatistics,
    pub rejected_reasons: BTreeMap<String, u64>,
    pub attempted_groups_at_failure: Vec<Vec<String>>,
    pub last_obstruction: Option<ConstructiveProductionObstruction>,
    pub elapsed_ms: u64,
    pub diagnostics: Vec<ConstructiveFrontierDiagnostic>,
}

struct Constructor<'a> {
    wiring: &'a FacilityInstanceWiringReport,
    facilities: &'a ValidatedFacilityCatalog,
    items: &'a ValidatedItemCatalog,
    transports: &'a ValidatedTransportCatalog,
    components: &'a ValidatedLogisticsComponentCatalog,
    request: &'a ConstructiveProductionRequest,
    reasons: BTreeMap<String, u64>,
    statistics: ConstructiveCompositionStatistics,
    attempts: usize,
    last_obstruction: Option<ConstructiveProductionObstruction>,
    screen_failure: Option<ConstructiveFrontierDiagnostic>,
}

pub fn construct_production(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    request: &ConstructiveProductionRequest,
) -> ConstructiveProductionReport {
    let _span = profile_span(ProfileOp::Construction);
    let started = Instant::now();
    let ids = wiring
        .nodes
        .iter()
        .filter_map(|n| match n {
            FacilityInstanceWiringNode::Facility { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let internal = wiring
        .edges
        .iter()
        .filter(|e| ids.contains(&e.source) && ids.contains(&e.target))
        .collect::<Vec<_>>();
    let external = wiring
        .edges
        .iter()
        .filter(|e| !ids.contains(&e.source) || !ids.contains(&e.target))
        .map(|e| e.id.clone())
        .collect::<Vec<_>>();
    let mut report = ConstructiveProductionReport {
        schema_version: CONSTRUCTIVE_PRODUCTION_REPORT_SCHEMA_VERSION,
        request: request.clone(),
        per_connection_route_limit: CONNECTION_ROUTE_LIMIT,
        routing_workspace_cell_limit: ROUTING_WORKSPACE_CELL_LIMIT,
        success: false,
        complete: false,
        internal_complete: false,
        status: "invalid-input".into(),
        required_facilities: ids.len(),
        required_internal_requirements: internal.len(),
        unresolved_internal_requirements: internal.iter().map(|e| e.id.clone()).collect(),
        unresolved_external_requirements: external,
        steps: Vec::new(),
        final_node: None,
        failed_placement_attempts: 0,
        failed_statistics: Default::default(),
        rejected_reasons: BTreeMap::new(),
        attempted_groups_at_failure: Vec::new(),
        last_obstruction: None,
        elapsed_ms: 0,
        diagnostics: Vec::new(),
    };
    let mut c = Constructor {
        wiring,
        facilities,
        items,
        transports,
        components,
        request,
        reasons: BTreeMap::new(),
        statistics: Default::default(),
        attempts: 0,
        last_obstruction: None,
        screen_failure: None,
    };
    if request.schema_version != CONSTRUCTIVE_PRODUCTION_SCHEMA_VERSION
        || !wiring.success
        || request.max_steps == 0
        || request.max_placement_attempts == 0
        || request.max_route_searches == 0
        || request.analysis_workers == 0
        || request.analysis_workers > 16
        || !ids.contains(&request.target_instance)
    {
        report.diagnostics.push(failure("production-input-invalid",&request.target_instance,"expected valid wiring, supported schema, an existing seed, positive operation budgets and 1..=16 analysis workers"));
        return report;
    }
    let all_ids = wiring
        .nodes
        .iter()
        .map(|n| match n {
            FacilityInstanceWiringNode::Facility { id, .. }
            | FacilityInstanceWiringNode::External { id, .. }
            | FacilityInstanceWiringNode::Target { id, .. }
            | FacilityInstanceWiringNode::Surplus { id, .. } => id,
        })
        .collect::<BTreeSet<_>>();
    if all_ids.len() != wiring.nodes.len()
        || wiring
            .edges
            .iter()
            .map(|e| &e.id)
            .collect::<BTreeSet<_>>()
            .len()
            != wiring.edges.len()
        || wiring.edges.iter().any(|e| {
            e.rate <= Rate::zero() || !all_ids.contains(&e.source) || !all_ids.contains(&e.target)
        })
    {
        report.diagnostics.push(failure(
            "production-input-invalid",
            &request.target_instance,
            "wiring requires unique node/edge identities, existing endpoints and positive rates",
        ));
        return report;
    }
    for e in &wiring.edges {
        let source = wiring
            .nodes
            .iter()
            .find(|n| match n {
                FacilityInstanceWiringNode::Facility { id, .. }
                | FacilityInstanceWiringNode::External { id, .. }
                | FacilityInstanceWiringNode::Target { id, .. }
                | FacilityInstanceWiringNode::Surplus { id, .. } => *id == e.source,
            })
            .unwrap();
        let target = wiring
            .nodes
            .iter()
            .find(|n| match n {
                FacilityInstanceWiringNode::Facility { id, .. }
                | FacilityInstanceWiringNode::External { id, .. }
                | FacilityInstanceWiringNode::Target { id, .. }
                | FacilityInstanceWiringNode::Surplus { id, .. } => *id == e.target,
            })
            .unwrap();
        let source_valid = match source {
            FacilityInstanceWiringNode::Facility { .. } => true,
            FacilityInstanceWiringNode::External { item, .. } => *item == e.item,
            _ => false,
        };
        let target_valid = match target {
            FacilityInstanceWiringNode::Facility { .. } => true,
            FacilityInstanceWiringNode::Target { item, .. }
            | FacilityInstanceWiringNode::Surplus { item, .. } => *item == e.item,
            _ => false,
        };
        if !source_valid || !target_valid || (!ids.contains(&e.source) && !ids.contains(&e.target))
        {
            report.diagnostics.push(failure(
                "production-input-invalid",
                &e.id,
                "external endpoint role/item mismatch or external-to-external edge",
            ));
            return report;
        }
    }
    let mut current = match c.fresh(&request.target_instance, 0) {
        Ok(node) => node,
        Err(d) => {
            report.diagnostics.push(d);
            return report;
        }
    };
    if let Err(d) = network::validate(&current, facilities, transports, components) {
        report.diagnostics.push(d);
        return report;
    }
    report.status = "step-limit".into();
    for index in 0..request.max_steps {
        let unresolved = internal
            .iter()
            .copied()
            .filter(|e| !current.internal_requirements.contains(&e.id))
            .collect::<Vec<_>>();
        if unresolved.is_empty() && current.member_instances.len() == ids.len() {
            break;
        }
        c.statistics = Default::default();
        c.attempts = 0;
        c.last_obstruction = None;
        c.screen_failure = None;
        let step_started = Instant::now();
        let mut frontier = unresolved
            .iter()
            .copied()
            .filter(|e| {
                current.member_instances.contains(&e.source)
                    || current.member_instances.contains(&e.target)
            })
            .collect::<Vec<_>>();
        frontier.sort_by_key(|e| {
            (
                c.items
                    .item(&e.item)
                    .is_none_or(|i| i.transport != TransportKind::Pipe),
                !current.member_instances.contains(&e.target),
                e.id.clone(),
            )
        });
        let mut chosen = None;
        let mut tried = BTreeSet::new();
        for edge in frontier {
            let new_id = if current.member_instances.contains(&edge.source) {
                &edge.target
            } else {
                &edge.source
            };
            let group = strong_component(new_id, &internal);
            if !tried.insert(group.clone()) {
                continue;
            }
            for variant in 0..4 {
                if let Some(candidate) = c.grow_group(&current, &group, variant) {
                    chosen = Some(candidate);
                    break;
                }
                if c.exhausted() {
                    break;
                }
            }
            if chosen.is_some() || c.exhausted() {
                break;
            }
        }
        let Some(next) = chosen else {
            report.status = "growth-exhausted".into();
            let code = if c.statistics.astar_searches >= request.max_route_searches {
                "production-route-budget-exhausted"
            } else if c.attempts >= request.max_placement_attempts {
                "production-placement-budget-exhausted"
            } else {
                "production-growth-exhausted"
            };
            report.diagnostics.push(failure(code,&request.target_instance,"no accepted facility/cycle transaction within the recorded candidate budget; prior state preserved"));
            report.failed_placement_attempts = c.attempts;
            report.failed_statistics = c.statistics.clone();
            report.attempted_groups_at_failure =
                tried.into_iter().map(|g| g.into_iter().collect()).collect();
            report.last_obstruction = c.last_obstruction.take();
            if let Some(diagnostic) = c.screen_failure.take() {
                report.diagnostics.push(diagnostic);
            }
            break;
        };
        if let Err(d) = audit::validate_coverage(&next, wiring, items) {
            report.status = "validation-failed".into();
            report.diagnostics.push(d);
            break;
        }
        let added = next
            .member_instances
            .iter()
            .filter(|id| !current.member_instances.contains(id))
            .cloned()
            .collect();
        report.steps.push(ConstructiveProductionStep {
            index,
            stage: "internal-growth",
            added_instances: added,
            elapsed_ms: millis(step_started),
            placement_attempts: c.attempts,
            statistics: c.statistics.clone(),
            node: next.clone(),
        });
        current = next;
    }
    report.unresolved_internal_requirements = internal
        .iter()
        .filter(|e| !current.internal_requirements.contains(&e.id))
        .map(|e| e.id.clone())
        .collect();
    report.internal_complete = report.unresolved_internal_requirements.is_empty()
        && current.member_instances.len() == ids.len();
    if report.internal_complete {
        let padding = 8;
        let width = current.bounds.width + 2 * padding;
        let height = current.bounds.height + 2 * padding;
        let mut working = translate_node(&current, padding, padding);
        let mut pending = wiring
            .edges
            .iter()
            .filter(|e| report.unresolved_external_requirements.contains(&e.id))
            .collect::<Vec<_>>();
        pending.sort_by_key(|e| {
            (
                items
                    .item(&e.item)
                    .is_none_or(|i| i.transport != TransportKind::Pipe),
                e.id.clone(),
            )
        });
        for edge in pending {
            c.statistics = Default::default();
            let step_started = Instant::now();
            if width
                .checked_mul(height)
                .is_none_or(|n| n > ROUTING_WORKSPACE_CELL_LIMIT as i64)
            {
                report.diagnostics.push(failure(
                    "production-routing-workspace-budget-exhausted",
                    &edge.id,
                    "external routing workspace resource guard exhausted",
                ));
                break;
            }
            let Some(next) = network::connect_external(
                &working,
                edge,
                width,
                height,
                transports,
                components,
                &mut c.statistics,
                request.max_route_searches.min(CONNECTION_ROUTE_LIMIT),
                &mut |n| {
                    network::validate(n, facilities, transports, components).is_ok()
                        && audit::validate_coverage(n, wiring, items).is_ok()
                        && analyze_constructive_port_demands(
                            &n.boundary_requirements,
                            &n.transport_networks,
                            transports,
                        )
                        .success
                },
            ) else {
                report.diagnostics.push(failure(
                    "production-external-routing-exhausted",
                    &edge.id,
                    "external connection candidates exhausted; last validated state retained",
                ));
                report.failed_statistics = c.statistics.clone();
                break;
            };
            working = next;
            current = canonicalize_node(&working);
            network::report_capacity(&mut current, transports).expect("validated network capacity");
            report.steps.push(ConstructiveProductionStep {
                index: report.steps.len(),
                stage: "external-connection",
                added_instances: vec![],
                elapsed_ms: millis(step_started),
                placement_attempts: 0,
                statistics: c.statistics.clone(),
                node: current.clone(),
            });
        }
        report
            .unresolved_external_requirements
            .retain(|id| !current.external_requirements.contains(id));
    }
    report.complete =
        report.internal_complete && report.unresolved_external_requirements.is_empty();
    report.success = report.complete;
    if let Err(d) = audit::validate_coverage(&current, wiring, items) {
        report.complete = false;
        report.success = false;
        report.diagnostics.push(d);
    }
    if report.internal_complete {
        report.status = if report.complete {
            "complete"
        } else {
            "external-connections-pending"
        }
        .into();
        if !report.complete {
            report.diagnostics.push(failure("production-external-connections-pending",&request.target_instance,"all facility connections are validated; external input/output connections remain unresolved"));
        }
    } else if report.status == "step-limit" {
        report.diagnostics.push(failure(
            "production-step-limit",
            &request.target_instance,
            "growth transaction limit reached; returning a validated partial layout",
        ));
    }
    report.final_node = Some(current);
    report.rejected_reasons = c.reasons;
    report.elapsed_ms = millis(started);
    report
}

fn millis(start: Instant) -> u64 {
    start.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}
fn failure(code: &'static str, entity: &str, message: &str) -> ConstructiveFrontierDiagnostic {
    ConstructiveFrontierDiagnostic::error(code, "/production", Some(entity.into()), message)
}

fn strong_component(seed: &str, edges: &[&FacilityInstanceWiringEdge]) -> BTreeSet<String> {
    let reach = |reverse: bool| {
        let mut found = BTreeSet::from([seed.to_string()]);
        let mut queue = VecDeque::from([seed.to_string()]);
        while let Some(id) = queue.pop_front() {
            for e in edges {
                let (a, b) = if reverse {
                    (&e.target, &e.source)
                } else {
                    (&e.source, &e.target)
                };
                if *a == id && found.insert(b.clone()) {
                    queue.push_back(b.clone());
                }
            }
        }
        found
    };
    reach(false).intersection(&reach(true)).cloned().collect()
}

impl Constructor<'_> {
    fn reject(&mut self, reason: &str) {
        *self.reasons.entry(reason.into()).or_default() += 1;
    }
    fn exhausted(&self) -> bool {
        self.attempts >= self.request.max_placement_attempts
            || self.statistics.astar_searches >= self.request.max_route_searches
    }

    fn fresh(
        &self,
        id: &str,
        variant: usize,
    ) -> Result<ConstructiveNode, ConstructiveFrontierDiagnostic> {
        let mut node = construct_facility_node(self.wiring, self.facilities, self.items, id)?;
        let mut groups =
            BTreeMap::<(u8, TransportKind, String), (Rate, Vec<PlacedFacilityPort>)>::new();
        for b in &node.boundary_requirements {
            let key = (
                u8::from(b.direction == FacilityPortDirection::Output),
                b.transport,
                b.item.clone(),
            );
            let group = groups.entry(key).or_insert((
                Rate::zero(),
                b.attachments.iter().map(|a| a.port().clone()).collect(),
            ));
            group.0 = group.0.checked_add(b.rate).map_err(|_| {
                failure(
                    "production-port-allocation-failed",
                    id,
                    "incident rate overflow",
                )
            })?;
        }
        let mut used = BTreeSet::new();
        let mut assignments = BTreeMap::new();
        for (key, (rate, mut ports)) in groups {
            ports.sort_by(|a, b| a.port.cmp(&b.port));
            if !ports.is_empty() {
                let offset = variant % ports.len();
                ports.rotate_left(offset);
            }
            let lanes =
                super::capacity::split_rate_into_lanes(rate, key.1, self.transports, id)?.len();
            let selected = ports
                .into_iter()
                .filter(|p| !used.contains(&p.port))
                .take(lanes)
                .collect::<Vec<_>>();
            if selected.len() != lanes {
                return Err(failure(
                    "production-port-allocation-failed",
                    id,
                    "capacity-grouped incident demand exceeds distinct compatible ports",
                ));
            }
            used.extend(selected.iter().map(|p| p.port.clone()));
            assignments.insert(key, selected);
        }
        for b in &mut node.boundary_requirements {
            b.attachments = assignments[&(
                u8::from(b.direction == FacilityPortDirection::Output),
                b.transport,
                b.item.clone(),
            )]
                .iter()
                .cloned()
                .map(|port| ConstructiveBoundaryAttachment::NewPort { port })
                .collect();
        }
        Ok(node)
    }

    fn grow_group(
        &mut self,
        base: &ConstructiveNode,
        group: &BTreeSet<String>,
        variant: usize,
    ) -> Option<ConstructiveNode> {
        let _span = profile_span(ProfileOp::GroupGrowth);
        let mut candidate = base.clone();
        while group
            .iter()
            .any(|id| !candidate.member_instances.contains(id))
        {
            let mut edges = self
                .wiring
                .edges
                .iter()
                .filter(|e| {
                    candidate.member_instances.contains(&e.source)
                        && group.contains(&e.target)
                        && !candidate.member_instances.contains(&e.target)
                        || candidate.member_instances.contains(&e.target)
                            && group.contains(&e.source)
                            && !candidate.member_instances.contains(&e.source)
                })
                .collect::<Vec<_>>();
            edges.sort_by_key(|e| {
                (
                    self.items
                        .item(&e.item)
                        .is_none_or(|i| i.transport != TransportKind::Pipe),
                    !candidate.member_instances.contains(&e.target),
                    e.id.clone(),
                )
            });
            let edge = *edges.first()?;
            let id = if candidate.member_instances.contains(&edge.source) {
                &edge.target
            } else {
                &edge.source
            };
            let anchor = if *id == edge.source {
                &edge.target
            } else {
                &edge.source
            };
            candidate = self.add_facility(&candidate, id, anchor, variant)?;
        }
        // Also close a frontier between existing facilities, including cycles touching the seed.
        self.close_internal(candidate)
    }

    fn add_facility(
        &mut self,
        current: &ConstructiveNode,
        id: &str,
        anchor: &str,
        variant: usize,
    ) -> Option<ConstructiveNode> {
        let _span = profile_span(ProfileOp::FacilityAddition);
        let fresh = match self.fresh(id, variant) {
            Ok(n) => n,
            Err(d) => {
                self.reject(&format!("{}: {}", d.code, id));
                return None;
            }
        };
        let clearance = 4 + fresh.placements[0].width.max(fresh.placements[0].height);
        let margin = clearance + fresh.bounds.width.max(fresh.bounds.height) + 4;
        let target = translate_node(current, margin, margin);
        let parent = target.placements.iter().find(|p| p.instance == anchor)?;
        let occupied = super::first_pipe_frontier::occupied_cells(&target.placements);
        for attempt in 0..32 {
            if self.exhausted() {
                return None;
            }
            self.attempts += 1;
            let rotation = ((attempt / 4 + variant) % 4) as i64 * 90;
            let rotated = rotate_node(&fresh, rotation);
            if !self
                .facilities
                .facility(&rotated.placements[0].facility)?
                .allowed_rotations
                .contains(&rotated.placements[0].rotation)
            {
                continue;
            }
            let shift = (attempt / 16) as i64 * clearance;
            let side = (attempt + current.member_instances.len() + variant) % 4;
            let (x, y) = if attempt < 16 {
                match side {
                    0 => (parent.x + parent.width + clearance, parent.y),
                    1 => (parent.x, parent.y + parent.height + clearance),
                    2 => (parent.x - clearance - rotated.bounds.width, parent.y),
                    _ => (parent.x, parent.y - clearance - rotated.bounds.height),
                }
            } else {
                match side {
                    0 => (margin + current.bounds.width + clearance, margin + shift),
                    1 => (margin + shift, margin + current.bounds.height + clearance),
                    2 => (margin - clearance - rotated.bounds.width, margin + shift),
                    _ => (margin + shift, margin - clearance - rotated.bounds.height),
                }
            };
            let source = translate_node(&rotated, x, y);
            let cells = super::first_pipe_frontier::occupied_cells(&source.placements);
            if cells.iter().any(|p| occupied.contains(p))
                || target
                    .transport_networks
                    .iter()
                    .flat_map(|n| &n.cells)
                    .any(|p| cells.contains(&(p.x, p.y)))
            {
                self.reject("production-placement-collision");
                continue;
            }
            let mut merged = merge_nodes(&source, &target);
            network::normalize(&mut merged.transport_networks);
            merged = canonicalize_node(&merged);
            if let Some(result) = self.close_internal(merged) {
                return Some(result);
            }
        }
        None
    }

    fn close_internal(&mut self, mut node: ConstructiveNode) -> Option<ConstructiveNode> {
        let _span = profile_span(ProfileOp::InternalClosure);
        let mut edges = self
            .wiring
            .edges
            .iter()
            .filter(|e| {
                node.member_instances.contains(&e.source)
                    && node.member_instances.contains(&e.target)
                    && !node.internal_requirements.contains(&e.id)
            })
            .collect::<Vec<_>>();
        edges.sort_by_key(|e| {
            (
                self.items
                    .item(&e.item)
                    .is_none_or(|i| i.transport != TransportKind::Pipe),
                e.id.clone(),
            )
        });
        for edge in edges {
            let padding = 8;
            let shifted = translate_node(&node, padding, padding);
            let width = node.bounds.width + 2 * padding;
            let height = node.bounds.height + 2 * padding;
            if width
                .checked_mul(height)
                .is_none_or(|cells| cells > ROUTING_WORKSPACE_CELL_LIMIT as i64)
            {
                self.reject("production-routing-workspace-budget-exhausted");
                return None;
            }
            let mut workspace = RouteWorkspace::new(width, height);
            let facilities = self.facilities;
            let transports = self.transports;
            let components = self.components;
            let reasons = &mut self.reasons;
            let last_obstruction = &mut self.last_obstruction;
            let screen_failure = &mut self.screen_failure;
            let analysis_workers = self.request.analysis_workers;
            let mut accept = |n: &ConstructiveNode| {
                if let Err(d) = network::validate(n, facilities, transports, components) {
                    *reasons.entry(d.message).or_default() += 1;
                    return false;
                }
                if !analyze_constructive_port_demands(
                    &n.boundary_requirements,
                    &n.transport_networks,
                    transports,
                )
                .success
                {
                    *reasons.entry("remaining capacity".into()).or_default() += 1;
                    return false;
                }
                let blocked_requirements = match blocked_boundaries(
                    n,
                    transports,
                    components,
                    analysis_workers,
                ) {
                    Ok(blocked) => blocked,
                    Err(error) => {
                        *reasons.entry(error.code().into()).or_default() += 1;
                        *screen_failure = Some(failure(
                            error.code(),
                            &edge.id,
                            "boundary screening encountered a workspace or worker failure; this is not an inaccessible-boundary or infeasibility result",
                        ));
                        return false;
                    }
                };
                if !blocked_requirements.is_empty() {
                    *reasons
                        .entry("remaining boundary escape".into())
                        .or_default() += 1;
                    *last_obstruction = Some(ConstructiveProductionObstruction {
                        connection: edge.id.clone(),
                        blocked_requirements,
                        candidate: canonicalize_node(n),
                    });
                    return false;
                }
                true
            };
            let connection_limit = self.request.max_route_searches.min(
                self.statistics
                    .astar_searches
                    .saturating_add(CONNECTION_ROUTE_LIMIT),
            );
            let Some(next) = network::connect_first(
                &shifted,
                edge,
                transports,
                components,
                &mut workspace,
                &mut self.statistics,
                connection_limit,
                &mut accept,
            ) else {
                self.reject(&format!("connection failed: {}", edge.id));
                return None;
            };
            node = canonicalize_node(&next);
        }
        if let Err(d) = network::validate(&node, self.facilities, self.transports, self.components)
        {
            self.reject(&d.message);
            return None;
        }
        if network::report_capacity(&mut node, self.transports).is_err() {
            return None;
        }
        Some(node)
    }
}

pub fn render_constructive_production_html(
    report: &ConstructiveProductionReport,
    localization: Option<&ValidatedLocalizationCatalog>,
) -> Result<String, IntegratedLayoutDiagnostic> {
    let _span = profile_span(ProfileOp::HtmlRendering);
    let nodes = if report.steps.is_empty() {
        report.final_node.iter().collect::<Vec<_>>()
    } else {
        report.steps.iter().map(|s| &s.node).collect()
    };
    if nodes.is_empty() {
        return render_layout_history_html(&[], false, localization);
    }
    let networks = nodes
        .iter()
        .map(|n| {
            let mut routes = n.transport_networks.clone();
            routes.extend(boundary_visualization_networks(
                &n.boundary_requirements,
                "production-boundary",
            ));
            routes
        })
        .collect::<Vec<_>>();
    let pages = nodes
        .iter()
        .zip(&networks)
        .enumerate()
        .map(|(i, (n, networks))| LayoutVisualizationPage {
            bounds: &n.bounds,
            placements: &n.placements,
            logistics_components: &n.logistics_components,
            transport_networks: networks,
            introduced_facilities: report
                .steps
                .get(i)
                .map(|s| s.added_instances.iter().map(String::as_str).collect())
                .unwrap_or_default(),
            label: format!(
                "Construction {} · {} · {}",
                i + 1,
                report.steps.get(i).map(|s| s.stage).unwrap_or("seed"),
                report.status
            ),
            detail: format!(
                " · {} / {} facilities · {} / {} internal requirements",
                n.member_instances.len(),
                report.required_facilities,
                n.internal_requirements.len(),
                report.required_internal_requirements
            ),
            history: true,
        })
        .collect::<Vec<_>>();
    let html = render_layout_history_html(&pages, report.complete, localization)?;
    Ok(add_capacity_inspection(html, &nodes))
}
