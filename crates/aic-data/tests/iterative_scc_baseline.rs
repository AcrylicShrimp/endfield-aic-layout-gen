use std::path::PathBuf;
use std::time::Duration;

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    FacilityPlacementRequest, IntegratedRouteEndpoint,
    construct_iterative_scc_layout_with_time_limit, plan_facility_growth,
};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use aic_data::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode, FacilityInstanceWiringReport, Rate,
};

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
fn one_facility_baseline_exposes_known_bad_perimeter_routing() {
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

    let report = construct_iterative_scc_layout_with_time_limit(
        &one_facility_wiring_fixture(),
        &facilities,
        &items,
        &transports,
        &components,
        &request,
        Duration::from_secs(2),
    );

    assert!(report.success, "{:#?}", report.diagnostics);
    assert_eq!(report.phases.len(), 1);
    let phase = &report.phases[0];
    assert_eq!(phase.cumulative_facility_count, 1);
    assert_eq!(phase.routes.len(), 3);
    assert!(
        phase.route_cells > phase.routes.len(),
        "known-bad boundary terminals should force routes longer than one cell"
    );
    for route in &phase.routes {
        let boundary = match (&route.source, &route.target) {
            (IntegratedRouteEndpoint::Boundary { .. }, _) => route.cells.first(),
            (_, IntegratedRouteEndpoint::Boundary { .. }) => route.cells.last(),
            _ => panic!("fixture route should have exactly one boundary endpoint"),
        }
        .expect("fixture route should contain a boundary cell");
        assert!(
            boundary.x == 0
                || boundary.y == 0
                || boundary.x == phase.bounds.width - 1
                || boundary.y == phase.bounds.height - 1,
            "boundary endpoint must lie on the reported prefix perimeter"
        );
    }
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
    plan_facility_growth(&FacilityInstanceWiringReport {
        success: true,
        nodes,
        edges,
        diagnostics: Vec::new(),
    })
}

fn one_facility_wiring_fixture() -> FacilityInstanceWiringReport {
    FacilityInstanceWiringReport {
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
    FacilityInstanceWiringEdge {
        source: source.to_string(),
        target: target.to_string(),
        kind: kind.to_string(),
        item: item.to_string(),
        rate: Rate {
            numerator: 1,
            denominator: 10,
        },
    }
}

fn unit_rate() -> Rate {
    Rate {
        numerator: 1,
        denominator: 1,
    }
}
