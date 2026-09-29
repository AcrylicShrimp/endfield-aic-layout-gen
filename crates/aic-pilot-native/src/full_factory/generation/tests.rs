use super::*;
use crate::full_factory::tests::{fixture, fixture_request};
use crate::full_factory::types::{Bounds, Point};
use indexmap::{IndexMap, IndexSet};

#[test]
fn external_route_stop_reaches_source_failure_counts() {
    use aic_data::recipes::{
        FACILITY_INSTANCE_WIRING_SCHEMA_VERSION, FacilityInstanceWiringEdge,
        FacilityInstanceWiringNode, FacilityInstanceWiringReport, Rate,
    };

    let (_, catalogs, _) = fixture();
    let item = "item-activity-xiranite-bottle";
    let rate = Rate {
        numerator: 1,
        denominator: 1,
    };
    let wiring = FacilityInstanceWiringReport {
        schema_version: FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
        success: true,
        nodes: vec![
            FacilityInstanceWiringNode::Facility {
                id: "a".into(),
                recipe: "fixture-output".into(),
                facility: "filling-powder-mc-1-mode-normal".into(),
                index: 0,
                runs_per_second: rate,
                work_seconds_per_second: rate,
                unused_capacity: Rate::zero(),
            },
            FacilityInstanceWiringNode::Target {
                id: "sale".into(),
                item: item.into(),
            },
        ],
        edges: vec![FacilityInstanceWiringEdge::original(
            "a",
            "sale",
            "recipe-flow",
            item,
            rate,
        )],
        diagnostics: Vec::new(),
    };
    for (step_limit, connection_limit, code) in [
        (1, 1, "production-external-step-route-budget-exhausted"),
        (
            64,
            1,
            "production-external-connection-route-budget-exhausted",
        ),
    ] {
        let request = aic_data::layouts::ConstructiveProductionRequest {
            schema_version: aic_data::layouts::CONSTRUCTIVE_PRODUCTION_SCHEMA_VERSION,
            target_instance: "a".into(),
            max_steps: 64,
            max_placement_attempts: 128,
            max_route_searches: step_limit,
            per_connection_route_searches: connection_limit,
            analysis_workers: 1,
            nearby_candidates: 0,
            downstream_closed_growth: false,
            flow_direction: None,
            connection_first_placement: false,
        };
        let result = aic_data::layouts::construct_production_compact(
            &wiring,
            &catalogs.facilities,
            &catalogs.items,
            &catalogs.transports,
            &catalogs.components,
            &request,
        );
        assert!(!result.complete);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == code)
        );
        let summary = source::construction_failure_summary(&result);
        assert_eq!(summary["search_stop_counts"][code], 1);
        assert_eq!(summary["top_rejected_reasons"][code], 1);
        assert_eq!(summary["failed_statistics"]["astar_searches"], 1);
    }
}

#[test]
#[ignore = "set AIC_SOURCE_CONFIG, AIC_SOURCE_SLOT and AIC_SOURCE_OUTPUT"]
fn measure_source_generated_input_band() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = root.join(std::env::var("AIC_SOURCE_OUTPUT").unwrap());
    let config_path = std::path::PathBuf::from(std::env::var("AIC_SOURCE_CONFIG").unwrap());
    let slot: u64 = std::env::var("AIC_SOURCE_SLOT").unwrap().parse().unwrap();
    let config: Value = crate::input::read(
        &root,
        std::path::Path::new("tools/native-training/training.json"),
    )
    .unwrap();
    let paths = serde_json::from_value(config["catalogs"].clone()).unwrap();
    let catalogs = crate::input::load_catalogs(&root, &paths).unwrap();
    let generation: source::Config = crate::input::read(&root, &config_path).unwrap();
    let seed = generation
        .seed_base
        .checked_add(slot.checked_mul(generation.max_attempts as u64).unwrap())
        .unwrap();
    let mut rng = rng::Rng::new(seed);
    let product = &generation.products[rng.index(generation.products.len())];
    let product_id = product.id.clone();
    let [low, high] = product.duration_ms;
    let count = (high - low) / generation.duration_step_ms + 1;
    let duration = low + rng.index(count as usize) as u64 * generation.duration_step_ms;
    let mut plan: Value = crate::input::read(&root, &product.source).unwrap();
    plan["target"]["quantity"] = json!(1);
    plan["target"]["duration_ms"] = json!(duration);
    let typed: aic_data::recipes::RecipeSourcePlanRequest = serde_json::from_value(plan).unwrap();
    let throughput = catalogs
        .data
        .recipes
        .calculate_contextual_throughput(&typed);
    let facilities = aic_data::recipes::calculate_contextual_facility_requirements(&throughput);
    let wiring =
        aic_data::recipes::build_contextual_facility_instance_wiring(&throughput, &facilities);
    let wiring_value = serde_json::to_value(&wiring).unwrap();
    let instances = wiring_value["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|node| node["kind"] == "facility")
        .count();
    let source =
        source::Source::load(&root, catalogs.clone(), generation, Default::default()).unwrap();
    println!(
        "source_wiring={}",
        json!({"product":product_id,"seed":seed,"slot":slot,"duration_ms":duration,
        "instances":instances,"success":wiring.success,"source":source.identity})
    );
    assert!(wiring.success, "preflight source wiring must succeed");
    if let Ok(expected) = std::env::var("AIC_SOURCE_EXPECT_FACILITIES") {
        assert_eq!(instances, expected.parse::<usize>().unwrap());
    }
    let compact_constructor = std::env::var_os("AIC_SOURCE_COMPACT_CONSTRUCTOR_ONLY").is_some();
    let measure_payload = std::env::var_os("AIC_SOURCE_CONSTRUCTOR_PAYLOAD_ONLY").is_some();
    if compact_constructor || std::env::var_os("AIC_SOURCE_CONSTRUCTOR_ONLY").is_some() {
        let targets = wiring_value["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|node| node["kind"] == "target")
            .filter_map(|node| node["id"].as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let target = wiring
            .edges
            .iter()
            .find(|edge| targets.contains(edge.target.as_str()))
            .unwrap()
            .source
            .clone();
        let c = &source.config.construction;
        let request = aic_data::layouts::ConstructiveProductionRequest {
            schema_version: aic_data::layouts::CONSTRUCTIVE_PRODUCTION_SCHEMA_VERSION,
            target_instance: target,
            max_steps: c.max_steps,
            max_placement_attempts: c.max_placement_attempts,
            max_route_searches: c.max_route_searches,
            per_connection_route_searches: c.per_connection_route_searches,
            analysis_workers: c.analysis_workers,
            nearby_candidates: c.nearby_candidates,
            downstream_closed_growth: c.downstream_closed_growth,
            flow_direction: c.flow_direction,
            connection_first_placement: c.connection_first_placement,
        };
        let started = std::time::Instant::now();
        if compact_constructor {
            let result = aic_data::layouts::construct_production_compact(
                &wiring,
                &catalogs.data.facilities,
                &catalogs.data.items,
                &catalogs.data.transports,
                &catalogs.data.components,
                &request,
            );
            let elapsed_s = started.elapsed().as_secs_f64();
            let final_sha256 = result
                .final_node
                .as_ref()
                .map(|node| crate::input::hash(&serde_json::to_vec(node).unwrap()));
            let payload_bytes = measure_payload.then(|| {
                serde_json::to_vec(&json!({
                    "success": result.success,
                    "complete": result.complete,
                    "status": &result.status,
                    "required_facilities": result.required_facilities,
                    "accepted_growth_steps": result.accepted_growth_steps,
                    "accepted_steps": result.accepted_steps,
                    "final_node": &result.final_node,
                    "failed_placement_attempts": result.failed_placement_attempts,
                    "failed_statistics": &result.failed_statistics,
                    "rejected_reasons": &result.rejected_reasons,
                    "diagnostics": &result.diagnostics,
                    "elapsed_ms": result.elapsed_ms,
                    "request": &result.request,
                    "per_connection_route_limit": result.per_connection_route_limit,
                    "routing_workspace_cell_limit": result.routing_workspace_cell_limit,
                }))
                .unwrap()
                .len()
            });
            println!(
                "source_constructor_profile={}",
                json!({"product":product_id,"seed":seed,"slot":slot,
                    "mode":"compact","complete":result.complete,"status":result.status,
                    "facilities":result.final_node.as_ref().map(|node|node.placements.len()),
                    "accepted_steps":result.accepted_steps,"final_sha256":final_sha256,
                    "elapsed_s":elapsed_s,"payload_bytes":payload_bytes})
            );
            assert!(
                result.complete,
                "{:?} {:?}",
                result.diagnostics, result.rejected_reasons
            );
            return;
        }
        let report = aic_data::layouts::construct_production(
            &wiring,
            &catalogs.data.facilities,
            &catalogs.data.items,
            &catalogs.data.transports,
            &catalogs.data.components,
            &request,
        );
        let elapsed_s = started.elapsed().as_secs_f64();
        let final_sha256 = report
            .final_node
            .as_ref()
            .map(|node| crate::input::hash(&serde_json::to_vec(node).unwrap()));
        let payload_bytes = measure_payload.then(|| serde_json::to_vec(&report).unwrap().len());
        let history_node_bytes = measure_payload.then(|| {
            report
                .steps
                .iter()
                .map(|step| serde_json::to_vec(&step.node).unwrap().len())
                .sum::<usize>()
        });
        println!(
            "source_constructor_profile={}",
            json!({"product":product_id,"seed":seed,"slot":slot,
                "mode":"full","complete":report.complete,"status":report.status,
                "facilities":report.final_node.as_ref().map(|node|node.placements.len()),
                "accepted_steps":report.steps.len(),"final_sha256":final_sha256,
                "astar":report.steps.iter().map(|step|step.statistics.astar_searches).sum::<u64>()
                    + report.failed_statistics.astar_searches,
                "placements":report.steps.iter().map(|step|step.placement_attempts).sum::<usize>()
                    + report.failed_placement_attempts,
                "elapsed_s":elapsed_s,"payload_bytes":payload_bytes,
                "history_node_bytes":history_node_bytes})
        );
        assert!(
            report.complete,
            "{:?} {:?}",
            report.diagnostics, report.rejected_reasons
        );
        return;
    }
    if std::env::var_os("AIC_SOURCE_WIRING_ONLY").is_some() {
        return;
    }
    let started = std::time::Instant::now();
    let actual = match source.generate(slot, &output) {
        Ok(value) => value,
        Err(failure) => {
            let input = output.join("input.json");
            let diagnostic: Value = serde_json::from_str(&failure).unwrap_or(Value::Null);
            let size_refusal = diagnostic["code"] == "aic-generation.input"
                && diagnostic["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("-byte input limit"))
                && std::fs::metadata(&input).is_ok_and(|metadata| {
                    metadata.len() > crate::input::MAX_GENERATED_INPUT_BYTES as u64
                });
            if !size_refusal || !input.exists() || output.join("manifest.json").exists() {
                panic!(
                    "source generation failed after {:.3}s: {failure}",
                    started.elapsed().as_secs_f64()
                );
            }
            let file = std::fs::File::open(&input).unwrap();
            let value: source::Generated = serde_json::from_reader(file).unwrap();
            aic_data::layouts::FactoryEditSession::new(
                value.request.clone(),
                value.original.clone(),
                catalogs.data.clone(),
                aic_data::layouts::FactoryEditBudget {
                    proposals: 1,
                    elapsed_ms: 600_000,
                },
            )
            .unwrap();
            println!(
                "source_upper_handoff={}",
                json!({"status":"input-size-refusal","source":source.identity,
                    "slot":slot,"seed":seed,"product":product_id,
                    "facilities":value.original.placements.len(),
                    "input_bytes":std::fs::metadata(input).unwrap().len(),
                    "elapsed_s":started.elapsed().as_secs_f64()})
            );
            return;
        }
    };
    let cached = source.generate(slot, &output).unwrap();
    assert_eq!(
        serde_json::to_value(&cached).unwrap(),
        serde_json::to_value(&actual).unwrap()
    );
    let input = output.join("input.json");
    let native: source::Generated =
        crate::input::read(&output, std::path::Path::new("input.json")).unwrap();
    assert_eq!(
        serde_json::to_value(&native).unwrap(),
        serde_json::to_value(&actual).unwrap()
    );
    let networks = actual.original.transport_networks.len();
    let cells = actual
        .original
        .transport_networks
        .iter()
        .map(|n| n.cells.len())
        .sum::<usize>();
    println!(
        "source_upper_handoff={}",
        json!({"status":"committed","product":product_id,"seed":seed,"slot":slot,"duration_ms":duration,
        "facilities":actual.original.placements.len(),"networks":networks,"cells":cells,
        "input_bytes":std::fs::metadata(input).unwrap().len(),
        "elapsed_s":started.elapsed().as_secs_f64()})
    );
}

#[test]
fn stopped_source_construction_retains_compact_attempt_evidence() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let training: Value = crate::input::read(
        &root,
        std::path::Path::new("tools/native-training/training.json"),
    )
    .unwrap();
    let paths = serde_json::from_value(training["catalogs"].clone()).unwrap();
    let catalogs = crate::input::load_catalogs(&root, &paths).unwrap();
    let mut config = training["generation"].clone();
    let copper = training["generation"]["products"]
        .as_array()
        .unwrap()
        .iter()
        .find(|product| product["id"] == "copper-liquid")
        .unwrap();
    config["products"] = json!([copper]);
    config["products"][0]["duration_ms"] = json!([20000, 20000]);
    config["max_attempts"] = json!(1);
    config["episode_setup_ms"] = json!(60000);
    config["construction"]["elapsed_ms"] = json!(10000);
    config["construction"]["max_steps"] = json!(1);
    config["construction"]["max_placement_attempts"] = json!(1);
    config["construction"]["max_route_searches"] = json!(1);
    let source = source::Source::load(
        &root,
        catalogs,
        serde_json::from_value(config).unwrap(),
        Default::default(),
    )
    .unwrap();
    let directory = std::env::temp_dir().join(format!(
        "aic-construction-stop-summary-{}",
        std::process::id()
    ));
    let outcome = source.generate(0, &directory);
    assert!(outcome.err().unwrap().contains("aic-generation.exhausted"));
    let failure: Value =
        crate::input::read(&directory, std::path::Path::new("failure.json")).unwrap();
    let attempt = &failure["attempts"][0];
    assert_eq!(attempt["stage"], "construction");
    assert_eq!(attempt["product"], "copper-liquid");
    assert_eq!(attempt["source"], source.identity);
    assert_eq!(attempt["outcome"]["per_connection_route_limit"], 64);
    assert!(attempt["outcome"]["failed_statistics"].is_object());
    assert!(attempt["outcome"]["accepted_closed_facilities"].is_number());
    assert!(!directory.join("manifest.json").exists());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn route_work_stop_keeps_incomplete_region_evidence() {
    let (w, catalogs, _) = fixture();
    let decomposition = chains::extract(&w, &IndexSet::new()).unwrap();
    let chain = decomposition
        .chains
        .iter()
        .find(|c| c.cells.len() > 4)
        .unwrap();
    let edit = routing::Edit {
        chain_ids: vec![chain.id],
        ..Default::default()
    };
    let limits = routing::Limits {
        attempts: 1,
        attempt_work: 1,
        ..Default::default()
    };
    let outcome = routing::reembed(&w, &catalogs, &edit, 1, &limits).unwrap();
    assert_eq!(outcome.status, "search-incomplete");
    assert!(outcome.witness.is_none());
    let report = outcome.region.unwrap();
    assert_eq!(report.contract, "factory-route-region-v3");
    assert_eq!(report.schema_version, 3);
    assert_eq!(report.termination, "search-incomplete");
    assert_eq!(report.route_expansions, outcome.route_expansions);
    assert!(report.route_expansions <= 1);
    assert!(report.incomplete.is_some());
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(|d| d.code == "region-search-incomplete")
    );
}

#[test]
fn route_input_work_limits_admit_large_finite_requests() {
    let mut limits = routing::Limits::default();
    limits.attempt_work = 1_000_001;
    limits.chains = 129;
    limits.validate().unwrap();
    let (w, catalogs, _) = fixture();
    let domain = candidates::describe(&w, &catalogs, &poses::VocabularyOptions::default()).unwrap();
    let reroute = domain
        .actions
        .iter()
        .find(|action| matches!(action, candidates::Action::Reroute { .. }))
        .unwrap();
    let result = candidates::execute(&w, &catalogs, reroute, 200_001, &limits).unwrap();
    assert_eq!(result.region.unwrap().limits.attempt_work, 1_000_001);
}

#[test]
#[ignore = "set AIC_REGION_WITNESS to a measured full-factory witness"]
fn measure_region_chain_capacity() {
    let path = std::env::var("AIC_REGION_WITNESS").unwrap();
    let w: crate::full_factory::types::Witness =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (_, catalogs, _) = fixture();
    let (_, acceptance_catalogs, _) = fixture();
    let accepted = FactoryEditSession::new(
        fixture_request(&w),
        w.clone(),
        Arc::new(acceptance_catalogs),
        FactoryEditBudget {
            proposals: 1,
            elapsed_ms: 30_000,
        },
    );
    if let Err(e) = accepted {
        panic!("factory admission: {e:?}");
    }
    let decomposition = chains::extract(&w, &IndexSet::new()).unwrap();
    let selected = decomposition.chains.len().min(129);
    println!(
        "facilities={} chains={} selected={selected}",
        w.placements.len(),
        decomposition.chains.len()
    );
    if selected <= 128 {
        return;
    }
    let edit = routing::Edit {
        chain_ids: decomposition.chains[..selected]
            .iter()
            .map(|chain| chain.id)
            .collect(),
        ..Default::default()
    };
    for limit in [64, 128, 256] {
        let limits = routing::Limits {
            attempts: 1,
            attempt_work: 1,
            chains: limit,
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let result = routing::reembed(&w, &catalogs, &edit, 1, &limits).unwrap();
        let report = result.region.as_ref().unwrap();
        println!(
            "chain_limit={limit} status={} expansions={} ms={:.3} incomplete={}",
            result.status,
            result.route_expansions,
            started.elapsed().as_secs_f64() * 1000.0,
            serde_json::to_string(&report.incomplete).unwrap()
        );
        assert_eq!(result.status, "search-incomplete");
        assert!(result.witness.is_none());
        assert_eq!(report.initial_chains.len(), selected);
        assert!(result.route_expansions <= 1);
        if limit < selected {
            assert_eq!(
                report.incomplete.as_ref().unwrap().code,
                "region-chain-limit"
            );
        }
    }
    let all = routing::Edit {
        chain_ids: decomposition.chains.iter().map(|chain| chain.id).collect(),
        ..Default::default()
    };
    let limits = routing::Limits {
        attempts: 1,
        attempt_work: 1,
        chains: usize::MAX,
        ..Default::default()
    };
    let started = std::time::Instant::now();
    let result = routing::reembed(&w, &catalogs, &all, 1, &limits).unwrap();
    let report = result.region.as_ref().unwrap();
    println!(
        "chain_limit=all status={} expansions={} ms={:.3} selected={}",
        result.status,
        result.route_expansions,
        started.elapsed().as_secs_f64() * 1000.0,
        report.initial_chains.len()
    );
    assert_eq!(result.status, "search-incomplete");
    assert_eq!(report.initial_chains.len(), decomposition.chains.len());
    assert!(result.route_expansions <= 1);
}

#[test]
#[ignore = "set AIC_REGION_WITNESS to a measured full-factory witness"]
fn measure_region_route_panel() {
    let path = std::env::var("AIC_REGION_WITNESS").unwrap();
    let w: crate::full_factory::types::Witness =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (_, catalogs, _) = fixture();
    let started = std::time::Instant::now();
    let domain = candidates::describe(&w, &catalogs, &poses::VocabularyOptions::default()).unwrap();
    println!(
        "describe_ms={:.3} actions={}",
        started.elapsed().as_secs_f64() * 1000.0,
        domain.actions.len()
    );
    let reroutes: Vec<_> = domain
        .actions
        .iter()
        .filter(|a| matches!(a, candidates::Action::Reroute { .. }))
        .collect();
    let first_pose = domain
        .actions
        .iter()
        .find(|a| matches!(a, candidates::Action::Pose(_)))
        .unwrap();
    let crossing = (w.placements.len() == 5).then(|| {
        candidates::Action::Pose(poses::action(
            &w,
            vec![poses::Pose {
                member: 0,
                x: 7,
                y: 13,
                rotation: 90,
            }],
        ))
    });
    let selected = [
        ("pose", first_pose, 50_000),
        ("reroute_first", reroutes[0], 50_000),
        ("reroute_middle", reroutes[reroutes.len() / 2], 50_000),
        ("reroute_last", reroutes[reroutes.len() - 1], 50_000),
        ("reroute_middle_stop", reroutes[reroutes.len() / 2], 1),
    ];
    for (name, action, work) in selected.into_iter().chain(
        crossing
            .as_ref()
            .map(|action| ("crossing_pose", action, 50_000)),
    ) {
        let started = std::time::Instant::now();
        let result =
            candidates::execute(&w, &catalogs, action, work, &routing::Limits::default()).unwrap();
        let route_statuses = result
            .region
            .as_ref()
            .map(|region| {
                region
                    .attempts
                    .iter()
                    .flat_map(|attempt| &attempt.routes)
                    .map(|route| (route.id, route.status, route.steps))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let witness_hash = result
            .witness
            .as_ref()
            .map(|w| crate::input::hash(&serde_json::to_vec(w).unwrap()));
        let crossings = result.region.as_ref().map_or(0, |region| {
            region
                .attempts
                .iter()
                .flat_map(|attempt| &attempt.routes)
                .map(|route| route.crossings.len())
                .sum::<usize>()
        });
        println!(
            "panel={} status={} candidate={} expansions={} ms={:.3} routes={:?} crossings={} witness_hash={:?}",
            name,
            result.status,
            result.witness.is_some(),
            result.route_expansions,
            started.elapsed().as_secs_f64() * 1000.0,
            route_statuses,
            crossings,
            witness_hash
        );
    }
}

#[test]
#[ignore = "set AIC_REGION_WITNESS to a measured full-factory witness"]
fn measure_region_action_chain_counts() {
    let path = std::env::var("AIC_REGION_WITNESS").unwrap();
    let w: crate::full_factory::types::Witness =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (_, catalogs, _) = fixture();
    let domain = candidates::describe(&w, &catalogs, &poses::VocabularyOptions::default()).unwrap();
    let mut measured = Vec::new();
    let mut errors = 0usize;
    for (index, action) in domain.actions.iter().enumerate() {
        if matches!(
            action,
            candidates::Action::Insert { .. }
                | candidates::Action::Contract { .. }
                | candidates::Action::RestoreBest { .. }
        ) {
            continue;
        }
        match candidates::execute(&w, &catalogs, action, 1, &routing::Limits::default()) {
            Ok(result) => {
                if let Some(region) = result.region {
                    measured.push((region.initial_chains.len(), index, action.kind()));
                } else {
                    errors += 1;
                }
            }
            Err(_) => errors += 1,
        }
    }
    measured.sort_by(|a, b| b.cmp(a));
    println!(
        "facilities={} actions={} measured={} errors={} above64={} above128={} top={:?}",
        w.placements.len(),
        domain.actions.len(),
        measured.len(),
        errors,
        measured.iter().filter(|row| row.0 > 64).count(),
        measured.iter().filter(|row| row.0 > 128).count(),
        &measured[..measured.len().min(10)]
    );
    assert!(!measured.is_empty());
}

#[test]
#[ignore = "set AIC_REGION_WITNESS to a measured full-factory witness"]
fn measure_region_default_panel() {
    use std::collections::BTreeMap;
    let path = std::env::var("AIC_REGION_WITNESS").unwrap();
    let w: crate::full_factory::types::Witness =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let (_, catalogs, _) = fixture();
    let work: usize =
        std::env::var("AIC_REGION_WORK").map_or(50_000, |value| value.parse().unwrap());
    let mut limits = routing::Limits::default();
    if let Ok(value) = std::env::var("AIC_REGION_ATTEMPT_WORK") {
        limits.attempt_work = value.parse().unwrap();
    }
    if let Ok(value) = std::env::var("AIC_REGION_ATTEMPTS") {
        limits.attempts = value.parse().unwrap();
    }
    if let Ok(value) = std::env::var("AIC_REGION_PORT_TRIALS") {
        limits.port_trials = value.parse().unwrap();
    }
    if let Ok(value) = std::env::var("AIC_REGION_EXTRA_CHAINS") {
        limits.extra_chains = value.parse().unwrap();
    }
    limits.validate().unwrap();
    let domain = candidates::describe(&w, &catalogs, &poses::VocabularyOptions::default()).unwrap();
    let mut kinds = BTreeMap::<&str, Vec<&candidates::Action>>::new();
    for action in &domain.actions {
        kinds.entry(action.kind()).or_default().push(action);
    }
    let mut status = BTreeMap::<String, usize>::new();
    let mut termination = BTreeMap::<String, usize>::new();
    let mut reason = BTreeMap::<String, usize>::new();
    let mut selected = BTreeMap::<&str, usize>::new();
    let mut max_chains = 0usize;
    let mut expansions = 0usize;
    let started = std::time::Instant::now();
    for (kind, quota) in [
        ("pose", 32),
        ("port", 16),
        ("reroute", 16),
        ("component", 16),
        ("release_bridge", 16),
    ] {
        let Some(actions) = kinds.get(kind) else {
            continue;
        };
        let count = quota.min(actions.len());
        selected.insert(kind, count);
        for ordinal in 0..count {
            let index = if count == 1 {
                0
            } else {
                ordinal * (actions.len() - 1) / (count - 1)
            };
            match candidates::execute(&w, &catalogs, actions[index], work, &limits) {
                Ok(result) => {
                    *status.entry(result.status.into()).or_default() += 1;
                    expansions += result.route_expansions;
                    if let Some(report) = result.region {
                        if std::env::var_os("AIC_REGION_DETAIL").is_some()
                            && result.status == "search-incomplete"
                        {
                            println!("detail={} ordinal={} attempts={} reason={:?} trials={}", kind, ordinal,
                                report.attempts.len(), report.incomplete.as_ref().map(|r| &r.code),
                                serde_json::json!(report.attempts.iter().map(|attempt| serde_json::json!({
                                    "status": attempt.status, "extra": attempt.extra_chains,
                                    "ports": attempt.port_assignments, "priority": attempt.prioritized_chain,
                                    "order": attempt.route_order,
                                })).collect::<Vec<_>>())
                            );
                        }
                        max_chains = max_chains.max(report.initial_chains.len());
                        *termination.entry(report.termination.into()).or_default() += 1;
                        if result.status == "search-incomplete" {
                            let code = report
                                .incomplete
                                .map_or("missing-reason".into(), |r| r.code);
                            *reason.entry(code).or_default() += 1;
                        }
                    }
                }
                Err(error) => *status.entry(format!("error:{}", error.code)).or_default() += 1,
            }
        }
    }
    println!(
        "default_panel={}",
        serde_json::json!({
            "facilities": w.placements.len(), "actions": domain.actions.len(), "selected": selected,
            "work_budget": work, "attempt_work": limits.attempt_work, "attempts": limits.attempts,
            "port_trials": limits.port_trials, "extra_chains": limits.extra_chains,
            "status": status, "termination": termination, "incomplete_reason": reason,
            "max_initial_chains": max_chains, "route_expansions": expansions,
            "wall_ms": started.elapsed().as_secs_f64() * 1000.0,
        })
    );
}

#[test]
fn region_retry_preserves_unknown_until_a_complete_candidate() {
    let (w, catalogs, _) = fixture();
    let limits = routing::Limits {
        attempts: 3,
        attempt_work: 25,
        ..Default::default()
    };
    let cases = [
        (
            vec![poses::Pose {
                member: 2,
                x: 2,
                y: 15,
                rotation: 90,
            }],
            "candidate",
            ["search-incomplete", "search-incomplete", "candidate"],
        ),
        (
            vec![
                poses::Pose {
                    member: 0,
                    x: 27,
                    y: 13,
                    rotation: 270,
                },
                poses::Pose {
                    member: 3,
                    x: 1,
                    y: 10,
                    rotation: 270,
                },
            ],
            "search-incomplete",
            ["search-incomplete", "rejected", "rejected"],
        ),
    ];
    for (placements, expected, statuses) in cases {
        let action = candidates::Action::Pose(poses::action(&w, placements));
        let result = candidates::execute(&w, &catalogs, &action, 75, &limits).unwrap();
        let report = result.region.as_ref().unwrap();
        assert_eq!(result.status, expected);
        assert_eq!(report.termination, expected);
        assert_eq!(
            report.attempts.iter().map(|a| a.status).collect::<Vec<_>>(),
            statuses
        );
        assert_eq!(report.incomplete.as_ref().unwrap().attempt, Some(0));
        assert_eq!(
            report.incomplete.as_ref().unwrap().code,
            "local-work-budget"
        );
        assert_eq!(result.witness.is_some(), expected == "candidate");
        assert_eq!(
            result.route_expansions,
            report
                .attempts
                .iter()
                .map(|a| a.route_expansions)
                .sum::<usize>()
        );
        assert!(result.route_expansions <= 75);
    }
}

#[test]
fn randomized_setup_records_route_search_stop() {
    let (w, catalogs, _) = fixture();
    let setup = Setup {
        identity_limits: physical_identity::Limits {
            max_individualization_nodes: 100_000,
        },
        steps: 1,
        elapsed_ms: 10_000,
        route_work: 1,
        kinds: vec!["reroute".into()],
    };
    let output = randomize(
        fixture_request(&w),
        w.clone(),
        Arc::new(crate::input::Catalogs::new(catalogs)),
        &setup,
        91,
        &|| Ok(()),
    )
    .unwrap_or_else(|e| panic!("{}", e.message));
    let record = &output.records[0];
    assert_eq!(record["changed"], false);
    assert!(record.get("acceptance").is_none());
    assert_eq!(record["executor_status"], "search-incomplete");
    assert_eq!(record["route_expansions"], 1);
    assert_eq!(record["region"]["contract"], "factory-route-region-v3");
    assert_eq!(record["region"]["termination"], "search-incomplete");
    assert_eq!(record["routes"][0]["status"], "work-budget");
}

#[test]
fn crossing_route_reembedding_retains_bridge_evidence() {
    let (w, catalogs, _) = fixture();
    let action = candidates::Action::Pose(poses::action(
        &w,
        vec![poses::Pose {
            member: 0,
            x: 7,
            y: 13,
            rotation: 90,
        }],
    ));
    let result =
        candidates::execute(&w, &catalogs, &action, 50_000, &routing::Limits::default()).unwrap();
    assert_eq!(result.status, "candidate");
    assert!(result.witness.is_some());
    let report = result.region.as_ref().unwrap();
    assert_eq!(report.termination, "candidate");
    assert!(
        report
            .attempts
            .iter()
            .flat_map(|attempt| &attempt.routes)
            .any(|route| route.status == "found" && !route.crossings.is_empty())
    );
    assert_eq!(
        result.route_expansions,
        report
            .attempts
            .iter()
            .map(|attempt| attempt.route_expansions)
            .sum::<usize>()
    );
}

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
    let serialized = serde_json::to_value(&source.config).unwrap();
    assert_eq!(serialized, configuration["generation"]);
    let directory =
        std::env::temp_dir().join(format!("aic-generated-slot-test-{}", std::process::id()));
    fs::create_dir(&directory).unwrap();
    let bytes = serde_json::to_vec(&input).unwrap();
    fs::write(directory.join("input.json"), &bytes).unwrap();
    fs::write(directory.join("manifest.json"),serde_json::to_vec(&json!({"schema":"aic-generated-slot-v2","slot":3,"source":source.identity,"input_sha256":crate::input::hash(&bytes)})).unwrap()).unwrap();
    assert!(source.generate(3, &directory).is_ok());
    let mut explicit = configuration["generation"].clone();
    explicit["construction"]["per_connection_route_searches"] = json!(64);
    explicit["construction"]["nearby_candidates"] = json!(0);
    explicit["construction"]["downstream_closed_growth"] = json!(true);
    explicit["construction"]["flow_direction"] = Value::Null;
    explicit["construction"]["connection_first_placement"] = json!(false);
    let equivalent = Source::load(
        &root,
        catalogs.clone(),
        serde_json::from_value(explicit.clone()).unwrap(),
        Default::default(),
    )
    .unwrap();
    assert_eq!(equivalent.identity, source.identity);
    assert_eq!(
        serde_json::to_value(equivalent.generate(3, &directory).unwrap().original).unwrap(),
        serde_json::to_value(&input.original).unwrap()
    );
    explicit["construction"]["per_connection_route_searches"] = json!(128);
    let wider = Source::load(
        &root,
        catalogs.clone(),
        serde_json::from_value(explicit.clone()).unwrap(),
        Default::default(),
    )
    .unwrap();
    assert_ne!(wider.identity, source.identity);
    assert!(
        wider
            .generate(3, &directory)
            .err()
            .unwrap()
            .contains("slot-binding")
    );
    explicit["construction"]["per_connection_route_searches"] = json!(64);
    explicit["construction"]["downstream_closed_growth"] = json!(false);
    let changed_policy = Source::load(
        &root,
        catalogs.clone(),
        serde_json::from_value(explicit.clone()).unwrap(),
        Default::default(),
    )
    .unwrap();
    assert_ne!(changed_policy.identity, source.identity);
    explicit["construction"]["per_connection_route_searches"] = json!(0);
    assert!(
        Source::load(
            &root,
            catalogs.clone(),
            serde_json::from_value(explicit).unwrap(),
            Default::default(),
        )
        .is_err()
    );
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
        if count.fetch_add(1, Ordering::Relaxed) >= 6 {
            Err("test-generation-cancelled".into())
        } else {
            Ok(())
        }
    }));
    assert_eq!(
        cancelled
            .generate(1, &directory.join("cancelled"))
            .err()
            .unwrap(),
        "test-generation-cancelled"
    );
    assert!(!directory.join("cancelled/manifest.json").exists());
    let failure: Value = crate::input::read(
        &directory.join("cancelled"),
        std::path::Path::new("failure.json"),
    )
    .unwrap();
    let construction = failure["attempts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|attempt| attempt["stage"] == "construction")
        .expect("the guard must interrupt construction after a selected draw");
    assert_eq!(construction["outcome"]["status"], "interrupted");
    assert_eq!(
        construction["outcome"]["kind"],
        "work-or-caller-interruption"
    );
    assert_eq!(construction["source"], cancelled.identity);
    assert!(construction["seed"].is_number());
    assert!(construction["duration_ms"].is_number());
    assert!(
        construction["outcome"]["error"]
            .as_str()
            .unwrap()
            .contains("test-generation-cancelled")
    );
    assert!(
        make()
            .generate(1, &directory.join("cancelled"))
            .err()
            .unwrap()
            .contains("incomplete")
    );
    std::fs::remove_dir_all(directory).unwrap();
}
