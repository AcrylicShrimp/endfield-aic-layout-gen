//! Research-only leaf-adjustment recipe source-plan probe.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

use aic_data::recipes::{
    FacilityInstanceWiringNode, Recipe, RecipeSource, RecipeSourceNode, RecipeSourcePlanRequest,
    RecipeSourceSelection, ThroughputTarget, ValidatedRecipeBook,
    build_contextual_facility_instance_wiring, calculate_contextual_facility_requirements,
    check_recipe_source_plan, load_recipe_book,
};
use anyhow::{Context, Result, ensure};
use clap::Parser;
use serde::Serialize;

const RATE_HORIZON_MS: i64 = 20_000;

#[derive(Parser)]
#[command(about = "Research only: adjust recipe source-plan leaves toward a facility count")]
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
    #[arg(long)]
    report_output: PathBuf,
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
    evaluated_plans: usize,
    attempted_starts: usize,
    target_quantity: i64,
    target_duration_ms: i64,
    active_recipe_occurrences: usize,
    facility_instances: usize,
    source_selections: Vec<OutputSelection>,
    moves: Vec<MoveRecord>,
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

#[derive(Clone)]
struct SearchResult {
    plan: Plan,
    evaluation: Evaluation,
    moves: Vec<MoveRecord>,
}

struct Candidate {
    plan: Plan,
    evaluation: Evaluation,
    movement: MoveRecord,
    key: String,
}

#[derive(Clone)]
struct ExternalLeaf {
    path: String,
    item: String,
}

#[derive(Clone)]
struct RecipeLeaf {
    path: String,
    recipe: String,
}

struct Leaves {
    external: Vec<ExternalLeaf>,
    recipes: Vec<RecipeLeaf>,
}

struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut value = self.state;
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^ (value >> 31)
    }

    fn index(&mut self, len: usize) -> usize {
        debug_assert!(len > 0);
        (self.next() % u64::try_from(len).expect("collection length fits u64")) as usize
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(args.facilities > 0, "--facilities must be positive");
    ensure!(args.attempts > 0, "--attempts must be positive");
    ensure!(args.max_steps > 0, "--max-steps must be positive");
    ensure!(
        !args.report_output.exists(),
        "report output already exists: {}",
        args.report_output.display()
    );
    let raw = load_recipe_book(&args.recipes)
        .with_context(|| format!("loading {}", args.recipes.display()))?;
    let book = ValidatedRecipeBook::try_from_recipe_book(raw)
        .map_err(|report| anyhow::anyhow!("recipe catalog is invalid: {report:?}"))?;
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
    let mut rng = SplitMix64::new(args.seed);
    let mut evaluated_plans = 0_usize;
    let mut best: Option<SearchResult> = None;
    let mut attempted_starts = 0_usize;
    for attempt in 0..args.attempts {
        let root_recipe = &root_producers[rng.index(root_producers.len())];
        let recipe = &recipes[root_recipe];
        let requested = usize::try_from(args.facilities)?;
        let root_facilities = if requested == 1 {
            1
        } else {
            1 + rng.index(requested - 1)
        };
        let target_quantity = target_quantity_for_root(recipe, &args.target_item, root_facilities)?;
        let initial = minimal_plan(recipe, target_quantity);
        let Some(result) = search_from(
            &book,
            &recipes,
            &producers,
            &args.target_item,
            args.facilities,
            initial,
            args.seed ^ u64::try_from(attempt)?,
            args.max_steps,
            &mut evaluated_plans,
        ) else {
            continue;
        };
        attempted_starts += 1;
        let replace = best.as_ref().is_none_or(|current| {
            result.evaluation.facilities.abs_diff(args.facilities)
                < current.evaluation.facilities.abs_diff(args.facilities)
        });
        if replace {
            best = Some(result);
        }
        if best
            .as_ref()
            .is_some_and(|result| result.evaluation.facilities == args.facilities)
        {
            break;
        }
    }
    let best = best.context("no valid source plan was constructed")?;
    let facility_instances =
        validate_final_plan(&book, &args.target_item, &best.plan, &best.evaluation)?;
    let achieved = best.evaluation.facilities;
    let report = Report {
        schema_version: 1,
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
        evaluated_plans,
        attempted_starts,
        target_quantity: best.plan.target_quantity,
        target_duration_ms: RATE_HORIZON_MS,
        active_recipe_occurrences: best.evaluation.active_recipes,
        facility_instances,
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
        "status={} requested={} achieved={} evaluated={} elapsed_us={}",
        report.status,
        report.requested_facilities,
        report.achieved_facilities,
        report.evaluated_plans,
        report.elapsed_us
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn search_from(
    book: &ValidatedRecipeBook,
    recipes: &BTreeMap<String, Recipe>,
    producers: &BTreeMap<String, Vec<String>>,
    target_item: &str,
    target_facilities: i64,
    initial: Plan,
    seed: u64,
    max_steps: usize,
    evaluated_plans: &mut usize,
) -> Option<SearchResult> {
    let mut cache = BTreeMap::<String, Option<Evaluation>>::new();
    let mut current = initial;
    let mut current_evaluation =
        evaluate_plan(book, target_item, &current, &mut cache, evaluated_plans)?;
    let mut best = SearchResult {
        plan: current.clone(),
        evaluation: current_evaluation.clone(),
        moves: Vec::new(),
    };
    let mut moves = Vec::new();
    let mut visited = BTreeSet::from([plan_key(&current)]);

    for step in 0..max_steps {
        if current_evaluation.facilities == target_facilities {
            break;
        }
        let check = check_recipe_source_plan(book, &request_for(target_item, &current));
        if !check.ready {
            break;
        }
        let mut leaves = Leaves {
            external: Vec::new(),
            recipes: Vec::new(),
        };
        collect_leaves(check.root.as_ref()?, true, &mut leaves);
        let mut candidates = Vec::new();
        if current_evaluation.facilities < target_facilities {
            for leaf in &leaves.external {
                for recipe_id in producers.get(&leaf.item).into_iter().flatten() {
                    let plan = expand_leaf(&current, leaf, &recipes[recipe_id]);
                    push_candidate(
                        book,
                        target_item,
                        target_facilities,
                        &current_evaluation,
                        plan,
                        MoveRecord {
                            kind: "expand",
                            path: leaf.path.clone(),
                            recipe: Some(recipe_id.clone()),
                            before_facilities: current_evaluation.facilities,
                            after_facilities: 0,
                        },
                        &visited,
                        &mut cache,
                        evaluated_plans,
                        &mut candidates,
                    );
                }
            }
        } else {
            for leaf in &leaves.recipes {
                let plan = contract_leaf(&current, leaf);
                push_candidate(
                    book,
                    target_item,
                    target_facilities,
                    &current_evaluation,
                    plan,
                    MoveRecord {
                        kind: "contract",
                        path: leaf.path.clone(),
                        recipe: Some(leaf.recipe.clone()),
                        before_facilities: current_evaluation.facilities,
                        after_facilities: 0,
                    },
                    &visited,
                    &mut cache,
                    evaluated_plans,
                    &mut candidates,
                );
            }
        }
        let current_distance = current_evaluation.facilities.abs_diff(target_facilities);
        candidates.retain(|candidate| {
            candidate.evaluation.facilities.abs_diff(target_facilities) < current_distance
        });
        candidates.sort_by_key(|candidate| {
            (
                candidate.evaluation.facilities.abs_diff(target_facilities),
                stable_tie(seed ^ u64::try_from(step).unwrap_or(0), &candidate.key),
            )
        });
        let Some(mut chosen) = candidates.into_iter().next() else {
            break;
        };
        chosen.movement.after_facilities = chosen.evaluation.facilities;
        visited.insert(chosen.key);
        moves.push(chosen.movement);
        current = chosen.plan;
        current_evaluation = chosen.evaluation;
        if current_evaluation.facilities.abs_diff(target_facilities)
            < best.evaluation.facilities.abs_diff(target_facilities)
        {
            best = SearchResult {
                plan: current.clone(),
                evaluation: current_evaluation.clone(),
                moves: moves.clone(),
            };
        }
    }
    Some(best)
}

#[allow(clippy::too_many_arguments)]
fn push_candidate(
    book: &ValidatedRecipeBook,
    target_item: &str,
    target_facilities: i64,
    current: &Evaluation,
    plan: Plan,
    movement: MoveRecord,
    visited: &BTreeSet<String>,
    cache: &mut BTreeMap<String, Option<Evaluation>>,
    evaluated_plans: &mut usize,
    candidates: &mut Vec<Candidate>,
) {
    let key = plan_key(&plan);
    if visited.contains(&key) {
        return;
    }
    let Some(evaluation) = evaluate_plan(book, target_item, &plan, cache, evaluated_plans) else {
        return;
    };
    let moves_in_direction = if current.facilities < target_facilities {
        evaluation.facilities > current.facilities
    } else {
        evaluation.facilities < current.facilities
    };
    if moves_in_direction {
        candidates.push(Candidate {
            plan,
            evaluation,
            movement,
            key,
        });
    }
}

fn evaluate_plan(
    book: &ValidatedRecipeBook,
    target_item: &str,
    plan: &Plan,
    cache: &mut BTreeMap<String, Option<Evaluation>>,
    evaluated_plans: &mut usize,
) -> Option<Evaluation> {
    let key = plan_key(plan);
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }
    *evaluated_plans += 1;
    let throughput = book.calculate_contextual_throughput(&request_for(target_item, plan));
    let evaluation = if throughput.success {
        let facilities = calculate_contextual_facility_requirements(&throughput);
        if facilities.success {
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
        } else {
            None
        }
    } else {
        None
    };
    cache.insert(key, evaluation.clone());
    evaluation
}

fn validate_final_plan(
    book: &ValidatedRecipeBook,
    target_item: &str,
    plan: &Plan,
    expected: &Evaluation,
) -> Result<usize> {
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
        "selected plan facility count changed during final validation"
    );
    Ok(facility_instances)
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

fn minimal_plan(root: &Recipe, target_quantity: i64) -> Plan {
    let mut selections = BTreeMap::from([(
        "/target".to_owned(),
        RecipeSource::Recipe {
            recipe: root.id.clone(),
        },
    )]);
    for input in &root.inputs {
        selections.insert(
            format!("/target/recipe:{}/input:{}", root.id, input.item),
            RecipeSource::ExternalInput,
        );
    }
    Plan {
        target_quantity,
        selections,
    }
}

fn expand_leaf(current: &Plan, leaf: &ExternalLeaf, recipe: &Recipe) -> Plan {
    let mut plan = current.clone();
    plan.selections.insert(
        leaf.path.clone(),
        RecipeSource::Recipe {
            recipe: recipe.id.clone(),
        },
    );
    let ancestor_marker = format!("/recipe:{}/", recipe.id);
    if !leaf.path.contains(&ancestor_marker) {
        for input in &recipe.inputs {
            plan.selections.insert(
                format!("{}/recipe:{}/input:{}", leaf.path, recipe.id, input.item),
                RecipeSource::ExternalInput,
            );
        }
    }
    plan
}

fn contract_leaf(current: &Plan, leaf: &RecipeLeaf) -> Plan {
    let mut plan = current.clone();
    let descendant_prefix = format!("{}/recipe:{}/", leaf.path, leaf.recipe);
    plan.selections
        .retain(|path, _| !path.starts_with(&descendant_prefix));
    plan.selections
        .insert(leaf.path.clone(), RecipeSource::ExternalInput);
    plan
}

fn collect_leaves(node: &RecipeSourceNode, is_root: bool, leaves: &mut Leaves) {
    match node.selected_source.as_ref() {
        Some(RecipeSource::ExternalInput) => leaves.external.push(ExternalLeaf {
            path: node.path.clone(),
            item: node.item.clone(),
        }),
        Some(RecipeSource::Recipe { recipe }) => {
            if !is_root
                && node.cycle_to_path.is_none()
                && node.children.iter().all(|child| {
                    matches!(child.selected_source, Some(RecipeSource::ExternalInput))
                        || child.cycle_to_path.is_some()
                })
            {
                leaves.recipes.push(RecipeLeaf {
                    path: node.path.clone(),
                    recipe: recipe.clone(),
                });
            }
            for child in &node.children {
                collect_leaves(child, false, leaves);
            }
        }
        None => {}
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

fn plan_key(plan: &Plan) -> String {
    let mut key = plan.target_quantity.to_string();
    for (path, source) in &plan.selections {
        key.push('\n');
        key.push_str(path);
        key.push('=');
        match source {
            RecipeSource::ExternalInput => key.push_str("external"),
            RecipeSource::Recipe { recipe } => key.push_str(recipe),
        }
    }
    key
}

fn stable_tie(seed: u64, value: &str) -> u64 {
    value
        .as_bytes()
        .iter()
        .fold(seed ^ 0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
        })
}
