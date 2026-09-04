use std::collections::BTreeSet;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

use crate::facilities::ValidatedFacilityCatalog;
use crate::logistics::{ValidatedItemCatalog, ValidatedTransportCatalog};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode, FacilityInstanceWiringReport,
};

use super::composition::compose_constructive_nodes_with_area_incumbent;
use super::port_demand::unavailable_constructive_port_demand_analysis;
use super::{
    CONSTRUCTIVE_ASSEMBLY_REPORT_SCHEMA_VERSION,
    CONSTRUCTIVE_AUTOMATIC_ASSEMBLY_REPORT_SCHEMA_VERSION,
    CONSTRUCTIVE_AUTOMATIC_ASSEMBLY_REQUEST_SCHEMA_VERSION, ConstructiveAssemblyReport,
    ConstructiveAssemblyStepReport, ConstructiveAutomaticAssemblyCandidateFailure,
    ConstructiveAutomaticAssemblyDiscoveryStep, ConstructiveAutomaticAssemblyReport,
    ConstructiveAutomaticAssemblyRequest, ConstructiveCompositionReport,
    ConstructiveFrontierDiagnostic, ConstructiveNode, analyze_constructive_port_demands,
    construct_facility_node, construct_process_module, constructive_node_from_process_module,
};

struct Candidate {
    root_instance: String,
    internal_item: String,
    requirement: String,
    module_member_instances: Vec<String>,
    composition: ConstructiveCompositionReport,
}

struct PreparedCandidate {
    root_instance: String,
    internal_item: String,
    requirement: String,
    module_member_instances: Vec<String>,
    source: ConstructiveNode,
    edge: FacilityInstanceWiringEdge,
}

#[derive(Default)]
struct CompositionWorkerOutcome {
    candidates: Vec<Candidate>,
    failures: Vec<ConstructiveAutomaticAssemblyCandidateFailure>,
}

pub fn automatically_assemble_constructive_modules(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &ConstructiveAutomaticAssemblyRequest,
) -> ConstructiveAutomaticAssemblyReport {
    if request.schema_version != CONSTRUCTIVE_AUTOMATIC_ASSEMBLY_REQUEST_SCHEMA_VERSION {
        return invalid_report(
            request,
            ConstructiveFrontierDiagnostic::error(
                "unsupported-constructive-automatic-assembly-schema-version",
                "/schema_version",
                None,
                format!(
                    "automatic assembly schema version {} is not supported; expected {}",
                    request.schema_version, CONSTRUCTIVE_AUTOMATIC_ASSEMBLY_REQUEST_SCHEMA_VERSION
                ),
            ),
        );
    }
    if request.max_steps == 0 {
        return invalid_report(
            request,
            ConstructiveFrontierDiagnostic::error(
                "zero-constructive-automatic-assembly-step-limit",
                "/max_steps",
                None,
                "automatic assembly max_steps must be greater than zero",
            ),
        );
    }
    let mut current =
        match construct_facility_node(wiring, facilities, items, &request.target_instance) {
            Ok(target) => target,
            Err(diagnostic) => return invalid_report(request, diagnostic),
        };
    let facility_instances = wiring
        .nodes
        .iter()
        .filter_map(|node| match node {
            FacilityInstanceWiringNode::Facility { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut steps = Vec::new();
    let mut discovery_steps = Vec::new();

    for index in 0..request.max_steps {
        let step_started = Instant::now();
        let frontier = facility_frontier(wiring, &current, &facility_instances);
        if frontier.is_empty() {
            return completed_report(request, current, steps, discovery_steps, transports);
        }
        let mut candidates_generated = 0usize;
        let mut module_constructions_failed = 0usize;
        let mut candidate_failures = Vec::new();
        let mut prepared = Vec::new();
        for edge in &frontier {
            let internal_items = wiring
                .edges
                .iter()
                .filter(|incoming| {
                    incoming.target == edge.source
                        && facility_instances.contains(incoming.source.as_str())
                        && !current.member_instances.contains(&incoming.source)
                })
                .map(|incoming| incoming.item.clone())
                .collect::<BTreeSet<_>>();
            for internal_item in internal_items {
                candidates_generated += 1;
                let module = construct_process_module(
                    wiring,
                    facilities,
                    items,
                    transports,
                    &edge.source,
                    &internal_item,
                );
                if !module.success {
                    module_constructions_failed += 1;
                    candidate_failures.push(ConstructiveAutomaticAssemblyCandidateFailure {
                        stage: "module-construction".to_string(),
                        root_instance: edge.source.clone(),
                        internal_item: internal_item.clone(),
                        requirement: edge.id.clone(),
                        composition_statistics: None,
                        diagnostics: module.growth.diagnostics.clone(),
                    });
                    continue;
                }
                let source = match constructive_node_from_process_module(&module) {
                    Ok(source) => source,
                    Err(diagnostic) => {
                        module_constructions_failed += 1;
                        candidate_failures.push(ConstructiveAutomaticAssemblyCandidateFailure {
                            stage: "module-conversion".to_string(),
                            root_instance: edge.source.clone(),
                            internal_item: internal_item.clone(),
                            requirement: edge.id.clone(),
                            composition_statistics: None,
                            diagnostics: vec![diagnostic],
                        });
                        continue;
                    }
                };
                if source
                    .member_instances
                    .iter()
                    .any(|instance| current.member_instances.contains(instance))
                {
                    module_constructions_failed += 1;
                    candidate_failures.push(ConstructiveAutomaticAssemblyCandidateFailure {
                        stage: "member-overlap".to_string(),
                        root_instance: edge.source.clone(),
                        internal_item: internal_item.clone(),
                        requirement: edge.id.clone(),
                        composition_statistics: None,
                        diagnostics: vec![ConstructiveFrontierDiagnostic::error(
                            "constructive-automatic-candidate-member-overlap",
                            format!("/steps/{index}"),
                            Some(edge.id.clone()),
                            "candidate process module contains a facility already present in the current composite",
                        )],
                    });
                    continue;
                }
                prepared.push(PreparedCandidate {
                    root_instance: edge.source.clone(),
                    internal_item,
                    requirement: edge.id.clone(),
                    module_member_instances: source.member_instances.clone(),
                    source,
                    edge: (*edge).clone(),
                });
            }
        }
        let (composition_workers, composition_outcome) =
            compose_candidates_parallel(&prepared, &current, transports, facilities);
        let compositions_failed = composition_outcome.failures.len();
        candidate_failures.extend(composition_outcome.failures);
        let mut candidates = composition_outcome.candidates;
        let composable_candidates = candidates.len();
        candidates.sort_by(|left, right| {
            let left_score = left
                .composition
                .score
                .expect("successful composition has a score");
            let right_score = right
                .composition
                .score
                .expect("successful composition has a score");
            (
                left_score,
                left.module_member_instances.len(),
                left.requirement.as_str(),
                left.internal_item.as_str(),
            )
                .cmp(&(
                    right_score,
                    right.module_member_instances.len(),
                    right.requirement.as_str(),
                    right.internal_item.as_str(),
                ))
        });
        let Some(selected) = candidates.into_iter().next() else {
            let diagnostic = ConstructiveFrontierDiagnostic::error(
                "constructive-automatic-assembly-exhausted",
                format!("/steps/{index}"),
                None,
                format!(
                    "found {} facility frontier requirements but no automatically discovered process module could be composed",
                    frontier.len()
                ),
            );
            return exhausted_report(
                request,
                current,
                steps,
                discovery_steps,
                candidate_failures,
                unresolved_requirement_ids(&frontier),
                diagnostic,
                transports,
            );
        };
        let Some(composite) = selected.composition.composite.clone() else {
            unreachable!("successful composition has a composite node");
        };
        discovery_steps.push(ConstructiveAutomaticAssemblyDiscoveryStep {
            index,
            elapsed_ms: step_started
                .elapsed()
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
            frontier_requirements: frontier.len(),
            candidates_generated,
            module_constructions_failed,
            composition_workers,
            compositions_failed,
            composable_candidates,
            selected_root_instance: selected.root_instance.clone(),
            selected_internal_item: selected.internal_item.clone(),
            selected_requirement: selected.requirement.clone(),
        });
        steps.push(ConstructiveAssemblyStepReport {
            index,
            root_instance: selected.root_instance,
            internal_item: selected.internal_item,
            requirement: selected.requirement,
            module_member_instances: selected.module_member_instances,
            composition: selected.composition,
        });
        current = composite;
    }

    let frontier = facility_frontier(wiring, &current, &facility_instances);
    partial_report(
        request,
        current,
        steps,
        discovery_steps,
        unresolved_requirement_ids(&frontier),
        transports,
    )
}

fn compose_candidates_parallel(
    prepared: &[PreparedCandidate],
    current: &ConstructiveNode,
    transports: &ValidatedTransportCatalog,
    facilities: &ValidatedFacilityCatalog,
) -> (usize, CompositionWorkerOutcome) {
    if prepared.is_empty() {
        return (0, CompositionWorkerOutcome::default());
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, std::num::NonZeroUsize::get)
        .min(prepared.len());
    let chunk_size = prepared.len().div_ceil(workers);
    let best_area = AtomicUsize::new(usize::MAX);
    let outcomes = std::thread::scope(|scope| {
        prepared
            .chunks(chunk_size)
            .map(|chunk| {
                let best_area = &best_area;
                scope.spawn(move || {
                    let mut outcome = CompositionWorkerOutcome::default();
                    for prepared in chunk {
                        let composition = compose_constructive_nodes_with_area_incumbent(
                            &prepared.source,
                            current,
                            &prepared.edge,
                            transports,
                            facilities,
                            best_area,
                        );
                        if !composition.success {
                            outcome
                                .failures
                                .push(ConstructiveAutomaticAssemblyCandidateFailure {
                                    stage: "composition".to_string(),
                                    root_instance: prepared.root_instance.clone(),
                                    internal_item: prepared.internal_item.clone(),
                                    requirement: prepared.requirement.clone(),
                                    composition_statistics: Some(composition.statistics.clone()),
                                    diagnostics: composition.diagnostics.clone(),
                                });
                            continue;
                        }
                        outcome.candidates.push(Candidate {
                            root_instance: prepared.root_instance.clone(),
                            internal_item: prepared.internal_item.clone(),
                            requirement: prepared.requirement.clone(),
                            module_member_instances: prepared.module_member_instances.clone(),
                            composition,
                        });
                    }
                    outcome
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .expect("constructive composition worker panicked")
            })
            .collect::<Vec<_>>()
    });
    let mut combined = CompositionWorkerOutcome::default();
    for mut outcome in outcomes {
        combined.failures.append(&mut outcome.failures);
        combined.candidates.append(&mut outcome.candidates);
    }
    (workers, combined)
}

fn facility_frontier<'a>(
    wiring: &'a FacilityInstanceWiringReport,
    current: &ConstructiveNode,
    facility_instances: &BTreeSet<&str>,
) -> Vec<&'a FacilityInstanceWiringEdge> {
    let boundary_requirements = current
        .boundary_requirements
        .iter()
        .map(|boundary| boundary.requirement.as_str())
        .collect::<BTreeSet<_>>();
    let mut frontier = wiring
        .edges
        .iter()
        .filter(|edge| {
            boundary_requirements.contains(edge.id.as_str())
                && current.member_instances.contains(&edge.target)
                && !current.member_instances.contains(&edge.source)
                && facility_instances.contains(edge.source.as_str())
        })
        .collect::<Vec<_>>();
    frontier.sort_by(|left, right| left.id.cmp(&right.id));
    frontier
}

fn unresolved_requirement_ids(frontier: &[&FacilityInstanceWiringEdge]) -> Vec<String> {
    frontier.iter().map(|edge| edge.id.clone()).collect()
}

fn assembly_report(
    request: &ConstructiveAutomaticAssemblyRequest,
    success: bool,
    current: Option<ConstructiveNode>,
    steps: Vec<ConstructiveAssemblyStepReport>,
    diagnostic: ConstructiveFrontierDiagnostic,
) -> ConstructiveAssemblyReport {
    ConstructiveAssemblyReport {
        schema_version: CONSTRUCTIVE_ASSEMBLY_REPORT_SCHEMA_VERSION,
        success,
        target_instance: request.target_instance.clone(),
        requested_modules: request.max_steps,
        completed_modules: steps.len(),
        steps,
        final_node: current,
        diagnostics: vec![diagnostic],
    }
}

fn invalid_report(
    request: &ConstructiveAutomaticAssemblyRequest,
    diagnostic: ConstructiveFrontierDiagnostic,
) -> ConstructiveAutomaticAssemblyReport {
    ConstructiveAutomaticAssemblyReport {
        schema_version: CONSTRUCTIVE_AUTOMATIC_ASSEMBLY_REPORT_SCHEMA_VERSION,
        success: false,
        complete: false,
        max_steps: request.max_steps,
        discovery_steps: Vec::new(),
        exhausted_candidate_failures: Vec::new(),
        unresolved_facility_requirements: Vec::new(),
        port_demand_analysis: unavailable_constructive_port_demand_analysis(
            "constructive-port-demand-analysis-not-run",
            "port demand analysis did not run because the automatic assembly request was invalid",
        ),
        assembly: assembly_report(request, false, None, Vec::new(), diagnostic.clone()),
        diagnostics: vec![diagnostic],
    }
}

fn completed_report(
    request: &ConstructiveAutomaticAssemblyRequest,
    current: ConstructiveNode,
    steps: Vec<ConstructiveAssemblyStepReport>,
    discovery_steps: Vec<ConstructiveAutomaticAssemblyDiscoveryStep>,
    transports: &ValidatedTransportCatalog,
) -> ConstructiveAutomaticAssemblyReport {
    let diagnostic = ConstructiveFrontierDiagnostic::info(
        "constructive-automatic-assembly-complete",
        "automatic module discovery resolved every facility-supplied boundary requirement",
    );
    let port_demand_analysis = analyze_constructive_port_demands(
        &current.boundary_requirements,
        &current.transport_networks,
        transports,
    );
    let success = port_demand_analysis.success;
    let diagnostics = if success {
        vec![diagnostic.clone()]
    } else {
        port_demand_analysis.diagnostics.clone()
    };
    let assembly_diagnostic = diagnostics
        .first()
        .cloned()
        .unwrap_or_else(|| diagnostic.clone());
    ConstructiveAutomaticAssemblyReport {
        schema_version: CONSTRUCTIVE_AUTOMATIC_ASSEMBLY_REPORT_SCHEMA_VERSION,
        success,
        complete: success,
        max_steps: request.max_steps,
        discovery_steps,
        exhausted_candidate_failures: Vec::new(),
        unresolved_facility_requirements: Vec::new(),
        port_demand_analysis,
        assembly: assembly_report(request, success, Some(current), steps, assembly_diagnostic),
        diagnostics,
    }
}

fn partial_report(
    request: &ConstructiveAutomaticAssemblyRequest,
    current: ConstructiveNode,
    steps: Vec<ConstructiveAssemblyStepReport>,
    discovery_steps: Vec<ConstructiveAutomaticAssemblyDiscoveryStep>,
    unresolved_facility_requirements: Vec<String>,
    transports: &ValidatedTransportCatalog,
) -> ConstructiveAutomaticAssemblyReport {
    let diagnostic = ConstructiveFrontierDiagnostic::info(
        "constructive-automatic-assembly-step-limit-reached",
        "automatic module discovery reached the requested growth-step limit with a valid partial composite",
    );
    let port_demand_analysis = analyze_constructive_port_demands(
        &current.boundary_requirements,
        &current.transport_networks,
        transports,
    );
    let success = port_demand_analysis.success;
    let diagnostics = if success {
        vec![diagnostic.clone()]
    } else {
        port_demand_analysis.diagnostics.clone()
    };
    let assembly_diagnostic = diagnostics
        .first()
        .cloned()
        .unwrap_or_else(|| diagnostic.clone());
    ConstructiveAutomaticAssemblyReport {
        schema_version: CONSTRUCTIVE_AUTOMATIC_ASSEMBLY_REPORT_SCHEMA_VERSION,
        success,
        complete: unresolved_facility_requirements.is_empty(),
        max_steps: request.max_steps,
        discovery_steps,
        exhausted_candidate_failures: Vec::new(),
        unresolved_facility_requirements,
        port_demand_analysis,
        assembly: assembly_report(request, success, Some(current), steps, assembly_diagnostic),
        diagnostics,
    }
}

fn exhausted_report(
    request: &ConstructiveAutomaticAssemblyRequest,
    current: ConstructiveNode,
    steps: Vec<ConstructiveAssemblyStepReport>,
    discovery_steps: Vec<ConstructiveAutomaticAssemblyDiscoveryStep>,
    exhausted_candidate_failures: Vec<ConstructiveAutomaticAssemblyCandidateFailure>,
    unresolved_facility_requirements: Vec<String>,
    diagnostic: ConstructiveFrontierDiagnostic,
    transports: &ValidatedTransportCatalog,
) -> ConstructiveAutomaticAssemblyReport {
    let port_demand_analysis = analyze_constructive_port_demands(
        &current.boundary_requirements,
        &current.transport_networks,
        transports,
    );
    ConstructiveAutomaticAssemblyReport {
        schema_version: CONSTRUCTIVE_AUTOMATIC_ASSEMBLY_REPORT_SCHEMA_VERSION,
        success: false,
        complete: false,
        max_steps: request.max_steps,
        discovery_steps,
        exhausted_candidate_failures,
        unresolved_facility_requirements,
        port_demand_analysis,
        assembly: assembly_report(request, false, Some(current), steps, diagnostic.clone()),
        diagnostics: vec![diagnostic],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layouts::constructive::assembly::tests::two_module_fixture;
    use crate::layouts::render_constructive_automatic_assembly_html;
    use crate::logistics::{
        SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION, TransportCapacity, TransportCatalog,
        TransportDefinition, TransportKind,
    };

    fn transports() -> ValidatedTransportCatalog {
        ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
            schema_version: SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION,
            transports: vec![
                TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 1_000,
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
        .expect("transport catalog validates")
    }

    #[test]
    fn discovers_and_assembles_two_modules_without_an_explicit_module_plan() {
        let (wiring, facilities, items, _, _) = two_module_fixture();
        let request = ConstructiveAutomaticAssemblyRequest {
            schema_version: CONSTRUCTIVE_AUTOMATIC_ASSEMBLY_REQUEST_SCHEMA_VERSION,
            target_instance: "target".to_string(),
            max_steps: 2,
        };

        let report = automatically_assemble_constructive_modules(
            &wiring,
            &facilities,
            &items,
            &transports(),
            &request,
        );

        assert!(report.success, "{:?}", report.diagnostics);
        assert!(report.complete);
        assert_eq!(report.discovery_steps.len(), 2);
        assert_eq!(report.assembly.completed_modules, 2);
        assert_eq!(report.discovery_steps[0].selected_requirement, "module-a");
        assert_eq!(report.discovery_steps[1].selected_requirement, "module-b");
        assert!(report.port_demand_analysis.success);
        assert_eq!(report.port_demand_analysis.boundary_requirements, 1);
        assert_eq!(report.port_demand_analysis.required_ports, 1);
        assert_eq!(report.port_demand_analysis.edge_implied_ports, 1);
        assert_eq!(report.port_demand_analysis.edge_implied_port_deficit, 0);
        assert!(
            report
                .discovery_steps
                .iter()
                .all(|step| step.candidates_generated > 0)
        );
        assert!(
            report
                .discovery_steps
                .iter()
                .all(|step| step.composition_workers > 0)
        );
        let html = render_constructive_automatic_assembly_html(&report, None)
            .expect("automatic assembly should render");
        assert!(html.contains("data-phase-label=\"Assembly 1/2\""));
        assert!(html.contains("data-phase-label=\"Assembly 2/2\""));
    }

    #[test]
    fn rejects_unknown_automatic_assembly_request_fields() {
        let error = serde_json::from_str::<ConstructiveAutomaticAssemblyRequest>(
            r#"{"schema_version":1,"target_instance":"target","max_steps":1,"extra":true}"#,
        )
        .expect_err("unknown automatic assembly request fields must be rejected");
        assert!(error.to_string().contains("unknown field `extra`"));
    }
}
