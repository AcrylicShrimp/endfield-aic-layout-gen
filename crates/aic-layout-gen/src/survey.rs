//! Equal-slot Monte Carlo survey of generator yield and spatial occupancy.

use crate::{
    Attempt, Config, Intent, IntentFailure, IntentGenerated, IntentGenerator, PackingProfile,
    ResolvedPolicy, TopologyProfile, io,
};
use aic_layout::{geometry::Diagnostic, physical::Metrics};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const SCHEMA: &str = "aic-layout-generation-survey-v2";
const REPORT_SCHEMA: &str = "aic-layout-generation-survey-report-v2";
const INTERRUPTED: &str = "aic-layout-gen.generation-interrupted";
const NOT_STARTED: &str = "aic-layout-generation-survey.deadline-not-started";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurveyConfig {
    pub schema: String,
    pub generator: Intent,
    pub facility_buckets: Vec<[usize; 2]>,
    pub slots_per_bucket: usize,
    pub workers: usize,
    pub max_elapsed_seconds: u64,
}

impl SurveyConfig {
    fn validate_shape(&self) -> Result<usize> {
        anyhow::ensure!(self.schema == SCHEMA, "unsupported survey schema");
        anyhow::ensure!(
            !self.facility_buckets.is_empty() && self.facility_buckets.len() <= 64,
            "facility_buckets must contain 1..64 ranges"
        );
        anyhow::ensure!(
            (1..=4096).contains(&self.slots_per_bucket),
            "slots_per_bucket must be 1..4096"
        );
        anyhow::ensure!((1..=16).contains(&self.workers), "workers must be 1..16");
        anyhow::ensure!(
            (1..=7 * 24 * 60 * 60).contains(&self.max_elapsed_seconds),
            "max_elapsed_seconds must be 1..604800"
        );
        let mut prior_end = None;
        for &[start, end] in &self.facility_buckets {
            anyhow::ensure!(
                (2..=256).contains(&start) && start <= end && end <= 256,
                "facility bucket {start}-{end} must be within 2..256"
            );
            anyhow::ensure!(
                prior_end.is_none_or(|prior| start > prior),
                "facility buckets must be ordered and non-overlapping"
            );
            prior_end = Some(end);
        }
        let requested = self
            .facility_buckets
            .len()
            .checked_mul(self.slots_per_bucket)
            .context("survey slot count overflow")?;
        anyhow::ensure!(
            requested <= 65_536,
            "survey may request at most 65536 slots"
        );
        Ok(requested)
    }
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub bucket_index: usize,
    pub facility_bucket: [usize; 2],
    pub slot: usize,
    pub seed: u64,
    pub status: String,
    pub completed: bool,
    pub facility_count: Option<usize>,
    pub connection_count: Option<usize>,
    pub target_connections: Option<usize>,
    pub connected_connections: Option<usize>,
    pub metrics: Option<Metrics>,
    pub elapsed_ms: f64,
    pub attempt_count: usize,
    pub total_routing_work: u64,
    pub last_attempt_status: Option<String>,
    pub diagnostic: Option<Diagnostic>,
    pub resolved: Option<ResolvedPolicy>,
}

impl Row {
    fn not_started(
        bucket_index: usize,
        facility_bucket: [usize; 2],
        slot: usize,
        seed: u64,
        resolved: ResolvedPolicy,
    ) -> Self {
        Self {
            bucket_index,
            facility_bucket,
            slot,
            seed,
            status: NOT_STARTED.into(),
            completed: false,
            facility_count: Some(resolved.request.facilities),
            connection_count: None,
            target_connections: None,
            connected_connections: None,
            metrics: None,
            elapsed_ms: 0.0,
            attempt_count: 0,
            total_routing_work: 0,
            last_attempt_status: None,
            diagnostic: None,
            resolved: Some(resolved),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct OccupancyQuantiles {
    pub count: usize,
    pub min: f64,
    pub p05: f64,
    pub p25: f64,
    pub p50: f64,
    pub p75: f64,
    pub p95: f64,
    pub max: f64,
}

#[derive(Debug, Serialize)]
pub struct BucketSummary {
    pub bucket_index: usize,
    pub facility_bucket: [usize; 2],
    pub requested: usize,
    pub completed: usize,
    pub successes: usize,
    pub failures: usize,
    pub incomplete: usize,
    pub yield_per_completed: Option<f64>,
    pub success_fraction_of_requested: f64,
    pub elapsed_ms: f64,
    pub total_routing_work: u64,
    pub status_counts: BTreeMap<String, usize>,
    pub failure_reasons: BTreeMap<String, usize>,
    pub occupancy: Option<OccupancyQuantiles>,
}

#[derive(Debug, Serialize)]
pub struct ProfileSummary {
    pub topology: TopologyProfile,
    pub packing: PackingProfile,
    pub requested: usize,
    pub completed: usize,
    pub successes: usize,
    pub failures: usize,
    pub incomplete: usize,
    pub yield_per_completed: Option<f64>,
    pub elapsed_ms: f64,
    pub total_routing_work: u64,
    pub failure_reasons: BTreeMap<String, usize>,
    pub occupancy: Option<OccupancyQuantiles>,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub catalog_sha256: String,
    pub config: SurveyConfig,
    pub seed: u64,
    pub requested: usize,
    pub completed: usize,
    pub successes: usize,
    pub incomplete: usize,
    pub deadline_reached: bool,
    pub elapsed_seconds: f64,
    pub summaries: Vec<BucketSummary>,
    pub profile_summaries: Vec<ProfileSummary>,
    pub rows: Vec<Row>,
}

pub fn run(config: SurveyConfig, catalog_dir: &Path, seed: u64, output: &Path) -> Result<Report> {
    let requested = config.validate_shape()?;
    let loaded = io::load_catalogs(catalog_dir)?;
    config.generator.validate()?;
    let mut generator_configs = Vec::with_capacity(config.facility_buckets.len());
    for &bucket in &config.facility_buckets {
        let generator_config = Config {
            schema: crate::intent::SCHEMA.into(),
            facilities: bucket,
            intent: config.generator.clone(),
        };
        generator_configs.push(generator_config);
    }
    let estimate = config.workers as u64
        * generator_configs
            .iter()
            .map(Config::conservative_workspace_bytes)
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .max()
            .unwrap_or(0);
    anyhow::ensure!(
        estimate <= 2 * 1024 * 1024 * 1024,
        "configured workers/route work exceed the 2 GiB conservative workspace guard"
    );
    anyhow::ensure!(
        !output.exists(),
        "output must be a new directory: {}",
        output.display()
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)
        .with_context(|| format!("output must be a new directory: {}", output.display()))?;
    io::write_json(&output.join("config.json"), &config)?;

    let started = Instant::now();
    let deadline = started + Duration::from_secs(config.max_elapsed_seconds);
    let generators = generator_configs
        .iter()
        .map(|generator_config| IntentGenerator::new(&loaded.catalogs, generator_config.clone()))
        .collect::<Result<Vec<_>>>()?;
    let next = AtomicUsize::new(0);
    let mut rows: Vec<Option<Row>> = std::iter::repeat_with(|| None).take(requested).collect();
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::sync_channel(config.workers * 2);
        for _ in 0..config.workers {
            let tx = tx.clone();
            let next = &next;
            let generators = &generators;
            let slots = config.slots_per_bucket;
            let bucket_count = generators.len();
            scope.spawn(move || {
                loop {
                    if Instant::now() >= deadline {
                        break;
                    }
                    let task = next.fetch_add(1, Ordering::Relaxed);
                    if task >= requested {
                        break;
                    }
                    // Round-robin scheduling prevents a deadline from starving
                    // the larger ranges at the end of the configuration.
                    let bucket_index = task % bucket_count;
                    let slot = task / bucket_count;
                    let index = bucket_index * slots + slot;
                    let slot_seed = seed.wrapping_add(index as u64);
                    let slot_started = Instant::now();
                    let result = generators[bucket_index]
                        .generate_once_until(slot_seed, || Instant::now() >= deadline);
                    let elapsed_ms = slot_started.elapsed().as_secs_f64() * 1000.0;
                    if tx.send((index, elapsed_ms, result)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        for (index, elapsed_ms, result) in rx {
            rows[index] = Some(row_from_result(
                index,
                config.slots_per_bucket,
                config.facility_buckets[index / config.slots_per_bucket],
                elapsed_ms,
                result,
            ));
        }
    });
    for (index, row) in rows.iter_mut().enumerate() {
        if row.is_none() {
            let bucket_index = index / config.slots_per_bucket;
            let seed = seed.wrapping_add(index as u64);
            let (request, _) = generator_configs[bucket_index].sample(seed);
            let (resolved, _) = generator_configs[bucket_index].resolve_request(request, true)?;
            *row = Some(Row::not_started(
                bucket_index,
                config.facility_buckets[bucket_index],
                index % config.slots_per_bucket,
                seed,
                resolved,
            ));
        }
    }
    let rows: Vec<_> = rows.into_iter().map(Option::unwrap).collect();
    let summaries = config
        .facility_buckets
        .iter()
        .enumerate()
        .map(|(bucket_index, &bucket)| {
            summarize(
                bucket_index,
                bucket,
                &rows[bucket_index * config.slots_per_bucket
                    ..(bucket_index + 1) * config.slots_per_bucket],
            )
        })
        .collect();
    let profile_summaries = profile_summaries(&rows);
    let completed = rows.iter().filter(|row| row.completed).count();
    let successes = rows.iter().filter(|row| row.metrics.is_some()).count();
    let incomplete = requested - completed;
    let report = Report {
        schema: REPORT_SCHEMA,
        catalog_sha256: loaded.catalogs.identity.clone(),
        config,
        seed,
        requested,
        completed,
        successes,
        incomplete,
        deadline_reached: incomplete > 0,
        elapsed_seconds: started.elapsed().as_secs_f64(),
        summaries,
        profile_summaries,
        rows,
    };
    io::write_json(&output.join("report.json"), &report)?;
    Ok(report)
}

fn row_from_result(
    index: usize,
    slots: usize,
    bucket: [usize; 2],
    elapsed_ms: f64,
    result: std::result::Result<IntentGenerated, IntentFailure>,
) -> Row {
    let bucket_index = index / slots;
    let slot = index % slots;
    match result {
        Ok(intent_generated) => {
            let generated = intent_generated.generated;
            let total_routing_work = route_work(&generated.attempts);
            let last = generated.attempts.last();
            Row {
                bucket_index,
                facility_bucket: bucket,
                slot,
                seed: intent_generated.request_seed,
                status: "complete".into(),
                completed: true,
                facility_count: Some(generated.layout.problem.blocks.len()),
                connection_count: Some(generated.layout.problem.connections.len()),
                target_connections: last.map(|attempt| attempt.target_connections),
                connected_connections: last.map(|attempt| attempt.connected),
                metrics: Some(generated.metrics),
                elapsed_ms,
                attempt_count: generated.attempts.len(),
                total_routing_work,
                last_attempt_status: last.map(|attempt| attempt.status.clone()),
                diagnostic: None,
                resolved: Some(intent_generated.resolved),
            }
        }
        Err(intent_failed) => {
            let failed = intent_failed.failure;
            let total_routing_work = route_work(&failed.attempts);
            let last = failed.attempts.last();
            let completed = failed.diagnostic.code != INTERRUPTED;
            Row {
                bucket_index,
                facility_bucket: bucket,
                slot,
                seed: intent_failed.request_seed,
                status: failed.diagnostic.code.clone(),
                completed,
                facility_count: Some(intent_failed.resolved.request.facilities),
                connection_count: None,
                target_connections: last.map(|attempt| attempt.target_connections),
                connected_connections: last.map(|attempt| attempt.connected),
                metrics: None,
                elapsed_ms,
                attempt_count: failed.attempts.len(),
                total_routing_work,
                last_attempt_status: last.map(|attempt| attempt.status.clone()),
                diagnostic: Some(failed.diagnostic),
                resolved: Some(intent_failed.resolved),
            }
        }
    }
}

fn profile_summaries(rows: &[Row]) -> Vec<ProfileSummary> {
    let mut groups = BTreeMap::<(TopologyProfile, PackingProfile), Vec<&Row>>::new();
    for row in rows {
        if let Some(resolved) = &row.resolved {
            groups
                .entry((resolved.request.topology, resolved.request.packing))
                .or_default()
                .push(row);
        }
    }
    groups
        .into_iter()
        .map(|((topology, packing), rows)| {
            let completed = rows.iter().filter(|row| row.completed).count();
            let successes = rows.iter().filter(|row| row.metrics.is_some()).count();
            let mut failure_reasons = BTreeMap::new();
            let mut occupancy: Vec<_> = rows
                .iter()
                .filter_map(|row| row.metrics.as_ref().map(|metrics| metrics.occupancy))
                .collect();
            for row in &rows {
                if row.completed && row.metrics.is_none() {
                    *failure_reasons
                        .entry(
                            row.last_attempt_status
                                .as_deref()
                                .unwrap_or("no-attempt")
                                .to_owned(),
                        )
                        .or_default() += 1;
                }
            }
            occupancy.sort_by(f64::total_cmp);
            ProfileSummary {
                topology,
                packing,
                requested: rows.len(),
                completed,
                successes,
                failures: completed - successes,
                incomplete: rows.len() - completed,
                yield_per_completed: (completed > 0).then_some(successes as f64 / completed as f64),
                elapsed_ms: rows.iter().map(|row| row.elapsed_ms).sum(),
                total_routing_work: rows.iter().map(|row| row.total_routing_work).sum(),
                failure_reasons,
                occupancy: quantiles(&occupancy),
            }
        })
        .collect()
}

fn route_work(attempts: &[Attempt]) -> u64 {
    attempts
        .iter()
        .map(|attempt| attempt.route_expansions as u64)
        .sum()
}

fn summarize(bucket_index: usize, bucket: [usize; 2], rows: &[Row]) -> BucketSummary {
    let completed = rows.iter().filter(|row| row.completed).count();
    let successes = rows.iter().filter(|row| row.metrics.is_some()).count();
    let mut status_counts = BTreeMap::new();
    let mut failure_reasons = BTreeMap::new();
    let mut occupancy: Vec<_> = rows
        .iter()
        .filter_map(|row| row.metrics.as_ref().map(|metrics| metrics.occupancy))
        .collect();
    for row in rows {
        *status_counts.entry(row.status.clone()).or_default() += 1;
        if row.completed && row.metrics.is_none() {
            let reason = row
                .last_attempt_status
                .as_deref()
                .unwrap_or("no-attempt")
                .to_owned();
            *failure_reasons.entry(reason).or_default() += 1;
        }
    }
    occupancy.sort_by(f64::total_cmp);
    BucketSummary {
        bucket_index,
        facility_bucket: bucket,
        requested: rows.len(),
        completed,
        successes,
        failures: completed - successes,
        incomplete: rows.len() - completed,
        yield_per_completed: (completed > 0).then_some(successes as f64 / completed as f64),
        success_fraction_of_requested: successes as f64 / rows.len() as f64,
        elapsed_ms: rows.iter().map(|row| row.elapsed_ms).sum(),
        total_routing_work: rows.iter().map(|row| row.total_routing_work).sum(),
        status_counts,
        failure_reasons,
        occupancy: quantiles(&occupancy),
    }
}

fn quantiles(sorted: &[f64]) -> Option<OccupancyQuantiles> {
    if sorted.is_empty() {
        return None;
    }
    let at = |percent: usize| sorted[(sorted.len() - 1) * percent / 100];
    Some(OccupancyQuantiles {
        count: sorted.len(),
        min: sorted[0],
        p05: at(5),
        p25: at(25),
        p50: at(50),
        p75: at(75),
        p95: at(95),
        max: sorted[sorted.len() - 1],
    })
}
