//! Fresh complete factory construction and durable canonical input slots.
use super::{Setup, geometry_identity, randomize, rng::Rng};
use crate::input::{Catalogs, hash, read};
use aic_data::layouts::{
    ConstructiveProductionRequest, FactoryEditRequest, WitnessProbeInput, construct_production,
};
use aic_data::recipes::{
    Rate, RecipeSourcePlanRequest, build_contextual_facility_instance_wiring,
    calculate_contextual_facility_requirements,
};
use aic_data::work_control::WorkControl;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
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
    plans: Vec<Value>,
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
    fn compute<T>(&self, milliseconds: u64, action: impl FnOnce() -> T) -> Result<T> {
        WorkControl::new(Duration::from_millis(milliseconds), self.guard.clone())?
            .run(action)
            .map_err(|e| error("interrupted", "native-computation", e))
    }
    pub fn with_guard(mut self, guard: Arc<dyn Fn() -> Result<()> + Send + Sync>) -> Self {
        self.guard = guard;
        self
    }
    pub fn load(
        root: &Path,
        catalogs: Arc<Catalogs>,
        config: Config,
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
            plans,
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
            let typed_plan: RecipeSourcePlanRequest =
                serde_json::from_value(plan.clone()).map_err(|e| error("source-plan", slot, e))?;
            let wiring = match self.compute(
                self.config.construction.flow_elapsed_ms.min(remaining()),
                || {
                    let throughput = self
                        .catalogs
                        .data
                        .recipes
                        .calculate_contextual_throughput(&typed_plan);
                    let facilities = calculate_contextual_facility_requirements(&throughput);
                    build_contextual_facility_instance_wiring(&throughput, &facilities)
                },
            ) {
                Ok(value) if value.success => value,
                result => {
                    (self.guard)()?;
                    attempts.push(json!({"seed":seed,"stage":"wiring","outcome":result}));
                    continue;
                }
            };
            let (request, target, utilizations) = request_from_wiring(
                &serde_json::to_value(&wiring).map_err(|e| error("wiring", slot, e))?,
            )?;
            if request.instances.len() > self.config.max_facilities {
                attempts.push(json!({"seed":seed,"stage":"instance-bound"}));
                continue;
            }
            let construction = ConstructiveProductionRequest {
                schema_version: aic_data::layouts::CONSTRUCTIVE_PRODUCTION_SCHEMA_VERSION,
                target_instance: target,
                max_steps: self.config.construction.max_steps,
                max_placement_attempts: self.config.construction.max_placement_attempts,
                max_route_searches: self.config.construction.max_route_searches,
                analysis_workers: self.config.construction.analysis_workers,
                nearby_candidates: 0,
                downstream_closed_growth: true,
                flow_direction: None,
                connection_first_placement: false,
            };
            let data = &self.catalogs.data;
            let mut report = match self.compute(
                self.config.construction.elapsed_ms.min(remaining()),
                || {
                    construct_production(
                        &wiring,
                        &data.facilities,
                        &data.items,
                        &data.transports,
                        &data.components,
                        &construction,
                    )
                },
            ) {
                Ok(value) if value.success && value.complete => value,
                result => {
                    (self.guard)()?;
                    // Keep concise failure evidence, not all speculative layout snapshots.
                    let outcome = result.map(|value| json!({"status":value.status,
                        "success":value.success,"complete":value.complete,"diagnostics":value.diagnostics}));
                    attempts.push(json!({"seed":seed,"stage":"construction","outcome":outcome}));
                    continue;
                }
            };
            let node = report.final_node.take().ok_or_else(|| {
                error(
                    "constructor-witness",
                    slot,
                    "complete constructor omitted its final node",
                )
            })?;
            drop(report); // Release construction history before randomized setup.
            let original = WitnessProbeInput {
                schema_version: 1,
                placements: node.placements,
                transport_networks: node.transport_networks,
                logistics_components: node.logistics_components,
                utilizations: serde_json::from_value(utilizations)
                    .map_err(|e| error("constructor-witness", slot, e))?,
            };
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
            if fs::metadata(&input)
                .map_err(|e| error("read", input.display(), e))?
                .len()
                > 16 * 1024 * 1024
            {
                return Err(error(
                    "input-size",
                    input.display(),
                    "generated input exceeds 16 MiB",
                ));
            }
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
