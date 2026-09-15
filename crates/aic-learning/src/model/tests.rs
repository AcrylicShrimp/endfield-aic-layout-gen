//! Missing references must preserve both node and context gradients exactly.
use super::*;
use aic_torch::tch::Kind;

fn masked_reference_parity(device: Device) {
    aic_torch::deterministic().unwrap();
    for node_count in [1, 7, 137] {
        for missing in [0, 1, 3, 8] {
            let candidates = 1024;
            let refs: Vec<i64> = (0..candidates * 2)
                .map(|i| {
                    if i % 8 < missing {
                        -1
                    } else {
                        (i / 5) % node_count
                    }
                })
                .collect();
            let refs = Tensor::from_slice(&refs)
                .view([candidates, 2])
                .to_device(device);
            let nodes = (Tensor::arange(node_count * 40, (Kind::Float, device)) * 0.17)
                .sin()
                .view([node_count, 40]);
            let context = (Tensor::arange(candidates * 40, (Kind::Float, device)) * 0.13)
                .cos()
                .view([candidates, 40]);
            let upstream = (Tensor::arange(candidates * 80, (Kind::Float, device)) * 0.11)
                .sin()
                .view([candidates, 2, 40]);
            let run = |spread: bool| {
                let nodes = nodes.copy().set_requires_grad(true);
                let context = context.copy().set_requires_grad(true);
                let selected = if spread {
                    masked_references(&nodes, &refs, &context)
                } else {
                    nodes
                        .index(&[Some(&refs.clamp_min(0))])
                        .where_self(&refs.ge(0).unsqueeze(-1), &context.unsqueeze(1))
                };
                (&selected * &upstream).sum(Kind::Float).backward();
                (selected.detach(), nodes.grad(), context.grad())
            };
            let expected = run(false);
            for _ in 0..2 {
                let actual = run(true);
                assert!(
                    expected.0.equal(&actual.0),
                    "selected: nodes={node_count} missing={missing}"
                );
                assert!(
                    expected.1.equal(&actual.1),
                    "node gradient: nodes={node_count} missing={missing}"
                );
                assert!(
                    expected.2.equal(&actual.2),
                    "context gradient: nodes={node_count} missing={missing}"
                );
            }
        }
    }
}

#[test]
fn masked_references_preserve_cpu_forward_and_backward() {
    masked_reference_parity(Device::Cpu);
}

#[test]
fn masked_references_preserve_cuda_forward_and_backward() {
    if aic_torch::tch::Cuda::is_available() {
        masked_reference_parity(Device::Cuda(0));
    }
}

fn physical_rows() -> Vec<crate::training::frozen::Frozen> {
    use crate::{collection::policy::Choice, training::frozen::Frozen};
    use aic_pilot_native::learning::{Domain, Input, Observation, PortGroup, schema::*};
    use std::sync::Arc;
    (0..3)
        .map(|graph| {
            let count = 3 + graph;
            let mut input = Input::default();
            for node in 0..count {
                input.node_static.push(std::array::from_fn(|i| {
                    ((i + node + graph) as f32 * 0.13).sin()
                }));
                input.node_state.push(std::array::from_fn(|i| {
                    ((i + node + graph) as f32 * 0.17).cos()
                }));
            }
            for edge in 0..count - 1 {
                input.edge_index.push([edge as i64, edge as i64 + 1]);
                input.edge_static.push(std::array::from_fn(|i| {
                    ((i + edge + graph) as f32 * 0.23).sin()
                }));
                input.edge_state.push([0., 0.5, 0.1, 0.2]);
                input.path_cells.extend([
                    [0.1, 0.2, 0., 0., 1., 0., 1., 0.],
                    [0.2, 0.2, 1., 0., 0., 0., 0., 1.],
                ]);
                input.path_edges.extend([edge as i64; 2]);
            }
            input.global_features = (0..GLOBAL_FEATURES)
                .map(|i| i as f32 * 0.01 + graph as f32 * 0.03)
                .collect();
            for root in 0..4 + graph {
                input
                    .action_features
                    .push(std::array::from_fn(|i| (i + root + graph) as f32 * 0.02));
                input
                    .action_references
                    .push(if root % 2 == 0 { [0, 1] } else { [-1, 2] });
                input.root_meta.push([0, 0, 0]);
                input.root_invalid.push(root == 2);
                input.kind_log_counts.push(1.1);
            }
            input.port_features = (0..3)
                .map(|p| std::array::from_fn(|i| (i + p + graph) as f32 * 0.07))
                .collect();
            let observation = Observation {
                input: Arc::new(input),
                domain: Arc::new(Domain {
                    groups: vec![
                        (0..2)
                            .map(|i| PortGroup {
                                graph_node: i + 1,
                                equivalence: i,
                                port_equivalences: vec![0, 1, 2],
                                ports: vec!["a".into(), "b".into(), "c".into()],
                                features: [(0, (0, 3))].into_iter().collect(),
                            })
                            .collect(),
                    ],
                }),
                handle: 1,
                blueprint_sha256: String::new(),
                revision: 0,
                attempts: 0,
                done: false,
            };
            Frozen::record(
                &observation,
                &Choice {
                    root: 0,
                    ports: vec![0, 1],
                    old_logp: 0.,
                    old_value: 0.,
                    entropy: 0.,
                },
            )
            .unwrap()
        })
        .collect()
}
fn packed_physical_parity(device: Device, tolerance: f64) {
    let _lock = crate::TORCH_TEST.lock().unwrap();
    use crate::{
        schema::ModelConfig,
        training::frozen::{self, Frozen},
    };
    use aic_pilot_native::learning::schema::*;
    aic_torch::tch::set_num_threads(1);
    aic_torch::deterministic().unwrap();
    let config = ModelConfig {
        width: 40,
        message_rounds: 2,
        node_features: (NODE_STATIC + NODE_STATE) as i64,
        edge_features: (EDGE_STATIC + EDGE_STATE) as i64,
        global_features: GLOBAL_FEATURES as i64,
        action_features: ACTION_FEATURES as i64,
        port_features: PORT_FEATURES as i64,
    };
    let (order, state) = Model::initial_state(&config, 397).unwrap();
    let rows = physical_rows();
    let compact: Vec<_> = rows.iter().map(Frozen::compact).collect();
    let restored: Vec<Frozen> = compact
        .iter()
        .map(|f| postcard::from_bytes(&postcard::to_stdvec(f).unwrap()).unwrap())
        .collect();
    let packed = frozen::pack(&rows.iter().collect::<Vec<_>>()).unwrap();
    let restored_packed = frozen::pack(&restored.iter().collect::<Vec<_>>()).unwrap();
    for (name, t) in &packed.graph {
        assert!(t.equal(&restored_packed.graph[name]), "frozen graph {name}");
    }
    for (name, t) in packed.branches.as_ref().unwrap() {
        assert!(
            t.equal(&restored_packed.branches.as_ref().unwrap()[name]),
            "frozen branch {name}"
        );
    }
    assert!(packed.roots.equal(&restored_packed.roots));
    assert!(packed.selected.equal(&restored_packed.selected));
    // Rows share the source Arc when recorded, then compact owns only retained banks.
    let row = &rows[0];
    assert!(row.input.port_features.len() > 0);
    assert!(compact[0].input.port_features.is_empty());
    assert!(compact[0].input.root_meta.is_empty());
    assert!(row.bytes() > compact[0].bytes());
    assert_eq!(compact[0].bytes(), restored[0].bytes());
    let run = |individual: bool| {
        let model = Model::load(&config, &order, &state, device).unwrap();
        let mut outputs = vec![];
        if individual {
            for row in &rows {
                let prediction = model
                    .predict(&frozen::pack(&[row]).unwrap().to_device(device))
                    .unwrap();
                (&prediction.logp.sum(Kind::Float)
                    + &prediction.values.sum(Kind::Float) * 0.17
                    + &prediction.entropy.sum(Kind::Float) * 0.03)
                    .backward();
                outputs.push(Tensor::stack(
                    &[prediction.logp, prediction.values, prediction.entropy],
                    1,
                ));
            }
        } else {
            let prediction = model.predict(&packed.to_device(device)).unwrap();
            (&prediction.logp.sum(Kind::Float)
                + &prediction.values.sum(Kind::Float) * 0.17
                + &prediction.entropy.sum(Kind::Float) * 0.03)
                .backward();
            outputs.push(Tensor::stack(
                &[prediction.logp, prediction.values, prediction.entropy],
                1,
            ));
        }
        (
            Tensor::cat(&outputs, 0),
            model
                .parameters
                .iter()
                .map(|(n, t)| (n.clone(), t.grad().copy()))
                .collect::<Vec<_>>(),
        )
    };
    let batched = run(false);
    let individual = run(true);
    let error = f64::try_from((&batched.0 - &individual.0).abs().max()).unwrap();
    assert!(error <= tolerance, "forward {device:?}: {error}");
    for ((name, a), (other, b)) in batched.1.iter().zip(&individual.1) {
        assert_eq!(name, other);
        let error = f64::try_from((a - b).abs().max()).unwrap();
        assert!(error <= tolerance, "gradient {device:?} {name}: {error}");
    }
    // Repeated evaluation of exactly the same physical row cannot observe wall scheduling.
    let model = Model::load(&config, &order, &state, device).unwrap();
    let first = model.predict(&packed.to_device(device)).unwrap();
    let second = model.predict(&restored_packed.to_device(device)).unwrap();
    assert!(first.logp.equal(&second.logp));
    assert!(first.values.equal(&second.values));
}
#[test]
fn physical_packing_frozen_restore_and_individual_cpu_gradients_agree() {
    packed_physical_parity(Device::Cpu, 1e-6);
}
#[test]
fn physical_packing_frozen_restore_and_individual_cuda_gradients_agree() {
    if aic_torch::tch::Cuda::is_available() {
        packed_physical_parity(Device::Cuda(0), 1e-5);
    }
}

#[test]
#[ignore = "requires AIC_PHYSICAL_INPUTS exported native cases and AIC_PHYSICAL_SIZES output"]
fn actual_native_banks_pack_freeze_restore_and_report_bytes() {
    use crate::{
        collection::{batch, policy::Choice},
        training::frozen::{self, Frozen},
    };
    use aic_pilot_native::learning::{Domain, Input, Observation};
    use std::sync::Arc;
    #[derive(serde::Deserialize)]
    struct Case {
        name: String,
        input: Input,
        domain: Domain,
        counts: serde_json::Value,
    }
    let cases: Vec<Case> = serde_json::from_slice(
        &std::fs::read(std::env::var("AIC_PHYSICAL_INPUTS").unwrap()).unwrap(),
    )
    .unwrap();
    let mut reports = vec![];
    for c in cases {
        let observation = Observation {
            input: Arc::new(c.input),
            domain: Arc::new(c.domain),
            handle: 1,
            blueprint_sha256: String::new(),
            revision: 0,
            attempts: 0,
            done: false,
        };
        let root = observation
            .input
            .root_meta
            .iter()
            .position(|r| r[0] == 0)
            .unwrap();
        let owner = observation.input.root_meta[root][1] as usize;
        let mut used = std::collections::BTreeSet::new();
        let ports = observation.domain.groups[owner]
            .iter()
            .map(|g| g.ports.iter().position(|p| used.insert(p.clone())).unwrap())
            .collect();
        let choice = Choice {
            root,
            ports,
            old_logp: 0.,
            old_value: 0.,
            entropy: 0.,
        };
        let recorded = Frozen::record(&observation, &choice).unwrap();
        assert!(Arc::ptr_eq(&observation.input, &recorded.input));
        let compact = recorded.compact();
        let bytes = postcard::to_stdvec(&compact).unwrap();
        let restored: Frozen = postcard::from_bytes(&bytes).unwrap();
        let original = batch::graph(&[&observation], Device::Cpu).unwrap();
        let packed = frozen::pack(&[&restored]).unwrap();
        for (name, t) in &original.graph {
            assert!(t.equal(&packed.graph[name]), "{} {name}", c.name);
        }
        let tensor_bytes = |t: &Tensor| {
            t.numel()
                * match t.kind() {
                    Kind::Int64 => 8,
                    Kind::Float => 4,
                    Kind::Bool => 1,
                    _ => unreachable!(),
                }
        };
        let packed_bytes = packed.graph.values().map(tensor_bytes).sum::<usize>()
            + packed
                .branches
                .as_ref()
                .unwrap()
                .values()
                .map(tensor_bytes)
                .sum::<usize>()
            + tensor_bytes(&packed.roots)
            + tensor_bytes(&packed.selected);
        let mut report = c.counts;
        report["packed_bytes"] = serde_json::json!(packed_bytes);
        report["compact_estimate_bytes"] = serde_json::json!(compact.bytes());
        report["compact_postcard_bytes"] = serde_json::json!(bytes.len());
        println!("{report}");
        reports.push(report);
    }
    std::fs::write(
        std::env::var("AIC_PHYSICAL_SIZES").unwrap(),
        serde_json::to_vec_pretty(&reports).unwrap(),
    )
    .unwrap();
}
