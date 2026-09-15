use super::*;
use crate::full_factory::tests::{fixture, fixture_request};
#[test]
fn seeded_setup_remains_valid_and_obeys_external_bounds() {
    let (w, catalogs, _) = fixture();
    let request = fixture_request(&w);
    let catalogs = Arc::new(crate::input::Catalogs::new(catalogs));
    let setup = Setup {
        identity_limits: physical_identity::Limits {
            max_individualization_nodes: 100_000,
        },
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
    assert_eq!(first.physical, second.physical);
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
fn cached_actual_slot_checks_source_identity_hash_and_reserved_geometry() {
    use source::{Generated, Source};
    use std::{fs, path::PathBuf};
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let configuration: Value = serde_json::from_slice(
        &fs::read(root.join("tools/native-training/training.json")).unwrap(),
    )
    .unwrap();
    let (w, catalogs, _) = fixture();
    let mut input = Generated {
        request: fixture_request(&w),
        original: w,
        metadata: json!({}),
        admission: None,
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
    input
        .admit(
            &catalogs,
            &physical_identity::Limits {
                max_individualization_nodes: 100_000,
            },
            "cached-fixture",
            Some("source/3".into()),
        )
        .unwrap();
    let source = make(Default::default());
    let directory =
        std::env::temp_dir().join(format!("aic-generated-slot-test-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let bytes = serde_json::to_vec(&input).unwrap();
    fs::write(directory.join("input.json"), &bytes).unwrap();
    fs::write(directory.join("manifest.json"),serde_json::to_vec(&json!({"schema":"aic-generated-slot-v2","slot":3,"source":source.identity,"input_sha256":crate::input::hash(&bytes)})).unwrap()).unwrap();
    assert!(source.generate(3, &directory).is_ok());
    assert!(
        source
            .generate(4, &directory)
            .err()
            .unwrap()
            .contains("slot-binding")
    );
    let reserved = make(
        [input.admitted().unwrap().physical.structural.clone()]
            .into_iter()
            .collect(),
    );
    assert!(
        reserved
            .generate(3, &directory)
            .err()
            .unwrap()
            .contains("reservation")
    );
    // Equivalent translation still fails the cached boundary, despite a different raw digest.
    let mut translated = serde_json::to_value(&input).unwrap();
    fn shift(value: &mut Value) {
        match value {
            Value::Object(o) => {
                if let (Some(x), Some(y)) = (
                    o.get("x").and_then(Value::as_i64),
                    o.get("y").and_then(Value::as_i64),
                ) {
                    o.insert("x".into(), json!(x + 129));
                    o.insert("y".into(), json!(y - 123));
                }
                for v in o.values_mut() {
                    shift(v);
                }
            }
            Value::Array(a) => {
                for v in a {
                    shift(v)
                }
            }
            _ => {}
        }
    }
    shift(&mut translated["original"]);
    let write_slot = |value: &Value| {
        let bytes = serde_json::to_vec(value).unwrap();
        fs::write(directory.join("input.json"), &bytes).unwrap();
        fs::write(directory.join("manifest.json"),serde_json::to_vec(&json!({"schema":"aic-generated-slot-v2","slot":3,"source":source.identity,"input_sha256":crate::input::hash(&bytes)})).unwrap()).unwrap();
    };
    write_slot(&translated);
    assert!(source.generate(3, &directory).is_ok());
    assert!(
        reserved
            .generate(3, &directory)
            .err()
            .unwrap()
            .contains("reservation")
    );
    // A valid current shape cannot release a separately reserved ancestor, even with a new episode label.
    translated["admission"]["root"]["structural"] = json!("f".repeat(64));
    translated["admission"]["root"]["episode"] = json!("renumbered");
    write_slot(&translated);
    assert!(
        make(["f".repeat(64)].into_iter().collect())
            .generate(3, &directory)
            .err()
            .unwrap()
            .contains("reservation")
    );
    translated["admission"]["root"]["structural"] = json!("F".repeat(64));
    write_slot(&translated);
    assert!(
        source
            .generate(3, &directory)
            .err()
            .unwrap()
            .contains("invalid admission")
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
    let reserved = source::Source::load(
        &root,
        catalogs.clone(),
        serde_json::from_value(config["generation"].clone()).unwrap(),
        [actual.admitted().unwrap().physical.structural.clone()]
            .into_iter()
            .collect(),
    )
    .unwrap();
    let fresh = reserved
        .generate(0, &directory.join("reserved-fresh"))
        .unwrap();
    assert_ne!(
        fresh.admitted().unwrap().physical.structural,
        actual.admitted().unwrap().physical.structural
    );
    assert!(
        fresh.metadata["attempts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["stage"] == "reservation")
    );
    let mut limited = config["generation"].clone();
    limited["identity_limits"]["max_individualization_nodes"] = json!(1);
    let limited = source::Source::load(
        &root,
        catalogs.clone(),
        serde_json::from_value(limited).unwrap(),
        Default::default(),
    )
    .unwrap();
    assert!(
        limited
            .generate(0, &directory.join("identity-budget"))
            .err()
            .unwrap()
            .contains("physical-identity-budget")
    );
    assert!(
        directory
            .join("identity-budget/setup-partial.json")
            .is_file()
    );
    assert!(!directory.join("identity-budget/manifest.json").exists());
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
