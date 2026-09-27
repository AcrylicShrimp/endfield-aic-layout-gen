use super::*;
use crate::full_factory::tests::{fixture, fixture_request};
use crate::full_factory::types::{Bounds, Point};
use indexmap::{IndexMap, IndexSet};

#[test]
fn spatial_admission_and_selected_body_exclusion() {
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: 2,
        max_y: 2,
    };
    let body =
        spatial::Prefix::try_new(bounds, [Point { x: 0, y: 0 }, Point { x: 1, y: 1 }]).unwrap();
    assert!(body.occupied(0, 0, 1, 1));
    assert!(!body.occupied_except(
        0,
        0,
        1,
        1,
        [spatial::Rectangle {
            x: 0,
            y: 0,
            width: 1,
            height: 1
        }]
    ));
    assert!(body.occupied_except(
        0,
        0,
        2,
        2,
        [spatial::Rectangle {
            x: 0,
            y: 0,
            width: 1,
            height: 1
        }]
    ));
    let sparse = spatial::Prefix::try_new(
        Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 1_100_000,
            max_y: 0,
        },
        [Point { x: 0, y: 0 }, Point { x: 1_100_000, y: 0 }],
    )
    .unwrap();
    assert!(sparse.occupied(1_100_000, 0, 1, 1));
    assert!(!sparse.occupied(500_000, 0, 1, 1));
    assert_eq!(
        spatial::Prefix::try_new(
            Bounds {
                min_x: 0,
                min_y: 0,
                max_x: i64::MAX,
                max_y: 0
            },
            []
        )
        .err()
        .unwrap()
        .code,
        "factory.vocabulary-resource-incomplete"
    );
    assert_eq!(
        spatial::canvas(Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 4_000_000_000,
            max_y: 4_000_000_000
        })
        .err()
        .unwrap()
        .code,
        "factory.vocabulary-resource-incomplete"
    );
    let (w, _, _) = fixture();
    assert_eq!(
        spatial::empty_rectangles(&w, crate::full_factory::types::extent(&w).unwrap(), 1, 1024)
            .err()
            .unwrap()
            .code,
        "factory.vocabulary-resource-incomplete"
    );
    assert_eq!(
        spatial::empty_rectangles(
            &w,
            crate::full_factory::types::extent(&w).unwrap(),
            100_000,
            1
        )
        .err()
        .unwrap()
        .code,
        "factory.vocabulary-resource-incomplete"
    );
}

#[test]
fn compound_global_work_stop_is_incomplete() {
    let (w, catalogs, _) = fixture();
    let decomposition = chains::extract(&w, &IndexSet::new()).unwrap();
    let index = movement::MovementIndex::new(&w, &decomposition.chains);
    let opts = poses::VocabularyOptions {
        compound_preflights: 1,
        ..Default::default()
    };
    let error = poses::actions(&w, &catalogs, &opts, &decomposition, &index)
        .err()
        .expect("unfinished compound streams must not be published as complete");
    assert_eq!(error.code, "factory.vocabulary-resource-incomplete");
    assert!(error.entity["pending_streams"].as_u64().unwrap() > 0);
}

#[test]
fn empty_compound_domain_skips_rectangle_scan() {
    let (mut w, catalogs, _) = fixture();
    w.placements.truncate(2);
    w.placements[1].x = 100_000_001;
    w.transport_networks.clear();
    w.logistics_components.clear();
    assert!(groups::compound_groups(&w).is_empty());
    let decomposition = chains::extract(&w, &IndexSet::new()).unwrap();
    let index = movement::MovementIndex::new(&w, &decomposition.chains);
    let opts = poses::VocabularyOptions::default();
    let (_, report) = poses::actions(&w, &catalogs, &opts, &decomposition, &index).unwrap();
    assert_eq!(report["compound_groups"], 0);
    assert_eq!(report["compound_streams"], 0);
}

#[test]
#[ignore = "set AIC_POSE_VOCABULARY_WITNESS to a measured witness"]
fn measure_pose_vocabulary_witness() {
    let path = std::env::var("AIC_POSE_VOCABULARY_WITNESS").unwrap();
    let w: crate::full_factory::types::Witness =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (_, catalogs, _) = fixture();
    let (_, acceptance_catalogs, _) = fixture();
    let request = fixture_request(&w);
    let accepted = FactoryEditSession::new(
        request,
        w.clone(),
        Arc::new(acceptance_catalogs),
        FactoryEditBudget {
            proposals: 1,
            elapsed_ms: 30_000,
        },
    );
    println!("factory_edit_accepted={}", accepted.is_ok());
    if let Err(e) = accepted {
        panic!("factory edit rejected: {e:?}");
    }
    let before = std::time::Instant::now();
    let mut opts = poses::VocabularyOptions::default();
    if let Ok(limit) = std::env::var("AIC_POSE_COMPOUND_PREFLIGHTS") {
        opts.compound_preflights = limit.parse().unwrap();
    }
    if std::env::var_os("AIC_POSE_REPORT").is_some() {
        let decomposition = chains::extract(&w, &IndexSet::new()).unwrap();
        let index = movement::MovementIndex::new(&w, &decomposition.chains);
        match poses::actions(&w, &catalogs, &opts, &decomposition, &index) {
            Ok((_, report)) => println!("pose_report={report}"),
            Err(e) => panic!("pose_error={} {}", e.code, e.message),
        }
    }
    let result = candidates::describe(&w, &catalogs, &opts);
    println!("describe_ms={:.3}", before.elapsed().as_secs_f64() * 1000.0);
    match result {
        Ok(domain) => {
            let encoded = serde_json::to_vec(&domain.actions).unwrap();
            if let Some(path) = std::env::var_os("AIC_POSE_DUMP_ACTIONS") {
                std::fs::write(path, &encoded).unwrap();
            }
            println!(
                "actions={} sha256={}",
                domain.actions.len(),
                crate::input::hash(&encoded)
            );
            let groups = groups::pose_groups(&w, true);
            let compound = groups::compound_groups(&w);
            println!(
                "pose_groups={} max_members={} omitted={} compound_groups={}",
                groups.groups.len(),
                groups
                    .groups
                    .iter()
                    .map(|g| g.members.len())
                    .max()
                    .unwrap_or(0),
                groups.omitted.len(),
                compound.len()
            );
            if std::env::var_os("AIC_POSE_EXECUTE_FIRST").is_some() {
                let action = domain
                    .actions
                    .iter()
                    .find(|a| matches!(a, candidates::Action::Pose(_)))
                    .unwrap();
                let outcome =
                    candidates::execute(&w, &catalogs, action, 5_000, &routing::Limits::default());
                match outcome {
                    Ok(outcome) => {
                        println!(
                            "route_status={} route_expansions={} candidate={}",
                            outcome.status,
                            outcome.route_expansions,
                            outcome.witness.is_some()
                        );
                        assert_eq!(outcome.status, "candidate");
                        assert!(outcome.witness.is_some());
                    }
                    Err(e) => panic!("route_error={} {}", e.code, e.message),
                }
            }
        }
        Err(e) => panic!("describe_error={} {}", e.code, e.message),
    }
}

#[test]
#[ignore = "set AIC_POSE_VOCABULARY_WITNESS to a structural group stress witness"]
fn measure_pose_group_stress() {
    let path = std::env::var("AIC_POSE_VOCABULARY_WITNESS").unwrap();
    let w: crate::full_factory::types::Witness =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    if std::env::var_os("AIC_POSE_VALIDATE_FACTORY").is_some() {
        let (_, catalogs, _) = fixture();
        let accepted = FactoryEditSession::new(
            fixture_request(&w),
            w.clone(),
            Arc::new(catalogs),
            FactoryEditBudget {
                proposals: 1,
                elapsed_ms: 30_000,
            },
        );
        println!("factory_edit_accepted={}", accepted.is_ok());
        if let Err(e) = accepted {
            panic!("factory edit rejected: {e:?}");
        }
        if w.placements.len() >= 17 {
            let proposed: Vec<_> = (0..17)
                .map(|i| {
                    let member = i * (w.placements.len() / 17);
                    let p = &w.placements[member];
                    poses::Pose {
                        member,
                        x: p.x + 1,
                        y: p.y,
                        rotation: p.rotation,
                    }
                })
                .collect();
            let (moves, _, changes) = poses::options(&w, &fixture().1, &proposed).unwrap();
            let decomposition = chains::extract(&w, &IndexSet::new()).unwrap();
            let index = movement::MovementIndex::new(&w, &decomposition.chains);
            let selected = movement::plan(
                &w,
                &decomposition.chains,
                &decomposition.anchors,
                &moves,
                &IndexMap::new(),
                &changes,
                &IndexSet::new(),
                &index,
                None,
            );
            println!("seventeen_member_movement={}", selected.is_ok());
            if let Err(e) = selected {
                panic!("17-member movement rejected: {e:?}");
            }
        }
    }
    let groups = groups::pose_groups(&w, true);
    let compound = groups::compound_groups(&w);
    println!(
        "facilities={} pose_groups={} max_members={} omitted={} compound_groups={}",
        w.placements.len(),
        groups.groups.len(),
        groups
            .groups
            .iter()
            .map(|g| g.members.len())
            .max()
            .unwrap_or(0),
        groups.omitted.len(),
        compound.len()
    );
}
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
