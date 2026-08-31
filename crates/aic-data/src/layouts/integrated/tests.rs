use crate::facilities::{
    FacilityCatalog, FacilityDefinition, FacilityFootprint, FacilityPortDefinition,
    FacilityPortDirection, FacilityPortEdge, FacilityPortPosition, ValidatedFacilityCatalog,
};
use crate::layouts::{FacilityPlacementBounds, FacilityPlacementRequest};
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
    ExactObjectiveKind, ExactObjectiveValue, ExactProofStatus, ExactValidationStatus,
    IntegratedLayoutStatus, TransportNetworkEndpoint, solve_cumulative_scc_growth_v2,
    solve_integrated_layout, sweep_cumulative_integrated_layout_fixed_dimensions,
};
use std::time::Duration;

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
            FacilityDefinition {
                id: "relay-machine".to_string(),
                footprint: FacilityFootprint {
                    width: 1,
                    height: 1,
                },
                allowed_rotations: vec![0],
                ports: vec![
                    FacilityPortDefinition {
                        id: "input".to_string(),
                        direction: FacilityPortDirection::Input,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 0, y: 0 },
                        edge: FacilityPortEdge::West,
                    },
                    FacilityPortDefinition {
                        id: "output".to_string(),
                        direction: FacilityPortDirection::Output,
                        transport: TransportKind::Belt,
                        position: FacilityPortPosition { x: 0, y: 0 },
                        edge: FacilityPortEdge::East,
                    },
                ],
            },
        ],
    })
    .expect("facility fixture should validate");
    let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
        schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
        items: vec![
            ItemDefinition {
                id: "part".to_string(),
                transport: TransportKind::Belt,
            },
            ItemDefinition {
                id: "part-b".to_string(),
                transport: TransportKind::Belt,
            },
        ],
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

fn branching_wiring() -> FacilityInstanceWiringReport {
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
            node("target-b", "target-recipe", "target-machine"),
        ],
        edges: vec![
            FacilityInstanceWiringEdge::original(
                "source",
                "target",
                "intermediate",
                "part",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
            FacilityInstanceWiringEdge::original(
                "source",
                "target-b",
                "intermediate",
                "part",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
        ],
        diagnostics: Vec::new(),
    }
}

fn converging_wiring() -> FacilityInstanceWiringReport {
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
            node("source-a", "source-recipe", "source-machine"),
            node("source-b", "source-recipe", "source-machine"),
            node("target", "target-recipe", "target-machine"),
        ],
        edges: vec![
            FacilityInstanceWiringEdge::original(
                "source-a",
                "target",
                "intermediate",
                "part",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
            FacilityInstanceWiringEdge::original(
                "source-b",
                "target",
                "intermediate",
                "part",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
        ],
        diagnostics: Vec::new(),
    }
}

fn enlarged_chain_wiring() -> FacilityInstanceWiringReport {
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
            node("relay", "relay-recipe", "relay-machine"),
            node("target", "target-recipe", "target-machine"),
        ],
        edges: vec![
            FacilityInstanceWiringEdge::original(
                "source",
                "relay",
                "intermediate",
                "part",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
            FacilityInstanceWiringEdge::original(
                "relay",
                "target",
                "intermediate",
                "part",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
        ],
        diagnostics: Vec::new(),
    }
}

fn two_item_wiring() -> FacilityInstanceWiringReport {
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
            node("source-a", "source-recipe", "source-machine"),
            node("target-a", "target-recipe", "target-machine"),
            node("source-b", "source-recipe", "source-machine"),
            node("target-b", "target-recipe", "target-machine"),
        ],
        edges: vec![
            FacilityInstanceWiringEdge::original(
                "source-a",
                "target-a",
                "first-item",
                "part",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
            FacilityInstanceWiringEdge::original(
                "source-b",
                "target-b",
                "second-item",
                "part-b",
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
        ],
        diagnostics: Vec::new(),
    }
}

fn external_connector_wiring() -> FacilityInstanceWiringReport {
    let rate = Rate {
        numerator: 1,
        denominator: 1,
    };
    FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: vec![
            FacilityInstanceWiringNode::External {
                id: "external-part".to_string(),
                item: "part".to_string(),
            },
            FacilityInstanceWiringNode::Facility {
                id: "processor".to_string(),
                recipe: "processor-recipe".to_string(),
                facility: "relay-machine".to_string(),
                index: 0,
                runs_per_second: rate,
                work_seconds_per_second: rate,
                unused_capacity: Rate::zero(),
            },
            FacilityInstanceWiringNode::Target {
                id: "target-part".to_string(),
                item: "part".to_string(),
            },
        ],
        edges: vec![
            FacilityInstanceWiringEdge::original(
                "external-part",
                "processor",
                "external-input",
                "part",
                rate,
            ),
            FacilityInstanceWiringEdge::original(
                "processor",
                "target-part",
                "target-output",
                "part",
                rate,
            ),
        ],
        diagnostics: Vec::new(),
    }
}

#[test]
fn factored_shared_layer_routes_external_boundary_terminals_in_the_commodity_network() {
    let (facilities, items, transports, components) = catalogs();
    let input = super::prepare_model(
        &external_connector_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: 4,
            max_height: 3,
        },
    )
    .expect("external connector fixture should prepare");

    let report = super::exact::shared_layer::solve_factored_endpoints(input, &components, None);

    assert!(report.success, "{:#?}", report.diagnostics);
    assert_eq!(report.transport_networks.len(), 1);
    assert!(
        report
            .transport_networks
            .iter()
            .flat_map(|network| &network.terminals)
            .filter(|terminal| matches!(
                terminal.endpoint,
                TransportNetworkEndpoint::External { .. }
            ))
            .count()
            == 2
    );
    let exact = report.exact.as_ref().expect("exact solve reports metrics");
    assert_eq!(
        exact.formulation,
        "joint-shared-boundary-terminals-canonical-occupancy-v4"
    );
    assert_eq!(exact.model.boundary_terminal_count, 2);
    assert_eq!(exact.model.commodity_network_count, 1);
    assert_eq!(exact.validation, ExactValidationStatus::Passed);
    let serialized = serde_json::to_value(&report).expect("boundary-terminal report serializes");
    assert!(serialized.get("external_connectors").is_none());
}

#[test]
fn feasibility_only_search_preserves_the_exact_model_and_validates_its_witness() {
    let (facilities, items, transports, components) = catalogs();
    let input = super::prepare_model(
        &external_connector_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: 4,
            max_height: 3,
        },
    )
    .expect("search-mode fixture should prepare");

    let optimized =
        super::exact::shared_layer::solve_factored_endpoints(input.clone(), &components, None);
    let feasible = super::exact::shared_layer::solve_factored_endpoints_feasibility_only(
        input,
        &components,
        None,
    );

    assert!(optimized.success, "{:#?}", optimized.diagnostics);
    assert!(feasible.success, "{:#?}", feasible.diagnostics);
    assert_eq!(feasible.status, IntegratedLayoutStatus::Feasible);
    let optimized_exact = optimized.exact.expect("optimized solve reports evidence");
    let feasible_exact = feasible.exact.expect("feasibility solve reports evidence");
    assert_eq!(feasible_exact.formulation, optimized_exact.formulation);
    assert_eq!(feasible_exact.model, optimized_exact.model);
    assert_eq!(
        feasible_exact.model_complexity,
        optimized_exact.model_complexity
    );
    assert!(feasible_exact.objective_stages.is_empty());
    assert_eq!(feasible_exact.incumbent_count, 1);
    assert_eq!(feasible_exact.validation, ExactValidationStatus::Passed);
}

#[test]
fn fixed_dimension_research_adds_only_two_equalities_and_validates_exact_bounds() {
    let (facilities, items, transports, components) = catalogs();
    let input = super::prepare_model(
        &external_connector_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: 4,
            max_height: 3,
        },
    )
    .expect("fixed-dimension fixture should prepare");

    let unfixed = super::exact::shared_layer::solve_factored_endpoints_feasibility_only(
        input.clone(),
        &components,
        None,
    );
    let fixed =
        super::exact::shared_layer::solve_factored_endpoints_fixed_dimensions_feasibility_only(
            input,
            &components,
            None,
            super::exact::shared_layer::FixedUsedDimensions {
                width: 4,
                height: 3,
            },
        );

    assert!(unfixed.success, "{:#?}", unfixed.diagnostics);
    assert!(fixed.success, "{:#?}", fixed.diagnostics);
    assert_eq!(
        fixed.bounds,
        Some(FacilityPlacementBounds {
            width: 4,
            height: 3,
        })
    );
    let unfixed_exact = unfixed.exact.expect("unfixed solve reports evidence");
    let fixed_exact = fixed.exact.expect("fixed solve reports evidence");
    assert_eq!(
        fixed_exact.formulation,
        "joint-shared-boundary-terminals-canonical-occupancy-v4-fixed-dimensions"
    );
    assert_eq!(
        fixed_exact.model_complexity.variables,
        unfixed_exact.model_complexity.variables
    );
    let unfixed_constraints = unfixed_exact
        .model_complexity
        .constraints
        .expect("unfixed constraints are recorded");
    let fixed_constraints = fixed_exact
        .model_complexity
        .constraints
        .expect("fixed constraints are recorded");
    assert_eq!(
        fixed_constraints.total_constraints,
        unfixed_constraints.total_constraints + 2
    );
    assert_eq!(
        fixed_constraints.total_terms,
        unfixed_constraints.total_terms + 2
    );
    assert_eq!(
        fixed_constraints
            .by_family
            .iter()
            .filter(|family| family.family == "research-fixation")
            .map(|family| family.constraints)
            .sum::<u64>(),
        2
    );
    assert_eq!(fixed_exact.validation, ExactValidationStatus::Passed);
}

#[test]
fn transport_tile_cap_research_adds_only_one_exact_upper_bound() {
    let (facilities, items, transports, components) = catalogs();
    let input = super::prepare_model(
        &external_connector_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: 4,
            max_height: 3,
        },
    )
    .expect("transport-tile-cap fixture should prepare");
    let dimensions = super::exact::shared_layer::FixedUsedDimensions {
        width: 4,
        height: 3,
    };
    let baseline =
        super::exact::shared_layer::solve_factored_endpoints_fixed_dimensions_feasibility_only(
            input.clone(),
            &components,
            None,
            dimensions,
        );
    let capped = super::exact::shared_layer::solve_factored_endpoints_fixed_dimensions_transport_tile_cap_feasibility_only_with_prior(
        input,
        &components,
        None,
        dimensions,
        24,
        None,
    );

    assert!(baseline.success, "{:#?}", baseline.diagnostics);
    assert!(capped.success, "{:#?}", capped.diagnostics);
    let baseline_exact = baseline.exact.expect("baseline reports exact evidence");
    let capped_exact = capped.exact.expect("capped solve reports exact evidence");
    assert_eq!(
        capped_exact.formulation,
        "joint-shared-boundary-terminals-canonical-occupancy-v4-fixed-dimensions-transport-tile-cap"
    );
    assert_eq!(
        capped_exact.model_complexity.variables,
        baseline_exact.model_complexity.variables
    );
    assert_eq!(
        capped_exact
            .model_complexity
            .constraints
            .expect("capped constraints are recorded")
            .total_constraints,
        baseline_exact
            .model_complexity
            .constraints
            .expect("baseline constraints are recorded")
            .total_constraints
            + 1
    );
    assert!(
        capped_exact
            .objective
            .is_some_and(|objective| objective.physical_transport_tiles <= 24)
    );
    assert_eq!(capped_exact.validation, ExactValidationStatus::Passed);
}

#[test]
fn parallel_dimension_sweep_never_skips_at_or_below_its_final_upper_bound() {
    let (facilities, items, transports, components) = catalogs();
    let report = super::research::sweep_first_integrated_layout_phase_fixed_dimensions(
        &external_connector_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: 4,
            max_height: 3,
        },
        &[0],
        2,
        Duration::from_secs(1),
    )
    .expect("parallel dimension sweep should build");

    let upper_bound = report
        .feasible_upper_bound_area
        .expect("controlled sweep should find a validated witness");
    assert!(report.primary_area_optimum_proven);
    assert_eq!(report.actual_worker_count, 2);
    assert!(report.upper_bound_improvements.len() >= 1);
    for case in &report.cases {
        if case.disposition
            == super::research::ExactDimensionCaseDisposition::SkippedAboveUpperBound
        {
            assert!(case.candidate.area > upper_bound);
        }
    }
    for candidate in report
        .candidates
        .iter()
        .filter(|candidate| candidate.area < upper_bound)
    {
        let case = report
            .cases
            .iter()
            .find(|case| case.candidate == *candidate)
            .expect("every smaller candidate has a case record");
        assert_eq!(
            case.outcome,
            Some(super::research::ExactDimensionCaseOutcome::ProvenInfeasible)
        );
    }
}

#[test]
fn cumulative_dimension_sweep_grows_with_non_binding_prior_hints() {
    let (facilities, items, transports, components) = catalogs();
    let report = sweep_cumulative_integrated_layout_fixed_dimensions(
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
        1,
        2,
        Duration::from_secs(1),
    )
    .expect("cumulative exact dimension sweep should build");

    assert!(
        report.completed_target_phase,
        "{:#?}",
        report.layout.diagnostics
    );
    assert_eq!(report.target_phase_index, 1);
    assert_eq!(report.phase_sweeps.len(), 2);
    assert_eq!(report.layout.phases.len(), 2);
    assert!(report.phase_sweeps[0].primary_area_optimum_proven);
    assert!(report.phase_sweeps[1].primary_area_optimum_proven);
    let phase_zero_exact = report.phase_sweeps[0]
        .selected_incumbent
        .as_ref()
        .and_then(|layout| layout.exact.as_ref())
        .expect("phase zero has exact evidence");
    let phase_one_exact = report.phase_sweeps[1]
        .selected_incumbent
        .as_ref()
        .and_then(|layout| layout.exact.as_ref())
        .expect("phase one has exact evidence");
    assert_eq!(phase_zero_exact.model.hint_variables, 0);
    assert!(phase_one_exact.model.hint_variables > 0);
    assert_eq!(phase_one_exact.model.hinted_placements, 1);
}

#[test]
fn factored_endpoint_uses_one_geometry_lookup_per_compatible_port() {
    let (facilities, items, transports, components) = catalogs();
    let request = FacilityPlacementRequest {
        schema_version: 2,
        max_width: 4,
        max_height: 3,
    };
    let baseline_input = super::prepare_model(
        &external_connector_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    )
    .expect("single-port boundary-terminal fixture should prepare");
    let baseline =
        super::exact::shared_layer::solve_factored_endpoints(baseline_input, &components, None);
    assert!(baseline.success, "{:#?}", baseline.diagnostics);
    let baseline_exact = baseline
        .exact
        .expect("baseline exact solve reports metrics");
    let baseline_endpoint_links = baseline_exact
        .model_complexity
        .constraints
        .as_ref()
        .expect("baseline records constraint families")
        .by_family
        .iter()
        .filter(|family| family.family == "endpoint-link")
        .map(|family| family.constraints)
        .sum::<u64>();
    let mut facility_catalog = facilities.catalog().clone();
    facility_catalog
        .facilities
        .iter_mut()
        .find(|facility| facility.id == "relay-machine")
        .expect("relay facility exists")
        .ports
        .push(FacilityPortDefinition {
            id: "input-south".to_string(),
            direction: FacilityPortDirection::Input,
            transport: TransportKind::Belt,
            position: FacilityPortPosition { x: 0, y: 0 },
            edge: FacilityPortEdge::South,
        });
    let facilities = ValidatedFacilityCatalog::try_from_catalog(facility_catalog)
        .expect("multi-port fixture should validate");
    let input = super::prepare_model(
        &external_connector_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    )
    .expect("multi-port external connector fixture should prepare");

    let report = super::exact::shared_layer::solve_factored_endpoints(input, &components, None);

    assert!(report.success, "{:#?}", report.diagnostics);
    let exact = report.exact.expect("exact solve reports metrics");
    let endpoint_links = exact
        .model_complexity
        .constraints
        .as_ref()
        .expect("comparison records constraint families")
        .by_family
        .iter()
        .filter(|family| family.family == "endpoint-link")
        .map(|family| family.constraints)
        .sum::<u64>();
    assert_eq!(endpoint_links, baseline_endpoint_links + 1);
    assert_eq!(exact.validation, ExactValidationStatus::Passed);
}

#[test]
fn shared_layer_matches_dense_objective_for_two_belt_items() {
    let (facilities, items, transports, components) = catalogs();
    let request = FacilityPlacementRequest {
        schema_version: 2,
        max_width: 6,
        max_height: 2,
    };
    let input = super::prepare_model(
        &two_item_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    )
    .expect("two-item fixture should prepare");
    let dense = super::exact::solve_with_prior_solution(input.clone(), &components, None, None);
    let shared = super::exact::shared_layer::solve(input.clone(), &components, None);
    let factored = super::exact::shared_layer::solve_factored_endpoints(input, &components, None);

    assert!(dense.success, "dense diagnostics: {:#?}", dense.diagnostics);
    assert!(
        shared.success,
        "shared-layer diagnostics: {:#?}",
        shared.diagnostics
    );
    assert!(
        factored.success,
        "factored endpoint diagnostics: {:#?}",
        factored.diagnostics
    );
    assert_eq!(
        dense.exact.as_ref().and_then(|exact| exact.objective),
        shared.exact.as_ref().and_then(|exact| exact.objective)
    );
    assert_eq!(
        dense.exact.as_ref().and_then(|exact| exact.objective),
        factored.exact.as_ref().and_then(|exact| exact.objective)
    );
    assert!(
        shared
            .exact
            .as_ref()
            .expect("shared solve has exact metrics")
            .model
            .route_arc_variables
            < dense
                .exact
                .as_ref()
                .expect("dense solve has exact metrics")
                .model
                .route_arc_variables
    );
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
            .any(|diagnostic| diagnostic.code == "experimental-shared-boundary-terminals")
    );
    let serialized = serde_json::to_value(&report).expect("report should serialize");
    assert!(serialized.get("transport_networks").is_some());
    assert!(serialized.get("routes").is_none());
    assert_eq!(report.phases.len(), 2);
    assert_eq!(report.phases[0].cumulative_facility_count, 1);
    assert_eq!(report.phases[1].cumulative_facility_count, 2);
    assert_eq!(report.phases[0].exact.model.hint_variables, 0);
    assert!(report.phases[1].exact.model.hint_variables > 0);
    let serialized_phase = &serialized["phases"][0];
    assert!(serialized_phase.get("exact").is_some());
    assert!(serialized_phase.get("attempts").is_none());
    assert!(serialized_phase.get("optimization").is_none());
    let exact = report.exact.expect("exact metrics should be present");
    assert_eq!(
        exact.formulation,
        "joint-shared-boundary-terminals-canonical-occupancy-v4"
    );
    assert!(
        exact
            .model_complexity
            .variables
            .by_family
            .iter()
            .all(|family| family.family != "route-order")
    );
    assert!(
        exact
            .model_complexity
            .constraints
            .as_ref()
            .expect("recorded exact model should include constraint metrics")
            .by_family
            .iter()
            .all(|family| family.family != "acyclicity")
    );
    assert_eq!(exact.model.commodity_network_count, 1);
    assert_eq!(exact.model.commodity_item_count, 1);
    assert_eq!(exact.model.belt_network_count, 1);
    assert_eq!(exact.model.pipe_network_count, 0);
    assert_eq!(exact.model.network_requirement_reference_count, 1);
    assert_eq!(exact.model.network_terminal_count, 2);
    assert_eq!(exact.model.external_terminal_count, 0);
    assert!(exact.model.network_flow_variables > 0);
    assert!(exact.model.objective_variables > 0);
    assert_eq!(
        exact.objective,
        Some(ExactObjectiveValue {
            used_bounding_box_area: 3,
            physical_transport_tiles: 1,
            total_route_turns: 0,
            maximum_used_side: 3,
            logistics_component_count: 0,
        })
    );
    assert_eq!(
        exact
            .objective_stages
            .iter()
            .map(|stage| stage.objective)
            .collect::<Vec<_>>(),
        vec![
            ExactObjectiveKind::UsedBoundingBoxArea,
            ExactObjectiveKind::PhysicalTransportTiles,
            ExactObjectiveKind::TotalRouteTurns,
            ExactObjectiveKind::MaximumUsedSide,
            ExactObjectiveKind::LogisticsComponentCount,
        ]
    );
    assert!(exact.objective_stages.iter().all(|stage| {
        stage.proof == ExactProofStatus::ProvenOptimal && stage.incumbent == stage.best_bound
    }));
    assert_eq!(exact.proof, ExactProofStatus::ProvenOptimal);
    assert_eq!(exact.validation, ExactValidationStatus::Passed);
}

#[test]
fn v2_cumulative_growth_reuses_placement_hints_without_cutting_over_production() {
    let (facilities, items, transports, components) = catalogs();
    let report = solve_cumulative_scc_growth_v2(
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
        1,
        Duration::from_secs(1),
    )
    .expect("two-phase fixture should build");

    assert!(report.layout.success, "{:#?}", report.layout.diagnostics);
    assert_eq!(report.target_phase_index, 1);
    assert_eq!(report.total_phase_count, 2);
    assert_eq!(report.layout.phases.len(), 2);
    assert_eq!(report.layout.phases[0].exact.model.hint_variables, 0);
    assert!(report.layout.phases[1].exact.model.hint_variables > 0);
    assert_eq!(
        report
            .layout
            .exact
            .expect("final exact metrics")
            .formulation,
        "joint-shared-boundary-terminals-canonical-occupancy-v4"
    );
}

#[test]
fn prior_solution_warm_start_is_non_binding() {
    let (facilities, items, transports, components) = catalogs();
    let request = FacilityPlacementRequest {
        schema_version: 2,
        max_width: 4,
        max_height: 1,
    };
    let baseline = solve_integrated_layout(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    );
    let baseline_objective = baseline
        .exact
        .as_ref()
        .and_then(|exact| exact.objective)
        .expect("baseline should have an exact objective");
    let matching_input = super::prepare_model(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    )
    .expect("fixture should prepare");
    let matching_hint =
        super::exact::solve_with_prior_solution(matching_input, &components, None, Some(&baseline));
    assert_eq!(
        matching_hint
            .exact
            .expect("matching hint should report metrics")
            .model
            .hinted_terminals,
        2,
        "terminal hints should map when both port geometry and owning placement agree"
    );
    let mut conflicting_hint = baseline.clone();
    for placement in &mut conflicting_hint.placements {
        placement.x = if placement.instance == "source" { 3 } else { 0 };
    }
    let input = super::prepare_model(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    )
    .expect("fixture should prepare");

    let hinted =
        super::exact::solve_with_prior_solution(input, &components, None, Some(&conflicting_hint));

    assert!(hinted.success, "{:#?}", hinted.diagnostics);
    assert_eq!(
        hinted.exact.as_ref().and_then(|exact| exact.objective),
        Some(baseline_objective),
        "a conflicting warm start must not change the exact optimum"
    );
    let metrics = hinted
        .exact
        .expect("hinted solve should report metrics")
        .model;
    assert!(metrics.hint_variables > 0);
    assert_eq!(metrics.hinted_placements, 2);
    assert_eq!(
        metrics.hinted_terminals, 0,
        "terminal hints must not map to a different placement candidate with the same port geometry"
    );
    assert_eq!(metrics.hinted_networks, 1);
}

#[test]
fn factored_shared_layer_reuses_only_non_binding_placement_hints() {
    let (facilities, items, transports, components) = catalogs();
    let request = FacilityPlacementRequest {
        schema_version: 2,
        max_width: 4,
        max_height: 1,
    };
    let prior = solve_integrated_layout(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    );
    let input = super::prepare_model(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    )
    .expect("fixture should prepare");

    let hinted = super::exact::shared_layer::solve_factored_endpoints_with_prior(
        input,
        &components,
        None,
        Some(&prior),
    );

    assert!(hinted.success, "{:#?}", hinted.diagnostics);
    let metrics = hinted.exact.expect("hinted solve reports metrics").model;
    assert!(metrics.hint_variables > 0);
    assert_eq!(metrics.hinted_placements, 2);
    assert_eq!(metrics.hinted_terminals, 0);
    assert_eq!(metrics.hinted_networks, 0);
}

#[test]
fn prior_solution_warm_start_maps_only_the_common_enlarged_graph() {
    let (facilities, items, transports, components) = catalogs();
    let small_request = FacilityPlacementRequest {
        schema_version: 2,
        max_width: 4,
        max_height: 1,
    };
    let prior = solve_integrated_layout(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &small_request,
    );
    let enlarged_request = FacilityPlacementRequest {
        schema_version: 2,
        max_width: 5,
        max_height: 1,
    };
    let unhinted = solve_integrated_layout(
        &enlarged_chain_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &enlarged_request,
    );
    let input = super::prepare_model(
        &enlarged_chain_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &enlarged_request,
    )
    .expect("enlarged fixture should prepare");
    let hinted = super::exact::solve_with_prior_solution(input, &components, None, Some(&prior));

    assert!(hinted.success, "{:#?}", hinted.diagnostics);
    assert_eq!(
        hinted.exact.as_ref().and_then(|exact| exact.objective),
        unhinted.exact.as_ref().and_then(|exact| exact.objective),
        "partial hints for old variables must preserve the enlarged exact optimum"
    );
    let metrics = hinted
        .exact
        .expect("hinted solve should report metrics")
        .model;
    assert!(metrics.hint_variables > 0);
    assert_eq!(metrics.hinted_placements, 2);
    assert_eq!(metrics.hinted_terminals, 0);
    assert_eq!(metrics.hinted_networks, 1);
    assert_eq!(hinted.placements.len(), 3);
}

#[test]
fn exact_solver_selects_and_validates_a_splitter_for_shared_flow() {
    let (facilities, items, transports, components) = catalogs();
    let report = solve_integrated_layout(
        &branching_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: 6,
            max_height: 4,
        },
    );

    assert!(report.success, "{:#?}", report.diagnostics);
    assert!(
        report
            .logistics_components
            .iter()
            .any(|component| { component.kind == LogisticsComponentKind::Splitter })
    );
    assert_eq!(
        report
            .logistics_components
            .iter()
            .filter(|component| component.kind == LogisticsComponentKind::Converger)
            .count(),
        0
    );
    assert_eq!(
        report.exact.expect("exact metrics").validation,
        ExactValidationStatus::Passed
    );
}

#[test]
fn exact_solver_selects_and_validates_a_converger_for_shared_flow() {
    let (facilities, items, transports, components) = catalogs();
    let report = solve_integrated_layout(
        &converging_wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &FacilityPlacementRequest {
            schema_version: 2,
            max_width: 6,
            max_height: 4,
        },
    );

    assert!(report.success, "{:#?}", report.diagnostics);
    assert!(
        report
            .logistics_components
            .iter()
            .any(|component| { component.kind == LogisticsComponentKind::Converger })
    );
    assert_eq!(
        report.exact.expect("exact metrics").validation,
        ExactValidationStatus::Passed
    );
}

#[test]
fn network_witness_rejects_terminal_flow_that_does_not_match_the_logical_graph() {
    let (facilities, items, transports, components) = catalogs();
    let request = FacilityPlacementRequest {
        schema_version: 2,
        max_width: 4,
        max_height: 1,
    };
    let input = super::prepare_model(
        &wiring(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
    )
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
