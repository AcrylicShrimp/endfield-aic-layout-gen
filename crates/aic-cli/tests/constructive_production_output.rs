use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::Value;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    directory: PathBuf,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "aic-production-output-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        Self {
            directory,
            root: Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
        }
    }

    fn command(&self, max_steps: &str, override_input: Option<(&str, &Path)>) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aic-cli"));
        command.current_dir(&self.root).args([
            "layouts",
            "construct-production",
            "--target-instance",
            "facility-instance:recipe-occurrence:/target:0",
            "--max-steps",
            max_steps,
        ]);
        for (flag, default) in [
            ("--recipes", "data/game/normalized/recipes.json"),
            (
                "--source-plan",
                "data/examples/source-plan.game-heavy-xiranite-forge.request.json",
            ),
            ("--facility-catalog", "data/game/normalized/facilities.json"),
            ("--item-catalog", "data/game/normalized/items.json"),
            (
                "--transport-catalog",
                "data/game/normalized/transports.json",
            ),
            (
                "--logistics-component-catalog",
                "data/game/normalized/logistics-components.json",
            ),
        ] {
            let input = override_input
                .filter(|(selected, _)| *selected == flag)
                .map_or(Path::new(default), |(_, input)| input);
            command.arg(flag).arg(input);
        }
        command
    }

    fn run(&self, steps: &str, input: Option<(&str, &Path)>) -> Output {
        self.command(steps, input)
            .arg("--report-output")
            .arg(self.directory.join("report.json"))
            .arg("--visualization-output")
            .arg(self.directory.join("history.html"))
            .output()
            .unwrap()
    }

    fn invalid_catalog(&self, name: &str) -> PathBuf {
        let mut catalog: Value = serde_json::from_slice(
            &fs::read(self.root.join(format!("data/game/normalized/{name}.json"))).unwrap(),
        )
        .unwrap();
        catalog["schema_version"] = 999.into();
        let output = self.directory.join(format!("invalid-{name}.json"));
        fs::write(&output, serde_json::to_vec(&catalog).unwrap()).unwrap();
        output
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Only this test's freshly created private directory is removed.
        fs::remove_dir_all(&self.directory).unwrap();
    }
}

fn summary(output: &Output) -> Value {
    assert!(output.stdout.len() < 1024);
    assert_eq!(output.stdout.iter().filter(|&&b| b == b'\n').count(), 1);
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["schema_version"], 1);
    assert_eq!(value["kind"], "constructive-production-summary");
    assert!(value.get("steps").is_none());
    assert!(value.get("diagnostics").is_none());
    value
}

#[test]
fn invalid_planner_request_writes_full_json_and_one_small_summary() {
    let fixture = Fixture::new();
    let output = fixture.run("0", None);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let summary = summary(&output);
    assert_eq!(summary["status"], "invalid-input");
    assert_eq!(summary["success"], false);
    assert!(summary["visualization_output"].is_null());
    let report: Value =
        serde_json::from_slice(&fs::read(fixture.directory.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["schema_version"], 2);
    assert!(!report["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn partial_factory_summary_identifies_both_written_artifacts() {
    let fixture = Fixture::new();
    let output = fixture.run("1", None);
    assert_eq!(output.status.code(), Some(1));
    let summary = summary(&output);
    assert_eq!(summary["success"], false);
    assert!(Path::new(summary["report_output"].as_str().unwrap()).is_file());
    assert!(Path::new(summary["visualization_output"].as_str().unwrap()).is_file());
}

#[test]
fn downstream_growth_flag_records_order_diagnostics_without_expanding_stdout() {
    let fixture = Fixture::new();
    let output = fixture
        .command("1", None)
        .arg("--downstream-closed-growth")
        .args(["--flow-direction", "east"])
        .arg("--connection-first-placement")
        .arg("--report-output")
        .arg(fixture.directory.join("report.json"))
        .arg("--visualization-output")
        .arg(fixture.directory.join("history.html"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(summary(&output)["success"], false);
    let report: Value =
        serde_json::from_slice(&fs::read(fixture.directory.join("report.json")).unwrap()).unwrap();
    assert_eq!(report["request"]["downstream_closed_growth"], true);
    assert_eq!(report["request"]["flow_direction"], "east");
    assert_eq!(report["request"]["connection_first_placement"], true);
    assert!(
        !report["direct_placement_decisions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        !report["orientation_decisions"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!report["growth_decisions"].as_array().unwrap().is_empty());
}

#[test]
fn early_catalog_diagnostics_go_to_file_not_stdout() {
    for (name, flag) in [
        ("recipes", "--recipes"),
        ("items", "--item-catalog"),
        ("facilities", "--facility-catalog"),
        ("transports", "--transport-catalog"),
        ("logistics-components", "--logistics-component-catalog"),
    ] {
        let fixture = Fixture::new();
        let invalid = fixture.invalid_catalog(name);
        let output = fixture.run("0", Some((flag, &invalid)));
        assert_eq!(output.status.code(), Some(1), "{name}");
        assert!(output.stderr.is_empty(), "{name}: {:?}", output.stderr);
        let summary = summary(&output);
        assert_eq!(summary["status"], "invalid-input");
        assert!(summary["visualization_output"].is_null());
        let report: Value =
            serde_json::from_slice(&fs::read(fixture.directory.join("report.json")).unwrap())
                .unwrap();
        assert!(
            !report["diagnostics"].as_array().unwrap().is_empty(),
            "{name}"
        );
    }
}

#[test]
fn missing_input_never_announces_completion() {
    let fixture = Fixture::new();
    let missing = fixture.directory.join("missing.json");
    let output = fixture.run("0", Some(("--logistics-component-catalog", &missing)));
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing.json"));
}

#[test]
fn failed_artifact_writes_never_announce_completion() {
    for html_failure in [false, true] {
        let fixture = Fixture::new();
        let report = fixture.directory.join("report.json");
        let html = fixture.directory.join("history.html");
        let output = fixture
            .command(if html_failure { "1" } else { "0" }, None)
            .arg("--report-output")
            .arg(if html_failure {
                &report
            } else {
                &fixture.directory
            })
            .arg("--visualization-output")
            .arg(if html_failure {
                &fixture.directory
            } else {
                &html
            })
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        assert!(!output.stderr.is_empty());
    }
}
