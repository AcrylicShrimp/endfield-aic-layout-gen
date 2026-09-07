//! Persistent environment ownership and validated, transactional snapshots.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::acceptance;
use crate::cache::Cache;
use crate::candidates;
use crate::catalog::{Catalog, hash};
use crate::command::{self, Command};
use crate::diagnostic::{Diagnostic, Result};
use crate::state::{Index, Layout, Score, State};
use crate::transition::Outcome;

pub const ENGINE_VERSION: u32 = 1;
pub const SNAPSHOT_VERSION: u32 = 2;
static NEXT_ENGINE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle {
    pub identity: u64,
    pub generation: u64,
    pub revision: u64,
}

pub struct Domain {
    pub handle: Handle,
    pub commands: Vec<Command>,
    pub index: Index,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub horizon: u64,
    pub halo: i64,
    pub raster_cell_guard: usize,
    pub move_lengths: Vec<i64>,
    pub segment_lengths: Vec<i64>,
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.horizon == 0
            || self.halo < 0
            || self.raster_cell_guard == 0
            || [&self.move_lengths, &self.segment_lengths]
                .into_iter()
                .any(|v| v.is_empty() || v.iter().any(|x| *x <= 0))
        {
            return Err(Diagnostic::new(
                "input.config",
                "admission",
                "Positive horizon, guard and action lengths, and nonnegative halo are required",
                None,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub schema_version: u32,
    pub engine_version: u32,
    pub provenance: BTreeMap<String, String>,
    pub config: Config,
    pub root_identity: String,
    pub draft: Layout,
    pub base: Layout,
    pub best: Layout,
    pub base_score: Score,
    pub best_score: Score,
    pub steps: u64,
    pub revision: u64,
    pub generation: u64,
    pub trace: Vec<Value>,
    pub episode_counts: BTreeMap<String, u64>,
    pub counters: BTreeMap<String, u64>,
    pub seconds: BTreeMap<String, f64>,
}

pub struct Engine {
    pub catalog: Catalog,
    pub config: Config,
    pub identity: u64,
    pub generation: u64,
    pub root_identity: String,
    pub root: State,
    pub draft: State,
    pub base: State,
    pub best: State,
    pub initial_score: Score,
    pub base_score: Score,
    pub best_score: Score,
    pub scale: f64,
    pub steps: u64,
    pub revision: u64,
    pub trace: Vec<Value>,
    pub episode_counts: BTreeMap<String, u64>,
    pub counters: BTreeMap<String, u64>,
    pub seconds: BTreeMap<String, f64>,
    pub domain: Option<Domain>,
    pub sort_keys: Cache<Command, Arc<str>>,
}

impl Engine {
    pub fn open(root: &Path, case_path: &Path, config: Config) -> Result<Self> {
        config.validate()?;
        let catalog = Catalog::load(root, case_path).map_err(|mut e| {
            e.message = format!("Input {}: {}", case_path.display(), e.message);
            e
        })?;
        for f in &catalog.types {
            if (f.footprint.width as i128) * (f.footprint.height as i128)
                > config.raster_cell_guard as i128
            {
                return Err(Diagnostic::new(
                    "input.resource_guard",
                    "admission",
                    "Facility exceeds declared geometry-work guard",
                    Some(&f.id),
                ));
            }
        }
        let root = State::initial(&catalog, config.raster_cell_guard)?;
        let (initial_score, _) = acceptance::check(&catalog, &root).map_err(|mut e| {
            e.message = format!(
                "Initial fixture {} failed admission: {}",
                case_path.display(),
                e.message
            );
            e
        })?;
        let bytes = serde_json::to_vec(&root.export(&catalog)).map_err(|e| {
            Diagnostic::new("snapshot.serialization", "snapshot", e.to_string(), None)
        })?;
        let root_identity = hash(&bytes);
        let identity = NEXT_ENGINE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| {
                Diagnostic::new(
                    "engine.identity",
                    "engine",
                    "Engine identity space exhausted",
                    None,
                )
            })?;
        Ok(Self {
            catalog,
            config,
            identity,
            generation: 1,
            root_identity,
            draft: root.clone(),
            base: root.clone(),
            best: root.clone(),
            root,
            initial_score,
            base_score: initial_score,
            best_score: initial_score,
            scale: (initial_score[0] as f64).sqrt(),
            steps: 0,
            revision: 0,
            trace: Vec::new(),
            episode_counts: BTreeMap::new(),
            counters: BTreeMap::new(),
            seconds: BTreeMap::new(),
            domain: None,
            sort_keys: Cache::new(4096),
        })
    }

    pub fn reset(&mut self) -> Result<()> {
        let generation = self.generation.checked_add(1).ok_or_else(|| {
            Diagnostic::new(
                "engine.generation",
                "engine",
                "Episode generation exhausted",
                None,
            )
        })?;
        self.draft = self.root.clone();
        self.base = self.root.clone();
        self.best = self.root.clone();
        self.base_score = self.initial_score;
        self.best_score = self.initial_score;
        self.steps = 0;
        self.revision = 0;
        self.trace.clear();
        self.episode_counts.clear();
        self.generation = generation;
        self.domain = None;
        Ok(())
    }

    pub fn export_layout(&self, which: &str) -> Result<Layout> {
        Ok(match which {
            "root" => &self.root,
            "draft" => &self.draft,
            "base" => &self.base,
            "best" => &self.best,
            _ => {
                return Err(Diagnostic::new(
                    "engine.layout",
                    "engine",
                    "Expected root, draft, base or best",
                    Some(which),
                ));
            }
        }
        .export(&self.catalog))
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            schema_version: SNAPSHOT_VERSION,
            engine_version: ENGINE_VERSION,
            provenance: self.catalog.provenance.clone(),
            config: self.config.clone(),
            root_identity: self.root_identity.clone(),
            draft: self.draft.export(&self.catalog),
            base: self.base.export(&self.catalog),
            best: self.best.export(&self.catalog),
            base_score: self.base_score,
            best_score: self.best_score,
            steps: self.steps,
            revision: self.revision,
            generation: self.generation,
            trace: self.trace.clone(),
            episode_counts: self.episode_counts.clone(),
            counters: self.counters.clone(),
            seconds: self.seconds.clone(),
        }
    }

    pub fn restore(&mut self, snapshot: Snapshot) -> Result<()> {
        let invalid = |code: &str, message: &str| Diagnostic::new(code, "snapshot", message, None);
        if snapshot.schema_version != SNAPSHOT_VERSION || snapshot.engine_version != ENGINE_VERSION
        {
            return Err(invalid(
                "snapshot.version",
                "Incompatible native environment snapshot; legacy Python states cannot be resumed",
            ));
        }
        if snapshot.provenance != self.catalog.provenance
            || snapshot.root_identity != self.root_identity
            || snapshot.config != self.config
        {
            return Err(invalid(
                "snapshot.provenance",
                "Snapshot inputs, root or environment configuration differ from this engine",
            ));
        }
        if snapshot.steps > self.config.horizon
            || snapshot.revision != snapshot.steps
            || snapshot.trace.len() as u64 != snapshot.steps
            || snapshot
                .episode_counts
                .get("commands")
                .copied()
                .unwrap_or(0)
                != snapshot.steps
            || snapshot
                .seconds
                .values()
                .any(|v| !v.is_finite() || *v < 0.0)
        {
            return Err(invalid(
                "snapshot.accounting",
                "Snapshot horizon, revision, trace or accounting is inconsistent",
            ));
        }
        let draft = State::import(&self.catalog, snapshot.draft)?;
        let base = State::import(&self.catalog, snapshot.base)?;
        let best = State::import(&self.catalog, snapshot.best)?;
        acceptance::mechanical(&self.catalog, &draft)?;
        let (base_score, _) = acceptance::check(&self.catalog, &base)?;
        let (best_score, _) = acceptance::check(&self.catalog, &best)?;
        if base_score != snapshot.base_score
            || best_score != snapshot.best_score
            || best_score > base_score
            || best_score > self.initial_score
        {
            return Err(invalid(
                "snapshot.score",
                "Rebuilt validated scores disagree with snapshot best/base accounting",
            ));
        }
        let generation = self
            .generation
            .max(snapshot.generation)
            .checked_add(1)
            .ok_or_else(|| invalid("snapshot.generation", "Episode generation exhausted"))?;
        // Commit only after every input and independently rebuilt state passed validation.
        self.draft = draft;
        self.base = base;
        self.best = best;
        self.base_score = base_score;
        self.best_score = best_score;
        self.steps = snapshot.steps;
        self.revision = snapshot.revision;
        self.generation = generation;
        self.trace = snapshot.trace;
        self.episode_counts = snapshot.episode_counts;
        self.counters = snapshot.counters;
        self.seconds = snapshot.seconds;
        self.domain = None;
        self.sort_keys = Cache::new(4096);
        Ok(())
    }

    pub fn candidates(&mut self) -> Result<(Handle, usize)> {
        let started = Instant::now();
        let index = Index::build(&self.catalog, &self.draft)?;
        let commands = candidates::generate(&self.catalog, &self.config, &self.draft, &index)?;
        let mut keyed = Vec::with_capacity(commands.len());
        for command in commands {
            let key = if let Some(key) = self.sort_keys.get(&command) {
                *self.counters.entry("sort_cache_hits".into()).or_default() += 1;
                key.clone()
            } else {
                *self.counters.entry("sort_cache_misses".into()).or_default() += 1;
                let key: Arc<str> = command::canonical(&command.export(&self.catalog)).into();
                self.sort_keys.insert(command.clone(), key.clone());
                key
            };
            keyed.push((key, command));
        }
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        let commands: Vec<_> = keyed.into_iter().map(|(_, c)| c).collect();
        let count = commands.len();
        *self
            .counters
            .entry("candidate_commands".into())
            .or_default() += count as u64;
        self.counters
            .insert("sort_cache_entries".into(), self.sort_keys.len() as u64);
        *self.seconds.entry("candidates".into()).or_default() += started.elapsed().as_secs_f64();
        let handle = Handle {
            identity: self.identity,
            generation: self.generation,
            revision: self.revision,
        };
        self.domain = Some(Domain {
            handle,
            commands,
            index,
        });
        Ok((handle, count))
    }

    pub fn domain(&self, handle: Handle) -> Result<&Domain> {
        self.domain
            .as_ref()
            .filter(|d| d.handle == handle)
            .ok_or_else(|| {
                Diagnostic::new(
                    "engine.stale_handle",
                    "engine",
                    "Candidate handle is stale or belongs to another engine",
                    None,
                )
            })
    }

    pub fn step(&mut self, handle: Handle, action_index: usize) -> Result<(f64, bool, Outcome)> {
        let command = self
            .domain(handle)?
            .commands
            .get(action_index)
            .cloned()
            .ok_or_else(|| {
                Diagnostic::new(
                    "engine.action_index",
                    "engine",
                    "Action index is outside the candidate domain",
                    None,
                )
            })?;
        let trace = command.export(&self.catalog);
        self.attempt(Ok(command), trace)
    }

    pub fn export_candidates(&self, handle: Handle) -> Result<Vec<Value>> {
        Ok(self
            .domain(handle)?
            .commands
            .iter()
            .map(|c| c.export(&self.catalog))
            .collect())
    }

    pub fn metadata(&self) -> Value {
        json!({
            "engine_version":ENGINE_VERSION,"snapshot_version":SNAPSHOT_VERSION,
            "case_path":self.catalog.path,"case":self.catalog.case_json,
            "provenance":self.catalog.provenance,"root_identity":self.root_identity,
            "config":self.config,"initial_score":self.initial_score,
            "facility_ids":self.catalog.case.facilities.iter().map(|f| &f.id).collect::<Vec<_>>(),
            "network_ids":self.catalog.case.networks.iter().map(|n| &n.id).collect::<Vec<_>>(),
            "terminal_ids":self.catalog.terminals.iter().map(|t| &t.id).collect::<Vec<_>>(),
            "facility_types":self.catalog.types,"recipes":self.catalog.recipes,
            "operations":self.operation_names(),
        })
    }
}
