//! Experimental private drafts with immutable original chain incidence.

pub(crate) mod declarative;
mod geometry;
mod topology;

use aic_data::layouts::{FacilityPlacement, TransportNetworkTerminal};
use aic_data::recipes::Rate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::*;

pub const CONTRACT: &str = "blueprint-edit-v1";

fn fail(code: &str, message: impl Into<String>, entity: impl Into<Value>) -> Diagnostic {
    let mut result = Diagnostic::new(format!("blueprint.{code}"), message, entity);
    result.stage = "blueprint-edit";
    result
}

#[derive(Clone, Debug, Serialize)]
struct Node {
    id: usize,
    network: usize,
    initial_position: Point,
    terminal: Option<TransportNetworkTerminal>,
    components: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
struct Edge {
    id: usize,
    network: usize,
    source: usize,
    target: usize,
    rate: Rate,
    first: Option<usize>,
    last: Option<usize>,
    zero_length: bool,
}

#[derive(Clone, Debug, Serialize)]
struct Blueprint {
    schema: &'static str,
    facilities: Vec<FacilityPlacement>,
    utilizations: std::collections::BTreeMap<String, Rate>,
    networks: Vec<Value>,
    components: Vec<aic_data::layouts::PlacedLogisticsComponent>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Draft {
    placements: Vec<FacilityPlacement>,
    paths: Vec<Option<Vec<Point>>>,
}
