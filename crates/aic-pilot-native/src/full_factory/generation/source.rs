//! Fresh complete factory construction and durable canonical input slots.
use super::{Setup, geometry_identity, randomize, rng::Rng};
use crate::input::{CatalogPaths, Catalogs, hash, read};
use aic_data::layouts::{FactoryEditRequest, WitnessProbeInput};
use aic_data::recipes::Rate;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

type Result<T> = std::result::Result<T, String>;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Product {
    pub id: String,
    pub source: PathBuf,
    pub duration_ms: [u64; 2],
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Construction {
    pub flow_elapsed_ms: u64,
    pub elapsed_ms: u64,
    pub max_steps: usize,
    pub max_placement_attempts: usize,
    pub max_route_searches: u64,
    pub analysis_workers: usize,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub seed_base: u64,
    pub max_attempts: usize,
    pub episode_setup_ms: u64,
    pub products: Vec<Product>,
    pub duration_step_ms: u64,
    pub max_facilities: usize,
    pub setup_steps: [usize; 2],
    pub setup_elapsed_ms: u64,
    pub setup_kinds: Vec<String>,
    pub route_work: usize,
    pub construction: Construction,
}
pub struct Source {
    pub config: Config,
    pub identity: String,
    catalogs: Arc<Catalogs>,
    root: PathBuf,
    paths: CatalogPaths,
    plans: Vec<Value>,
    executable: PathBuf,
    reserved: BTreeSet<String>,
    guard: Arc<dyn Fn() -> Result<()> + Send + Sync>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Generated {
    pub request: FactoryEditRequest,
    pub original: WitnessProbeInput,
    pub metadata: Value,
}
fn error(stage: &str, entity: impl std::fmt::Display, message: impl std::fmt::Display) -> String {
    json!({"code":format!("aic-generation.{stage}"),"severity":"error","stage":"input-generation",
        "entity":entity.to_string(),"message":message.to_string()})
    .to_string()
}
fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = std::io::BufWriter::new(
        fs::File::create(path).map_err(|e| error("write", path.display(), e))?,
    );
    serde_json::to_writer(&mut file, value).map_err(|e| error("write", path.display(), e))?;
    file.flush().map_err(|e| error("write", path.display(), e))
}
impl Source {
    pub fn with_guard(mut self, guard: Arc<dyn Fn() -> Result<()> + Send + Sync>) -> Self {
        self.guard = guard;
        self
    }
    pub fn load(
        root: &Path,
        paths: CatalogPaths,
        catalogs: Arc<Catalogs>,
        config: Config,
        executable: PathBuf,
        reserved: BTreeSet<String>,
    ) -> Result<Self> {
        let c = &config;
        if c.schema != "aic-native-generation-v1"
            || c.max_attempts == 0
            || c.max_attempts > 32
            || c.episode_setup_ms == 0
            || c.products.is_empty()
            || c.products.len() > 4096
            || c.duration_step_ms == 0
            || c.max_facilities == 0
            || c.max_facilities > 4096
            || c.setup_steps[0] == 0
            || c.setup_steps[0] > c.setup_steps[1]
            || c.setup_steps[1] > 32
            || c.setup_elapsed_ms == 0
            || c.route_work == 0
            || c.construction.flow_elapsed_ms == 0
            || c.construction.elapsed_ms == 0
            || c.construction.max_steps == 0
            || c.construction.max_placement_attempts == 0
            || c.construction.max_route_searches == 0
            || c.construction.analysis_workers == 0
            || c.setup_kinds.is_empty()
            || c.setup_kinds.iter().any(|k| {
                !matches!(
                    k.as_str(),
                    "pose"
                        | "contract"
                        | "insert"
                        | "port"
                        | "reroute"
                        | "component"
                        | "release_bridge"
                )
            })
        {
            return Err(error(
                "config",
                root.display(),
                "invalid fresh input schema, bounds or setup kinds",
            ));
        }
        let mut ids = BTreeSet::new();
        let mut plans = vec![];
        for product in &c.products {
            let [low, high] = product.duration_ms;
            if product.id.is_empty()
                || !ids.insert(&product.id)
                || low == 0
                || high < low
                || high > 1_000_000_000
                || low % c.duration_step_ms != 0
                || high % c.duration_step_ms != 0
            {
                return Err(error(
                    "product",
                    &product.id,
                    "invalid identity or duration grid",
                ));
            }
            let plan: Value = read(root, &product.source)?;
            if plan["schema_version"] != 1 || !plan["target"].is_object() {
                return Err(error(
                    "source-plan",
                    product.source.display(),
                    "expected source-plan schema 1",
                ));
            }
            plans.push(plan);
        }
        let identity = hash(
            serde_json::to_string(
                &json!({"config":config,"catalogs":catalogs.identity,"plans":plans,
            "rng":"splitmix64-v1"}),
            )
            .unwrap()
            .as_bytes(),
        );
        Ok(Self {
            config,
            identity,
            catalogs,
            root: root.to_path_buf(),
            paths,
            plans,
            executable,
            reserved,
            guard: Arc::new(|| Ok(())),
        })
    }
    pub fn generate(&self, slot: u64, directory: &Path) -> Result<Generated> {
        let completed = directory.join("manifest.json");
        if completed.exists() {
            let manifest: Value = read(directory, Path::new("manifest.json"))?;
            if fs::metadata(directory.join("input.json"))
                .map_err(|e| error("read", directory.display(), e))?
                .len()
                > 16 * 1024 * 1024
            {
                return Err(error(
                    "input-size",
                    directory.display(),
                    "actual input exceeds 16 MiB",
                ));
            }
            let bytes = fs::read(directory.join("input.json"))
                .map_err(|e| error("read", directory.display(), e))?;
            if bytes.len() > 16 * 1024 * 1024
                || manifest["schema"] != "aic-generated-slot-v1"
                || manifest["slot"] != slot
                || manifest["source"] != self.identity
                || manifest["input_sha256"] != hash(&bytes)
            {
                return Err(error(
                    "slot-binding",
                    directory.display(),
                    "committed actual input differs from this source/slot",
                ));
            }
            let value: Generated = serde_json::from_slice(&bytes)
                .map_err(|e| error("input", directory.display(), e))?;
            if self.reserved.contains(&geometry_identity(&value.original)) {
                return Err(error(
                    "reservation",
                    directory.display(),
                    "training geometry is reserved for evaluation",
                ));
            }
            return Ok(value);
        }
        if directory.exists() {
            return Err(error(
                "incomplete",
                directory.display(),
                "preserved incomplete generation; no replacement from seed",
            ));
        }
        fs::create_dir_all(directory).map_err(|e| error("directory", directory.display(), e))?;
        let started = Instant::now();
        let first = slot
            .checked_mul(self.config.max_attempts as u64)
            .and_then(|n| self.config.seed_base.checked_add(n))
            .ok_or_else(|| error("seed", slot, "canonical seed namespace exhausted"))?;
        let mut attempts = vec![];
        for attempt in 0..self.config.max_attempts {
            (self.guard)()?;
            if started.elapsed().as_millis() >= self.config.episode_setup_ms as u128 {
                break;
            }
            let seed = first
                .checked_add(attempt as u64)
                .ok_or_else(|| error("seed", slot, "attempt seed overflow"))?;
            let mut rng = Rng::new(seed);
            let product_index = rng.index(self.config.products.len());
            let product = &self.config.products[product_index];
            let low = product.duration_ms[0] / self.config.duration_step_ms;
            let count = (product.duration_ms[1] - product.duration_ms[0])
                / self.config.duration_step_ms
                + 1;
            let duration_ms =
                (low + rng.index(count as usize) as u64) * self.config.duration_step_ms;
            let steps = self.config.setup_steps[0]
                + rng.index(self.config.setup_steps[1] - self.config.setup_steps[0] + 1);
            let setup_seed = rng.next_u64();
            let draw = json!({"product":product.id,"source":product.source,"rate":{"quantity":1,"duration_ms":duration_ms},"setup_steps":steps,"setup_seed":setup_seed});
            let mut plan = self.plans[product_index].clone();
            plan["target"]["quantity"] = json!(1);
            plan["target"]["duration_ms"] = json!(duration_ms);
            let source_path = directory.join("source-plan.json");
            write(&source_path, &plan)?;
            let remaining = || {
                self.config
                    .episode_setup_ms
                    .saturating_sub(started.elapsed().as_millis() as u64)
            };
            let flow_args = vec![
                "recipes".into(),
                "contextual-instance-wiring".into(),
                "--file".into(),
                self.root.join(&self.paths.recipes).display().to_string(),
                "--request".into(),
                source_path.display().to_string(),
            ];
            let wiring = match process(
                &self.executable,
                &flow_args,
                self.config.construction.flow_elapsed_ms.min(remaining()),
                &*self.guard,
            ) {
                Ok(value) if value["success"] == true => value,
                result => {
                    attempts.push(json!({"seed":seed,"stage":"wiring","outcome":result.map_err(|e| e.to_string())}));
                    continue;
                }
            };
            let (request, target, utilizations) = request_from_wiring(&wiring)?;
            if request.instances.len() > self.config.max_facilities {
                attempts.push(json!({"seed":seed,"stage":"instance-bound"}));
                continue;
            }
            let mut args = vec![
                "layouts".into(),
                "construct-production".into(),
                "--recipes".into(),
                self.root.join(&self.paths.recipes).display().to_string(),
                "--source-plan".into(),
                source_path.display().to_string(),
                "--target-instance".into(),
                target,
                "--downstream-closed-growth".into(),
                "--final-json".into(),
            ];
            for (flag, path) in [
                ("facility-catalog", &self.paths.facilities),
                ("item-catalog", &self.paths.items),
                ("transport-catalog", &self.paths.transports),
                ("logistics-component-catalog", &self.paths.components),
            ] {
                args.extend([
                    format!("--{flag}"),
                    self.root.join(path).display().to_string(),
                ]);
            }
            for (flag, n) in [
                ("max-steps", self.config.construction.max_steps as u64),
                (
                    "max-placement-attempts",
                    self.config.construction.max_placement_attempts as u64,
                ),
                (
                    "max-route-searches",
                    self.config.construction.max_route_searches,
                ),
                (
                    "analysis-workers",
                    self.config.construction.analysis_workers as u64,
                ),
            ] {
                args.extend([format!("--{flag}"), n.to_string()]);
            }
            let report = match process(
                &self.executable,
                &args,
                self.config.construction.elapsed_ms.min(remaining()),
                &*self.guard,
            ) {
                Ok(value)
                    if value["schema_version"] == 1
                        && value["kind"] == "constructive-production-final"
                        && value["success"] == true
                        && value["complete"] == true =>
                {
                    value
                }
                result => {
                    attempts.push(json!({"seed":seed,"stage":"construction","outcome":result.map_err(|e| e.to_string())}));
                    continue;
                }
            };
            let node = &report["final_node"];
            let original: WitnessProbeInput = serde_json::from_value(json!({"schema_version":1,"placements":node["placements"],
                "transport_networks":node["transport_networks"],"logistics_components":node["logistics_components"],"utilizations":utilizations}))
                .map_err(|e| error("constructor-witness", slot, e))?;
            let setup = Setup {
                steps,
                elapsed_ms: self.config.setup_elapsed_ms.min(remaining()),
                route_work: self.config.route_work,
                kinds: self.config.setup_kinds.clone(),
            };
            let randomized = match randomize(
                request.clone(),
                original,
                self.catalogs.clone(),
                &setup,
                setup_seed,
                &*self.guard,
            ) {
                Ok(value) => value,
                Err(failure) => {
                    write(
                        &directory.join("setup-partial.json"),
                        &json!({"request":request,"draw":draw,"failure":failure}),
                    )?;
                    return Err(error("setup-failed", directory.display(), &failure.message));
                }
            };
            if self.reserved.contains(&randomized.geometry_sha256) {
                attempts.push(json!({"seed":seed,"stage":"reservation","geometry_sha256":randomized.geometry_sha256}));
                continue;
            }
            let metadata = json!({"schema":"aic-generated-input-v1","slot":slot,"generation_seed":seed,"source":self.identity,
                "rng":"splitmix64-v1","draw":draw,"geometry_sha256":randomized.geometry_sha256,
                "constructor_geometry_sha256":randomized.constructor_geometry_sha256,"setup":randomized.records,"attempts":attempts,
                "setup_seconds":started.elapsed().as_secs_f64()});
            let value = Generated {
                request,
                original: randomized.original,
                metadata,
            };
            let input = directory.join("input.json");
            write(&input, &value)?;
            let bytes = fs::read(&input).map_err(|e| error("read", input.display(), e))?;
            let pending = directory.join("manifest.pending");
            write(
                &pending,
                &json!({"schema":"aic-generated-slot-v1","slot":slot,"source":self.identity,"input_sha256":hash(&bytes)}),
            )?;
            fs::File::open(&input)
                .and_then(|f| f.sync_all())
                .map_err(|e| error("sync", input.display(), e))?;
            fs::File::open(&pending)
                .and_then(|f| f.sync_all())
                .map_err(|e| error("sync", pending.display(), e))?;
            fs::rename(&pending, &completed)
                .map_err(|e| error("publish", completed.display(), e))?;
            fs::remove_file(source_path).map_err(|e| error("cleanup", directory.display(), e))?;
            fs::File::open(directory)
                .and_then(|f| f.sync_all())
                .map_err(|e| error("sync", directory.display(), e))?;
            return Ok(value);
        }
        write(
            &directory.join("failure.json"),
            &json!({"slot":slot,"source":self.identity,"attempts":attempts,"seconds":started.elapsed().as_secs_f64()}),
        )?;
        Err(error(
            "exhausted",
            directory.display(),
            "bounded generation attempts or elapsed time exhausted; not global infeasibility",
        ))
    }
}

fn request_from_wiring(wiring: &Value) -> Result<(FactoryEditRequest, String, Value)> {
    let nodes = wiring["nodes"]
        .as_array()
        .ok_or_else(|| error("wiring", "nodes", "missing nodes"))?;
    let facilities: Vec<_> = nodes.iter().filter(|n| n["kind"] == "facility").collect();
    let members: BTreeSet<_> = facilities.iter().filter_map(|n| n["id"].as_str()).collect();
    let targets: BTreeSet<_> = nodes
        .iter()
        .filter(|n| n["kind"] == "target")
        .filter_map(|n| n["id"].as_str())
        .collect();
    let mut totals: BTreeMap<(String, String, String), Rate> = BTreeMap::new();
    let edges = wiring["edges"]
        .as_array()
        .ok_or_else(|| error("wiring", "edges", "missing edges"))?;
    for edge in edges {
        let rate: Rate = serde_json::from_value(edge["rate"].clone())
            .map_err(|e| error("wiring-rate", "edge", e))?;
        if rate.numerator <= 0 || rate.denominator <= 0 {
            return Err(error("wiring-rate", "edge", "nonpositive rate"));
        }
        let item = edge["item"]
            .as_str()
            .ok_or_else(|| error("wiring-item", "edge", "missing item"))?;
        for (field, direction) in [("source", "output"), ("target", "input")] {
            let node = edge[field]
                .as_str()
                .ok_or_else(|| error("wiring-node", field, "missing endpoint"))?;
            let key = (node.to_owned(), direction.to_owned(), item.to_owned());
            let current = totals.entry(key).or_insert(Rate::zero());
            *current = current
                .checked_add(rate)
                .map_err(|e| error("wiring-rate", "sum", format!("{e:?}")))?;
        }
    }
    let terminals: Vec<_> = totals
        .into_iter()
        .map(|((node, direction, item), rate)| {
            let endpoint = if members.contains(node.as_str()) {
                json!({"kind":"facility","instance":node})
            } else {
                json!({"kind":"external","node":node})
            };
            json!({"endpoint":endpoint,"direction":direction,"item":item,"rate":rate})
        })
        .collect();
    let instances: Vec<_> = facilities.iter().map(|n| json!({"instance":n["id"],"recipe":n["recipe"],"utilization":n["work_seconds_per_second"]})).collect();
    let request = serde_json::from_value(
        json!({"schema_version":1,"instances":instances,"terminals":terminals}),
    )
    .map_err(|e| error("request", "wiring", e))?;
    let target = edges
        .iter()
        .find(|e| e["target"].as_str().is_some_and(|s| targets.contains(s)))
        .and_then(|e| e["source"].as_str())
        .ok_or_else(|| error("target", "wiring", "missing target source"))?
        .to_owned();
    let utilizations: serde_json::Map<String, Value> = facilities
        .iter()
        .map(|n| {
            (
                n["id"].as_str().unwrap().to_owned(),
                n["work_seconds_per_second"].clone(),
            )
        })
        .collect();
    Ok((request, target, Value::Object(utilizations)))
}

fn process(
    binary: &Path,
    args: &[String],
    milliseconds: u64,
    guard: &dyn Fn() -> Result<()>,
) -> Result<Value> {
    if milliseconds == 0 {
        return Err(error(
            "process-budget",
            binary.display(),
            "no remaining setup time",
        ));
    }
    let start = Instant::now();
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| error("process-spawn", binary.display(), e))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let out = std::thread::spawn(move || {
        let mut bytes = vec![];
        stdout
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let err = std::thread::spawn(move || {
        let mut bytes = vec![];
        stderr
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let status = loop {
        if let Err(error) = guard() {
            let _ = child.kill();
            let _ = child.wait();
            break Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() < Duration::from_millis(milliseconds) => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(error(
                    "process-timeout",
                    binary.display(),
                    "native constructor exceeded its elapsed budget",
                ));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(error("process-wait", binary.display(), e));
            }
        }
    };
    let bytes = out
        .join()
        .map_err(|_| error("process-output", binary.display(), "stdout reader panicked"))?
        .map_err(|e| error("process-output", binary.display(), e))?;
    let errors = err
        .join()
        .map_err(|_| error("process-output", binary.display(), "stderr reader panicked"))?
        .map_err(|e| error("process-output", binary.display(), e))?;
    let status = status?;
    if bytes.len() > 16 * 1024 * 1024 || errors.len() > 1024 * 1024 {
        return Err(error(
            "process-output",
            binary.display(),
            "bounded output exceeded",
        ));
    }
    if !status.success() {
        return Err(error(
            "process-failed",
            binary.display(),
            String::from_utf8_lossy(&errors),
        ));
    }
    serde_json::from_slice(&bytes).map_err(|e| error("process-json", binary.display(), e))
}
