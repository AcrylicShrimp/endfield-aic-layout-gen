use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc as SyncArc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::branching::Brancher;
use pumpkin_solver::core::branching::branchers::dynamic_brancher::DynamicBrancher;
use pumpkin_solver::core::branching::branchers::warm_start::WarmStart;
use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::results::{ProblemSolution, SatisfactionResult};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::boundary_terminals::{self, UsedBoundsVariables};
use super::connectivity_propagator::{
    PossibleRouteArc, PossibleRouteReachabilityArgs, PossibleRouteReachabilityCounters,
    PossibleRouteReachabilityStatistics, PossibleRouteReachabilityTraversalMode,
    PossibleRouteReachabilityWakeMode, PossibleTerminalOption,
};
use super::extract::rate_from_flow_units;
use super::formulation::{
    DIRECTIONS, direction_between, direction_index, external_endpoint_options,
    generate_candidate_geometries, grid_arcs, model_facility_endpoint_options, post_at_most_one,
    rotate_direction,
};
use super::grid_analyzer::{
    DirtyMaterialUniqueSupportChainGridPropagatorArgs, LayerGridAnalyzerCounters,
    LayerGridAnalyzerStatistics, LayerGridMaterial, LayerGridOpportunityAnalyzerArgs,
    LayerGridRule, LayerGridRuleArgs, LocalPositiveFlowContinuationAnalyzerArgs,
    TerminalSupportGridPropagatorArgs, UniqueSupportChainGridPropagatorArgs,
    UniqueSupportChainWakeMode, WatchedDemandUniqueSupportChainGridPropagatorArgs,
};
use super::metrics::{elapsed_millis, finish_report_with_formulation};
use super::objective::{
    ExactObjectives, optimise_lexicographically, post_and, post_count, post_exactly_one_indicator,
    post_sum_variable, require_canonical_origin,
};
use super::recorder::{ConstraintFamily, RecordedModel, VariableFamily};
use super::search_statistics::{MeteredBrancher, SearchEventCounters, capture_search_statistics};
use super::{
    Arc, Candidate, EdgeEndpointOptions, EndpointOption, ModelBridge, ModelInstance, post_arm,
    post_presence,
};
use crate::facilities::{FacilityPortDirection, FacilityPortEdge};
use crate::layouts::integrated::{
    EndpointInput, ExactModelMetrics, ExactValidationStatus, FacilityPlacement,
    INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic, IntegratedLayoutReport,
    IntegratedLayoutStatus, LayoutScore, ModelInput, PlacedLogisticsComponent, TransportKind,
    TransportNetwork, TransportNetworkEndpoint, TransportNetworkSegment, TransportNetworkTerminal,
    canonicalize_report_geometry, world_position,
};
use crate::logistics::{
    CardinalDirection, LogisticsComponentKind, ValidatedLogisticsComponentCatalog,
};

#[derive(Debug)]
struct SharedBranchComponent {
    transport: TransportKind,
    cell: usize,
    component: String,
    kind: LogisticsComponentKind,
    rotation: i64,
    selected: DomainId,
}

struct SharedLayer {
    transport: TransportKind,
    network_indices: Vec<usize>,
    arcs: Vec<Arc>,
    route_cells: Vec<DomainId>,
    arm_items: Vec<[DomainId; 4]>,
}

#[derive(Clone, Copy)]
enum EndpointEncoding {
    Flattened,
    Factored,
}

#[derive(Clone, Copy)]
enum SearchMode {
    Optimize,
    FeasibilityOnly,
}

#[derive(Clone)]
enum ConnectivityMode {
    None,
    DeclarativeWitness,
    PossibleGraphPropagator {
        counters: SyncArc<PossibleRouteReachabilityCounters>,
        wake_mode: PossibleRouteReachabilityWakeMode,
        traversal_mode: PossibleRouteReachabilityTraversalMode,
        grid_analyzer: Option<(SyncArc<LayerGridAnalyzerCounters>, LayerGridRule)>,
    },
}

#[derive(Clone, Copy)]
pub(in crate::layouts::integrated) enum ReferenceAblationFixation {
    Placements,
    PlacementsAndFacilityPorts,
    PlacementsAndAllTerminals,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct ReferenceRoutingFixation {
    pub route_cells: bool,
    pub route_cell_transport: Option<TransportKind>,
    pub route_cell_value: Option<bool>,
    pub route_cell_network_index: Option<usize>,
    pub route_cell_network_cell_index: Option<usize>,
    pub arm_items: bool,
    pub arc_activation: bool,
    pub arc_flow: bool,
    pub topology_components: bool,
}

#[derive(Clone, Copy)]
pub(in crate::layouts::integrated) struct FixedUsedDimensions {
    pub(in crate::layouts::integrated) width: i32,
    pub(in crate::layouts::integrated) height: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct FixedFacilityCoordinate {
    pub instance: String,
    pub x: i32,
    pub y: i32,
    pub rotation: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct FacilityPortPartitionDomain {
    pub(in crate::layouts::integrated) terminal: String,
    pub(in crate::layouts::integrated) ports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct FixedTerminalPortChoice {
    pub terminal: String,
    pub port: String,
}

struct SharedSearchResult {
    report: IntegratedLayoutReport,
    stages: Vec<crate::layouts::integrated::ExactObjectiveStageReport>,
    search_ms: u64,
    first_incumbent_ms: Option<u64>,
    incumbent_count: usize,
    search_statistics: crate::layouts::integrated::ExactSearchStatistics,
}

#[derive(Clone)]
struct SharedRoutingOption {
    cell: usize,
    arm_direction: CardinalDirection,
    selected: DomainId,
}

#[derive(Clone)]
enum SharedTerminalEndpoint {
    Flattened(Vec<EndpointOption>),
    Factored {
        key: DomainId,
        kind: FactoredEndpointKind,
    },
}

#[derive(Clone)]
enum FactoredEndpointKind {
    Facility {
        instance: String,
        port_choice: DomainId,
        port_ids: Vec<String>,
    },
    External {
        node: String,
    },
}

struct SharedTerminal {
    id: String,
    direction: FacilityPortDirection,
    rate: crate::recipes::Rate,
    flow_units: i32,
    routing_options: Vec<SharedRoutingOption>,
    endpoint: SharedTerminalEndpoint,
}

#[derive(Clone, Copy)]
struct PlacementChoice {
    choice: DomainId,
}

#[derive(Clone)]
struct FactoredEndpointSelector {
    facility_key: DomainId,
    port_choice: DomainId,
    port_ids: Vec<String>,
    instance: String,
    facility_keys: Vec<i32>,
}

struct FactoredEdgeEndpoints {
    source: FactoredTerminalView,
    target: FactoredTerminalView,
}

struct FactoredTerminalView {
    key: DomainId,
    kind: FactoredEndpointKind,
    reachable_keys: Vec<i32>,
}

pub(in crate::layouts::integrated) fn solve(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Flattened,
        None,
        SearchMode::Optimize,
        None,
        None,
        None,
        None,
        None,
        None,
        ConnectivityMode::None,
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    solve_factored_endpoints_with_prior(input, logistics_components, time_limit, None)
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_with_prior(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    prior_solution: Option<&IntegratedLayoutReport>,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        prior_solution,
        SearchMode::Optimize,
        None,
        None,
        None,
        None,
        None,
        None,
        ConnectivityMode::None,
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_feasibility_only(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        None,
        SearchMode::FeasibilityOnly,
        None,
        None,
        None,
        None,
        None,
        None,
        ConnectivityMode::None,
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_feasibility_only(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
) -> IntegratedLayoutReport {
    solve_factored_endpoints_fixed_dimensions_feasibility_only_with_prior(
        input,
        logistics_components,
        time_limit,
        fixed_dimensions,
        None,
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_feasibility_only_with_prior(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    prior_solution: Option<&IntegratedLayoutReport>,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        prior_solution,
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        None,
        None,
        None,
        ConnectivityMode::None,
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_transport_tile_cap_feasibility_only_with_prior(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    transport_tile_upper_bound: i32,
    prior_solution: Option<&IntegratedLayoutReport>,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        prior_solution,
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        None,
        Some(transport_tile_upper_bound),
        None,
        ConnectivityMode::None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_coordinate_feasibility_only_with_prior(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    fixed_coordinate: FixedFacilityCoordinate,
    prior_solution: Option<&IntegratedLayoutReport>,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        prior_solution,
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        Some(fixed_coordinate),
        None,
        None,
        None,
        None,
        ConnectivityMode::None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_coordinate_ports_feasibility_only_with_prior(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    fixed_coordinate: FixedFacilityCoordinate,
    fixed_ports: Vec<FixedTerminalPortChoice>,
    prior_solution: Option<&IntegratedLayoutReport>,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        prior_solution,
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        Some(fixed_coordinate),
        Some(fixed_ports),
        None,
        None,
        None,
        ConnectivityMode::None,
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_ablation(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
    fixation: ReferenceAblationFixation,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(fixation),
        None,
        None,
        ConnectivityMode::None,
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_routing_ablation(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
    routing_fixation: ReferenceRoutingFixation,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        Some(routing_fixation),
        ConnectivityMode::None,
    )
}

#[cfg(test)]
pub(in crate::layouts::integrated) fn solve_factored_endpoints_connectivity_witness(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        None,
        SearchMode::Optimize,
        None,
        None,
        None,
        None,
        None,
        None,
        ConnectivityMode::DeclarativeWitness,
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_connectivity_witness(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> IntegratedLayoutReport {
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::DeclarativeWitness,
    )
}

#[cfg(test)]
pub(in crate::layouts::integrated) fn solve_factored_endpoints_possible_graph_connectivity(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    let counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        None,
        SearchMode::Optimize,
        None,
        None,
        None,
        None,
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters,
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason,
            grid_analyzer: None,
        },
    )
}

#[cfg(test)]
pub(in crate::layouts::integrated) fn solve_factored_endpoints_event_selective_possible_graph_connectivity(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    let counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        None,
        SearchMode::Optimize,
        None,
        None,
        None,
        None,
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters,
            wake_mode: PossibleRouteReachabilityWakeMode::ExclusionPredicates,
            traversal_mode: PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason,
            grid_analyzer: None,
        },
    )
}

#[cfg(test)]
pub(in crate::layouts::integrated) fn solve_factored_endpoints_lazy_possible_graph_connectivity(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    let counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        None,
        SearchMode::Optimize,
        None,
        None,
        None,
        None,
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters,
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: None,
        },
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_possible_graph_connectivity(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (IntegratedLayoutReport, PossibleRouteReachabilityStatistics) {
    let counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason,
            grid_analyzer: None,
        },
    );
    (report, counters.snapshot())
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_event_selective_possible_graph_connectivity(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (IntegratedLayoutReport, PossibleRouteReachabilityStatistics) {
    let counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&counters),
            wake_mode: PossibleRouteReachabilityWakeMode::ExclusionPredicates,
            traversal_mode: PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason,
            grid_analyzer: None,
        },
    );
    (report, counters.snapshot())
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_lazy_possible_graph_connectivity(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (IntegratedLayoutReport, PossibleRouteReachabilityStatistics) {
    let counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: None,
        },
    );
    (report, counters.snapshot())
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_grouped_demand_possible_graph_connectivity(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (IntegratedLayoutReport, PossibleRouteReachabilityStatistics) {
    let counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndGroupedDemands,
            grid_analyzer: None,
        },
    );
    (report, counters.snapshot())
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_demand_silent_possible_graph_connectivity(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (IntegratedLayoutReport, PossibleRouteReachabilityStatistics) {
    let counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&counters),
            wake_mode: PossibleRouteReachabilityWakeMode::PathAndSupplyDomainEvents,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: None,
        },
    );
    (report, counters.snapshot())
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_layer_grid_analysis(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (
    IntegratedLayoutReport,
    PossibleRouteReachabilityStatistics,
    LayerGridAnalyzerStatistics,
) {
    let connectivity_counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let grid_counters = SyncArc::new(LayerGridAnalyzerCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&connectivity_counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: Some((SyncArc::clone(&grid_counters), LayerGridRule::Observe)),
        },
    );
    (
        report,
        connectivity_counters.snapshot(),
        grid_counters.snapshot(),
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_terminal_support_grid_propagation(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (
    IntegratedLayoutReport,
    PossibleRouteReachabilityStatistics,
    LayerGridAnalyzerStatistics,
) {
    let connectivity_counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let grid_counters = SyncArc::new(LayerGridAnalyzerCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&connectivity_counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: Some((
                SyncArc::clone(&grid_counters),
                LayerGridRule::ForceTerminalSupport,
            )),
        },
    );
    (
        report,
        connectivity_counters.snapshot(),
        grid_counters.snapshot(),
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_unique_support_chain_grid_propagation(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (
    IntegratedLayoutReport,
    PossibleRouteReachabilityStatistics,
    LayerGridAnalyzerStatistics,
) {
    let connectivity_counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let grid_counters = SyncArc::new(LayerGridAnalyzerCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&connectivity_counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: Some((
                SyncArc::clone(&grid_counters),
                LayerGridRule::ForceUniqueSupportChain,
            )),
        },
    );
    (
        report,
        connectivity_counters.snapshot(),
        grid_counters.snapshot(),
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_selective_unique_support_chain_grid_propagation(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (
    IntegratedLayoutReport,
    PossibleRouteReachabilityStatistics,
    LayerGridAnalyzerStatistics,
) {
    let connectivity_counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let grid_counters = SyncArc::new(LayerGridAnalyzerCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&connectivity_counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: Some((
                SyncArc::clone(&grid_counters),
                LayerGridRule::ForceUniqueSupportChainSelectiveWake,
            )),
        },
    );
    (
        report,
        connectivity_counters.snapshot(),
        grid_counters.snapshot(),
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_dirty_material_unique_support_chain_grid_propagation(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (
    IntegratedLayoutReport,
    PossibleRouteReachabilityStatistics,
    LayerGridAnalyzerStatistics,
) {
    let connectivity_counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let grid_counters = SyncArc::new(LayerGridAnalyzerCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&connectivity_counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: Some((
                SyncArc::clone(&grid_counters),
                LayerGridRule::ForceDirtyMaterialUniqueSupportChain,
            )),
        },
    );
    (
        report,
        connectivity_counters.snapshot(),
        grid_counters.snapshot(),
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_watched_demand_unique_support_chain_grid_propagation(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (
    IntegratedLayoutReport,
    PossibleRouteReachabilityStatistics,
    LayerGridAnalyzerStatistics,
) {
    let connectivity_counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let grid_counters = SyncArc::new(LayerGridAnalyzerCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&connectivity_counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: Some((
                SyncArc::clone(&grid_counters),
                LayerGridRule::ForceWatchedDemandUniqueSupportChain,
            )),
        },
    );
    (
        report,
        connectivity_counters.snapshot(),
        grid_counters.snapshot(),
    )
}

pub(in crate::layouts::integrated) fn solve_factored_endpoints_fixed_dimensions_reference_watched_demand_with_local_continuation_analysis(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    fixed_dimensions: FixedUsedDimensions,
    reference: &IntegratedLayoutReport,
) -> (
    IntegratedLayoutReport,
    PossibleRouteReachabilityStatistics,
    LayerGridAnalyzerStatistics,
) {
    let connectivity_counters = SyncArc::new(PossibleRouteReachabilityCounters::default());
    let grid_counters = SyncArc::new(LayerGridAnalyzerCounters::default());
    let report = solve_with_endpoint_encoding(
        input,
        logistics_components,
        time_limit,
        EndpointEncoding::Factored,
        Some(reference),
        SearchMode::FeasibilityOnly,
        Some(fixed_dimensions),
        None,
        None,
        Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
        None,
        None,
        ConnectivityMode::PossibleGraphPropagator {
            counters: SyncArc::clone(&connectivity_counters),
            wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
            traversal_mode: PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
            grid_analyzer: Some((
                SyncArc::clone(&grid_counters),
                LayerGridRule::ForceWatchedDemandUniqueSupportChainAndObserveLocalContinuation,
            )),
        },
    );
    (
        report,
        connectivity_counters.snapshot(),
        grid_counters.snapshot(),
    )
}

pub(in crate::layouts::integrated) fn facility_coordinate_partitions(
    input: &ModelInput,
    instance_id: &str,
) -> Result<Vec<FixedFacilityCoordinate>, IntegratedLayoutDiagnostic> {
    let instance = input
        .instances
        .iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "unknown-coordinate-partition-facility",
                "/fixed_coordinate/instance",
                Some(instance_id.to_string()),
                "the coordinate partition facility is not present in the cumulative exact model",
            )
        })?;
    Ok(
        generate_candidate_geometries(instance, input.width, input.height)
            .into_iter()
            .map(|candidate| (candidate.x, candidate.y))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|(x, y)| FixedFacilityCoordinate {
                instance: instance_id.to_string(),
                x,
                y,
                rotation: None,
            })
            .collect(),
    )
}

pub(in crate::layouts::integrated) fn facility_rotations_at_coordinate(
    input: &ModelInput,
    instance_id: &str,
    x: i32,
    y: i32,
) -> Result<Vec<i64>, IntegratedLayoutDiagnostic> {
    let instance = input
        .instances
        .iter()
        .find(|instance| instance.id == instance_id)
        .ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "unknown-rotation-partition-facility",
                "/fixed_rotation/instance",
                Some(instance_id.to_string()),
                "the rotation partition facility is not present in the cumulative exact model",
            )
        })?;
    Ok(
        generate_candidate_geometries(instance, input.width, input.height)
            .into_iter()
            .filter(|candidate| candidate.x == x && candidate.y == y)
            .map(|candidate| candidate.rotation)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    )
}

pub(in crate::layouts::integrated) fn facility_port_partition_domains(
    input: &ModelInput,
    instance_id: &str,
) -> Result<Vec<FacilityPortPartitionDomain>, IntegratedLayoutDiagnostic> {
    if !input
        .instances
        .iter()
        .any(|instance| instance.id == instance_id)
    {
        return Err(IntegratedLayoutDiagnostic::error(
            "unknown-port-partition-facility",
            "/fixed_ports/instance",
            Some(instance_id.to_string()),
            "the port partition facility is not present in the cumulative exact model",
        ));
    }
    let mut domains = Vec::new();
    let mut seen = BTreeSet::new();
    for network in &input.networks {
        for terminal in network.terminals() {
            let edge = &input.edges[terminal.route_index()];
            let endpoint = if terminal.direction() == FacilityPortDirection::Output {
                &edge.source
            } else {
                &edge.target
            };
            let EndpointInput::Facility { instance, ports } = endpoint else {
                continue;
            };
            if instance != instance_id || !seen.insert(terminal.id().to_string()) {
                continue;
            }
            domains.push(FacilityPortPartitionDomain {
                terminal: terminal.id().to_string(),
                ports: ports.iter().map(|port| port.id.clone()).collect(),
            });
        }
    }
    domains.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    Ok(domains)
}

fn solve_with_endpoint_encoding(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
    endpoint_encoding: EndpointEncoding,
    prior_solution: Option<&IntegratedLayoutReport>,
    search_mode: SearchMode,
    fixed_dimensions: Option<FixedUsedDimensions>,
    fixed_coordinate: Option<FixedFacilityCoordinate>,
    fixed_ports: Option<Vec<FixedTerminalPortChoice>>,
    reference_fixation: Option<ReferenceAblationFixation>,
    transport_tile_upper_bound: Option<i32>,
    reference_routing_fixation: Option<ReferenceRoutingFixation>,
    connectivity_mode: ConnectivityMode,
) -> IntegratedLayoutReport {
    if transport_tile_upper_bound.is_some_and(|upper_bound| upper_bound < 0) {
        return IntegratedLayoutReport::invalid(IntegratedLayoutDiagnostic::error(
            "invalid-transport-tile-upper-bound",
            "/transport_tile_upper_bound",
            transport_tile_upper_bound.map(|upper_bound| upper_bound.to_string()),
            "transport tile upper bound must be non-negative",
        ));
    }
    let construction_started = Instant::now();
    let mut model_metrics = initial_metrics(&input);
    model_metrics.boundary_terminal_count = model_metrics.external_terminal_count;
    let cell_count = input.cell_count as usize;
    let mut solver = RecordedModel::default();
    let tag = solver.new_constraint_tag();
    let used_bounds = boundary_terminals::new_used_bounds(&mut solver, &input);
    if let Some(fixed) = fixed_dimensions {
        solver.post_equals(
            ConstraintFamily::ResearchFixation,
            vec![used_bounds.width.scaled(1)],
            fixed.width,
            1,
            tag,
        );
        solver.post_equals(
            ConstraintFamily::ResearchFixation,
            vec![used_bounds.height.scaled(1)],
            fixed.height,
            1,
            tag,
        );
    }

    let (model_instances, placement_choices, facility_occupancy) =
        build_placements(&mut solver, &input, &mut model_metrics, cell_count, tag);
    if model_instances.is_empty() && !input.instances.is_empty() {
        return IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::Infeasible,
            IntegratedLayoutDiagnostic::error(
                "facility-has-no-placement-candidate",
                "/",
                None,
                "a facility has no rotation and origin within the hard layout bounds",
            ),
        );
    }
    if reference_fixation.is_some()
        && let Err(diagnostic) =
            post_reference_placements(&mut solver, &model_instances, prior_solution, tag)
    {
        return IntegratedLayoutReport::invalid(diagnostic);
    }
    if let Some(fixed) = fixed_coordinate.as_ref()
        && let Err(diagnostic) =
            post_fixed_facility_coordinate(&mut solver, &model_instances, fixed, tag)
    {
        return IntegratedLayoutReport::invalid(diagnostic);
    }
    let model_terminals = match endpoint_encoding {
        EndpointEncoding::Flattened => {
            let edge_endpoint_options = build_endpoint_options(
                &mut solver,
                &input,
                &model_instances,
                &mut model_metrics,
                tag,
            );
            build_flattened_terminals(&input, &edge_endpoint_options)
        }
        EndpointEncoding::Factored => build_factored_terminals(
            &mut solver,
            &input,
            &model_instances,
            &placement_choices,
            used_bounds,
            &mut model_metrics,
            tag,
        ),
    };
    if let Some(fixed) = fixed_ports.as_ref()
        && let Err(diagnostic) =
            post_fixed_terminal_ports(&mut solver, &model_terminals, fixed, tag)
    {
        return IntegratedLayoutReport::invalid(diagnostic);
    }
    if let Some(fixation) = reference_fixation
        && !matches!(fixation, ReferenceAblationFixation::Placements)
        && let Err(diagnostic) = post_reference_terminals(
            &mut solver,
            &input,
            &model_terminals,
            prior_solution,
            fixation,
            tag,
        )
    {
        return IntegratedLayoutReport::invalid(diagnostic);
    }

    let mut layers = Vec::new();
    let mut branch_components = Vec::new();
    let mut bridges = Vec::new();
    for transport in [TransportKind::Belt, TransportKind::Pipe] {
        let network_indices = input
            .networks
            .iter()
            .enumerate()
            .filter_map(|(index, network)| (network.transport() == transport).then_some(index))
            .collect::<Vec<_>>();
        if network_indices.is_empty() {
            continue;
        }
        let layer = build_layer(
            &mut solver,
            &input,
            &model_terminals,
            transport,
            network_indices,
            logistics_components,
            &mut model_metrics,
            &mut branch_components,
            &mut bridges,
            tag,
        );
        layers.push(layer);
    }
    match &connectivity_mode {
        ConnectivityMode::None => {}
        ConnectivityMode::DeclarativeWitness => {
            post_connectivity_witness(&mut solver, &input, &layers, &model_terminals, tag);
        }
        ConnectivityMode::PossibleGraphPropagator {
            counters,
            wake_mode,
            traversal_mode,
            grid_analyzer,
        } => {
            post_possible_graph_connectivity(
                &mut solver,
                &input,
                &layers,
                &model_terminals,
                SyncArc::clone(counters),
                *wake_mode,
                *traversal_mode,
                tag,
            );
            if let Some((grid_analyzer_counters, mode)) = grid_analyzer {
                post_layer_grid_analyzer(
                    &mut solver,
                    &input,
                    &layers,
                    &model_terminals,
                    &bridges,
                    SyncArc::clone(grid_analyzer_counters),
                    *mode,
                    tag,
                );
            }
        }
    }
    if let Some(fixation) = reference_routing_fixation
        && let Err(diagnostic) = post_reference_routing_fixation(
            &mut solver,
            &input,
            &layers,
            &branch_components,
            &bridges,
            prior_solution,
            fixation,
            tag,
        )
    {
        return IntegratedLayoutReport::invalid(diagnostic);
    }
    let transport_occupancy = build_transport_occupancy(
        &mut solver,
        &input,
        &facility_occupancy,
        |transport, cell| {
            layers
                .iter()
                .find(|layer| layer.transport == transport)
                .map(|layer| layer.route_cells[cell])
        },
        &mut model_metrics,
        tag,
    );

    let objectives = match build_objectives(
        &mut solver,
        &input,
        &facility_occupancy,
        &transport_occupancy,
        &layers,
        &branch_components,
        &bridges,
        used_bounds,
        &mut model_metrics,
        tag,
    ) {
        Ok(objectives) => objectives,
        Err(diagnostic) => {
            return IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::InvalidInput,
                diagnostic,
            );
        }
    };
    if let Some(upper_bound) = transport_tile_upper_bound {
        solver.post_less_than_or_equals(
            ConstraintFamily::ResearchFixation,
            vec![objectives.physical_transport_tiles.scaled(1)],
            upper_bound,
            1,
            tag,
        );
    }

    let (facility_network_incidences, shared_network_facility_pairs) =
        super::logical_coupling_metrics(&input);
    solver.set_logical_coupling(facility_network_incidences, shared_network_facility_pairs);
    let model_complexity = solver.metrics();
    let construction_ms = elapsed_millis(construction_started.elapsed());
    let solver_hint = super::hint::build_placement_solver_hint(
        prior_solution,
        &model_instances,
        &mut model_metrics,
    );
    let search = match search_mode {
        SearchMode::Optimize => {
            let result = optimise_lexicographically(
                solver.solver_mut(),
                objectives,
                &solver_hint,
                time_limit,
                tag,
                |solution, status| {
                    extract_report(
                        solution,
                        status,
                        &input,
                        &model_instances,
                        &model_terminals,
                        &layers,
                        &branch_components,
                        &bridges,
                    )
                },
            );
            SharedSearchResult {
                report: result.report,
                stages: result.stages,
                search_ms: result.search_ms,
                first_incumbent_ms: result.first_incumbent_ms,
                incumbent_count: result.incumbent_count,
                search_statistics: result.search_statistics,
            }
        }
        SearchMode::FeasibilityOnly => {
            let search_started = Instant::now();
            let hint_variables = solver_hint.assignments.keys().copied().collect::<Vec<_>>();
            let hint_values = solver_hint
                .assignments
                .values()
                .copied()
                .collect::<Vec<_>>();
            let mut branchers: Vec<Box<dyn Brancher>> = Vec::new();
            if !hint_variables.is_empty() {
                branchers.push(Box::new(WarmStart::new(&hint_variables, &hint_values)));
            }
            branchers.push(Box::new(solver.solver_mut().default_brancher()));
            let search_event_counters = SyncArc::new(Mutex::new(SearchEventCounters::default()));
            let mut brancher = MeteredBrancher::new(
                DynamicBrancher::new(branchers),
                SyncArc::clone(&search_event_counters),
            );
            let mut resolver = ResolutionResolver::default();
            let mut termination = time_limit.map(TimeBudget::starting_now);
            let result =
                solver
                    .solver_mut()
                    .satisfy(&mut brancher, &mut termination, &mut resolver);
            let search_ms = elapsed_millis(search_started.elapsed());
            let (report, first_incumbent_ms, incumbent_count, search_statistics) = match result {
                SatisfactionResult::Satisfiable(satisfiable) => {
                    let solution = satisfiable.solution();
                    let search_statistics = capture_search_statistics(
                        satisfiable.solver(),
                        satisfiable.brancher(),
                        satisfiable.conflict_resolver(),
                        &search_event_counters,
                    );
                    (
                        extract_report(
                            &solution,
                            IntegratedLayoutStatus::Feasible,
                            &input,
                            &model_instances,
                            &model_terminals,
                            &layers,
                            &branch_components,
                            &bridges,
                        ),
                        Some(search_ms),
                        1,
                        search_statistics,
                    )
                }
                SatisfactionResult::Unsatisfiable(solver, brancher, resolver) => {
                    let search_statistics = capture_search_statistics(
                        solver,
                        brancher,
                        resolver,
                        &search_event_counters,
                    );
                    (
                        IntegratedLayoutReport::failure(
                            IntegratedLayoutStatus::Infeasible,
                            IntegratedLayoutDiagnostic::error(
                                "integrated-layout-infeasible",
                                "/",
                                None,
                                "facility placement, port selection, and route constraints are infeasible",
                            ),
                        ),
                        None,
                        0,
                        search_statistics,
                    )
                }
                SatisfactionResult::Unknown(solver, brancher, resolver) => {
                    let search_statistics = capture_search_statistics(
                        solver,
                        brancher,
                        resolver,
                        &search_event_counters,
                    );
                    (
                        IntegratedLayoutReport::failure(
                            IntegratedLayoutStatus::Unknown,
                            IntegratedLayoutDiagnostic::error(
                                "integrated-layout-unknown",
                                "/",
                                None,
                                "solver terminated without a solution or proof",
                            ),
                        ),
                        None,
                        0,
                        search_statistics,
                    )
                }
            };
            SharedSearchResult {
                report,
                stages: Vec::new(),
                search_ms,
                first_incumbent_ms,
                incumbent_count,
                search_statistics,
            }
        }
    };
    let mut report = search.report;
    let validation = if report.success {
        match boundary_terminals::validate_witness(&report)
            .and_then(|()| validate_fixed_dimensions(&report, fixed_dimensions))
            .and_then(|()| validate_fixed_coordinate(&report, fixed_coordinate.as_ref()))
            .and_then(|()| validate_fixed_terminal_ports(&report, fixed_ports.as_deref()))
            .and_then(|()| validate_transport_tile_upper_bound(&report, transport_tile_upper_bound))
            .and_then(|()| {
                crate::layouts::integrated::witness::validate(&input, logistics_components, &report)
            }) {
            Ok(()) => match super::validate_objective_witness(&report, &search.stages) {
                Ok(()) => ExactValidationStatus::Passed,
                Err(diagnostic) => {
                    report.success = false;
                    report.status = IntegratedLayoutStatus::Unknown;
                    report.diagnostics.push(diagnostic);
                    ExactValidationStatus::Failed
                }
            },
            Err(diagnostic) => {
                report.success = false;
                report.status = IntegratedLayoutStatus::Unknown;
                report.diagnostics.push(diagnostic);
                ExactValidationStatus::Failed
            }
        }
    } else {
        ExactValidationStatus::NotAttempted
    };
    finish_report_with_formulation(
        report,
        match (
            reference_fixation,
            reference_routing_fixation,
            connectivity_mode,
        ) {
            (_, _, ConnectivityMode::DeclarativeWitness) => "joint-shared-v4-connectivity-witness",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    wake_mode: PossibleRouteReachabilityWakeMode::AnyDomainEvent,
                    traversal_mode: PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason,
                    ..
                },
            ) => "joint-shared-v4-possible-graph-connectivity",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    wake_mode: PossibleRouteReachabilityWakeMode::ExclusionPredicates,
                    traversal_mode: PossibleRouteReachabilityTraversalMode::EagerAdjacencyAndReason,
                    ..
                },
            ) => "joint-shared-v4-event-selective-possible-graph-connectivity",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    grid_analyzer: Some((_, LayerGridRule::Observe)),
                    ..
                },
            ) => "joint-shared-v4-layer-grid-opportunity-analysis",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    grid_analyzer: Some((_, LayerGridRule::ForceTerminalSupport)),
                    ..
                },
            ) => "joint-shared-v4-terminal-support-grid-propagation",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    grid_analyzer: Some((_, LayerGridRule::ForceUniqueSupportChain)),
                    ..
                },
            ) => "joint-shared-v4-unique-support-chain-grid-propagation",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    grid_analyzer: Some((_, LayerGridRule::ForceUniqueSupportChainSelectiveWake)),
                    ..
                },
            ) => "joint-shared-v4-selective-unique-support-chain-grid-propagation",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    grid_analyzer: Some((_, LayerGridRule::ForceDirtyMaterialUniqueSupportChain)),
                    ..
                },
            ) => "joint-shared-v4-dirty-material-unique-support-chain-grid-propagation",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    grid_analyzer: Some((_, LayerGridRule::ForceWatchedDemandUniqueSupportChain)),
                    ..
                },
            ) => "joint-shared-v4-watched-demand-unique-support-chain-grid-propagation",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    grid_analyzer: Some((
                        _,
                        LayerGridRule::ForceWatchedDemandUniqueSupportChainAndObserveLocalContinuation,
                    )),
                    ..
                },
            ) => "joint-shared-v4-watched-demand-local-continuation-analysis",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    wake_mode: PossibleRouteReachabilityWakeMode::PathAndSupplyDomainEvents,
                    traversal_mode:
                        PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
                    ..
                },
            ) => "joint-shared-v4-demand-silent-possible-graph-connectivity",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    traversal_mode:
                        PossibleRouteReachabilityTraversalMode::ReachableArcsAndLazyReason,
                    ..
                },
            ) => "joint-shared-v4-lazy-possible-graph-connectivity",
            (
                _,
                _,
                ConnectivityMode::PossibleGraphPropagator {
                    traversal_mode:
                        PossibleRouteReachabilityTraversalMode::ReachableArcsAndGroupedDemands,
                    ..
                },
            ) => "joint-shared-v4-grouped-demand-possible-graph-connectivity",
            (_, _, ConnectivityMode::PossibleGraphPropagator { .. }) => {
                unreachable!("unsupported diagnostic connectivity mode combination")
            }
            (_, Some(_), ConnectivityMode::None) => {
                "joint-shared-v4-reference-routing-state-ablation"
            }
            (Some(ReferenceAblationFixation::Placements), None, ConnectivityMode::None) => {
                "joint-shared-v4-reference-placements-ablation"
            }
            (
                Some(ReferenceAblationFixation::PlacementsAndFacilityPorts),
                None,
                ConnectivityMode::None,
            ) => "joint-shared-v4-reference-placements-facility-ports-ablation",
            (
                Some(ReferenceAblationFixation::PlacementsAndAllTerminals),
                None,
                ConnectivityMode::None,
            ) => "joint-shared-v4-reference-placements-all-terminals-ablation",
            (None, None, ConnectivityMode::None) if transport_tile_upper_bound.is_some() => {
                "joint-shared-boundary-terminals-canonical-occupancy-v4-fixed-dimensions-transport-tile-cap"
            }
            (None, None, ConnectivityMode::None) => match (
                endpoint_encoding,
                fixed_dimensions,
                fixed_coordinate,
                fixed_ports,
            ) {
                (EndpointEncoding::Flattened, _, _, _) => {
                    "joint-shared-transport-layer-canonical-occupancy-v2"
                }
                (EndpointEncoding::Factored, Some(_), Some(_), Some(_)) => {
                    "joint-shared-boundary-terminals-canonical-occupancy-v4-fixed-dimensions-coordinate-port-partition"
                }
                (EndpointEncoding::Factored, Some(_), Some(_), None) => {
                    "joint-shared-boundary-terminals-canonical-occupancy-v4-fixed-dimensions-coordinate-partition"
                }
                (EndpointEncoding::Factored, Some(_), None, None) => {
                    "joint-shared-boundary-terminals-canonical-occupancy-v4-fixed-dimensions"
                }
                (EndpointEncoding::Factored, None, None, None) => {
                    "joint-shared-boundary-terminals-canonical-occupancy-v4"
                }
                (EndpointEncoding::Factored, _, _, _) => {
                    unreachable!("port and coordinate partitions always fix their parent decisions")
                }
            },
        },
        model_metrics,
        model_complexity,
        construction_ms,
        search.search_ms,
        search.first_incumbent_ms,
        search.incumbent_count,
        search.search_statistics,
        validation,
        search.stages,
    )
}

fn validate_transport_tile_upper_bound(
    report: &IntegratedLayoutReport,
    upper_bound: Option<i32>,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let Some(upper_bound) = upper_bound else {
        return Ok(());
    };
    let observed = LayoutScore::from_report(report, &[])
        .expect("successful exact witness has scoreable bounds")
        .physical_transport_tiles;
    if observed <= usize::try_from(upper_bound).expect("validated upper bound is non-negative") {
        return Ok(());
    }
    Err(IntegratedLayoutDiagnostic::error(
        "transport-tile-upper-bound-violated",
        "/transport_networks",
        None,
        format!(
            "validated witness uses {observed} physical transport tiles, exceeding cap {upper_bound}"
        ),
    ))
}

fn post_fixed_terminal_ports(
    solver: &mut RecordedModel,
    model_terminals: &[Vec<SharedTerminal>],
    fixed_ports: &[FixedTerminalPortChoice],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    for fixed in fixed_ports {
        let terminal = model_terminals
            .iter()
            .flatten()
            .find(|terminal| terminal.id == fixed.terminal)
            .ok_or_else(|| {
                IntegratedLayoutDiagnostic::error(
                    "unknown-port-partition-terminal",
                    "/fixed_ports/terminal",
                    Some(fixed.terminal.clone()),
                    "the port partition terminal is not present in the exact model",
                )
            })?;
        let SharedTerminalEndpoint::Factored {
            kind:
                FactoredEndpointKind::Facility {
                    port_choice,
                    port_ids,
                    ..
                },
            ..
        } = &terminal.endpoint
        else {
            return Err(IntegratedLayoutDiagnostic::error(
                "invalid-port-partition-terminal",
                "/fixed_ports/terminal",
                Some(fixed.terminal.clone()),
                "the port partition terminal is not a factored facility endpoint",
            ));
        };
        let port_index = port_ids
            .iter()
            .position(|port| port == &fixed.port)
            .ok_or_else(|| {
                IntegratedLayoutDiagnostic::error(
                    "invalid-port-partition-choice",
                    "/fixed_ports/port",
                    Some(format!("{}:{}", fixed.terminal, fixed.port)),
                    "the selected port is outside the terminal's compatible port domain",
                )
            })?;
        solver.post_equals(
            ConstraintFamily::ResearchFixation,
            vec![port_choice.scaled(1)],
            i32::try_from(port_index).expect("port index fits i32"),
            1,
            tag,
        );
    }
    Ok(())
}

fn post_reference_placements(
    solver: &mut RecordedModel,
    instances: &[ModelInstance],
    reference: Option<&IntegratedLayoutReport>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let reference = successful_ablation_reference(reference)?;
    for instance in instances {
        let placement = reference
            .placements
            .iter()
            .find(|placement| placement.instance == instance.input.id)
            .ok_or_else(|| reference_mismatch("placement", &instance.input.id))?;
        let candidate = instance
            .candidates
            .iter()
            .find(|candidate| {
                candidate.rotation == placement.rotation
                    && i64::from(candidate.x) == placement.x
                    && i64::from(candidate.y) == placement.y
            })
            .ok_or_else(|| reference_mismatch("placement candidate", &instance.input.id))?;
        solver.post_equals(
            ConstraintFamily::ResearchFixation,
            vec![candidate.selected.scaled(1)],
            1,
            1,
            tag,
        );
    }
    Ok(())
}

fn post_reference_terminals(
    solver: &mut RecordedModel,
    input: &ModelInput,
    model_terminals: &[Vec<SharedTerminal>],
    reference: Option<&IntegratedLayoutReport>,
    fixation: ReferenceAblationFixation,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let reference = successful_ablation_reference(reference)?;
    for terminal in model_terminals.iter().flatten() {
        let prior = reference
            .transport_networks
            .iter()
            .flat_map(|network| network.terminals.iter())
            .find(|candidate| candidate.id == terminal.id)
            .ok_or_else(|| reference_mismatch("terminal", &terminal.id))?;
        let SharedTerminalEndpoint::Factored { key, kind } = &terminal.endpoint else {
            return Err(reference_mismatch("factored terminal", &terminal.id));
        };
        match (kind, &prior.endpoint) {
            (
                FactoredEndpointKind::Facility {
                    port_choice,
                    port_ids,
                    ..
                },
                TransportNetworkEndpoint::Facility { port, .. },
            ) => {
                let port_index = port_ids
                    .iter()
                    .position(|candidate| candidate == port)
                    .ok_or_else(|| reference_mismatch("facility port", port))?;
                solver.post_equals(
                    ConstraintFamily::ResearchFixation,
                    vec![port_choice.scaled(1)],
                    i32::try_from(port_index).expect("reference port index fits i32"),
                    1,
                    tag,
                );
            }
            (
                FactoredEndpointKind::External { .. },
                TransportNetworkEndpoint::External { side, .. },
            ) if matches!(
                fixation,
                ReferenceAblationFixation::PlacementsAndAllTerminals
            ) =>
            {
                let x = i32::try_from(prior.position.x)
                    .map_err(|_| reference_mismatch("terminal x", &terminal.id))?;
                let y = i32::try_from(prior.position.y)
                    .map_err(|_| reference_mismatch("terminal y", &terminal.id))?;
                let cell = y
                    .checked_mul(input.width)
                    .and_then(|value| value.checked_add(x))
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| reference_mismatch("terminal cell", &terminal.id))?;
                solver.post_equals(
                    ConstraintFamily::ResearchFixation,
                    vec![key.scaled(1)],
                    geometry_key(cell, edge_direction(*side)),
                    1,
                    tag,
                );
            }
            (FactoredEndpointKind::External { .. }, TransportNetworkEndpoint::External { .. }) => {}
            _ => return Err(reference_mismatch("terminal endpoint", &terminal.id)),
        }
    }
    Ok(())
}

fn post_connectivity_witness(
    solver: &mut RecordedModel,
    input: &ModelInput,
    layers: &[SharedLayer],
    terminals: &[Vec<SharedTerminal>],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let cell_count = usize::try_from(input.cell_count).expect("validated cell count is positive");
    let maximum_depth = input.cell_count;

    for layer in layers {
        for (local_index, network_index) in layer.network_indices.iter().copied().enumerate() {
            let item_code = i32::try_from(local_index + 1).expect("layer item code fits i32");
            let network_name = format!(
                "{}-network-{network_index}",
                match layer.transport {
                    TransportKind::Belt => "belt",
                    TransportKind::Pipe => "pipe",
                }
            );
            let reached = (0..cell_count)
                .map(|cell| {
                    solver.new_variable(
                        VariableFamily::ConnectivityReachability,
                        0,
                        1,
                        format!("{network_name}-cell-{cell}-proof-reached"),
                    )
                })
                .collect::<Vec<_>>();
            let depths = (0..cell_count)
                .map(|cell| {
                    solver.new_variable(
                        VariableFamily::ConnectivityDepth,
                        0,
                        maximum_depth,
                        format!("{network_name}-cell-{cell}-proof-depth"),
                    )
                })
                .collect::<Vec<_>>();
            let roots = (0..cell_count)
                .map(|cell| {
                    post_presence(
                        solver,
                        VariableFamily::ConnectivityRoot,
                        ConstraintFamily::ConnectivityWitness,
                        format!("{network_name}-cell-{cell}-proof-root"),
                        unique_variables(
                            terminals[network_index]
                                .iter()
                                .filter(|terminal| {
                                    terminal.direction == FacilityPortDirection::Output
                                })
                                .flat_map(|terminal| &terminal.routing_options)
                                .filter(move |option| option.cell == cell)
                                .map(|option| option.selected),
                        )
                        .into_iter(),
                        tag,
                    )
                })
                .collect::<Vec<_>>();

            for cell in 0..cell_count {
                solver.post_less_than_or_equals(
                    ConstraintFamily::ConnectivityWitness,
                    vec![depths[cell].scaled(1), reached[cell].scaled(-maximum_depth)],
                    0,
                    maximum_depth.unsigned_abs() as u64,
                    tag,
                );
                let root_condition = solver.solver_mut().new_named_literal_for_predicate(
                    roots[cell].equality_predicate(1),
                    tag,
                    format!("{network_name}-cell-{cell}-is-proof-root"),
                );
                solver.post_implied_equals(
                    ConstraintFamily::ConnectivityWitness,
                    vec![depths[cell].scaled(1)],
                    0,
                    maximum_depth.unsigned_abs() as u64,
                    root_condition,
                    roots[cell],
                    tag,
                );
            }

            for terminal in terminals[network_index]
                .iter()
                .filter(|terminal| terminal.direction == FacilityPortDirection::Input)
            {
                for option in &terminal.routing_options {
                    solver.post_less_than_or_equals(
                        ConstraintFamily::ConnectivityWitness,
                        vec![option.selected.scaled(1), reached[option.cell].scaled(-1)],
                        0,
                        1,
                        tag,
                    );
                }
            }

            let mut incoming_parents = vec![Vec::new(); cell_count];
            for (arc_index, arc) in layer.arcs.iter().enumerate() {
                let parent = solver.new_variable(
                    VariableFamily::ConnectivityParent,
                    0,
                    1,
                    format!(
                        "{network_name}-proof-parent-{arc_index}-{}-{}",
                        arc.from, arc.to
                    ),
                );
                incoming_parents[arc.to].push(parent);
                solver.post_less_than_or_equals(
                    ConstraintFamily::ConnectivityWitness,
                    vec![parent.scaled(1), arc.selected.scaled(-1)],
                    0,
                    1,
                    tag,
                );
                for endpoint in [arc.from, arc.to] {
                    solver.post_less_than_or_equals(
                        ConstraintFamily::ConnectivityWitness,
                        vec![parent.scaled(1), reached[endpoint].scaled(-1)],
                        0,
                        1,
                        tag,
                    );
                }
                let condition = solver.solver_mut().new_named_literal_for_predicate(
                    parent.equality_predicate(1),
                    tag,
                    format!("{network_name}-proof-parent-{arc_index}-selected"),
                );
                let from_direction =
                    direction_index(direction_between(arc.from, arc.to, input.width));
                let to_direction =
                    direction_index(direction_between(arc.to, arc.from, input.width));
                for item in [
                    layer.arm_items[arc.from][from_direction],
                    layer.arm_items[arc.to][to_direction],
                ] {
                    solver.post_implied_equals(
                        ConstraintFamily::ConnectivityWitness,
                        vec![item.scaled(1)],
                        item_code,
                        item_code.unsigned_abs() as u64,
                        condition,
                        parent,
                        tag,
                    );
                }
                solver.post_implied_equals(
                    ConstraintFamily::ConnectivityWitness,
                    vec![depths[arc.to].scaled(1), depths[arc.from].scaled(-1)],
                    1,
                    maximum_depth.unsigned_abs() as u64,
                    condition,
                    parent,
                    tag,
                );
            }

            for cell in 0..cell_count {
                let mut definition = incoming_parents[cell]
                    .iter()
                    .map(|parent| parent.scaled(1))
                    .collect::<Vec<_>>();
                definition.push(roots[cell].scaled(1));
                definition.push(reached[cell].scaled(-1));
                solver.post_equals(ConstraintFamily::ConnectivityWitness, definition, 0, 1, tag);
            }
        }
    }
}

fn post_possible_graph_connectivity(
    solver: &mut RecordedModel,
    input: &ModelInput,
    layers: &[SharedLayer],
    terminals: &[Vec<SharedTerminal>],
    counters: SyncArc<PossibleRouteReachabilityCounters>,
    wake_mode: PossibleRouteReachabilityWakeMode,
    traversal_mode: PossibleRouteReachabilityTraversalMode,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for layer in layers {
        for (local_index, network_index) in layer.network_indices.iter().copied().enumerate() {
            let item_code = i32::try_from(local_index + 1).expect("layer item code fits i32");
            let arcs = layer
                .arcs
                .iter()
                .map(|arc| {
                    let from_direction =
                        direction_index(direction_between(arc.from, arc.to, input.width));
                    let to_direction =
                        direction_index(direction_between(arc.to, arc.from, input.width));
                    PossibleRouteArc {
                        from: arc.from,
                        to: arc.to,
                        selected: arc.selected,
                        from_item: layer.arm_items[arc.from][from_direction],
                        to_item: layer.arm_items[arc.to][to_direction],
                    }
                })
                .collect::<Vec<_>>();
            let terminal_options = |direction| {
                terminals[network_index]
                    .iter()
                    .filter(move |terminal| terminal.direction == direction)
                    .flat_map(|terminal| &terminal.routing_options)
                    .map(|option| PossibleTerminalOption {
                        cell: option.cell,
                        selected: option.selected,
                    })
                    .collect::<Vec<_>>()
            };
            let args = PossibleRouteReachabilityArgs {
                name: format!(
                    "possible-{}-network-{network_index}-reachability",
                    match layer.transport {
                        TransportKind::Belt => "belt",
                        TransportKind::Pipe => "pipe",
                    }
                ),
                cell_count: input.cell_count as usize,
                item_code,
                arcs,
                supplies: terminal_options(FacilityPortDirection::Output),
                demands: terminal_options(FacilityPortDirection::Input),
                constraint_tag: tag,
                counters: SyncArc::clone(&counters),
                wake_mode,
                traversal_mode,
            };
            solver.record_global_constraint(
                ConstraintFamily::ConnectivityPropagator,
                args.variables(),
            );
            let _ = solver.solver_mut().add_propagator(args);
        }
    }
}

fn post_layer_grid_analyzer(
    solver: &mut RecordedModel,
    input: &ModelInput,
    layers: &[SharedLayer],
    terminals: &[Vec<SharedTerminal>],
    bridges: &[ModelBridge],
    counters: SyncArc<LayerGridAnalyzerCounters>,
    rule: LayerGridRule,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for layer in layers {
        let arcs = layer
            .arcs
            .iter()
            .map(|arc| {
                let from_direction =
                    direction_index(direction_between(arc.from, arc.to, input.width));
                let to_direction =
                    direction_index(direction_between(arc.to, arc.from, input.width));
                PossibleRouteArc {
                    from: arc.from,
                    to: arc.to,
                    selected: arc.selected,
                    from_item: layer.arm_items[arc.from][from_direction],
                    to_item: layer.arm_items[arc.to][to_direction],
                }
            })
            .collect::<Vec<_>>();
        let materials = layer
            .network_indices
            .iter()
            .copied()
            .enumerate()
            .map(|(local_index, network_index)| {
                let terminal_options = |direction| {
                    terminals[network_index]
                        .iter()
                        .filter(move |terminal| terminal.direction == direction)
                        .flat_map(|terminal| &terminal.routing_options)
                        .map(|option| PossibleTerminalOption {
                            cell: option.cell,
                            selected: option.selected,
                        })
                        .collect::<Vec<_>>()
                };
                LayerGridMaterial {
                    item_code: i32::try_from(local_index + 1).expect("layer item code fits i32"),
                    supplies: terminal_options(FacilityPortDirection::Output),
                    demands: terminal_options(FacilityPortDirection::Input),
                }
            })
            .collect::<Vec<_>>();
        let args = LayerGridRuleArgs {
            name: format!(
                "{}-layer-grid-opportunity-analyzer",
                match layer.transport {
                    TransportKind::Belt => "belt",
                    TransportKind::Pipe => "pipe",
                }
            ),
            cell_count: input.cell_count as usize,
            arcs,
            materials,
            counters: SyncArc::clone(&counters),
            constraint_tag: tag,
        };
        solver.record_global_constraint(ConstraintFamily::GridAnalyzer, args.variables());
        match rule {
            LayerGridRule::Observe => {
                let _ = solver
                    .solver_mut()
                    .add_propagator(LayerGridOpportunityAnalyzerArgs(args));
            }
            LayerGridRule::ForceTerminalSupport => {
                let _ = solver
                    .solver_mut()
                    .add_propagator(TerminalSupportGridPropagatorArgs(args));
            }
            LayerGridRule::ForceUniqueSupportChain => {
                let _ = solver
                    .solver_mut()
                    .add_propagator(UniqueSupportChainGridPropagatorArgs {
                        rule: args,
                        wake_mode: UniqueSupportChainWakeMode::AnyDomainEvent,
                    });
            }
            LayerGridRule::ForceUniqueSupportChainSelectiveWake => {
                let _ = solver
                    .solver_mut()
                    .add_propagator(UniqueSupportChainGridPropagatorArgs {
                        rule: args,
                        wake_mode: UniqueSupportChainWakeMode::SupportLossEvents,
                    });
            }
            LayerGridRule::ForceDirtyMaterialUniqueSupportChain => {
                let _ = solver
                    .solver_mut()
                    .add_propagator(DirtyMaterialUniqueSupportChainGridPropagatorArgs(args));
            }
            LayerGridRule::ForceWatchedDemandUniqueSupportChain => {
                let _ = solver
                    .solver_mut()
                    .add_propagator(WatchedDemandUniqueSupportChainGridPropagatorArgs(args));
            }
            LayerGridRule::ForceWatchedDemandUniqueSupportChainAndObserveLocalContinuation => {
                let analyzer_args = args.clone();
                let mut bridge_selected_by_cell = vec![None; input.cell_count as usize];
                for bridge in bridges
                    .iter()
                    .filter(|bridge| bridge.transport == layer.transport)
                {
                    bridge_selected_by_cell[bridge.cell] = Some(bridge.selected);
                }
                let _ = solver
                    .solver_mut()
                    .add_propagator(WatchedDemandUniqueSupportChainGridPropagatorArgs(args));
                let _ =
                    solver
                        .solver_mut()
                        .add_propagator(LocalPositiveFlowContinuationAnalyzerArgs {
                            rule: analyzer_args,
                            bridge_selected_by_cell,
                        });
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn post_reference_routing_fixation(
    solver: &mut RecordedModel,
    input: &ModelInput,
    layers: &[SharedLayer],
    branches: &[SharedBranchComponent],
    bridges: &[ModelBridge],
    reference: Option<&IntegratedLayoutReport>,
    fixation: ReferenceRoutingFixation,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let reference = successful_ablation_reference(reference)?;
    if fixation
        .route_cell_network_index
        .is_some_and(|index| index >= input.networks.len())
    {
        return Err(reference_mismatch(
            "route-cell network index",
            &fixation
                .route_cell_network_index
                .expect("checked route-cell network index")
                .to_string(),
        ));
    }
    for layer in layers {
        let mut reference_cells = BTreeSet::new();
        let mut selected_network_cells = BTreeSet::new();
        let mut selected_network_is_in_layer = fixation.route_cell_network_index.is_none();
        let mut reference_arm_items = vec![[0_i32; 4]; input.cell_count as usize];
        let mut reference_arc_flows = BTreeMap::<(usize, usize), i32>::new();

        for (local_index, network_index) in layer.network_indices.iter().copied().enumerate() {
            let network_input = &input.networks[network_index];
            let prior_network = reference
                .transport_networks
                .iter()
                .find(|network| network.id == network_input.id())
                .ok_or_else(|| reference_mismatch("transport network", network_input.id()))?;
            if prior_network.transport != layer.transport
                || prior_network.item != network_input.item()
            {
                return Err(reference_mismatch(
                    "transport network identity",
                    network_input.id(),
                ));
            }
            let item_code = i32::try_from(local_index + 1).expect("layer item code fits i32");
            for position in &prior_network.cells {
                let cell =
                    reference_position_cell(input, position.x, position.y, network_input.id())?;
                reference_cells.insert(cell);
                if fixation.route_cell_network_index == Some(network_index) {
                    selected_network_cells.insert(cell);
                    selected_network_is_in_layer = true;
                }
            }
            for segment in &prior_network.segments {
                let from = reference_position_cell(
                    input,
                    segment.from.x,
                    segment.from.y,
                    network_input.id(),
                )?;
                let to =
                    reference_position_cell(input, segment.to.x, segment.to.y, network_input.id())?;
                if !orthogonally_adjacent(from, to, input.width) {
                    return Err(reference_mismatch(
                        "orthogonal route segment",
                        network_input.id(),
                    ));
                }
                set_reference_arm_item(
                    &mut reference_arm_items,
                    from,
                    direction_index(direction_between(from, to, input.width)),
                    item_code,
                    network_input.id(),
                )?;
                set_reference_arm_item(
                    &mut reference_arm_items,
                    to,
                    direction_index(direction_between(to, from, input.width)),
                    item_code,
                    network_input.id(),
                )?;
                let flow = network_input
                    .flow_units_for_hint(segment.rate)
                    .ok_or_else(|| reference_mismatch("route segment flow", network_input.id()))?;
                if let Some(previous) = reference_arc_flows.insert((from, to), flow)
                    && previous != flow
                {
                    return Err(reference_mismatch(
                        "unique route arc flow",
                        network_input.id(),
                    ));
                }
            }
            for prior_terminal in &prior_network.terminals {
                let cell = reference_position_cell(
                    input,
                    prior_terminal.position.x,
                    prior_terminal.position.y,
                    &prior_terminal.id,
                )?;
                let direction = reference_terminal_arm_direction(input, reference, prior_terminal)?;
                set_reference_arm_item(
                    &mut reference_arm_items,
                    cell,
                    direction_index(direction),
                    item_code,
                    &prior_terminal.id,
                )?;
            }
        }

        if fixation.route_cells {
            if selected_network_is_in_layer {
                let selected_network_cell = fixation
                    .route_cell_network_cell_index
                    .map(|index| {
                        selected_network_cells
                            .iter()
                            .nth(index)
                            .copied()
                            .ok_or_else(|| {
                                reference_mismatch(
                                    "route-cell network cell index",
                                    &index.to_string(),
                                )
                            })
                    })
                    .transpose()?;
                for (cell, variable) in layer.route_cells.iter().copied().enumerate() {
                    let occupied = selected_network_cell.map_or_else(
                        || {
                            fixation.route_cell_network_index.map_or_else(
                                || reference_cells.contains(&cell),
                                |_| selected_network_cells.contains(&cell),
                            )
                        },
                        |selected| selected == cell,
                    );
                    if fixation
                        .route_cell_transport
                        .is_some_and(|transport| transport != layer.transport)
                        || fixation
                            .route_cell_value
                            .is_some_and(|selected| selected != occupied)
                    {
                        continue;
                    }
                    fix_reference_value(solver, variable, i32::from(occupied), tag);
                }
            }
        }
        if fixation.arm_items {
            for (cell, items) in layer.arm_items.iter().enumerate() {
                for (direction, variable) in items.iter().copied().enumerate() {
                    fix_reference_value(
                        solver,
                        variable,
                        reference_arm_items[cell][direction],
                        tag,
                    );
                }
            }
        }
        if fixation.arc_activation || fixation.arc_flow {
            for arc in &layer.arcs {
                let flow = reference_arc_flows
                    .get(&(arc.from, arc.to))
                    .copied()
                    .unwrap_or(0);
                if fixation.arc_activation {
                    fix_reference_value(solver, arc.selected, i32::from(flow > 0), tag);
                }
                if fixation.arc_flow {
                    fix_reference_value(solver, arc.flow, flow, tag);
                }
            }
        }
    }

    if fixation.topology_components {
        for branch in branches {
            let position = world_position(branch.cell, input.width);
            let selected = reference.logistics_components.iter().any(|component| {
                component.transport == branch.transport
                    && component.kind == branch.kind
                    && component.component == branch.component
                    && component.position == position
                    && component.rotation == branch.rotation
            });
            fix_reference_value(solver, branch.selected, i32::from(selected), tag);
        }
        for bridge in bridges {
            let position = world_position(bridge.cell, input.width);
            let prior = reference.logistics_components.iter().find(|component| {
                component.transport == bridge.transport
                    && component.kind == LogisticsComponentKind::Bridge
                    && component.component == bridge.component
                    && component.position == position
            });
            fix_reference_value(solver, bridge.selected, i32::from(prior.is_some()), tag);
            for (rotation, variable) in &bridge.rotations {
                fix_reference_value(
                    solver,
                    *variable,
                    i32::from(prior.is_some_and(|component| component.rotation == *rotation)),
                    tag,
                );
            }
        }
    }
    Ok(())
}

fn reference_terminal_arm_direction(
    input: &ModelInput,
    reference: &IntegratedLayoutReport,
    terminal: &TransportNetworkTerminal,
) -> Result<CardinalDirection, IntegratedLayoutDiagnostic> {
    match &terminal.endpoint {
        TransportNetworkEndpoint::External { side, .. } => Ok(edge_direction(*side)),
        TransportNetworkEndpoint::Facility { instance, port } => {
            let placement = reference
                .placements
                .iter()
                .find(|placement| placement.instance == *instance)
                .ok_or_else(|| reference_mismatch("terminal placement", instance))?;
            let instance_input = input
                .instances
                .iter()
                .find(|candidate| candidate.id == *instance)
                .ok_or_else(|| reference_mismatch("terminal facility", instance))?;
            let port = instance_input
                .definition
                .ports
                .iter()
                .find(|candidate| candidate.id == *port)
                .ok_or_else(|| reference_mismatch("terminal facility port", port))?;
            Ok(opposite_direction(edge_direction(
                port.edge.rotated_clockwise(placement.rotation),
            )))
        }
    }
}

fn reference_position_cell(
    input: &ModelInput,
    x: i64,
    y: i64,
    entity: &str,
) -> Result<usize, IntegratedLayoutDiagnostic> {
    let x = i32::try_from(x).map_err(|_| reference_mismatch("grid x", entity))?;
    let y = i32::try_from(y).map_err(|_| reference_mismatch("grid y", entity))?;
    if x < 0 || y < 0 || x >= input.width || y >= input.height {
        return Err(reference_mismatch("in-bounds grid position", entity));
    }
    y.checked_mul(input.width)
        .and_then(|value| value.checked_add(x))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| reference_mismatch("grid cell", entity))
}

fn orthogonally_adjacent(from: usize, to: usize, width: i32) -> bool {
    let width = usize::try_from(width).expect("validated grid width is positive");
    let (from_x, from_y) = (from % width, from / width);
    let (to_x, to_y) = (to % width, to / width);
    from_x.abs_diff(to_x) + from_y.abs_diff(to_y) == 1
}

fn set_reference_arm_item(
    arm_items: &mut [[i32; 4]],
    cell: usize,
    direction: usize,
    item_code: i32,
    entity: &str,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let previous = arm_items[cell][direction];
    if previous != 0 && previous != item_code {
        return Err(reference_mismatch("unique directional arm item", entity));
    }
    arm_items[cell][direction] = item_code;
    Ok(())
}

fn fix_reference_value(
    solver: &mut RecordedModel,
    variable: DomainId,
    value: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    solver.post_equals(
        ConstraintFamily::ResearchFixation,
        vec![variable.scaled(1)],
        value,
        value.unsigned_abs() as u64,
        tag,
    );
}

fn successful_ablation_reference(
    reference: Option<&IntegratedLayoutReport>,
) -> Result<&IntegratedLayoutReport, IntegratedLayoutDiagnostic> {
    reference.filter(|report| report.success).ok_or_else(|| {
        IntegratedLayoutDiagnostic::error(
            "missing-successful-shared-reference",
            "/reference",
            None,
            "shared-layer reference ablation requires a successful validated layout",
        )
    })
}

fn reference_mismatch(kind: &str, entity: &str) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "shared-reference-mismatch",
        "/reference",
        Some(entity.to_string()),
        format!("shared-layer reference has no matching {kind} for '{entity}'"),
    )
}

fn post_fixed_facility_coordinate(
    solver: &mut RecordedModel,
    instances: &[ModelInstance],
    fixed: &FixedFacilityCoordinate,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let instance = instances
        .iter()
        .find(|instance| instance.input.id == fixed.instance)
        .ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "unknown-coordinate-partition-facility",
                "/fixed_coordinate/instance",
                Some(fixed.instance.clone()),
                "the coordinate partition facility is not present in the cumulative exact model",
            )
        })?;
    let matching = instance
        .candidates
        .iter()
        .filter(|candidate| {
            candidate.x == fixed.x
                && candidate.y == fixed.y
                && fixed
                    .rotation
                    .is_none_or(|rotation| candidate.rotation == rotation)
        })
        .map(|candidate| candidate.selected.scaled(1))
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return Err(IntegratedLayoutDiagnostic::error(
            "invalid-coordinate-partition",
            "/fixed_coordinate",
            Some(format!(
                "{}@{},{}:{:?}",
                fixed.instance, fixed.x, fixed.y, fixed.rotation
            )),
            "the requested coordinate has no legal rotation within the hard layout bounds",
        ));
    }
    solver.post_equals(ConstraintFamily::ResearchFixation, matching, 1, 1, tag);
    Ok(())
}

fn validate_fixed_dimensions(
    report: &IntegratedLayoutReport,
    fixed_dimensions: Option<FixedUsedDimensions>,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let Some(fixed) = fixed_dimensions else {
        return Ok(());
    };
    let bounds = report.bounds.as_ref().ok_or_else(|| {
        IntegratedLayoutDiagnostic::error(
            "invalid-fixed-dimension-witness",
            "/bounds",
            None,
            "fixed-dimension research witness is missing exact used bounds",
        )
    })?;
    if bounds.width != i64::from(fixed.width) || bounds.height != i64::from(fixed.height) {
        return Err(IntegratedLayoutDiagnostic::error(
            "invalid-fixed-dimension-witness",
            "/bounds",
            Some(format!("{}x{}", bounds.width, bounds.height)),
            format!(
                "fixed-dimension research witness must use exactly {}x{} cells",
                fixed.width, fixed.height
            ),
        ));
    }
    Ok(())
}

fn validate_fixed_coordinate(
    report: &IntegratedLayoutReport,
    fixed_coordinate: Option<&FixedFacilityCoordinate>,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let Some(fixed) = fixed_coordinate else {
        return Ok(());
    };
    let placement = report
        .placements
        .iter()
        .find(|placement| placement.instance == fixed.instance)
        .ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "invalid-coordinate-partition-witness",
                "/placements",
                Some(fixed.instance.clone()),
                "coordinate-partition witness is missing the partitioned facility",
            )
        })?;
    if placement.x != i64::from(fixed.x) || placement.y != i64::from(fixed.y) {
        return Err(IntegratedLayoutDiagnostic::error(
            "invalid-coordinate-partition-witness",
            "/placements",
            Some(format!(
                "{}@{},{}",
                placement.instance, placement.x, placement.y
            )),
            format!(
                "coordinate-partition witness must place '{}' at {},{}",
                fixed.instance, fixed.x, fixed.y
            ),
        ));
    }
    if let Some(rotation) = fixed.rotation
        && placement.rotation != rotation
    {
        return Err(IntegratedLayoutDiagnostic::error(
            "invalid-coordinate-partition-witness",
            "/placements",
            Some(format!(
                "{}@{},{}:{}",
                placement.instance, placement.x, placement.y, placement.rotation
            )),
            format!(
                "rotation-partition witness must rotate '{}' to {} degrees",
                fixed.instance, rotation
            ),
        ));
    }
    Ok(())
}

fn validate_fixed_terminal_ports(
    report: &IntegratedLayoutReport,
    fixed_ports: Option<&[FixedTerminalPortChoice]>,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let Some(fixed_ports) = fixed_ports else {
        return Ok(());
    };
    for fixed in fixed_ports {
        let terminal = report
            .transport_networks
            .iter()
            .flat_map(|network| network.terminals.iter())
            .find(|terminal| terminal.id == fixed.terminal)
            .ok_or_else(|| {
                IntegratedLayoutDiagnostic::error(
                    "invalid-port-partition-witness",
                    "/transport_networks",
                    Some(fixed.terminal.clone()),
                    "port-partition witness is missing the fixed terminal",
                )
            })?;
        let TransportNetworkEndpoint::Facility { port, .. } = &terminal.endpoint else {
            return Err(IntegratedLayoutDiagnostic::error(
                "invalid-port-partition-witness",
                "/transport_networks",
                Some(fixed.terminal.clone()),
                "port-partition witness fixed a non-facility terminal",
            ));
        };
        if port != &fixed.port {
            return Err(IntegratedLayoutDiagnostic::error(
                "invalid-port-partition-witness",
                "/transport_networks",
                Some(format!("{}:{}", fixed.terminal, port)),
                format!(
                    "port-partition witness must select port '{}' for terminal '{}'",
                    fixed.port, fixed.terminal
                ),
            ));
        }
    }
    Ok(())
}

fn initial_metrics(input: &ModelInput) -> ExactModelMetrics {
    ExactModelMetrics {
        facility_count: input.instances.len(),
        route_requirement_count: input.edges.len(),
        commodity_network_count: input.networks.len(),
        commodity_item_count: input
            .networks
            .iter()
            .map(|network| network.item())
            .collect::<BTreeSet<_>>()
            .len(),
        belt_network_count: input
            .networks
            .iter()
            .filter(|network| network.transport() == TransportKind::Belt)
            .count(),
        pipe_network_count: input
            .networks
            .iter()
            .filter(|network| network.transport() == TransportKind::Pipe)
            .count(),
        network_requirement_reference_count: input
            .networks
            .iter()
            .map(|network| network.route_indices().len())
            .sum(),
        network_terminal_count: input
            .networks
            .iter()
            .map(|network| network.terminal_count())
            .sum(),
        external_terminal_count: input
            .networks
            .iter()
            .map(|network| network.external_terminal_count())
            .sum(),
        maximum_network_flow_scale: input
            .networks
            .iter()
            .map(|network| network.flow_scale())
            .max()
            .unwrap_or(0),
        maximum_line_capacity_units: input
            .networks
            .iter()
            .map(|network| network.line_capacity_units())
            .max()
            .unwrap_or(0),
        total_terminal_flow_units: input
            .networks
            .iter()
            .map(|network| network.total_terminal_flow_units())
            .sum(),
        grid_cell_count: input.cell_count as usize,
        ..ExactModelMetrics::default()
    }
}

fn build_placements(
    solver: &mut RecordedModel,
    input: &ModelInput,
    metrics: &mut ExactModelMetrics,
    cell_count: usize,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (
    Vec<ModelInstance>,
    BTreeMap<String, PlacementChoice>,
    Vec<DomainId>,
) {
    let mut placement_choices = BTreeMap::new();
    let mut instances = Vec::with_capacity(input.instances.len());
    for instance in &input.instances {
        let geometries = generate_candidate_geometries(instance, input.width, input.height);
        if geometries.is_empty() {
            return (Vec::new(), BTreeMap::new(), Vec::new());
        }
        let upper_bound =
            i32::try_from(geometries.len() - 1).expect("placement candidate count fits i32");
        let choice = solver.new_variable(
            VariableFamily::Placement,
            0,
            upper_bound,
            format!("placement-choice-{}", instance.id),
        );
        let candidates = geometries
            .into_iter()
            .enumerate()
            .map(|(index, geometry)| {
                let selected = solver.new_named_literal_for_predicate(
                    VariableFamily::Placement,
                    choice.equality_predicate(
                        i32::try_from(index).expect("placement candidate index fits i32"),
                    ),
                    tag,
                    format!(
                        "place-{}-{}-{}-{}",
                        instance.id, geometry.rotation, geometry.x, geometry.y
                    ),
                );
                super::Candidate {
                    rotation: geometry.rotation,
                    x: geometry.x,
                    y: geometry.y,
                    width: geometry.width,
                    height: geometry.height,
                    occupied_cells: geometry.occupied_cells,
                    port_connections: geometry.port_connections,
                    selected: *selected.get_integer_variable().inner(),
                }
            })
            .collect::<Vec<_>>();
        metrics.placement_variables += candidates.len() + 1;
        placement_choices.insert(instance.id.clone(), PlacementChoice { choice });
        instances.push(ModelInstance {
            input: instance.clone(),
            candidates,
        });
    }

    let facility_occupancy = (0..cell_count)
        .map(|cell| {
            let instance_occupancy = instances
                .iter()
                .map(|instance| {
                    let occupied = solver.new_variable(
                        VariableFamily::PhysicalOccupancy,
                        0,
                        1,
                        format!("facility-{}-occupies-{cell}", instance.input.id),
                    );
                    let values = instance
                        .candidates
                        .iter()
                        .map(|candidate| i32::from(candidate.occupied_cells.contains(&cell)))
                        .collect::<Vec<_>>();
                    solver.post_constant_element(
                        ConstraintFamily::OccupancyChannel,
                        placement_choices[&instance.input.id].choice,
                        values,
                        occupied,
                        tag,
                    );
                    occupied
                })
                .collect::<Vec<_>>();
            let occupied = solver.new_variable(
                VariableFamily::PhysicalOccupancy,
                0,
                1,
                format!("facility-occupancy-{cell}"),
            );
            let mut definition = vec![occupied.scaled(1)];
            definition.extend(
                instance_occupancy
                    .iter()
                    .map(|instance| instance.scaled(-1)),
            );
            solver.post_equals(ConstraintFamily::OccupancyChannel, definition, 0, 1, tag);
            metrics.placement_variables += instance_occupancy.len() + 1;
            occupied
        })
        .collect::<Vec<_>>();
    (instances, placement_choices, facility_occupancy)
}

fn build_endpoint_options(
    solver: &mut RecordedModel,
    input: &ModelInput,
    instances: &[ModelInstance],
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<EdgeEndpointOptions> {
    input
        .edges
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| {
            let (source, target) = match (&edge.source, &edge.target) {
                (
                    super::super::EndpointInput::Facility { .. },
                    super::super::EndpointInput::Facility { .. },
                ) => (
                    model_facility_endpoint_options(
                        solver,
                        edge_index,
                        "source",
                        &edge.source,
                        instances,
                        tag,
                    ),
                    model_facility_endpoint_options(
                        solver,
                        edge_index,
                        "target",
                        &edge.target,
                        instances,
                        tag,
                    ),
                ),
                (
                    super::super::EndpointInput::External { node },
                    super::super::EndpointInput::Facility { .. },
                ) => {
                    let target = model_facility_endpoint_options(
                        solver,
                        edge_index,
                        "target",
                        &edge.target,
                        instances,
                        tag,
                    );
                    (external_endpoint_options(node, &target), target)
                }
                (
                    super::super::EndpointInput::Facility { .. },
                    super::super::EndpointInput::External { node },
                ) => {
                    let source = model_facility_endpoint_options(
                        solver,
                        edge_index,
                        "source",
                        &edge.source,
                        instances,
                        tag,
                    );
                    let target = external_endpoint_options(node, &source);
                    (source, target)
                }
                (
                    super::super::EndpointInput::External { .. },
                    super::super::EndpointInput::External { .. },
                ) => unreachable!(),
            };
            metrics.endpoint_variables += match (&edge.source, &edge.target) {
                (
                    super::super::EndpointInput::Facility { .. },
                    super::super::EndpointInput::Facility { .. },
                ) => source.len() + target.len(),
                (
                    super::super::EndpointInput::External { .. },
                    super::super::EndpointInput::Facility { .. },
                ) => target.len(),
                (
                    super::super::EndpointInput::Facility { .. },
                    super::super::EndpointInput::External { .. },
                ) => source.len(),
                (
                    super::super::EndpointInput::External { .. },
                    super::super::EndpointInput::External { .. },
                ) => 0,
            };
            EdgeEndpointOptions { source, target }
        })
        .collect()
}

fn build_flattened_terminals(
    input: &ModelInput,
    edge_options: &[EdgeEndpointOptions],
) -> Vec<Vec<SharedTerminal>> {
    input
        .networks
        .iter()
        .map(|network| {
            network
                .terminals()
                .iter()
                .map(|terminal| {
                    let options = &edge_options[terminal.route_index()];
                    let selected_options = if terminal.direction() == FacilityPortDirection::Output
                    {
                        options.source.clone()
                    } else {
                        options.target.clone()
                    };
                    SharedTerminal {
                        id: terminal.id().to_string(),
                        direction: terminal.direction(),
                        rate: terminal.rate(),
                        flow_units: terminal.flow_units(),
                        routing_options: selected_options
                            .iter()
                            .map(|option| SharedRoutingOption {
                                cell: option.cell,
                                arm_direction: option.arm_direction,
                                selected: option.selected,
                            })
                            .collect(),
                        endpoint: SharedTerminalEndpoint::Flattened(selected_options),
                    }
                })
                .collect()
        })
        .collect()
}

fn build_factored_terminals(
    solver: &mut RecordedModel,
    input: &ModelInput,
    instances: &[ModelInstance],
    placement_choices: &BTreeMap<String, PlacementChoice>,
    used_bounds: UsedBoundsVariables,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<Vec<SharedTerminal>> {
    let edge_endpoints = input
        .edges
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| match (&edge.source, &edge.target) {
            (EndpointInput::Facility { .. }, EndpointInput::Facility { .. }) => {
                let source = build_factored_selector(
                    solver,
                    input,
                    instances,
                    placement_choices,
                    edge_index,
                    "source",
                    &edge.source,
                    metrics,
                    tag,
                );
                let target = build_factored_selector(
                    solver,
                    input,
                    instances,
                    placement_choices,
                    edge_index,
                    "target",
                    &edge.target,
                    metrics,
                    tag,
                );
                FactoredEdgeEndpoints {
                    source: factored_facility_view(&source),
                    target: factored_facility_view(&target),
                }
            }
            (EndpointInput::External { node }, EndpointInput::Facility { .. }) => {
                let source = boundary_terminals::build_selector(
                    solver,
                    input,
                    edge_index,
                    "source",
                    used_bounds,
                    metrics,
                    tag,
                );
                let target = build_factored_selector(
                    solver,
                    input,
                    instances,
                    placement_choices,
                    edge_index,
                    "target",
                    &edge.target,
                    metrics,
                    tag,
                );
                FactoredEdgeEndpoints {
                    source: factored_boundary_view(&source, node),
                    target: factored_facility_view(&target),
                }
            }
            (EndpointInput::Facility { .. }, EndpointInput::External { node }) => {
                let source = build_factored_selector(
                    solver,
                    input,
                    instances,
                    placement_choices,
                    edge_index,
                    "source",
                    &edge.source,
                    metrics,
                    tag,
                );
                let target = boundary_terminals::build_selector(
                    solver,
                    input,
                    edge_index,
                    "target",
                    used_bounds,
                    metrics,
                    tag,
                );
                FactoredEdgeEndpoints {
                    source: factored_facility_view(&source),
                    target: factored_boundary_view(&target, node),
                }
            }
            (EndpointInput::External { .. }, EndpointInput::External { .. }) => unreachable!(),
        })
        .collect::<Vec<_>>();

    let mut geometry_literals = BTreeMap::<(DomainId, i32), DomainId>::new();
    input
        .networks
        .iter()
        .map(|network| {
            network
                .terminals()
                .iter()
                .map(|terminal| {
                    let endpoints = &edge_endpoints[terminal.route_index()];
                    let view = if terminal.direction() == FacilityPortDirection::Output {
                        &endpoints.source
                    } else {
                        &endpoints.target
                    };
                    let routing_options = view
                        .reachable_keys
                        .iter()
                        .map(|key| {
                            let variable_family =
                                if matches!(&view.kind, FactoredEndpointKind::External { .. }) {
                                    VariableFamily::BoundaryTerminal
                                } else {
                                    VariableFamily::EndpointGeometry
                                };
                            let selected = *geometry_literals
                                .entry((view.key, *key))
                                .or_insert_with(|| {
                                    let literal = solver.new_named_literal_for_predicate(
                                        variable_family,
                                        view.key.equality_predicate(*key),
                                        tag,
                                        format!(
                                            "endpoint-{}-geometry-{key}",
                                            terminal.route_index()
                                        ),
                                    );
                                    if variable_family == VariableFamily::BoundaryTerminal {
                                        metrics.boundary_terminal_variables += 1;
                                    } else {
                                        metrics.endpoint_variables += 1;
                                    }
                                    *literal.get_integer_variable().inner()
                                });
                            let key = usize::try_from(*key)
                                .expect("terminal geometry key is non-negative");
                            let direction = DIRECTIONS[key % 4];
                            SharedRoutingOption {
                                cell: key / 4,
                                arm_direction: direction,
                                selected,
                            }
                        })
                        .collect();
                    SharedTerminal {
                        id: terminal.id().to_string(),
                        direction: terminal.direction(),
                        rate: terminal.rate(),
                        flow_units: terminal.flow_units(),
                        routing_options,
                        endpoint: SharedTerminalEndpoint::Factored {
                            key: view.key,
                            kind: view.kind.clone(),
                        },
                    }
                })
                .collect()
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn build_factored_selector(
    solver: &mut RecordedModel,
    input: &ModelInput,
    instances: &[ModelInstance],
    placement_choices: &BTreeMap<String, PlacementChoice>,
    edge_index: usize,
    endpoint_kind: &str,
    endpoint: &EndpointInput,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> FactoredEndpointSelector {
    let EndpointInput::Facility {
        instance: instance_id,
        ports,
    } = endpoint
    else {
        unreachable!("factored selector requires a facility endpoint")
    };
    let instance = instances
        .iter()
        .find(|candidate| candidate.input.id == *instance_id)
        .expect("prepared endpoint instance exists");
    let placement = placement_choices
        .get(instance_id)
        .expect("every modeled instance has a factored placement choice");
    let port_ids = ports.iter().map(|port| port.id.clone()).collect::<Vec<_>>();
    let port_upper = i32::try_from(port_ids.len() - 1).expect("port count fits i32");
    let key_upper = input
        .cell_count
        .checked_mul(4)
        .and_then(|value| value.checked_sub(1))
        .expect("validated grid key domain fits i32");
    let port_choice = solver.new_variable(
        VariableFamily::Endpoint,
        0,
        port_upper,
        format!("edge-{edge_index}-{endpoint_kind}-port"),
    );
    let facility_key = solver.new_variable(
        VariableFamily::EndpointGeometry,
        0,
        key_upper,
        format!("edge-{edge_index}-{endpoint_kind}-facility-geometry"),
    );
    metrics.endpoint_variables += 2;

    let mut facility_values_by_port = ports
        .iter()
        .map(|_| Vec::with_capacity(instance.candidates.len()))
        .collect::<Vec<_>>();
    let mut facility_keys = BTreeSet::new();
    for candidate in &instance.candidates {
        for (port_index, port) in ports.iter().enumerate() {
            if let Some(cell) = candidate.port_connections.get(&port.id).copied() {
                let outward = edge_direction(port.edge.rotated_clockwise(candidate.rotation));
                let facility_direction = opposite_direction(outward);
                let facility_value = geometry_key(cell, facility_direction);
                facility_keys.insert(facility_value);
                facility_values_by_port[port_index].push(facility_value);
            } else {
                facility_values_by_port[port_index].push(-1);
            }
        }
    }
    let port_geometry = facility_values_by_port
        .into_iter()
        .enumerate()
        .map(|(port_index, facility_values)| {
            let geometry = solver.new_variable(
                VariableFamily::EndpointGeometry,
                -1,
                key_upper,
                format!("edge-{edge_index}-{endpoint_kind}-port-{port_index}-geometry"),
            );
            metrics.endpoint_variables += 1;
            solver.post_constant_element(
                ConstraintFamily::EndpointLink,
                placement.choice,
                facility_values,
                geometry,
                tag,
            );
            geometry
        })
        .collect::<Vec<_>>();
    solver.post_variable_element(
        ConstraintFamily::EndpointLink,
        port_choice,
        port_geometry,
        facility_key,
        tag,
    );
    FactoredEndpointSelector {
        facility_key,
        port_choice,
        port_ids,
        instance: instance_id.clone(),
        facility_keys: facility_keys.into_iter().collect(),
    }
}

fn factored_facility_view(selector: &FactoredEndpointSelector) -> FactoredTerminalView {
    FactoredTerminalView {
        key: selector.facility_key,
        kind: FactoredEndpointKind::Facility {
            instance: selector.instance.clone(),
            port_choice: selector.port_choice,
            port_ids: selector.port_ids.clone(),
        },
        reachable_keys: selector.facility_keys.clone(),
    }
}

fn factored_boundary_view(
    selector: &boundary_terminals::BoundaryTerminalSelector,
    node: &str,
) -> FactoredTerminalView {
    FactoredTerminalView {
        key: selector.key,
        kind: FactoredEndpointKind::External {
            node: node.to_string(),
        },
        reachable_keys: selector.reachable_keys.clone(),
    }
}

fn geometry_key(cell: usize, direction: CardinalDirection) -> i32 {
    i32::try_from(cell * 4 + direction_index(direction)).expect("terminal geometry key fits i32")
}

fn opposite_direction(direction: CardinalDirection) -> CardinalDirection {
    match direction {
        CardinalDirection::North => CardinalDirection::South,
        CardinalDirection::East => CardinalDirection::West,
        CardinalDirection::South => CardinalDirection::North,
        CardinalDirection::West => CardinalDirection::East,
    }
}

fn edge_direction(edge: FacilityPortEdge) -> CardinalDirection {
    match edge {
        FacilityPortEdge::North => CardinalDirection::North,
        FacilityPortEdge::East => CardinalDirection::East,
        FacilityPortEdge::South => CardinalDirection::South,
        FacilityPortEdge::West => CardinalDirection::West,
    }
}

type TerminalContribution = (DomainId, i32, usize);

#[allow(clippy::too_many_arguments)]
fn build_layer(
    solver: &mut RecordedModel,
    input: &ModelInput,
    terminals: &[Vec<SharedTerminal>],
    transport: TransportKind,
    network_indices: Vec<usize>,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    metrics: &mut ExactModelMetrics,
    all_branch_components: &mut Vec<SharedBranchComponent>,
    all_bridges: &mut Vec<ModelBridge>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> SharedLayer {
    let cell_count = input.cell_count as usize;
    let transport_name = format!("{transport:?}").to_lowercase();
    let item_codes = network_indices
        .iter()
        .enumerate()
        .map(|(local_index, network_index)| (*network_index, local_index as i32 + 1))
        .collect::<BTreeMap<_, _>>();
    let item_count = i32::try_from(network_indices.len()).expect("network count fits i32");
    let maximum_capacity = network_indices
        .iter()
        .map(|network_index| input.networks[*network_index].line_capacity_units())
        .max()
        .expect("a shared layer has at least one network");
    let layer_index = usize::from(transport == TransportKind::Pipe);
    let (arcs, incoming, outgoing) = grid_arcs(
        solver,
        layer_index,
        input.width,
        input.height,
        maximum_capacity,
        tag,
    );
    metrics.route_arc_variables += arcs.len();
    metrics.network_flow_variables += arcs.len();

    let mut supply_by_cell: Vec<[Vec<TerminalContribution>; 4]> = (0..cell_count)
        .map(|_| std::array::from_fn(|_| Vec::new()))
        .collect();
    let mut demand_by_cell: Vec<[Vec<TerminalContribution>; 4]> = (0..cell_count)
        .map(|_| std::array::from_fn(|_| Vec::new()))
        .collect();
    for network_index in &network_indices {
        for terminal in &terminals[*network_index] {
            let destination = if terminal.direction == FacilityPortDirection::Output {
                &mut supply_by_cell
            } else {
                &mut demand_by_cell
            };
            for option in &terminal.routing_options {
                destination[option.cell][direction_index(option.arm_direction)].push((
                    option.selected,
                    terminal.flow_units,
                    *network_index,
                ));
            }
        }
    }

    let mut incoming_arms_by_cell = Vec::with_capacity(cell_count);
    let mut outgoing_arms_by_cell = Vec::with_capacity(cell_count);
    let mut arm_items = Vec::with_capacity(cell_count);
    let mut route_cells = Vec::with_capacity(cell_count);
    let mut incoming_flow_by_cell = Vec::with_capacity(cell_count);
    let mut outgoing_flow_by_cell = Vec::with_capacity(cell_count);
    let item_rows = std::iter::once(vec![0, 0, 0])
        .chain((1..=item_count).flat_map(|item| [vec![1, 0, item], vec![0, 1, item]]))
        .collect::<Vec<_>>();

    for cell in 0..cell_count {
        let incoming_flow: [Vec<(DomainId, i32)>; 4] = std::array::from_fn(|direction| {
            let mut terms = supply_by_cell[cell][direction]
                .iter()
                .map(|(variable, units, _)| (*variable, *units))
                .collect::<Vec<_>>();
            terms.extend(
                incoming[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.from, input.width)) == direction
                    })
                    .map(|arc| (arc.flow, 1)),
            );
            terms
        });
        let outgoing_flow: [Vec<(DomainId, i32)>; 4] = std::array::from_fn(|direction| {
            let mut terms = demand_by_cell[cell][direction]
                .iter()
                .map(|(variable, units, _)| (*variable, *units))
                .collect::<Vec<_>>();
            terms.extend(
                outgoing[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.to, input.width)) == direction
                    })
                    .map(|arc| (arc.flow, 1)),
            );
            terms
        });

        let incoming_arms: [DomainId; 4] = std::array::from_fn(|direction| {
            let terminal_presence = post_presence(
                solver,
                VariableFamily::TerminalPresence,
                ConstraintFamily::TerminalPresence,
                format!(
                    "{transport_name}-cell-{cell}-{}-supply",
                    direction_name(direction)
                ),
                unique_variables(
                    supply_by_cell[cell][direction]
                        .iter()
                        .map(|(variable, _, _)| *variable),
                ),
                tag,
            );
            let grid_arcs = incoming[cell]
                .iter()
                .filter(|arc| {
                    direction_index(direction_between(cell, arc.from, input.width)) == direction
                })
                .map(|arc| arc.selected)
                .collect::<Vec<_>>();
            post_arm(
                solver,
                format!(
                    "{transport_name}-cell-{cell}-{}-incoming",
                    direction_name(direction)
                ),
                terminal_presence,
                &grid_arcs,
                tag,
            )
        });
        let outgoing_arms: [DomainId; 4] = std::array::from_fn(|direction| {
            let terminal_presence = post_presence(
                solver,
                VariableFamily::TerminalPresence,
                ConstraintFamily::TerminalPresence,
                format!(
                    "{transport_name}-cell-{cell}-{}-demand",
                    direction_name(direction)
                ),
                unique_variables(
                    demand_by_cell[cell][direction]
                        .iter()
                        .map(|(variable, _, _)| *variable),
                ),
                tag,
            );
            let grid_arcs = outgoing[cell]
                .iter()
                .filter(|arc| {
                    direction_index(direction_between(cell, arc.to, input.width)) == direction
                })
                .map(|arc| arc.selected)
                .collect::<Vec<_>>();
            post_arm(
                solver,
                format!(
                    "{transport_name}-cell-{cell}-{}-outgoing",
                    direction_name(direction)
                ),
                terminal_presence,
                &grid_arcs,
                tag,
            )
        });
        let cell_arm_items: [DomainId; 4] = std::array::from_fn(|direction| {
            let item = solver.new_variable(
                VariableFamily::ArmItem,
                0,
                item_count,
                format!(
                    "{transport_name}-cell-{cell}-{}-item",
                    direction_name(direction)
                ),
            );
            solver.post_table(
                ConstraintFamily::ItemAssignment,
                vec![incoming_arms[direction], outgoing_arms[direction], item],
                item_rows.clone(),
                tag,
            );
            for (selected, _, network_index) in supply_by_cell[cell][direction]
                .iter()
                .chain(&demand_by_cell[cell][direction])
            {
                post_selected_item(
                    solver,
                    *selected,
                    item,
                    item_codes[network_index],
                    item_count,
                    tag,
                );
            }
            for network_index in &network_indices {
                let condition = solver.solver_mut().new_named_literal_for_predicate(
                    item.equality_predicate(item_codes[network_index]),
                    tag,
                    format!(
                        "{transport_name}-cell-{cell}-{}-is-item-{}",
                        direction_name(direction),
                        item_codes[network_index]
                    ),
                );
                let incoming_capacity = incoming[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.from, input.width)) == direction
                    })
                    .map(|arc| (arc.flow, 1))
                    .chain(
                        supply_by_cell[cell][direction]
                            .iter()
                            .filter(|(_, _, candidate)| candidate == network_index)
                            .map(|(variable, units, _)| (*variable, *units)),
                    )
                    .collect::<Vec<_>>();
                let outgoing_capacity = outgoing[cell]
                    .iter()
                    .filter(|arc| {
                        direction_index(direction_between(cell, arc.to, input.width)) == direction
                    })
                    .map(|arc| (arc.flow, 1))
                    .chain(
                        demand_by_cell[cell][direction]
                            .iter()
                            .filter(|(_, _, candidate)| candidate == network_index)
                            .map(|(variable, units, _)| (*variable, *units)),
                    )
                    .collect::<Vec<_>>();
                for flow in [&incoming_capacity, &outgoing_capacity] {
                    if flow.is_empty() {
                        continue;
                    }
                    solver.post_implied_less_than_or_equals(
                        ConstraintFamily::LineCapacity,
                        flow.iter()
                            .map(|(variable, coefficient)| variable.scaled(*coefficient))
                            .collect(),
                        input.networks[*network_index].line_capacity_units(),
                        maximum_capacity.unsigned_abs() as u64,
                        condition,
                        item,
                        tag,
                    );
                }
            }
            item
        });

        for direction in 0..4 {
            solver.post_less_than_or_equals(
                ConstraintFamily::OpposingArms,
                vec![
                    incoming_arms[direction].scaled(1),
                    outgoing_arms[direction].scaled(1),
                ],
                1,
                1,
                tag,
            );
        }
        let mut conservation = outgoing_flow
            .iter()
            .flatten()
            .map(|(variable, coefficient)| variable.scaled(*coefficient))
            .collect::<Vec<_>>();
        conservation.extend(
            incoming_flow
                .iter()
                .flatten()
                .map(|(variable, coefficient)| variable.scaled(-*coefficient)),
        );
        solver.post_equals(
            ConstraintFamily::FlowConservation,
            conservation,
            0,
            maximum_capacity.unsigned_abs() as u64,
            tag,
        );
        let route_cell = post_presence(
            solver,
            VariableFamily::RouteCell,
            ConstraintFamily::RouteCellActivation,
            format!("{transport_name}-cell-{cell}-occupied"),
            incoming_arms
                .iter()
                .copied()
                .chain(outgoing_arms.iter().copied()),
            tag,
        );
        route_cells.push(route_cell);
        incoming_arms_by_cell.push(incoming_arms);
        outgoing_arms_by_cell.push(outgoing_arms);
        arm_items.push(cell_arm_items);
        incoming_flow_by_cell.push(incoming_flow);
        outgoing_flow_by_cell.push(outgoing_flow);
    }
    metrics.route_cell_variables += route_cells.len();

    for arc in &arcs {
        let from_direction = direction_index(direction_between(arc.from, arc.to, input.width));
        let to_direction = direction_index(direction_between(arc.to, arc.from, input.width));
        let condition = solver.solver_mut().new_named_literal_for_predicate(
            arc.selected.equality_predicate(1),
            tag,
            format!("{transport_name}-arc-{}-{}-selected", arc.from, arc.to),
        );
        solver.post_implied_binary_equals(
            ConstraintFamily::ItemAssignment,
            arm_items[arc.from][from_direction],
            arm_items[arc.to][to_direction],
            condition,
            arc.selected,
            tag,
        );
    }

    for cell in 0..cell_count {
        let (branches, bridge) = post_cell_topology(
            solver,
            input,
            transport,
            &network_indices,
            &item_codes,
            cell,
            &incoming[cell],
            &outgoing[cell],
            &incoming_arms_by_cell[cell],
            &outgoing_arms_by_cell[cell],
            &arm_items[cell],
            &incoming_flow_by_cell[cell],
            &outgoing_flow_by_cell[cell],
            route_cells[cell],
            &supply_by_cell[cell],
            &demand_by_cell[cell],
            logistics_components,
            metrics,
            tag,
        );
        all_branch_components.extend(branches);
        all_bridges.push(bridge);
    }

    SharedLayer {
        transport,
        network_indices,
        arcs,
        route_cells,
        arm_items,
    }
}

fn unique_variables(variables: impl Iterator<Item = DomainId>) -> impl Iterator<Item = DomainId> {
    variables.collect::<BTreeSet<_>>().into_iter()
}

fn direction_name(direction: usize) -> &'static str {
    match direction {
        0 => "north",
        1 => "east",
        2 => "south",
        3 => "west",
        _ => unreachable!(),
    }
}

fn post_selected_item(
    solver: &mut RecordedModel,
    selected: DomainId,
    item: DomainId,
    item_code: i32,
    _maximum_item_code: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let condition = solver
        .solver_mut()
        .new_literal_for_predicate(selected.equality_predicate(1), tag);
    solver.post_implied_equals(
        ConstraintFamily::ItemAssignment,
        vec![item.scaled(1)],
        item_code,
        1,
        condition,
        selected,
        tag,
    );
}

#[allow(clippy::too_many_arguments)]
fn post_cell_topology(
    solver: &mut RecordedModel,
    input: &ModelInput,
    transport: TransportKind,
    network_indices: &[usize],
    item_codes: &BTreeMap<usize, i32>,
    cell: usize,
    incoming_arcs: &[Arc],
    outgoing_arcs: &[Arc],
    incoming_arms: &[DomainId; 4],
    outgoing_arms: &[DomainId; 4],
    arm_items: &[DomainId; 4],
    _incoming_flow: &[Vec<(DomainId, i32)>; 4],
    _outgoing_flow: &[Vec<(DomainId, i32)>; 4],
    route_cell: DomainId,
    supply: &[Vec<TerminalContribution>; 4],
    demand: &[Vec<TerminalContribution>; 4],
    components: &ValidatedLogisticsComponentCatalog,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (Vec<SharedBranchComponent>, ModelBridge) {
    let transport_name = format!("{transport:?}").to_lowercase();
    let item_count = i32::try_from(network_indices.len()).expect("network count fits i32");
    let maximum_capacity = network_indices
        .iter()
        .map(|network_index| input.networks[*network_index].line_capacity_units())
        .max()
        .expect("a shared layer has at least one network");
    let maximum_total_flow = maximum_capacity
        .checked_mul(4)
        .expect("validated flow bounds fit shared topology constraints");

    let mut branches = Vec::new();
    for kind in [
        LogisticsComponentKind::Splitter,
        LogisticsComponentKind::Converger,
    ] {
        let definition = components
            .component_by_kind(transport, kind)
            .expect("validated catalog contains branch capabilities");
        for rotation in &definition.allowed_rotations {
            let selected = solver.new_variable(
                VariableFamily::BranchComponent,
                0,
                1,
                format!(
                    "{transport_name}-cell-{cell}-{:?}-rotation-{rotation}",
                    kind
                )
                .to_lowercase(),
            );
            let allowed_inputs = definition
                .input_directions
                .iter()
                .map(|direction| rotate_direction(*direction, *rotation))
                .collect::<Vec<_>>();
            let allowed_outputs = definition
                .output_directions
                .iter()
                .map(|direction| rotate_direction(*direction, *rotation))
                .collect::<Vec<_>>();
            for (index, direction) in DIRECTIONS.iter().enumerate() {
                if !allowed_inputs.contains(direction) {
                    solver.post_less_than_or_equals(
                        ConstraintFamily::BranchTopology,
                        vec![incoming_arms[index].scaled(1), selected.scaled(1)],
                        1,
                        1,
                        tag,
                    );
                }
                if !allowed_outputs.contains(direction) {
                    solver.post_less_than_or_equals(
                        ConstraintFamily::BranchTopology,
                        vec![outgoing_arms[index].scaled(1), selected.scaled(1)],
                        1,
                        1,
                        tag,
                    );
                }
            }
            branches.push(SharedBranchComponent {
                transport,
                cell,
                component: definition.id.clone(),
                kind,
                rotation: *rotation,
                selected,
            });
        }
    }
    metrics.branch_component_variables += branches.len();

    let bridge_definition = components
        .component_by_kind(transport, LogisticsComponentKind::Bridge)
        .expect("validated catalog contains bridge capabilities");
    let bridge_selected = solver.new_variable(
        VariableFamily::Bridge,
        0,
        1,
        format!("{transport_name}-bridge-{cell}"),
    );
    let bridge_rotations = bridge_definition
        .allowed_rotations
        .iter()
        .map(|rotation| {
            (
                *rotation,
                solver.new_variable(
                    VariableFamily::BridgeRotation,
                    0,
                    1,
                    format!("{transport_name}-bridge-{cell}-rotation-{rotation}"),
                ),
            )
        })
        .collect::<Vec<_>>();
    let mut rotation_definition = bridge_rotations
        .iter()
        .map(|(_, variable)| variable.scaled(1))
        .collect::<Vec<_>>();
    rotation_definition.push(bridge_selected.scaled(-1));
    solver.post_equals(
        ConstraintFamily::BridgeRotation,
        rotation_definition,
        0,
        1,
        tag,
    );
    metrics.bridge_variables += 1;
    metrics.bridge_rotation_variables += bridge_rotations.len();

    post_at_most_one(
        solver,
        ConstraintFamily::BranchTopology,
        branches
            .iter()
            .map(|component| component.selected)
            .chain(std::iter::once(bridge_selected)),
        tag,
    );

    let cell_item = solver.new_variable(
        VariableFamily::ArmItem,
        0,
        item_count,
        format!("{transport_name}-cell-{cell}-non-bridge-item"),
    );
    let cell_item_rows = std::iter::once(vec![0, 0, 0])
        .chain((1..=item_count).map(|item| vec![1, 0, item]))
        .chain(std::iter::once(vec![1, 1, 0]))
        .collect::<Vec<_>>();
    solver.post_table(
        ConstraintFamily::ItemAssignment,
        vec![route_cell, bridge_selected, cell_item],
        cell_item_rows,
        tag,
    );
    for direction in 0..4 {
        let arm_presence_terms = [incoming_arms[direction], outgoing_arms[direction]];
        let mut upper = vec![
            arm_items[direction].scaled(1),
            cell_item.scaled(-1),
            bridge_selected.scaled(-item_count),
        ];
        upper.extend(
            arm_presence_terms
                .iter()
                .map(|presence| presence.scaled(item_count)),
        );
        solver.post_less_than_or_equals(
            ConstraintFamily::ItemAssignment,
            upper,
            item_count,
            item_count.unsigned_abs() as u64,
            tag,
        );
        let mut lower = vec![
            cell_item.scaled(1),
            arm_items[direction].scaled(-1),
            bridge_selected.scaled(-item_count),
        ];
        lower.extend(
            arm_presence_terms
                .iter()
                .map(|presence| presence.scaled(item_count)),
        );
        solver.post_less_than_or_equals(
            ConstraintFamily::ItemAssignment,
            lower,
            item_count,
            item_count.unsigned_abs() as u64,
            tag,
        );
    }

    let splitters = branches
        .iter()
        .filter(|component| component.kind == LogisticsComponentKind::Splitter)
        .map(|component| component.selected)
        .collect::<Vec<_>>();
    let convergers = branches
        .iter()
        .filter(|component| component.kind == LogisticsComponentKind::Converger)
        .map(|component| component.selected)
        .collect::<Vec<_>>();
    let incoming_count = incoming_arms
        .iter()
        .map(|arm| arm.scaled(1))
        .collect::<Vec<_>>();
    let outgoing_count = outgoing_arms
        .iter()
        .map(|arm| arm.scaled(1))
        .collect::<Vec<_>>();
    let mut incoming_maximum = incoming_count.clone();
    incoming_maximum.extend(convergers.iter().map(|selected| selected.scaled(-2)));
    incoming_maximum.push(bridge_selected.scaled(-1));
    solver.post_less_than_or_equals(
        ConstraintFamily::BranchTopology,
        incoming_maximum,
        1,
        2,
        tag,
    );
    let mut outgoing_maximum = outgoing_count.clone();
    outgoing_maximum.extend(splitters.iter().map(|selected| selected.scaled(-2)));
    outgoing_maximum.push(bridge_selected.scaled(-1));
    solver.post_less_than_or_equals(
        ConstraintFamily::BranchTopology,
        outgoing_maximum,
        1,
        2,
        tag,
    );
    let mut splitter_minimum = outgoing_count.clone();
    splitter_minimum.extend(splitters.iter().map(|selected| selected.scaled(-2)));
    solver.post_greater_than_or_equals(
        ConstraintFamily::BranchTopology,
        splitter_minimum,
        0,
        2,
        tag,
    );
    let mut splitter_input = incoming_count.clone();
    splitter_input.extend(splitters.iter().map(|selected| selected.scaled(-1)));
    solver.post_greater_than_or_equals(ConstraintFamily::BranchTopology, splitter_input, 0, 1, tag);
    let mut converger_minimum = incoming_count;
    converger_minimum.extend(convergers.iter().map(|selected| selected.scaled(-2)));
    solver.post_greater_than_or_equals(
        ConstraintFamily::BranchTopology,
        converger_minimum,
        0,
        2,
        tag,
    );
    let mut converger_output = outgoing_count;
    converger_output.extend(convergers.iter().map(|selected| selected.scaled(-1)));
    solver.post_greater_than_or_equals(
        ConstraintFamily::BranchTopology,
        converger_output,
        0,
        1,
        tag,
    );

    for branch in &branches {
        for network_index in network_indices {
            let condition = solver.solver_mut().new_named_literal_for_predicate(
                cell_item.equality_predicate(item_codes[network_index]),
                tag,
                format!(
                    "{transport_name}-cell-{cell}-branch-item-{}",
                    item_codes[network_index]
                ),
            );
            let capacity = input.networks[*network_index].component_capacity_units(branch.kind);
            let mut terms = incoming_arcs
                .iter()
                .map(|arc| arc.flow.scaled(1))
                .chain(
                    supply
                        .iter()
                        .flatten()
                        .filter(|(_, _, candidate)| candidate == network_index)
                        .map(|(variable, coefficient, _)| variable.scaled(*coefficient)),
                )
                .collect::<Vec<_>>();
            terms.push(branch.selected.scaled(maximum_total_flow));
            solver.post_implied_less_than_or_equals(
                ConstraintFamily::BranchTopology,
                terms,
                capacity + maximum_total_flow,
                maximum_total_flow.unsigned_abs() as u64,
                condition,
                cell_item,
                tag,
            );
        }
    }

    let horizontal_incoming = incoming_arcs
        .iter()
        .filter(|arc| same_axis(cell, arc.from, input.width, true))
        .collect::<Vec<_>>();
    let horizontal_outgoing = outgoing_arcs
        .iter()
        .filter(|arc| same_axis(cell, arc.to, input.width, true))
        .collect::<Vec<_>>();
    let vertical_incoming = incoming_arcs
        .iter()
        .filter(|arc| same_axis(cell, arc.from, input.width, false))
        .collect::<Vec<_>>();
    let vertical_outgoing = outgoing_arcs
        .iter()
        .filter(|arc| same_axis(cell, arc.to, input.width, false))
        .collect::<Vec<_>>();
    let bridge_condition = solver.solver_mut().new_named_literal_for_predicate(
        bridge_selected.equality_predicate(1),
        tag,
        format!("{transport_name}-bridge-{cell}-selected-condition"),
    );
    let bridge_possible = [
        &horizontal_incoming,
        &horizontal_outgoing,
        &vertical_incoming,
        &vertical_outgoing,
    ]
    .iter()
    .all(|group| !group.is_empty());
    if !bridge_possible {
        solver.post_equals(
            ConstraintFamily::BridgeCrossing,
            vec![bridge_selected.scaled(1)],
            0,
            1,
            tag,
        );
    }
    for group in [
        &horizontal_incoming,
        &horizontal_outgoing,
        &vertical_incoming,
        &vertical_outgoing,
    ] {
        if group.is_empty() {
            continue;
        }
        solver.post_implied_equals(
            ConstraintFamily::BridgeCrossing,
            group.iter().map(|arc| arc.selected.scaled(1)).collect(),
            1,
            1,
            bridge_condition,
            bridge_selected,
            tag,
        );
    }
    for (incoming_axis, outgoing_axis) in [
        (&horizontal_incoming, &horizontal_outgoing),
        (&vertical_incoming, &vertical_outgoing),
    ] {
        let balance = incoming_axis
            .iter()
            .map(|arc| arc.flow.scaled(1))
            .chain(outgoing_axis.iter().map(|arc| arc.flow.scaled(-1)))
            .collect::<Vec<_>>();
        if balance.is_empty() {
            continue;
        }
        solver.post_implied_equals(
            ConstraintFamily::BridgeCrossing,
            balance,
            0,
            maximum_capacity.unsigned_abs() as u64,
            bridge_condition,
            bridge_selected,
            tag,
        );
    }
    solver.post_implied_binary_equals(
        ConstraintFamily::BridgeCrossing,
        arm_items[direction_index(CardinalDirection::West)],
        arm_items[direction_index(CardinalDirection::East)],
        bridge_condition,
        bridge_selected,
        tag,
    );
    solver.post_implied_binary_equals(
        ConstraintFamily::BridgeCrossing,
        arm_items[direction_index(CardinalDirection::North)],
        arm_items[direction_index(CardinalDirection::South)],
        bridge_condition,
        bridge_selected,
        tag,
    );
    for selected in unique_variables(
        supply
            .iter()
            .chain(demand)
            .flatten()
            .map(|(selected, _, _)| *selected),
    ) {
        solver.post_less_than_or_equals(
            ConstraintFamily::BridgeCrossing,
            vec![bridge_selected.scaled(1), selected.scaled(1)],
            1,
            1,
            tag,
        );
    }
    for (axis_item, incoming_axis) in [
        (
            arm_items[direction_index(CardinalDirection::West)],
            &horizontal_incoming,
        ),
        (
            arm_items[direction_index(CardinalDirection::North)],
            &vertical_incoming,
        ),
    ] {
        for network_index in network_indices {
            let condition = solver.solver_mut().new_named_literal_for_predicate(
                axis_item.equality_predicate(item_codes[network_index]),
                tag,
                format!(
                    "{transport_name}-bridge-{cell}-axis-item-{}",
                    item_codes[network_index]
                ),
            );
            let mut terms = incoming_axis
                .iter()
                .map(|arc| arc.flow.scaled(1))
                .collect::<Vec<_>>();
            terms.push(bridge_selected.scaled(maximum_total_flow));
            solver.post_implied_less_than_or_equals(
                ConstraintFamily::BridgeCrossing,
                terms,
                input.networks[*network_index]
                    .component_capacity_units(LogisticsComponentKind::Bridge)
                    + maximum_total_flow,
                maximum_total_flow.unsigned_abs() as u64,
                condition,
                axis_item,
                tag,
            );
        }
    }
    metrics.crossing_constraints += 13 + supply.iter().chain(demand).flatten().count();

    (
        branches,
        ModelBridge {
            transport,
            cell,
            component: bridge_definition.id.clone(),
            selected: bridge_selected,
            rotations: bridge_rotations,
        },
    )
}

fn same_axis(cell: usize, neighbor: usize, width: i32, horizontal: bool) -> bool {
    let width = usize::try_from(width).expect("validated width is positive");
    (cell / width == neighbor / width) == horizontal
}

fn build_transport_occupancy(
    solver: &mut RecordedModel,
    input: &ModelInput,
    facility_occupancy: &[DomainId],
    internal_cells: impl Fn(TransportKind, usize) -> Option<DomainId>,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> BTreeMap<TransportKind, Vec<DomainId>> {
    let mut layers = BTreeMap::new();
    for transport in [TransportKind::Belt, TransportKind::Pipe] {
        let mut cells = Vec::with_capacity(input.cell_count as usize);
        for cell in 0..input.cell_count as usize {
            let occupied = solver.new_variable(
                VariableFamily::TransportOccupancy,
                0,
                1,
                format!(
                    "{}-occupancy-{cell}",
                    format!("{transport:?}").to_lowercase()
                ),
            );
            let mut definition = vec![occupied.scaled(1)];
            if let Some(route_cell) = internal_cells(transport, cell) {
                definition.push(route_cell.scaled(-1));
            }
            solver.post_equals(ConstraintFamily::OccupancyChannel, definition, 0, 1, tag);
            solver.post_less_than_or_equals(
                ConstraintFamily::TransportCollision,
                vec![facility_occupancy[cell].scaled(1), occupied.scaled(1)],
                1,
                1,
                tag,
            );
            cells.push(occupied);
        }
        metrics.route_cell_variables += cells.len();
        layers.insert(transport, cells);
    }
    layers
}

#[allow(clippy::too_many_arguments)]
fn build_objectives(
    solver: &mut RecordedModel,
    input: &ModelInput,
    facility_occupancy: &[DomainId],
    transport_occupancy: &BTreeMap<TransportKind, Vec<DomainId>>,
    layers: &[SharedLayer],
    branches: &[SharedBranchComponent],
    bridges: &[ModelBridge],
    used_bounds: UsedBoundsVariables,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<ExactObjectives, IntegratedLayoutDiagnostic> {
    let cell_count = input.cell_count as usize;
    let used_cells = (0..cell_count)
        .map(|cell| {
            post_presence(
                solver,
                VariableFamily::Objective,
                ConstraintFamily::UsedGeometry,
                format!("used-geometry-cell-{cell}"),
                std::iter::once(facility_occupancy[cell]).chain(
                    [TransportKind::Belt, TransportKind::Pipe]
                        .into_iter()
                        .map(|transport| transport_occupancy[&transport][cell]),
                ),
                tag,
            )
        })
        .collect::<Vec<_>>();
    metrics.objective_variables += used_cells.len() + 2;
    require_canonical_origin(solver, input, &used_cells, tag);

    solver.post_maximum(
        ConstraintFamily::BoundingBox,
        used_cells
            .iter()
            .enumerate()
            .map(|(cell, used)| {
                let x = i32::try_from(cell).expect("grid index fits i32") % input.width;
                used.scaled(x + 1)
            })
            .collect(),
        used_bounds.width,
        input.width as u64,
        tag,
    );
    solver.post_maximum(
        ConstraintFamily::BoundingBox,
        used_cells
            .iter()
            .enumerate()
            .map(|(cell, used)| {
                let y = i32::try_from(cell).expect("grid index fits i32") / input.width;
                used.scaled(y + 1)
            })
            .collect(),
        used_bounds.height,
        input.height as u64,
        tag,
    );
    let used_bounding_box_area = solver.new_variable(
        VariableFamily::Objective,
        1,
        input.cell_count,
        "used-bounding-box-area",
    );
    solver.post_times(
        ConstraintFamily::BoundingBox,
        used_bounds.width,
        used_bounds.height,
        used_bounding_box_area,
        tag,
    );
    let maximum_used_side = solver.new_variable(
        VariableFamily::Objective,
        1,
        input.width.max(input.height),
        "maximum-used-side",
    );
    solver.post_maximum(
        ConstraintFamily::BoundingBox,
        vec![used_bounds.width.scaled(1), used_bounds.height.scaled(1)],
        maximum_used_side,
        1,
        tag,
    );
    metrics.objective_variables += 4;

    let physical_tiles = [TransportKind::Belt, TransportKind::Pipe]
        .into_iter()
        .flat_map(|transport| transport_occupancy[&transport].iter().copied())
        .collect::<Vec<_>>();
    let physical_transport_tiles =
        post_sum_variable(solver, "physical-transport-tiles", &physical_tiles, tag)?;
    metrics.objective_variables += 1;

    let mut turns = Vec::with_capacity(layers.len() * cell_count);
    for (layer_index, layer) in layers.iter().enumerate() {
        for cell in 0..cell_count {
            turns.push(post_shared_turn(
                solver,
                layer_index,
                cell,
                input.width,
                &layer.arcs,
                tag,
                metrics,
            ));
        }
    }
    let total_route_turns = post_sum_variable(solver, "total-route-turns", &turns, tag)?;
    metrics.objective_variables += 1;

    let component_variables = branches
        .iter()
        .map(|component| component.selected)
        .chain(bridges.iter().map(|bridge| bridge.selected))
        .collect::<Vec<_>>();
    let logistics_component_count = post_sum_variable(
        solver,
        "logistics-component-count",
        &component_variables,
        tag,
    )?;
    metrics.objective_variables += 1;

    Ok(ExactObjectives {
        used_bounding_box_area,
        physical_transport_tiles,
        total_route_turns,
        maximum_used_side,
        logistics_component_count,
    })
}

fn post_shared_turn(
    solver: &mut RecordedModel,
    layer_index: usize,
    cell: usize,
    width: i32,
    arcs: &[Arc],
    tag: pumpkin_solver::core::proof::ConstraintTag,
    metrics: &mut ExactModelMetrics,
) -> DomainId {
    let incoming = arcs.iter().filter(|arc| arc.to == cell).collect::<Vec<_>>();
    let outgoing = arcs
        .iter()
        .filter(|arc| arc.from == cell)
        .collect::<Vec<_>>();
    let incoming_count = post_count(
        solver,
        format!("layer-{layer_index}-cell-{cell}-incoming-segment-count"),
        incoming.iter().map(|arc| arc.selected),
        tag,
    );
    let outgoing_count = post_count(
        solver,
        format!("layer-{layer_index}-cell-{cell}-outgoing-segment-count"),
        outgoing.iter().map(|arc| arc.selected),
        tag,
    );
    let exactly_one_incoming = post_exactly_one_indicator(
        solver,
        format!("layer-{layer_index}-cell-{cell}-exactly-one-incoming"),
        incoming_count,
        incoming.len(),
        tag,
    );
    let exactly_one_outgoing = post_exactly_one_indicator(
        solver,
        format!("layer-{layer_index}-cell-{cell}-exactly-one-outgoing"),
        outgoing_count,
        outgoing.len(),
        tag,
    );
    let mut orthogonal_pairs = Vec::new();
    let width_usize = usize::try_from(width).expect("validated width is positive");
    for incoming_arc in &incoming {
        for outgoing_arc in &outgoing {
            let incoming_horizontal = cell / width_usize == incoming_arc.from / width_usize;
            let outgoing_horizontal = cell / width_usize == outgoing_arc.to / width_usize;
            if incoming_horizontal == outgoing_horizontal {
                continue;
            }
            orthogonal_pairs.push(post_and(
                solver,
                format!(
                    "layer-{layer_index}-cell-{cell}-turn-pair-{}-{}",
                    incoming_arc.from, outgoing_arc.to
                ),
                incoming_arc.selected,
                outgoing_arc.selected,
                tag,
            ));
        }
    }
    let has_orthogonal_pair = post_presence(
        solver,
        VariableFamily::Objective,
        ConstraintFamily::TurnDefinition,
        format!("layer-{layer_index}-cell-{cell}-has-orthogonal-pair"),
        orthogonal_pairs.iter().copied(),
        tag,
    );
    let exact_segments = post_and(
        solver,
        format!("layer-{layer_index}-cell-{cell}-exact-segments"),
        exactly_one_incoming,
        exactly_one_outgoing,
        tag,
    );
    metrics.objective_variables += 7 + orthogonal_pairs.len();
    post_and(
        solver,
        format!("layer-{layer_index}-cell-{cell}-turn"),
        exact_segments,
        has_orthogonal_pair,
        tag,
    )
}

#[allow(clippy::too_many_arguments)]
fn extract_report(
    solution: &impl ProblemSolution,
    status: IntegratedLayoutStatus,
    input: &ModelInput,
    instances: &[ModelInstance],
    terminals: &[Vec<SharedTerminal>],
    layers: &[SharedLayer],
    branches: &[SharedBranchComponent],
    bridges: &[ModelBridge],
) -> IntegratedLayoutReport {
    let mut placements = instances
        .iter()
        .map(|instance| {
            let candidate = selected_candidate(solution, &instance.candidates);
            FacilityPlacement {
                instance: instance.input.id.clone(),
                recipe: instance.input.recipe.clone(),
                facility: instance.input.facility.clone(),
                x: i64::from(candidate.x),
                y: i64::from(candidate.y),
                width: i64::from(candidate.width),
                height: i64::from(candidate.height),
                rotation: candidate.rotation,
            }
        })
        .collect::<Vec<_>>();
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));

    let mut transport_networks = input
        .networks
        .iter()
        .enumerate()
        .map(|(network_index, network)| {
            let layer = layers
                .iter()
                .find(|layer| layer.transport == network.transport())
                .expect("every network belongs to a shared layer");
            let code = layer
                .network_indices
                .iter()
                .position(|candidate| *candidate == network_index)
                .expect("shared layer contains the network") as i32
                + 1;
            let mut cells = layer
                .arm_items
                .iter()
                .enumerate()
                .filter(|(_, items)| {
                    items
                        .iter()
                        .any(|item| solution.get_integer_value(*item) == code)
                })
                .map(|(cell, _)| world_position(cell, input.width))
                .collect::<Vec<_>>();
            cells.sort_by_key(|position| (position.y, position.x));
            cells.dedup();
            let segments = layer
                .arcs
                .iter()
                .filter(|arc| solution.get_integer_value(arc.selected) == 1)
                .filter(|arc| {
                    let direction =
                        direction_index(direction_between(arc.from, arc.to, input.width));
                    solution.get_integer_value(layer.arm_items[arc.from][direction]) == code
                })
                .map(|arc| TransportNetworkSegment {
                    from: world_position(arc.from, input.width),
                    to: world_position(arc.to, input.width),
                    rate: rate_from_flow_units(
                        solution.get_integer_value(arc.flow),
                        network.flow_scale(),
                    ),
                })
                .collect::<Vec<_>>();
            let network_terminals = terminals[network_index]
                .iter()
                .map(|terminal| {
                    let (node, endpoint, cell) =
                        selected_terminal_endpoint(solution, &terminal.endpoint);
                    TransportNetworkTerminal {
                        id: terminal.id.clone(),
                        node,
                        direction: terminal.direction,
                        endpoint,
                        position: world_position(cell, input.width),
                        rate: terminal.rate,
                    }
                })
                .collect();
            TransportNetwork {
                id: network.id().to_string(),
                requirement_ids: network
                    .route_indices()
                    .iter()
                    .map(|route_index| input.edges[*route_index].requirement_id.clone())
                    .collect(),
                item: network.item().to_string(),
                transport: network.transport(),
                cells,
                segments,
                terminals: network_terminals,
                component_ids: Vec::new(),
            }
        })
        .collect::<Vec<_>>();

    let mut logistics_components = branches
        .iter()
        .filter(|component| solution.get_integer_value(component.selected) == 1)
        .map(|component| {
            let position = world_position(component.cell, input.width);
            let owners = transport_networks
                .iter()
                .filter(|network| {
                    network.transport == component.transport && network.cells.contains(&position)
                })
                .map(|network| network.id.clone())
                .collect::<BTreeSet<_>>();
            PlacedLogisticsComponent {
                id: super::super::identity::logistics_component_id(
                    component.kind,
                    component.transport,
                    position.x,
                    position.y,
                    &owners,
                ),
                component: component.component.clone(),
                kind: component.kind,
                transport: component.transport,
                position,
                rotation: component.rotation,
            }
        })
        .collect::<Vec<_>>();
    logistics_components.extend(
        bridges
            .iter()
            .filter(|bridge| solution.get_integer_value(bridge.selected) == 1)
            .map(|bridge| {
                let position = world_position(bridge.cell, input.width);
                let owners = transport_networks
                    .iter()
                    .filter(|network| {
                        network.transport == bridge.transport && network.cells.contains(&position)
                    })
                    .map(|network| network.id.clone())
                    .collect::<BTreeSet<_>>();
                let rotation = bridge
                    .rotations
                    .iter()
                    .find(|(_, selected)| solution.get_integer_value(*selected) == 1)
                    .map(|(rotation, _)| *rotation)
                    .expect("selected bridge has one rotation");
                PlacedLogisticsComponent {
                    id: super::super::identity::logistics_component_id(
                        LogisticsComponentKind::Bridge,
                        bridge.transport,
                        position.x,
                        position.y,
                        &owners,
                    ),
                    component: bridge.component.clone(),
                    kind: LogisticsComponentKind::Bridge,
                    transport: bridge.transport,
                    position,
                    rotation,
                }
            }),
    );
    for network in &mut transport_networks {
        network.component_ids = logistics_components
            .iter()
            .filter(|component| {
                component.transport == network.transport
                    && network.cells.contains(&component.position)
            })
            .map(|component| component.id.clone())
            .collect();
    }

    let mut report = IntegratedLayoutReport {
        schema_version: INTEGRATED_LAYOUT_SCHEMA_VERSION,
        success: true,
        status,
        bounds: None,
        placements,
        logistics_components,
        transport_networks,
        phases: Vec::new(),
        exact: None,
        diagnostics: vec![
            IntegratedLayoutDiagnostic::info(
                "experimental-shared-boundary-terminals",
                "facility placement, port assignment, external boundary terminals, and item-labelled belt and pipe flow were solved in shared commodity networks",
            ),
            IntegratedLayoutDiagnostic::info(
                if status == IntegratedLayoutStatus::Optimal {
                    "integrated-layout-optimal"
                } else {
                    "integrated-layout-feasible"
                },
                "the experimental shared-layer model produced a complete solver witness",
            ),
        ],
    };
    canonicalize_report_geometry(&mut report);
    report
}

fn selected_candidate<'a>(
    solution: &impl ProblemSolution,
    candidates: &'a [Candidate],
) -> &'a Candidate {
    candidates
        .iter()
        .find(|candidate| solution.get_integer_value(candidate.selected) == 1)
        .expect("exactly one placement candidate is selected")
}

fn selected_terminal_endpoint(
    solution: &impl ProblemSolution,
    endpoint: &SharedTerminalEndpoint,
) -> (String, TransportNetworkEndpoint, usize) {
    match endpoint {
        SharedTerminalEndpoint::Flattened(options) => {
            let option = options
                .iter()
                .find(|option| solution.get_integer_value(option.selected) == 1)
                .expect("exactly one flattened endpoint option is selected");
            (
                endpoint_node(&option.endpoint).to_string(),
                option.endpoint.clone(),
                option.cell,
            )
        }
        SharedTerminalEndpoint::Factored { key, kind } => {
            let key = usize::try_from(solution.get_integer_value(*key))
                .expect("selected terminal geometry key is non-negative");
            let direction = DIRECTIONS[key % 4];
            let cell = key / 4;
            match kind {
                FactoredEndpointKind::Facility {
                    instance,
                    port_choice,
                    port_ids,
                } => {
                    let port_index = usize::try_from(solution.get_integer_value(*port_choice))
                        .expect("selected port index is non-negative");
                    let port = port_ids
                        .get(port_index)
                        .expect("selected port index belongs to the endpoint domain")
                        .clone();
                    (
                        instance.clone(),
                        TransportNetworkEndpoint::Facility {
                            instance: instance.clone(),
                            port,
                        },
                        cell,
                    )
                }
                FactoredEndpointKind::External { node } => (
                    node.clone(),
                    TransportNetworkEndpoint::External {
                        node: node.clone(),
                        side: direction_edge(direction),
                    },
                    cell,
                ),
            }
        }
    }
}

fn direction_edge(direction: CardinalDirection) -> FacilityPortEdge {
    match direction {
        CardinalDirection::North => FacilityPortEdge::North,
        CardinalDirection::East => FacilityPortEdge::East,
        CardinalDirection::South => FacilityPortEdge::South,
        CardinalDirection::West => FacilityPortEdge::West,
    }
}

fn endpoint_node(endpoint: &TransportNetworkEndpoint) -> &str {
    match endpoint {
        TransportNetworkEndpoint::Facility { instance, .. } => instance,
        TransportNetworkEndpoint::External { node, .. } => node,
    }
}
