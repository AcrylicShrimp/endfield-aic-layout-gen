//! Compare under explicit entity correspondence, including input/catalog row permutations.
use super::super::super::tests::session;
use super::tests::options;
use super::*;
use aic_data::{
    facilities::ValidatedFacilityCatalog,
    layouts::FactoryEditCatalogs,
    logistics::{
        ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    },
    recipes::ValidatedRecipeBook,
};
fn strings(value: &Value, names: &mut BTreeSet<String>) {
    match value {
        Value::String(s) => {
            names.insert(s.clone());
        }
        Value::Array(a) => {
            for v in a {
                strings(v, names)
            }
        }
        Value::Object(o) => {
            for v in o.values() {
                strings(v, names)
            }
        }
        _ => {}
    }
}
fn rename(value: &Value, map: &BTreeMap<String, String>) -> Value {
    match value {
        Value::String(s) => json!(map.get(s).unwrap_or(s)),
        Value::Array(a) => Value::Array(a.iter().map(|v| rename(v, map)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, v)| (map.get(k).unwrap_or(k).clone(), rename(v, map)))
                .collect(),
        ),
        v => v.clone(),
    }
}
fn canonical(
    s: &Session,
    l: &Learning,
    input: &Input,
    inverse: &BTreeMap<String, String>,
) -> Value {
    let name = |s: &str| inverse.get(s).cloned().unwrap_or_else(|| s.to_owned());
    let mut keys = vec![String::new(); input.node_static.len()];
    for (owner, f) in s.blueprint.facilities.iter().enumerate() {
        keys[l.tables.facilities[owner]] = format!("facility:{}", name(&f.instance));
        for (port, &id) in &l.tables.ports[owner] {
            keys[id] = format!("port:{}:{}", name(&f.instance), name(port));
        }
    }
    for n in &s.blueprint.nodes {
        let terminal = n.terminal.as_ref().map(|t| name(&t.id));
        let mut components = n.components.iter().map(|c| name(c)).collect::<Vec<_>>();
        components.sort();
        keys[l.tables.nodes[n.id]] = format!(
            "node:{}:{terminal:?}:{components:?}",
            name(s.blueprint.networks[n.network]["id"].as_str().unwrap())
        );
    }
    for (c, &id) in &l.tables.components {
        keys[id] = format!("component:{}", name(c));
    }
    for (i, n) in s.blueprint.networks.iter().enumerate() {
        let id = l.tables.networks[i];
        keys[id] = format!("network:{}", name(n["id"].as_str().unwrap()));
        let commodity = input
            .edge_index
            .iter()
            .zip(&input.edge_static)
            .find(|(p, r)| p[0] == id as i64 && r[e::COMMODITY_MEMBER] == 1.)
            .unwrap()
            .0[1] as usize;
        keys[commodity] = format!("commodity:{}", name(n["item"].as_str().unwrap()));
    }
    assert!(keys.iter().all(|k| !k.is_empty()));
    assert_eq!(keys.iter().collect::<BTreeSet<_>>().len(), keys.len());
    let sorted = |rows: Vec<Value>| {
        let mut rows = rows.into_iter().map(|r| r.to_string()).collect::<Vec<_>>();
        rows.sort();
        rows
    };
    let nodes = sorted(
        keys.iter()
            .enumerate()
            .map(|(i, k)| json!([k, input.node_static[i], input.node_state[i]]))
            .collect(),
    );
    let edges = sorted(
        input
            .edge_index
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let path = input
                    .path_edges
                    .iter()
                    .enumerate()
                    .filter(|(_, id)| **id == i as i64)
                    .map(|(j, _)| input.path_cells[j])
                    .collect::<Vec<_>>();
                json!([
                    keys[p[0] as usize],
                    keys[p[1] as usize],
                    input.edge_static[i].as_slice(),
                    input.edge_state[i],
                    path
                ])
            })
            .collect(),
    );
    let actions = sorted(
        input
            .action_features
            .iter()
            .enumerate()
            .map(|(i, f)| {
                json!([
                    f,
                    input.action_references[i].map(|id| if id < 0 {
                        String::new()
                    } else {
                        keys[id as usize].clone()
                    }),
                    input.root_meta[i][0],
                    input.root_meta[i][2],
                    input.root_invalid[i],
                    input.kind_log_counts[i]
                ])
            })
            .collect(),
    );
    json!({"nodes":nodes,"edges":edges,"actions":actions,"ports":sorted(input.port_features.iter().map(|p|json!(p)).collect()),"globals":input.global_features})
}
fn renamed(original: &Session) -> (Session, BTreeMap<String, String>) {
    let c = &original.catalogs.data;
    let raw = json!({"witness":original.original,"facilities":c.facilities.catalog(),"recipes":{"schema_version":c.recipes.recipe_book().schema_version,"external_items":c.recipes.recipe_book().external_items,"recipes":c.recipes.recipe_book().recipes},
        "items":c.items.catalog(),"transports":c.transports.catalog(),"components":c.components.catalog()});
    let mut names = BTreeSet::new();
    strings(&raw, &mut names);
    let enums = BTreeSet::from([
        "belt",
        "pipe",
        "input",
        "output",
        "north",
        "east",
        "south",
        "west",
        "splitter",
        "converger",
        "bridge",
        "facility",
        "external",
    ]);
    let names = names
        .into_iter()
        .filter(|s| !enums.contains(s.as_str()))
        .collect::<Vec<_>>();
    let map = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), format!("renamed-{}", names.len() - i)))
        .collect::<BTreeMap<_, _>>();
    let inverse = map
        .iter()
        .map(|(a, b)| (b.clone(), a.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut raw = rename(&raw, &map);
    for (group, rows) in [
        ("facilities", "facilities"),
        ("recipes", "recipes"),
        ("components", "components"),
        ("items", "items"),
    ] {
        raw[group][rows].as_array_mut().unwrap().reverse();
    }
    for f in raw["facilities"]["facilities"].as_array_mut().unwrap() {
        f["ports"].as_array_mut().unwrap().reverse();
    }
    let catalogs = FactoryEditCatalogs {
        facilities: ValidatedFacilityCatalog::try_from_catalog(
            serde_json::from_value(raw["facilities"].clone()).unwrap(),
        )
        .unwrap(),
        recipes: ValidatedRecipeBook::try_from_recipe_book(
            serde_json::from_value(raw["recipes"].clone()).unwrap(),
        )
        .unwrap(),
        items: ValidatedItemCatalog::try_from_catalog(
            serde_json::from_value(raw["items"].clone()).unwrap(),
        )
        .unwrap(),
        transports: ValidatedTransportCatalog::try_from_catalog(
            serde_json::from_value(raw["transports"].clone()).unwrap(),
        )
        .unwrap(),
        components: ValidatedLogisticsComponentCatalog::try_from_catalog(
            serde_json::from_value(raw["components"].clone()).unwrap(),
        )
        .unwrap(),
    };
    let mut w: Witness = serde_json::from_value(raw["witness"].clone()).unwrap();
    w.placements.reverse();
    w.logistics_components.reverse();
    w.transport_networks.reverse();
    for n in &mut w.transport_networks {
        n.terminals.reverse();
        n.cells.reverse();
        n.segments.reverse();
        n.component_ids.reverse();
    }
    let request = super::super::super::super::super::tests::fixture_request(&w);
    let changed = Session::new(
        request,
        w,
        Arc::new(crate::input::Catalogs::new(catalogs)),
        original.config.clone(),
    )
    .unwrap();
    (changed, inverse)
}
#[test]
fn opaque_renaming_and_input_permutations_preserve_physical_arrays() {
    let original = session();
    let (changed, inverse) = renamed(&original);
    let mut a = Learning::new(&original, options()).unwrap();
    let mut b = Learning::new(&changed, options()).unwrap();
    let x = a.observe(&original).unwrap();
    let y = b.observe(&changed).unwrap();
    assert_eq!(
        canonical(&original, &a, &x, &BTreeMap::new()),
        canonical(&changed, &b, &y, &inverse)
    );
}

fn interchangeable_unused_ports() -> Session {
    let s = super::tests::zero_length_session();
    let mut raw = s.catalogs.data.facilities.catalog().clone();
    let owner = &s.blueprint.facilities[0];
    let definition = raw
        .facilities
        .iter_mut()
        .find(|f| f.id == owner.facility)
        .unwrap();
    let used: BTreeSet<_> = s
        .blueprint
        .nodes
        .iter()
        .filter_map(|n| n.terminal.as_ref())
        .filter_map(|t| match &t.endpoint {
            TransportNetworkEndpoint::Facility { instance, port }
                if instance == &owner.instance =>
            {
                Some(port.as_str())
            }
            _ => None,
        })
        .collect();
    let mut duplicate = definition
        .ports
        .iter()
        .find(|p| !used.contains(p.id.as_str()))
        .unwrap()
        .clone();
    duplicate.id = "physically-identical-unused".into();
    definition.ports.push(duplicate);
    let d = &s.catalogs.data;
    let catalogs = FactoryEditCatalogs {
        facilities: ValidatedFacilityCatalog::try_from_catalog(raw).unwrap(),
        recipes: d.recipes.clone(),
        items: d.items.clone(),
        transports: d.transports.clone(),
        components: d.components.clone(),
    };
    Session::new(
        s.request,
        s.original,
        Arc::new(crate::input::Catalogs::new(catalogs)),
        s.config,
    )
    .unwrap()
}

#[test]
fn physical_identity_survives_all_names_and_array_permutations() {
    use crate::generation::physical_identity::{Limits, identify};
    let limits = Limits {
        max_individualization_nodes: 100_000,
    };
    for original in [session(), interchangeable_unused_ports()] {
        let (changed, _) = renamed(&original);
        let a = identify(
            &original.original,
            &original.catalogs.data,
            &limits,
            "original",
        )
        .unwrap();
        let b = identify(
            &changed.original,
            &changed.catalogs.data,
            &limits,
            "renamed",
        )
        .unwrap();
        assert_eq!(a, b);
    }
}
fn enumerable(s: &Session, inverse: &BTreeMap<String, String>) -> Value {
    let mut l = Learning::new(
        s,
        Options {
            translation_steps: vec![],
            band_counts: vec![],
            masking: None,
        },
    )
    .unwrap();
    let input = l.observe(s).unwrap();
    let name = |s: &str| inverse.get(s).cloned().unwrap_or_else(|| s.to_owned());
    let domain = runtime::Domain {
        groups: l
            .facilities
            .iter()
            .map(|f| {
                f.groups
                    .iter()
                    .map(|g| runtime::PortGroup {
                        graph_node: l.tables.nodes[g.node],
                        equivalence: g.equivalence,
                        port_equivalences: g.port_equivalences.clone(),
                        ports: g.ports.clone(),
                        features: g.features.clone(),
                    })
                    .collect()
            })
            .collect(),
    };
    let mut selections = vec![];
    for (root, r) in l.roots.iter().enumerate() {
        let groups = &l.facilities[r.owner].groups;
        let mut prefixes = vec![vec![]];
        for (depth, g) in groups.iter().enumerate() {
            prefixes = prefixes
                .into_iter()
                .flat_map(|prefix| {
                    g.ports.iter().enumerate().filter_map(move |(p, name)| {
                        if prefix
                            .iter()
                            .enumerate()
                            .any(|(i, &index)| &groups[i].ports[index] == name)
                        {
                            return None;
                        }
                        let mut next = prefix.clone();
                        next.push(p);
                        Some(next)
                    })
                })
                .collect();
            assert!(
                depth < 4 && prefixes.len() < 1000,
                "explicit tiny enumeration only"
            );
        }
        for choices in prefixes {
            let decision = l.selected(s, l.handle, root, &choices).unwrap();
            let Edit::Place {
                instance,
                x,
                y,
                rotation,
                ports,
            } = decision.action
            else {
                panic!("expected placement")
            };
            let mut mapping = vec![];
            for (g, (&choice, selected)) in groups.iter().zip(choices.iter().zip(ports)) {
                assert_eq!(selected.node, g.node);
                assert_eq!(selected.port, g.ports[choice]);
                let features = input.port_features[g.features[&rotation].0 + choice];
                mapping.push(json!([
                    g.equivalence,
                    g.port_equivalences[choice],
                    features
                ]));
            }
            mapping.sort_by_key(Value::to_string);
            selections.push(json!({"root":root,"ports":choices,"physical":json!([name(&instance),x,y,rotation,mapping]).to_string()}));
        }
    }
    json!({"input":input,"domain":domain,"selections":selections})
}
#[test]
fn canonical_order_resolves_exact_native_slots_and_explicit_equivalence() {
    let original = interchangeable_unused_ports();
    let (changed, inverse) = renamed(&original);
    let a = enumerable(&original, &BTreeMap::new());
    let b = enumerable(&changed, &inverse);
    let physical = |v: &Value| {
        let mut keys = v["selections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["physical"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>();
        keys.sort();
        keys
    };
    let keys = physical(&a);
    assert_eq!(keys, physical(&b));
    assert!(
        keys.windows(2).any(|w| w[0] == w[1]),
        "must contain interchangeable labels"
    );
    let mut l = Learning::new(&original, options()).unwrap();
    l.observe(&original).unwrap();
    let mut restored = Session::from_checkpoint(
        &original.checkpoint().to_string(),
        original.catalogs.clone(),
    )
    .unwrap();
    // Admission order depends on original physical data, not the restored revision.
    restored.attempts += 1;
    let r = Learning::new(&restored, options()).unwrap();
    for (a, b) in l.facilities.iter().zip(&r.facilities) {
        assert_eq!(
            a.groups
                .iter()
                .map(|g| (&g.ports, g.node, &g.features))
                .collect::<Vec<_>>(),
            b.groups
                .iter()
                .map(|g| (&g.ports, g.node, &g.features))
                .collect::<Vec<_>>()
        );
    }
}
#[test]
#[ignore = "requires AIC_ORDER_CASES path for explicit model distribution verification"]
fn export_enumerable_physical_action_domains() {
    let original = interchangeable_unused_ports();
    let (changed, inverse) = renamed(&original);
    let cases = [
        enumerable(&original, &BTreeMap::new()),
        enumerable(&changed, &inverse),
    ];
    std::fs::write(
        std::env::var("AIC_ORDER_CASES").unwrap(),
        serde_json::to_vec(&cases).unwrap(),
    )
    .unwrap();
}
