use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::thread;
use std::time::{Duration, Instant};

use pumpkin_solver::Solver;
use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::DefaultBrancher;
use pumpkin_solver::core::optimisation::OptimisationDirection;
use pumpkin_solver::core::optimisation::linear_sat_unsat::LinearSatUnsat;
use pumpkin_solver::core::results::{OptimisationResult, ProblemSolution, SolutionReference};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};
use serde::Serialize;

use crate::facilities::{
    FacilityDefinition, FacilityPortDefinition, FacilityPortDirection, FacilityPortEdge,
    FacilityPortPosition, ValidatedFacilityCatalog,
};
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementRequest, FacilityPlacementStatus,
    validate_facility_placement_request,
};
use crate::logistics::{
    LogisticsComponentKind, TransportKind, ValidatedItemCatalog,
    ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::{
    FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
    FacilityInstanceWiringNode, FacilityInstanceWiringProjection, FacilityInstanceWiringReport,
    Rate, facility_instance_wiring_edge_id,
};

use super::WorldGridPosition;
use super::placement::solve_facility_placement_feasibly_with_time_limit;

mod html;
mod iterative;
mod networks;
mod optimization;
mod score;
mod sparse;
mod witness;

pub use html::{render_integrated_layout_html, render_integrated_layout_html_with_localization};
pub use iterative::construct_iterative_scc_layout_with_time_limit;
pub use optimization::{
    CANDIDATE_POLICY_TABLE_SCHEMA_VERSION, CandidatePolicy, CandidatePolicyTable,
    ITERATIVE_OPTIMIZATION_CONFIG_SCHEMA_VERSION, IterativeOptimizationConfig,
    OptimizationConfigDiagnostic, PlacementPolicy, RoutingOrderPolicy,
    validate_candidate_policy_table, validate_iterative_optimization_config,
};
pub use score::{CandidateRank, DeterministicCandidateKey, LayoutScore, RefinementKind};

const STAGE: &str = "integrated-layout";
const PRODUCTION_FACILITY_GAP: i64 = 1;
const COORDINATE_ROUTING_FRAME: i64 = 1;
pub const INTEGRATED_LAYOUT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IntegratedLayoutStatus {
    Optimal,
    Feasible,
    Infeasible,
    InvalidInput,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutReport {
    pub schema_version: u32,
    pub success: bool,
    pub status: IntegratedLayoutStatus,
    pub bounds: Option<FacilityPlacementBounds>,
    pub placements: Vec<FacilityPlacement>,
    pub logistics_components: Vec<PlacedLogisticsComponent>,
    pub routes: Vec<IntegratedRoute>,
    pub phases: Vec<IntegratedLayoutPhase>,
    pub diagnostics: Vec<IntegratedLayoutDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutPhase {
    pub index: usize,
    pub introduced_components: Vec<String>,
    pub introduced_facilities: Vec<String>,
    pub cumulative_facility_count: usize,
    pub selected_movement_radius: Option<i64>,
    pub bounds: FacilityPlacementBounds,
    pub placements: Vec<FacilityPlacement>,
    pub logistics_components: Vec<PlacedLogisticsComponent>,
    pub routes: Vec<IntegratedRoute>,
    pub route_turns: usize,
    pub route_cells: usize,
    pub bridge_count: usize,
    pub attempts: Vec<IntegratedLayoutPhaseAttempt>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutPhaseAttempt {
    pub movement_radius: Option<i64>,
    pub status: IntegratedLayoutStatus,
    pub diagnostic_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlacedLogisticsComponent {
    pub id: String,
    pub component: String,
    pub kind: LogisticsComponentKind,
    pub transport: TransportKind,
    pub position: WorldGridPosition,
    pub rotation: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedRoute {
    pub requirement_id: String,
    pub requirement_fingerprint: RouteRequirementFingerprint,
    pub source: IntegratedRouteEndpoint,
    pub target: IntegratedRouteEndpoint,
    pub item: String,
    pub rate: Rate,
    pub transport: TransportKind,
    pub cells: Vec<WorldGridPosition>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RouteRequirementFingerprint {
    pub source: String,
    pub target: String,
    pub item: String,
    pub rate: Rate,
    pub transport: TransportKind,
    pub projection: FacilityInstanceWiringProjection,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum IntegratedRouteEndpoint {
    Facility {
        instance: String,
        port: String,
    },
    External {
        node: String,
        side: FacilityPortEdge,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl IntegratedLayoutDiagnostic {
    pub fn error(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "error",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }

    fn info(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: "/".to_string(),
            entity: None,
            message: message.into(),
        }
    }

    fn info_for(code: &'static str, entity: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: "/".to_string(),
            entity: Some(entity.into()),
            message: message.into(),
        }
    }
}

impl IntegratedLayoutReport {
    pub fn invalid(diagnostic: IntegratedLayoutDiagnostic) -> Self {
        Self::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
    }

    fn failure(status: IntegratedLayoutStatus, diagnostic: IntegratedLayoutDiagnostic) -> Self {
        Self {
            schema_version: INTEGRATED_LAYOUT_SCHEMA_VERSION,
            success: false,
            status,
            bounds: None,
            placements: Vec::new(),
            logistics_components: Vec::new(),
            routes: Vec::new(),
            phases: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }
}

pub fn solve_integrated_layout(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &FacilityPlacementRequest,
) -> IntegratedLayoutReport {
    solve_integrated_layout_with_optional_time_limit(
        instance_wiring,
        facilities,
        items,
        transports,
        request,
        None,
    )
}

pub fn solve_integrated_layout_with_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Duration,
) -> IntegratedLayoutReport {
    solve_integrated_layout_with_optional_time_limit(
        instance_wiring,
        facilities,
        items,
        transports,
        request,
        Some(time_limit),
    )
}

pub fn construct_sparse_integrated_layout(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
) -> IntegratedLayoutReport {
    match prepare_model(instance_wiring, facilities, items, transports, request) {
        Ok(input) => match required_facility_area(&input) {
            Ok(required_area) => {
                let available_area = i64::from(input.width) * i64::from(input.height);
                if required_area > available_area {
                    IntegratedLayoutReport::failure(
                        IntegratedLayoutStatus::Infeasible,
                        IntegratedLayoutDiagnostic::error(
                            "facility-area-exceeds-layout-bounds",
                            "/",
                            None,
                            format!(
                                "facility footprints require at least {required_area} cells but hard layout bounds provide {available_area} cells"
                            ),
                        ),
                    )
                } else {
                    sparse::construct(input, logistics_components)
                }
            }
            Err(diagnostic) => {
                IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
            }
        },
        Err(diagnostic) => {
            IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
        }
    }
}

pub fn construct_coordinate_integrated_layout_with_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Duration,
) -> IntegratedLayoutReport {
    let worker_limit = thread::available_parallelism()
        .map(|parallelism| parallelism.get())
        .unwrap_or(1)
        .min(4);
    let placement_widths = portfolio_widths(request.max_width, worker_limit);
    let mut candidates = thread::scope(|scope| {
        let mut workers = Vec::with_capacity(placement_widths.len());
        for placement_width in placement_widths.iter().copied() {
            workers.push(scope.spawn(move || {
                solve_coordinate_candidate(
                    instance_wiring,
                    facilities,
                    items,
                    transports,
                    logistics_components,
                    request,
                    placement_width,
                    time_limit,
                )
            }));
        }
        workers
            .into_iter()
            .map(|worker| worker.join().expect("coordinate portfolio worker panicked"))
            .collect::<Vec<_>>()
    });

    let successful_candidates = candidates
        .iter()
        .filter(|candidate| candidate.report.success)
        .count();
    let worker_diagnostics = candidates
        .iter()
        .map(|candidate| {
            let message = candidate.score().map_or_else(
                || {
                    format!(
                        "portfolio worker finished with status {} and no validated witness",
                        integrated_status_name(candidate.report.status)
                    )
                },
                |score| {
                    format!(
                        "portfolio worker produced a validated witness with route_cells={}, route_turns={}, area={}, max_side={}, logistics_components={}",
                        score.total_route_cells,
                        score.total_route_turns,
                        score.used_bounding_box_area,
                        score.maximum_used_side,
                        score.logistics_component_count,
                    )
                },
            );
            IntegratedLayoutDiagnostic::info_for(
                "parallel-portfolio-worker-result",
                candidate.placement_width.to_string(),
                message,
            )
        })
        .collect::<Vec<_>>();
    let best_index = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            candidate.score().map(|score| {
                (
                    index,
                    CandidateRank {
                        score,
                        deterministic_candidate_key: DeterministicCandidateKey {
                            phase_index: 0,
                            refinement_kind: RefinementKind::FinalGlobal,
                            neighborhood_rank: 3,
                            restart_index: 0,
                            policy_index: 0,
                            attempt_index: index,
                            yield_index: 0,
                        },
                    },
                )
            })
        })
        .min_by_key(|(_, rank)| *rank)
        .map(|(index, _)| index);

    let Some(best_index) = best_index else {
        let mut report = candidates
            .into_iter()
            .find(|candidate| candidate.placement_width == request.max_width)
            .expect("portfolio always contains the full-width worker")
            .report;
        report.diagnostics.extend(worker_diagnostics);
        report.diagnostics.push(IntegratedLayoutDiagnostic::info(
            "parallel-portfolio-no-witness",
            format!(
                "{} independent coordinate workers completed without a validated witness",
                placement_widths.len()
            ),
        ));
        return report;
    };

    let mut selected = candidates.swap_remove(best_index);
    let score = selected.score().expect("selected candidate is successful");
    selected.report.diagnostics.extend(worker_diagnostics);
    selected
        .report
        .diagnostics
        .push(IntegratedLayoutDiagnostic::info(
            "parallel-portfolio-selected",
            format!(
                "selected placement width cap {} from {} validated witnesses across {} independent workers with score route_cells={}, route_turns={}, area={}, max_side={}, logistics_components={}",
                selected.placement_width,
                successful_candidates,
                placement_widths.len(),
                score.total_route_cells,
                score.total_route_turns,
                score.used_bounding_box_area,
                score.maximum_used_side,
                score.logistics_component_count,
            ),
        ));
    selected.report
}

fn integrated_status_name(status: IntegratedLayoutStatus) -> &'static str {
    match status {
        IntegratedLayoutStatus::Optimal => "optimal",
        IntegratedLayoutStatus::Feasible => "feasible",
        IntegratedLayoutStatus::Infeasible => "infeasible",
        IntegratedLayoutStatus::InvalidInput => "invalid-input",
        IntegratedLayoutStatus::Unknown => "unknown",
    }
}

struct CoordinateCandidate {
    placement_width: i64,
    report: IntegratedLayoutReport,
}

impl CoordinateCandidate {
    fn score(&self) -> Option<LayoutScore> {
        LayoutScore::from_report(&self.report, &[])
    }
}

fn route_turn_count(route: &IntegratedRoute) -> usize {
    route
        .cells
        .windows(3)
        .filter(|cells| {
            let first_horizontal = cells[0].y == cells[1].y;
            let second_horizontal = cells[1].y == cells[2].y;
            first_horizontal != second_horizontal
        })
        .count()
}

fn portfolio_widths(max_width: i64, worker_limit: usize) -> Vec<i64> {
    let candidates = [
        max_width,
        max_width - max_width / 10,
        max_width - max_width / 5,
        max_width - max_width / 4,
    ];
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|width| *width > 0 && seen.insert(*width))
        .take(worker_limit.max(1))
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn solve_coordinate_candidate(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    hard_request: &FacilityPlacementRequest,
    placement_width: i64,
    time_limit: Duration,
) -> CoordinateCandidate {
    let placement_request = FacilityPlacementRequest {
        schema_version: hard_request.schema_version,
        max_width: placement_width,
        max_height: hard_request.max_height,
    };
    let placement = solve_facility_placement_feasibly_with_time_limit(
        instance_wiring,
        facilities,
        &placement_request,
        PRODUCTION_FACILITY_GAP,
        time_limit,
    );
    if !placement.success {
        let status = match placement.status {
            FacilityPlacementStatus::Infeasible => IntegratedLayoutStatus::Infeasible,
            FacilityPlacementStatus::InvalidInput => IntegratedLayoutStatus::InvalidInput,
            FacilityPlacementStatus::Unknown => IntegratedLayoutStatus::Unknown,
            FacilityPlacementStatus::Optimal | FacilityPlacementStatus::Feasible => {
                IntegratedLayoutStatus::Unknown
            }
        };
        let diagnostic = placement.diagnostics.into_iter().next().map_or_else(
            || {
                IntegratedLayoutDiagnostic::error(
                    "coordinate-placement-failed",
                    "/",
                    None,
                    "coordinate placement failed without a diagnostic",
                )
            },
            |diagnostic| {
                IntegratedLayoutDiagnostic::error(
                    "coordinate-placement-failed",
                    diagnostic.path,
                    diagnostic.entity,
                    diagnostic.message,
                )
            },
        );
        return CoordinateCandidate {
            placement_width,
            report: IntegratedLayoutReport::failure(status, diagnostic),
        };
    }

    let Some(framed_placements) = frame_placements_for_routing(
        placement.placements,
        hard_request.max_width,
        hard_request.max_height,
    ) else {
        return CoordinateCandidate {
            placement_width,
            report: IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Unknown,
                IntegratedLayoutDiagnostic::error(
                    "coordinate-routing-frame-does-not-fit",
                    "/",
                    None,
                    "coordinate placement heuristic could not translate the facilities into a one-cell routing frame inside the hard search domain",
                ),
            ),
        };
    };

    let report = match prepare_model(instance_wiring, facilities, items, transports, hard_request) {
        Ok(input) => {
            let topology = match networks::plan_topology(
                &input.networks,
                &input.edges,
                logistics_components,
            ) {
                Ok(topology) => topology,
                Err(diagnostic) => {
                    return CoordinateCandidate {
                        placement_width,
                        report: IntegratedLayoutReport::failure(
                            IntegratedLayoutStatus::InvalidInput,
                            diagnostic,
                        ),
                    };
                }
            };
            let mut report = sparse::construct_from_placements(
                input,
                logistics_components,
                framed_placements,
                Instant::now()
                    .checked_add(time_limit.max(Duration::from_secs(5)))
                    .unwrap_or_else(Instant::now),
            );
            if report.success {
                report.diagnostics.push(IntegratedLayoutDiagnostic::info(
                    "routing-network-topology-planned",
                    format!(
                        "normalized {} routing networks and identified {} capacity-share bundles across {} terminals, requiring {} splitters and {} convergers before spatial component placement; the largest bundle has {} branches",
                        topology.network_count(),
                        topology.shared_bundle_count(),
                        topology.referenced_terminal_count(),
                        topology.component_count(LogisticsComponentKind::Splitter),
                        topology.component_count(LogisticsComponentKind::Converger),
                        topology.max_branch_count(),
                    ),
                ));
            }
            report
        }
        Err(diagnostic) => {
            IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
        }
    };
    CoordinateCandidate {
        placement_width,
        report,
    }
}

fn solve_integrated_layout_with_optional_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    match prepare_model(instance_wiring, facilities, items, transports, request) {
        Ok(input) => match required_facility_area(&input) {
            Ok(required_area) => {
                let available_area = i64::from(input.width) * i64::from(input.height);
                if required_area > available_area {
                    IntegratedLayoutReport::failure(
                        IntegratedLayoutStatus::Infeasible,
                        IntegratedLayoutDiagnostic::error(
                            "facility-area-exceeds-layout-bounds",
                            "/",
                            None,
                            format!(
                                "facility footprints require at least {required_area} cells but hard layout bounds provide {available_area} cells"
                            ),
                        ),
                    )
                } else {
                    solve(input, time_limit)
                }
            }
            Err(diagnostic) => {
                IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
            }
        },
        Err(diagnostic) => {
            IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
        }
    }
}

fn required_facility_area(input: &ModelInput) -> Result<i64, IntegratedLayoutDiagnostic> {
    input.instances.iter().try_fold(0_i64, |total, instance| {
        let area = instance
            .definition
            .footprint
            .width
            .checked_mul(instance.definition.footprint.height)
            .ok_or_else(|| facility_area_overflow(&instance.id))?;
        total
            .checked_add(area)
            .ok_or_else(|| facility_area_overflow(&instance.id))
    })
}

fn facility_area_overflow(instance: &str) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "facility-area-arithmetic-overflow",
        "/",
        Some(instance.to_string()),
        "required facility area exceeds the exact layout area domain",
    )
}

struct ModelInput {
    width: i32,
    height: i32,
    instances: Vec<InstanceInput>,
    edges: Vec<EdgeInput>,
    networks: Vec<networks::RoutingNetworkInput>,
}

struct EdgeInput {
    requirement_id: String,
    requirement_fingerprint: RouteRequirementFingerprint,
    edge: FacilityInstanceWiringEdge,
    source: EndpointInput,
    target: EndpointInput,
    transport: TransportKind,
}

#[derive(Clone)]
enum EndpointInput {
    Facility {
        instance: String,
        ports: Vec<FacilityPortDefinition>,
    },
    External {
        node: String,
    },
}

struct InstanceInput {
    id: String,
    recipe: String,
    facility: String,
    definition: FacilityDefinition,
}

fn prepare_model(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &FacilityPlacementRequest,
) -> Result<ModelInput, IntegratedLayoutDiagnostic> {
    if !instance_wiring.success {
        return Err(IntegratedLayoutDiagnostic::error(
            "upstream-instance-wiring-failed",
            "/",
            None,
            "integrated layout requires successful facility instance wiring",
        ));
    }
    if instance_wiring.schema_version != FACILITY_INSTANCE_WIRING_SCHEMA_VERSION {
        return Err(IntegratedLayoutDiagnostic::error(
            "unsupported-facility-instance-wiring-schema-version",
            "/schema_version",
            None,
            format!(
                "facility instance wiring schema version {} is unsupported; expected {}",
                instance_wiring.schema_version, FACILITY_INSTANCE_WIRING_SCHEMA_VERSION
            ),
        ));
    }
    if let Some(diagnostic) = validate_facility_placement_request(request).first() {
        return Err(IntegratedLayoutDiagnostic::error(
            "invalid-layout-bounds",
            diagnostic.path.clone(),
            diagnostic.entity.clone(),
            diagnostic.message.clone(),
        ));
    }
    let mut node_by_id = BTreeMap::new();
    for (index, node) in instance_wiring.nodes.iter().enumerate() {
        let id = wiring_node_id(node);
        if node_by_id.insert(id, node).is_some() {
            return Err(IntegratedLayoutDiagnostic::error(
                "duplicate-wiring-node",
                format!("/nodes/{index}/id"),
                Some(id.to_string()),
                format!("wiring node '{id}' appears more than once"),
            ));
        }
    }
    let mut instances = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, node) in instance_wiring.nodes.iter().enumerate() {
        let FacilityInstanceWiringNode::Facility {
            id,
            recipe,
            facility,
            ..
        } = node
        else {
            continue;
        };
        if !seen.insert(id.clone()) {
            return Err(IntegratedLayoutDiagnostic::error(
                "duplicate-facility-instance",
                format!("/nodes/{index}/id"),
                Some(id.clone()),
                format!("facility instance '{id}' appears more than once"),
            ));
        }
        let definition = facilities.facility(facility).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-facility-definition",
                format!("/nodes/{index}/facility"),
                Some(facility.clone()),
                format!("facility '{facility}' is absent from the validated catalog"),
            )
        })?;
        i32::try_from(definition.footprint.width).map_err(|_| {
            IntegratedLayoutDiagnostic::error(
                "solver-domain-out-of-range",
                format!("/nodes/{index}/facility"),
                Some(facility.clone()),
                "facility width does not fit the solver's 32-bit integer domain",
            )
        })?;
        i32::try_from(definition.footprint.height).map_err(|_| {
            IntegratedLayoutDiagnostic::error(
                "solver-domain-out-of-range",
                format!("/nodes/{index}/facility"),
                Some(facility.clone()),
                "facility height does not fit the solver's 32-bit integer domain",
            )
        })?;
        instances.push(InstanceInput {
            id: id.clone(),
            recipe: recipe.clone(),
            facility: facility.clone(),
            definition: definition.clone(),
        });
    }
    instances.sort_by(|left, right| left.id.cmp(&right.id));

    let mut edges = Vec::new();
    let mut edge_ids = BTreeSet::new();
    for (edge_index, edge) in instance_wiring.edges.iter().cloned().enumerate() {
        if !edge_ids.insert(edge.id.clone()) {
            return Err(IntegratedLayoutDiagnostic::error(
                "duplicate-wiring-edge-id",
                format!("/edges/{edge_index}/id"),
                Some(edge.id.clone()),
                format!("wiring edge ID '{}' appears more than once", edge.id),
            ));
        }
        if matches!(&edge.projection, FacilityInstanceWiringProjection::Original)
            && edge.id
                != facility_instance_wiring_edge_id(
                    &edge.source,
                    &edge.target,
                    &edge.kind,
                    &edge.item,
                )
        {
            return Err(IntegratedLayoutDiagnostic::error(
                "invalid-wiring-edge-id",
                format!("/edges/{edge_index}/id"),
                Some(edge.id.clone()),
                "original wiring edge ID does not match its canonical endpoint, kind, and item tuple",
            ));
        }
        let source_node = node_by_id
            .get(edge.source.as_str())
            .ok_or_else(|| missing_route_endpoint(edge_index, "source", edge.source.as_str()))?;
        let target_node = node_by_id
            .get(edge.target.as_str())
            .ok_or_else(|| missing_route_endpoint(edge_index, "target", edge.target.as_str()))?;
        if edge.source == edge.target {
            return Err(IntegratedLayoutDiagnostic::error(
                "unsupported-self-route",
                format!("/edges/{edge_index}"),
                Some(edge.source.clone()),
                "integrated routing does not support a route from a node to itself",
            ));
        }

        let item = items.item(&edge.item).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-item-definition",
                format!("/edges/{edge_index}/item"),
                Some(edge.item.clone()),
                format!(
                    "item '{}' is absent from the validated item catalog",
                    edge.item
                ),
            )
        })?;
        let capacity = transports.capacity(item.transport);
        let capacity_rate =
            Rate::from_quantity_per_duration_ms(capacity.quantity, capacity.duration_ms).map_err(
                |_| {
                    IntegratedLayoutDiagnostic::error(
                        "transport-capacity-out-of-range",
                        format!("/edges/{edge_index}/rate"),
                        Some(format!("{:?}", item.transport).to_lowercase()),
                        "transport capacity cannot be represented in the exact rate domain",
                    )
                },
            )?;
        let source = prepare_endpoint(
            edge_index,
            "source",
            source_node,
            &instances,
            FacilityPortDirection::Output,
            item.transport,
            &edge.item,
        )?;
        let target = prepare_endpoint(
            edge_index,
            "target",
            target_node,
            &instances,
            FacilityPortDirection::Input,
            item.transport,
            &edge.item,
        )?;
        if matches!(
            (&source, &target),
            (
                EndpointInput::External { .. },
                EndpointInput::External { .. }
            )
        ) {
            return Err(IntegratedLayoutDiagnostic::error(
                "unsupported-external-to-external-route",
                format!("/edges/{edge_index}"),
                Some(edge.id.clone()),
                "integrated routing requires at least one facility endpoint",
            ));
        }
        let mut remaining_rate = edge.rate;
        let mut lane_index = 0_usize;
        while !remaining_rate.is_zero() {
            let route_rate = remaining_rate.min(capacity_rate);
            let requirement_id = format!("{}:lane:{lane_index:04}", edge.id);
            let requirement_fingerprint = RouteRequirementFingerprint {
                source: edge.source.clone(),
                target: edge.target.clone(),
                item: edge.item.clone(),
                rate: route_rate,
                transport: item.transport,
                projection: edge.projection.clone(),
            };
            edges.push(EdgeInput {
                requirement_id,
                requirement_fingerprint,
                source: source.clone(),
                target: target.clone(),
                transport: item.transport,
                edge: FacilityInstanceWiringEdge {
                    rate: route_rate,
                    ..edge.clone()
                },
            });
            remaining_rate = remaining_rate.checked_sub(route_rate).map_err(|_| {
                IntegratedLayoutDiagnostic::error(
                    "route-rate-arithmetic-overflow",
                    format!("/edges/{edge_index}/rate"),
                    Some(edge.item.clone()),
                    "route capacity splitting exceeded the exact rate domain",
                )
            })?;
            lane_index += 1;
        }
    }

    let width = i32::try_from(request.max_width).map_err(|_| solver_domain_error("max_width"))?;
    let height =
        i32::try_from(request.max_height).map_err(|_| solver_domain_error("max_height"))?;
    let networks = networks::normalize(&edges)?;

    Ok(ModelInput {
        width,
        height,
        instances,
        edges,
        networks,
    })
}

fn wiring_node_id(node: &FacilityInstanceWiringNode) -> &str {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. }
        | FacilityInstanceWiringNode::External { id, .. }
        | FacilityInstanceWiringNode::Target { id, .. }
        | FacilityInstanceWiringNode::Surplus { id, .. } => id,
    }
}

fn prepare_endpoint(
    edge_index: usize,
    endpoint_kind: &str,
    node: &FacilityInstanceWiringNode,
    instances: &[InstanceInput],
    port_direction: FacilityPortDirection,
    transport: TransportKind,
    item: &str,
) -> Result<EndpointInput, IntegratedLayoutDiagnostic> {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. } => {
            let instance = instances
                .iter()
                .find(|instance| instance.id == *id)
                .expect("every prepared facility node has an instance");
            let ports = compatible_ports(&instance.definition, port_direction, transport);
            if ports.is_empty() {
                let direction = match port_direction {
                    FacilityPortDirection::Input => "input",
                    FacilityPortDirection::Output => "output",
                };
                return Err(missing_compatible_port(
                    edge_index, id, direction, transport,
                ));
            }
            Ok(EndpointInput::Facility {
                instance: id.clone(),
                ports,
            })
        }
        FacilityInstanceWiringNode::External {
            id,
            item: node_item,
        } if endpoint_kind == "source" => {
            prepare_external_endpoint(edge_index, endpoint_kind, id, node_item, item)
        }
        FacilityInstanceWiringNode::Target {
            id,
            item: node_item,
        }
        | FacilityInstanceWiringNode::Surplus {
            id,
            item: node_item,
        } if endpoint_kind == "target" => {
            prepare_external_endpoint(edge_index, endpoint_kind, id, node_item, item)
        }
        _ => Err(IntegratedLayoutDiagnostic::error(
            "invalid-route-endpoint-kind",
            format!("/edges/{edge_index}/{endpoint_kind}"),
            Some(wiring_node_id(node).to_string()),
            format!(
                "wiring node '{}' cannot be used as a route {endpoint_kind}",
                wiring_node_id(node)
            ),
        )),
    }
}

fn prepare_external_endpoint(
    edge_index: usize,
    endpoint_kind: &str,
    node: &str,
    node_item: &str,
    edge_item: &str,
) -> Result<EndpointInput, IntegratedLayoutDiagnostic> {
    if node_item != edge_item {
        return Err(IntegratedLayoutDiagnostic::error(
            "external-item-mismatch",
            format!("/edges/{edge_index}/item"),
            Some(node.to_string()),
            format!(
                "external {endpoint_kind} node '{node}' carries item '{node_item}' but the route carries '{edge_item}'"
            ),
        ));
    }
    Ok(EndpointInput::External {
        node: node.to_string(),
    })
}

fn compatible_ports(
    definition: &FacilityDefinition,
    direction: FacilityPortDirection,
    transport: TransportKind,
) -> Vec<FacilityPortDefinition> {
    definition
        .ports
        .iter()
        .filter(|port| port.direction == direction && port.transport == transport)
        .cloned()
        .collect()
}

fn missing_route_endpoint(
    edge_index: usize,
    kind: &str,
    endpoint: &str,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "missing-route-endpoint",
        format!("/edges/{edge_index}/{kind}"),
        Some(endpoint.to_string()),
        format!("route {kind} node '{endpoint}' is absent from the wiring graph"),
    )
}

fn missing_compatible_port(
    edge_index: usize,
    instance: &str,
    direction: &str,
    transport: TransportKind,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "missing-compatible-port",
        format!("/edges/{edge_index}"),
        Some(instance.to_string()),
        format!("facility instance '{instance}' has no {direction} {transport:?} port"),
    )
}

fn solver_domain_error(field: &str) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "solver-domain-out-of-range",
        format!("/{field}"),
        None,
        format!("layout {field} does not fit the solver's 32-bit integer domain"),
    )
}

struct Candidate {
    rotation: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    occupied_cells: Vec<usize>,
    port_connections: BTreeMap<String, usize>,
    selected: DomainId,
}

struct ModelInstance {
    input: InstanceInput,
    candidates: Vec<Candidate>,
}

struct EndpointOption {
    endpoint: IntegratedRouteEndpoint,
    cell: usize,
    selected: DomainId,
    external_side: Option<FacilityPortEdge>,
}

#[derive(Clone, Copy)]
struct Arc {
    from: usize,
    to: usize,
    selected: DomainId,
}

struct ModelRoute {
    source_options: Vec<EndpointOption>,
    target_options: Vec<EndpointOption>,
    arcs: Vec<Arc>,
}

fn solve(mut input: ModelInput, time_limit: Option<Duration>) -> IntegratedLayoutReport {
    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let cell_count = (input.width as usize) * (input.height as usize);
    let mut occupancy = vec![Vec::<DomainId>::new(); cell_count];
    let mut model_instances = Vec::with_capacity(input.instances.len());

    for instance in std::mem::take(&mut input.instances) {
        let candidates = generate_candidates(&mut solver, &instance, input.width, input.height);
        if candidates.is_empty() {
            return IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Infeasible,
                IntegratedLayoutDiagnostic::error(
                    "facility-has-no-placement-candidate",
                    "/",
                    Some(instance.id),
                    "facility has no rotation and origin within the hard layout bounds",
                ),
            );
        }
        post_equals_one(
            &mut solver,
            candidates.iter().map(|candidate| candidate.selected),
            tag,
        );
        for candidate in &candidates {
            for cell in &candidate.occupied_cells {
                occupancy[*cell].push(candidate.selected);
            }
        }
        model_instances.push(ModelInstance {
            input: instance,
            candidates,
        });
    }

    for cell_candidates in &occupancy {
        post_at_most_one(&mut solver, cell_candidates.iter().copied(), tag);
    }

    let mut model_routes = Vec::with_capacity(input.edges.len());
    let mut belt_route_cells_by_grid = vec![Vec::<DomainId>::new(); cell_count];
    let mut pipe_route_cells_by_grid = vec![Vec::<DomainId>::new(); cell_count];
    let mut route_arc_variables = Vec::new();
    for (edge_index, edge) in input.edges.iter().enumerate() {
        let (source_options, target_options) = match (&edge.source, &edge.target) {
            (EndpointInput::Facility { .. }, EndpointInput::Facility { .. }) => (
                model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "source",
                    &edge.source,
                    &model_instances,
                    tag,
                ),
                model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "target",
                    &edge.target,
                    &model_instances,
                    tag,
                ),
            ),
            (EndpointInput::External { node }, EndpointInput::Facility { .. }) => {
                let target = model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "target",
                    &edge.target,
                    &model_instances,
                    tag,
                );
                (external_endpoint_options(node, &target), target)
            }
            (EndpointInput::Facility { .. }, EndpointInput::External { node }) => {
                let source = model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "source",
                    &edge.source,
                    &model_instances,
                    tag,
                );
                let target = external_endpoint_options(node, &source);
                (source, target)
            }
            (EndpointInput::External { .. }, EndpointInput::External { .. }) => unreachable!(
                "external-to-external requirements are rejected during model preparation"
            ),
        };

        let (arcs, incoming, outgoing) =
            grid_arcs(&mut solver, edge_index, input.width, input.height);
        let mut source_by_cell = vec![Vec::<DomainId>::new(); cell_count];
        let mut target_by_cell = vec![Vec::<DomainId>::new(); cell_count];
        for option in &source_options {
            source_by_cell[option.cell].push(option.selected);
        }
        for option in &target_options {
            target_by_cell[option.cell].push(option.selected);
        }

        for cell in 0..cell_count {
            let route_cell =
                solver.new_named_bounded_integer(0, 1, format!("route-{edge_index}-cell-{cell}"));
            match edge.transport {
                TransportKind::Belt => belt_route_cells_by_grid[cell].push(route_cell),
                TransportKind::Pipe => pipe_route_cells_by_grid[cell].push(route_cell),
            }

            let mut conservation = Vec::new();
            conservation.extend(outgoing[cell].iter().map(|variable| variable.scaled(1)));
            conservation.extend(incoming[cell].iter().map(|variable| variable.scaled(-1)));
            conservation.extend(
                source_by_cell[cell]
                    .iter()
                    .map(|variable| variable.scaled(-1)),
            );
            conservation.extend(
                target_by_cell[cell]
                    .iter()
                    .map(|variable| variable.scaled(1)),
            );
            solver
                .add_constraint(pumpkin_solver::equals(conservation, 0, tag))
                .post();

            post_at_most_one(&mut solver, incoming[cell].iter().copied(), tag);
            post_at_most_one(&mut solver, outgoing[cell].iter().copied(), tag);

            let mut route_definition = vec![route_cell.scaled(1)];
            route_definition.extend(outgoing[cell].iter().map(|variable| variable.scaled(-1)));
            route_definition.extend(
                target_by_cell[cell]
                    .iter()
                    .map(|variable| variable.scaled(-1)),
            );
            solver
                .add_constraint(pumpkin_solver::equals(route_definition, 0, tag))
                .post();
        }

        route_arc_variables.extend(arcs.iter().map(|arc| arc.selected));
        model_routes.push(ModelRoute {
            source_options,
            target_options,
            arcs,
        });
    }

    for cell in 0..cell_count {
        for layer in [
            &belt_route_cells_by_grid[cell],
            &pipe_route_cells_by_grid[cell],
        ] {
            let exclusion = occupancy[cell]
                .iter()
                .chain(layer.iter())
                .map(|variable| variable.scaled(1))
                .collect::<Vec<_>>();
            if exclusion.len() > 1 {
                solver
                    .add_constraint(pumpkin_solver::less_than_or_equals(exclusion, 1, tag))
                    .post();
            }
        }
    }

    let route_length =
        solver.new_named_bounded_integer(0, route_arc_variables.len() as i32, "total-route-length");
    let mut route_length_definition = route_arc_variables
        .iter()
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    route_length_definition.push(route_length.scaled(-1));
    solver
        .add_constraint(pumpkin_solver::equals(route_length_definition, 0, tag))
        .post();

    let mut brancher = solver.default_brancher();
    let mut resolver = ResolutionResolver::default();
    let callback = |_: &Solver,
                    _: SolutionReference,
                    _: &DefaultBrancher,
                    _: &ResolutionResolver|
     -> ControlFlow<()> { ControlFlow::Continue(()) };
    let mut termination = time_limit.map(TimeBudget::starting_now);
    let result = solver.optimise(
        &mut brancher,
        &mut termination,
        &mut resolver,
        LinearSatUnsat::new(OptimisationDirection::Minimise, route_length, callback),
    );

    match result {
        OptimisationResult::Optimal(solution) => extract_report(
            &solution,
            IntegratedLayoutStatus::Optimal,
            &input,
            &model_instances,
            &model_routes,
        ),
        OptimisationResult::Satisfiable(solution) | OptimisationResult::Stopped(solution, ()) => {
            extract_report(
                &solution,
                IntegratedLayoutStatus::Feasible,
                &input,
                &model_instances,
                &model_routes,
            )
        }
        OptimisationResult::Unsatisfiable => IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::Infeasible,
            IntegratedLayoutDiagnostic::error(
                "integrated-layout-infeasible",
                "/",
                None,
                "facility placement, port selection, and route constraints are infeasible",
            ),
        ),
        OptimisationResult::Unknown => IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::Unknown,
            IntegratedLayoutDiagnostic::error(
                "integrated-layout-unknown",
                "/",
                None,
                "solver terminated without a solution or proof",
            ),
        ),
    }
}

fn generate_candidates(
    solver: &mut Solver,
    instance: &InstanceInput,
    max_width: i32,
    max_height: i32,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for rotation in &instance.definition.allowed_rotations {
        let source_width = instance.definition.footprint.width as i32;
        let source_height = instance.definition.footprint.height as i32;
        let (width, height) = if matches!(rotation, 90 | 270) {
            (source_height, source_width)
        } else {
            (source_width, source_height)
        };
        if width > max_width || height > max_height {
            continue;
        }
        for y in 0..=(max_height - height) {
            for x in 0..=(max_width - width) {
                let port_connections = candidate_port_connections(
                    &instance.definition,
                    *rotation,
                    x,
                    y,
                    max_width,
                    max_height,
                );
                let occupied_cells = (y..(y + height))
                    .flat_map(|occupied_y| {
                        (x..(x + width))
                            .map(move |occupied_x| grid_index(occupied_x, occupied_y, max_width))
                    })
                    .collect();
                candidates.push(Candidate {
                    rotation: *rotation,
                    x,
                    y,
                    width,
                    height,
                    occupied_cells,
                    port_connections,
                    selected: solver.new_named_bounded_integer(
                        0,
                        1,
                        format!("place-{}-{rotation}-{x}-{y}", instance.id),
                    ),
                });
            }
        }
    }
    candidates
}

fn candidate_port_connections(
    definition: &FacilityDefinition,
    rotation: i64,
    origin_x: i32,
    origin_y: i32,
    max_width: i32,
    max_height: i32,
) -> BTreeMap<String, usize> {
    let mut connections = BTreeMap::new();
    for port in &definition.ports {
        let (position, edge) = rotate_port(
            &port.position,
            port.edge,
            rotation,
            definition.footprint.width,
            definition.footprint.height,
        );
        let port_x = origin_x + position.x as i32;
        let port_y = origin_y + position.y as i32;
        let (connection_x, connection_y) = match edge {
            FacilityPortEdge::North => (port_x, port_y - 1),
            FacilityPortEdge::East => (port_x + 1, port_y),
            FacilityPortEdge::South => (port_x, port_y + 1),
            FacilityPortEdge::West => (port_x - 1, port_y),
        };
        if connection_x < 0
            || connection_y < 0
            || connection_x >= max_width
            || connection_y >= max_height
        {
            continue;
        }
        connections.insert(
            port.id.clone(),
            grid_index(connection_x, connection_y, max_width),
        );
    }
    connections
}

fn rotate_port(
    position: &FacilityPortPosition,
    edge: FacilityPortEdge,
    rotation: i64,
    width: i64,
    height: i64,
) -> (FacilityPortPosition, FacilityPortEdge) {
    let position = match rotation {
        0 => position.clone(),
        90 => FacilityPortPosition {
            x: height - 1 - position.y,
            y: position.x,
        },
        180 => FacilityPortPosition {
            x: width - 1 - position.x,
            y: height - 1 - position.y,
        },
        270 => FacilityPortPosition {
            x: position.y,
            y: width - 1 - position.x,
        },
        _ => unreachable!("validated facility rotation"),
    };
    (position, edge.rotated_clockwise(rotation))
}

fn endpoint_options(
    solver: &mut Solver,
    edge_index: usize,
    endpoint_kind: &str,
    instance: &ModelInstance,
    ports: &[FacilityPortDefinition],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<EndpointOption> {
    let mut options = Vec::new();
    for (candidate_index, candidate) in instance.candidates.iter().enumerate() {
        let mut candidate_options = Vec::new();
        for port in ports {
            let Some(cell) = candidate.port_connections.get(&port.id).copied() else {
                continue;
            };
            let selected = solver.new_named_bounded_integer(
                0,
                1,
                format!(
                    "edge-{edge_index}-{endpoint_kind}-{}-{}-{candidate_index}",
                    instance.input.id, port.id
                ),
            );
            candidate_options.push(selected);
            options.push(EndpointOption {
                endpoint: IntegratedRouteEndpoint::Facility {
                    instance: instance.input.id.clone(),
                    port: port.id.clone(),
                },
                cell,
                selected,
                external_side: Some(port.edge.rotated_clockwise(candidate.rotation)),
            });
        }
        let mut definition = candidate_options
            .iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        definition.push(candidate.selected.scaled(-1));
        solver
            .add_constraint(pumpkin_solver::equals(definition, 0, tag))
            .post();
    }
    post_equals_one(solver, options.iter().map(|option| option.selected), tag);
    options
}

#[allow(clippy::too_many_arguments)]
fn model_facility_endpoint_options(
    solver: &mut Solver,
    edge_index: usize,
    endpoint_kind: &str,
    endpoint: &EndpointInput,
    instances: &[ModelInstance],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<EndpointOption> {
    match endpoint {
        EndpointInput::Facility { instance, ports } => {
            let instance = instances
                .iter()
                .find(|model_instance| model_instance.input.id == *instance)
                .expect("prepared endpoint instance exists");
            endpoint_options(solver, edge_index, endpoint_kind, instance, ports, tag)
        }
        EndpointInput::External { .. } => unreachable!("expected a facility endpoint"),
    }
}

fn external_endpoint_options(
    node: &str,
    facility_options: &[EndpointOption],
) -> Vec<EndpointOption> {
    facility_options
        .iter()
        .map(|option| EndpointOption {
            endpoint: IntegratedRouteEndpoint::External {
                node: node.to_string(),
                side: option
                    .external_side
                    .expect("facility endpoint option records its outward side"),
            },
            cell: option.cell,
            selected: option.selected,
            external_side: option.external_side,
        })
        .collect()
}

fn grid_arcs(
    solver: &mut Solver,
    edge_index: usize,
    width: i32,
    height: i32,
) -> (Vec<Arc>, Vec<Vec<DomainId>>, Vec<Vec<DomainId>>) {
    let cell_count = (width as usize) * (height as usize);
    let mut arcs = Vec::new();
    let mut incoming = vec![Vec::new(); cell_count];
    let mut outgoing = vec![Vec::new(); cell_count];
    for y in 0..height {
        for x in 0..width {
            let from = grid_index(x, y, width);
            for (to_x, to_y) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if to_x < 0 || to_y < 0 || to_x >= width || to_y >= height {
                    continue;
                }
                let to = grid_index(to_x, to_y, width);
                let selected = solver.new_named_bounded_integer(
                    0,
                    1,
                    format!("route-{edge_index}-arc-{from}-{to}"),
                );
                arcs.push(Arc { from, to, selected });
                outgoing[from].push(selected);
                incoming[to].push(selected);
            }
        }
    }
    (arcs, incoming, outgoing)
}

fn post_equals_one(
    solver: &mut Solver,
    variables: impl Iterator<Item = DomainId>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    solver
        .add_constraint(pumpkin_solver::equals(
            variables
                .map(|variable| variable.scaled(1))
                .collect::<Vec<_>>(),
            1,
            tag,
        ))
        .post();
}

fn post_at_most_one(
    solver: &mut Solver,
    variables: impl Iterator<Item = DomainId>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let terms = variables
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    if terms.len() > 1 {
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(terms, 1, tag))
            .post();
    }
}

fn extract_report(
    solution: &impl ProblemSolution,
    status: IntegratedLayoutStatus,
    input: &ModelInput,
    instances: &[ModelInstance],
    model_routes: &[ModelRoute],
) -> IntegratedLayoutReport {
    let mut placements = Vec::new();
    for instance in instances {
        let candidate = instance
            .candidates
            .iter()
            .find(|candidate| solution.get_integer_value(candidate.selected) == 1)
            .expect("exactly one placement candidate is selected");
        placements.push(FacilityPlacement {
            instance: instance.input.id.clone(),
            recipe: instance.input.recipe.clone(),
            facility: instance.input.facility.clone(),
            x: i64::from(candidate.x),
            y: i64::from(candidate.y),
            width: i64::from(candidate.width),
            height: i64::from(candidate.height),
            rotation: candidate.rotation,
        });
    }
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));

    let routes = input
        .edges
        .iter()
        .zip(model_routes)
        .map(|(edge, model_route)| {
            let source = selected_endpoint(solution, &model_route.source_options);
            let target = selected_endpoint(solution, &model_route.target_options);
            let cells = extract_path(
                solution,
                source.cell,
                target.cell,
                &model_route.arcs,
                input.width,
            );
            IntegratedRoute {
                requirement_id: edge.requirement_id.clone(),
                requirement_fingerprint: edge.requirement_fingerprint.clone(),
                source: source.endpoint.clone(),
                target: target.endpoint.clone(),
                item: edge.edge.item.clone(),
                rate: edge.edge.rate,
                transport: edge.transport,
                cells,
            }
        })
        .collect();

    let mut report = IntegratedLayoutReport {
        schema_version: INTEGRATED_LAYOUT_SCHEMA_VERSION,
        success: true,
        status,
        bounds: None,
        placements,
        logistics_components: Vec::new(),
        routes,
        phases: Vec::new(),
        diagnostics: vec![IntegratedLayoutDiagnostic::info(
            if status == IntegratedLayoutStatus::Optimal {
                "integrated-layout-optimal"
            } else {
                "integrated-layout-feasible"
            },
            if status == IntegratedLayoutStatus::Optimal {
                "facility placement, port selection, and routes are solved with proven minimum total route length"
            } else {
                "facility placement, port selection, and routing are feasible but not proven optimal"
            },
        )],
    };
    canonicalize_report_geometry(&mut report);
    report
}

pub(super) fn canonicalize_report_geometry(report: &mut IntegratedLayoutReport) {
    let mut minimum_x = i64::MAX;
    let mut minimum_y = i64::MAX;
    for placement in &report.placements {
        minimum_x = minimum_x.min(placement.x);
        minimum_y = minimum_y.min(placement.y);
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
    }
    if minimum_x == i64::MAX {
        report.bounds = Some(FacilityPlacementBounds {
            width: 0,
            height: 0,
        });
        return;
    }
    for placement in &mut report.placements {
        placement.x -= minimum_x;
        placement.y -= minimum_y;
    }
    for position in report
        .routes
        .iter_mut()
        .flat_map(|route| route.cells.iter_mut())
        .chain(
            report
                .logistics_components
                .iter_mut()
                .map(|component| &mut component.position),
        )
    {
        position.x -= minimum_x;
        position.y -= minimum_y;
    }
    let width = report
        .placements
        .iter()
        .map(|placement| placement.x + placement.width)
        .chain(
            report
                .routes
                .iter()
                .flat_map(|route| route.cells.iter().map(|cell| cell.x + 1)),
        )
        .chain(
            report
                .logistics_components
                .iter()
                .map(|component| component.position.x + 1),
        )
        .max()
        .unwrap_or(0);
    let height = report
        .placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .chain(
            report
                .routes
                .iter()
                .flat_map(|route| route.cells.iter().map(|cell| cell.y + 1)),
        )
        .chain(
            report
                .logistics_components
                .iter()
                .map(|component| component.position.y + 1),
        )
        .max()
        .unwrap_or(0);
    report.bounds = Some(FacilityPlacementBounds { width, height });
}

pub(super) fn frame_placements_for_routing(
    mut placements: Vec<FacilityPlacement>,
    hard_width: i64,
    hard_height: i64,
) -> Option<Vec<FacilityPlacement>> {
    if placements.is_empty() {
        return Some(placements);
    }
    let minimum_x = placements.iter().map(|placement| placement.x).min()?;
    let minimum_y = placements.iter().map(|placement| placement.y).min()?;
    let maximum_x = placements
        .iter()
        .map(|placement| placement.x + placement.width)
        .max()?;
    let maximum_y = placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .max()?;
    let delta_x = COORDINATE_ROUTING_FRAME - minimum_x;
    let delta_y = COORDINATE_ROUTING_FRAME - minimum_y;
    if maximum_x + delta_x + COORDINATE_ROUTING_FRAME > hard_width
        || maximum_y + delta_y + COORDINATE_ROUTING_FRAME > hard_height
    {
        return None;
    }
    for placement in &mut placements {
        placement.x += delta_x;
        placement.y += delta_y;
    }
    Some(placements)
}

fn selected_endpoint<'a>(
    solution: &impl ProblemSolution,
    options: &'a [EndpointOption],
) -> &'a EndpointOption {
    options
        .iter()
        .find(|option| solution.get_integer_value(option.selected) == 1)
        .expect("exactly one endpoint option is selected")
}

fn extract_path(
    solution: &impl ProblemSolution,
    source: usize,
    target: usize,
    arcs: &[Arc],
    width: i32,
) -> Vec<WorldGridPosition> {
    let mut next_by_cell = BTreeMap::new();
    for arc in arcs {
        if solution.get_integer_value(arc.selected) == 1 {
            next_by_cell.insert(arc.from, arc.to);
        }
    }
    let mut cells = vec![world_position(source, width)];
    let mut current = source;
    let mut seen = BTreeSet::from([source]);
    while current != target {
        current = *next_by_cell.get(&current).unwrap_or_else(|| {
            panic!(
                "solver route stops before target: source={source}, target={target}, current={current}, arcs={next_by_cell:?}"
            )
        });
        assert!(seen.insert(current), "solver route contains a cycle");
        cells.push(world_position(current, width));
    }
    cells
}

fn grid_index(x: i32, y: i32, width: i32) -> usize {
    (y as usize) * (width as usize) + (x as usize)
}

fn world_position(index: usize, width: i32) -> WorldGridPosition {
    WorldGridPosition {
        x: (index % width as usize) as i64,
        y: (index / width as usize) as i64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityCatalog, FacilityFootprint};
    use crate::logistics::{
        CardinalDirection, ItemCatalog, ItemDefinition, LogisticsComponentCatalog,
        LogisticsComponentDefinition, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
        SUPPORTED_LOGISTICS_COMPONENT_CATALOG_SCHEMA_VERSION,
        SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION, TransportCapacity, TransportCatalog,
        TransportDefinition,
    };

    fn facility(
        id: &str,
        port_id: &str,
        direction: FacilityPortDirection,
        edge: FacilityPortEdge,
    ) -> FacilityDefinition {
        FacilityDefinition {
            id: id.to_string(),
            footprint: FacilityFootprint {
                width: 1,
                height: 1,
            },
            allowed_rotations: vec![0],
            ports: vec![FacilityPortDefinition {
                id: port_id.to_string(),
                direction,
                transport: TransportKind::Belt,
                position: FacilityPortPosition { x: 0, y: 0 },
                edge,
            }],
        }
    }

    fn facility_with_ports(
        id: &str,
        ports: &[(&str, FacilityPortDirection, FacilityPortEdge)],
    ) -> FacilityDefinition {
        FacilityDefinition {
            id: id.to_string(),
            footprint: FacilityFootprint {
                width: 1,
                height: 1,
            },
            allowed_rotations: vec![0],
            ports: ports
                .iter()
                .map(|(port_id, direction, edge)| FacilityPortDefinition {
                    id: (*port_id).to_string(),
                    direction: *direction,
                    transport: TransportKind::Belt,
                    position: FacilityPortPosition { x: 0, y: 0 },
                    edge: *edge,
                })
                .collect(),
        }
    }

    fn facility_with_typed_ports(
        id: &str,
        ports: &[(&str, FacilityPortDirection, TransportKind, FacilityPortEdge)],
    ) -> FacilityDefinition {
        FacilityDefinition {
            id: id.to_string(),
            footprint: FacilityFootprint {
                width: 1,
                height: 1,
            },
            allowed_rotations: vec![0],
            ports: ports
                .iter()
                .map(
                    |(port_id, direction, transport, edge)| FacilityPortDefinition {
                        id: (*port_id).to_string(),
                        direction: *direction,
                        transport: *transport,
                        position: FacilityPortPosition { x: 0, y: 0 },
                        edge: *edge,
                    },
                )
                .collect(),
        }
    }

    fn wiring() -> FacilityInstanceWiringReport {
        FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:source#1".to_string(),
                    recipe: "source".to_string(),
                    facility: "source-machine".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:target#1".to_string(),
                    recipe: "target".to_string(),
                    facility: "target-machine".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
            ],
            edges: vec![FacilityInstanceWiringEdge::original(
                "recipe:source#1",
                "recipe:target#1",
                "intermediate",
                "part",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            )],
            diagnostics: Vec::new(),
        }
    }

    fn transport_catalog() -> ValidatedTransportCatalog {
        ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
            schema_version: SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION,
            transports: vec![
                TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 2,
                        duration_ms: 1000,
                    },
                },
                TransportDefinition {
                    kind: TransportKind::Pipe,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 500,
                    },
                },
            ],
        })
        .expect("transport catalog should validate")
    }

    fn logistics_component_catalog() -> ValidatedLogisticsComponentCatalog {
        let mut components = Vec::new();
        for transport in [TransportKind::Belt, TransportKind::Pipe] {
            for kind in [
                LogisticsComponentKind::Splitter,
                LogisticsComponentKind::Converger,
                LogisticsComponentKind::Bridge,
            ] {
                let (input_directions, output_directions) = match kind {
                    LogisticsComponentKind::Splitter => (
                        vec![CardinalDirection::North],
                        vec![
                            CardinalDirection::East,
                            CardinalDirection::South,
                            CardinalDirection::West,
                        ],
                    ),
                    LogisticsComponentKind::Converger => (
                        vec![
                            CardinalDirection::North,
                            CardinalDirection::East,
                            CardinalDirection::West,
                        ],
                        vec![CardinalDirection::South],
                    ),
                    LogisticsComponentKind::Bridge => (
                        vec![
                            CardinalDirection::North,
                            CardinalDirection::East,
                            CardinalDirection::South,
                            CardinalDirection::West,
                        ],
                        vec![
                            CardinalDirection::North,
                            CardinalDirection::East,
                            CardinalDirection::South,
                            CardinalDirection::West,
                        ],
                    ),
                };
                components.push(LogisticsComponentDefinition {
                    id: format!("{transport:?}-{kind:?}").to_lowercase(),
                    transport,
                    kind,
                    footprint: FacilityFootprint {
                        width: 1,
                        height: 1,
                    },
                    allowed_rotations: vec![0, 90, 180, 270],
                    input_directions,
                    output_directions,
                    capacity: TransportCapacity {
                        quantity: 2,
                        duration_ms: 1000,
                    },
                });
            }
        }
        ValidatedLogisticsComponentCatalog::try_from_catalog(LogisticsComponentCatalog {
            schema_version: SUPPORTED_LOGISTICS_COMPONENT_CATALOG_SCHEMA_VERSION,
            components,
        })
        .expect("logistics component catalog should validate")
    }

    fn catalogs() -> (
        ValidatedFacilityCatalog,
        ValidatedItemCatalog,
        ValidatedTransportCatalog,
    ) {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility(
                    "source-machine",
                    "output",
                    FacilityPortDirection::Output,
                    FacilityPortEdge::East,
                ),
                facility(
                    "target-machine",
                    "input",
                    FacilityPortDirection::Input,
                    FacilityPortEdge::West,
                ),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![ItemDefinition {
                id: "part".to_string(),
                transport: TransportKind::Belt,
            }],
        })
        .expect("item catalog should validate");
        (facilities, items, transport_catalog())
    }

    #[test]
    fn jointly_places_selects_ports_and_routes_one_edge() {
        let (facilities, items, transports) = catalogs();
        let wiring = wiring();
        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transports,
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 1,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.schema_version, INTEGRATED_LAYOUT_SCHEMA_VERSION);
        assert_eq!(
            serde_json::to_value(&report).expect("layout report should serialize")["schema_version"],
            INTEGRATED_LAYOUT_SCHEMA_VERSION
        );
        assert_eq!(report.status, IntegratedLayoutStatus::Optimal);
        assert_eq!(report.routes.len(), 1);
        assert_eq!(
            report.routes[0].requirement_id,
            format!("{}:lane:0000", wiring.edges[0].id)
        );
        assert_eq!(
            report.routes[0].requirement_fingerprint.source,
            wiring.edges[0].source
        );
        assert_eq!(
            report.routes[0].requirement_fingerprint.target,
            wiring.edges[0].target
        );
        assert_eq!(report.routes[0].transport, TransportKind::Belt);
        assert!(matches!(
            &report.routes[0].source,
            IntegratedRouteEndpoint::Facility { port, .. } if port == "output"
        ));
        assert!(matches!(
            &report.routes[0].target,
            IntegratedRouteEndpoint::Facility { port, .. } if port == "input"
        ));
        assert_eq!(report.routes[0].cells.len(), 1);
        assert_eq!(report.placements.len(), 2);
    }

    #[test]
    fn witness_validation_joins_routes_by_stable_requirement_id() {
        let (facilities, items, transports) = catalogs();
        let components = logistics_component_catalog();
        let wiring = wiring();
        let request = FacilityPlacementRequest {
            schema_version: 2,
            max_width: 4,
            max_height: 1,
        };
        let input = prepare_model(&wiring, &facilities, &items, &transports, &request)
            .expect("fixture model should prepare");
        let report = solve_integrated_layout(&wiring, &facilities, &items, &transports, &request);
        assert!(report.success, "{:#?}", report.diagnostics);

        let mut duplicate = report.clone();
        duplicate.routes.push(duplicate.routes[0].clone());
        let error = witness::validate(&input, &components, &duplicate)
            .expect_err("duplicate requirement IDs must fail");
        assert_eq!(error.path, "/routes/1/requirement_id");

        let mut missing = report.clone();
        missing.routes.clear();
        let error = witness::validate(&input, &components, &missing)
            .expect_err("missing requirement IDs must fail");
        assert_eq!(error.path, "/routes");
        assert!(error.message.contains("is missing"));

        let mut unexpected = report.clone();
        unexpected.routes[0].requirement_id = "unexpected:lane:0000".to_string();
        let error = witness::validate(&input, &components, &unexpected)
            .expect_err("unexpected requirement IDs must fail");
        assert_eq!(error.path, "/routes/0/requirement_id");

        let mut mismatched = report;
        mismatched.routes[0].requirement_fingerprint.item = "different-item".to_string();
        let error = witness::validate(&input, &components, &mismatched)
            .expect_err("mismatched requirement fingerprints must fail");
        assert_eq!(error.path, "/routes/0/requirement_fingerprint");
    }

    #[test]
    fn coordinate_feasibility_stops_at_a_valid_routed_witness() {
        let (facilities, items, transports) = catalogs();
        let report = construct_coordinate_integrated_layout_with_time_limit(
            &wiring(),
            &facilities,
            &items,
            &transports,
            &logistics_component_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 40,
                max_height: 30,
            },
            Duration::from_secs(1),
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.status, IntegratedLayoutStatus::Feasible);
        assert_eq!(report.placements.len(), 2);
        assert_eq!(report.routes.len(), 1);
        assert_eq!(
            report.diagnostics[0].code,
            "coordinate-integrated-layout-feasible"
        );
        assert_eq!(
            report.diagnostics[1].code,
            "routing-network-topology-planned"
        );
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "parallel-portfolio-selected")
        );
    }

    #[test]
    fn portfolio_uses_conservative_distinct_width_caps() {
        assert_eq!(portfolio_widths(500, 4), vec![500, 450, 400, 375]);
        assert_eq!(portfolio_widths(500, 2), vec![500, 450]);
        assert_eq!(portfolio_widths(1, 4), vec![1]);
    }

    #[test]
    fn portfolio_prefers_shorter_routes_before_compactness() {
        let compact = LayoutScore {
            total_route_cells: 12_046,
            total_route_turns: 800,
            used_bounding_box_area: 20_176,
            maximum_used_side: 388,
            physical_transport_tiles: 12_000,
            logistics_component_count: 793,
            moved_prior_facility_count: 0,
            total_prior_facility_manhattan_displacement: 0,
            rotation_change_count: 0,
        };
        let wide = LayoutScore {
            total_route_cells: 10_198,
            total_route_turns: 600,
            used_bounding_box_area: 21_252,
            maximum_used_side: 483,
            physical_transport_tiles: 10_000,
            logistics_component_count: 618,
            moved_prior_facility_count: 0,
            total_prior_facility_manhattan_displacement: 0,
            rotation_change_count: 0,
        };

        assert!(wide < compact);
    }

    #[test]
    fn portfolio_prefers_fewer_turns_at_equal_route_length() {
        let straighter = LayoutScore {
            total_route_cells: 10_000,
            total_route_turns: 40,
            used_bounding_box_area: 20_000,
            maximum_used_side: 400,
            physical_transport_tiles: 9_900,
            logistics_component_count: 80,
            moved_prior_facility_count: 0,
            total_prior_facility_manhattan_displacement: 0,
            rotation_change_count: 0,
        };
        let shorter = LayoutScore {
            total_route_cells: 10_000,
            total_route_turns: 50,
            used_bounding_box_area: 20_000,
            maximum_used_side: 400,
            physical_transport_tiles: 9_800,
            logistics_component_count: 70,
            moved_prior_facility_count: 0,
            total_prior_facility_manhattan_displacement: 0,
            rotation_change_count: 0,
        };

        assert!(straighter < shorter);
    }

    #[test]
    fn rejects_facility_area_above_hard_layout_bounds_before_search() {
        let (facilities, items, transports) = catalogs();
        let report = solve_integrated_layout(
            &wiring(),
            &facilities,
            &items,
            &transports,
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 1,
                max_height: 1,
            },
        );

        assert!(!report.success);
        assert_eq!(report.status, IntegratedLayoutStatus::Infeasible);
        assert_eq!(
            report.diagnostics[0].code,
            "facility-area-exceeds-layout-bounds"
        );
        assert!(report.diagnostics[0].message.contains("2 cells"));
        assert!(report.diagnostics[0].message.contains("1 cells"));
    }

    #[test]
    fn handles_grid_cells_without_endpoint_candidates() {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility(
                    "source-machine",
                    "output",
                    FacilityPortDirection::Output,
                    FacilityPortEdge::East,
                ),
                facility(
                    "target-machine",
                    "input",
                    FacilityPortDirection::Input,
                    FacilityPortEdge::East,
                ),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![ItemDefinition {
                id: "part".to_string(),
                transport: TransportKind::Belt,
            }],
        })
        .expect("item catalog should validate");

        let report = solve_integrated_layout(
            &wiring(),
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 2,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.routes.len(), 1);
    }

    #[test]
    fn jointly_routes_multiple_edges_without_shared_cells() {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility(
                    "source-machine",
                    "output",
                    FacilityPortDirection::Output,
                    FacilityPortEdge::East,
                ),
                facility_with_ports(
                    "middle-machine",
                    &[
                        (
                            "input",
                            FacilityPortDirection::Input,
                            FacilityPortEdge::West,
                        ),
                        (
                            "output",
                            FacilityPortDirection::Output,
                            FacilityPortEdge::East,
                        ),
                    ],
                ),
                facility(
                    "target-machine",
                    "input",
                    FacilityPortDirection::Input,
                    FacilityPortEdge::West,
                ),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "part-a".to_string(),
                    transport: TransportKind::Belt,
                },
                ItemDefinition {
                    id: "part-b".to_string(),
                    transport: TransportKind::Belt,
                },
            ],
        })
        .expect("item catalog should validate");
        let mut wiring = wiring();
        wiring.nodes.insert(
            1,
            FacilityInstanceWiringNode::Facility {
                id: "recipe:middle#1".to_string(),
                recipe: "middle".to_string(),
                facility: "middle-machine".to_string(),
                index: 1,
                runs_per_second: Rate {
                    numerator: 1,
                    denominator: 1,
                },
                work_seconds_per_second: Rate {
                    numerator: 1,
                    denominator: 1,
                },
                unused_capacity: Rate::zero(),
            },
        );
        wiring.edges = vec![
            FacilityInstanceWiringEdge::original(
                "recipe:source#1",
                "recipe:middle#1",
                "intermediate",
                "part-a",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
            FacilityInstanceWiringEdge::original(
                "recipe:middle#1",
                "recipe:target#1",
                "intermediate",
                "part-b",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
        ];

        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 7,
                max_height: 1,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.status, IntegratedLayoutStatus::Optimal);
        assert_eq!(report.placements.len(), 3);
        assert_eq!(report.routes.len(), 2);
        assert_eq!(
            report
                .routes
                .iter()
                .map(|route| route.cells.len())
                .sum::<usize>(),
            2
        );
        let route_cells = report
            .routes
            .iter()
            .flat_map(|route| route.cells.iter().map(|cell| (cell.x, cell.y)))
            .collect::<BTreeSet<_>>();
        assert_eq!(route_cells.len(), 2);
    }

    #[test]
    fn allows_belt_and_pipe_routes_to_share_a_horizontal_cell() {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility_with_typed_ports(
                    "source-machine",
                    &[
                        (
                            "belt-output",
                            FacilityPortDirection::Output,
                            TransportKind::Belt,
                            FacilityPortEdge::East,
                        ),
                        (
                            "pipe-output",
                            FacilityPortDirection::Output,
                            TransportKind::Pipe,
                            FacilityPortEdge::East,
                        ),
                    ],
                ),
                facility_with_typed_ports(
                    "target-machine",
                    &[
                        (
                            "belt-input",
                            FacilityPortDirection::Input,
                            TransportKind::Belt,
                            FacilityPortEdge::West,
                        ),
                        (
                            "pipe-input",
                            FacilityPortDirection::Input,
                            TransportKind::Pipe,
                            FacilityPortEdge::West,
                        ),
                    ],
                ),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "solid".to_string(),
                    transport: TransportKind::Belt,
                },
                ItemDefinition {
                    id: "liquid".to_string(),
                    transport: TransportKind::Pipe,
                },
            ],
        })
        .expect("item catalog should validate");
        let mut wiring = wiring();
        wiring.edges = vec![
            FacilityInstanceWiringEdge::original(
                "recipe:source#1",
                "recipe:target#1",
                "intermediate",
                "solid",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
            FacilityInstanceWiringEdge::original(
                "recipe:source#1",
                "recipe:target#1",
                "intermediate",
                "liquid",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
        ];

        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 1,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.status, IntegratedLayoutStatus::Optimal);
        assert_eq!(report.routes.len(), 2);
        assert_ne!(report.routes[0].transport, report.routes[1].transport);
        assert_eq!(report.routes[0].cells, report.routes[1].cells);
    }

    #[test]
    fn rejects_two_plain_routes_sharing_the_same_transport_layer() {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility(
                    "source-machine",
                    "output",
                    FacilityPortDirection::Output,
                    FacilityPortEdge::East,
                ),
                facility(
                    "target-machine",
                    "input",
                    FacilityPortDirection::Input,
                    FacilityPortEdge::West,
                ),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "solid-a".to_string(),
                    transport: TransportKind::Belt,
                },
                ItemDefinition {
                    id: "solid-b".to_string(),
                    transport: TransportKind::Belt,
                },
            ],
        })
        .expect("item catalog should validate");
        let mut wiring = wiring();
        wiring.edges = vec![
            FacilityInstanceWiringEdge::original(
                "recipe:source#1",
                "recipe:target#1",
                "intermediate",
                "solid-a",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
            FacilityInstanceWiringEdge::original(
                "recipe:source#1",
                "recipe:target#1",
                "intermediate",
                "solid-b",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
        ];

        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 1,
            },
        );

        assert!(!report.success);
        assert_eq!(report.status, IntegratedLayoutStatus::Infeasible);
    }

    #[test]
    fn jointly_routes_a_two_facility_cycle() {
        let cycle_ports = [
            (
                "input",
                FacilityPortDirection::Input,
                FacilityPortEdge::West,
            ),
            (
                "output",
                FacilityPortDirection::Output,
                FacilityPortEdge::East,
            ),
        ];
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility_with_ports("planter", &cycle_ports),
                facility_with_ports("seed-collector", &cycle_ports),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "crop".to_string(),
                    transport: TransportKind::Belt,
                },
                ItemDefinition {
                    id: "seed".to_string(),
                    transport: TransportKind::Belt,
                },
            ],
        })
        .expect("item catalog should validate");
        let wiring = FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:grow#1".to_string(),
                    recipe: "grow".to_string(),
                    facility: "planter".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:collect#1".to_string(),
                    recipe: "collect".to_string(),
                    facility: "seed-collector".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
            ],
            edges: vec![
                FacilityInstanceWiringEdge::original(
                    "recipe:grow#1",
                    "recipe:collect#1",
                    "intermediate",
                    "crop",
                    Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                ),
                FacilityInstanceWiringEdge::original(
                    "recipe:collect#1",
                    "recipe:grow#1",
                    "intermediate",
                    "seed",
                    Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                ),
            ],
            diagnostics: Vec::new(),
        };

        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 6,
                max_height: 2,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.routes.len(), 2);
        let route_cells = report
            .routes
            .iter()
            .flat_map(|route| route.cells.iter().map(|cell| (cell.x, cell.y)))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            route_cells.len(),
            report
                .routes
                .iter()
                .map(|route| route.cells.len())
                .sum::<usize>()
        );
    }

    #[test]
    fn routes_external_input_and_output_as_minimal_dangling_connections() {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![facility_with_ports(
                "processor",
                &[
                    (
                        "input",
                        FacilityPortDirection::Input,
                        FacilityPortEdge::West,
                    ),
                    (
                        "output",
                        FacilityPortDirection::Output,
                        FacilityPortEdge::East,
                    ),
                ],
            )],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "ore".to_string(),
                    transport: TransportKind::Belt,
                },
                ItemDefinition {
                    id: "product".to_string(),
                    transport: TransportKind::Belt,
                },
            ],
        })
        .expect("item catalog should validate");
        let mut wiring = FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![
                FacilityInstanceWiringNode::External {
                    id: "external:ore".to_string(),
                    item: "ore".to_string(),
                },
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:process#1".to_string(),
                    recipe: "process".to_string(),
                    facility: "processor".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
                FacilityInstanceWiringNode::Target {
                    id: "target:product".to_string(),
                    item: "product".to_string(),
                },
            ],
            edges: vec![
                FacilityInstanceWiringEdge::original(
                    "external:ore",
                    "recipe:process#1",
                    "external-input",
                    "ore",
                    Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                ),
                FacilityInstanceWiringEdge::original(
                    "recipe:process#1",
                    "target:product",
                    "target",
                    "product",
                    Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                ),
            ],
            diagnostics: Vec::new(),
        };

        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 5,
                max_height: 1,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.routes.len(), 2);
        assert!(report.routes.iter().all(|route| route.cells.len() == 1));
        assert_eq!(
            report.bounds,
            Some(FacilityPlacementBounds {
                width: 3,
                height: 1,
            })
        );
        assert!(matches!(
            &report.routes[0].source,
            IntegratedRouteEndpoint::External { node, side: FacilityPortEdge::West }
                if node == "external:ore"
        ));
        assert!(matches!(
            &report.routes[0].target,
            IntegratedRouteEndpoint::Facility { instance, port }
                if instance == "recipe:process#1" && port == "input"
        ));
        assert!(matches!(
            &report.routes[1].source,
            IntegratedRouteEndpoint::Facility { instance, port }
                if instance == "recipe:process#1" && port == "output"
        ));
        assert!(matches!(
            &report.routes[1].target,
            IntegratedRouteEndpoint::External { node, side: FacilityPortEdge::East }
                if node == "target:product"
        ));

        let FacilityInstanceWiringNode::External { item, .. } = &mut wiring.nodes[0] else {
            unreachable!("the first test node is external")
        };
        *item = "product".to_string();
        let mismatch = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 5,
                max_height: 1,
            },
        );
        assert!(!mismatch.success);
        assert_eq!(mismatch.diagnostics[0].code, "external-item-mismatch");
    }

    #[test]
    fn rejects_item_port_transport_mismatch() {
        let (facilities, _, _) = catalogs();
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![ItemDefinition {
                id: "part".to_string(),
                transport: TransportKind::Pipe,
            }],
        })
        .expect("item catalog should validate");

        let report = solve_integrated_layout(
            &wiring(),
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 1,
            },
        );

        assert!(!report.success);
        assert_eq!(report.diagnostics[0].code, "missing-compatible-port");
    }

    #[test]
    fn splits_route_rate_above_transport_capacity() {
        let (facilities, items, _) = catalogs();
        let transports = ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
            schema_version: SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION,
            transports: vec![
                TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 2000,
                    },
                },
                TransportDefinition {
                    kind: TransportKind::Pipe,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 500,
                    },
                },
            ],
        })
        .expect("transport catalog should validate");

        let input = prepare_model(
            &wiring(),
            &facilities,
            &items,
            &transports,
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 1,
            },
        )
        .expect("capacity splitting should prepare a valid model");

        assert_eq!(input.edges.len(), 2);
        let original_edge_id = wiring().edges[0].id.clone();
        assert_eq!(
            input.edges[0].requirement_id,
            format!("{original_edge_id}:lane:0000")
        );
        assert_eq!(
            input.edges[1].requirement_id,
            format!("{original_edge_id}:lane:0001")
        );
        assert!(input.edges.iter().all(|edge| {
            edge.edge.rate
                == Rate {
                    numerator: 1,
                    denominator: 2,
                }
        }));
    }
}
