use super::*;
use crate::logistics::{ItemCatalog, ItemDefinition};
use crate::recipes::{FacilityInstanceWiringProjection, Rate};

fn fixture() -> (
    FixedRoutingInput,
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
    let input = FixedRoutingInput {
        schema_version: 1,
        kind: "placement-only-visual-diagnostic".into(),
        routing_performed: false,
        factory_validated: false,
        required_facilities: 2,
        placements: ["a", "b"]
            .iter()
            .enumerate()
            .map(|(i, id)| FacilityPlacement {
                instance: id.to_string(),
                recipe: "fixture".into(),
                facility: "network-fixture-machine".into(),
                x: i as i64 * 2,
                y: 0,
                width: 1,
                height: 1,
                rotation: 0,
            })
            .collect(),
        internal_requirements: vec![FacilityInstanceWiringEdge {
            id: "edge".into(),
            source: "a".into(),
            target: "b".into(),
            kind: "recipe-flow".into(),
            item: "ore".into(),
            rate: Rate {
                numerator: 1,
                denominator: 5,
            },
            projection: FacilityInstanceWiringProjection::Original,
        }],
        external_requirements: vec![],
    };
    (input, f, i, t, super::super::test_components())
}

#[test]
fn adjacent_opposed_ports_use_one_straight_cell_without_moving_facilities() {
    let (input, f, i, t, c) = fixture();
    let r = probe_fixed_routing(&input, &f, &i, &t, &c, 4, 32).unwrap();
    assert!(r.unresolved_internal.is_empty());
    assert_eq!(r.final_node.transport_networks[0].cells.len(), 1);
    assert_eq!(r.attempts[0].stage, "straight-pairs");
    assert!(r.attempts[0].connected);
    assert!(!r.factory_validated);
    assert!(r.covered_routes_validated);
    for (a, b) in input.placements.iter().zip(&r.final_node.placements) {
        assert_eq!((a.x + 4, a.y + 4, a.rotation), (b.x, b.y, b.rotation));
    }
}

#[test]
fn offset_ports_fall_through_to_validated_astar_detour() {
    let (mut input, f, i, t, c) = fixture();
    input.placements[1].x = 4;
    input.placements[1].y = 2;
    let r = probe_fixed_routing(&input, &f, &i, &t, &c, 4, 32).unwrap();
    assert!(r.unresolved_internal.is_empty());
    assert!(!r.attempts[0].connected);
    assert_eq!(r.attempts[1].stage, "alternate-ports-astar");
    assert!(r.attempts[1].connected);
}

#[test]
fn initial_obstruction_remains_explicit_and_cannot_be_reported_as_factory_success() {
    let (mut input, f, i, t, c) = fixture();
    let mut blocker = input.placements[0].clone();
    blocker.instance = "blocker".into();
    blocker.x = 1;
    input.placements.push(blocker);
    input.required_facilities = 3;
    let r = probe_fixed_routing(&input, &f, &i, &t, &c, 4, 32).unwrap();
    assert_eq!(r.unresolved_internal, vec!["edge"]);
    assert_eq!(r.initial_empty_domains.len(), 2);
    assert!(r.final_node.transport_networks.is_empty());
    assert!(!r.factory_validated);
}

#[test]
fn invalid_geometry_rates_membership_and_resource_bounds_fail() {
    let (mut input, f, i, t, c) = fixture();
    assert!(probe_fixed_routing(&input, &f, &i, &t, &c, 0, 32).is_err());
    assert!(probe_fixed_routing(&input, &f, &i, &t, &c, 4, 0).is_err());
    input.internal_requirements[0].rate.denominator = 0;
    assert!(probe_fixed_routing(&input, &f, &i, &t, &c, 4, 32).is_err());
    input.internal_requirements[0].rate.denominator = 5;
    input.placements[1].x = 0;
    assert!(probe_fixed_routing(&input, &f, &i, &t, &c, 4, 32).is_err());
    input.placements[1].x = 2;
    input.placements[1].instance = "a".into();
    assert!(probe_fixed_routing(&input, &f, &i, &t, &c, 4, 32).is_err());
}

#[test]
fn rejects_a_shortest_path_that_consumes_the_last_pending_external_port() {
    let (mut input, f, i, t, c) = fixture();
    input.placements[1].x = 4;
    let mut future = input.placements[0].clone();
    future.instance = "future".into();
    future.x = 2;
    future.y = 1;
    future.rotation = 90;
    input.placements.push(future);
    input.required_facilities = 3;
    let mut external = input.internal_requirements[0].clone();
    external.id = "pending-external".into();
    external.source = "external".into();
    external.target = "future".into();
    input.external_requirements.push(external);
    let r = probe_fixed_routing(&input, &f, &i, &t, &c, 4, 32).unwrap();
    assert!(r.initial_empty_domains.is_empty());
    assert!(r.attempts.iter().any(|a| {
        a.future_domain_rejections > 0
            && a.rejected_future_requirements
                .contains(&"pending-external".to_string())
    }));
    assert_eq!(r.unresolved_external, vec!["pending-external"]);
    assert!(
        r.final_node
            .boundary_requirements
            .iter()
            .all(|b| !b.attachments.is_empty())
    );
    assert!(!r.factory_validated);
}
