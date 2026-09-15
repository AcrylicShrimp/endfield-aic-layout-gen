use super::{curriculum, frozen::Frozen, replay, schedule};
use crate::{
    collection::{
        policy::{self, Choice},
        rollout::{Rollout, Sample, Settings},
    },
    model::Model,
    schema::{ModelConfig, UpdateConfig},
    update::Learner,
};
use aic_pilot_native::learning::{Domain, Input, Observation};
use aic_torch::{Generator, tch::Device};
use anyhow::Result;
use serde_json::json;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
static TORCH: Mutex<()> = Mutex::new(());
fn observation(attempts: u64) -> Observation {
    Observation {
        input: Arc::new(Input {
            node_static: vec![[0.1; 14]],
            node_state: vec![[0.; 10]],
            global_features: vec![0.; 7],
            action_features: vec![[0.; 12], [0.5; 12]],
            action_references: vec![[0, -1]; 2],
            root_meta: vec![[1, 0, 0]; 2],
            kind_log_counts: vec![0.; 2],
            root_invalid: vec![false; 2],
            ..Input::default()
        }),
        domain: Arc::new(Domain { groups: vec![] }),
        handle: 1,
        blueprint_sha256: "test".into(),
        revision: attempts,
        attempts,
        done: false,
    }
}
fn learner() -> Result<Learner> {
    let model = ModelConfig {
        width: 8,
        message_rounds: 1,
        node_features: 24,
        edge_features: 14,
        global_features: 7,
        action_features: 12,
        port_features: 12,
    };
    let optimizer = UpdateConfig {
        adam_backend: "foreach".into(),
        learning_rate: 0.003,
        beta1: 0.9,
        beta2: 0.999,
        epsilon: 1e-8,
        clip_epsilon: 0.2,
        entropy_coefficient: 0.,
        imitation_coefficient: 0.05,
        gradient_clip: 0.5,
    };
    let (order, state) = Model::initial_state(&model, 17)?;
    Learner::load(&model, &optimizer, &order, &state, Device::Cpu, 0)
}
fn replay_config() -> replay::Config {
    replay::Config {
        sampler: "balanced_available".into(),
        max_sequences: 8,
        max_labels: 64,
        max_bytes: 1024 * 1024,
        retention: replay::Retention {
            ema_alpha: 0.5,
            minimum_draws: 8,
            minimum_updates: 2,
            stale_updates: 64,
        },
    }
}
fn rows(start: u64, count: usize, terminal: bool, improved: Option<usize>) -> Rollout {
    let mut r = Rollout::default();
    let mut evidence = vec![];
    for i in 0..count {
        let reward = if improved == Some(i) { 2. } else { 0. };
        r.samples.push(Sample {stream:0,observation:observation(start+i as u64),choice:Choice {root:i%2,ports:vec![],old_logp:-0.7,old_value:0.,entropy:0.7},reward,terminal:terminal&&i==count-1,outcome:json!({"action":{"kind":if i%2==0{"prepare"}else{"route"}},"outcome":{"status":if reward>0.{"improved"}else{"edited"}}})});
        evidence.push(json!({"observation":{"facilities":[{"x":start+i as u64}],"revision":start+i as u64},"validated_witness":if reward>0.{json!({"valid":true})}else{serde_json::Value::Null}}));
    }
    r.evidence = vec![json!({"rows":evidence})];
    r.raw = (0..count).map(|i| i as f32 - 2.).collect();
    r.targets = vec![1.; count];
    r
}
fn frozen(r: &Rollout) -> Result<Vec<Frozen>> {
    r.samples
        .iter()
        .map(|s| Frozen::record(&s.observation, &s.choice))
        .collect()
}

#[test]
fn full_epochs_cover_partial_minibatch_and_freeze_actor_after_kl() -> Result<()> {
    let _lock = TORCH.lock().unwrap();
    aic_torch::tch::set_num_threads(1);
    let mut a = learner()?;
    let mut b = learner()?;
    let mut r = rows(0, 5, true, None);
    for row in &mut r.samples {
        row.choice.old_logp = 10.;
    }
    let settings = Settings {
        episode_ids: vec!["episode".into()],
        episode_actions: 5,
        chunk_actions: 5,
        minibatch_size: 2,
        gamma: 1.,
        gae_lambda: 0.95,
        advantage_normalization: "center".into(),
        max_observation_bytes: 1024 * 1024,
    };
    let (one, _) = schedule::update(
        &mut a,
        &r,
        &settings,
        &schedule::Config {
            epochs: 1,
            target_kl: 0.02,
        },
        &mut replay::Buffer::default(),
        &replay_config(),
        17,
        1,
        || Ok(()),
    )?;
    let (all, _) = schedule::update(
        &mut b,
        &r,
        &settings,
        &schedule::Config {
            epochs: 3,
            target_kl: 0.02,
        },
        &mut replay::Buffer::default(),
        &replay_config(),
        17,
        1,
        || Ok(()),
    )?;
    assert_eq!(one.minibatch_updates, 3);
    assert_eq!(all.minibatch_updates, 9);
    assert_eq!(all.actor_epochs, 1);
    assert_eq!(all.critic_epochs, 3);
    assert!(
        all.epochs
            .iter()
            .all(|e| e.samples == 5 && e.minibatches == 3)
    );
    let sa = a.snapshot()?;
    let sb = b.snapshot()?;
    for (name, value) in &sa {
        if name.starts_with("adam.actor.")
            || (name.starts_with("model.") && !name.starts_with("model.critic."))
        {
            assert!(
                value.equal(&sb[name]),
                "actor changed through critic tail: {name}"
            );
        }
    }
    assert!(!sa["model.critic.2.weight"].equal(&sb["model.critic.2.weight"]));
    Ok(())
}
#[test]
fn replay_crosses_chunks_waits_for_terminal_and_deduplicates() -> Result<()> {
    let mut buffer = replay::Buffer::default();
    let config = replay_config();
    let products = vec!["a".into()];
    let strata = BTreeMap::from([
        ("small".into(), products.clone()),
        ("large".into(), vec!["b".into()]),
    ]);
    let first = rows(0, 2, false, None);
    let second = rows(2, 2, true, Some(0));
    buffer.ingest(
        &first,
        &frozen(&first)?,
        1,
        0,
        &products,
        &strata,
        0.5,
        &config,
    )?;
    assert!(buffer.state.entries.is_empty());
    assert_eq!(buffer.inputs.len(), 2);
    buffer.ingest(
        &second,
        &frozen(&second)?,
        2,
        0,
        &products,
        &strata,
        0.5,
        &config,
    )?;
    assert_eq!(buffer.state.entries.len(), 1);
    assert!(buffer.state.active.is_empty());
    let entry = buffer.state.entries.values().next().unwrap();
    assert_eq!(
        entry.labels.iter().map(|l| l.returned).collect::<Vec<_>>(),
        vec![0.5, 1., 2.]
    );
    assert_eq!(
        entry.labels.iter().map(|l| l.reference).collect::<Vec<_>>(),
        vec![[1, 0], [1, 1], [2, 0]]
    );
    assert!(replay::Sampling::new(&buffer, 2).is_err());
    assert!(replay::Sampling::new(&buffer, 3).is_ok());
    buffer.ingest(
        &first,
        &frozen(&first)?,
        3,
        1,
        &products,
        &strata,
        0.5,
        &config,
    )?;
    buffer.ingest(
        &second,
        &frozen(&second)?,
        4,
        1,
        &products,
        &strata,
        0.5,
        &config,
    )?;
    assert_eq!(buffer.state.entries.len(), 1);
    assert_eq!(buffer.state.duplicates, 1);
    assert_eq!(buffer.inputs.len(), 3);
    // Sparse binary archives round-trip without a tensor or Python serializer.
    let bytes = postcard::to_allocvec(
        &buffer
            .inputs
            .iter()
            .map(|(r, v)| (*r, v.as_ref()))
            .collect::<Vec<_>>(),
    )?;
    let recovered: Vec<(replay::Reference, Frozen)> = postcard::from_bytes(&bytes)?;
    assert_eq!(recovered.len(), 3);
    assert_eq!(recovered[0].1.root, 0);
    Ok(())
}
#[test]
fn replay_stages_and_sequences_each_receive_equal_total_weight() -> Result<()> {
    let _lock = TORCH.lock().unwrap();
    let f = Arc::new(frozen(&rows(0, 1, true, None))?.remove(0));
    let prepared = replay::prepare(&[
        replay::Draw {
            id: "a".into(),
            rows: vec![
                (f.clone(), 1., "pose".into()),
                (f.clone(), 1., "pose".into()),
                (f.clone(), 1., "route".into()),
            ],
        },
        replay::Draw {
            id: "b".into(),
            rows: vec![(f, 1., "route".into())],
        },
    ])?
    .unwrap();
    assert_eq!(
        Vec::<f32>::try_from(prepared.batch.weights)?,
        vec![0.125, 0.125, 0.25, 0.5]
    );
    Ok(())
}
#[test]
fn independent_evaluation_sampling_does_not_advance_training_rng() -> Result<()> {
    let _lock = TORCH.lock().unwrap();
    let mut learner = learner()?;
    let before = learner.sampler.state()?;
    let o = observation(0);
    let mut evaluation = Generator::new(99)?;
    policy::sample_with(
        &learner.model,
        &[&o],
        |_, p| evaluation.categorical(p),
        Device::Cpu,
    )?;
    assert!(before.equal(&learner.sampler.state()?));
    let a = policy::sample(&learner.model, &[&o], &mut learner.sampler, Device::Cpu)?;
    let mut restored = Generator::new(0)?;
    restored.restore(&before)?;
    let b = policy::sample(&learner.model, &[&o], &mut restored, Device::Cpu)?;
    assert_eq!(a[0].root, b[0].root);
    assert_eq!(a[0].old_logp, b[0].old_logp);
    Ok(())
}
#[test]
fn curriculum_rotates_integer_quotas_and_falls_back_without_donors() -> Result<()> {
    for count in [1, 4, 32, 128] {
        let total = (1..=10)
            .map(|w| curriculum::quotas(count, w))
            .collect::<Result<Vec<_>>>()?;
        assert_eq!(total.iter().map(|q| q[1]).sum::<usize>(), count);
        assert_eq!(
            (2..=11)
                .map(|w| curriculum::quotas(count, w).unwrap()[2])
                .sum::<usize>(),
            count
        );
        assert!(total.iter().all(|q| q.iter().sum::<usize>() == count));
    }
    let config = curriculum::Config {
        schema: "aic-best-starts-v1".into(),
        selection_seed: 19,
        percentages: [80, 10, 10],
        donors: "strict-area-improvement".into(),
    };
    let plan = curriculum::choose(&config, 17, 2, 128, &[], &Default::default())?;
    assert_eq!(plan.actual, [128, 0, 0]);
    assert_eq!(plan.slots.len(), 128);
    assert!(
        plan.slots
            .iter()
            .all(|s| s.depth == 0 && s.inherited.is_none())
    );
    Ok(())
}

#[test]
fn curriculum_uses_only_strict_improvements_and_preserves_root_depth() -> Result<()> {
    use aic_pilot_native::generation::{geometry_identity, source::Generated};
    use curriculum::{Config, Parent, Root};
    let input: Generated = serde_json::from_value(
        json!({"request":{"schema_version":1,"instances":[],"terminals":[]},"original":{"schema_version":1,"placements":[],"utilizations":{},"transport_networks":[],"logistics_components":[]},"metadata":{}}),
    )?;
    let root = Root {
        wave: 0,
        area: 100,
        request_sha256: "root-request".into(),
        geometry_sha256: "root-geometry".into(),
    };
    let good = Parent {
        stream: 0,
        wave: 0,
        depth: 0,
        root: root.clone(),
        start_area: 100,
        best_area: 90,
        complete: true,
        validated: true,
        best: input.clone(),
    };
    let neutral = Parent {
        stream: 1,
        best_area: 100,
        ..good.clone()
    };
    let config = Config {
        schema: "aic-best-starts-v1".into(),
        selection_seed: 19,
        percentages: [80, 10, 10],
        donors: "strict-area-improvement".into(),
    };
    let plan = curriculum::choose(
        &config,
        17,
        1,
        10,
        &[good.clone(), neutral],
        &Default::default(),
    )?;
    assert_eq!(plan.actual, [9, 1, 0]);
    let inherited = plan.slots.iter().find(|s| s.depth == 1).unwrap();
    assert_eq!(inherited.parent, Some([0, 0, 0]));
    assert_eq!(
        inherited.root.as_ref().unwrap().request_sha256,
        "root-request"
    );
    let second = Parent {
        wave: 1,
        depth: 1,
        start_area: 90,
        best_area: 80,
        ..good.clone()
    };
    let next = curriculum::choose(&config, 17, 2, 10, &[second], &Default::default())?;
    assert_eq!(next.actual, [9, 0, 1]);
    assert_eq!(
        next.slots
            .iter()
            .find(|s| s.depth == 2)
            .unwrap()
            .root
            .as_ref()
            .unwrap()
            .wave,
        0
    );
    let reserved = [geometry_identity(&input.original)].into_iter().collect();
    assert_eq!(
        curriculum::choose(&config, 17, 1, 10, &[good.clone()], &reserved)?.actual,
        [10, 0, 0]
    );
    assert!(
        curriculum::choose(
            &config,
            17,
            1,
            10,
            &[Parent {
                complete: false,
                ..good
            }],
            &Default::default()
        )
        .is_err()
    );
    Ok(())
}
#[test]
fn utility_eviction_removes_low_observed_value_in_oversupplied_stratum() {
    use replay::{Entry, Utility};
    let entry = |id: &str, stratum: &str, utility: f64| Entry {
        id: id.into(),
        admitted_update: 1,
        stream: 0,
        episode: 0,
        stratum: stratum.into(),
        product: "p".into(),
        labels: vec![],
        uses: 8,
        occurrences: 1,
        utility: Some(Utility {
            ema: utility,
            draws: 8,
            updates: 2,
            last_update: 3,
        }),
    };
    let entries = [
        entry("a", "small", 8.),
        entry("b", "small", 1.),
        entry("c", "large", 0.),
    ]
    .into_iter()
    .map(|e| (e.id.clone(), e))
    .collect();
    let mut index = super::retention::Index::new(&entries, 4, &replay_config().retention);
    assert_eq!(index.choose("large").unwrap(), "c");
    assert_eq!(index.choose("small").unwrap(), "b");
    assert_eq!(index.choose("small").unwrap(), "a");
}

#[test]
fn nonempty_learner_checkpoint_restores_every_field_and_next_update() -> Result<()> {
    let _lock = TORCH.lock().unwrap();
    aic_torch::tch::set_num_threads(1);
    for device in [Device::Cpu]
        .into_iter()
        .chain(aic_torch::tch::Cuda::is_available().then_some(Device::Cuda(0)))
    {
        let initial = learner()?;
        let names: Vec<_> = initial
            .model
            .parameters
            .iter()
            .map(|p| p.0.clone())
            .collect();
        let mut initial = initial;
        let saved = initial.snapshot()?;
        let mut a = Learner::load(
            &initial.model.config,
            &initial.config,
            &names,
            &saved,
            device,
            0,
        )?;
        let r = rows(0, 5, true, None);
        let f = frozen(&r)?;
        let batch = crate::input::PpoBatch {
            packed: super::frozen::pack(&f.iter().collect::<Vec<_>>())?,
            old_logp: aic_torch::tch::Tensor::from_slice(&[-0.7f32; 5]),
            advantages: aic_torch::tch::Tensor::from_slice(&[-2f32, -1., 0., 1., 2.]),
            targets: aic_torch::tch::Tensor::from_slice(&[1f32; 5]),
        }
        .to_device(device);
        a.apply_minibatch(&batch, None, true)?;
        let saved = a.snapshot()?;
        let header = json!({"schema":"aic-training-checkpoint-v2", "contract":aic_pilot_native::learning::current_contract()});
        let path = std::path::Path::new("nonempty/manifest.json");
        for component in ["observation", "family_prior"] {
            let mut changed = header.clone();
            changed["contract"][component] = "different-semantics-same-shapes".into();
            assert!(
                aic_pilot_native::learning::validate_envelope(
                    &changed,
                    "aic-training-checkpoint-v2",
                    path
                )
                .is_err()
            );
        }
        aic_pilot_native::learning::validate_envelope(&header, "aic-training-checkpoint-v2", path)
            .map_err(anyhow::Error::msg)?;
        let mut b = Learner::load(
            &a.model.config,
            &a.config,
            &names,
            &saved,
            device,
            a.completed_steps,
        )?;
        let restored = b.snapshot()?;
        for (name, value) in &saved {
            assert!(
                value.equal(&restored[name]),
                "restore changed {device:?} {name}"
            );
        }
        a.apply_minibatch(&batch, None, true)?;
        b.apply_minibatch(&batch, None, true)?;
        let left = a.snapshot()?;
        let right = b.snapshot()?;
        for (name, value) in left {
            assert!(
                value.equal(&right[&name]),
                "next update changed {device:?} {name}"
            );
        }
    }
    Ok(())
}
