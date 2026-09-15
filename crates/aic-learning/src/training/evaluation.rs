//! Held-out native episodes with per-case RNGs and an immutable learner model.
use crate::{collection::policy, input::sha256, model::Model, schema::Archive};
use aic_pilot_native::{
    generation::{geometry_identity, source::Generated},
    input::Catalogs,
    learning::{
        Limits, Observation, Options,
        pool::{Command, Pool, Reply},
    },
};
use aic_torch::{Generator, tch::Device};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub panel: PathBuf,
    pub interval_updates: u64,
    pub replicates: usize,
    pub max_actions: u64,
    pub elapsed_ms: u64,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            !self.panel.as_os_str().is_empty()
                && self.interval_updates > 0
                && self.replicates > 0
                && self.replicates <= 64
                && self.max_actions > 0
                && self.elapsed_ms > 0,
            "aic-learning.evaluation-config: positive panel/cadence/budgets required"
        );
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub product: String,
    pub stratum: String,
    pub action_seed: u64,
    pub geometry_sha256: String,
    pub input: Archive,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: String,
    pub cases: Vec<Case>,
}
pub struct Panel {
    pub manifest: Manifest,
    pub inputs: Vec<Generated>,
    pub reserved: BTreeSet<String>,
    pub identity: String,
}
impl Panel {
    pub fn load(path: &Path, max_input_bytes: u64) -> Result<Self> {
        ensure!(
            fs::metadata(path)?.len() <= 4 * 1024 * 1024,
            "aic-learning.evaluation-panel: oversized {}",
            path.display()
        );
        let manifest: Manifest = serde_json::from_slice(&fs::read(path)?)?;
        ensure!(
            manifest.schema == "aic-evaluation-panel-v1"
                && !manifest.cases.is_empty()
                && manifest.cases.len() <= 4096,
            "aic-learning.evaluation-panel: invalid schema/case bound at {}",
            path.display()
        );
        let mut ids = BTreeSet::new();
        let mut reserved = BTreeSet::new();
        let mut inputs = vec![];
        let mut input_bytes = 0u64;
        let root = path.parent().unwrap_or(Path::new("."));
        for case in &manifest.cases {
            ensure!(
                !case.id.is_empty()
                    && ids.insert(case.id.clone())
                    && !case.product.is_empty()
                    && !case.stratum.is_empty(),
                "aic-learning.evaluation-identity: invalid or duplicate case {}",
                case.id
            );
            let input = root.join(&case.input.path);
            input_bytes = input_bytes.saturating_add(fs::metadata(&input)?.len());
            ensure!(
                input_bytes <= max_input_bytes
                    && fs::metadata(&input)?.len() <= 16 * 1024 * 1024
                    && sha256(&input)? == case.input.sha256,
                "aic-learning.evaluation-input: changed/oversized {}",
                input.display()
            );
            let value: Generated = serde_json::from_slice(&fs::read(&input)?)?;
            ensure!(
                value.metadata["draw"]["product"] == case.product
                    && geometry_identity(&value.original) == case.geometry_sha256
                    && reserved.insert(case.geometry_sha256.clone()),
                "aic-learning.evaluation-geometry: changed or repeated case {}",
                case.id
            );
            inputs.push(value);
        }
        Ok(Self {
            manifest,
            inputs,
            reserved,
            identity: sha256(path)?,
        })
    }
}
pub struct Evaluator {
    pool: Pool,
}
impl Evaluator {
    pub fn new(
        catalogs: Arc<Catalogs>,
        panel: &Panel,
        config: &Config,
        workers: usize,
    ) -> Result<Self> {
        let streams = panel.inputs.len() * config.replicates;
        ensure!(
            streams <= 4096,
            "aic-learning.evaluation-size: too many parallel cases"
        );
        Ok(Self {
            pool: Pool::new(catalogs, workers.min(streams), streams).map_err(anyhow::Error::msg)?,
        })
    }
    pub fn run(
        &mut self,
        panel: &Panel,
        config: &Config,
        native: &Limits,
        options: &Options,
        model: &Model,
        device: Device,
        update: u64,
        mut guard: impl FnMut() -> Result<()>,
    ) -> Result<Value> {
        let began = Instant::now();
        let count = panel.inputs.len() * config.replicates;
        let mut limits = native.clone();
        limits.max_actions = config.max_actions;
        limits.elapsed_ms = config.elapsed_ms;
        let mut commands = vec![];
        let mut generators = vec![];
        for (case, input) in panel.inputs.iter().enumerate() {
            for replicate in 0..config.replicates {
                let id = case * config.replicates + replicate;
                let seed = panel.manifest.cases[case]
                    .action_seed
                    .checked_add(replicate as u64)
                    .ok_or_else(|| anyhow::anyhow!("aic-learning.evaluation-seed: overflow"))?;
                generators.push(Generator::new(seed)?);
                commands.push((
                    id,
                    Command::Next {
                        request: input.request.clone(),
                        original: input.original.clone(),
                        limits: limits.clone(),
                        options: options.clone(),
                    },
                ));
            }
        }
        guard()?;
        self.pool.exchange(commands).map_err(anyhow::Error::msg)?;
        let replies = self
            .pool
            .exchange((0..count).map(|i| (i, Command::Observe)).collect())
            .map_err(anyhow::Error::msg)?;
        let mut current: Vec<Option<Observation>> = (0..count).map(|_| None).collect();
        for (i, reply) in replies {
            let Reply::Observation(o) = reply else {
                anyhow::bail!("aic-learning.evaluation-protocol: observation required")
            };
            current[i] = Some(o);
        }
        let mut charged = vec![0u64; count];
        let mut diagnostics = vec![];
        for _ in 0..config.max_actions {
            guard()?;
            let active: Vec<_> = current
                .iter()
                .enumerate()
                .filter(|(_, o)| !o.as_ref().unwrap().done)
                .map(|(i, _)| i)
                .collect();
            if active.is_empty() {
                break;
            }
            let rows: Vec<_> = active
                .iter()
                .map(|&i| current[i].as_ref().unwrap())
                .collect();
            let choices = policy::sample_with(
                model,
                &rows,
                |owner, probabilities| generators[active[owner]].categorical(probabilities),
                device,
            )?;
            let commands = active
                .iter()
                .zip(choices)
                .map(|(&i, c)| {
                    (
                        i,
                        Command::Step {
                            handle: current[i].as_ref().unwrap().handle,
                            root: c.root,
                            ports: c.ports,
                        },
                    )
                })
                .collect();
            for (i, reply) in self.pool.exchange(commands).map_err(anyhow::Error::msg)? {
                let Reply::Step { result, next } = reply else {
                    anyhow::bail!("aic-learning.evaluation-protocol: step required")
                };
                charged[i] += u64::from(result["outcome"]["charged"] == true);
                if let Some(rows) = result["outcome"]["diagnostics"].as_array() {
                    diagnostics.extend(rows.iter().cloned());
                }
                current[i] = Some(next);
            }
        }
        let mut rows = vec![];
        for (i, reply) in self
            .pool
            .exchange((0..count).map(|i| (i, Command::Result)).collect())
            .map_err(anyhow::Error::msg)?
        {
            let Reply::Data(result) = reply else {
                anyhow::bail!("aic-learning.evaluation-protocol: result required")
            };
            let case = &panel.manifest.cases[i / config.replicates];
            let initial = result["initial_metrics"]["score"][0]
                .as_f64()
                .ok_or_else(|| {
                    anyhow::anyhow!("aic-learning.evaluation-result: missing initial area")
                })?;
            let best = result["best_metrics"]["score"][0].as_f64().ok_or_else(|| {
                anyhow::anyhow!("aic-learning.evaluation-result: missing best area")
            })?;
            ensure!(
                result["factory_validated"] == true && initial > 0. && best > 0.,
                "aic-learning.evaluation-result: invalid retained best for {}",
                case.id
            );
            rows.push(json!({"case":case.id,"product":case.product,"stratum":case.stratum,"replicate":i%config.replicates,
                "actions":charged[i],"budget_complete":charged[i]==config.max_actions,"initial_area":initial,"best_area":best,
                "area_reduction_percent":100.*(1.-best/initial),"best":result["best"]}));
        }
        let mean = rows
            .iter()
            .map(|r| r["area_reduction_percent"].as_f64().unwrap())
            .sum::<f64>()
            / count as f64;
        Ok(
            json!({"schema":"aic-training-evaluation-v1","update":update,"panel_sha256":panel.identity,
            "complete":charged.iter().all(|&n|n==config.max_actions),"mean_area_reduction_percent":mean,
            "seconds":began.elapsed().as_secs_f64(),"rows":rows,"diagnostics":diagnostics}),
        )
    }
}

/// Generate and freeze a reusable held-out panel before training. Its own seed
/// namespace is supplied explicitly; training reserves the resulting geometry.
pub fn prepare(config_path: &Path, output: &Path, cases: usize, seed: u64) -> Result<Value> {
    use super::{config::Config, resources::Guard};
    use crate::collection::storage::write;
    use aic_pilot_native::{generation::source::Source, input::load_catalogs};
    let config = Config::load(config_path)?;
    ensure!(
        !output.exists() && cases >= 2 && cases <= 4096 && cases % 2 == 0,
        "aic-learning.panel-output: new directory and an even 2..4096 cases required for two balanced strata"
    );
    let catalogs =
        load_catalogs(&config.catalog_root, &config.catalogs).map_err(anyhow::Error::msg)?;
    fs::create_dir_all(output)?;
    let guard = Guard::new(
        output,
        &config.resources,
        config.duration_seconds + config.finish_grace_seconds,
    )?;
    let sources: Vec<_> = config
        .strata
        .values()
        .map(|products| -> Result<_> {
            let check = guard.check();
            let mut generation = config.generation.clone();
            generation.seed_base = seed;
            generation.products.retain(|p| products.contains(&p.id));
            Ok(Arc::new(
                Source::load(
                    &config.catalog_root,
                    catalogs.clone(),
                    generation,
                    BTreeSet::new(),
                )
                .map_err(anyhow::Error::msg)?
                .with_guard(Arc::new(move || check.native())),
            ))
        })
        .collect::<Result<_>>()?;
    let mut pool =
        Pool::new(catalogs, config.workers.min(cases), cases).map_err(anyhow::Error::msg)?;
    let replies = pool
        .exchange(
            (0..cases)
                .map(|i| {
                    (
                        i,
                        Command::Generate {
                            source: sources[i % 2].clone(),
                            slot: i as u64,
                            directory: output.join(format!("case-{i:04}")),
                            limits: config.native.clone(),
                            options: config.actions.clone(),
                        },
                    )
                })
                .collect(),
        )
        .map_err(anyhow::Error::msg)?;
    let mut manifest = Manifest {
        schema: "aic-evaluation-panel-v1".into(),
        cases: vec![],
    };
    let mut seen = BTreeSet::new();
    for (i, reply) in replies {
        let Reply::Data(v) = reply else {
            anyhow::bail!("aic-learning.panel-generation: missing actual input")
        };
        let value: Generated = serde_json::from_value(v)?;
        let geometry = geometry_identity(&value.original);
        ensure!(
            seen.insert(geometry.clone()),
            "aic-learning.panel-geometry: repeated geometry; use a different seed"
        );
        let product = value.metadata["draw"]["product"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("aic-learning.panel-product: missing product"))?
            .to_owned();
        let stratum = config
            .strata
            .iter()
            .find(|(_, ps)| ps.contains(&product))
            .unwrap()
            .0
            .clone();
        let path = PathBuf::from(format!("case-{i:04}/input.json"));
        manifest.cases.push(Case {
            id: format!("case-{i:04}"),
            product,
            stratum,
            action_seed: seed
                .checked_add(i as u64 * 64)
                .ok_or_else(|| anyhow::anyhow!("aic-learning.panel-seed: overflow"))?,
            geometry_sha256: geometry,
            input: Archive {
                sha256: sha256(&output.join(&path))?,
                path: path.to_string_lossy().into_owned(),
            },
        });
    }
    write(&output.join("panel.json"), &manifest)?;
    Ok(
        json!({"schema":"aic-panel-preparation-v1","status":"completed","cases":cases,"panel":output.join("panel.json")}),
    )
}
