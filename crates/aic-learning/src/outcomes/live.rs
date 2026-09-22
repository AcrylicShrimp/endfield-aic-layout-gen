//! In-memory factory outcome collection for online supervised learning.
//!
//! Recipe factories are generated through persistent workers and admitted to the
//! same native factory editor used by held-out evaluation. Rows never touch disk.

use super::{
    sample::choose_random,
    storage::{Row, observed_factory_outcome},
};
use aic_layout::rng::Rng;
use aic_pilot_native::{
    input::{CatalogPaths, load_catalogs},
    learning::{
        Limits, Observation, Options,
        pool::{Command, Pool, Reply},
    },
};
use aic_recipe_layout_gen::{
    GeneratedFactory, RecipeFactoryGenerator, RecipeFactorySpec, RecipeGeneratorConfig,
};
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, path::PathBuf, sync::mpsc, thread::JoinHandle};

#[derive(Clone, Debug)]
pub struct RootSpec {
    pub root_id: String,
    pub factory: RecipeFactorySpec,
    pub action_seed: u64,
}

struct Active {
    root_id: String,
    observation: Observation,
    rng: Rng,
    rows: Vec<Row>,
    finished: bool,
}

type GenerationReply = std::result::Result<GeneratedFactory, String>;

struct GenerationPool {
    commands: Vec<mpsc::Sender<Option<RootSpec>>>,
    replies: Vec<mpsc::Receiver<GenerationReply>>,
    workers: Vec<JoinHandle<()>>,
}

impl GenerationPool {
    fn new(generator: RecipeFactoryGenerator, workers: usize) -> Result<Self> {
        let mut commands = Vec::with_capacity(workers);
        let mut replies = Vec::with_capacity(workers);
        let mut handles = Vec::with_capacity(workers);
        for worker in 0..workers {
            let (command, receiver) = mpsc::channel::<Option<RootSpec>>();
            let (reply, replies_from_worker) = mpsc::channel();
            let generator = generator.clone();
            let handle = std::thread::Builder::new()
                .name(format!("aic-recipe-root-generator-{worker}"))
                .spawn(move || {
                    while let Ok(Some(spec)) = receiver.recv() {
                        let result = generator
                            .generate(&spec.factory)
                            .map_err(|error| format!("{}: {error:#}", spec.root_id));
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

    fn generate(&self, specs: &[RootSpec]) -> Result<Vec<GeneratedFactory>> {
        let mut task_by_factory = BTreeMap::<RecipeFactorySpec, usize>::new();
        let mut tasks = Vec::new();
        let task_indices = specs
            .iter()
            .map(|spec| {
                if let Some(&task) = task_by_factory.get(&spec.factory) {
                    task
                } else {
                    let task = tasks.len();
                    task_by_factory.insert(spec.factory.clone(), task);
                    tasks.push(spec.clone());
                    task
                }
            })
            .collect::<Vec<_>>();
        for (command, spec) in self.commands.iter().zip(tasks.iter().cloned()) {
            command
                .send(Some(spec))
                .map_err(|_| anyhow::anyhow!("aic-learning.online-generation: worker exited"))?;
        }
        let mut generated = (0..tasks.len()).map(|_| None).collect::<Vec<_>>();
        let mut failure = None;
        for (index, reply) in self.replies.iter().take(tasks.len()).enumerate() {
            let result = reply.recv().map_err(|_| {
                anyhow::anyhow!(
                    "aic-learning.online-generation: worker {index} reply channel closed"
                )
            })?;
            match result {
                Ok(factory) => generated[index] = Some(factory),
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        if let Some(error) = failure {
            anyhow::bail!(error);
        }
        let generated = generated
            .into_iter()
            .map(|factory| {
                factory.ok_or_else(|| {
                    anyhow::anyhow!("aic-learning.online-generation: missing ordered factory")
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(task_indices
            .into_iter()
            .map(|task| generated[task].clone())
            .collect())
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

pub struct Collector {
    generation: GenerationPool,
    options: Options,
    limits: Limits,
    workers: usize,
    pool: Pool,
}

impl Collector {
    pub fn new(
        catalog_root: &std::path::Path,
        generator: RecipeGeneratorConfig,
        options: Options,
        limits: Limits,
        workers: usize,
    ) -> Result<(Self, String)> {
        ensure!(
            workers > 0,
            "aic-learning.online-workers: workers must be positive"
        );
        ensure!(
            limits.max_actions > 0,
            "aic-learning.online-actions: max actions must be positive"
        );
        let generator = RecipeFactoryGenerator::load(catalog_root, generator)?;
        let generation = GenerationPool::new(generator, workers)?;
        let paths = CatalogPaths {
            facilities: PathBuf::from("facilities.json"),
            recipes: PathBuf::from("recipes.json"),
            items: PathBuf::from("items.json"),
            transports: PathBuf::from("transports.json"),
            components: PathBuf::from("logistics-components.json"),
        };
        let catalogs = load_catalogs(catalog_root, &paths).map_err(anyhow::Error::msg)?;
        let identity = catalogs.identity.clone();
        let pool = Pool::new(catalogs, workers, workers).map_err(anyhow::Error::msg)?;
        Ok((
            Self {
                generation,
                options,
                limits,
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
        let factories = self.generation.generate(specs)?;
        let opens = factories
            .into_iter()
            .enumerate()
            .map(|(stream, factory)| {
                (
                    stream,
                    Command::Next {
                        request: factory.request,
                        original: factory.original,
                        limits: self.limits.clone(),
                        options: self.options.clone(),
                    },
                )
            })
            .collect();
        expect_ready(self.pool.exchange(opens).map_err(anyhow::Error::msg)?)?;
        let observations = self
            .pool
            .exchange(
                (0..specs.len())
                    .map(|stream| (stream, Command::Observe))
                    .collect(),
            )
            .map_err(anyhow::Error::msg)?;
        let mut active = BTreeMap::new();
        for (stream, reply) in observations {
            let Reply::Observation(observation) = reply else {
                anyhow::bail!("aic-learning.online-protocol: expected observation")
            };
            active.insert(
                stream,
                Active {
                    root_id: specs[stream].root_id.clone(),
                    observation,
                    rng: Rng::new(specs[stream].action_seed),
                    rows: Vec::with_capacity(self.limits.max_actions as usize),
                    finished: false,
                },
            );
        }

        while active.values().any(|root| !root.finished) {
            let mut pending = BTreeMap::new();
            let mut commands = Vec::new();
            for (&stream, root) in active.iter_mut().filter(|(_, root)| !root.finished) {
                if root.observation.done || root.rows.len() >= self.limits.max_actions as usize {
                    root.finished = true;
                    continue;
                }
                let selected = choose_random(
                    &root.observation.input,
                    &root.observation.domain,
                    &mut root.rng,
                )?;
                let input = root.observation.input.as_ref().clone();
                commands.push((
                    stream,
                    Command::Step {
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
                let Reply::Step { result, next } = reply else {
                    anyhow::bail!("aic-learning.online-protocol: expected step")
                };
                let (selected, pre_action) = pending.remove(&stream).unwrap();
                let charged = result["outcome"]["charged"] == true;
                let observed = observed_factory_outcome(&result["outcome"], &next, &self.limits)?;
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
                root.observation = next;
                if !charged
                    || root.observation.done
                    || root.rows.len() >= self.limits.max_actions as usize
                {
                    root.finished = true;
                }
            }
        }
        ensure!(
            active.values().all(|root| root.observation.done),
            "aic-learning.online-protocol: collection ended before factory sessions became replaceable"
        );
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

fn expect_ready(replies: Vec<(usize, Reply)>) -> Result<()> {
    ensure!(
        replies
            .into_iter()
            .all(|(_, reply)| matches!(reply, Reply::Ready)),
        "aic-learning.online-protocol: expected ready replies"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_recipe_layout_gen::{RECIPE_GENERATOR_SCHEMA, RecipeFactorySpec, RecipePackingProfile};

    #[test]
    #[ignore = "release-mode integrated smoke; debug routing is intentionally too slow"]
    fn generated_recipe_factories_feed_repeated_native_collection() -> Result<()> {
        let catalogs =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized");
        let generator = RecipeGeneratorConfig {
            schema: RECIPE_GENERATOR_SCHEMA.into(),
            semantic_attempts: 64,
            max_steps: 64,
            max_depth: 4,
            physical_attempts: 16,
            factory_route_searches_per_facility: 8,
            max_factory_route_searches: 200,
        };
        let options = Options {
            translation_steps: vec![-2, -1, 1, 2],
            band_counts: vec![1, 2],
            masking: Some("structural-bands-v1".into()),
        };
        let limits = Limits {
            schema: "declarative-edit-v2".into(),
            max_actions: 4,
            elapsed_ms: 60_000,
            route_expansions: 50_000,
            max_total_route_expansions: 200_000,
            search_padding: 4,
        };
        let (mut collector, identity) = Collector::new(&catalogs, generator, options, limits, 1)?;
        assert_eq!(identity.len(), 64);
        for (ordinal, seed) in [100, 101].into_iter().enumerate() {
            let started = std::time::Instant::now();
            let root_id = format!("integration-{ordinal}");
            let rows = collector.collect(&[RootSpec {
                root_id: root_id.clone(),
                factory: RecipeFactorySpec {
                    target_item: "item-proc-battery-5".into(),
                    facilities: 8,
                    packing_profile: RecipePackingProfile::Compact,
                    seed,
                },
                action_seed: 900 + ordinal as u64,
            }])?;
            assert_eq!(rows.len(), 4);
            assert!(rows.iter().all(|row| row.root_id == root_id));
            eprintln!(
                "root={ordinal} rows={} elapsed_ms={:.3}",
                rows.len(),
                started.elapsed().as_secs_f64() * 1000.0
            );
        }
        Ok(())
    }
}
