use super::*;

fn key(g: &Graph) -> PhysicalIdentity {
    identities(
        g,
        &Limits {
            max_individualization_nodes: 100_000,
        },
        "test",
    )
    .unwrap()
    .0
}
fn permute(g: &Graph, order: &[usize]) -> Graph {
    let mut inverse = vec![0; order.len()];
    for (i, &v) in order.iter().enumerate() {
        inverse[v] = i;
    }
    Graph {
        vertices: order.iter().map(|&i| g.vertices[i].clone()).collect(),
        relations: g
            .relations
            .iter()
            .rev()
            .map(|&(a, r, b)| (inverse[a], r, inverse[b]))
            .collect(),
    }
}
fn permutations(n: usize) -> Vec<Vec<usize>> {
    let mut result = vec![vec![]];
    for v in 0..n {
        result = result
            .into_iter()
            .flat_map(|p| {
                (0..=p.len()).map(move |at| {
                    let mut p = p.clone();
                    p.insert(at, v);
                    p
                })
            })
            .collect();
    }
    result
}
fn rated(symmetric: bool) -> Graph {
    let mut g = Graph::default();
    for (i, kind) in [Kind::Terminal, Kind::Terminal, Kind::Port, Kind::Port]
        .into_iter()
        .enumerate()
    {
        g.vertex(
            kind,
            vec![if symmetric { 0 } else { i as i64 % 2 }],
            vec![i as i64 + 1, 7],
        );
    }
    g.relation(0, Relation::TerminalEndpoint, 2);
    g.relation(1, Relation::TerminalEndpoint, 3);
    g
}
/// Independent exhaustive isomorphism oracle checks all permutations of these
/// small colored graphs, with no refinement or canonical-search helper.
fn isomorphic(a: &Graph, b: &Graph, exact: bool) -> bool {
    permutations(a.vertices.len()).iter().any(|order| {
        let b = permute(b, order);
        if !a.vertices.iter().zip(&b.vertices).all(|(a, b)| {
            a.kind == b.kind && a.structural == b.structural && (!exact || a.exact == b.exact)
        }) {
            return false;
        }
        let mut x = a.relations.clone();
        let mut y = b.relations;
        x.sort();
        y.sort();
        x == y
    })
}

#[test]
fn independent_rate_projections_match_exhaustive_permutation_oracle() {
    for symmetric in [true, false] {
        let g = rated(symmetric);
        let expected = key(&g);
        for order in permutations(4) {
            assert_eq!(key(&permute(&g, &order)), expected);
        }
        let mut variants = vec![g.clone()];
        for (a, b) in [(0, 1), (2, 3)] {
            let mut changed = g.clone();
            let tmp = changed.vertices[a].exact.clone();
            changed.vertices[a].exact = changed.vertices[b].exact.clone();
            changed.vertices[b].exact = tmp;
            variants.push(changed);
        }
        let mut scaled = g.clone();
        scaled.vertices[0].exact[0] += 10;
        variants.push(scaled);
        for a in &variants {
            for b in &variants {
                let (ka, kb) = (key(a), key(b));
                assert_eq!(ka.structural == kb.structural, isomorphic(a, b, false));
                assert_eq!(ka.exact_problem == kb.exact_problem, isomorphic(a, b, true));
            }
        }
    }
}

#[test]
fn relations_direction_and_multiplicity_are_not_lost() {
    let g = rated(false);
    let original = key(&g);
    for edit in 0..3 {
        let mut changed = g.clone();
        match edit {
            0 => changed.relations[0] = (2, Relation::TerminalEndpoint, 0),
            1 => changed.relations[0].2 = 3,
            _ => changed.relations.push(changed.relations[0]),
        }
        assert_ne!(key(&changed).structural, original.structural);
    }
}

#[test]
fn combined_budget_rejects_after_structural_projection_has_completed() {
    let g = rated(true);
    let (_, used) = identities(
        &g,
        &Limits {
            max_individualization_nodes: 100_000,
        },
        "fixture/case-7",
    )
    .unwrap();
    assert!(used > 2);
    // With this graph structural takes 3 visits and exact is immediately discrete.
    assert_eq!(used, 4);
    for limit in [0, 1, used - 1] {
        let err = identities(
            &g,
            &Limits {
                max_individualization_nodes: limit,
            },
            "fixture/case-7",
        )
        .unwrap_err();
        assert_eq!(err.code, "aic-learning.physical-identity-budget");
        assert_eq!(err.work_used, limit);
        assert_eq!(err.entity, "fixture/case-7");
    }
}

#[test]
fn cancellation_never_certifies_an_identity() {
    let mut checks = 0;
    let failure = identities_until(
        &rated(true),
        &Limits {
            max_individualization_nodes: 100_000,
        },
        "fixture/cancelled",
        || {
            checks += 1;
            checks > 1
        },
    )
    .unwrap_err();
    assert_eq!(failure.code, "aic-learning.physical-identity-interrupted");
    assert_eq!(failure.work_used, 1);
}

#[test]
fn commodity_equality_partition_is_a_relation_not_a_name() {
    let mut graph = Graph::default();
    for _ in 0..4 {
        graph.vertex(Kind::Network, vec![], vec![]);
    }
    for _ in 0..2 {
        graph.vertex(Kind::Commodity, vec![], vec![]);
    }
    for n in 0..4 {
        graph.relation(n, Relation::Commodity, 4 + n / 2);
    }
    let original = key(&graph);
    graph.relations[1].2 = 5;
    assert_ne!(key(&graph).structural, original.structural);
}
