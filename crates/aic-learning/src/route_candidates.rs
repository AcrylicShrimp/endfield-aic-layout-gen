//! Parallel frozen-state harness for the non-learned route candidate boundary.

use std::{collections::BTreeSet, fs, path::Path, sync::Arc, time::Instant};

use aic_layout::{
    geometry::Point,
    physical::{BridgeAxis, Problem},
    validation::Catalogs,
};
use aic_pilot_native::constructive::{
    Command, Limits, Session,
    routing_candidates::{Candidate, Config as CandidateConfig, Request, Termination},
};
use anyhow::{Context, Result, ensure};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const PANEL_SCHEMA: &str = "aic-route-candidate-panel-v1";
const REPORT_SCHEMA: &str = "aic-route-candidate-harness-report-v2";
const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Panel {
    schema: String,
    partition: String,
    cpu_threads: usize,
    repetitions: usize,
    limits: Limits,
    candidate: CandidateConfig,
    cases: Vec<Case>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    id: String,
    problem: Problem,
    prefix: Vec<Command>,
    request: String,
    later_connections: Vec<String>,
}

#[derive(Debug, Serialize)]
struct HarnessReport {
    schema: &'static str,
    status: &'static str,
    partition: String,
    panel_sha256: String,
    catalog_sha256: String,
    cpu_threads: usize,
    repetitions: usize,
    case_count: usize,
    case_runs: usize,
    successful_fixed_requests: usize,
    fixed_request_proofs: usize,
    diverse_fixed_requests: usize,
    later_route_cases: usize,
    all_later_routes_survive: usize,
    root_candidate_samples: usize,
    downstream_candidate_samples: usize,
    total_candidate_samples: usize,
    sample_definition: &'static str,
    samples_per_minute: f64,
    root_searches: usize,
    downstream_searches: usize,
    total_searches: usize,
    root_expanded: usize,
    downstream_expanded: usize,
    total_expanded: usize,
    root_preflight_expanded: usize,
    downstream_preflight_expanded: usize,
    unique_guides: usize,
    native_rejections: usize,
    native_valid_ratio: f64,
    mean_pairwise_dissimilarity: Option<f64>,
    elapsed_seconds: f64,
    cases: Vec<CaseReport>,
}

#[derive(Clone, Debug, Serialize)]
struct CaseReport {
    id: String,
    state_revision: u64,
    prefix_commands: usize,
    request: String,
    termination: Termination,
    fixed_request_proven_infeasible: bool,
    endpoint_pairs: usize,
    searches: usize,
    expanded: usize,
    crossing_limit: usize,
    preflight_expanded: usize,
    guides_found: usize,
    duplicate_guides: usize,
    native_rejection_codes: Vec<String>,
    minimum_pairwise_dissimilarity: Option<f64>,
    mean_pairwise_dissimilarity: Option<f64>,
    mean_elapsed_seconds: f64,
    candidates: Vec<CandidateReport>,
}

#[derive(Clone, Debug, Serialize)]
struct CandidateReport {
    id: usize,
    candidate_sha256: String,
    source_port: Option<String>,
    target_port: Option<String>,
    cells: usize,
    steps: i64,
    turns: i64,
    bridges: Vec<BridgeUseSummary>,
    overlap_with_previous: usize,
    later_routes_requested: usize,
    later_routes_completed: usize,
    all_later_routes_survive: bool,
    downstream_candidate_samples: usize,
    downstream_searches: usize,
    downstream_expanded: usize,
    downstream_preflight_expanded: usize,
    downstream_unique_guides: usize,
    downstream_native_rejections: usize,
}

#[derive(Clone, Debug, Serialize)]
struct BridgeUseSummary {
    position: Point,
    component: String,
    rotation: i64,
    direction: usize,
    crossed_axis: BridgeAxis,
}

pub fn run(panel_path: &Path, catalog_root: &Path, output: &Path) -> Result<Value> {
    let started = Instant::now();
    ensure!(
        !output.exists(),
        "aic-learning.route-candidate-output: {} already exists",
        output.display()
    );
    let bytes = bounded_read(panel_path)?;
    let panel: Panel = serde_json::from_slice(&bytes)
        .with_context(|| format!("parse route candidate panel {}", panel_path.display()))?;
    validate_panel(&panel)?;
    let loaded = aic_layout_gen::io::load_catalogs(catalog_root)?;
    let catalogs = Arc::new(loaded.catalogs);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(panel.cpu_threads)
        .thread_name(|index| format!("route-candidate-{index}"))
        .build()
        .context("aic-learning.route-candidate-workers: build CPU worker pool")?;
    let tasks = (0..panel.cases.len())
        .flat_map(|case| (0..panel.repetitions).map(move |_| case))
        .collect::<Vec<_>>();
    let results = pool.install(|| {
        tasks
            .par_iter()
            .map(|&case| evaluate_case(&panel.cases[case], &panel, catalogs.clone()))
            .collect::<Vec<_>>()
    });
    let runs = results.into_iter().collect::<Result<Vec<_>>>()?;
    let cases = runs
        .chunks_exact(panel.repetitions)
        .map(|runs| {
            let mut representative = runs[0].clone();
            representative.mean_elapsed_seconds =
                runs.iter().map(|run| run.mean_elapsed_seconds).sum::<f64>() / runs.len() as f64;
            representative
        })
        .collect::<Vec<_>>();
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let root_candidate_samples = runs.iter().map(|case| case.candidates.len()).sum::<usize>();
    let downstream_candidate_samples = runs
        .iter()
        .flat_map(|case| &case.candidates)
        .map(|candidate| candidate.downstream_candidate_samples)
        .sum::<usize>();
    let total_candidate_samples = root_candidate_samples + downstream_candidate_samples;
    let root_searches = runs.iter().map(|case| case.searches).sum::<usize>();
    let downstream_searches = runs
        .iter()
        .flat_map(|case| &case.candidates)
        .map(|candidate| candidate.downstream_searches)
        .sum::<usize>();
    let root_expanded = runs.iter().map(|case| case.expanded).sum::<usize>();
    let downstream_expanded = runs
        .iter()
        .flat_map(|case| &case.candidates)
        .map(|candidate| candidate.downstream_expanded)
        .sum::<usize>();
    let root_preflight_expanded = runs
        .iter()
        .map(|case| case.preflight_expanded)
        .sum::<usize>();
    let downstream_preflight_expanded = runs
        .iter()
        .flat_map(|case| &case.candidates)
        .map(|candidate| candidate.downstream_preflight_expanded)
        .sum::<usize>();
    let root_native_rejections = runs
        .iter()
        .map(|case| case.native_rejection_codes.len())
        .sum::<usize>();
    let downstream_native_rejections = runs
        .iter()
        .flat_map(|case| &case.candidates)
        .map(|candidate| candidate.downstream_native_rejections)
        .sum::<usize>();
    let native_rejections = root_native_rejections + downstream_native_rejections;
    let downstream_unique_guides = runs
        .iter()
        .flat_map(|case| &case.candidates)
        .map(|candidate| candidate.downstream_unique_guides)
        .sum::<usize>();
    let unique_guides = root_candidate_samples + root_native_rejections + downstream_unique_guides;
    let diversity = cases
        .iter()
        .filter_map(|case| case.mean_pairwise_dissimilarity)
        .collect::<Vec<_>>();
    let report = HarnessReport {
        schema: REPORT_SCHEMA,
        status: "complete",
        partition: panel.partition,
        panel_sha256: sha256(&bytes),
        catalog_sha256: catalogs.identity.clone(),
        cpu_threads: panel.cpu_threads,
        repetitions: panel.repetitions,
        case_count: cases.len(),
        case_runs: runs.len(),
        successful_fixed_requests: cases
            .iter()
            .filter(|case| !case.candidates.is_empty())
            .count(),
        fixed_request_proofs: cases
            .iter()
            .filter(|case| case.fixed_request_proven_infeasible)
            .count(),
        diverse_fixed_requests: cases
            .iter()
            .filter(|case| {
                case.minimum_pairwise_dissimilarity
                    .is_some_and(|value| value > 0.0)
            })
            .count(),
        later_route_cases: cases
            .iter()
            .filter(|case| {
                case.candidates
                    .first()
                    .is_some_and(|candidate| candidate.later_routes_requested > 0)
            })
            .count(),
        all_later_routes_survive: cases
            .iter()
            .filter(|case| {
                case.candidates
                    .first()
                    .is_some_and(|candidate| candidate.later_routes_requested > 0)
                    && case
                        .candidates
                        .iter()
                        .all(|candidate| candidate.all_later_routes_survive)
            })
            .count(),
        root_candidate_samples,
        downstream_candidate_samples,
        total_candidate_samples,
        sample_definition: "native-valid complete route candidates",
        samples_per_minute: if elapsed_seconds == 0.0 {
            0.0
        } else {
            total_candidate_samples as f64 * 60.0 / elapsed_seconds
        },
        root_searches,
        downstream_searches,
        total_searches: root_searches + downstream_searches,
        root_expanded,
        downstream_expanded,
        total_expanded: root_expanded + downstream_expanded,
        root_preflight_expanded,
        downstream_preflight_expanded,
        unique_guides,
        native_rejections,
        native_valid_ratio: if unique_guides == 0 {
            0.0
        } else {
            total_candidate_samples as f64 / unique_guides as f64
        },
        mean_pairwise_dissimilarity: (!diversity.is_empty())
            .then(|| diversity.iter().sum::<f64>() / diversity.len() as f64),
        elapsed_seconds,
        cases,
    };
    write_report(output, &report)?;
    Ok(json!({
        "schema": REPORT_SCHEMA,
        "status": "complete",
        "output": output,
        "cases": report.case_count,
        "root_candidate_samples": report.root_candidate_samples,
        "total_candidate_samples": report.total_candidate_samples,
        "samples_per_minute": report.samples_per_minute,
        "elapsed_seconds": report.elapsed_seconds,
    }))
}

fn validate_panel(panel: &Panel) -> Result<()> {
    ensure!(
        panel.schema == PANEL_SCHEMA
            && panel.partition == "held-out"
            && (1..=256).contains(&panel.cpu_threads)
            && (1..=10_000).contains(&panel.repetitions)
            && (1..=4_096).contains(&panel.cases.len()),
        "aic-learning.route-candidate-panel: invalid schema, partition, worker count, repetition count or case count"
    );
    let mut ids = BTreeSet::new();
    let mut total_commands = 0usize;
    for case in &panel.cases {
        total_commands = total_commands.saturating_add(case.prefix.len());
        ensure!(
            !case.id.is_empty()
                && ids.insert(case.id.as_str())
                && !case.request.is_empty()
                && case.prefix.len() <= 1_000_000
                && case.later_connections.len() <= 4_096,
            "aic-learning.route-candidate-panel: malformed or duplicate case {}",
            case.id
        );
    }
    ensure!(
        total_commands <= 2_000_000,
        "aic-learning.route-candidate-panel: prefix command materialization guard exceeded"
    );
    Ok(())
}

fn evaluate_case(case: &Case, panel: &Panel, catalogs: Arc<Catalogs>) -> Result<CaseReport> {
    let started = Instant::now();
    let mut session = Session::new(case.problem.clone(), catalogs, panel.limits)
        .map_err(anyhow::Error::new)
        .with_context(|| format!("admit route candidate case {}", case.id))?;
    for (index, command) in case.prefix.iter().enumerate() {
        let step = session.step(command.clone());
        ensure!(
            step.accepted && step.diagnostic.is_none(),
            "aic-learning.route-candidate-prefix: case {} command {} rejected: {:?}",
            case.id,
            index,
            step.diagnostic
        );
    }
    ensure!(
        session.observe().active.is_none() && !session.done(),
        "aic-learning.route-candidate-prefix: case {} is not a stable unfinished state",
        case.id
    );
    let state_revision = session.observe().revision;
    let root = session
        .route_candidates(
            &Request {
                state_revision,
                connection: case.request.clone(),
            },
            &panel.candidate,
        )
        .map_err(anyhow::Error::new)
        .with_context(|| format!("generate candidates for case {}", case.id))?;
    let candidates = root
        .candidates
        .iter()
        .map(|candidate| evaluate_candidate(&session, candidate, case, &panel.candidate))
        .collect::<Result<Vec<_>>>()?;
    Ok(CaseReport {
        id: case.id.clone(),
        state_revision,
        prefix_commands: case.prefix.len(),
        request: case.request.clone(),
        termination: root.termination,
        fixed_request_proven_infeasible: root.fixed_request_proven_infeasible,
        endpoint_pairs: root.endpoint_pairs,
        searches: root.searches,
        expanded: root.expanded,
        crossing_limit: root.crossing_limit,
        preflight_expanded: root.preflight_expanded,
        guides_found: root.guides_found,
        duplicate_guides: root.duplicate_guides,
        native_rejection_codes: root
            .native_rejections
            .into_iter()
            .map(|rejection| rejection.diagnostic.code)
            .collect(),
        minimum_pairwise_dissimilarity: root.minimum_pairwise_dissimilarity,
        mean_pairwise_dissimilarity: root.mean_pairwise_dissimilarity,
        mean_elapsed_seconds: started.elapsed().as_secs_f64(),
        candidates,
    })
}

fn evaluate_candidate(
    root: &Session,
    candidate: &Candidate,
    case: &Case,
    config: &CandidateConfig,
) -> Result<CandidateReport> {
    let mut session = root.clone();
    session
        .apply_route_candidate(candidate)
        .map_err(anyhow::Error::new)
        .with_context(|| format!("apply candidate {} in case {}", candidate.id, case.id))?;
    let mut later_routes_completed = 0usize;
    let mut downstream_candidate_samples = 0usize;
    let mut downstream_searches = 0usize;
    let mut downstream_expanded = 0usize;
    let mut downstream_preflight_expanded = 0usize;
    let mut downstream_unique_guides = 0usize;
    let mut downstream_native_rejections = 0usize;
    for connection in &case.later_connections {
        if session.done() {
            break;
        }
        let report = session
            .route_candidates(
                &Request {
                    state_revision: session.observe().revision,
                    connection: connection.clone(),
                },
                config,
            )
            .map_err(anyhow::Error::new)
            .with_context(|| {
                format!(
                    "generate later connection {} after candidate {} in case {}",
                    connection, candidate.id, case.id
                )
            })?;
        downstream_candidate_samples += report.candidates.len();
        downstream_searches += report.searches;
        downstream_expanded += report.expanded;
        downstream_preflight_expanded += report.preflight_expanded;
        downstream_unique_guides += report.candidates.len() + report.native_rejections.len();
        downstream_native_rejections += report.native_rejections.len();
        let Some(next) = report.candidates.first() else {
            break;
        };
        session
            .apply_route_candidate(next)
            .map_err(anyhow::Error::new)
            .with_context(|| {
                format!(
                    "apply later connection {} after candidate {} in case {}",
                    connection, candidate.id, case.id
                )
            })?;
        later_routes_completed += 1;
    }
    Ok(CandidateReport {
        id: candidate.id,
        candidate_sha256: sha256(&serde_json::to_vec(candidate)?),
        source_port: candidate.source_port.clone(),
        target_port: candidate.target_port.clone(),
        cells: candidate.cells.len(),
        steps: candidate.steps,
        turns: candidate.turns,
        bridges: candidate
            .crossings
            .iter()
            .map(|crossing| BridgeUseSummary {
                position: crossing.position,
                component: crossing.component.clone(),
                rotation: crossing.rotation,
                direction: crossing.direction,
                crossed_axis: if crossing.direction % 2 == 0 {
                    BridgeAxis::Vertical
                } else {
                    BridgeAxis::Horizontal
                },
            })
            .collect(),
        overlap_with_previous: candidate.overlap_with_previous,
        later_routes_requested: case.later_connections.len(),
        later_routes_completed,
        all_later_routes_survive: later_routes_completed == case.later_connections.len(),
        downstream_candidate_samples,
        downstream_searches,
        downstream_expanded,
        downstream_preflight_expanded,
        downstream_unique_guides,
        downstream_native_rejections,
    })
}

fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    ensure!(
        metadata.len() <= MAX_INPUT_BYTES,
        "aic-learning.route-candidate-input: {} exceeds {} bytes",
        path.display(),
        MAX_INPUT_BYTES
    );
    fs::read(path).with_context(|| format!("read {}", path.display()))
}

fn write_report(output: &Path, report: &HarnessReport) -> Result<()> {
    if let Some(parent) = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let name = output
        .file_name()
        .and_then(|name| name.to_str())
        .context("aic-learning.route-candidate-output: output requires a UTF-8 file name")?;
    let temporary = output.with_file_name(format!(".{name}.tmp"));
    let bytes = serde_json::to_vec_pretty(report)?;
    fs::write(&temporary, bytes).with_context(|| format!("write {}", temporary.display()))?;
    fs::rename(&temporary, output)
        .with_context(|| format!("commit route candidate report {}", output.display()))
}

fn sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
