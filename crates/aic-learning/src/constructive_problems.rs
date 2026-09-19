//! Seeded full-production-graph problem preparation without physical layout work.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufWriter, Read, Write},
    path::Path,
    sync::Arc,
};

use aic_data::recipes::{
    FacilityInstanceWiringNode, Recipe, RecipeSource, RecipeSourcePlanRequest,
    RecipeSourceSelection, ThroughputTarget, ValidatedRecipeBook,
    build_contextual_facility_instance_wiring, calculate_contextual_facility_requirements,
    load_recipe_book,
};
use aic_layout::{
    facilities::{FacilityCatalog, ValidatedFacilityCatalog},
    logistics::{
        LogisticsComponentCatalog, TransportCatalog, ValidatedLogisticsComponentCatalog,
        ValidatedTransportCatalog,
    },
    logistics::{TransportKind, ValidatedItemCatalog, load_item_catalog},
    physical::{Block, Connection, Problem, Terminal, TerminalDirection},
    rate::Rate,
    rng::Rng,
    validation::Catalogs,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const PROBLEM_SET_SCHEMA: &str = "aic-constructive-problem-set-v2";
const MAX_INPUT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProblemSet {
    pub schema: String,
    pub sampling_sha256: String,
    pub training: Vec<PreparedProblem>,
    pub evaluation: Vec<PreparedProblem>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PreparedProblem {
    pub id: String,
    pub partition: String,
    pub size_band: String,
    pub target_item: String,
    pub target_rate: Rate,
    pub facility_count: usize,
    pub source_plan: RecipeSourcePlanRequest,
    pub nodes: Vec<NodeProvenance>,
    pub connections: Vec<ConnectionProvenance>,
    pub problem: Problem,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NodeProvenance {
    pub physical_id: String,
    pub wiring_node: String,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConnectionProvenance {
    pub physical_id: String,
    pub wiring_edge: String,
    pub lane: usize,
    pub lanes: usize,
}

#[derive(Clone, Debug, Deserialize)]
struct SamplingDocument {
    sampling: SamplingPolicy,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SamplingPolicy {
    schema_version: u32,
    partition_salt: String,
    partition_buckets: PartitionBuckets,
    target_loads: Vec<TargetLoad>,
    procurement_pool: String,
    procurement_probability: f64,
    source_selection_scope: String,
    max_source_nodes: usize,
    max_depth: usize,
    max_facilities: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PartitionBuckets {
    train: u64,
    validation: u64,
    test: u64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TargetLoad {
    numerator: i64,
    denominator: i64,
}

#[derive(Default, Serialize)]
struct SamplingStats {
    attempts: usize,
    source_failures: usize,
    flow_failures: usize,
    facility_guard_rejections: usize,
    duplicate_graphs: usize,
    accepted_candidates: usize,
}

struct Candidate {
    prepared: PreparedProblem,
    signature: String,
}

pub fn prepare(
    sampling_path: &Path,
    catalog_root: &Path,
    output: &Path,
    seed: u64,
    training: usize,
    evaluation: usize,
) -> Result<Value> {
    ensure!(
        training > 0 && evaluation > 0 && !output.exists(),
        "aic-learning.constructive-problems: invalid counts or existing output {}",
        output.display()
    );
    let document: SamplingDocument = read_json(sampling_path)?;
    let policy = document.sampling;
    validate_policy(&policy)?;

    let recipe_path = catalog_root.join("recipes.json");
    let item_path = catalog_root.join("items.json");
    let recipes = ValidatedRecipeBook::try_from_recipe_book(load_recipe_book(&recipe_path)?)
        .map_err(|report| anyhow::anyhow!("aic-learning.constructive-recipes: {report:?}"))?;
    let items = ValidatedItemCatalog::try_from_catalog(load_item_catalog(&item_path)?)
        .map_err(|report| anyhow::anyhow!("aic-learning.constructive-items: {report:?}"))?;
    let catalogs = load_catalogs(catalog_root)?;
    validate_catalog_references(
        recipes.recipe_book().recipes.as_slice(),
        &items,
        &catalogs.facilities,
    )?;
    let producers = producer_index(recipes.recipe_book().recipes.as_slice());

    let mut training_stats = SamplingStats::default();
    let mut evaluation_stats = SamplingStats::default();
    let training_problems = prepare_partition(
        "train",
        training,
        seed,
        &policy,
        &recipes,
        &items,
        &catalogs.transports,
        &producers,
        &mut training_stats,
    )?;
    let evaluation_problems = prepare_partition(
        "validation",
        evaluation,
        seed ^ 0xa076_1d64_78bd_642f,
        &policy,
        &recipes,
        &items,
        &catalogs.transports,
        &producers,
        &mut evaluation_stats,
    )?;

    let problem_set = ProblemSet {
        schema: PROBLEM_SET_SCHEMA.into(),
        sampling_sha256: sha256(sampling_path)?,
        training: training_problems,
        evaluation: evaluation_problems,
    };
    write_json(output, &problem_set)?;

    Ok(json!({
        "schema":"aic-constructive-problem-preparation-report-v2",
        "status":"completed",
        "seed":seed,
        "training_problems":problem_set.training.len(),
        "evaluation_problems":problem_set.evaluation.len(),
        "training_size_bands":band_counts(&problem_set.training),
        "evaluation_size_bands":band_counts(&problem_set.evaluation),
        "training_sampling":training_stats,
        "evaluation_sampling":evaluation_stats,
        "recipes_sha256":sha256(&recipe_path)?,
        "items_sha256":sha256(&item_path)?,
        "sampling_sha256":problem_set.sampling_sha256,
        "output_sha256":sha256(output)?,
        "output":output,
    }))
}

fn validate_policy(policy: &SamplingPolicy) -> Result<()> {
    let buckets = policy
        .partition_buckets
        .train
        .checked_add(policy.partition_buckets.validation)
        .and_then(|sum| sum.checked_add(policy.partition_buckets.test));
    ensure!(
        policy.schema_version == 2
            && !policy.partition_salt.is_empty()
            && buckets.is_some_and(|sum| sum > 0)
            && policy.partition_buckets.train > 0
            && policy.partition_buckets.validation > 0
            && policy.partition_buckets.test > 0
            && !policy.target_loads.is_empty()
            && policy
                .target_loads
                .iter()
                .all(|load| load.numerator > 0 && load.denominator > 0)
            && matches!(
                policy.procurement_pool.as_str(),
                "all-catalog-inputs" | "catalog-external-items"
            )
            && policy.procurement_probability.is_finite()
            && (0.0..=1.0).contains(&policy.procurement_probability)
            && matches!(
                policy.source_selection_scope.as_str(),
                "item" | "occurrence"
            )
            && (1..=4096).contains(&policy.max_source_nodes)
            && (1..=128).contains(&policy.max_depth)
            && policy.max_facilities > 0,
        "aic-learning.constructive-sampling: invalid sampling policy"
    );
    Ok(())
}

fn validate_catalog_references(
    recipes: &[Recipe],
    items: &ValidatedItemCatalog,
    facilities: &aic_layout::facilities::ValidatedFacilityCatalog,
) -> Result<()> {
    for recipe in recipes {
        ensure!(
            facilities.facility(&recipe.facility).is_some(),
            "aic-learning.constructive-catalog: recipe {} uses missing facility {}",
            recipe.id,
            recipe.facility
        );
        for amount in recipe.inputs.iter().chain(&recipe.outputs) {
            ensure!(
                items.item(&amount.item).is_some(),
                "aic-learning.constructive-catalog: recipe {} uses missing item {}",
                recipe.id,
                amount.item
            );
        }
    }
    Ok(())
}

fn producer_index(recipes: &[Recipe]) -> BTreeMap<String, Vec<usize>> {
    let mut producers = BTreeMap::<String, Vec<usize>>::new();
    for (index, recipe) in recipes.iter().enumerate() {
        for item in recipe
            .outputs
            .iter()
            .map(|output| output.item.as_str())
            .collect::<BTreeSet<_>>()
        {
            producers.entry(item.into()).or_default().push(index);
        }
    }
    for indices in producers.values_mut() {
        indices.sort_by_key(|&index| recipes[index].id.as_str());
    }
    producers
}

#[allow(clippy::too_many_arguments)]
fn prepare_partition(
    partition: &str,
    count: usize,
    seed: u64,
    policy: &SamplingPolicy,
    book: &ValidatedRecipeBook,
    items: &ValidatedItemCatalog,
    transports: &aic_layout::logistics::ValidatedTransportCatalog,
    producers: &BTreeMap<String, Vec<usize>>,
    stats: &mut SamplingStats,
) -> Result<Vec<PreparedProblem>> {
    let targets = producers
        .keys()
        .filter(|item| partition_for(item, policy) == partition)
        .cloned()
        .collect::<Vec<_>>();
    ensure!(
        !targets.is_empty(),
        "aic-learning.constructive-problems: no target item belongs to partition {partition}"
    );
    let attempt_limit = count
        .max(16)
        .checked_mul(128)
        .context("aic-learning.constructive-problems: attempt limit overflow")?;
    let mut rng = Rng::new(seed);
    let mut seen = BTreeSet::new();
    let mut bands = BTreeMap::<String, Vec<Candidate>>::new();
    for ordinal in 0..attempt_limit {
        stats.attempts += 1;
        let target = &targets[rng.index(targets.len())];
        let sink_options = &producers[target];
        let sink = sink_options[rng.index(sink_options.len())];
        let load = policy.target_loads[rng.index(policy.target_loads.len())];
        let Some(source_plan) = sample_source_plan(
            target,
            sink,
            load,
            policy,
            book.recipe_book().recipes.as_slice(),
            producers,
            book.recipe_book().external_items.as_slice(),
            &mut rng,
        ) else {
            stats.source_failures += 1;
            continue;
        };
        let throughput = book.calculate_contextual_throughput(&source_plan);
        if !throughput.success {
            stats.flow_failures += 1;
            continue;
        }
        let requirements = calculate_contextual_facility_requirements(&throughput);
        if !requirements.success {
            stats.flow_failures += 1;
            continue;
        }
        let wiring = build_contextual_facility_instance_wiring(&throughput, &requirements);
        if !wiring.success {
            stats.flow_failures += 1;
            continue;
        }
        let facility_count = wiring
            .nodes
            .iter()
            .filter(|node| matches!(node, FacilityInstanceWiringNode::Facility { .. }))
            .count();
        if facility_count == 0 || facility_count > policy.max_facilities {
            stats.facility_guard_rejections += 1;
            continue;
        }
        let target_rate = throughput
            .target
            .as_ref()
            .expect("successful throughput has target")
            .rate;
        let id = format!("{partition}-{ordinal}");
        let prepared = compile_problem(
            id,
            partition,
            target,
            target_rate,
            source_plan,
            wiring,
            items,
            transports,
        )?;
        let signature = problem_signature(&prepared.problem)?;
        if !seen.insert(signature.clone()) {
            stats.duplicate_graphs += 1;
            continue;
        }
        stats.accepted_candidates += 1;
        bands
            .entry(prepared.size_band.clone())
            .or_default()
            .push(Candidate {
                prepared,
                signature,
            });
    }
    let available = bands.values().map(Vec::len).sum::<usize>();
    ensure!(
        available >= count,
        "aic-learning.constructive-problems: partition {partition} produced only {available} unique full graphs for {count} requested problems after {attempt_limit} attempts"
    );
    for candidates in bands.values_mut() {
        rng.shuffle(candidates);
    }
    let mut selected = Vec::with_capacity(count);
    while selected.len() < count {
        let mut progressed = false;
        for candidates in bands.values_mut() {
            if let Some(candidate) = candidates.pop() {
                debug_assert!(!candidate.signature.is_empty());
                selected.push(candidate.prepared);
                progressed = true;
                if selected.len() == count {
                    break;
                }
            }
        }
        ensure!(
            progressed,
            "aic-learning.constructive-problems: balanced selection stalled"
        );
    }
    Ok(selected)
}

#[allow(clippy::too_many_arguments)]
fn sample_source_plan(
    target: &str,
    sink: usize,
    load: TargetLoad,
    policy: &SamplingPolicy,
    recipes: &[Recipe],
    producers: &BTreeMap<String, Vec<usize>>,
    external_items: &[String],
    rng: &mut Rng,
) -> Option<RecipeSourcePlanRequest> {
    let quantity = recipes[sink]
        .outputs
        .iter()
        .filter(|output| output.item == target)
        .try_fold(0i64, |sum, output| sum.checked_add(output.quantity))?
        .checked_mul(load.numerator)?;
    let duration_ms = recipes[sink].duration_ms.checked_mul(load.denominator)?;
    let mut sampler = SourceSampler {
        policy,
        recipes,
        producers,
        external_items: external_items.iter().map(String::as_str).collect(),
        rng,
        selections: Vec::new(),
        selected_by_item: BTreeMap::new(),
        visited: 0,
    };
    let ancestors = BTreeMap::new();
    sampler.visit(target, "/target", &ancestors, 0, Some(sink))?;
    Some(RecipeSourcePlanRequest {
        schema_version: 1,
        target: ThroughputTarget {
            item: target.into(),
            quantity,
            duration_ms,
        },
        source_selections: sampler.selections,
    })
}

struct SourceSampler<'a> {
    policy: &'a SamplingPolicy,
    recipes: &'a [Recipe],
    producers: &'a BTreeMap<String, Vec<usize>>,
    external_items: BTreeSet<&'a str>,
    rng: &'a mut Rng,
    selections: Vec<RecipeSourceSelection>,
    selected_by_item: BTreeMap<String, Option<usize>>,
    visited: usize,
}

impl SourceSampler<'_> {
    fn visit(
        &mut self,
        item: &str,
        path: &str,
        ancestors: &BTreeMap<String, String>,
        depth: usize,
        forced: Option<usize>,
    ) -> Option<()> {
        if self.visited >= self.policy.max_source_nodes || depth > self.policy.max_depth {
            return None;
        }
        self.visited += 1;
        let options = self.producers.get(item).map(Vec::as_slice).unwrap_or(&[]);
        let known = (self.policy.source_selection_scope == "item")
            .then(|| self.selected_by_item.get(item).copied())
            .flatten();
        let may_procure = self.policy.procurement_pool == "all-catalog-inputs"
            || self.external_items.contains(item);
        let procure = forced.is_none()
            && match known {
                Some(selected) => selected.is_none(),
                None => {
                    may_procure
                        && (options.is_empty()
                            || (self.rng.next_u64() as f64 / u64::MAX as f64)
                                < self.policy.procurement_probability)
                }
            };
        if forced.is_none() && options.is_empty() && !procure {
            return None;
        }
        let selected = if procure {
            None
        } else if let Some(forced) = forced {
            Some(forced)
        } else if let Some(known) = known {
            known
        } else {
            Some(options[self.rng.index(options.len())])
        };
        if self.policy.source_selection_scope == "item" {
            self.selected_by_item.insert(item.into(), selected);
        }
        self.selections.push(RecipeSourceSelection {
            path: path.into(),
            source: selected.map_or(RecipeSource::ExternalInput, |recipe| RecipeSource::Recipe {
                recipe: self.recipes[recipe].id.clone(),
            }),
        });
        let Some(selected) = selected else {
            return Some(());
        };
        let recipe = &self.recipes[selected];
        if ancestors.contains_key(&recipe.id) {
            return Some(());
        }
        let mut next = ancestors.clone();
        next.insert(recipe.id.clone(), path.into());
        for input in &recipe.inputs {
            let child = format!("{path}/recipe:{}/input:{}", recipe.id, input.item);
            self.visit(&input.item, &child, &next, depth + 1, None)?;
        }
        Some(())
    }
}

fn partition_for(item: &str, policy: &SamplingPolicy) -> &'static str {
    let total = policy.partition_buckets.train
        + policy.partition_buckets.validation
        + policy.partition_buckets.test;
    let mut digest = Sha256::new();
    digest.update(
        serde_json::to_vec(&(&policy.partition_salt, item))
            .expect("partition identity is always serializable"),
    );
    let hash = digest.finalize();
    let bucket = hash.iter().fold(0u64, |remainder, byte| {
        (remainder * 256 + u64::from(*byte)) % total
    });
    if bucket < policy.partition_buckets.train {
        "train"
    } else if bucket < policy.partition_buckets.train + policy.partition_buckets.validation {
        "validation"
    } else {
        "test"
    }
}

#[allow(clippy::too_many_arguments)]
fn compile_problem(
    id: String,
    partition: &str,
    target_item: &str,
    target_rate: Rate,
    source_plan: RecipeSourcePlanRequest,
    wiring: aic_data::recipes::FacilityInstanceWiringReport,
    items: &ValidatedItemCatalog,
    transports: &aic_layout::logistics::ValidatedTransportCatalog,
) -> Result<PreparedProblem> {
    let mut blocks = Vec::new();
    let mut endpoint_ids = BTreeMap::<String, String>::new();
    let mut node_kinds = BTreeMap::<String, &'static str>::new();
    let mut nodes = Vec::new();
    for node in &wiring.nodes {
        match node {
            FacilityInstanceWiringNode::Facility { id, facility, .. } => {
                let physical_id = format!("block-{}", blocks.len());
                endpoint_ids.insert(id.clone(), physical_id.clone());
                node_kinds.insert(id.clone(), "facility");
                blocks.push(Block {
                    id: physical_id.clone(),
                    facility: facility.clone(),
                });
                nodes.push(NodeProvenance {
                    physical_id,
                    wiring_node: id.clone(),
                    kind: "facility".into(),
                });
            }
            FacilityInstanceWiringNode::External { id, .. } => {
                node_kinds.insert(id.clone(), "external");
            }
            FacilityInstanceWiringNode::Target { id, .. } => {
                node_kinds.insert(id.clone(), "target");
            }
            FacilityInstanceWiringNode::Surplus { id, .. } => {
                node_kinds.insert(id.clone(), "surplus");
            }
        }
    }
    let facility_count = blocks.len();
    let mut terminals = Vec::new();
    let mut logical_connections = Vec::new();
    let mut connections = Vec::new();
    for edge in &wiring.edges {
        let item = items.item(&edge.item).with_context(|| {
            format!(
                "aic-learning.constructive-problems: wiring uses missing item {}",
                edge.item
            )
        })?;
        let lanes = split_lanes(edge.rate, item.transport, transports)?;
        for (lane, rate) in lanes.iter().copied().enumerate() {
            let source = if let Some(block) = endpoint_ids.get(&edge.source) {
                block.clone()
            } else {
                let terminal = format!("terminal-{}", terminals.len());
                ensure!(
                    node_kinds.get(&edge.source) == Some(&"external"),
                    "aic-learning.constructive-problems: non-facility source is not external"
                );
                terminals.push(Terminal {
                    id: terminal.clone(),
                    direction: TerminalDirection::Input,
                    commodity: edge.item.clone(),
                    transport: item.transport,
                });
                nodes.push(NodeProvenance {
                    physical_id: terminal.clone(),
                    wiring_node: edge.source.clone(),
                    kind: "external".into(),
                });
                terminal
            };
            let target = if let Some(block) = endpoint_ids.get(&edge.target) {
                block.clone()
            } else {
                let terminal = format!("terminal-{}", terminals.len());
                ensure!(
                    matches!(node_kinds.get(&edge.target), Some(&"target" | &"surplus")),
                    "aic-learning.constructive-problems: non-facility target is not output"
                );
                terminals.push(Terminal {
                    id: terminal.clone(),
                    direction: TerminalDirection::Output,
                    commodity: edge.item.clone(),
                    transport: item.transport,
                });
                nodes.push(NodeProvenance {
                    physical_id: terminal.clone(),
                    wiring_node: edge.target.clone(),
                    kind: node_kinds[&edge.target].into(),
                });
                terminal
            };
            let physical_id = format!("connection-{}", logical_connections.len());
            logical_connections.push(Connection {
                id: physical_id.clone(),
                source,
                target,
                commodity: edge.item.clone(),
                transport: item.transport,
                rate,
            });
            connections.push(ConnectionProvenance {
                physical_id,
                wiring_edge: edge.id.clone(),
                lane,
                lanes: lanes.len(),
            });
        }
    }
    ensure!(
        !logical_connections.is_empty(),
        "aic-learning.constructive-problems: full production graph has no material connections"
    );
    Ok(PreparedProblem {
        id,
        partition: partition.into(),
        size_band: size_band(facility_count),
        target_item: target_item.into(),
        target_rate,
        facility_count,
        source_plan,
        nodes,
        connections,
        problem: Problem {
            blocks,
            terminals,
            connections: logical_connections,
        },
    })
}

fn split_lanes(
    rate: Rate,
    transport: TransportKind,
    transports: &aic_layout::logistics::ValidatedTransportCatalog,
) -> Result<Vec<Rate>> {
    let capacity = transports.capacity(transport);
    let capacity = Rate::from_quantity_per_duration_ms(capacity.quantity, capacity.duration_ms)
        .map_err(|diagnostic| anyhow::anyhow!(diagnostic.message))?;
    let mut remaining = rate;
    let mut lanes = Vec::new();
    while !remaining.is_zero() {
        let lane = remaining.min(capacity);
        lanes.push(lane);
        remaining = remaining
            .checked_sub(lane)
            .map_err(|diagnostic| anyhow::anyhow!(diagnostic.message))?;
    }
    ensure!(
        !lanes.is_empty(),
        "aic-learning.constructive-problems: zero-rate wiring edge"
    );
    Ok(lanes)
}

fn size_band(facilities: usize) -> String {
    let upper = facilities.next_power_of_two().max(2);
    let lower = if upper == 2 { 1 } else { upper / 2 + 1 };
    format!("{lower}-{upper}")
}

fn band_counts(problems: &[PreparedProblem]) -> BTreeMap<&str, usize> {
    let mut counts = BTreeMap::new();
    for problem in problems {
        *counts.entry(problem.size_band.as_str()).or_default() += 1;
    }
    counts
}

fn problem_signature(problem: &Problem) -> Result<String> {
    let encoded = serde_json::to_vec(problem)?;
    let digest = Sha256::digest(encoded);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn load_catalogs(root: &Path) -> Result<Arc<Catalogs>> {
    let facilities: FacilityCatalog = read_json(&root.join("facilities.json"))?;
    let transports: TransportCatalog = read_json(&root.join("transports.json"))?;
    let components: LogisticsComponentCatalog = read_json(&root.join("logistics-components.json"))?;
    let identity_bytes = serde_json::to_vec(&(&facilities, &transports, &components))?;
    let identity = Sha256::digest(identity_bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok(Arc::new(Catalogs {
        identity,
        facilities: ValidatedFacilityCatalog::try_from_catalog(facilities)
            .map_err(|report| anyhow::anyhow!("aic-learning.constructive-catalog: {report:?}"))?,
        transports: ValidatedTransportCatalog::try_from_catalog(transports)
            .map_err(|report| anyhow::anyhow!("aic-learning.constructive-catalog: {report:?}"))?,
        components: ValidatedLogisticsComponentCatalog::try_from_catalog(components)
            .map_err(|report| anyhow::anyhow!("aic-learning.constructive-catalog: {report:?}"))?,
    }))
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    ensure!(
        file.metadata()?.len() <= MAX_INPUT_BYTES,
        "aic-learning.constructive-input: {} exceeds 16 MiB",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(MAX_INPUT_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= MAX_INPUT_BYTES,
        "aic-learning.constructive-input: {} exceeds 16 MiB",
        path.display()
    );
    serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    serde_json::to_writer_pretty(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observed_size_bands_are_open_ended_powers_of_two() {
        assert_eq!(size_band(1), "1-2");
        assert_eq!(size_band(2), "1-2");
        assert_eq!(size_band(3), "3-4");
        assert_eq!(size_band(59), "33-64");
        assert_eq!(size_band(100), "65-128");
        assert_eq!(size_band(257), "257-512");
    }
}
