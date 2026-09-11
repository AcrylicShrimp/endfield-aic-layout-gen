use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use aic_data::facilities::ValidatedFacilityCatalog;
use aic_data::layouts::FactoryEditCatalogs;
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use aic_data::recipes::ValidatedRecipeBook;
use indexmap::{IndexMap, IndexSet};
use serde_json::{Value, json};

use super::types::*;
use super::{candidates, poses};
use super::{catalog, chains, path, routing};

mod granularity;
mod region;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
fn oracle(request: Value) -> Value {
    let mut child = Command::new("node")
        .arg(root().join("tools/factory-edit-pilot/native_geometry_oracle.mjs"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("standalone oracle requires Node");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&request).unwrap())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
pub(super) fn fixture() -> (Witness, FactoryEditCatalogs, Value) {
    let root = root();
    let data = root.join("data/game/normalized");
    let paths = json!({"facilities": data.join("facilities.json"), "recipes": data.join("recipes.json"), "items": data.join("items.json"),
        "transports": data.join("transports.json"), "components": data.join("logistics-components.json")});
    let read = |key: &str| std::fs::read(paths[key].as_str().unwrap()).unwrap();
    let catalogs = FactoryEditCatalogs {
        facilities: ValidatedFacilityCatalog::try_from_catalog(
            serde_json::from_slice(&read("facilities")).unwrap(),
        )
        .unwrap(),
        recipes: ValidatedRecipeBook::try_from_recipe_book(
            serde_json::from_slice(&read("recipes")).unwrap(),
        )
        .unwrap(),
        items: ValidatedItemCatalog::try_from_catalog(
            serde_json::from_slice(&read("items")).unwrap(),
        )
        .unwrap(),
        transports: ValidatedTransportCatalog::try_from_catalog(
            serde_json::from_slice(&read("transports")).unwrap(),
        )
        .unwrap(),
        components: ValidatedLogisticsComponentCatalog::try_from_catalog(
            serde_json::from_slice(&read("components")).unwrap(),
        )
        .unwrap(),
    };
    let w = serde_json::from_slice(
        &std::fs::read(root.join(
            "docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json",
        ))
        .unwrap(),
    )
    .unwrap();
    (w, catalogs, paths)
}

pub(super) fn fixture_request(w: &Witness) -> aic_data::layouts::FactoryEditRequest {
    use aic_data::layouts::{
        FactoryEditEndpoint, FactoryEditInstance, FactoryEditRequest, FactoryEditTerminal,
        TransportNetworkEndpoint,
    };
    // Fixed external fixture obligations are captured before any test mutation.
    let mut terminals = std::collections::BTreeMap::new();
    for n in &w.transport_networks {
        for t in &n.terminals {
            let endpoint = match &t.endpoint {
                TransportNetworkEndpoint::Facility { instance, .. } => {
                    FactoryEditEndpoint::Facility {
                        instance: instance.clone(),
                    }
                }
                TransportNetworkEndpoint::External { node, .. } => {
                    FactoryEditEndpoint::External { node: node.clone() }
                }
            };
            let rate = terminals
                .entry((endpoint, t.direction, n.item.clone()))
                .or_insert(aic_data::recipes::Rate::zero());
            *rate = rate.checked_add(t.rate).unwrap();
        }
    }
    FactoryEditRequest {
        schema_version: 1,
        instances: w
            .placements
            .iter()
            .map(|p| FactoryEditInstance {
                instance: p.instance.clone(),
                recipe: p.recipe.clone(),
                utilization: w.utilizations[&p.instance],
            })
            .collect(),
        terminals: terminals
            .into_iter()
            .map(|((endpoint, direction, item), rate)| FactoryEditTerminal {
                endpoint,
                direction,
                item,
                rate,
            })
            .collect(),
    }
}

#[test]
fn retained_episode_rejects_invalid_handles_without_charging() {
    use super::{engine, identity};
    let (w, catalogs, _) = fixture();
    let catalogs = std::sync::Arc::new(catalogs);
    let config = engine::Config {
        input_contract: "current-history-16".into(),
        horizon: 8,
        elapsed_ms: 30000,
        route_work: 5000,
        memory: true,
        vocabulary: poses::VocabularyOptions::default(),
        repair: routing::Limits::default(),
    };
    let mut e = engine::Episode::new(
        fixture_request(&w),
        w.clone(),
        catalogs.clone(),
        config.clone(),
    )
    .unwrap();
    let foreign = engine::Episode::new(fixture_request(&w), w.clone(), catalogs, config)
        .unwrap()
        .prepare(false, false)
        .unwrap();
    let first = e.prepare(true, true).unwrap();
    assert!(e.step(foreign.handle, 0).is_err());
    assert_eq!(e.selected, 0);
    assert!(e.step(first.handle, usize::MAX).is_err());
    assert_eq!(e.selected, 0);
    let restore = first
        .kinds
        .iter()
        .position(|k| *k == "restore_best")
        .unwrap();
    let row = e.step(first.handle, restore).unwrap();
    assert_eq!(row["status"], "accepted_equal");
    assert_eq!(e.session.snapshot().proposals, 1);
    assert!(e.step(first.handle, restore).is_err());
    assert_eq!(e.selected, 1);
    let second = e.prepare(true, true).unwrap();
    assert!(!second.allowed[restore]);
    let before = identity::digest(e.session.snapshot().current);
    assert!(e.step(second.handle, restore).is_err());
    assert_eq!(e.selected, 1);
    assert_eq!(identity::digest(e.session.snapshot().current), before);
    let index = second.allowed.iter().position(|b| *b).unwrap();
    e.step(second.handle, index).unwrap();
    assert_eq!(e.selected, 2);
}

#[test]
fn route_charges_expansions_and_retains_direction_constraints() {
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 4,
        max_y: 4,
    };
    let crossings = IndexMap::new();
    let blocked = IndexSet::new();
    let options = |budget| path::PathOptions {
        first: Some(1),
        last: Some(0),
        work_budget: budget,
        crossings: &crossings,
        crossing_limit: 0,
        allow_zero_length: false,
    };
    let start = Point { x: 0, y: 0 };
    let end = Point { x: 4, y: 4 };
    let stopped = path::shortest_path(start, end, &blocked, bounds, options(1)).unwrap();
    assert_eq!((stopped.status, stopped.expanded), ("work-budget", 1));
    let complete = path::shortest_path(start, end, &blocked, bounds, options(100)).unwrap();
    assert_eq!(
        (complete.status, complete.steps, complete.turns),
        ("found", Some(8), Some(1))
    );
    let ps = complete.cells.unwrap();
    assert_eq!(direction(ps[0], ps[1]), Some(1));
    assert_eq!(direction(ps[ps.len() - 2], end), Some(0));
}

#[test]
#[ignore = "explicit migration oracle requires Node; no production Node dependency"]
fn astar_quality_matches_reference_matrix() {
    let bounds = Bounds {
        min_x: -2,
        min_y: -1,
        max_x: 4,
        max_y: 5,
    };
    let mut cases = Vec::new();
    let mut actual = Vec::new();
    for pattern in 0..6 {
        let blocked: IndexSet<_> = (bounds.min_y..=bounds.max_y)
            .flat_map(|y| (bounds.min_x..=bounds.max_x).map(move |x| Point { x, y }))
            .filter(|p| (p.x * 7 + p.y * 13 + pattern).rem_euclid(11) < pattern)
            .collect();
        for first in [None, Some(0), Some(1), Some(2), Some(3)] {
            for last in [None, Some(0), Some(1), Some(2), Some(3)] {
                for budget in [5000] {
                    let start = Point { x: -1, y: 0 };
                    let end = Point { x: 3, y: 4 };
                    let crossings = IndexMap::new();
                    cases.push(json!({"start": start, "end": end, "blocked": blocked.iter().collect::<Vec<_>>(), "bounds": bounds,
                "options": {"first": first, "last": last, "workBudget": budget}}));
                    actual.push(
                        serde_json::to_value(
                            path::shortest_path(
                                start,
                                end,
                                &blocked,
                                bounds,
                                path::PathOptions {
                                    first,
                                    last,
                                    work_budget: budget,
                                    crossings: &crossings,
                                    crossing_limit: 0,
                                    allow_zero_length: false,
                                },
                            )
                            .unwrap(),
                        )
                        .unwrap(),
                    );
                }
            }
        }
    }
    let expected = oracle(json!({"operation": "paths", "cases": cases}));
    for (i, (a, b)) in actual.iter().zip(expected.as_array().unwrap()).enumerate() {
        // Preflight changes work accounting, not the uncapped routing objective.
        let mut a = a.clone();
        let mut b = b.clone();
        for key in ["expanded", "preflight_expanded", "preflight_status"] {
            a.as_object_mut().unwrap().remove(key);
            b.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(a, b, "path case {i}: {}", cases[i]);
    }
}

#[test]
#[ignore = "explicit projection oracle requires Node; no production Node dependency"]
fn geometry_projection_matches_reference_crossing_fixture() {
    let (w, catalogs, paths) = fixture();
    let decomposition = chains::extract(&w, &IndexSet::new()).unwrap();
    let expected = oracle(
        json!({"operation": "geometry", "catalogs": paths, "witness": w, "edits": [], "work": 5000}),
    );
    assert_eq!(
        serde_json::to_value(&decomposition.chains).unwrap(),
        expected["chains"]
    );
    let ports: serde_json::Map<String, Value> = catalog::project(&w, &catalogs)
        .unwrap()
        .iter()
        .map(|(id, ports)| (id.clone(), serde_json::to_value(ports).unwrap()))
        .collect();
    assert_eq!(Value::Object(ports), expected["ports"]);
}
