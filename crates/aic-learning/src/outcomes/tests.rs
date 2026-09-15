use super::{
    config::{Config, Split},
    run::reserve_partition,
    sample::{PortMode, choose},
    storage,
};
use aic_layout::rng::Rng;
use aic_pilot_native::physical_editor::{
    EditWork, PhysicalCandidateOptions, PhysicalEditorDiagnostic, PhysicalSessionLimits,
    PhysicalSessionOutcome, ProceduralSession,
    learning::{Domain, Input, Observation},
};
use std::{collections::BTreeMap, fs, path::Path, sync::Arc};

const CONFIG: &[u8] = include_bytes!("../../../../docs/benchmarks/physical-outcomes/config.json");
const SOURCE: &[u8] =
    include_bytes!("../../../aic-pilot-native/tests/fixtures/procedural-no-bridge.json");

fn catalogs() -> aic_layout_gen::io::LoadedCatalogs {
    aic_layout_gen::io::load_catalogs(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized"),
    )
    .unwrap()
}

#[test]
fn registered_configuration_has_the_fixed_partition() {
    let config: Config = serde_json::from_slice(CONFIG).unwrap();
    config.validate().unwrap();
}

#[test]
fn structural_identity_and_ancestry_cannot_cross_splits() {
    let mut structures = BTreeMap::new();
    let mut ancestry = BTreeMap::new();
    assert!(reserve_partition(
        &mut structures,
        &mut ancestry,
        "physical-a",
        "root-a",
        Split::Training,
    ));
    assert!(!reserve_partition(
        &mut structures,
        &mut ancestry,
        "physical-a",
        "root-b",
        Split::Development,
    ));
    assert!(!reserve_partition(
        &mut structures,
        &mut ancestry,
        "physical-b",
        "root-a",
        Split::Confirmation,
    ));
}

#[test]
fn family_draw_is_independent_of_family_cardinality_and_reproducible() {
    let mut input = Input::default();
    for (family, count) in [(0, 20), (1, 2), (2, 1)] {
        for _ in 0..count {
            input.root_meta.push([family, 0, 0]);
            input.root_invalid.push(false);
            input.action_features.push([0.; 12]);
            input.action_references.push([-1; 2]);
        }
    }
    let observation = Observation {
        input: Arc::new(input),
        domain: Arc::new(Domain {
            groups: vec![vec![]],
        }),
        current_ports: Arc::new(vec![]),
        handle: 1,
        source_sha256: "source".into(),
        revision: 0,
        attempts: 0,
        done: false,
    };
    let mut left = Rng::new(77);
    let mut right = Rng::new(77);
    for _ in 0..50 {
        assert_eq!(
            choose(&observation, &mut left).unwrap(),
            choose(&observation, &mut right).unwrap()
        );
    }
    let mut rng = Rng::new(19);
    let mut counts = [0usize; 3];
    for _ in 0..6000 {
        counts[choose(&observation, &mut rng).unwrap().family] += 1;
    }
    assert!(
        counts
            .into_iter()
            .all(|count| (1800..=2200).contains(&count)),
        "{counts:?}"
    );
}

#[test]
fn retain_mode_exactly_reproduces_graph_aligned_current_ports() {
    let loaded = catalogs();
    let mut session = ProceduralSession::new(
        SOURCE,
        &loaded.catalogs,
        PhysicalSessionLimits {
            max_actions: 32,
            elapsed_ms: 60_000,
            route_expansions: 50_000,
            max_total_route_expansions: 1_600_000,
            search_padding: 4,
        },
    )
    .unwrap();
    let observation = session
        .observe_learning(&PhysicalCandidateOptions {
            translation_steps: vec![-2, -1, 1, 2],
            band_counts: vec![1, 2],
            structural_band_mask: true,
        })
        .unwrap();
    let mut rng = Rng::new(920_001);
    let selected = (0..10_000)
        .find_map(|_| {
            let selected = choose(&observation, &mut rng).unwrap();
            (selected.actual_port_mode == Some(PortMode::RetainCompatible)).then_some(selected)
        })
        .expect("a placement retain draw");
    let meta = observation.input.root_meta[selected.root];
    let groups = &observation.domain.groups[meta[1] as usize];
    assert_eq!(groups.len(), selected.ports.len());
    for (group, &port) in groups.iter().zip(&selected.ports) {
        assert_eq!(
            observation.current_ports[group.graph_node].as_deref(),
            Some(group.ports[port].as_str())
        );
    }
}

#[test]
fn admission_deadline_publishes_all_slots_without_action_rows() {
    let mut config: Config = serde_json::from_slice(CONFIG).unwrap();
    config.max_elapsed_ms = 1;
    config.session.elapsed_ms = 1;
    config.max_output_bytes = 2 * 1024 * 1024;
    config.metadata_reserve_bytes = 1024 * 1024;
    config.validate().unwrap();
    let base = std::env::temp_dir().join(format!("aic-outcomes-deadline-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir(&base).unwrap();
    let config_path = base.join("config.json");
    let output = base.join("output");
    fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();
    super::collect(
        &config_path,
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized"),
        &output,
    )
    .unwrap();
    let (manifest, rows) = storage::load(&output).unwrap();
    assert_eq!(manifest.configured_roots, 96);
    assert_eq!(manifest.rows, 0);
    assert!(rows.is_empty());
    assert_eq!(manifest.status, "incomplete_deadline");
    assert!(manifest.roots.iter().all(|root| root.rows == 0));
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn observed_outcome_masks_unobserved_route_labels() {
    let loaded = catalogs();
    let session = ProceduralSession::new(
        SOURCE,
        &loaded.catalogs,
        PhysicalSessionLimits {
            max_actions: 1,
            elapsed_ms: 60_000,
            route_expansions: 50_000,
            max_total_route_expansions: 50_000,
            search_padding: 4,
        },
    )
    .unwrap();
    let metrics = session.checkpoint().best_metrics;
    let censored: storage::ObservedOutcome = PhysicalSessionOutcome {
        schema: "physical-editor-outcome-v1",
        status: "draft_accepted",
        charged: true,
        attempts: 1,
        revision: 0,
        done: false,
        work: EditWork::default(),
        best_metrics: metrics.clone(),
        current_pending: None,
        current_conflicts: None,
        current_metrics: None,
        diagnostics: vec![PhysicalEditorDiagnostic {
            code: "physical-editor.elapsed".into(),
            severity: "error",
            stage: "session",
            source: "physical-editor",
            entity: "test".into(),
            message: "deadline".into(),
        }],
    }
    .into();
    assert!(censored.censored);
    assert_eq!(censored.route_budget_exhausted, None);

    let observed: storage::ObservedOutcome = PhysicalSessionOutcome {
        schema: "physical-editor-outcome-v1",
        status: "validated",
        charged: true,
        attempts: 1,
        revision: 1,
        done: true,
        work: EditWork {
            expanded: 50_000,
            committed: true,
            ..EditWork::default()
        },
        best_metrics: metrics.clone(),
        current_pending: Some(0),
        current_conflicts: Some(0),
        current_metrics: Some(metrics),
        diagnostics: vec![],
    }
    .into();
    assert!(!observed.censored);
    assert_eq!(observed.route_budget_exhausted, Some(true));
    assert!(observed.current_metrics.is_some());
}

#[test]
fn output_ceiling_refuses_data_that_would_consume_metadata_reserve() {
    let base = std::env::temp_dir().join(format!("aic-outcomes-cap-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir(&base).unwrap();
    let output = base.join("output");
    let mut stage = storage::Stage::new(&output, 1024, 768).unwrap();
    assert!(stage.write_source("fits", &[0; 256]).unwrap().is_some());
    assert!(stage.write_source("over", &[0]).unwrap().is_none());
    assert_eq!(
        fs::metadata(stage.root().join("sources/fits.json"))
            .unwrap()
            .len(),
        256
    );
    assert!(!stage.root().join("sources/over.json").exists());
    drop(stage);
    fs::remove_dir_all(base).unwrap();
}
