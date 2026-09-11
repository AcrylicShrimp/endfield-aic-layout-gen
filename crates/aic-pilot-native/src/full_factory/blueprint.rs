//! Experimental private drafts with immutable original chain incidence.

mod geometry;
pub(crate) mod python;
mod session;
#[cfg(test)]
mod tests;
mod topology;

use aic_data::layouts::{FacilityPlacement, TransportNetworkTerminal};
use aic_data::recipes::Rate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::*;
pub use session::Session;

pub const CONTRACT: &str = "blueprint-edit-v1";
pub const OBSERVATION: &str = "blueprint-edit-observation-v1";

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

#[derive(Clone, Serialize)]
struct Draft {
    placements: Vec<FacilityPlacement>,
    paths: Vec<Option<Vec<Point>>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub max_actions: u64,
    pub elapsed_ms: u64,
    pub route_expansions: usize,
    pub search_padding: i64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    blueprint_sha256: String,
    revision: u64,
    action: Action,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Action {
    Pose {
        instance: String,
        x: i64,
        y: i64,
        rotation: i64,
    },
    Route {
        edge: usize,
        cells: Vec<Point>,
    },
    Repair {
        edge: usize,
    },
    RepairAll,
    Commit,
    RestoreBest,
}
