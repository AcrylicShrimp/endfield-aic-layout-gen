use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    FacilityPlacementRequest, IntegratedRouteEndpoint, IterativeOptimizationConfig,
    construct_iterative_scc_layout, plan_facility_growth,
};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use aic_data::recipes::{
    FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
    FacilityInstanceWiringNode, FacilityInstanceWiringReport, Rate,
};
use std::path::PathBuf;

#[test]
fn baseline_graph_fixtures_have_deterministic_output_first_ids() {
    let chain = graph_fixture(&["a", "b", "c"], &[("a", "b"), ("b", "c")], &["c"]);
    let branch = graph_fixture(
        &["a", "b", "c", "d"],
        &[("a", "c"), ("b", "c"), ("b", "d")],
        &["c", "d"],
    );
    let cycle = graph_fixture(
        &["a", "b", "c"],
        &[("a", "b"), ("b", "a"), ("b", "c")],
        &["c"],
    );

    assert_eq!(
        phase_facilities(&chain),
        vec![vec!["c"], vec!["b"], vec!["a"]]
    );
    assert_eq!(
        phase_facilities(&branch),
        vec![vec!["c", "d"], vec!["a", "b"]]
    );
    assert_eq!(phase_facilities(&cycle), vec![vec!["c"], vec!["a", "b"]]);
    assert_eq!(cycle.components[0].id, "component:0000");
    assert_eq!(cycle.components[0].facilities, vec!["a", "b"]);
}

#[test]
fn one_facility_external_routes_are_minimal_and_search_domain_independent() {
    let root = repository_root();
    let facilities = ValidatedFacilityCatalog::try_from_catalog(
        load_facility_catalog(root.join("data/game/normalized/facilities.json"))
            .expect("game facility catalog should load"),
    )
    .expect("game facility catalog should validate");
    let items = ValidatedItemCatalog::try_from_catalog(
        load_item_catalog(root.join("data/game/normalized/items.json"))
            .expect("game item catalog should load"),
    )
    .expect("game item catalog should validate");
    let transports = ValidatedTransportCatalog::try_from_catalog(
        load_transport_catalog(root.join("data/game/normalized/transports.json"))
            .expect("game transport catalog should load"),
    )
    .expect("game transport catalog should validate");
    let components = ValidatedLogisticsComponentCatalog::try_from_catalog(
        load_logistics_component_catalog(
            root.join("data/game/normalized/logistics-components.json"),
        )
        .expect("game logistics component catalog should load"),
    )
    .expect("game logistics component catalog should validate");
    let request: FacilityPlacementRequest = serde_json::from_str(
        &std::fs::read_to_string(root.join("data/examples/placement.factory-500.request.json"))
            .expect("factory placement request should load"),
    )
    .expect("factory placement request should parse");

    let config = IterativeOptimizationConfig {
        total_time_limit_ms: 2_000,
        ..IterativeOptimizationConfig::default()
    };
    let large_report = construct_iterative_scc_layout(
        &one_facility_wiring_fixture(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
        &config,
    );
    let small_request = FacilityPlacementRequest {
        schema_version: request.schema_version,
        max_width: 50,
        max_height: 50,
    };
    let small_report = construct_iterative_scc_layout(
        &one_facility_wiring_fixture(),
        &facilities,
        &items,
        &transports,
        &components,
        &small_request,
        &config,
    );

    for (report, expected_search_width) in [(&large_report, 500), (&small_report, 50)] {
        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.phases.len(), 2);
        let phase = &report.phases[0];
        assert_eq!(phase.ready_component_count, 1);
        assert_eq!(phase.selected_component_count, 1);
        assert_eq!(phase.deferred_component_count, 0);
        assert_eq!(phase.oversized_component_count, 0);
        assert_eq!(phase.cumulative_facility_count, 1);
        assert_eq!(phase.cumulative_route_requirement_count, 3);
        assert_eq!(phase.routes.len(), 3);
        assert_eq!(phase.route_cells, phase.routes.len());
        assert_eq!(
            phase.optimization.final_incumbent.score.total_route_cells,
            phase.route_cells
        );
        assert!(phase.optimization.candidate_counts.validated >= 2);
        assert_eq!(
            phase.optimization.search_bounds.width,
            expected_search_width
        );
        let final_refinement = &report.phases[1];
        assert!(final_refinement.introduced_components.is_empty());
        assert_eq!(final_refinement.routes.len(), 3);
        assert_eq!(final_refinement.route_cells, 3);
        assert_eq!(final_refinement.optimization.neighborhoods[0].rank, 3);
        for route in &phase.routes {
            assert_eq!(route.cells.len(), 1);
            assert!(matches!(
                (&route.source, &route.target),
                (
                    IntegratedRouteEndpoint::External { .. },
                    IntegratedRouteEndpoint::Facility { .. }
                ) | (
                    IntegratedRouteEndpoint::Facility { .. },
                    IntegratedRouteEndpoint::External { .. }
                )
            ));
        }
    }
    assert_eq!(large_report.bounds, small_report.bounds);
    assert_eq!(large_report.placements, small_report.placements);
    assert_eq!(large_report.routes, small_report.routes);
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn phase_facilities(report: &aic_data::layouts::FacilityGrowthPlanReport) -> Vec<Vec<&str>> {
    report
        .phases
        .iter()
        .map(|phase| phase.facilities.iter().map(String::as_str).collect())
        .collect()
}

fn graph_fixture(
    facility_ids: &[&str],
    internal_edges: &[(&str, &str)],
    outputs: &[&str],
) -> aic_data::layouts::FacilityGrowthPlanReport {
    let mut nodes = facility_ids
        .iter()
        .map(|id| facility_node(id, "fixture-facility"))
        .collect::<Vec<_>>();
    nodes.push(FacilityInstanceWiringNode::Target {
        id: "target".to_string(),
        item: "item".to_string(),
    });
    let mut edges = internal_edges
        .iter()
        .map(|(source, target)| wiring_edge(source, target, "item", "production"))
        .collect::<Vec<_>>();
    edges.extend(
        outputs
            .iter()
            .map(|source| wiring_edge(source, "target", "item", "target")),
    );
    plan_facility_growth(
        &FacilityInstanceWiringReport {
            schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes,
            edges,
            diagnostics: Vec::new(),
        },
        8,
    )
}

fn one_facility_wiring_fixture() -> FacilityInstanceWiringReport {
    FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: vec![
            facility_node("facility:oven", "xiranite-oven-1-mode-liquid"),
            FacilityInstanceWiringNode::External {
                id: "external:solid".to_string(),
                item: "item-xiranite-powder".to_string(),
            },
            FacilityInstanceWiringNode::External {
                id: "external:liquid".to_string(),
                item: "item-liquid-xiranite-poly".to_string(),
            },
            FacilityInstanceWiringNode::Target {
                id: "target:product".to_string(),
                item: "item-xiranite-enr-powder".to_string(),
            },
        ],
        edges: vec![
            wiring_edge(
                "external:solid",
                "facility:oven",
                "item-xiranite-powder",
                "external-input",
            ),
            wiring_edge(
                "external:liquid",
                "facility:oven",
                "item-liquid-xiranite-poly",
                "external-input",
            ),
            wiring_edge(
                "facility:oven",
                "target:product",
                "item-xiranite-enr-powder",
                "target",
            ),
        ],
        diagnostics: Vec::new(),
    }
}

fn facility_node(id: &str, facility: &str) -> FacilityInstanceWiringNode {
    FacilityInstanceWiringNode::Facility {
        id: id.to_string(),
        recipe: format!("recipe:{id}"),
        facility: facility.to_string(),
        index: 0,
        runs_per_second: unit_rate(),
        work_seconds_per_second: unit_rate(),
        unused_capacity: Rate {
            numerator: 0,
            denominator: 1,
        },
    }
}

fn wiring_edge(source: &str, target: &str, item: &str, kind: &str) -> FacilityInstanceWiringEdge {
    FacilityInstanceWiringEdge::original(
        source,
        target,
        kind,
        item,
        Rate {
            numerator: 1,
            denominator: 10,
        },
    )
}

fn unit_rate() -> Rate {
    Rate {
        numerator: 1,
        denominator: 1,
    }
}
