use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pumpkin_solver::Solver;
use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::branching::Brancher;
use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::propagation::Priority;
use pumpkin_solver::core::results::{CSPSolverExecutionFlag, ProblemSolution, SatisfactionResult};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, Literal, TransformableVariable};

use super::super::endpoint_clearance_propagator::{
    EndpointClearanceOrientation, EndpointClearancePropagationCounters,
    EndpointRectangleClearancePropagatorArgs,
};
use super::super::endpoint_support_propagator::{
    EndpointSupportPropagationCounters, SparseEndpointSupportPropagatorArgs,
};
use super::super::metrics::elapsed_millis;
use super::super::recorder::{ConstraintFamily, VariableFamily};
use super::super::search_statistics::{
    MeteredBrancher, SearchEventCounters, capture_search_statistics,
};
use super::{
    BOTTOM_UP_RUNG_SCHEMA_VERSION, BottomUpRootClearanceOpportunity, BottomUpRootDomainSnapshot,
    BottomUpRootEndpointDomain, BottomUpRootFacilityDomain, BottomUpRootIntegerDomain,
    BottomUpRootLocalConnectionDomain, BottomUpRootOrientationDomain, BottomUpRungKind,
    BottomUpRungOutcome, BottomUpRungReport, BottomUpRungWitness, BottomUpSearchProfile,
    BottomUpSearchSpaceProfile, BottomUpSemanticCertificate, BottomUpTerminationReason,
    EndpointClearanceSchedulingPriority, FacilityEndpointPlacement, FacilityPortPlacement,
    FacilityPortsWitness, ModelInstance, PlacementModel, build_model,
    facility_geometry_search_space_profile, oriented_dimensions_i64,
};
use crate::facilities::{FacilityPortDefinition, FacilityPortDirection, FacilityPortEdge};
use crate::layouts::FacilityPlacementBounds;
use crate::layouts::integrated::geometry::rotate_port;
use crate::layouts::integrated::{
    EndpointInput, ExactSearchStatistics, ExactValidationStatus, IntegratedLayoutDiagnostic,
    ModelInput,
};
use crate::logistics::{CardinalDirection, TransportKind};
use crate::research::ModelComplexityMetrics;

mod provenance;

const GEOMETRY_FORMULATION: &str = "factorized-coordinate-geometry-rotation-port-support-v1";
const CLEARANCE_FORMULATION: &str =
    "factorized-coordinate-geometry-rotation-port-support-clearance-v1";
const PROPAGATED_CLEARANCE_FORMULATION: &str =
    "factorized-coordinate-geometry-rotation-port-support-point-rectangle-clearance-v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClearanceEncoding {
    None,
    ReifiedDirections,
    PointRectanglePropagator,
}

#[derive(Debug, Clone, Copy)]
struct RungContract {
    rung: BottomUpRungKind,
    formulation: &'static str,
    clearance: ClearanceEncoding,
    clearance_priority: Option<EndpointClearanceSchedulingPriority>,
    clearance_counters_enabled: Option<bool>,
    clearance_false_event_filter_enabled: Option<bool>,
}

const GEOMETRY_CONTRACT: RungContract = RungContract {
    rung: BottomUpRungKind::FacilityPortGeometry,
    formulation: GEOMETRY_FORMULATION,
    clearance: ClearanceEncoding::None,
    clearance_priority: None,
    clearance_counters_enabled: None,
    clearance_false_event_filter_enabled: None,
};

const CLEARANCE_CONTRACT: RungContract = RungContract {
    rung: BottomUpRungKind::FacilityPorts,
    formulation: CLEARANCE_FORMULATION,
    clearance: ClearanceEncoding::ReifiedDirections,
    clearance_priority: None,
    clearance_counters_enabled: None,
    clearance_false_event_filter_enabled: None,
};

const PROPAGATED_CLEARANCE_CONTRACT: RungContract = RungContract {
    rung: BottomUpRungKind::FacilityPortsPropagated,
    formulation: PROPAGATED_CLEARANCE_FORMULATION,
    clearance: ClearanceEncoding::PointRectanglePropagator,
    clearance_priority: Some(EndpointClearanceSchedulingPriority::High),
    clearance_counters_enabled: Some(true),
    clearance_false_event_filter_enabled: Some(false),
};

fn pumpkin_priority(priority: EndpointClearanceSchedulingPriority) -> Priority {
    match priority {
        EndpointClearanceSchedulingPriority::High => Priority::High,
        EndpointClearanceSchedulingPriority::Medium => Priority::Medium,
    }
}

fn search_profile(contract: RungContract) -> BottomUpSearchProfile {
    BottomUpSearchProfile {
        endpoint_clearance_priority: contract.clearance_priority,
        endpoint_clearance_counters_enabled: contract.clearance_counters_enabled,
        endpoint_clearance_false_event_filter_enabled: contract
            .clearance_false_event_filter_enabled,
    }
}

struct PortModel {
    placement: PlacementModel,
    rotations: BTreeMap<String, DomainId>,
    endpoints: Vec<ModelEndpoint>,
    support_counters: Arc<EndpointSupportPropagationCounters>,
    clearance_counters: Option<Arc<EndpointClearancePropagationCounters>>,
}

struct ModelEndpoint {
    terminal: String,
    instance: String,
    direction: FacilityPortDirection,
    transport: TransportKind,
    ports: Vec<FacilityPortDefinition>,
    port_choice: DomainId,
    local_key: DomainId,
    local_connections: Vec<LocalConnection>,
    connection_x: DomainId,
    connection_y: DomainId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct LocalConnection {
    dx: i32,
    dy: i32,
    arm_direction: CardinalDirection,
}

#[derive(Clone)]
struct RootFacilityProbe {
    instance: String,
    x: DomainId,
    y: DomainId,
    rotation: DomainId,
    orientations: Vec<RootOrientationProbe>,
}

#[derive(Clone)]
struct RootOrientationProbe {
    width: i32,
    height: i32,
    equivalent_rotations: Vec<i64>,
    selected: Literal,
}

#[derive(Clone)]
struct RootEndpointProbe {
    terminal: String,
    instance: String,
    direction: FacilityPortDirection,
    transport: TransportKind,
    port_ids: Vec<String>,
    port_choice: DomainId,
    local_key: DomainId,
    local_connections: Vec<LocalConnection>,
    connection_x: DomainId,
    connection_y: DomainId,
}

pub(super) fn solve_geometry(input: ModelInput, time_limit: Duration) -> BottomUpRungReport {
    solve(input, time_limit, GEOMETRY_CONTRACT, &BTreeMap::new())
}

pub(super) fn solve_with_clearance(input: ModelInput, time_limit: Duration) -> BottomUpRungReport {
    solve(input, time_limit, CLEARANCE_CONTRACT, &BTreeMap::new())
}

pub(super) fn solve_with_propagated_clearance(
    input: ModelInput,
    time_limit: Duration,
    priority: EndpointClearanceSchedulingPriority,
    counters_enabled: bool,
    false_event_filter_enabled: bool,
) -> BottomUpRungReport {
    solve_with_propagated_clearance_and_fixed_rotations(
        input,
        time_limit,
        priority,
        counters_enabled,
        false_event_filter_enabled,
        &BTreeMap::new(),
    )
}

pub(super) fn solve_with_propagated_clearance_and_fixed_rotations(
    input: ModelInput,
    time_limit: Duration,
    priority: EndpointClearanceSchedulingPriority,
    counters_enabled: bool,
    false_event_filter_enabled: bool,
    fixed_rotations: &BTreeMap<String, i64>,
) -> BottomUpRungReport {
    solve(
        input,
        time_limit,
        RungContract {
            clearance_priority: Some(priority),
            clearance_counters_enabled: Some(counters_enabled),
            clearance_false_event_filter_enabled: Some(false_event_filter_enabled),
            ..PROPAGATED_CLEARANCE_CONTRACT
        },
        fixed_rotations,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn solve_with_search_provenance(
    input: ModelInput,
    time_limit: Duration,
    priority: EndpointClearanceSchedulingPriority,
    counters_enabled: bool,
    false_event_filter_enabled: bool,
    fixed_rotations: &BTreeMap<String, i64>,
    target_instance: &str,
    maximum_detailed_decisions: usize,
) -> (BottomUpRungReport, super::BottomUpSearchProvenanceTrace) {
    let collector = provenance::collector(target_instance, maximum_detailed_decisions);
    let brancher_collector = Rc::clone(&collector);
    let target_instance = target_instance.to_string();
    let report = solve_with_brancher(
        input,
        time_limit,
        RungContract {
            clearance_priority: Some(priority),
            clearance_counters_enabled: Some(counters_enabled),
            clearance_false_event_filter_enabled: Some(false_event_filter_enabled),
            ..PROPAGATED_CLEARANCE_CONTRACT
        },
        fixed_rotations,
        move |model, brancher| {
            provenance::SearchProvenanceBrancher::new(
                brancher,
                provenance::SearchProvenanceProbe::new(model, &target_instance),
                brancher_collector,
            )
        },
    );
    (report, provenance::finish(&collector))
}

pub(super) fn snapshot_propagated_root(
    input: ModelInput,
    priority: EndpointClearanceSchedulingPriority,
    counters_enabled: bool,
    false_event_filter_enabled: bool,
    fixed_rotations: &BTreeMap<String, i64>,
) -> Result<BottomUpRootDomainSnapshot, IntegratedLayoutDiagnostic> {
    let construction_started = Instant::now();
    let mut port_model = build_port_model(
        &input,
        ClearanceEncoding::PointRectanglePropagator,
        pumpkin_priority(priority),
        counters_enabled,
        false_event_filter_enabled,
        fixed_rotations,
    )?;
    let model_construction_us = construction_started
        .elapsed()
        .as_micros()
        .min(u128::from(u64::MAX)) as u64;
    let model_complexity = port_model.placement.model.metrics();
    let facility_probes = root_facility_probes(&port_model);
    let endpoint_probes = root_endpoint_probes(&port_model);
    let support_counters = Arc::clone(&port_model.support_counters);
    let clearance_counters = Arc::clone(
        port_model
            .clearance_counters
            .as_ref()
            .expect("propagated root snapshot has clearance counters"),
    );
    let started = Instant::now();
    let status = port_model
        .placement
        .model
        .solver_mut()
        .propagate_to_fixpoint();
    let root_propagation_us = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
    Ok(match status {
        CSPSolverExecutionFlag::Infeasible => BottomUpRootDomainSnapshot {
            root_status: "root-infeasible",
            model_construction_us,
            root_propagation_us,
            fixed_rotations: fixed_rotations.clone(),
            facilities: Vec::new(),
            endpoints: Vec::new(),
            clearance_opportunities: Vec::new(),
            model_complexity,
            endpoint_support_statistics: support_counters.snapshot(),
            endpoint_clearance_statistics: clearance_counters.snapshot(),
        },
        CSPSolverExecutionFlag::Feasible => capture_root_snapshot(
            &port_model.placement.model,
            "root-fixpoint",
            model_construction_us,
            root_propagation_us,
            fixed_rotations,
            &facility_probes,
            &endpoint_probes,
            model_complexity,
            support_counters.snapshot(),
            clearance_counters.snapshot(),
        ),
        CSPSolverExecutionFlag::Timeout => {
            return Err(IntegratedLayoutDiagnostic::error(
                "bottom-up-root-propagation-timeout",
                "/root_propagation",
                None,
                "Pumpkin returned timeout while running the unbounded root fixpoint API",
            ));
        }
    })
}

fn solve(
    input: ModelInput,
    time_limit: Duration,
    contract: RungContract,
    fixed_rotations: &BTreeMap<String, i64>,
) -> BottomUpRungReport {
    solve_with_brancher(
        input,
        time_limit,
        contract,
        fixed_rotations,
        |_, brancher| brancher,
    )
}

fn solve_with_brancher<B, F>(
    input: ModelInput,
    time_limit: Duration,
    contract: RungContract,
    fixed_rotations: &BTreeMap<String, i64>,
    decorate_brancher: F,
) -> BottomUpRungReport
where
    B: Brancher,
    F: FnOnce(&PortModel, pumpkin_solver::core::DefaultBrancher) -> B,
{
    let ceiling = [input.width, input.height];
    let facility_count = input.instances.len();
    let endpoint_descriptors = facility_endpoint_descriptors(&input);
    let facility_terminal_count = endpoint_descriptors.len();
    let facility_terminal_ids = endpoint_descriptors
        .iter()
        .map(|descriptor| descriptor.0.clone())
        .collect::<Vec<_>>();
    let search_space = search_space_profile(&input);
    let construction_started = Instant::now();
    let mut port_model = match build_port_model(
        &input,
        contract.clearance,
        contract
            .clearance_priority
            .map(pumpkin_priority)
            .unwrap_or(Priority::High),
        contract.clearance_counters_enabled.unwrap_or(true),
        contract
            .clearance_false_event_filter_enabled
            .unwrap_or(false),
        fixed_rotations,
    ) {
        Ok(model) => model,
        Err(diagnostic) => {
            return BottomUpRungReport {
                schema_version: BOTTOM_UP_RUNG_SCHEMA_VERSION,
                rung: contract.rung,
                formulation: contract.formulation,
                search_profile: search_profile(contract),
                ceiling,
                facility_count,
                facility_terminal_count,
                facility_terminal_ids,
                semantic_certificate: semantic_certificate(contract.clearance),
                construction_ms: elapsed_millis(construction_started.elapsed()),
                search_ms: 0,
                first_witness_ms: None,
                outcome: BottomUpRungOutcome::Infeasible,
                termination_reason: BottomUpTerminationReason::ProvenInfeasible,
                witness_count: 0,
                validation: ExactValidationStatus::NotAttempted,
                search_space,
                model_complexity: ModelComplexityMetrics::unavailable(),
                search_statistics: ExactSearchStatistics::default(),
                endpoint_support_statistics: None,
                endpoint_clearance_statistics: None,
                witness: None,
                diagnostics: vec![diagnostic],
            };
        }
    };
    let construction_ms = elapsed_millis(construction_started.elapsed());
    let model_complexity = port_model.placement.model.metrics();

    let search_started = Instant::now();
    let search_event_counters = Arc::new(Mutex::new(SearchEventCounters::default()));
    let default_brancher = port_model.placement.model.solver_mut().default_brancher();
    let decorated_brancher = decorate_brancher(&port_model, default_brancher);
    let mut brancher = MeteredBrancher::new(decorated_brancher, Arc::clone(&search_event_counters));
    let mut resolver = ResolutionResolver::default();
    let mut termination = TimeBudget::starting_now(time_limit);
    let result = port_model.placement.model.solver_mut().satisfy(
        &mut brancher,
        &mut termination,
        &mut resolver,
    );
    let search_ms = elapsed_millis(search_started.elapsed());

    let (
        outcome,
        termination_reason,
        validation,
        first_witness_ms,
        witness,
        diagnostics,
        search_statistics,
    ) = match result {
        SatisfactionResult::Satisfiable(satisfiable) => {
            let solution = satisfiable.solution();
            let extracted = extract_witness(
                &solution,
                &port_model.placement.instances,
                &port_model.rotations,
                &port_model.endpoints,
            );
            let validation_diagnostics = validate_witness(
                &input,
                &extracted,
                contract.clearance != ClearanceEncoding::None,
            );
            let validation = if validation_diagnostics.is_empty() {
                ExactValidationStatus::Passed
            } else {
                ExactValidationStatus::Failed
            };
            let outcome = if validation == ExactValidationStatus::Passed {
                BottomUpRungOutcome::Feasible
            } else {
                BottomUpRungOutcome::InvalidWitness
            };
            let statistics = capture_search_statistics(
                satisfiable.solver(),
                satisfiable.brancher(),
                satisfiable.conflict_resolver(),
                &search_event_counters,
            );
            (
                outcome,
                if validation == ExactValidationStatus::Passed {
                    BottomUpTerminationReason::FirstWitness
                } else {
                    BottomUpTerminationReason::InvalidWitness
                },
                validation,
                Some(search_ms),
                Some(BottomUpRungWitness::FacilityPorts { witness: extracted }),
                validation_diagnostics,
                statistics,
            )
        }
        SatisfactionResult::Unsatisfiable(solver, brancher, resolver) => (
            BottomUpRungOutcome::Infeasible,
            BottomUpTerminationReason::ProvenInfeasible,
            ExactValidationStatus::NotAttempted,
            None,
            None,
            Vec::new(),
            capture_search_statistics(solver, brancher, resolver, &search_event_counters),
        ),
        SatisfactionResult::Unknown(solver, brancher, resolver) => (
            BottomUpRungOutcome::Unknown,
            BottomUpTerminationReason::TimeLimit,
            ExactValidationStatus::NotAttempted,
            None,
            None,
            Vec::new(),
            capture_search_statistics(solver, brancher, resolver, &search_event_counters),
        ),
    };

    BottomUpRungReport {
        schema_version: BOTTOM_UP_RUNG_SCHEMA_VERSION,
        rung: contract.rung,
        formulation: contract.formulation,
        search_profile: search_profile(contract),
        ceiling,
        facility_count,
        facility_terminal_count,
        facility_terminal_ids,
        semantic_certificate: semantic_certificate(contract.clearance),
        construction_ms,
        search_ms,
        first_witness_ms,
        outcome,
        termination_reason,
        witness_count: u32::from(witness.is_some()),
        validation,
        search_space,
        model_complexity,
        search_statistics,
        endpoint_support_statistics: Some(port_model.support_counters.snapshot()),
        endpoint_clearance_statistics: port_model
            .clearance_counters
            .as_ref()
            .map(|counters| counters.snapshot()),
        witness,
        diagnostics,
    }
}

fn root_facility_probes(model: &PortModel) -> Vec<RootFacilityProbe> {
    model
        .placement
        .instances
        .iter()
        .map(|instance| RootFacilityProbe {
            instance: instance.id.clone(),
            x: instance.x,
            y: instance.y,
            rotation: model.rotations[&instance.id],
            orientations: instance
                .orientations
                .iter()
                .map(|orientation| RootOrientationProbe {
                    width: orientation.width,
                    height: orientation.height,
                    equivalent_rotations: orientation.equivalent_rotations.clone(),
                    selected: orientation.selected,
                })
                .collect(),
        })
        .collect()
}

fn root_endpoint_probes(model: &PortModel) -> Vec<RootEndpointProbe> {
    model
        .endpoints
        .iter()
        .map(|endpoint| RootEndpointProbe {
            terminal: endpoint.terminal.clone(),
            instance: endpoint.instance.clone(),
            direction: endpoint.direction,
            transport: endpoint.transport,
            port_ids: endpoint.ports.iter().map(|port| port.id.clone()).collect(),
            port_choice: endpoint.port_choice,
            local_key: endpoint.local_key,
            local_connections: endpoint.local_connections.clone(),
            connection_x: endpoint.connection_x,
            connection_y: endpoint.connection_y,
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn capture_root_snapshot(
    solver: &Solver,
    root_status: &'static str,
    model_construction_us: u64,
    root_propagation_us: u64,
    fixed_rotations: &BTreeMap<String, i64>,
    facility_probes: &[RootFacilityProbe],
    endpoint_probes: &[RootEndpointProbe],
    model_complexity: ModelComplexityMetrics,
    endpoint_support_statistics: super::super::super::research::EndpointSupportPropagationStatistics,
    endpoint_clearance_statistics: super::EndpointClearancePropagationStatistics,
) -> BottomUpRootDomainSnapshot {
    let facilities = facility_probes
        .iter()
        .map(|probe| BottomUpRootFacilityDomain {
            instance: probe.instance.clone(),
            x: root_domain(solver, probe.x),
            y: root_domain(solver, probe.y),
            rotation: root_domain(solver, probe.rotation),
            orientations: probe
                .orientations
                .iter()
                .map(|orientation| BottomUpRootOrientationDomain {
                    width: orientation.width,
                    height: orientation.height,
                    equivalent_rotations: orientation.equivalent_rotations.clone(),
                    can_be_selected: solver.contains(&orientation.selected, 1),
                    can_be_rejected: solver.contains(&orientation.selected, 0),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let endpoints = endpoint_probes
        .iter()
        .map(|probe| {
            let local_key = root_domain(solver, probe.local_key);
            BottomUpRootEndpointDomain {
                terminal: probe.terminal.clone(),
                instance: probe.instance.clone(),
                direction: probe.direction,
                transport: probe.transport,
                port_ids: probe.port_ids.clone(),
                port_choice: root_domain(solver, probe.port_choice),
                local_connections: probe
                    .local_connections
                    .iter()
                    .enumerate()
                    .map(|(key, connection)| BottomUpRootLocalConnectionDomain {
                        key: i32::try_from(key).expect("local connection key fits i32"),
                        dx: connection.dx,
                        dy: connection.dy,
                        arm_direction: connection.arm_direction,
                        supported: root_domain_contains(
                            &local_key,
                            i32::try_from(key).expect("local connection key fits i32"),
                        ),
                    })
                    .collect(),
                local_key,
                connection_x: root_domain(solver, probe.connection_x),
                connection_y: root_domain(solver, probe.connection_y),
            }
        })
        .collect::<Vec<_>>();
    let clearance_opportunities = clearance_opportunities(&facilities, &endpoints);
    BottomUpRootDomainSnapshot {
        root_status,
        model_construction_us,
        root_propagation_us,
        fixed_rotations: fixed_rotations.clone(),
        facilities,
        endpoints,
        clearance_opportunities,
        model_complexity,
        endpoint_support_statistics,
        endpoint_clearance_statistics,
    }
}

fn root_domain(solver: &Solver, variable: DomainId) -> BottomUpRootIntegerDomain {
    let lower_bound = solver.lower_bound(&variable);
    let upper_bound = solver.upper_bound(&variable);
    let mut cardinality = 0;
    let mut ranges = Vec::<[i32; 2]>::new();
    for value in lower_bound..=upper_bound {
        if !solver.contains(&variable, value) {
            continue;
        }
        cardinality += 1;
        if let Some(range) = ranges.last_mut()
            && range[1].checked_add(1) == Some(value)
        {
            range[1] = value;
        } else {
            ranges.push([value, value]);
        }
    }
    BottomUpRootIntegerDomain {
        lower_bound,
        upper_bound,
        cardinality,
        ranges,
    }
}

fn root_domain_contains(domain: &BottomUpRootIntegerDomain, value: i32) -> bool {
    domain
        .ranges
        .iter()
        .any(|range| range[0] <= value && value <= range[1])
}

fn clearance_opportunities(
    facilities: &[BottomUpRootFacilityDomain],
    endpoints: &[BottomUpRootEndpointDomain],
) -> Vec<BottomUpRootClearanceOpportunity> {
    let by_instance = facilities
        .iter()
        .map(|facility| (facility.instance.as_str(), facility))
        .collect::<BTreeMap<_, _>>();
    let mut opportunities = Vec::new();
    for endpoint in endpoints {
        let owner = by_instance[endpoint.instance.as_str()];
        for connection in endpoint
            .local_connections
            .iter()
            .filter(|connection| connection.supported)
        {
            for target in facilities
                .iter()
                .filter(|target| target.instance != endpoint.instance)
            {
                for orientation in target
                    .orientations
                    .iter()
                    .filter(|orientation| orientation.can_be_selected)
                {
                    if !cartesian_domains_can_clear_rectangle(
                        &owner.x,
                        &owner.y,
                        connection.dx,
                        connection.dy,
                        &target.x,
                        &target.y,
                        orientation.width,
                        orientation.height,
                    ) {
                        opportunities.push(BottomUpRootClearanceOpportunity {
                            terminal: endpoint.terminal.clone(),
                            owner_instance: endpoint.instance.clone(),
                            local_key: connection.key,
                            dx: connection.dx,
                            dy: connection.dy,
                            target_instance: target.instance.clone(),
                            target_width: orientation.width,
                            target_height: orientation.height,
                            target_equivalent_rotations: orientation.equivalent_rotations.clone(),
                        });
                    }
                }
            }
        }
    }
    opportunities
}

#[allow(clippy::too_many_arguments)]
fn cartesian_domains_can_clear_rectangle(
    owner_x: &BottomUpRootIntegerDomain,
    owner_y: &BottomUpRootIntegerDomain,
    dx: i32,
    dy: i32,
    target_x: &BottomUpRootIntegerDomain,
    target_y: &BottomUpRootIntegerDomain,
    target_width: i32,
    target_height: i32,
) -> bool {
    i64::from(owner_x.lower_bound) + i64::from(dx) < i64::from(target_x.upper_bound)
        || i64::from(owner_x.upper_bound) + i64::from(dx)
            >= i64::from(target_x.lower_bound) + i64::from(target_width)
        || i64::from(owner_y.lower_bound) + i64::from(dy) < i64::from(target_y.upper_bound)
        || i64::from(owner_y.upper_bound) + i64::from(dy)
            >= i64::from(target_y.lower_bound) + i64::from(target_height)
}

fn semantic_certificate(clearance: ClearanceEncoding) -> BottomUpSemanticCertificate {
    BottomUpSemanticCertificate {
        facility_geometry: true,
        facility_ports: true,
        facility_endpoint_clearance: clearance != ClearanceEncoding::None,
        boundary_terminals: false,
        pipe_routing: false,
        belt_routing: false,
        item_flow: false,
        logistics_components: false,
        objective: false,
        hints: false,
        transferred_learned_state: false,
    }
}

fn search_space_profile(input: &ModelInput) -> BottomUpSearchSpaceProfile {
    let geometry = facility_geometry_search_space_profile(input);
    let mut port_log2 = 0.0;
    let mut empty = false;
    let mut port_domain_histogram = BTreeMap::new();
    for (_, _, _, _, ports) in facility_endpoint_descriptors(input) {
        if ports.is_empty() {
            empty = true;
        } else {
            port_log2 += (ports.len() as f64).log2();
            *port_domain_histogram.entry(ports.len()).or_insert(0) += 1;
        }
    }
    let directional = geometry.directional_rotation_upper_bound_log2;
    if empty || directional.is_none() {
        return BottomUpSearchSpaceProfile {
            semantic_assignment_upper_bound_log2: None,
            semantic_assignment_upper_bound_log10: None,
            directional_rotation_upper_bound_log2: directional,
            directional_rotation_upper_bound_log10: geometry.directional_rotation_upper_bound_log10,
            rotation_equivalence_reduction_log2: None,
            facility_port_choice_upper_bound_log2: None,
            facility_port_choice_upper_bound_log10: None,
            facility_port_domain_histogram: Some(port_domain_histogram),
        };
    }
    let total_log2 = directional.expect("checked directional volume") + port_log2;
    BottomUpSearchSpaceProfile {
        semantic_assignment_upper_bound_log2: Some(total_log2),
        semantic_assignment_upper_bound_log10: Some(total_log2 * std::f64::consts::LOG10_2),
        directional_rotation_upper_bound_log2: directional,
        directional_rotation_upper_bound_log10: geometry.directional_rotation_upper_bound_log10,
        rotation_equivalence_reduction_log2: Some(0.0),
        facility_port_choice_upper_bound_log2: Some(port_log2),
        facility_port_choice_upper_bound_log10: Some(port_log2 * std::f64::consts::LOG10_2),
        facility_port_domain_histogram: Some(port_domain_histogram),
    }
}

fn build_port_model(
    input: &ModelInput,
    clearance: ClearanceEncoding,
    clearance_priority: Priority,
    clearance_counters_enabled: bool,
    clearance_false_event_filter_enabled: bool,
    fixed_rotations: &BTreeMap<String, i64>,
) -> Result<PortModel, IntegratedLayoutDiagnostic> {
    let mut placement = build_model(input)?;
    let tag = placement.model.new_constraint_tag();
    let rotations = build_rotation_channels(&mut placement, tag);
    post_fixed_rotations(&mut placement, &rotations, fixed_rotations, tag)?;
    let support_counters = Arc::new(EndpointSupportPropagationCounters::default());
    let clearance_counters =
        (clearance == ClearanceEncoding::PointRectanglePropagator).then(|| {
            Arc::new(EndpointClearancePropagationCounters::new(
                clearance_counters_enabled,
            ))
        });
    let endpoints = build_endpoints(
        &mut placement,
        input,
        &rotations,
        Arc::clone(&support_counters),
        clearance,
        clearance_priority,
        clearance_counters.clone(),
        clearance_false_event_filter_enabled,
        tag,
    )?;
    Ok(PortModel {
        placement,
        rotations,
        endpoints,
        support_counters,
        clearance_counters,
    })
}

fn post_fixed_rotations(
    placement: &mut PlacementModel,
    rotations: &BTreeMap<String, DomainId>,
    fixed_rotations: &BTreeMap<String, i64>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    for (instance_id, fixed_rotation) in fixed_rotations {
        let Some(rotation) = rotations.get(instance_id) else {
            return Err(IntegratedLayoutDiagnostic::error(
                "bottom-up-fixed-rotation-unknown-facility",
                "/fixed_rotations",
                Some(instance_id.clone()),
                "fixed directional rotation refers to a facility outside the cumulative phase",
            ));
        };
        let legal = placement
            .instances
            .iter()
            .find(|instance| instance.id == *instance_id)
            .is_some_and(|instance| {
                instance
                    .orientations
                    .iter()
                    .any(|orientation| orientation.equivalent_rotations.contains(fixed_rotation))
            });
        if !legal {
            return Err(IntegratedLayoutDiagnostic::error(
                "bottom-up-fixed-rotation-illegal",
                "/fixed_rotations",
                Some(instance_id.clone()),
                format!(
                    "directional rotation {fixed_rotation} is not legal for the selected facility"
                ),
            ));
        }
        let fixed_rotation = i32::try_from(*fixed_rotation).map_err(|_| {
            IntegratedLayoutDiagnostic::error(
                "bottom-up-fixed-rotation-out-of-range",
                "/fixed_rotations",
                Some(instance_id.clone()),
                "fixed directional rotation does not fit the solver integer range",
            )
        })?;
        placement.model.post_predicate_clause(
            ConstraintFamily::ResearchFixation,
            &[*rotation],
            vec![rotation.equality_predicate(fixed_rotation)],
            tag,
        );
    }
    Ok(())
}

fn build_rotation_channels(
    placement: &mut PlacementModel,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> BTreeMap<String, DomainId> {
    let mut rotations = BTreeMap::new();
    for instance in &placement.instances {
        let mut values = instance
            .orientations
            .iter()
            .flat_map(|orientation| orientation.equivalent_rotations.iter().copied())
            .map(|rotation| i32::try_from(rotation).expect("validated rotation fits i32"))
            .collect::<Vec<_>>();
        values.sort_unstable();
        values.dedup();
        let rotation = placement.model.new_sparse_variable(
            VariableFamily::Placement,
            values,
            format!("facility:{}:directional-rotation", instance.id),
        );
        for orientation in &instance.orientations {
            let mut variables = vec![orientation.selected_parent];
            variables.extend(orientation.equivalent_rotations.iter().map(|_| rotation));
            let mut predicates = vec![orientation.selected.get_false_predicate()];
            predicates.extend(orientation.equivalent_rotations.iter().map(|value| {
                rotation
                    .equality_predicate(i32::try_from(*value).expect("validated rotation fits i32"))
            }));
            placement.model.post_predicate_clause(
                ConstraintFamily::PlacementChoice,
                &variables,
                predicates,
                tag,
            );
            for value in &orientation.equivalent_rotations {
                placement.model.post_predicate_clause(
                    ConstraintFamily::PlacementChoice,
                    &[rotation, orientation.selected_parent],
                    vec![
                        rotation.disequality_predicate(
                            i32::try_from(*value).expect("validated rotation fits i32"),
                        ),
                        orientation.selected.get_true_predicate(),
                    ],
                    tag,
                );
            }
        }
        rotations.insert(instance.id.clone(), rotation);
    }
    rotations
}

fn build_endpoints(
    placement: &mut PlacementModel,
    input: &ModelInput,
    rotations: &BTreeMap<String, DomainId>,
    counters: Arc<EndpointSupportPropagationCounters>,
    clearance: ClearanceEncoding,
    clearance_priority: Priority,
    clearance_counters: Option<Arc<EndpointClearancePropagationCounters>>,
    clearance_false_event_filter_enabled: bool,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<Vec<ModelEndpoint>, IntegratedLayoutDiagnostic> {
    let instances = input
        .instances
        .iter()
        .map(|instance| (instance.id.as_str(), instance))
        .collect::<BTreeMap<_, _>>();
    let mut endpoints = Vec::new();
    for (terminal, instance_id, direction, transport, ports) in facility_endpoint_descriptors(input)
    {
        let instance = instances[instance_id.as_str()];
        let rotation = rotations[instance_id.as_str()];
        let rotation_values = placement
            .instances
            .iter()
            .find(|candidate| candidate.id == instance_id)
            .expect("prepared endpoint instance is modeled")
            .orientations
            .iter()
            .flat_map(|orientation| orientation.equivalent_rotations.iter().copied())
            .map(|value| i32::try_from(value).expect("validated rotation fits i32"))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mut connections = BTreeSet::new();
        let mut raw_rows = Vec::new();
        for rotation_value in &rotation_values {
            for (port_index, port) in ports.iter().enumerate() {
                let connection =
                    local_connection(&instance.definition, port, i64::from(*rotation_value));
                connections.insert(connection);
                raw_rows.push((*rotation_value, port_index, connection));
            }
        }
        if raw_rows.is_empty() {
            return Err(IntegratedLayoutDiagnostic::error(
                "bottom-up-terminal-has-no-port-support",
                "/networks/terminals",
                Some(terminal.clone()),
                "facility terminal has no compatible directional port support",
            ));
        }
        let local_connections = connections.into_iter().collect::<Vec<_>>();
        let connection_keys = local_connections
            .iter()
            .enumerate()
            .map(|(index, connection)| {
                (
                    *connection,
                    i32::try_from(index).expect("local connection count fits i32"),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let rows = raw_rows
            .into_iter()
            .map(|(rotation_value, port_index, connection)| {
                [
                    rotation_value,
                    i32::try_from(port_index).expect("port count fits i32"),
                    connection_keys[&connection],
                ]
            })
            .collect::<Vec<_>>();
        let port_values = (0..ports.len())
            .map(|index| i32::try_from(index).expect("port count fits i32"))
            .collect::<Vec<_>>();
        let local_values = (0..local_connections.len())
            .map(|index| i32::try_from(index).expect("local connection count fits i32"))
            .collect::<Vec<_>>();
        let port_choice = placement.model.new_sparse_variable(
            VariableFamily::Endpoint,
            port_values.clone(),
            format!("terminal:{terminal}:port"),
        );
        let local_key = placement.model.new_sparse_variable(
            VariableFamily::EndpointGeometry,
            local_values.clone(),
            format!("terminal:{terminal}:local-connection"),
        );
        let connection_x = placement.model.new_variable(
            VariableFamily::EndpointGeometry,
            0,
            input.width - 1,
            format!("terminal:{terminal}:connection-x"),
        );
        let connection_y = placement.model.new_variable(
            VariableFamily::EndpointGeometry,
            0,
            input.height - 1,
            format!("terminal:{terminal}:connection-y"),
        );
        placement.model.record_global_constraint(
            ConstraintFamily::EndpointLink,
            [rotation, port_choice, local_key],
        );
        let _ = placement
            .model
            .solver_mut()
            .add_propagator(SparseEndpointSupportPropagatorArgs {
                name: format!("terminal:{terminal}:rotation-port-local-support"),
                variables: [rotation, port_choice, local_key],
                domain_values: [rotation_values, port_values, local_values],
                rows,
                counters: Arc::clone(&counters),
                constraint_tag: tag,
            });

        let modeled_instance = placement
            .instances
            .iter()
            .find(|candidate| candidate.id == instance_id)
            .expect("prepared endpoint instance is modeled");
        for (key, connection) in local_connections.iter().enumerate() {
            let key_value = i32::try_from(key).expect("local connection count fits i32");
            let selected = placement.model.new_named_literal_for_predicate(
                VariableFamily::EndpointGeometry,
                local_key.equality_predicate(key_value),
                tag,
                format!("terminal:{terminal}:local-connection:{key_value}"),
            );
            let selected_parent = *selected.get_integer_variable().inner();
            placement.model.post_implied_equals(
                ConstraintFamily::EndpointLink,
                vec![connection_x.scaled(1), modeled_instance.x.scaled(-1)],
                connection.dx,
                1,
                selected,
                selected_parent,
                tag,
            );
            placement.model.post_implied_equals(
                ConstraintFamily::EndpointLink,
                vec![connection_y.scaled(1), modeled_instance.y.scaled(-1)],
                connection.dy,
                1,
                selected,
                selected_parent,
                tag,
            );
        }
        match clearance {
            ClearanceEncoding::None => {}
            ClearanceEncoding::ReifiedDirections => post_connection_clearance(
                &mut placement.model,
                &terminal,
                &instance_id,
                connection_x,
                connection_y,
                &placement.instances,
                tag,
            ),
            ClearanceEncoding::PointRectanglePropagator => {
                post_propagated_connection_clearance(
                    &mut placement.model,
                    &terminal,
                    &instance_id,
                    connection_x,
                    connection_y,
                    &placement.instances,
                    clearance_priority,
                    Arc::clone(
                        clearance_counters
                            .as_ref()
                            .expect("propagated clearance has counters"),
                    ),
                    clearance_false_event_filter_enabled,
                    tag,
                );
            }
        }
        endpoints.push(ModelEndpoint {
            terminal,
            instance: instance_id,
            direction,
            transport,
            ports,
            port_choice,
            local_key,
            local_connections,
            connection_x,
            connection_y,
        });
    }
    endpoints.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    Ok(endpoints)
}

fn post_connection_clearance(
    model: &mut super::super::recorder::RecordedModel,
    terminal: &str,
    owner: &str,
    connection_x: DomainId,
    connection_y: DomainId,
    instances: &[ModelInstance],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for instance in instances.iter().filter(|instance| instance.id != owner) {
        for orientation in &instance.orientations {
            let left = reify_endpoint_clearance(
                model,
                &format!(
                    "terminal:{terminal}:left-of:{}:{}x{}",
                    instance.id, orientation.width, orientation.height
                ),
                vec![connection_x.scaled(1), instance.x.scaled(-1)],
                -1,
                tag,
            );
            let right = reify_endpoint_clearance(
                model,
                &format!(
                    "terminal:{terminal}:right-of:{}:{}x{}",
                    instance.id, orientation.width, orientation.height
                ),
                vec![instance.x.scaled(1), connection_x.scaled(-1)],
                -orientation.width,
                tag,
            );
            let above = reify_endpoint_clearance(
                model,
                &format!(
                    "terminal:{terminal}:above:{}:{}x{}",
                    instance.id, orientation.width, orientation.height
                ),
                vec![connection_y.scaled(1), instance.y.scaled(-1)],
                -1,
                tag,
            );
            let below = reify_endpoint_clearance(
                model,
                &format!(
                    "terminal:{terminal}:below:{}:{}x{}",
                    instance.id, orientation.width, orientation.height
                ),
                vec![instance.y.scaled(1), connection_y.scaled(-1)],
                -orientation.height,
                tag,
            );
            let separations = [left, right, above, below];
            let mut variables = vec![orientation.selected_parent];
            variables.extend(separations.iter().map(|(_, parent)| *parent));
            let mut predicates = vec![orientation.selected.get_false_predicate()];
            predicates.extend(
                separations
                    .iter()
                    .map(|(literal, _)| literal.get_true_predicate()),
            );
            model.post_predicate_clause(
                ConstraintFamily::EndpointClearance,
                &variables,
                predicates,
                tag,
            );
        }
    }
}

fn post_propagated_connection_clearance(
    model: &mut super::super::recorder::RecordedModel,
    terminal: &str,
    owner: &str,
    connection_x: DomainId,
    connection_y: DomainId,
    instances: &[ModelInstance],
    priority: Priority,
    counters: Arc<EndpointClearancePropagationCounters>,
    false_event_filter_enabled: bool,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for instance in instances.iter().filter(|instance| instance.id != owner) {
        let orientations = instance
            .orientations
            .iter()
            .map(|orientation| EndpointClearanceOrientation {
                selected: orientation.selected,
                selected_parent: orientation.selected_parent,
                width: orientation.width,
                height: orientation.height,
            })
            .collect::<Vec<_>>();
        let mut variables = vec![connection_x, connection_y, instance.x, instance.y];
        variables.extend(
            orientations
                .iter()
                .map(|orientation| orientation.selected_parent),
        );
        model.record_global_constraint(ConstraintFamily::EndpointClearance, variables);
        let _ = model
            .solver_mut()
            .add_propagator(EndpointRectangleClearancePropagatorArgs {
                name: format!(
                    "terminal:{terminal}:outside-facility:{}:point-rectangle-clearance",
                    instance.id
                ),
                connection_x,
                connection_y,
                facility_x: instance.x,
                facility_y: instance.y,
                orientations,
                priority,
                counters: Arc::clone(&counters),
                false_event_filter_enabled,
                constraint_tag: tag,
            });
    }
}

fn reify_endpoint_clearance(
    model: &mut super::super::recorder::RecordedModel,
    name: &str,
    terms: Vec<pumpkin_solver::core::variables::AffineView<DomainId>>,
    rhs: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> (Literal, DomainId) {
    let literal = model.new_named_literal(VariableFamily::EndpointGeometry, name);
    let parent = *literal.get_integer_variable().inner();
    model.post_reified_less_than_or_equals(
        ConstraintFamily::EndpointClearance,
        terms,
        rhs,
        1,
        literal,
        parent,
        tag,
    );
    (literal, parent)
}

fn facility_endpoint_descriptors(
    input: &ModelInput,
) -> Vec<(
    String,
    String,
    FacilityPortDirection,
    TransportKind,
    Vec<FacilityPortDefinition>,
)> {
    let mut descriptors = Vec::new();
    for network in &input.networks {
        for terminal in network.terminals() {
            let EndpointInput::Facility { instance, ports } = terminal.endpoint() else {
                continue;
            };
            descriptors.push((
                terminal.id().to_string(),
                instance.clone(),
                terminal.direction(),
                network.transport(),
                ports.clone(),
            ));
        }
    }
    descriptors.sort_by(|left, right| left.0.cmp(&right.0));
    descriptors
}

fn local_connection(
    definition: &crate::facilities::FacilityDefinition,
    port: &FacilityPortDefinition,
    rotation: i64,
) -> LocalConnection {
    let (position, edge) = rotate_port(
        &port.position,
        port.edge,
        rotation,
        definition.footprint.width,
        definition.footprint.height,
    );
    let (delta_x, delta_y) = edge_delta(edge);
    LocalConnection {
        dx: i32::try_from(position.x).expect("validated port x fits i32") + delta_x,
        dy: i32::try_from(position.y).expect("validated port y fits i32") + delta_y,
        arm_direction: opposite_direction(edge_direction(edge)),
    }
}

fn edge_delta(edge: FacilityPortEdge) -> (i32, i32) {
    match edge {
        FacilityPortEdge::North => (0, -1),
        FacilityPortEdge::East => (1, 0),
        FacilityPortEdge::South => (0, 1),
        FacilityPortEdge::West => (-1, 0),
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

fn opposite_direction(direction: CardinalDirection) -> CardinalDirection {
    match direction {
        CardinalDirection::North => CardinalDirection::South,
        CardinalDirection::East => CardinalDirection::West,
        CardinalDirection::South => CardinalDirection::North,
        CardinalDirection::West => CardinalDirection::East,
    }
}

fn extract_witness(
    solution: &impl ProblemSolution,
    instances: &[ModelInstance],
    rotations: &BTreeMap<String, DomainId>,
    model_endpoints: &[ModelEndpoint],
) -> FacilityPortsWitness {
    let mut placements = instances
        .iter()
        .map(|instance| {
            let rotation = i64::from(solution.get_integer_value(rotations[&instance.id]));
            let orientation = instance
                .orientations
                .iter()
                .find(|orientation| orientation.equivalent_rotations.contains(&rotation))
                .expect("directional rotation belongs to one geometry class");
            FacilityPortPlacement {
                instance: instance.id.clone(),
                recipe: instance.recipe.clone(),
                facility: instance.facility.clone(),
                x: i64::from(solution.get_integer_value(instance.x)),
                y: i64::from(solution.get_integer_value(instance.y)),
                width: i64::from(orientation.width),
                height: i64::from(orientation.height),
                rotation,
            }
        })
        .collect::<Vec<_>>();
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));

    let mut endpoints = model_endpoints
        .iter()
        .map(|endpoint| {
            let port_index = usize::try_from(solution.get_integer_value(endpoint.port_choice))
                .expect("port choice is non-negative");
            let local_index = usize::try_from(solution.get_integer_value(endpoint.local_key))
                .expect("local connection key is non-negative");
            FacilityEndpointPlacement {
                terminal: endpoint.terminal.clone(),
                instance: endpoint.instance.clone(),
                port: endpoint.ports[port_index].id.clone(),
                direction: endpoint.direction,
                transport: endpoint.transport,
                connection_x: i64::from(solution.get_integer_value(endpoint.connection_x)),
                connection_y: i64::from(solution.get_integer_value(endpoint.connection_y)),
                arm_direction: endpoint.local_connections[local_index].arm_direction,
            }
        })
        .collect::<Vec<_>>();
    endpoints.sort_by(|left, right| left.terminal.cmp(&right.terminal));
    let bounds = used_bounds(&placements, &endpoints);
    FacilityPortsWitness {
        bounds,
        placements,
        endpoints,
    }
}

fn validate_witness(
    input: &ModelInput,
    witness: &FacilityPortsWitness,
    enforce_clearance: bool,
) -> Vec<IntegratedLayoutDiagnostic> {
    let mut diagnostics = Vec::new();
    let expected_instances = input
        .instances
        .iter()
        .map(|instance| (instance.id.as_str(), instance))
        .collect::<BTreeMap<_, _>>();
    let mut placements = BTreeMap::new();
    for placement in &witness.placements {
        if placements
            .insert(placement.instance.as_str(), placement)
            .is_some()
        {
            diagnostics.push(diagnostic(
                "bottom-up-duplicate-facility-placement",
                &placement.instance,
                "facility-port witness contains a duplicate facility placement",
            ));
            continue;
        }
        let Some(instance) = expected_instances.get(placement.instance.as_str()) else {
            diagnostics.push(diagnostic(
                "bottom-up-unexpected-facility-placement",
                &placement.instance,
                "facility-port witness contains an unexpected facility placement",
            ));
            continue;
        };
        if placement.recipe != instance.recipe
            || placement.facility != instance.facility
            || !instance
                .definition
                .allowed_rotations
                .contains(&placement.rotation)
        {
            diagnostics.push(diagnostic(
                "bottom-up-invalid-directional-facility-placement",
                &placement.instance,
                "facility-port witness changed facility identity or selected an invalid rotation",
            ));
        }
        let expected_dimensions = oriented_dimensions_i64(
            instance.definition.footprint.width,
            instance.definition.footprint.height,
            placement.rotation,
        );
        if (placement.width, placement.height) != expected_dimensions
            || placement.x < 0
            || placement.y < 0
            || placement.x + placement.width > i64::from(input.width)
            || placement.y + placement.height > i64::from(input.height)
        {
            diagnostics.push(diagnostic(
                "bottom-up-invalid-directional-facility-geometry",
                &placement.instance,
                "facility-port witness dimensions or origin are inconsistent with the ceiling",
            ));
        }
    }
    for instance in &input.instances {
        if !placements.contains_key(instance.id.as_str()) {
            diagnostics.push(diagnostic(
                "bottom-up-missing-facility-placement",
                &instance.id,
                "facility-port witness omitted a modeled facility",
            ));
        }
    }
    for left in 0..witness.placements.len() {
        for right in (left + 1)..witness.placements.len() {
            if port_placements_overlap(&witness.placements[left], &witness.placements[right]) {
                diagnostics.push(diagnostic(
                    "bottom-up-overlapping-facilities",
                    &format!(
                        "{}:{}",
                        witness.placements[left].instance, witness.placements[right].instance
                    ),
                    "facility-port witness contains overlapping facility footprints",
                ));
            }
        }
    }

    let expected_endpoints = facility_endpoint_descriptors(input)
        .into_iter()
        .map(|descriptor| (descriptor.0.clone(), descriptor))
        .collect::<BTreeMap<_, _>>();
    let mut seen_endpoints = BTreeSet::new();
    for endpoint in &witness.endpoints {
        if !seen_endpoints.insert(endpoint.terminal.as_str()) {
            diagnostics.push(diagnostic(
                "bottom-up-duplicate-facility-endpoint",
                &endpoint.terminal,
                "facility-port witness contains a duplicate facility endpoint",
            ));
            continue;
        }
        let Some((_, instance_id, direction, transport, ports)) =
            expected_endpoints.get(&endpoint.terminal)
        else {
            diagnostics.push(diagnostic(
                "bottom-up-unexpected-facility-endpoint",
                &endpoint.terminal,
                "facility-port witness contains an unexpected facility endpoint",
            ));
            continue;
        };
        let Some(placement) = placements.get(instance_id.as_str()) else {
            continue;
        };
        let Some(port) = ports.iter().find(|port| port.id == endpoint.port) else {
            diagnostics.push(diagnostic(
                "bottom-up-invalid-facility-port",
                &endpoint.terminal,
                "facility-port witness selected a port outside the compatible endpoint domain",
            ));
            continue;
        };
        if endpoint.instance != *instance_id
            || endpoint.direction != *direction
            || endpoint.transport != *transport
        {
            diagnostics.push(diagnostic(
                "bottom-up-facility-endpoint-identity-mismatch",
                &endpoint.terminal,
                "facility-port witness changed endpoint ownership, direction, or transport",
            ));
        }
        let definition = &expected_instances[instance_id.as_str()].definition;
        let expected_local = local_connection(definition, port, placement.rotation);
        let expected_x = placement.x + i64::from(expected_local.dx);
        let expected_y = placement.y + i64::from(expected_local.dy);
        if endpoint.connection_x != expected_x
            || endpoint.connection_y != expected_y
            || endpoint.arm_direction != expected_local.arm_direction
            || endpoint.connection_x < 0
            || endpoint.connection_y < 0
            || endpoint.connection_x >= i64::from(input.width)
            || endpoint.connection_y >= i64::from(input.height)
        {
            diagnostics.push(diagnostic(
                "bottom-up-invalid-facility-endpoint-geometry",
                &endpoint.terminal,
                "facility-port witness connection cell or arm does not match placement, rotation, and port",
            ));
        }
        if enforce_clearance
            && witness.placements.iter().any(|facility| {
                endpoint.connection_x >= facility.x
                    && endpoint.connection_x < facility.x + facility.width
                    && endpoint.connection_y >= facility.y
                    && endpoint.connection_y < facility.y + facility.height
            })
        {
            diagnostics.push(diagnostic(
                "bottom-up-blocked-facility-endpoint",
                &endpoint.terminal,
                "facility-port witness connection cell is blocked by a facility footprint",
            ));
        }
    }
    for terminal in expected_endpoints.keys() {
        if !seen_endpoints.contains(terminal.as_str()) {
            diagnostics.push(diagnostic(
                "bottom-up-missing-facility-endpoint",
                terminal,
                "facility-port witness omitted a modeled facility endpoint",
            ));
        }
    }
    if witness.bounds != used_bounds(&witness.placements, &witness.endpoints) {
        diagnostics.push(IntegratedLayoutDiagnostic::error(
            "bottom-up-facility-port-bounds-mismatch",
            "/witness/bounds",
            None,
            "facility-port witness bounds do not equal its modeled facility and connection geometry",
        ));
    }
    diagnostics
}

fn used_bounds(
    placements: &[FacilityPortPlacement],
    endpoints: &[FacilityEndpointPlacement],
) -> FacilityPlacementBounds {
    let mut min_x = i64::MAX;
    let mut min_y = i64::MAX;
    let mut max_x = i64::MIN;
    let mut max_y = i64::MIN;
    for placement in placements {
        min_x = min_x.min(placement.x);
        min_y = min_y.min(placement.y);
        max_x = max_x.max(placement.x + placement.width);
        max_y = max_y.max(placement.y + placement.height);
    }
    for endpoint in endpoints {
        min_x = min_x.min(endpoint.connection_x);
        min_y = min_y.min(endpoint.connection_y);
        max_x = max_x.max(endpoint.connection_x + 1);
        max_y = max_y.max(endpoint.connection_y + 1);
    }
    if min_x == i64::MAX {
        FacilityPlacementBounds {
            width: 0,
            height: 0,
        }
    } else {
        FacilityPlacementBounds {
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }
}

fn port_placements_overlap(left: &FacilityPortPlacement, right: &FacilityPortPlacement) -> bool {
    left.x < right.x + right.width
        && right.x < left.x + left.width
        && left.y < right.y + right.height
        && right.y < left.y + left.height
}

fn diagnostic(
    code: &'static str,
    entity: &str,
    message: &'static str,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(code, "/witness", Some(entity.to_string()), message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityDefinition, FacilityFootprint, FacilityPortPosition};
    use crate::layouts::integrated::{ComponentCapacityRates, EdgeInput, InstanceInput, networks};
    use crate::recipes::{FacilityInstanceWiringEdge, Rate};

    fn east_port_fixture() -> (FacilityDefinition, FacilityPortDefinition) {
        let port = FacilityPortDefinition {
            id: "output".to_string(),
            direction: FacilityPortDirection::Output,
            transport: TransportKind::Belt,
            position: FacilityPortPosition { x: 2, y: 0 },
            edge: FacilityPortEdge::East,
        };
        let definition = FacilityDefinition {
            id: "fixture".to_string(),
            footprint: FacilityFootprint {
                width: 3,
                height: 2,
            },
            allowed_rotations: vec![0, 90, 180, 270],
            ports: vec![port.clone()],
        };
        (definition, port)
    }

    #[test]
    fn rotates_the_selected_port_to_the_exact_outside_connection_cell() {
        let (definition, port) = east_port_fixture();

        assert_eq!(
            local_connection(&definition, &port, 0),
            LocalConnection {
                dx: 3,
                dy: 0,
                arm_direction: CardinalDirection::West,
            }
        );
        assert_eq!(
            local_connection(&definition, &port, 90),
            LocalConnection {
                dx: 1,
                dy: 3,
                arm_direction: CardinalDirection::North,
            }
        );
        assert_eq!(
            local_connection(&definition, &port, 180),
            LocalConnection {
                dx: -1,
                dy: 1,
                arm_direction: CardinalDirection::East,
            }
        );
        assert_eq!(
            local_connection(&definition, &port, 270),
            LocalConnection {
                dx: 0,
                dy: -1,
                arm_direction: CardinalDirection::South,
            }
        );
    }

    #[test]
    fn every_rotated_connection_cell_is_outside_its_owner_footprint() {
        let (definition, port) = east_port_fixture();

        for rotation in definition.allowed_rotations.iter().copied() {
            let connection = local_connection(&definition, &port, rotation);
            let (width, height) = oriented_dimensions_i64(
                definition.footprint.width,
                definition.footprint.height,
                rotation,
            );
            assert!(
                connection.dx < 0
                    || connection.dy < 0
                    || i64::from(connection.dx) >= width
                    || i64::from(connection.dy) >= height
            );
        }
    }

    #[test]
    fn solves_and_validates_a_facility_owned_port_without_routing_state() {
        let (definition, port) = east_port_fixture();
        let rate = Rate {
            numerator: 1,
            denominator: 1,
        };
        let edge = EdgeInput {
            requirement_id: "requirement".to_string(),
            edge: FacilityInstanceWiringEdge::original(
                "fixture-instance",
                "external-output",
                "surplus",
                "item",
                rate,
            ),
            source: EndpointInput::Facility {
                instance: "fixture-instance".to_string(),
                ports: vec![port],
            },
            target: EndpointInput::External {
                node: "external-output".to_string(),
            },
            transport: TransportKind::Belt,
            capacity_rate: rate,
            component_capacity_rates: ComponentCapacityRates {
                splitter: rate,
                converger: rate,
                bridge: rate,
            },
        };
        let edges = vec![edge];
        let input = ModelInput {
            width: 4,
            height: 4,
            cell_count: 16,
            instances: vec![InstanceInput {
                id: "fixture-instance".to_string(),
                recipe: "fixture-recipe".to_string(),
                facility: definition.id.clone(),
                definition,
            }],
            networks: networks::normalize(&edges).expect("fixture network should normalize"),
            edges,
        };

        let report = solve_with_clearance(input.clone(), Duration::from_secs(1));

        assert_eq!(report.outcome, BottomUpRungOutcome::Feasible);
        assert_eq!(report.validation, ExactValidationStatus::Passed);
        assert!(report.semantic_certificate.facility_endpoint_clearance);
        assert!(!report.semantic_certificate.pipe_routing);
        assert!(!report.semantic_certificate.belt_routing);
        let BottomUpRungWitness::FacilityPorts { witness } = report.witness.unwrap() else {
            panic!("facility-port rung should return its dedicated witness");
        };
        assert_eq!(witness.placements.len(), 1);
        assert_eq!(witness.endpoints.len(), 1);
        assert_eq!(witness.endpoints[0].port, "output");

        let propagated = solve_with_propagated_clearance(
            input.clone(),
            Duration::from_secs(1),
            EndpointClearanceSchedulingPriority::High,
            true,
            true,
        );
        assert_eq!(propagated.outcome, BottomUpRungOutcome::Feasible);
        assert_eq!(propagated.validation, ExactValidationStatus::Passed);
        assert_eq!(propagated.rung, BottomUpRungKind::FacilityPortsPropagated);
        assert_eq!(
            propagated
                .search_profile
                .endpoint_clearance_false_event_filter_enabled,
            Some(true)
        );
        assert!(propagated.endpoint_clearance_statistics.is_some());

        let medium = solve_with_propagated_clearance(
            input.clone(),
            Duration::from_secs(1),
            EndpointClearanceSchedulingPriority::Medium,
            true,
            false,
        );
        assert_eq!(medium.outcome, BottomUpRungOutcome::Feasible);
        assert_eq!(medium.validation, ExactValidationStatus::Passed);
        assert_eq!(medium.rung, BottomUpRungKind::FacilityPortsPropagated);
        assert_eq!(
            medium.search_profile.endpoint_clearance_priority,
            Some(EndpointClearanceSchedulingPriority::Medium)
        );
        assert!(medium.endpoint_clearance_statistics.is_some());

        let fixed_rotations = BTreeMap::from([("fixture-instance".to_string(), 90)]);
        let root = snapshot_propagated_root(
            input.clone(),
            EndpointClearanceSchedulingPriority::High,
            true,
            false,
            &fixed_rotations,
        )
        .expect("fixed-rotation root snapshot should build");
        assert_eq!(root.root_status, "root-fixpoint");
        assert_eq!(root.facilities[0].rotation.ranges, [[90, 90]]);
        assert_eq!(root.endpoints.len(), 1);
        assert!(root.clearance_opportunities.is_empty());

        let fixed = solve_with_propagated_clearance_and_fixed_rotations(
            input.clone(),
            Duration::from_secs(1),
            EndpointClearanceSchedulingPriority::High,
            true,
            false,
            &fixed_rotations,
        );
        let (traced_fixed, trace) = solve_with_search_provenance(
            input.clone(),
            Duration::from_secs(1),
            EndpointClearanceSchedulingPriority::High,
            true,
            false,
            &fixed_rotations,
            "fixture-instance",
            16,
        );
        assert_eq!(traced_fixed.outcome, fixed.outcome);
        assert_eq!(traced_fixed.validation, fixed.validation);
        assert_eq!(traced_fixed.witness, fixed.witness);
        assert_eq!(
            traced_fixed.search_statistics.branch_decisions,
            fixed.search_statistics.branch_decisions
        );
        assert_eq!(
            traced_fixed.search_statistics.conflicts,
            fixed.search_statistics.conflicts
        );
        assert!(trace.decision_histogram_matches_total);
        assert!(trace.decision_catalog_covers_all);
        assert_eq!(trace.unrecorded_decisions, 0);
        assert!(trace.target_rotation_decisions <= trace.decisions);
        assert_eq!(trace.first_singleton_decision.get(&90), Some(&0));
        assert_eq!(
            trace.family_checkpoints[0].trigger,
            "pre-first-branch-fixpoint"
        );
        assert_eq!(trace.target_transitions[0].state.rotation_values, [90]);
        assert_eq!(fixed.outcome, BottomUpRungOutcome::Feasible);
        assert_eq!(fixed.validation, ExactValidationStatus::Passed);
        let Some(BottomUpRungWitness::FacilityPorts { witness }) = fixed.witness else {
            panic!("fixed-rotation rung should return its dedicated witness");
        };
        assert_eq!(witness.placements[0].rotation, 90);
        assert_eq!(
            fixed
                .model_complexity
                .constraints
                .as_ref()
                .expect("complete ladder metrics include constraints")
                .by_family
                .iter()
                .find(|family| family.family == "research-fixation")
                .map(|family| family.constraints),
            Some(1)
        );

        let counters_disabled = solve_with_propagated_clearance(
            input,
            Duration::from_secs(1),
            EndpointClearanceSchedulingPriority::High,
            false,
            false,
        );
        assert_eq!(counters_disabled.outcome, BottomUpRungOutcome::Feasible);
        assert_eq!(counters_disabled.validation, ExactValidationStatus::Passed);
        assert_eq!(
            counters_disabled
                .search_profile
                .endpoint_clearance_counters_enabled,
            Some(false)
        );
        assert_eq!(
            counters_disabled.endpoint_clearance_statistics,
            Some(Default::default())
        );
    }

    #[test]
    fn cartesian_clearance_oracle_rejects_only_a_point_forced_inside() {
        let domain = |lower_bound, upper_bound| BottomUpRootIntegerDomain {
            lower_bound,
            upper_bound,
            cardinality: usize::try_from(upper_bound - lower_bound + 1).unwrap(),
            ranges: vec![[lower_bound, upper_bound]],
        };

        assert!(!cartesian_domains_can_clear_rectangle(
            &domain(5, 5),
            &domain(5, 5),
            0,
            0,
            &domain(4, 4),
            &domain(4, 4),
            3,
            3,
        ));
        assert!(cartesian_domains_can_clear_rectangle(
            &domain(0, 5),
            &domain(5, 5),
            0,
            0,
            &domain(4, 4),
            &domain(4, 4),
            3,
            3,
        ));
    }

    #[test]
    fn clearance_is_the_only_difference_for_a_forced_blocked_connection() {
        let (definition, port) = east_port_fixture();
        let blocker = FacilityDefinition {
            id: "blocker".to_string(),
            footprint: FacilityFootprint {
                width: 1,
                height: 2,
            },
            allowed_rotations: vec![0],
            ports: Vec::new(),
        };
        let rate = Rate {
            numerator: 1,
            denominator: 1,
        };
        let edge = EdgeInput {
            requirement_id: "requirement".to_string(),
            edge: FacilityInstanceWiringEdge::original(
                "fixture-instance",
                "external-output",
                "surplus",
                "item",
                rate,
            ),
            source: EndpointInput::Facility {
                instance: "fixture-instance".to_string(),
                ports: vec![port],
            },
            target: EndpointInput::External {
                node: "external-output".to_string(),
            },
            transport: TransportKind::Belt,
            capacity_rate: rate,
            component_capacity_rates: ComponentCapacityRates {
                splitter: rate,
                converger: rate,
                bridge: rate,
            },
        };
        let edges = vec![edge];
        let input = ModelInput {
            width: 4,
            height: 2,
            cell_count: 8,
            instances: vec![
                InstanceInput {
                    id: "fixture-instance".to_string(),
                    recipe: "fixture-recipe".to_string(),
                    facility: definition.id.clone(),
                    definition,
                },
                InstanceInput {
                    id: "blocker-instance".to_string(),
                    recipe: "blocker-recipe".to_string(),
                    facility: blocker.id.clone(),
                    definition: blocker,
                },
            ],
            networks: networks::normalize(&edges).expect("fixture network should normalize"),
            edges,
        };

        let geometry_report = solve_geometry(input.clone(), Duration::from_secs(1));
        assert_eq!(geometry_report.outcome, BottomUpRungOutcome::Feasible);
        assert_eq!(geometry_report.validation, ExactValidationStatus::Passed);
        assert!(
            !geometry_report
                .semantic_certificate
                .facility_endpoint_clearance
        );

        let report = solve_with_clearance(input.clone(), Duration::from_secs(1));

        assert_eq!(report.outcome, BottomUpRungOutcome::Infeasible);
        assert_eq!(report.validation, ExactValidationStatus::NotAttempted);
        assert!(report.semantic_certificate.facility_endpoint_clearance);
        assert!(report.witness.is_none());

        let propagated = solve_with_propagated_clearance(
            input.clone(),
            Duration::from_secs(1),
            EndpointClearanceSchedulingPriority::High,
            true,
            false,
        );
        assert_eq!(propagated.outcome, BottomUpRungOutcome::Infeasible);
        assert_eq!(propagated.validation, ExactValidationStatus::NotAttempted);
        assert!(propagated.semantic_certificate.facility_endpoint_clearance);
        assert!(propagated.witness.is_none());
        let statistics = propagated
            .endpoint_clearance_statistics
            .expect("propagated clearance should report counters");
        assert!(statistics.relations > 0);

        let medium = solve_with_propagated_clearance(
            input,
            Duration::from_secs(1),
            EndpointClearanceSchedulingPriority::Medium,
            true,
            false,
        );
        assert_eq!(medium.outcome, BottomUpRungOutcome::Infeasible);
        assert_eq!(medium.validation, ExactValidationStatus::NotAttempted);
        assert!(medium.semantic_certificate.facility_endpoint_clearance);
        assert!(medium.witness.is_none());
    }
}
