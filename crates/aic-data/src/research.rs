//! Versioned contracts for layout-solver research workloads and observations.
//!
//! This module does not build or solve a constraint model. It defines the external workload
//! identity and the reports that later analysis and benchmark stages will populate.

mod manifest;
mod report;

pub use manifest::{
    BenchmarkTargetIdentity, BenchmarkWorkloadInputs, BenchmarkWorkloadKind,
    BenchmarkWorkloadManifest, BenchmarkWorkloadManifestDiagnostic,
    BenchmarkWorkloadManifestValidationReport, LoadBenchmarkWorkloadManifestError,
    SUPPORTED_BENCHMARK_WORKLOAD_SCHEMA_VERSION, ValidatedBenchmarkWorkloadManifest,
    load_benchmark_workload_manifest, validate_benchmark_workload_manifest,
};
pub use report::{
    AnalysisDiagnostic, AnalysisDiagnosticSeverity, AnalysisInputFileIdentity, AnalysisInputRole,
    BenchmarkRequestBounds, BuildObservation, ConstraintFamilyMetrics, ConstraintRelation,
    ConstraintSummaryMetrics, CountDistribution, CouplingMetrics, DomainCardinalitySummary,
    EXPERIMENT_RUN_SCHEMA_VERSION, ExperimentArtifact, ExperimentCaseIdentity, ExperimentRunReport,
    ExperimentStrategyIdentity, FactorGraphMetrics, FormulationIdentity, GraphStructureMetrics,
    IrComplexityMetrics, ModelComplexityMetrics, ModelEstimateError, ObjectiveKind,
    ObjectiveStageObservation, ObjectiveValue, PhaseGrowthMetrics, ProofStatus,
    ResourceObservation, SEARCH_SPACE_ANALYSIS_SCHEMA_VERSION, SearchSpaceAnalysisReport,
    SolveObservation, SolveStatus, StrategyClassification, SymmetryGroupMetrics, SymmetryMetrics,
    ValidationObservation, ValidationStatus, VariableDomainMetrics, VariableFamilyMetrics,
    WorkloadIdentity,
};
