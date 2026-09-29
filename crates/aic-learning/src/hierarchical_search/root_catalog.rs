//! Source-order prior storage and stable external ordering for exhaustive roots.

pub(in crate::hierarchical_search) mod case;
mod collector;
mod disk_admission;
pub(crate) mod publication;
pub(crate) mod replay;
mod search;

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use aic_pilot_native::constructive::learning::{
    CommandKind, CommandPage, Config as DomainConfig, Domain as LearningDomain,
};
use serde::{Deserialize, Serialize};

use super::{
    ACTION_FEATURES, AllowanceStop, Factory, MacroAction, RouteUpdate, ScoredActionRows,
    ScoredChildren, SearchMemoryBudget, Session, TreeReservation, action_bytes,
    apply_scoring_children, deterministic_action_gumbel, finite_root_q_scale, gumbel_from_bits,
    mark_route_provenance_values, physical_action_identity, placement_host_stop,
    placement_page_peak_bytes,
    placement_scoring::{PlacementActionScorer, PreparedPlacement, ScoredPlacement},
    rank_gumbel_state,
    root_scoring::{
        RootNoiseMode, RootScoreDiskStop, RootScorePageStop, RootScorePass, SourceOrderPriorReader,
    },
    splitmix64, state_value,
};

const RUN_ROWS: usize = 1_048_576;
const MERGE_FAN_IN: usize = 32;
const LEDGER_ROW_BYTES: usize = 16;
const SORT_ROW_BYTES: usize = 24;
const NOISE_SORT_ROW_BYTES: usize = SORT_ROW_BYTES + std::mem::size_of::<u64>();

#[derive(Deserialize, Serialize)]
pub(super) struct NonplacementWitness {
    pub(super) initial_ordinal: Option<u64>,
    pub(super) identity: Vec<u8>,
    pub(super) action: MacroAction,
    #[serde(with = "feature_array_serde")]
    pub(super) features: [f32; ACTION_FEATURES],
    pub(super) entity_index: i64,
    pub(super) anchor_score: f64,
    #[serde(skip)]
    provenance_memory: Option<TreeReservation>,
}

mod feature_array_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error};

    use crate::hierarchical_search::ACTION_FEATURES;

    pub(super) fn serialize<S>(
        features: &[f32; ACTION_FEATURES],
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        features.as_slice().serialize(serializer)
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<[f32; ACTION_FEATURES], D::Error>
    where
        D: Deserializer<'de>,
    {
        let features = Vec::<f32>::deserialize(deserializer)?;
        features.try_into().map_err(|values: Vec<f32>| {
            D::Error::custom(format!(
                "action feature row has {} values; expected {ACTION_FEATURES}",
                values.len()
            ))
        })
    }
}

pub(super) struct NonplacementWitnesses {
    initial_base: u64,
    initial_count: usize,
    pub(super) witnesses: Vec<NonplacementWitness>,
    route_by_key: HashMap<(String, usize), usize>,
    physical_ids: HashSet<Vec<u8>>,
    memory: TreeReservation,
    committed_bytes: usize,
}

impl NonplacementWitnesses {
    pub(super) fn new(
        initial_base: u64,
        budget: &SearchMemoryBudget,
    ) -> Result<Self, AllowanceStop> {
        Ok(Self {
            initial_base,
            initial_count: 0,
            witnesses: Vec::new(),
            route_by_key: HashMap::new(),
            physical_ids: HashSet::new(),
            memory: TreeReservation::new(budget, 0)?,
            committed_bytes: 0,
        })
    }

    pub(super) fn reserve_witnesses(
        &mut self,
        actions: &[MacroAction],
        identities: &[Vec<u8>],
        budget: &SearchMemoryBudget,
    ) -> Result<(), AllowanceStop> {
        self.reserve_witnesses_transferring(actions, identities, 0, budget)
            .map(|_| ())
    }

    fn reserve_witnesses_transferring(
        &mut self,
        actions: &[MacroAction],
        identities: &[Vec<u8>],
        transferred_bytes: usize,
        budget: &SearchMemoryBudget,
    ) -> Result<usize, AllowanceStop> {
        assert_eq!(
            actions.len(),
            identities.len(),
            "one physical ID per lower witness"
        );
        let route_count = actions
            .iter()
            .filter(|action| matches!(action, MacroAction::Route(_)))
            .count();
        let added = actions
            .iter()
            .zip(identities)
            .fold(0usize, |bytes, (action, identity)| {
                let route_key = match action {
                    MacroAction::Route(candidate) => candidate.connection.len(),
                    _ => 0,
                };
                bytes
                    .saturating_add(std::mem::size_of::<NonplacementWitness>())
                    .saturating_add(action_bytes(action))
                    .saturating_add(identity.capacity())
                    .saturating_add(identity.capacity())
                    .saturating_add(route_key)
                    .saturating_add(256)
            });
        let full_bytes = self.committed_bytes.saturating_add(added);
        self.memory.resize(
            self.committed_bytes
                .saturating_add(added.saturating_sub(transferred_bytes)),
        )?;
        if self.witnesses.try_reserve(actions.len()).is_err()
            || self.route_by_key.try_reserve(route_count).is_err()
            || self.physical_ids.try_reserve(identities.len()).is_err()
        {
            return Err(placement_host_stop(budget, added));
        }
        Ok(full_bytes)
    }

    fn finish_transfer(&mut self, full_bytes: usize) {
        self.memory
            .resize(full_bytes)
            .expect("released identity reservation covers the transferred bytes");
        self.committed_bytes = full_bytes;
    }

    pub(super) fn append_initial(
        &mut self,
        actions: Vec<MacroAction>,
        identities: Vec<Vec<u8>>,
        features: Vec<[f32; ACTION_FEATURES]>,
        entity_indices: Vec<i64>,
        anchor_scores: Vec<f64>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.witnesses.len() == self.initial_count,
            "aic-learning.root-catalog-lower: cannot append initial witnesses after widening"
        );
        self.append(
            actions,
            identities,
            features,
            entity_indices,
            anchor_scores,
            true,
        )
    }

    pub(super) fn append_late(
        &mut self,
        actions: Vec<MacroAction>,
        identities: Vec<Vec<u8>>,
        features: Vec<[f32; ACTION_FEATURES]>,
        entity_indices: Vec<i64>,
        anchor_scores: Vec<f64>,
    ) -> anyhow::Result<()> {
        self.append(
            actions,
            identities,
            features,
            entity_indices,
            anchor_scores,
            false,
        )
    }

    fn append(
        &mut self,
        actions: Vec<MacroAction>,
        identities: Vec<Vec<u8>>,
        features: Vec<[f32; ACTION_FEATURES]>,
        entity_indices: Vec<i64>,
        anchor_scores: Vec<f64>,
        initial: bool,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            actions.len() == identities.len()
                && actions.len() == features.len()
                && actions.len() == entity_indices.len()
                && actions.len() == anchor_scores.len(),
            "aic-learning.root-catalog-lower: incomplete witness rows"
        );
        let mut page_ids = HashSet::new();
        page_ids.try_reserve(identities.len())?;
        for identity in &identities {
            anyhow::ensure!(
                !self.physical_ids.contains(identity) && page_ids.insert(identity.as_slice()),
                "aic-learning.hierarchical-root-identity: lower action duplicates a physical ID"
            );
        }
        let mut page_routes = HashSet::new();
        page_routes.try_reserve(actions.len())?;
        for action in &actions {
            if let MacroAction::Route(candidate) = action {
                let key = (candidate.connection.clone(), candidate.id);
                anyhow::ensure!(
                    !self.route_by_key.contains_key(&key) && page_routes.insert(key),
                    "aic-learning.root-catalog-lower: duplicate route witness key"
                );
            }
        }
        let start = self.witnesses.len();
        let added = actions.len();
        for (index, ((((action, identity), features), entity_index), anchor_score)) in actions
            .into_iter()
            .zip(identities)
            .zip(features)
            .zip(entity_indices)
            .zip(anchor_scores)
            .enumerate()
        {
            if let MacroAction::Route(candidate) = &action {
                self.route_by_key
                    .insert((candidate.connection.clone(), candidate.id), start + index);
            }
            self.physical_ids.insert(identity.clone());
            self.witnesses.push(NonplacementWitness {
                initial_ordinal: initial
                    .then_some(self.initial_base + (self.initial_count + index) as u64),
                identity,
                action,
                features,
                entity_index,
                anchor_score,
                provenance_memory: None,
            });
        }
        if initial {
            self.initial_count += added;
        }
        self.committed_bytes = self.memory.bytes();
        Ok(())
    }

    pub(super) fn initial(&self, ordinal: u64) -> Option<&NonplacementWitness> {
        let local = ordinal.checked_sub(self.initial_base)? as usize;
        (local < self.initial_count).then(|| &self.witnesses[local])
    }

    pub(super) fn apply_route_update(&mut self, update: RouteUpdate) -> anyhow::Result<()> {
        let index = *self
            .route_by_key
            .get(&(update.connection.clone(), update.id))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "aic-learning.root-catalog-route-update: unpublished route {}:{}",
                    update.connection,
                    update.id
                )
            })?;
        let witness = &mut self.witnesses[index];
        let MacroAction::Route(candidate) = &mut witness.action else {
            unreachable!("route key indexes route witness")
        };
        witness.features[13..16].fill(0.0);
        mark_route_provenance_values(
            &mut witness.features,
            &candidate.provenance,
            &update.additional_provenance,
        );
        candidate.additional_provenance = update.additional_provenance;
        witness.provenance_memory = Some(update._memory);
        Ok(())
    }

    pub(super) fn has_route_key(&self, update: &RouteUpdate) -> bool {
        self.route_by_key
            .contains_key(&(update.connection.clone(), update.id))
    }

    pub(super) fn into_frozen(self) -> (Vec<NonplacementWitness>, TreeReservation) {
        (self.witnesses, self.memory)
    }
}

pub(super) struct PendingInitialLower {
    actions: Vec<MacroAction>,
    rows: Option<ScoredActionRows>,
    identities: Option<Vec<Vec<u8>>>,
    noise_bits: Option<Vec<u64>>,
    identity_memory: Option<TreeReservation>,
    pub(super) exact_value: Option<f64>,
    committed: bool,
}

impl PendingInitialLower {
    pub(super) fn new(actions: Vec<MacroAction>) -> Self {
        Self {
            actions,
            rows: None,
            identities: None,
            noise_bits: None,
            identity_memory: None,
            exact_value: None,
            committed: false,
        }
    }

    pub(super) fn action_count(&self) -> usize {
        self.actions.len()
    }

    pub(super) fn advance(
        &mut self,
        source: &Session,
        factory: &Factory<'_>,
        pass: &mut RootScorePass<'_>,
        witnesses: &mut NonplacementWitnesses,
        seed: u64,
        state_identity: &str,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<Option<PlacementCatalogStop>> {
        assert!(!self.committed, "initial lower page already committed");
        if self.actions.is_empty() {
            self.committed = true;
            return Ok(None);
        }
        if let Some(stop) =
            self.prepare(source, factory, pass.mode(), seed, state_identity, budget)?
        {
            return Ok(Some(PlacementCatalogStop::Memory(stop)));
        }
        let full_witness_bytes = match self.reserve_table(witnesses, budget) {
            Ok(bytes) => bytes,
            Err(stop) => return Ok(Some(PlacementCatalogStop::Memory(stop))),
        };
        let rows = self.rows.as_ref().expect("lower rows scored");
        match pass.append_page_with_noise(
            &rows.features,
            &rows.entity_indices,
            &rows.anchor_scores,
            self.noise_bits.as_deref(),
            budget,
        )? {
            None => {}
            Some(RootScorePageStop::Memory(stop)) => {
                return Ok(Some(PlacementCatalogStop::Memory(stop)));
            }
            Some(RootScorePageStop::Disk(stop)) => {
                return Ok(Some(PlacementCatalogStop::Disk(stop)));
            }
        }
        self.commit(witnesses, full_witness_bytes, true)?;
        Ok(None)
    }

    pub(super) fn advance_late(
        &mut self,
        source: &Session,
        factory: &Factory<'_>,
        witnesses: &mut NonplacementWitnesses,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<Option<AllowanceStop>> {
        assert!(!self.committed, "late lower page already committed");
        if self.actions.is_empty() {
            self.committed = true;
            return Ok(None);
        }
        if let Some(stop) = self.prepare(source, factory, RootNoiseMode::Rank, 0, "", budget)? {
            return Ok(Some(stop));
        }
        let full_witness_bytes = match self.reserve_table(witnesses, budget) {
            Ok(bytes) => bytes,
            Err(stop) => return Ok(Some(stop)),
        };
        self.commit(witnesses, full_witness_bytes, false)?;
        Ok(None)
    }

    fn prepare(
        &mut self,
        source: &Session,
        factory: &Factory<'_>,
        mode: RootNoiseMode,
        seed: u64,
        state_identity: &str,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<Option<AllowanceStop>> {
        if self.rows.is_none() {
            self.rows = Some(
                match apply_scoring_children(source, &self.actions, factory, budget)? {
                    ScoredChildren::Ready(rows) => rows,
                    ScoredChildren::MemoryStop(stop) => return Ok(Some(stop)),
                },
            );
        }
        if self.identities.is_none() {
            let slot_bytes = self
                .actions
                .len()
                .saturating_mul(std::mem::size_of::<Vec<u8>>() + 16);
            let mut memory = match TreeReservation::new(budget, slot_bytes) {
                Ok(memory) => memory,
                Err(stop) => return Ok(Some(stop)),
            };
            let mut identities = Vec::new();
            let mut noise_bits = (mode == RootNoiseMode::ActionIdentity).then(Vec::new);
            if identities.try_reserve_exact(self.actions.len()).is_err()
                || noise_bits
                    .as_mut()
                    .is_some_and(|noise| noise.try_reserve_exact(self.actions.len()).is_err())
            {
                return Ok(Some(placement_host_stop(budget, slot_bytes)));
            }
            for action in &self.actions {
                let identity = physical_action_identity(action)?;
                let next_bytes = memory.bytes().saturating_add(identity.capacity());
                if let Err(stop) = memory.resize(next_bytes) {
                    return Ok(Some(stop));
                }
                if let Some(noise) = &mut noise_bits {
                    noise.push(
                        deterministic_action_gumbel(seed, state_identity, &identity)?.to_bits(),
                    );
                }
                identities.push(identity);
            }
            self.identity_memory = Some(memory);
            self.identities = Some(identities);
            self.noise_bits = noise_bits;
        }
        Ok(None)
    }

    fn reserve_table(
        &mut self,
        witnesses: &mut NonplacementWitnesses,
        budget: &SearchMemoryBudget,
    ) -> Result<usize, AllowanceStop> {
        let identities = self.identities.as_ref().expect("physical IDs prepared");
        let rows = self.rows.as_ref().expect("lower rows scored");
        witnesses.reserve_witnesses_transferring(
            &self.actions,
            identities,
            self.identity_memory
                .as_ref()
                .map_or(0, TreeReservation::bytes)
                .saturating_add(rows._memory.bytes()),
            budget,
        )
    }

    fn commit(
        &mut self,
        witnesses: &mut NonplacementWitnesses,
        full_witness_bytes: usize,
        initial: bool,
    ) -> anyhow::Result<()> {
        let ScoredActionRows {
            features,
            entity_indices,
            anchor_scores,
            exact_value,
            _memory: row_memory,
        } = self.rows.take().expect("lower rows scored");
        self.exact_value = Some(exact_value);
        let actions = std::mem::take(&mut self.actions);
        let identities = self.identities.take().expect("physical IDs prepared");
        if initial {
            witnesses.append_initial(
                actions,
                identities,
                features,
                entity_indices,
                anchor_scores,
            )?;
        } else {
            witnesses.append_late(actions, identities, features, entity_indices, anchor_scores)?;
        }
        self.identity_memory = None;
        drop(row_memory);
        witnesses.finish_transfer(full_witness_bytes);
        self.committed = true;
        Ok(())
    }
}

pub(super) enum PlacementCatalogStart<'s> {
    Ready(PlacementCatalogScan<'s>),
    MemoryStop(AllowanceStop),
}

pub(super) enum PlacementCatalogStop {
    Memory(AllowanceStop),
    Disk(RootScoreDiskStop),
}

struct PendingPlacementPage {
    page: CommandPage,
    next_index: usize,
    rows: Vec<[f32; ACTION_FEATURES]>,
    entities: Vec<i64>,
    anchors: Vec<f64>,
    noise_bits: Option<Vec<u64>>,
    _memory: TreeReservation,
}

pub(super) struct PlacementCatalogScan<'s> {
    source: &'s Session,
    domain: Arc<LearningDomain>,
    _domain_memory: TreeReservation,
    scorer: PlacementActionScorer<'s>,
    page_size: usize,
    mode: RootNoiseMode,
    seed: u64,
    state_identity: String,
    kind_index: usize,
    offset: usize,
    pending: Option<PendingPlacementPage>,
    pub(super) count: u64,
    pub(super) exact_value: f64,
}

impl<'s> PlacementCatalogScan<'s> {
    pub(super) fn legal_count(&self) -> anyhow::Result<usize> {
        Ok(self.domain.placement_command_count()?)
    }

    pub(super) fn into_scorer(self) -> PlacementActionScorer<'s> {
        assert_eq!(self.kind_index, 2, "placement scan is unfinished");
        self.scorer
    }

    pub(super) fn new(
        source: &'s Session,
        config: &DomainConfig,
        page_size: usize,
        mode: RootNoiseMode,
        seed: u64,
        state_identity: String,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<PlacementCatalogStart<'s>> {
        let peak = source.learning_placement_domain_peak_bytes(config)?;
        let mut domain_memory = match TreeReservation::new(budget, peak) {
            Ok(memory) => memory,
            Err(stop) => return Ok(PlacementCatalogStart::MemoryStop(stop)),
        };
        let domain = source.learning_placement_domain(config)?;
        if let Err(stop) = domain_memory.resize(domain.retained_bytes()) {
            return Ok(PlacementCatalogStart::MemoryStop(stop));
        }
        let scorer = match PlacementActionScorer::prepare(source, config.placement, budget)? {
            PreparedPlacement::Ready(scorer) => scorer,
            PreparedPlacement::MemoryStop(stop) => {
                return Ok(PlacementCatalogStart::MemoryStop(stop));
            }
        };
        Ok(PlacementCatalogStart::Ready(Self {
            source,
            domain,
            _domain_memory: domain_memory,
            scorer,
            page_size,
            mode,
            seed,
            state_identity,
            kind_index: 0,
            offset: 0,
            pending: None,
            count: 0,
            exact_value: state_value(source, config.placement),
        }))
    }

    pub(super) fn advance(
        &mut self,
        pass: &mut RootScorePass<'_>,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<Option<PlacementCatalogStop>> {
        assert_eq!(
            pass.mode(),
            self.mode,
            "root score and native scan noise modes differ"
        );
        let kinds = [CommandKind::Place, CommandKind::PlaceTerminal];
        while self.kind_index < kinds.len() {
            if self.pending.is_none() {
                let memory = match TreeReservation::new(
                    budget,
                    placement_page_peak_bytes(self.source, self.page_size),
                ) {
                    Ok(memory) => memory,
                    Err(stop) => return Ok(Some(PlacementCatalogStop::Memory(stop))),
                };
                let page = self.domain.command_page(
                    kinds[self.kind_index],
                    self.offset,
                    self.page_size,
                )?;
                if page.commands.is_empty() {
                    self.kind_index += 1;
                    self.offset = 0;
                    continue;
                }
                let count = page.commands.len();
                let host_stop =
                    || PlacementCatalogStop::Memory(placement_host_stop(budget, memory.bytes()));
                let mut rows = Vec::new();
                let mut entities = Vec::new();
                let mut anchors = Vec::new();
                let mut noise_bits = (self.mode == RootNoiseMode::ActionIdentity).then(Vec::new);
                if rows.try_reserve_exact(count).is_err()
                    || entities.try_reserve_exact(count).is_err()
                    || anchors.try_reserve_exact(count).is_err()
                    || noise_bits
                        .as_mut()
                        .is_some_and(|bits| bits.try_reserve_exact(count).is_err())
                {
                    return Ok(Some(host_stop()));
                }
                self.pending = Some(PendingPlacementPage {
                    page,
                    next_index: 0,
                    rows,
                    entities,
                    anchors,
                    noise_bits,
                    _memory: memory,
                });
            }
            let pending = self.pending.as_mut().expect("pending page exists");
            let page_scorer = self.scorer.begin_page(self.source)?;
            while pending.next_index < pending.page.commands.len() {
                let command = &pending.page.commands[pending.next_index].command.command;
                let score = match page_scorer.score(command, budget)? {
                    ScoredPlacement::Ready(score) => score,
                    ScoredPlacement::MemoryStop(stop) => {
                        return Ok(Some(PlacementCatalogStop::Memory(stop)));
                    }
                };
                if let Some(bits) = &mut pending.noise_bits {
                    let identity =
                        physical_action_identity(&MacroAction::Command(command.clone()))?;
                    bits.push(
                        deterministic_action_gumbel(self.seed, &self.state_identity, &identity)?
                            .to_bits(),
                    );
                }
                self.exact_value = self.exact_value.max(score.child_value);
                pending.rows.push(score.features);
                pending.entities.push(score.entity_index);
                pending.anchors.push(score.anchor_score);
                pending.next_index += 1;
            }
            match pass.append_page_with_noise(
                &pending.rows,
                &pending.entities,
                &pending.anchors,
                pending.noise_bits.as_deref(),
                budget,
            )? {
                None => {}
                Some(RootScorePageStop::Memory(stop)) => {
                    return Ok(Some(PlacementCatalogStop::Memory(stop)));
                }
                Some(RootScorePageStop::Disk(stop)) => {
                    return Ok(Some(PlacementCatalogStop::Disk(stop)));
                }
            }
            self.count += pending.rows.len() as u64;
            self.offset = pending.page.next_offset;
            if pending.page.exhausted {
                self.kind_index += 1;
                self.offset = 0;
            }
            self.pending = None;
        }
        Ok(None)
    }

    pub(super) fn resolve_placement_choices(
        &self,
        choices: &[CatalogChoice],
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<Result<ResolvedPlacementChoices, AllowanceStop>> {
        assert_eq!(
            self.kind_index, 2,
            "placement catalog must finish before resolving choices"
        );
        let longest_id = self
            .source
            .problem()
            .blocks
            .iter()
            .map(|block| block.id.len())
            .chain(
                self.source
                    .problem()
                    .terminals
                    .iter()
                    .map(|terminal| terminal.id.len()),
            )
            .max()
            .unwrap_or(0);
        let bytes = placement_page_peak_bytes(self.source, self.page_size).saturating_add(
            choices.len().saturating_mul(
                std::mem::size_of::<ResolvedPlacementChoice>()
                    + std::mem::size_of::<CatalogChoice>()
                    + longest_id
                    + 256,
            ),
        );
        let memory = match TreeReservation::new(budget, bytes) {
            Ok(memory) => memory,
            Err(stop) => return Ok(Err(stop)),
        };
        let mut wanted = Vec::new();
        if wanted.try_reserve_exact(choices.len()).is_err() {
            return Ok(Err(placement_host_stop(budget, bytes)));
        }
        wanted.extend(
            choices
                .iter()
                .copied()
                .filter(|choice| choice.ordinal < self.count),
        );
        wanted.sort_unstable_by_key(|choice| choice.ordinal);
        let mut resolved = Vec::new();
        if resolved.try_reserve_exact(wanted.len()).is_err() {
            return Ok(Err(placement_host_stop(budget, bytes)));
        }
        let mut next_wanted = 0usize;
        let mut ordinal = 0u64;
        for kind in [CommandKind::Place, CommandKind::PlaceTerminal] {
            let mut offset = 0usize;
            loop {
                if next_wanted == wanted.len() {
                    break;
                }
                let page = self.domain.command_page(kind, offset, self.page_size)?;
                for factored in page.commands {
                    if wanted[next_wanted].ordinal == ordinal {
                        let action = MacroAction::Command(factored.command.command);
                        resolved.push(ResolvedPlacementChoice {
                            choice: wanted[next_wanted],
                            identity: physical_action_identity(&action)?,
                            action,
                        });
                        next_wanted += 1;
                    }
                    ordinal += 1;
                    if next_wanted == wanted.len() {
                        break;
                    }
                }
                offset = page.next_offset;
                if page.exhausted {
                    break;
                }
            }
        }
        anyhow::ensure!(
            next_wanted == wanted.len(),
            "aic-learning.root-catalog-resolve: missing native placement ordinal"
        );
        resolved.sort_unstable_by_key(|entry| entry.choice.rank);
        Ok(Ok(ResolvedPlacementChoices {
            choices: resolved,
            _memory: memory,
        }))
    }
}

fn sort_row_bytes(mode: RootNoiseMode) -> usize {
    match mode {
        RootNoiseMode::Rank => SORT_ROW_BYTES,
        RootNoiseMode::ActionIdentity => NOISE_SORT_ROW_BYTES,
    }
}

struct PartialOutput(PathBuf);

impl Drop for PartialOutput {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.0);
        }
    }
}

#[derive(Default)]
struct RunPaths(Vec<PathBuf>);

impl Drop for RunPaths {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}

#[derive(Debug)]
pub(super) enum CatalogStop {
    Memory(AllowanceStop),
    Io {
        path: PathBuf,
        operation: &'static str,
        source: std::io::Error,
    },
}

impl std::fmt::Display for CatalogStop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory(stop) => write!(formatter, "root catalog memory stop: {stop:?}"),
            Self::Io {
                path,
                operation,
                source,
            } => {
                write!(
                    formatter,
                    "root catalog {operation} {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for CatalogStop {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Memory(_) => None,
            Self::Io { source, .. } => Some(source),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct PriorRow {
    pub(super) prior: f64,
    pub(super) anchor_prior: f64,
    pub(super) ordinal: u64,
    pub(super) noise_bits: u64,
}

impl PriorRow {
    fn write(self, file: &mut impl Write, mode: RootNoiseMode) -> std::io::Result<()> {
        let mut bytes = [0u8; NOISE_SORT_ROW_BYTES];
        bytes[0..8].copy_from_slice(&self.prior.to_le_bytes());
        bytes[8..16].copy_from_slice(&self.anchor_prior.to_le_bytes());
        bytes[16..24].copy_from_slice(&self.ordinal.to_le_bytes());
        if mode == RootNoiseMode::ActionIdentity {
            bytes[24..32].copy_from_slice(&self.noise_bits.to_le_bytes());
        }
        file.write_all(&bytes[..sort_row_bytes(mode)])
    }

    fn read(file: &mut impl Read, mode: RootNoiseMode) -> std::io::Result<Self> {
        let mut bytes = [0u8; NOISE_SORT_ROW_BYTES];
        file.read_exact(&mut bytes[..sort_row_bytes(mode)])?;
        Ok(Self {
            prior: f64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            anchor_prior: f64::from_le_bytes(bytes[8..16].try_into().unwrap()),
            ordinal: u64::from_le_bytes(bytes[16..24].try_into().unwrap()),
            noise_bits: if mode == RootNoiseMode::ActionIdentity {
                u64::from_le_bytes(bytes[24..32].try_into().unwrap())
            } else {
                0
            },
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CatalogChoice {
    pub(super) rank: u64,
    pub(super) ordinal: u64,
    pub(super) prior: f64,
    pub(super) anchor_prior: f64,
    pub(super) gumbel_bits: u64,
}

pub(super) struct ResolvedPlacementChoice {
    pub(super) choice: CatalogChoice,
    pub(super) identity: Vec<u8>,
    pub(super) action: MacroAction,
}

pub(super) struct ResolvedPlacementChoices {
    pub(super) choices: Vec<ResolvedPlacementChoice>,
    _memory: TreeReservation,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub(super) struct CatalogVisit {
    pub(super) ordinal: u64,
    pub(super) visits: u32,
    pub(super) value_sum: f64,
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct FrozenTargetScalars {
    pub(super) q0: f64,
    pub(super) log_z: f64,
    pub(super) unvisited_weight: f64,
    pub(super) second_denominator: f64,
    pub(super) visited_q: Vec<(u64, f64)>,
}

fn transformed_q(value: f64, minimum: f64, denominator: f64, scale: f64) -> f64 {
    if denominator <= 1e-8 {
        0.0
    } else {
        (value - minimum) / denominator * scale
    }
}

fn policy_weight(row: PriorRow, anchor_policy_mass: f64) -> f64 {
    (row.prior * (1.0 - anchor_policy_mass) + row.anchor_prior * anchor_policy_mass)
        .max(f64::MIN_POSITIVE)
}

struct RankedChoice {
    score: f64,
    choice: CatalogChoice,
}

impl PartialEq for RankedChoice {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for RankedChoice {}

impl PartialOrd for RankedChoice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedChoice {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .score
            .total_cmp(&self.score)
            .then_with(|| self.choice.rank.cmp(&other.choice.rank))
    }
}

fn keep_best(heap: &mut BinaryHeap<RankedChoice>, capacity: usize, candidate: RankedChoice) {
    if capacity == 0 {
        return;
    }
    if heap.len() < capacity {
        heap.push(candidate);
    } else if heap
        .peek()
        .is_some_and(|worst| candidate.cmp(worst) == Ordering::Less)
    {
        heap.pop();
        heap.push(candidate);
    }
}

fn best_first(left: &PriorRow, right: &PriorRow) -> Ordering {
    right
        .prior
        .total_cmp(&left.prior)
        .then_with(|| left.ordinal.cmp(&right.ordinal))
}

struct HeapRow {
    row: PriorRow,
    run: usize,
}

impl PartialEq for HeapRow {
    fn eq(&self, other: &Self) -> bool {
        self.row.prior.to_bits() == other.row.prior.to_bits()
            && self.row.ordinal == other.row.ordinal
            && self.run == other.run
    }
}

impl Eq for HeapRow {}

impl PartialOrd for HeapRow {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapRow {
    fn cmp(&self, other: &Self) -> Ordering {
        best_first(&other.row, &self.row).then_with(|| other.run.cmp(&self.run))
    }
}

pub(super) struct PriorCatalogBuilder {
    mode: RootNoiseMode,
    prefix: PathBuf,
    ledger_path: PathBuf,
    ledger: Option<BufWriter<File>>,
    runs: Vec<PathBuf>,
    retired_runs: Vec<PathBuf>,
    pending: Vec<PriorRow>,
    run_rows: usize,
    count: u64,
    sealed: bool,
    merge_pass: usize,
    #[cfg(test)]
    fail_ledger_after_rows: Option<usize>,
    #[cfg(test)]
    stop_after_merge_pass: Option<usize>,
    #[cfg(test)]
    fail_retired_after_removals: Option<usize>,
    pending_memory: TreeReservation,
}

pub(super) struct RootCatalogNormalizer {
    reader: Option<SourceOrderPriorReader>,
    builder: PriorCatalogBuilder,
    pending: Option<super::root_scoring::PriorPage>,
    offset: usize,
    count: usize,
}

impl RootCatalogNormalizer {
    pub(super) fn new(
        reader: &mut Option<SourceOrderPriorReader>,
        prefix: &Path,
        budget: &SearchMemoryBudget,
    ) -> Result<Self, CatalogStop> {
        let source = reader
            .as_ref()
            .expect("normalizer source is owned by caller");
        let count = source.count();
        let builder = PriorCatalogBuilder::new(prefix, budget, source.mode())?;
        Ok(Self {
            reader: reader.take(),
            builder,
            pending: None,
            offset: 0,
            count,
        })
    }

    pub(super) fn advance(
        &mut self,
        page_size: usize,
        budget: &SearchMemoryBudget,
    ) -> anyhow::Result<Option<CatalogStop>> {
        while self.offset < self.count {
            if self.pending.is_none() {
                let reader = self.reader.as_mut().expect("normalization is unfinished");
                let page = match reader.read_page(page_size.min(self.builder.run_rows), budget)? {
                    Ok(page) => page,
                    Err(stop) => return Ok(Some(CatalogStop::Memory(stop))),
                };
                self.pending = Some(page);
            }
            let page = self.pending.as_ref().expect("prior page is retained");
            match self
                .builder
                .append_page(&page.priors, page.noise_bits.as_deref())
            {
                Ok(()) => {}
                Err(stop) => {
                    if matches!(
                        &stop,
                        CatalogStop::Io { source, .. }
                            if matches!(source.kind(), std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded)
                    ) {
                        return Ok(Some(stop));
                    }
                    return Err(stop.into());
                }
            }
            self.offset = page.next_offset;
            self.pending = None;
        }
        if let Some(reader) = self.reader.as_mut() {
            if let Err(source) = reader.close() {
                let stop = CatalogStop::Io {
                    path: reader.path().to_owned(),
                    operation: "remove",
                    source,
                };
                if matches!(
                    &stop,
                    CatalogStop::Io { source, .. }
                        if matches!(source.kind(), std::io::ErrorKind::StorageFull | std::io::ErrorKind::QuotaExceeded)
                ) {
                    return Ok(Some(stop));
                }
                return Err(stop.into());
            }
            self.reader = None;
        }
        Ok(None)
    }

    pub(super) fn finish(
        &mut self,
        budget: &SearchMemoryBudget,
    ) -> Result<PriorCatalogFiles, CatalogStop> {
        assert!(
            self.reader.is_none(),
            "raw score file must close before merge"
        );
        self.builder.finish(budget)
    }
}

pub(super) struct PriorCatalogFiles {
    pub(super) ledger_path: PathBuf,
    pub(super) sorted_path: PathBuf,
    pub(super) count: u64,
    pub(super) mode: RootNoiseMode,
}

impl PriorCatalogFiles {
    fn scan_sorted(&self, mut visit: impl FnMut(u64, PriorRow)) -> Result<(), CatalogStop> {
        let file = File::open(&self.sorted_path).map_err(|source| CatalogStop::Io {
            path: self.sorted_path.clone(),
            operation: "open",
            source,
        })?;
        let expected = self.count.saturating_mul(sort_row_bytes(self.mode) as u64);
        let actual = file
            .metadata()
            .map_err(|source| CatalogStop::Io {
                path: self.sorted_path.clone(),
                operation: "metadata",
                source,
            })?
            .len();
        if actual != expected {
            return Err(CatalogStop::Io {
                path: self.sorted_path.clone(),
                operation: "read",
                source: std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "sorted root length mismatch",
                ),
            });
        }
        let mut reader = BufReader::with_capacity(64 * 1024, file);
        for rank in 0..self.count {
            let row = PriorRow::read(&mut reader, self.mode).map_err(|source| CatalogStop::Io {
                path: self.sorted_path.clone(),
                operation: "read",
                source,
            })?;
            visit(rank, row);
        }
        Ok(())
    }

    pub(super) fn shortlist(
        &self,
        requested: usize,
        requested_anchors: usize,
        anchor_policy_mass: f64,
        seed: u64,
        case_id: &str,
        state_identity: &str,
        budget: &SearchMemoryBudget,
    ) -> Result<Vec<CatalogChoice>, CatalogStop> {
        let count = requested.min(self.count as usize);
        let anchor_count = requested_anchors.min(count);
        let reserve_bytes = count.saturating_mul(256).saturating_add(128 * 1024);
        let _memory = TreeReservation::new(budget, reserve_bytes).map_err(CatalogStop::Memory)?;
        let mut anchors = BinaryHeap::new();
        anchors
            .try_reserve(anchor_count)
            .map_err(|_| CatalogStop::Memory(placement_host_stop(budget, reserve_bytes)))?;
        self.scan_sorted(|rank, row| {
            keep_best(
                &mut anchors,
                anchor_count,
                RankedChoice {
                    score: row.anchor_prior,
                    choice: CatalogChoice {
                        rank,
                        ordinal: row.ordinal,
                        prior: row.prior,
                        anchor_prior: row.anchor_prior,
                        gumbel_bits: 0,
                    },
                },
            );
        })?;
        let mut anchor_ordinals = HashSet::new();
        anchor_ordinals
            .try_reserve(anchor_count)
            .map_err(|_| CatalogStop::Memory(placement_host_stop(budget, reserve_bytes)))?;
        for anchor in &anchors {
            anchor_ordinals.insert(anchor.choice.ordinal);
        }
        let mut ranked = BinaryHeap::new();
        ranked
            .try_reserve(count - anchor_count)
            .map_err(|_| CatalogStop::Memory(placement_host_stop(budget, reserve_bytes)))?;
        let mut selected = Vec::new();
        selected
            .try_reserve_exact(count)
            .map_err(|_| CatalogStop::Memory(placement_host_stop(budget, reserve_bytes)))?;
        let mut noise_state = rank_gumbel_state(seed, case_id, state_identity);
        self.scan_sorted(|rank, row| {
            let gumbel = match self.mode {
                RootNoiseMode::Rank => gumbel_from_bits(splitmix64(&mut noise_state)),
                RootNoiseMode::ActionIdentity => f64::from_bits(row.noise_bits),
            };
            if anchor_ordinals.contains(&row.ordinal) {
                selected.push(CatalogChoice {
                    rank,
                    ordinal: row.ordinal,
                    prior: row.prior,
                    anchor_prior: row.anchor_prior,
                    gumbel_bits: gumbel.to_bits(),
                });
                return;
            }
            keep_best(
                &mut ranked,
                count - anchor_count,
                RankedChoice {
                    score: gumbel + policy_weight(row, anchor_policy_mass).ln(),
                    choice: CatalogChoice {
                        rank,
                        ordinal: row.ordinal,
                        prior: row.prior,
                        anchor_prior: row.anchor_prior,
                        gumbel_bits: gumbel.to_bits(),
                    },
                },
            );
        })?;
        selected.extend(ranked.into_iter().map(|candidate| candidate.choice));
        selected.sort_unstable_by_key(|choice| choice.rank);
        Ok(selected)
    }

    pub(super) fn target_scalars(
        &self,
        root_value: f64,
        visits: &[CatalogVisit],
        value_scale: f64,
        max_visit_init: f64,
        anchor_policy_mass: f64,
        budget: &SearchMemoryBudget,
    ) -> Result<FrozenTargetScalars, CatalogStop> {
        let reserve_bytes = visits.len().saturating_mul(128).saturating_add(128 * 1024);
        let _memory = TreeReservation::new(budget, reserve_bytes).map_err(CatalogStop::Memory)?;
        let total_visits = visits
            .iter()
            .map(|entry| u64::from(entry.visits))
            .sum::<u64>();
        let total_value = visits.iter().map(|entry| entry.value_sum).sum::<f64>();
        let mixed = (root_value + total_value) / (1.0 + total_visits as f64);
        let mut minimum = root_value;
        let mut maximum = root_value;
        if visits.len() < self.count as usize {
            minimum = minimum.min(mixed);
            maximum = maximum.max(mixed);
        }
        for visit in visits {
            let value = visit.value_sum / f64::from(visit.visits);
            minimum = minimum.min(value);
            maximum = maximum.max(value);
        }
        let max_visits = visits.iter().map(|entry| entry.visits).max().unwrap_or(0);
        let scale = finite_root_q_scale(max_visit_init, max_visits, value_scale);
        let q0 = transformed_q(mixed, minimum, maximum - minimum, scale);
        let mut visited_q = Vec::new();
        visited_q
            .try_reserve_exact(visits.len())
            .map_err(|_| CatalogStop::Memory(placement_host_stop(budget, reserve_bytes)))?;
        for visit in visits {
            visited_q.push((
                visit.ordinal,
                transformed_q(
                    visit.value_sum / f64::from(visit.visits),
                    minimum,
                    maximum - minimum,
                    scale,
                ),
            ));
        }
        visited_q.sort_unstable_by_key(|entry| entry.0);
        let mut unvisited_weight = 0.0f64;
        let mut visited_weights = Vec::new();
        visited_weights
            .try_reserve_exact(visited_q.len())
            .map_err(|_| CatalogStop::Memory(placement_host_stop(budget, reserve_bytes)))?;
        visited_weights.resize(visited_q.len(), 0.0f64);
        self.scan_sorted(|_, row| {
            let weight = policy_weight(row, anchor_policy_mass);
            match visited_q.binary_search_by_key(&row.ordinal, |entry| entry.0) {
                Ok(index) => visited_weights[index] = weight,
                Err(_) => unvisited_weight += weight,
            }
        })?;
        let unvisited_logit = if unvisited_weight > 0.0 {
            unvisited_weight.ln() + q0
        } else {
            f64::NEG_INFINITY
        };
        let maximum_logit = visited_q
            .iter()
            .zip(&visited_weights)
            .fold(unvisited_logit, |maximum, ((_, q), weight)| {
                maximum.max(weight.ln() + q)
            });
        let mut shifted_sum = if unvisited_weight > 0.0 {
            (unvisited_logit - maximum_logit).exp()
        } else {
            0.0
        };
        for ((_, q), weight) in visited_q.iter().zip(&visited_weights) {
            shifted_sum += (weight.ln() + q - maximum_logit).exp();
        }
        let log_z = maximum_logit + shifted_sum.ln();
        let mut second_denominator = 0.0f64;
        self.scan_sorted(|_, row| {
            let q = visited_q
                .binary_search_by_key(&row.ordinal, |entry| entry.0)
                .map_or(q0, |index| visited_q[index].1);
            second_denominator += (policy_weight(row, anchor_policy_mass).ln() + q - log_z).exp();
        })?;
        Ok(FrozenTargetScalars {
            q0,
            log_z,
            unvisited_weight,
            second_denominator,
            visited_q,
        })
    }
}

impl Drop for PriorCatalogFiles {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.ledger_path);
        let _ = fs::remove_file(&self.sorted_path);
    }
}

impl PriorCatalogBuilder {
    pub(super) fn new(
        prefix: &Path,
        budget: &SearchMemoryBudget,
        mode: RootNoiseMode,
    ) -> Result<Self, CatalogStop> {
        let available = budget.limit_bytes().saturating_sub(budget.live_bytes());
        let row_memory = std::mem::size_of::<PriorRow>();
        let run_rows = RUN_ROWS.min(available.saturating_sub(8192) / (4 * row_memory));
        if run_rows == 0 {
            return Err(CatalogStop::Memory(placement_host_stop(
                budget,
                row_memory + 4096,
            )));
        }
        let reserved = run_rows.saturating_mul(row_memory).saturating_add(8192);
        let mut pending_memory =
            TreeReservation::new(budget, reserved).map_err(CatalogStop::Memory)?;
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(run_rows)
            .map_err(|_| CatalogStop::Memory(placement_host_stop(budget, reserved)))?;
        pending_memory
            .resize(
                pending
                    .capacity()
                    .saturating_mul(row_memory)
                    .saturating_add(8192),
            )
            .map_err(CatalogStop::Memory)?;
        let ledger_path = prefix.with_extension("prior-ledger.tmp");
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&ledger_path)
            .map_err(|source| CatalogStop::Io {
                path: ledger_path.clone(),
                operation: "create",
                source,
            })?;
        Ok(Self {
            mode,
            prefix: prefix.to_owned(),
            ledger_path,
            ledger: Some(BufWriter::new(file)),
            runs: Vec::new(),
            retired_runs: Vec::new(),
            pending,
            run_rows,
            count: 0,
            sealed: false,
            merge_pass: 0,
            #[cfg(test)]
            fail_ledger_after_rows: None,
            #[cfg(test)]
            stop_after_merge_pass: None,
            #[cfg(test)]
            fail_retired_after_removals: None,
            pending_memory,
        })
    }

    pub(super) fn append_page(
        &mut self,
        priors: &[(f64, f64)],
        noise_bits: Option<&[u64]>,
    ) -> Result<(), CatalogStop> {
        assert!(!self.sealed, "cannot append to a sealed root catalog");
        assert!(match self.mode {
            RootNoiseMode::Rank => noise_bits.is_none(),
            RootNoiseMode::ActionIdentity =>
                noise_bits.is_some_and(|bits| bits.len() == priors.len()),
        });
        assert!(
            priors.len() <= self.run_rows,
            "page exceeds admitted sort run"
        );
        if self.pending.len() + priors.len() > self.run_rows {
            self.flush_run()?;
        }
        let committed_len = self.count * LEDGER_ROW_BYTES as u64;
        let ledger = self
            .ledger
            .as_mut()
            .expect("unfinished catalog owns ledger");
        ledger.flush().map_err(|source| CatalogStop::Io {
            path: self.ledger_path.clone(),
            operation: "flush",
            source,
        })?;
        for (_index, &(prior, anchor_prior)) in priors.iter().enumerate() {
            let mut bytes = [0u8; LEDGER_ROW_BYTES];
            bytes[..8].copy_from_slice(&prior.to_le_bytes());
            bytes[8..].copy_from_slice(&anchor_prior.to_le_bytes());
            #[cfg(test)]
            let mut write = ledger.write_all(&bytes);
            #[cfg(not(test))]
            let write = ledger.write_all(&bytes);
            #[cfg(test)]
            if write.is_ok() && self.fail_ledger_after_rows == Some(_index + 1) {
                self.fail_ledger_after_rows = None;
                write = Err(std::io::Error::from(std::io::ErrorKind::StorageFull));
            }
            if let Err(source) = write {
                let old = self.ledger.take().expect("ledger is open");
                let (mut file, _) = old.into_parts();
                if let Err(rollback) = file
                    .set_len(committed_len)
                    .and_then(|()| file.seek(SeekFrom::Start(committed_len)).map(|_| ()))
                {
                    return Err(CatalogStop::Io {
                        path: self.ledger_path.clone(),
                        operation: "rollback",
                        source: rollback,
                    });
                }
                self.ledger = Some(BufWriter::new(file));
                return Err(CatalogStop::Io {
                    path: self.ledger_path.clone(),
                    operation: "write",
                    source,
                });
            }
        }
        for (index, &(prior, anchor_prior)) in priors.iter().enumerate() {
            let ordinal = self.count;
            self.pending.push(PriorRow {
                prior,
                anchor_prior,
                ordinal,
                noise_bits: noise_bits.map_or(0, |bits| bits[index]),
            });
            self.count += 1;
        }
        Ok(())
    }

    fn flush_run(&mut self) -> Result<(), CatalogStop> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.pending.sort_unstable_by(best_first);
        let path = self
            .prefix
            .with_extension(format!("prior-run-{}.tmp", self.runs.len()));
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|source| CatalogStop::Io {
                path: path.clone(),
                operation: "create",
                source,
            })?;
        let mut partial = PartialOutput(path.clone());
        let mut writer = BufWriter::new(file);
        for row in &self.pending {
            row.write(&mut writer, self.mode)
                .map_err(|source| CatalogStop::Io {
                    path: path.clone(),
                    operation: "write",
                    source,
                })?;
        }
        writer.flush().map_err(|source| CatalogStop::Io {
            path: path.clone(),
            operation: "flush",
            source,
        })?;
        self.runs.push(path);
        partial.0.clear();
        self.pending.clear();
        Ok(())
    }

    pub(super) fn finish(
        &mut self,
        budget: &SearchMemoryBudget,
    ) -> Result<PriorCatalogFiles, CatalogStop> {
        assert!(
            !self.ledger_path.as_os_str().is_empty(),
            "root catalog already finished"
        );
        if !self.sealed {
            self.flush_run()?;
            if let Some(ledger) = self.ledger.as_mut() {
                ledger.flush().map_err(|source| CatalogStop::Io {
                    path: self.ledger_path.clone(),
                    operation: "flush",
                    source,
                })?;
            }
            self.ledger.take();
            self.pending = Vec::new();
            self.pending_memory.resize(0).expect("releasing run memory");
            self.sealed = true;
        }
        let mut pass = self.merge_pass;
        while self.runs.len() > 1 {
            self.cleanup_retired_runs()?;
            #[cfg(test)]
            if self.stop_after_merge_pass == Some(pass) {
                self.stop_after_merge_pass = None;
                return Err(CatalogStop::Memory(placement_host_stop(budget, 1)));
            }
            let mut next_runs = RunPaths::default();
            for (batch, group) in self.runs.chunks(MERGE_FAN_IN).enumerate() {
                let path = self
                    .prefix
                    .with_extension(format!("prior-merge-{pass}-{batch}.tmp"));
                merge_runs(group, &path, budget, self.mode)?;
                next_runs.0.push(path);
            }
            self.retired_runs.extend(std::mem::replace(
                &mut self.runs,
                std::mem::take(&mut next_runs.0),
            ));
            pass += 1;
            self.merge_pass = pass;
            self.cleanup_retired_runs()?;
        }
        self.cleanup_retired_runs()?;
        let sorted_path = self.prefix.with_extension("prior-sorted.tmp");
        if let Some(run) = self.runs.last() {
            fs::rename(&run, &sorted_path).map_err(|source| CatalogStop::Io {
                path: run.clone(),
                operation: "rename",
                source,
            })?;
            self.runs.clear();
        } else {
            File::create(&sorted_path).map_err(|source| CatalogStop::Io {
                path: sorted_path.clone(),
                operation: "create",
                source,
            })?;
        }
        let ledger_path = std::mem::take(&mut self.ledger_path);
        Ok(PriorCatalogFiles {
            ledger_path,
            sorted_path,
            count: self.count,
            mode: self.mode,
        })
    }

    fn cleanup_retired_runs(&mut self) -> Result<(), CatalogStop> {
        #[cfg(test)]
        let mut removed = 0;
        while let Some(path) = self.retired_runs.last() {
            #[cfg(test)]
            if self.fail_retired_after_removals == Some(removed) {
                self.fail_retired_after_removals = None;
                return Err(CatalogStop::Io {
                    path: path.clone(),
                    operation: "remove",
                    source: std::io::Error::from(std::io::ErrorKind::StorageFull),
                });
            }
            match fs::remove_file(path) {
                Ok(()) => {}
                Err(source) if source.kind() == std::io::ErrorKind::NotFound => {}
                Err(source) => {
                    return Err(CatalogStop::Io {
                        path: path.clone(),
                        operation: "remove",
                        source,
                    });
                }
            }
            self.retired_runs.pop();
            #[cfg(test)]
            {
                removed += 1;
            }
        }
        Ok(())
    }
}

impl Drop for PriorCatalogBuilder {
    fn drop(&mut self) {
        if !self.ledger_path.as_os_str().is_empty() {
            let _ = fs::remove_file(&self.ledger_path);
        }
        for path in &self.runs {
            let _ = fs::remove_file(path);
        }
        for path in &self.retired_runs {
            let _ = fs::remove_file(path);
        }
    }
}

struct RunReader {
    file: BufReader<File>,
    remaining: u64,
}

fn merge_runs(
    inputs: &[PathBuf],
    output: &Path,
    budget: &SearchMemoryBudget,
    mode: RootNoiseMode,
) -> Result<(), CatalogStop> {
    let bytes = inputs
        .len()
        .saturating_mul(64 * 1024 + 128)
        .saturating_add(64 * 1024);
    let _memory = TreeReservation::new(budget, bytes).map_err(CatalogStop::Memory)?;
    let mut readers = Vec::with_capacity(inputs.len());
    let mut heap = BinaryHeap::with_capacity(inputs.len());
    for (index, path) in inputs.iter().enumerate() {
        let file = File::open(path).map_err(|source| CatalogStop::Io {
            path: path.clone(),
            operation: "open",
            source,
        })?;
        let length = file
            .metadata()
            .map_err(|source| CatalogStop::Io {
                path: path.clone(),
                operation: "metadata",
                source,
            })?
            .len();
        if length % sort_row_bytes(mode) as u64 != 0 {
            return Err(CatalogStop::Io {
                path: path.clone(),
                operation: "read",
                source: std::io::Error::new(std::io::ErrorKind::InvalidData, "partial sort row"),
            });
        }
        let mut reader = RunReader {
            file: BufReader::with_capacity(64 * 1024, file),
            remaining: length / sort_row_bytes(mode) as u64,
        };
        if reader.remaining > 0 {
            let row = PriorRow::read(&mut reader.file, mode).map_err(|source| CatalogStop::Io {
                path: path.clone(),
                operation: "read",
                source,
            })?;
            reader.remaining -= 1;
            heap.push(HeapRow { row, run: index });
        }
        readers.push(reader);
    }
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)
        .map_err(|source| CatalogStop::Io {
            path: output.to_owned(),
            operation: "create",
            source,
        })?;
    let mut partial = PartialOutput(output.to_owned());
    let mut writer = BufWriter::with_capacity(64 * 1024, file);
    while let Some(item) = heap.pop() {
        item.row
            .write(&mut writer, mode)
            .map_err(|source| CatalogStop::Io {
                path: output.to_owned(),
                operation: "write",
                source,
            })?;
        let reader = &mut readers[item.run];
        if reader.remaining > 0 {
            let row = PriorRow::read(&mut reader.file, mode).map_err(|source| CatalogStop::Io {
                path: inputs[item.run].clone(),
                operation: "read",
                source,
            })?;
            reader.remaining -= 1;
            heap.push(HeapRow { row, run: item.run });
        }
    }
    writer.flush().map_err(|source| CatalogStop::Io {
        path: output.to_owned(),
        operation: "flush",
        source,
    })?;
    partial.0.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hierarchical_search::{
        ACTION_FEATURES, Bounds, Edge, MacroAction, completed_q_transform, considered_root_actions,
        deterministic_gumbels, improved_policy,
        root_scoring::{RootScorePass, RootScoreStart},
    };
    use aic_pilot_native::constructive::Command;
    use aic_torch::tch::Device;

    #[test]
    fn nonplacement_table_updates_a_route_outside_the_shortlist() -> anyhow::Result<()> {
        use crate::route_guidance::Model as GuidanceModel;
        use aic_pilot_native::constructive::{
            Session,
            routing_candidates::{Config, Provenance, Request},
        };
        let _guard = crate::TORCH_TEST.lock().unwrap();
        let panel: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/benchmarks/hierarchical-route-guidance/training-panel.json"
        ))?;
        let case = &panel["cases"][0];
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            crate::constructive::test_support::catalogs(),
            serde_json::from_value(panel["limits"].clone())?,
        )?;
        for command in case["prefix"].as_array().unwrap() {
            assert!(
                session
                    .step(serde_json::from_value(command.clone())?)
                    .accepted
            );
        }
        let config: Config = serde_json::from_value(panel["candidate"].clone())?;
        let request = Request {
            state_revision: session.revision(),
            connection: case["request"].as_str().unwrap().into(),
        };
        let mut candidates = session.route_candidates(&request, &config)?.candidates;
        assert!(candidates.len() >= 2);
        let mut candidate = candidates.remove(0);
        let mut later = candidates.remove(0);
        candidate.provenance = Provenance::Anchor;
        later.provenance = Provenance::Anchor;
        let action = MacroAction::Route(candidate.clone());
        let identity = physical_action_identity(&action)?;
        let route_config: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../docs/benchmarks/hierarchical-route-guidance/config.json"
        ))?;
        let model_config = serde_json::from_value(route_config["model"].clone())?;
        let (order, state) = GuidanceModel::initial_state(&model_config, 17)?;
        let model = GuidanceModel::load(&model_config, &order, &state, Device::Cpu)?;
        let factory = Factory::new(
            &model,
            "checkpoint",
            Device::Cpu,
            serde_json::from_value(route_config["learning_domain"].clone())?,
            &config,
            0,
            4,
            1,
            0.0,
            false,
            None,
        )?;
        let budget = SearchMemoryBudget::new(128 << 20);
        let directory =
            std::env::temp_dir().join(format!("aic-lower-stage-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let raw_path = directory.join("raw.tmp");
        let RootScoreStart::Ready(mut score) = RootScorePass::new_with_mode(
            None,
            None,
            Device::Cpu,
            config.bounds,
            session.problem().connections.len(),
            0.25,
            0.0,
            &raw_path,
            &budget,
            RootNoiseMode::ActionIdentity,
        )?
        else {
            anyhow::bail!("score admission stopped");
        };
        let mut table = NonplacementWitnesses::new(100, &budget).unwrap();
        let mut staged = PendingInitialLower::new(vec![action]);
        let peer = TreeReservation::new(&budget, budget.limit_bytes() - budget.live_bytes() - 1024)
            .unwrap();
        assert!(matches!(
            staged.advance(
                &session, &factory, &mut score, &mut table, 7, "state", &budget
            )?,
            Some(PlacementCatalogStop::Memory(_))
        ));
        assert!(table.witnesses.is_empty());
        drop(peer);
        assert!(
            staged
                .advance(
                    &session, &factory, &mut score, &mut table, 7, "state", &budget
                )?
                .is_none()
        );
        assert!(staged.exact_value.is_some());
        assert!(table.committed_bytes >= action_bytes(&table.witnesses[0].action));
        let (reader, _) = score.finish()?;
        let mut reader = Some(reader);
        let mut catalog =
            RootCatalogNormalizer::new(&mut reader, &directory.join("root"), &budget)?;
        assert!(catalog.advance(2, &budget)?.is_none());
        assert!(!raw_path.exists());
        let files = catalog.finish(&budget)?;
        assert_eq!(files.count, 1);
        let mut sorted = BufReader::new(File::open(&files.sorted_path)?);
        assert_eq!(
            PriorRow::read(&mut sorted, files.mode)?.noise_bits,
            deterministic_action_gumbel(7, "state", &identity)?.to_bits()
        );
        assert_eq!(table.initial(100).unwrap().identity, identity);
        assert!(table.initial(99).is_none());
        assert!(table.initial(101).is_none());
        let mut same_physical = candidate.clone();
        same_physical.id += 1;
        let same_action = MacroAction::Route(same_physical);
        let same_id = physical_action_identity(&same_action)?;
        assert_eq!(same_id, identity);
        table
            .reserve_witnesses(&[same_action.clone()], &[same_id.clone()], &budget)
            .unwrap();
        assert!(
            table
                .append_late(
                    vec![same_action],
                    vec![same_id],
                    vec![[0.0; ACTION_FEATURES]],
                    vec![0],
                    vec![0.0],
                )
                .is_err()
        );
        assert_eq!(table.witnesses.len(), 1);
        table.apply_route_update(RouteUpdate {
            connection: candidate.connection.clone(),
            id: candidate.id,
            additional_provenance: vec![Provenance::Guided {
                model_version: "checkpoint".into(),
                head: "head-0".into(),
            }],
            _memory: TreeReservation::new(&budget, 0).unwrap(),
        })?;
        let witness = table.initial(100).unwrap();
        assert_eq!(witness.identity, identity);
        assert_eq!(witness.features[13..15], [1.0, 1.0]);
        let MacroAction::Route(updated) = &witness.action else {
            panic!("route witness")
        };
        assert_eq!(updated.additional_provenance.len(), 1);
        assert_eq!(physical_action_identity(&witness.action)?, identity);
        let later_action = MacroAction::Route(later.clone());
        let later_identity = physical_action_identity(&later_action)?;
        table
            .reserve_witnesses(&[later_action.clone()], &[later_identity.clone()], &budget)
            .unwrap();
        table.append_late(
            vec![later_action],
            vec![later_identity.clone()],
            vec![[0.0; ACTION_FEATURES]],
            vec![0],
            vec![0.0],
        )?;
        assert!(table.witnesses[1].initial_ordinal.is_none());
        assert!(table.initial(101).is_none());
        table.apply_route_update(RouteUpdate {
            connection: later.connection.clone(),
            id: later.id,
            additional_provenance: vec![Provenance::Guided {
                model_version: "checkpoint".into(),
                head: "head-1".into(),
            }],
            _memory: TreeReservation::new(&budget, 0).unwrap(),
        })?;
        assert_eq!(table.witnesses[1].identity, later_identity);
        assert_eq!(table.witnesses[1].features[13..15], [1.0, 1.0]);
        drop(files);
        drop(catalog);
        drop(staged);
        drop(table);
        assert_eq!(budget.live_bytes(), 0);
        assert_eq!(fs::read_dir(&directory)?.count(), 0);
        fs::remove_dir(directory)?;
        Ok(())
    }

    #[test]
    #[ignore = "set AIC_ROOT_CATALOG_PANEL for a local complete physical placement catalog"]
    fn complete_physical_placement_catalog_probe() -> anyhow::Result<()> {
        use crate::{
            hierarchical_search::{OuterRunner, state_sha256},
            outer_model::Model as OuterModel,
            schema::{ConstructiveModelConfig, OuterModelConfig},
        };
        use aic_pilot_native::constructive::{
            Limits, Session, routing_candidates::Config as CandidateConfig,
        };
        use std::{sync::Arc, time::Instant};

        let _guard = crate::TORCH_TEST.lock().unwrap();
        let path = std::env::var("AIC_ROOT_CATALOG_PANEL")?;
        let panel: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
        let case = &panel["cases"][0];
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let catalogs = aic_layout_gen::io::load_catalogs(&root.join("data/game/normalized"))?;
        let mut session = Session::new(
            serde_json::from_value(case["problem"].clone())?,
            Arc::new(catalogs.catalogs),
            serde_json::from_value::<Limits>(panel["limits"].clone())?,
        )?;
        let prefix = std::env::var("AIC_ROOT_CATALOG_PREFIX")?.parse::<usize>()?;
        for command in case["prefix"].as_array().unwrap().iter().take(prefix) {
            let step = session.step(serde_json::from_value(command.clone())?);
            anyhow::ensure!(
                step.accepted && step.diagnostic.is_none(),
                "prefix rejected: {step:?}"
            );
        }
        let candidate: CandidateConfig = serde_json::from_value(panel["candidate"].clone())?;
        let domain = DomainConfig {
            placement: candidate.bounds,
            extension_lengths: vec![1, 2, 4],
        };
        let input = session.macro_learning_input(&domain)?;
        let config = OuterModelConfig {
            schema: OuterModelConfig::SCHEMA.into(),
            encoder: ConstructiveModelConfig {
                schema: ConstructiveModelConfig::SCHEMA.into(),
                width: 32,
                message_rounds: 1,
            },
            width: 32,
        };
        let (order, initial) = OuterModel::initial_state(&config, 51)?;
        let model = OuterModel::load(&config, &order, &initial, Device::Cpu)?;
        let runner = OuterRunner::Compiled(crate::outer_model::CompiledModel::new(&model));
        let budget = SearchMemoryBudget::new(512 * 1024 * 1024);
        let directory = root.join("artifacts/plan03-physical-catalog-probe");
        fs::create_dir_all(&directory)?;
        let raw = directory.join("root.raw.tmp");
        let start = Instant::now();
        let RootScoreStart::Ready(mut score) = RootScorePass::new_with_mode(
            Some(&runner),
            Some(&input),
            Device::Cpu,
            candidate.bounds,
            session.problem().connections.len(),
            0.25,
            0.0,
            &raw,
            &budget,
            RootNoiseMode::Rank,
        )?
        else {
            anyhow::bail!("score pass admission stopped");
        };
        let state_identity = state_sha256(&session)?;
        let PlacementCatalogStart::Ready(mut scan) = PlacementCatalogScan::new(
            &session,
            &domain,
            256,
            RootNoiseMode::Rank,
            7,
            state_identity.clone(),
            &budget,
        )?
        else {
            anyhow::bail!("placement admission stopped");
        };
        anyhow::ensure!(
            scan.advance(&mut score, &budget)?.is_none(),
            "placement scan stopped"
        );
        let scan_seconds = start.elapsed().as_secs_f64();
        let count = scan.count;
        let root_value = scan.exact_value;
        let (reader, model_value) = score.finish()?;
        let mut reader = Some(reader);
        let mut catalog =
            RootCatalogNormalizer::new(&mut reader, &directory.join("root"), &budget)?;
        anyhow::ensure!(
            catalog.advance(65_536, &budget)?.is_none(),
            "normalization stopped"
        );
        let normalize_seconds = start.elapsed().as_secs_f64() - scan_seconds;
        let files = catalog.finish(&budget)?;
        let sort_seconds = start.elapsed().as_secs_f64() - scan_seconds - normalize_seconds;
        let ledger_bytes = fs::metadata(&files.ledger_path)?.len();
        let sort_bytes = fs::metadata(&files.sorted_path)?.len();
        let shortlist = files.shortlist(
            16,
            4,
            0.25,
            7,
            case["id"].as_str().unwrap_or("case"),
            &state_identity,
            &budget,
        )?;
        let resolved = scan
            .resolve_placement_choices(&shortlist, &budget)?
            .unwrap();
        let targets = files.target_scalars(root_value, &[], 0.1, 50.0, 0.25, &budget)?;
        anyhow::ensure!(
            resolved.choices.len() == shortlist.len() && targets.second_denominator.is_finite(),
            "catalog result invalid"
        );
        eprintln!(
            "plan03 placement catalog count={count} scan_s={scan_seconds:.3} normalize_s={normalize_seconds:.3} sort_s={sort_seconds:.3} total_s={:.3} ledger_bytes={ledger_bytes} sort_bytes={sort_bytes} peak_admitted_bytes={} model_value={model_value:.6} exact_value={root_value:.6}",
            start.elapsed().as_secs_f64(),
            budget.peak_bytes()
        );
        drop(resolved);
        drop(files);
        drop(catalog);
        drop(scan);
        anyhow::ensure!(
            fs::read_dir(&directory)?.next().is_none(),
            "probe left temporary files"
        );
        fs::remove_dir(directory)?;
        Ok(())
    }

    #[test]
    fn external_prior_order_preserves_source_ties_and_ledger() {
        let directory = std::env::temp_dir().join(format!("aic-prior-sort-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let prefix = directory.join("root");
        let budget = SearchMemoryBudget::new(8 << 20);
        let mut builder = PriorCatalogBuilder::new(&prefix, &budget, RootNoiseMode::Rank).unwrap();
        builder.run_rows = 3;
        builder
            .append_page(&[(0.5, 0.1), (0.9, 0.2), (0.5, 0.3)], None)
            .unwrap();
        builder
            .append_page(&[(0.9, 0.4), (0.2, 0.5)], None)
            .unwrap();
        let files = builder.finish(&budget).unwrap();
        assert_eq!(files.count, 5);
        let ledger = fs::read(&files.ledger_path).unwrap();
        assert_eq!(ledger.len(), 5 * LEDGER_ROW_BYTES);
        for (index, expected) in [(0.5, 0.1), (0.9, 0.2), (0.5, 0.3), (0.9, 0.4), (0.2, 0.5)]
            .iter()
            .enumerate()
        {
            let offset = index * LEDGER_ROW_BYTES;
            assert_eq!(
                f64::from_le_bytes(ledger[offset..offset + 8].try_into().unwrap()),
                expected.0
            );
            assert_eq!(
                f64::from_le_bytes(ledger[offset + 8..offset + 16].try_into().unwrap()),
                expected.1
            );
        }
        let mut sorted = BufReader::new(File::open(&files.sorted_path).unwrap());
        let ordinals = (0..files.count)
            .map(|_| {
                PriorRow::read(&mut sorted, RootNoiseMode::Rank)
                    .unwrap()
                    .ordinal
            })
            .collect::<Vec<_>>();
        assert_eq!(ordinals, [1, 3, 0, 2, 4]);
        drop(files);
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn external_prior_order_merges_more_than_one_fan_in() {
        let directory =
            std::env::temp_dir().join(format!("aic-prior-sort-multipass-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let budget = SearchMemoryBudget::new(8 << 20);
        let mut builder =
            PriorCatalogBuilder::new(&directory.join("root"), &budget, RootNoiseMode::Rank)
                .unwrap();
        builder.run_rows = 2;
        builder.stop_after_merge_pass = Some(1);
        builder.fail_retired_after_removals = Some(1);
        for ordinal in 0..70 {
            builder
                .append_page(&[((ordinal % 5) as f64, ordinal as f64)], None)
                .unwrap();
        }
        assert!(matches!(
            builder.finish(&budget),
            Err(CatalogStop::Io { .. })
        ));
        assert_eq!(builder.merge_pass, 1);
        assert_eq!(builder.runs.len(), 2);
        assert!(!builder.retired_runs.is_empty());
        assert!(matches!(
            builder.finish(&budget),
            Err(CatalogStop::Memory(_))
        ));
        assert!(builder.retired_runs.is_empty());
        let files = builder.finish(&budget).unwrap();
        let mut sorted = BufReader::new(File::open(&files.sorted_path).unwrap());
        let actual = (0..70)
            .map(|_| {
                PriorRow::read(&mut sorted, RootNoiseMode::Rank)
                    .unwrap()
                    .ordinal
            })
            .collect::<Vec<_>>();
        let mut expected = (0..70).collect::<Vec<_>>();
        expected.sort_by_key(|ordinal| (std::cmp::Reverse(ordinal % 5), *ordinal));
        assert_eq!(actual, expected);
        drop(files);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn memory_stopped_merge_discards_unpublished_runs() {
        let directory =
            std::env::temp_dir().join(format!("aic-prior-sort-stopped-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let budget = SearchMemoryBudget::new(1 << 20);
        let mut builder =
            PriorCatalogBuilder::new(&directory.join("root"), &budget, RootNoiseMode::Rank)
                .unwrap();
        builder.run_rows = 2;
        for ordinal in 0..70 {
            builder.append_page(&[(ordinal as f64, 0.0)], None).unwrap();
        }
        assert!(matches!(
            builder.finish(&budget),
            Err(CatalogStop::Memory(_))
        ));
        drop(builder);
        assert_eq!(budget.live_bytes(), 0);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn aggregate_memory_stop_retains_runs_for_merge_retry() {
        let directory =
            std::env::temp_dir().join(format!("aic-prior-sort-retry-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let budget = SearchMemoryBudget::new(8 << 20);
        let mut builder =
            PriorCatalogBuilder::new(&directory.join("root"), &budget, RootNoiseMode::Rank)
                .unwrap();
        builder.run_rows = 2;
        for ordinal in 0..70 {
            builder.append_page(&[(ordinal as f64, 0.0)], None).unwrap();
        }
        let peer_bytes = budget.limit_bytes() - builder.pending_memory.bytes() - 4096;
        let peer = TreeReservation::new(&budget, peer_bytes).unwrap();
        assert!(matches!(
            builder.finish(&budget),
            Err(CatalogStop::Memory(_))
        ));
        assert!(builder.sealed);
        assert!(!builder.runs.is_empty());
        drop(peer);
        let files = builder.finish(&budget).unwrap();
        assert_eq!(files.count, 70);
        drop(files);
        drop(builder);
        assert_eq!(budget.live_bytes(), 0);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn streamed_shortlist_matches_dense_anchor_quota_and_noise_modes() {
        let directory =
            std::env::temp_dir().join(format!("aic-prior-shortlist-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let priors = [
            (0.15, 0.01),
            (0.45, 0.02),
            (0.45, 0.30),
            (0.11, 0.20),
            (0.04, 0.25),
            (0.08, 0.08),
            (0.27, 0.04),
            (0.03, 0.10),
        ];
        let identity_noise = [2.0f64, -0.5, 0.25, 1.3, -1.1, 0.7, 0.1, 2.5];
        let budget = SearchMemoryBudget::new(8 << 20);
        for mode in [RootNoiseMode::Rank, RootNoiseMode::ActionIdentity] {
            let prefix = directory.join(format!("root-{mode:?}"));
            let mut builder = PriorCatalogBuilder::new(&prefix, &budget, mode).unwrap();
            builder.run_rows = priors.len();
            let noise =
                (mode == RootNoiseMode::ActionIdentity).then(|| identity_noise.map(f64::to_bits));
            builder
                .append_page(&priors, noise.as_ref().map(|value| value.as_slice()))
                .unwrap();
            let files = builder.finish(&budget).unwrap();
            let choices = files
                .shortlist(5, 2, 0.35, 7, "case", "state", &budget)
                .unwrap();
            let mut source = (0..priors.len()).collect::<Vec<_>>();
            source.sort_by(|left, right| {
                priors[*right]
                    .0
                    .total_cmp(&priors[*left].0)
                    .then_with(|| left.cmp(right))
            });
            let actions = source
                .iter()
                .map(|ordinal| Edge {
                    identity: vec![*ordinal as u8],
                    action: MacroAction::Command(Command::Connect),
                    features: [0.0; ACTION_FEATURES],
                    entity_index: 0,
                    anchor_score: 0.0,
                    prior: priors[*ordinal].0,
                    anchor_prior: priors[*ordinal].1,
                    visits: 0,
                    value_sum: 0.0,
                    child: None,
                    blocked_value: None,
                })
                .collect::<Vec<_>>();
            let noise = match mode {
                RootNoiseMode::Rank => deterministic_gumbels(7, "case", "state", source.len()),
                RootNoiseMode::ActionIdentity => source
                    .iter()
                    .map(|ordinal| identity_noise[*ordinal])
                    .collect(),
            };
            let expected = considered_root_actions(&actions, &noise, 5, 2, 0.35);
            assert_eq!(
                choices
                    .iter()
                    .map(|choice| choice.rank as usize)
                    .collect::<Vec<_>>(),
                expected
            );
            for choice in &choices {
                assert_eq!(choice.ordinal as usize, source[choice.rank as usize]);
                assert_eq!(choice.gumbel_bits, noise[choice.rank as usize].to_bits());
            }
            drop(files);
        }
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn frozen_target_scalars_reconstruct_dense_q_and_f32_targets() {
        let directory =
            std::env::temp_dir().join(format!("aic-frozen-target-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let budget = SearchMemoryBudget::new(8 << 20);
        let mut builder =
            PriorCatalogBuilder::new(&directory.join("root"), &budget, RootNoiseMode::Rank)
                .unwrap();
        let source = [(0.1, 0.5), (0.4, 0.1), (0.2, 0.1), (0.3, 0.2), (0.05, 0.1)];
        builder.run_rows = source.len();
        builder.append_page(&source, None).unwrap();
        let files = builder.finish(&budget).unwrap();
        let order = [1usize, 3, 2, 0, 4];
        let visits = [
            CatalogVisit {
                ordinal: 1,
                visits: 3,
                value_sum: 1.8,
            },
            CatalogVisit {
                ordinal: 0,
                visits: 2,
                value_sum: -0.2,
            },
        ];
        let mut actions = order
            .iter()
            .map(|ordinal| Edge {
                identity: vec![*ordinal as u8],
                action: MacroAction::Command(Command::Connect),
                features: [0.0; ACTION_FEATURES],
                entity_index: 0,
                anchor_score: 0.0,
                prior: source[*ordinal].0,
                anchor_prior: source[*ordinal].1,
                visits: visits
                    .iter()
                    .find(|entry| entry.ordinal == *ordinal as u64)
                    .map_or(0, |entry| entry.visits),
                value_sum: visits
                    .iter()
                    .find(|entry| entry.ordinal == *ordinal as u64)
                    .map_or(0.0, |entry| entry.value_sum),
                child: None,
                blocked_value: None,
            })
            .collect::<Vec<_>>();
        let root_value = 0.25;
        let dense_q = completed_q_transform(&actions, root_value, 2.0, 0.5, None);
        let dense_target = improved_policy(&actions, &dense_q, 0.3, None);
        let dense_denominator = dense_target.iter().sum::<f64>();
        let scalars = files
            .target_scalars(root_value, &visits, 2.0, 0.5, 0.3, &budget)
            .unwrap();
        assert!(scalars.unvisited_weight > 0.0);
        for (rank, ordinal) in order.iter().enumerate() {
            let q = scalars
                .visited_q
                .iter()
                .find(|entry| entry.0 == *ordinal as u64)
                .map_or(scalars.q0, |entry| entry.1);
            assert!((q - dense_q[rank]).abs() < 1e-12);
            let weight = (actions[rank].prior * 0.7 + actions[rank].anchor_prior * 0.3)
                .max(f64::MIN_POSITIVE);
            let reconstructed =
                ((weight.ln() + q - scalars.log_z).exp() / scalars.second_denominator) as f32;
            let dense = (dense_target[rank] / dense_denominator) as f32;
            assert!((reconstructed - dense).abs() < 1e-6);
        }
        let extreme = files
            .target_scalars(
                0.0,
                &[CatalogVisit {
                    ordinal: 1,
                    visits: 1,
                    value_sum: 1.0,
                }],
                1e308,
                50.0,
                0.3,
                &budget,
            )
            .unwrap();
        assert!(
            extreme.q0.is_finite()
                && extreme.log_z.is_finite()
                && extreme.second_denominator.is_finite()
                && extreme.second_denominator > 0.0
                && extreme.visited_q.iter().all(|entry| entry.1.is_finite())
        );
        actions.clear();
        drop(files);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn action_identity_noise_survives_external_reordering() {
        let directory =
            std::env::temp_dir().join(format!("aic-prior-sort-noise-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let budget = SearchMemoryBudget::new(8 << 20);
        let mut builder = PriorCatalogBuilder::new(
            &directory.join("root"),
            &budget,
            RootNoiseMode::ActionIdentity,
        )
        .unwrap();
        builder.run_rows = 3;
        let noise = [11u64, 22, 33, 44, 55];
        builder
            .append_page(&[(0.5, 0.1), (0.9, 0.2), (0.5, 0.3)], Some(&noise[..3]))
            .unwrap();
        builder
            .append_page(&[(0.9, 0.4), (0.2, 0.5)], Some(&noise[3..]))
            .unwrap();
        let files = builder.finish(&budget).unwrap();
        assert_eq!(files.mode, RootNoiseMode::ActionIdentity);
        assert_eq!(
            fs::metadata(&files.sorted_path).unwrap().len(),
            5 * NOISE_SORT_ROW_BYTES as u64
        );
        let mut sorted = BufReader::new(File::open(&files.sorted_path).unwrap());
        let actual = (0..5)
            .map(|_| PriorRow::read(&mut sorted, files.mode).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            actual.iter().map(|row| row.ordinal).collect::<Vec<_>>(),
            [1, 3, 0, 2, 4]
        );
        assert!(
            actual
                .iter()
                .all(|row| row.noise_bits == noise[row.ordinal as usize])
        );
        drop(files);
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 0);
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn raw_score_reader_closes_before_action_identity_merge() -> anyhow::Result<()> {
        let directory =
            std::env::temp_dir().join(format!("aic-prior-score-to-sort-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let budget = SearchMemoryBudget::new(8 << 20);
        let raw_path = directory.join("raw.tmp");
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 4,
            max_y: 4,
        };
        let RootScoreStart::Ready(mut score) = RootScorePass::new_with_mode(
            None,
            None,
            Device::Cpu,
            bounds,
            1,
            0.25,
            0.0,
            &raw_path,
            &budget,
            RootNoiseMode::ActionIdentity,
        )?
        else {
            anyhow::bail!("root score admission stopped");
        };
        let rows = [[0.0; ACTION_FEATURES]; 4];
        let entities = [0; 4];
        let anchors = [0.0, 1.0, 0.5, 1.0];
        let noise = [101u64, 202, 303, 404];
        assert!(
            score
                .append_page_with_noise(&rows, &entities, &anchors, Some(&noise), &budget)?
                .is_none()
        );
        let (reader, _) = score.finish()?;
        let mut reader = Some(reader);
        let start_peer =
            TreeReservation::new(&budget, budget.limit_bytes() - budget.live_bytes() - 64).unwrap();
        assert!(matches!(
            RootCatalogNormalizer::new(&mut reader, &directory.join("root"), &budget),
            Err(CatalogStop::Memory(_))
        ));
        assert!(reader.is_some() && raw_path.exists());
        drop(start_peer);
        let ledger_path = directory.join("root").with_extension("prior-ledger.tmp");
        File::create(&ledger_path)?;
        assert!(matches!(
            RootCatalogNormalizer::new(&mut reader, &directory.join("root"), &budget),
            Err(CatalogStop::Io { .. })
        ));
        assert!(reader.is_some() && raw_path.exists());
        fs::remove_file(ledger_path)?;
        let mut catalog =
            RootCatalogNormalizer::new(&mut reader, &directory.join("root"), &budget)?;
        catalog.builder.run_rows = 2;
        let peer = TreeReservation::new(&budget, budget.limit_bytes() - budget.live_bytes() - 1024)
            .unwrap();
        assert!(catalog.advance(2, &budget)?.is_some());
        assert_eq!(catalog.offset, 0);
        drop(peer);
        catalog.builder.fail_ledger_after_rows = Some(1);
        assert!(matches!(
            catalog.advance(2, &budget)?,
            Some(CatalogStop::Io { source, .. })
                if source.kind() == std::io::ErrorKind::StorageFull
        ));
        assert_eq!(catalog.offset, 0);
        assert!(catalog.pending.is_some());
        assert_eq!(catalog.builder.count, 0);
        assert_eq!(fs::metadata(&catalog.builder.ledger_path)?.len(), 0);
        assert!(catalog.advance(2, &budget)?.is_none());
        assert!(!raw_path.exists());
        let files = catalog.finish(&budget)?;
        let mut sorted = BufReader::new(File::open(&files.sorted_path)?);
        let actual = (0..files.count)
            .map(|_| PriorRow::read(&mut sorted, files.mode))
            .collect::<std::io::Result<Vec<_>>>()?;
        assert_eq!(
            actual.iter().map(|row| row.ordinal).collect::<Vec<_>>(),
            [1, 3, 2, 0]
        );
        assert!(
            actual
                .iter()
                .all(|row| row.noise_bits == noise[row.ordinal as usize])
        );
        drop(files);
        assert_eq!(budget.live_bytes(), 0);
        assert_eq!(fs::read_dir(&directory)?.count(), 0);
        fs::remove_dir(&directory)?;
        Ok(())
    }

    #[test]
    fn native_placement_scan_keeps_complete_order_and_physical_noise() -> anyhow::Result<()> {
        let directory =
            std::env::temp_dir().join(format!("aic-native-root-scan-{}", std::process::id()));
        fs::create_dir_all(&directory)?;
        let session = crate::constructive::test_support::session(64);
        let config = crate::constructive::test_support::domain();
        let domain = session.learning_placement_domain(&config)?;
        let mut commands = Vec::new();
        for kind in [CommandKind::Place, CommandKind::PlaceTerminal] {
            let mut offset = 0;
            loop {
                let page = domain.command_page(kind, offset, 7)?;
                commands.extend(
                    page.commands
                        .into_iter()
                        .map(|factored| factored.command.command),
                );
                offset = page.next_offset;
                if page.exhausted {
                    break;
                }
            }
        }
        let budget = SearchMemoryBudget::new(16 << 20);
        let raw_path = directory.join("raw.tmp");
        let RootScoreStart::Ready(mut score) = RootScorePass::new_with_mode(
            None,
            None,
            Device::Cpu,
            config.placement,
            1,
            0.25,
            0.0,
            &raw_path,
            &budget,
            RootNoiseMode::ActionIdentity,
        )?
        else {
            anyhow::bail!("score pass admission stopped");
        };
        let PlacementCatalogStart::Ready(mut scan) = PlacementCatalogScan::new(
            &session,
            &config,
            7,
            RootNoiseMode::ActionIdentity,
            19,
            "state".into(),
            &budget,
        )?
        else {
            anyhow::bail!("native placement admission stopped");
        };
        assert_eq!(scan.legal_count()?, commands.len());
        let peer = TreeReservation::new(&budget, budget.limit_bytes() - budget.live_bytes() - 1024)
            .unwrap();
        assert!(matches!(
            scan.advance(&mut score, &budget)?,
            Some(PlacementCatalogStop::Memory(_))
        ));
        assert_eq!(scan.count, 0);
        drop(peer);
        assert!(scan.advance(&mut score, &budget)?.is_none());
        assert_eq!(scan.count as usize, commands.len());
        let (reader, _) = score.finish()?;
        let mut reader = Some(reader);
        let mut catalog =
            RootCatalogNormalizer::new(&mut reader, &directory.join("root"), &budget)?;
        assert!(catalog.advance(13, &budget)?.is_none());
        assert!(!raw_path.exists());
        let files = catalog.finish(&budget)?;
        assert_eq!(files.count as usize, commands.len());
        let mut sorted = BufReader::new(File::open(&files.sorted_path)?);
        for _ in 0..files.count {
            let row = PriorRow::read(&mut sorted, files.mode)?;
            let command = &commands[row.ordinal as usize];
            let identity = physical_action_identity(&MacroAction::Command(command.clone()))?;
            let expected = deterministic_action_gumbel(19, "state", &identity)?.to_bits();
            assert_eq!(row.noise_bits, expected);
        }
        let shortlist = files.shortlist(4, 1, 0.2, 19, "case", "state", &budget)?;
        let resolved = scan
            .resolve_placement_choices(&shortlist, &budget)?
            .unwrap();
        assert_eq!(resolved.choices.len(), shortlist.len());
        for entry in &resolved.choices {
            assert_eq!(entry.identity, physical_action_identity(&entry.action)?);
            let MacroAction::Command(actual) = &entry.action else {
                panic!("expected placement")
            };
            assert_eq!(actual, &commands[entry.choice.ordinal as usize]);
        }
        drop(resolved);
        drop(scan);
        drop(files);
        drop(catalog);
        assert_eq!(budget.live_bytes(), 0);
        assert_eq!(fs::read_dir(&directory)?.count(), 0);
        fs::remove_dir(directory)?;
        Ok(())
    }
}
