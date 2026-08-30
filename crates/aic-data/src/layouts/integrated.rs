use std::collections::BTreeSet;
use std::thread;
use std::time::{Duration, Instant};

use crate::facilities::{FacilityPortEdge, ValidatedFacilityCatalog};
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementRequest, FacilityPlacementStatus,
};
use crate::logistics::{
    LogisticsComponentKind, TransportKind, ValidatedItemCatalog,
    ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::WorldGridPosition;
use super::placement::solve_facility_placement_feasibly_with_time_limit;
#[cfg(test)]
use crate::facilities::{
    FacilityDefinition, FacilityPortDefinition, FacilityPortDirection, FacilityPortPosition,
};
#[cfg(test)]
use crate::recipes::{
    FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
    FacilityInstanceWiringNode, Rate,
};

mod budget;
mod exact;
mod extension;
mod geometry;
mod html;
mod iterative;
mod model;
mod neighborhood;
mod networks;
mod optimization;
mod report;
mod retained;
mod score;
mod sparse;
mod witness;

use exact::solve;
pub use extension::{
    IncumbentExtensionCounts, IncumbentExtensionResult, PhaseIncumbent, extend_phase_incumbent,
};
use geometry::{candidate_port_connections, grid_index, world_position};
pub use html::{render_integrated_layout_html, render_integrated_layout_html_with_localization};
pub use iterative::{
    construct_iterative_scc_layout, construct_iterative_scc_layout_with_cancellation,
};
use model::{
    EdgeInput, EndpointInput, InstanceInput, ModelInput, prepare_model, required_facility_area,
};
pub use optimization::{
    CANDIDATE_POLICY_TABLE_SCHEMA_VERSION, CandidatePolicy, CandidatePolicyTable,
    ITERATIVE_OPTIMIZATION_CONFIG_SCHEMA_VERSION, IterativeOptimizationConfig,
    OptimizationConfigDiagnostic, PlacementPolicy, RoutingOrderPolicy,
    validate_candidate_policy_table, validate_iterative_optimization_config,
};
pub use report::{
    CandidateCounts, ExactModelMetrics, ExactProofStatus, ExactSolveReport, ExactTerminationReason,
    ExactValidationStatus, FacilityChangeCounts, INTEGRATED_LAYOUT_SCHEMA_VERSION,
    IncumbentProvenance, IntegratedLayoutDiagnostic, IntegratedLayoutIncumbentSummary,
    IntegratedLayoutNeighborhoodReport, IntegratedLayoutPhase, IntegratedLayoutPhaseAttempt,
    IntegratedLayoutPhaseOptimization, IntegratedLayoutReport, IntegratedLayoutStatus,
    IntegratedRoute, IntegratedRouteEndpoint, LayoutScoreDelta, OptimizationProofStatus,
    OptimizationTerminationReason, PhaseElapsedMilliseconds, PlacedLogisticsComponent,
    RouteChangeCounts, RouteRequirementFingerprint,
};
pub use retained::{
    CUMULATIVE_GRAPH_KEY_SCHEMA_VERSION, CumulativeGraphFingerprint, CumulativeGraphKey,
    EndpointPortSelection, FacilityGraphRecord, GridCellKey, RequirementGraphRecord,
    RetainedComponent, RetainedOccupant, RetainedRoutingResult, RetainedRoutingState,
    RoutingConflict, SelectedPortAssignment,
};
pub use score::{CandidateRank, DeterministicCandidateKey, LayoutScore, RefinementKind};

const PRODUCTION_FACILITY_GAP: i64 = 1;
const COORDINATE_ROUTING_FRAME: i64 = 1;

pub fn solve_integrated_layout(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
) -> IntegratedLayoutReport {
    solve_integrated_layout_with_optional_time_limit(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        None,
    )
}

pub fn solve_integrated_layout_with_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Duration,
) -> IntegratedLayoutReport {
    solve_integrated_layout_with_optional_time_limit(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
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

#[allow(clippy::too_many_arguments)]
pub fn reroute_integrated_layout_subset(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    prior_report: &IntegratedLayoutReport,
    invalidated_requirement_ids: &BTreeSet<String>,
    time_limit: Duration,
) -> RetainedRoutingResult {
    let input = match prepare_model(instance_wiring, facilities, items, transports, request) {
        Ok(input) => input,
        Err(diagnostic) => return failed_retained_result(diagnostic),
    };
    if let Err(diagnostic) = witness::validate(&input, logistics_components, prior_report) {
        return failed_retained_result(diagnostic);
    }
    let retained = match RetainedRoutingState::from_validated_report(&input, prior_report) {
        Ok(retained) => retained,
        Err(diagnostic) => return failed_retained_result(diagnostic),
    };
    let deadline = Instant::now()
        .checked_add(time_limit)
        .unwrap_or_else(Instant::now);
    sparse::construct_from_retained(
        input,
        logistics_components,
        prior_report.placements.clone(),
        &retained,
        invalidated_requirement_ids,
        deadline,
    )
}

fn failed_retained_result(diagnostic: IntegratedLayoutDiagnostic) -> RetainedRoutingResult {
    RetainedRoutingResult {
        report: IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic),
        invalidated_requirement_ids: Vec::new(),
        reused_requirement_ids: Vec::new(),
        conflict: None,
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
    logistics_components: &ValidatedLogisticsComponentCatalog,
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
                    solve(input, logistics_components, time_limit)
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
            &logistics_component_catalog(),
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
        let exact = report.exact.expect("exact solve metrics should be present");
        assert_eq!(exact.formulation, "joint-placement-routing-v1");
        assert_eq!(exact.model.facility_count, 2);
        assert_eq!(exact.model.route_requirement_count, 1);
        assert_eq!(exact.model.grid_cell_count, 4);
        assert_eq!(exact.model.route_cell_variables, 4);
        assert_eq!(exact.model.route_arc_variables, 6);
        assert_eq!(exact.model.route_order_variables, 4);
        assert_eq!(exact.model.acyclicity_constraints, 6);
        assert_eq!(exact.objective_route_cells, Some(1));
        assert_eq!(exact.proof, ExactProofStatus::ProvenOptimal);
        assert_eq!(exact.validation, ExactValidationStatus::Passed);
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
        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transports,
            &components,
            &request,
        );
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
    fn builds_collision_checked_retained_state_from_a_valid_witness() {
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
        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transports,
            &components,
            &request,
        );
        witness::validate(&input, &components, &report).expect("fixture witness should validate");

        let state = retained::RetainedRoutingState::from_validated_report(&input, &report)
            .expect("valid witness should produce retained state");
        assert_eq!(state.retained_routes.len(), 1);
        assert_eq!(state.selected_ports.len(), 1);
        assert!(state.invalidated_requirement_ids.is_empty());
        assert_eq!(state.graph_fingerprint.sha256_hex.len(), 64);
        assert_eq!(
            state
                .occupied_cells_by_transport
                .get(&TransportKind::Belt)
                .expect("belt occupancy should exist")
                .len(),
            1
        );
    }

    #[test]
    fn empty_subset_routing_reuses_the_complete_valid_witness() {
        let (facilities, items, transports) = catalogs();
        let components = logistics_component_catalog();
        let wiring = wiring();
        let request = FacilityPlacementRequest {
            schema_version: 2,
            max_width: 4,
            max_height: 1,
        };
        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transports,
            &components,
            &request,
        );
        let result = reroute_integrated_layout_subset(
            &wiring,
            &facilities,
            &items,
            &transports,
            &components,
            &request,
            &report,
            &BTreeSet::new(),
            Duration::from_secs(1),
        );

        assert!(result.report.success, "{:#?}", result.report.diagnostics);
        assert!(result.invalidated_requirement_ids.is_empty());
        assert_eq!(
            result.reused_requirement_ids,
            vec![report.routes[0].requirement_id.clone()]
        );
        assert_eq!(result.report.placements, report.placements);
        assert_eq!(result.report.routes, report.routes);
        assert_eq!(
            result.report.logistics_components,
            report.logistics_components
        );
    }

    #[test]
    fn moving_a_facility_invalidates_and_reroutes_its_incident_requirement() {
        let (facilities, items, transports) = catalogs();
        let components = logistics_component_catalog();
        let wiring = wiring();
        let request = FacilityPlacementRequest {
            schema_version: 2,
            max_width: 40,
            max_height: 30,
        };
        let input = prepare_model(&wiring, &facilities, &items, &transports, &request)
            .expect("fixture model should prepare");
        let report = construct_coordinate_integrated_layout_with_time_limit(
            &wiring,
            &facilities,
            &items,
            &transports,
            &components,
            &request,
            Duration::from_secs(1),
        );
        let retained = retained::RetainedRoutingState::from_validated_report(&input, &report)
            .expect("fixture retained state should build");
        let mut moved = report.placements.clone();
        let source = moved
            .iter_mut()
            .find(|placement| placement.instance == "recipe:source#1")
            .expect("source fixture placement should exist");
        source.y += 2;
        let result = sparse::construct_from_retained(
            input,
            &components,
            moved,
            &retained,
            &BTreeSet::new(),
            Instant::now() + Duration::from_secs(1),
        );

        assert!(result.report.success, "{:#?}", result.report.diagnostics);
        assert_eq!(
            result.invalidated_requirement_ids,
            vec![report.routes[0].requirement_id.clone()]
        );
        assert!(result.reused_requirement_ids.is_empty());
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
            &logistics_component_catalog(),
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
            &logistics_component_catalog(),
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

        let transports = transport_catalog();
        let request = FacilityPlacementRequest {
            schema_version: 2,
            max_width: 7,
            max_height: 1,
        };
        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transports,
            &logistics_component_catalog(),
            &request,
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

        let invalidated = [report.routes[0].requirement_id.clone()]
            .into_iter()
            .collect::<BTreeSet<_>>();
        let retained_route = report.routes[1].clone();
        let result = reroute_integrated_layout_subset(
            &wiring,
            &facilities,
            &items,
            &transports,
            &logistics_component_catalog(),
            &request,
            &report,
            &invalidated,
            Duration::from_secs(1),
        );
        assert!(result.report.success, "{:#?}", result.report.diagnostics);
        assert_eq!(
            result.invalidated_requirement_ids,
            invalidated.into_iter().collect::<Vec<_>>()
        );
        assert_eq!(
            result.reused_requirement_ids,
            vec![retained_route.requirement_id.clone()]
        );
        assert_eq!(
            result
                .report
                .routes
                .iter()
                .find(|route| route.requirement_id == retained_route.requirement_id)
                .expect("retained route should remain in the complete witness"),
            &retained_route
        );
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
            &logistics_component_catalog(),
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
            &logistics_component_catalog(),
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
            &logistics_component_catalog(),
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
            &logistics_component_catalog(),
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
            &logistics_component_catalog(),
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
            &logistics_component_catalog(),
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
