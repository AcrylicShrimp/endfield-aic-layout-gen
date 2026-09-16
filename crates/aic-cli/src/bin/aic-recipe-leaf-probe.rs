//! Research-only recipe source-tree dynamic-programming probe.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use aic_data::recipes::{
    FacilityInstanceWiringNode, FacilityInstanceWiringReport, Rate, Recipe, RecipeSource,
    RecipeSourceNode, RecipeSourcePlanRequest, RecipeSourceSelection, ThroughputTarget,
    ValidatedRecipeBook, build_contextual_facility_instance_wiring,
    calculate_contextual_facility_requirements, check_recipe_source_plan, load_recipe_book,
};
use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::{Bounds, Point},
    logistics::{ValidatedItemCatalog, load_item_catalog},
    physical::Block,
    projection::{Port, edge_index, rotate},
    validation::Catalogs,
};
use aic_layout_gen::{
    io::load_catalogs,
    placement::{self, DensityBand, DensityPlacementRequest, Placement, PlacementRequest},
};
use aic_routing::{
    GlobalRouteJob, GlobalRouteTerminal, GlobalRoutingComparison, GlobalRoutingConfig,
    GlobalRoutingProblem, compare_global_routing,
};
use anyhow::{Context, Result, anyhow, ensure};
use clap::Parser;
use serde::Serialize;

const RATE_HORIZON_MS: i64 = 20_000;
const MAX_DP_MEMO_STATES: usize = 50_000;
const MAX_DP_COMBINATIONS: usize = 2_000_000;

#[derive(Parser)]
#[command(about = "Research only: combine precomputed recipe source subtrees by facility count")]
struct Args {
    #[arg(long)]
    recipes: PathBuf,
    #[arg(long)]
    target_item: String,
    #[arg(long)]
    facilities: i64,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long, default_value_t = 16)]
    attempts: usize,
    #[arg(long, default_value_t = 64)]
    max_steps: usize,
    #[arg(long, default_value_t = 4)]
    max_depth: usize,
    #[arg(long)]
    report_output: PathBuf,
    /// Optional normalized catalog directory for one frozen-block placement sample.
    #[arg(long)]
    catalog_dir: Option<PathBuf>,
    #[arg(long, default_value_t = 0)]
    placement_gap_min: i64,
    #[arg(long, default_value_t = 4)]
    placement_gap_max: i64,
    /// Compare irreversible sequential guides with negotiated congestion on the frozen placement.
    #[arg(long, requires = "catalog_dir")]
    compare_routing: bool,
    #[arg(long, default_value_t = 24)]
    routing_iterations: usize,
    #[arg(long, default_value_t = 100_000)]
    routing_work_per_route: usize,
    /// Research-only achieved-density band lower bound (inclusive).
    #[arg(long, requires = "placement_density_max")]
    placement_density_min: Option<f64>,
    /// Research-only achieved-density band upper bound (exclusive).
    #[arg(long, requires = "placement_density_min")]
    placement_density_max: Option<f64>,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    status: &'static str,
    target_item: String,
    requested_facilities: i64,
    achieved_facilities: i64,
    absolute_error: i64,
    seed: u64,
    elapsed_us: u128,
    exact_plans_evaluated: usize,
    attempted_starts: usize,
    dp_memo_states: usize,
    dp_cache_hits: usize,
    dp_combinations: usize,
    dp_recipe_options: usize,
    cycle_choices_deferred: usize,
    dp_truncated: bool,
    max_depth: usize,
    target_quantity: i64,
    target_duration_ms: i64,
    active_recipe_occurrences: usize,
    facility_instances: usize,
    external_source_occurrences: usize,
    distinct_external_items: usize,
    catalog_produced_external_sources: usize,
    external_input_edges: usize,
    total_wiring_edges: usize,
    placement: Option<PlacementObservation>,
    routing: Option<RoutingObservation>,
    source_selections: Vec<OutputSelection>,
    moves: Vec<MoveRecord>,
}

#[derive(Clone, Serialize)]
struct PlacementObservation {
    semantic_facilities: Vec<SemanticFacilityBlock>,
    sequence_pair: [Vec<usize>; 2],
    body_width: i64,
    body_height: i64,
    body_cells: u64,
    body_bounding_area: u64,
    body_density: f64,
    gap: [i64; 2],
    canvas_width: i64,
    canvas_height: i64,
}

#[derive(Serialize)]
struct RoutingObservation {
    status: &'static str,
    guide_only: bool,
    shared_network_legalization_performed: bool,
    exact_component_legalization_performed: bool,
    frozen_obligations: usize,
    internal_obligations: usize,
    external_obligations: usize,
    boundary_only_obligations: usize,
    jobs: Vec<RoutingJobObservation>,
    diagnostic: Option<String>,
    comparison: Option<GlobalRoutingComparison>,
}

#[derive(Serialize)]
struct RoutingJobObservation {
    id: String,
    source: String,
    target: String,
    commodity: String,
    transport: aic_layout::logistics::TransportKind,
    rate: Rate,
    source_candidates: usize,
    target_candidates: usize,
}

struct PhysicalPlacement {
    observation: PlacementObservation,
    sampled: Placement,
    catalogs: Catalogs,
}

#[derive(Clone, Serialize)]
struct SemanticFacilityBlock {
    semantic_node: String,
    block: String,
    facility: String,
}

#[derive(Serialize)]
struct OutputSelection {
    path: String,
    source: RecipeSource,
}

#[derive(Clone, Serialize)]
struct MoveRecord {
    kind: &'static str,
    path: String,
    recipe: Option<String>,
    before_facilities: i64,
    after_facilities: i64,
}

#[derive(Clone)]
struct Plan {
    target_quantity: i64,
    selections: BTreeMap<String, RecipeSource>,
}

#[derive(Clone)]
struct Evaluation {
    facilities: i64,
    active_recipes: usize,
}

struct FinalObservations {
    facility_instances: usize,
    external_source_occurrences: usize,
    distinct_external_items: usize,
    catalog_produced_external_sources: usize,
    external_input_edges: usize,
    total_wiring_edges: usize,
    facility_blocks: Vec<(String, Block)>,
    wiring: FacilityInstanceWiringReport,
}

#[derive(Clone)]
struct SearchResult {
    plan: Plan,
    evaluation: Evaluation,
    moves: Vec<MoveRecord>,
    tie: u64,
}

#[derive(Clone)]
enum Blueprint {
    External,
    Recipe {
        recipe: String,
        own_facilities: i64,
        children: Vec<Arc<Blueprint>>,
    },
}

#[derive(Clone)]
struct SubtreeOption {
    facilities: i64,
    active_recipes: usize,
    hash: u64,
    blueprint: Arc<Blueprint>,
}

#[derive(Clone)]
struct PartialOption {
    facilities: i64,
    active_recipes: usize,
    hash: u64,
    children: Vec<Arc<Blueprint>>,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct MemoKey {
    item: String,
    demand: Rate,
    facility_budget: i64,
    depth_left: usize,
    ancestors: Vec<u64>,
    ancestor_items: Vec<u64>,
}

#[derive(Default)]
struct DpStats {
    cache_hits: usize,
    combinations: usize,
    recipe_options: usize,
    cycle_choices_deferred: usize,
    truncated: bool,
}

struct DpContext<'a> {
    recipes: &'a BTreeMap<String, Recipe>,
    producers: &'a BTreeMap<String, Vec<String>>,
    recipe_indices: BTreeMap<String, usize>,
    item_indices: BTreeMap<String, usize>,
    facility_limit: i64,
    max_active_recipes: usize,
    seed: u64,
    memo: BTreeMap<MemoKey, Vec<SubtreeOption>>,
    stats: DpStats,
}

#[derive(Clone)]
struct ExternalLeaf {
    path: String,
    item: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(args.facilities > 0, "--facilities must be positive");
    ensure!(args.attempts > 0, "--attempts must be positive");
    ensure!(args.max_steps > 0, "--max-steps must be positive");
    ensure!(args.max_depth > 0, "--max-depth must be positive");
    ensure!(
        (1..=1_024).contains(&args.routing_iterations),
        "--routing-iterations must be 1..=1024"
    );
    ensure!(
        (1..=10_000_000).contains(&args.routing_work_per_route),
        "--routing-work-per-route must be 1..=10000000"
    );
    ensure!(
        !args.report_output.exists(),
        "report output already exists: {}",
        args.report_output.display()
    );
    let raw = load_recipe_book(&args.recipes)
        .with_context(|| format!("loading {}", args.recipes.display()))?;
    let book = ValidatedRecipeBook::try_from_recipe_book(raw)
        .map_err(|report| anyhow!("recipe catalog is invalid: {report:?}"))?;
    let producers = producer_index(&book);
    let recipes = recipe_index(&book);
    let root_producers = producers
        .get(&args.target_item)
        .cloned()
        .unwrap_or_default();
    ensure!(
        !root_producers.is_empty(),
        "target item '{}' has no producer",
        args.target_item
    );

    let started = Instant::now();
    let max_active_recipes = args
        .max_steps
        .checked_add(1)
        .context("--max-steps is too large")?;
    let mut dp = DpContext::new(
        &recipes,
        &producers,
        args.facilities,
        max_active_recipes,
        args.seed,
    );
    let starts = start_schedule(&root_producers, args.facilities, args.seed, args.attempts)?;
    let mut best: Option<SearchResult> = None;
    let mut attempted_starts = 0_usize;
    for (root_recipe, root_facilities) in starts {
        attempted_starts += 1;
        let recipe = &recipes[&root_recipe];
        let target_quantity = target_quantity_for_root(recipe, &args.target_item, root_facilities)?;
        for option in dp.root_options(recipe, &args.target_item, target_quantity, args.max_depth)? {
            let candidate = materialize_result(target_quantity, &option, &recipes)?;
            if better_result(&candidate, best.as_ref(), args.facilities) {
                best = Some(candidate);
            }
        }
        if best
            .as_ref()
            .is_some_and(|result| result.evaluation.facilities == args.facilities)
        {
            break;
        }
    }
    let mut best = best.context("no valid acyclic source plan was constructed")?;
    let mut exact_plans_evaluated = 0_usize;
    if best.evaluation.facilities != args.facilities {
        best = refine_with_cycle_closures(
            &book,
            &recipes,
            &producers,
            &args.target_item,
            args.facilities,
            best,
            &mut exact_plans_evaluated,
        )?;
    }
    let observations = validate_final_plan(
        &book,
        &producers,
        &args.target_item,
        &best.plan,
        &best.evaluation,
    )?;
    let physical = args
        .catalog_dir
        .as_deref()
        .map(|catalog_dir| place_facilities(catalog_dir, &args, &observations.facility_blocks))
        .transpose()?;
    let routing = if args.compare_routing {
        let physical = physical
            .as_ref()
            .expect("clap requires a catalog directory for routing comparison");
        Some(
            compare_recipe_routing(
                args.catalog_dir.as_deref().unwrap(),
                &args,
                &observations,
                physical,
            )
            .unwrap_or_else(|error| {
                let diagnostic = format!("{error:#}");
                RoutingObservation {
                    status: "invalid-input",
                    guide_only: true,
                    shared_network_legalization_performed: false,
                    exact_component_legalization_performed: false,
                    frozen_obligations: observations.wiring.edges.len(),
                    internal_obligations: 0,
                    external_obligations: 0,
                    boundary_only_obligations: 0,
                    jobs: vec![],
                    diagnostic: Some(diagnostic),
                    comparison: None,
                }
            }),
        )
    } else {
        None
    };
    let placement = physical.as_ref().map(|value| value.observation.clone());
    exact_plans_evaluated += 1;
    let achieved = best.evaluation.facilities;
    let report = Report {
        schema_version: 5,
        status: if achieved == args.facilities {
            "exact"
        } else {
            "closest"
        },
        target_item: args.target_item,
        requested_facilities: args.facilities,
        achieved_facilities: achieved,
        absolute_error: i64::try_from(achieved.abs_diff(args.facilities))?,
        seed: args.seed,
        elapsed_us: started.elapsed().as_micros(),
        exact_plans_evaluated,
        attempted_starts,
        dp_memo_states: dp.memo.len(),
        dp_cache_hits: dp.stats.cache_hits,
        dp_combinations: dp.stats.combinations,
        dp_recipe_options: dp.stats.recipe_options,
        cycle_choices_deferred: dp.stats.cycle_choices_deferred,
        dp_truncated: dp.stats.truncated,
        max_depth: args.max_depth,
        target_quantity: best.plan.target_quantity,
        target_duration_ms: RATE_HORIZON_MS,
        active_recipe_occurrences: best.evaluation.active_recipes,
        facility_instances: observations.facility_instances,
        external_source_occurrences: observations.external_source_occurrences,
        distinct_external_items: observations.distinct_external_items,
        catalog_produced_external_sources: observations.catalog_produced_external_sources,
        external_input_edges: observations.external_input_edges,
        total_wiring_edges: observations.total_wiring_edges,
        placement,
        routing,
        source_selections: best
            .plan
            .selections
            .into_iter()
            .map(|(path, source)| OutputSelection { path, source })
            .collect(),
        moves: best.moves,
    };
    std::fs::write(&args.report_output, serde_json::to_vec_pretty(&report)?)
        .with_context(|| format!("writing {}", args.report_output.display()))?;
    println!(
        "status={} requested={} achieved={} external_sources={} external_edges={} exact_evaluations={} dp_states={} elapsed_us={}",
        report.status,
        report.requested_facilities,
        report.achieved_facilities,
        report.external_source_occurrences,
        report.external_input_edges,
        report.exact_plans_evaluated,
        report.dp_memo_states,
        report.elapsed_us
    );
    Ok(())
}

impl<'a> DpContext<'a> {
    fn new(
        recipes: &'a BTreeMap<String, Recipe>,
        producers: &'a BTreeMap<String, Vec<String>>,
        facility_limit: i64,
        max_active_recipes: usize,
        seed: u64,
    ) -> Self {
        let recipe_indices = recipes
            .keys()
            .enumerate()
            .map(|(index, recipe)| (recipe.clone(), index))
            .collect();
        let item_indices = recipes
            .values()
            .flat_map(|recipe| recipe.inputs.iter().chain(&recipe.outputs))
            .map(|amount| amount.item.clone())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .enumerate()
            .map(|(index, item)| (item, index))
            .collect();
        Self {
            recipes,
            producers,
            recipe_indices,
            item_indices,
            facility_limit,
            max_active_recipes,
            seed,
            memo: BTreeMap::new(),
            stats: DpStats::default(),
        }
    }

    fn root_options(
        &mut self,
        recipe: &Recipe,
        target_item: &str,
        target_quantity: i64,
        depth_left: usize,
    ) -> Result<Vec<SubtreeOption>> {
        let target_rate = Rate::from_quantity_per_duration_ms(target_quantity, RATE_HORIZON_MS)
            .map_err(|error| anyhow!("target rate failed: {error:?}"))?;
        let (run_rate, own_facilities) = producer_requirement(recipe, target_item, target_rate)?;
        if own_facilities > self.facility_limit {
            return Ok(Vec::new());
        }
        let mut ancestors = empty_recipe_bits(self.recipes.len());
        set_recipe_bit(&mut ancestors, self.recipe_indices[recipe.id.as_str()]);
        let mut ancestor_items = empty_recipe_bits(self.item_indices.len());
        set_recipe_bit(&mut ancestor_items, self.item_indices[target_item]);
        let mut partials = vec![PartialOption {
            facilities: own_facilities,
            active_recipes: 1,
            hash: hash_recipe_choice(&recipe.id, own_facilities),
            children: Vec::new(),
        }];
        for input in &recipe.inputs {
            let demand = run_rate
                .checked_mul_i64(input.quantity)
                .map_err(|error| anyhow!("root input rate failed: {error:?}"))?;
            let child_options = self.subtree_options(
                &input.item,
                demand,
                self.facility_limit - own_facilities,
                depth_left,
                &ancestors,
                &ancestor_items,
            )?;
            partials = self.combine_partials(partials, &child_options, self.facility_limit);
            if partials.is_empty() {
                break;
            }
        }
        Ok(partials
            .into_iter()
            .map(|partial| SubtreeOption {
                facilities: partial.facilities,
                active_recipes: partial.active_recipes,
                hash: partial.hash,
                blueprint: Arc::new(Blueprint::Recipe {
                    recipe: recipe.id.clone(),
                    own_facilities,
                    children: partial.children,
                }),
            })
            .collect())
    }

    fn subtree_options(
        &mut self,
        item: &str,
        demand: Rate,
        facility_budget: i64,
        depth_left: usize,
        ancestors: &[u64],
        ancestor_items: &[u64],
    ) -> Result<Vec<SubtreeOption>> {
        let key = MemoKey {
            item: item.to_owned(),
            demand,
            facility_budget,
            depth_left,
            ancestors: ancestors.to_vec(),
            ancestor_items: ancestor_items.to_vec(),
        };
        if let Some(options) = self.memo.get(&key) {
            self.stats.cache_hits += 1;
            return Ok(options.clone());
        }
        let external = external_option(item, demand);
        let item_index = self.item_indices[item];
        if recipe_bit_is_set(ancestor_items, item_index) {
            self.stats.cycle_choices_deferred += 1;
            return Ok(vec![external]);
        }
        if depth_left == 0 || self.memo.len() >= MAX_DP_MEMO_STATES {
            if self.memo.len() >= MAX_DP_MEMO_STATES {
                self.stats.truncated = true;
            }
            return Ok(vec![external]);
        }

        let mut by_facilities = BTreeMap::from([(0_i64, external)]);
        let producer_ids = self.producers.get(item).cloned().unwrap_or_default();
        for recipe_id in producer_ids {
            let recipe_index = self.recipe_indices[recipe_id.as_str()];
            if recipe_bit_is_set(ancestors, recipe_index) {
                self.stats.cycle_choices_deferred += 1;
                continue;
            }
            let recipe = self.recipes[&recipe_id].clone();
            let (run_rate, own_facilities) = producer_requirement(&recipe, item, demand)?;
            if own_facilities > facility_budget {
                continue;
            }
            let mut next_ancestors = ancestors.to_vec();
            set_recipe_bit(&mut next_ancestors, recipe_index);
            let mut next_ancestor_items = ancestor_items.to_vec();
            set_recipe_bit(&mut next_ancestor_items, item_index);
            let mut partials = vec![PartialOption {
                facilities: own_facilities,
                active_recipes: 1,
                hash: hash_recipe_choice(&recipe.id, own_facilities),
                children: Vec::new(),
            }];
            for input in &recipe.inputs {
                let child_demand = run_rate
                    .checked_mul_i64(input.quantity)
                    .map_err(|error| anyhow!("recipe input rate failed: {error:?}"))?;
                let child_options = self.subtree_options(
                    &input.item,
                    child_demand,
                    facility_budget - own_facilities,
                    depth_left - 1,
                    &next_ancestors,
                    &next_ancestor_items,
                )?;
                partials = self.combine_partials(partials, &child_options, facility_budget);
                if partials.is_empty() {
                    break;
                }
            }
            for partial in partials {
                self.stats.recipe_options += 1;
                let option = SubtreeOption {
                    facilities: partial.facilities,
                    active_recipes: partial.active_recipes,
                    hash: partial.hash,
                    blueprint: Arc::new(Blueprint::Recipe {
                        recipe: recipe.id.clone(),
                        own_facilities,
                        children: partial.children,
                    }),
                };
                insert_best_subtree(&mut by_facilities, option, self.seed);
            }
        }
        let options = by_facilities.into_values().collect::<Vec<_>>();
        self.memo.insert(key, options.clone());
        Ok(options)
    }

    fn combine_partials(
        &mut self,
        parents: Vec<PartialOption>,
        children: &[SubtreeOption],
        facility_limit: i64,
    ) -> Vec<PartialOption> {
        let mut combined = BTreeMap::<i64, PartialOption>::new();
        for parent in parents {
            for child in children {
                if self.stats.combinations >= MAX_DP_COMBINATIONS {
                    self.stats.truncated = true;
                    break;
                }
                self.stats.combinations += 1;
                let Some(facilities) = parent.facilities.checked_add(child.facilities) else {
                    self.stats.truncated = true;
                    continue;
                };
                let Some(active_recipes) = parent.active_recipes.checked_add(child.active_recipes)
                else {
                    self.stats.truncated = true;
                    continue;
                };
                if facilities > facility_limit || active_recipes > self.max_active_recipes {
                    continue;
                }
                let mut child_blueprints = parent.children.clone();
                child_blueprints.push(Arc::clone(&child.blueprint));
                let option = PartialOption {
                    facilities,
                    active_recipes,
                    hash: combine_hash(parent.hash, child.hash),
                    children: child_blueprints,
                };
                insert_best_partial(&mut combined, option, self.seed);
            }
            if self.stats.combinations >= MAX_DP_COMBINATIONS {
                break;
            }
        }
        combined.into_values().collect()
    }
}

fn start_schedule(
    root_producers: &[String],
    requested_facilities: i64,
    seed: u64,
    attempts: usize,
) -> Result<Vec<(String, usize)>> {
    let requested = usize::try_from(requested_facilities)?;
    let root_counts: Vec<usize> = if requested == 1 {
        vec![1]
    } else {
        (1..requested).collect()
    };
    let mut starts = root_producers
        .iter()
        .flat_map(|recipe| {
            root_counts
                .iter()
                .map(move |&count| (recipe.clone(), count))
        })
        .collect::<Vec<_>>();
    starts.sort_by_key(|(recipe, count)| stable_tie(seed, &format!("{recipe}\n{count}")));
    starts.truncate(attempts);
    Ok(starts)
}

fn producer_requirement(recipe: &Recipe, item: &str, demand: Rate) -> Result<(Rate, i64)> {
    let output_quantity = recipe
        .outputs
        .iter()
        .find(|output| output.item == item)
        .with_context(|| format!("recipe '{}' does not produce '{item}'", recipe.id))?
        .quantity;
    let run_rate = demand
        .checked_div_i64(output_quantity)
        .map_err(|error| anyhow!("producer run rate failed: {error:?}"))?;
    let work_rate = run_rate
        .checked_work_seconds_per_second(recipe.duration_ms)
        .map_err(|error| anyhow!("producer work rate failed: {error:?}"))?;
    Ok((run_rate, ceil_rate(work_rate)?))
}

fn ceil_rate(rate: Rate) -> Result<i64> {
    if rate.is_zero() {
        return Ok(0);
    }
    let base = rate.numerator / rate.denominator;
    if rate.numerator % rate.denominator == 0 {
        Ok(base)
    } else {
        base.checked_add(1).context("facility count overflow")
    }
}

fn external_option(item: &str, demand: Rate) -> SubtreeOption {
    SubtreeOption {
        facilities: 0,
        active_recipes: 0,
        hash: combine_hash(
            stable_tie(0, item),
            combine_hash(demand.numerator as u64, demand.denominator as u64),
        ),
        blueprint: Arc::new(Blueprint::External),
    }
}

fn insert_best_subtree(
    options: &mut BTreeMap<i64, SubtreeOption>,
    candidate: SubtreeOption,
    seed: u64,
) {
    let replace = options
        .get(&candidate.facilities)
        .is_none_or(|current| option_rank(seed, candidate.hash) < option_rank(seed, current.hash));
    if replace {
        options.insert(candidate.facilities, candidate);
    }
}

fn insert_best_partial(
    options: &mut BTreeMap<i64, PartialOption>,
    candidate: PartialOption,
    seed: u64,
) {
    let replace = options
        .get(&candidate.facilities)
        .is_none_or(|current| option_rank(seed, candidate.hash) < option_rank(seed, current.hash));
    if replace {
        options.insert(candidate.facilities, candidate);
    }
}

fn option_rank(seed: u64, hash: u64) -> u64 {
    mix64(seed ^ hash)
}

fn hash_recipe_choice(recipe: &str, own_facilities: i64) -> u64 {
    combine_hash(stable_tie(0, recipe), own_facilities as u64)
}

fn combine_hash(left: u64, right: u64) -> u64 {
    mix64(left ^ right.wrapping_add(0x9e37_79b9_7f4a_7c15))
}

fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn empty_recipe_bits(recipe_count: usize) -> Vec<u64> {
    vec![0; recipe_count.div_ceil(64)]
}

fn recipe_bit_is_set(bits: &[u64], index: usize) -> bool {
    bits[index / 64] & (1_u64 << (index % 64)) != 0
}

fn set_recipe_bit(bits: &mut [u64], index: usize) {
    bits[index / 64] |= 1_u64 << (index % 64);
}

fn materialize_result(
    target_quantity: i64,
    option: &SubtreeOption,
    recipes: &BTreeMap<String, Recipe>,
) -> Result<SearchResult> {
    let mut plan = Plan {
        target_quantity,
        selections: BTreeMap::new(),
    };
    let mut moves = Vec::new();
    let mut running_facilities = 0_i64;
    materialize_blueprint(
        "/target",
        &option.blueprint,
        true,
        recipes,
        &mut plan,
        &mut moves,
        &mut running_facilities,
    )?;
    ensure!(
        running_facilities == option.facilities,
        "materialized facility count does not match DP result"
    );
    Ok(SearchResult {
        plan,
        evaluation: Evaluation {
            facilities: option.facilities,
            active_recipes: option.active_recipes,
        },
        moves,
        tie: option.hash,
    })
}

#[allow(clippy::too_many_arguments)]
fn materialize_blueprint(
    path: &str,
    blueprint: &Blueprint,
    is_root: bool,
    recipes: &BTreeMap<String, Recipe>,
    plan: &mut Plan,
    moves: &mut Vec<MoveRecord>,
    running_facilities: &mut i64,
) -> Result<()> {
    match blueprint {
        Blueprint::External => {
            ensure!(!is_root, "target source cannot be external");
            plan.selections
                .insert(path.to_owned(), RecipeSource::ExternalInput);
        }
        Blueprint::Recipe {
            recipe,
            own_facilities,
            children,
        } => {
            plan.selections.insert(
                path.to_owned(),
                RecipeSource::Recipe {
                    recipe: recipe.clone(),
                },
            );
            let before = *running_facilities;
            *running_facilities = running_facilities
                .checked_add(*own_facilities)
                .context("materialized facility count overflow")?;
            if !is_root {
                moves.push(MoveRecord {
                    kind: "expand",
                    path: path.to_owned(),
                    recipe: Some(recipe.clone()),
                    before_facilities: before,
                    after_facilities: *running_facilities,
                });
            }
            let definition = &recipes[recipe];
            ensure!(
                definition.inputs.len() == children.len(),
                "DP blueprint child count changed during materialization"
            );
            for (input, child) in definition.inputs.iter().zip(children) {
                materialize_blueprint(
                    &format!("{path}/recipe:{recipe}/input:{}", input.item),
                    child,
                    false,
                    recipes,
                    plan,
                    moves,
                    running_facilities,
                )?;
            }
        }
    }
    Ok(())
}

fn better_result(
    candidate: &SearchResult,
    current: Option<&SearchResult>,
    target_facilities: i64,
) -> bool {
    current.is_none_or(|current| {
        (
            candidate.evaluation.facilities.abs_diff(target_facilities),
            candidate.tie,
        ) < (
            current.evaluation.facilities.abs_diff(target_facilities),
            current.tie,
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn refine_with_cycle_closures(
    book: &ValidatedRecipeBook,
    recipes: &BTreeMap<String, Recipe>,
    producers: &BTreeMap<String, Vec<String>>,
    target_item: &str,
    target_facilities: i64,
    mut best: SearchResult,
    exact_plans_evaluated: &mut usize,
) -> Result<SearchResult> {
    let exact_base = evaluate_plan(book, target_item, &best.plan)
        .context("DP source plan failed exact evaluation")?;
    *exact_plans_evaluated += 1;
    ensure!(
        exact_base.facilities == best.evaluation.facilities
            && exact_base.active_recipes == best.evaluation.active_recipes,
        "DP facility prediction disagrees with exact acyclic evaluation"
    );
    let base = best.clone();
    let check = check_recipe_source_plan(book, &request_for(target_item, &base.plan));
    ensure!(check.ready, "DP source plan is not ready");
    let mut leaves = Vec::new();
    collect_external_leaves(check.root.as_ref(), &mut leaves);
    for leaf in leaves {
        for recipe_id in producers.get(&leaf.item).into_iter().flatten() {
            if !path_has_ancestor_recipe(&leaf.path, recipe_id) {
                continue;
            }
            let plan = expand_cycle_leaf(&base.plan, &leaf, &recipes[recipe_id]);
            let Some(evaluation) = evaluate_plan(book, target_item, &plan) else {
                *exact_plans_evaluated += 1;
                continue;
            };
            *exact_plans_evaluated += 1;
            let candidate = SearchResult {
                tie: stable_tie(base.tie, &format!("{}={recipe_id}", leaf.path)),
                moves: {
                    let mut moves = base.moves.clone();
                    moves.push(MoveRecord {
                        kind: "cycle-close",
                        path: leaf.path.clone(),
                        recipe: Some(recipe_id.clone()),
                        before_facilities: base.evaluation.facilities,
                        after_facilities: evaluation.facilities,
                    });
                    moves
                },
                plan,
                evaluation,
            };
            if better_result(&candidate, Some(&best), target_facilities) {
                best = candidate;
            }
            if best.evaluation.facilities == target_facilities {
                return Ok(best);
            }
        }
    }
    Ok(best)
}

fn collect_external_leaves(node: Option<&RecipeSourceNode>, leaves: &mut Vec<ExternalLeaf>) {
    let Some(node) = node else {
        return;
    };
    match node.selected_source.as_ref() {
        Some(RecipeSource::ExternalInput) => leaves.push(ExternalLeaf {
            path: node.path.clone(),
            item: node.item.clone(),
        }),
        Some(RecipeSource::Recipe { .. }) => {
            for child in &node.children {
                collect_external_leaves(Some(child), leaves);
            }
        }
        None => {}
    }
}

fn path_has_ancestor_recipe(path: &str, recipe: &str) -> bool {
    path.contains(&format!("/recipe:{recipe}/"))
}

fn expand_cycle_leaf(current: &Plan, leaf: &ExternalLeaf, recipe: &Recipe) -> Plan {
    let mut plan = current.clone();
    plan.selections.insert(
        leaf.path.clone(),
        RecipeSource::Recipe {
            recipe: recipe.id.clone(),
        },
    );
    plan
}

fn evaluate_plan(book: &ValidatedRecipeBook, target_item: &str, plan: &Plan) -> Option<Evaluation> {
    let throughput = book.calculate_contextual_throughput(&request_for(target_item, plan));
    if !throughput.success {
        return None;
    }
    let facilities = calculate_contextual_facility_requirements(&throughput);
    if !facilities.success {
        return None;
    }
    let active_recipes = facilities.occurrence_requirements.len();
    facilities
        .occurrence_requirements
        .iter()
        .try_fold(0_i64, |sum, requirement| {
            sum.checked_add(requirement.required_facilities)
        })
        .map(|facilities| Evaluation {
            facilities,
            active_recipes,
        })
}

fn validate_final_plan(
    book: &ValidatedRecipeBook,
    producers: &BTreeMap<String, Vec<String>>,
    target_item: &str,
    plan: &Plan,
    expected: &Evaluation,
) -> Result<FinalObservations> {
    let throughput = book.calculate_contextual_throughput(&request_for(target_item, plan));
    ensure!(
        throughput.success,
        "selected plan failed final throughput validation"
    );
    let facilities = calculate_contextual_facility_requirements(&throughput);
    ensure!(
        facilities.success,
        "selected plan failed final facility validation"
    );
    ensure!(
        facilities.occurrence_requirements.len() == expected.active_recipes,
        "selected plan active recipe count changed during final validation"
    );
    let exact_facilities = facilities
        .occurrence_requirements
        .iter()
        .try_fold(0_i64, |sum, requirement| {
            sum.checked_add(requirement.required_facilities)
        })
        .context("selected plan facility count overflow")?;
    ensure!(
        exact_facilities == expected.facilities,
        "selected plan facility count changed during final validation"
    );
    let wiring = build_contextual_facility_instance_wiring(&throughput, &facilities);
    ensure!(
        wiring.success,
        "selected plan failed final instance wiring validation"
    );
    let facility_instances = wiring
        .nodes
        .iter()
        .filter(|node| matches!(node, FacilityInstanceWiringNode::Facility { .. }))
        .count();
    ensure!(
        i64::try_from(facility_instances)? == expected.facilities,
        "selected plan facility count changed during final wiring validation"
    );
    let external_items = throughput
        .external_input_rates
        .iter()
        .map(|input| input.item.as_str())
        .collect::<BTreeSet<_>>();
    let facility_blocks = wiring
        .nodes
        .iter()
        .filter_map(|node| match node {
            FacilityInstanceWiringNode::Facility { id, facility, .. } => {
                Some((id.clone(), facility.clone()))
            }
            _ => None,
        })
        .enumerate()
        .map(|(index, (semantic_node, facility))| {
            (
                semantic_node,
                Block {
                    id: format!("block-{index}"),
                    facility,
                },
            )
        })
        .collect();
    Ok(FinalObservations {
        facility_instances,
        external_source_occurrences: throughput.external_input_rates.len(),
        distinct_external_items: external_items.len(),
        catalog_produced_external_sources: throughput
            .external_input_rates
            .iter()
            .filter(|input| producers.contains_key(&input.item))
            .count(),
        external_input_edges: wiring
            .edges
            .iter()
            .filter(|edge| edge.kind == "external-input")
            .count(),
        total_wiring_edges: wiring.edges.len(),
        facility_blocks,
        wiring,
    })
}

fn place_facilities(
    catalog_dir: &std::path::Path,
    args: &Args,
    facility_blocks: &[(String, Block)],
) -> Result<PhysicalPlacement> {
    ensure!(
        args.placement_gap_min >= 0
            && args.placement_gap_max >= args.placement_gap_min
            && args.placement_gap_max <= 128,
        "placement gaps must satisfy 0 <= min <= max <= 128"
    );
    let loaded = load_catalogs(catalog_dir)?;
    let blocks = facility_blocks
        .iter()
        .map(|(_, block)| block.clone())
        .collect::<Vec<_>>();
    let sampled = match (args.placement_density_min, args.placement_density_max) {
        (Some(minimum), Some(maximum)) => placement::sample_in_density_band(
            &loaded.catalogs,
            &blocks,
            &DensityPlacementRequest {
                seed: args.seed,
                band: DensityBand { minimum, maximum },
                max_uniform_gap: 128,
                margin: 6,
                max_cells: 250_000_000,
            },
        ),
        (None, None) => placement::sample(
            &loaded.catalogs,
            &blocks,
            &PlacementRequest {
                seed: args.seed,
                gap: [args.placement_gap_min, args.placement_gap_max],
                margin: 6,
                max_cells: 250_000_000,
            },
        ),
        _ => unreachable!("clap requires both density bounds"),
    }
    .map_err(|diagnostic| anyhow!("frozen facility placement failed: {diagnostic:?}"))?;
    Ok(PhysicalPlacement {
        observation: PlacementObservation {
            semantic_facilities: facility_blocks
                .iter()
                .map(|(semantic_node, block)| SemanticFacilityBlock {
                    semantic_node: semantic_node.clone(),
                    block: block.id.clone(),
                    facility: block.facility.clone(),
                })
                .collect(),
            sequence_pair: sampled.sequence_pair.clone(),
            body_width: sampled.body_width,
            body_height: sampled.body_height,
            body_cells: sampled.body_cells,
            body_bounding_area: sampled.body_bounding_area,
            body_density: sampled.body_density,
            gap: sampled.gap,
            canvas_width: sampled.canvas_width,
            canvas_height: sampled.canvas_height,
        },
        sampled,
        catalogs: loaded.catalogs,
    })
}

fn compare_recipe_routing(
    catalog_dir: &std::path::Path,
    args: &Args,
    observations: &FinalObservations,
    physical: &PhysicalPlacement,
) -> Result<RoutingObservation> {
    let items =
        ValidatedItemCatalog::try_from_catalog(load_item_catalog(catalog_dir.join("items.json"))?)
            .map_err(|report| anyhow!("item catalog is invalid: {report:?}"))?;
    let bounds = Bounds {
        min_x: 0,
        min_y: 0,
        max_x: physical.sampled.canvas_width - 1,
        max_y: physical.sampled.canvas_height - 1,
    };
    let poses = physical
        .sampled
        .poses
        .iter()
        .map(|pose| (pose.block.as_str(), pose))
        .collect::<BTreeMap<_, _>>();
    let mut body = BTreeSet::new();
    let mut ports = BTreeMap::<String, Vec<Port>>::new();
    for (semantic_node, block) in &observations.facility_blocks {
        let pose = poses
            .get(block.id.as_str())
            .with_context(|| format!("placement omitted block '{}'", block.id))?;
        let definition = physical
            .catalogs
            .facilities
            .facility(&block.facility)
            .with_context(|| format!("catalog omitted facility '{}'", block.facility))?;
        let (width, height, projected) = rotate(definition, pose.rotation, pose.position);
        for y in pose.position.y..pose.position.y + height {
            for x in pose.position.x..pose.position.x + width {
                body.insert(Point { x, y });
            }
        }
        ports.insert(semantic_node.clone(), projected);
    }

    let boundary_sources = boundary_terminals("external-source", bounds, true, &body);
    let boundary_targets = boundary_terminals("external-target", bounds, false, &body);
    let mut jobs = Vec::with_capacity(observations.wiring.edges.len());
    let mut job_observations = Vec::with_capacity(observations.wiring.edges.len());
    let internal = observations
        .wiring
        .edges
        .iter()
        .filter(|edge| ports.contains_key(&edge.source) && ports.contains_key(&edge.target))
        .count();
    let boundary_only = observations
        .wiring
        .edges
        .iter()
        .filter(|edge| !ports.contains_key(&edge.source) && !ports.contains_key(&edge.target))
        .count();
    let external = observations.wiring.edges.len() - internal;
    for edge in &observations.wiring.edges {
        let item = items
            .item(&edge.item)
            .with_context(|| format!("wiring edge '{}' references unknown item", edge.id))?;
        let sources = if let Some(candidates) = ports.get(&edge.source) {
            facility_terminals(
                &format!("source:{}", edge.source),
                candidates,
                FacilityPortDirection::Output,
                item.transport,
                bounds,
                &body,
            )
        } else {
            boundary_sources.clone()
        };
        let targets = if let Some(candidates) = ports.get(&edge.target) {
            facility_terminals(
                &format!("target:{}", edge.target),
                candidates,
                FacilityPortDirection::Input,
                item.transport,
                bounds,
                &body,
            )
        } else {
            boundary_targets.clone()
        };
        job_observations.push(RoutingJobObservation {
            id: edge.id.clone(),
            source: edge.source.clone(),
            target: edge.target.clone(),
            commodity: edge.item.clone(),
            transport: item.transport,
            rate: edge.rate,
            source_candidates: sources.len(),
            target_candidates: targets.len(),
        });
        if sources.is_empty() || targets.is_empty() {
            return Ok(RoutingObservation {
                status: "terminal-domain-empty",
                guide_only: true,
                shared_network_legalization_performed: false,
                exact_component_legalization_performed: false,
                frozen_obligations: observations.wiring.edges.len(),
                internal_obligations: internal,
                external_obligations: external,
                boundary_only_obligations: boundary_only,
                jobs: job_observations,
                diagnostic: Some(format!(
                    "wiring edge '{}' has {} source and {} target candidates",
                    edge.id,
                    sources.len(),
                    targets.len()
                )),
                comparison: None,
            });
        }
        jobs.push(GlobalRouteJob {
            id: edge.id.clone(),
            commodity: edge.item.clone(),
            transport: item.transport,
            rate: edge.rate,
            sources,
            targets,
        });
    }
    let problem = GlobalRoutingProblem {
        bounds,
        blocked: body.iter().copied().collect(),
        jobs,
    };
    let comparison = compare_global_routing(
        &problem,
        &GlobalRoutingConfig {
            max_iterations: args.routing_iterations,
            work_per_route: args.routing_work_per_route,
            present_penalty: 8,
            history_increment: 2,
        },
    )
    .map_err(|diagnostic| anyhow!("global routing comparison failed: {diagnostic:?}"))?;
    Ok(RoutingObservation {
        status: "compared",
        guide_only: true,
        shared_network_legalization_performed: false,
        exact_component_legalization_performed: false,
        frozen_obligations: observations.wiring.edges.len(),
        internal_obligations: internal,
        external_obligations: external,
        boundary_only_obligations: boundary_only,
        jobs: job_observations,
        diagnostic: None,
        comparison: Some(comparison),
    })
}

fn facility_terminals(
    label: &str,
    ports: &[Port],
    expected: FacilityPortDirection,
    transport: aic_layout::logistics::TransportKind,
    bounds: Bounds,
    body: &BTreeSet<Point>,
) -> Vec<GlobalRouteTerminal> {
    ports
        .iter()
        .filter(|port| {
            port.direction == expected
                && port.transport == transport
                && bounds.contains(port.connection)
                && !body.contains(&port.connection)
        })
        .map(|port| GlobalRouteTerminal {
            id: format!("{label}:{}", port.id),
            point: port.connection,
            direction: Some(match expected {
                FacilityPortDirection::Output => edge_index(port.edge),
                FacilityPortDirection::Input => (edge_index(port.edge) + 2) % 4,
            }),
        })
        .collect()
}

fn boundary_terminals(
    label: &str,
    bounds: Bounds,
    source: bool,
    body: &BTreeSet<Point>,
) -> Vec<GlobalRouteTerminal> {
    let mut terminals = Vec::new();
    let mut push = |side: &str, point: Point, inward: usize| {
        if !body.contains(&point) {
            terminals.push(GlobalRouteTerminal {
                id: format!("{label}:{side}:{}:{}", point.x, point.y),
                point,
                direction: Some(if source { inward } else { (inward + 2) % 4 }),
            });
        }
    };
    for x in bounds.min_x..=bounds.max_x {
        push("north", Point { x, y: bounds.min_y }, 1);
        if bounds.max_y != bounds.min_y {
            push("south", Point { x, y: bounds.max_y }, 3);
        }
    }
    for y in bounds.min_y + 1..bounds.max_y {
        push("west", Point { x: bounds.min_x, y }, 0);
        if bounds.max_x != bounds.min_x {
            push("east", Point { x: bounds.max_x, y }, 2);
        }
    }
    terminals
}

fn request_for(target_item: &str, plan: &Plan) -> RecipeSourcePlanRequest {
    RecipeSourcePlanRequest {
        schema_version: 1,
        target: ThroughputTarget {
            item: target_item.to_owned(),
            quantity: plan.target_quantity,
            duration_ms: RATE_HORIZON_MS,
        },
        source_selections: plan
            .selections
            .iter()
            .map(|(path, source)| RecipeSourceSelection {
                path: path.clone(),
                source: source.clone(),
            })
            .collect(),
    }
}

fn target_quantity_for_root(recipe: &Recipe, target_item: &str, facilities: usize) -> Result<i64> {
    ensure!(
        RATE_HORIZON_MS % recipe.duration_ms == 0,
        "recipe '{}' duration does not divide {RATE_HORIZON_MS}ms",
        recipe.id
    );
    let output_per_run = recipe
        .outputs
        .iter()
        .find(|output| output.item == target_item)
        .context("root recipe does not produce target")?
        .quantity;
    let runs_per_facility = RATE_HORIZON_MS / recipe.duration_ms;
    i64::try_from(facilities)?
        .checked_mul(runs_per_facility)
        .and_then(|runs| runs.checked_mul(output_per_run))
        .context("target quantity overflow")
}

fn producer_index(book: &ValidatedRecipeBook) -> BTreeMap<String, Vec<String>> {
    let mut producers = BTreeMap::<String, Vec<String>>::new();
    for recipe in &book.recipe_book().recipes {
        for output in &recipe.outputs {
            producers
                .entry(output.item.clone())
                .or_default()
                .push(recipe.id.clone());
        }
    }
    for recipes in producers.values_mut() {
        recipes.sort();
        recipes.dedup();
    }
    producers
}

fn recipe_index(book: &ValidatedRecipeBook) -> BTreeMap<String, Recipe> {
    book.recipe_book()
        .recipes
        .iter()
        .map(|recipe| (recipe.id.clone(), recipe.clone()))
        .collect()
}

fn stable_tie(seed: u64, value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(seed ^ 0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_data::recipes::ItemAmount;

    #[test]
    fn external_boundary_domains_are_unique_and_directional() {
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: 2,
            max_y: 2,
        };
        let sources = boundary_terminals("source", bounds, true, &BTreeSet::new());
        let targets = boundary_terminals("target", bounds, false, &BTreeSet::new());

        assert_eq!(sources.len(), 8);
        assert_eq!(targets.len(), 8);
        assert_eq!(
            sources
                .iter()
                .map(|terminal| &terminal.id)
                .collect::<BTreeSet<_>>()
                .len(),
            sources.len()
        );
        assert_eq!(
            sources
                .iter()
                .find(|terminal| terminal.point == Point { x: 1, y: 0 })
                .unwrap()
                .direction,
            Some(1)
        );
        assert_eq!(
            targets
                .iter()
                .find(|terminal| terminal.point == Point { x: 1, y: 0 })
                .unwrap()
                .direction,
            Some(3)
        );
    }

    fn recipe(id: &str, inputs: &[&str], output: &str, duration_ms: i64) -> Recipe {
        Recipe {
            id: id.to_owned(),
            facility: format!("facility-{id}"),
            inputs: inputs
                .iter()
                .map(|item| ItemAmount {
                    item: (*item).to_owned(),
                    quantity: 1,
                })
                .collect(),
            outputs: vec![ItemAmount {
                item: output.to_owned(),
                quantity: 1,
            }],
            duration_ms,
        }
    }

    #[test]
    fn combines_sibling_choices_instead_of_committing_to_the_closest_leaf() {
        let recipes = [
            recipe("root", &["a", "b"], "target", 1_000),
            recipe("a-near", &[], "a", 4_000),
            recipe("a-combines", &[], "a", 2_000),
            recipe("b-three", &[], "b", 3_000),
        ]
        .into_iter()
        .map(|recipe| (recipe.id.clone(), recipe))
        .collect::<BTreeMap<_, _>>();
        let producers = producer_index_from_recipes(&recipes);
        let mut dp = DpContext::new(&recipes, &producers, 6, 8, 0);

        let options = dp.root_options(&recipes["root"], "target", 20, 8).unwrap();
        let exact = options
            .iter()
            .find(|option| option.facilities == 6)
            .expect("two sibling choices should combine to the exact count");
        let result = materialize_result(20, exact, &recipes).unwrap();

        assert_eq!(result.evaluation.facilities, 6);
        assert!(matches!(
            result.plan.selections["/target/recipe:root/input:a"],
            RecipeSource::Recipe { ref recipe } if recipe == "a-combines"
        ));
        assert!(matches!(
            result.plan.selections["/target/recipe:root/input:b"],
            RecipeSource::Recipe { ref recipe } if recipe == "b-three"
        ));
    }

    #[test]
    fn equal_size_representatives_do_not_prefer_fewer_recipe_occurrences() {
        let seed = 19;
        let first_hash = 3;
        let second_hash = (4..10_000)
            .find(|hash| option_rank(seed, *hash) < option_rank(seed, first_hash))
            .expect("the fixed search range contains a better seeded rank");
        let mut options = BTreeMap::new();
        insert_best_subtree(
            &mut options,
            SubtreeOption {
                facilities: 8,
                active_recipes: 1,
                hash: first_hash,
                blueprint: Arc::new(Blueprint::External),
            },
            seed,
        );
        insert_best_subtree(
            &mut options,
            SubtreeOption {
                facilities: 8,
                active_recipes: 9,
                hash: second_hash,
                blueprint: Arc::new(Blueprint::External),
            },
            seed,
        );

        assert_eq!(options[&8].active_recipes, 9);
        assert_eq!(options[&8].hash, second_hash);
    }

    fn producer_index_from_recipes(
        recipes: &BTreeMap<String, Recipe>,
    ) -> BTreeMap<String, Vec<String>> {
        let mut producers = BTreeMap::<String, Vec<String>>::new();
        for recipe in recipes.values() {
            for output in &recipe.outputs {
                producers
                    .entry(output.item.clone())
                    .or_default()
                    .push(recipe.id.clone());
            }
        }
        producers
    }
}
