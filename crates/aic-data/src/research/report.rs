use serde::{Deserialize, Serialize};

pub const SEARCH_SPACE_ANALYSIS_SCHEMA_VERSION: u32 = 3;
pub const EXPERIMENT_RUN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SearchSpaceAnalysisReport {
    pub schema_version: u32,
    pub workload: WorkloadIdentity,
    pub formulation: FormulationIdentity,
    pub request_bounds: BenchmarkRequestBounds,
    pub ir: IrComplexityMetrics,
    pub model_estimate: ModelComplexityMetrics,
    pub model_actual: Option<ModelComplexityMetrics>,
    pub estimate_error: Option<ModelEstimateError>,
    pub diagnostics: Vec<AnalysisDiagnostic>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct StaticSearchSpaceAnalysis {
    pub ir: IrComplexityMetrics,
    pub model_estimate: ModelComplexityMetrics,
    pub diagnostics: Vec<AnalysisDiagnostic>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct WorkloadIdentity {
    pub workload_id: String,
    pub manifest_sha256: String,
    pub inputs: Vec<AnalysisInputFileIdentity>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AnalysisInputFileIdentity {
    pub role: AnalysisInputRole,
    pub path: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AnalysisInputRole {
    Recipes,
    SourcePlan,
    FacilityCatalog,
    ItemCatalog,
    TransportCatalog,
    LogisticsComponentCatalog,
    LocalizationCatalog,
    PlacementRequest,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FormulationIdentity {
    pub formulation: String,
    pub solver: String,
    pub solver_version: String,
    pub source_revision: Option<String>,
    pub configuration_sha256: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkRequestBounds {
    pub max_width: u32,
    pub max_height: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct IrComplexityMetrics {
    pub facility_count: u64,
    pub facility_type_count: u64,
    pub required_facility_area: u64,
    pub grid_cell_count: u64,
    pub facility_area_slack: Option<u64>,
    pub rotations_per_facility: CountDistribution,
    pub placement_candidates_per_facility: CountDistribution,
    pub placement_log2_volume: f64,
    pub compatible_ports_per_facility_endpoint: CountDistribution,
    pub endpoint_options_per_facility_endpoint: CountDistribution,
    pub endpoint_log2_volume: f64,
    pub logical_wiring_edges: u64,
    pub capacity_split_lanes: u64,
    pub commodity_networks: u64,
    pub belt_networks: u64,
    pub pipe_networks: u64,
    pub terminals_per_network: CountDistribution,
    pub external_terminals: u64,
    pub maximum_flow_scale: u64,
    pub maximum_line_capacity_units: u64,
    pub total_terminal_flow_units: u64,
    pub graph: GraphStructureMetrics,
    pub phases: Vec<PhaseGrowthMetrics>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CountDistribution {
    pub samples: u64,
    pub total: u64,
    pub minimum: u64,
    pub maximum: u64,
    pub p50: u64,
    pub p95: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct GraphStructureMetrics {
    pub vertices: u64,
    pub edges: u64,
    pub weak_components: u64,
    pub mean_degree: f64,
    pub maximum_degree: u64,
    pub p95_degree: u64,
    pub density: f64,
    pub articulation_points: Option<u64>,
    pub biconnected_blocks: Option<u64>,
    pub scc_count: u64,
    pub cyclic_scc_count: u64,
    pub maximum_scc_size: u64,
    pub condensation_depth: u64,
    pub maximum_condensation_width: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PhaseGrowthMetrics {
    pub phase_index: u64,
    pub introduced_scc_ids: Vec<String>,
    pub introduced_facilities: u64,
    pub cumulative_facilities: u64,
    pub introduced_networks: u64,
    pub cumulative_networks: u64,
    pub introduced_terminals: u64,
    pub cumulative_terminals: u64,
    pub frontier_cut_logical_edges: u64,
    pub frontier_cut_networks: u64,
    pub formulation: PhaseFormulationEstimate,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PhaseFormulationEstimate {
    pub coverage: MetricCoverage,
    pub grid_cells: u64,
    pub placement_variables: u64,
    pub endpoint_variables: u64,
    pub route_cell_variables: u64,
    pub route_arc_variables: u64,
    pub flow_variables: u64,
    pub terminal_presence_and_arm_variables: u64,
    pub branch_component_variables: u64,
    pub bridge_variables: u64,
    pub bridge_rotation_variables: u64,
    pub crossing_owner_variables: u64,
    pub covered_variable_lower_bound: u64,
    pub covered_log2_domain_volume: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelComplexityMetrics {
    pub variables: VariableDomainMetrics,
    pub constraints: Option<ConstraintSummaryMetrics>,
    pub factor_graph: Option<FactorGraphMetrics>,
    pub coupling: Option<CouplingMetrics>,
    pub symmetry: Option<SymmetryMetrics>,
    pub estimated_bytes: Option<u64>,
}

impl ModelComplexityMetrics {
    pub fn unavailable() -> Self {
        Self {
            variables: VariableDomainMetrics {
                coverage: MetricCoverage::Unavailable,
                total_variables: 0,
                boolean_variables: 0,
                integer_variables: 0,
                log2_domain_volume: 0.0,
                by_family: Vec::new(),
            },
            constraints: None,
            factor_graph: None,
            coupling: None,
            symmetry: None,
            estimated_bytes: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VariableDomainMetrics {
    pub coverage: MetricCoverage,
    pub total_variables: u64,
    pub boolean_variables: u64,
    pub integer_variables: u64,
    pub log2_domain_volume: f64,
    pub by_family: Vec<VariableFamilyMetrics>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MetricCoverage {
    Complete,
    PartialLowerBound,
    Unavailable,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VariableFamilyMetrics {
    pub family: String,
    pub total_variables: u64,
    pub boolean_variables: u64,
    pub integer_variables: u64,
    pub domains: DomainCardinalitySummary,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DomainCardinalitySummary {
    pub minimum: u64,
    pub maximum: u64,
    pub p50: u64,
    pub p95: u64,
    pub log2_volume: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConstraintSummaryMetrics {
    pub total_constraints: u64,
    pub total_terms: u64,
    pub maximum_arity: u64,
    pub p95_arity: u64,
    pub maximum_absolute_coefficient: u64,
    pub by_family: Vec<ConstraintFamilyMetrics>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ConstraintFamilyMetrics {
    pub family: String,
    pub relation: ConstraintRelation,
    pub constraints: u64,
    pub terms: u64,
    pub maximum_arity: u64,
    pub p95_arity: u64,
    pub maximum_absolute_coefficient: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum ConstraintRelation {
    Equality,
    LessThanOrEqual,
    GreaterThanOrEqual,
    Implication,
    Cardinality,
    Maximum,
    Multiplication,
    Other,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FactorGraphMetrics {
    pub variable_vertices: u64,
    pub constraint_vertices: u64,
    pub incidences: u64,
    pub mean_variable_degree: f64,
    pub maximum_variable_degree: u64,
    pub p95_variable_degree: u64,
    pub mean_constraint_degree: f64,
    pub maximum_constraint_degree: u64,
    pub p95_constraint_degree: u64,
    pub density: f64,
    pub connected_components: Option<u64>,
    pub articulation_points: Option<u64>,
    pub retained_full_graph: bool,
    pub family_incidences: Vec<FamilyIncidenceMetrics>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FamilyIncidenceMetrics {
    pub variable_family: String,
    pub constraint_family: String,
    pub incidences: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CouplingMetrics {
    pub facility_network_incidences: u64,
    pub shared_network_facility_pairs: u64,
    pub cross_family_constraints: u64,
    pub placement_routing_constraints: u64,
    pub placement_routing_incidences: u64,
    pub network_collision_constraints: u64,
    pub objective_incidences: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SymmetryMetrics {
    pub canonical_translation_active: bool,
    pub interchangeable_facilities: Vec<SymmetryGroupMetrics>,
    pub interchangeable_terminals: Vec<SymmetryGroupMetrics>,
    pub equivalent_ports: Vec<SymmetryGroupMetrics>,
    pub candidate_geometric_symmetries: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SymmetryGroupMetrics {
    pub signature: String,
    pub member_count: u64,
    pub estimated_log2_permutations: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModelEstimateError {
    pub variable_count_delta: i64,
    pub constraint_count_delta: i64,
    pub incidence_count_delta: Option<i64>,
    pub log2_domain_volume_delta: f64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AnalysisDiagnostic {
    pub stage: String,
    pub severity: AnalysisDiagnosticSeverity,
    pub code: String,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum AnalysisDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExperimentRunReport {
    pub schema_version: u32,
    pub case: ExperimentCaseIdentity,
    pub analysis: Option<SearchSpaceAnalysisReport>,
    pub build: BuildObservation,
    pub solve: SolveObservation,
    pub validation: ValidationObservation,
    pub resources: ResourceObservation,
    pub artifacts: Vec<ExperimentArtifact>,
    pub diagnostics: Vec<AnalysisDiagnostic>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExperimentCaseIdentity {
    pub suite_id: String,
    pub case_id: String,
    pub workload: WorkloadIdentity,
    pub request_bounds: BenchmarkRequestBounds,
    pub strategy: ExperimentStrategyIdentity,
    pub search_budget_ms: u64,
    pub outer_wall_time_ms: u64,
    pub repetition: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExperimentStrategyIdentity {
    pub id: String,
    pub classification: StrategyClassification,
    pub configuration_sha256: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StrategyClassification {
    SemanticsPreserving,
    SymmetryBreakingOnly,
    Heuristic,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildObservation {
    pub upstream_pipeline_ms: u64,
    pub ir_analysis_ms: u64,
    pub model_construction_ms: u64,
    pub completed: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SolveObservation {
    pub status: SolveStatus,
    pub search_ms: u64,
    pub first_validated_feasible_ms: Option<u64>,
    pub validated_incumbent_count: u64,
    pub objective: Option<ObjectiveValue>,
    pub objective_stages: Vec<ObjectiveStageObservation>,
    pub proof: ProofStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum SolveStatus {
    Optimal,
    Feasible,
    Infeasible,
    Unknown,
    Timeout,
    ResourceLimit,
    Cancelled,
    ConstructionFailed,
    ProcessFailed,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ProofStatus {
    ProvenOptimal,
    ProvenInfeasible,
    Unproven,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveValue {
    pub used_bounding_box_area: u64,
    pub physical_transport_tiles: u64,
    pub total_route_turns: u64,
    pub maximum_used_side: u64,
    pub logistics_component_count: u64,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveStageObservation {
    pub objective: ObjectiveKind,
    pub incumbent: Option<i64>,
    pub best_bound: Option<i64>,
    pub search_ms: u64,
    pub proof: ProofStatus,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ObjectiveKind {
    UsedBoundingBoxArea,
    PhysicalTransportTiles,
    TotalRouteTurns,
    MaximumUsedSide,
    LogisticsComponentCount,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ValidationObservation {
    pub status: ValidationStatus,
    pub diagnostic_codes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationStatus {
    Passed,
    Failed,
    NotAttempted,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ResourceObservation {
    pub total_wall_time_ms: u64,
    pub peak_resident_bytes: Option<u64>,
    pub outer_timeout: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExperimentArtifact {
    pub kind: String,
    pub path: String,
    pub sha256: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_strategy_classification_and_missing_bound_explicitly() {
        let strategy = ExperimentStrategyIdentity {
            id: "exact-baseline".to_string(),
            classification: StrategyClassification::SemanticsPreserving,
            configuration_sha256: "abc".to_string(),
        };
        let stage = ObjectiveStageObservation {
            objective: ObjectiveKind::UsedBoundingBoxArea,
            incumbent: Some(42),
            best_bound: None,
            search_ms: 100,
            proof: ProofStatus::Unproven,
        };

        let strategy = serde_json::to_value(strategy).expect("strategy should serialize");
        let stage = serde_json::to_value(stage).expect("stage should serialize");

        assert_eq!(strategy["classification"], "semantics-preserving");
        assert_eq!(stage["objective"], "used-bounding-box-area");
        assert!(stage["best_bound"].is_null());
        assert_eq!(stage["proof"], "unproven");
    }

    #[test]
    fn rejects_unknown_analysis_contract_fields() {
        let value = serde_json::json!({
            "max_width": 10,
            "max_height": 10,
            "canonical_game_limit": true
        });

        let error = serde_json::from_value::<BenchmarkRequestBounds>(value)
            .expect_err("request bounds must reject invented invariants");

        assert!(
            error
                .to_string()
                .contains("unknown field `canonical_game_limit`")
        );
    }
}
