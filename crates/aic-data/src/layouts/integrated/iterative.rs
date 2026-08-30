use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::growth::plan_facility_growth;
use crate::layouts::placement::{
    solve_facility_placement_feasibly_with_time_limit,
    solve_hinted_facility_placement_with_time_limit,
};
use crate::layouts::{FacilityPlacement, FacilityPlacementRequest, FacilityPlacementStatus};
use crate::logistics::{
    LogisticsComponentKind, ValidatedItemCatalog, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode,
    FacilityInstanceWiringProjectedEndpoint, FacilityInstanceWiringProjection,
    FacilityInstanceWiringReport,
};

use super::{
    IntegratedLayoutDiagnostic, IntegratedLayoutPhase, IntegratedLayoutPhaseAttempt,
    IntegratedLayoutReport, IntegratedLayoutStatus, PRODUCTION_FACILITY_GAP,
    frame_placements_for_routing, prepare_model, route_turn_count, sparse,
};

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

        let placement = if anchors.is_empty() {
            solve_facility_placement_feasibly_with_time_limit(
                &partial_wiring,
                facilities,
                request,
                PRODUCTION_FACILITY_GAP,
                time_limit,
            )
        } else {
            solve_hinted_facility_placement_with_time_limit(
                &partial_wiring,
                facilities,
                request,
                PRODUCTION_FACILITY_GAP,
                &anchors,
                time_limit,
            )
        };
        let mut attempt_reports = Vec::with_capacity(2);
        let mut selected = None;
        if !placement.success {
            attempt_reports.push(IntegratedLayoutPhaseAttempt {
                placement_hint_count: anchors.len(),
                status: placement_status(placement.status),
                diagnostic_code: placement
                    .diagnostics
                    .first()
                    .map(|diagnostic| diagnostic.code.to_string()),
            });
        } else {
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
            let framed_placements = frame_placements_for_routing(
                placement.placements,
                request.max_width,
                request.max_height,
            );
            if let Some(framed_placements) = framed_placements {
                let mut routed = sparse::construct_from_placements(
                    input,
                    logistics_components,
                    framed_placements,
                    routing_deadline(time_limit),
                );
                if !routed.success {
                    let fallback_input = match prepare_model(
                        &partial_wiring,
                        facilities,
                        items,
                        transports,
                        request,
                    ) {
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
                    routed = sparse::construct(fallback_input, logistics_components);
                }
                attempt_reports.push(IntegratedLayoutPhaseAttempt {
                    placement_hint_count: anchors.len(),
                    status: routed.status,
                    diagnostic_code: routed
                        .diagnostics
                        .first()
                        .map(|diagnostic| diagnostic.code.to_string()),
                });
                if routed.success {
                    selected = Some(routed);
                }
            } else {
                attempt_reports.push(IntegratedLayoutPhaseAttempt {
                    placement_hint_count: anchors.len(),
                    status: IntegratedLayoutStatus::Unknown,
                    diagnostic_code: Some("iterative-routing-frame-does-not-fit".to_string()),
                });
            }
        }

        let Some(mut phase_report) = selected else {
            let attempt_summary = attempt_reports
                .iter()
                .map(|attempt| {
                    format!(
                        "placement_hints={}, status={:?}, diagnostic={}",
                        attempt.placement_hint_count,
                        attempt.status,
                        attempt.diagnostic_code.as_deref().unwrap_or("none"),
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            let mut report = IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Unknown,
                IntegratedLayoutDiagnostic::error(
                    "iterative-scc-phase-unsolved",
                    format!("/phases/{}", phase.index),
                    Some(format!("phase:{}", phase.index)),
                    format!(
                        "could not place and route SCC growth phase {} after {} bounded and fallback attempts: {attempt_summary}",
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
            prior_placement_hint_count: anchors.len(),
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
                "solved output-first SCC growth phase {} with {} cumulative facilities using {} prior coordinate hints without movement constraints",
                phase.index,
                cumulative_facilities.len(),
                anchors.len(),
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
                let frontier_id = format!("iterative-external:{}", edge.id);
                synthetic_nodes.push(FacilityInstanceWiringNode::External {
                    id: frontier_id.clone(),
                    item: edge.item.clone(),
                });
                projected_edges.push(FacilityInstanceWiringEdge {
                    id: edge.id.clone(),
                    source: frontier_id,
                    target: edge.target.clone(),
                    kind: edge.kind.clone(),
                    item: edge.item.clone(),
                    rate: edge.rate,
                    projection: FacilityInstanceWiringProjection::FrontierExternal {
                        missing_facility: edge.source.clone(),
                        original_endpoint: FacilityInstanceWiringProjectedEndpoint::Source,
                    },
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
        schema_version: wiring.schema_version,
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
        FacilityInstanceWiringEdge, FacilityInstanceWiringNode,
        FacilityInstanceWiringProjectedEndpoint, FacilityInstanceWiringProjection,
        FacilityInstanceWiringReport, Rate,
    };

    use super::project_cumulative_wiring;

    #[test]
    fn replaces_missing_upstream_facilities_with_frontier_external_connections() {
        let wiring = chain_wiring();
        let original = wiring
            .edges
            .iter()
            .find(|edge| edge.source == "facility:a")
            .expect("fixture has the upstream edge");
        let frontier_id = format!("iterative-external:{}", original.id);
        let projected =
            project_cumulative_wiring(&wiring, &BTreeSet::from(["facility:b".to_string()]), 2)
                .expect("output phase should project");

        assert_eq!(projected.edges.len(), 2);
        assert!(projected.nodes.iter().any(|node| matches!(
            node,
            FacilityInstanceWiringNode::External { id, item }
                if id == &frontier_id && item == "middle"
        )));
        let projected_edge = projected
            .edges
            .iter()
            .find(|edge| edge.source == frontier_id)
            .expect("projected wiring has the frontier edge");
        assert_eq!(projected_edge.id, original.id);
        assert_eq!(projected_edge.target, "facility:b");
        assert_eq!(
            projected_edge.projection,
            FacilityInstanceWiringProjection::FrontierExternal {
                missing_facility: "facility:a".to_string(),
                original_endpoint: FacilityInstanceWiringProjectedEndpoint::Source,
            }
        );
    }

    #[test]
    fn final_phase_preserves_the_original_wiring() {
        let wiring = chain_wiring();
        let projected = project_cumulative_wiring(
            &wiring,
            &BTreeSet::from(["facility:a".to_string(), "facility:b".to_string()]),
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
            schema_version: crate::recipes::FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
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
                FacilityInstanceWiringEdge::original(
                    "facility:a",
                    "facility:b",
                    "production",
                    "middle",
                    Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                ),
                FacilityInstanceWiringEdge::original(
                    "facility:b",
                    "target:final",
                    "target",
                    "final",
                    Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                ),
            ],
            diagnostics: Vec::new(),
        }
    }
}
