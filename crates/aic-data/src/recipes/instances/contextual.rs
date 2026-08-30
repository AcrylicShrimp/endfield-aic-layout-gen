use std::collections::{BTreeMap, BTreeSet};

use crate::recipes::{
    ContextualFacilityRequirement, ContextualFacilityRequirementReport,
    ContextualProductionFlowKind, ContextualRecipeRunRate, ContextualThroughputReport, Rate,
};

use super::{
    FacilityInstanceWiringDiagnostic, FacilityInstanceWiringEdge, FacilityInstanceWiringNode,
    FacilityInstanceWiringReport, arithmetic_overflow, map_throughput_arithmetic,
};

pub fn build_contextual_facility_instance_wiring(
    throughput: &ContextualThroughputReport,
    facilities: &ContextualFacilityRequirementReport,
) -> FacilityInstanceWiringReport {
    if !throughput.success {
        return FacilityInstanceWiringReport::failure(FacilityInstanceWiringDiagnostic::error(
            "upstream-contextual-throughput-failed",
            "/",
            None,
            "contextual facility instance wiring requires successful contextual throughput",
        ));
    }
    if !facilities.success {
        return FacilityInstanceWiringReport::failure(FacilityInstanceWiringDiagnostic::error(
            "upstream-contextual-facility-requirements-failed",
            "/",
            None,
            "contextual facility instance wiring requires successful contextual facility requirements",
        ));
    }
    let Some(target) = throughput.target.as_ref() else {
        return FacilityInstanceWiringReport::failure(FacilityInstanceWiringDiagnostic::error(
            "missing-target",
            "/target",
            None,
            "successful contextual throughput must contain a target",
        ));
    };

    let requirements = match requirement_lookup(throughput, facilities) {
        Ok(requirements) => requirements,
        Err(diagnostic) => return FacilityInstanceWiringReport::failure(diagnostic),
    };
    let instances = match build_instances(throughput, &requirements) {
        Ok(instances) => instances,
        Err(diagnostic) => return FacilityInstanceWiringReport::failure(diagnostic),
    };
    let target_id = match target_node_id(throughput) {
        Ok(id) => id,
        Err(diagnostic) => return FacilityInstanceWiringReport::failure(diagnostic),
    };

    let nodes = graph_nodes(throughput, target, &target_id, &instances);
    let edges = match graph_edges(throughput, &target_id, &instances) {
        Ok(edges) => edges,
        Err(diagnostic) => return FacilityInstanceWiringReport::failure(diagnostic),
    };

    FacilityInstanceWiringReport::success(nodes, edges)
}

fn requirement_lookup<'a>(
    throughput: &'a ContextualThroughputReport,
    facilities: &'a ContextualFacilityRequirementReport,
) -> Result<BTreeMap<&'a str, &'a ContextualFacilityRequirement>, FacilityInstanceWiringDiagnostic>
{
    let recipe_rates = throughput
        .recipe_rates
        .iter()
        .map(|rate| (rate.occurrence.as_str(), rate))
        .collect::<BTreeMap<_, _>>();
    let mut requirements = BTreeMap::new();

    for requirement in &facilities.occurrence_requirements {
        if requirements
            .insert(requirement.occurrence.as_str(), requirement)
            .is_some()
        {
            return Err(FacilityInstanceWiringDiagnostic::error(
                "duplicate-contextual-facility-requirement",
                "/occurrence_requirements",
                Some(requirement.occurrence.clone()),
                format!(
                    "recipe occurrence '{}' has more than one facility requirement",
                    requirement.occurrence
                ),
            ));
        }
        let Some(recipe_rate) = recipe_rates.get(requirement.occurrence.as_str()) else {
            return Err(FacilityInstanceWiringDiagnostic::error(
                "unexpected-contextual-facility-requirement",
                "/occurrence_requirements",
                Some(requirement.occurrence.clone()),
                format!(
                    "facility requirement references unknown recipe occurrence '{}'",
                    requirement.occurrence
                ),
            ));
        };
        validate_requirement(requirement, recipe_rate)?;
    }

    for occurrence in recipe_rates.keys() {
        if !requirements.contains_key(occurrence) {
            return Err(FacilityInstanceWiringDiagnostic::error(
                "missing-contextual-facility-requirement",
                "/occurrence_requirements",
                Some((*occurrence).to_string()),
                format!("recipe occurrence '{occurrence}' has no facility requirement"),
            ));
        }
    }

    Ok(requirements)
}

fn validate_requirement(
    requirement: &ContextualFacilityRequirement,
    recipe_rate: &ContextualRecipeRunRate,
) -> Result<(), FacilityInstanceWiringDiagnostic> {
    if requirement.path != recipe_rate.path
        || requirement.recipe != recipe_rate.recipe
        || requirement.facility != recipe_rate.facility
        || requirement.work_seconds_per_second != recipe_rate.work_seconds_per_second
    {
        return Err(FacilityInstanceWiringDiagnostic::error(
            "contextual-facility-requirement-mismatch",
            "/occurrence_requirements",
            Some(requirement.occurrence.clone()),
            format!(
                "facility requirement for occurrence '{}' does not match contextual throughput",
                requirement.occurrence
            ),
        ));
    }
    let expected_count = ceil_rate(recipe_rate.work_seconds_per_second)?;
    if requirement.required_facilities != expected_count {
        return Err(FacilityInstanceWiringDiagnostic::error(
            "contextual-facility-count-mismatch",
            "/occurrence_requirements",
            Some(requirement.occurrence.clone()),
            format!(
                "recipe occurrence '{}' requires {} facilities but expected {}",
                requirement.occurrence, requirement.required_facilities, expected_count
            ),
        ));
    }
    Ok(())
}

fn build_instances(
    throughput: &ContextualThroughputReport,
    requirements: &BTreeMap<&str, &ContextualFacilityRequirement>,
) -> Result<BTreeMap<String, Vec<FacilityInstanceWiringNode>>, FacilityInstanceWiringDiagnostic> {
    let mut instances = BTreeMap::new();
    for recipe_rate in &throughput.recipe_rates {
        let required_facilities = requirements[recipe_rate.occurrence.as_str()].required_facilities;
        let mut occurrence_instances = Vec::new();
        for index in 0..required_facilities {
            occurrence_instances.push(facility_node(recipe_rate, index, required_facilities)?);
        }
        if occurrence_instances.is_empty() && !recipe_rate.runs_per_second.is_zero() {
            return Err(FacilityInstanceWiringDiagnostic::error(
                "missing-contextual-facility-instances",
                "/occurrence_requirements",
                Some(recipe_rate.occurrence.clone()),
                format!(
                    "nonzero recipe occurrence '{}' has no facility instances",
                    recipe_rate.occurrence
                ),
            ));
        }
        instances.insert(recipe_rate.occurrence.clone(), occurrence_instances);
    }
    Ok(instances)
}

fn facility_node(
    recipe_rate: &ContextualRecipeRunRate,
    index: i64,
    required_facilities: i64,
) -> Result<FacilityInstanceWiringNode, FacilityInstanceWiringDiagnostic> {
    let runs_per_second = recipe_rate
        .runs_per_second
        .checked_div_i64(required_facilities)
        .map_err(map_throughput_arithmetic)?;
    let work_seconds_per_second = recipe_rate
        .work_seconds_per_second
        .checked_div_i64(required_facilities)
        .map_err(map_throughput_arithmetic)?;
    let unused_capacity = Rate {
        numerator: 1,
        denominator: 1,
    }
    .checked_sub(work_seconds_per_second)
    .map_err(map_throughput_arithmetic)?;

    Ok(FacilityInstanceWiringNode::Facility {
        id: facility_instance_id(&recipe_rate.occurrence, index),
        recipe: recipe_rate.recipe.clone(),
        facility: recipe_rate.facility.clone(),
        index,
        runs_per_second,
        work_seconds_per_second,
        unused_capacity,
    })
}

fn target_node_id(
    throughput: &ContextualThroughputReport,
) -> Result<String, FacilityInstanceWiringDiagnostic> {
    let target_ids = throughput
        .flow_rates
        .iter()
        .filter(|flow| flow.kind == ContextualProductionFlowKind::TargetOutput)
        .map(|flow| flow.target.as_str())
        .collect::<BTreeSet<_>>();
    if target_ids.len() != 1 {
        return Err(FacilityInstanceWiringDiagnostic::error(
            "invalid-contextual-target-endpoints",
            "/flow_rates",
            None,
            format!(
                "contextual throughput must have exactly one target endpoint, found {}",
                target_ids.len()
            ),
        ));
    }
    Ok(target_ids
        .into_iter()
        .next()
        .expect("one target ID was validated")
        .to_string())
}

fn graph_nodes(
    throughput: &ContextualThroughputReport,
    target: &crate::recipes::ItemRate,
    target_id: &str,
    instances: &BTreeMap<String, Vec<FacilityInstanceWiringNode>>,
) -> Vec<FacilityInstanceWiringNode> {
    let mut nodes = Vec::new();
    for external in &throughput.external_input_rates {
        nodes.push(FacilityInstanceWiringNode::External {
            id: external.occurrence.clone(),
            item: external.item.clone(),
        });
    }
    for recipe_rate in &throughput.recipe_rates {
        nodes.extend(instances[&recipe_rate.occurrence].iter().cloned());
    }
    nodes.push(FacilityInstanceWiringNode::Target {
        id: target_id.to_string(),
        item: target.item.clone(),
    });
    for surplus in &throughput.surplus_rates {
        nodes.push(FacilityInstanceWiringNode::Surplus {
            id: surplus_node_id(&surplus.occurrence, &surplus.item),
            item: surplus.item.clone(),
        });
    }
    nodes
}

fn graph_edges(
    throughput: &ContextualThroughputReport,
    target_id: &str,
    instances: &BTreeMap<String, Vec<FacilityInstanceWiringNode>>,
) -> Result<Vec<FacilityInstanceWiringEdge>, FacilityInstanceWiringDiagnostic> {
    let external_ids = throughput
        .external_input_rates
        .iter()
        .map(|rate| rate.occurrence.as_str())
        .collect::<BTreeSet<_>>();
    let mut edges = Vec::new();

    for flow in &throughput.flow_rates {
        let sources =
            endpoint_instance_ids(&flow.source, instances, &external_ids, target_id, false)?;
        let targets =
            endpoint_instance_ids(&flow.target, instances, &external_ids, target_id, true)?;
        split_flow(
            &mut edges,
            &sources,
            &targets,
            flow_kind_name(flow.kind),
            &flow.item,
            flow.rate,
        )?;
    }

    for surplus in &throughput.surplus_rates {
        let sources = instance_ids(instances, &surplus.occurrence)?;
        split_flow(
            &mut edges,
            &sources,
            &[surplus_node_id(&surplus.occurrence, &surplus.item)],
            "surplus-output",
            &surplus.item,
            surplus.rate,
        )?;
    }

    Ok(edges)
}

fn endpoint_instance_ids(
    endpoint: &str,
    instances: &BTreeMap<String, Vec<FacilityInstanceWiringNode>>,
    external_ids: &BTreeSet<&str>,
    target_id: &str,
    allow_target: bool,
) -> Result<Vec<String>, FacilityInstanceWiringDiagnostic> {
    if instances.contains_key(endpoint) {
        return instance_ids(instances, endpoint);
    }
    if external_ids.contains(endpoint) || (allow_target && endpoint == target_id) {
        return Ok(vec![endpoint.to_string()]);
    }
    Err(FacilityInstanceWiringDiagnostic::error(
        "unknown-contextual-flow-endpoint",
        "/flow_rates",
        Some(endpoint.to_string()),
        format!("contextual flow endpoint '{endpoint}' is not known"),
    ))
}

fn instance_ids(
    instances: &BTreeMap<String, Vec<FacilityInstanceWiringNode>>,
    occurrence: &str,
) -> Result<Vec<String>, FacilityInstanceWiringDiagnostic> {
    let nodes = instances.get(occurrence).ok_or_else(|| {
        FacilityInstanceWiringDiagnostic::error(
            "missing-contextual-facility-instances",
            "/occurrence_requirements",
            Some(occurrence.to_string()),
            format!("recipe occurrence '{occurrence}' has no facility instance collection"),
        )
    })?;
    if nodes.is_empty() {
        return Err(FacilityInstanceWiringDiagnostic::error(
            "missing-contextual-facility-instances",
            "/occurrence_requirements",
            Some(occurrence.to_string()),
            format!("recipe occurrence '{occurrence}' has no facility instances"),
        ));
    }
    Ok(nodes.iter().map(node_id).collect())
}

fn split_flow(
    edges: &mut Vec<FacilityInstanceWiringEdge>,
    sources: &[String],
    targets: &[String],
    kind: &str,
    item: &str,
    total_rate: Rate,
) -> Result<(), FacilityInstanceWiringDiagnostic> {
    if total_rate.is_zero() {
        return Ok(());
    }
    if sources.is_empty() || targets.is_empty() {
        return Err(FacilityInstanceWiringDiagnostic::error(
            "empty-contextual-flow-endpoint-set",
            "/flow_rates",
            Some(item.to_string()),
            "nonzero contextual flow must have at least one source and target instance",
        ));
    }

    let source_rate = total_rate
        .checked_div_i64(sources.len() as i64)
        .map_err(map_throughput_arithmetic)?;
    let target_rate = total_rate
        .checked_div_i64(targets.len() as i64)
        .map_err(map_throughput_arithmetic)?;
    let mut source_index = 0;
    let mut target_index = 0;
    let mut source_remaining = source_rate;
    let mut target_remaining = target_rate;

    while source_index < sources.len() && target_index < targets.len() {
        let rate = source_remaining.min(target_remaining);
        edges.push(FacilityInstanceWiringEdge {
            source: sources[source_index].clone(),
            target: targets[target_index].clone(),
            kind: kind.to_string(),
            item: item.to_string(),
            rate,
        });
        source_remaining = source_remaining
            .checked_sub(rate)
            .map_err(map_throughput_arithmetic)?;
        target_remaining = target_remaining
            .checked_sub(rate)
            .map_err(map_throughput_arithmetic)?;

        if source_remaining.is_zero() {
            source_index += 1;
            if source_index < sources.len() {
                source_remaining = source_rate;
            }
        }
        if target_remaining.is_zero() {
            target_index += 1;
            if target_index < targets.len() {
                target_remaining = target_rate;
            }
        }
    }
    Ok(())
}

fn flow_kind_name(kind: ContextualProductionFlowKind) -> &'static str {
    match kind {
        ContextualProductionFlowKind::RecipeFlow => "recipe-flow",
        ContextualProductionFlowKind::ExternalInput => "external-input",
        ContextualProductionFlowKind::TargetOutput => "target-output",
    }
}

fn node_id(node: &FacilityInstanceWiringNode) -> String {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. }
        | FacilityInstanceWiringNode::External { id, .. }
        | FacilityInstanceWiringNode::Target { id, .. }
        | FacilityInstanceWiringNode::Surplus { id, .. } => id.clone(),
    }
}

fn facility_instance_id(occurrence: &str, index: i64) -> String {
    format!("facility-instance:{occurrence}:{index}")
}

fn surplus_node_id(occurrence: &str, item: &str) -> String {
    format!("surplus:{occurrence}:{item}")
}

fn ceil_rate(rate: Rate) -> Result<i64, FacilityInstanceWiringDiagnostic> {
    if rate.is_zero() {
        return Ok(0);
    }
    let base = rate.numerator / rate.denominator;
    let remainder = rate.numerator % rate.denominator;
    if remainder == 0 {
        Ok(base)
    } else {
        base.checked_add(1).ok_or_else(arithmetic_overflow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipes::{
        ItemAmount, Recipe, RecipeBook, RecipeSource, RecipeSourcePlanRequest,
        RecipeSourceSelection, SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION, ThroughputTarget,
        ValidatedRecipeBook, calculate_contextual_facility_requirements,
    };

    fn sum_edge_rates<'a>(edges: impl Iterator<Item = &'a FacilityInstanceWiringEdge>) -> Rate {
        edges.fold(Rate::zero(), |total, edge| {
            total
                .checked_add(edge.rate)
                .expect("test edge rates should add exactly")
        })
    }

    fn single_io_recipe(id: &str, input: &str, output: &str) -> Recipe {
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

    #[test]
    fn expands_one_occurrence_across_multiple_facilities_without_losing_flow() {
        let book = ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: vec!["ore".to_string()],
            recipes: vec![Recipe {
                id: "grind-powder".to_string(),
                facility: "grinder".to_string(),
                inputs: vec![ItemAmount {
                    item: "ore".to_string(),
                    quantity: 1,
                }],
                outputs: vec![ItemAmount {
                    item: "powder".to_string(),
                    quantity: 1,
                }],
                duration_ms: 3000,
            }],
        })
        .expect("facility expansion book should validate");
        let throughput = book.calculate_contextual_throughput(&RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "powder".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections: Vec::new(),
        });
        let requirements = calculate_contextual_facility_requirements(&throughput);

        assert!(requirements.success, "{:#?}", requirements.diagnostics);
        assert_eq!(requirements.occurrence_requirements.len(), 1);
        assert_eq!(
            requirements.occurrence_requirements[0].required_facilities,
            3
        );
        let wiring = build_contextual_facility_instance_wiring(&throughput, &requirements);

        assert!(wiring.success, "{:#?}", wiring.diagnostics);
        assert_eq!(
            wiring
                .nodes
                .iter()
                .filter(|node| matches!(node, FacilityInstanceWiringNode::Facility { .. }))
                .count(),
            3
        );
        assert_eq!(wiring.edges.len(), 6);
        assert_eq!(
            sum_edge_rates(
                wiring
                    .edges
                    .iter()
                    .filter(|edge| edge.kind == "external-input")
            ),
            Rate {
                numerator: 1,
                denominator: 1
            }
        );
        assert_eq!(
            sum_edge_rates(
                wiring
                    .edges
                    .iter()
                    .filter(|edge| edge.kind == "target-output")
            ),
            Rate {
                numerator: 1,
                denominator: 1
            }
        );
        for node in &wiring.nodes {
            if let FacilityInstanceWiringNode::Facility {
                id,
                runs_per_second,
                work_seconds_per_second,
                ..
            } = node
            {
                assert!(id.starts_with("facility-instance:recipe-occurrence:/target:"));
                assert_eq!(
                    *runs_per_second,
                    Rate {
                        numerator: 1,
                        denominator: 3
                    }
                );
                assert_eq!(
                    *work_seconds_per_second,
                    Rate {
                        numerator: 1,
                        denominator: 1
                    }
                );
            }
        }
    }

    #[test]
    fn expands_contextual_cycle_edges_between_occurrence_instances() {
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
        .expect("cyclic facility expansion book should validate");
        let throughput = book.calculate_contextual_throughput(&RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "crop".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections: Vec::new(),
        });
        let requirements = calculate_contextual_facility_requirements(&throughput);
        let wiring = build_contextual_facility_instance_wiring(&throughput, &requirements);

        assert!(wiring.success, "{:#?}", wiring.diagnostics);
        assert_eq!(wiring.nodes.len(), 3);
        assert_eq!(wiring.edges.len(), 3);
        let cycle_edges = wiring
            .edges
            .iter()
            .filter(|edge| edge.kind == "recipe-flow")
            .collect::<Vec<_>>();
        assert_eq!(cycle_edges.len(), 2);
        assert_eq!(
            cycle_edges
                .iter()
                .map(|edge| edge.item.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["crop", "seed"])
        );
    }

    #[test]
    fn keeps_same_item_external_and_internal_wiring_paths_separate() {
        let book = ValidatedRecipeBook::try_from_recipe_book(RecipeBook {
            schema_version: 1,
            external_items: vec!["raw-material".to_string()],
            recipes: vec![
                single_io_recipe("make-shared-a", "raw-material", "shared-material"),
                single_io_recipe("make-shared-b", "raw-material", "shared-material"),
                single_io_recipe("make-left", "shared-material", "left-material"),
                single_io_recipe("make-right", "shared-material", "right-material"),
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
        .expect("contextual path separation book should validate");
        let left_path = "/target/recipe:assemble-target/input:left-material/recipe:make-left/input:shared-material";
        let right_path = "/target/recipe:assemble-target/input:right-material/recipe:make-right/input:shared-material";
        let throughput = book.calculate_contextual_throughput(&RecipeSourcePlanRequest {
            schema_version: SUPPORTED_RECIPE_SOURCE_PLAN_SCHEMA_VERSION,
            target: ThroughputTarget {
                item: "target-material".to_string(),
                quantity: 1,
                duration_ms: 1000,
            },
            source_selections: vec![
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
            ],
        });
        let requirements = calculate_contextual_facility_requirements(&throughput);
        let wiring = build_contextual_facility_instance_wiring(&throughput, &requirements);

        assert!(wiring.success, "{:#?}", wiring.diagnostics);
        let shared_edges = wiring
            .edges
            .iter()
            .filter(|edge| edge.item == "shared-material")
            .collect::<Vec<_>>();
        assert_eq!(shared_edges.len(), 2);
        assert!(shared_edges.iter().any(|edge| {
            edge.kind == "external-input"
                && edge.source == format!("external-input:{left_path}")
                && edge.target.contains("input:left-material")
        }));
        assert!(shared_edges.iter().any(|edge| {
            edge.kind == "recipe-flow"
                && edge.source.contains(right_path)
                && edge.target.contains("input:right-material")
        }));
    }
}
