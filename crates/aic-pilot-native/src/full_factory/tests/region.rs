use super::*;
use crate::full_factory::{engine, identity};

fn configuration(work: usize) -> engine::Config {
    engine::Config {
        input_contract: "current-history-16".into(),
        horizon: 1,
        elapsed_ms: 30000,
        route_work: work,
        memory: true,
        vocabulary: Default::default(),
        repair: Default::default(),
    }
}

fn shifted_first(w: &Witness) -> candidates::Action {
    let p = &w.placements[0];
    candidates::Action::Pose(poses::action(
        w,
        vec![poses::Pose {
            member: 0,
            x: p.x + 1,
            y: p.y,
            rotation: p.rotation,
        }],
    ))
}

fn verify_accounting(execution: &routing::Execution, budget: usize) {
    let report = execution.region.as_ref().unwrap();
    assert!(execution.route_expansions <= budget);
    assert_eq!(execution.route_expansions, report.route_expansions);
    assert_eq!(
        report.route_expansions,
        report
            .attempts
            .iter()
            .map(|a| a.route_expansions)
            .sum::<usize>()
    );
    for a in &report.attempts {
        assert!(a.extra_chains.len() <= report.limits.extra_chains);
        assert_eq!(
            a.route_expansions,
            a.routes.iter().map(|r| r.expanded).sum::<usize>()
        );
    }
}

fn verify_outside(before: &Witness, after: &Witness, execution: &routing::Execution) {
    let report = execution.region.as_ref().unwrap();
    let last = report.attempts.last().unwrap();
    let changed: IndexSet<_> = last
        .chains
        .iter()
        .chain(&last.carried_chains)
        .copied()
        .collect();
    let decomposition = chains::extract(before, &IndexSet::new()).unwrap();
    for c in decomposition
        .chains
        .iter()
        .filter(|c| !changed.contains(&c.id))
    {
        let network = after
            .transport_networks
            .iter()
            .find(|n| n.id == before.transport_networks[c.network].id)
            .unwrap();
        for pair in c.cells.windows(2) {
            assert!(
                network
                    .segments
                    .iter()
                    .any(|s| Point::from(&s.from) == pair[0]
                        && Point::from(&s.to) == pair[1]
                        && equal_rate(s.rate, c.rate)),
                "outside chain {} changed",
                c.id
            );
        }
    }
    for contact in &last.fixed_contacts {
        let c = &decomposition.chains[contact.chain];
        let network = &after.transport_networks[c.network];
        assert!(
            network
                .cells
                .iter()
                .any(|p| Point::from(p) == contact.position)
        );
        if let Some(direction) = contact.direction {
            assert!(
                network.segments.iter().any(|s| {
                    let from = Point::from(&s.from);
                    let to = Point::from(&s.to);
                    if contact.end {
                        to == contact.position && from.offset(direction) == to
                    } else {
                        from == contact.position && from.offset(direction) == to
                    }
                }),
                "fixed contact arm changed: {contact:?}"
            );
        }
    }
}

#[test]
fn region_move_preserves_outside_routes_and_fixed_contacts() {
    let (w, catalogs, _) = fixture();
    let action = shifted_first(&w);
    let execution =
        candidates::execute(&w, &catalogs, &action, 5000, &routing::Limits::default()).unwrap();
    verify_accounting(&execution, 5000);
    let after = execution
        .witness
        .as_ref()
        .expect("small fixture move must produce a candidate");
    verify_outside(&w, after, &execution);
    for (old, new) in w.placements.iter().zip(&after.placements).skip(1) {
        assert_eq!(
            serde_json::to_value(old).unwrap(),
            serde_json::to_value(new).unwrap()
        );
    }
    let mut episode = engine::Episode::new(
        fixture_request(&w),
        w.clone(),
        std::sync::Arc::new(catalogs),
        configuration(5000),
    )
    .unwrap();
    assert!(episode.session.propose(after).candidate_validated);
}

#[test]
fn exhausted_repair_preserves_current_best_and_selected_action_accounting() {
    let (w, catalogs, _) = fixture();
    let mut episode = engine::Episode::new(
        fixture_request(&w),
        w.clone(),
        std::sync::Arc::new(catalogs),
        configuration(1),
    )
    .unwrap();
    let before = identity::Identity::new(episode.session.snapshot().current);
    let view = episode.prepare(false, false).unwrap();
    let domain = episode.diagnostic_domain().unwrap();
    let index = domain["actions"]
        .as_array()
        .unwrap()
        .iter()
        .position(|a| a["kind"] == "pose")
        .unwrap();
    let result = episode.step(view.handle, index).unwrap();
    assert_eq!(result["status"], "executor_rejected");
    assert_eq!(result["region"]["termination"], "work-exhausted");
    assert_eq!(result["route_expansions"], 1);
    assert_eq!(result["current_sha256"], before.ordered);
    assert_eq!(result["best_sha256"], before.ordered);
    assert_eq!(episode.selected, 1);
    assert_eq!(episode.session.snapshot().proposals, 0);
}

#[test]
fn invalid_region_limits_fail_before_route_search() {
    let (w, catalogs, _) = fixture();
    for limits in [
        routing::Limits {
            attempts: 0,
            ..Default::default()
        },
        routing::Limits {
            crossings_per_route: 9,
            ..Default::default()
        },
    ] {
        let error =
            candidates::execute(&w, &catalogs, &shifted_first(&w), 5000, &limits).unwrap_err();
        assert_eq!(error.code, "region-invalid-limits");
    }
}

#[test]
#[ignore = "requires retained external route-region regression inputs"]
fn retained_region_transactions_validate() {
    let input = PathBuf::from(std::env::var("AIC_REGION_CASE_DIR").expect("AIC_REGION_CASE_DIR"));
    let read = |name| std::fs::read(input.join(name)).unwrap();
    let w: Witness = serde_json::from_slice(&read("before.json")).unwrap();
    let request: aic_data::layouts::FactoryEditRequest =
        serde_json::from_slice(&read("request.json")).unwrap();
    let suite: Vec<Value> = serde_json::from_slice(&read("suite.json")).unwrap();
    let (_, catalogs, _) = fixture();
    let catalogs = std::sync::Arc::new(catalogs);
    let domain = candidates::describe(&w, &catalogs, &poses::VocabularyOptions::default()).unwrap();
    let mut results = Vec::new();
    for case in &suite {
        let action = domain
            .actions
            .iter()
            .find(|a| serde_json::to_value(a).unwrap() == case["action"])
            .unwrap();
        let execution =
            candidates::execute(&w, &catalogs, action, 5000, &routing::Limits::default()).unwrap();
        verify_accounting(&execution, 5000);
        let after = execution
            .witness
            .as_ref()
            .expect("retained transaction must recover");
        verify_outside(&w, after, &execution);
        let region = execution.region.as_ref().unwrap();
        if case["expect_ports"] == true {
            assert!(
                region
                    .attempts
                    .iter()
                    .any(|a| !a.port_assignments.is_empty())
            );
        }
        if case["expect_expansion"] == true {
            assert!(region.attempts.iter().any(|a| !a.extra_chains.is_empty()));
        }
        let mut episode = engine::Episode::new(
            request.clone(),
            w.clone(),
            catalogs.clone(),
            configuration(5000),
        )
        .unwrap();
        let acceptance = episode.session.propose(after);
        assert!(acceptance.candidate_validated, "{acceptance:?}");
        let repeated =
            candidates::execute(&w, &catalogs, action, 5000, &routing::Limits::default()).unwrap();
        assert_eq!(
            serde_json::to_value(&execution).unwrap(),
            serde_json::to_value(repeated).unwrap()
        );
        results.push(json!({"name":case["name"], "region":region, "acceptance":acceptance}));
    }
    std::fs::write(
        input.join("verification.json"),
        serde_json::to_vec_pretty(&results).unwrap(),
    )
    .unwrap();
}
