//! Verified source-order pages for a frozen Gumbel root target.

use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::Path,
    sync::Arc,
};

use aic_pilot_native::constructive::Diagnostic;
use aic_pilot_native::constructive::learning::{
    CommandKind, Config as DomainConfig, Domain as LearningDomain,
};
use anyhow::{Result, ensure};

use super::{
    ACTION_FEATURES, LEDGER_ROW_BYTES, PriorRow, policy_weight,
    publication::{CatalogBinding, RootCatalogDescriptor, open_replay},
};
use crate::hierarchical_search::{
    AllowanceStop, SearchMemoryBudget, Session, TreeReservation, placement_host_stop,
    placement_page_allocation_failed, placement_page_peak_bytes,
    placement_scoring::{PlacementActionScorer, PreparedPlacement, ScoredPlacement},
    state_sha256,
};

pub(crate) enum RootReplayStart<'s, 'd> {
    Ready(RootReplaySource<'s, 'd>),
    MemoryStop(AllowanceStop),
}

pub(crate) enum RootReplayStep {
    Page(RootReplayPage),
    Complete,
    MemoryStop(AllowanceStop),
}

/// One bounded page. Its reservation covers output rows and the transient
/// native command page peak used while constructing a placement page.
pub(crate) struct RootReplayPage {
    pub(crate) ordinal_start: u64,
    pub(crate) features: Vec<[f32; ACTION_FEATURES]>,
    pub(crate) entity_indices: Vec<i64>,
    pub(crate) targets: Vec<f32>,
    _memory: TreeReservation,
}

pub(crate) struct RootReplaySource<'s, 'd> {
    source: &'s Session,
    descriptor: &'d RootCatalogDescriptor,
    reader: BufReader<File>,
    domain: Arc<LearningDomain>,
    _domain_memory: TreeReservation,
    scorer: PlacementActionScorer<'s>,
    page_size: usize,
    anchor_policy_mass: f64,
    kind_index: usize,
    offset: usize,
    ordinal: u64,
    witness_index: usize,
}

/// A stable, verified sidecar handle retained by the replay window. Reusing
/// this handle avoids rescanning its checksum at every optimizer update.
pub(crate) struct VerifiedRootLedger {
    descriptor: Arc<RootCatalogDescriptor>,
    file: File,
}

impl VerifiedRootLedger {
    pub(crate) fn open(
        report_path: &Path,
        descriptor: Arc<RootCatalogDescriptor>,
        expected: &CatalogBinding,
        source: &Session,
    ) -> Result<Self> {
        ensure!(
            expected.source_revision == source.revision()
                && expected.source_sha256 == state_sha256(source)?,
            "aic-learning.root-replay-source: source identity differs"
        );
        let reader = open_replay(report_path, &descriptor, expected)?;
        validate_descriptor_order(&descriptor)?;
        Ok(Self {
            descriptor,
            file: reader.into_inner(),
        })
    }

    pub(crate) fn descriptor(&self) -> &RootCatalogDescriptor {
        &self.descriptor
    }
}

impl<'s, 'd> RootReplaySource<'s, 'd> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn open(
        report_path: &Path,
        descriptor: &'d RootCatalogDescriptor,
        expected: &CatalogBinding,
        source: &'s Session,
        domain_config: &DomainConfig,
        page_size: usize,
        anchor_policy_mass: f64,
        budget: &SearchMemoryBudget,
    ) -> Result<RootReplayStart<'s, 'd>> {
        ensure!(
            page_size > 0,
            "aic-learning.root-replay-page: zero page size"
        );
        ensure!(
            (0.0..=1.0).contains(&anchor_policy_mass),
            "aic-learning.root-replay-anchor: invalid anchor mass"
        );
        ensure!(
            expected.source_revision == source.revision()
                && expected.source_sha256 == state_sha256(source)?,
            "aic-learning.root-replay-source: source identity differs"
        );
        let reader = open_replay(report_path, descriptor, expected)?;
        validate_descriptor_order(descriptor)?;
        Self::prepare(
            reader,
            descriptor,
            source,
            domain_config,
            page_size,
            anchor_policy_mass,
            budget,
        )
    }

    pub(crate) fn from_verified(
        ledger: &'d VerifiedRootLedger,
        source: &'s Session,
        domain_config: &DomainConfig,
        page_size: usize,
        anchor_policy_mass: f64,
        budget: &SearchMemoryBudget,
    ) -> Result<RootReplayStart<'s, 'd>> {
        let mut reader = BufReader::new(ledger.file.try_clone()?);
        reader.seek(SeekFrom::Start(0))?;
        Self::prepare(
            reader,
            ledger.descriptor(),
            source,
            domain_config,
            page_size,
            anchor_policy_mass,
            budget,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare(
        reader: BufReader<File>,
        descriptor: &'d RootCatalogDescriptor,
        source: &'s Session,
        domain_config: &DomainConfig,
        page_size: usize,
        anchor_policy_mass: f64,
        budget: &SearchMemoryBudget,
    ) -> Result<RootReplayStart<'s, 'd>> {
        ensure!(
            page_size > 0,
            "aic-learning.root-replay-page: zero page size"
        );
        ensure!(
            (0.0..=1.0).contains(&anchor_policy_mass),
            "aic-learning.root-replay-anchor: invalid anchor mass"
        );
        let peak = match native_or_memory(
            source.learning_placement_domain_peak_bytes(domain_config),
            budget,
            usize::MAX,
        )? {
            Ok(peak) => peak,
            Err(stop) => return Ok(RootReplayStart::MemoryStop(stop)),
        };
        let mut domain_memory = match TreeReservation::new(budget, peak) {
            Ok(reservation) => reservation,
            Err(stop) => return Ok(RootReplayStart::MemoryStop(stop)),
        };
        let domain = match native_or_memory(
            source.learning_placement_domain(domain_config),
            budget,
            peak,
        )? {
            Ok(domain) => domain,
            Err(stop) => return Ok(RootReplayStart::MemoryStop(stop)),
        };
        if let Err(stop) = domain_memory.resize(domain.retained_bytes()) {
            return Ok(RootReplayStart::MemoryStop(stop));
        }
        let placement_count =
            match native_or_memory(domain.placement_command_count(), budget, usize::MAX)? {
                Ok(count) => count,
                Err(stop) => return Ok(RootReplayStart::MemoryStop(stop)),
            };
        ensure!(
            placement_count as u64 == descriptor.placement_count,
            "aic-learning.root-replay-order: native placement count differs"
        );
        let scorer = match PlacementActionScorer::prepare(source, domain_config.placement, budget)?
        {
            PreparedPlacement::Ready(scorer) => scorer,
            PreparedPlacement::MemoryStop(stop) => return Ok(RootReplayStart::MemoryStop(stop)),
        };
        Ok(RootReplayStart::Ready(Self {
            source,
            descriptor,
            reader,
            domain,
            _domain_memory: domain_memory,
            scorer,
            page_size,
            anchor_policy_mass,
            kind_index: 0,
            offset: 0,
            ordinal: 0,
            witness_index: 0,
        }))
    }

    /// Reuse the same verified file handle for the normalizer and gradient pass.
    pub(crate) fn rewind(&mut self) -> Result<()> {
        self.reader.seek(SeekFrom::Start(0))?;
        self.kind_index = 0;
        self.offset = 0;
        self.ordinal = 0;
        self.witness_index = 0;
        Ok(())
    }

    pub(crate) fn next_page(&mut self, budget: &SearchMemoryBudget) -> Result<RootReplayStep> {
        while self.kind_index < 2 {
            let kind = [CommandKind::Place, CommandKind::PlaceTerminal][self.kind_index];
            let output_bytes = page_output_bytes(self.page_size);
            let estimate =
                placement_page_peak_bytes(self.source, self.page_size).saturating_add(output_bytes);
            let mut memory = match TreeReservation::new(budget, estimate) {
                Ok(memory) => memory,
                Err(stop) => return Ok(RootReplayStep::MemoryStop(stop)),
            };
            let page = match native_or_memory(
                self.domain.command_page(kind, self.offset, self.page_size),
                budget,
                memory.bytes(),
            )? {
                Ok(page) => page,
                Err(stop) => return Ok(RootReplayStep::MemoryStop(stop)),
            };
            if page.commands.is_empty() {
                ensure!(
                    page.exhausted,
                    "aic-learning.root-replay-page: empty live page"
                );
                self.kind_index += 1;
                self.offset = 0;
                continue;
            }
            let ordinal_start = self.ordinal;
            let mut features = Vec::new();
            let mut entity_indices = Vec::new();
            let mut targets = Vec::new();
            if features.try_reserve_exact(page.commands.len()).is_err()
                || entity_indices
                    .try_reserve_exact(page.commands.len())
                    .is_err()
                || targets.try_reserve_exact(page.commands.len()).is_err()
            {
                return Ok(RootReplayStep::MemoryStop(placement_host_stop(
                    budget,
                    output_bytes,
                )));
            }
            let page_scorer = self.scorer.begin_page(self.source)?;
            let file_position = self.reader.stream_position()?;
            for factored in &page.commands {
                let scored = match page_scorer.score(&factored.command.command, budget)? {
                    ScoredPlacement::Ready(scored) => scored,
                    ScoredPlacement::MemoryStop(stop) => {
                        self.reader.seek(SeekFrom::Start(file_position))?;
                        self.ordinal = ordinal_start;
                        return Ok(RootReplayStep::MemoryStop(stop));
                    }
                };
                let prior = Self::read_prior(&mut self.reader, self.ordinal)?;
                features.push(scored.features);
                entity_indices.push(scored.entity_index);
                targets.push(self.target(self.ordinal, prior));
                self.ordinal += 1;
            }
            self.offset = page.next_offset;
            if page.exhausted {
                self.kind_index += 1;
                self.offset = 0;
            }
            drop(page);
            memory
                .resize(output_bytes)
                .expect("released native page peak cannot be refused");
            return Ok(RootReplayStep::Page(RootReplayPage {
                ordinal_start,
                features,
                entity_indices,
                targets,
                _memory: memory,
            }));
        }
        if self.witness_index == self.descriptor.nonplacement_witnesses.len() {
            ensure!(
                self.ordinal == self.descriptor.initial_eligible_count,
                "aic-learning.root-replay-order: sidecar rows were not fully consumed"
            );
            return Ok(RootReplayStep::Complete);
        }
        let count = self
            .page_size
            .min(self.descriptor.nonplacement_witnesses.len() - self.witness_index);
        let bytes = page_output_bytes(count);
        let memory = match TreeReservation::new(budget, bytes) {
            Ok(memory) => memory,
            Err(stop) => return Ok(RootReplayStep::MemoryStop(stop)),
        };
        let mut features = Vec::new();
        let mut entity_indices = Vec::new();
        let mut targets = Vec::new();
        if features.try_reserve_exact(count).is_err()
            || entity_indices.try_reserve_exact(count).is_err()
            || targets.try_reserve_exact(count).is_err()
        {
            return Ok(RootReplayStep::MemoryStop(placement_host_stop(
                budget, bytes,
            )));
        }
        let ordinal_start = self.ordinal;
        for witness in
            &self.descriptor.nonplacement_witnesses[self.witness_index..self.witness_index + count]
        {
            features.push(witness.features);
            entity_indices.push(witness.entity_index);
            if witness.initial_ordinal.is_some() {
                let prior = Self::read_prior(&mut self.reader, self.ordinal)?;
                targets.push(self.target(self.ordinal, prior));
                self.ordinal += 1;
            } else {
                targets.push(0.0);
            }
        }
        self.witness_index += count;
        Ok(RootReplayStep::Page(RootReplayPage {
            ordinal_start,
            features,
            entity_indices,
            targets,
            _memory: memory,
        }))
    }

    fn read_prior(reader: &mut BufReader<File>, ordinal: u64) -> Result<PriorRow> {
        let mut row = [0u8; LEDGER_ROW_BYTES];
        reader.read_exact(&mut row)?;
        Ok(PriorRow {
            prior: f64::from_le_bytes(row[..8].try_into()?),
            anchor_prior: f64::from_le_bytes(row[8..].try_into()?),
            ordinal,
            noise_bits: 0,
        })
    }

    fn target(&self, ordinal: u64, prior: PriorRow) -> f32 {
        let frozen = &self.descriptor.target;
        let q = frozen
            .visited_q
            .binary_search_by_key(&ordinal, |entry| entry.0)
            .map_or(frozen.q0, |index| frozen.visited_q[index].1);
        ((policy_weight(prior, self.anchor_policy_mass).ln() + q - frozen.log_z).exp()
            / frozen.second_denominator) as f32
    }
}

fn validate_descriptor_order(descriptor: &RootCatalogDescriptor) -> Result<()> {
    let initial_lower_count = descriptor
        .nonplacement_witnesses
        .iter()
        .take_while(|witness| witness.initial_ordinal.is_some())
        .count();
    let expected_initial_count =
        checked_initial_count(descriptor.placement_count, initial_lower_count)?;
    ensure!(
        descriptor
            .nonplacement_witnesses
            .iter()
            .skip(initial_lower_count)
            .all(|witness| witness.initial_ordinal.is_none())
            && descriptor.initial_eligible_count == expected_initial_count,
        "aic-learning.root-replay-order: initial witness count differs"
    );
    for (index, witness) in descriptor
        .nonplacement_witnesses
        .iter()
        .take(initial_lower_count)
        .enumerate()
    {
        let ordinal = descriptor
            .placement_count
            .checked_add(index as u64)
            .ok_or_else(|| anyhow::anyhow!("aic-learning.root-replay-order: ordinal overflow"))?;
        ensure!(
            witness.initial_ordinal == Some(ordinal),
            "aic-learning.root-replay-order: initial witness ordinal differs"
        );
    }
    Ok(())
}

fn page_output_bytes(count: usize) -> usize {
    count
        .saturating_mul(
            std::mem::size_of::<[f32; ACTION_FEATURES]>()
                + std::mem::size_of::<i64>()
                + std::mem::size_of::<f32>(),
        )
        .saturating_add(4096)
}

fn checked_initial_count(placement_count: u64, initial_lower_count: usize) -> Result<u64> {
    let initial_lower_count = u64::try_from(initial_lower_count)
        .map_err(|_| anyhow::anyhow!("aic-learning.root-replay-order: initial count overflow"))?;
    placement_count
        .checked_add(initial_lower_count)
        .ok_or_else(|| anyhow::anyhow!("aic-learning.root-replay-order: initial count overflow"))
}

fn native_or_memory<T>(
    result: std::result::Result<T, Diagnostic>,
    budget: &SearchMemoryBudget,
    requested_bytes: usize,
) -> Result<std::result::Result<T, AllowanceStop>> {
    match result {
        Ok(value) => Ok(Ok(value)),
        Err(diagnostic) => {
            let failure = anyhow::Error::new(diagnostic);
            if placement_page_allocation_failed(&failure) {
                Ok(Err(placement_host_stop(budget, requested_bytes)))
            } else {
                Err(failure)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_allocation_failure_is_retryable_resource_stop() {
        let budget = SearchMemoryBudget::new(1 << 20);
        let diagnostic = Diagnostic {
            code: "learning-resource".into(),
            severity: "error",
            stage: "learning",
            message: "injected allocation failure".into(),
            entity: "page".into(),
        };
        assert!(matches!(
            native_or_memory::<()>(Err(diagnostic), &budget, 4096).unwrap(),
            Err(_)
        ));
        assert_eq!(budget.live_bytes(), 0);
    }

    #[test]
    fn corrupt_persistent_action_count_is_rejected_without_panicking() {
        assert!(checked_initial_count(u64::MAX, 1).is_err());
    }
}
