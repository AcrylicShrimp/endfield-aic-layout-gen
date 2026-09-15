use super::super::super::tests::{place, session, step};
use super::*;

pub(super) fn options() -> Options {
    Options {
        translation_steps: vec![-1, 1],
        band_counts: vec![1],
        masking: Some("structural-bands-v1".into()),
    }
}
fn assert_rules(s: &Session, input: &Input) {
    let bp = s.routing_blueprint(&s.state).unwrap();
    for edge in &bp.edges {
        let a = topology::position(&bp.nodes[edge.source], &s.state.geometry, &s.catalogs.data)
            .unwrap();
        let b = topology::position(&bp.nodes[edge.target], &s.state.geometry, &s.catalogs.data)
            .unwrap();
        let expected = topology::query_directions(
            edge.zero_length,
            a,
            b,
            topology::directions(edge, &bp, &s.state.geometry, &s.catalogs.data).unwrap(),
        );
        for (offset, direction) in [
            (e::CURRENT_FIRST, expected.0),
            (e::CURRENT_LAST, expected.1),
        ] {
            let mut mask = [0.; 5];
            mask[direction.unwrap_or(4)] = 1.;
            assert_eq!(
                &input.edge_static[edge.id][offset..offset + 5],
                &mask,
                "edge {}",
                edge.id
            );
        }
    }
}
#[test]
fn observations_match_routing_after_rotations_port_changes_and_restore() {
    let mut s = session();
    let mut l = Learning::new(&s, options()).unwrap();
    assert_rules(&s, &l.observe(&s).unwrap());
    let initial = s.state.geometry.placements[0].clone();
    for rotation in [90, 180, 270, 0] {
        let p = &s.state.geometry.placements[0];
        let action = place(&s, &initial.instance, p.x, p.y, rotation);
        let outcome = step(&mut s, action);
        assert_ne!(outcome["status"], "rejected", "{outcome}");
        assert_rules(&s, &l.observe(&s).unwrap());
    }
    let group = l.facilities[0]
        .groups
        .iter()
        .find(|g| g.ports.len() > 1)
        .unwrap();
    let node = group.node;
    let original = s.state.ports[node].clone().unwrap();
    let alternative = group
        .ports
        .iter()
        .find(|p| **p != original)
        .unwrap()
        .clone();
    // Mutate a retained draft to isolate the endpoint rule, then exercise the actual router view.
    s.state.ports[node] = Some(alternative);
    s.revision += 1;
    let changed = l.observe(&s).unwrap();
    assert_rules(&s, &changed);
    for edge in s
        .blueprint
        .edges
        .iter()
        .filter(|e| e.source == node || e.target == node)
    {
        let column = if edge.source == node {
            e::CURRENT_FIRST
        } else {
            e::CURRENT_LAST
        };
        assert_eq!(changed.edge_static[edge.id][column + 4], 1.);
    }
    s.state.ports[node] = Some(original);
    s.revision += 1;
    assert_rules(&s, &l.observe(&s).unwrap());
}
#[test]
fn original_headings_remain_distinguishable_while_current_ports_match() {
    let mut a = session();
    let mut la = Learning::new(&a, options()).unwrap();
    let (node, alternate) = la.facilities[0]
        .groups
        .iter()
        .find_map(|g| {
            g.ports
                .iter()
                .find(|p| Some(*p) != a.state.ports[g.node].as_ref())
                .map(|p| (g.node, p.clone()))
        })
        .unwrap();
    let edge_id = a
        .blueprint
        .edges
        .iter()
        .position(|e| e.source == node || e.target == node)
        .unwrap();
    let side = usize::from(a.blueprint.edges[edge_id].target == node);
    let original = a.state.ports[node].clone();
    a.state.ports[node] = Some(alternate);
    let mut b = session();
    b.state = a.state.clone();
    let record = if side == 0 {
        &mut b.blueprint.edges[edge_id].first
    } else {
        &mut b.blueprint.edges[edge_id].last
    };
    *record = Some((record.unwrap() + 1) % 4);
    let mut lb = Learning::new(&b, options()).unwrap();
    let x = la.observe(&a).unwrap();
    let y = lb.observe(&b).unwrap();
    assert_eq!(
        &x.edge_static[edge_id][e::CURRENT_FIRST..e::BASELINE_FIRST],
        &y.edge_static[edge_id][e::CURRENT_FIRST..e::BASELINE_FIRST]
    );
    assert_ne!(
        &x.edge_static[edge_id][e::BASELINE_FIRST..e::ZERO_LENGTH],
        &y.edge_static[edge_id][e::BASELINE_FIRST..e::ZERO_LENGTH]
    );
    a.state.ports[node] = original.clone();
    b.state.ports[node] = original;
    a.revision += 1;
    b.revision += 1;
    let x = la.observe(&a).unwrap();
    let y = lb.observe(&b).unwrap();
    assert_rules(&a, &x);
    assert_rules(&b, &y);
    assert_ne!(
        &x.edge_static[edge_id][e::CURRENT_FIRST..e::BASELINE_FIRST],
        &y.edge_static[edge_id][e::CURRENT_FIRST..e::BASELINE_FIRST]
    );
    // A recorded heading rotates in the original facility frame; None stays explicit.
    assert_eq!(
        topology::endpoint_rule(Some(1), Some((90, 180, 0)), 0, false),
        (Some(2), Some(0))
    );
    assert_eq!(
        topology::endpoint_rule(None, Some((90, 180, 2)), 0, false),
        (Some(2), None)
    );
}
#[test]
fn wall_schedule_does_not_change_inputs_and_unchanged_actions_refresh_budgets() {
    let mut s = session();
    let mut l = Learning::new(&s, options()).unwrap();
    let first = l.observe(&s).unwrap();
    // Wall time is advanced without sleeping or consuming physical work.
    s.elapsed_before += Duration::from_secs(1);
    let delayed = l.observe(&s).unwrap();
    assert_eq!(json!(first), json!(delayed));
    let p = &s.state.geometry.placements[0];
    let action = place(&s, &p.instance, p.x, p.y, p.rotation);
    assert_eq!(step(&mut s, action)["status"], "unchanged");
    s.work.route_expansions += 7;
    let budget = l.observe(&s).unwrap();
    assert_eq!(first.node_static, budget.node_static);
    assert_eq!(first.edge_static, budget.edge_static);
    assert_eq!(first.path_cells, budget.path_cells);
    assert_eq!(first.action_features, budget.action_features);
    assert!(budget.global_features[8] < first.global_features[8]);
    assert!(budget.global_features[9] < first.global_features[9]);
    assert_eq!(s.revision, 0);
    assert_eq!(first.global_features[8], 1.); // Earlier owned rows remain immutable.
    let saved = serde_json::to_vec(&*budget).unwrap();
    let restored: Input = serde_json::from_slice(&saved).unwrap();
    assert_eq!(json!(restored), json!(budget));
    s.state.ports[l.facilities[0].groups[0].node] = Some("missing-port".into());
    s.revision += 1;
    assert_eq!(
        l.observe(&s).err().unwrap().code,
        "declarative.learning-port"
    );
}
fn zero_length_session() -> Session {
    use aic_data::layouts::{TransportNetworkSegment, TransportNetworkTerminal};
    let base = session();
    let mut w = base.original.clone();
    w.placements
        .retain(|p| p.instance == "A0" || p.instance == "C0");
    w.utilizations.retain(|id, _| id == "A0" || id == "C0");
    w.logistics_components.clear();
    w.placements[0].x = 5;
    w.placements[0].y = 5;
    let ports = catalog::placement_ports(&w.placements[0], &base.catalogs.data).unwrap();
    let output = ports
        .iter()
        .filter(|p| p.direction == FacilityPortDirection::Output)
        .max_by_key(|p| p.connection.y)
        .unwrap();
    let incoming = catalog::placement_ports(&w.placements[1], &base.catalogs.data)
        .unwrap()
        .into_iter()
        .find(|p| p.id == "input-belt-0")
        .unwrap();
    w.placements[1].x += output.connection.x - incoming.connection.x;
    w.placements[1].y += output.connection.y - incoming.connection.y;
    let mut networks = vec![];
    for network in &w.transport_networks {
        let terminals: Vec<_> = network
            .terminals
            .iter()
            .filter(|t| {
                matches!(&t.endpoint,
            TransportNetworkEndpoint::Facility{instance,..} if instance=="A0" || instance=="C0")
            })
            .cloned()
            .collect();
        if network.item == "item-plant-moss-powder-2" {
            let mut n = network.clone();
            n.component_ids.clear();
            n.segments.clear();
            n.terminals = terminals;
            for t in &mut n.terminals {
                t.position = output.connection.into();
                if let TransportNetworkEndpoint::Facility { instance, port } = &mut t.endpoint
                    && instance == "A0"
                {
                    *port = output.id.clone();
                }
            }
            n.cells = vec![output.connection.into()];
            networks.push(n);
        } else {
            for mut t in terminals {
                let TransportNetworkEndpoint::Facility { instance, port } = &t.endpoint else {
                    unreachable!()
                };
                let placement = w
                    .placements
                    .iter()
                    .find(|p| &p.instance == instance)
                    .unwrap();
                let port = catalog::placement_ports(placement, &base.catalogs.data)
                    .unwrap()
                    .into_iter()
                    .find(|p| &p.id == port)
                    .unwrap();
                use aic_data::facilities::FacilityPortEdge;
                let external = match port.edge {
                    FacilityPortEdge::West => Point {
                        x: w.placements.iter().map(|p| p.x).min().unwrap() - 2,
                        y: port.connection.y,
                    },
                    FacilityPortEdge::East => Point {
                        x: w.placements.iter().map(|p| p.x + p.width).max().unwrap() + 1,
                        y: port.connection.y,
                    },
                    FacilityPortEdge::North => Point {
                        x: port.connection.x,
                        y: w.placements.iter().map(|p| p.y).min().unwrap() - 2,
                    },
                    FacilityPortEdge::South => Point {
                        x: port.connection.x,
                        y: w.placements.iter().map(|p| p.y + p.height).max().unwrap() + 1,
                    },
                };
                t.position = port.connection.into();
                let mut n = network.clone();
                n.component_ids.clear();
                n.id = format!("direct:{}", t.id);
                let external_id = format!("external:{}", t.id);
                let other = TransportNetworkTerminal {
                    id: external_id.clone(),
                    node: external_id.clone(),
                    endpoint: TransportNetworkEndpoint::External {
                        node: external_id,
                        side: port.edge,
                    },
                    position: external.into(),
                    direction: if t.direction == FacilityPortDirection::Input {
                        FacilityPortDirection::Output
                    } else {
                        FacilityPortDirection::Input
                    },
                    rate: t.rate,
                };
                let (from, to) = if t.direction == FacilityPortDirection::Input {
                    (external, port.connection)
                } else {
                    (port.connection, external)
                };
                let mut cells = vec![from];
                while *cells.last().unwrap() != to {
                    let p = *cells.last().unwrap();
                    cells.push(p.shifted((to.x - p.x).signum(), (to.y - p.y).signum()));
                }
                n.segments = cells
                    .windows(2)
                    .map(|p| TransportNetworkSegment {
                        from: p[0].into(),
                        to: p[1].into(),
                        rate: t.rate,
                    })
                    .collect();
                n.cells = cells.into_iter().map(Into::into).collect();
                n.terminals = vec![t, other];
                networks.push(n);
            }
        }
    }
    w.transport_networks = networks;
    let request = super::super::super::super::super::tests::fixture_request(&w);
    Session::new(request, w, base.catalogs, base.config).unwrap()
}
#[test]
fn directed_path_features_keep_order_and_zero_length_endpoint_bits() {
    let path = [
        Point { x: 0, y: 0 },
        Point { x: 1, y: 0 },
        Point { x: 1, y: 1 },
    ];
    let frame = frame::PhysicalFrame::from_points(path).unwrap();
    let forward = path_rows(&path, frame).collect::<Vec<_>>();
    let reverse = path_rows(&path.into_iter().rev().collect::<Vec<_>>(), frame).collect::<Vec<_>>();
    assert_eq!(&forward[1][..2], &reverse[1][..2]);
    assert_ne!(&forward[1][2..6], &reverse[1][2..6]);
    assert_eq!(&forward[0][2..4], &[0., 0.]);
    assert_eq!(&forward[2][4..6], &[0., 0.]);
    let single = path_rows(&path[..1], frame).collect::<Vec<_>>();
    assert_eq!(&single[0][6..8], &[1., 1.]);
    let s = zero_length_session();
    let mut l = Learning::new(&s, options()).unwrap();
    let input = l.observe(&s).unwrap();
    assert!(
        s.blueprint.edges.iter().any(|e| e.zero_length),
        "fixture must exercise an admitted zero-length route"
    );
    for edge in s.blueprint.edges.iter().filter(|e| e.zero_length) {
        let indices: Vec<_> = input
            .path_edges
            .iter()
            .enumerate()
            .filter(|(_, id)| **id == edge.id as i64)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(indices.len(), 1);
        assert_eq!(&input.path_cells[indices[0]][6..8], &[1., 1.]);
        assert_eq!(input.edge_static[edge.id][e::CURRENT_FIRST + 4], 1.);
        assert_eq!(input.edge_static[edge.id][e::BASELINE_FIRST + 4], 1.);
    }
}
#[test]
fn components_ports_network_membership_and_exact_capacity_remain_physical() {
    let s = session();
    let mut l = Learning::new(&s, options()).unwrap();
    let input = l.observe(&s).unwrap();
    assert_eq!(
        input
            .node_static
            .iter()
            .filter(|r| r[n::PORT] == 1.)
            .count(),
        l.tables.ports.iter().map(BTreeMap::len).sum::<usize>()
    );
    for (owner, groups) in l.facilities.iter().enumerate() {
        for g in &groups.groups {
            let original = s.state.ports[g.node].as_ref().unwrap();
            let id = l.tables.ports[owner][original];
            assert!(
                input
                    .edge_index
                    .iter()
                    .zip(&input.edge_static)
                    .any(
                        |(pair, row)| pair == &[l.tables.nodes[g.node] as i64, id as i64]
                            && row[e::ORIGINAL_PORT] == 1.
                    )
            );
        }
    }
    let mut bridge_channels = 0;
    for c in &s.blueprint.components {
        let id = l.tables.components[&c.id];
        let d = s.catalogs.data.components.component(&c.component).unwrap();
        assert_eq!(
            input.node_static[id][n::CAPACITY],
            rate(capacity(&d.capacity).unwrap())
        );
        if d.kind == LogisticsComponentKind::Bridge {
            let channels: Vec<_> = s
                .blueprint
                .nodes
                .iter()
                .filter(|n| n.components.contains(&c.id))
                .collect();
            assert!(channels.len() >= 2);
            bridge_channels += channels.len();
            for (i, a) in channels.iter().enumerate() {
                for b in &channels[i + 1..] {
                    assert_ne!(l.tables.nodes[a.id], l.tables.nodes[b.id]);
                    assert!(!input.edge_index.iter().zip(&input.edge_static).any(
                        |(pair, row)| row[e::TRANSPORT] == 1.
                            && (pair
                                == &[l.tables.nodes[a.id] as i64, l.tables.nodes[b.id] as i64]
                                || pair
                                    == &[l.tables.nodes[b.id] as i64, l.tables.nodes[a.id] as i64])
                    ));
                }
            }
        }
    }
    assert!(bridge_channels > 0);
    // Equality is represented by network -> commodity links, not commodity spelling.
    let mut same = session();
    same.blueprint.networks[1]["item"] = same.blueprint.networks[0]["item"].clone();
    let mut equal = Learning::new(&same, options()).unwrap();
    let same_input = equal.observe(&same).unwrap();
    assert_eq!(
        same_input
            .node_static
            .iter()
            .filter(|r| r[n::COMMODITY] == 1.)
            .count()
            + 1,
        input
            .node_static
            .iter()
            .filter(|r| r[n::COMMODITY] == 1.)
            .count()
    );
    assert_eq!(
        same_input
            .node_static
            .iter()
            .filter(|r| r[n::NETWORK] == 1.)
            .count(),
        input
            .node_static
            .iter()
            .filter(|r| r[n::NETWORK] == 1.)
            .count()
    );
}

#[test]
fn external_catalog_capacity_masks_and_unused_port_layout_are_observable() {
    use aic_data::facilities::ValidatedFacilityCatalog;
    use aic_data::logistics::{
        LogisticsComponentDefinition, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    };
    let s = session();
    let mut l = Learning::new(&s, options()).unwrap();
    let before = l.observe(&s).unwrap();
    let component: LogisticsComponentDefinition = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/observation-component.json"
    )))
    .unwrap();
    let clone_catalogs =
        |c: &aic_data::layouts::FactoryEditCatalogs| aic_data::layouts::FactoryEditCatalogs {
            facilities: c.facilities.clone(),
            recipes: c.recipes.clone(),
            items: c.items.clone(),
            transports: c.transports.clone(),
            components: c.components.clone(),
        };
    let mut catalogs = clone_catalogs(&s.catalogs.data);
    let mut components = catalogs.components.catalog().clone();
    components
        .components
        .retain(|c| c.transport != component.transport || c.kind != component.kind);
    components.components.push(component.clone());
    catalogs.components = ValidatedLogisticsComponentCatalog::try_from_catalog(components).unwrap();
    let mut transports = catalogs.transports.catalog().clone();
    for t in &mut transports.transports {
        t.capacity.quantity = 2;
        t.capacity.duration_ms = 1000;
    }
    catalogs.transports = ValidatedTransportCatalog::try_from_catalog(transports).unwrap();
    let mut w = s.original.clone();
    let c = w
        .logistics_components
        .iter_mut()
        .find(|c| c.kind == LogisticsComponentKind::Converger)
        .unwrap();
    let component_id = c.id.clone();
    c.component = component.id.clone();
    let changed = Session::new(
        s.request.clone(),
        w.clone(),
        Arc::new(crate::input::Catalogs::new(clone_catalogs(&catalogs))),
        s.config.clone(),
    )
    .unwrap();
    let mut other = Learning::new(&changed, options()).unwrap();
    let after = other.observe(&changed).unwrap();
    let id = other.tables.components[&component_id];
    assert_eq!(after.node_static[id][n::CAPACITY], 3f32.ln());
    let rotation = w
        .logistics_components
        .iter()
        .find(|c| c.id == component_id)
        .unwrap()
        .rotation;
    for (offset, directions) in [
        (n::INPUT_MASK, &component.input_directions),
        (n::OUTPUT_MASK, &component.output_directions),
    ] {
        let mut expected = [0.; 4];
        for &d in directions {
            expected[(cardinal(d) + (rotation / 90) as usize) % 4] = 1.;
        }
        assert_eq!(&after.node_static[id][offset..offset + 4], &expected);
    }
    let mut changed_edges = 0;
    for edge in &s.blueprint.edges {
        if before.edge_static[edge.id][e::CAPACITY] != after.edge_static[edge.id][e::CAPACITY] {
            changed_edges += 1;
        }
        assert!(after.edge_static[edge.id][e::CAPACITY] <= 3f32.ln());
        if [edge.source, edge.target].iter().any(|&n| {
            s.blueprint.nodes[n]
                .components
                .iter()
                .any(|id| id != &component_id)
        }) {
            assert_eq!(after.edge_static[edge.id][e::CAPACITY], 1.5f32.ln());
        }
    }
    assert!(changed_edges > 0);
    // Exchange two unused input positions without changing any selected physical port.
    let mut facilities = catalogs.facilities.catalog().clone();
    let f = facilities
        .facilities
        .iter_mut()
        .find(|f| f.id == w.placements[0].facility)
        .unwrap();
    let unused: Vec<_> = f
        .ports
        .iter()
        .enumerate()
        .filter(|(_, p)| p.direction == FacilityPortDirection::Input && p.id != "input-belt-0")
        .map(|(i, _)| i)
        .collect();
    let a = f.ports[unused[0]].position.clone();
    f.ports[unused[0]].position = f.ports[unused[1]].position.clone();
    f.ports[unused[1]].position = a;
    f.allowed_rotations.retain(|&r| r != 270);
    catalogs.facilities = ValidatedFacilityCatalog::try_from_catalog(facilities).unwrap();
    let changed = Session::new(
        s.request.clone(),
        w,
        Arc::new(crate::input::Catalogs::new(catalogs)),
        s.config.clone(),
    )
    .unwrap();
    let mut altered = Learning::new(&changed, options()).unwrap();
    let geometry = altered.observe(&changed).unwrap();
    assert_ne!(geometry.port_features, after.port_features);
    assert_ne!(geometry.node_state, after.node_state);
    assert_eq!(
        geometry.node_static[altered.tables.facilities[0]][n::ROTATIONS + 3],
        0.
    );
    let d = changed
        .catalogs
        .data
        .facilities
        .facility(&changed.blueprint.facilities[0].facility)
        .unwrap();
    let scale = altered.cached.as_ref().unwrap().1.frame.scale() as f32;
    assert_eq!(
        geometry.node_static[0][n::WIDTH],
        d.footprint.width as f32 / scale
    );
}

/// Explicit small/large schema evidence; never writes during normal execution.
#[test]
#[ignore = "requires AIC_PHYSICAL_INPUTS output path"]
fn export_physical_size_cases() {
    let mut cases = vec![];
    let mut sessions = vec![
        (
            "two-facility-zero-length".to_owned(),
            zero_length_session(),
            options(),
        ),
        ("crossing-shared-network".to_owned(), session(), options()),
    ];
    if let Ok(path) = std::env::var("AIC_PHYSICAL_FAILURE") {
        let saved: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let config: Value = serde_json::from_slice(
            &std::fs::read(std::env::var("AIC_PHYSICAL_CONFIG").unwrap()).unwrap(),
        )
        .unwrap();
        let catalogs = sessions[0].1.catalogs.clone();
        for row in saved["snapshots"].as_array().unwrap() {
            sessions.push((
                format!("retained-native-stream-{}", row["stream"]),
                Session::from_checkpoint(&row["native"].to_string(), catalogs.clone()).unwrap(),
                serde_json::from_value(config["actions"].clone()).unwrap(),
            ));
        }
    }
    for (name, s, options) in sessions {
        let mut l = Learning::new(&s, options).unwrap();
        let input = l.observe(&s).unwrap();
        let domain = runtime::Domain {
            groups: l
                .facilities
                .iter()
                .map(|f| {
                    f.groups
                        .iter()
                        .map(|g| runtime::PortGroup {
                            graph_node: l.tables.nodes[g.node],
                            ports: g.ports.clone(),
                            features: g.features.clone(),
                        })
                        .collect()
                })
                .collect(),
        };
        let counts = json!({"name":name,"facilities":s.blueprint.facilities.len(),"nodes":input.node_static.len(),"relations":input.edge_static.len(),"transport_edges":s.blueprint.edges.len(),
            "path_cells":input.path_cells.len(),"roots":input.action_features.len(),"port_rows":input.port_features.len(),"numeric_length_bytes":input.bytes(false),"numeric_capacity_bytes":input.bytes(true)});
        println!("{counts}");
        cases.push(json!({"name":name,"input":input,"domain":domain,"counts":counts}));
    }
    std::fs::write(
        std::env::var("AIC_PHYSICAL_INPUTS").unwrap(),
        serde_json::to_vec(&cases).unwrap(),
    )
    .unwrap();
}
