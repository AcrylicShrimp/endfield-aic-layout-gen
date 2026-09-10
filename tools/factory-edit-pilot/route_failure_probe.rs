use super::*;
use crate::full_factory::{changes, engine};
use std::cell::RefCell;

#[derive(Clone)]
struct Settings {
    name: String,
    halo: i64,
    crossings: usize,
    priority: bool,
}
thread_local! { static SETTINGS: RefCell<Settings> = RefCell::new(Settings { name:"baseline".into(),halo:0,crossings:2,priority:false }); }
pub(crate) fn ban_gate() -> bool {
    SETTINGS.with(|s| s.borrow().name.starts_with("ban-gate"))
}
pub(crate) fn halo() -> i64 {
    SETTINGS.with(|s| s.borrow().halo)
}
pub(crate) fn crossings() -> usize {
    SETTINGS.with(|s| s.borrow().crossings)
}
pub(crate) fn priority(id: usize) -> usize {
    SETTINGS.with(|s| {
        let s = s.borrow();
        usize::from(s.priority && id != if s.name.contains("43-first") { 43 } else { 28 })
    })
}
pub(crate) fn per_chain() -> usize {
    SETTINGS.with(|s| {
        if s.borrow().name.contains("balanced") {
            4
        } else {
            8
        }
    })
}
fn directory() -> PathBuf {
    PathBuf::from(std::env::var("AIC_ROUTE_PROBE_OUTPUT").expect("AIC_ROUTE_PROBE_OUTPUT"))
}
pub(crate) fn capture(value: Value) {
    if SETTINGS.with(|s| !s.borrow().name.starts_with("ports-")) {
        use std::fs::OpenOptions;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory().join(SETTINGS.with(|s| format!("{}-paths.jsonl", s.borrow().name))))
            .unwrap();
        writeln!(f, "{value}").unwrap();
    }
}

#[test]
#[ignore = "requires retained external failure inputs and an isolated diagnostic checkout"]
fn retained_route_failure_probe() {
    let input =
        PathBuf::from(std::env::var("AIC_ROUTE_PROBE_INPUT").expect("AIC_ROUTE_PROBE_INPUT"));
    let output = directory();
    std::fs::create_dir_all(&output).unwrap();
    let w: Witness =
        serde_json::from_slice(&std::fs::read(input.join("failure-before.json")).unwrap()).unwrap();
    let request: Value =
        serde_json::from_slice(&std::fs::read(input.join("request.json")).unwrap()).unwrap();
    let action: Value =
        serde_json::from_slice(&std::fs::read(input.join("failure-action.json")).unwrap()).unwrap();
    let ps: Vec<poses::Pose> = serde_json::from_value(action["poses"].clone()).unwrap();
    let (_, catalogs, _) = fixture();
    let catalogs = std::sync::Arc::new(catalogs);
    let (moves, rotations, _) = poses::options(&w, &catalogs, &ps).unwrap();
    let make_edit = || routing::Edit {
        moves: moves.clone(),
        facility_rotations: rotations.clone(),
        ..Default::default()
    };
    let mut cases = vec![
        ("baseline".to_string(), 0, 2, false, 5000, make_edit()),
        ("budget-200000".into(), 0, 2, false, 200000, make_edit()),
        ("halo-2".into(), 2, 2, false, 200000, make_edit()),
        ("halo-8".into(), 8, 2, false, 200000, make_edit()),
        ("crossings-4".into(), 0, 4, false, 200000, make_edit()),
        ("crossings-8".into(), 0, 8, false, 200000, make_edit()),
        ("chain-28-first".into(), 0, 2, true, 200000, make_edit()),
    ];
    for limit in [3, 4, 6, 8] {
        cases.push((
            format!("crossings-{limit}-43-first"),
            0,
            limit,
            true,
            200000,
            make_edit(),
        ));
    }
    cases.push(("crossings-3".into(), 0, 3, false, 200000, make_edit()));
    cases.push(("balanced-8-4".into(), 0, 8, false, 200000, make_edit()));
    cases.push((
        "balanced-8-4-43-first".into(),
        0,
        8,
        true,
        200000,
        make_edit(),
    ));
    cases.push(("ban-gate-28-first".into(), 0, 8, false, 200000, make_edit()));
    cases.push(("ban-gate-43-first".into(), 0, 8, true, 200000, make_edit()));
    for limit in [10, 11, 12, 16] {
        cases.push((
            format!("crossings-{limit}"),
            0,
            limit,
            false,
            200000,
            make_edit(),
        ));
        cases.push((
            format!("crossings-{limit}-43-first"),
            0,
            limit,
            true,
            200000,
            make_edit(),
        ));
    }
    cases.push((
        "crossings-12-43-first-budget-5000".into(),
        0,
        12,
        true,
        5000,
        make_edit(),
    ));
    for mask in 1..16 {
        for limit in [2, 8] {
            let mut edit = make_edit();
            edit.chain_ids = [13, 41, 57, 59]
                .into_iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, id)| id)
                .collect();
            cases.push((
                format!("ripup-{mask}-crossings-{limit}"),
                0,
                limit,
                false,
                200000,
                edit,
            ));
        }
    }
    let member = &w.placements[ps[0].member].instance;
    let ports = catalog::project(&w, &catalogs).unwrap();
    let terminals:Vec<_>=w.transport_networks.iter().flat_map(|n|n.terminals.iter()).filter(|t|matches!(&t.endpoint,aic_data::layouts::TransportNetworkEndpoint::Facility {instance,..} if instance==member)).collect();
    assert_eq!(terminals.len(), 2);
    let choices: Vec<Vec<_>> = terminals
        .iter()
        .map(|t| {
            ports[member]
                .iter()
                .filter(|p| p.direction == t.direction)
                .map(|p| p.id.clone())
                .collect()
        })
        .collect();
    for (i, a) in choices[0].iter().enumerate() {
        for (j, b) in choices[1].iter().enumerate() {
            let mut edit = make_edit();
            edit.port_assignments = vec![
                changes::PortAssignment {
                    terminal: terminals[0].id.clone(),
                    port: a.clone(),
                },
                changes::PortAssignment {
                    terminal: terminals[1].id.clone(),
                    port: b.clone(),
                },
            ];
            cases.push((format!("ports-{i}-{j}"), 0, 2, false, 200000, edit));
        }
    }
    let mut summaries = vec![];
    for (name, halo, crossings, priority, work, edit) in cases {
        SETTINGS.with(|s| {
            *s.borrow_mut() = Settings {
                name: name.clone(),
                halo,
                crossings,
                priority,
            }
        });
        let began = std::time::Instant::now();
        let result = routing::reembed(&w, &catalogs, &edit, work);
        let mut summary = match result {
            Err(e) => json!({"status":"error","diagnostics":[e]}),
            Ok(r) => {
                let mut value = json!({"status":r.status,"diagnostics":r.diagnostics,"routes":r.routes,"route_expansions":r.route_expansions});
                if let Some(candidate) = r.witness {
                    let config = engine::Config {
                        input_contract: "current-history-16".into(),
                        horizon: 1,
                        elapsed_ms: 120000,
                        route_work: 200000,
                        memory: true,
                        vocabulary: Default::default(),
                    };
                    let mut e = engine::Episode::new(
                        serde_json::from_value(request.clone()).unwrap(),
                        w.clone(),
                        catalogs.clone(),
                        config,
                    )
                    .unwrap();
                    value["acceptance"] =
                        serde_json::to_value(e.session.propose(&candidate)).unwrap();
                    std::fs::write(
                        output.join(format!("{name}-candidate.json")),
                        serde_json::to_vec(&candidate).unwrap(),
                    )
                    .unwrap();
                }
                value
            }
        };
        summary["name"] = json!(name);
        summary["halo"] = json!(halo);
        summary["crossing_limit"] = json!(crossings);
        summary["work_budget"] = json!(work);
        summary["seconds"] = json!(began.elapsed().as_secs_f64());
        println!(
            "PROBE {} {} {}",
            summary["name"], summary["status"], summary["acceptance"]["status"]
        );
        summaries.push(summary);
    }
    let baseline = summaries.iter().find(|s| s["name"] == "baseline").unwrap();
    assert_eq!(baseline["route_expansions"], 1253);
    assert_eq!(baseline["diagnostics"][0]["code"], "local-no-path");
    assert_eq!(baseline["diagnostics"][0]["entity"], 28);
    let recovered = summaries
        .iter()
        .find(|s| s["name"] == "crossings-12-43-first-budget-5000")
        .unwrap();
    assert_eq!(recovered["route_expansions"], 4964);
    assert_eq!(recovered["acceptance"]["candidate_validated"], true);
    assert_eq!(recovered["acceptance"]["status"], "accepted_exploration");
    assert_eq!(
        recovered["acceptance"]["candidate"]["score"],
        json!([1672, 456, 108])
    );
    std::fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summaries).unwrap(),
    )
    .unwrap();
}
