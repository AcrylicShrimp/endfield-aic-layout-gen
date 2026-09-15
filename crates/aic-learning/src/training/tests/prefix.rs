//! Binding alias, linear prefix work, and actual sparse checkpoint round trips.
use super::*;
use crate::{
    collection::batch,
    input::{PpoBatch, Tensors},
    model,
    training::{config, frozen, state, storage},
};
use aic_pilot_native::learning::{PortGroup, schema::*};
use aic_torch::tch::{Kind, Tensor, no_grad};

fn config() -> config::Config {
    serde_json::from_str(include_str!(
        "../../../../../tools/native-training/smoke.json"
    ))
    .unwrap()
}
fn observation(terminals: usize) -> Observation {
    let mut input = Input::default();
    for i in 0..=terminals {
        input
            .node_static
            .push(std::array::from_fn(|j| ((i + j) as f32 * 0.13).sin()));
        input
            .node_state
            .push(std::array::from_fn(|j| ((i + j) as f32 * 0.17).cos()));
        if i > 0 {
            input.edge_index.push([0, i as i64]);
            input
                .edge_static
                .push(std::array::from_fn(|j| ((i + j) as f32 * 0.23).sin()));
            input.edge_state.push([0., 0., 0., 1.]);
        }
    }
    input.global_features = vec![0.1; GLOBAL_FEATURES];
    input.action_features = vec![[0.1; ACTION_FEATURES], [0.2; ACTION_FEATURES]];
    input.action_references = vec![[0, -1], [-1, -1]];
    input.root_meta = vec![[0, 0, 0], [1, 0, 0]];
    input.root_invalid = vec![false, true];
    input.kind_log_counts = vec![0.; 2];
    let port_count = terminals + 2;
    let mut groups = vec![];
    for terminal in 0..terminals {
        let start = input.port_features.len();
        for port in 0..port_count {
            let mut row = [0.; PORT_FEATURES];
            row[0] = [1., -1., 0.25, -0.75, 1.5][port % 5];
            row[1] = port as f32 * 0.2;
            row[port::ORIGINAL] = f32::from(port == terminal + 2);
            row[port::CURRENT] = row[port::ORIGINAL];
            input.port_features.push(row);
        }
        groups.push(PortGroup {
            graph_node: terminal + 1,
            equivalence: terminal,
            port_equivalences: (0..port_count).collect(),
            ports: (0..port_count).map(|p| format!("port-{p}")).collect(),
            features: [(0, (start, start + port_count))].into_iter().collect(),
        });
    }
    Observation {
        input: Arc::new(input),
        domain: Arc::new(Domain {
            groups: vec![groups],
        }),
        handle: 1,
        blueprint_sha256: String::new(),
        revision: 0,
        attempts: 0,
        done: false,
    }
}
fn choice(ports: Vec<usize>) -> Choice {
    Choice {
        root: 0,
        ports,
        old_logp: 0.,
        old_value: 0.,
        entropy: 0.,
    }
}
fn set(state: &Tensors, name: &str, row: i64, col: i64, value: f64) {
    let _ = state[&format!("model.{name}.weight")]
        .get(row)
        .get(col)
        .fill_(value);
}
fn alias(device: Device) -> Result<()> {
    let c = config();
    let w = c.model.width;
    let (order, state) = Model::initial_state(&c.model, 29)?;
    no_grad(|| {
        for (name, value) in &state {
            if name.starts_with("model.") {
                let _ = value.shallow_clone().zero_();
            }
        }
        set(&state, "port_pair.0", 0, 0, 1.);
        set(&state, "port_pair.0", 0, w, 1.);
        set(&state, "port_pair.2", 0, 0, 1.);
        set(
            &state,
            "port_actor.0",
            0,
            3 * w + ACTION_FEATURES as i64,
            0.2,
        );
        set(
            &state,
            "port_actor.0",
            0,
            3 * w + ACTION_FEATURES as i64 + PORT_FEATURES as i64,
            1.,
        );
        set(&state, "port_actor.2", 0, 0, 1.);
    });
    let model = Model::load(&c.model, &order, &state, device)?;
    let row = observation(3);
    assert!(Frozen::record(&row, &choice(vec![0, 0, 4])).is_err());
    assert!(Frozen::record(&row, &choice(vec![0, 5, 4])).is_err());
    assert!(Frozen::record(&row, &choice(vec![0, 1])).is_err());
    assert!(
        Frozen::record(
            &row,
            &Choice {
                root: 2,
                ..choice(vec![0, 1, 4])
            }
        )
        .is_err()
    );
    let a = Frozen::record(&row, &choice(vec![0, 1, 4]))?;
    let b = Frozen::record(&row, &choice(vec![1, 0, 4]))?;
    let mean = |f: &Frozen| -> [f32; PORT_FEATURES] {
        std::array::from_fn(|i| (f.assignments[0].port[i] + f.assignments[1].port[i]) / 2.)
    };
    assert_eq!(mean(&a), mean(&b));
    assert_ne!(a.assignments, b.assignments);
    assert_eq!(a.branches[2].keep, b.branches[2].keep);
    let nodes = Tensor::zeros([4, w], (Kind::Float, device));
    let _ = nodes.get(1).get(0).fill_(2.);
    let _ = nodes.get(2).get(0).fill_(-1.);
    let nodes = nodes.set_requires_grad(true);
    let pooled = Tensor::zeros([1, w], (Kind::Float, device));
    let run = |f: &Frozen| {
        let mut branches = batch::branches(&f.branches[2..], &[0], device);
        batch::assignments(
            &mut branches,
            &f.branches[2..],
            &[&f.assignments],
            &[0],
            device,
        );
        let ports = branches["assignment_ports"]
            .shallow_clone()
            .set_requires_grad(true);
        branches.insert("assignment_ports".into(), ports.shallow_clone());
        (model.ports(&nodes, &pooled, &branches), ports)
    };
    let (left, ports) = run(&a);
    let (right, _) = run(&b);
    assert!(f64::try_from((&left - right).abs().max())? > 0.01);
    left.sum(Kind::Float).backward();
    for terminal in [1, 2] {
        assert!(nodes.grad().double_value(&[terminal, 0]).abs() > 1e-5);
    }
    for pair in [0, 1] {
        assert!(ports.grad().double_value(&[pair, 0]).abs() > 1e-5);
    }
    for name in ["port_pair.0.weight", "port_pair.2.weight"] {
        let grad = model
            .parameters
            .iter()
            .find(|p| p.0 == name)
            .unwrap()
            .1
            .grad();
        assert!(f64::try_from(grad.abs().max())? > 1e-5, "{name}");
    }
    Ok(())
}
#[test]
fn swapped_bindings_have_identical_old_mean_but_distinct_learnable_logits() -> Result<()> {
    let _lock = TORCH.lock().unwrap();
    aic_torch::tch::set_num_threads(1);
    alias(Device::Cpu)?;
    if aic_torch::tch::Cuda::is_available() {
        alias(Device::Cuda(0))?;
    }
    Ok(())
}

fn assert_behavior(
    model: &Model,
    frozen: &[&Frozen],
    choices: &[Choice],
    device: Device,
    tolerance: f64,
) -> Result<()> {
    let prediction = model.predict(&frozen::pack(frozen)?.to_device(device))?;
    for (i, choice) in choices.iter().enumerate() {
        assert!(
            (prediction.logp.double_value(&[i as i64]) - choice.old_logp).abs() <= tolerance,
            "joint logp {device:?} row {i}"
        );
        assert!(
            (prediction.values.double_value(&[i as i64]) - choice.old_value).abs() <= tolerance,
            "value {device:?} row {i}"
        );
        assert!(
            (prediction.entropy.double_value(&[i as i64]) - choice.entropy).abs() <= tolerance,
            "entropy {device:?} row {i}"
        );
    }
    Ok(())
}
fn checkpoint(device: Device, tolerance: f64) -> Result<()> {
    let mut c = config();
    c.streams = 1;
    c.workers = 1;
    c.chunk_actions = c.native.max_actions as usize;
    let (order, tensors) = Model::initial_state(&c.model, 53)?;
    let mut learner = Learner::load(&c.model, &c.optimizer, &order, &tensors, device, 0)?;
    let observations = [0, 1, 2, 3].map(observation);
    let refs = observations.iter().collect::<Vec<_>>();
    batch::take_insertions();
    model::take_pair_rows();
    let choices = policy::sample(&learner.model, &refs, &mut learner.sampler, device)?;
    assert_eq!(batch::take_insertions(), 6);
    assert_eq!(model::take_pair_rows(), 6);
    let frozen = refs
        .iter()
        .zip(&choices)
        .map(|(o, c)| Frozen::record(o, c).map(|f| f.compact()))
        .collect::<Result<Vec<_>>>()?;
    assert_eq!(batch::take_insertions(), 6);
    assert_eq!(frozen.iter().map(|f| f.assignments.len()).sum::<usize>(), 6);
    assert_behavior(
        &learner.model,
        &frozen.iter().collect::<Vec<_>>(),
        &choices,
        device,
        tolerance,
    )?;
    assert_eq!(model::take_pair_rows(), 6);
    // Every branch references a single bank, including empty/heterogeneous graphs.
    for (f, choice) in frozen.iter().zip(&choices) {
        assert_eq!(f.assignments.len(), choice.ports.len());
        assert_eq!(
            f.branches.iter().map(|b| b.depth).collect::<Vec<_>>(),
            (0..choice.ports.len()).collect::<Vec<_>>()
        );
    }
    let mut buffer = replay::Buffer::default();
    for (i, f) in frozen.iter().enumerate() {
        buffer.inputs.insert([1, i as u64], Arc::new(f.clone()));
    }
    buffer.state.entries.insert(
        "binding-sequence".into(),
        replay::Entry {
            id: "binding-sequence".into(),
            admitted_update: 1,
            stream: 0,
            episode: 0,
            stratum: "small".into(),
            product: "synthetic".into(),
            labels: (0..frozen.len())
                .map(|i| replay::Label {
                    reference: [1, i as u64],
                    stage: "edit".into(),
                    returned: 1.,
                })
                .collect(),
            uses: 0,
            occurrences: 1,
            utility: None,
        },
    );
    buffer.prune_inputs();
    assert_eq!(buffer.bytes, frozen.iter().map(Frozen::bytes).sum::<u64>());
    let root = std::env::temp_dir().join(format!(
        "aic-prefix-checkpoint-{}-{device:?}",
        std::process::id()
    ));
    for name in ["checkpoints", "replay", "updates", "evaluations", "inputs"] {
        std::fs::create_dir_all(root.join(name))?;
    }
    let catalogs = aic_pilot_native::input::load_catalogs(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .as_path(),
        &c.catalogs,
    )
    .map_err(anyhow::Error::msg)?;
    let mut pool =
        aic_pilot_native::learning::pool::Pool::new(catalogs, 1, 1).map_err(anyhow::Error::msg)?;
    let state = state::State {
        update: 1,
        samples: c.chunk_actions as u64,
        wave: 0,
        actions: c.native.max_actions,
        next: Some(curriculum::choose(
            &c.curriculum,
            c.initialization_seed,
            1,
            1,
            &[],
            &Default::default(),
        )?),
        ..Default::default()
    };
    state.validate(&c)?;
    let mut store = storage::Store::default();
    let path = store.save(
        &root,
        "prefix-test",
        &state,
        &buffer,
        &mut learner,
        &mut pool,
        &order,
        &c,
    )?;
    assert_eq!(store.chunks.len(), 1);
    let (_, _, restored, mut same, _, sessions) =
        storage::restore(&root, &path, "prefix-test", &c, device)?;
    assert!(sessions.is_empty());
    let restored_rows = restored
        .inputs
        .values()
        .map(Arc::as_ref)
        .collect::<Vec<_>>();
    assert_behavior(&same.model, &restored_rows, &choices, device, tolerance)?;
    assert_eq!(restored.bytes, buffer.bytes);
    for (a, b) in frozen.iter().zip(&restored_rows) {
        assert_eq!(a.assignments, b.assignments);
        assert_eq!(a.selected, b.selected);
    }
    // Exercise ordinary SIL preparation from the hydrated sparse bank.
    let draw = replay::Draw {
        id: "binding-sequence".into(),
        rows: restored
            .inputs
            .values()
            .map(|f| (f.clone(), 1., "edit".into()))
            .collect(),
    };
    let replay = replay::prepare(&[draw])?.unwrap().batch.to_device(device);
    let ppo = PpoBatch {
        packed: frozen::pack(&restored_rows)?,
        old_logp: Tensor::from_slice(
            &choices
                .iter()
                .map(|c| c.old_logp as f32)
                .collect::<Vec<_>>(),
        ),
        advantages: Tensor::from_slice(&[-1f32, 0.5, 1., -0.5]),
        targets: Tensor::from_slice(&[1f32; 4]),
    }
    .to_device(device);
    learner.apply_minibatch(&ppo, Some(&replay), true)?;
    same.apply_minibatch(&ppo, Some(&replay), true)?;
    let same_snapshot = same.snapshot()?;
    for (name, value) in learner.snapshot()? {
        assert!(
            value.equal(&same_snapshot[&name]),
            "next update {device:?} {name}"
        );
    }
    // Save nonempty Adam state and re-run the next update after an actual restore.
    let state = state::State {
        update: 2,
        samples: 2 * c.chunk_actions as u64,
        ..state
    };
    let path = store.save(
        &root,
        "prefix-test",
        &state,
        &buffer,
        &mut learner,
        &mut pool,
        &order,
        &c,
    )?;
    let (_, _, _, mut same, _, _) = storage::restore(&root, &path, "prefix-test", &c, device)?;
    learner.apply_minibatch(&ppo, Some(&replay), true)?;
    same.apply_minibatch(&ppo, Some(&replay), true)?;
    let restored_snapshot = same.snapshot()?;
    for (name, value) in learner.snapshot()? {
        assert!(
            value.equal(&restored_snapshot[&name]),
            "nonempty next update {device:?} {name}"
        );
    }
    drop(pool);
    std::fs::remove_dir_all(root)?;
    Ok(())
}
#[test]
fn online_linear_prefix_work_matches_sparse_replay_and_real_checkpoint_restore() -> Result<()> {
    let _lock = TORCH.lock().unwrap();
    aic_torch::tch::set_num_threads(1);
    aic_torch::deterministic()?;
    checkpoint(Device::Cpu, 1e-6)?;
    if aic_torch::tch::Cuda::is_available() {
        checkpoint(Device::Cuda(0), 1e-5)?;
    }
    Ok(())
}

#[test]
#[ignore = "requires AIC_ORDER_CASES exported native enumerable domains"]
fn complete_physical_action_distribution_survives_renaming_and_equivalent_labels() -> Result<()> {
    let _lock = TORCH.lock().unwrap();
    aic_torch::tch::set_num_threads(1);
    #[derive(serde::Deserialize)]
    struct Selected {
        root: usize,
        ports: Vec<usize>,
        physical: String,
    }
    #[derive(serde::Deserialize)]
    struct Case {
        input: Input,
        domain: Domain,
        selections: Vec<Selected>,
    }
    let cases: Vec<Case> =
        serde_json::from_slice(&std::fs::read(std::env::var("AIC_ORDER_CASES")?)?)?;
    assert_eq!(cases.len(), 2);
    let c = config();
    let (order, state) = Model::initial_state(&c.model, 113)?;
    for (device, tolerance) in [(Device::Cpu, 1e-6)]
        .into_iter()
        .chain(aic_torch::tch::Cuda::is_available().then_some((Device::Cuda(0), 1e-5)))
    {
        let model = Model::load(&c.model, &order, &state, device)?;
        let distributions = no_grad(|| -> Result<Vec<BTreeMap<String, f64>>> {
            let mut distributions = vec![];
            for case in &cases {
                // Deserialize once per test case, never per alternative/normal run.
                let o = Observation {
                    input: Arc::new(case.input.clone()),
                    domain: Arc::new(serde_json::from_value(serde_json::to_value(&case.domain)?)?),
                    handle: 1,
                    blueprint_sha256: String::new(),
                    revision: 0,
                    attempts: 0,
                    done: false,
                };
                let mut mass = BTreeMap::<String, f64>::new();
                for chunk in case.selections.chunks(64) {
                    let rows = chunk
                        .iter()
                        .map(|s| {
                            Frozen::record(
                                &o,
                                &Choice {
                                    root: s.root,
                                    ports: s.ports.clone(),
                                    ..choice(vec![])
                                },
                            )
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let prediction = model.predict(
                        &frozen::pack(&rows.iter().collect::<Vec<_>>())?.to_device(device),
                    )?;
                    for (i, s) in chunk.iter().enumerate() {
                        *mass.entry(s.physical.clone()).or_default() +=
                            prediction.logp.double_value(&[i as i64]).exp();
                    }
                }
                assert!(
                    mass.len() < case.selections.len(),
                    "equivalence aggregation must be exercised"
                );
                assert!((mass.values().sum::<f64>() - 1.).abs() <= tolerance);
                distributions.push(mass);
            }
            Ok(distributions)
        })?;
        assert_eq!(
            distributions[0].keys().collect::<Vec<_>>(),
            distributions[1].keys().collect::<Vec<_>>()
        );
        let max_error = distributions[0]
            .iter()
            .map(|(key, p)| (p - distributions[1][key]).abs())
            .fold(0f64, f64::max);
        assert!(
            max_error <= tolerance,
            "{device:?} complete distribution error {max_error}"
        );
        println!(
            "device={device:?} complete_labels={} physical_classes={} maximum_probability_error={max_error}",
            cases[0].selections.len(),
            distributions[0].len()
        );
    }
    Ok(())
}
