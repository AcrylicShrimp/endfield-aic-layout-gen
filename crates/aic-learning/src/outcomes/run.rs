use super::{
    config::{Config, RootSlot, Split},
    sample::{SelectedAction, choose},
    storage::{
        ChunkManifest, DATASET_SCHEMA, DatasetManifest, RootManifest, Row, SPLIT_SCHEMA,
        SplitManifest, Stage, hash, load,
    },
};
use aic_layout::{editor::compiled_identity_until, rng::Rng};
use aic_layout_gen::Generator;
use aic_pilot_native::physical_editor::{
    ProceduralAdmission,
    learning::Observation,
    pool::{PhysicalCommand, PhysicalPool, PhysicalReply},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

struct Admitted {
    root: usize,
    admission: ProceduralAdmission,
}

struct Active {
    root: usize,
    observation: Observation,
    rng: Rng,
    rows: Vec<Row>,
    finished: bool,
    closable: bool,
}

struct State {
    roots: Vec<RootManifest>,
    chunks: Vec<ChunkManifest>,
    coverage: BTreeMap<String, u64>,
    classes: BTreeMap<String, [u64; 2]>,
    admitted: usize,
    collected: usize,
    rows: usize,
    status: String,
    failure: Option<String>,
}

pub fn collect(config_path: &Path, catalog_root: &Path, output: &Path) -> Result<Value> {
    let started = Instant::now();
    let config_bytes = bounded_read(config_path)?;
    let config: Config = serde_json::from_slice(&config_bytes)
        .with_context(|| format!("parse {}", config_path.display()))?;
    config.validate()?;
    let deadline = started
        .checked_add(Duration::from_millis(config.max_elapsed_ms))
        .ok_or_else(|| anyhow::anyhow!("aic-learning.outcomes-deadline: deadline overflow"))?;
    let loaded = aic_layout_gen::io::load_catalogs(catalog_root)?;
    let catalogs = Arc::new(loaded.catalogs);
    let config_sha256 = hash(&config_bytes);
    let catalog_sha256 = catalogs.identity.clone();
    let mut stage = Stage::new(
        output,
        config.max_output_bytes,
        config.metadata_reserve_bytes,
    )?;
    stage.write_config(&config_bytes)?;
    let mut state = State {
        roots: config
            .roots
            .iter()
            .cloned()
            .map(|slot| RootManifest {
                slot,
                status: "configured".into(),
                source_file: None,
                source_sha256: None,
                structural_identity: None,
                exact_identity: None,
                draft_sha256: None,
                identity_work: None,
                diagnostic: None,
                rows: 0,
            })
            .collect(),
        chunks: vec![],
        coverage: BTreeMap::new(),
        classes: BTreeMap::new(),
        admitted: 0,
        collected: 0,
        rows: 0,
        status: "collecting".into(),
        failure: None,
    };

    let result = execute(
        &config,
        &config_sha256,
        &catalog_sha256,
        catalogs,
        deadline,
        &mut stage,
        &mut state,
    );
    if let Err(error) = result {
        state.status = "failed".into();
        state.failure = Some(format!("{error:#}"));
        if !stage.root().join("split-manifest.json").exists() {
            if let Err(split_error) = stage.write_split(&SplitManifest {
                schema: SPLIT_SCHEMA.into(),
                config_sha256: config_sha256.clone(),
                catalog_sha256: catalog_sha256.clone(),
                roots: state.roots.clone(),
            }) {
                state.failure = Some(format!(
                    "{}; retaining staging failed to write split manifest: {split_error:#}",
                    state.failure.as_deref().unwrap_or("collection failed")
                ));
            }
        }
    } else if state.status == "collecting" {
        state.status = if state.roots.iter().all(|root| root.status == "collected") {
            "complete".into()
        } else {
            "incomplete_admission".into()
        };
    }

    let elapsed_ms = millis(started.elapsed());
    let mut single_class = Vec::new();
    for blocks in [8, 16] {
        for split in [Split::Training, Split::Development, Split::Confirmation] {
            for family in 0..3 {
                let stratum = format!("size={blocks}/split={split:?}/family={family}");
                let counts = state.classes.get(&stratum).copied().unwrap_or_default();
                if counts[0] == 0 || counts[1] == 0 {
                    single_class.push(
                        json!({"stratum":stratum,"incomplete":counts[0],"complete":counts[1]}),
                    );
                }
            }
        }
    }
    let report = json!({
        "schema":"aic-physical-outcome-collection-report-v1",
        "status":state.status,
        "elapsed_ms":elapsed_ms,
        "configured_roots":state.roots.len(),
        "admitted_roots":state.admitted,
        "collected_roots":state.collected,
        "rows":state.rows,
        "coverage":state.coverage,
        "single_class_strata":single_class,
        "accuracy_claim_allowed":single_class.is_empty(),
        "failure":state.failure,
    });
    let mut manifest = DatasetManifest {
        schema: DATASET_SCHEMA.into(),
        status: state.status,
        config_sha256,
        config_file: "config.json".into(),
        catalog_sha256,
        elapsed_ms,
        configured_roots: state.roots.len(),
        admitted_roots: state.admitted,
        collected_roots: state.collected,
        rows: state.rows,
        output_bytes: 0,
        roots: state.roots,
        chunks: state.chunks,
        coverage: state.coverage,
        failure: state.failure,
    };
    let published = stage.publish(&mut manifest, &report)?;
    ensure!(
        manifest.output_bytes == directory_bytes(&published)?,
        "aic-learning.outcomes-publication: published aggregate byte count failed"
    );
    Ok(
        json!({"manifest":published.join("manifest.json"),"report":published.join("report.json"),
        "status":manifest.status,"configured_roots":manifest.configured_roots,
        "admitted_roots":manifest.admitted_roots,"collected_roots":manifest.collected_roots,
        "rows":manifest.rows,"output_bytes":manifest.output_bytes}),
    )
}

pub fn validate(path: &Path) -> Result<Value> {
    let (manifest, rows) = load(path)?;
    let bytes = directory_bytes(path)?;
    ensure!(
        bytes == manifest.output_bytes,
        "aic-learning.outcomes-load: aggregate output byte count mismatch"
    );
    let mut expected_steps = BTreeMap::<&str, u64>::new();
    let mut route_work = BTreeMap::<&str, usize>::new();
    let mut previous = BTreeMap::<&str, (&str, Option<usize>, Option<usize>)>::new();
    let mut recovered_pending = 0u64;
    for row in &rows {
        let root = manifest
            .roots
            .iter()
            .find(|root| root.slot.root_id == row.root_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "aic-learning.outcomes-load: unknown row root {}",
                    row.root_id
                )
            })?;
        let step = expected_steps.entry(&row.root_id).or_default();
        ensure!(
            row.step == *step && row.outcome.attempts == row.step + 1 && row.outcome.charged,
            "aic-learning.outcomes-load: nonsequential or uncharged row for {}",
            row.root_id
        );
        *step += 1;
        ensure!(
            row.selected.family < 3
                && row.selected.root < row.pre_action.root_meta.len()
                && row.pre_action.root_meta[row.selected.root][0] == row.selected.family as i64
                && !row.pre_action.root_invalid[row.selected.root],
            "aic-learning.outcomes-load: selected root binding failed for {}",
            row.root_id
        );
        if row.selected.family == 0 {
            ensure!(
                row.selected.ports.len() == row.selected.assignments.len(),
                "aic-learning.outcomes-load: incomplete placement assignment for {}",
                row.root_id
            );
            let terminals = row
                .selected
                .assignments
                .iter()
                .map(|assignment| assignment.terminal)
                .collect::<std::collections::BTreeSet<_>>();
            ensure!(
                terminals.len() == row.selected.assignments.len(),
                "aic-learning.outcomes-load: duplicate placement terminal for {}",
                row.root_id
            );
        } else {
            ensure!(
                row.selected.ports.is_empty() && row.selected.assignments.is_empty(),
                "aic-learning.outcomes-load: non-placement action has port assignment"
            );
        }
        ensure!(
            row.outcome.work.expanded <= 50_000,
            "aic-learning.outcomes-load: per-action route work exceeded for {}",
            row.root_id
        );
        let total = route_work.entry(&row.root_id).or_default();
        *total = total.saturating_add(row.outcome.work.expanded);
        ensure!(
            *total <= 1_600_000,
            "aic-learning.outcomes-load: cumulative route work exceeded for {}",
            row.root_id
        );
        let changed = matches!(
            row.outcome.status.as_str(),
            "draft_accepted" | "validated" | "improved"
        );
        if changed && !row.outcome.censored {
            ensure!(
                row.outcome.current_pending.is_some() && row.outcome.current_conflicts.is_some(),
                "aic-learning.outcomes-load: changed row lacks observed current state"
            );
        }
        if matches!(row.outcome.status.as_str(), "validated" | "improved") {
            ensure!(
                row.outcome.current_metrics.is_some(),
                "aic-learning.outcomes-load: completed row lacks current metrics"
            );
        }
        if changed && !row.outcome.censored {
            ensure!(
                row.outcome.route_budget_exhausted.is_some(),
                "aic-learning.outcomes-load: changed row lacks route exhaustion label"
            );
        } else {
            ensure!(
                row.outcome.route_budget_exhausted.is_none(),
                "aic-learning.outcomes-load: unobserved row fabricates route exhaustion label"
            );
        }
        if matches!(row.outcome.status.as_str(), "unchanged" | "rejected") {
            ensure!(
                row.outcome.current_pending.is_none()
                    && row.outcome.current_conflicts.is_none()
                    && row.outcome.current_metrics.is_none(),
                "aic-learning.outcomes-load: unobserved action fabricates post-state labels"
            );
        }
        if let Some((status, pending, conflicts)) = previous.get(row.root_id.as_str()) {
            if matches!(*status, "draft_accepted" | "validated" | "improved") {
                ensure!(
                    Some(row.pre_action.pending) == *pending
                        && Some(row.pre_action.conflicts) == *conflicts,
                    "aic-learning.outcomes-load: next pre-action state does not continue prior outcome"
                );
                recovered_pending += u64::from(row.pre_action.pending > 0);
            }
        }
        previous.insert(
            &row.root_id,
            (
                row.outcome.status.as_str(),
                row.outcome.current_pending,
                row.outcome.current_conflicts,
            ),
        );
        ensure!(
            *step as usize <= root.rows,
            "aic-learning.outcomes-load: row exceeds root manifest count"
        );
    }
    for root in &manifest.roots {
        ensure!(
            expected_steps
                .get(root.slot.root_id.as_str())
                .copied()
                .unwrap_or(0) as usize
                == root.rows,
            "aic-learning.outcomes-load: root row count mismatch for {}",
            root.slot.root_id
        );
    }
    Ok(json!({
        "schema":"aic-physical-outcome-validation-v1",
        "dataset":path,
        "status":manifest.status,
        "configured_roots":manifest.configured_roots,
        "admitted_roots":manifest.admitted_roots,
        "collected_roots":manifest.collected_roots,
        "rows":rows.len(),
        "output_bytes":bytes,
        "recovered_pending_pre_actions":recovered_pending,
        "validated":true,
    }))
}

fn execute(
    config: &Config,
    config_sha256: &str,
    catalog_sha256: &str,
    catalogs: Arc<aic_layout::validation::Catalogs>,
    deadline: Instant,
    stage: &mut Stage,
    state: &mut State,
) -> Result<()> {
    let mut admitted = Vec::new();
    let mut structural = BTreeMap::<String, Split>::new();
    let mut ancestry = BTreeMap::<String, Split>::new();
    let mut output_limited = false;
    for (index, slot) in config.roots.iter().enumerate() {
        if output_limited || Instant::now() >= deadline {
            state.roots[index].status = if output_limited {
                "not_started_output_limit".into()
            } else {
                "not_started_deadline".into()
            };
            continue;
        }
        let mut generator_config = config.generator.clone();
        generator_config.blocks = [slot.blocks, slot.blocks];
        let generator = Generator::new(&catalogs, generator_config)
            .map_err(|failure| anyhow::anyhow!(serde_json::to_string(&failure).unwrap()))?;
        let generated =
            match generator.generate_until(slot.generation_seed, || Instant::now() >= deadline) {
                Ok(generated) => generated,
                Err(failure) => {
                    state.roots[index].status =
                        if failure.diagnostic.code.ends_with("generation-interrupted") {
                            "generation_interrupted".into()
                        } else {
                            "generation_failed".into()
                        };
                    state.roots[index].diagnostic = Some(serde_json::to_value(failure)?);
                    continue;
                }
            };
        if Instant::now() >= deadline {
            state.roots[index].status = "generation_late".into();
            continue;
        }
        let source = serde_json::to_vec(&generated.layout)?;
        let Some((source_file, source_sha256)) = stage.write_source(&slot.root_id, &source)? else {
            state.roots[index].status = "output_limited".into();
            output_limited = true;
            continue;
        };
        state.roots[index].source_file = Some(source_file);
        state.roots[index].source_sha256 = Some(source_sha256);
        let admission = match ProceduralAdmission::compile(&source, &catalogs) {
            Ok(admission) => admission,
            Err(failure) => {
                state.roots[index].status = "admission_failed".into();
                state.roots[index].diagnostic = Some(serde_json::to_value(failure)?);
                continue;
            }
        };
        if Instant::now() >= deadline {
            state.roots[index].status = "admission_late".into();
            continue;
        }
        let (identity, identity_work) = match compiled_identity_until(
            &admission.problem,
            &admission.draft,
            &config.identity,
            || Instant::now() >= deadline,
        ) {
            Ok(identity) => identity,
            Err(failure) => {
                state.roots[index].status = "identity_failed".into();
                state.roots[index].diagnostic = Some(json!({"message":failure.to_string()}));
                continue;
            }
        };
        if Instant::now() >= deadline {
            state.roots[index].status = "identity_late".into();
            continue;
        }
        state.roots[index].structural_identity = Some(identity.physical.structural.clone());
        state.roots[index].exact_identity = Some(identity.physical.exact_problem.clone());
        state.roots[index].draft_sha256 = Some(identity.draft_sha256);
        state.roots[index].identity_work = Some(identity_work);
        let structural_split = structural.get(&identity.physical.structural).copied();
        let ancestry_split = ancestry.get(&slot.ancestry).copied();
        if !reserve_partition(
            &mut structural,
            &mut ancestry,
            &identity.physical.structural,
            &slot.ancestry,
            slot.split,
        ) {
            state.roots[index].status = "cross_split_rejected".into();
            state.roots[index].diagnostic = Some(json!({
                "code":"aic-learning.outcomes-cross-split",
                "structural_split":structural_split,
                "ancestry_split":ancestry_split,
            }));
            continue;
        }
        state.roots[index].status = "admitted".into();
        admitted.push(Admitted {
            root: index,
            admission,
        });
    }
    state.admitted = admitted.len();
    stage.write_split(&SplitManifest {
        schema: SPLIT_SCHEMA.into(),
        config_sha256: config_sha256.into(),
        catalog_sha256: catalog_sha256.into(),
        roots: state.roots.clone(),
    })?;
    if Instant::now() >= deadline {
        state.status = "incomplete_deadline".into();
        return Ok(());
    }
    if output_limited {
        state.status = "incomplete_output".into();
        return Ok(());
    }
    if admitted.is_empty() {
        state.status = "incomplete_admission".into();
        return Ok(());
    }

    let mut pool =
        PhysicalPool::new(catalogs, config.workers, config.workers).map_err(anyhow::Error::msg)?;
    let mut queue: VecDeque<_> = admitted.into();
    while !queue.is_empty() {
        if Instant::now() >= deadline {
            state.status = "incomplete_deadline".into();
            break;
        }
        let mut batch = Vec::new();
        for stream in 0..config.workers {
            let Some(item) = queue.pop_front() else { break };
            batch.push((stream, item));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            state.status = "incomplete_deadline".into();
            break;
        }
        let open: Vec<_> = batch
            .into_iter()
            .map(|(stream, item)| {
                let mut limits = config.session.clone();
                limits.elapsed_ms = limits.elapsed_ms.min(
                    u64::try_from(remaining.as_millis())
                        .unwrap_or(u64::MAX)
                        .max(1),
                );
                (
                    stream,
                    PhysicalCommand::Open {
                        admission: item.admission,
                        limits,
                        options: config.candidates.clone(),
                    },
                    item.root,
                )
            })
            .collect();
        let roots: BTreeMap<_, _> = open
            .iter()
            .map(|(stream, _, root)| (*stream, *root))
            .collect();
        expect_ready(
            pool.exchange(
                open.into_iter()
                    .map(|(stream, command, _)| (stream, command))
                    .collect(),
            )
            .map_err(anyhow::Error::msg)?,
        )?;
        let observations = pool
            .exchange(
                roots
                    .keys()
                    .map(|&stream| (stream, PhysicalCommand::Observe))
                    .collect(),
            )
            .map_err(anyhow::Error::msg)?;
        let mut active = BTreeMap::new();
        for (stream, reply) in observations {
            let PhysicalReply::Observation(observation) = reply else {
                anyhow::bail!("aic-learning.outcomes-protocol: expected observation")
            };
            let root = roots[&stream];
            active.insert(
                stream,
                Active {
                    root,
                    observation,
                    rng: Rng::new(config.roots[root].action_seed),
                    rows: Vec::with_capacity(config.session.max_actions as usize),
                    finished: false,
                    closable: false,
                },
            );
        }

        while active.values().any(|root| !root.finished) {
            if Instant::now() >= deadline {
                state.status = "incomplete_deadline".into();
                break;
            }
            let mut pending = BTreeMap::<
                usize,
                (
                    SelectedAction,
                    aic_pilot_native::physical_editor::learning::Input,
                ),
            >::new();
            let mut commands = Vec::new();
            for (&stream, root) in active.iter_mut().filter(|(_, root)| !root.finished) {
                if root.observation.done || root.rows.len() >= config.session.max_actions as usize {
                    root.finished = true;
                    root.closable = root.observation.done;
                    continue;
                }
                let selected = choose(&root.observation, &mut root.rng)?;
                let input = root.observation.input.as_ref().clone();
                commands.push((
                    stream,
                    PhysicalCommand::Step {
                        source_sha256: root.observation.source_sha256.clone(),
                        revision: root.observation.revision,
                        handle: root.observation.handle,
                        root: selected.root,
                        ports: selected.ports.clone(),
                    },
                ));
                pending.insert(stream, (selected, input));
            }
            if commands.is_empty() {
                continue;
            }
            let replies = pool.exchange(commands).map_err(anyhow::Error::msg)?;
            for (stream, reply) in replies {
                let PhysicalReply::Step { outcome, next } = reply else {
                    anyhow::bail!("aic-learning.outcomes-protocol: expected step")
                };
                let (selected, pre_action) = pending.remove(&stream).unwrap();
                let charged = outcome.charged;
                let done = outcome.done;
                let observed = super::storage::observed_physical_outcome(
                    outcome,
                    config.session.route_expansions,
                );
                if charged {
                    let active_root = &mut active.get_mut(&stream).unwrap();
                    active_root.rows.push(Row {
                        root_id: config.roots[active_root.root].root_id.clone(),
                        step: active_root.rows.len() as u64,
                        pre_action,
                        selected,
                        outcome: observed,
                    });
                }
                let active_root = active.get_mut(&stream).unwrap();
                match next {
                    Ok(next) => active_root.observation = next,
                    Err(failure) => anyhow::bail!(serde_json::to_string(&failure)?),
                }
                if !charged || done || active_root.rows.len() >= config.session.max_actions as usize
                {
                    active_root.finished = true;
                    active_root.closable = done;
                }
            }
        }

        let timed_out = state.status == "incomplete_deadline";
        let close: Vec<_> = active
            .iter()
            .filter_map(|(&stream, root)| root.closable.then_some((stream, PhysicalCommand::Close)))
            .collect();
        if !close.is_empty() {
            expect_closed(pool.exchange(close).map_err(anyhow::Error::msg)?)?;
        }
        for (_, root) in active {
            let intended = root.rows.len();
            let chunk = stage.write_chunk(&config.roots[root.root].root_id, &root.rows)?;
            let written = chunk.as_ref().map_or(0, |chunk| chunk.rows);
            for row in root.rows.iter().take(written) {
                account(row, &config.roots[root.root], state);
            }
            state.roots[root.root].rows = written;
            if let Some(chunk) = chunk {
                state.chunks.push(chunk);
            }
            if written < intended {
                state.roots[root.root].status = "size_limited".into();
                state.status = "incomplete_output".into();
            } else if timed_out {
                state.roots[root.root].status = "time_limited".into();
            } else {
                state.roots[root.root].status = "collected".into();
                state.collected += 1;
            }
        }
        if timed_out || state.status == "incomplete_output" {
            break;
        }
    }
    for item in queue {
        state.roots[item.root].status = if state.status == "incomplete_output" {
            "not_started_output_limit".into()
        } else {
            "not_started_deadline".into()
        };
    }
    Ok(())
}

pub(super) fn reserve_partition(
    structural: &mut BTreeMap<String, Split>,
    ancestry: &mut BTreeMap<String, Split>,
    structural_key: &str,
    ancestry_key: &str,
    split: Split,
) -> bool {
    if structural
        .get(structural_key)
        .is_some_and(|reserved| *reserved != split)
        || ancestry
            .get(ancestry_key)
            .is_some_and(|reserved| *reserved != split)
    {
        return false;
    }
    structural.insert(structural_key.into(), split);
    ancestry.insert(ancestry_key.into(), split);
    true
}

fn expect_ready(replies: Vec<(usize, PhysicalReply)>) -> Result<()> {
    ensure!(
        replies
            .into_iter()
            .all(|(_, reply)| matches!(reply, PhysicalReply::Ready)),
        "aic-learning.outcomes-protocol: expected ready replies"
    );
    Ok(())
}

fn expect_closed(replies: Vec<(usize, PhysicalReply)>) -> Result<()> {
    ensure!(
        replies
            .into_iter()
            .all(|(_, reply)| matches!(reply, PhysicalReply::Closed)),
        "aic-learning.outcomes-protocol: expected closed replies"
    );
    Ok(())
}

fn account(row: &Row, slot: &RootSlot, state: &mut State) {
    state.rows += 1;
    let prefix = format!(
        "size={}/split={:?}/family={}",
        slot.blocks, slot.split, row.selected.family
    );
    *state
        .coverage
        .entry(format!("{prefix}/status={}", row.outcome.status))
        .or_default() += 1;
    if !row.outcome.censored
        && matches!(
            row.outcome.status.as_str(),
            "draft_accepted" | "validated" | "improved"
        )
    {
        let counts = state.classes.entry(prefix).or_default();
        counts[usize::from(matches!(
            row.outcome.status.as_str(),
            "validated" | "improved"
        ))] += 1;
    }
}

fn bounded_read(path: &Path) -> Result<Vec<u8>> {
    let bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "aic-learning.outcomes-config: config exceeds 16 MiB"
    );
    Ok(bytes)
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn directory_bytes(path: &Path) -> Result<u64> {
    let mut total = 0u64;
    let mut pending = vec![path.to_owned()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.checked_add(metadata.len()).ok_or_else(|| {
                    anyhow::anyhow!("aic-learning.outcomes-output: byte count overflow")
                })?;
            }
        }
    }
    Ok(total)
}
