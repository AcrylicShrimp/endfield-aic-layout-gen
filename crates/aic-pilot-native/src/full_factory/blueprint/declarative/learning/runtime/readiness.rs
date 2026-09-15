//! Explicit bounded diagnostic; never invoked by normal collection.
use super::*;
use crate::full_factory::blueprint::declarative::tests::session;

#[derive(Default, Serialize)]
struct Counts {
    calls: u64,
    charged: u64,
    selection_errors: u64,
    statuses: BTreeMap<String, u64>,
    families: BTreeMap<String, u64>,
    work: BTreeMap<String, u64>,
    unfinished: u64,
    stops: Vec<Value>,
}
impl Counts {
    fn record(&mut self, runtime: &Runtime, reply: std::result::Result<Value, String>) -> Value {
        self.calls += 1;
        match reply {
            Err(error) => {
                self.selection_errors += 1;
                json!({"selection_error":error})
            }
            Ok(reply) => {
                let outcome = &reply["outcome"];
                self.charged += u64::from(outcome["charged"] == true);
                *self
                    .statuses
                    .entry(outcome["status"].as_str().unwrap().into())
                    .or_default() += 1;
                *self
                    .families
                    .entry(reply["action"]["kind"].as_str().unwrap().into())
                    .or_default() += 1;
                for name in [
                    "route_calls",
                    "route_expansions",
                    "preflight_expansions",
                    "validations",
                ] {
                    let total = self.work.entry(name.into()).or_default();
                    *total += outcome["details"][name].as_u64().unwrap();
                    assert_eq!(*total, reply["work"][name].as_u64().unwrap());
                }
                if runtime.result()["unfinished_draft"] == true {
                    self.unfinished += 1;
                    assert!(outcome["details"]["native_acceptance"].is_null());
                }
                if outcome["done"] == true {
                    self.stops.push(outcome["diagnostics"].clone());
                }
                reply
            }
        }
    }
}
fn band(runtime: &mut Runtime, counts: &mut Counts, kind: usize, at: i64) -> Value {
    let o = runtime.observe().unwrap();
    let root = runtime
        .learning
        .roots
        .iter()
        .position(|r| r.kind == kind && matches!(r.axis, Axis::X) && r.at == at && r.count == 1)
        .expect("scripted band exists");
    let reply = runtime.step(o.handle, root, &[]);
    counts.record(runtime, reply)
}
fn runtime() -> Runtime {
    Runtime::from_session(
        session(),
        Options {
            translation_steps: (-32..=32).filter(|x| *x != 0).collect(),
            band_counts: vec![1],
            masking: None,
        },
    )
    .unwrap()
}
#[test]
#[ignore = "explicit final-only Runtime::step matrix; requires AIC_IO_MATRIX_REPORT"]
fn integrated_io_readiness_matrix() {
    let mut rows = vec![];
    for name in [
        "accepted-better-worse",
        "unchanged-consumed-syntax",
        "overlap-pending",
        "work-exhaustion",
        "wall-cancellation",
        "complete-validation-failure",
        "true-horizon",
    ] {
        let mut r = runtime();
        let mut counts = Counts::default();
        let mut outcomes = vec![];
        match name {
            "accepted-better-worse" => {
                let better = band(&mut r, &mut counts, 2, 24);
                assert_eq!(better["outcome"]["status"], "improved");
                let worse = band(&mut r, &mut counts, 1, 24);
                assert_eq!(worse["outcome"]["status"], "validated");
                assert!(
                    worse["outcome"]["details"]["native_acceptance"]["metrics"]["score"][0]
                        .as_u64()
                        .unwrap()
                        > worse["outcome"]["best_metrics"]["score"][0]
                            .as_u64()
                            .unwrap()
                );
                outcomes.extend([better, worse]);
            }
            "unchanged-consumed-syntax" => {
                let o = r.observe().unwrap();
                let reply = r.step(o.handle, usize::MAX, &[]);
                outcomes.push(counts.record(&r, reply));
                outcomes.push(band(&mut r, &mut counts, 1, 0));
                let reply = r.step(o.handle, 0, &[]);
                outcomes.push(counts.record(&r, reply));
                outcomes.push(band(&mut r, &mut counts, 2, 3));
                assert_eq!(counts.selection_errors, 2);
                assert_eq!(counts.statuses["unchanged"], 1);
                assert_eq!(counts.statuses["rejected"], 1);
            }
            "overlap-pending" | "work-exhaustion" => {
                if name == "work-exhaustion" {
                    r.session.config.route_expansions = 1;
                    r.session.config.max_total_route_expansions = 1;
                }
                let o = r.observe().unwrap();
                let root = r
                    .learning
                    .roots
                    .iter()
                    .position(|p| {
                        p.kind == 0
                            && p.rotation == 90
                            && if name == "work-exhaustion" {
                                r.session.blueprint.facilities[p.owner].instance == "C1"
                                    && p.x == 27
                                    && p.y == 13
                            } else {
                                r.session.blueprint.facilities[p.owner].instance == "A0"
                                    && p.x == 2
                                    && p.y == 7
                            }
                    })
                    .expect("fixture placement is available");
                let p = &r.learning.roots[root];
                let ports: Vec<_> = r.learning.facilities[p.owner]
                    .groups
                    .iter()
                    .map(|g| {
                        g.ports
                            .iter()
                            .position(|port| Some(port) == r.session.state.ports[g.node].as_ref())
                            .unwrap()
                    })
                    .collect();
                let reply = r.step(o.handle, root, &ports);
                outcomes.push(counts.record(&r, reply));
                assert_eq!(r.result()["unfinished_draft"], true, "{name}: {outcomes:?}");
                if name == "work-exhaustion" {
                    assert_eq!(r.session.work.route_expansions, 1);
                    assert!(r.session.done());
                } else {
                    assert!(!r.session.done());
                    assert!(r.observe().unwrap().input.conflicts > 0);
                }
            }
            "wall-cancellation" => {
                r.session.elapsed_before = Duration::from_millis(r.session.config.elapsed_ms);
                outcomes.push(band(&mut r, &mut counts, 2, 24));
                assert_eq!(counts.charged, 0);
                assert!(r.session.done());
            }
            "complete-validation-failure" => {
                // Explicit post-admission fault injection exercises the complete-candidate validator.
                r.session.request.instances[0].utilization.numerator += 1;
                outcomes.push(band(&mut r, &mut counts, 2, 24));
                assert_eq!(r.session.work.validations, 1);
                assert!(!r.session.validated);
                assert!(
                    outcomes[0]["outcome"]["diagnostics"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|d| d["code"] == "declarative.native-validation")
                );
            }
            "true-horizon" => {
                r.session.config.max_actions = 2;
                outcomes.push(band(&mut r, &mut counts, 1, 0));
                assert_eq!(outcomes[0]["outcome"]["done"], false);
                outcomes.push(band(&mut r, &mut counts, 1, 0));
                assert_eq!(outcomes[1]["outcome"]["done"], true);
            }
            _ => unreachable!(),
        }
        assert!(counts.calls <= 32);
        assert_eq!(counts.charged, r.session.attempts);
        assert_eq!(
            counts.calls,
            counts.selection_errors + counts.statuses.values().sum::<u64>()
        );
        assert_eq!(
            counts.families.values().sum::<u64>(),
            counts.statuses.values().sum::<u64>()
        );
        rows.push(json!({"case":name,"counts":counts,"outcomes":outcomes,"retained":r.result(),"episode_complete":r.session.attempts==r.session.config.max_actions,"resource_censored":r.session.expired() || r.session.work.route_expansions>=r.session.config.max_total_route_expansions,"validation_failure":name=="complete-validation-failure"}));
    }
    std::fs::write(
        std::env::var("AIC_IO_MATRIX_REPORT").unwrap(),
        serde_json::to_vec(&json!({"schema":"aic-io-readiness-matrix-v1","rows":rows})).unwrap(),
    )
    .unwrap();
}
