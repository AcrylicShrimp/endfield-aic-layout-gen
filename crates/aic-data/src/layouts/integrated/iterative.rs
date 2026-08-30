use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::growth::plan_facility_growth;
use crate::layouts::placement::{
    solve_anchored_facility_placement_with_time_limit,
    solve_facility_placement_feasibly_with_time_limit,
};
use crate::layouts::{FacilityPlacement, FacilityPlacementRequest, FacilityPlacementStatus};
use crate::logistics::{
    LogisticsComponentKind, ValidatedItemCatalog, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode, FacilityInstanceWiringReport,
};

use super::{
    COORDINATE_ROUTING_CLEARANCE, IntegratedLayoutDiagnostic, IntegratedLayoutPhase,
    IntegratedLayoutPhaseAttempt, IntegratedLayoutReport, IntegratedLayoutStatus, prepare_model,
    route_turn_count, sparse,
};

const ANCHOR_RADII: [i64; 3] = [0, 4, 12];

#[allow(clippy::too_many_arguments)]
pub fn construct_iterative_scc_layout_with_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Duration,
) -> IntegratedLayoutReport {
    let growth = plan_facility_growth(instance_wiring);
    if !growth.success {
        let diagnostic = growth.diagnostics.into_iter().next().map_or_else(
            || {
                IntegratedLayoutDiagnostic::error(
                    "iterative-growth-planning-failed",
                    "/",
                    None,
                    "SCC growth planning failed without a diagnostic",
                )
            },
            |diagnostic| {
                IntegratedLayoutDiagnostic::error(
                    "iterative-growth-planning-failed",
                    diagnostic.path,
                    diagnostic.entity,
                    diagnostic.message,
                )
            },
        );
        return IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic);
    }
    if growth.phases.is_empty() {
        return super::construct_coordinate_integrated_layout_with_time_limit(
            instance_wiring,
            facilities,
            items,
            transports,
            logistics_components,
            request,
            time_limit,
        );
    }

    let total_facilities = growth
        .phases
        .iter()
        .map(|phase| phase.facilities.len())
        .sum::<usize>();
    let mut cumulative_facilities = BTreeSet::new();
    let mut anchors = Vec::<FacilityPlacement>::new();
    let mut snapshots = Vec::with_capacity(growth.phases.len());
    let mut latest_success = None;

    for phase in &growth.phases {
        cumulative_facilities.extend(phase.facilities.iter().cloned());
        let partial_wiring = match project_cumulative_wiring(
            instance_wiring,
            &cumulative_facilities,
            phase.index,
            total_facilities,
        ) {
            Ok(wiring) => wiring,
            Err(diagnostic) => {
                let mut report = IntegratedLayoutReport::failure(
                    IntegratedLayoutStatus::InvalidInput,
                    diagnostic,
                );
                report.phases = snapshots;
                return report;
            }
        };

        let attempts = if anchors.is_empty() {
            vec![None]
        } else {
            ANCHOR_RADII
                .into_iter()
                .map(Some)
                .chain(std::iter::once(None))
                .collect()
        };
        let mut attempt_reports = Vec::with_capacity(attempts.len());
        let mut selected = None;

        for movement_radius in attempts {
            let placement = match movement_radius {
                Some(radius) => solve_anchored_facility_placement_with_time_limit(
                    &partial_wiring,
                    facilities,
                    request,
                    COORDINATE_ROUTING_CLEARANCE,
                    &anchors,
                    radius,
                    time_limit,
                ),
                None => solve_facility_placement_feasibly_with_time_limit(
                    &partial_wiring,
                    facilities,
                    request,
                    COORDINATE_ROUTING_CLEARANCE,
                    time_limit,
                ),
            };
            if !placement.success {
                attempt_reports.push(IntegratedLayoutPhaseAttempt {
                    movement_radius,
                    status: placement_status(placement.status),
                    diagnostic_code: placement
                        .diagnostics
                        .first()
                        .map(|diagnostic| diagnostic.code.to_string()),
                });
                continue;
            }

            let input = match prepare_model(&partial_wiring, facilities, items, transports, request)
            {
                Ok(input) => input,
                Err(diagnostic) => {
                    let mut report = IntegratedLayoutReport::failure(
                        IntegratedLayoutStatus::InvalidInput,
                        diagnostic,
                    );
                    report.phases = snapshots;
                    return report;
                }
            };
            let routed = sparse::construct_from_placements(
                input,
                logistics_components,
                placement.placements,
                routing_deadline(time_limit),
            );
            attempt_reports.push(IntegratedLayoutPhaseAttempt {
                movement_radius,
                status: routed.status,
                diagnostic_code: routed
                    .diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.code.to_string()),
            });
            if routed.success {
                selected = Some((movement_radius, routed));
                break;
            }
        }

        let Some((selected_movement_radius, mut phase_report)) = selected else {
            let mut report = IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Unknown,
                IntegratedLayoutDiagnostic::error(
                    "iterative-scc-phase-unsolved",
                    format!("/phases/{}", phase.index),
                    Some(format!("phase:{}", phase.index)),
                    format!(
                        "could not place and route SCC growth phase {} after {} bounded and fallback attempts",
                        phase.index,
                        attempt_reports.len(),
                    ),
                ),
            );
            report.phases = snapshots;
            return report;
        };

        let bounds = phase_report
            .bounds
            .clone()
            .expect("successful integrated phase must have bounds");
        let route_turns = phase_report.routes.iter().map(route_turn_count).sum();
        let route_cells = phase_report
            .routes
            .iter()
            .map(|route| route.cells.len())
            .sum();
        let bridge_count = phase_report
            .logistics_components
            .iter()
            .filter(|component| component.kind == LogisticsComponentKind::Bridge)
            .count();
        anchors = phase_report.placements.clone();
        snapshots.push(IntegratedLayoutPhase {
            index: phase.index,
            introduced_components: phase.components.clone(),
            introduced_facilities: phase.facilities.clone(),
            cumulative_facility_count: cumulative_facilities.len(),
            selected_movement_radius,
            bounds,
            placements: phase_report.placements.clone(),
            logistics_components: phase_report.logistics_components.clone(),
            routes: phase_report.routes.clone(),
            route_turns,
            route_cells,
            bridge_count,
            attempts: attempt_reports,
        });
        phase_report.diagnostics.push(IntegratedLayoutDiagnostic::info_for(
            "iterative-scc-phase-solved",
            format!("phase:{}", phase.index),
            format!(
                "solved output-first SCC growth phase {} with {} cumulative facilities using movement radius {:?}",
                phase.index,
                cumulative_facilities.len(),
                selected_movement_radius,
            ),
        ));
        latest_success = Some(phase_report);
    }

    let mut report = latest_success.expect("non-empty growth plan produced a final report");
    report.phases = snapshots;
    report.diagnostics.push(IntegratedLayoutDiagnostic::info(
        "iterative-scc-layout-complete",
        format!(
            "constructed and validated the full layout through {} output-first SCC growth phases",
            report.phases.len(),
        ),
    ));
    report
}

fn routing_deadline(time_limit: Duration) -> Instant {
    Instant::now()
        .checked_add(time_limit.max(Duration::from_secs(5)))
        .unwrap_or_else(Instant::now)
}

fn placement_status(status: FacilityPlacementStatus) -> IntegratedLayoutStatus {
    match status {
        FacilityPlacementStatus::Optimal => IntegratedLayoutStatus::Optimal,
        FacilityPlacementStatus::Feasible => IntegratedLayoutStatus::Feasible,
        FacilityPlacementStatus::Infeasible => IntegratedLayoutStatus::Infeasible,
        FacilityPlacementStatus::InvalidInput => IntegratedLayoutStatus::InvalidInput,
        FacilityPlacementStatus::Unknown => IntegratedLayoutStatus::Unknown,
    }
}

fn project_cumulative_wiring(
    wiring: &FacilityInstanceWiringReport,
    cumulative_facilities: &BTreeSet<String>,
    phase_index: usize,
    total_facilities: usize,
) -> Result<FacilityInstanceWiringReport, IntegratedLayoutDiagnostic> {
    if cumulative_facilities.len() == total_facilities {
        return Ok(wiring.clone());
    }

    let nodes = wiring
        .nodes
        .iter()
        .map(|node| (node_id(node).to_string(), node))
        .collect::<BTreeMap<_, _>>();
    let mut retained_nodes = cumulative_facilities.clone();
    let mut projected_edges = Vec::new();
    let mut synthetic_nodes = Vec::new();

    for (edge_index, edge) in wiring.edges.iter().enumerate() {
        let source = nodes.get(&edge.source).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-wiring-edge-endpoint",
                format!("/edges/{edge_index}/source"),
                Some(edge.source.clone()),
                format!(
                    "wiring edge source '{}' does not reference a known node",
                    edge.source
                ),
            )
        })?;
        let target = nodes.get(&edge.target).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-wiring-edge-endpoint",
                format!("/edges/{edge_index}/target"),
                Some(edge.target.clone()),
                format!(
                    "wiring edge target '{}' does not reference a known node",
                    edge.target
                ),
            )
        })?;
        let source_is_facility = matches!(source, FacilityInstanceWiringNode::Facility { .. });
        let target_is_facility = matches!(target, FacilityInstanceWiringNode::Facility { .. });
        let source_included = source_is_facility && cumulative_facilities.contains(&edge.source);
        let target_included = target_is_facility && cumulative_facilities.contains(&edge.target);

        match (
            source_is_facility,
            target_is_facility,
            source_included,
            target_included,
        ) {
            (true, true, true, true) => projected_edges.push(edge.clone()),
            (true, true, false, true) => {
                let boundary_id = format!("iterative-external:{phase_index}:{edge_index}");
                synthetic_nodes.push(FacilityInstanceWiringNode::External {
                    id: boundary_id.clone(),
                    item: edge.item.clone(),
                });
                projected_edges.push(FacilityInstanceWiringEdge {
                    source: boundary_id,
                    target: edge.target.clone(),
                    kind: edge.kind.clone(),
                    item: edge.item.clone(),
                    rate: edge.rate,
                });
            }
            (true, true, true, false) => {
                return Err(IntegratedLayoutDiagnostic::error(
                    "invalid-output-first-growth-order",
                    format!("/edges/{edge_index}"),
                    Some(format!("{}->{}", edge.source, edge.target)),
                    "an included facility feeds a facility that is absent from the cumulative output-first phase",
                ));
            }
            (false, true, false, true) | (true, false, true, false) => {
                retained_nodes.insert(edge.source.clone());
                retained_nodes.insert(edge.target.clone());
                projected_edges.push(edge.clone());
            }
            _ => {}
        }
    }

    let mut projected_nodes = wiring
        .nodes
        .iter()
        .filter(|node| retained_nodes.contains(node_id(node)))
        .cloned()
        .collect::<Vec<_>>();
    projected_nodes.extend(synthetic_nodes);
    Ok(FacilityInstanceWiringReport {
        success: true,
        nodes: projected_nodes,
        edges: projected_edges,
        diagnostics: wiring.diagnostics.clone(),
    })
}

fn node_id(node: &FacilityInstanceWiringNode) -> &str {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. }
        | FacilityInstanceWiringNode::External { id, .. }
        | FacilityInstanceWiringNode::Target { id, .. }
        | FacilityInstanceWiringNode::Surplus { id, .. } => id,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use crate::recipes::{
        FacilityInstanceWiringEdge, FacilityInstanceWiringNode, FacilityInstanceWiringReport, Rate,
    };

    use super::project_cumulative_wiring;

    #[test]
    fn replaces_missing_upstream_facilities_with_phase_boundaries() {
        let wiring = chain_wiring();
        let projected =
            project_cumulative_wiring(&wiring, &BTreeSet::from(["facility:b".to_string()]), 0, 2)
                .expect("output phase should project");

        assert_eq!(projected.edges.len(), 2);
        assert!(projected.nodes.iter().any(|node| matches!(
            node,
            FacilityInstanceWiringNode::External { id, item }
                if id == "iterative-external:0:0" && item == "middle"
        )));
        assert!(projected.edges.iter().any(|edge| {
            edge.source == "iterative-external:0:0" && edge.target == "facility:b"
        }));
    }

    #[test]
    fn final_phase_preserves_the_original_wiring() {
        let wiring = chain_wiring();
        let projected = project_cumulative_wiring(
            &wiring,
            &BTreeSet::from(["facility:a".to_string(), "facility:b".to_string()]),
            1,
            2,
        )
        .expect("complete phase should project");

        assert_eq!(projected, wiring);
    }

    fn chain_wiring() -> FacilityInstanceWiringReport {
        let facility = |id: &str| FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: format!("recipe:{id}"),
            facility: "assembler".to_string(),
            index: 0,
            runs_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            work_seconds_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            unused_capacity: Rate {
                numerator: 0,
                denominator: 1,
            },
        };
        FacilityInstanceWiringReport {
            success: true,
            nodes: vec![
                facility("facility:a"),
                facility("facility:b"),
                FacilityInstanceWiringNode::Target {
                    id: "target:final".to_string(),
                    item: "final".to_string(),
                },
            ],
            edges: vec![
                FacilityInstanceWiringEdge {
                    source: "facility:a".to_string(),
                    target: "facility:b".to_string(),
                    kind: "production".to_string(),
                    item: "middle".to_string(),
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                },
                FacilityInstanceWiringEdge {
                    source: "facility:b".to_string(),
                    target: "target:final".to_string(),
                    kind: "target".to_string(),
                    item: "final".to_string(),
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                },
            ],
            diagnostics: Vec::new(),
        }
    }
}
