use std::path::PathBuf;
use std::time::{Duration, Instant};

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{
    FacilityPlacementRequest, IncumbentProvenance, SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
    construct_sparse_integrated_layout, extend_phase_incumbent,
};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use aic_data::recipes::{
    FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
    FacilityInstanceWiringNode, FacilityInstanceWiringProjectedEndpoint,
    FacilityInstanceWiringProjection, FacilityInstanceWiringReport, Rate,
};

#[test]
fn extension_reuses_unchanged_route_and_replaces_frontier_projection() {
    let catalogs = Catalogs::load();
    let request = FacilityPlacementRequest {
        schema_version: SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
        max_width: 50,
        max_height: 50,
    };
    let (previous, current, unchanged_requirement_id) = chain_phase_wiring();
    let prior = construct_sparse_integrated_layout(
        &previous,
        &catalogs.facilities,
        &catalogs.items,
        &catalogs.transports,
        &catalogs.components,
        &request,
    );
    assert!(prior.success, "{:#?}", prior.diagnostics);
    let prior_unchanged = prior
        .routes
        .iter()
        .find(|route| route.requirement_id == unchanged_requirement_id)
        .expect("previous phase contains the output route")
        .clone();

    let extended = extend_phase_incumbent(
        &previous,
        &current,
        &catalogs.facilities,
        &catalogs.items,
        &catalogs.transports,
        &catalogs.components,
        &request,
        &prior,
        1,
        Instant::now() + Duration::from_secs(2),
    );

    assert!(
        extended.diagnostics.is_empty(),
        "{:#?}",
        extended.diagnostics
    );
    assert_eq!(extended.counts.reused_facilities, 1);
    assert_eq!(extended.counts.newly_placed_facilities, 1);
    assert_eq!(extended.counts.reused_routes, 1);
    assert_eq!(extended.counts.invalidated_routes, 2);
    assert_eq!(extended.counts.rerouted_routes, 1);
    assert_eq!(extended.counts.new_routes, 1);
    let incumbent = extended
        .incumbent
        .expect("extension should yield an incumbent");
    assert_eq!(
        incumbent.provenance,
        IncumbentProvenance::ExtendedPriorPhase
    );
    assert_eq!(incumbent.witness.placements.len(), 2);
    assert_eq!(incumbent.witness.routes.len(), 3);
    let new_placement = incumbent
        .witness
        .placements
        .iter()
        .find(|placement| placement.instance == "facility:a")
        .expect("extension places the new facility");
    assert!(prior_unchanged.cells.iter().all(|cell| {
        cell.x < new_placement.x
            || cell.x >= new_placement.x + new_placement.width
            || cell.y < new_placement.y
            || cell.y >= new_placement.y + new_placement.height
    }));
    assert_eq!(
        incumbent
            .witness
            .routes
            .iter()
            .find(|route| route.requirement_id == unchanged_requirement_id),
        Some(&prior_unchanged),
    );
}

#[test]
fn failed_extension_returns_a_conflict_without_a_false_incumbent() {
    let catalogs = Catalogs::load();
    let request = FacilityPlacementRequest {
        schema_version: SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
        max_width: 50,
        max_height: 50,
    };
    let (previous, current, _) = chain_phase_wiring();
    let prior = construct_sparse_integrated_layout(
        &previous,
        &catalogs.facilities,
        &catalogs.items,
        &catalogs.transports,
        &catalogs.components,
        &request,
    );
    assert!(prior.success, "{:#?}", prior.diagnostics);

    let failed = extend_phase_incumbent(
        &previous,
        &current,
        &catalogs.facilities,
        &catalogs.items,
        &catalogs.transports,
        &catalogs.components,
        &request,
        &prior,
        1,
        Instant::now(),
    );

    assert!(failed.incumbent.is_none());
    let conflict = failed.conflict.expect("failure should identify a conflict");
    assert_eq!(conflict.code, "incumbent-extension-time-limit");
    assert_eq!(conflict.related_facility_ids, ["facility:a"]);
    assert_eq!(failed.diagnostics.len(), 1);
    assert_eq!(failed.diagnostics[0].code, "incumbent-extension-failed");
}

#[test]
fn branch_extension_reuses_every_unchanged_route_exactly() {
    let catalogs = Catalogs::load();
    let request = FacilityPlacementRequest {
        schema_version: SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
        max_width: 50,
        max_height: 50,
    };
    let (previous, current, unchanged_requirement_ids) = branch_phase_wiring();
    let prior = construct_sparse_integrated_layout(
        &previous,
        &catalogs.facilities,
        &catalogs.items,
        &catalogs.transports,
        &catalogs.components,
        &request,
    );
    assert!(prior.success, "{:#?}", prior.diagnostics);

    let extended = extend_phase_incumbent(
        &previous,
        &current,
        &catalogs.facilities,
        &catalogs.items,
        &catalogs.transports,
        &catalogs.components,
        &request,
        &prior,
        1,
        Instant::now() + Duration::from_secs(2),
    );

    assert!(
        extended.diagnostics.is_empty(),
        "{:#?}",
        extended.diagnostics
    );
    assert_eq!(extended.counts.reused_routes, 2);
    assert_eq!(extended.counts.invalidated_routes, 2);
    assert_eq!(extended.counts.rerouted_routes, 1);
    assert_eq!(extended.counts.new_routes, 1);
    let witness = &extended
        .incumbent
        .expect("branch extension should yield an incumbent")
        .witness;
    for requirement_id in unchanged_requirement_ids {
        assert_eq!(
            witness
                .routes
                .iter()
                .find(|route| route.requirement_id == requirement_id),
            prior
                .routes
                .iter()
                .find(|route| route.requirement_id == requirement_id),
        );
    }
}

struct Catalogs {
    facilities: ValidatedFacilityCatalog,
    items: ValidatedItemCatalog,
    transports: ValidatedTransportCatalog,
    components: ValidatedLogisticsComponentCatalog,
}

impl Catalogs {
    fn load() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        Self {
            facilities: ValidatedFacilityCatalog::try_from_catalog(
                load_facility_catalog(root.join("data/game/normalized/facilities.json"))
                    .expect("facility catalog should load"),
            )
            .expect("facility catalog should validate"),
            items: ValidatedItemCatalog::try_from_catalog(
                load_item_catalog(root.join("data/game/normalized/items.json"))
                    .expect("item catalog should load"),
            )
            .expect("item catalog should validate"),
            transports: ValidatedTransportCatalog::try_from_catalog(
                load_transport_catalog(root.join("data/game/normalized/transports.json"))
                    .expect("transport catalog should load"),
            )
            .expect("transport catalog should validate"),
            components: ValidatedLogisticsComponentCatalog::try_from_catalog(
                load_logistics_component_catalog(
                    root.join("data/game/normalized/logistics-components.json"),
                )
                .expect("logistics component catalog should load"),
            )
            .expect("logistics component catalog should validate"),
        }
    }
}

fn chain_phase_wiring() -> (
    FacilityInstanceWiringReport,
    FacilityInstanceWiringReport,
    String,
) {
    let internal = edge("facility:a", "facility:b", "production");
    let external_input = edge("external:input", "facility:a", "external-input");
    let output = edge("facility:b", "target:output", "target");
    let frontier_id = format!("iterative-external:{}", internal.id);
    let previous = FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: vec![
            facility("facility:b"),
            FacilityInstanceWiringNode::External {
                id: frontier_id.clone(),
                item: "item-xiranite-powder".to_string(),
            },
            target(),
        ],
        edges: vec![
            FacilityInstanceWiringEdge {
                source: frontier_id,
                projection: FacilityInstanceWiringProjection::FrontierExternal {
                    missing_facility: "facility:a".to_string(),
                    original_endpoint: FacilityInstanceWiringProjectedEndpoint::Source,
                },
                ..internal.clone()
            },
            output.clone(),
        ],
        diagnostics: Vec::new(),
    };
    let current = FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: vec![
            facility("facility:a"),
            facility("facility:b"),
            FacilityInstanceWiringNode::External {
                id: "external:input".to_string(),
                item: "item-xiranite-powder".to_string(),
            },
            target(),
        ],
        edges: vec![external_input, internal, output.clone()],
        diagnostics: Vec::new(),
    };
    (previous, current, format!("{}:lane:0000", output.id))
}

fn branch_phase_wiring() -> (
    FacilityInstanceWiringReport,
    FacilityInstanceWiringReport,
    Vec<String>,
) {
    let a_to_c = edge("facility:a", "facility:c", "production-a");
    let b_to_c = edge("facility:b", "facility:c", "production-b");
    let input_a = edge("external:input-a", "facility:a", "external-input");
    let output = edge("facility:c", "target:output", "target");
    let frontier_a_id = format!("iterative-external:{}", a_to_c.id);
    let frontier_b_id = format!("iterative-external:{}", b_to_c.id);
    let projected_a = FacilityInstanceWiringEdge {
        source: frontier_a_id.clone(),
        projection: FacilityInstanceWiringProjection::FrontierExternal {
            missing_facility: "facility:a".to_string(),
            original_endpoint: FacilityInstanceWiringProjectedEndpoint::Source,
        },
        ..a_to_c.clone()
    };
    let projected_b = FacilityInstanceWiringEdge {
        source: frontier_b_id.clone(),
        projection: FacilityInstanceWiringProjection::FrontierExternal {
            missing_facility: "facility:b".to_string(),
            original_endpoint: FacilityInstanceWiringProjectedEndpoint::Source,
        },
        ..b_to_c
    };
    let previous = FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: vec![
            facility("facility:c"),
            external(&frontier_a_id),
            external(&frontier_b_id),
            target(),
        ],
        edges: vec![projected_a, projected_b.clone(), output.clone()],
        diagnostics: Vec::new(),
    };
    let current = FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: vec![
            facility("facility:a"),
            facility("facility:c"),
            external("external:input-a"),
            external(&frontier_b_id),
            target(),
        ],
        edges: vec![input_a, a_to_c, projected_b.clone(), output.clone()],
        diagnostics: Vec::new(),
    };
    (
        previous,
        current,
        vec![
            format!("{}:lane:0000", projected_b.id),
            format!("{}:lane:0000", output.id),
        ],
    )
}

fn facility(id: &str) -> FacilityInstanceWiringNode {
    FacilityInstanceWiringNode::Facility {
        id: id.to_string(),
        recipe: format!("recipe:{id}"),
        facility: "component-mc-1-mode-normal".to_string(),
        index: 0,
        runs_per_second: unit_rate(),
        work_seconds_per_second: unit_rate(),
        unused_capacity: Rate {
            numerator: 0,
            denominator: 1,
        },
    }
}

fn target() -> FacilityInstanceWiringNode {
    FacilityInstanceWiringNode::Target {
        id: "target:output".to_string(),
        item: "item-xiranite-powder".to_string(),
    }
}

fn external(id: &str) -> FacilityInstanceWiringNode {
    FacilityInstanceWiringNode::External {
        id: id.to_string(),
        item: "item-xiranite-powder".to_string(),
    }
}

fn edge(source: &str, target: &str, kind: &str) -> FacilityInstanceWiringEdge {
    FacilityInstanceWiringEdge::original(
        source,
        target,
        kind,
        "item-xiranite-powder",
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
