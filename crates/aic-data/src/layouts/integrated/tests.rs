use crate::facilities::{
    FacilityCatalog, FacilityDefinition, FacilityFootprint, FacilityPortDefinition,
    FacilityPortDirection, FacilityPortEdge, FacilityPortPosition, ValidatedFacilityCatalog,
};
use crate::layouts::FacilityPlacementRequest;
use crate::logistics::{
    CardinalDirection, ItemCatalog, ItemDefinition, LogisticsComponentCatalog,
    LogisticsComponentDefinition, LogisticsComponentKind, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
    SUPPORTED_LOGISTICS_COMPONENT_CATALOG_SCHEMA_VERSION,
    SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION, TransportCapacity, TransportCatalog,
    TransportDefinition, TransportKind, ValidatedItemCatalog, ValidatedLogisticsComponentCatalog,
    ValidatedTransportCatalog,
};
use crate::recipes::{
    FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
    FacilityInstanceWiringNode, FacilityInstanceWiringReport, Rate,
};

use super::{
    ExactProofStatus, ExactValidationStatus, IntegratedLayoutStatus, solve_integrated_layout,
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

fn catalogs() -> (
    ValidatedFacilityCatalog,
    ValidatedItemCatalog,
    ValidatedTransportCatalog,
    ValidatedLogisticsComponentCatalog,
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
    .expect("facility fixture should validate");
    let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
        schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
        items: vec![ItemDefinition {
            id: "part".to_string(),
            transport: TransportKind::Belt,
        }],
    })
    .expect("item fixture should validate");
    let transports = ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
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
    .expect("transport fixture should validate");
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
    let components =
        ValidatedLogisticsComponentCatalog::try_from_catalog(LogisticsComponentCatalog {
            schema_version: SUPPORTED_LOGISTICS_COMPONENT_CATALOG_SCHEMA_VERSION,
            components,
        })
        .expect("component fixture should validate");
    (facilities, items, transports, components)
}

fn wiring() -> FacilityInstanceWiringReport {
    let node = |id: &str, recipe: &str, facility: &str| FacilityInstanceWiringNode::Facility {
        id: id.to_string(),
        recipe: recipe.to_string(),
        facility: facility.to_string(),
        index: 0,
        runs_per_second: Rate {
            numerator: 1,
            denominator: 1,
        },
        work_seconds_per_second: Rate {
            numerator: 1,
            denominator: 1,
        },
        unused_capacity: Rate::zero(),
    };
    FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: vec![
            node("source", "source-recipe", "source-machine"),
            node("target", "target-recipe", "target-machine"),
        ],
        edges: vec![FacilityInstanceWiringEdge::original(
            "source",
            "target",
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

#[test]
fn exact_solver_jointly_places_selects_ports_routes_and_validates() {
    let (facilities, items, transports, components) = catalogs();
    let report = solve_integrated_layout(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: 4,
            max_height: 1,
        },
    );

    assert!(report.success, "{:#?}", report.diagnostics);
    assert_eq!(report.status, IntegratedLayoutStatus::Optimal);
    assert_eq!(report.placements.len(), 2);
    assert_eq!(report.transport_networks.len(), 1);
    assert_eq!(report.transport_networks[0].cells.len(), 1);
    assert_eq!(report.transport_networks[0].terminals.len(), 2);
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "commodity-flow-without-branch-components")
    );
    let serialized = serde_json::to_value(&report).expect("report should serialize");
    assert!(serialized.get("transport_networks").is_some());
    assert!(serialized.get("routes").is_none());
    let exact = report.exact.expect("exact metrics should be present");
    assert_eq!(exact.model.commodity_network_count, 1);
    assert_eq!(exact.model.commodity_item_count, 1);
    assert_eq!(exact.model.belt_network_count, 1);
    assert_eq!(exact.model.pipe_network_count, 0);
    assert_eq!(exact.model.network_requirement_reference_count, 1);
    assert_eq!(exact.model.network_terminal_count, 2);
    assert_eq!(exact.model.external_terminal_count, 0);
    assert!(exact.model.network_flow_variables > 0);
    assert_eq!(exact.proof, ExactProofStatus::ProvenOptimal);
    assert_eq!(exact.validation, ExactValidationStatus::Passed);
}

#[test]
fn network_witness_rejects_terminal_flow_that_does_not_match_the_logical_graph() {
    let (facilities, items, transports, components) = catalogs();
    let request = FacilityPlacementRequest {
        schema_version: 2,
        max_width: 4,
        max_height: 1,
    };
    let input = super::prepare_model(&wiring(), &facilities, &items, &transports, &request)
        .expect("fixture should prepare");
    let mut report = solve_integrated_layout(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    );
    report.transport_networks[0].terminals[0].rate = Rate {
        numerator: 2,
        denominator: 1,
    };

    let diagnostic = super::witness::validate(&input, &components, &report)
        .expect_err("changed terminal flow must invalidate the witness");

    assert_eq!(diagnostic.code, "invalid-integrated-layout-witness");
    assert!(diagnostic.message.contains("terminal rates"));
}

#[test]
fn exact_solver_rejects_facility_area_above_hard_bounds_before_search() {
    let (facilities, items, transports, components) = catalogs();
    let report = solve_integrated_layout(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
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
}
