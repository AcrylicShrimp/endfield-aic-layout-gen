use std::collections::BTreeMap;

use serde::Serialize;

use crate::recipes::{
    Recipe, RecipeSource, RecipeSourceCheckStatus, RecipeSourceNode, RecipeSourcePlanRequest,
    ThroughputTarget, ValidatedRecipeBook, check_recipe_source_plan,
};

const STAGE: &str = "contextual-production-graph";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextualProductionGraphReport {
    pub success: bool,
    pub source_status: RecipeSourceCheckStatus,
    pub target: Option<ThroughputTarget>,
    pub nodes: Vec<ContextualProductionNode>,
    pub edges: Vec<ContextualProductionEdge>,
    pub required_selection_paths: Vec<String>,
    pub diagnostics: Vec<ContextualProductionGraphDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ContextualProductionNode {
    RecipeOccurrence {
        id: String,
        path: String,
        recipe: Recipe,
    },
    ExternalInput {
        id: String,
        path: String,
        item: String,
    },
    Target {
        id: String,
        path: String,
        item: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ContextualProductionFlowKind {
    RecipeFlow,
    ExternalInput,
    TargetOutput,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextualProductionEdge {
    pub id: String,
    pub path: String,
    pub source: String,
    pub target: String,
    pub kind: ContextualProductionFlowKind,
    pub item: String,
    pub cycle_to_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ContextualProductionGraphDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl ContextualProductionGraphReport {
    fn failure(status: RecipeSourceCheckStatus, required_selection_paths: Vec<String>) -> Self {
        Self {
            success: false,
            source_status: status,
            target: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            required_selection_paths,
            diagnostics: vec![ContextualProductionGraphDiagnostic::error(
                "source-plan-not-ready",
                "/",
                None,
                "contextual production graph requires a ready source plan",
            )],
        }
    }
}

impl ContextualProductionGraphDiagnostic {
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
}

pub fn build_contextual_production_graph(
    book: &ValidatedRecipeBook,
    request: &RecipeSourcePlanRequest,
) -> ContextualProductionGraphReport {
    let source_report = check_recipe_source_plan(book, request);
    if !source_report.ready {
        return ContextualProductionGraphReport::failure(
            source_report.status,
            source_report.required_selection_paths,
        );
    }

    let Some(root) = source_report.root.as_ref() else {
        return ContextualProductionGraphReport::failure(
            RecipeSourceCheckStatus::InvalidInput,
            Vec::new(),
        );
    };

    let target_id = target_node_id();
    let mut builder = ProductionGraphBuilder {
        book,
        nodes: vec![ContextualProductionNode::Target {
            id: target_id.clone(),
            path: "/target-output".to_string(),
            item: request.target.item.clone(),
        }],
        edges: Vec::new(),
        recipe_occurrences: BTreeMap::new(),
        diagnostic: None,
    };
    builder.visit(root, &target_id, true);

    if let Some(diagnostic) = builder.diagnostic {
        return ContextualProductionGraphReport {
            success: false,
            source_status: RecipeSourceCheckStatus::Ready,
            target: None,
            nodes: Vec::new(),
            edges: Vec::new(),
            required_selection_paths: Vec::new(),
            diagnostics: vec![diagnostic],
        };
    }

    ContextualProductionGraphReport {
        success: true,
        source_status: RecipeSourceCheckStatus::Ready,
        target: Some(request.target.clone()),
        nodes: builder.nodes,
        edges: builder.edges,
        required_selection_paths: Vec::new(),
        diagnostics: vec![ContextualProductionGraphDiagnostic::info(
            "contextual-production-graph-built",
            "/",
            Some(request.target.item.clone()),
            "contextual production graph was built from the ready source hierarchy",
        )],
    }
}

struct ProductionGraphBuilder<'a> {
    book: &'a ValidatedRecipeBook,
    nodes: Vec<ContextualProductionNode>,
    edges: Vec<ContextualProductionEdge>,
    recipe_occurrences: BTreeMap<String, String>,
    diagnostic: Option<ContextualProductionGraphDiagnostic>,
}

impl ProductionGraphBuilder<'_> {
    fn visit(&mut self, node: &RecipeSourceNode, consumer_id: &str, is_root: bool) {
        if self.diagnostic.is_some() {
            return;
        }

        let Some(selected_source) = node.selected_source.as_ref() else {
            self.diagnostic = Some(ContextualProductionGraphDiagnostic::error(
                "missing-selected-source",
                &node.path,
                Some(node.item.clone()),
                "ready source hierarchy node is missing a selected source",
            ));
            return;
        };

        let (source_id, source_kind) = match selected_source {
            RecipeSource::ExternalInput => {
                let id = external_node_id(&node.path);
                self.nodes.push(ContextualProductionNode::ExternalInput {
                    id: id.clone(),
                    path: node.path.clone(),
                    item: node.item.clone(),
                });
                (id, ContextualProductionFlowKind::ExternalInput)
            }
            RecipeSource::Recipe { recipe } => {
                if let Some(cycle_path) = &node.cycle_to_path {
                    let Some(id) = self.recipe_occurrences.get(cycle_path) else {
                        self.diagnostic = Some(ContextualProductionGraphDiagnostic::error(
                            "unknown-cycle-reference",
                            &node.path,
                            Some(cycle_path.clone()),
                            format!(
                                "cycle reference '{}' does not identify an ancestor recipe occurrence",
                                cycle_path
                            ),
                        ));
                        return;
                    };
                    (id.clone(), ContextualProductionFlowKind::RecipeFlow)
                } else {
                    let Some(recipe) = self.book.index().recipe(recipe).cloned() else {
                        self.diagnostic = Some(ContextualProductionGraphDiagnostic::error(
                            "unknown-selected-recipe",
                            &node.path,
                            Some(recipe.clone()),
                            format!(
                                "selected recipe '{recipe}' is not in the validated recipe book"
                            ),
                        ));
                        return;
                    };
                    let id = recipe_node_id(&node.path);
                    self.recipe_occurrences
                        .insert(node.path.clone(), id.clone());
                    self.nodes.push(ContextualProductionNode::RecipeOccurrence {
                        id: id.clone(),
                        path: node.path.clone(),
                        recipe,
                    });
                    (id, ContextualProductionFlowKind::RecipeFlow)
                }
            }
        };

        self.edges.push(ContextualProductionEdge {
            id: material_edge_id(&node.path),
            path: node.path.clone(),
            source: source_id.clone(),
            target: consumer_id.to_string(),
            kind: if is_root {
                ContextualProductionFlowKind::TargetOutput
            } else {
                source_kind
            },
            item: node.item.clone(),
            cycle_to_path: node.cycle_to_path.clone(),
        });

        if matches!(selected_source, RecipeSource::Recipe { .. }) && node.cycle_to_path.is_none() {
            for child in &node.children {
                self.visit(child, &source_id, false);
            }
        }
    }
}

fn target_node_id() -> String {
    "target:/target-output".to_string()
}

fn recipe_node_id(path: &str) -> String {
    format!("recipe-occurrence:{path}")
}

fn external_node_id(path: &str) -> String {
    format!("external-input:{path}")
}

fn material_edge_id(path: &str) -> String {
    format!("material-demand:{path}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipes::{
        ItemAmount, RecipeBook, RecipeSourceSelection, SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
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
        .expect("contextual production book should validate")
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
    fn preserves_context_specific_sources_for_the_same_material() {
        let left_path = "/target/recipe:assemble-target/input:left-material/recipe:make-left/input:shared-material";
        let right_path = "/target/recipe:assemble-target/input:right-material/recipe:make-right/input:shared-material";
        let report = build_contextual_production_graph(
            &contextual_book(),
            &request(vec![
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
            ]),
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert!(report.nodes.iter().any(|node| matches!(
            node,
            ContextualProductionNode::ExternalInput { path, item, .. }
                if path == left_path && item == "shared-material"
        )));
        assert!(report.nodes.iter().any(|node| matches!(
            node,
            ContextualProductionNode::RecipeOccurrence { path, recipe, .. }
                if path == right_path && recipe.id == "make-shared-a"
        )));
        assert!(report.edges.iter().any(|edge| {
            edge.path == left_path && edge.kind == ContextualProductionFlowKind::ExternalInput
        }));
        assert!(report.edges.iter().any(|edge| {
            edge.path == right_path && edge.kind == ContextualProductionFlowKind::RecipeFlow
        }));
    }

    #[test]
    fn keeps_repeated_recipe_selections_as_distinct_occurrences() {
        let left_path = "/target/recipe:assemble-target/input:left-material/recipe:make-left/input:shared-material";
        let right_path = "/target/recipe:assemble-target/input:right-material/recipe:make-right/input:shared-material";
        let report = build_contextual_production_graph(
            &contextual_book(),
            &request(vec![
                RecipeSourceSelection {
                    path: left_path.to_string(),
                    source: RecipeSource::Recipe {
                        recipe: "make-shared-a".to_string(),
                    },
                },
                RecipeSourceSelection {
                    path: right_path.to_string(),
                    source: RecipeSource::Recipe {
                        recipe: "make-shared-a".to_string(),
                    },
                },
            ]),
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        let occurrence_paths = report
            .nodes
            .iter()
            .filter_map(|node| match node {
                ContextualProductionNode::RecipeOccurrence { path, recipe, .. }
                    if recipe.id == "make-shared-a" =>
                {
                    Some(path.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(occurrence_paths, vec![left_path, right_path]);
    }

    #[test]
    fn projects_a_finite_cycle_back_to_the_ancestor_occurrence() {
        let book = ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: Vec::new(),
            recipes: vec![
                recipe("grow-crop", "seed", "crop"),
                recipe("collect-seed", "crop", "seed"),
            ],
        })
        .expect("cyclic production book should validate");
        let request = RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "crop".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections: Vec::new(),
        };

        let report = build_contextual_production_graph(&book, &request);

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.nodes.len(), 3);
        assert_eq!(report.edges.len(), 3);
        let cycle_edge = report
            .edges
            .iter()
            .find(|edge| edge.cycle_to_path.is_some())
            .expect("cycle should become one finite back edge");
        assert_eq!(cycle_edge.cycle_to_path.as_deref(), Some("/target"));
        assert_eq!(cycle_edge.source, recipe_node_id("/target"));
        assert_eq!(
            cycle_edge.target,
            recipe_node_id("/target/recipe:grow-crop/input:seed")
        );
    }

    #[test]
    fn reports_all_required_paths_when_the_source_plan_is_not_ready() {
        let report = build_contextual_production_graph(&contextual_book(), &request(Vec::new()));

        assert!(!report.success);
        assert_eq!(report.nodes, Vec::new());
        assert_eq!(report.edges, Vec::new());
        assert_eq!(report.required_selection_paths.len(), 2);
        assert_eq!(report.diagnostics[0].code, "source-plan-not-ready");
    }
}
