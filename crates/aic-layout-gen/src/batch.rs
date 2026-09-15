use crate::{Attempt, Config, Generator, io, render};
use aic_layout::physical::Metrics;
use anyhow::{Context, Result};
use serde::Serialize;
use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Instant,
};

#[derive(Clone, Debug, Serialize)]
pub struct Row {
    pub index: usize,
    pub seed: u64,
    pub status: String,
    pub blocks: usize,
    pub connections: usize,
    pub metrics: Option<Metrics>,
    pub layout_sha256: Option<String>,
    pub generation_ms: f64,
    pub validation_ms: f64,
    pub publication_ms: f64,
    pub attempts: Vec<Attempt>,
}

#[derive(Serialize)]
pub struct Report {
    pub schema: &'static str,
    pub catalog_sha256: String,
    pub config: Config,
    pub seed: u64,
    pub workers: usize,
    pub requested: usize,
    pub complete: usize,
    pub elapsed_seconds: f64,
    pub complete_per_second: f64,
    pub rows: Vec<Row>,
}

pub fn run(
    config: Config,
    catalog_dir: &Path,
    seed: u64,
    count: usize,
    workers: usize,
    output: &Path,
) -> Result<Report> {
    let started = Instant::now();
    anyhow::ensure!(
        (1..=4096).contains(&count) && (1..=16).contains(&workers),
        "count must be 1..4096 and workers 1..16"
    );
    let loaded = io::load_catalogs(catalog_dir)?;
    let generator = Generator::new(&loaded.catalogs, config.clone())
        .map_err(|d| anyhow::anyhow!(serde_json::to_string(&d).unwrap()))?;
    let estimate =
        workers as u64 * (config.route_work_per_call as u64 * 640 + config.max_cells as u64 * 256);
    anyhow::ensure!(
        estimate <= 2 * 1024 * 1024 * 1024,
        "configured workers/route work exceed the 2 GiB conservative workspace guard"
    );
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)
        .with_context(|| format!("output must be a new directory: {}", output.display()))?;
    fs::create_dir(output.join("catalogs"))?;
    for (name, bytes) in &loaded.sources {
        io::write(&output.join("catalogs").join(name), bytes)?;
    }
    io::write_json(&output.join("config.json"), &config)?;
    let next = AtomicUsize::new(0);
    let mut rows: Vec<Option<Row>> = vec![None; count];
    std::thread::scope(|scope| -> Result<()> {
        let (tx, rx) = mpsc::sync_channel(workers * 2);
        for _ in 0..workers {
            let tx = tx.clone();
            let next = &next;
            let generator = &generator;
            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    if index >= count {
                        break;
                    }
                    let result = generator.generate(seed.wrapping_add(index as u64));
                    if tx.send((index, result)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let result = (|| -> Result<()> {
            for (index, result) in &rx {
                let before = Instant::now();
                let row = match result {
                    Ok(generated) => {
                        let bytes = serde_json::to_vec(&generated.layout)?;
                        anyhow::ensure!(
                            bytes.len() <= 16 * 1024 * 1024,
                            "layout slot {index} exceeds the 16 MiB publication/input guard"
                        );
                        io::write(&output.join(format!("layout-{index:05}.json")), &bytes)?;
                        let svg = render::svg(&generated.layout, &loaded.catalogs);
                        io::write(&output.join(format!("case-{index:05}.svg")), svg.as_bytes())?;
                        io::write(
                            &output.join(format!("case-{index:05}.html")),
                            render::page(&svg, generated.seed, &generated.metrics).as_bytes(),
                        )?;
                        let row = Row {
                            index,
                            seed: generated.seed,
                            status: "complete".into(),
                            blocks: generated.layout.problem.blocks.len(),
                            connections: generated.layout.problem.connections.len(),
                            metrics: Some(generated.metrics),
                            layout_sha256: Some(io::hash(&bytes)),
                            generation_ms: generated.generation_ms,
                            validation_ms: generated.validation_ms,
                            publication_ms: before.elapsed().as_secs_f64() * 1000.0,
                            attempts: generated.attempts,
                        };
                        io::write_json(
                            &output.join(format!("case-{index:05}.json")),
                            &serde_json::json!({"row":row,"sequence_pair":generated.sequence_pair}),
                        )?;
                        row
                    }
                    Err(failed) => {
                        io::write_json(&output.join(format!("case-{index:05}.json")), &failed)?;
                        Row {
                            index,
                            seed: failed.seed,
                            status: failed.diagnostic.code,
                            blocks: failed.attempts.last().map_or(0, |a| a.blocks),
                            connections: 0,
                            metrics: None,
                            layout_sha256: None,
                            generation_ms: failed.attempts.iter().map(|a| a.elapsed_ms).sum(),
                            validation_ms: 0.0,
                            publication_ms: before.elapsed().as_secs_f64() * 1000.0,
                            attempts: failed.attempts,
                        }
                    }
                };
                rows[index] = Some(row);
            }
            Ok(())
        })();
        // A publication failure closes the bounded queue before joining workers.
        drop(rx);
        result
    })?;
    let rows: Vec<_> = rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.ok_or_else(|| anyhow::anyhow!("worker returned no result for slot {i}")))
        .collect::<Result<_>>()?;
    let complete = rows.iter().filter(|r| r.metrics.is_some()).count();
    io::write(
        &output.join("index.html"),
        render::gallery(&rows, started.elapsed().as_secs_f64()).as_bytes(),
    )?;
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let report = Report {
        schema: "aic-layout-generation-report-v1",
        catalog_sha256: loaded.catalogs.identity,
        config,
        seed,
        workers,
        requested: count,
        complete,
        elapsed_seconds,
        complete_per_second: complete as f64 / elapsed_seconds,
        rows,
    };
    io::write_json(&output.join("report.json"), &report)?;
    Ok(report)
}
