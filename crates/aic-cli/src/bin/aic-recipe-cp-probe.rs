//! Research-only aggregate recipe-flow feasibility probe.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use aic_data::recipes::{Recipe, ValidatedRecipeBook, load_recipe_book};
use anyhow::{Context, Result, ensure};
use clap::{Parser, ValueEnum};
use pumpkin_solver::Solver;
use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::DefaultBrancher;
use pumpkin_solver::core::optimisation::OptimisationDirection;
use pumpkin_solver::core::optimisation::linear_sat_unsat::LinearSatUnsat;
use pumpkin_solver::core::results::{
    OptimisationResult, ProblemSolution, SatisfactionResult, SolutionReference,
};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};
use serde::Serialize;

const HORIZON_MS: i64 = 20_000;

#[derive(Parser)]
#[command(about = "Research only: probe aggregate recipe-flow CP feasibility")]
struct Args {
    #[arg(long)]
    recipes: PathBuf,
    #[arg(long)]
    target_item: String,
    #[arg(long)]
    facilities: i32,
    #[arg(long, default_value_t = 5_000)]
    time_limit_ms: u64,
    #[arg(long, value_enum, default_value_t = ConnectivityMode::None)]
    connectivity: ConnectivityMode,
    #[arg(long)]
    maximize_target: bool,
    #[arg(long, conflicts_with = "maximize_target")]
    free_target: bool,
    #[arg(long)]
    max_external_items: Option<i32>,
    #[arg(long)]
    max_waste_units: Option<i32>,
    #[arg(long, default_value_t = 64)]
    max_lazy_cuts: usize,
    #[arg(long, value_enum, default_value_t = ExternalPolicy::Any)]
    external_policy: ExternalPolicy,
    #[arg(long)]
    report_output: PathBuf,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    status: &'static str,
    target_item: String,
    requested_facilities: i32,
    horizon_ms: i64,
    elapsed_ms: u128,
    model: ModelSummary,
    solution: Option<SolutionSummary>,
    caveats: Vec<&'static str>,
}

#[derive(Serialize)]
struct ModelSummary {
    recipes: usize,
    items: usize,
    relevance_arcs: usize,
    connectivity: ConnectivityMode,
    external_policy: ExternalPolicy,
    integer_variables: usize,
    linear_constraints: usize,
    lazy_connectivity_cuts: usize,
}

#[derive(Serialize)]
struct SolutionSummary {
    facility_count: i32,
    active_recipe_count: usize,
    total_run_units: i64,
    total_external_units: i64,
    total_waste_units: i64,
    target_units: i32,
    recipes: Vec<ActiveRecipe>,
    external_inputs: Vec<ItemUnits>,
    waste_outputs: Vec<ItemUnits>,
    relevance_arcs: Vec<ActiveRelevanceArc>,
}

#[derive(Serialize)]
struct ActiveRecipe {
    recipe: String,
    facility: String,
    facilities: i32,
    run_units: i32,
    run_capacity_per_facility: i32,
}

#[derive(Serialize)]
struct ItemUnits {
    item: String,
    units: i32,
}

#[derive(Serialize)]
struct ActiveRelevanceArc {
    source_recipe: String,
    target_recipe: Option<String>,
    units: i32,
}

struct RecipeVars {
    facilities: DomainId,
    runs: DomainId,
    active: DomainId,
    capacity: i32,
}

struct ItemVars {
    external: DomainId,
    waste: DomainId,
    external_direction: DomainId,
}

struct RelevanceArc {
    source: usize,
    target: Option<usize>,
    source_name: String,
    target_name: Option<String>,
    flow: DomainId,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum ConnectivityMode {
    None,
    Recipe,
    Scc,
    Lazy,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum ExternalPolicy {
    Any,
    Catalog,
}

struct BuiltModel {
    solver: Solver,
    target_units: DomainId,
    recipe_vars: Vec<RecipeVars>,
    item_vars: BTreeMap<String, ItemVars>,
    relevance: Vec<RelevanceArc>,
    summary: ModelSummary,
}

fn main() -> Result<()> {
    let args = Args::parse();
    ensure!(args.facilities > 0, "--facilities must be positive");
    ensure!(
        args.max_external_items.is_none_or(|value| value >= 0),
        "--max-external-items must be non-negative"
    );
    ensure!(
        args.max_waste_units.is_none_or(|value| value >= 0),
        "--max-waste-units must be non-negative"
    );
    ensure!(
        !args.report_output.exists(),
        "report output already exists: {}",
        args.report_output.display()
    );

    let raw = load_recipe_book(&args.recipes)
        .with_context(|| format!("loading {}", args.recipes.display()))?;
    let book = ValidatedRecipeBook::try_from_recipe_book(raw)
        .map_err(|report| anyhow::anyhow!("recipe catalog is invalid: {report:?}"))?;
    ensure!(
        book.recipe_book().recipes.iter().any(|recipe| recipe
            .outputs
            .iter()
            .any(|output| output.item == args.target_item)),
        "target item '{}' has no producer",
        args.target_item
    );

    let started = Instant::now();
    let BuiltModel {
        mut solver,
        target_units,
        recipe_vars,
        item_vars,
        relevance,
        summary: mut model,
    } = build_model(
        &book,
        &args.target_item,
        args.facilities,
        args.connectivity,
        args.maximize_target || args.free_target,
        args.external_policy,
        args.max_external_items,
        args.max_waste_units,
    )?;
    let mut brancher = solver.default_brancher();
    let mut resolver = ResolutionResolver::default();
    let mut termination = TimeBudget::starting_now(Duration::from_millis(args.time_limit_ms));
    let (status, solution, lazy_cuts) = if args.connectivity == ConnectivityMode::Lazy {
        ensure!(
            !args.maximize_target,
            "lazy connectivity currently supports satisfaction or --free-target"
        );
        solve_with_lazy_connectivity(
            &mut solver,
            &mut brancher,
            &mut termination,
            &mut resolver,
            book.recipe_book().recipes.as_slice(),
            &recipe_vars,
            &item_vars,
            &relevance,
            target_units,
            &args.target_item,
            args.max_lazy_cuts,
        )
    } else if args.maximize_target {
        let callback = |_: &Solver,
                        _: SolutionReference,
                        _: &DefaultBrancher,
                        _: &ResolutionResolver|
         -> ControlFlow<()> { ControlFlow::Continue(()) };
        match solver.optimise(
            &mut brancher,
            &mut termination,
            &mut resolver,
            LinearSatUnsat::new(OptimisationDirection::Maximise, target_units, callback),
        ) {
            OptimisationResult::Optimal(solution) => (
                "optimal",
                Some(extract_solution(
                    &solution,
                    book.recipe_book().recipes.as_slice(),
                    &recipe_vars,
                    &item_vars,
                    &relevance,
                    target_units,
                )),
                0,
            ),
            OptimisationResult::Satisfiable(solution)
            | OptimisationResult::Stopped(solution, ()) => (
                "feasible-time-limit",
                Some(extract_solution(
                    &solution,
                    book.recipe_book().recipes.as_slice(),
                    &recipe_vars,
                    &item_vars,
                    &relevance,
                    target_units,
                )),
                0,
            ),
            OptimisationResult::Unsatisfiable => ("unsatisfiable", None, 0),
            OptimisationResult::Unknown => ("time-limit", None, 0),
        }
    } else {
        match solver.satisfy(&mut brancher, &mut termination, &mut resolver) {
            SatisfactionResult::Satisfiable(result) => (
                "satisfiable",
                Some(extract_solution(
                    &result.solution(),
                    book.recipe_book().recipes.as_slice(),
                    &recipe_vars,
                    &item_vars,
                    &relevance,
                    target_units,
                )),
                0,
            ),
            SatisfactionResult::Unsatisfiable(_, _, _) => ("unsatisfiable", None, 0),
            SatisfactionResult::Unknown(_, _, _) => ("time-limit", None, 0),
        }
    };
    model.lazy_connectivity_cuts = lazy_cuts;
    let elapsed_ms = started.elapsed().as_millis();
    let report = Report {
        schema_version: 1,
        status,
        target_item: args.target_item,
        requested_facilities: args.facilities,
        horizon_ms: HORIZON_MS,
        elapsed_ms,
        model,
        solution,
        caveats: vec![
            "Aggregate recipe flows are not yet convertible to the path-specific source-plan contract.",
            match args.connectivity {
                ConnectivityMode::None => {
                    "Target connectivity is disabled, so unrelated active recipes are permitted."
                }
                ConnectivityMode::Recipe => {
                    "Recipe relevance arcs prove graph connectivity but are not linked to item-specific material flow."
                }
                ConnectivityMode::Scc => {
                    "SCC relevance proves component connectivity; recipes inside one active SCC can still form an irrelevant subcycle."
                }
                ConnectivityMode::Lazy => {
                    "Lazy cuts enforce target reachability in the active recipe graph but are not linked to item-specific material allocation."
                }
            },
            "The first feasible solution has no quality or diversity objective.",
        ],
    };
    let bytes = serde_json::to_vec_pretty(&report)?;
    std::fs::write(&args.report_output, bytes)
        .with_context(|| format!("writing {}", args.report_output.display()))?;
    println!(
        "status={status} facilities={} elapsed_ms={elapsed_ms}",
        args.facilities
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn solve_with_lazy_connectivity(
    solver: &mut Solver,
    brancher: &mut DefaultBrancher,
    termination: &mut TimeBudget,
    resolver: &mut ResolutionResolver,
    recipes: &[Recipe],
    recipe_vars: &[RecipeVars],
    item_vars: &BTreeMap<String, ItemVars>,
    relevance: &[RelevanceArc],
    target_units: DomainId,
    target_item: &str,
    max_cuts: usize,
) -> (&'static str, Option<SolutionSummary>, usize) {
    enum Step {
        Connected(SolutionSummary),
        Disconnected(Vec<usize>, SolutionSummary),
        Unsatisfiable,
        Unknown,
    }

    let mut cuts = 0;
    loop {
        let step = match solver.satisfy(brancher, termination, resolver) {
            SatisfactionResult::Satisfiable(result) => {
                let solution = result.solution();
                let summary = extract_solution(
                    &solution,
                    recipes,
                    recipe_vars,
                    item_vars,
                    relevance,
                    target_units,
                );
                let disconnected =
                    disconnected_active_recipes(&solution, recipes, recipe_vars, target_item);
                if disconnected.is_empty() {
                    Step::Connected(summary)
                } else {
                    Step::Disconnected(disconnected, summary)
                }
            }
            SatisfactionResult::Unsatisfiable(_, _, _) => Step::Unsatisfiable,
            SatisfactionResult::Unknown(_, _, _) => Step::Unknown,
        };
        match step {
            Step::Connected(solution) => return ("satisfiable", Some(solution), cuts),
            Step::Disconnected(_, solution) if cuts >= max_cuts => {
                return ("lazy-cut-limit", Some(solution), cuts);
            }
            Step::Disconnected(disconnected, _) => {
                let rhs = i32::try_from(disconnected.len().saturating_sub(1))
                    .expect("recipe count fits i32");
                let tag = solver.new_constraint_tag();
                solver
                    .add_constraint(pumpkin_solver::less_than_or_equals(
                        disconnected
                            .into_iter()
                            .map(|recipe| recipe_vars[recipe].active.scaled(1))
                            .collect::<Vec<_>>(),
                        rhs,
                        tag,
                    ))
                    .post();
                cuts += 1;
            }
            Step::Unsatisfiable => return ("unsatisfiable", None, cuts),
            Step::Unknown => return ("time-limit", None, cuts),
        }
    }
}

fn disconnected_active_recipes(
    solution: &impl ProblemSolution,
    recipes: &[Recipe],
    recipe_vars: &[RecipeVars],
    target_item: &str,
) -> Vec<usize> {
    let active = recipe_vars
        .iter()
        .map(|vars| solution.get_integer_value(vars.active) == 1)
        .collect::<Vec<_>>();
    let outputs = recipes
        .iter()
        .map(|recipe| {
            recipe
                .outputs
                .iter()
                .map(|amount| amount.item.as_str())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let inputs = recipes
        .iter()
        .map(|recipe| {
            recipe
                .inputs
                .iter()
                .map(|amount| amount.item.as_str())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut reachable = recipes
        .iter()
        .enumerate()
        .map(|(index, recipe)| {
            active[index]
                && recipe
                    .outputs
                    .iter()
                    .any(|output| output.item == target_item)
        })
        .collect::<Vec<_>>();
    loop {
        let mut changed = false;
        for producer in 0..recipes.len() {
            if !active[producer] || reachable[producer] {
                continue;
            }
            if (0..recipes.len()).any(|consumer| {
                active[consumer]
                    && reachable[consumer]
                    && !outputs[producer].is_disjoint(&inputs[consumer])
            }) {
                reachable[producer] = true;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    active
        .iter()
        .zip(reachable)
        .enumerate()
        .filter_map(|(index, (&active, reachable))| (active && !reachable).then_some(index))
        .collect()
}

fn build_model(
    book: &ValidatedRecipeBook,
    target_item: &str,
    facility_count: i32,
    connectivity: ConnectivityMode,
    maximize_target: bool,
    external_policy: ExternalPolicy,
    max_external_items: Option<i32>,
    max_waste_units: Option<i32>,
) -> Result<BuiltModel> {
    let recipes = &book.recipe_book().recipes;
    let items = recipes
        .iter()
        .flat_map(|recipe| recipe.inputs.iter().chain(&recipe.outputs))
        .map(|amount| amount.item.clone())
        .collect::<BTreeSet<_>>();
    let catalog_external = book
        .recipe_book()
        .external_items
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let max_quantity = recipes
        .iter()
        .flat_map(|recipe| recipe.inputs.iter().chain(&recipe.outputs))
        .map(|amount| amount.quantity)
        .max()
        .unwrap_or(1);
    let max_capacity = recipes
        .iter()
        .map(run_capacity)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max()
        .unwrap_or(1);
    let material_bound_i64 = i64::from(facility_count)
        .checked_mul(i64::from(max_capacity))
        .and_then(|value| value.checked_mul(max_quantity))
        .context("material bound overflow")?;
    let material_bound = i32::try_from(material_bound_i64).context("material bound exceeds i32")?;

    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let mut linear_constraints = 0_usize;
    let target_units = solver.new_named_bounded_integer(
        1,
        if maximize_target { material_bound } else { 1 },
        "target-units",
    );
    let recipe_vars = recipes
        .iter()
        .map(|recipe| -> Result<RecipeVars> {
            let capacity = run_capacity(recipe)?;
            let facilities = solver.new_named_bounded_integer(
                0,
                facility_count,
                format!("facilities:{}", recipe.id),
            );
            let runs = solver.new_named_bounded_integer(
                0,
                capacity * facility_count,
                format!("runs:{}", recipe.id),
            );
            let active = solver.new_named_bounded_integer(0, 1, format!("active:{}", recipe.id));
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![runs.scaled(1), facilities.scaled(-capacity)],
                    0,
                    tag,
                ))
                .post();
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![facilities.scaled(capacity), runs.scaled(-1)],
                    capacity - 1,
                    tag,
                ))
                .post();
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![facilities.scaled(1), active.scaled(-facility_count)],
                    0,
                    tag,
                ))
                .post();
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![active.scaled(1), facilities.scaled(-1)],
                    0,
                    tag,
                ))
                .post();
            linear_constraints += 4;
            Ok(RecipeVars {
                facilities,
                runs,
                active,
                capacity,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    solver
        .add_constraint(pumpkin_solver::equals(
            recipe_vars
                .iter()
                .map(|vars| vars.facilities.scaled(1))
                .collect::<Vec<_>>(),
            facility_count,
            tag,
        ))
        .post();
    linear_constraints += 1;

    let item_vars = items
        .iter()
        .map(|item| {
            let external_upper = if item == target_item
                || (external_policy == ExternalPolicy::Catalog
                    && !catalog_external.contains(item.as_str()))
            {
                0
            } else {
                material_bound
            };
            (
                item.clone(),
                ItemVars {
                    external: solver.new_named_bounded_integer(
                        0,
                        external_upper,
                        format!("external:{item}"),
                    ),
                    waste: solver.new_named_bounded_integer(
                        0,
                        material_bound,
                        format!("waste:{item}"),
                    ),
                    external_direction: solver.new_named_bounded_integer(
                        0,
                        1,
                        format!("external-direction:{item}"),
                    ),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    for vars in item_vars.values() {
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                vec![
                    vars.external.scaled(1),
                    vars.external_direction.scaled(-material_bound),
                ],
                0,
                tag,
            ))
            .post();
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                vec![
                    vars.waste.scaled(1),
                    vars.external_direction.scaled(material_bound),
                ],
                material_bound,
                tag,
            ))
            .post();
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                vec![vars.external_direction.scaled(1), vars.external.scaled(-1)],
                0,
                tag,
            ))
            .post();
        linear_constraints += 3;
    }
    if let Some(max_external_items) = max_external_items {
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                item_vars
                    .values()
                    .map(|vars| vars.external_direction.scaled(1))
                    .collect::<Vec<_>>(),
                max_external_items,
                tag,
            ))
            .post();
        linear_constraints += 1;
    }
    if let Some(max_waste_units) = max_waste_units {
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                item_vars
                    .values()
                    .map(|vars| vars.waste.scaled(1))
                    .collect::<Vec<_>>(),
                max_waste_units,
                tag,
            ))
            .post();
        linear_constraints += 1;
    }

    for item in &items {
        let mut terms = vec![
            item_vars[item].external.scaled(1),
            item_vars[item].waste.scaled(-1),
        ];
        if item == target_item {
            terms.push(target_units.scaled(-1));
        }
        for (index, recipe) in recipes.iter().enumerate() {
            let produced = quantity_for(&recipe.outputs, item)?;
            let consumed = quantity_for(&recipe.inputs, item)?;
            let coefficient = produced - consumed;
            if coefficient != 0 {
                terms.push(recipe_vars[index].runs.scaled(coefficient));
            }
        }
        solver
            .add_constraint(pumpkin_solver::equals(terms, 0, tag))
            .post();
        linear_constraints += 1;
    }

    let output_items = recipes
        .iter()
        .map(|recipe| {
            recipe
                .outputs
                .iter()
                .map(|amount| amount.item.as_str())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let input_items = recipes
        .iter()
        .map(|recipe| {
            recipe
                .inputs
                .iter()
                .map(|amount| amount.item.as_str())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();
    let mut relevance = Vec::new();
    if connectivity == ConnectivityMode::Recipe {
        for source in 0..recipes.len() {
            for target in 0..recipes.len() {
                if output_items[source].is_disjoint(&input_items[target]) {
                    continue;
                }
                let flow = solver.new_named_bounded_integer(
                    0,
                    facility_count,
                    format!("relevance:{}->{}", recipes[source].id, recipes[target].id),
                );
                post_flow_activation(
                    &mut solver,
                    flow,
                    recipe_vars[source].active,
                    recipe_vars[target].active,
                    facility_count,
                    tag,
                );
                linear_constraints += 2;
                relevance.push(RelevanceArc {
                    source,
                    target: Some(target),
                    source_name: recipes[source].id.clone(),
                    target_name: Some(recipes[target].id.clone()),
                    flow,
                });
            }
            if output_items[source].contains(target_item) {
                let flow = solver.new_named_bounded_integer(
                    0,
                    facility_count,
                    format!("relevance:{}->target", recipes[source].id),
                );
                solver
                    .add_constraint(pumpkin_solver::less_than_or_equals(
                        vec![
                            flow.scaled(1),
                            recipe_vars[source].active.scaled(-facility_count),
                        ],
                        0,
                        tag,
                    ))
                    .post();
                linear_constraints += 1;
                relevance.push(RelevanceArc {
                    source,
                    target: None,
                    source_name: recipes[source].id.clone(),
                    target_name: None,
                    flow,
                });
            }
        }
        for recipe_index in 0..recipes.len() {
            let mut terms = vec![recipe_vars[recipe_index].active.scaled(-1)];
            for arc in &relevance {
                if arc.source == recipe_index {
                    terms.push(arc.flow.scaled(1));
                }
                if arc.target == Some(recipe_index) {
                    terms.push(arc.flow.scaled(-1));
                }
            }
            solver
                .add_constraint(pumpkin_solver::equals(terms, 0, tag))
                .post();
            linear_constraints += 1;
        }
    } else if connectivity == ConnectivityMode::Scc {
        post_scc_connectivity(
            &mut solver,
            recipes,
            &recipe_vars,
            &output_items,
            &input_items,
            target_item,
            facility_count,
            tag,
            &mut relevance,
            &mut linear_constraints,
        );
    }

    let relevance_arcs = relevance.len();
    let integer_variables = 1 + recipe_vars.len() * 3 + item_vars.len() * 3 + relevance_arcs;
    Ok(BuiltModel {
        solver,
        target_units,
        recipe_vars,
        item_vars,
        relevance,
        summary: ModelSummary {
            recipes: recipes.len(),
            items: items.len(),
            relevance_arcs,
            connectivity,
            external_policy,
            integer_variables,
            linear_constraints,
            lazy_connectivity_cuts: 0,
        },
    })
}

fn post_flow_activation(
    solver: &mut Solver,
    flow: DomainId,
    source_active: DomainId,
    target_active: DomainId,
    bound: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for active in [source_active, target_active] {
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                vec![flow.scaled(1), active.scaled(-bound)],
                0,
                tag,
            ))
            .post();
    }
}

#[allow(clippy::too_many_arguments)]
fn post_scc_connectivity(
    solver: &mut Solver,
    recipes: &[Recipe],
    recipe_vars: &[RecipeVars],
    output_items: &[BTreeSet<&str>],
    input_items: &[BTreeSet<&str>],
    target_item: &str,
    bound: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
    relevance: &mut Vec<RelevanceArc>,
    linear_constraints: &mut usize,
) {
    let adjacency = output_items
        .iter()
        .map(|outputs| {
            input_items
                .iter()
                .enumerate()
                .filter_map(|(target, inputs)| (!outputs.is_disjoint(inputs)).then_some(target))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let (component_of, components) = strongly_connected_components(&adjacency);
    let component_active = components
        .iter()
        .enumerate()
        .map(|(component, _)| {
            solver.new_named_bounded_integer(0, 1, format!("scc-active:{component}"))
        })
        .collect::<Vec<_>>();

    for (component, members) in components.iter().enumerate() {
        for &recipe in members {
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![
                        recipe_vars[recipe].active.scaled(1),
                        component_active[component].scaled(-1),
                    ],
                    0,
                    tag,
                ))
                .post();
            *linear_constraints += 1;
        }
        let mut any_member = vec![component_active[component].scaled(1)];
        any_member.extend(
            members
                .iter()
                .map(|&recipe| recipe_vars[recipe].active.scaled(-1)),
        );
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(any_member, 0, tag))
            .post();
        *linear_constraints += 1;
    }

    let mut component_arcs = BTreeSet::new();
    for (source, targets) in adjacency.iter().enumerate() {
        for &target in targets {
            let source_component = component_of[source];
            let target_component = component_of[target];
            if source_component != target_component {
                component_arcs.insert((source_component, target_component));
            }
        }
    }
    for (source, target) in component_arcs {
        let flow =
            solver.new_named_bounded_integer(0, bound, format!("scc-relevance:{source}->{target}"));
        post_flow_activation(
            solver,
            flow,
            component_active[source],
            component_active[target],
            bound,
            tag,
        );
        *linear_constraints += 2;
        relevance.push(RelevanceArc {
            source,
            target: Some(target),
            source_name: format!("scc:{source}"),
            target_name: Some(format!("scc:{target}")),
            flow,
        });
    }
    let target_components = recipes
        .iter()
        .enumerate()
        .filter(|(_, recipe)| {
            recipe
                .outputs
                .iter()
                .any(|output| output.item == target_item)
        })
        .map(|(recipe, _)| component_of[recipe])
        .collect::<BTreeSet<_>>();
    for component in target_components {
        let flow = solver.new_named_bounded_integer(
            0,
            bound,
            format!("scc-relevance:{component}->target"),
        );
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                vec![flow.scaled(1), component_active[component].scaled(-bound)],
                0,
                tag,
            ))
            .post();
        *linear_constraints += 1;
        relevance.push(RelevanceArc {
            source: component,
            target: None,
            source_name: format!("scc:{component}"),
            target_name: None,
            flow,
        });
    }
    for component in 0..components.len() {
        let mut terms = vec![component_active[component].scaled(-1)];
        for arc in relevance.iter() {
            if arc.source == component {
                terms.push(arc.flow.scaled(1));
            }
            if arc.target == Some(component) {
                terms.push(arc.flow.scaled(-1));
            }
        }
        solver
            .add_constraint(pumpkin_solver::equals(terms, 0, tag))
            .post();
        *linear_constraints += 1;
    }
}

fn strongly_connected_components(adjacency: &[Vec<usize>]) -> (Vec<usize>, Vec<Vec<usize>>) {
    struct Tarjan<'a> {
        adjacency: &'a [Vec<usize>],
        next_index: usize,
        indices: Vec<Option<usize>>,
        lowlinks: Vec<usize>,
        stack: Vec<usize>,
        on_stack: Vec<bool>,
        component_of: Vec<usize>,
        components: Vec<Vec<usize>>,
    }

    impl Tarjan<'_> {
        fn visit(&mut self, node: usize) {
            let index = self.next_index;
            self.next_index += 1;
            self.indices[node] = Some(index);
            self.lowlinks[node] = index;
            self.stack.push(node);
            self.on_stack[node] = true;
            for &neighbor in &self.adjacency[node] {
                if self.indices[neighbor].is_none() {
                    self.visit(neighbor);
                    self.lowlinks[node] = self.lowlinks[node].min(self.lowlinks[neighbor]);
                } else if self.on_stack[neighbor] {
                    self.lowlinks[node] = self.lowlinks[node].min(
                        self.indices[neighbor].expect("a node on the Tarjan stack has an index"),
                    );
                }
            }
            if self.lowlinks[node] == index {
                let component = self.components.len();
                let mut members = Vec::new();
                loop {
                    let member = self.stack.pop().expect("SCC root remains on the stack");
                    self.on_stack[member] = false;
                    self.component_of[member] = component;
                    members.push(member);
                    if member == node {
                        break;
                    }
                }
                self.components.push(members);
            }
        }
    }

    let mut tarjan = Tarjan {
        adjacency,
        next_index: 0,
        indices: vec![None; adjacency.len()],
        lowlinks: vec![0; adjacency.len()],
        stack: Vec::new(),
        on_stack: vec![false; adjacency.len()],
        component_of: vec![0; adjacency.len()],
        components: Vec::new(),
    };
    for node in 0..adjacency.len() {
        if tarjan.indices[node].is_none() {
            tarjan.visit(node);
        }
    }
    (tarjan.component_of, tarjan.components)
}

fn run_capacity(recipe: &Recipe) -> Result<i32> {
    ensure!(
        HORIZON_MS % recipe.duration_ms == 0,
        "recipe '{}' duration {} does not divide probe horizon {HORIZON_MS}",
        recipe.id,
        recipe.duration_ms
    );
    i32::try_from(HORIZON_MS / recipe.duration_ms).context("run capacity exceeds i32")
}

fn quantity_for(amounts: &[aic_data::recipes::ItemAmount], item: &str) -> Result<i32> {
    let quantity = amounts
        .iter()
        .filter(|amount| amount.item == item)
        .try_fold(0_i64, |sum, amount| sum.checked_add(amount.quantity))
        .context("item quantity overflow")?;
    i32::try_from(quantity).context("item quantity exceeds i32")
}

fn extract_solution(
    solution: &(impl ProblemSolution + ?Sized),
    recipes: &[Recipe],
    recipe_vars: &[RecipeVars],
    item_vars: &BTreeMap<String, ItemVars>,
    relevance: &[RelevanceArc],
    target_units: DomainId,
) -> SolutionSummary {
    let active_recipes = recipes
        .iter()
        .zip(recipe_vars)
        .filter_map(|(recipe, vars)| {
            let facilities = solution.get_integer_value(vars.facilities);
            (facilities > 0).then(|| ActiveRecipe {
                recipe: recipe.id.clone(),
                facility: recipe.facility.clone(),
                facilities,
                run_units: solution.get_integer_value(vars.runs),
                run_capacity_per_facility: vars.capacity,
            })
        })
        .collect::<Vec<_>>();
    let external_inputs = positive_item_units(solution, item_vars, |vars| vars.external);
    let waste_outputs = positive_item_units(solution, item_vars, |vars| vars.waste);
    let active_relevance = relevance
        .iter()
        .filter_map(|arc| {
            let units = solution.get_integer_value(arc.flow);
            (units > 0).then(|| ActiveRelevanceArc {
                source_recipe: arc.source_name.clone(),
                target_recipe: arc.target_name.clone(),
                units,
            })
        })
        .collect();
    SolutionSummary {
        facility_count: active_recipes.iter().map(|recipe| recipe.facilities).sum(),
        active_recipe_count: active_recipes.len(),
        total_run_units: active_recipes
            .iter()
            .map(|recipe| i64::from(recipe.run_units))
            .sum(),
        total_external_units: external_inputs
            .iter()
            .map(|item| i64::from(item.units))
            .sum(),
        total_waste_units: waste_outputs.iter().map(|item| i64::from(item.units)).sum(),
        target_units: solution.get_integer_value(target_units),
        recipes: active_recipes,
        external_inputs,
        waste_outputs,
        relevance_arcs: active_relevance,
    }
}

fn positive_item_units(
    solution: &(impl ProblemSolution + ?Sized),
    item_vars: &BTreeMap<String, ItemVars>,
    select: impl Fn(&ItemVars) -> DomainId,
) -> Vec<ItemUnits> {
    item_vars
        .iter()
        .filter_map(|(item, vars)| {
            let units = solution.get_integer_value(select(vars));
            (units > 0).then(|| ItemUnits {
                item: item.clone(),
                units,
            })
        })
        .collect()
}
