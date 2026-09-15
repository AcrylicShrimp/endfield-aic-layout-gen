//! Available-family prior against direct enumerated probabilities and gradients.
use super::*;
use crate::collection::{batch, policy};
use crate::training::frozen::{self, Frozen};
use aic_pilot_native::learning::{Domain, Input, Observation};
use aic_torch::tch::no_grad;
use std::{collections::BTreeMap, sync::Arc};

fn numeric(device: Device, tolerance: f64) -> Result<()> {
    let valid = [[100usize, 10, 1], [0, 7, 1], [0, 0, 1]];
    let mut b = Tensors::new();
    let (mut owners, mut mask, mut corrections, mut selected) = (vec![], vec![], vec![], vec![]);
    for (graph, counts) in valid.iter().enumerate() {
        selected.push((graph * 300 + counts.iter().position(|&n| n > 0).unwrap() * 100) as i64);
        for &count in counts {
            for i in 0..100 {
                owners.push(graph as i64);
                mask.push(i >= count);
                corrections.push(if i < count {
                    (count as f64).ln() as f32
                } else {
                    0.
                });
            }
        }
    }
    b.insert(
        "action_graph".into(),
        Tensor::from_slice(&owners).to_device(device),
    );
    b.insert(
        "root_invalid".into(),
        Tensor::from_slice(&mask).to_device(device),
    );
    b.insert(
        "kind_log_counts".into(),
        Tensor::from_slice(&corrections).to_device(device),
    );
    let zero = Tensor::zeros([900], (Kind::Float, device));
    let (logp, entropy) = distribution(&zero, &b, 3);
    assert!(bool::try_from(entropy.isfinite().all())?);
    let p = Vec::<f32>::try_from(logp.exp().to_device(Device::Cpu))?;
    for (graph, counts) in valid.iter().enumerate() {
        let families = counts.iter().filter(|&&n| n > 0).count();
        for (family, &count) in counts.iter().enumerate() {
            let start = graph * 300 + family * 100;
            let mass = p[start..start + 100].iter().map(|&p| p as f64).sum::<f64>();
            assert!((mass - if count > 0 { 1. / families as f64 } else { 0. }).abs() <= tolerance);
        }
    }
    assert!(
        p.iter()
            .zip(&mask)
            .all(|(&p, &invalid)| !invalid || p == 0.)
    );
    // Historical totals would give (100,10,1)/111, not equal family mass.
    let saved = b
        .insert(
            "kind_log_counts".into(),
            Tensor::full([900], 100f64.ln(), (Kind::Float, device)),
        )
        .unwrap();
    let (old, _) = distribution(&zero, &b, 3);
    let old = Vec::<f32>::try_from(old.exp().to_device(Device::Cpu))?;
    for (family, count) in [100., 10., 1.].into_iter().enumerate() {
        assert!(
            (old[family * 100..family * 100 + 100]
                .iter()
                .map(|&p| p as f64)
                .sum::<f64>()
                - count / 111.)
                .abs()
                <= tolerance
        );
    }
    b.insert("kind_log_counts".into(), saved);
    let values = (Tensor::arange(900, (Kind::Float, device)) * 0.17).sin();
    let run =
        |reference: bool| -> Result<(Tensor, Tensor, Tensor)> {
            let logits = values.copy().set_requires_grad(true);
            let (logp, entropy) =
                if reference {
                    let mut logs = vec![];
                    let mut entropies = vec![];
                    for graph in 0..3 {
                        let indices = (graph * 300..graph * 300 + 300)
                            .filter(|&i| !mask[i])
                            .map(|i| i as i64)
                            .collect::<Vec<_>>();
                        let indices = Tensor::from_slice(&indices).to_device(device);
                        let valid_logits = logits.index_select(0, &indices)
                            - b["kind_log_counts"].index_select(0, &indices);
                        let valid_logp = valid_logits.log_softmax(0, Kind::Float);
                        entropies.push((-valid_logp.exp() * &valid_logp).sum(Kind::Float));
                        logs.push(
                            Tensor::full([300], f64::NEG_INFINITY, (Kind::Float, device))
                                .index_copy(0, &(indices - (graph * 300) as i64), &valid_logp),
                        );
                    }
                    (Tensor::cat(&logs, 0), Tensor::stack(&entropies, 0))
                } else {
                    distribution(&logits, &b, 3)
                };
            (logp
                .index_select(0, &Tensor::from_slice(&selected).to_device(device))
                .sum(Kind::Float)
                + entropy.sum(Kind::Float) * 0.23)
                .backward();
            assert!(bool::try_from(logits.grad().isfinite().all())?);
            let gradient = Vec::<f32>::try_from(logits.grad().to_device(Device::Cpu))?;
            assert!(
                gradient
                    .iter()
                    .zip(&mask)
                    .all(|(&g, &invalid)| !invalid || g == 0.)
            );
            Ok((logp.exp().detach(), entropy.detach(), logits.grad()))
        };
    let actual = run(false)?;
    let expected = run(true)?;
    for (name, a, b) in [
        ("probability", actual.0, expected.0),
        ("entropy", actual.1, expected.1),
        ("gradient", actual.2, expected.2),
    ] {
        let error = f64::try_from((a - b).abs().max())?;
        assert!(error <= tolerance, "{device:?} {name}: {error}");
    }
    Ok(())
}
#[test]
fn available_families_exact_masks_entropy_and_gradients_match_enumeration() -> Result<()> {
    aic_torch::tch::set_num_threads(1);
    numeric(Device::Cpu, 1e-6)?;
    if aic_torch::tch::Cuda::is_available() {
        numeric(Device::Cuda(0), 1e-5)?;
    }
    Ok(())
}
#[test]
fn all_masked_or_empty_graph_fails_at_packing_with_case_identity() -> Result<()> {
    let input = Input {
        node_static: vec![[0.; aic_pilot_native::learning::schema::NODE_STATIC]],
        node_state: vec![[0.; 10]],
        global_features: vec![0.; aic_pilot_native::learning::GLOBAL_FEATURES],
        action_features: vec![[0.; 12]],
        action_references: vec![[0, -1]],
        root_meta: vec![[0, 0, 0]],
        kind_log_counts: vec![0.],
        root_invalid: vec![true],
        ..Input::default()
    };
    let mut row = Observation {
        input: Arc::new(input),
        domain: Arc::new(Domain { groups: vec![] }),
        handle: 1,
        blueprint_sha256: "explicit-test-case".into(),
        revision: 0,
        attempts: 0,
        done: false,
    };
    let error = batch::graph(&[&row], Device::Cpu)
        .err()
        .unwrap()
        .to_string();
    assert!(error.starts_with("aic-learning.empty-actions: graph 0"));
    assert!(error.contains("explicit-test-case"));
    let input = Arc::make_mut(&mut row.input);
    input.action_features.clear();
    input.action_references.clear();
    input.root_meta.clear();
    input.kind_log_counts.clear();
    input.root_invalid.clear();
    assert!(batch::graph(&[&row], Device::Cpu).is_err());
    Ok(())
}

#[test]
#[ignore = "requires AIC_PHYSICAL_INPUTS exported current native masked observations"]
fn actual_native_masks_sampling_and_frozen_replay_use_available_family_prior() -> Result<()> {
    let _lock = crate::TORCH_TEST.lock().unwrap();
    aic_torch::tch::set_num_threads(1);
    #[derive(serde::Deserialize)]
    struct Case {
        input: Input,
        domain: Domain,
        name: String,
    }
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(std::env::var("AIC_PHYSICAL_INPUTS")?)?)?;
    let c: crate::training::config::Config =
        serde_json::from_str(include_str!("../../../../tools/native-training/smoke.json"))?;
    let (order, state) = Model::initial_state(&c.model, 71)?;
    for (n, t) in &state {
        if n.starts_with("model.actor.") {
            let _ = t.shallow_clone().zero_();
        }
    }
    let rows = cases
        .into_iter()
        .map(|c| Observation {
            input: Arc::new(c.input),
            domain: Arc::new(c.domain),
            handle: 1,
            blueprint_sha256: c.name,
            revision: 0,
            attempts: 0,
            done: false,
        })
        .collect::<Vec<_>>();
    for row in &rows {
        let mut counts = [0usize; 3];
        for (meta, &invalid) in row.input.root_meta.iter().zip(&row.input.root_invalid) {
            counts[meta[0] as usize] += usize::from(!invalid);
        }
        for ((meta, &invalid), &correction) in row
            .input
            .root_meta
            .iter()
            .zip(&row.input.root_invalid)
            .zip(&row.input.kind_log_counts)
        {
            assert_eq!(
                correction,
                if invalid {
                    0.
                } else {
                    (counts[meta[0] as usize] as f64).ln() as f32
                }
            );
        }
    }
    assert!(rows.iter().any(|o| o.input.root_invalid.iter().any(|&v| v)));
    for (device, tolerance) in [(Device::Cpu, 1e-6)]
        .into_iter()
        .chain(aic_torch::tch::Cuda::is_available().then_some((Device::Cuda(0), 1e-5)))
    {
        let model = Model::load(&c.model, &order, &state, device)?;
        let refs = rows.iter().collect::<Vec<_>>();
        let packed = batch::graph(&refs, device)?;
        let predicted = no_grad(|| model.roots(&packed));
        let probabilities = Vec::<f32>::try_from(predicted.root_logp.exp().to_device(Device::Cpu))?;
        let mut offset = 0;
        for row in &rows {
            let mut masses = BTreeMap::<i64, f64>::new();
            for (i, (meta, &invalid)) in row
                .input
                .root_meta
                .iter()
                .zip(&row.input.root_invalid)
                .enumerate()
            {
                let p = probabilities[offset + i];
                if invalid {
                    assert_eq!(p, 0.);
                } else {
                    *masses.entry(meta[0]).or_default() += p as f64;
                }
            }
            for p in masses.values() {
                assert!(
                    (p - 1. / masses.len() as f64).abs() <= tolerance,
                    "{device:?} case={} mass={p} expected={} error={}",
                    row.blueprint_sha256,
                    1. / masses.len() as f64,
                    (p - 1. / masses.len() as f64).abs()
                );
            }
            offset += row.input.root_meta.len();
        }
        let mut sampler = aic_torch::Generator::new(31)?;
        let choices = policy::sample(&model, &refs, &mut sampler, device)?;
        let frozen = refs
            .iter()
            .zip(&choices)
            .map(|(o, c)| Frozen::record(o, c).map(|f| f.compact()))
            .collect::<Result<Vec<_>>>()?;
        let restored: Vec<Frozen> = postcard::from_bytes(&postcard::to_stdvec(&frozen)?)?;
        let prediction = model
            .predict(&frozen::pack(&restored.iter().collect::<Vec<_>>())?.to_device(device))?;
        for (i, c) in choices.iter().enumerate() {
            assert!(!rows[i].input.root_invalid[c.root]);
            assert!((prediction.logp.double_value(&[i as i64]) - c.old_logp).abs() <= tolerance);
            assert!((prediction.values.double_value(&[i as i64]) - c.old_value).abs() <= tolerance);
            assert!((prediction.entropy.double_value(&[i as i64]) - c.entropy).abs() <= tolerance);
        }
        println!(
            "device={device:?} cases={} available family masses, exact masks, frozen joint logp/value/entropy passed",
            rows.len()
        );
    }
    Ok(())
}
