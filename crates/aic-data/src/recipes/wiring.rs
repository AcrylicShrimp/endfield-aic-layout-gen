use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::recipes::{ItemRate, Rate, RecipeThroughputReport};

const STAGE: &str = "recipe-wiring-graph";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeWiringGraphReport {
    pub success: bool,
    pub nodes: Vec<RecipeWiringGraphNode>,
    pub edges: Vec<RecipeWiringEdge>,
    pub diagnostics: Vec<RecipeWiringGraphDiagnostic>,
}

impl RecipeWiringGraphReport {
    fn success(nodes: Vec<RecipeWiringGraphNode>, edges: Vec<RecipeWiringEdge>) -> Self {
        Self {
            success: true,
            nodes,
            edges,
            diagnostics: vec![RecipeWiringGraphDiagnostic::info(
                "recipe-wiring-graph-built",
                "/",
                None,
                "recipe-level wiring graph was built",
            )],
        }
    }

    fn failure(diagnostic: RecipeWiringGraphDiagnostic) -> Self {
        Self {
            success: false,
            nodes: Vec::new(),
            edges: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeWiringGraphNode {
    pub id: String,
    pub kind: String,
    pub recipe: Option<String>,
    pub item: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeWiringEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub item: String,
    pub rate: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeWiringGraphDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl RecipeWiringGraphDiagnostic {
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

pub fn build_recipe_wiring_graph(throughput: &RecipeThroughputReport) -> RecipeWiringGraphReport {
    if !throughput.success {
        return RecipeWiringGraphReport::failure(RecipeWiringGraphDiagnostic::error(
            "upstream-throughput-failed",
            "/",
            None,
            "recipe wiring graph requires a successful throughput report",
        ));
    }

    let Some(target) = &throughput.target else {
        return RecipeWiringGraphReport::failure(RecipeWiringGraphDiagnostic::error(
            "missing-target",
            "/target",
            None,
            "successful throughput report must contain a target",
        ));
    };

    let producers = match producer_lookup(throughput) {
        Ok(producers) => producers,
        Err(diagnostic) => return RecipeWiringGraphReport::failure(diagnostic),
    };
    let external_items = throughput
        .external_input_rates
        .iter()
        .map(|rate| rate.item.as_str())
        .collect::<BTreeSet<_>>();

    let nodes = graph_nodes(throughput);
    let edges = match graph_edges(throughput, target, &producers, &external_items) {
        Ok(edges) => edges,
        Err(diagnostic) => return RecipeWiringGraphReport::failure(diagnostic),
    };

    RecipeWiringGraphReport::success(nodes, edges)
}

fn producer_lookup(
    throughput: &RecipeThroughputReport,
) -> Result<BTreeMap<&str, &str>, RecipeWiringGraphDiagnostic> {
    let mut producers = BTreeMap::<&str, &str>::new();

    for recipe_rate in &throughput.recipe_rates {
        for output_rate in &recipe_rate.output_rates {
            if producers
                .insert(output_rate.item.as_str(), recipe_rate.recipe.as_str())
                .is_some()
            {
                return Err(RecipeWiringGraphDiagnostic::error(
                    "ambiguous-producer",
                    "/recipe_rates",
                    Some(output_rate.item.clone()),
                    format!("item '{}' has more than one producer", output_rate.item),
                ));
            }
        }
    }

    Ok(producers)
}

fn graph_nodes(throughput: &RecipeThroughputReport) -> Vec<RecipeWiringGraphNode> {
    let mut nodes = Vec::new();

    for external in &throughput.external_input_rates {
        nodes.push(item_node("external", &external.item));
    }

    for recipe_rate in &throughput.recipe_rates {
        nodes.push(RecipeWiringGraphNode {
            id: recipe_node_id(&recipe_rate.recipe),
            kind: "recipe".to_string(),
            recipe: Some(recipe_rate.recipe.clone()),
            item: None,
        });
    }

    if let Some(target) = &throughput.target {
        nodes.push(item_node("target", &target.item));
    }

    for surplus in &throughput.surplus_rates {
        nodes.push(item_node("surplus", &surplus.item));
    }

    nodes
}

fn graph_edges(
    throughput: &RecipeThroughputReport,
    target: &ItemRate,
    producers: &BTreeMap<&str, &str>,
    external_items: &BTreeSet<&str>,
) -> Result<Vec<RecipeWiringEdge>, RecipeWiringGraphDiagnostic> {
    let mut edges = Vec::new();
    add_recipe_input_edges(&mut edges, throughput, producers, external_items)?;
    add_target_edge(&mut edges, target, producers, external_items)?;
    add_surplus_edges(&mut edges, throughput, producers)?;
    Ok(edges)
}

fn add_recipe_input_edges(
    edges: &mut Vec<RecipeWiringEdge>,
    throughput: &RecipeThroughputReport,
    producers: &BTreeMap<&str, &str>,
    external_items: &BTreeSet<&str>,
) -> Result<(), RecipeWiringGraphDiagnostic> {
    for recipe_rate in &throughput.recipe_rates {
        for input_rate in &recipe_rate.input_rates {
            let (source, kind) =
                source_for_item(&input_rate.item, producers, external_items, "/recipe_rates")?;
            edges.push(RecipeWiringEdge {
                source,
                target: recipe_node_id(&recipe_rate.recipe),
                kind,
                item: input_rate.item.clone(),
                rate: input_rate.rate,
            });
        }
    }

    Ok(())
}

fn add_target_edge(
    edges: &mut Vec<RecipeWiringEdge>,
    target: &ItemRate,
    producers: &BTreeMap<&str, &str>,
    external_items: &BTreeSet<&str>,
) -> Result<(), RecipeWiringGraphDiagnostic> {
    let (source, _) = source_for_item(&target.item, producers, external_items, "/target/item")?;
    edges.push(RecipeWiringEdge {
        source,
        target: item_node_id("target", &target.item),
        kind: "target-output".to_string(),
        item: target.item.clone(),
        rate: target.rate,
    });
    Ok(())
}

fn add_surplus_edges(
    edges: &mut Vec<RecipeWiringEdge>,
    throughput: &RecipeThroughputReport,
    producers: &BTreeMap<&str, &str>,
) -> Result<(), RecipeWiringGraphDiagnostic> {
    for surplus_rate in &throughput.surplus_rates {
        let Some(recipe) = producers.get(surplus_rate.item.as_str()) else {
            return Err(missing_producer(&surplus_rate.item, "/surplus_rates"));
        };
        edges.push(RecipeWiringEdge {
            source: recipe_node_id(recipe),
            target: item_node_id("surplus", &surplus_rate.item),
            kind: "surplus-output".to_string(),
            item: surplus_rate.item.clone(),
            rate: surplus_rate.rate,
        });
    }

    Ok(())
}

fn source_for_item(
    item: &str,
    producers: &BTreeMap<&str, &str>,
    external_items: &BTreeSet<&str>,
    path: &str,
) -> Result<(String, String), RecipeWiringGraphDiagnostic> {
    if external_items.contains(item) {
        return Ok((item_node_id("external", item), "external-input".to_string()));
    }

    if let Some(recipe) = producers.get(item) {
        return Ok((recipe_node_id(recipe), "recipe-flow".to_string()));
    }

    Err(missing_producer(item, path))
}

fn missing_producer(item: &str, path: &str) -> RecipeWiringGraphDiagnostic {
    RecipeWiringGraphDiagnostic::error(
        "missing-producer",
        path,
        Some(item.to_string()),
        format!("item '{item}' has no external source or producer recipe"),
    )
}

fn item_node(kind: &str, item: &str) -> RecipeWiringGraphNode {
    RecipeWiringGraphNode {
        id: item_node_id(kind, item),
        kind: kind.to_string(),
        recipe: None,
        item: Some(item.to_string()),
    }
}

fn item_node_id(kind: &str, item: &str) -> String {
    format!("{kind}:{item}")
}

fn recipe_node_id(recipe: &str) -> String {
    format!("recipe:{recipe}")
}
