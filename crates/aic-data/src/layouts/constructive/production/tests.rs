use super::*;
use crate::logistics::{ItemCatalog, ItemDefinition};
use crate::recipes::{FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringProjection};

fn catalogs() -> (
    ValidatedFacilityCatalog,
    ValidatedItemCatalog,
    ValidatedTransportCatalog,
    ValidatedLogisticsComponentCatalog,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let f = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(
            root.join("data/examples/constructive-network.facilities.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let i = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
        schema_version: 1,
        items: vec![ItemDefinition {
            id: "ore".into(),
            transport: TransportKind::Belt,
        }],
    })
    .unwrap();
    let t = ValidatedTransportCatalog::try_from_catalog(
        crate::logistics::load_transport_catalog(root.join("data/game/normalized/transports.json"))
            .unwrap(),
    )
    .unwrap();
    (f, i, t, super::super::test_components())
}
fn wiring(edges: &[(&str, &str)]) -> FacilityInstanceWiringReport {
    let ids = edges
        .iter()
        .flat_map(|(a, b)| [*a, *b])
        .collect::<BTreeSet<_>>();
    let rate = Rate {
        numerator: 1,
        denominator: 5,
    };
    FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: ids
            .into_iter()
            .map(|id| FacilityInstanceWiringNode::Facility {
                id: id.into(),
                recipe: format!("recipe-{id}"),
                facility: "network-fixture-machine".into(),
                index: 0,
                runs_per_second: rate,
                work_seconds_per_second: rate,
                unused_capacity: Rate::zero(),
            })
            .collect(),
        edges: edges
            .iter()
            .enumerate()
            .map(|(i, (a, b))| FacilityInstanceWiringEdge {
                id: format!("edge-{i}"),
                source: (*a).into(),
                target: (*b).into(),
                kind: "recipe-flow".into(),
                item: "ore".into(),
                rate,
                projection: FacilityInstanceWiringProjection::Original,
            })
            .collect(),
        diagnostics: Vec::new(),
    }
}
fn request(seed: &str) -> ConstructiveProductionRequest {
    ConstructiveProductionRequest {
        schema_version: 1,
        target_instance: seed.into(),
        max_steps: 64,
        max_placement_attempts: 128,
        max_route_searches: 512,
    }
}

#[test]
fn completes_connected_chain_without_compact_module_discovery() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c")]);
    let r = construct_production(&w, &f, &i, &t, &c, &request("c"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    assert_eq!(r.steps.len(), 2);
    assert_eq!(r.final_node.as_ref().unwrap().member_instances.len(), 3);
    for step in &r.steps {
        network::validate(&step.node, &f, &t, &c).unwrap();
        assert_eq!(step.statistics.valid_candidates_scored, 0);
    }
    let repeated = construct_production(&w, &f, &i, &t, &c, &request("c"));
    assert_eq!(r.final_node, repeated.final_node);
}

#[test]
fn reuses_existing_producer_and_constructs_shared_network() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("a", "c")]);
    let r = construct_production(&w, &f, &i, &t, &c, &request("b"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    let n = r.final_node.unwrap();
    assert_eq!(n.member_instances.len(), 3);
    assert_eq!(n.logistics_components.len(), 1);
    assert_eq!(n.transport_networks.len(), 1);
    assert_eq!(n.internal_requirements.len(), 2);
}

#[test]
fn commits_cycle_atomically_and_routes_all_induced_edges() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c"), ("c", "a")]);
    let r = construct_production(&w, &f, &i, &t, &c, &request("c"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    assert_eq!(r.steps.len(), 1);
    assert_eq!(r.steps[0].added_instances.len(), 2);
    assert_eq!(r.steps[0].node.internal_requirements.len(), 3);
}

#[test]
fn operation_budget_preserves_seed_and_does_not_claim_completion() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c"), ("c", "a")]);
    let mut req = request("c");
    req.max_route_searches = 1;
    let r = construct_production(&w, &f, &i, &t, &c, &req);
    assert!(!r.success);
    assert!(!r.internal_complete);
    assert!(r.steps.is_empty());
    let n = r.final_node.unwrap();
    assert_eq!(n.member_instances, vec!["c"]);
    assert!(n.transport_networks.is_empty());
    assert_eq!(r.failed_statistics.astar_searches, 1);
}

#[test]
fn step_limit_is_a_partial_result_not_a_successful_blueprint() {
    let (f, i, t, c) = catalogs();
    let w = wiring(&[("a", "b"), ("b", "c")]);
    let mut req = request("c");
    req.max_steps = 1;
    let r = construct_production(&w, &f, &i, &t, &c, &req);
    assert_eq!(r.status, "step-limit");
    assert!(!r.success);
    assert_eq!(r.steps.len(), 1);
    assert_eq!(r.unresolved_internal_requirements.len(), 1);
}

#[test]
fn external_io_is_not_silently_counted_as_complete() {
    let (f, i, t, c) = catalogs();
    let mut w = wiring(&[("a", "b")]);
    w.nodes.push(FacilityInstanceWiringNode::External {
        id: "import".into(),
        item: "ore".into(),
    });
    w.edges.push(FacilityInstanceWiringEdge {
        id: "import-edge".into(),
        source: "import".into(),
        target: "a".into(),
        kind: "external-input".into(),
        item: "ore".into(),
        rate: Rate {
            numerator: 1,
            denominator: 5,
        },
        projection: FacilityInstanceWiringProjection::Original,
    });
    let r = construct_production(&w, &f, &i, &t, &c, &request("b"));
    assert!(r.internal_complete, "{:?}", r.diagnostics);
    assert!(!r.complete);
    assert!(!r.success);
    assert_eq!(r.status, "external-connections-pending");
    assert_eq!(r.unresolved_external_requirements, vec!["import-edge"]);
}

#[test]
fn rejects_duplicate_wiring_ids_and_zero_budgets() {
    let (f, i, t, c) = catalogs();
    let mut w = wiring(&[("a", "b")]);
    w.edges.push(w.edges[0].clone());
    let r = construct_production(&w, &f, &i, &t, &c, &request("b"));
    assert_eq!(r.status, "invalid-input");
    assert!(r.final_node.is_none());
    let mut req = request("b");
    req.max_route_searches = 0;
    assert!(
        construct_production(&wiring(&[("a", "b")]), &f, &i, &t, &c, &req)
            .final_node
            .is_none()
    );
}

#[test]
fn separated_moss_fan_in_preserves_four_total_output_port_domains() {
    let (_, _, t, c) = catalogs();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let f = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(root.join("data/game/normalized/facilities.json"))
            .unwrap(),
    )
    .unwrap();
    let i = ValidatedItemCatalog::try_from_catalog(
        crate::logistics::load_item_catalog(root.join("data/game/normalized/items.json")).unwrap(),
    )
    .unwrap();
    let mut w = wiring(&[("planter-a", "grinder"), ("planter-b", "grinder")]);
    for node in &mut w.nodes {
        if let FacilityInstanceWiringNode::Facility { id, facility, .. } = node {
            *facility = if id == "grinder" {
                "grinder-1-mode-normal"
            } else {
                "planter-1-mode-normal"
            }
            .into();
        }
    }
    for e in &mut w.edges {
        e.item = "item-plant-moss-2".into();
        e.rate = Rate {
            numerator: 1,
            denominator: 2,
        };
    }
    for (j, id) in ["planter-a", "planter-b"].into_iter().enumerate() {
        let endpoint = format!("external-{j}");
        w.nodes.push(FacilityInstanceWiringNode::Target {
            id: endpoint.clone(),
            item: "item-plant-moss-2".into(),
        });
        w.edges.push(FacilityInstanceWiringEdge {
            id: format!("export-{j}"),
            source: id.into(),
            target: endpoint,
            kind: "target-output".into(),
            item: "item-plant-moss-2".into(),
            rate: Rate {
                numerator: 1,
                denominator: 4,
            },
            projection: FacilityInstanceWiringProjection::Original,
        });
    }
    let r = construct_production(&w, &f, &i, &t, &c, &request("grinder"));
    assert!(
        r.internal_complete,
        "{:?} {:?}",
        r.diagnostics, r.rejected_reasons
    );
    let n = r.final_node.unwrap();
    assert_eq!(n.placements.len(), 3);
    assert_eq!(n.boundary_requirements.len(), 2);
    assert!(blocked_boundaries(&n).is_empty());
    let used = n
        .transport_networks
        .iter()
        .flat_map(|n| &n.terminals)
        .filter(|t| t.direction == FacilityPortDirection::Output)
        .count();
    let remaining = n
        .boundary_requirements
        .iter()
        .flat_map(|b| &b.attachments)
        .filter(|a| matches!(a, ConstructiveBoundaryAttachment::NewPort { .. }))
        .count();
    assert_eq!(used + remaining, 4);
    assert!(!r.complete);
}

#[test]
fn grows_pipe_frontier_before_lexically_earlier_belt_frontier() {
    let (_, _, t, c) = catalogs();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let f = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(
            root.join("data/examples/constructive-production.facilities.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let i = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
        schema_version: 1,
        items: vec![
            ItemDefinition {
                id: "ore".into(),
                transport: TransportKind::Belt,
            },
            ItemDefinition {
                id: "fluid".into(),
                transport: TransportKind::Pipe,
            },
        ],
    })
    .unwrap();
    let mut w = wiring(&[("belt", "target"), ("pipe", "target")]);
    for node in &mut w.nodes {
        if let FacilityInstanceWiringNode::Facility { facility, .. } = node {
            *facility = "mixed-machine".into();
        }
    }
    w.edges[1].item = "fluid".into();
    let r = construct_production(&w, &f, &i, &t, &c, &request("target"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    assert_eq!(r.steps[0].added_instances, vec!["pipe"]);
    assert_eq!(
        r.steps[0].node.transport_networks[0].transport,
        TransportKind::Pipe
    );
}

#[test]
fn splits_new_demand_across_partially_loaded_existing_ports() {
    let (_, i, t, c) = catalogs();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let f = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(root.join("data/game/normalized/facilities.json"))
            .unwrap(),
    )
    .unwrap();
    let mut w = wiring(&[("a", "b"), ("a", "c"), ("a", "d")]);
    for node in &mut w.nodes {
        if let FacilityInstanceWiringNode::Facility { facility, .. } = node {
            *facility = "grinder-1-mode-normal".into();
        }
    }
    w.edges[0].rate = Rate {
        numerator: 3,
        denominator: 10,
    };
    w.edges[1].rate = Rate {
        numerator: 3,
        denominator: 10,
    };
    w.edges[2].rate = Rate {
        numerator: 2,
        denominator: 5,
    };
    let r = construct_production(&w, &f, &i, &t, &c, &request("b"));
    assert!(r.complete, "{:?} {:?}", r.diagnostics, r.rejected_reasons);
    let n = r.final_node.unwrap();
    let lanes = n
        .attachment_events
        .iter()
        .filter(|e| e.requirement == "edge-2")
        .collect::<Vec<_>>();
    assert_eq!(lanes.len(), 2);
    assert!(lanes.iter().all(|e| e.rate
        == Rate {
            numerator: 1,
            denominator: 5
        }
        && e.source_kind == "existing-network"));
    network::validate(&n, &f, &t, &c).unwrap();
}
