use serde::Serialize;

use crate::facilities::{FacilityPortDirection, FacilityPortEdge};
use crate::layouts::{FacilityPlacement, FacilityPlacementBounds};
use crate::logistics::{LogisticsComponentKind, TransportKind};
use crate::recipes::Rate;
use crate::research::ModelComplexityMetrics;

use super::WorldGridPosition;

const STAGE: &str = "integrated-layout";
pub const INTEGRATED_LAYOUT_SCHEMA_VERSION: u32 = 17;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IntegratedLayoutStatus {
    Optimal,
    Feasible,
    Infeasible,
    InvalidInput,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IntegratedLayoutReport {
    pub schema_version: u32,
    pub success: bool,
    pub status: IntegratedLayoutStatus,
    pub bounds: Option<FacilityPlacementBounds>,
    pub placements: Vec<FacilityPlacement>,
    pub logistics_components: Vec<PlacedLogisticsComponent>,
    pub transport_networks: Vec<TransportNetwork>,
    pub phases: Vec<IntegratedLayoutPhase>,
    pub exact: Option<ExactSolveReport>,
    pub diagnostics: Vec<IntegratedLayoutDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExactSolveReport {
    pub formulation: &'static str,
    pub model: ExactModelMetrics,
    pub model_complexity: ModelComplexityMetrics,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub incumbent_count: usize,
    pub objective: Option<ExactObjectiveValue>,
    pub objective_stages: Vec<ExactObjectiveStageReport>,
    pub termination: ExactTerminationReason,
    pub proof: ExactProofStatus,
    pub validation: ExactValidationStatus,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct ExactObjectiveValue {
    pub used_bounding_box_area: u64,
    pub physical_transport_tiles: usize,
    pub total_route_turns: usize,
    pub maximum_used_side: i64,
    pub logistics_component_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct ExactObjectiveStageReport {
    pub objective: ExactObjectiveKind,
    pub incumbent: Option<i64>,
    pub best_bound: Option<i64>,
    pub search_ms: u64,
    pub proof: ExactProofStatus,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExactObjectiveKind {
    UsedBoundingBoxArea,
    PhysicalTransportTiles,
    TotalRouteTurns,
    MaximumUsedSide,
    LogisticsComponentCount,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct ExactModelMetrics {
    pub facility_count: usize,
    pub route_requirement_count: usize,
    pub commodity_network_count: usize,
    pub commodity_item_count: usize,
    pub belt_network_count: usize,
    pub pipe_network_count: usize,
    pub network_requirement_reference_count: usize,
    pub network_terminal_count: usize,
    pub external_terminal_count: usize,
    pub maximum_network_flow_scale: i64,
    pub maximum_line_capacity_units: i32,
    pub total_terminal_flow_units: i64,
    pub grid_cell_count: usize,
    pub placement_variables: usize,
    pub endpoint_variables: usize,
    pub route_cell_variables: usize,
    pub route_arc_variables: usize,
    pub network_flow_variables: usize,
    pub branch_component_variables: usize,
    pub objective_variables: usize,
    pub hint_variables: usize,
    pub hinted_placements: usize,
    pub hinted_terminals: usize,
    pub hinted_networks: usize,
    pub hinted_components: usize,
    pub bridge_variables: usize,
    pub bridge_rotation_variables: usize,
    pub crossing_owner_variables: usize,
    pub crossing_constraints: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExactTerminationReason {
    Optimal,
    Feasible,
    Infeasible,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExactProofStatus {
    ProvenOptimal,
    ProvenInfeasible,
    Unproven,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExactValidationStatus {
    Passed,
    Failed,
    NotAttempted,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IntegratedLayoutPhase {
    pub index: usize,
    pub introduced_components: Vec<String>,
    pub introduced_facilities: Vec<String>,
    pub cumulative_facility_count: usize,
    pub cumulative_route_requirement_count: usize,
    pub bounds: FacilityPlacementBounds,
    pub placements: Vec<FacilityPlacement>,
    pub logistics_components: Vec<PlacedLogisticsComponent>,
    pub transport_networks: Vec<TransportNetwork>,
    pub exact: ExactSolveReport,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PlacedLogisticsComponent {
    pub id: String,
    pub component: String,
    pub kind: LogisticsComponentKind,
    pub transport: TransportKind,
    pub position: WorldGridPosition,
    pub rotation: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TransportNetwork {
    pub id: String,
    pub requirement_ids: Vec<String>,
    pub item: String,
    pub transport: TransportKind,
    pub cells: Vec<WorldGridPosition>,
    pub segments: Vec<TransportNetworkSegment>,
    pub terminals: Vec<TransportNetworkTerminal>,
    pub component_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TransportNetworkSegment {
    pub from: WorldGridPosition,
    pub to: WorldGridPosition,
    pub rate: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TransportNetworkTerminal {
    pub id: String,
    pub node: String,
    pub direction: FacilityPortDirection,
    pub endpoint: TransportNetworkEndpoint,
    pub position: WorldGridPosition,
    pub rate: Rate,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum TransportNetworkEndpoint {
    Facility {
        instance: String,
        port: String,
    },
    External {
        node: String,
        side: FacilityPortEdge,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl IntegratedLayoutDiagnostic {
    pub fn error(
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

    pub(super) fn info(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: "/".to_string(),
            entity: None,
            message: message.into(),
        }
    }
}

impl IntegratedLayoutReport {
    pub fn invalid(diagnostic: IntegratedLayoutDiagnostic) -> Self {
        Self::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
    }

    pub(super) fn failure(
        status: IntegratedLayoutStatus,
        diagnostic: IntegratedLayoutDiagnostic,
    ) -> Self {
        Self {
            schema_version: INTEGRATED_LAYOUT_SCHEMA_VERSION,
            success: false,
            status,
            bounds: None,
            placements: Vec::new(),
            logistics_components: Vec::new(),
            transport_networks: Vec::new(),
            phases: Vec::new(),
            exact: None,
            diagnostics: vec![diagnostic],
        }
    }
}
