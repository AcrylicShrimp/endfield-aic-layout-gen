use aic_layout::physical_identity;
use aic_layout_gen::Config as GeneratorConfig;
use aic_pilot_native::physical_editor::{PhysicalCandidateOptions, PhysicalSessionLimits};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "aic-physical-outcome-collection-v1";
pub const ROOTS: usize = 96;
pub const MAX_ELAPSED_MS: u64 = 15 * 60 * 1000;
pub const MAX_OUTPUT_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Split {
    Training,
    Development,
    Confirmation,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RootSlot {
    pub root_id: String,
    pub ancestry: String,
    pub split: Split,
    pub blocks: usize,
    pub generation_seed: u64,
    pub action_seed: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub roots: Vec<RootSlot>,
    pub generator: GeneratorConfig,
    pub candidates: PhysicalCandidateOptions,
    pub session: PhysicalSessionLimits,
    pub identity: physical_identity::Limits,
    pub workers: usize,
    pub max_elapsed_ms: u64,
    pub max_output_bytes: u64,
    pub metadata_reserve_bytes: u64,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == SCHEMA,
            "aic-learning.outcomes-config-schema: expected {SCHEMA}"
        );
        ensure!(
            self.roots.len() == ROOTS,
            "aic-learning.outcomes-config-roots: expected exactly {ROOTS} root slots"
        );
        ensure!(
            self.generator == GeneratorConfig::default(),
            "aic-learning.outcomes-config-generator: expected generator defaults before fixed-size override"
        );
        ensure!(
            self.workers == 4,
            "aic-learning.outcomes-config-workers: expected four workers"
        );
        ensure!(
            self.session.max_actions == 32
                && self.session.route_expansions == 50_000
                && self.session.max_total_route_expansions == 1_600_000,
            "aic-learning.outcomes-config-session: expected 32 actions, 50000 per-action and 1600000 total expansions"
        );
        ensure!(
            self.session.elapsed_ms <= MAX_ELAPSED_MS && self.session.elapsed_ms > 0,
            "aic-learning.outcomes-config-session: invalid session elapsed limit"
        );
        ensure!(
            (1..=MAX_ELAPSED_MS).contains(&self.max_elapsed_ms),
            "aic-learning.outcomes-config-elapsed: command limit exceeds 15 minutes"
        );
        ensure!(
            self.max_output_bytes > self.metadata_reserve_bytes
                && self.max_output_bytes <= MAX_OUTPUT_BYTES,
            "aic-learning.outcomes-config-output: invalid aggregate output limit"
        );
        ensure!(
            (1_048_576..=16_777_216).contains(&self.metadata_reserve_bytes),
            "aic-learning.outcomes-config-output: metadata reserve must be 1 to 16 MiB"
        );
        ensure!(
            self.identity.max_individualization_nodes > 0,
            "aic-learning.outcomes-config-identity: identity work must be positive"
        );
        ensure!(
            !self.candidates.translation_steps.is_empty()
                && !self.candidates.band_counts.is_empty(),
            "aic-learning.outcomes-config-actions: action options must be nonempty"
        );

        let mut ids = BTreeSet::new();
        let mut ancestry = BTreeSet::new();
        let mut generation = BTreeSet::new();
        let mut action = BTreeSet::new();
        let mut matrix = BTreeMap::<(usize, Split), usize>::new();
        for root in &self.roots {
            ensure!(
                !root.root_id.is_empty() && !root.ancestry.is_empty(),
                "aic-learning.outcomes-config-root: root and ancestry IDs must be nonempty"
            );
            ensure!(
                root.root_id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')),
                "aic-learning.outcomes-config-root: root ID is not path-safe: {}",
                root.root_id
            );
            ensure!(
                ids.insert(root.root_id.as_str()),
                "aic-learning.outcomes-config-root: duplicate root ID {}",
                root.root_id
            );
            ensure!(
                ancestry.insert(root.ancestry.as_str()),
                "aic-learning.outcomes-config-ancestry: duplicate fresh-root ancestry {}",
                root.ancestry
            );
            ensure!(
                generation.insert(root.generation_seed),
                "aic-learning.outcomes-config-seed: duplicate generation seed {}",
                root.generation_seed
            );
            ensure!(
                action.insert(root.action_seed),
                "aic-learning.outcomes-config-seed: duplicate action seed {}",
                root.action_seed
            );
            ensure!(
                matches!(root.blocks, 8 | 16),
                "aic-learning.outcomes-config-size: root {} is not size 8 or 16",
                root.root_id
            );
            *matrix.entry((root.blocks, root.split)).or_default() += 1;
        }
        for blocks in [8, 16] {
            for (split, expected) in [
                (Split::Training, 32),
                (Split::Development, 8),
                (Split::Confirmation, 8),
            ] {
                ensure!(
                    matrix.get(&(blocks, split)).copied().unwrap_or(0) == expected,
                    "aic-learning.outcomes-config-split: expected {expected} {split:?} roots at size {blocks}"
                );
            }
        }
        Ok(())
    }
}
