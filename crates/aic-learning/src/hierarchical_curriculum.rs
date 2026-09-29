//! Recoverable fresh-production-graph curriculum for hierarchical generations.

use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};

use aic_torch::tch::Device;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    hierarchical_generation,
    hierarchical_search::{self, Panel, Report as SearchReport, SearchResult},
    input,
};

const CONFIG_SCHEMA: &str = "aic-hierarchical-curriculum-v1";
const BINDING_SCHEMA: &str = "aic-hierarchical-curriculum-binding-v1";
const STATE_SCHEMA: &str = "aic-hierarchical-curriculum-state-v3";
const SUMMARY_SCHEMA: &str = "aic-hierarchical-curriculum-generation-v2";
const REPORT_SCHEMA: &str = "aic-hierarchical-curriculum-report-v2";
const MAX_INPUT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub generations: usize,
    pub training_cases: usize,
    pub evaluation_interval: usize,
    pub success_window_cases: usize,
    pub advance_completion_rate: f64,
    pub seed: u64,
    pub namespace: String,
    pub max_turns: u64,
    pub max_extension: i64,
    pub levels: Vec<Level>,
    #[serde(default)]
    pub outer_success_replay_generations: usize,
    #[serde(default)]
    pub diagnostic_training_evaluation: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Level {
    pub min_facilities: usize,
    pub max_facilities: usize,
    pub max_terminals: usize,
    pub max_connections: usize,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Binding {
    schema: String,
    config_sha256: String,
    search_config_sha256: String,
    generation_config_sha256: String,
    sampling_sha256: String,
    heldout_panel_sha256: String,
    catalog_sha256: String,
    initial_lower_config_sha256: String,
    initial_lower_order_sha256: String,
    initial_lower_checkpoint_sha256: String,
    initial_outer_config_sha256: String,
    initial_outer_order_sha256: String,
    initial_outer_checkpoint_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct State {
    schema: String,
    binding_sha256: String,
    completed_generations: usize,
    level: usize,
    recent_training_success: VecDeque<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GenerationSummary {
    schema: String,
    generation: usize,
    level: usize,
    level_spec: Level,
    training_completed: usize,
    training_cases: usize,
    training_completion_rate: f64,
    training_mean_footprint_area: Option<f64>,
    training_mean_transport_cells: Option<f64>,
    training_mean_turns: Option<f64>,
    training_mcts_samples_per_minute: f64,
    recent_training_cases: usize,
    recent_training_completion_rate: f64,
    advanced: bool,
    next_level: usize,
    heldout_completed: Option<usize>,
    heldout_cases: Option<usize>,
    heldout_completion_rate: Option<f64>,
    heldout_mean_footprint_area: Option<f64>,
    heldout_mean_transport_cells: Option<f64>,
    heldout_mean_turns: Option<f64>,
    heldout_mcts_samples_per_minute: Option<f64>,
    lower_checkpoint_sha256: String,
    outer_checkpoint_sha256: String,
    recent_training_success_after: VecDeque<bool>,
    training_problem_identities: BTreeSet<String>,
}

pub fn run(
    config_path: &Path,
    search_config_path: &Path,
    generation_config_path: &Path,
    sampling_path: &Path,
    heldout_panel_path: &Path,
    initial_lower_output: &Path,
    initial_outer_output: &Path,
    catalog_root: &Path,
    output: &Path,
    device: Device,
    resume: bool,
) -> Result<Value> {
    let config_bytes = bounded_read(config_path)?;
    let search_config_bytes = bounded_read(search_config_path)?;
    let generation_config_bytes = bounded_read(generation_config_path)?;
    let config: Config = serde_json::from_slice(&config_bytes)?;
    let search_config: hierarchical_search::Config = serde_json::from_slice(&search_config_bytes)?;
    let generation_template: hierarchical_generation::Config =
        serde_json::from_slice(&generation_config_bytes)?;
    validate_config(&config)?;
    ensure!(
        !search_config.run_beam,
        "aic-learning.hierarchical-curriculum-search: normal curriculum collection requires run_beam=false"
    );
    ensure!(
        generation_template.seed <= i64::MAX as u64,
        "aic-learning.hierarchical-curriculum-generation-config: invalid seed"
    );
    let heldout = hierarchical_search::load_published_panel(heldout_panel_path)?;
    let evaluation_partition = if config.diagnostic_training_evaluation {
        "training"
    } else {
        "held-out"
    };
    ensure!(
        heldout.value.partition == evaluation_partition && !heldout.value.cases.is_empty(),
        "aic-learning.hierarchical-curriculum-heldout: expected a nonempty {evaluation_partition} evaluation panel"
    );
    let heldout_identities = problem_identities(&heldout.value)?;

    let loaded = aic_layout_gen::io::load_catalogs(catalog_root)?;
    let binding = Binding {
        schema: BINDING_SCHEMA.into(),
        config_sha256: bytes_sha256(&config_bytes),
        search_config_sha256: bytes_sha256(&search_config_bytes),
        generation_config_sha256: bytes_sha256(&generation_config_bytes),
        sampling_sha256: input::sha256(sampling_path)?,
        heldout_panel_sha256: heldout.sha256.clone(),
        catalog_sha256: loaded.catalogs.identity,
        initial_lower_config_sha256: input::sha256(&initial_lower_output.join("config.json"))?,
        initial_lower_order_sha256: input::sha256(
            &initial_lower_output.join("parameter-order.json"),
        )?,
        initial_lower_checkpoint_sha256: input::sha256(
            &initial_lower_output.join("state.safetensors"),
        )?,
        initial_outer_config_sha256: input::sha256(&initial_outer_output.join("config.json"))?,
        initial_outer_order_sha256: input::sha256(
            &initial_outer_output.join("parameter-order.json"),
        )?,
        initial_outer_checkpoint_sha256: input::sha256(
            &initial_outer_output.join("state.safetensors"),
        )?,
    };
    let binding_bytes = serde_json::to_vec(&binding)?;
    let binding_sha256 = bytes_sha256(&binding_bytes);
    let binding_path = output.join("binding.json");
    let state_path = output.join("state.json");

    let mut state = if resume {
        ensure!(
            output.is_dir(),
            "aic-learning.hierarchical-curriculum-resume: output does not exist"
        );
        let retained: Binding = read_json(&binding_path)?;
        ensure!(
            retained == binding,
            "aic-learning.hierarchical-curriculum-resume: stable input binding changed"
        );
        let state: State = read_json(&state_path)?;
        ensure!(
            state.schema == STATE_SCHEMA
                && state.binding_sha256 == binding_sha256
                && state.completed_generations <= config.generations
                && state.level < config.levels.len()
                && state.recent_training_success.len() <= config.success_window_cases,
            "aic-learning.hierarchical-curriculum-resume: invalid retained state"
        );
        state
    } else {
        ensure!(
            !output.exists(),
            "aic-learning.hierarchical-curriculum-output: {} already exists",
            output.display()
        );
        fs::create_dir_all(output.join("generations"))?;
        write_json_atomic(&binding_path, &binding)?;
        fs::write(output.join("config.json"), &config_bytes)?;
        let state = State {
            schema: STATE_SCHEMA.into(),
            binding_sha256: binding_sha256.clone(),
            completed_generations: 0,
            level: 0,
            recent_training_success: VecDeque::new(),
        };
        write_json_atomic(&state_path, &state)?;
        state
    };

    let mut replay =
        hierarchical_generation::ReplayWindow::new(config.outer_success_replay_generations);
    if config.outer_success_replay_generations > 0 {
        for previous in state
            .completed_generations
            .saturating_sub(config.outer_success_replay_generations)
            ..state.completed_generations
        {
            restore_replay_source(
                &mut replay,
                output,
                previous,
                initial_lower_output,
                initial_outer_output,
                catalog_root,
                &generation_template,
            )?;
        }
    }
    if resume {
        retire_inactive_replay_data(
            output,
            state.completed_generations,
            config.outer_success_replay_generations,
        )?;
    }

    while state.completed_generations < config.generations {
        let generation = state.completed_generations;
        let directory = output.join("generations").join(format!("{generation:06}"));
        fs::create_dir_all(&directory)?;
        let summary_path = directory.join("summary.json");
        if summary_path.exists() {
            let summary: GenerationSummary = read_json(&summary_path)?;
            ensure!(
                summary.schema == SUMMARY_SCHEMA
                    && summary.generation == generation
                    && summary.level == state.level,
                "aic-learning.hierarchical-curriculum-summary: retained generation differs"
            );
            restore_replay_source(
                &mut replay,
                output,
                generation,
                initial_lower_output,
                initial_outer_output,
                catalog_root,
                &generation_template,
            )?;
            state.completed_generations += 1;
            state.level = summary.next_level;
            state.recent_training_success = summary.recent_training_success_after;
            write_json_atomic(&state_path, &state)?;
            retire_newly_inactive_replay_data(
                output,
                state.completed_generations,
                config.outer_success_replay_generations,
            )?;
            continue;
        }

        let level = config.levels[state.level];
        let panel_path = directory.join("training-panel.json");
        let training_panel = if panel_path.exists() {
            hierarchical_search::load_published_panel(&panel_path)?
        } else {
            hierarchical_search::prepare_panel_published(
                sampling_path,
                catalog_root,
                &panel_path,
                "training",
                config.training_cases,
                config
                    .seed
                    .checked_add(generation as u64)
                    .context("aic-learning.hierarchical-curriculum-seed: overflow")?,
                &format!("{}-g{generation:06}", config.namespace),
                level.min_facilities,
                level.max_facilities,
                level.max_terminals,
                level.max_connections,
                config.max_turns,
                config.max_extension,
            )?
        };
        let training_identities = problem_identities(&training_panel.value)?;
        ensure!(
            training_panel.value.partition == "training"
                && training_identities.is_disjoint(&heldout_identities),
            "aic-learning.hierarchical-curriculum-separation: training problems overlap held-out"
        );

        let (source_lower, source_outer) = source_outputs(
            output,
            generation,
            initial_lower_output,
            initial_outer_output,
        );
        let collection_path = directory.join("collection.json");
        let training_report = if collection_path.exists() {
            hierarchical_search::load_published_report(&collection_path)?
        } else {
            hierarchical_search::run_training_published(
                search_config_path,
                &training_panel,
                &source_lower,
                &source_outer,
                catalog_root,
                &collection_path,
                device,
            )?
        };
        ensure!(
            training_report.value.schema == hierarchical_search::REPORT_SCHEMA
                && training_report.value.partition == "training"
                && training_report.value.root_records_retained
                && training_report.value.cases.len() == config.training_cases,
            "aic-learning.hierarchical-curriculum-collection: invalid training report"
        );
        ensure!(
            training_report.value.panel_sha256 == training_panel.sha256,
            "aic-learning.hierarchical-curriculum-collection-lineage: {} differs from {}",
            collection_path.display(),
            panel_path.display()
        );
        let training_success = training_report
            .value
            .cases
            .iter()
            .map(|case| case.mcts.fully_validated)
            .collect::<Vec<_>>();
        let training_mean_footprint_area =
            mean_completed_metric(&training_report.value, |result| result.footprint_area);
        let training_mean_transport_cells =
            mean_completed_metric(&training_report.value, |result| result.transport_cells);
        let training_mean_turns =
            mean_completed_metric(&training_report.value, |result| result.turns);
        let training_mcts_samples_per_minute = aggregate_throughput(&training_report.value);

        let mut generation_config = generation_template.clone();
        generation_config.seed = generation_template
            .seed
            .checked_add(generation as u64)
            .context("aic-learning.hierarchical-curriculum-generation-seed: overflow")?;
        let current_generation_config = directory.join("generation-config.json");
        if !current_generation_config.exists() {
            write_json_atomic(&current_generation_config, &generation_config)?;
        } else {
            let retained: hierarchical_generation::Config = read_json(&current_generation_config)?;
            ensure!(
                retained.seed == generation_config.seed,
                "aic-learning.hierarchical-curriculum-generation-config: retained seed differs"
            );
        }

        let training_output = directory.join("training");
        if !training_output.exists() {
            let pending = pending_path(&training_output)?;
            hierarchical_generation::run_published(
                &current_generation_config,
                training_panel,
                training_report,
                &collection_path,
                &source_lower,
                &source_outer,
                catalog_root,
                &training_output,
                device,
                pending.exists(),
                &mut replay,
            )?;
        } else {
            restore_replay_source(
                &mut replay,
                output,
                generation,
                initial_lower_output,
                initial_outer_output,
                catalog_root,
                &generation_template,
            )?;
        }

        let evaluate = (generation + 1) % config.evaluation_interval == 0
            || generation + 1 == config.generations;
        let heldout_path = directory.join("heldout.json");
        let heldout_report = if evaluate {
            Some(if heldout_path.exists() {
                hierarchical_search::load_published_report(&heldout_path)?
            } else {
                hierarchical_search::run_evaluation_published(
                    search_config_path,
                    &heldout,
                    &training_output.join("lower"),
                    &training_output.join("outer"),
                    catalog_root,
                    &heldout_path,
                    device,
                )?
            })
        } else {
            None
        };
        ensure!(
            heldout_report.as_ref().is_none_or(|report| {
                report.value.schema == hierarchical_search::REPORT_SCHEMA
                    && report.value.partition == evaluation_partition
                    && !report.value.root_records_retained
            }),
            "aic-learning.hierarchical-curriculum-heldout: invalid evaluation report"
        );
        if let Some(report) = &heldout_report {
            ensure!(
                report.value.panel_sha256 == heldout.sha256,
                "aic-learning.hierarchical-curriculum-heldout-lineage: {} differs from {}",
                heldout_path.display(),
                heldout_panel_path.display()
            );
        }

        for success in &training_success {
            state.recent_training_success.push_back(*success);
            if state.recent_training_success.len() > config.success_window_cases {
                state.recent_training_success.pop_front();
            }
        }
        let recent_rate = completion_rate(state.recent_training_success.iter().copied());
        let recent_cases = state.recent_training_success.len();
        let advanced = should_advance(
            &state.recent_training_success,
            config.success_window_cases,
            config.advance_completion_rate,
            state.level,
            config.levels.len(),
        );
        let next_level = state.level + usize::from(advanced);
        if advanced {
            state.recent_training_success.clear();
        }

        let training_completed = training_success.iter().filter(|success| **success).count();
        let heldout_completed = heldout_report.as_ref().map(|report| {
            report
                .value
                .cases
                .iter()
                .filter(|case| case.mcts.fully_validated)
                .count()
        });
        let summary = GenerationSummary {
            schema: SUMMARY_SCHEMA.into(),
            generation,
            level: state.level,
            level_spec: level,
            training_completed,
            training_cases: training_success.len(),
            training_completion_rate: completion_rate(training_success.iter().copied()),
            training_mean_footprint_area,
            training_mean_transport_cells,
            training_mean_turns,
            training_mcts_samples_per_minute,
            recent_training_cases: recent_cases,
            recent_training_completion_rate: recent_rate,
            advanced,
            next_level,
            heldout_completed,
            heldout_cases: heldout_report
                .as_ref()
                .map(|report| report.value.cases.len()),
            heldout_completion_rate: heldout_report.as_ref().map(|report| {
                completion_rate(
                    report
                        .value
                        .cases
                        .iter()
                        .map(|case| case.mcts.fully_validated),
                )
            }),
            heldout_mean_footprint_area: heldout_report.as_ref().and_then(|report| {
                mean_completed_metric(&report.value, |result| result.footprint_area)
            }),
            heldout_mean_transport_cells: heldout_report.as_ref().and_then(|report| {
                mean_completed_metric(&report.value, |result| result.transport_cells)
            }),
            heldout_mean_turns: heldout_report
                .as_ref()
                .and_then(|report| mean_completed_metric(&report.value, |result| result.turns)),
            heldout_mcts_samples_per_minute: heldout_report
                .as_ref()
                .map(|report| aggregate_throughput(&report.value)),
            lower_checkpoint_sha256: input::sha256(
                &training_output.join("lower/state.safetensors"),
            )?,
            outer_checkpoint_sha256: input::sha256(
                &training_output.join("outer/state.safetensors"),
            )?,
            recent_training_success_after: state.recent_training_success.clone(),
            training_problem_identities: training_identities.clone(),
        };
        write_json_atomic(&summary_path, &summary)?;
        state.completed_generations += 1;
        state.level = next_level;
        write_json_atomic(&state_path, &state)?;
        write_report(output, &config, &state)?;
        retire_newly_inactive_replay_data(
            output,
            state.completed_generations,
            config.outer_success_replay_generations,
        )?;
    }

    write_report(output, &config, &state)?;
    let summaries = read_summaries(output, state.completed_generations)?;
    Ok(json!({
        "schema": REPORT_SCHEMA,
        "status": "complete",
        "output": output,
        "completed_generations": state.completed_generations,
        "level": state.level,
        "latest": summaries.last(),
    }))
}

fn validate_config(config: &Config) -> Result<()> {
    ensure!(
        config.schema == CONFIG_SCHEMA
            && (1..=1_000_000).contains(&config.generations)
            && (1..=4096).contains(&config.training_cases)
            && config
                .generations
                .checked_mul(config.training_cases)
                .is_some_and(|cases| cases <= 500_000)
            && (1..=1_000_000).contains(&config.evaluation_interval)
            && config.success_window_cases >= config.training_cases
            && config.success_window_cases <= 1_000_000
            && config.success_window_cases % config.training_cases == 0
            && config.advance_completion_rate.is_finite()
            && (0.0..=1.0).contains(&config.advance_completion_rate)
            && config.seed <= i64::MAX as u64
            && !config.namespace.is_empty()
            && config.max_turns > 0
            && config.max_extension > 0
            && !config.levels.is_empty()
            && config.levels.len() <= 1024
            && config.outer_success_replay_generations <= 4,
        "aic-learning.hierarchical-curriculum-config: invalid controls"
    );
    for (index, level) in config.levels.iter().enumerate() {
        ensure!(
            level.min_facilities > 0
                && level.min_facilities <= level.max_facilities
                && level.max_terminals > 0
                && level.max_connections > 0
                && (index == 0 || config.levels[index - 1].min_facilities <= level.min_facilities)
                && (index == 0 || config.levels[index - 1].max_facilities <= level.max_facilities)
                && (index == 0 || config.levels[index - 1].max_terminals <= level.max_terminals)
                && (index == 0
                    || config.levels[index - 1].max_connections <= level.max_connections),
            "aic-learning.hierarchical-curriculum-level: levels must be valid and monotone"
        );
    }
    Ok(())
}

fn source_outputs(
    output: &Path,
    generation: usize,
    initial_lower: &Path,
    initial_outer: &Path,
) -> (PathBuf, PathBuf) {
    if generation == 0 {
        (initial_lower.into(), initial_outer.into())
    } else {
        let previous = output
            .join("generations")
            .join(format!("{:06}", generation - 1))
            .join("training");
        (previous.join("lower"), previous.join("outer"))
    }
}

#[allow(clippy::too_many_arguments)]
fn restore_replay_source(
    replay: &mut hierarchical_generation::ReplayWindow,
    output: &Path,
    generation: usize,
    initial_lower: &Path,
    initial_outer: &Path,
    catalog_root: &Path,
    generation_config: &hierarchical_generation::Config,
) -> Result<()> {
    let directory = output.join("generations").join(format!("{generation:06}"));
    let (lower, outer) = source_outputs(output, generation, initial_lower, initial_outer);
    replay.restore_source(
        &directory.join("training-panel.json"),
        &directory.join("collection.json"),
        &lower,
        &outer,
        catalog_root,
        generation_config,
    )
}

fn retire_newly_inactive_replay_data(
    output: &Path,
    completed: usize,
    replay_capacity: usize,
) -> Result<()> {
    if let Some(generation) = completed.checked_sub(replay_capacity + 1) {
        let collection_path = output
            .join("generations")
            .join(format!("{generation:06}"))
            .join("collection.json");
        hierarchical_search::retire_replay_data(&collection_path)?;
    }
    Ok(())
}

fn retire_inactive_replay_data(
    output: &Path,
    completed: usize,
    replay_capacity: usize,
) -> Result<()> {
    for generation in 0..completed.saturating_sub(replay_capacity) {
        let collection_path = output
            .join("generations")
            .join(format!("{generation:06}"))
            .join("collection.json");
        hierarchical_search::retire_replay_data(&collection_path)?;
    }
    Ok(())
}

fn problem_identities(panel: &Panel) -> Result<BTreeSet<String>> {
    panel
        .cases
        .iter()
        .map(|case| serde_json::to_vec(&case.problem).map(|bytes| bytes_sha256(&bytes)))
        .collect::<Result<_, _>>()
        .map_err(Into::into)
}

fn completion_rate(values: impl Iterator<Item = bool>) -> f64 {
    let (completed, total) = values.fold((0usize, 0usize), |(completed, total), value| {
        (completed + usize::from(value), total + 1)
    });
    completed as f64 / total.max(1) as f64
}

fn mean_completed_metric(
    report: &SearchReport,
    metric: impl Fn(&SearchResult) -> Option<u64>,
) -> Option<f64> {
    let values = report
        .cases
        .iter()
        .filter(|case| case.mcts.fully_validated)
        .filter_map(|case| metric(&case.mcts))
        .collect::<Vec<_>>();
    (!values.is_empty())
        .then(|| values.iter().map(|value| *value as f64).sum::<f64>() / values.len() as f64)
}

fn should_advance(
    recent: &VecDeque<bool>,
    window: usize,
    threshold: f64,
    level: usize,
    levels: usize,
) -> bool {
    recent.len() == window
        && completion_rate(recent.iter().copied()) >= threshold
        && level + 1 < levels
}

fn aggregate_throughput(report: &SearchReport) -> f64 {
    let (samples, seconds) =
        report
            .cases
            .iter()
            .fold((0usize, 0.0f64), |(samples, seconds), case| {
                (
                    samples + case.mcts.counters.generated_macro_samples,
                    seconds + case.mcts.elapsed_seconds,
                )
            });
    if seconds == 0.0 {
        0.0
    } else {
        samples as f64 * 60.0 / seconds
    }
}

fn write_report(output: &Path, config: &Config, state: &State) -> Result<()> {
    let summaries = read_summaries(output, state.completed_generations)?;
    write_json_atomic(
        &output.join("report.json"),
        &json!({
            "schema": REPORT_SCHEMA,
            "status": if state.completed_generations == config.generations { "complete" } else { "running" },
            "completed_generations": state.completed_generations,
            "configured_generations": config.generations,
            "level": state.level,
            "summaries": summaries,
        }),
    )
}

fn read_summaries(output: &Path, completed: usize) -> Result<Vec<GenerationSummary>> {
    (0..completed)
        .map(|generation| {
            read_json(
                &output
                    .join("generations")
                    .join(format!("{generation:06}"))
                    .join("summary.json"),
            )
        })
        .collect()
}

fn pending_path(output: &Path) -> Result<PathBuf> {
    let name = output
        .file_name()
        .context("aic-learning.hierarchical-curriculum-output: missing output name")?
        .to_string_lossy();
    Ok(output.with_file_name(format!(".{name}.pending")))
}

fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    ensure!(
        metadata.len() <= MAX_INPUT_BYTES,
        "aic-learning.hierarchical-curriculum-input: {} exceeds {} bytes",
        path.display(),
        MAX_INPUT_BYTES
    );
    fs::read(path).with_context(|| format!("read {}", path.display()))
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&bounded_read(path)?)?)
}

fn bytes_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let pending = path.with_extension("json.pending");
    fs::write(&pending, serde_json::to_vec_pretty(value)?)?;
    fs::rename(pending, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_training_window_advances_only_after_mastery() {
        let mut recent = VecDeque::new();
        for success in [true, true, false] {
            recent.push_back(success);
        }
        assert!(!should_advance(&recent, 4, 0.75, 0, 3));
        recent.push_back(true);
        assert_eq!(completion_rate(recent.iter().copied()), 0.75);
        assert!(should_advance(&recent, 4, 0.75, 0, 3));
        assert!(!should_advance(&recent, 4, 0.75, 2, 3));
        recent[2] = false;
        recent[3] = false;
        assert!(!should_advance(&recent, 4, 0.75, 0, 3));
        recent.clear();
        assert!(recent.is_empty());
    }

    #[test]
    fn generation_pending_path_matches_generation_trainer() {
        let output = Path::new("artifacts/curriculum/generations/000001/training");
        assert_eq!(
            pending_path(output).unwrap(),
            Path::new("artifacts/curriculum/generations/000001/.training.pending")
        );
    }
}
