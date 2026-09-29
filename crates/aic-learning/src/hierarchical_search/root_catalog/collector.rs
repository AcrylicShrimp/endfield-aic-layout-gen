//! Source-bound collection of one complete initial Gumbel root.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use aic_torch::tch::Device;

use super::{
    CatalogChoice, CatalogStop, NonplacementWitnesses, PendingInitialLower, PlacementCatalogScan,
    PlacementCatalogStart, PlacementCatalogStop, PriorCatalogFiles, PriorRow,
    ResolvedPlacementChoices, RootCatalogNormalizer, disk_admission,
};
use crate::hierarchical_search::{
    AllowanceStop, Counters, Edge, Factory, LowerPager, MacroAction, MacroDiagnostic, Node,
    OuterRunner, RouteUpdate, SearchMemoryBudget, Session, TreeReservation, action_bytes,
    no_macro_reason, placement_host_stop,
    placement_scoring::{PlacementActionScorer, ScoredPlacement},
    provenance_bytes,
    root_scoring::{
        RootNoiseMode, RootScoreDiskStop, RootScorePass, RootScoreStart, SourceOrderPriorReader,
    },
    state_sha256, state_value,
};

pub(super) enum CatalogCollectStart<'s, 'o> {
    Ready(RootCatalogCollector<'s, 'o>),
    MemoryStop(AllowanceStop),
    DiskStop(RootScoreDiskStop),
}

#[derive(Debug)]
pub(super) enum CatalogCollectStop {
    Memory(AllowanceStop),
    Lower(MacroDiagnostic),
    Disk {
        path: PathBuf,
        operation: &'static str,
        source: std::io::Error,
    },
}

pub(super) enum CatalogCollectProgress {
    Ready,
    NoActions(MacroDiagnostic),
    Stopped(CatalogCollectStop),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CollectPhase {
    Placements,
    InitialLower,
    RawFinish,
    Normalize,
    Sort,
    Shortlist,
    Ready,
    NoActions,
}

/// Retains the source, raw score stream and uncommitted sidecars across an
/// in-process resource stop. Once ready, only shortlisted placement commands
/// and the complete nonplacement witness table remain in host memory.
pub(super) struct RootCatalogCollector<'s, 'o> {
    source: &'s Session,
    source_arc: Arc<Session>,
    prefix: PathBuf,
    page_size: usize,
    mode: RootNoiseMode,
    seed: u64,
    case_id: String,
    state_identity: String,
    candidates: usize,
    anchor_candidates: usize,
    anchor_policy_mass: f64,
    phase: CollectPhase,
    score: Option<RootScorePass<'o>>,
    placement: Option<PlacementCatalogScan<'s>>,
    placement_disk_admitted: bool,
    placement_count: u64,
    lower: Option<LowerPager>,
    lower_started: bool,
    lower_before: Option<Counters>,
    empty_diagnostic: Option<MacroDiagnostic>,
    pending_lower: Option<PendingInitialLower>,
    pending_lower_disk_admitted: bool,
    witnesses: NonplacementWitnesses,
    reader: Option<SourceOrderPriorReader>,
    normalization_disk_admitted: bool,
    normalizer: Option<RootCatalogNormalizer>,
    files: Option<PriorCatalogFiles>,
    shortlist: Option<Vec<CatalogChoice>>,
    resolved_placements: Option<ResolvedPlacementChoices>,
    exact_value: f64,
    model_value: Option<f64>,
}

pub(super) struct InitialRootCatalog<'s> {
    pub(super) source: Arc<Session>,
    pub(super) files: PriorCatalogFiles,
    pub(super) shortlist: Vec<CatalogChoice>,
    pub(super) resolved_placements: Option<ResolvedPlacementChoices>,
    placement_scorer: Option<PlacementActionScorer<'s>>,
    pub(super) witnesses: NonplacementWitnesses,
    pub(super) lower: LowerPager,
    pub(super) lower_loaded_pages: usize,
    pending_late: Option<PendingInitialLower>,
    pending_updates: Vec<RouteUpdate>,
    pub(super) placement_count: u64,
    pub(super) initial_root_value: f64,
    pub(super) state_identity: String,
}

pub(super) struct SparseRootEdges {
    pub(super) edges: Vec<Edge>,
    pub(super) _memory: TreeReservation,
}

impl<'s, 'o> RootCatalogCollector<'s, 'o> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        source: &'s Session,
        source_arc: Arc<Session>,
        factory: &Factory<'_>,
        runner: Option<&'o OuterRunner<'o>>,
        device: Device,
        prefix: &Path,
        mode: RootNoiseMode,
        seed: u64,
        case_id: &str,
        candidates: usize,
        anchor_candidates: usize,
        anchor_policy_mass: f64,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<CatalogCollectStart<'s, 'o>> {
        anyhow::ensure!(
            std::ptr::eq(source, source_arc.as_ref()),
            "aic-learning.root-catalog-source: borrowed and owned sources differ"
        );
        let input = runner
            .map(|_| source.macro_learning_input(&factory.domain))
            .transpose()?;
        let raw_path = prefix.with_extension("root-score.tmp");
        let score = match RootScorePass::new_with_mode(
            runner,
            input.as_deref(),
            device,
            factory.domain.placement,
            source.problem().connections.len(),
            factory.anchor_prior_mass(),
            state_value(source, factory.domain.placement),
            &raw_path,
            budget,
            mode,
        )? {
            RootScoreStart::Ready(score) => score,
            RootScoreStart::MemoryStop(stop) => return Ok(CatalogCollectStart::MemoryStop(stop)),
            RootScoreStart::DiskStop(stop) => return Ok(CatalogCollectStart::DiskStop(stop)),
        };
        let state_identity = state_sha256(source)?;
        let witnesses = NonplacementWitnesses::new(0, budget)
            .map_err(|stop| anyhow::anyhow!("root witness admission: {stop:?}"))?;
        Ok(CatalogCollectStart::Ready(Self {
            source,
            source_arc,
            prefix: prefix.to_owned(),
            page_size: factory.placement_page,
            mode,
            seed,
            case_id: case_id.to_owned(),
            state_identity,
            candidates,
            anchor_candidates,
            anchor_policy_mass,
            phase: CollectPhase::Placements,
            score: Some(score),
            placement: None,
            placement_disk_admitted: false,
            placement_count: 0,
            lower: None,
            lower_started: false,
            lower_before: None,
            empty_diagnostic: None,
            pending_lower: None,
            pending_lower_disk_admitted: false,
            witnesses,
            reader: None,
            normalization_disk_admitted: false,
            normalizer: None,
            files: None,
            shortlist: None,
            resolved_placements: None,
            exact_value: state_value(source, factory.domain.placement),
            model_value: None,
        }))
    }

    pub(super) fn advance(
        &mut self,
        factory: &Factory<'_>,
        counters: &mut Counters,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<CatalogCollectProgress> {
        loop {
            match self.phase {
                CollectPhase::Placements => {
                    if self.source.has_unplaced_entities() {
                        if self.placement.is_none() {
                            self.placement = Some(
                                match PlacementCatalogScan::new(
                                    self.source,
                                    &factory.domain,
                                    self.page_size,
                                    self.mode,
                                    self.seed,
                                    self.state_identity.clone(),
                                    budget,
                                )? {
                                    PlacementCatalogStart::Ready(scan) => scan,
                                    PlacementCatalogStart::MemoryStop(stop) => {
                                        return Ok(CatalogCollectProgress::Stopped(
                                            CatalogCollectStop::Memory(stop),
                                        ));
                                    }
                                },
                            );
                        }
                        let scan = self.placement.as_mut().expect("placement scan started");
                        if !self.placement_disk_admitted {
                            let count = scan.legal_count()? as u64;
                            if let Err(stop) = disk_admission::admit_peak(
                                &self.prefix,
                                self.mode,
                                count,
                                0,
                                super::RUN_ROWS,
                                self.page_size,
                            ) {
                                return catalog_stop(stop);
                            }
                            self.placement_disk_admitted = true;
                        }
                        if let Some(stop) = scan.advance(
                            self.score.as_mut().expect("raw score pass is active"),
                            budget,
                        )? {
                            return Ok(CatalogCollectProgress::Stopped(stop.into()));
                        }
                        self.placement_count = scan.count;
                        self.exact_value = scan.exact_value;
                        counters.placement_samples += scan.count as usize;
                        counters.generated_macro_samples += scan.count as usize;
                    }
                    self.witnesses.initial_base = self.placement_count;
                    self.phase = CollectPhase::InitialLower;
                }
                CollectPhase::InitialLower => {
                    if self.lower_before.is_none() {
                        self.lower_before = Some(counters.clone());
                    }
                    if self.lower.is_none() {
                        self.lower = Some(LowerPager::new(
                            Arc::clone(&self.source_arc),
                            factory,
                            counters,
                            budget,
                            0,
                        )?);
                    }
                    let lower = self.lower.as_mut().expect("lower pager started");
                    if self.pending_lower.is_none() {
                        let mut actions = if self.lower_started {
                            Vec::new()
                        } else {
                            self.lower_started = true;
                            lower.first_actions(factory, counters)?
                        };
                        while actions.is_empty()
                            && lower.pending() > 0
                            && !lower.all_pending_blocked_on_memory()
                        {
                            actions.extend(lower.next_actions(factory, counters)?);
                        }
                        if !actions.is_empty() {
                            self.pending_lower = Some(PendingInitialLower::new(actions));
                        } else if self.placement_count == 0
                            && let Some(stop) = lower.pending_memory_stop()
                        {
                            return Ok(CatalogCollectProgress::Stopped(
                                CatalogCollectStop::Memory(stop),
                            ));
                        }
                    }
                    if let Some(pending) = self.pending_lower.as_mut() {
                        if !self.pending_lower_disk_admitted {
                            let score = self.score.as_ref().expect("raw score pass is active");
                            let current_count = score.count() as u64;
                            let current_raw =
                                current_count.saturating_mul(self.mode.record_bytes() as u64);
                            let count = current_count.saturating_add(pending.action_count() as u64);
                            if let Err(stop) = disk_admission::admit_peak(
                                &self.prefix,
                                self.mode,
                                count,
                                current_raw,
                                super::RUN_ROWS,
                                self.page_size,
                            ) {
                                return catalog_stop(stop);
                            }
                            self.pending_lower_disk_admitted = true;
                        }
                        if let Some(stop) = pending.advance(
                            self.source,
                            factory,
                            self.score.as_mut().expect("raw score pass is active"),
                            &mut self.witnesses,
                            self.seed,
                            &self.state_identity,
                            budget,
                        )? {
                            return Ok(CatalogCollectProgress::Stopped(stop.into()));
                        }
                        self.exact_value = self
                            .exact_value
                            .max(pending.exact_value.expect("lower page committed"));
                        self.pending_lower = None;
                        self.pending_lower_disk_admitted = false;
                    }
                    if self.placement_count == 0 && self.witnesses.initial_count == 0 {
                        let before = self.lower_before.as_ref().expect("lower counters captured");
                        let diagnostic = no_macro_reason(
                            counters,
                            (
                                before.request_stops_total,
                                before.route_deadline,
                                before.route_work_limit,
                                before.route_frontier_exhausted,
                                before.route_no_compatible_endpoints,
                                before.network_unknown,
                                before.pending_route_networks,
                                before.route_resource_limit,
                                before.route_search_limit,
                                before.route_work_budget,
                                before.route_candidate_limit,
                                before.repair_unknown,
                            ),
                            self.source.revision(),
                        );
                        if diagnostic.code != "hierarchical.no-macro-actions"
                            && diagnostic.code != "hierarchical.not-ready"
                        {
                            return Ok(CatalogCollectProgress::Stopped(CatalogCollectStop::Lower(
                                diagnostic,
                            )));
                        }
                        self.empty_diagnostic = Some(diagnostic);
                        self.phase = CollectPhase::NoActions;
                    } else {
                        self.phase = CollectPhase::RawFinish;
                    }
                }
                CollectPhase::RawFinish => {
                    let (reader, model_value) = self
                        .score
                        .as_mut()
                        .expect("raw score pass is active")
                        .finish()?;
                    self.score = None;
                    self.model_value = Some(model_value);
                    self.reader = Some(reader);
                    self.phase = CollectPhase::Normalize;
                }
                CollectPhase::Normalize => {
                    if !self.normalization_disk_admitted {
                        let reader = self.reader.as_ref().expect("raw prior stream exists");
                        let count = reader.count() as u64;
                        if let Err(stop) = disk_admission::admit_peak(
                            &self.prefix,
                            self.mode,
                            count,
                            count.saturating_mul(self.mode.record_bytes() as u64),
                            disk_admission::planned_run_rows(
                                budget.limit_bytes().saturating_sub(budget.live_bytes()),
                                std::mem::size_of::<PriorRow>(),
                            ),
                            self.page_size,
                        ) {
                            return catalog_stop(stop);
                        }
                        self.normalization_disk_admitted = true;
                    }
                    if self.normalizer.is_none() {
                        self.normalizer = Some(
                            match RootCatalogNormalizer::new(&mut self.reader, &self.prefix, budget)
                            {
                                Ok(normalizer) => normalizer,
                                Err(stop) => return catalog_stop(stop),
                            },
                        );
                    }
                    if let Some(stop) = self
                        .normalizer
                        .as_mut()
                        .expect("normalizer started")
                        .advance(self.page_size, budget)?
                    {
                        return catalog_stop(stop);
                    }
                    self.phase = CollectPhase::Sort;
                }
                CollectPhase::Sort => {
                    self.files = Some(
                        match self
                            .normalizer
                            .as_mut()
                            .expect("normalizer completed")
                            .finish(budget)
                        {
                            Ok(files) => files,
                            Err(stop) => return catalog_stop(stop),
                        },
                    );
                    self.normalizer = None;
                    self.phase = CollectPhase::Shortlist;
                }
                CollectPhase::Shortlist => {
                    let files = self.files.as_ref().expect("sorted root files exist");
                    if self.shortlist.is_none() {
                        self.shortlist = Some(
                            match files.shortlist(
                                self.candidates,
                                self.anchor_candidates,
                                self.anchor_policy_mass,
                                self.seed,
                                &self.case_id,
                                &self.state_identity,
                                budget,
                            ) {
                                Ok(shortlist) => shortlist,
                                Err(stop) => return catalog_stop(stop),
                            },
                        );
                    }
                    if let Some(scan) = &self.placement {
                        let placements = self
                            .shortlist
                            .as_ref()
                            .expect("shortlist exists")
                            .iter()
                            .copied()
                            .filter(|choice| choice.ordinal < self.placement_count)
                            .collect::<Vec<_>>();
                        self.resolved_placements =
                            Some(match scan.resolve_placement_choices(&placements, budget)? {
                                Ok(choices) => choices,
                                Err(stop) => {
                                    return Ok(CatalogCollectProgress::Stopped(
                                        CatalogCollectStop::Memory(stop),
                                    ));
                                }
                            });
                    }
                    self.phase = CollectPhase::Ready;
                }
                CollectPhase::Ready => return Ok(CatalogCollectProgress::Ready),
                CollectPhase::NoActions => {
                    return Ok(CatalogCollectProgress::NoActions(
                        self.empty_diagnostic
                            .clone()
                            .expect("empty root diagnostic"),
                    ));
                }
            }
        }
    }

    pub(super) fn finish(mut self) -> InitialRootCatalog<'s> {
        assert_eq!(self.phase, CollectPhase::Ready, "root catalog is not ready");
        InitialRootCatalog {
            source: self.source_arc,
            files: self.files.take().expect("sorted files exist"),
            shortlist: self.shortlist.take().expect("shortlist exists"),
            resolved_placements: self.resolved_placements.take(),
            placement_scorer: self.placement.take().map(PlacementCatalogScan::into_scorer),
            witnesses: self.witnesses,
            lower: self.lower.take().expect("lower pager exists"),
            lower_loaded_pages: 1,
            pending_late: None,
            pending_updates: Vec::new(),
            placement_count: self.placement_count,
            initial_root_value: if self.source.has_unplaced_entities() {
                self.exact_value
            } else {
                self.exact_value
                    .max(self.model_value.expect("score pass finished"))
            },
            state_identity: self.state_identity,
        }
    }
}

impl InitialRootCatalog<'_> {
    pub(super) fn advance_late_lower(
        &mut self,
        root: &mut Node,
        factory: &Factory<'_>,
        counters: &mut Counters,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<Option<AllowanceStop>> {
        if self.pending_late.is_none() && self.pending_updates.is_empty() {
            let desired_pages = (f64::from(root.visits.max(1)).sqrt().floor() as usize).max(1);
            let memory_retry_ready = self.lower.hash_memory_retry_ready()
                || self.lower.publication_memory_retry_ready()
                || self.lower.scope_memory_retry_ready();
            if (desired_pages <= self.lower_loaded_pages && !memory_retry_ready)
                || self.lower.pending() == 0
            {
                return Ok(None);
            }
            let additional = self.lower.next_actions(factory, counters)?;
            self.pending_updates = self.lower.take_updated_routes();
            if !memory_retry_ready
                && !self.lower.hash_memory_pending()
                && !self.lower.publication_memory_retry_ready()
                && !self.lower.scope_memory_pending()
            {
                self.lower_loaded_pages += 1;
            }
            if !additional.is_empty() {
                self.pending_late = Some(PendingInitialLower::new(additional));
            }
        }
        if let Some(pending) = self.pending_late.as_mut() {
            if let Some(stop) =
                pending.advance_late(&self.source, factory, &mut self.witnesses, budget)?
            {
                return Ok(Some(stop));
            }
            root.value = Some(
                root.value
                    .unwrap_or_default()
                    .max(pending.exact_value.expect("late page committed")),
            );
            self.pending_late = None;
        }
        let mut shortlisted_growth = 0usize;
        for update in &self.pending_updates {
            anyhow::ensure!(
                self.witnesses.has_route_key(update),
                "aic-learning.root-catalog-route-update: unpublished route {}:{}",
                update.connection,
                update.id
            );
            let index = self.witnesses.route_by_key[&(update.connection.clone(), update.id)];
            let witness = &self.witnesses.witnesses[index];
            if let Some(edge) = root
                .actions
                .as_ref()
                .expect("sparse root edges exist")
                .iter()
                .find(|edge| edge.identity == witness.identity)
            {
                let MacroAction::Route(current) = &edge.action else {
                    unreachable!("route witness ID indexes route edge")
                };
                shortlisted_growth = shortlisted_growth.saturating_add(
                    provenance_bytes(&update.additional_provenance)
                        .saturating_sub(provenance_bytes(&current.additional_provenance)),
                );
            }
        }
        if shortlisted_growth > 0 {
            let next_bytes = root.memory.bytes().saturating_add(shortlisted_growth);
            if let Err(stop) = root.memory.resize(next_bytes) {
                return Ok(Some(stop));
            }
        }
        for update in std::mem::take(&mut self.pending_updates) {
            let key = (update.connection.clone(), update.id);
            self.witnesses.apply_route_update(update)?;
            let witness = &self.witnesses.witnesses[self.witnesses.route_by_key[&key]];
            if let Some(edge) = root
                .actions
                .as_mut()
                .expect("sparse root edges exist")
                .iter_mut()
                .find(|edge| edge.identity == witness.identity)
            {
                edge.action = witness.action.clone();
                edge.features = witness.features;
            }
        }
        Ok(None)
    }

    pub(super) fn sparse_edges(
        &mut self,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<Result<SparseRootEdges, AllowanceStop>> {
        let edge_bytes = self.shortlist.iter().fold(
            4096usize.saturating_add(self.shortlist.len() * std::mem::size_of::<Edge>()),
            |bytes, choice| {
                let (action, identity_len) = if choice.ordinal < self.placement_count {
                    let resolved = self
                        .resolved_placements
                        .as_ref()
                        .expect("selected placement commands retained")
                        .choices
                        .iter()
                        .find(|row| row.choice.ordinal == choice.ordinal)
                        .expect("shortlisted placement ordinal resolved");
                    (&resolved.action, resolved.identity.len())
                } else {
                    let witness = self
                        .witnesses
                        .initial(choice.ordinal)
                        .expect("shortlisted lower ordinal retained");
                    (&witness.action, witness.identity.len())
                };
                bytes
                    .saturating_add(action_bytes(action))
                    .saturating_add(identity_len)
                    .saturating_add(256)
            },
        );
        let placement_count = self
            .resolved_placements
            .as_ref()
            .map_or(0, |resolved| resolved.choices.len());
        let score_bytes = placement_count.saturating_mul(std::mem::size_of::<(
            [f32; crate::hierarchical_search::ACTION_FEATURES],
            i64,
            f64,
        )>());
        let transferred = self
            .resolved_placements
            .as_ref()
            .map_or(0, |resolved| resolved._memory.bytes());
        let full_bytes = edge_bytes.saturating_add(score_bytes);
        let mut memory = match TreeReservation::new(budget, full_bytes.saturating_sub(transferred))
        {
            Ok(memory) => memory,
            Err(stop) => return Ok(Err(stop)),
        };
        let mut placement_scores = Vec::new();
        if placement_scores.try_reserve_exact(placement_count).is_err() {
            return Ok(Err(placement_host_stop(budget, full_bytes)));
        }
        if let Some(resolved) = &self.resolved_placements {
            let scorer = self
                .placement_scorer
                .as_ref()
                .expect("prepared placement scorer retained");
            let page = scorer.begin_page(&self.source)?;
            for choice in &resolved.choices {
                let MacroAction::Command(command) = &choice.action else {
                    unreachable!("native placement resolves to a command")
                };
                let score = match page.score(command, budget)? {
                    ScoredPlacement::Ready(score) => score,
                    ScoredPlacement::MemoryStop(stop) => return Ok(Err(stop)),
                };
                placement_scores.push((score.features, score.entity_index, score.anchor_score));
            }
        }
        let mut edges = Vec::new();
        if edges.try_reserve_exact(self.shortlist.len()).is_err() {
            return Ok(Err(placement_host_stop(budget, full_bytes)));
        }
        let (mut placements, placement_memory) =
            if let Some(resolved) = self.resolved_placements.take() {
                let ResolvedPlacementChoices { choices, _memory } = resolved;
                (Some(choices.into_iter().peekable()), Some(_memory))
            } else {
                (None, None)
            };
        let mut placement_scores = placement_scores.into_iter();
        for choice in &self.shortlist {
            let (action, identity, features, entity_index, anchor_score) =
                if choice.ordinal < self.placement_count {
                    let resolved = placements
                        .as_mut()
                        .expect("selected placement commands retained")
                        .next()
                        .expect("one resolved row per placement choice");
                    assert_eq!(resolved.choice.rank, choice.rank);
                    let (features, entity_index, anchor_score) = placement_scores
                        .next()
                        .expect("one score per placement choice");
                    (
                        resolved.action,
                        resolved.identity,
                        features,
                        entity_index,
                        anchor_score,
                    )
                } else {
                    let witness = self
                        .witnesses
                        .initial(choice.ordinal)
                        .expect("shortlisted lower ordinal retained");
                    (
                        witness.action.clone(),
                        witness.identity.clone(),
                        witness.features,
                        witness.entity_index,
                        witness.anchor_score,
                    )
                };
            edges.push(Edge {
                identity,
                action,
                features,
                entity_index,
                anchor_score,
                prior: choice.prior,
                anchor_prior: choice.anchor_prior,
                visits: 0,
                value_sum: 0.0,
                child: None,
                blocked_value: None,
            });
        }
        drop(placements);
        drop(placement_scores);
        drop(placement_memory);
        self.placement_scorer = None;
        memory
            .resize(edge_bytes)
            .expect("released placement choice reservation covers moved sparse edges");
        Ok(Ok(SparseRootEdges {
            edges,
            _memory: memory,
        }))
    }
}

fn catalog_stop(stop: CatalogStop) -> anyhow::Result<CatalogCollectProgress> {
    match stop {
        CatalogStop::Memory(stop) => Ok(CatalogCollectProgress::Stopped(
            CatalogCollectStop::Memory(stop),
        )),
        CatalogStop::Io {
            path,
            operation,
            source,
        } if matches!(
            source.kind(),
            std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded
        ) =>
        {
            Ok(CatalogCollectProgress::Stopped(CatalogCollectStop::Disk {
                path,
                operation,
                source,
            }))
        }
        other => Err(other.into()),
    }
}

impl From<PlacementCatalogStop> for CatalogCollectStop {
    fn from(stop: PlacementCatalogStop) -> Self {
        match stop {
            PlacementCatalogStop::Memory(stop) => Self::Memory(stop),
            PlacementCatalogStop::Disk(stop) => Self::Disk {
                path: stop.path,
                operation: stop.operation,
                source: stop.source,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{constructive::test_support, route_guidance::Model as GuidanceModel};
    use aic_pilot_native::constructive::{
        learning::CommandKind, routing_candidates::Config as CandidateConfig,
    };
    use std::fs;

    struct FailSerialize;

    impl serde::Serialize for FailSerialize {
        fn serialize<S>(&self, _serializer: S) -> std::result::Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            Err(serde::ser::Error::custom("injected report interruption"))
        }
    }

    #[test]
    fn collector_reports_lower_budget_exhaustion_as_unknown() -> anyhow::Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let panel: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-route-guidance/training-panel.json"
        ))?;
        let case = &panel["cases"][0];
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            test_support::catalogs(),
            serde_json::from_value(panel["limits"].clone())?,
        )?;
        for command in case["prefix"].as_array().unwrap() {
            assert!(
                session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        let source = Arc::new(session);
        assert!(!source.has_unplaced_entities());
        let mut candidate: CandidateConfig = serde_json::from_value(panel["candidate"].clone())?;
        candidate.max_candidates = 2;
        candidate.max_searches = 48;
        candidate.work_per_search = 1;
        candidate.max_expanded = 100;
        candidate.request_allowance.expansions = 1;
        let route_config: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-route-guidance/config.json"
        ))?;
        let model_config = serde_json::from_value(route_config["model"].clone())?;
        let (order, state) = GuidanceModel::initial_state(&model_config, 17)?;
        let model = GuidanceModel::load(&model_config, &order, &state, Device::Cpu)?;
        let factory = Factory::new(
            &model,
            "checkpoint",
            Device::Cpu,
            serde_json::from_value(route_config["learning_domain"].clone())?,
            &candidate,
            0,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let directory =
            std::env::temp_dir().join(format!("aic-root-lower-stop-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let budget = SearchMemoryBudget::new(128 << 20);
        let CatalogCollectStart::Ready(mut collector) = RootCatalogCollector::new(
            &source,
            Arc::clone(&source),
            &factory,
            None,
            Device::Cpu,
            &directory.join("root"),
            RootNoiseMode::ActionIdentity,
            7,
            "case",
            8,
            0,
            0.0,
            &budget,
        )?
        else {
            anyhow::bail!("lower root admission stopped");
        };
        let mut counters = Counters::default();
        let progress = collector.advance(&factory, &mut counters, &budget)?;
        match progress {
            CatalogCollectProgress::Stopped(CatalogCollectStop::Lower(diagnostic)) => {
                assert_eq!(diagnostic.code, "hierarchical.request-work-limit");
            }
            CatalogCollectProgress::NoActions(diagnostic) => {
                panic!("lower work cutoff was reported as no actions: {diagnostic:?}");
            }
            _ => panic!("lower work cutoff was not classified"),
        }
        drop(collector);
        assert_eq!(budget.live_bytes(), 0);
        assert_eq!(fs::read_dir(&directory)?.count(), 0);
        fs::remove_dir(directory)?;
        Ok(())
    }

    #[test]
    fn collector_retains_source_across_memory_stop_and_resolves_shortlist() -> anyhow::Result<()> {
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let route_config: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-route-guidance/config.json"
        ))?;
        let panel: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-route-guidance/training-panel.json"
        ))?;
        let candidate: CandidateConfig = serde_json::from_value(panel["candidate"].clone())?;
        let model_config = serde_json::from_value(route_config["model"].clone())?;
        let (order, state) = GuidanceModel::initial_state(&model_config, 17)?;
        let model = GuidanceModel::load(&model_config, &order, &state, Device::Cpu)?;
        let factory = Factory::new(
            &model,
            "checkpoint",
            Device::Cpu,
            test_support::domain(),
            &candidate,
            0,
            7,
            1,
            0.25,
            false,
            None,
        )?;
        let source = Arc::new(test_support::session(64));
        let native = source.learning_placement_domain(&factory.domain)?;
        let mut native_count = 0;
        for kind in [CommandKind::Place, CommandKind::PlaceTerminal] {
            let mut offset = 0;
            loop {
                let page = native.command_page(kind, offset, 7)?;
                native_count += page.commands.len();
                offset = page.next_offset;
                if page.exhausted {
                    break;
                }
            }
        }
        let directory =
            std::env::temp_dir().join(format!("aic-root-collector-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let budget = SearchMemoryBudget::new(64 << 20);
        assert!(
            RootCatalogCollector::new(
                &source,
                Arc::new((*source).clone()),
                &factory,
                None,
                Device::Cpu,
                &directory.join("root"),
                RootNoiseMode::ActionIdentity,
                7,
                "case",
                8,
                2,
                0.25,
                &budget,
            )
            .is_err()
        );
        let CatalogCollectStart::Ready(mut collector) = RootCatalogCollector::new(
            &source,
            Arc::clone(&source),
            &factory,
            None,
            Device::Cpu,
            &directory.join("root"),
            RootNoiseMode::ActionIdentity,
            7,
            "case",
            8,
            2,
            0.25,
            &budget,
        )?
        else {
            anyhow::bail!("collector admission stopped");
        };
        let mut counters = Counters::default();
        let peer = crate::hierarchical_search::TreeReservation::new(
            &budget,
            budget.limit_bytes() - budget.live_bytes() - 1024,
        )
        .unwrap();
        assert!(matches!(
            collector.advance(&factory, &mut counters, &budget)?,
            CatalogCollectProgress::Stopped(CatalogCollectStop::Memory(_))
        ));
        drop(peer);
        assert!(matches!(
            collector.advance(&factory, &mut counters, &budget)?,
            CatalogCollectProgress::Ready
        ));
        let mut collected = collector.finish();
        assert_eq!(collected.files.count as usize, native_count);
        assert_eq!(collected.placement_count as usize, native_count);
        assert_eq!(collected.shortlist.len(), 8);
        let resolved = collected.resolved_placements.as_ref().unwrap();
        assert_eq!(resolved.choices.len(), 8);
        assert!(collected.shortlist.iter().all(|choice| {
            resolved
                .choices
                .iter()
                .any(|row| row.choice.ordinal == choice.ordinal)
        }));
        let dense_budget = SearchMemoryBudget::new(128 << 20);
        let mut nodes = vec![crate::hierarchical_search::Node {
            session: Arc::clone(&source),
            memory: crate::hierarchical_search::TreeReservation::new(&dense_budget, 0).unwrap(),
            visits: 0,
            value: None,
            actions: None,
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        crate::hierarchical_search::ensure_node_expanded(
            &mut nodes,
            0,
            &factory,
            &mut Counters::default(),
            true,
            &dense_budget,
        )?;
        let dense = nodes[0].actions.as_ref().unwrap();
        assert_eq!(dense.len(), native_count);
        let dense_initial_value = nodes[0].value.unwrap();
        assert!((collected.initial_root_value - dense_initial_value).abs() < 1e-12);
        let sparse = collected.sparse_edges(&budget)?.unwrap();
        assert_eq!(sparse.edges.len(), 8);
        for edge in &sparse.edges {
            let reference = dense
                .iter()
                .find(|row| row.identity == edge.identity)
                .expect("physical shortlist ID in dense root");
            assert_eq!(edge.features, reference.features);
            assert_eq!(edge.entity_index, reference.entity_index);
            assert_eq!(
                edge.anchor_score.to_bits(),
                reference.anchor_score.to_bits()
            );
            assert!((edge.prior - reference.prior).abs() < 1e-12);
            assert!((edge.anchor_prior - reference.anchor_prior).abs() < 1e-12);
        }
        let mut search_config: crate::hierarchical_search::Config = serde_json::from_str(
            include_str!("../../../../../docs/benchmarks/hierarchical-curriculum/search.json"),
        )?;
        search_config.candidate = candidate.clone();
        search_config.bounds = factory.domain.placement;
        search_config.horizon = 1;
        search_config.repair_max_removed = 0;
        search_config.simulations_per_move =
            crate::hierarchical_search::minimum_sequential_halving_simulations(8);
        let controls = crate::hierarchical_search::GumbelControls {
            candidates: 8,
            anchor_candidates: 2,
            anchor_policy_mass: 0.25,
            seed: 7,
            value_scale: 0.1,
            max_visit_init: 50.0,
            gumbel_assignment: crate::hierarchical_search::GumbelAssignment::ActionIdentity,
        };
        let dense_decision = crate::hierarchical_search::gumbel_sequential_halving(
            &mut nodes,
            0,
            &factory,
            &search_config,
            &mut Counters::default(),
            &dense_budget,
            controls,
            "case",
            &collected.state_identity,
        )?;
        let mut sparse = Some(sparse);
        let super::super::search::SparseSearchStart::Ready(mut search) =
            super::super::search::SparseGumbelSearch::new(&collected, &mut sparse, &budget)
        else {
            anyhow::bail!("sparse root admission stopped");
        };
        loop {
            match search.step(
                &mut collected,
                &factory,
                &search_config,
                controls,
                &mut counters,
                &budget,
            )? {
                super::super::search::SparseSearchStep::Running => {}
                super::super::search::SparseSearchStep::Complete => break,
                super::super::search::SparseSearchStep::MemoryStop(stop) => {
                    anyhow::bail!("sparse root stopped: {stop:?}")
                }
            }
        }
        let sparse_result = search.finish(&collected);
        assert_eq!(sparse_result.simulations, dense_decision.simulations);
        let dense_selected = dense_decision.selected.map(|index| {
            nodes[0].actions.as_ref().unwrap()[index]
                .identity
                .as_slice()
        });
        assert_eq!(sparse_result.selected_id.as_deref(), dense_selected);
        assert_eq!(
            collected
                .shortlist
                .iter()
                .map(|choice| (choice.rank as usize, choice.gumbel_bits))
                .collect::<Vec<_>>(),
            dense_decision
                .shortlist
                .iter()
                .map(|choice| (choice.initial_rank, choice.gumbel_bits))
                .collect::<Vec<_>>()
        );
        for visit in &sparse_result.visits {
            let identity = &collected
                .shortlist
                .iter()
                .find(|choice| choice.ordinal == visit.ordinal)
                .and_then(|choice| {
                    dense_decision
                        .shortlist
                        .iter()
                        .find(|row| row.initial_rank == choice.rank as usize)
                })
                .expect("visited physical shortlist action")
                .physical_id;
            let dense_edge = nodes[0]
                .actions
                .as_ref()
                .unwrap()
                .iter()
                .find(|edge| &edge.identity == identity)
                .unwrap();
            assert_eq!(visit.visits, dense_edge.visits);
            assert!((visit.value_sum - dense_edge.value_sum).abs() < 1e-12);
        }
        let scalars = collected.files.target_scalars(
            collected.initial_root_value,
            &sparse_result.visits,
            controls.value_scale,
            controls.max_visit_init,
            controls.anchor_policy_mass,
            &budget,
        )?;
        let dense_targets = dense_decision.policy_targets.as_ref().unwrap();
        let dense_denominator = dense_targets.iter().sum::<f64>();
        let mut maximum_error = 0.0f64;
        collected.files.scan_sorted(|rank, row| {
            let edge = &nodes[0].actions.as_ref().unwrap()[rank as usize];
            assert!((row.prior - edge.prior).abs() < 1e-12);
            let q = scalars
                .visited_q
                .binary_search_by_key(&row.ordinal, |entry| entry.0)
                .map_or(scalars.q0, |index| scalars.visited_q[index].1);
            let weight = row.prior * (1.0 - controls.anchor_policy_mass)
                + row.anchor_prior * controls.anchor_policy_mass;
            let actual = ((weight.max(f64::MIN_POSITIVE).ln() + q - scalars.log_z).exp()
                / scalars.second_denominator) as f32;
            let expected = (dense_targets[rank as usize] / dense_denominator) as f32;
            maximum_error = maximum_error.max((f64::from(actual) - f64::from(expected)).abs());
        })?;
        assert!(
            maximum_error <= 1e-6,
            "maximum target error {maximum_error}"
        );
        let dense_by_id = nodes[0]
            .actions
            .as_ref()
            .unwrap()
            .iter()
            .zip(dense_targets)
            .map(|(edge, target)| {
                (
                    edge.identity.clone(),
                    (
                        edge.features,
                        edge.entity_index,
                        (*target / dense_denominator) as f32,
                    ),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        let mut expected_source_rows = Vec::new();
        for kind in [CommandKind::Place, CommandKind::PlaceTerminal] {
            let mut offset = 0;
            loop {
                let page = native.command_page(kind, offset, 7)?;
                for factored in page.commands {
                    let identity = crate::hierarchical_search::physical_action_identity(
                        &MacroAction::Command(factored.command.command),
                    )?;
                    expected_source_rows.push(dense_by_id[&identity]);
                }
                offset = page.next_offset;
                if page.exhausted {
                    break;
                }
            }
        }
        assert_eq!(expected_source_rows.len(), native_count);
        drop(nodes);
        assert_eq!(dense_budget.live_bytes(), 0);
        let binding = super::super::publication::CatalogBinding {
            source_sha256: collected.state_identity.clone(),
            source_revision: source.revision(),
            limits_sha256: "limits".into(),
            effective_config_sha256: "effective-config".into(),
            catalog_sha256: "catalog".into(),
            outer_checkpoint_sha256: "checkpoint".into(),
        };
        let report_path = directory.join("report.json");
        let mut collected = Some(collected);
        let mut sparse_result = Some(sparse_result);
        let peer = crate::hierarchical_search::TreeReservation::new(
            &budget,
            budget.limit_bytes() - budget.live_bytes() - 1024,
        )
        .unwrap();
        assert!(matches!(
            super::super::publication::StagedRootCatalog::new(
                &mut collected,
                &mut sparse_result,
                &binding,
                &report_path,
                "case",
                controls.value_scale,
                controls.max_visit_init,
                controls.anchor_policy_mass,
                &budget,
            )?,
            Err(super::super::CatalogStop::Memory(_))
        ));
        assert!(collected.is_some() && sparse_result.is_some());
        drop(peer);
        let mut stages = vec![match super::super::publication::StagedRootCatalog::new(
            &mut collected,
            &mut sparse_result,
            &binding,
            &report_path,
            "case",
            controls.value_scale,
            controls.max_visit_init,
            controls.anchor_policy_mass,
            &budget,
        )? {
            Ok(stage) => stage,
            Err(stop) => anyhow::bail!("catalog publication stopped: {stop:?}"),
        }];
        let report = serde_json::json!({"status": "complete"});
        assert!(
            super::super::publication::publish_with_catalogs(&report_path, &report, &mut stages)?
                .is_ok()
        );
        let descriptor = &stages[0].descriptor;
        let published: serde_json::Value = serde_json::from_slice(&fs::read(&report_path)?)?;
        assert_eq!(
            published["schema"],
            super::super::publication::REPORT_SCHEMA
        );
        assert_eq!(
            published["root_catalogs"][0]["sidecar_name"],
            descriptor.sidecar_name
        );
        let super::super::replay::RootReplayStart::Ready(mut paged) =
            super::super::replay::RootReplaySource::open(
                &report_path,
                descriptor,
                &binding,
                &source,
                &factory.domain,
                7,
                controls.anchor_policy_mass,
                &budget,
            )?
        else {
            anyhow::bail!("placement replay admission stopped");
        };
        let peer = crate::hierarchical_search::TreeReservation::new(
            &budget,
            budget.limit_bytes() - budget.live_bytes() - 1024,
        )
        .unwrap();
        assert!(matches!(
            paged.next_page(&budget)?,
            super::super::replay::RootReplayStep::MemoryStop(_)
        ));
        drop(peer);
        for _ in 0..2 {
            let mut cursor = 0usize;
            loop {
                match paged.next_page(&budget)? {
                    super::super::replay::RootReplayStep::Page(page) => {
                        assert_eq!(page.ordinal_start, cursor as u64);
                        for index in 0..page.targets.len() {
                            let expected = expected_source_rows[cursor];
                            assert_eq!(page.features[index], expected.0);
                            assert_eq!(page.entity_indices[index], expected.1);
                            assert!((page.targets[index] - expected.2).abs() <= 1e-6);
                            cursor += 1;
                        }
                    }
                    super::super::replay::RootReplayStep::Complete => break,
                    super::super::replay::RootReplayStep::MemoryStop(stop) => {
                        anyhow::bail!("placement replay admission stopped: {stop:?}")
                    }
                }
            }
            assert_eq!(cursor, native_count);
            paged.rewind()?;
        }
        drop(paged);
        let mut reader =
            super::super::publication::open_replay(&report_path, descriptor, &binding)?;
        let mut first_prior = [0u8; 16];
        std::io::Read::read_exact(&mut reader, &mut first_prior)?;
        assert!(f64::from_le_bytes(first_prior[..8].try_into().unwrap()).is_finite());
        assert!(
            super::super::publication::open_replay(
                &report_path,
                descriptor,
                &super::super::publication::CatalogBinding {
                    limits_sha256: "other-limits".into(),
                    ..binding.clone()
                }
            )
            .is_err()
        );
        let sidecar_path = directory.join(&descriptor.sidecar_name);
        let orphan = directory.join(".report.json.root-orphan.priors");
        let interrupted_sort = directory.join("root.prior-run-0.tmp");
        fs::write(&orphan, b"orphan")?;
        fs::write(&interrupted_sort, b"unpublished")?;
        fs::write(
            &report_path,
            serde_json::to_vec(&serde_json::json!({
                "schema": crate::hierarchical_search::REPORT_SCHEMA,
                "cases": [{"mcts": {"root_records": [{"target": {
                    "kind": "gumbel-catalog",
                    "data": {"descriptor": {"sidecar_name": descriptor.sidecar_name}}
                }}]}}]
            }))?,
        )?;
        super::super::publication::recover_unpublished(&report_path, &[directory.join("root")])?;
        assert!(sidecar_path.exists());
        assert!(!orphan.exists() && !interrupted_sort.exists());
        let mut bytes = fs::read(&sidecar_path)?;
        bytes[0] ^= 1;
        fs::write(&sidecar_path, bytes)?;
        assert!(
            super::super::publication::open_replay(&report_path, descriptor, &binding).is_err()
        );
        drop(reader);
        drop(stages);
        assert_eq!(budget.live_bytes(), 0);
        fs::remove_file(&sidecar_path)?;
        fs::remove_file(&report_path)?;
        assert_eq!(fs::read_dir(&directory)?.count(), 0);
        fs::remove_dir(directory)?;
        Ok(())
    }

    #[test]
    fn collector_keeps_all_initial_lower_witnesses() -> anyhow::Result<()> {
        use std::time::Instant;

        let _guard = crate::TORCH_TEST.lock().unwrap();
        let panel: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-route-guidance/training-panel.json"
        ))?;
        let route_config: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-route-guidance/config.json"
        ))?;
        let case = &panel["cases"][0];
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            test_support::catalogs(),
            serde_json::from_value(panel["limits"].clone())?,
        )?;
        for command in case["prefix"].as_array().unwrap() {
            assert!(
                session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        assert!(!session.has_unplaced_entities());
        let candidate: CandidateConfig = serde_json::from_value(panel["candidate"].clone())?;
        let model_config = serde_json::from_value(route_config["model"].clone())?;
        let (order, state) = GuidanceModel::initial_state(&model_config, 17)?;
        let model = GuidanceModel::load(&model_config, &order, &state, Device::Cpu)?;
        let factory = Factory::new(
            &model,
            "checkpoint",
            Device::Cpu,
            serde_json::from_value(route_config["learning_domain"].clone())?,
            &candidate,
            0,
            7,
            1,
            0.25,
            false,
            None,
        )?;
        let source = Arc::new(session);
        let dense_budget = SearchMemoryBudget::new(128 << 20);
        let mut nodes = vec![crate::hierarchical_search::Node {
            session: Arc::clone(&source),
            memory: crate::hierarchical_search::TreeReservation::new(&dense_budget, 0).unwrap(),
            visits: 0,
            value: None,
            actions: None,
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        crate::hierarchical_search::ensure_node_expanded(
            &mut nodes,
            0,
            &factory,
            &mut Counters::default(),
            true,
            &dense_budget,
        )?;
        let expected_ids = nodes[0]
            .actions
            .as_ref()
            .unwrap()
            .iter()
            .map(|edge| edge.identity.clone())
            .collect::<Vec<_>>();
        let expected_rows = nodes[0]
            .actions
            .as_ref()
            .unwrap()
            .iter()
            .map(|edge| {
                (
                    edge.identity.clone(),
                    edge.features,
                    edge.entity_index,
                    edge.anchor_score,
                    edge.prior,
                )
            })
            .collect::<Vec<_>>();
        let initial_actions = nodes[0]
            .actions
            .as_ref()
            .unwrap()
            .iter()
            .map(|edge| (edge.identity.clone(), edge.action.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let directory =
            std::env::temp_dir().join(format!("aic-lower-root-collector-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let budget = SearchMemoryBudget::new(128 << 20);
        let collect_started = Instant::now();
        let CatalogCollectStart::Ready(mut collector) = RootCatalogCollector::new(
            &source,
            Arc::clone(&source),
            &factory,
            None,
            Device::Cpu,
            &directory.join("root"),
            RootNoiseMode::ActionIdentity,
            7,
            "case",
            1,
            1,
            0.25,
            &budget,
        )?
        else {
            anyhow::bail!("collector admission stopped");
        };
        let mut counters = Counters::default();
        assert!(matches!(
            collector.advance(&factory, &mut counters, &budget)?,
            CatalogCollectProgress::Ready
        ));
        let mut collected = collector.finish();
        let collect_seconds = collect_started.elapsed().as_secs_f64();
        let sorted_bytes = fs::metadata(&collected.files.sorted_path)?.len();
        assert_eq!(collected.placement_count, 0);
        assert!(collected.resolved_placements.is_none());
        assert!(collected.files.count > 0);
        assert_eq!(
            collected.files.count as usize,
            collected.witnesses.initial_count
        );
        assert_eq!(collected.witnesses.initial_count, expected_ids.len());
        assert_eq!(
            collected
                .witnesses
                .witnesses
                .iter()
                .map(|witness| witness.identity.as_slice())
                .collect::<Vec<_>>(),
            expected_ids.iter().map(Vec::as_slice).collect::<Vec<_>>()
        );
        assert!(
            collected
                .shortlist
                .iter()
                .all(|choice| { collected.witnesses.initial(choice.ordinal).is_some() })
        );
        let sparse = collected.sparse_edges(&budget)?.unwrap();
        for edge in &sparse.edges {
            let reference = expected_rows
                .iter()
                .find(|row| row.0 == edge.identity)
                .expect("shortlisted lower witness in dense root");
            assert_eq!(edge.features, reference.1);
            assert_eq!(edge.entity_index, reference.2);
            assert_eq!(edge.anchor_score.to_bits(), reference.3.to_bits());
            assert!((edge.prior - reference.4).abs() < 1e-12);
        }
        let mut search_config: crate::hierarchical_search::Config = serde_json::from_str(
            include_str!("../../../../../docs/benchmarks/hierarchical-curriculum/search.json"),
        )?;
        search_config.candidate = candidate.clone();
        search_config.bounds = factory.domain.placement;
        search_config.horizon = 1;
        search_config.repair_max_removed = 0;
        search_config.simulations_per_move =
            crate::hierarchical_search::minimum_sequential_halving_simulations(8);
        let controls = crate::hierarchical_search::GumbelControls {
            candidates: 1,
            anchor_candidates: 1,
            anchor_policy_mass: 0.25,
            seed: 7,
            value_scale: 0.1,
            max_visit_init: 50.0,
            gumbel_assignment: crate::hierarchical_search::GumbelAssignment::ActionIdentity,
        };
        let dense_decision = crate::hierarchical_search::gumbel_sequential_halving(
            &mut nodes,
            0,
            &factory,
            &search_config,
            &mut Counters::default(),
            &dense_budget,
            controls,
            "case",
            &collected.state_identity,
        )?;
        let mut sparse = Some(sparse);
        let search_started = Instant::now();
        let super::super::search::SparseSearchStart::Ready(mut search) =
            super::super::search::SparseGumbelSearch::new(&collected, &mut sparse, &budget)
        else {
            anyhow::bail!("sparse lower root admission stopped");
        };
        let root_memory_before_updates = search.root_memory_bytes();
        loop {
            match search.step(
                &mut collected,
                &factory,
                &search_config,
                controls,
                &mut counters,
                &budget,
            )? {
                super::super::search::SparseSearchStep::Running => {}
                super::super::search::SparseSearchStep::Complete => break,
                super::super::search::SparseSearchStep::MemoryStop(stop) => {
                    anyhow::bail!("sparse lower root stopped: {stop:?}")
                }
            }
        }
        assert!(search.root_memory_bytes() > root_memory_before_updates);
        let sparse_result = search.finish(&collected);
        let search_seconds = search_started.elapsed().as_secs_f64();
        assert_eq!(collected.shortlist.len(), 1);
        assert!(collected.witnesses.witnesses.len() > collected.witnesses.initial_count);
        assert!(collected.lower_loaded_pages > 1);
        let outside_shortlist = collected.witnesses.witnesses[..collected.witnesses.initial_count]
            .iter()
            .find(|witness| {
                !collected
                    .shortlist
                    .iter()
                    .any(|choice| choice.ordinal == witness.initial_ordinal.unwrap())
            })
            .expect("one initial route stays outside the shortlist");
        let MacroAction::Route(updated_route) = &outside_shortlist.action else {
            panic!("outside-shortlist witness is a route");
        };
        let Some(MacroAction::Route(initial_route)) =
            initial_actions.get(&outside_shortlist.identity)
        else {
            panic!("original outside-shortlist route is retained");
        };
        assert!(
            updated_route.additional_provenance.len() > initial_route.additional_provenance.len()
        );
        let shortlisted = collected
            .witnesses
            .initial(collected.shortlist[0].ordinal)
            .expect("shortlisted route remains retained");
        let MacroAction::Route(shortlisted_route) = &shortlisted.action else {
            panic!("shortlisted witness is a route");
        };
        let Some(MacroAction::Route(initial_shortlisted_route)) =
            initial_actions.get(&shortlisted.identity)
        else {
            panic!("original shortlisted route is retained");
        };
        assert!(
            shortlisted_route.additional_provenance.len()
                > initial_shortlisted_route.additional_provenance.len()
        );
        let dense_selected = dense_decision.selected.map(|index| {
            nodes[0].actions.as_ref().unwrap()[index]
                .identity
                .as_slice()
        });
        assert_eq!(sparse_result.selected_id.as_deref(), dense_selected);
        assert_eq!(sparse_result.simulations, dense_decision.simulations);
        let dense_edges = nodes[0].actions.as_ref().unwrap();
        assert_eq!(dense_edges.len(), collected.witnesses.witnesses.len());
        let dense_targets = dense_decision.policy_targets.as_ref().unwrap();
        let dense_target_by_id = dense_edges
            .iter()
            .zip(dense_targets)
            .map(|(edge, target)| (edge.identity.clone(), *target as f32))
            .collect::<std::collections::HashMap<_, _>>();
        let dense_policy_by_id = dense_edges
            .iter()
            .zip(dense_targets)
            .map(|(edge, target)| {
                (
                    edge.identity.clone(),
                    (edge.features, edge.entity_index, *target as f32),
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        for witness in &collected.witnesses.witnesses {
            let edge = dense_edges
                .iter()
                .find(|edge| edge.identity == witness.identity)
                .expect("complete lower witness stays replayable");
            assert_eq!(edge.features, witness.features);
            assert_eq!(
                serde_json::to_value(&edge.action)?,
                serde_json::to_value(&witness.action)?
            );
            if witness.initial_ordinal.is_none() {
                assert_eq!(dense_target_by_id[&witness.identity], 0.0);
            }
        }
        for visit in &sparse_result.visits {
            let initial = &collected.witnesses.initial(visit.ordinal).unwrap().identity;
            let edge = dense_edges
                .iter()
                .find(|edge| &edge.identity == initial)
                .unwrap();
            assert_eq!(visit.visits, edge.visits);
            assert!((visit.value_sum - edge.value_sum).abs() < 1e-12);
        }
        drop(nodes);
        assert_eq!(dense_budget.live_bytes(), 0);
        let binding = super::super::publication::CatalogBinding {
            source_sha256: collected.state_identity.clone(),
            source_revision: source.revision(),
            limits_sha256: "limits".into(),
            effective_config_sha256: "effective-config".into(),
            catalog_sha256: "catalog".into(),
            outer_checkpoint_sha256: "checkpoint".into(),
        };
        let report_path = directory.join("report.json");
        let mut collected = Some(collected);
        let mut sparse_result = Some(sparse_result);
        let publication_started = Instant::now();
        let mut stages = vec![match super::super::publication::StagedRootCatalog::new(
            &mut collected,
            &mut sparse_result,
            &binding,
            &report_path,
            "case",
            controls.value_scale,
            controls.max_visit_init,
            controls.anchor_policy_mass,
            &budget,
        )? {
            Ok(stage) => stage,
            Err(stop) => anyhow::bail!("catalog publication stopped: {stop:?}"),
        }];
        let sidecar_path = directory.join(&stages[0].descriptor.sidecar_name);
        assert!(sidecar_path.exists());
        assert!(
            super::super::publication::publish_with_catalogs(
                &report_path,
                &FailSerialize,
                &mut stages,
            )
            .is_err()
        );
        assert!(sidecar_path.exists());
        let report = serde_json::json!({"status": "complete"});
        assert!(
            super::super::publication::publish_with_catalogs(&report_path, &report, &mut stages,)?
                .is_ok()
        );
        let descriptor = &stages[0].descriptor;
        assert_eq!(descriptor.initial_eligible_count, 2);
        assert_eq!(
            descriptor.nonplacement_witnesses.len(),
            dense_target_by_id.len()
        );
        let super::super::replay::RootReplayStart::Ready(mut paged) =
            super::super::replay::RootReplaySource::open(
                &report_path,
                descriptor,
                &binding,
                &source,
                &factory.domain,
                3,
                controls.anchor_policy_mass,
                &budget,
            )?
        else {
            anyhow::bail!("published root replay admission stopped");
        };
        for _ in 0..2 {
            let mut cursor = 0usize;
            loop {
                match paged.next_page(&budget)? {
                    super::super::replay::RootReplayStep::Page(page) => {
                        assert_eq!(page.ordinal_start, cursor.min(2) as u64);
                        for index in 0..page.targets.len() {
                            let witness = &descriptor.nonplacement_witnesses[cursor];
                            assert_eq!(page.features[index], witness.features);
                            assert_eq!(page.entity_indices[index], witness.entity_index);
                            assert_eq!(page.targets[index], dense_target_by_id[&witness.identity]);
                            cursor += 1;
                        }
                    }
                    super::super::replay::RootReplayStep::Complete => break,
                    super::super::replay::RootReplayStep::MemoryStop(_) => {
                        anyhow::bail!("published root replay page admission stopped")
                    }
                }
            }
            assert_eq!(cursor, descriptor.nonplacement_witnesses.len());
            paged.rewind()?;
        }
        drop(paged);
        let mut replay =
            super::super::publication::open_replay(&report_path, descriptor, &binding)?;
        let mut maximum_target_error = 0.0f64;
        for witness in &descriptor.nonplacement_witnesses {
            let actual = if let Some(ordinal) = witness.initial_ordinal {
                let mut row = [0u8; 16];
                std::io::Read::read_exact(&mut replay, &mut row)?;
                assert!(ordinal < descriptor.initial_eligible_count);
                let prior = f64::from_le_bytes(row[..8].try_into().unwrap());
                let anchor = f64::from_le_bytes(row[8..].try_into().unwrap());
                let q = descriptor
                    .target
                    .visited_q
                    .binary_search_by_key(&ordinal, |entry| entry.0)
                    .map_or(descriptor.target.q0, |index| {
                        descriptor.target.visited_q[index].1
                    });
                let weight = prior * (1.0 - controls.anchor_policy_mass)
                    + anchor * controls.anchor_policy_mass;
                ((weight.max(f64::MIN_POSITIVE).ln() + q - descriptor.target.log_z).exp()
                    / descriptor.target.second_denominator) as f32
            } else {
                0.0
            };
            maximum_target_error = maximum_target_error
                .max((f64::from(actual) - f64::from(dense_target_by_id[&witness.identity])).abs());
        }
        assert!(
            maximum_target_error <= 1e-6,
            "late lower target error {maximum_target_error}"
        );
        let mut remainder = [0u8; 1];
        assert_eq!(std::io::Read::read(&mut replay, &mut remainder)?, 0);
        assert!(descriptor.target.second_denominator.is_finite());
        assert!(descriptor.target.second_denominator > 0.0);
        // Compare every parameter gradient through the actual published
        // descriptor, verified ledger and two replay passes.
        {
            use crate::{
                outer_model::{
                    Batch as OuterBatch, Model as OuterModel, StreamedPolicyRoot,
                    StreamedPolicyStatistics, ValueTarget, value_loss,
                },
                schema::{ConstructiveModelConfig, OuterModelConfig},
            };
            use aic_torch::tch::{Kind, Tensor};

            let config = OuterModelConfig {
                schema: OuterModelConfig::SCHEMA.into(),
                encoder: ConstructiveModelConfig {
                    schema: ConstructiveModelConfig::SCHEMA.into(),
                    width: 16,
                    message_rounds: 1,
                },
                width: 16,
            };
            let (order, state) = OuterModel::initial_state(&config, 29)?;
            let dense_model = OuterModel::load(&config, &order, &state, Device::Cpu)?;
            let streamed_model = OuterModel::load(&config, &order, &state, Device::Cpu)?;
            let input = source
                .macro_learning_input(&factory.domain)
                .map_err(anyhow::Error::new)?;
            let mut rows = Vec::new();
            let mut entities = Vec::new();
            let mut targets = Vec::new();
            for witness in &descriptor.nonplacement_witnesses {
                let (row, entity, target) = dense_policy_by_id[&witness.identity];
                rows.push(row);
                entities.push(entity);
                targets.push(target);
            }
            let values = ValueTarget {
                completion: 0.0,
                additional_fraction: 0.25,
                next_progress: 0.0,
                blocked_fraction: 0.75,
                area: None,
                transport: None,
                turns: None,
            };
            let dense_batch = OuterBatch::from_rows(
                &[&input],
                &[rows.as_slice()],
                &[entities.as_slice()],
                Device::Cpu,
            )?;
            let prediction = dense_model.forward(&dense_batch)?;
            let dense_loss = -(Tensor::from_slice(&targets) * prediction.logp).sum(Kind::Float)
                + value_loss(&prediction.values, &[values], Device::Cpu)?;
            let dense_scalar = f64::try_from(&dense_loss)?;
            dense_loss.backward();

            let ledger = super::super::replay::VerifiedRootLedger::open(
                &report_path,
                Arc::clone(descriptor),
                &binding,
                &source,
            )?;
            let super::super::replay::RootReplayStart::Ready(mut cursor) =
                super::super::replay::RootReplaySource::from_verified(
                    &ledger,
                    &source,
                    &factory.domain,
                    3,
                    controls.anchor_policy_mass,
                    &budget,
                )?
            else {
                anyhow::bail!("verified gradient replay admission stopped")
            };
            let streamed = StreamedPolicyRoot::new(&streamed_model, &input, Device::Cpu)?;
            let mut statistics = StreamedPolicyStatistics::new(Device::Cpu);
            loop {
                match cursor.next_page(&budget)? {
                    super::super::replay::RootReplayStep::Page(page) => statistics.observe(
                        &streamed.frozen_logits(&page.features, &page.entity_indices)?,
                        &page.targets,
                    )?,
                    super::super::replay::RootReplayStep::Complete => break,
                    super::super::replay::RootReplayStep::MemoryStop(stop) => {
                        anyhow::bail!("gradient normalizer stopped: {stop:?}")
                    }
                }
            }
            let streamed_value = value_loss(streamed.values(), &[values], Device::Cpu)?;
            let streamed_scalar =
                f64::try_from(statistics.true_cross_entropy()? + &streamed_value)?;
            cursor.rewind()?;
            loop {
                match cursor.next_page(&budget)? {
                    super::super::replay::RootReplayStep::Page(page) => streamed.backward_page(
                        &page.features,
                        &page.entity_indices,
                        &page.targets,
                        &statistics,
                        1,
                    )?,
                    super::super::replay::RootReplayStep::Complete => break,
                    super::super::replay::RootReplayStep::MemoryStop(stop) => {
                        anyhow::bail!("gradient pass stopped: {stop:?}")
                    }
                }
            }
            (streamed.encoder_surrogate()? + streamed_value).backward();
            assert!((streamed_scalar - dense_scalar).abs() <= 2e-6);
            for ((dense_name, dense_value), (stream_name, stream_value)) in dense_model
                .parameters
                .iter()
                .zip(&streamed_model.parameters)
            {
                assert_eq!(dense_name, stream_name);
                let dense_gradient = dense_value.grad();
                let stream_gradient = stream_value.grad();
                assert_eq!(
                    dense_gradient.defined(),
                    stream_gradient.defined(),
                    "{dense_name}"
                );
                if dense_gradient.defined() {
                    assert!(
                        bool::try_from(dense_gradient.allclose(
                            &stream_gradient,
                            1e-4,
                            2e-5,
                            false,
                        ))?,
                        "catalog gradient differs for {dense_name}"
                    );
                }
            }
            drop(cursor);
            drop(ledger);
            drop(streamed);
        }
        let publication_seconds = publication_started.elapsed().as_secs_f64();
        let sidecar_bytes = fs::metadata(&sidecar_path)?.len();
        let report_bytes = fs::metadata(&report_path)?.len();
        let phase_file_peak_bytes =
            (descriptor.initial_eligible_count * (20 + 32 + 16)).max(sidecar_bytes + report_bytes);
        eprintln!(
            "plan03 sparse lower initial={} total={} outside_shortlist_update=true shortlist_update=true maximum_target_error={maximum_target_error:.3e} collect_s={collect_seconds:.3} search_s={search_seconds:.3} publication_and_replay_s={publication_seconds:.3} sorted_bytes={sorted_bytes} sidecar_bytes={sidecar_bytes} report_bytes={report_bytes} phase_file_peak_bytes={phase_file_peak_bytes} retained_bytes={} peak_tree_bytes={}",
            descriptor.initial_eligible_count,
            descriptor.nonplacement_witnesses.len(),
            sidecar_bytes + report_bytes,
            budget.peak_bytes()
        );
        drop(replay);
        drop(stages);
        fs::remove_file(sidecar_path)?;
        fs::remove_file(report_path)?;
        assert_eq!(budget.live_bytes(), 0);
        assert_eq!(fs::read_dir(&directory)?.count(), 0);
        fs::remove_dir(directory)?;
        Ok(())
    }

    #[test]
    fn page_prior_injected_dense_reference_covers_changed_selection() -> anyhow::Result<()> {
        use std::collections::HashMap;

        use crate::{
            hierarchical_search::{
                Config, GumbelAssignment, GumbelControls, RootSelection, physical_action_identity,
            },
            outer_model::{Batch as OuterBatch, Model as OuterModel},
            schema::{ConstructiveModelConfig, OuterModelConfig},
        };
        use aic_torch::tch;

        let _guard = crate::TORCH_TEST.lock().unwrap();
        let route_config: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-route-guidance/config.json"
        ))?;
        let panel: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-route-guidance/training-panel.json"
        ))?;
        let mut candidate: CandidateConfig = serde_json::from_value(panel["candidate"].clone())?;
        let mut domain = test_support::domain();
        domain.placement.max_x = 5;
        domain.placement.max_y = 3;
        candidate.bounds = domain.placement;
        let guidance_config = serde_json::from_value(route_config["model"].clone())?;
        let (guidance_order, guidance_state) = GuidanceModel::initial_state(&guidance_config, 17)?;
        let guidance = GuidanceModel::load(
            &guidance_config,
            &guidance_order,
            &guidance_state,
            Device::Cpu,
        )?;
        let outer_config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: ConstructiveModelConfig {
                schema: ConstructiveModelConfig::SCHEMA.into(),
                width: 16,
                message_rounds: 1,
            },
            width: 16,
        };
        let (outer_order, outer_state) = OuterModel::initial_state(&outer_config, 151)?;
        let outer = OuterModel::load(&outer_config, &outer_order, &outer_state, Device::Cpu)?;
        let runner = OuterRunner::Eager(&outer);
        let factory = Factory::new(
            &guidance,
            "initial-guidance-seed-17",
            Device::Cpu,
            domain,
            &candidate,
            0,
            7,
            1,
            0.0,
            false,
            Some(&outer),
        )?;
        let source = Arc::new(test_support::session(64));
        let dense_budget = SearchMemoryBudget::new(256 << 20);
        let mut nodes = vec![crate::hierarchical_search::Node {
            session: Arc::clone(&source),
            memory: TreeReservation::new(&dense_budget, 0).unwrap(),
            visits: 0,
            value: None,
            actions: None,
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        }];
        crate::hierarchical_search::ensure_node_expanded(
            &mut nodes,
            0,
            &factory,
            &mut Counters::default(),
            true,
            &dense_budget,
        )?;
        let initial_root_value = nodes[0].value.unwrap();
        let mut dense_by_id = nodes[0]
            .actions
            .take()
            .unwrap()
            .into_iter()
            .map(|edge| (edge.identity.clone(), edge))
            .collect::<HashMap<_, _>>();
        let directory =
            std::env::temp_dir().join(format!("aic-real-near-tie-{}", std::process::id()));
        fs::create_dir(&directory)?;
        let budget = SearchMemoryBudget::new(256 << 20);
        let CatalogCollectStart::Ready(mut collector) = RootCatalogCollector::new(
            &source,
            Arc::clone(&source),
            &factory,
            Some(&runner),
            Device::Cpu,
            &directory.join("root"),
            RootNoiseMode::Rank,
            7,
            "case",
            16,
            4,
            0.0,
            &budget,
        )?
        else {
            anyhow::bail!("near-tie collector admission stopped");
        };
        anyhow::ensure!(
            matches!(
                collector.advance(&factory, &mut Counters::default(), &budget)?,
                CatalogCollectProgress::Ready
            ),
            "near-tie collector stopped"
        );
        let mut catalog = collector.finish();
        let mut ids = Vec::new();
        let mut source_actions = Vec::new();
        let native = source.learning_placement_domain(&factory.domain)?;
        for kind in [CommandKind::Place, CommandKind::PlaceTerminal] {
            let mut offset = 0;
            loop {
                let page = native.command_page(kind, offset, 64)?;
                for command in page.commands {
                    let action = MacroAction::Command(command.command.command);
                    ids.push(physical_action_identity(&action)?);
                    source_actions.push(action);
                }
                offset = page.next_offset;
                if page.exhausted {
                    break;
                }
            }
        }
        anyhow::ensure!(
            catalog.witnesses.witnesses.is_empty(),
            "near-tie fixture needs placement-only root"
        );
        anyhow::ensure!(
            ids.len() == dense_by_id.len() && ids.len() == 64,
            "near-tie action IDs differ"
        );
        let source_edges = ids
            .iter()
            .map(|id| dense_by_id.remove(id).unwrap())
            .collect::<Vec<_>>();
        let exact_child_max = source_edges
            .iter()
            .map(|edge| {
                factory
                    .apply_action(&edge.action, &source)
                    .map(|child| super::super::state_value(&child, factory.domain.placement))
            })
            .collect::<anyhow::Result<Vec<_>>>()?
            .into_iter()
            .fold(
                super::super::state_value(&source, factory.domain.placement),
                f64::max,
            );
        assert_eq!(catalog.initial_root_value, initial_root_value);
        assert_eq!(catalog.initial_root_value, exact_child_max);
        let mut rows = vec![[0.5; crate::hierarchical_search::ACTION_FEATURES]; ids.len()];
        for (index, row) in rows.iter_mut().enumerate().skip(2) {
            row[10] += (index as f32 - 32.0) * 1e-6;
        }
        let entities = vec![0; ids.len()];
        let anchors = vec![0.0; ids.len()];
        let input = source.macro_learning_input(&factory.domain)?;
        let batch = OuterBatch::from_rows(&[&input], &[&rows], &[&entities], Device::Cpu)?;
        let dense_output = tch::no_grad(|| outer.forward(&batch))?;
        let original = Vec::<f32>::try_from(dense_output.logp.exp().to_device(Device::Cpu))?
            .into_iter()
            .map(f64::from)
            .collect::<Vec<_>>();
        let raw_path = directory.join("injected-raw.tmp");
        let RootScoreStart::Ready(mut pass) = RootScorePass::new(
            Some(&runner),
            Some(&input),
            Device::Cpu,
            factory.domain.placement,
            source.problem().connections.len(),
            0.0,
            0.0,
            &raw_path,
            &budget,
        )?
        else {
            anyhow::bail!("injected near-tie score pass stopped");
        };
        for (row_page, entity_page) in rows.chunks(7).zip(entities.chunks(7)) {
            let start = pass.count();
            anyhow::ensure!(
                pass.append_page(
                    row_page,
                    entity_page,
                    &anchors[start..start + row_page.len()],
                    &budget,
                )?
                .is_none(),
                "injected near-tie page stopped"
            );
        }
        let (reader, _) = pass.finish()?;
        let mut reader = Some(reader);
        let mut normalizer =
            RootCatalogNormalizer::new(&mut reader, &directory.join("injected"), &budget)?;
        anyhow::ensure!(
            normalizer.advance(7, &budget)?.is_none(),
            "injected normalization stopped"
        );
        let files = normalizer.finish(&budget)?;
        catalog.files = files;
        let bytes = fs::read(&catalog.files.ledger_path)?;
        let paged = bytes
            .chunks_exact(16)
            .map(|row| f64::from_le_bytes(row[..8].try_into().unwrap()))
            .collect::<Vec<_>>();
        anyhow::ensure!(
            paged.len() == ids.len(),
            "injected near-tie ledger length differs"
        );
        let max_error = original
            .iter()
            .zip(&paged)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f64, f64::max);
        let mut reversals = 0usize;
        for a in 0..ids.len() {
            for b in a + 1..ids.len() {
                if original[a].total_cmp(&original[b]) != paged[a].total_cmp(&paged[b]) {
                    reversals += 1;
                }
            }
        }
        anyhow::ensure!(
            max_error <= 1e-6 && reversals > 0,
            "near-tie fixture did not change prior order"
        );
        let anchor_priors = bytes
            .chunks_exact(16)
            .map(|row| f64::from_le_bytes(row[8..16].try_into().unwrap()))
            .collect::<Vec<_>>();
        let mut page_order = (0..ids.len()).collect::<Vec<_>>();
        page_order.sort_by(|left, right| paged[*right].total_cmp(&paged[*left]));
        catalog.files.scan_sorted(|rank, row| {
            assert_eq!(row.ordinal as usize, page_order[rank as usize]);
        })?;
        fn edges_with_priors(source: &[Edge], priors: &[f64], anchor_priors: &[f64]) -> Vec<Edge> {
            let mut edges = source
                .iter()
                .zip(priors)
                .zip(anchor_priors)
                .map(|((edge, prior), anchor_prior)| Edge {
                    action: edge.action.clone(),
                    identity: edge.identity.clone(),
                    features: edge.features,
                    entity_index: edge.entity_index,
                    anchor_score: edge.anchor_score,
                    prior: *prior,
                    anchor_prior: *anchor_prior,
                    visits: 0,
                    value_sum: 0.0,
                    child: None,
                    blocked_value: None,
                })
                .collect::<Vec<_>>();
            edges.sort_by(|left, right| right.prior.total_cmp(&left.prior));
            edges
        }
        let original_edges = edges_with_priors(&source_edges, &original, &anchor_priors);
        let page_edges = edges_with_priors(&source_edges, &paged, &anchor_priors);
        for id in &ids {
            let original_edge = original_edges
                .iter()
                .find(|edge| &edge.identity == id)
                .unwrap();
            let page_edge = page_edges.iter().find(|edge| &edge.identity == id).unwrap();
            assert_eq!(
                serde_json::to_value(&original_edge.action)?,
                serde_json::to_value(&page_edge.action)?
            );
            assert_eq!(original_edge.features, page_edge.features);
        }
        drop(nodes);
        let make_node = |edges| crate::hierarchical_search::Node {
            session: Arc::clone(&source),
            memory: TreeReservation::new(&dense_budget, 0).unwrap(),
            visits: 0,
            value: Some(initial_root_value),
            actions: Some(edges),
            placement_pages: None,
            lower: None,
            lower_loaded_pages: 0,
            pending_actions: Vec::new(),
            pending_updates: Vec::new(),
            pending_retry_live: None,
            no_macro_diagnostic: None,
        };
        let mut search_config: Config = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-curriculum/search.json"
        ))?;
        search_config.candidate = candidate.clone();
        search_config.bounds = factory.domain.placement;
        search_config.placement_page = 7;
        search_config.cpu_threads = 1;
        search_config.horizon = 1;
        search_config.simulations_per_move = 128;
        search_config.root_selection = RootSelection::GumbelSequentialHalving {
            candidates: 16,
            anchor_candidates: 4,
            anchor_policy_mass: 0.0,
            seed: 0,
            value_scale: 0.1,
            max_visit_init: 50.0,
            gumbel_assignment: GumbelAssignment::Rank,
        };
        let controls = GumbelControls {
            candidates: 16,
            anchor_candidates: 4,
            anchor_policy_mass: 0.0,
            seed: 0,
            value_scale: 0.1,
            max_visit_init: 50.0,
            gumbel_assignment: GumbelAssignment::Rank,
        };
        let mut original_nodes = vec![make_node(edges_with_priors(
            &source_edges,
            &original,
            &anchor_priors,
        ))];
        let original_decision = crate::hierarchical_search::gumbel_sequential_halving(
            &mut original_nodes,
            0,
            &factory,
            &search_config,
            &mut Counters::default(),
            &dense_budget,
            controls,
            "near-tie",
            &catalog.state_identity,
        )?;
        let original_selected = original_nodes[0].actions.as_ref().unwrap()[original_decision
            .selected
            .expect("original reference selected")]
        .identity
        .clone();
        drop(original_nodes);
        let mut page_nodes = vec![make_node(edges_with_priors(
            &source_edges,
            &paged,
            &anchor_priors,
        ))];
        let page_decision = crate::hierarchical_search::gumbel_sequential_halving(
            &mut page_nodes,
            0,
            &factory,
            &search_config,
            &mut Counters::default(),
            &dense_budget,
            controls,
            "near-tie",
            &catalog.state_identity,
        )?;
        eprintln!(
            "injected near-tie actions={} max_prior_error={max_error:.3e} reversals={reversals} changed_seed={}",
            ids.len(),
            controls.seed
        );
        catalog.shortlist = catalog.files.shortlist(
            controls.candidates,
            controls.anchor_candidates,
            controls.anchor_policy_mass,
            controls.seed,
            "near-tie",
            &catalog.state_identity,
            &budget,
        )?;
        let resolved = catalog
            .shortlist
            .iter()
            .copied()
            .map(|choice| {
                let ordinal = choice.ordinal as usize;
                super::super::ResolvedPlacementChoice {
                    choice,
                    identity: ids[ordinal].clone(),
                    action: source_actions[ordinal].clone(),
                }
            })
            .collect::<Vec<_>>();
        catalog.resolved_placements = Some(super::super::ResolvedPlacementChoices {
            _memory: TreeReservation::new(&budget, 4096 + resolved.len() * 512).unwrap(),
            choices: resolved,
        });
        let sparse = catalog.sparse_edges(&budget)?.unwrap();
        let mut sparse = Some(sparse);
        let super::super::search::SparseSearchStart::Ready(mut search) =
            super::super::search::SparseGumbelSearch::new(&catalog, &mut sparse, &budget)
        else {
            anyhow::bail!("near-tie sparse root admission stopped");
        };
        loop {
            match search.step(
                &mut catalog,
                &factory,
                &search_config,
                controls,
                &mut Counters::default(),
                &budget,
            )? {
                super::super::search::SparseSearchStep::Running => {}
                super::super::search::SparseSearchStep::Complete => break,
                super::super::search::SparseSearchStep::MemoryStop(stop) => {
                    anyhow::bail!("near-tie sparse root stopped: {stop:?}")
                }
            }
        }
        let sparse_result = search.finish(&catalog);
        let page_selected = page_nodes[0].actions.as_ref().unwrap()
            [page_decision.selected.expect("page reference selected")]
        .identity
        .clone();
        assert_eq!(
            sparse_result.selected_id.as_deref(),
            Some(page_selected.as_slice())
        );
        assert_ne!(original_selected, page_selected);
        assert_eq!(sparse_result.simulations, page_decision.simulations);
        assert_eq!(sparse_result.shortlist, page_decision.shortlist);
        for visit in &sparse_result.visits {
            let id = &ids[visit.ordinal as usize];
            let edge = page_nodes[0]
                .actions
                .as_ref()
                .unwrap()
                .iter()
                .find(|edge| &edge.identity == id)
                .unwrap();
            assert_eq!(visit.visits, edge.visits);
            assert!((visit.value_sum - edge.value_sum).abs() < 1e-12);
        }
        let scalars = catalog.files.target_scalars(
            initial_root_value,
            &sparse_result.visits,
            controls.value_scale,
            controls.max_visit_init,
            controls.anchor_policy_mass,
            &budget,
        )?;
        let dense_edges = page_nodes[0].actions.as_ref().unwrap();
        let dense_q = crate::hierarchical_search::completed_q_transform(
            dense_edges,
            initial_root_value,
            controls.value_scale,
            controls.max_visit_init,
            page_decision.eligible_ids.as_ref(),
        );
        let dense_targets = page_decision.policy_targets.as_ref().unwrap();
        let dense_denominator = dense_targets.iter().sum::<f64>();
        let mut maximum_q_error = 0.0f64;
        let mut maximum_target_error = 0.0f64;
        catalog.files.scan_sorted(|_, row| {
            let id = &ids[row.ordinal as usize];
            let dense_index = dense_edges
                .iter()
                .position(|edge| &edge.identity == id)
                .unwrap();
            let q = scalars
                .visited_q
                .binary_search_by_key(&row.ordinal, |entry| entry.0)
                .map_or(scalars.q0, |index| scalars.visited_q[index].1);
            maximum_q_error = maximum_q_error.max((q - dense_q[dense_index]).abs());
            let weight = row.prior * (1.0 - controls.anchor_policy_mass)
                + row.anchor_prior * controls.anchor_policy_mass;
            let actual = ((weight.max(f64::MIN_POSITIVE).ln() + q - scalars.log_z).exp()
                / scalars.second_denominator) as f32;
            let expected = (dense_targets[dense_index] / dense_denominator) as f32;
            maximum_target_error =
                maximum_target_error.max((f64::from(actual) - f64::from(expected)).abs());
        })?;
        assert!(
            maximum_q_error <= 1e-6,
            "injected Q error {maximum_q_error}"
        );
        assert!(
            maximum_target_error <= 1e-6,
            "injected target error {maximum_target_error}"
        );
        eprintln!(
            "injected near-tie selected changed; maximum_q_error={maximum_q_error:.3e} maximum_target_error={maximum_target_error:.3e}"
        );
        assert_eq!(page_decision.simulations, 128);
        assert_ne!(
            original_selected,
            page_nodes[0].actions.as_ref().unwrap()[page_decision.selected.unwrap()].identity
        );
        drop(page_nodes);
        drop(original_edges);
        drop(page_edges);
        drop(sparse_result);
        drop(catalog);
        drop(normalizer);
        anyhow::ensure!(
            dense_budget.live_bytes() == 0 && budget.live_bytes() == 0,
            "near-tie fixture leaked memory admission"
        );
        anyhow::ensure!(
            fs::read_dir(&directory)?.next().is_none(),
            "near-tie temp files remain"
        );
        fs::remove_dir(directory)?;
        Ok(())
    }

    #[test]
    #[ignore = "set AIC_ROOT_CATALOG_PANEL and AIC_ROOT_CATALOG_PREFIX for a full local physical root"]
    fn complete_physical_root_catalog_probe() -> anyhow::Result<()> {
        use std::{io::Read, time::Instant};

        use sha2::{Digest, Sha256};

        use crate::{
            hierarchical_search::{Config, GumbelAssignment, GumbelControls, RootSelection},
            outer_model::Model as OuterModel,
            schema::{ConstructiveModelConfig, OuterModelConfig},
        };
        use aic_pilot_native::constructive::{Limits, learning::Config as DomainConfig};

        fn sha_json(value: &impl serde::Serialize) -> anyhow::Result<String> {
            Ok(Sha256::digest(serde_json::to_vec(value)?)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect())
        }

        let _guard = crate::TORCH_TEST.lock().unwrap();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let panel_path = std::env::var("AIC_ROOT_CATALOG_PANEL")?;
        let panel: serde_json::Value = serde_json::from_slice(&fs::read(&panel_path)?)?;
        let case = &panel["cases"][0];
        let case_id = case["id"].as_str().unwrap_or("physical-root");
        let loaded = aic_layout_gen::io::load_catalogs(&root.join("data/game/normalized"))?;
        let catalog_sha256 = loaded.catalogs.identity.clone();
        let limits: Limits = serde_json::from_value(panel["limits"].clone())?;
        let mut source = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            Arc::new(loaded.catalogs),
            limits,
        )?;
        let prefix_len = std::env::var("AIC_ROOT_CATALOG_PREFIX")?.parse::<usize>()?;
        for command in case["prefix"].as_array().unwrap().iter().take(prefix_len) {
            let step = source.step(serde_json::from_value(command.clone())?);
            anyhow::ensure!(
                step.accepted && step.diagnostic.is_none(),
                "prefix rejected: {step:?}"
            );
        }
        let source = Arc::new(source);
        let panel_candidate: CandidateConfig = serde_json::from_value(panel["candidate"].clone())?;
        let mut search_config: Config = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-curriculum/search.json"
        ))?;
        let mut candidate = search_config.candidate.clone();
        candidate.bounds = panel_candidate.bounds;
        candidate.max_crossings_per_route = panel_candidate.max_crossings_per_route;
        search_config.candidate = candidate.clone();
        search_config.bounds = candidate.bounds;
        search_config.placement_page = 256;
        let domain = DomainConfig {
            placement: candidate.bounds,
            extension_lengths: search_config.extension_lengths.clone(),
        };
        let route_config: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../docs/benchmarks/hierarchical-route-guidance/config.json"
        ))?;
        let guidance_config = serde_json::from_value(route_config["model"].clone())?;
        let (guidance_order, guidance_state) = GuidanceModel::initial_state(&guidance_config, 17)?;
        let guidance = GuidanceModel::load(
            &guidance_config,
            &guidance_order,
            &guidance_state,
            Device::Cpu,
        )?;
        let outer_config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: ConstructiveModelConfig {
                schema: ConstructiveModelConfig::SCHEMA.into(),
                width: 32,
                message_rounds: 1,
            },
            width: 32,
        };
        let (outer_order, outer_state) = OuterModel::initial_state(&outer_config, 51)?;
        let outer = OuterModel::load(&outer_config, &outer_order, &outer_state, Device::Cpu)?;
        let runner = OuterRunner::Compiled(crate::outer_model::CompiledModel::new(&outer));
        let factory = Factory::new(
            &guidance,
            "initial-guidance-seed-17",
            Device::Cpu,
            domain,
            &candidate,
            search_config.repair_max_removed,
            search_config.placement_page,
            search_config.cpu_threads,
            search_config.anchor_prior_mass,
            search_config.compiled_inference,
            Some(&outer),
        )?;
        let RootSelection::GumbelSequentialHalving {
            candidates,
            anchor_candidates,
            anchor_policy_mass,
            seed,
            value_scale,
            max_visit_init,
            gumbel_assignment,
        } = search_config.root_selection
        else {
            anyhow::bail!("physical probe requires Gumbel root selection");
        };
        let controls = GumbelControls {
            candidates,
            anchor_candidates,
            anchor_policy_mass,
            seed,
            value_scale,
            max_visit_init,
            gumbel_assignment,
        };
        let mode = match gumbel_assignment {
            GumbelAssignment::Rank => RootNoiseMode::Rank,
            GumbelAssignment::ActionIdentity => RootNoiseMode::ActionIdentity,
        };
        let directory = root.join(format!(
            "artifacts/plan03-full-root-{}-{prefix_len}",
            std::process::id()
        ));
        fs::create_dir(&directory)?;
        let budget = SearchMemoryBudget::new(search_config.max_live_search_bytes);
        let start = Instant::now();
        let CatalogCollectStart::Ready(mut collector) = RootCatalogCollector::new(
            &source,
            Arc::clone(&source),
            &factory,
            Some(&runner),
            Device::Cpu,
            &directory.join("root"),
            mode,
            seed,
            case_id,
            candidates,
            anchor_candidates,
            anchor_policy_mass,
            &budget,
        )?
        else {
            anyhow::bail!("full root score-pass admission stopped");
        };
        let mut counters = Counters::default();
        match collector.advance(&factory, &mut counters, &budget)? {
            CatalogCollectProgress::Ready => {}
            CatalogCollectProgress::Stopped(stop) => {
                anyhow::bail!("full root collection stopped: {stop:?}")
            }
            CatalogCollectProgress::NoActions(reason) => {
                anyhow::bail!("full root has no actions: {reason:?}")
            }
        }
        let collect_seconds = start.elapsed().as_secs_f64();
        let mut catalog = collector.finish();
        let count = catalog.files.count;
        let placement_count = catalog.placement_count;
        let initial_lower = catalog.witnesses.initial_count;
        let sorted_bytes = fs::metadata(&catalog.files.sorted_path)?.len();
        let mut sparse = Some(
            catalog
                .sparse_edges(&budget)?
                .map_err(|stop| anyhow::anyhow!("sparse edge admission: {stop:?}"))?,
        );
        let super::super::search::SparseSearchStart::Ready(mut search) =
            super::super::search::SparseGumbelSearch::new(&catalog, &mut sparse, &budget)
        else {
            anyhow::bail!("sparse root admission stopped");
        };
        loop {
            match search.step(
                &mut catalog,
                &factory,
                &search_config,
                controls,
                &mut counters,
                &budget,
            )? {
                super::super::search::SparseSearchStep::Running => {}
                super::super::search::SparseSearchStep::Complete => break,
                super::super::search::SparseSearchStep::MemoryStop(stop) => {
                    anyhow::bail!("physical sparse root stopped: {stop:?}")
                }
            }
        }
        let simulation = search.finish(&catalog);
        let search_seconds = start.elapsed().as_secs_f64() - collect_seconds;
        let binding = super::super::publication::CatalogBinding {
            source_sha256: catalog.state_identity.clone(),
            source_revision: source.revision(),
            limits_sha256: sha_json(&panel["limits"])?,
            effective_config_sha256: sha_json(&serde_json::json!({
                "search": search_config,
                "outer_model": outer_config,
                "guidance_model": route_config["model"],
            }))?,
            catalog_sha256,
            outer_checkpoint_sha256: sha_json(&serde_json::json!({
                "initial_seed": 51,
                "outer_model": outer_config,
            }))?,
        };
        let report_path = directory.join("report.json");
        let mut catalog = Some(catalog);
        let mut simulation = Some(simulation);
        let mut stages = vec![match super::super::publication::StagedRootCatalog::new(
            &mut catalog,
            &mut simulation,
            &binding,
            &report_path,
            case_id,
            value_scale,
            max_visit_init,
            anchor_policy_mass,
            &budget,
        )? {
            Ok(stage) => stage,
            Err(stop) => anyhow::bail!("stage publication stopped: {stop:?}"),
        }];
        let report = serde_json::json!({
            "case_id": case_id,
            "placement_count": placement_count,
            "initial_lower_count": initial_lower,
            "simulations": search_config.simulations_per_move,
        });
        super::super::publication::publish_with_catalogs(&report_path, &report, &mut stages)?
            .map_err(|stop| anyhow::anyhow!("report publication stopped: {stop:?}"))?;
        let mut replay =
            super::super::publication::open_replay(&report_path, &stages[0].descriptor, &binding)?;
        let mut first = [0u8; 16];
        replay.read_exact(&mut first)?;
        let sidecar_bytes = stages[0].descriptor.sidecar_bytes;
        let total_seconds = start.elapsed().as_secs_f64();
        eprintln!(
            "plan03 full root case={case_id} actions={count} placement={placement_count} initial_lower={initial_lower} simulations={} collect_s={collect_seconds:.3} search_s={search_seconds:.3} total_s={total_seconds:.3} sorted_bytes={sorted_bytes} sidecar_bytes={sidecar_bytes} peak_tree_bytes={} pending_lower_tasks={} loaded_lower_pages={}",
            search_config.simulations_per_move,
            budget.peak_bytes(),
            stages[0].descriptor.pending_lower_tasks,
            stages[0].descriptor.loaded_lower_pages,
        );
        drop(replay);
        let sidecar_path = directory.join(&stages[0].descriptor.sidecar_name);
        drop(stages);
        anyhow::ensure!(budget.live_bytes() == 0, "full root leaked tree admission");
        fs::remove_file(&sidecar_path)?;
        fs::remove_file(&report_path)?;
        anyhow::ensure!(
            fs::read_dir(&directory)?.next().is_none(),
            "full root left temporary files"
        );
        fs::remove_dir(directory)?;
        Ok(())
    }
}
