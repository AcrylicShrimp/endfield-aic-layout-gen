//! Recipe-independent physical problems and their routed solutions.
//!
//! A connection's node identities, material label and rate belong to the problem;
//! selected physical ports and paths belong to the solution. Catalogs are loaded
//! separately and can be shared by every layout in a batch.

use serde::{Deserialize, Serialize};

use crate::{geometry::Point, logistics::TransportKind, rate::Rate};

pub const SCHEMA: &str = "aic-physical-layout-v3";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    pub schema: String,
    pub catalog_sha256: String,
    pub problem: Problem,
    pub solution: Solution,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Problem {
    pub blocks: Vec<Block>,
    pub terminals: Vec<Terminal>,
    pub connections: Vec<Connection>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: String,
    pub facility: String,
}

/// A physical factory-boundary endpoint. Input terminals supply the factory;
/// output terminals receive target or surplus production.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Terminal {
    pub id: String,
    pub direction: TerminalDirection,
    pub commodity: String,
    pub transport: TransportKind,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TerminalDirection {
    Input,
    Output,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub id: String,
    pub source: String,
    pub target: String,
    /// An opaque flow-equivalence label, not a recipe or catalog item embedding.
    pub commodity: String,
    pub transport: TransportKind,
    pub rate: Rate,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Solution {
    pub poses: Vec<Pose>,
    pub terminal_poses: Vec<TerminalPose>,
    pub routes: Vec<Route>,
    pub bridges: Vec<Bridge>,
    pub junctions: Vec<Junction>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Pose {
    pub block: String,
    pub position: Point,
    pub rotation: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TerminalPose {
    pub terminal: String,
    pub position: Point,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub connection: String,
    /// Present for facility endpoints and absent for boundary terminals.
    pub source_port: Option<String>,
    /// Present for facility endpoints and absent for boundary terminals.
    pub target_port: Option<String>,
    pub cells: Vec<Point>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Bridge {
    pub id: String,
    pub component: String,
    pub position: Point,
    pub rotation: i64,
    pub channels: [BridgeChannel; 2],
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum BridgeAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BridgeChannel {
    pub axis: BridgeAxis,
    pub commodity: String,
    pub connections: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Junction {
    pub id: String,
    pub component: String,
    pub position: Point,
    pub rotation: i64,
    pub connections: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Metrics {
    pub width: i64,
    pub height: i64,
    pub area: u64,
    pub body_cells: u64,
    pub transport_cells: u64,
    pub turns: u64,
    pub occupancy: f64,
    pub weak_components: usize,
    pub branch_blocks: usize,
    pub merge_blocks: usize,
    pub max_degree: usize,
    pub mean_port_distance: f64,
    pub mean_detour: f64,
    pub belt_connections: usize,
    pub pipe_connections: usize,
    pub bridges: usize,
}
