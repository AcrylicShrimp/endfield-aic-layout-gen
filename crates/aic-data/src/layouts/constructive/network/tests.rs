use super::*;
use crate::layouts::constructive::{
    ConstructiveCompositionReport, compose_constructive_nodes, construct_facility_node,
    render_constructive_composition_html,
};
use crate::logistics::{ItemCatalog, ItemDefinition, ValidatedItemCatalog};
use crate::recipes::{
    FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringNode,
    FacilityInstanceWiringProjection, FacilityInstanceWiringReport,
};

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn catalogs() -> (
    ValidatedFacilityCatalog,
    ValidatedItemCatalog,
    ValidatedTransportCatalog,
    ValidatedLogisticsComponentCatalog,
) {
    let facilities = ValidatedFacilityCatalog::try_from_catalog(
        crate::facilities::load_facility_catalog(
            root().join("data/examples/constructive-network.facilities.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
        schema_version: 1,
        items: vec![ItemDefinition {
            id: "ore".into(),
            transport: TransportKind::Belt,
        }],
    })
    .unwrap();
    let transports = ValidatedTransportCatalog::try_from_catalog(
        crate::logistics::load_transport_catalog(
            root().join("data/game/normalized/transports.json"),
        )
        .unwrap(),
    )
    .unwrap();
    (
        facilities,
        items,
        transports,
        super::super::test_components(),
    )
}

fn fixture(
    merge: bool,
    rate: Rate,
) -> (ConstructiveCompositionReport, ConstructiveCompositionReport) {
    let (facilities, items, transports, components) = catalogs();
    let wiring = FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: ["a", "b", "c"]
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
        edges: if merge {
            [("a", "c"), ("b", "c")]
        } else {
            [("a", "b"), ("a", "c")]
        }
        .into_iter()
        .enumerate()
        .map(|(i, (source, target))| FacilityInstanceWiringEdge {
            id: format!("edge-{i}"),
            source: source.into(),
            target: target.into(),
            kind: "recipe-flow".into(),
            item: "ore".into(),
            rate,
            projection: FacilityInstanceWiringProjection::Original,
        })
        .collect(),
        diagnostics: Vec::new(),
    };
    let first_source = construct_facility_node(&wiring, &facilities, &items, "a").unwrap();
    let first_target =
        construct_facility_node(&wiring, &facilities, &items, if merge { "c" } else { "b" })
            .unwrap();
    let first = compose_constructive_nodes(
        &first_source,
        &first_target,
        &wiring.edges[0],
        &transports,
        &facilities,
        &components,
    );
    if !first.success
        && rate
            > (Rate {
                numerator: 1,
                denominator: 4,
            })
    {
        return (first.clone(), first);
    }
    assert!(
        first.success,
        "first composition: {:?} {:?}",
        first.diagnostics, first.statistics
    );
    let fresh =
        construct_facility_node(&wiring, &facilities, &items, if merge { "b" } else { "c" })
            .unwrap();
    let (source, target) = if merge {
        (&fresh, first.composite.as_ref().unwrap())
    } else {
        (first.composite.as_ref().unwrap(), &fresh)
    };
    let before = source.clone();
    let second = compose_constructive_nodes(
        source,
        target,
        &wiring.edges[1],
        &transports,
        &facilities,
        &components,
    );
    assert_eq!(*source, before, "composition must preserve the prior node");
    (first, second)
}

#[test]
fn fan_out_and_merge_share_one_port_with_catalog_components() {
    for merge in [false, true] {
        let (_, report) = fixture(
            merge,
            Rate {
                numerator: 1,
                denominator: 5,
            },
        );
        assert!(
            report.success,
            "{:?} {:?}",
            report.diagnostics, report.statistics
        );
        let node = report.composite.as_ref().unwrap();
        let (facilities, _, transports, components) = catalogs();
        validate(node, &facilities, &transports, &components).unwrap();
        for rotation in [0, 90, 180, 270] {
            let rotated = super::super::composition::rotate_node(node, rotation);
            let translated = super::super::composition::translate_node(&rotated, 7, 11);
            validate(&translated, &facilities, &transports, &components).unwrap();
        }
        assert_eq!(node.transport_networks.len(), 1);
        assert_eq!(node.transport_networks[0].terminals.len(), 3);
        assert_eq!(node.logistics_components.len(), 1);
        assert_eq!(
            node.logistics_components[0].kind,
            if merge {
                LogisticsComponentKind::Converger
            } else {
                LogisticsComponentKind::Splitter
            }
        );
        assert!(node.transport_networks[0].segments.iter().any(|s| s.rate
            == Rate {
                numerator: 2,
                denominator: 5
            }));
        assert!(node.network_capacity[0].segments.iter().any(|s| s.residual
            == Rate {
                numerator: 1,
                denominator: 10
            }));
        assert!(report.statistics.existing_network_attempts > 0);
    }
}

#[test]
fn validator_rejects_implicit_branch_overload_bad_rotation_and_wrong_port() {
    let (_, report) = fixture(
        false,
        Rate {
            numerator: 1,
            denominator: 5,
        },
    );
    let node = report.composite.unwrap();
    let (facilities, _, transports, components) = catalogs();
    let mut bad = node.clone();
    bad.logistics_components.clear();
    assert!(validate(&bad, &facilities, &transports, &components).is_err());
    let mut bad = node.clone();
    bad.transport_networks[0].segments[0].rate = Rate {
        numerator: 1,
        denominator: 1,
    };
    assert!(validate(&bad, &facilities, &transports, &components).is_err());
    let mut bad = node.clone();
    bad.logistics_components[0].rotation = (bad.logistics_components[0].rotation + 180) % 360;
    assert!(validate(&bad, &facilities, &transports, &components).is_err());
    let mut bad = node;
    bad.transport_networks[0].terminals[0].position.x += 1;
    assert!(validate(&bad, &facilities, &transports, &components).is_err());
}

#[test]
fn overloaded_shared_trunk_cannot_use_the_only_port() {
    let (_, report) = fixture(
        false,
        Rate {
            numerator: 3,
            denominator: 10,
        },
    );
    assert!(!report.success);
    assert!(
        report.statistics.attachment_capacity_rejections
            + report.statistics.boundary_capacity_dead_ends_rejected
            > 0
    );
}

#[test]
#[ignore = "writes reproducible controlled JSON and HTML benchmark artifacts"]
fn write_network_attachment_artifacts() {
    let directory = root().join("docs/benchmarks/constructive-network-attachments");
    std::fs::create_dir_all(&directory).unwrap();
    for (name, merge, rate) in [
        (
            "fan-out",
            false,
            Rate {
                numerator: 1,
                denominator: 5,
            },
        ),
        (
            "merge",
            true,
            Rate {
                numerator: 1,
                denominator: 5,
            },
        ),
        (
            "overload",
            false,
            Rate {
                numerator: 3,
                denominator: 10,
            },
        ),
    ] {
        let (first, report) = fixture(merge, rate);
        for (suffix, r) in [("point-to-point", &first), ("result", &report)] {
            std::fs::write(
                directory.join(format!("{name}-{suffix}.json")),
                serde_json::to_vec_pretty(r).unwrap(),
            )
            .unwrap();
            std::fs::write(
                directory.join(format!("{name}-{suffix}.html")),
                render_constructive_composition_html(r, None).unwrap(),
            )
            .unwrap();
        }
    }
}
