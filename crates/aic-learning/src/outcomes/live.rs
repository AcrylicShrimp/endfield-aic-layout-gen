//! In-memory outcome collection for online supervised learning.
//!
//! This deliberately shares the native editor protocol and action sampler with
//! the bounded dataset collector, but never serializes layouts or rows.

use super::{
    sample::choose,
    storage::{ObservedOutcome, Row},
};
use aic_layout::rng::Rng;
use aic_layout_gen::{Config as GeneratorConfig, Generator};
use aic_pilot_native::physical_editor::{
    PhysicalCandidateOptions, PhysicalSessionLimits, ProceduralAdmission,
    learning::Observation,
    pool::{PhysicalCommand, PhysicalPool, PhysicalReply},
};
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug)]
pub struct RootSpec {
    pub root_id: String,
    pub blocks: usize,
    pub generation_seed: u64,
    pub action_seed: u64,
}

struct Active {
    root_id: String,
    observation: Observation,
    rng: Rng,
    rows: Vec<Row>,
    finished: bool,
    closable: bool,
}

pub struct Collector {
    catalogs: Arc<aic_layout::validation::Catalogs>,
    generator: GeneratorConfig,
    candidates: PhysicalCandidateOptions,
    session: PhysicalSessionLimits,
    workers: usize,
    pool: PhysicalPool,
}

impl Collector {
    pub fn new(
        catalog_root: &std::path::Path,
        generator: GeneratorConfig,
        candidates: PhysicalCandidateOptions,
        session: PhysicalSessionLimits,
        workers: usize,
    ) -> Result<(Self, String)> {
        ensure!(
            workers > 0,
            "aic-learning.online-workers: workers must be positive"
        );
        ensure!(
            session.max_actions > 0,
            "aic-learning.online-actions: max actions must be positive"
        );
        let loaded = aic_layout_gen::io::load_catalogs(catalog_root)?;
        let catalogs = Arc::new(loaded.catalogs);
        let identity = catalogs.identity.clone();
        let pool =
            PhysicalPool::new(catalogs.clone(), workers, workers).map_err(anyhow::Error::msg)?;
        Ok((
            Self {
                catalogs,
                generator,
                candidates,
                session,
                workers,
                pool,
            },
            identity,
        ))
    }

    pub fn collect(&mut self, specs: &[RootSpec]) -> Result<Vec<Row>> {
        ensure!(
            !specs.is_empty() && specs.len() <= self.workers,
            "aic-learning.online-batch: root count must fit the persistent worker pool"
        );
        let mut admissions = Vec::with_capacity(specs.len());
        for spec in specs {
            ensure!(
                matches!(spec.blocks, 8 | 16),
                "aic-learning.online-size: expected 8 or 16 blocks"
            );
            let mut config = self.generator.clone();
            config.blocks = [spec.blocks, spec.blocks];
            let generator = Generator::new(&self.catalogs, config)
                .map_err(|failure| anyhow::anyhow!(serde_json::to_string(&failure).unwrap()))?;
            let mut admitted = None;
            for retry in 0..16u64 {
                let seed = spec
                    .generation_seed
                    .checked_add(retry)
                    .ok_or_else(|| anyhow::anyhow!("aic-learning.online-seed: retry overflow"))?;
                let Ok(generated) = generator.generate(seed) else {
                    continue;
                };
                let source = serde_json::to_vec(&generated.layout)?;
                if let Ok(admission) = ProceduralAdmission::compile(&source, &self.catalogs) {
                    admitted = Some(admission);
                    break;
                }
            }
            admissions.push(admitted.ok_or_else(|| {
                anyhow::anyhow!(
                    "aic-learning.online-generation: exhausted 16 deterministic seeds for {}",
                    spec.root_id
                )
            })?);
        }

        let opens = admissions
            .into_iter()
            .enumerate()
            .map(|(stream, admission)| {
                (
                    stream,
                    PhysicalCommand::Open {
                        admission,
                        limits: self.session.clone(),
                        options: self.candidates.clone(),
                    },
                )
            })
            .collect();
        expect_ready(self.pool.exchange(opens).map_err(anyhow::Error::msg)?)?;
        let observations = self
            .pool
            .exchange(
                (0..specs.len())
                    .map(|stream| (stream, PhysicalCommand::Observe))
                    .collect(),
            )
            .map_err(anyhow::Error::msg)?;
        let mut active = BTreeMap::new();
        for (stream, reply) in observations {
            let PhysicalReply::Observation(observation) = reply else {
                anyhow::bail!("aic-learning.online-protocol: expected observation")
            };
            active.insert(
                stream,
                Active {
                    root_id: specs[stream].root_id.clone(),
                    observation,
                    rng: Rng::new(specs[stream].action_seed),
                    rows: Vec::with_capacity(self.session.max_actions as usize),
                    finished: false,
                    closable: false,
                },
            );
        }

        while active.values().any(|root| !root.finished) {
            let mut pending = BTreeMap::new();
            let mut commands = Vec::new();
            for (&stream, root) in active.iter_mut().filter(|(_, root)| !root.finished) {
                if root.observation.done || root.rows.len() >= self.session.max_actions as usize {
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
            for (stream, reply) in self.pool.exchange(commands).map_err(anyhow::Error::msg)? {
                let PhysicalReply::Step { outcome, next } = reply else {
                    anyhow::bail!("aic-learning.online-protocol: expected step")
                };
                let (selected, pre_action) = pending.remove(&stream).unwrap();
                let charged = outcome.charged;
                let done = outcome.done;
                let observed = ObservedOutcome::from(outcome);
                let root = active.get_mut(&stream).unwrap();
                if charged {
                    root.rows.push(Row {
                        root_id: root.root_id.clone(),
                        step: root.rows.len() as u64,
                        pre_action,
                        selected,
                        outcome: observed,
                    });
                }
                root.observation = next
                    .map_err(|failure| anyhow::anyhow!(serde_json::to_string(&failure).unwrap()))?;
                if !charged || done || root.rows.len() >= self.session.max_actions as usize {
                    root.finished = true;
                    root.closable = done;
                }
            }
        }
        let close: Vec<_> = active
            .iter()
            .filter_map(|(&stream, root)| root.closable.then_some((stream, PhysicalCommand::Close)))
            .collect();
        if !close.is_empty() {
            expect_closed(self.pool.exchange(close).map_err(anyhow::Error::msg)?)?;
        }
        let rows = active
            .into_values()
            .flat_map(|root| root.rows)
            .collect::<Vec<_>>();
        ensure!(
            !rows.is_empty(),
            "aic-learning.online-batch: native editor produced no charged rows"
        );
        Ok(rows)
    }
}

fn expect_ready(replies: Vec<(usize, PhysicalReply)>) -> Result<()> {
    ensure!(
        replies
            .into_iter()
            .all(|(_, reply)| matches!(reply, PhysicalReply::Ready)),
        "aic-learning.online-protocol: expected ready replies"
    );
    Ok(())
}
fn expect_closed(replies: Vec<(usize, PhysicalReply)>) -> Result<()> {
    ensure!(
        replies
            .into_iter()
            .all(|(_, reply)| matches!(reply, PhysicalReply::Closed)),
        "aic-learning.online-protocol: expected closed replies"
    );
    Ok(())
}
