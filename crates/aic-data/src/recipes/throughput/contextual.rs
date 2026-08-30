use std::collections::{BTreeMap, BTreeSet};

use good_lp::{Expression, ProblemVariables, Solution, SolverModel, microlp, variable};
use serde::Serialize;

use crate::recipes::{
    ContextualProductionEdge, ContextualProductionFlowKind, ContextualProductionGraphReport,
    ContextualProductionNode, ItemRate, Recipe, RecipeSourcePlanRequest, ValidatedRecipeBook,
    build_contextual_production_graph,
};

use super::{Rate, ThroughputDiagnostic, cyclic, multiply_amounts};

const STAGE: &str = "contextual-throughput";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextualRecipeRunRate {
    pub occurrence: String,
    pub path: String,
    pub recipe: String,
    pub facility: String,
    pub runs_per_second: Rate,
    pub work_seconds_per_second: Rate,
    pub limiting_outputs: Vec<String>,
    pub input_rates: Vec<ItemRate>,
    pub output_rates: Vec<ItemRate>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextualMaterialFlowRate {
    pub edge: String,
    pub path: String,
    pub source: String,
    pub target: String,
    pub kind: ContextualProductionFlowKind,
    pub item: String,
    pub rate: Rate,
    pub cycle_to_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextualExternalInputRate {
    pub occurrence: String,
    pub path: String,
    pub item: String,
    pub rate: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextualSurplusRate {
    pub occurrence: String,
    pub path: String,
    pub item: String,
    pub rate: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextualThroughputReport {
    pub success: bool,
    pub target: Option<ItemRate>,
    pub recipe_rates: Vec<ContextualRecipeRunRate>,
    pub flow_rates: Vec<ContextualMaterialFlowRate>,
    pub external_input_rates: Vec<ContextualExternalInputRate>,
    pub surplus_rates: Vec<ContextualSurplusRate>,
    pub bootstrap_item_options: Vec<String>,
    pub required_selection_paths: Vec<String>,
    pub diagnostics: Vec<ContextualThroughputDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextualThroughputDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl ContextualThroughputReport {
    fn failure(diagnostic: ContextualThroughputDiagnostic) -> Self {
        Self::failure_with_paths(diagnostic, Vec::new())
    }

    fn failure_with_paths(
        diagnostic: ContextualThroughputDiagnostic,
        required_selection_paths: Vec<String>,
    ) -> Self {
        Self {
            success: false,
            target: None,
            recipe_rates: Vec::new(),
            flow_rates: Vec::new(),
            external_input_rates: Vec::new(),
            surplus_rates: Vec::new(),
            bootstrap_item_options: Vec::new(),
            required_selection_paths,
            diagnostics: vec![diagnostic],
        }
    }
}

impl ContextualThroughputDiagnostic {
    fn error(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "error",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }

    fn info(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }

    fn from_rate_error(error: ThroughputDiagnostic) -> Self {
        Self::error(error.code, error.path, error.entity, error.message)
    }
}

impl ValidatedRecipeBook {
    pub fn calculate_contextual_throughput(
        &self,
        request: &RecipeSourcePlanRequest,
    ) -> ContextualThroughputReport {
        let graph = build_contextual_production_graph(self, request);
        if !graph.success {
            return ContextualThroughputReport::failure_with_paths(
                ContextualThroughputDiagnostic::error(
                    "contextual-production-graph-failed",
                    "/",
                    Some(request.target.item.clone()),
                    "contextual throughput requires a successful contextual production graph",
                ),
                graph.required_selection_paths,
            );
        }

        calculate_graph_throughput(&graph)
    }
}

fn calculate_graph_throughput(
    graph: &ContextualProductionGraphReport,
) -> ContextualThroughputReport {
    let Some(target) = graph.target.as_ref() else {
        return ContextualThroughputReport::failure(ContextualThroughputDiagnostic::error(
            "missing-target",
            "/target",
            None,
            "successful contextual production graph must contain a target",
        ));
    };
    let target_rate = match Rate::from_quantity_per_duration_ms(target.quantity, target.duration_ms)
    {
        Ok(rate) => rate,
        Err(error) => {
            return ContextualThroughputReport::failure(
                ContextualThroughputDiagnostic::from_rate_error(error),
            );
        }
    };

    let structure = match GraphStructure::new(graph) {
        Ok(structure) => structure,
        Err(diagnostic) => return ContextualThroughputReport::failure(diagnostic),
    };
    let recipe_rates = match solve_recipe_rates(&structure, target_rate) {
        Ok(rates) => rates,
        Err(diagnostic) => return ContextualThroughputReport::failure(diagnostic),
    };
    let flow_rates = match derive_flow_rates(&structure, &recipe_rates, target_rate) {
        Ok(rates) => rates,
        Err(diagnostic) => return ContextualThroughputReport::failure(diagnostic),
    };
    let (recipe_run_rates, surplus_rates) =
        match build_recipe_reports(&structure, &recipe_rates, &flow_rates) {
            Ok(reports) => reports,
            Err(diagnostic) => return ContextualThroughputReport::failure(diagnostic),
        };
    let external_input_rates = match build_external_input_rates(&structure, &flow_rates) {
        Ok(rates) => rates,
        Err(diagnostic) => return ContextualThroughputReport::failure(diagnostic),
    };

    ContextualThroughputReport {
        success: true,
        target: Some(ItemRate {
            item: target.item.clone(),
            rate: target_rate,
        }),
        recipe_rates: recipe_run_rates,
        flow_rates,
        external_input_rates,
        surplus_rates,
        bootstrap_item_options: bootstrap_items(&structure),
        required_selection_paths: Vec::new(),
        diagnostics: vec![ContextualThroughputDiagnostic::info(
            "contextual-throughput-calculated",
            "/",
            Some(target.item.clone()),
            "contextual recipe occurrence and material flow rates were calculated",
        )],
    }
}

struct RecipeOccurrence<'a> {
    id: &'a str,
    path: &'a str,
    recipe: &'a Recipe,
}

struct ExternalOccurrence<'a> {
    id: &'a str,
    path: &'a str,
    item: &'a str,
}

struct GraphStructure<'a> {
    recipes: Vec<RecipeOccurrence<'a>>,
    recipes_by_id: BTreeMap<&'a str, usize>,
    externals: Vec<ExternalOccurrence<'a>>,
    target_id: &'a str,
    edges: &'a [ContextualProductionEdge],
}

impl<'a> GraphStructure<'a> {
    fn new(
        graph: &'a ContextualProductionGraphReport,
    ) -> Result<Self, ContextualThroughputDiagnostic> {
        let mut recipes = Vec::new();
        let mut recipes_by_id = BTreeMap::new();
        let mut externals = Vec::new();
        let mut target_id = None;

        for node in &graph.nodes {
            match node {
                ContextualProductionNode::RecipeOccurrence { id, path, recipe } => {
                    if recipes_by_id.insert(id.as_str(), recipes.len()).is_some() {
                        return Err(duplicate_node(id));
                    }
                    recipes.push(RecipeOccurrence { id, path, recipe });
                }
                ContextualProductionNode::ExternalInput { id, path, item } => {
                    externals.push(ExternalOccurrence { id, path, item });
                }
                ContextualProductionNode::Target { id, .. } => {
                    if target_id.replace(id.as_str()).is_some() {
                        return Err(ContextualThroughputDiagnostic::error(
                            "duplicate-target-node",
                            "/nodes",
                            Some(id.clone()),
                            "contextual production graph contains more than one target node",
                        ));
                    }
                }
            }
        }
        let Some(target_id) = target_id else {
            return Err(ContextualThroughputDiagnostic::error(
                "missing-target-node",
                "/nodes",
                None,
                "contextual production graph contains no target node",
            ));
        };

        let structure = Self {
            recipes,
            recipes_by_id,
            externals,
            target_id,
            edges: &graph.edges,
        };
        structure.validate()?;
        Ok(structure)
    }

    fn validate(&self) -> Result<(), ContextualThroughputDiagnostic> {
        let target_edges = self
            .edges
            .iter()
            .filter(|edge| edge.target == self.target_id)
            .count();
        if target_edges != 1 {
            return Err(ContextualThroughputDiagnostic::error(
                "invalid-target-edge-count",
                "/edges",
                Some(self.target_id.to_string()),
                format!("target node must have exactly one incoming edge, found {target_edges}"),
            ));
        }

        for occurrence in &self.recipes {
            for input in &occurrence.recipe.inputs {
                let count = self
                    .edges
                    .iter()
                    .filter(|edge| edge.target == occurrence.id && edge.item == input.item)
                    .count();
                if count != 1 {
                    return Err(ContextualThroughputDiagnostic::error(
                        "invalid-recipe-input-edge-count",
                        "/edges",
                        Some(occurrence.id.to_string()),
                        format!(
                            "recipe occurrence '{}' input '{}' must have exactly one edge, found {count}",
                            occurrence.id, input.item
                        ),
                    ));
                }
            }
            for edge in self
                .edges
                .iter()
                .filter(|edge| edge.source == occurrence.id)
            {
                if !occurrence
                    .recipe
                    .outputs
                    .iter()
                    .any(|output| output.item == edge.item)
                {
                    return Err(ContextualThroughputDiagnostic::error(
                        "recipe-edge-output-mismatch",
                        &edge.path,
                        Some(edge.id.clone()),
                        format!(
                            "recipe occurrence '{}' does not output item '{}'",
                            occurrence.id, edge.item
                        ),
                    ));
                }
            }
        }

        for external in &self.externals {
            let count = self
                .edges
                .iter()
                .filter(|edge| edge.source == external.id && edge.item == external.item)
                .count();
            if count != 1 {
                return Err(ContextualThroughputDiagnostic::error(
                    "invalid-external-edge-count",
                    "/edges",
                    Some(external.id.to_string()),
                    format!(
                        "external occurrence '{}' must have exactly one matching outgoing edge, found {count}",
                        external.id
                    ),
                ));
            }
        }

        Ok(())
    }
}

fn duplicate_node(id: &str) -> ContextualThroughputDiagnostic {
    ContextualThroughputDiagnostic::error(
        "duplicate-recipe-occurrence-node",
        "/nodes",
        Some(id.to_string()),
        format!("recipe occurrence node '{id}' appears more than once"),
    )
}

fn solve_recipe_rates(
    structure: &GraphStructure<'_>,
    target_rate: Rate,
) -> Result<BTreeMap<String, Rate>, ContextualThroughputDiagnostic> {
    let mut variables = ProblemVariables::new();
    let recipe_variables = structure
        .recipes
        .iter()
        .map(|occurrence| {
            (
                occurrence.id,
                variables.add(variable().min(0).name(occurrence.id)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let edge_variables = structure
        .edges
        .iter()
        .map(|edge| {
            (
                edge.id.as_str(),
                variables.add(variable().min(0).name(&edge.id)),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let objective =
        structure
            .recipes
            .iter()
            .fold(Expression::from(0.0), |objective, occurrence| {
                objective
                    + (occurrence.recipe.duration_ms as f64 / 1000.0)
                        * recipe_variables[occurrence.id]
            });
    let mut problem = variables.minimise(objective).using(microlp);

    for edge in structure
        .edges
        .iter()
        .filter(|edge| edge.target == structure.target_id)
    {
        problem = problem.with(
            Expression::from(edge_variables[edge.id.as_str()]).eq(cyclic::rate_as_f64(target_rate)),
        );
    }

    for occurrence in &structure.recipes {
        let recipe_variable = recipe_variables[occurrence.id];
        for input in &occurrence.recipe.inputs {
            let edge = structure
                .edges
                .iter()
                .find(|edge| edge.target == occurrence.id && edge.item == input.item)
                .expect("graph structure validation found every recipe input edge");
            problem = problem.with(
                (Expression::from(edge_variables[edge.id.as_str()])
                    - input.quantity as f64 * recipe_variable)
                    .eq(0.0),
            );
        }
        for output in &occurrence.recipe.outputs {
            let outgoing = structure
                .edges
                .iter()
                .filter(|edge| edge.source == occurrence.id && edge.item == output.item)
                .fold(Expression::from(0.0), |sum, edge| {
                    sum + edge_variables[edge.id.as_str()]
                });
            problem = problem.with(outgoing.leq(output.quantity as f64 * recipe_variable));
        }
    }

    let solution = problem.solve().map_err(|error| {
        ContextualThroughputDiagnostic::error(
            "contextual-throughput-infeasible",
            "/edges",
            None,
            format!("contextual material-flow model failed: {error}"),
        )
    })?;

    structure
        .recipes
        .iter()
        .map(|occurrence| {
            let rate = cyclic::approximate_rate(solution.value(recipe_variables[occurrence.id]))
                .map_err(ContextualThroughputDiagnostic::from_rate_error)?;
            Ok((occurrence.id.to_string(), rate))
        })
        .collect()
}

fn derive_flow_rates(
    structure: &GraphStructure<'_>,
    recipe_rates: &BTreeMap<String, Rate>,
    target_rate: Rate,
) -> Result<Vec<ContextualMaterialFlowRate>, ContextualThroughputDiagnostic> {
    structure
        .edges
        .iter()
        .map(|edge| {
            let rate = if edge.target == structure.target_id {
                target_rate
            } else {
                let Some(&recipe_index) = structure.recipes_by_id.get(edge.target.as_str()) else {
                    return Err(ContextualThroughputDiagnostic::error(
                        "unknown-flow-consumer",
                        &edge.path,
                        Some(edge.target.clone()),
                        format!(
                            "flow edge target '{}' is not a recipe or target node",
                            edge.target
                        ),
                    ));
                };
                let occurrence = &structure.recipes[recipe_index];
                let input = occurrence
                    .recipe
                    .inputs
                    .iter()
                    .find(|input| input.item == edge.item)
                    .expect("graph structure validation matched every recipe input edge");
                recipe_rates[occurrence.id]
                    .checked_mul_i64(input.quantity)
                    .map_err(ContextualThroughputDiagnostic::from_rate_error)?
            };
            Ok(ContextualMaterialFlowRate {
                edge: edge.id.clone(),
                path: edge.path.clone(),
                source: edge.source.clone(),
                target: edge.target.clone(),
                kind: edge.kind,
                item: edge.item.clone(),
                rate,
                cycle_to_path: edge.cycle_to_path.clone(),
            })
        })
        .collect()
}

fn build_recipe_reports(
    structure: &GraphStructure<'_>,
    recipe_rates: &BTreeMap<String, Rate>,
    flow_rates: &[ContextualMaterialFlowRate],
) -> Result<
    (Vec<ContextualRecipeRunRate>, Vec<ContextualSurplusRate>),
    ContextualThroughputDiagnostic,
> {
    let mut reports = Vec::new();
    let mut surplus_rates = Vec::new();

    for occurrence in &structure.recipes {
        let runs_per_second = recipe_rates[occurrence.id];
        let input_rates = multiply_amounts(&occurrence.recipe.inputs, runs_per_second)
            .map_err(ContextualThroughputDiagnostic::from_rate_error)?;
        let output_rates = multiply_amounts(&occurrence.recipe.outputs, runs_per_second)
            .map_err(ContextualThroughputDiagnostic::from_rate_error)?;
        let work_seconds_per_second = runs_per_second
            .checked_work_seconds_per_second(occurrence.recipe.duration_ms)
            .map_err(ContextualThroughputDiagnostic::from_rate_error)?;
        let mut limiting_outputs = Vec::new();

        for output in &output_rates {
            let used = flow_rates
                .iter()
                .filter(|flow| flow.source == occurrence.id && flow.item == output.item)
                .try_fold(Rate::zero(), |total, flow| {
                    total
                        .checked_add(flow.rate)
                        .map_err(ContextualThroughputDiagnostic::from_rate_error)
                })?;
            if used > output.rate {
                return Err(ContextualThroughputDiagnostic::error(
                    "contextual-rate-rounding-error",
                    occurrence.path,
                    Some(output.item.clone()),
                    format!(
                        "recipe occurrence '{}' cannot supply its exact contextual flow rate for item '{}'",
                        occurrence.id, output.item
                    ),
                ));
            }
            if used == output.rate && !used.is_zero() {
                limiting_outputs.push(output.item.clone());
            }
            let surplus = output
                .rate
                .checked_sub(used)
                .map_err(ContextualThroughputDiagnostic::from_rate_error)?;
            if !surplus.is_zero() {
                surplus_rates.push(ContextualSurplusRate {
                    occurrence: occurrence.id.to_string(),
                    path: occurrence.path.to_string(),
                    item: output.item.clone(),
                    rate: surplus,
                });
            }
        }

        reports.push(ContextualRecipeRunRate {
            occurrence: occurrence.id.to_string(),
            path: occurrence.path.to_string(),
            recipe: occurrence.recipe.id.clone(),
            facility: occurrence.recipe.facility.clone(),
            runs_per_second,
            work_seconds_per_second,
            limiting_outputs,
            input_rates,
            output_rates,
        });
    }

    Ok((reports, surplus_rates))
}

fn build_external_input_rates(
    structure: &GraphStructure<'_>,
    flow_rates: &[ContextualMaterialFlowRate],
) -> Result<Vec<ContextualExternalInputRate>, ContextualThroughputDiagnostic> {
    structure
        .externals
        .iter()
        .map(|external| {
            let flow = flow_rates
                .iter()
                .find(|flow| flow.source == external.id && flow.item == external.item)
                .ok_or_else(|| {
                    ContextualThroughputDiagnostic::error(
                        "missing-external-flow-rate",
                        external.path,
                        Some(external.id.to_string()),
                        format!(
                            "external occurrence '{}' has no material flow rate",
                            external.id
                        ),
                    )
                })?;
            Ok(ContextualExternalInputRate {
                occurrence: external.id.to_string(),
                path: external.path.to_string(),
                item: external.item.to_string(),
                rate: flow.rate,
            })
        })
        .collect()
}

fn bootstrap_items(structure: &GraphStructure<'_>) -> Vec<String> {
    let adjacency = structure
        .recipes
        .iter()
        .map(|occurrence| {
            let targets = structure
                .edges
                .iter()
                .filter(|edge| edge.source == occurrence.id)
                .filter(|edge| structure.recipes_by_id.contains_key(edge.target.as_str()))
                .map(|edge| edge.target.as_str())
                .collect::<BTreeSet<_>>();
            (occurrence.id, targets)
        })
        .collect::<BTreeMap<_, _>>();
    structure
        .edges
        .iter()
        .filter(|edge| {
            structure.recipes_by_id.contains_key(edge.source.as_str())
                && structure.recipes_by_id.contains_key(edge.target.as_str())
                && reachable(&adjacency, edge.target.as_str(), edge.source.as_str())
        })
        .map(|edge| edge.item.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn reachable(adjacency: &BTreeMap<&str, BTreeSet<&str>>, start: &str, target: &str) -> bool {
    let mut pending = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(current) = pending.pop() {
        if current == target {
            return true;
        }
        if seen.insert(current)
            && let Some(next) = adjacency.get(current)
        {
            pending.extend(next.iter().copied());
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipes::{
        ItemAmount, RecipeBook, RecipeSource, RecipeSourceSelection,
        SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION, ThroughputTarget,
    };

    fn recipe(id: &str, input: &str, output: &str) -> Recipe {
        Recipe {
            id: id.to_string(),
            facility: format!("{id}-facility"),
            inputs: vec![ItemAmount {
                item: input.to_string(),
                quantity: 1,
            }],
            outputs: vec![ItemAmount {
                item: output.to_string(),
                quantity: 1,
            }],
            duration_ms: 1000,
        }
    }

    fn contextual_book() -> ValidatedRecipeBook {
        ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: vec!["raw-material".to_string()],
            recipes: vec![
                recipe("make-shared-a", "raw-material", "shared-material"),
                recipe("make-shared-b", "raw-material", "shared-material"),
                recipe("make-left", "shared-material", "left-material"),
                recipe("make-right", "shared-material", "right-material"),
                Recipe {
                    id: "assemble-target".to_string(),
                    facility: "assembler".to_string(),
                    inputs: vec![
                        ItemAmount {
                            item: "left-material".to_string(),
                            quantity: 1,
                        },
                        ItemAmount {
                            item: "right-material".to_string(),
                            quantity: 1,
                        },
                    ],
                    outputs: vec![ItemAmount {
                        item: "target-material".to_string(),
                        quantity: 1,
                    }],
                    duration_ms: 1000,
                },
            ],
        })
        .expect("contextual throughput book should validate")
    }

    fn request(source_selections: Vec<RecipeSourceSelection>) -> RecipeSourcePlanRequest {
        RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "target-material".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections,
        }
    }

    #[test]
    fn calculates_a_simple_contextual_chain() {
        let book = ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: vec!["ore".to_string()],
            recipes: vec![Recipe {
                id: "grind-powder".to_string(),
                facility: "grinder".to_string(),
                inputs: vec![ItemAmount {
                    item: "ore".to_string(),
                    quantity: 2,
                }],
                outputs: vec![ItemAmount {
                    item: "powder".to_string(),
                    quantity: 1,
                }],
                duration_ms: 2000,
            }],
        })
        .expect("simple contextual throughput book should validate");
        let report = book.calculate_contextual_throughput(&RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "powder".to_string(),
                quantity: 1,
                duration_ms: 2000,
            },
            source_selections: Vec::new(),
        });

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(
            report.recipe_rates[0].runs_per_second,
            Rate {
                numerator: 1,
                denominator: 2
            }
        );
        assert_eq!(
            report.external_input_rates[0].rate,
            Rate {
                numerator: 1,
                denominator: 1
            }
        );
        assert!(report.surplus_rates.is_empty());
    }

    #[test]
    fn preserves_independent_external_and_recipe_sources_for_one_item() {
        let left_path = "/target/recipe:assemble-target/input:left-material/recipe:make-left/input:shared-material";
        let right_path = "/target/recipe:assemble-target/input:right-material/recipe:make-right/input:shared-material";
        let report = contextual_book().calculate_contextual_throughput(&request(vec![
            RecipeSourceSelection {
                path: left_path.to_string(),
                source: RecipeSource::ExternalInput,
            },
            RecipeSourceSelection {
                path: right_path.to_string(),
                source: RecipeSource::Recipe {
                    recipe: "make-shared-a".to_string(),
                },
            },
        ]));

        assert!(report.success, "{:#?}", report.diagnostics);
        assert!(report.external_input_rates.iter().any(|rate| {
            rate.path == left_path
                && rate.item == "shared-material"
                && rate.rate
                    == Rate {
                        numerator: 1,
                        denominator: 1,
                    }
        }));
        assert!(report.recipe_rates.iter().any(|rate| {
            rate.path == right_path
                && rate.recipe == "make-shared-a"
                && rate.runs_per_second
                    == Rate {
                        numerator: 1,
                        denominator: 1,
                    }
        }));
    }

    #[test]
    fn solves_an_amplifying_contextual_cycle() {
        let book = ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: Vec::new(),
            recipes: vec![
                Recipe {
                    id: "grow-crop".to_string(),
                    facility: "planter".to_string(),
                    inputs: vec![ItemAmount {
                        item: "seed".to_string(),
                        quantity: 1,
                    }],
                    outputs: vec![ItemAmount {
                        item: "crop".to_string(),
                        quantity: 2,
                    }],
                    duration_ms: 1000,
                },
                Recipe {
                    id: "collect-seed".to_string(),
                    facility: "collector".to_string(),
                    inputs: vec![ItemAmount {
                        item: "crop".to_string(),
                        quantity: 1,
                    }],
                    outputs: vec![ItemAmount {
                        item: "seed".to_string(),
                        quantity: 2,
                    }],
                    duration_ms: 1000,
                },
            ],
        })
        .expect("amplifying cycle book should validate");
        let report = book.calculate_contextual_throughput(&RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "crop".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections: Vec::new(),
        });

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.bootstrap_item_options, vec!["crop", "seed"]);
        assert_eq!(report.recipe_rates.len(), 2);
        assert_eq!(
            report.recipe_rates[0].runs_per_second,
            Rate {
                numerator: 2,
                denominator: 3
            }
        );
        assert_eq!(
            report.recipe_rates[1].runs_per_second,
            Rate {
                numerator: 1,
                denominator: 3
            }
        );
    }

    #[test]
    fn reports_surplus_per_recipe_occurrence() {
        let book = ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: vec!["ore".to_string()],
            recipes: vec![Recipe {
                id: "separate-ore".to_string(),
                facility: "separator".to_string(),
                inputs: vec![ItemAmount {
                    item: "ore".to_string(),
                    quantity: 1,
                }],
                outputs: vec![
                    ItemAmount {
                        item: "powder".to_string(),
                        quantity: 1,
                    },
                    ItemAmount {
                        item: "slag".to_string(),
                        quantity: 2,
                    },
                ],
                duration_ms: 1000,
            }],
        })
        .expect("co-product book should validate");
        let report = book.calculate_contextual_throughput(&RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "powder".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections: Vec::new(),
        });

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.surplus_rates.len(), 1);
        assert_eq!(report.surplus_rates[0].path, "/target");
        assert_eq!(report.surplus_rates[0].item, "slag");
        assert_eq!(
            report.surplus_rates[0].rate,
            Rate {
                numerator: 2,
                denominator: 1
            }
        );
    }

    #[test]
    fn rejects_a_non_amplifying_contextual_cycle_with_target_drain() {
        let book = ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: Vec::new(),
            recipes: vec![
                recipe("grow-crop", "seed", "crop"),
                recipe("collect-seed", "crop", "seed"),
            ],
        })
        .expect("lossless cycle book should validate");
        let report = book.calculate_contextual_throughput(&RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "crop".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections: Vec::new(),
        });

        assert!(!report.success);
        assert_eq!(
            report.diagnostics[0].code,
            "contextual-throughput-infeasible"
        );
    }

    #[test]
    fn preserves_required_paths_when_sources_are_unresolved() {
        let report = contextual_book().calculate_contextual_throughput(&request(Vec::new()));

        assert!(!report.success);
        assert_eq!(report.required_selection_paths.len(), 2);
        assert_eq!(
            report.diagnostics[0].code,
            "contextual-production-graph-failed"
        );
    }
}
