//! Natural-footprint first-feasible construction, independent of rigid module composition.
mod audit;
mod boundary;
mod direct;
mod order;
mod orientation;
mod placement;
use boundary::blocked_boundaries;
pub use direct::{ConstructiveDirectPlacementDecision, ConstructiveDirectPlacementTrial};
pub use order::ConstructiveGrowthDecision;
pub use orientation::ConstructiveOrientationDecision;
pub use placement::ConstructivePlacementDecision;
#[cfg(test)]
mod tests;
use super::composition::{canonicalize_node, merge_nodes, rotate_node, translate_node};
use super::routing::RouteWorkspace;
use super::*;
use crate::facilities::{FacilityPortEdge, ValidatedFacilityCatalog};
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

fn connection_route_limit() -> u64 {
    CONNECTION_ROUTE_LIMIT
}

fn is_default_connection_route_limit(value: &u64) -> bool {
    *value == CONNECTION_ROUTE_LIMIT
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConstructiveProductionRequest {
    pub schema_version: u32,
    pub target_instance: String,
    pub max_steps: usize,
    pub max_placement_attempts: usize,
    pub max_route_searches: u64,
    #[serde(
        default = "connection_route_limit",
        skip_serializing_if = "is_default_connection_route_limit"
    )]
    pub per_connection_route_searches: u64,
    #[serde(default = "default_analysis_workers")]
    pub analysis_workers: usize,
    /// Number of valid nearby alternatives to compare; zero retains coarse first-feasible growth.
    #[serde(default)]
    pub nearby_candidates: usize,
    /// Commit only internal downstream-closed growth; external terminals remain a final stage.
    #[serde(default)]
    pub downstream_closed_growth: bool,
    /// Fixed catalog-derived facility orientation along the desired material-flow axis.
    #[serde(default)]
    pub flow_direction: Option<FacilityPortEdge>,
    /// Place beside direct graph neighbors and accept the first routable local position.
    #[serde(default)]
    pub connection_first_placement: bool,
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
    pub placement_decisions: Vec<ConstructivePlacementDecision>,
    pub growth_decisions: Vec<ConstructiveGrowthDecision>,
    pub orientation_decisions: Vec<ConstructiveOrientationDecision>,
    pub direct_placement_decisions: Vec<ConstructiveDirectPlacementDecision>,
    pub attempted_groups_at_failure: Vec<Vec<String>>,
    pub last_obstruction: Option<ConstructiveProductionObstruction>,
    pub elapsed_ms: u64,
    pub diagnostics: Vec<ConstructiveFrontierDiagnostic>,
}

/// Final geometry and construction outcome for callers that do not consume step history.
/// The same search engine and validation gates produce this and the full CLI report.
#[derive(Debug)]
pub struct ConstructiveProductionCompactResult {
    pub success: bool,
    pub complete: bool,
    pub status: String,
    pub required_facilities: usize,
    pub accepted_growth_steps: usize,
    pub accepted_steps: usize,
    pub final_node: Option<ConstructiveNode>,
    pub failed_placement_attempts: usize,
    pub failed_statistics: ConstructiveCompositionStatistics,
    pub rejected_reasons: BTreeMap<String, u64>,
    pub diagnostics: Vec<ConstructiveFrontierDiagnostic>,
    pub elapsed_ms: u64,
    pub request: ConstructiveProductionRequest,
    pub per_connection_route_limit: u64,
    pub routing_workspace_cell_limit: usize,
}

struct Constructor<'a> {
    retain_history: bool,
    fixed_rotations: BTreeMap<String, i64>,
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
    placement_decisions: Vec<ConstructivePlacementDecision>,
    direct_placement_decisions: Vec<ConstructiveDirectPlacementDecision>,
}

pub fn construct_production(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    request: &ConstructiveProductionRequest,
) -> ConstructiveProductionReport {
    construct_production_inner(
        wiring, facilities, items, transports, components, request, true,
    )
    .0
}

pub fn construct_production_compact(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    request: &ConstructiveProductionRequest,
) -> ConstructiveProductionCompactResult {
    let (report, accepted_steps, accepted_growth_steps) = construct_production_inner(
        wiring, facilities, items, transports, components, request, false,
    );
    ConstructiveProductionCompactResult {
        success: report.success,
        complete: report.complete,
        status: report.status,
        required_facilities: report.required_facilities,
        accepted_growth_steps,
        accepted_steps,
        final_node: report.final_node,
        failed_placement_attempts: report.failed_placement_attempts,
        failed_statistics: report.failed_statistics,
        rejected_reasons: report.rejected_reasons,
        diagnostics: report.diagnostics,
        elapsed_ms: report.elapsed_ms,
        request: report.request,
        per_connection_route_limit: report.per_connection_route_limit,
        routing_workspace_cell_limit: report.routing_workspace_cell_limit,
    }
}

fn construct_production_inner(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
    request: &ConstructiveProductionRequest,
    retain_history: bool,
) -> (ConstructiveProductionReport, usize, usize) {
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
        per_connection_route_limit: request.per_connection_route_searches,
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
        placement_decisions: Vec::new(),
        growth_decisions: Vec::new(),
        orientation_decisions: Vec::new(),
        direct_placement_decisions: Vec::new(),
        attempted_groups_at_failure: Vec::new(),
        last_obstruction: None,
        elapsed_ms: 0,
        diagnostics: Vec::new(),
    };
    let mut accepted_steps = 0;
    let mut accepted_growth_steps = 0;
    let mut c = Constructor {
        retain_history,
        fixed_rotations: BTreeMap::new(),
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
        placement_decisions: Vec::new(),
        direct_placement_decisions: Vec::new(),
    };
    if request.schema_version != CONSTRUCTIVE_PRODUCTION_SCHEMA_VERSION
        || !wiring.success
        || request.max_steps == 0
        || request.max_placement_attempts == 0
        || request.max_route_searches == 0
        || request.per_connection_route_searches == 0
        || request.analysis_workers == 0
        || request.analysis_workers > 16
        || request.nearby_candidates > 8
        || !ids.contains(&request.target_instance)
    {
        report.diagnostics.push(failure("production-input-invalid",&request.target_instance,"expected valid wiring, supported schema, an existing seed, positive operation budgets, 1..=16 analysis workers and 0..=8 nearby candidates"));
        return (report, accepted_steps, accepted_growth_steps);
    }
    if request.connection_first_placement
        && (request.flow_direction.is_none() || request.nearby_candidates != 0)
    {
        report.diagnostics.push(failure(
            "production-direct-placement-options-invalid",
            &request.target_instance,
            "connection-first placement requires flow_direction and nearby_candidates = 0",
        ));
        return (report, accepted_steps, accepted_growth_steps);
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
        return (report, accepted_steps, accepted_growth_steps);
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
            return (report, accepted_steps, accepted_growth_steps);
        }
    }
    if let Some(direction) = request.flow_direction {
        for id in &ids {
            let decision = construct_facility_node(wiring, facilities, items, id)
                .and_then(|node| orientation::choose(&node, facilities, direction));
            match decision {
                Ok(decision) => {
                    c.fixed_rotations.insert(id.clone(), decision.rotation);
                    if retain_history {
                        report.orientation_decisions.push(decision);
                    }
                }
                Err(d) => {
                    report.diagnostics.push(d);
                    return (report, accepted_steps, accepted_growth_steps);
                }
            }
        }
    }
    let mut current = match c.fresh(&request.target_instance) {
        Ok(node) => node,
        Err(d) => {
            report.diagnostics.push(d);
            return (report, accepted_steps, accepted_growth_steps);
        }
    };
    if let Err(d) = network::validate(&current, facilities, transports, components) {
        report.diagnostics.push(d);
        return (report, accepted_steps, accepted_growth_steps);
    }
    report.status = "step-limit".into();
    // A seed with downstream obligations is private until its full dependency group closes.
    let mut has_closed_incumbent = !request.downstream_closed_growth
        || order::unclosed_outputs(&current, &internal).is_empty();
    for index in 0..request.max_steps {
        if crate::work_control::stopped() {
            break;
        }
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
        let mut groups = Vec::new();
        for edge in frontier {
            let new_id = if current.member_instances.contains(&edge.source) {
                &edge.target
            } else {
                &edge.source
            };
            let group = if request.downstream_closed_growth {
                order::downstream_group(new_id, &current.member_instances, &internal)
            } else {
                strong_component(new_id, &internal)
            };
            groups.push((new_id.clone(), group));
        }
        if request.downstream_closed_growth {
            if has_closed_incumbent {
                // Stable sorting retains the existing pipe/upstream/ID order within group sizes.
                groups.sort_by_key(|(_, group)| {
                    group
                        .iter()
                        .filter(|id| !current.member_instances.contains(id))
                        .count()
                });
            } else {
                groups = vec![(
                    request.target_instance.clone(),
                    order::downstream_group(&request.target_instance, &[], &internal),
                )];
            }
        }
        let mut chosen = None;
        let mut tried = BTreeSet::new();
        for (root, group) in groups {
            if !tried.insert(group.clone()) {
                continue;
            }
            for variant in 0..4 {
                let candidate = c.grow_group(&current, &group, variant);
                if retain_history && request.downstream_closed_growth {
                    report.growth_decisions.push(ConstructiveGrowthDecision {
                        step: index,
                        root: root.clone(),
                        variant,
                        new_members: group
                            .iter()
                            .filter(|id| !current.member_instances.contains(id))
                            .cloned()
                            .collect(),
                        success: candidate.is_some(),
                    });
                }
                if let Some(candidate) = candidate {
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
            report.diagnostics.push(failure(code, &request.target_instance,
                if !has_closed_incumbent {
                    "private downstream seed transaction exhausted its candidates; no closed incumbent exists"
                } else {
                    "no accepted connected growth transaction within the recorded candidate budget; prior state preserved"
                }));
            report.failed_placement_attempts = c.attempts;
            report.failed_statistics = c.statistics.clone();
            if retain_history {
                report.attempted_groups_at_failure =
                    tried.into_iter().map(|g| g.into_iter().collect()).collect();
                report.last_obstruction = c.last_obstruction.take();
            }
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
        if request.downstream_closed_growth && !order::unclosed_outputs(&next, &internal).is_empty()
        {
            report.status = "validation-failed".into();
            report.diagnostics.push(failure(
                "production-downstream-closure-failed",
                &request.target_instance,
                "growth candidate retained uncovered internal output requirements",
            ));
            break;
        }
        has_closed_incumbent = true;
        accepted_steps += 1;
        accepted_growth_steps += 1;
        if retain_history {
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
        }
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
            if crate::work_control::stopped() {
                break;
            }
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
            let external_limit = request
                .max_route_searches
                .min(request.per_connection_route_searches);
            let mut stop = network::ConnectionSearchStop::default();
            let Some(next) = network::connect_external(
                &working,
                edge,
                width,
                height,
                transports,
                components,
                &mut c.statistics,
                external_limit,
                &mut stop,
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
                let code = match stop {
                    network::ConnectionSearchStop::RouteCallBudget
                        if request.per_connection_route_searches < request.max_route_searches =>
                    {
                        "production-external-connection-route-budget-exhausted"
                    }
                    network::ConnectionSearchStop::RouteCallBudget => {
                        "production-external-step-route-budget-exhausted"
                    }
                    network::ConnectionSearchStop::WorkInterrupted => {
                        "production-external-work-interrupted"
                    }
                    network::ConnectionSearchStop::ResourceExhausted => {
                        "production-external-search-resource-exhausted"
                    }
                    network::ConnectionSearchStop::CandidateExhaustion => {
                        "production-external-routing-exhausted"
                    }
                };
                c.reject(code);
                report.diagnostics.push(failure(
                    code,
                    &edge.id,
                    "external connection search stopped; last validated state retained",
                ));
                report.failed_statistics = c.statistics.clone();
                break;
            };
            working = next;
            current = canonicalize_node(&working);
            network::report_capacity(&mut current, transports).expect("validated network capacity");
            let step_index = accepted_steps;
            accepted_steps += 1;
            if retain_history {
                report.steps.push(ConstructiveProductionStep {
                    index: step_index,
                    stage: "external-connection",
                    added_instances: vec![],
                    elapsed_ms: millis(step_started),
                    placement_attempts: 0,
                    statistics: c.statistics.clone(),
                    node: current.clone(),
                });
            }
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
    report.final_node = has_closed_incumbent.then_some(current);
    report.rejected_reasons = c.reasons;
    report.placement_decisions = c.placement_decisions;
    report.direct_placement_decisions = c.direct_placement_decisions;
    report.elapsed_ms = millis(started);
    (report, accepted_steps, accepted_growth_steps)
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
        crate::work_control::stopped()
            || self.attempts >= self.request.max_placement_attempts
            || self.statistics.astar_searches >= self.request.max_route_searches
    }

    fn fresh(&self, id: &str) -> Result<ConstructiveNode, ConstructiveFrontierDiagnostic> {
        let mut node = construct_facility_node(self.wiring, self.facilities, self.items, id)?;
        if let Some(rotation) = self.fixed_rotations.get(id) {
            let delta = (rotation - node.placements[0].rotation).rem_euclid(360);
            node = rotate_node(&node, delta);
        }
        let demand = analyze_constructive_port_demands(
            &node.boundary_requirements,
            &node.transport_networks,
            self.transports,
        );
        if let Some(diagnostic) = demand.diagnostics.into_iter().next() {
            return Err(diagnostic);
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
        let fresh = match self.fresh(id) {
            Ok(n) => n,
            Err(d) => {
                self.reject(&format!("{}: {}", d.code, id));
                return None;
            }
        };
        let clearance = 4 + fresh.placements[0].width.max(fresh.placements[0].height);
        let margin = clearance + fresh.bounds.width.max(fresh.bounds.height) + 4;
        let target = translate_node(current, margin, margin);
        if self.request.connection_first_placement {
            return self.select_direct(&target, &fresh, variant);
        }
        let parent = target.placements.iter().find(|p| p.instance == anchor)?;
        let occupied = super::first_pipe_frontier::occupied_cells(&target.placements);
        if self.request.nearby_candidates > 0 {
            if let Some(best) = self.select_nearby(&target, &fresh, anchor, variant, margin) {
                return Some(best);
            }
        }
        for attempt in 0..32 {
            if self.exhausted() {
                return None;
            }
            let rotation = ((attempt / 4 + variant) % 4) as i64 * 90;
            if self.request.flow_direction.is_some() && rotation != 0 {
                continue;
            }
            self.attempts += 1;
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
            if self.request.nearby_candidates > 0
                && !placement::future_corridors_clear(&merged, self.wiring)
            {
                self.reject("production-placement-future-corridor-blocked");
                continue;
            }
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
            let retain_history = self.retain_history;
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
                    if retain_history {
                        *last_obstruction = Some(ConstructiveProductionObstruction {
                            connection: edge.id.clone(),
                            blocked_requirements,
                            candidate: canonicalize_node(n),
                        });
                    }
                    return false;
                }
                true
            };
            let per_connection_end = self
                .statistics
                .astar_searches
                .saturating_add(self.request.per_connection_route_searches);
            let connection_limit = self.request.max_route_searches.min(per_connection_end);
            let mut stop = network::ConnectionSearchStop::default();
            let Some(next) = network::connect_first(
                &shifted,
                edge,
                transports,
                components,
                &mut workspace,
                &mut self.statistics,
                connection_limit,
                &mut stop,
                &mut accept,
            ) else {
                let reason = match stop {
                    network::ConnectionSearchStop::RouteCallBudget
                        if per_connection_end < self.request.max_route_searches =>
                    {
                        "production-connection-route-budget-exhausted"
                    }
                    network::ConnectionSearchStop::RouteCallBudget => {
                        "production-step-route-budget-exhausted"
                    }
                    network::ConnectionSearchStop::WorkInterrupted => {
                        "production-connection-work-interrupted"
                    }
                    network::ConnectionSearchStop::ResourceExhausted => {
                        "production-connection-search-resource-exhausted"
                    }
                    network::ConnectionSearchStop::CandidateExhaustion => {
                        "production-connection-candidates-exhausted"
                    }
                };
                self.reject(reason);
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
