//! Bounded, source-order outer scoring with one encoded root state.

use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, ErrorKind, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use aic_pilot_native::constructive::learning::Input;
use aic_torch::tch::Device;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};

use super::{
    ACTION_FEATURES, AREA_HEAD, AllowanceStop, Bounds, COMPLETION_HEAD, OuterRunner,
    SearchMemoryBudget, TRANSPORT_HEAD, TURNS_HEAD, TreeReservation, lexicographic_quality_penalty,
    placement_host_stop, predicted_quality_value,
};

const RAW_RECORD_BYTES: usize = std::mem::size_of::<f32>() + std::mem::size_of::<f64>();
const NOISE_RECORD_BYTES: usize = RAW_RECORD_BYTES + std::mem::size_of::<u64>();

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum RootNoiseMode {
    Rank,
    ActionIdentity,
}

impl RootNoiseMode {
    pub(super) fn record_bytes(self) -> usize {
        match self {
            Self::Rank => RAW_RECORD_BYTES,
            Self::ActionIdentity => NOISE_RECORD_BYTES,
        }
    }
}

pub(super) enum RootScoreStart<'a> {
    Ready(RootScorePass<'a>),
    MemoryStop(AllowanceStop),
    DiskStop(RootScoreDiskStop),
}

#[derive(Debug)]
pub(super) struct RootScoreDiskStop {
    pub(super) path: PathBuf,
    pub(super) operation: &'static str,
    pub(super) source: std::io::Error,
}

pub(super) enum RootScorePageStop {
    Memory(AllowanceStop),
    Disk(RootScoreDiskStop),
}

pub(super) struct RootScorePass<'a> {
    noise_mode: RootNoiseMode,
    runner: Option<&'a OuterRunner<'a>>,
    encoded: Option<crate::outer_model::EncodedOuterState>,
    device: Device,
    path: Option<PathBuf>,
    file: Option<File>,
    count: usize,
    model_max: f32,
    anchor_max: f64,
    anchor_mass: f64,
    model_value: f64,
    failed: bool,
    #[cfg(test)]
    fail_raw_after_bytes: Option<usize>,
    _graph_memory: Option<TreeReservation>,
    io_memory: Option<TreeReservation>,
}

pub(super) struct SourceOrderPriorReader {
    noise_mode: RootNoiseMode,
    path: PathBuf,
    file: Option<BufReader<File>>,
    count: usize,
    offset: usize,
    model_max: f32,
    model_log_denominator: f32,
    anchor_max: f64,
    anchor_denominator: f64,
    anchor_mass: f64,
    model_present: bool,
    _io_memory: TreeReservation,
}

pub(super) struct PriorPage {
    pub(super) priors: Vec<(f64, f64)>,
    pub(super) noise_bits: Option<Vec<u64>>,
    pub(super) next_offset: usize,
    _memory: TreeReservation,
}

impl<'a> RootScorePass<'a> {
    pub(super) fn mode(&self) -> RootNoiseMode {
        self.noise_mode
    }

    pub(super) fn count(&self) -> usize {
        self.count
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        runner: Option<&'a OuterRunner<'a>>,
        input: Option<&Input>,
        device: Device,
        bounds: Bounds,
        connections: usize,
        anchor_mass: f64,
        fallback_value: f64,
        path: &Path,
        budget: &SearchMemoryBudget,
    ) -> Result<RootScoreStart<'a>> {
        ensure!(
            anchor_mass.is_finite() && (0.0..=1.0).contains(&anchor_mass),
            "aic-learning.root-score-anchor-mass: invalid mixture"
        );
        let (encoded, graph_memory, model_value) = if let Some(runner) = runner {
            let input = input.context("aic-learning.root-score-input: missing model graph")?;
            let width = runner.config().encoder.width as usize;
            let graph_bytes = input
                .entity_features
                .len()
                .saturating_mul(width.saturating_mul(32))
                .saturating_add(
                    input
                        .relation_features
                        .len()
                        .saturating_mul(width.saturating_mul(16)),
                )
                .saturating_add(
                    input
                        .spatial_features
                        .len()
                        .saturating_mul(width.saturating_mul(16)),
                )
                .saturating_add(64 * 1024);
            let reservation = match TreeReservation::new(budget, graph_bytes) {
                Ok(reservation) => reservation,
                Err(stop) => return Ok(RootScoreStart::MemoryStop(stop)),
            };
            let graph = crate::constructive::InputBatch::from_inputs(&[input], device)?;
            let encoded = runner.encode_one_root(&graph)?;
            ensure!(
                bool::try_from(encoded.values.isfinite().all())?,
                "aic-learning.root-score-value: nonfinite value head"
            );
            let completion = encoded.values.double_value(&[0, COMPLETION_HEAD]);
            let penalty = lexicographic_quality_penalty(
                encoded.values.double_value(&[0, AREA_HEAD]),
                encoded.values.double_value(&[0, TRANSPORT_HEAD]),
                encoded.values.double_value(&[0, TURNS_HEAD]),
                bounds,
                connections,
            );
            (
                Some(encoded),
                Some(reservation),
                completion * predicted_quality_value(penalty),
            )
        } else {
            (None, None, fallback_value)
        };
        ensure!(
            model_value.is_finite(),
            "aic-learning.root-score-value: nonfinite model value"
        );
        let io_memory = match TreeReservation::new(budget, 64 * 1024) {
            Ok(reservation) => reservation,
            Err(stop) => return Ok(RootScoreStart::MemoryStop(stop)),
        };
        let file = match OpenOptions::new()
            .write(true)
            .read(true)
            .create_new(true)
            .open(path)
        {
            Ok(file) => file,
            Err(source) if is_disk_resource(&source) => {
                return Ok(RootScoreStart::DiskStop(RootScoreDiskStop {
                    path: path.to_owned(),
                    operation: "create",
                    source,
                }));
            }
            Err(source) => {
                return Err(source)
                    .with_context(|| format!("aic-learning.root-score-temp: {}", path.display()));
            }
        };
        Ok(RootScoreStart::Ready(Self {
            noise_mode: RootNoiseMode::Rank,
            runner,
            encoded,
            device,
            path: Some(path.to_owned()),
            file: Some(file),
            count: 0,
            model_max: f32::NEG_INFINITY,
            anchor_max: f64::NEG_INFINITY,
            anchor_mass,
            model_value,
            failed: false,
            #[cfg(test)]
            fail_raw_after_bytes: None,
            _graph_memory: graph_memory,
            io_memory: Some(io_memory),
        }))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn new_with_mode(
        runner: Option<&'a OuterRunner<'a>>,
        input: Option<&Input>,
        device: Device,
        bounds: Bounds,
        connections: usize,
        anchor_mass: f64,
        fallback_value: f64,
        path: &Path,
        budget: &SearchMemoryBudget,
        noise_mode: RootNoiseMode,
    ) -> Result<RootScoreStart<'a>> {
        let mut started = Self::new(
            runner,
            input,
            device,
            bounds,
            connections,
            anchor_mass,
            fallback_value,
            path,
            budget,
        )?;
        if let RootScoreStart::Ready(pass) = &mut started {
            pass.noise_mode = noise_mode;
        }
        Ok(started)
    }

    pub(super) fn append_page(
        &mut self,
        rows: &[[f32; ACTION_FEATURES]],
        entities: &[i64],
        anchors: &[f64],
        budget: &SearchMemoryBudget,
    ) -> Result<Option<RootScorePageStop>> {
        self.append_page_with_noise(rows, entities, anchors, None, budget)
    }

    pub(super) fn append_page_with_noise(
        &mut self,
        rows: &[[f32; ACTION_FEATURES]],
        entities: &[i64],
        anchors: &[f64],
        noise_bits: Option<&[u64]>,
        budget: &SearchMemoryBudget,
    ) -> Result<Option<RootScorePageStop>> {
        ensure!(
            !self.failed,
            "aic-learning.root-score-aborted: discard a failed raw score pass"
        );
        ensure!(
            !rows.is_empty()
                && rows.len() == entities.len()
                && rows.len() == anchors.len()
                && match self.noise_mode {
                    RootNoiseMode::Rank => noise_bits.is_none(),
                    RootNoiseMode::ActionIdentity =>
                        noise_bits.is_some_and(|bits| bits.len() == rows.len()),
                },
            "aic-learning.root-score-page: invalid page rows"
        );
        let next_count = self.count.checked_add(rows.len()).context(
            "aic-learning.root-score-count: native action count exceeds addressable domain",
        )?;
        let bytes_per_action = if let Some(runner) = self.runner {
            let encoder_width = runner.config().encoder.width as usize;
            let head_width = runner.config().width as usize;
            self.noise_mode
                .record_bytes()
                .saturating_add(16)
                .saturating_add(
                    4usize.saturating_mul(
                        3usize
                            .saturating_mul(ACTION_FEATURES)
                            .saturating_add(4usize.saturating_mul(encoder_width))
                            .saturating_add(2usize.saturating_mul(head_width))
                            .saturating_add(8),
                    ),
                )
        } else {
            self.noise_mode.record_bytes() + std::mem::size_of::<f32>() + 16
        };
        let page_bytes = rows.len().saturating_mul(bytes_per_action);
        let _page_memory = match TreeReservation::new(budget, page_bytes) {
            Ok(reservation) => reservation,
            Err(stop) => return Ok(Some(RootScorePageStop::Memory(stop))),
        };
        let logits = if let (Some(runner), Some(encoded)) = (self.runner, self.encoded.as_ref()) {
            let tensor = runner.score_one_root_page(encoded, rows, entities, self.device)?;
            Vec::<f32>::try_from(tensor.to_device(Device::Cpu))?
        } else {
            vec![0.0; rows.len()]
        };
        ensure!(
            logits.len() == rows.len(),
            "aic-learning.root-score-page: invalid logit count"
        );
        let mut raw = Vec::new();
        if raw
            .try_reserve_exact(rows.len().saturating_mul(self.noise_mode.record_bytes()))
            .is_err()
        {
            return Ok(Some(RootScorePageStop::Memory(placement_host_stop(
                budget, page_bytes,
            ))));
        }
        let mut next_model_max = self.model_max;
        let mut next_anchor_max = self.anchor_max;
        for (index, (&logit, &anchor)) in logits.iter().zip(anchors).enumerate() {
            raw.extend_from_slice(&logit.to_le_bytes());
            raw.extend_from_slice(&anchor.to_le_bytes());
            if let Some(bits) = noise_bits {
                raw.extend_from_slice(&bits[index].to_le_bytes());
            }
            next_model_max = next_model_max.max(logit);
            next_anchor_max = next_anchor_max.max(anchor);
        }
        let file = self
            .file
            .as_mut()
            .expect("unfinished score pass owns raw file");
        let committed_len = self
            .count
            .checked_mul(self.noise_mode.record_bytes())
            .context("aic-learning.root-score-length: raw stream exceeds addressable domain")?;
        #[cfg(test)]
        let write = match self.fail_raw_after_bytes.take() {
            Some(bytes) => file
                .write_all(&raw[..bytes.min(raw.len())])
                .and_then(|()| Err(std::io::Error::from(ErrorKind::StorageFull))),
            None => file.write_all(&raw),
        };
        #[cfg(not(test))]
        let write = file.write_all(&raw);
        if let Err(source) = write {
            if let Err(rollback) = file
                .set_len(committed_len as u64)
                .and_then(|()| file.seek(SeekFrom::Start(committed_len as u64)).map(|_| ()))
            {
                self.failed = true;
                return Err(rollback).with_context(|| {
                    format!(
                        "aic-learning.root-score-rollback: {} after {source}",
                        self.path.as_ref().expect("raw path").display()
                    )
                });
            }
            if is_disk_resource(&source) {
                return Ok(Some(RootScorePageStop::Disk(RootScoreDiskStop {
                    path: self.path.as_ref().expect("raw path").clone(),
                    operation: "write",
                    source,
                })));
            }
            return Err(source).with_context(|| {
                format!(
                    "aic-learning.root-score-write: {}",
                    self.path.as_ref().expect("raw path").display()
                )
            });
        }
        self.count = next_count;
        self.model_max = next_model_max;
        self.anchor_max = next_anchor_max;
        Ok(None)
    }

    pub(super) fn finish(&mut self) -> Result<(SourceOrderPriorReader, f64)> {
        ensure!(
            !self.failed,
            "aic-learning.root-score-aborted: discard a failed raw score pass"
        );
        ensure!(
            self.count > 0,
            "aic-learning.root-score-empty: no action rows"
        );
        let path = self.path.as_ref().expect("raw path");
        let file = self.file.as_mut().expect("unfinished score pass owns file");
        file.flush()?;
        let expected = self
            .count
            .checked_mul(self.noise_mode.record_bytes())
            .context("aic-learning.root-score-length: raw stream exceeds addressable domain")?;
        ensure!(
            file.metadata()?.len() == expected as u64,
            "aic-learning.root-score-length: {} has incomplete raw records",
            path.display()
        );
        file.seek(SeekFrom::Start(0))?;
        let mut model_denominator = 0.0f64;
        let mut anchor_denominator = 0.0f64;
        let mut raw = [0u8; NOISE_RECORD_BYTES];
        {
            let mut buffered = BufReader::with_capacity(64 * 1024, &mut *file);
            for _ in 0..self.count {
                buffered.read_exact(&mut raw[..self.noise_mode.record_bytes()])?;
                let logit = f32::from_le_bytes(raw[..4].try_into()?);
                let anchor = f64::from_le_bytes(raw[4..12].try_into()?);
                model_denominator += f64::from((logit - self.model_max).exp());
                anchor_denominator += (anchor - self.anchor_max).exp();
            }
        }
        ensure!(
            model_denominator.is_finite()
                && model_denominator > 0.0
                && anchor_denominator.is_finite()
                && anchor_denominator > 0.0,
            "aic-learning.root-score-normalizer: invalid global normalizer"
        );
        file.seek(SeekFrom::Start(0))?;
        let file = self.file.take().expect("finished score pass owns file");
        let reader = SourceOrderPriorReader {
            noise_mode: self.noise_mode,
            path: self.path.take().expect("raw path"),
            file: Some(BufReader::with_capacity(64 * 1024, file)),
            count: self.count,
            offset: 0,
            model_max: self.model_max,
            model_log_denominator: model_denominator.ln() as f32,
            anchor_max: self.anchor_max,
            anchor_denominator,
            anchor_mass: self.anchor_mass,
            model_present: self.runner.is_some(),
            _io_memory: self
                .io_memory
                .take()
                .expect("score pass owns I/O reservation"),
        };
        self.encoded = None;
        self._graph_memory = None;
        Ok((reader, self.model_value))
    }
}

impl Drop for RootScorePass<'_> {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

fn is_disk_resource(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded
    )
}

impl SourceOrderPriorReader {
    pub(super) fn mode(&self) -> RootNoiseMode {
        self.noise_mode
    }

    pub(super) fn read_page(
        &mut self,
        limit: usize,
        budget: &SearchMemoryBudget,
    ) -> Result<Result<PriorPage, AllowanceStop>> {
        ensure!(limit > 0, "aic-learning.root-prior-page: zero page limit");
        let count = limit.min(self.count - self.offset);
        let bytes = count
            .saturating_mul(
                std::mem::size_of::<(f64, f64)>()
                    + usize::from(self.noise_mode == RootNoiseMode::ActionIdentity)
                        * std::mem::size_of::<u64>(),
            )
            .saturating_add(4096);
        let reservation = match TreeReservation::new(budget, bytes) {
            Ok(reservation) => reservation,
            Err(stop) => return Ok(Err(stop)),
        };
        let mut priors = Vec::new();
        if priors.try_reserve_exact(count).is_err() {
            return Ok(Err(placement_host_stop(budget, bytes)));
        }
        let mut noise_bits = (self.noise_mode == RootNoiseMode::ActionIdentity).then(Vec::new);
        if noise_bits
            .as_mut()
            .is_some_and(|bits| bits.try_reserve_exact(count).is_err())
        {
            return Ok(Err(placement_host_stop(budget, bytes)));
        }
        let mut raw = [0u8; NOISE_RECORD_BYTES];
        for _ in 0..count {
            self.file
                .as_mut()
                .expect("raw reader is open")
                .read_exact(&mut raw[..self.noise_mode.record_bytes()])?;
            let logit = f32::from_le_bytes(raw[..4].try_into()?);
            let anchor = f64::from_le_bytes(raw[4..12].try_into()?);
            let anchor_prior =
                (anchor - self.anchor_max).exp() / self.anchor_denominator.max(f64::MIN_POSITIVE);
            let first_prior = if self.model_present {
                let learned =
                    f64::from(((logit - self.model_max) - self.model_log_denominator).exp());
                learned * (1.0 - self.anchor_mass) + anchor_prior * self.anchor_mass
            } else {
                anchor_prior
            };
            priors.push((first_prior, anchor_prior));
            if let Some(bits) = &mut noise_bits {
                bits.push(u64::from_le_bytes(raw[12..20].try_into()?));
            }
        }
        self.offset += count;
        Ok(Ok(PriorPage {
            priors,
            noise_bits,
            next_offset: self.offset,
            _memory: reservation,
        }))
    }

    pub(super) fn count(&self) -> usize {
        self.count
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn close(&mut self) -> std::io::Result<()> {
        assert_eq!(self.offset, self.count, "raw score file has unread actions");
        self.file.take();
        fs::remove_file(&self.path)?;
        self.path.clear();
        Ok(())
    }
}

impl Drop for SourceOrderPriorReader {
    fn drop(&mut self) {
        if !self.path.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        hierarchical_search::{
            MacroAction, deterministic_action_gumbel, physical_action_identity, state_sha256,
        },
        outer_model::{Batch as OuterBatch, Model as OuterModel},
        schema::{ConstructiveModelConfig, OuterModelConfig},
    };
    use aic_layout::{
        geometry::Point,
        logistics::TransportKind,
        physical::{Terminal, TerminalDirection},
    };
    use aic_pilot_native::constructive::{
        Command, Limits, Session,
        routing_candidates::{Config as CandidateConfig, Request},
    };
    use aic_torch::tch::{self, Kind};

    #[test]
    #[ignore = "set AIC_ROOT_SCORE_PASS_PANEL for a complete local physical score pass"]
    fn complete_physical_placement_score_pass_probe() -> Result<()> {
        use crate::hierarchical_search::placement_scoring::{
            PlacementActionScorer, PreparedPlacement, ScoredPlacement,
        };
        use aic_pilot_native::constructive::{
            Session,
            learning::{CommandKind, Config as DomainConfig},
        };
        use std::{sync::Arc, time::Instant};

        let _guard = crate::TORCH_TEST.lock().unwrap();
        let panel_path = std::env::var("AIC_ROOT_SCORE_PASS_PANEL")?;
        let panel: serde_json::Value = serde_json::from_slice(&fs::read(&panel_path)?)?;
        let case = &panel["cases"][0];
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let loaded = aic_layout_gen::io::load_catalogs(&root.join("data/game/normalized"))?;
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            Arc::new(loaded.catalogs),
            serde_json::from_value(panel["limits"].clone())?,
        )?;
        let prefix = std::env::var("AIC_ROOT_SCORE_PASS_PREFIX")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        for command in case["prefix"].as_array().unwrap().iter().take(prefix) {
            let step = session.step(serde_json::from_value(command.clone())?);
            ensure!(step.accepted && step.diagnostic.is_none(), "{step:?}");
        }
        let candidate: aic_pilot_native::constructive::routing_candidates::Config =
            serde_json::from_value(panel["candidate"].clone())?;
        let domain_config = DomainConfig {
            placement: candidate.bounds,
            extension_lengths: vec![1, 2, 4],
        };
        let domain = session.learning_placement_domain(&domain_config)?;
        let input = session.macro_learning_input(&domain_config)?;
        let config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: ConstructiveModelConfig {
                schema: ConstructiveModelConfig::SCHEMA.into(),
                width: 32,
                message_rounds: 1,
            },
            width: 32,
        };
        let (order, state) = OuterModel::initial_state(&config, 51)?;
        let model = OuterModel::load(&config, &order, &state, Device::Cpu)?;
        let compiled = std::env::var("AIC_ROOT_SCORE_PASS_COMPILED").as_deref() == Ok("1");
        let runner = if compiled {
            OuterRunner::Compiled(crate::outer_model::CompiledModel::new(&model))
        } else {
            OuterRunner::Eager(&model)
        };
        let budget = SearchMemoryBudget::new(512 * 1024 * 1024);
        let scorer = match PlacementActionScorer::prepare(&session, candidate.bounds, &budget)? {
            PreparedPlacement::Ready(scorer) => scorer,
            PreparedPlacement::MemoryStop(stop) => anyhow::bail!("scorer stopped: {stop:?}"),
        };
        let page_scorer = scorer.begin_page(&session)?;
        let path = root.join("artifacts/plan02-complete-physical-root-scores.raw");
        let started = Instant::now();
        let pass = RootScorePass::new(
            Some(&runner),
            Some(&input),
            Device::Cpu,
            candidate.bounds,
            session.problem().connections.len(),
            0.25,
            0.0,
            &path,
            &budget,
        )?;
        let RootScoreStart::Ready(mut pass) = pass else {
            anyhow::bail!("root score graph admission stopped");
        };
        let encode_seconds = started.elapsed().as_secs_f64();
        let mut rows = Vec::with_capacity(65_536);
        let mut entities = Vec::with_capacity(65_536);
        let mut anchors = Vec::with_capacity(65_536);
        let mut count = 0usize;
        let mut pages = 0usize;
        let mut append_seconds = 0.0f64;
        let max_pages = std::env::var("AIC_ROOT_SCORE_PASS_MAX_PAGES")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        for kind in [CommandKind::Place, CommandKind::PlaceTerminal] {
            let mut offset = 0usize;
            loop {
                let page = domain.command_page(kind, offset, 256)?;
                for factored in &page.commands {
                    let score = match page_scorer.score(&factored.command.command, &budget)? {
                        ScoredPlacement::Ready(score) => score,
                        ScoredPlacement::MemoryStop(stop) => {
                            anyhow::bail!("placement scoring stopped: {stop:?}")
                        }
                    };
                    rows.push(score.features);
                    entities.push(score.entity_index);
                    anchors.push(score.anchor_score);
                    count += 1;
                    if rows.len() == 65_536 {
                        let now = Instant::now();
                        ensure!(
                            pass.append_page(&rows, &entities, &anchors, &budget)?
                                .is_none(),
                            "root score page stopped"
                        );
                        append_seconds += now.elapsed().as_secs_f64();
                        rows.clear();
                        entities.clear();
                        anchors.clear();
                    }
                }
                offset = page.next_offset;
                pages += 1;
                if page.exhausted || pages >= max_pages {
                    break;
                }
            }
            if pages >= max_pages {
                break;
            }
        }
        if !rows.is_empty() {
            let now = Instant::now();
            ensure!(
                pass.append_page(&rows, &entities, &anchors, &budget)?
                    .is_none(),
                "root score final page stopped"
            );
            append_seconds += now.elapsed().as_secs_f64();
        }
        let now = Instant::now();
        let (mut reader, model_value) = pass.finish()?;
        let finish_seconds = now.elapsed().as_secs_f64();
        ensure!(reader.count() == count, "root score count mismatch");
        let bytes = fs::metadata(&path)?.len();
        let now = Instant::now();
        let mut prior_count = 0usize;
        let mut checksum = 0.0f64;
        while prior_count < count {
            let page = reader
                .read_page(65_536, &budget)?
                .expect("prior page admitted");
            for (prior, anchor) in &page.priors {
                checksum += prior + anchor;
            }
            prior_count = page.next_offset;
        }
        let read_seconds = now.elapsed().as_secs_f64();
        drop(reader);
        ensure!(!path.exists(), "raw temporary survived reader drop");
        eprintln!(
            "plan02 complete score pass compiled={compiled} count={count} pages={pages} total_seconds={:.3} encode_seconds={encode_seconds:.3} append_seconds={append_seconds:.3} finish_seconds={finish_seconds:.3} read_seconds={read_seconds:.3} raw_bytes={bytes} model_value={model_value:.6} checksum={checksum:.6} peak_admitted_bytes={}",
            started.elapsed().as_secs_f64(),
            budget.peak_bytes(),
        );
        Ok(())
    }

    #[test]
    fn root_score_pass_matches_dense_normalizers_and_cleans_temp() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: ConstructiveModelConfig {
                schema: ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            width: 16,
        };
        let (order, state) = OuterModel::initial_state(&config, 91)?;
        let model = OuterModel::load(&config, &order, &state, Device::Cpu)?;
        let session = crate::constructive::test_support::session(64);
        let domain = crate::constructive::test_support::domain();
        let input = session.macro_learning_input(&domain)?;
        let mut rows = [[0.0; ACTION_FEATURES]; 5];
        rows[1].fill(0.5);
        rows[2].fill(1.0);
        rows[3].fill(0.5);
        rows[4].fill(-0.5);
        let entities = [0, 1, 0, 1, 0];
        let anchors = [0.0, 1.0, -1.0, 1.0, 0.2];
        let dense_batch = OuterBatch::from_rows(&[&input], &[&rows], &[&entities], Device::Cpu)?;
        let dense = tch::no_grad(|| model.forward(&dense_batch))?;
        let learned = Vec::<f32>::try_from(dense.logp.exp().to_device(Device::Cpu))?;
        let anchor_max = anchors.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let anchor_denominator = anchors
            .iter()
            .map(|anchor| (anchor - anchor_max).exp())
            .sum::<f64>();
        let budget = SearchMemoryBudget::new(64 * 1024 * 1024);
        for (mode, runner) in [
            ("eager", OuterRunner::Eager(&model)),
            (
                "compiled",
                OuterRunner::Compiled(crate::outer_model::CompiledModel::new(&model)),
            ),
        ] {
            let path = std::env::temp_dir().join(format!(
                "aic-plan02-root-score-{}-{mode}.raw",
                std::process::id()
            ));
            let pass = RootScorePass::new(
                Some(&runner),
                Some(&input),
                Device::Cpu,
                domain.placement,
                session.problem().connections.len(),
                0.25,
                0.0,
                &path,
                &budget,
            )?;
            let RootScoreStart::Ready(mut pass) = pass else {
                anyhow::bail!("root score graph admission stopped");
            };
            assert!(
                pass.append_page(&rows[..2], &entities[..2], &anchors[..2], &budget)?
                    .is_none()
            );
            assert!(
                pass.append_page(&rows[2..], &entities[2..], &anchors[2..], &budget)?
                    .is_none()
            );
            let (mut reader, model_value) = pass.finish()?;
            assert_eq!(reader.count(), rows.len());
            let completion = dense.values.double_value(&[0, COMPLETION_HEAD]);
            let penalty = lexicographic_quality_penalty(
                dense.values.double_value(&[0, AREA_HEAD]),
                dense.values.double_value(&[0, TRANSPORT_HEAD]),
                dense.values.double_value(&[0, TURNS_HEAD]),
                domain.placement,
                session.problem().connections.len(),
            );
            assert!((model_value - completion * predicted_quality_value(penalty)).abs() < 1e-8);
            let mut actual = Vec::new();
            for limit in [2, 2, 2] {
                let page = reader
                    .read_page(limit, &budget)?
                    .expect("prior page admitted");
                assert_eq!(page.next_offset, actual.len() + page.priors.len());
                actual.extend_from_slice(&page.priors);
            }
            assert_eq!(actual.len(), rows.len());
            for index in 0..rows.len() {
                let anchor_prior =
                    (anchors[index] - anchor_max).exp() / anchor_denominator.max(f64::MIN_POSITIVE);
                let prior = f64::from(learned[index]) * 0.75 + anchor_prior * 0.25;
                assert!((actual[index].0 - prior).abs() < 1e-6);
                assert_eq!(actual[index].1.to_bits(), anchor_prior.to_bits());
            }
            assert_eq!(
                actual[1].0.to_bits(),
                actual[3].0.to_bits(),
                "identical actions split across pages must retain a prior tie"
            );
            let mut ordered = (0..actual.len()).collect::<Vec<_>>();
            ordered.sort_by(|&left, &right| actual[right].0.total_cmp(&actual[left].0));
            assert!(
                ordered.iter().position(|&index| index == 1)
                    < ordered.iter().position(|&index| index == 3),
                "stable prior sort must keep source order for a tie"
            );
            drop(reader);
            assert!(!path.exists());
            assert_eq!(budget.live_bytes(), 0);
        }
        let path = std::env::temp_dir().join(format!(
            "aic-plan02-root-score-abort-{}.raw",
            std::process::id()
        ));
        let RootScoreStart::Ready(mut aborted) = RootScorePass::new(
            None,
            None,
            Device::Cpu,
            domain.placement,
            session.problem().connections.len(),
            0.25,
            0.7,
            &path,
            &budget,
        )?
        else {
            anyhow::bail!("anchor-only admission stopped");
        };
        assert!(
            aborted
                .append_page(&rows[..1], &entities[..1], &anchors[..1], &budget)?
                .is_none()
        );
        drop(aborted);
        assert!(!path.exists());
        assert_eq!(budget.live_bytes(), 0);
        let RootScoreStart::Ready(mut anchor_only) = RootScorePass::new(
            None,
            None,
            Device::Cpu,
            domain.placement,
            session.problem().connections.len(),
            0.25,
            0.7,
            &path,
            &budget,
        )?
        else {
            anyhow::bail!("anchor-only score pass admission stopped");
        };
        assert!(
            anchor_only
                .append_page(&rows[..2], &entities[..2], &anchors[..2], &budget)?
                .is_none()
        );
        let (mut reader, model_value) = anchor_only.finish()?;
        assert_eq!(model_value, 0.7);
        let page = reader
            .read_page(2, &budget)?
            .expect("anchor-only page admitted");
        assert_eq!(page.priors[0].0.to_bits(), page.priors[0].1.to_bits());
        assert_eq!(page.priors[1].0.to_bits(), page.priors[1].1.to_bits());
        drop(page);
        drop(reader);
        assert!(!path.exists());
        assert_eq!(budget.live_bytes(), 0);
        let tight = SearchMemoryBudget::new(64 * 1024);
        let RootScoreStart::Ready(mut refused) = RootScorePass::new(
            None,
            None,
            Device::Cpu,
            domain.placement,
            session.problem().connections.len(),
            0.25,
            0.7,
            &path,
            &tight,
        )?
        else {
            anyhow::bail!("I/O-only score pass admission stopped");
        };
        assert!(
            refused
                .append_page(&rows[..1], &entities[..1], &anchors[..1], &tight)?
                .is_some()
        );
        drop(refused);
        assert!(!path.exists());
        assert_eq!(tight.live_bytes(), 0);
        assert_eq!(dense.logits.kind(), Kind::Float);
        Ok(())
    }

    #[test]
    fn mixed_terminal_and_route_witness_priors_match_dense() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let mut problem = crate::constructive::test_support::session(64)
            .problem()
            .clone();
        problem.terminals.push(Terminal {
            id: "unconnected-terminal".into(),
            direction: TerminalDirection::Input,
            commodity: "test-item".into(),
            transport: TransportKind::Belt,
        });
        let mut session = Session::new(
            problem,
            crate::constructive::test_support::catalogs(),
            Limits {
                max_turns: 64,
                max_extension: 16,
            },
        )?;
        for (block, position) in [
            ("source", Point { x: 5, y: 8 }),
            ("target", Point { x: 5, y: 2 }),
        ] {
            ensure!(
                session
                    .step(Command::Place {
                        block: block.into(),
                        position,
                        rotation: 0
                    })
                    .accepted,
                "placed fixture rejected"
            );
        }
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 15,
            max_y: 15,
        };
        let domain = aic_pilot_native::constructive::learning::Config {
            placement: bounds,
            extension_lengths: vec![1, 2, 4],
        };
        let candidate = CandidateConfig {
            bounds,
            max_candidates: 3,
            max_searches: 12,
            work_per_search: 10_000,
            max_expanded: 100_000,
            overlap_penalty: 8,
            max_crossings_per_route: 0,
            request_allowance: Default::default(),
        };
        let terminal = session
            .learning_placement_domain(&domain)?
            .command_page(
                aic_pilot_native::constructive::learning::CommandKind::PlaceTerminal,
                0,
                1,
            )?
            .commands
            .remove(0)
            .command
            .command;
        let route = session
            .route_candidates(
                &Request {
                    state_revision: session.revision(),
                    connection: "connection-0".into(),
                },
                &candidate,
            )?
            .candidates
            .into_iter()
            .next()
            .context("missing route witness")?;
        let actions = [MacroAction::Command(terminal), MacroAction::Route(route)];
        let mut rows = Vec::new();
        let mut entities = Vec::new();
        let mut anchors = Vec::new();
        for action in &actions {
            let child = action.apply(&session, &candidate, 0)?;
            rows.push(action.features(&session, &child, bounds));
            entities.push(action.entity_index(session.problem())?);
            anchors.push(action.prior_score(&session, &child, bounds));
        }
        let input = session.macro_learning_input(&domain)?;
        let config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: ConstructiveModelConfig {
                schema: ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            width: 16,
        };
        let (order, state) = OuterModel::initial_state(&config, 113)?;
        let model = OuterModel::load(&config, &order, &state, Device::Cpu)?;
        let dense = OuterBatch::from_rows(&[&input], &[&rows], &[&entities], Device::Cpu)?;
        let dense = tch::no_grad(|| model.forward(&dense))?;
        let learned = Vec::<f32>::try_from(dense.logp.exp().to_device(Device::Cpu))?;
        let max = anchors.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let denominator = anchors
            .iter()
            .map(|anchor| (anchor - max).exp())
            .sum::<f64>();
        let budget = SearchMemoryBudget::new(64 * 1024 * 1024);
        let path =
            std::env::temp_dir().join(format!("aic-plan02-mixed-{}.raw", std::process::id()));
        let runner = OuterRunner::Eager(&model);
        let RootScoreStart::Ready(mut pass) = RootScorePass::new(
            Some(&runner),
            Some(&input),
            Device::Cpu,
            bounds,
            session.problem().connections.len(),
            0.25,
            0.0,
            &path,
            &budget,
        )?
        else {
            anyhow::bail!("mixed score pass admission stopped");
        };
        ensure!(
            pass.append_page(&rows[..1], &entities[..1], &anchors[..1], &budget)?
                .is_none(),
            "terminal page stopped"
        );
        ensure!(
            pass.append_page(&rows[1..], &entities[1..], &anchors[1..], &budget)?
                .is_none(),
            "route page stopped"
        );
        let (mut reader, _) = pass.finish()?;
        let page = reader
            .read_page(2, &budget)?
            .expect("mixed prior page admitted");
        for index in 0..2 {
            let anchor = (anchors[index] - max).exp() / denominator;
            let prior = f64::from(learned[index]) * 0.75 + anchor * 0.25;
            assert!((page.priors[index].0 - prior).abs() < 1e-6);
            assert_eq!(page.priors[index].1.to_bits(), anchor.to_bits());
        }
        let baseline_priors = page.priors.clone();
        drop(page);
        drop(reader);
        assert!(!path.exists());
        assert_eq!(budget.live_bytes(), 0);
        let noise_path =
            std::env::temp_dir().join(format!("aic-plan03-noise-{}.raw", std::process::id()));
        let state_identity = state_sha256(&session)?;
        let noise_bits = actions
            .iter()
            .map(|action| {
                Ok(deterministic_action_gumbel(
                    7,
                    &state_identity,
                    &physical_action_identity(action)?,
                )?
                .to_bits())
            })
            .collect::<Result<Vec<u64>>>()?;
        let RootScoreStart::Ready(mut noisy) = RootScorePass::new_with_mode(
            Some(&runner),
            Some(&input),
            Device::Cpu,
            bounds,
            session.problem().connections.len(),
            0.25,
            0.0,
            &noise_path,
            &budget,
            RootNoiseMode::ActionIdentity,
        )?
        else {
            anyhow::bail!("action-identity score pass admission stopped");
        };
        assert!(
            noisy
                .append_page_with_noise(&rows, &entities, &anchors, Some(&noise_bits), &budget)?
                .is_none()
        );
        let (mut reader, _) = noisy.finish()?;
        assert_eq!(
            fs::metadata(&noise_path)?.len(),
            (NOISE_RECORD_BYTES * 2) as u64
        );
        let noise_page = reader.read_page(2, &budget)?.expect("noise page admitted");
        assert_eq!(
            noise_page.noise_bits.as_deref(),
            Some(noise_bits.as_slice())
        );
        for (with_noise, without_noise) in noise_page.priors.iter().zip(&baseline_priors) {
            assert_eq!(with_noise.0.to_bits(), without_noise.0.to_bits());
            assert_eq!(with_noise.1.to_bits(), without_noise.1.to_bits());
        }
        drop(noise_page);
        reader.close()?;
        drop(reader);
        assert!(!noise_path.exists());
        assert_eq!(budget.live_bytes(), 0);
        Ok(())
    }

    #[test]
    fn raw_score_disk_stop_retries_the_same_page() -> Result<()> {
        let budget = SearchMemoryBudget::new(8 << 20);
        let path = std::env::temp_dir().join(format!(
            "aic-root-score-disk-retry-{}.raw",
            std::process::id()
        ));
        let RootScoreStart::Ready(mut pass) = RootScorePass::new(
            None,
            None,
            Device::Cpu,
            crate::constructive::test_support::domain().placement,
            0,
            0.0,
            0.0,
            &path,
            &budget,
        )?
        else {
            anyhow::bail!("raw score admission stopped");
        };
        let rows = [[0.0; ACTION_FEATURES]; 2];
        pass.fail_raw_after_bytes = Some(5);
        assert!(matches!(
            pass.append_page(&rows, &[0, 0], &[0.0, 1.0], &budget)?,
            Some(RootScorePageStop::Disk(_))
        ));
        assert_eq!(pass.count, 0);
        assert_eq!(fs::metadata(&path)?.len(), 0);
        assert!(
            pass.append_page(&rows, &[0, 0], &[0.0, 1.0], &budget)?
                .is_none()
        );
        let (mut reader, _) = pass.finish()?;
        assert_eq!(reader.count(), 2);
        assert_eq!(reader.read_page(2, &budget)?.unwrap().priors.len(), 2);
        reader.close()?;
        drop(reader);
        assert!(!path.exists());
        assert_eq!(budget.live_bytes(), 0);
        Ok(())
    }

    #[test]
    fn near_tie_prior_ordering_is_confined_to_numeric_tolerance() -> Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: ConstructiveModelConfig {
                schema: ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            width: 16,
        };
        let (order, state) = OuterModel::initial_state(&config, 151)?;
        let model = OuterModel::load(&config, &order, &state, Device::Cpu)?;
        let session = crate::constructive::test_support::session(64);
        let domain = crate::constructive::test_support::domain();
        let input = session.macro_learning_input(&domain)?;
        let mut rows = vec![[0.5; ACTION_FEATURES]; 64];
        for (index, row) in rows.iter_mut().enumerate().skip(2) {
            row[10] += (index as f32 - 32.0) * 1e-6;
        }
        let entities = vec![0; rows.len()];
        let anchors = vec![0.0; rows.len()];
        let batch = OuterBatch::from_rows(&[&input], &[&rows], &[&entities], Device::Cpu)?;
        let dense = tch::no_grad(|| model.forward(&batch))?;
        let dense = Vec::<f32>::try_from(dense.logp.exp().to_device(Device::Cpu))?
            .into_iter()
            .map(f64::from)
            .collect::<Vec<_>>();
        let budget = SearchMemoryBudget::new(64 * 1024 * 1024);
        for (mode, runner) in [
            ("eager", OuterRunner::Eager(&model)),
            (
                "compiled",
                OuterRunner::Compiled(crate::outer_model::CompiledModel::new(&model)),
            ),
        ] {
            let path = std::env::temp_dir().join(format!(
                "aic-plan02-near-tie-{}-{mode}.raw",
                std::process::id()
            ));
            let RootScoreStart::Ready(mut pass) = RootScorePass::new(
                Some(&runner),
                Some(&input),
                Device::Cpu,
                domain.placement,
                session.problem().connections.len(),
                0.0,
                0.0,
                &path,
                &budget,
            )?
            else {
                anyhow::bail!("near-tie score pass admission stopped");
            };
            let mut offset = 0;
            for (row_page, entity_page) in rows.chunks(7).zip(entities.chunks(7)) {
                assert!(
                    pass.append_page(
                        row_page,
                        entity_page,
                        &anchors[offset..offset + row_page.len()],
                        &budget,
                    )?
                    .is_none()
                );
                offset += row_page.len();
            }
            let (mut reader, _) = pass.finish()?;
            let mut paged = Vec::new();
            while paged.len() < rows.len() {
                let page = reader
                    .read_page(11, &budget)?
                    .expect("near-tie page admitted");
                paged.extend(page.priors.iter().map(|prior| prior.0));
            }
            let max_error = dense
                .iter()
                .zip(&paged)
                .map(|(left, right)| (left - right).abs())
                .fold(0.0f64, f64::max);
            assert!(max_error <= 1e-6, "near-tie prior error {max_error}");
            let mut near_pairs = 0usize;
            let mut reversals = 0usize;
            for left in 0..rows.len() {
                for right in left + 1..rows.len() {
                    let dense_margin = (dense[left] - dense[right]).abs();
                    if dense_margin <= 2.0 * max_error + 1e-9 {
                        near_pairs += 1;
                    }
                    let dense_order = dense[left].total_cmp(&dense[right]);
                    let paged_order = paged[left].total_cmp(&paged[right]);
                    if dense_order != paged_order {
                        reversals += 1;
                        assert!(
                            dense_margin <= 2.0 * max_error + 1e-9,
                            "prior order changed beyond measured numeric tolerance"
                        );
                    }
                }
            }
            assert!(near_pairs > 0, "fixture did not create near-tie priors");
            assert_eq!(paged[0].to_bits(), paged[1].to_bits());
            eprintln!(
                "plan02 near-tie mode={mode} max_error={max_error:.3e} near_pairs={near_pairs} changed_pair_orders={reversals}"
            );
            drop(reader);
            assert!(!path.exists());
        }
        assert_eq!(budget.live_bytes(), 0);
        Ok(())
    }
}
