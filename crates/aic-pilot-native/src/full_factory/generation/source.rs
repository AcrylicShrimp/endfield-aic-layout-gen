//! Fresh complete factory construction and durable canonical input slots.
use super::{
    Setup,
    physical_identity::{self, Limits, PhysicalIdentity, SEMANTICS},
    randomize,
    rng::Rng,
};
use crate::input::{Catalogs, hash, hash_file, read, read_generated_bytes};
use aic_data::facilities::FacilityPortEdge;
use aic_data::layouts::{
    ConstructiveProductionCompactResult, ConstructiveProductionRequest, FactoryEditRequest,
    WitnessProbeInput, construct_production_compact,
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
const DEFAULT_PER_CONNECTION_ROUTE_SEARCHES: u64 = 64;

fn default_per_connection_route_searches() -> u64 {
    DEFAULT_PER_CONNECTION_ROUTE_SEARCHES
}
fn is_default_per_connection_route_searches(value: &u64) -> bool {
    *value == DEFAULT_PER_CONNECTION_ROUTE_SEARCHES
}
fn default_downstream_closed_growth() -> bool {
    true
}
fn is_true(value: &bool) -> bool {
    *value
}
fn is_false(value: &bool) -> bool {
    !*value
}
fn is_zero(value: &usize) -> bool {
    *value == 0
}
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
    #[serde(
        default = "default_per_connection_route_searches",
        skip_serializing_if = "is_default_per_connection_route_searches"
    )]
    pub per_connection_route_searches: u64,
    pub analysis_workers: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub nearby_candidates: usize,
    #[serde(
        default = "default_downstream_closed_growth",
        skip_serializing_if = "is_true"
    )]
    pub downstream_closed_growth: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_direction: Option<FacilityPortEdge>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub connection_first_placement: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub identity_limits: Limits,
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
    /// Absent only in legacy actual payloads accepted by explicit panel migration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admission: Option<Admission>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootReservation {
    pub semantics: String,
    pub structural: String,
    pub episode: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Admission {
    pub physical: PhysicalIdentity,
    pub catalog_sha256: String,
    pub root: RootReservation,
}
impl Admission {
    pub fn new(physical: PhysicalIdentity, catalogs: &Catalogs, episode: String) -> Self {
        Self {
            root: RootReservation {
                semantics: SEMANTICS.into(),
                structural: physical.structural.clone(),
                episode,
            },
            physical,
            catalog_sha256: catalogs.identity.clone(),
        }
    }
    pub fn is_reserved(&self, reserved: &BTreeSet<String>) -> bool {
        reserved.contains(&self.physical.structural) || reserved.contains(&self.root.structural)
    }
    pub fn validate(&self) -> Result<()> {
        let hash = |s: &str| {
            s.len() == 64
                && s.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        };
        if self.physical.semantics != SEMANTICS
            || self.root.semantics != SEMANTICS
            || self.root.episode.is_empty()
            || !hash(&self.root.structural)
            || !hash(&self.physical.structural)
            || !hash(&self.physical.exact_problem)
            || !hash(&self.catalog_sha256)
        {
            return Err(
                "aic-learning.physical-identity-metadata: invalid admission keys/root".into(),
            );
        }
        Ok(())
    }
}
impl Generated {
    pub fn admitted(&self) -> Result<&Admission> {
        self.admission.as_ref().ok_or_else(|| {
            "aic-learning.physical-identity-metadata: legacy input needs explicit migration".into()
        })
    }
    /// External/cached input boundary: validate actual payload, compute keys once,
    /// and compare any supplied metadata. Descendant roots are preserved unchanged.
    pub fn admit(
        &mut self,
        catalogs: &Catalogs,
        limits: &Limits,
        entity: &str,
        legacy_episode: Option<String>,
    ) -> Result<()> {
        let physical = physical_identity::validate_and_identify(
            &self.request,
            &self.original,
            &catalogs.data,
            limits,
            entity,
        )?;
        if let Some(admission) = &self.admission {
            admission.validate()?;
            if admission.physical != physical || admission.catalog_sha256 != catalogs.identity {
                return Err(format!(
                    "aic-learning.physical-identity-metadata: changed payload/catalog at {entity}"
                ));
            }
        } else {
            let episode = legacy_episode.ok_or_else(|| format!("aic-learning.physical-identity-metadata: legacy input at {entity}; migrate its panel explicitly"))?;
            self.admission = Some(Admission::new(physical, catalogs, episode));
        }
        Ok(())
    }
}
fn error(stage: &str, entity: impl std::fmt::Display, message: impl std::fmt::Display) -> String {
    json!({"code":format!("aic-generation.{stage}"),"severity":"error","stage":"input-generation",
        "entity":entity.to_string(),"message":message.to_string()})
    .to_string()
}
pub(super) fn construction_failure_summary(report: &ConstructiveProductionCompactResult) -> Value {
    let closed = report.final_node.as_ref();
    let mut reasons = report.rejected_reasons.iter().collect::<Vec<_>>();
    reasons.sort_by(|(ak, av), (bk, bv)| bv.cmp(av).then_with(|| ak.cmp(bk)));
    let top_reasons = reasons.into_iter().take(8).collect::<BTreeMap<_, _>>();
    let stop_counts = report
        .rejected_reasons
        .iter()
        .filter(|(key, _)| {
            key.starts_with("production-connection-")
                || key == &"production-step-route-budget-exhausted"
                || key.starts_with("production-external-")
        })
        .collect::<BTreeMap<_, _>>();
    json!({
        "status": report.status,
        "success": report.success,
        "complete": report.complete,
        "required_facilities": report.required_facilities,
        "completed_growth_steps": report.accepted_growth_steps,
        "accepted_closed_facilities": closed.map_or(0, |node| node.member_instances.len()),
        "accepted_closed_internal_connections": closed.map_or(0, |node| node.internal_requirements.len()),
        "failed_step_index": report.accepted_steps,
        "failed_placement_attempts": report.failed_placement_attempts,
        "failed_statistics": report.failed_statistics,
        "elapsed_ms": report.elapsed_ms,
        "max_steps": report.request.max_steps,
        "max_placement_attempts": report.request.max_placement_attempts,
        "max_route_searches": report.request.max_route_searches,
        "per_connection_route_limit": report.per_connection_route_limit,
        "routing_workspace_cell_limit": report.routing_workspace_cell_limit,
        "search_stop_counts": stop_counts,
        "top_rejected_reasons": top_reasons,
        "diagnostics": report.diagnostics,
    })
}

fn work_error_kind(error: &str) -> &'static str {
    let message = serde_json::from_str::<Value>(error)
        .ok()
        .and_then(|value| value["message"].as_str().map(str::to_owned));
    if message
        .as_deref()
        .is_some_and(|message| message.starts_with("native-work.elapsed-budget:"))
    {
        "elapsed-budget"
    } else {
        "work-or-caller-interruption"
    }
}
fn write(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = std::io::BufWriter::new(
        fs::File::create(path).map_err(|e| error("write", path.display(), e))?,
    );
    serde_json::to_writer(&mut file, value).map_err(|e| error("write", path.display(), e))?;
    file.flush().map_err(|e| error("write", path.display(), e))
}
impl Source {
    /// Retained, factory-validated episode result. Recompute only at publication;
    /// the root reservation survives every descendant geometry change.
    pub fn publish_best(&self, value: &mut Generated, entity: &str) -> Result<()> {
        let admission = value.admission.as_mut().ok_or("Missing admitted parent")?;
        admission.validate()?;
        if admission.catalog_sha256 != self.catalogs.identity {
            return Err(format!(
                "aic-learning.physical-identity-metadata: catalog mismatch at {entity}"
            ));
        }
        admission.physical = physical_identity::identify(
            &value.original,
            &self.catalogs.data,
            &self.config.identity_limits,
            entity,
        )?
        .0;
        Ok(())
    }
    pub fn check_reservation(&self, value: &Generated, entity: &str) -> Result<()> {
        let admission = value.admitted()?;
        admission.validate()?;
        if admission.catalog_sha256 != self.catalogs.identity
            || admission.is_reserved(&self.reserved)
        {
            return Err(format!(
                "aic-learning.physical-reservation: catalog mismatch or reserved layout/root at {entity}"
            ));
        }
        Ok(())
    }
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
        if c.schema != "aic-native-generation-v2"
            || c.identity_limits.max_individualization_nodes == 0
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
            || c.construction.per_connection_route_searches == 0
            || c.construction.analysis_workers == 0
            || c.construction.analysis_workers > 16
            || c.construction.nearby_candidates > 8
            || (c.construction.connection_first_placement
                && (c.construction.flow_direction.is_none()
                    || c.construction.nearby_candidates != 0))
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
            let input = directory.join("input.json");
            let bytes =
                read_generated_bytes(&input).map_err(|e| error("input", input.display(), e))?;
            if manifest["schema"] != "aic-generated-slot-v2"
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
            let mut value: Generated = serde_json::from_slice(&bytes)
                .map_err(|e| error("input", directory.display(), e))?;
            value.admit(
                &self.catalogs,
                &self.config.identity_limits,
                &directory.display().to_string(),
                None,
            )?;
            if value.admitted()?.is_reserved(&self.reserved) {
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
                attempts.push(json!({"seed":seed,"stage":"instance-bound",
                    "instances":request.instances.len(),"limit":self.config.max_facilities}));
                continue;
            }
            let construction = ConstructiveProductionRequest {
                schema_version: aic_data::layouts::CONSTRUCTIVE_PRODUCTION_SCHEMA_VERSION,
                target_instance: target,
                max_steps: self.config.construction.max_steps,
                max_placement_attempts: self.config.construction.max_placement_attempts,
                max_route_searches: self.config.construction.max_route_searches,
                per_connection_route_searches: self
                    .config
                    .construction
                    .per_connection_route_searches,
                analysis_workers: self.config.construction.analysis_workers,
                nearby_candidates: self.config.construction.nearby_candidates,
                downstream_closed_growth: self.config.construction.downstream_closed_growth,
                flow_direction: self.config.construction.flow_direction,
                connection_first_placement: self.config.construction.connection_first_placement,
            };
            let data = &self.catalogs.data;
            let mut report =
                match self.compute(self.config.construction.elapsed_ms.min(remaining()), || {
                    construct_production_compact(
                        &wiring,
                        &data.facilities,
                        &data.items,
                        &data.transports,
                        &data.components,
                        &construction,
                    )
                }) {
                    Ok(value) if value.success && value.complete => value,
                    Ok(value) => {
                        attempts.push(
                            json!({"seed":seed,"product":product.id,"duration_ms":duration_ms,
                        "source":self.identity,"stage":"construction",
                        "outcome":construction_failure_summary(&value)}),
                        );
                        (self.guard)()?;
                        continue;
                    }
                    Err(work_error) => {
                        attempts.push(
                            json!({"seed":seed,"product":product.id,"duration_ms":duration_ms,
                        "source":self.identity,"stage":"construction","outcome":{
                            "status":"interrupted","kind":work_error_kind(&work_error),
                            "error":work_error}}),
                        );
                        if let Err(cancel) = (self.guard)() {
                            write(
                                &directory.join("failure.json"),
                                &json!({"slot":slot,"source":self.identity,"attempts":attempts,
                                "seconds":started.elapsed().as_secs_f64()}),
                            )?;
                            return Err(cancel);
                        }
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
            drop(report);
            let original = WitnessProbeInput {
                schema_version: 1,
                placements: node.placements,
                transport_networks: node.transport_networks,
                logistics_components: node.logistics_components,
                utilizations: serde_json::from_value(utilizations)
                    .map_err(|e| error("constructor-witness", slot, e))?,
            };
            let setup = Setup {
                identity_limits: self.config.identity_limits.clone(),
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
                    if let Ok(mut diagnostic) = serde_json::from_str::<Value>(&failure.message) {
                        if diagnostic["code"] == "aic-learning.physical-identity-budget" {
                            diagnostic["entity"] =
                                json!(format!("{}/setup/{setup_seed}", directory.display()));
                            return Err(diagnostic.to_string());
                        }
                    }
                    return Err(error("setup-failed", directory.display(), &failure.message));
                }
            };
            let admission = Admission::new(
                randomized.physical,
                &self.catalogs,
                format!("{}/{slot}", self.identity),
            );
            if admission.is_reserved(&self.reserved) {
                attempts.push(json!({"seed":seed,"stage":"reservation","structural":admission.physical.structural}));
                continue;
            }
            let metadata = json!({"schema":"aic-generated-input-v2","slot":slot,"generation_seed":seed,"source":self.identity,
                "rng":"splitmix64-v1","draw":draw,
                "constructor_physical":randomized.constructor_physical,"setup":randomized.records,"attempts":attempts,
                "setup_seconds":started.elapsed().as_secs_f64()});
            let value = Generated {
                request,
                original: randomized.original,
                metadata,
                admission: Some(admission),
            };
            let input = directory.join("input.json");
            write(&input, &value)?;
            let input_sha256 = hash_file(&input).map_err(|e| error("input", input.display(), e))?;
            let pending = directory.join("manifest.pending");
            write(
                &pending,
                &json!({"schema":"aic-generated-slot-v2","slot":slot,"source":self.identity,"input_sha256":input_sha256}),
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
