use std::{
    io::Write,
    path::{Path, PathBuf},
};

use aic_data::facilities::{ValidatedFacilityCatalog, load_facility_catalog};
use aic_data::layouts::{FacilityPlacementRequest, analyze_integrated_layout_search_space};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
    load_item_catalog, load_logistics_component_catalog, load_transport_catalog,
};
use aic_data::recipes::{
    RecipeSourcePlanRequest, ValidatedRecipeBook, build_contextual_facility_instance_wiring,
    calculate_contextual_facility_requirements, load_recipe_book,
};
use aic_data::research::{
    AnalysisInputFileIdentity, AnalysisInputRole, BenchmarkRequestBounds, BenchmarkWorkloadInputs,
    FormulationIdentity, SEARCH_SPACE_ANALYSIS_SCHEMA_VERSION, SearchSpaceAnalysisReport,
    ValidatedBenchmarkWorkloadManifest, WorkloadIdentity, load_benchmark_workload_manifest,
    validate_benchmark_workload_manifest,
};
use anyhow::{Context, Result, ensure};
use clap::{Args, Subcommand, ValueEnum};
use sha2::{Digest, Sha256};

mod connectivity_witness;
mod coordinate_partition;
mod cumulative_growth;
mod dimension_sweep;
mod endpoint_channel;
mod external_connectors;
mod facility_state_partition;
mod factored_networks;
mod first_phase;
mod fixed_dimensions;
mod pair_cliff;
mod physical_occupancy;
mod possible_graph_connectivity;
mod reference_ablation;
mod requirement_cliff;
mod residual_facility_state;
mod routing_state_breakdown;
mod scaled_endpoint_channel;
mod search_mode;
mod shared_layer;
mod transport_tile_cap;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum PortDomainClassificationArg {
    FaithfulBaseline,
    DiagnosticOnly,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum PhysicalOccupancyEncodingArg {
    CandidateCollision,
    CanonicalSharedOccupancy,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum DiagnosticSearchModeArg {
    Optimize,
    FeasibilityOnly,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(super) enum EndpointChannelEncodingArg {
    NestedElement,
    PositiveTable,
}

#[derive(Debug, Args)]
pub(crate) struct FacilityStateResearchArgs {
    #[arg(long, value_name = "FILE")]
    pub(crate) workload: PathBuf,
    #[arg(long, value_name = "DIR", default_value = ".")]
    pub(crate) workspace_root: PathBuf,
    #[arg(long, value_name = "FILE")]
    pub(crate) placement_request: PathBuf,
    #[arg(long, value_name = "INDEX")]
    pub(crate) target_phase: usize,
    #[arg(long, value_name = "CELLS")]
    pub(crate) used_width: i32,
    #[arg(long, value_name = "CELLS")]
    pub(crate) used_height: i32,
    #[arg(long, value_name = "CELL")]
    pub(crate) facility_x: i32,
    #[arg(long, value_name = "CELL")]
    pub(crate) facility_y: i32,
    /// Run the residual prior-overlap ablation instead of the full state portfolio.
    #[arg(long)]
    pub(crate) prior_overlap_ablation: bool,
    /// Fix preceding-phase placements and matching facility ports in every portfolio case.
    #[arg(long, conflicts_with = "prior_overlap_ablation")]
    pub(crate) fix_prior_overlap_facility_state: bool,
    /// Complete introduced-facility port assignment selected by the residual ablation.
    #[arg(long, value_name = "INDEX", requires = "prior_overlap_ablation")]
    pub(crate) port_assignment_index: Option<usize>,
    /// Introduced-facility rotation selected by the residual ablation.
    #[arg(long, value_name = "DEGREES", requires = "prior_overlap_ablation")]
    pub(crate) facility_rotation: Option<i64>,
    #[arg(long, value_name = "COUNT")]
    pub(crate) worker_count: usize,
    #[arg(long, value_name = "MILLISECONDS")]
    pub(crate) prefix_case_time_limit_ms: u64,
    #[arg(long, value_name = "MILLISECONDS")]
    pub(crate) state_case_time_limit_ms: u64,
    #[arg(long, value_name = "DIR")]
    pub(crate) output_dir: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct ScaledEndpointChannelResearchArgs {
    /// Benchmark workload manifest JSON file.
    #[arg(long, value_name = "FILE")]
    pub(crate) workload: PathBuf,
    /// Root used to resolve portable input paths in the workload manifest.
    #[arg(long, value_name = "DIR", default_value = ".")]
    pub(crate) workspace_root: PathBuf,
    /// Fixed used-dimension request for this diagnostic.
    #[arg(long, value_name = "FILE")]
    pub(crate) placement_request: PathBuf,
    /// Zero-based cumulative growth phase containing the introduced facility.
    #[arg(long, value_name = "INDEX")]
    pub(crate) target_phase: usize,
    /// Exact endpoint-channel encoding to measure.
    #[arg(long, value_enum)]
    pub(crate) encoding: EndpointChannelEncodingArg,
    /// JSON artifact path.
    #[arg(long, value_name = "FILE")]
    pub(crate) output: PathBuf,
    /// Self-contained HTML comparison path.
    #[arg(long, value_name = "FILE")]
    pub(crate) visualization_output: PathBuf,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ResearchCommand {
    /// Validate a benchmark workload identity without building a solver model.
    ValidateWorkload {
        /// Benchmark workload manifest JSON file to validate.
        #[arg(long, short, value_name = "FILE")]
        file: PathBuf,
    },
    /// Estimate search-space growth without constructing a Pumpkin model.
    AnalyzeWorkload {
        /// Benchmark workload manifest JSON file to analyze.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Optional JSON artifact path. The full report is always written to stdout.
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Solve only cumulative SCC phase 0 for a controlled exact-model experiment.
    SolveFirstPhase {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Exact solver wall-clock budget in milliseconds.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Optional standalone HTML result, including structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: Option<PathBuf>,
    },
    /// Solve cumulative SCC phases through one target with the v2 exact formulation.
    SolveCumulativeSccGrowth {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based cumulative SCC target phase.
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,

        /// Exact solver wall-clock budget independently given to every phase.
        #[arg(long, value_name = "MILLISECONDS")]
        phase_time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Standalone HTML history or structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Solve one clean subset of phase-zero external connector requirements.
    SolveFirstPhaseExternalSubset {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based phase-zero route indices. Repeat for every selected requirement.
        #[arg(long = "route-index", value_name = "INDEX", required = true)]
        route_indices: Vec<usize>,

        /// Exact solver wall-clock budget in milliseconds.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Standalone HTML result, including structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Solve one diagnostic external connector port domain in isolation.
    SolveFirstPhaseExternalPortDomain {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Stable label for this isolated matrix case.
        #[arg(long, value_name = "ID")]
        case_id: String,

        /// Whether this case retains the full legal port domain.
        #[arg(long, value_enum)]
        classification: PortDomainClassificationArg,

        /// Zero-based phase-zero route index containing one external requirement.
        #[arg(long, value_name = "INDEX")]
        route_index: usize,

        /// Compatible port ID retained by this case. Repeat for every retained port.
        #[arg(long = "port-id", value_name = "PORT", required = true)]
        port_ids: Vec<String>,

        /// Exact solver wall-clock budget in milliseconds.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Standalone HTML result, including structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Decompose the first-phase cliff for one exact two-network model.
    DecomposeFirstPhasePair {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Two zero-based phase-zero network indices. Repeat this flag twice.
        #[arg(long = "network-index", value_name = "INDEX")]
        network_indices: Vec<usize>,

        /// Wall-clock budget for each comparable ablation case.
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,

        /// Wall-clock budget used only to obtain a validated joint reference.
        #[arg(long, value_name = "MILLISECONDS")]
        reference_time_limit_ms: u64,

        /// Directory receiving summary JSON and per-case JSON/HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Compare the dense and shared-layer exact formulations on cumulative SCC phase 0.
    CompareFirstPhaseSharedLayer {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Wall-clock budget given independently to each formulation.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// Directory receiving comparison JSON and dense/shared HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Compare flattened and factored endpoint encodings on the shared-layer phase-0 model.
    CompareFirstPhaseFactoredEndpoints {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Wall-clock budget given independently to each formulation.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// Directory receiving comparison JSON and flattened/factored HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Rebuild the factored shared-layer phase-0 model for every network subset size.
    DecomposeFirstPhaseFactoredNetworks {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Wall-clock budget given independently to every network-subset case.
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,

        /// Optional stable case ID used to run one subset in an isolated process.
        #[arg(long, value_name = "ID")]
        case_id: Option<String>,

        /// Directory receiving summary JSON and per-case JSON/HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Compare optimization and first-solution search on one exact phase-zero network subset.
    DiagnoseFirstPhaseSearchMode {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based phase-zero commodity network index. Repeat for the selected subset.
        #[arg(long = "network-index", value_name = "INDEX", required = true)]
        network_indices: Vec<usize>,

        /// Whether Pumpkin optimizes or stops at the first satisfying assignment.
        #[arg(long, value_enum)]
        search_mode: DiagnosticSearchModeArg,

        /// Exact solver wall-clock budget in milliseconds.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Standalone HTML result, including structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Fix exact used dimensions for one first-solution phase-zero diagnosis.
    DiagnoseFirstPhaseFixedDimensions {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based phase-zero commodity network index. Repeat for the selected subset.
        #[arg(long = "network-index", value_name = "INDEX", required = true)]
        network_indices: Vec<usize>,

        /// Exact actual used width for this partition.
        #[arg(long, value_name = "CELLS")]
        used_width: i32,

        /// Exact actual used height for this partition.
        #[arg(long, value_name = "CELLS")]
        used_height: i32,

        /// Exact solver wall-clock budget in milliseconds.
        #[arg(long, value_name = "MILLISECONDS")]
        time_limit_ms: u64,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Standalone HTML result, including structured failure evidence.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Sweep exact used dimensions with multiple independent Pumpkin workers.
    SweepFirstPhaseFixedDimensions {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based phase-zero commodity network index. Repeat for the selected subset.
        #[arg(long = "network-index", value_name = "INDEX", required = true)]
        network_indices: Vec<usize>,

        /// Independent Pumpkin worker threads.
        #[arg(long, value_name = "COUNT")]
        worker_count: usize,

        /// Wall-clock search budget independently given to every dimension case.
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,

        /// Directory receiving summary and per-case JSON/HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Grow cumulative SCC phases with an exact parallel dimension portfolio.
    SweepCumulativeSccFixedDimensions {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based cumulative SCC target phase.
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,

        /// Independent Pumpkin worker threads in every phase portfolio.
        #[arg(long, value_name = "COUNT")]
        worker_count: usize,

        /// Wall-clock search budget independently given to every dimension case.
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,

        /// Enable the exact watched-demand plus event-driven local-continuation propagator stack.
        #[arg(long)]
        active_local_continuation: bool,

        /// Observe unresolved guarded item equalities without changing solver semantics.
        #[arg(long, requires = "active_local_continuation")]
        observe_guarded_item_intersections: bool,

        /// Actively reject route-arc and bridge guards with no common positive item support.
        #[arg(
            long,
            requires = "active_local_continuation",
            conflicts_with = "observe_guarded_item_intersections"
        )]
        active_guarded_item_intersections: bool,

        /// Directory receiving cumulative, phase, and per-case JSON/HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Partition one cumulative fixed-dimension model by the introduced facility coordinate.
    DiagnoseCumulativeFacilityCoordinates {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based cumulative SCC target phase introducing one facility.
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,

        /// Exact used width for every coordinate case.
        #[arg(long, value_name = "CELLS")]
        used_width: i32,

        /// Exact used height for every coordinate case.
        #[arg(long, value_name = "CELLS")]
        used_height: i32,

        /// Independent Pumpkin worker threads.
        #[arg(long, value_name = "COUNT")]
        worker_count: usize,

        /// Per-dimension case budget used to obtain the preceding phase hint.
        #[arg(long, value_name = "MILLISECONDS")]
        prefix_case_time_limit_ms: u64,

        /// Wall-clock search budget independently given to every coordinate case.
        #[arg(long, value_name = "MILLISECONDS")]
        coordinate_case_time_limit_ms: u64,

        /// Enable the exact watched-demand plus event-driven local-continuation propagator stack.
        #[arg(long)]
        active_local_continuation: bool,

        /// Directory receiving summary JSON/HTML and a representative layout.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Partition one fixed facility coordinate by every compatible port assignment.
    DiagnoseCumulativeFacilityPorts {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based cumulative SCC target phase introducing one facility.
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,

        /// Exact used width for every port-assignment case.
        #[arg(long, value_name = "CELLS")]
        used_width: i32,

        /// Exact used height for every port-assignment case.
        #[arg(long, value_name = "CELLS")]
        used_height: i32,

        /// Fixed X coordinate of the introduced facility.
        #[arg(long, value_name = "CELL")]
        facility_x: i32,

        /// Fixed Y coordinate of the introduced facility.
        #[arg(long, value_name = "CELL")]
        facility_y: i32,

        /// Independent Pumpkin worker threads.
        #[arg(long, value_name = "COUNT")]
        worker_count: usize,

        /// Per-dimension case budget used to obtain the preceding phase hint.
        #[arg(long, value_name = "MILLISECONDS")]
        prefix_case_time_limit_ms: u64,

        /// Wall-clock search budget independently given to every port assignment.
        #[arg(long, value_name = "MILLISECONDS")]
        port_case_time_limit_ms: u64,

        /// Enable the exact watched-demand plus event-driven local-continuation propagator stack.
        #[arg(long)]
        active_local_continuation: bool,

        /// Directory receiving summary JSON/HTML and a representative layout.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Partition one unresolved coordinate and port assignment by facility rotation.
    DiagnoseCumulativeFacilityRotations {
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,
        #[arg(long, value_name = "CELLS")]
        used_width: i32,
        #[arg(long, value_name = "CELLS")]
        used_height: i32,
        #[arg(long, value_name = "CELL")]
        facility_x: i32,
        #[arg(long, value_name = "CELL")]
        facility_y: i32,
        /// Index from the complete facility-port assignment report.
        #[arg(long, value_name = "INDEX")]
        port_assignment_index: usize,
        #[arg(long, value_name = "MILLISECONDS")]
        prefix_case_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        rotation_case_time_limit_ms: u64,
        /// Enable the exact watched-demand plus event-driven local-continuation propagator stack.
        #[arg(long)]
        active_local_continuation: bool,
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Partition facility states, or compare one state with prior-overlap fixations.
    DiagnoseCumulativeFacilityStates(Box<FacilityStateResearchArgs>),
    /// Compare placement, facility-port, and all-terminal reference fixations.
    DiagnosePhase2ReferenceAblation {
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,
        #[arg(long, value_name = "CELLS")]
        used_width: i32,
        #[arg(long, value_name = "CELLS")]
        used_height: i32,
        #[arg(long, value_name = "CELL")]
        facility_x: i32,
        #[arg(long, value_name = "CELL")]
        facility_y: i32,
        #[arg(long, value_name = "INDEX")]
        port_assignment_index: usize,
        #[arg(long, value_name = "MILLISECONDS")]
        prefix_case_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        reference_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Break the Phase 2 routing-only cliff into shared routing state families.
    DiagnosePhase2RoutingStateBreakdown {
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,
        #[arg(long, value_name = "CELLS")]
        used_width: i32,
        #[arg(long, value_name = "CELLS")]
        used_height: i32,
        #[arg(long, value_name = "CELL")]
        facility_x: i32,
        #[arg(long, value_name = "CELL")]
        facility_y: i32,
        #[arg(long, value_name = "INDEX")]
        port_assignment_index: usize,
        #[arg(long, value_name = "MILLISECONDS")]
        prefix_case_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        reference_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Split the Phase 2 route-cell support decision by layer and Boolean value.
    DiagnosePhase2RouteCellBreakdown {
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,
        #[arg(long, value_name = "CELLS")]
        used_width: i32,
        #[arg(long, value_name = "CELLS")]
        used_height: i32,
        #[arg(long, value_name = "CELL")]
        facility_x: i32,
        #[arg(long, value_name = "CELL")]
        facility_y: i32,
        #[arg(long, value_name = "INDEX")]
        port_assignment_index: usize,
        #[arg(long, value_name = "MILLISECONDS")]
        prefix_case_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        reference_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Add an exact source-to-demand proof forest to the fixed-terminal Phase 2 baseline.
    DiagnosePhase2ConnectivityWitness {
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,
        #[arg(long, value_name = "CELLS")]
        used_width: i32,
        #[arg(long, value_name = "CELLS")]
        used_height: i32,
        #[arg(long, value_name = "CELL")]
        facility_x: i32,
        #[arg(long, value_name = "CELL")]
        facility_y: i32,
        #[arg(long, value_name = "INDEX")]
        port_assignment_index: usize,
        #[arg(long, value_name = "MILLISECONDS")]
        prefix_case_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        reference_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Check global possible-route reachability without route-certificate variables.
    DiagnosePhase2PossibleGraphConnectivity {
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,
        #[arg(long, value_name = "CELLS")]
        used_width: i32,
        #[arg(long, value_name = "CELLS")]
        used_height: i32,
        #[arg(long, value_name = "CELL")]
        facility_x: i32,
        #[arg(long, value_name = "CELL")]
        facility_y: i32,
        #[arg(long, value_name = "INDEX")]
        port_assignment_index: usize,
        #[arg(long, value_name = "MILLISECONDS")]
        prefix_case_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        reference_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Compare the unchanged cumulative exact model against physical transport tile caps.
    DiagnoseCumulativeTransportTileCaps {
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,
        #[arg(long, value_name = "INDEX")]
        target_phase: usize,
        #[arg(long, value_name = "CELLS")]
        used_width: i32,
        #[arg(long, value_name = "CELLS")]
        used_height: i32,
        /// Physical belt-plus-pipe tile cap. Repeat to compare multiple exact caps.
        #[arg(long, value_name = "TILES", required = true)]
        transport_tile_cap: Vec<u32>,
        #[arg(long, value_name = "COUNT", default_value_t = 4)]
        prefix_worker_count: usize,
        #[arg(long, value_name = "MILLISECONDS")]
        prefix_case_time_limit_ms: u64,
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Rebuild one factored shared-layer network from each logical requirement subset.
    DecomposeFirstPhaseFactoredRequirements {
        /// Benchmark workload manifest JSON file to solve.
        #[arg(long, value_name = "FILE")]
        workload: PathBuf,

        /// Root used to resolve portable input paths in the workload manifest.
        #[arg(long, value_name = "DIR", default_value = ".")]
        workspace_root: PathBuf,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// Zero-based phase-zero commodity network index to decompose.
        #[arg(long, value_name = "INDEX")]
        network_index: usize,

        /// Wall-clock budget given independently to every requirement-subset case.
        #[arg(long, value_name = "MILLISECONDS")]
        case_time_limit_ms: u64,

        /// Directory receiving summary JSON and per-case JSON/HTML artifacts.
        #[arg(long, value_name = "DIR")]
        output_dir: PathBuf,
    },
    /// Measure root propagation through facility and transport occupancy constraints.
    ProbePhysicalOccupancy {
        /// Validated facility catalog JSON file.
        #[arg(long, value_name = "FILE")]
        facility_catalog: PathBuf,

        /// Stable ID of the 5 by 5 facility used by the controlled probe.
        #[arg(long, value_name = "ID")]
        facility_id: String,

        /// Exact occupancy encoding to measure.
        #[arg(long, value_enum)]
        encoding: PhysicalOccupancyEncodingArg,

        /// Hard maximum layout bounds for this experiment only.
        #[arg(long, value_name = "FILE")]
        placement_request: PathBuf,

        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Self-contained HTML comparison path.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Compare exact endpoint channels under controlled root-domain restrictions.
    ProbeEndpointChannels {
        /// JSON artifact path.
        #[arg(long, value_name = "FILE")]
        output: PathBuf,

        /// Self-contained HTML comparison path.
        #[arg(long, value_name = "FILE")]
        visualization_output: PathBuf,
    },
    /// Scale an exact endpoint channel to the actual introduced facility of a cumulative phase.
    ProbeScaledEndpointChannels(Box<ScaledEndpointChannelResearchArgs>),
}

pub(crate) fn run(command: ResearchCommand) -> Result<bool> {
    match command {
        ResearchCommand::ValidateWorkload { file } => validate_workload(file),
        ResearchCommand::AnalyzeWorkload {
            workload,
            workspace_root,
            placement_request,
            output,
        } => analyze_workload(workload, workspace_root, placement_request, output),
        ResearchCommand::SolveFirstPhase {
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output,
            visualization_output,
        } => first_phase::solve(
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::SolveCumulativeSccGrowth {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            phase_time_limit_ms,
            output,
            visualization_output,
        } => cumulative_growth::solve(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            phase_time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::SolveFirstPhaseExternalSubset {
            workload,
            workspace_root,
            placement_request,
            route_indices,
            time_limit_ms,
            output,
            visualization_output,
        } => external_connectors::solve(
            workload,
            workspace_root,
            placement_request,
            route_indices,
            time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::SolveFirstPhaseExternalPortDomain {
            workload,
            workspace_root,
            placement_request,
            case_id,
            classification,
            route_index,
            port_ids,
            time_limit_ms,
            output,
            visualization_output,
        } => external_connectors::solve_port_domain(
            workload,
            workspace_root,
            placement_request,
            case_id,
            classification,
            route_index,
            port_ids,
            time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::DecomposeFirstPhasePair {
            workload,
            workspace_root,
            placement_request,
            network_indices,
            case_time_limit_ms,
            reference_time_limit_ms,
            output_dir,
        } => pair_cliff::run(
            workload,
            workspace_root,
            placement_request,
            network_indices,
            case_time_limit_ms,
            reference_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::CompareFirstPhaseSharedLayer {
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output_dir,
        } => shared_layer::run(
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output_dir,
            shared_layer::Comparison::SharedLayer,
        ),
        ResearchCommand::CompareFirstPhaseFactoredEndpoints {
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output_dir,
        } => shared_layer::run(
            workload,
            workspace_root,
            placement_request,
            time_limit_ms,
            output_dir,
            shared_layer::Comparison::FactoredEndpoints,
        ),
        ResearchCommand::DecomposeFirstPhaseFactoredNetworks {
            workload,
            workspace_root,
            placement_request,
            case_time_limit_ms,
            case_id,
            output_dir,
        } => factored_networks::run(
            workload,
            workspace_root,
            placement_request,
            case_time_limit_ms,
            case_id,
            output_dir,
        ),
        ResearchCommand::DiagnoseFirstPhaseSearchMode {
            workload,
            workspace_root,
            placement_request,
            network_indices,
            search_mode,
            time_limit_ms,
            output,
            visualization_output,
        } => search_mode::run(
            workload,
            workspace_root,
            placement_request,
            network_indices,
            search_mode,
            time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::DiagnoseFirstPhaseFixedDimensions {
            workload,
            workspace_root,
            placement_request,
            network_indices,
            used_width,
            used_height,
            time_limit_ms,
            output,
            visualization_output,
        } => fixed_dimensions::run(
            workload,
            workspace_root,
            placement_request,
            network_indices,
            used_width,
            used_height,
            time_limit_ms,
            output,
            visualization_output,
        ),
        ResearchCommand::SweepFirstPhaseFixedDimensions {
            workload,
            workspace_root,
            placement_request,
            network_indices,
            worker_count,
            case_time_limit_ms,
            output_dir,
        } => dimension_sweep::run(
            workload,
            workspace_root,
            placement_request,
            network_indices,
            worker_count,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::SweepCumulativeSccFixedDimensions {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            worker_count,
            case_time_limit_ms,
            active_local_continuation,
            observe_guarded_item_intersections,
            active_guarded_item_intersections,
            output_dir,
        } => dimension_sweep::run_cumulative(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            worker_count,
            case_time_limit_ms,
            active_local_continuation,
            observe_guarded_item_intersections,
            active_guarded_item_intersections,
            output_dir,
        ),
        ResearchCommand::DiagnoseCumulativeFacilityCoordinates {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            worker_count,
            prefix_case_time_limit_ms,
            coordinate_case_time_limit_ms,
            active_local_continuation,
            output_dir,
        } => coordinate_partition::run(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            worker_count,
            prefix_case_time_limit_ms,
            coordinate_case_time_limit_ms,
            active_local_continuation,
            output_dir,
        ),
        ResearchCommand::DiagnoseCumulativeFacilityPorts {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            worker_count,
            prefix_case_time_limit_ms,
            port_case_time_limit_ms,
            active_local_continuation,
            output_dir,
        } => coordinate_partition::run_ports(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            worker_count,
            prefix_case_time_limit_ms,
            port_case_time_limit_ms,
            active_local_continuation,
            output_dir,
        ),
        ResearchCommand::DiagnoseCumulativeFacilityRotations {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            rotation_case_time_limit_ms,
            active_local_continuation,
            output_dir,
        } => coordinate_partition::run_rotations(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            rotation_case_time_limit_ms,
            active_local_continuation,
            output_dir,
        ),
        ResearchCommand::DiagnoseCumulativeFacilityStates(args) => {
            if args.prior_overlap_ablation {
                residual_facility_state::run(
                    args.workload,
                    args.workspace_root,
                    args.placement_request,
                    args.target_phase,
                    args.used_width,
                    args.used_height,
                    args.facility_x,
                    args.facility_y,
                    args.port_assignment_index
                        .context("prior-overlap ablation requires --port-assignment-index")?,
                    args.facility_rotation
                        .context("prior-overlap ablation requires --facility-rotation")?,
                    args.worker_count,
                    args.prefix_case_time_limit_ms,
                    args.state_case_time_limit_ms,
                    args.output_dir,
                )
            } else {
                facility_state_partition::run(
                    args.workload,
                    args.workspace_root,
                    args.placement_request,
                    args.target_phase,
                    args.used_width,
                    args.used_height,
                    args.facility_x,
                    args.facility_y,
                    args.worker_count,
                    args.prefix_case_time_limit_ms,
                    args.state_case_time_limit_ms,
                    args.fix_prior_overlap_facility_state,
                    args.output_dir,
                )
            }
        }
        ResearchCommand::DiagnosePhase2ReferenceAblation {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        } => reference_ablation::run(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::DiagnosePhase2RoutingStateBreakdown {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        } => routing_state_breakdown::run(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::DiagnosePhase2RouteCellBreakdown {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        } => routing_state_breakdown::run_route_cells(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::DiagnosePhase2ConnectivityWitness {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        } => connectivity_witness::run(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::DiagnosePhase2PossibleGraphConnectivity {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        } => possible_graph_connectivity::run(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            facility_x,
            facility_y,
            port_assignment_index,
            prefix_case_time_limit_ms,
            reference_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::DiagnoseCumulativeTransportTileCaps {
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            transport_tile_cap,
            prefix_worker_count,
            prefix_case_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        } => transport_tile_cap::run(
            workload,
            workspace_root,
            placement_request,
            target_phase,
            used_width,
            used_height,
            transport_tile_cap,
            prefix_worker_count,
            prefix_case_time_limit_ms,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::DecomposeFirstPhaseFactoredRequirements {
            workload,
            workspace_root,
            placement_request,
            network_index,
            case_time_limit_ms,
            output_dir,
        } => requirement_cliff::run(
            workload,
            workspace_root,
            placement_request,
            network_index,
            case_time_limit_ms,
            output_dir,
        ),
        ResearchCommand::ProbePhysicalOccupancy {
            facility_catalog,
            facility_id,
            encoding,
            placement_request,
            output,
            visualization_output,
        } => physical_occupancy::run(
            facility_catalog,
            facility_id,
            encoding,
            placement_request,
            output,
            visualization_output,
        ),
        ResearchCommand::ProbeEndpointChannels {
            output,
            visualization_output,
        } => endpoint_channel::run(output, visualization_output),
        ResearchCommand::ProbeScaledEndpointChannels(args) => scaled_endpoint_channel::run(
            args.workload,
            args.workspace_root,
            args.placement_request,
            args.target_phase,
            args.encoding,
            args.output,
            args.visualization_output,
        ),
    }
}

fn validate_workload(file: PathBuf) -> Result<bool> {
    let manifest = load_benchmark_workload_manifest(&file)?;
    let report = validate_benchmark_workload_manifest(&manifest);
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
        .context("failed to write benchmark workload validation report")?;
    println!();
    Ok(report.valid)
}

pub(super) struct ResolvedWorkloadPaths {
    pub(super) recipes: PathBuf,
    pub(super) source_plan: PathBuf,
    pub(super) facility_catalog: PathBuf,
    pub(super) item_catalog: PathBuf,
    pub(super) transport_catalog: PathBuf,
    pub(super) logistics_component_catalog: PathBuf,
    pub(super) localization_catalog: Option<PathBuf>,
}

fn analyze_workload(
    workload_path: PathBuf,
    workspace_root: PathBuf,
    placement_request_path: PathBuf,
    output: Option<PathBuf>,
) -> Result<bool> {
    let manifest = load_benchmark_workload_manifest(&workload_path)?;
    let validated = match ValidatedBenchmarkWorkloadManifest::try_from_manifest(manifest) {
        Ok(validated) => validated,
        Err(report) => {
            serde_json::to_writer_pretty(std::io::stdout().lock(), &report)
                .context("failed to write benchmark workload validation report")?;
            println!();
            return Ok(false);
        }
    };
    let manifest = validated.manifest();
    let paths = resolve_workload_paths(&workspace_root, &manifest.inputs);
    let (book, source_plan) = load_contextual_recipe_request(&paths.recipes, &paths.source_plan)?;
    ensure!(
        source_plan.target.item == manifest.expected_target.item
            && source_plan.target.quantity == manifest.expected_target.quantity
            && source_plan.target.duration_ms == manifest.expected_target.duration_ms,
        "benchmark workload '{}' expected target does not match source plan '{}'",
        manifest.id,
        paths.source_plan.display()
    );
    let throughput = book.calculate_contextual_throughput(&source_plan);
    ensure!(throughput.success, "benchmark contextual throughput failed");
    let requirements = calculate_contextual_facility_requirements(&throughput);
    ensure!(
        requirements.success,
        "benchmark facility requirements failed"
    );
    let wiring = build_contextual_facility_instance_wiring(&throughput, &requirements);
    ensure!(wiring.success, "benchmark facility instance wiring failed");

    let facilities =
        ValidatedFacilityCatalog::try_from_catalog(load_facility_catalog(&paths.facility_catalog)?)
            .map_err(|report| anyhow::anyhow!("facility catalog validation failed: {report:?}"))?;
    let items = ValidatedItemCatalog::try_from_catalog(load_item_catalog(&paths.item_catalog)?)
        .map_err(|report| anyhow::anyhow!("item catalog validation failed: {report:?}"))?;
    let transports = ValidatedTransportCatalog::try_from_catalog(load_transport_catalog(
        &paths.transport_catalog,
    )?)
    .map_err(|report| anyhow::anyhow!("transport catalog validation failed: {report:?}"))?;
    let components = ValidatedLogisticsComponentCatalog::try_from_catalog(
        load_logistics_component_catalog(&paths.logistics_component_catalog)?,
    )
    .map_err(|report| anyhow::anyhow!("logistics component validation failed: {report:?}"))?;
    let placement_request_path = workspace_root.join(placement_request_path);
    let placement_request_json =
        std::fs::read_to_string(&placement_request_path).with_context(|| {
            format!(
                "failed to read research placement request '{}'",
                placement_request_path.display()
            )
        })?;
    let placement_request = serde_json::from_str::<FacilityPlacementRequest>(
        &placement_request_json,
    )
    .with_context(|| {
        format!(
            "failed to parse research placement request '{}'",
            placement_request_path.display()
        )
    })?;
    let static_analysis = analyze_integrated_layout_search_space(
        &wiring,
        &facilities,
        &items,
        &transports,
        &components,
        &placement_request,
    )
    .map_err(|diagnostic| {
        anyhow::anyhow!(
            "search-space analysis failed with {}: {}",
            diagnostic.code,
            diagnostic.message
        )
    })?;

    let mut input_identities = input_identities(&paths)?;
    input_identities.push(AnalysisInputFileIdentity {
        role: AnalysisInputRole::PlacementRequest,
        path: placement_request_path.display().to_string(),
        sha256: file_sha256(&placement_request_path)?,
    });
    let report = SearchSpaceAnalysisReport {
        schema_version: SEARCH_SPACE_ANALYSIS_SCHEMA_VERSION,
        workload: WorkloadIdentity {
            workload_id: manifest.id.clone(),
            manifest_sha256: validated.manifest_sha256().to_string(),
            inputs: input_identities,
        },
        formulation: FormulationIdentity {
            formulation: "joint-lexicographic-layout-v5".to_string(),
            solver: "pumpkin".to_string(),
            solver_version: "0.5".to_string(),
            source_revision: None,
            configuration_sha256: text_sha256(
                "joint-lexicographic-layout-v5:iterative-scc-one-ready:circulation-permitted",
            ),
        },
        request_bounds: BenchmarkRequestBounds {
            max_width: u32::try_from(placement_request.max_width)
                .context("research max_width does not fit report domain")?,
            max_height: u32::try_from(placement_request.max_height)
                .context("research max_height does not fit report domain")?,
        },
        ir: static_analysis.ir,
        model_estimate: static_analysis.model_estimate,
        model_actual: None,
        estimate_error: None,
        diagnostics: static_analysis.diagnostics,
    };
    let encoded = serde_json::to_vec_pretty(&report)
        .context("failed to serialize search-space analysis report")?;
    if let Some(output) = output {
        if let Some(parent) = output
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create analysis output directory '{}'",
                    parent.display()
                )
            })?;
        }
        std::fs::write(&output, &encoded)
            .with_context(|| format!("failed to write analysis report '{}'", output.display()))?;
    }
    std::io::stdout()
        .lock()
        .write_all(&encoded)
        .context("failed to write search-space analysis report")?;
    println!();
    Ok(true)
}

pub(super) fn load_contextual_recipe_request(
    recipes: &Path,
    source_plan: &Path,
) -> Result<(ValidatedRecipeBook, RecipeSourcePlanRequest)> {
    let recipe_book = load_recipe_book(recipes)?;
    let book = ValidatedRecipeBook::try_from_recipe_book(recipe_book)
        .map_err(|report| anyhow::anyhow!("recipe validation failed: {report:?}"))?;
    let source_plan_json = std::fs::read_to_string(source_plan).with_context(|| {
        format!(
            "failed to read recipe source-plan request '{}'",
            source_plan.display()
        )
    })?;
    let source_plan = serde_json::from_str(&source_plan_json).with_context(|| {
        format!(
            "failed to parse recipe source-plan request '{}'",
            source_plan.display()
        )
    })?;
    Ok((book, source_plan))
}

pub(super) fn resolve_workload_paths(
    workspace_root: &Path,
    inputs: &BenchmarkWorkloadInputs,
) -> ResolvedWorkloadPaths {
    ResolvedWorkloadPaths {
        recipes: workspace_root.join(&inputs.recipes),
        source_plan: workspace_root.join(&inputs.source_plan),
        facility_catalog: workspace_root.join(&inputs.facility_catalog),
        item_catalog: workspace_root.join(&inputs.item_catalog),
        transport_catalog: workspace_root.join(&inputs.transport_catalog),
        logistics_component_catalog: workspace_root.join(&inputs.logistics_component_catalog),
        localization_catalog: inputs
            .localization_catalog
            .as_ref()
            .map(|path| workspace_root.join(path)),
    }
}

fn input_identities(paths: &ResolvedWorkloadPaths) -> Result<Vec<AnalysisInputFileIdentity>> {
    let mut inputs = vec![
        input_identity(AnalysisInputRole::Recipes, &paths.recipes)?,
        input_identity(AnalysisInputRole::SourcePlan, &paths.source_plan)?,
        input_identity(AnalysisInputRole::FacilityCatalog, &paths.facility_catalog)?,
        input_identity(AnalysisInputRole::ItemCatalog, &paths.item_catalog)?,
        input_identity(
            AnalysisInputRole::TransportCatalog,
            &paths.transport_catalog,
        )?,
        input_identity(
            AnalysisInputRole::LogisticsComponentCatalog,
            &paths.logistics_component_catalog,
        )?,
    ];
    if let Some(localization) = &paths.localization_catalog {
        inputs.push(input_identity(
            AnalysisInputRole::LocalizationCatalog,
            localization,
        )?);
    }
    Ok(inputs)
}

fn input_identity(role: AnalysisInputRole, path: &Path) -> Result<AnalysisInputFileIdentity> {
    Ok(AnalysisInputFileIdentity {
        role,
        path: path.display().to_string(),
        sha256: file_sha256(path)?,
    })
}

fn file_sha256(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)
        .with_context(|| format!("failed to hash research input '{}'", path.display()))?;
    Ok(hex_sha256(Sha256::digest(bytes)))
}

fn text_sha256(value: &str) -> String {
    hex_sha256(Sha256::digest(value.as_bytes()))
}

fn hex_sha256(digest: impl AsRef<[u8]>) -> String {
    digest
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
