use std::collections::{BTreeMap, BTreeSet};

use good_lp::{Expression, ProblemVariables, Solution, SolverModel, microlp, variable};

use crate::recipes::{ItemRate, RecipeGraph, RecipeRunRate, RecipeThroughputRequest};

use super::{
    Rate, RecipeThroughputReport, ThroughputDiagnostic, add_input_demands, item_rates_from_map,
    multiply_amounts,
};

pub(super) fn bootstrap_items(graph: &RecipeGraph) -> Option<Vec<String>> {
    let producer_by_item = graph
        .recipes
        .iter()
        .enumerate()
        .flat_map(|(recipe_index, recipe)| {
            recipe
                .outputs
                .iter()
                .map(move |output| (output.item.as_str(), recipe_index))
        })
        .collect::<BTreeMap<_, _>>();
    let adjacency = graph
        .recipes
        .iter()
        .map(|recipe| {
            recipe
                .inputs
                .iter()
                .filter_map(|input| producer_by_item.get(input.item.as_str()).copied())
                .collect::<BTreeSet<_>>()
        })
        .collect::<Vec<_>>();

    let mut cyclic_items = BTreeSet::new();
    for (consumer_index, recipe) in graph.recipes.iter().enumerate() {
        for input in &recipe.inputs {
            let Some(&producer_index) = producer_by_item.get(input.item.as_str()) else {
                continue;
            };
            if reachable(&adjacency, producer_index, consumer_index) {
                cyclic_items.insert(input.item.clone());
            }
        }
    }

    (!cyclic_items.is_empty()).then(|| cyclic_items.into_iter().collect())
}

fn reachable(adjacency: &[BTreeSet<usize>], start: usize, target: usize) -> bool {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if current == target {
            return true;
        }
        if seen.insert(current) {
            pending.extend(adjacency[current].iter().copied());
        }
    }
    false
}

pub(super) fn calculate_cyclic_throughput(
    graph: &RecipeGraph,
    request: &RecipeThroughputRequest,
    target_rate: Rate,
    bootstrap_item_options: Vec<String>,
) -> RecipeThroughputReport {
    let mut variables = ProblemVariables::new();
    let recipe_variables = graph
        .recipes
        .iter()
        .map(|recipe| variables.add(variable().min(0).name(&recipe.id)))
        .collect::<Vec<_>>();
    let objective = graph.recipes.iter().enumerate().fold(
        Expression::from(0.0),
        |objective, (index, recipe)| {
            objective + (recipe.duration_ms as f64 / 1000.0) * recipe_variables[index]
        },
    );
    let external_items = graph
        .external_items
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut balanced_items = BTreeSet::new();
    for recipe in &graph.recipes {
        balanced_items.extend(recipe.inputs.iter().map(|amount| amount.item.as_str()));
        balanced_items.extend(recipe.outputs.iter().map(|amount| amount.item.as_str()));
    }
    balanced_items.remove(request.target.item.as_str());
    balanced_items.insert(request.target.item.as_str());

    let mut problem = variables.minimise(objective).using(microlp);
    for item in balanced_items {
        if external_items.contains(item) {
            continue;
        }
        let balance = graph.recipes.iter().enumerate().fold(
            Expression::from(0.0),
            |balance, (index, recipe)| {
                let produced = recipe
                    .outputs
                    .iter()
                    .filter(|amount| amount.item == item)
                    .map(|amount| amount.quantity)
                    .sum::<i64>();
                let consumed = recipe
                    .inputs
                    .iter()
                    .filter(|amount| amount.item == item)
                    .map(|amount| amount.quantity)
                    .sum::<i64>();
                balance + (produced - consumed) as f64 * recipe_variables[index]
            },
        );
        let demand = if item == request.target.item {
            rate_as_f64(target_rate)
        } else {
            0.0
        };
        problem = problem.with(balance.geq(demand));
    }

    let solution = match problem.solve() {
        Ok(solution) => solution,
        Err(error) => {
            return RecipeThroughputReport::failure(ThroughputDiagnostic::error(
                "cyclic-throughput-infeasible",
                "/recipes",
                Some(request.target.item.clone()),
                format!("cyclic material-balance model failed: {error}"),
            ));
        }
    };

    let mut recipe_rates = Vec::new();
    let mut item_demands = BTreeMap::from([(request.target.item.clone(), target_rate)]);
    let mut produced_rates = BTreeMap::<String, Rate>::new();

    for (index, recipe) in graph.recipes.iter().enumerate() {
        let runs_per_second = match approximate_rate(solution.value(recipe_variables[index])) {
            Ok(rate) => rate,
            Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
        };
        if runs_per_second.is_zero() {
            continue;
        }
        let input_rates = match multiply_amounts(&recipe.inputs, runs_per_second) {
            Ok(rates) => rates,
            Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
        };
        let output_rates = match multiply_amounts(&recipe.outputs, runs_per_second) {
            Ok(rates) => rates,
            Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
        };
        if let Err(diagnostic) = add_input_demands(&mut item_demands, &input_rates) {
            return RecipeThroughputReport::failure(diagnostic);
        }
        for output in &output_rates {
            let current = produced_rates
                .get(&output.item)
                .copied()
                .unwrap_or_else(Rate::zero);
            let next = match current.checked_add(output.rate) {
                Ok(rate) => rate,
                Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
            };
            produced_rates.insert(output.item.clone(), next);
        }
        let work_seconds_per_second =
            match runs_per_second.checked_work_seconds_per_second(recipe.duration_ms) {
                Ok(rate) => rate,
                Err(diagnostic) => return RecipeThroughputReport::failure(diagnostic),
            };
        recipe_rates.push(RecipeRunRate {
            recipe: recipe.id.clone(),
            facility: recipe.facility.clone(),
            runs_per_second,
            work_seconds_per_second,
            limiting_outputs: Vec::new(),
            input_rates,
            output_rates,
        });
    }

    let mut surplus_rates = Vec::new();
    for (item, produced) in &produced_rates {
        let demand = item_demands.get(item).copied().unwrap_or_else(Rate::zero);
        let surplus = match produced.checked_sub(demand) {
            Ok(rate) => rate,
            Err(_) => {
                return RecipeThroughputReport::failure(ThroughputDiagnostic::error(
                    "cyclic-rate-rounding-error",
                    "/recipes",
                    Some(item.clone()),
                    "rationalized cyclic recipe rates violate material balance",
                ));
            }
        };
        if !surplus.is_zero() {
            surplus_rates.push(ItemRate {
                item: item.clone(),
                rate: surplus,
            });
        }
    }

    RecipeThroughputReport {
        success: true,
        target: Some(ItemRate {
            item: request.target.item.clone(),
            rate: target_rate,
        }),
        recipe_rates,
        external_input_rates: graph
            .external_items
            .iter()
            .filter_map(|item| {
                item_demands.get(item).map(|rate| ItemRate {
                    item: item.clone(),
                    rate: *rate,
                })
            })
            .collect(),
        item_demand_rates: item_rates_from_map(&item_demands),
        surplus_rates,
        bootstrap_item_options,
        diagnostics: vec![ThroughputDiagnostic::info(
            "cyclic-throughput-calculated",
            "/",
            Some(request.target.item.clone()),
            "cyclic recipe rates were calculated from steady-state material balances; bootstrap_item_options can initialize the loop but are not continuous inputs",
        )],
    }
}

fn rate_as_f64(rate: Rate) -> f64 {
    rate.numerator as f64 / rate.denominator as f64
}

fn approximate_rate(value: f64) -> Result<Rate, ThroughputDiagnostic> {
    if !value.is_finite() || value < -1e-9 {
        return Err(ThroughputDiagnostic::error(
            "invalid-cyclic-rate",
            "/recipes",
            None,
            format!("cyclic solver returned invalid recipe rate {value}"),
        ));
    }
    if value.abs() <= 1e-9 {
        return Ok(Rate::zero());
    }

    let max_denominator = 1_000_000_i128;
    let mut remainder = value;
    let (mut previous_numerator, mut numerator) = (0_i128, 1_i128);
    let (mut previous_denominator, mut denominator) = (1_i128, 0_i128);

    for _ in 0..64 {
        let integer = remainder.floor() as i128;
        let next_numerator = integer * numerator + previous_numerator;
        let next_denominator = integer * denominator + previous_denominator;
        if next_denominator > max_denominator {
            break;
        }
        previous_numerator = numerator;
        numerator = next_numerator;
        previous_denominator = denominator;
        denominator = next_denominator;

        let approximation = numerator as f64 / denominator as f64;
        if (approximation - value).abs() <= 1e-9 {
            break;
        }
        let fraction = remainder - integer as f64;
        if fraction.abs() <= f64::EPSILON {
            break;
        }
        remainder = 1.0 / fraction;
    }

    Rate::normalize(numerator, denominator, "cyclic-rate-rationalization")
}
