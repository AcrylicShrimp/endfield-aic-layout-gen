use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode,
    FacilityInstanceWiringProjectedEndpoint, FacilityInstanceWiringProjection,
    FacilityInstanceWiringReport,
};

use super::super::{
    IntegratedLayoutDiagnostic, IntegratedLayoutPhase, IntegratedLayoutReport,
    IntegratedLayoutStatus, prepare_exact_model, solve_exact_model,
};

// This schedules one ready SCC at a time. It changes only the sequence of complete cumulative
// solves; every phase still contains the full legal placement-and-routing domain for its graph.
const MAX_NEW_FACILITIES_PER_PHASE: usize = 1;

#[allow(clippy::too_many_arguments)]
pub(in crate::layouts::integrated) fn solve_first_iterative_scc_phase(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Duration,
) -> IntegratedLayoutReport {
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_PHASE);
    if !growth.success {
        return growth_failure_report(growth.diagnostics);
    }
    let Some(first_phase) = growth.phases.first() else {
        return solve_exact_model(
            instance_wiring,
            facilities,
            items,
            transports,
            logistics_components,
            request,
            Some(time_limit),
            None,
        );
    };
    let total_facilities = growth
        .components
        .iter()
        .map(|component| component.facilities.len())
        .sum();
    let cumulative_facilities = first_phase.facilities.iter().cloned().collect();
    let partial_wiring = match project_cumulative_wiring(
        instance_wiring,
        &cumulative_facilities,
        total_facilities,
    ) {
        Ok(wiring) => wiring,
        Err(diagnostic) => {
            return IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::InvalidInput,
                diagnostic,
            );
        }
    };
    let mut report = solve_exact_model(
        &partial_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        Some(time_limit),
        None,
    );
    report.diagnostics.push(IntegratedLayoutDiagnostic::info(
        "research-first-scc-phase",
        "solved only cumulative SCC phase 0 as a complete joint placement-and-routing model for an explicit research experiment",
    ));
    report
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layouts::integrated) fn solve_iterative_scc(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    if let Err(report) = prepare_exact_model(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    ) {
        return report;
    }
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_PHASE);
    if !growth.success {
        return growth_failure_report(growth.diagnostics);
    }
    if growth.phases.is_empty() {
        return solve_exact_model(
            instance_wiring,
            facilities,
            items,
            transports,
            logistics_components,
            request,
            time_limit,
            None,
        );
    }

    let started = Instant::now();
    let deadline = time_limit.and_then(|limit| started.checked_add(limit));
    let total_facilities = growth
        .components
        .iter()
        .map(|component| component.facilities.len())
        .sum();
    let mut cumulative_facilities = BTreeSet::new();
    let mut previous_solution = None;
    let mut snapshots = Vec::with_capacity(growth.phases.len());

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
        let phase_time_limit =
            deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()));
        if phase_time_limit.is_some_and(|limit| limit.is_zero()) {
            let mut report = IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Unknown,
                IntegratedLayoutDiagnostic::error(
                    "iterative-scc-time-limit",
                    format!("/phases/{}", phase.index),
                    Some(format!("phase:{}", phase.index)),
                    "the total exact-search time limit expired before this cumulative SCC phase started",
                ),
            );
            report.phases = snapshots;
            return report;
        }
        let mut phase_report = solve_exact_model(
            &partial_wiring,
            facilities,
            items,
            transports,
            logistics_components,
            request,
            phase_time_limit,
            previous_solution.as_ref(),
        );
        if !phase_report.success {
            phase_report.diagnostics.push(IntegratedLayoutDiagnostic::error(
                "iterative-scc-phase-unsolved",
                format!("/phases/{}", phase.index),
                Some(format!("phase:{}", phase.index)),
                format!(
                    "cumulative SCC phase {} did not produce a complete validated layout; no heuristic fallback was attempted",
                    phase.index,
                ),
            ));
            phase_report.phases = snapshots;
            return phase_report;
        }

        let bounds = phase_report
            .bounds
            .clone()
            .expect("a successful exact solve has canonical used bounds");
        let exact = phase_report
            .exact
            .clone()
            .expect("a successful exact solve has exact metrics");
        snapshots.push(IntegratedLayoutPhase {
            index: phase.index,
            introduced_components: phase.components.clone(),
            introduced_facilities: phase.facilities.clone(),
            cumulative_facility_count: exact.model.facility_count,
            cumulative_route_requirement_count: exact.model.route_requirement_count,
            bounds,
            placements: phase_report.placements.clone(),
            logistics_components: phase_report.logistics_components.clone(),
            transport_networks: phase_report.transport_networks.clone(),
            exact,
        });
        previous_solution = Some(phase_report);
    }

    let mut report = previous_solution.expect("a non-empty growth plan solved at least one phase");
    report.phases = snapshots;
    report.diagnostics.push(IntegratedLayoutDiagnostic::info(
        "iterative-scc-complete",
        format!(
            "solved {} cumulative SCC phases, scheduled one ready SCC at a time, as complete joint placement-and-routing models; prior solutions were supplied only as non-binding warm-start hints",
            report.phases.len(),
        ),
    ));
    report
}

fn growth_failure_report(
    diagnostics: Vec<crate::layouts::FacilityGrowthDiagnostic>,
) -> IntegratedLayoutReport {
    let diagnostic = diagnostics.into_iter().next().map_or_else(
        || {
            IntegratedLayoutDiagnostic::error(
                "iterative-scc-growth-planning-failed",
                "/",
                None,
                "SCC growth planning failed without a diagnostic",
            )
        },
        |diagnostic| {
            IntegratedLayoutDiagnostic::error(
                "iterative-scc-growth-planning-failed",
                diagnostic.path,
                diagnostic.entity,
                diagnostic.message,
            )
        },
    );
    IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
}

pub(in crate::layouts::integrated) fn project_cumulative_wiring(
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
            missing_endpoint_diagnostic(edge_index, "source", edge.source.as_str())
        })?;
        let target = nodes.get(&edge.target).ok_or_else(|| {
            missing_endpoint_diagnostic(edge_index, "target", edge.target.as_str())
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
                    "an included facility feeds a facility absent from the cumulative output-first phase",
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

fn missing_endpoint_diagnostic(
    edge_index: usize,
    endpoint: &str,
    node: &str,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "missing-wiring-edge-endpoint",
        format!("/edges/{edge_index}/{endpoint}"),
        Some(node.to_string()),
        format!("wiring edge {endpoint} '{node}' does not reference a known node"),
    )
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
    use super::*;
    use crate::recipes::{
        FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringProjectedEndpoint, Rate,
    };

    #[test]
    fn missing_upstream_facility_becomes_a_non_authoritative_frontier_input() {
        let wiring = chain_wiring();
        let original = &wiring.edges[0];
        let projected =
            project_cumulative_wiring(&wiring, &BTreeSet::from(["facility:b".to_string()]), 2)
                .expect("output-first phase should project");
        let projected_edge = projected
            .edges
            .iter()
            .find(|edge| edge.id == original.id)
            .expect("projected edge should preserve its stable identity");

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
    fn final_phase_is_the_original_full_graph() {
        let wiring = chain_wiring();
        let projected = project_cumulative_wiring(
            &wiring,
            &BTreeSet::from(["facility:a".to_string(), "facility:b".to_string()]),
            2,
        )
        .expect("full phase should project");

        assert_eq!(projected, wiring);
    }

    fn chain_wiring() -> FacilityInstanceWiringReport {
        let facility = |id: &str| FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: format!("recipe:{id}"),
            facility: "assembler".to_string(),
            index: 0,
            runs_per_second: unit_rate(),
            work_seconds_per_second: unit_rate(),
            unused_capacity: Rate::zero(),
        };
        FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
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
                    unit_rate(),
                ),
                FacilityInstanceWiringEdge::original(
                    "facility:b",
                    "target:final",
                    "target",
                    "final",
                    unit_rate(),
                ),
            ],
            diagnostics: Vec::new(),
        }
    }

    fn unit_rate() -> Rate {
        Rate {
            numerator: 1,
            denominator: 1,
        }
    }
}
