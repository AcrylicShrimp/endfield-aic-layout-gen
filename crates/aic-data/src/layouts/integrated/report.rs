use serde::Serialize;

use crate::facilities::FacilityPortEdge;
use crate::layouts::{FacilityPlacement, FacilityPlacementBounds};
use crate::logistics::{LogisticsComponentKind, TransportKind};
use crate::recipes::{FacilityInstanceWiringProjection, Rate};

use super::{DeterministicCandidateKey, LayoutScore, WorldGridPosition};

const STAGE: &str = "integrated-layout";
pub const INTEGRATED_LAYOUT_SCHEMA_VERSION: u32 = 6;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum IntegratedLayoutStatus {
    Optimal,
    Feasible,
    Infeasible,
    InvalidInput,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutReport {
    pub schema_version: u32,
    pub success: bool,
    pub status: IntegratedLayoutStatus,
    pub bounds: Option<FacilityPlacementBounds>,
    pub placements: Vec<FacilityPlacement>,
    pub logistics_components: Vec<PlacedLogisticsComponent>,
    pub routes: Vec<IntegratedRoute>,
    pub phases: Vec<IntegratedLayoutPhase>,
    pub exact: Option<ExactSolveReport>,
    pub diagnostics: Vec<IntegratedLayoutDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExactSolveReport {
    pub formulation: &'static str,
    pub model: ExactModelMetrics,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub incumbent_count: usize,
    pub objective_route_cells: Option<usize>,
    pub best_bound_route_cells: Option<usize>,
    pub termination: ExactTerminationReason,
    pub proof: ExactProofStatus,
    pub validation: ExactValidationStatus,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct ExactModelMetrics {
    pub facility_count: usize,
    pub route_requirement_count: usize,
    pub grid_cell_count: usize,
    pub placement_variables: usize,
    pub endpoint_variables: usize,
    pub route_cell_variables: usize,
    pub route_arc_variables: usize,
    pub route_order_variables: usize,
    pub acyclicity_constraints: usize,
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

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutPhase {
    pub index: usize,
    pub introduced_components: Vec<String>,
    pub introduced_facilities: Vec<String>,
    pub ready_component_count: usize,
    pub selected_component_count: usize,
    pub deferred_component_count: usize,
    pub oversized_component_count: usize,
    pub cumulative_facility_count: usize,
    pub cumulative_route_requirement_count: usize,
    pub prior_placement_hint_count: usize,
    pub bounds: FacilityPlacementBounds,
    pub placements: Vec<FacilityPlacement>,
    pub logistics_components: Vec<PlacedLogisticsComponent>,
    pub routes: Vec<IntegratedRoute>,
    pub route_turns: usize,
    pub route_cells: usize,
    pub bridge_count: usize,
    pub attempts: Vec<IntegratedLayoutPhaseAttempt>,
    pub optimization: IntegratedLayoutPhaseOptimization,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutPhaseAttempt {
    pub candidate_key: Option<DeterministicCandidateKey>,
    pub policy_id: Option<String>,
    pub placement_hint_count: usize,
    pub status: IntegratedLayoutStatus,
    pub diagnostic_code: Option<String>,
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
pub struct IntegratedRoute {
    pub requirement_id: String,
    pub requirement_fingerprint: RouteRequirementFingerprint,
    pub source: IntegratedRouteEndpoint,
    pub target: IntegratedRouteEndpoint,
    pub item: String,
    pub rate: Rate,
    pub transport: TransportKind,
    pub cells: Vec<WorldGridPosition>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RouteRequirementFingerprint {
    pub source: String,
    pub target: String,
    pub item: String,
    pub rate: Rate,
    pub transport: TransportKind,
    pub projection: FacilityInstanceWiringProjection,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum IntegratedRouteEndpoint {
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

    pub(super) fn info_for(
        code: &'static str,
        entity: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: "/".to_string(),
            entity: Some(entity.into()),
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
            routes: Vec::new(),
            phases: Vec::new(),
            exact: None,
            diagnostics: vec![diagnostic],
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutIncumbentSummary {
    pub score: LayoutScore,
    pub candidate_key: DeterministicCandidateKey,
    pub provenance: IncumbentProvenance,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum IncumbentProvenance {
    ExtendedPriorPhase,
    NeighborhoodCandidate {
        neighborhood_rank: usize,
        attempt_index: usize,
    },
    FinalGlobalRefinement {
        attempt_index: usize,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutPhaseOptimization {
    pub search_bounds: FacilityPlacementBounds,
    pub canonical_used_bounds: FacilityPlacementBounds,
    pub initial_incumbent: Option<IntegratedLayoutIncumbentSummary>,
    pub final_incumbent: IntegratedLayoutIncumbentSummary,
    pub score_delta: Option<LayoutScoreDelta>,
    pub candidate_counts: CandidateCounts,
    pub facility_changes: FacilityChangeCounts,
    pub route_changes: RouteChangeCounts,
    pub neighborhoods: Vec<IntegratedLayoutNeighborhoodReport>,
    pub elapsed_ms: PhaseElapsedMilliseconds,
    pub termination_reason: OptimizationTerminationReason,
    pub optimality: OptimizationProofStatus,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedLayoutNeighborhoodReport {
    pub rank: usize,
    pub free_facility_ids: Vec<String>,
    pub fixed_facility_ids: Vec<String>,
    pub invalidated_requirement_ids: Vec<String>,
    pub escalation_causes: Vec<String>,
    pub conflict_codes: Vec<String>,
    pub improved: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct CandidateCounts {
    pub generated: usize,
    pub routed: usize,
    pub validated: usize,
    pub improved: usize,
    pub rejected: usize,
    pub timed_out: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct FacilityChangeCounts {
    pub unchanged_prior: usize,
    pub moved_prior: usize,
    pub newly_placed: usize,
    pub rotation_changed: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct RouteChangeCounts {
    pub reused: usize,
    pub invalidated: usize,
    pub rerouted: usize,
    pub new: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct PhaseElapsedMilliseconds {
    pub graph_construction: u64,
    pub incumbent_extension: u64,
    pub placement: u64,
    pub routing: u64,
    pub validation: Option<u64>,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OptimizationTerminationReason {
    NeighborhoodScheduleExhausted,
    PhaseBudgetExhaustedWithIncumbent,
    PhaseBudgetExhaustedWithoutIncumbent,
    GlobalOptimumProven,
    InvalidInput,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OptimizationProofStatus {
    Proven,
    Unproven,
    NotAttempted,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct LayoutScoreDelta {
    pub total_route_cells: i128,
    pub total_route_turns: i128,
    pub used_bounding_box_area: i128,
    pub maximum_used_side: i128,
    pub physical_transport_tiles: i128,
    pub logistics_component_count: i128,
    pub moved_prior_facility_count: i128,
    pub total_prior_facility_manhattan_displacement: i128,
    pub rotation_change_count: i128,
}

impl LayoutScoreDelta {
    pub fn between(initial: LayoutScore, final_score: LayoutScore) -> Self {
        Self {
            total_route_cells: delta_usize(
                initial.total_route_cells,
                final_score.total_route_cells,
            ),
            total_route_turns: delta_usize(
                initial.total_route_turns,
                final_score.total_route_turns,
            ),
            used_bounding_box_area: delta(
                initial.used_bounding_box_area,
                final_score.used_bounding_box_area,
            ),
            maximum_used_side: delta(initial.maximum_used_side, final_score.maximum_used_side),
            physical_transport_tiles: delta_usize(
                initial.physical_transport_tiles,
                final_score.physical_transport_tiles,
            ),
            logistics_component_count: delta_usize(
                initial.logistics_component_count,
                final_score.logistics_component_count,
            ),
            moved_prior_facility_count: delta_usize(
                initial.moved_prior_facility_count,
                final_score.moved_prior_facility_count,
            ),
            total_prior_facility_manhattan_displacement: delta(
                initial.total_prior_facility_manhattan_displacement,
                final_score.total_prior_facility_manhattan_displacement,
            ),
            rotation_change_count: delta_usize(
                initial.rotation_change_count,
                final_score.rotation_change_count,
            ),
        }
    }
}

fn delta<T>(initial: T, final_value: T) -> i128
where
    T: Into<i128>,
{
    final_value.into() - initial.into()
}

fn delta_usize(initial: usize, final_value: usize) -> i128 {
    final_value as i128 - initial as i128
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_delta_uses_final_minus_initial_for_every_component() {
        let initial = score(10, 4, 100);
        let final_score = score(8, 5, 80);
        let delta = LayoutScoreDelta::between(initial, final_score);

        assert_eq!(delta.total_route_cells, -2);
        assert_eq!(delta.total_route_turns, 1);
        assert_eq!(delta.used_bounding_box_area, -20);
    }

    fn score(route_cells: usize, turns: usize, area: u64) -> LayoutScore {
        LayoutScore {
            total_route_cells: route_cells,
            total_route_turns: turns,
            used_bounding_box_area: area,
            maximum_used_side: 10,
            physical_transport_tiles: route_cells,
            logistics_component_count: 0,
            moved_prior_facility_count: 0,
            total_prior_facility_manhattan_displacement: 0,
            rotation_change_count: 0,
        }
    }
}
