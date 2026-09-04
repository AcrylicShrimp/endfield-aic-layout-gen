use crate::facilities::ValidatedFacilityCatalog;
use crate::logistics::{ValidatedItemCatalog, ValidatedTransportCatalog};
use crate::recipes::FacilityInstanceWiringReport;

use super::{
    CONSTRUCTIVE_ASSEMBLY_REPORT_SCHEMA_VERSION, CONSTRUCTIVE_ASSEMBLY_REQUEST_SCHEMA_VERSION,
    CONSTRUCTIVE_COMPOSITION_SCHEMA_VERSION, ConstructiveAssemblyReport,
    ConstructiveAssemblyRequest, ConstructiveAssemblyStepReport, ConstructiveCompositionReport,
    ConstructiveCompositionStatistics, ConstructiveFrontierDiagnostic, ConstructiveNode,
    compose_constructive_nodes, construct_facility_node, construct_process_module,
    constructive_node_from_process_module,
};

pub fn assemble_constructive_modules(
    wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &ConstructiveAssemblyRequest,
) -> ConstructiveAssemblyReport {
    if request.schema_version != CONSTRUCTIVE_ASSEMBLY_REQUEST_SCHEMA_VERSION {
        return failed_report(
            request,
            Vec::new(),
            None,
            ConstructiveFrontierDiagnostic::error(
                "unsupported-constructive-assembly-schema-version",
                "/schema_version",
                None,
                format!(
                    "constructive assembly schema version {} is not supported; expected {}",
                    request.schema_version, CONSTRUCTIVE_ASSEMBLY_REQUEST_SCHEMA_VERSION
                ),
            ),
        );
    }
    if request.modules.is_empty() {
        return failed_report(
            request,
            Vec::new(),
            None,
            ConstructiveFrontierDiagnostic::error(
                "empty-constructive-assembly",
                "/modules",
                None,
                "constructive assembly requires at least one process module",
            ),
        );
    }
    let mut current =
        match construct_facility_node(wiring, facilities, items, &request.target_instance) {
            Ok(target) => target,
            Err(diagnostic) => return failed_report(request, Vec::new(), None, diagnostic),
        };
    let mut steps = Vec::new();
    for (index, module_request) in request.modules.iter().enumerate() {
        let module = construct_process_module(
            wiring,
            facilities,
            items,
            transports,
            &module_request.root_instance,
            &module_request.internal_item,
        );
        if !module.success {
            let diagnostic = module
                .growth
                .diagnostics
                .first()
                .cloned()
                .unwrap_or_else(|| {
                    ConstructiveFrontierDiagnostic::error(
                        "constructive-assembly-module-failed",
                        format!("/modules/{index}"),
                        Some(module_request.root_instance.clone()),
                        "process module construction failed without a detailed diagnostic",
                    )
                });
            steps.push(ConstructiveAssemblyStepReport {
                index,
                root_instance: module_request.root_instance.clone(),
                internal_item: module_request.internal_item.clone(),
                requirement: module_request.requirement.clone(),
                module_member_instances: module.member_instances,
                composition: failed_composition(&current, module_request, diagnostic.clone()),
            });
            return failed_report(request, steps, Some(current), diagnostic);
        }
        let source = match constructive_node_from_process_module(&module) {
            Ok(source) => source,
            Err(diagnostic) => {
                steps.push(ConstructiveAssemblyStepReport {
                    index,
                    root_instance: module_request.root_instance.clone(),
                    internal_item: module_request.internal_item.clone(),
                    requirement: module_request.requirement.clone(),
                    module_member_instances: module.member_instances,
                    composition: failed_composition(&current, module_request, diagnostic.clone()),
                });
                return failed_report(request, steps, Some(current), diagnostic);
            }
        };
        let duplicate_members = source
            .member_instances
            .iter()
            .filter(|instance| current.member_instances.contains(instance))
            .cloned()
            .collect::<Vec<_>>();
        if !duplicate_members.is_empty() {
            let diagnostic = ConstructiveFrontierDiagnostic::error(
                "overlapping-constructive-assembly-members",
                format!("/modules/{index}"),
                Some(module_request.root_instance.clone()),
                format!(
                    "process module reuses facility instances already present in the composite: {}",
                    duplicate_members.join(", ")
                ),
            );
            steps.push(ConstructiveAssemblyStepReport {
                index,
                root_instance: module_request.root_instance.clone(),
                internal_item: module_request.internal_item.clone(),
                requirement: module_request.requirement.clone(),
                module_member_instances: source.member_instances,
                composition: failed_composition(&current, module_request, diagnostic.clone()),
            });
            return failed_report(request, steps, Some(current), diagnostic);
        }
        let Some(edge) = wiring
            .edges
            .iter()
            .find(|edge| edge.id == module_request.requirement)
        else {
            let diagnostic = ConstructiveFrontierDiagnostic::error(
                "missing-constructive-assembly-requirement",
                format!("/modules/{index}/requirement"),
                Some(module_request.requirement.clone()),
                "constructive assembly requirement does not exist in the wiring graph",
            );
            steps.push(ConstructiveAssemblyStepReport {
                index,
                root_instance: module_request.root_instance.clone(),
                internal_item: module_request.internal_item.clone(),
                requirement: module_request.requirement.clone(),
                module_member_instances: source.member_instances,
                composition: failed_composition(&current, module_request, diagnostic.clone()),
            });
            return failed_report(request, steps, Some(current), diagnostic);
        };
        let module_member_instances = source.member_instances.clone();
        let composition =
            compose_constructive_nodes(&source, &current, edge, transports, facilities);
        let Some(composite) = composition.composite.clone() else {
            let diagnostic = composition.diagnostics.first().cloned().unwrap_or_else(|| {
                ConstructiveFrontierDiagnostic::error(
                    "constructive-assembly-composition-failed",
                    format!("/modules/{index}"),
                    Some(module_request.requirement.clone()),
                    "constructive node composition failed without a detailed diagnostic",
                )
            });
            steps.push(ConstructiveAssemblyStepReport {
                index,
                root_instance: module_request.root_instance.clone(),
                internal_item: module_request.internal_item.clone(),
                requirement: module_request.requirement.clone(),
                module_member_instances,
                composition,
            });
            return failed_report(request, steps, Some(current), diagnostic);
        };
        steps.push(ConstructiveAssemblyStepReport {
            index,
            root_instance: module_request.root_instance.clone(),
            internal_item: module_request.internal_item.clone(),
            requirement: module_request.requirement.clone(),
            module_member_instances,
            composition,
        });
        current = composite;
    }

    ConstructiveAssemblyReport {
        schema_version: CONSTRUCTIVE_ASSEMBLY_REPORT_SCHEMA_VERSION,
        success: true,
        target_instance: request.target_instance.clone(),
        requested_modules: request.modules.len(),
        completed_modules: steps.len(),
        steps,
        final_node: Some(current),
        diagnostics: vec![ConstructiveFrontierDiagnostic::info(
            "constructive-assembly-constructed",
            "assembled every requested process module through recursive node composition",
        )],
    }
}

fn failed_composition(
    current: &ConstructiveNode,
    request: &super::ConstructiveAssemblyModuleRequest,
    diagnostic: ConstructiveFrontierDiagnostic,
) -> ConstructiveCompositionReport {
    ConstructiveCompositionReport {
        schema_version: CONSTRUCTIVE_COMPOSITION_SCHEMA_VERSION,
        success: false,
        requirement: request.requirement.clone(),
        source_node: format!(
            "process-module:{}:{}",
            request.root_instance, request.internal_item
        ),
        target_node: current.id.clone(),
        score: None,
        composite: None,
        statistics: ConstructiveCompositionStatistics::default(),
        diagnostics: vec![diagnostic],
    }
}

fn failed_report(
    request: &ConstructiveAssemblyRequest,
    steps: Vec<ConstructiveAssemblyStepReport>,
    final_node: Option<ConstructiveNode>,
    diagnostic: ConstructiveFrontierDiagnostic,
) -> ConstructiveAssemblyReport {
    let completed_modules = steps.iter().filter(|step| step.composition.success).count();
    ConstructiveAssemblyReport {
        schema_version: CONSTRUCTIVE_ASSEMBLY_REPORT_SCHEMA_VERSION,
        success: false,
        target_instance: request.target_instance.clone(),
        requested_modules: request.modules.len(),
        completed_modules,
        steps,
        final_node,
        diagnostics: vec![diagnostic],
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::facilities::{
        FacilityCatalog, FacilityDefinition, FacilityFootprint, FacilityPortDefinition,
        FacilityPortDirection, FacilityPortEdge, FacilityPortPosition,
        SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
    };
    use crate::layouts::{ConstructiveAssemblyModuleRequest, render_constructive_assembly_html};
    use crate::logistics::{
        ItemCatalog, ItemDefinition, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
        SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION, TransportCapacity, TransportCatalog,
        TransportDefinition, TransportKind, ValidatedTransportCatalog,
    };
    use crate::recipes::{
        FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
        FacilityInstanceWiringNode, FacilityInstanceWiringProjection, Rate,
    };

    fn node(id: &str) -> FacilityInstanceWiringNode {
        let one = Rate {
            numerator: 1,
            denominator: 1,
        };
        FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: format!("{id}-recipe"),
            facility: "machine".to_string(),
            index: 0,
            runs_per_second: one,
            work_seconds_per_second: one,
            unused_capacity: Rate::zero(),
        }
    }

    fn edge(id: &str, source: &str, target: &str, item: &str) -> FacilityInstanceWiringEdge {
        FacilityInstanceWiringEdge {
            id: id.to_string(),
            source: source.to_string(),
            target: target.to_string(),
            kind: "recipe-flow".to_string(),
            item: item.to_string(),
            rate: Rate {
                numerator: 1,
                denominator: 1,
            },
            projection: FacilityInstanceWiringProjection::Original,
        }
    }

    fn facilities() -> ValidatedFacilityCatalog {
        ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: SUPPORTED_FACILITY_CATALOG_SCHEMA_VERSION,
            facilities: vec![FacilityDefinition {
                id: "machine".to_string(),
                footprint: FacilityFootprint {
                    width: 2,
                    height: 2,
                },
                allowed_rotations: vec![0, 90, 180, 270],
                ports: vec![
                    FacilityPortDefinition {
                        id: "input-west".to_string(),
                        direction: FacilityPortDirection::Input,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 0, y: 0 },
                        edge: FacilityPortEdge::West,
                    },
                    FacilityPortDefinition {
                        id: "input-north".to_string(),
                        direction: FacilityPortDirection::Input,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 1, y: 0 },
                        edge: FacilityPortEdge::North,
                    },
                    FacilityPortDefinition {
                        id: "output-east".to_string(),
                        direction: FacilityPortDirection::Output,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 1, y: 1 },
                        edge: FacilityPortEdge::East,
                    },
                    FacilityPortDefinition {
                        id: "output-south".to_string(),
                        direction: FacilityPortDirection::Output,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 0, y: 1 },
                        edge: FacilityPortEdge::South,
                    },
                ],
            }],
        })
        .expect("facility catalog validates")
    }

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

    pub(crate) fn two_module_fixture() -> (
        FacilityInstanceWiringReport,
        ValidatedFacilityCatalog,
        ValidatedItemCatalog,
        ValidatedTransportCatalog,
        ConstructiveAssemblyRequest,
    ) {
        let wiring = FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes: vec![
                node("supplier-a"),
                node("root-a"),
                node("supplier-b"),
                node("root-b"),
                node("target"),
                node("outside"),
            ],
            edges: vec![
                edge("internal-a", "supplier-a", "root-a", "input-a"),
                edge("module-a", "root-a", "target", "product"),
                edge("internal-b", "supplier-b", "root-b", "input-b"),
                edge("module-b", "root-b", "target", "product"),
                edge("final", "target", "outside", "final"),
            ],
            diagnostics: Vec::new(),
        };
        let facilities = facilities();
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: ["input-a", "input-b", "product", "final"]
                .into_iter()
                .map(|id| ItemDefinition {
                    id: id.to_string(),
                    transport: TransportKind::Belt,
                })
                .collect(),
        })
        .expect("item catalog validates");
        let request = ConstructiveAssemblyRequest {
            schema_version: CONSTRUCTIVE_ASSEMBLY_REQUEST_SCHEMA_VERSION,
            target_instance: "target".to_string(),
            modules: vec![
                ConstructiveAssemblyModuleRequest {
                    root_instance: "root-a".to_string(),
                    internal_item: "input-a".to_string(),
                    requirement: "module-a".to_string(),
                },
                ConstructiveAssemblyModuleRequest {
                    root_instance: "root-b".to_string(),
                    internal_item: "input-b".to_string(),
                    requirement: "module-b".to_string(),
                },
            ],
        };
        (wiring, facilities, items, transports(), request)
    }

    #[test]
    fn recursively_assembles_two_modules_into_the_same_target_node() {
        let (wiring, facilities, items, transports, request) = two_module_fixture();

        let report =
            assemble_constructive_modules(&wiring, &facilities, &items, &transports, &request);

        assert!(report.success, "{:?}", report.diagnostics);
        assert_eq!(report.completed_modules, 2);
        assert_eq!(report.steps.len(), 2);
        let final_node = report.final_node.as_ref().expect("final composite node");
        assert_eq!(final_node.placements.len(), 5);
        assert_eq!(final_node.transport_networks.len(), 4);
        assert_eq!(final_node.internal_requirements.len(), 4);
        let html = render_constructive_assembly_html(&report, None)
            .expect("recursive assembly should render");
        assert!(html.contains("data-phase-label=\"Assembly 1/2\""));
        assert!(html.contains("data-phase-label=\"Assembly 2/2\""));
    }

    #[test]
    fn rejects_unknown_assembly_request_fields() {
        let error = serde_json::from_str::<ConstructiveAssemblyRequest>(
            r#"{"schema_version":1,"target_instance":"target","modules":[],"extra":true}"#,
        )
        .expect_err("unknown assembly request fields must be rejected");
        assert!(error.to_string().contains("unknown field `extra`"));
    }
}
