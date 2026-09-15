use super::*;
use crate::full_factory::tests::{fixture, fixture_request};
#[test]
fn seeded_setup_remains_valid_and_obeys_external_bounds() {
    let (w, catalogs, _) = fixture();
    let request = fixture_request(&w);
    let catalogs = Arc::new(crate::input::Catalogs::new(catalogs));
    let setup = Setup {
        steps: 3,
        elapsed_ms: 10_000,
        route_work: 50_000,
        kinds: vec![
            "pose".into(),
            "contract".into(),
            "insert".into(),
            "port".into(),
            "reroute".into(),
            "component".into(),
            "release_bridge".into(),
        ],
    };
    let first = randomize(
        request.clone(),
        w.clone(),
        catalogs.clone(),
        &setup,
        91,
        &|| Ok(()),
    )
    .unwrap_or_else(|e| panic!("{}", e.message));
    let second = randomize(request.clone(), w, catalogs.clone(), &setup, 91, &|| Ok(()))
        .unwrap_or_else(|e| panic!("{}", e.message));
    assert_eq!(first.geometry_sha256, second.geometry_sha256);
    FactoryEditSession::new(
        request,
        first.original,
        catalogs.data.clone(),
        FactoryEditBudget {
            proposals: 3,
            elapsed_ms: 10_000,
        },
    )
    .unwrap();
    assert!(first.records.len() <= 3);
    let (w, catalogs, _) = fixture();
    let invalid = Setup { steps: 0, ..setup };
    assert!(
        randomize(
            fixture_request(&w),
            w,
            Arc::new(crate::input::Catalogs::new(catalogs)),
            &invalid,
            91,
            &|| Ok(())
        )
        .is_err()
    );
}
#[test]
fn geometry_reservation_ignores_rate_and_input_array_order() {
    let (mut witness, _, _) = fixture();
    let identity = geometry_identity(&witness);
    witness.placements.reverse();
    witness.transport_networks.reverse();
    witness.utilizations.clear();
    assert_eq!(geometry_identity(&witness), identity);
    witness.placements[0].x += 1;
    assert_ne!(geometry_identity(&witness), identity);
}

#[test]
fn cached_actual_slot_checks_source_identity_hash_and_reserved_geometry() {
    use source::{Generated, Source};
    use std::{fs, path::PathBuf};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let configuration: Value = serde_json::from_slice(
        &fs::read(root.join("tools/native-training/training.json")).unwrap(),
    )
    .unwrap();
    let (w, catalogs, _) = fixture();
    let input = Generated {
        request: fixture_request(&w),
        original: w,
        metadata: json!({}),
    };
    let catalogs = Arc::new(crate::input::Catalogs::new(catalogs));
    let make = |reserved| {
        Source::load(
            &root,
            catalogs.clone(),
            serde_json::from_value(configuration["generation"].clone()).unwrap(),
            reserved,
        )
        .unwrap()
    };
    let source = make(Default::default());
    let directory =
        std::env::temp_dir().join(format!("aic-generated-slot-test-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let bytes = serde_json::to_vec(&input).unwrap();
    fs::write(directory.join("input.json"), &bytes).unwrap();
    fs::write(directory.join("manifest.json"),serde_json::to_vec(&json!({"schema":"aic-generated-slot-v1","slot":3,"source":source.identity,"input_sha256":crate::input::hash(&bytes)})).unwrap()).unwrap();
    assert!(source.generate(3, &directory).is_ok());
    assert!(
        source
            .generate(4, &directory)
            .err()
            .unwrap()
            .contains("slot-binding")
    );
    let reserved = make([geometry_identity(&input.original)].into_iter().collect());
    assert!(
        reserved
            .generate(3, &directory)
            .err()
            .unwrap()
            .contains("reservation")
    );
    fs::write(directory.join("input.json"), b"{}").unwrap();
    assert!(
        source
            .generate(3, &directory)
            .err()
            .unwrap()
            .contains("slot-binding")
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn direct_generation_commits_actual_input_and_guard_failure_never_commits() {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config: Value = crate::input::read(
        &root,
        std::path::Path::new("tools/native-training/training.json"),
    )
    .unwrap();
    let paths = serde_json::from_value(config["catalogs"].clone()).unwrap();
    let catalogs = crate::input::load_catalogs(&root, &paths).unwrap();
    let make = || {
        source::Source::load(
            &root,
            catalogs.clone(),
            serde_json::from_value(config["generation"].clone()).unwrap(),
            Default::default(),
        )
        .unwrap()
    };
    let directory =
        std::env::temp_dir().join(format!("aic-direct-generation-test-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let actual = make().generate(0, &directory.join("success")).unwrap();
    let cached = make().generate(0, &directory.join("success")).unwrap();
    assert_eq!(
        serde_json::to_value(&actual).unwrap(),
        serde_json::to_value(&cached).unwrap()
    );
    let calls = Arc::new(AtomicUsize::new(0));
    let count = calls.clone();
    let cancelled = make().with_guard(Arc::new(move || {
        if count.fetch_add(1, Ordering::Relaxed) >= 4 {
            Err("test-generation-cancelled".into())
        } else {
            Ok(())
        }
    }));
    assert!(
        cancelled
            .generate(1, &directory.join("cancelled"))
            .err()
            .unwrap()
            .contains("test-generation-cancelled")
    );
    assert!(!directory.join("cancelled/manifest.json").exists());
    assert!(
        make()
            .generate(1, &directory.join("cancelled"))
            .err()
            .unwrap()
            .contains("incomplete")
    );
    std::fs::remove_dir_all(directory).unwrap();
}
