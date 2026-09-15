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
use std::{
    collections::BTreeMap,
    sync::{Arc, mpsc},
    thread::JoinHandle,
};

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

type GenerationReply = std::result::Result<ProceduralAdmission, String>;

struct GenerationPool {
    commands: Vec<mpsc::Sender<Option<RootSpec>>>,
    replies: Vec<mpsc::Receiver<GenerationReply>>,
    workers: Vec<JoinHandle<()>>,
}

impl GenerationPool {
    fn new(
        catalogs: Arc<aic_layout::validation::Catalogs>,
        config: GeneratorConfig,
        workers: usize,
    ) -> Result<Self> {
        let mut commands = Vec::with_capacity(workers);
        let mut replies = Vec::with_capacity(workers);
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            let (command, receiver) = mpsc::channel();
            let (reply, replies_from_worker) = mpsc::channel();
            let catalogs = catalogs.clone();
            let config = config.clone();
            let handle = std::thread::Builder::new()
                .name(format!("aic-root-generator-{worker}"))
                .spawn(move || {
                    while let Ok(Some(spec)) = receiver.recv() {
                        let result = generate(&catalogs, &config, &spec)
                            .map_err(|error| format!("{error:#}"));
                        if reply.send(result).is_err() {
                            break;
                        }
                    }
                })?;
            commands.push(command);
            replies.push(replies_from_worker);
            handles.push(handle);
        }
        Ok(Self {
            commands,
            replies,
            workers: handles,
        })
    }

    fn generate(&self, specs: &[RootSpec]) -> Result<Vec<ProceduralAdmission>> {
        for (command, spec) in self.commands.iter().zip(specs.iter().cloned()) {
            command
                .send(Some(spec))
                .map_err(|_| anyhow::anyhow!("aic-learning.online-generation: worker exited"))?;
        }
        let mut admissions = (0..specs.len()).map(|_| None).collect::<Vec<_>>();
        let mut failure = None;
        for (index, reply) in self.replies.iter().take(specs.len()).enumerate() {
            let result = reply.recv().map_err(|_| {
                anyhow::anyhow!(
                    "aic-learning.online-generation: worker {index} reply channel closed"
                )
            })?;
            match result {
                Ok(admission) => admissions[index] = Some(admission),
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        if let Some(error) = failure {
            anyhow::bail!(error);
        }
        admissions
            .into_iter()
            .map(|admission| {
                admission.ok_or_else(|| {
                    anyhow::anyhow!("aic-learning.online-generation: missing ordered admission")
                })
            })
            .collect()
    }
}

impl Drop for GenerationPool {
    fn drop(&mut self) {
        for command in &self.commands {
            let _ = command.send(None);
        }
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn generate(
    catalogs: &aic_layout::validation::Catalogs,
    base: &GeneratorConfig,
    spec: &RootSpec,
) -> Result<ProceduralAdmission> {
    ensure!(
        matches!(spec.blocks, 8 | 16),
        "aic-learning.online-size: expected 8 or 16 blocks"
    );
    let mut config = base.clone();
    config.blocks = [spec.blocks, spec.blocks];
    let generator = Generator::new(catalogs, config)
        .map_err(|failure| anyhow::anyhow!(serde_json::to_string(&failure).unwrap()))?;
    for retry in 0..16u64 {
        let seed = spec
            .generation_seed
            .checked_add(retry)
            .ok_or_else(|| anyhow::anyhow!("aic-learning.online-seed: retry overflow"))?;
        let Ok(generated) = generator.generate(seed) else {
            continue;
        };
        let source = serde_json::to_vec(&generated.layout)?;
        if let Ok(admission) = ProceduralAdmission::compile(&source, catalogs) {
            return Ok(admission);
        }
    }
    anyhow::bail!(
        "aic-learning.online-generation: exhausted 16 deterministic seeds for {}",
        spec.root_id
    )
}

pub struct Collector {
    generation: GenerationPool,
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
        let generation = GenerationPool::new(catalogs.clone(), generator, workers)?;
        let pool =
            PhysicalPool::new(catalogs.clone(), workers, workers).map_err(anyhow::Error::msg)?;
        Ok((
            Self {
                generation,
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
        let admissions = self.generation.generate(specs)?;

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
