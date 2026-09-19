//! Seeded recipe-derived pairwise problem preparation.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use aic_data::recipes::{Recipe, ValidatedRecipeBook, load_recipe_book};
use aic_layout::{
    facilities::FacilityPortDirection,
    logistics::{TransportKind, ValidatedItemCatalog, load_item_catalog},
    physical::{Block, Connection, Problem},
    rate::Rate,
};
use aic_pilot_native::constructive::Session;
use aic_torch::Generator;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::input::sha256;

use super::run::{Config, load_catalogs, read_json, write_json};

pub const PROBLEM_SET_SCHEMA: &str = "aic-constructive-problem-set-v1";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProblemSet {
    pub schema: String,
    pub training: Vec<Problem>,
    pub evaluation: Vec<Problem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Edge {
    source: usize,
    target: usize,
    commodity: String,
    transport: TransportKind,
    source_quantity: i64,
    target_quantity: i64,
}

pub fn prepare(
    config_path: &Path,
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
    let config: Config = read_json(config_path)?;
    ensure!(
        config.schema == "aic-constructive-training-v2",
        "aic-learning.constructive-problems: unsupported training config schema"
    );
    let recipe_path = catalog_root.join("recipes.json");
    let item_path = catalog_root.join("items.json");
    let recipes = ValidatedRecipeBook::try_from_recipe_book(load_recipe_book(&recipe_path)?)
        .map_err(|report| anyhow::anyhow!("aic-learning.constructive-recipes: {report:?}"))?;
    let items = ValidatedItemCatalog::try_from_catalog(load_item_catalog(&item_path)?)
        .map_err(|report| anyhow::anyhow!("aic-learning.constructive-items: {report:?}"))?;
    let catalogs = load_catalogs(catalog_root)?;
    let groups = dependency_groups(recipes.recipe_book().recipes.as_slice(), &items)?;
    ensure!(
        training
            .checked_add(evaluation)
            .is_some_and(|count| count <= groups.len()),
        "aic-learning.constructive-problems: requested {training} training and {evaluation} evaluation problems from only {} eligible target recipes",
        groups.len()
    );
    validate_edges(
        &groups,
        recipes.recipe_book().recipes.as_slice(),
        &items,
        &catalogs.facilities,
    )?;

    let mut generator = Generator::new(seed)?;
    let selected = select_edges(&groups, training + evaluation, &mut generator)?;
    let mut prepared = selected
        .iter()
        .map(|edge| {
            problem(
                edge,
                recipes.recipe_book().recipes.as_slice(),
                &catalogs.transports,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let evaluation_problems = prepared.split_off(training);
    let training_problems = prepared;
    for problem in training_problems.iter().chain(&evaluation_problems) {
        let session = Session::new(problem.clone(), catalogs.clone(), config.native)
            .map_err(anyhow::Error::new)?;
        session
            .learning_observation(&config.domain)
            .map_err(anyhow::Error::new)?;
    }
    let problem_set = ProblemSet {
        schema: PROBLEM_SET_SCHEMA.into(),
        training: training_problems,
        evaluation: evaluation_problems,
    };
    write_json(output, &problem_set)?;

    let chosen_targets = selected
        .iter()
        .map(|edge| edge.target)
        .collect::<BTreeSet<_>>();
    let selected_facility_pairs = selected
        .iter()
        .map(|edge| {
            let source = &recipes.recipe_book().recipes[edge.source];
            let target = &recipes.recipe_book().recipes[edge.target];
            (source.facility.as_str(), target.facility.as_str())
        })
        .collect::<BTreeSet<_>>();
    let selected_commodities = selected
        .iter()
        .map(|edge| edge.commodity.as_str())
        .collect::<BTreeSet<_>>();
    let transport_counts = problem_set
        .training
        .iter()
        .chain(&problem_set.evaluation)
        .fold(BTreeMap::<String, usize>::new(), |mut counts, problem| {
            *counts
                .entry(format!("{:?}", problem.connections[0].transport).to_lowercase())
                .or_default() += 1;
            counts
        });
    let capacity_limited = selected
        .iter()
        .filter(|edge| {
            edge_rate(edge, recipes.recipe_book().recipes.as_slice()).is_ok_and(|rate| {
                let capacity = catalogs.transports.capacity(edge.transport);
                Rate::from_quantity_per_duration_ms(capacity.quantity, capacity.duration_ms)
                    .is_ok_and(|capacity| rate > capacity)
            })
        })
        .count();
    Ok(json!({
        "schema":"aic-constructive-problem-preparation-report-v1",
        "status":"completed",
        "seed":seed,
        "candidate_dependency_edges":groups.values().map(Vec::len).sum::<usize>(),
        "eligible_target_recipes":groups.len(),
        "selected_target_recipes":chosen_targets.len(),
        "training_problems":problem_set.training.len(),
        "evaluation_problems":problem_set.evaluation.len(),
        "selected_facility_pairs":selected_facility_pairs.len(),
        "selected_commodities":selected_commodities.len(),
        "transport_counts":transport_counts,
        "capacity_limited_connections":capacity_limited,
        "recipes_sha256":sha256(&recipe_path)?,
        "items_sha256":sha256(&item_path)?,
        "output_sha256":sha256(output)?,
        "output":output,
    }))
}

fn dependency_groups(
    recipes: &[Recipe],
    items: &ValidatedItemCatalog,
) -> Result<BTreeMap<usize, Vec<Edge>>> {
    let mut producers = BTreeMap::<&str, Vec<(usize, i64)>>::new();
    for (source, recipe) in recipes.iter().enumerate() {
        for output in &recipe.outputs {
            producers
                .entry(output.item.as_str())
                .or_default()
                .push((source, output.quantity));
        }
    }
    let mut groups = BTreeMap::<usize, Vec<Edge>>::new();
    for (target, recipe) in recipes.iter().enumerate() {
        for input in &recipe.inputs {
            for &(source, source_quantity) in
                producers.get(input.item.as_str()).into_iter().flatten()
            {
                let transport = items
                    .item(&input.item)
                    .with_context(|| {
                        format!(
                            "aic-learning.constructive-problems: missing item {}",
                            input.item
                        )
                    })?
                    .transport;
                groups.entry(target).or_default().push(Edge {
                    source,
                    target,
                    commodity: input.item.clone(),
                    transport,
                    source_quantity,
                    target_quantity: input.quantity,
                });
            }
        }
    }
    ensure!(
        !groups.is_empty(),
        "aic-learning.constructive-problems: recipe catalog has no internal dependency edges"
    );
    Ok(groups)
}

fn validate_edges(
    groups: &BTreeMap<usize, Vec<Edge>>,
    recipes: &[Recipe],
    items: &ValidatedItemCatalog,
    facilities: &aic_layout::facilities::ValidatedFacilityCatalog,
) -> Result<()> {
    for edge in groups.values().flatten() {
        let source = &recipes[edge.source];
        let target = &recipes[edge.target];
        let item = items.item(&edge.commodity).with_context(|| {
            format!(
                "aic-learning.constructive-problems: missing item {}",
                edge.commodity
            )
        })?;
        for (recipe, direction) in [
            (source, FacilityPortDirection::Output),
            (target, FacilityPortDirection::Input),
        ] {
            let facility = facilities.facility(&recipe.facility).with_context(|| {
                format!(
                    "aic-learning.constructive-problems: recipe {} uses missing facility {}",
                    recipe.id, recipe.facility
                )
            })?;
            ensure!(
                facility
                    .ports
                    .iter()
                    .any(|port| port.direction == direction && port.transport == item.transport),
                "aic-learning.constructive-problems: recipe {} lacks a {:?} {:?} port for {}",
                recipe.id,
                direction,
                item.transport,
                edge.commodity
            );
        }
    }
    Ok(())
}

fn select_edges(
    groups: &BTreeMap<usize, Vec<Edge>>,
    count: usize,
    generator: &mut Generator,
) -> Result<Vec<Edge>> {
    let targets = groups.keys().copied().collect::<Vec<_>>();
    let order = Vec::<i64>::try_from(generator.permutation(targets.len() as i64)?)?;
    order
        .into_iter()
        .take(count)
        .map(|position| {
            let edges = &groups[&targets[usize::try_from(position)?]];
            let selected = i64::try_from(generator.integers(edges.len() as i64, 1)?)?;
            Ok(edges[usize::try_from(selected)?].clone())
        })
        .collect()
}

fn edge_rate(edge: &Edge, recipes: &[Recipe]) -> Result<Rate> {
    let source = &recipes[edge.source];
    let target = &recipes[edge.target];
    let source_rate = Rate::from_quantity_per_duration_ms(edge.source_quantity, source.duration_ms)
        .map_err(|diagnostic| anyhow::anyhow!(diagnostic.message))?;
    let target_rate = Rate::from_quantity_per_duration_ms(edge.target_quantity, target.duration_ms)
        .map_err(|diagnostic| anyhow::anyhow!(diagnostic.message))?;
    Ok(source_rate.min(target_rate))
}

fn problem(
    edge: &Edge,
    recipes: &[Recipe],
    transports: &aic_layout::logistics::ValidatedTransportCatalog,
) -> Result<Problem> {
    let source = &recipes[edge.source];
    let target = &recipes[edge.target];
    let capacity = transports.capacity(edge.transport);
    let capacity_rate =
        Rate::from_quantity_per_duration_ms(capacity.quantity, capacity.duration_ms)
            .map_err(|diagnostic| anyhow::anyhow!(diagnostic.message))?;
    let source_id = format!("{}-source", source.id);
    let target_id = format!("{}-target", target.id);
    Ok(Problem {
        blocks: vec![
            Block {
                id: source_id.clone(),
                facility: source.facility.clone(),
            },
            Block {
                id: target_id.clone(),
                facility: target.facility.clone(),
            },
        ],
        connections: vec![Connection {
            id: format!("{}-flow", edge.commodity),
            source: source_id,
            target: target_id,
            commodity: edge.commodity.clone(),
            transport: edge.transport,
            rate: edge_rate(edge, recipes)?.min(capacity_rate),
        }],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_selection_uses_distinct_target_recipe_groups() {
        let groups = (0..8)
            .map(|target| {
                (
                    target,
                    vec![Edge {
                        source: target + 10,
                        target,
                        commodity: format!("item-{target}"),
                        transport: TransportKind::Belt,
                        source_quantity: 1,
                        target_quantity: 1,
                    }],
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut first = Generator::new(71).unwrap();
        let mut repeated = Generator::new(71).unwrap();
        let selected = select_edges(&groups, 6, &mut first).unwrap();
        assert_eq!(selected, select_edges(&groups, 6, &mut repeated).unwrap());
        assert_eq!(
            selected
                .iter()
                .map(|edge| edge.target)
                .collect::<BTreeSet<_>>()
                .len(),
            6
        );
    }
}
