use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::recipes::{
    FacilityRequirementReport, Rate, RecipeFacilityRequirement, RecipeRunRate,
    RecipeThroughputReport, RecipeWiringEdge, RecipeWiringGraphNode, RecipeWiringGraphReport,
};

mod contextual;

pub use contextual::build_contextual_facility_instance_wiring;

const STAGE: &str = "facility-instance-wiring";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityInstanceWiringReport {
    pub success: bool,
    pub nodes: Vec<FacilityInstanceWiringNode>,
    pub edges: Vec<FacilityInstanceWiringEdge>,
    pub diagnostics: Vec<FacilityInstanceWiringDiagnostic>,
}

impl FacilityInstanceWiringReport {
    fn success(
        nodes: Vec<FacilityInstanceWiringNode>,
        edges: Vec<FacilityInstanceWiringEdge>,
    ) -> Self {
        Self {
            success: true,
            nodes,
            edges,
            diagnostics: vec![FacilityInstanceWiringDiagnostic::info(
                "facility-instance-wiring-built",
                "/",
                None,
                "logical facility instance wiring was built",
            )],
        }
    }

    fn failure(diagnostic: FacilityInstanceWiringDiagnostic) -> Self {
        Self {
            success: false,
            nodes: Vec::new(),
            edges: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FacilityInstanceWiringNode {
    Facility {
        id: String,
        recipe: String,
        facility: String,
        index: i64,
        runs_per_second: Rate,
        work_seconds_per_second: Rate,
        unused_capacity: Rate,
    },
    External {
        id: String,
        item: String,
    },
    Target {
        id: String,
        item: String,
    },
    Surplus {
        id: String,
        item: String,
    },
}

impl FacilityInstanceWiringNode {
    fn facility(
        recipe_rate: &RecipeRunRate,
        index: i64,
        required_facilities: i64,
    ) -> Result<Self, FacilityInstanceWiringDiagnostic> {
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

        Ok(Self::Facility {
            id: facility_instance_id(&recipe_rate.recipe, index),
            recipe: recipe_rate.recipe.clone(),
            facility: recipe_rate.facility.clone(),
            index,
            runs_per_second,
            work_seconds_per_second,
            unused_capacity,
        })
    }

    fn external(item: &str) -> Self {
        Self::External {
            id: item_node_id("external", item),
            item: item.to_string(),
        }
    }

    fn target(item: &str) -> Self {
        Self::Target {
            id: item_node_id("target", item),
            item: item.to_string(),
        }
    }

    fn surplus(item: &str) -> Self {
        Self::Surplus {
            id: item_node_id("surplus", item),
            item: item.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityInstanceWiringEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    pub item: String,
    pub rate: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityInstanceWiringDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl FacilityInstanceWiringDiagnostic {
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

pub fn build_facility_instance_wiring(
    throughput: &RecipeThroughputReport,
    facilities: &FacilityRequirementReport,
    recipe_wiring: &RecipeWiringGraphReport,
) -> FacilityInstanceWiringReport {
    if !throughput.success {
        return FacilityInstanceWiringReport::failure(FacilityInstanceWiringDiagnostic::error(
            "upstream-throughput-failed",
            "/",
            None,
            "facility instance wiring requires a successful throughput report",
        ));
    }

    if !facilities.success {
        return FacilityInstanceWiringReport::failure(FacilityInstanceWiringDiagnostic::error(
            "upstream-facility-requirements-failed",
            "/",
            None,
            "facility instance wiring requires successful facility requirements",
        ));
    }

    if !recipe_wiring.success {
        return FacilityInstanceWiringReport::failure(FacilityInstanceWiringDiagnostic::error(
            "upstream-recipe-wiring-failed",
            "/",
            None,
            "facility instance wiring requires a successful recipe wiring graph",
        ));
    }

    let Some(target) = &throughput.target else {
        return FacilityInstanceWiringReport::failure(FacilityInstanceWiringDiagnostic::error(
            "missing-target",
            "/target",
            None,
            "successful throughput report must contain a target",
        ));
    };

    let recipe_rates = recipe_rate_lookup(throughput);
    let requirements = match facility_requirement_lookup(facilities, &recipe_rates) {
        Ok(requirements) => requirements,
        Err(diagnostic) => return FacilityInstanceWiringReport::failure(diagnostic),
    };
    let recipe_instances = match build_recipe_instances(throughput, &requirements) {
        Ok(recipe_instances) => recipe_instances,
        Err(diagnostic) => return FacilityInstanceWiringReport::failure(diagnostic),
    };

    let endpoint_sets = EndpointSets::from_throughput(throughput);
    let nodes = graph_nodes(throughput, target, &recipe_instances);
    let edges = match expand_edges(recipe_wiring, &recipe_instances, &endpoint_sets) {
        Ok(edges) => edges,
        Err(diagnostic) => return FacilityInstanceWiringReport::failure(diagnostic),
    };

    FacilityInstanceWiringReport::success(nodes, edges)
}

fn recipe_rate_lookup(throughput: &RecipeThroughputReport) -> BTreeMap<&str, &RecipeRunRate> {
    throughput
        .recipe_rates
        .iter()
        .map(|recipe_rate| (recipe_rate.recipe.as_str(), recipe_rate))
        .collect()
}

fn facility_requirement_lookup<'a>(
    facilities: &'a FacilityRequirementReport,
    recipe_rates: &BTreeMap<&str, &RecipeRunRate>,
) -> Result<BTreeMap<&'a str, &'a RecipeFacilityRequirement>, FacilityInstanceWiringDiagnostic> {
    let mut requirements = BTreeMap::new();

    for requirement in &facilities.recipe_requirements {
        if requirements
            .insert(requirement.recipe.as_str(), requirement)
            .is_some()
        {
            return Err(FacilityInstanceWiringDiagnostic::error(
                "duplicate-facility-requirement",
                "/recipe_requirements",
                Some(requirement.recipe.clone()),
                format!(
                    "recipe '{}' has more than one facility requirement",
                    requirement.recipe
                ),
            ));
        }
    }

    for requirement in &facilities.recipe_requirements {
        let Some(recipe_rate) = recipe_rates.get(requirement.recipe.as_str()) else {
            return Err(FacilityInstanceWiringDiagnostic::error(
                "unexpected-facility-requirement",
                "/recipe_requirements",
                Some(requirement.recipe.clone()),
                format!(
                    "facility requirement references unknown recipe '{}'",
                    requirement.recipe
                ),
            ));
        };
        validate_facility_requirement(requirement, recipe_rate)?;
    }

    for recipe in recipe_rates.keys() {
        if !requirements.contains_key(recipe) {
            return Err(FacilityInstanceWiringDiagnostic::error(
                "missing-facility-requirement",
                "/recipe_requirements",
                Some((*recipe).to_string()),
                format!("recipe '{recipe}' has no facility requirement"),
            ));
        }
    }

    Ok(requirements)
}

fn validate_facility_requirement(
    requirement: &RecipeFacilityRequirement,
    recipe_rate: &RecipeRunRate,
) -> Result<(), FacilityInstanceWiringDiagnostic> {
    if requirement.facility != recipe_rate.facility {
        return Err(FacilityInstanceWiringDiagnostic::error(
            "facility-id-mismatch",
            "/recipe_requirements",
            Some(requirement.recipe.clone()),
            format!(
                "recipe '{}' facility requirement uses facility '{}' but throughput uses '{}'",
                requirement.recipe, requirement.facility, recipe_rate.facility
            ),
        ));
    }

    if requirement.work_seconds_per_second != recipe_rate.work_seconds_per_second {
        return Err(FacilityInstanceWiringDiagnostic::error(
            "facility-work-rate-mismatch",
            "/recipe_requirements",
            Some(requirement.recipe.clone()),
            format!(
                "recipe '{}' facility work rate does not match throughput",
                requirement.recipe
            ),
        ));
    }

    let expected_count = ceil_rate(recipe_rate.work_seconds_per_second)?;
    if requirement.required_facilities != expected_count {
        return Err(FacilityInstanceWiringDiagnostic::error(
            "facility-count-mismatch",
            "/recipe_requirements",
            Some(requirement.recipe.clone()),
            format!(
                "recipe '{}' requires {} facilities but expected {}",
                requirement.recipe, requirement.required_facilities, expected_count
            ),
        ));
    }

    Ok(())
}

fn build_recipe_instances(
    throughput: &RecipeThroughputReport,
    requirements: &BTreeMap<&str, &RecipeFacilityRequirement>,
) -> Result<BTreeMap<String, Vec<FacilityInstanceWiringNode>>, FacilityInstanceWiringDiagnostic> {
    let mut instances = BTreeMap::new();

    for recipe_rate in &throughput.recipe_rates {
        let required_facilities = requirements
            .get(recipe_rate.recipe.as_str())
            .expect("facility requirements should already be validated")
            .required_facilities;
        let mut recipe_instances = Vec::new();

        for index in 0..required_facilities {
            recipe_instances.push(FacilityInstanceWiringNode::facility(
                recipe_rate,
                index,
                required_facilities,
            )?);
        }

        instances.insert(recipe_rate.recipe.clone(), recipe_instances);
    }

    Ok(instances)
}

fn graph_nodes(
    throughput: &RecipeThroughputReport,
    target: &crate::recipes::ItemRate,
    recipe_instances: &BTreeMap<String, Vec<FacilityInstanceWiringNode>>,
) -> Vec<FacilityInstanceWiringNode> {
    let mut nodes = Vec::new();

    for external in &throughput.external_input_rates {
        nodes.push(FacilityInstanceWiringNode::external(&external.item));
    }

    for recipe_rate in &throughput.recipe_rates {
        if let Some(instances) = recipe_instances.get(&recipe_rate.recipe) {
            nodes.extend(instances.iter().cloned());
        }
    }

    nodes.push(FacilityInstanceWiringNode::target(&target.item));

    for surplus in &throughput.surplus_rates {
        nodes.push(FacilityInstanceWiringNode::surplus(&surplus.item));
    }

    nodes
}

fn expand_edges(
    recipe_wiring: &RecipeWiringGraphReport,
    recipe_instances: &BTreeMap<String, Vec<FacilityInstanceWiringNode>>,
    endpoint_sets: &EndpointSets,
) -> Result<Vec<FacilityInstanceWiringEdge>, FacilityInstanceWiringDiagnostic> {
    let wiring_node_ids = recipe_wiring
        .nodes
        .iter()
        .map(recipe_wiring_node_id)
        .collect::<BTreeSet<_>>();
    let mut edges = Vec::new();

    for edge in &recipe_wiring.edges {
        if !wiring_node_ids.contains(edge.source.as_str())
            || !wiring_node_ids.contains(edge.target.as_str())
        {
            return Err(unknown_wiring_endpoint(&edge.source, &edge.target));
        }

        expand_edge(&mut edges, edge, recipe_instances, endpoint_sets)?;
    }

    Ok(edges)
}

fn expand_edge(
    edges: &mut Vec<FacilityInstanceWiringEdge>,
    edge: &RecipeWiringEdge,
    recipe_instances: &BTreeMap<String, Vec<FacilityInstanceWiringNode>>,
    endpoint_sets: &EndpointSets,
) -> Result<(), FacilityInstanceWiringDiagnostic> {
    let source = parse_endpoint(&edge.source)?;
    let target = parse_endpoint(&edge.target)?;
    endpoint_sets.validate(&source)?;
    endpoint_sets.validate(&target)?;

    let expected_kind = expected_edge_kind(&source, &target).ok_or_else(|| {
        edge_kind_mismatch(edge, "no supported edge kind exists for endpoint pair")
    })?;
    if edge.kind != expected_kind {
        return Err(edge_kind_mismatch(
            edge,
            format!(
                "edge kind '{}' does not match expected kind '{expected_kind}'",
                edge.kind
            ),
        ));
    }

    match (&source, &target) {
        (Endpoint::External(_), Endpoint::Recipe(target_recipe)) => {
            let targets = instance_ids_for_recipe(recipe_instances, target_recipe)?;
            split_to_targets(edges, &edge.source, &targets, edge)?;
        }
        (Endpoint::Recipe(source_recipe), Endpoint::Recipe(target_recipe)) => {
            let sources = instance_ids_for_recipe(recipe_instances, source_recipe)?;
            let targets = instance_ids_for_recipe(recipe_instances, target_recipe)?;
            balanced_linear_split(edges, &sources, &targets, edge)?;
        }
        (Endpoint::Recipe(source_recipe), Endpoint::Target(_))
        | (Endpoint::Recipe(source_recipe), Endpoint::Surplus(_)) => {
            let sources = instance_ids_for_recipe(recipe_instances, source_recipe)?;
            split_from_sources(edges, &sources, &edge.target, edge)?;
        }
        (Endpoint::External(_), Endpoint::Target(_)) => {
            edges.push(expanded_edge(
                edge.source.clone(),
                edge.target.clone(),
                edge,
                edge.rate,
            ));
        }
        _ => return Err(edge_kind_mismatch(edge, "unsupported endpoint pair")),
    }

    Ok(())
}

fn split_to_targets(
    edges: &mut Vec<FacilityInstanceWiringEdge>,
    source: &str,
    targets: &[String],
    edge: &RecipeWiringEdge,
) -> Result<(), FacilityInstanceWiringDiagnostic> {
    if edge.rate.is_zero() {
        return Ok(());
    }

    let rate = edge
        .rate
        .checked_div_i64(targets.len() as i64)
        .map_err(map_throughput_arithmetic)?;
    for target in targets {
        edges.push(expanded_edge(
            source.to_string(),
            target.clone(),
            edge,
            rate,
        ));
    }

    Ok(())
}

fn split_from_sources(
    edges: &mut Vec<FacilityInstanceWiringEdge>,
    sources: &[String],
    target: &str,
    edge: &RecipeWiringEdge,
) -> Result<(), FacilityInstanceWiringDiagnostic> {
    if edge.rate.is_zero() {
        return Ok(());
    }

    let rate = edge
        .rate
        .checked_div_i64(sources.len() as i64)
        .map_err(map_throughput_arithmetic)?;
    for source in sources {
        edges.push(expanded_edge(
            source.clone(),
            target.to_string(),
            edge,
            rate,
        ));
    }

    Ok(())
}

fn balanced_linear_split(
    edges: &mut Vec<FacilityInstanceWiringEdge>,
    sources: &[String],
    targets: &[String],
    edge: &RecipeWiringEdge,
) -> Result<(), FacilityInstanceWiringDiagnostic> {
    if edge.rate.is_zero() {
        return Ok(());
    }

    let source_rate = edge
        .rate
        .checked_div_i64(sources.len() as i64)
        .map_err(map_throughput_arithmetic)?;
    let target_rate = edge
        .rate
        .checked_div_i64(targets.len() as i64)
        .map_err(map_throughput_arithmetic)?;

    let mut source_index = 0;
    let mut target_index = 0;
    let mut source_remaining = source_rate;
    let mut target_remaining = target_rate;

    while source_index < sources.len() && target_index < targets.len() {
        let rate = source_remaining.min(target_remaining);
        edges.push(expanded_edge(
            sources[source_index].clone(),
            targets[target_index].clone(),
            edge,
            rate,
        ));

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

fn expanded_edge(
    source: String,
    target: String,
    edge: &RecipeWiringEdge,
    rate: Rate,
) -> FacilityInstanceWiringEdge {
    FacilityInstanceWiringEdge {
        source,
        target,
        kind: edge.kind.clone(),
        item: edge.item.clone(),
        rate,
    }
}

fn instance_ids_for_recipe(
    recipe_instances: &BTreeMap<String, Vec<FacilityInstanceWiringNode>>,
    recipe: &str,
) -> Result<Vec<String>, FacilityInstanceWiringDiagnostic> {
    let instances = recipe_instances.get(recipe).ok_or_else(|| {
        FacilityInstanceWiringDiagnostic::error(
            "missing-facility-instances",
            "/recipe_requirements",
            Some(recipe.to_string()),
            format!("recipe '{recipe}' has no facility instance collection"),
        )
    })?;
    if instances.is_empty() {
        return Err(FacilityInstanceWiringDiagnostic::error(
            "missing-facility-instances",
            "/recipe_requirements",
            Some(recipe.to_string()),
            format!("recipe '{recipe}' has no facility instances"),
        ));
    }

    Ok(instances.iter().map(instance_node_id).collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Endpoint {
    Recipe(String),
    External(String),
    Target(String),
    Surplus(String),
}

fn parse_endpoint(id: &str) -> Result<Endpoint, FacilityInstanceWiringDiagnostic> {
    if let Some(recipe) = id.strip_prefix("recipe:") {
        return Ok(Endpoint::Recipe(recipe.to_string()));
    }
    if let Some(item) = id.strip_prefix("external:") {
        return Ok(Endpoint::External(item.to_string()));
    }
    if let Some(item) = id.strip_prefix("target:") {
        return Ok(Endpoint::Target(item.to_string()));
    }
    if let Some(item) = id.strip_prefix("surplus:") {
        return Ok(Endpoint::Surplus(item.to_string()));
    }

    Err(FacilityInstanceWiringDiagnostic::error(
        "unknown-wiring-endpoint",
        "/edges",
        Some(id.to_string()),
        format!("wiring endpoint '{id}' is not recognized"),
    ))
}

struct EndpointSets {
    recipes: BTreeSet<String>,
    external_items: BTreeSet<String>,
    target_item: String,
    surplus_items: BTreeSet<String>,
}

impl EndpointSets {
    fn from_throughput(throughput: &RecipeThroughputReport) -> Self {
        Self {
            recipes: throughput
                .recipe_rates
                .iter()
                .map(|recipe_rate| recipe_rate.recipe.clone())
                .collect(),
            external_items: throughput
                .external_input_rates
                .iter()
                .map(|rate| rate.item.clone())
                .collect(),
            target_item: throughput
                .target
                .as_ref()
                .expect("target should already be validated")
                .item
                .clone(),
            surplus_items: throughput
                .surplus_rates
                .iter()
                .map(|rate| rate.item.clone())
                .collect(),
        }
    }

    fn validate(&self, endpoint: &Endpoint) -> Result<(), FacilityInstanceWiringDiagnostic> {
        let known = match endpoint {
            Endpoint::Recipe(recipe) => self.recipes.contains(recipe),
            Endpoint::External(item) => self.external_items.contains(item),
            Endpoint::Target(item) => item == &self.target_item,
            Endpoint::Surplus(item) => self.surplus_items.contains(item),
        };

        if known {
            Ok(())
        } else {
            Err(FacilityInstanceWiringDiagnostic::error(
                "unknown-wiring-endpoint",
                "/edges",
                Some(endpoint_id(endpoint)),
                format!("wiring endpoint '{}' is not known", endpoint_id(endpoint)),
            ))
        }
    }
}

fn expected_edge_kind(source: &Endpoint, target: &Endpoint) -> Option<&'static str> {
    match (source, target) {
        (Endpoint::External(_), Endpoint::Recipe(_)) => Some("external-input"),
        (Endpoint::Recipe(_), Endpoint::Recipe(_)) => Some("recipe-flow"),
        (Endpoint::Recipe(_), Endpoint::Target(_))
        | (Endpoint::External(_), Endpoint::Target(_)) => Some("target-output"),
        (Endpoint::Recipe(_), Endpoint::Surplus(_)) => Some("surplus-output"),
        _ => None,
    }
}

fn edge_kind_mismatch(
    edge: &RecipeWiringEdge,
    message: impl Into<String>,
) -> FacilityInstanceWiringDiagnostic {
    FacilityInstanceWiringDiagnostic::error(
        "edge-kind-mismatch",
        "/edges",
        Some(format!("{}->{}", edge.source, edge.target)),
        message,
    )
}

fn unknown_wiring_endpoint(source: &str, target: &str) -> FacilityInstanceWiringDiagnostic {
    FacilityInstanceWiringDiagnostic::error(
        "unknown-wiring-endpoint",
        "/edges",
        Some(format!("{source}->{target}")),
        "recipe wiring edge references a node that is missing from the recipe wiring graph",
    )
}

fn recipe_wiring_node_id(node: &RecipeWiringGraphNode) -> &str {
    match node {
        RecipeWiringGraphNode::External { id, .. }
        | RecipeWiringGraphNode::Recipe { id, .. }
        | RecipeWiringGraphNode::Target { id, .. }
        | RecipeWiringGraphNode::Surplus { id, .. } => id,
    }
}

fn instance_node_id(node: &FacilityInstanceWiringNode) -> String {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. }
        | FacilityInstanceWiringNode::External { id, .. }
        | FacilityInstanceWiringNode::Target { id, .. }
        | FacilityInstanceWiringNode::Surplus { id, .. } => id.clone(),
    }
}

fn endpoint_id(endpoint: &Endpoint) -> String {
    match endpoint {
        Endpoint::Recipe(recipe) => format!("recipe:{recipe}"),
        Endpoint::External(item) => item_node_id("external", item),
        Endpoint::Target(item) => item_node_id("target", item),
        Endpoint::Surplus(item) => item_node_id("surplus", item),
    }
}

fn item_node_id(kind: &str, item: &str) -> String {
    format!("{kind}:{item}")
}

fn facility_instance_id(recipe: &str, index: i64) -> String {
    format!("facility-instance:{recipe}:{index}")
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

fn map_throughput_arithmetic(
    _diagnostic: crate::recipes::ThroughputDiagnostic,
) -> FacilityInstanceWiringDiagnostic {
    arithmetic_overflow()
}

fn arithmetic_overflow() -> FacilityInstanceWiringDiagnostic {
    FacilityInstanceWiringDiagnostic::error(
        "arithmetic-overflow",
        "/",
        None,
        "arithmetic overflow while building facility instance wiring",
    )
}
