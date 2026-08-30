use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
use std::time::Duration;

use pumpkin_solver::Solver;
use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::DefaultBrancher;
use pumpkin_solver::core::optimisation::OptimisationDirection;
use pumpkin_solver::core::optimisation::linear_sat_unsat::LinearSatUnsat;
use pumpkin_solver::core::results::{OptimisationResult, ProblemSolution, SolutionReference};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};
use serde::Serialize;

use crate::facilities::{
    FacilityDefinition, FacilityPortDefinition, FacilityPortDirection, FacilityPortEdge,
    FacilityPortPosition, ValidatedFacilityCatalog,
};
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, FacilityPlacementRequest,
    validate_facility_placement_request,
};
use crate::logistics::{TransportKind, ValidatedItemCatalog, ValidatedTransportCatalog};
use crate::recipes::{
    FacilityInstanceWiringEdge, FacilityInstanceWiringNode, FacilityInstanceWiringReport, Rate,
};

use super::WorldGridPosition;

const STAGE: &str = "integrated-layout";

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
    pub success: bool,
    pub status: IntegratedLayoutStatus,
    pub bounds: Option<FacilityPlacementBounds>,
    pub placements: Vec<FacilityPlacement>,
    pub routes: Vec<IntegratedRoute>,
    pub diagnostics: Vec<IntegratedLayoutDiagnostic>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedRoute {
    pub source: IntegratedRouteEndpoint,
    pub target: IntegratedRouteEndpoint,
    pub item: String,
    pub rate: Rate,
    pub transport: TransportKind,
    pub cells: Vec<WorldGridPosition>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum IntegratedRouteEndpoint {
    Facility { instance: String, port: String },
    Boundary { node: String, side: BoundarySide },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum BoundarySide {
    North,
    East,
    South,
    West,
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

    fn info(code: &'static str, message: impl Into<String>) -> Self {
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

    fn failure(status: IntegratedLayoutStatus, diagnostic: IntegratedLayoutDiagnostic) -> Self {
        Self {
            success: false,
            status,
            bounds: None,
            placements: Vec::new(),
            routes: Vec::new(),
            diagnostics: vec![diagnostic],
        }
    }
}

pub fn solve_integrated_layout(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &FacilityPlacementRequest,
) -> IntegratedLayoutReport {
    solve_integrated_layout_with_optional_time_limit(
        instance_wiring,
        facilities,
        items,
        transports,
        request,
        None,
    )
}

pub fn solve_integrated_layout_with_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Duration,
) -> IntegratedLayoutReport {
    solve_integrated_layout_with_optional_time_limit(
        instance_wiring,
        facilities,
        items,
        transports,
        request,
        Some(time_limit),
    )
}

fn solve_integrated_layout_with_optional_time_limit(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &FacilityPlacementRequest,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    match prepare_model(instance_wiring, facilities, items, transports, request) {
        Ok(input) => solve(input, time_limit),
        Err(diagnostic) => {
            IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
        }
    }
}

struct ModelInput {
    width: i32,
    height: i32,
    instances: Vec<InstanceInput>,
    edges: Vec<EdgeInput>,
}

struct EdgeInput {
    edge: FacilityInstanceWiringEdge,
    source: EndpointInput,
    target: EndpointInput,
    transport: TransportKind,
}

enum EndpointInput {
    Facility {
        instance: String,
        ports: Vec<FacilityPortDefinition>,
    },
    Boundary {
        node: String,
    },
}

struct InstanceInput {
    id: String,
    recipe: String,
    facility: String,
    definition: FacilityDefinition,
}

fn prepare_model(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    request: &FacilityPlacementRequest,
) -> Result<ModelInput, IntegratedLayoutDiagnostic> {
    if !instance_wiring.success {
        return Err(IntegratedLayoutDiagnostic::error(
            "upstream-instance-wiring-failed",
            "/",
            None,
            "integrated layout requires successful facility instance wiring",
        ));
    }
    if let Some(diagnostic) = validate_facility_placement_request(request).first() {
        return Err(IntegratedLayoutDiagnostic::error(
            "invalid-layout-bounds",
            diagnostic.path.clone(),
            diagnostic.entity.clone(),
            diagnostic.message.clone(),
        ));
    }
    let mut node_by_id = BTreeMap::new();
    for (index, node) in instance_wiring.nodes.iter().enumerate() {
        let id = wiring_node_id(node);
        if node_by_id.insert(id, node).is_some() {
            return Err(IntegratedLayoutDiagnostic::error(
                "duplicate-wiring-node",
                format!("/nodes/{index}/id"),
                Some(id.to_string()),
                format!("wiring node '{id}' appears more than once"),
            ));
        }
    }
    let mut instances = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, node) in instance_wiring.nodes.iter().enumerate() {
        let FacilityInstanceWiringNode::Facility {
            id,
            recipe,
            facility,
            ..
        } = node
        else {
            continue;
        };
        if !seen.insert(id.clone()) {
            return Err(IntegratedLayoutDiagnostic::error(
                "duplicate-facility-instance",
                format!("/nodes/{index}/id"),
                Some(id.clone()),
                format!("facility instance '{id}' appears more than once"),
            ));
        }
        let definition = facilities.facility(facility).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-facility-definition",
                format!("/nodes/{index}/facility"),
                Some(facility.clone()),
                format!("facility '{facility}' is absent from the validated catalog"),
            )
        })?;
        i32::try_from(definition.footprint.width).map_err(|_| {
            IntegratedLayoutDiagnostic::error(
                "solver-domain-out-of-range",
                format!("/nodes/{index}/facility"),
                Some(facility.clone()),
                "facility width does not fit the solver's 32-bit integer domain",
            )
        })?;
        i32::try_from(definition.footprint.height).map_err(|_| {
            IntegratedLayoutDiagnostic::error(
                "solver-domain-out-of-range",
                format!("/nodes/{index}/facility"),
                Some(facility.clone()),
                "facility height does not fit the solver's 32-bit integer domain",
            )
        })?;
        instances.push(InstanceInput {
            id: id.clone(),
            recipe: recipe.clone(),
            facility: facility.clone(),
            definition: definition.clone(),
        });
    }
    instances.sort_by(|left, right| left.id.cmp(&right.id));

    let mut edges = Vec::with_capacity(instance_wiring.edges.len());
    for (edge_index, edge) in instance_wiring.edges.iter().cloned().enumerate() {
        let source_node = node_by_id
            .get(edge.source.as_str())
            .ok_or_else(|| missing_route_endpoint(edge_index, "source", edge.source.as_str()))?;
        let target_node = node_by_id
            .get(edge.target.as_str())
            .ok_or_else(|| missing_route_endpoint(edge_index, "target", edge.target.as_str()))?;
        if edge.source == edge.target {
            return Err(IntegratedLayoutDiagnostic::error(
                "unsupported-self-route",
                format!("/edges/{edge_index}"),
                Some(edge.source.clone()),
                "integrated routing does not support a route from a node to itself",
            ));
        }

        let item = items.item(&edge.item).ok_or_else(|| {
            IntegratedLayoutDiagnostic::error(
                "missing-item-definition",
                format!("/edges/{edge_index}/item"),
                Some(edge.item.clone()),
                format!(
                    "item '{}' is absent from the validated item catalog",
                    edge.item
                ),
            )
        })?;
        let capacity = transports.capacity(item.transport);
        let capacity_rate =
            Rate::from_quantity_per_duration_ms(capacity.quantity, capacity.duration_ms).map_err(
                |_| {
                    IntegratedLayoutDiagnostic::error(
                        "transport-capacity-out-of-range",
                        format!("/edges/{edge_index}/rate"),
                        Some(format!("{:?}", item.transport).to_lowercase()),
                        "transport capacity cannot be represented in the exact rate domain",
                    )
                },
            )?;
        if edge.rate > capacity_rate {
            return Err(IntegratedLayoutDiagnostic::error(
                "route-capacity-exceeded",
                format!("/edges/{edge_index}/rate"),
                Some(edge.item.clone()),
                format!(
                    "route rate {}/{} per second exceeds {:?} capacity of {} per {} ms",
                    edge.rate.numerator,
                    edge.rate.denominator,
                    item.transport,
                    capacity.quantity,
                    capacity.duration_ms
                ),
            ));
        }
        let source = prepare_endpoint(
            edge_index,
            "source",
            source_node,
            &instances,
            FacilityPortDirection::Output,
            item.transport,
            &edge.item,
        )?;
        let target = prepare_endpoint(
            edge_index,
            "target",
            target_node,
            &instances,
            FacilityPortDirection::Input,
            item.transport,
            &edge.item,
        )?;
        edges.push(EdgeInput {
            source,
            target,
            transport: item.transport,
            edge,
        });
    }

    let width = i32::try_from(request.max_width).map_err(|_| solver_domain_error("max_width"))?;
    let height =
        i32::try_from(request.max_height).map_err(|_| solver_domain_error("max_height"))?;

    Ok(ModelInput {
        width,
        height,
        instances,
        edges,
    })
}

fn wiring_node_id(node: &FacilityInstanceWiringNode) -> &str {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. }
        | FacilityInstanceWiringNode::External { id, .. }
        | FacilityInstanceWiringNode::Target { id, .. }
        | FacilityInstanceWiringNode::Surplus { id, .. } => id,
    }
}

fn prepare_endpoint(
    edge_index: usize,
    endpoint_kind: &str,
    node: &FacilityInstanceWiringNode,
    instances: &[InstanceInput],
    port_direction: FacilityPortDirection,
    transport: TransportKind,
    item: &str,
) -> Result<EndpointInput, IntegratedLayoutDiagnostic> {
    match node {
        FacilityInstanceWiringNode::Facility { id, .. } => {
            let instance = instances
                .iter()
                .find(|instance| instance.id == *id)
                .expect("every prepared facility node has an instance");
            let ports = compatible_ports(&instance.definition, port_direction, transport);
            if ports.is_empty() {
                let direction = match port_direction {
                    FacilityPortDirection::Input => "input",
                    FacilityPortDirection::Output => "output",
                };
                return Err(missing_compatible_port(
                    edge_index, id, direction, transport,
                ));
            }
            Ok(EndpointInput::Facility {
                instance: id.clone(),
                ports,
            })
        }
        FacilityInstanceWiringNode::External {
            id,
            item: node_item,
        } if endpoint_kind == "source" => {
            prepare_boundary_endpoint(edge_index, endpoint_kind, id, node_item, item)
        }
        FacilityInstanceWiringNode::Target {
            id,
            item: node_item,
        }
        | FacilityInstanceWiringNode::Surplus {
            id,
            item: node_item,
        } if endpoint_kind == "target" => {
            prepare_boundary_endpoint(edge_index, endpoint_kind, id, node_item, item)
        }
        _ => Err(IntegratedLayoutDiagnostic::error(
            "invalid-route-endpoint-kind",
            format!("/edges/{edge_index}/{endpoint_kind}"),
            Some(wiring_node_id(node).to_string()),
            format!(
                "wiring node '{}' cannot be used as a route {endpoint_kind}",
                wiring_node_id(node)
            ),
        )),
    }
}

fn prepare_boundary_endpoint(
    edge_index: usize,
    endpoint_kind: &str,
    node: &str,
    node_item: &str,
    edge_item: &str,
) -> Result<EndpointInput, IntegratedLayoutDiagnostic> {
    if node_item != edge_item {
        return Err(IntegratedLayoutDiagnostic::error(
            "boundary-item-mismatch",
            format!("/edges/{edge_index}/item"),
            Some(node.to_string()),
            format!(
                "boundary {endpoint_kind} node '{node}' carries item '{node_item}' but the route carries '{edge_item}'"
            ),
        ));
    }
    Ok(EndpointInput::Boundary {
        node: node.to_string(),
    })
}

fn compatible_ports(
    definition: &FacilityDefinition,
    direction: FacilityPortDirection,
    transport: TransportKind,
) -> Vec<FacilityPortDefinition> {
    definition
        .ports
        .iter()
        .filter(|port| port.direction == direction && port.transport == transport)
        .cloned()
        .collect()
}

fn missing_route_endpoint(
    edge_index: usize,
    kind: &str,
    endpoint: &str,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "missing-route-endpoint",
        format!("/edges/{edge_index}/{kind}"),
        Some(endpoint.to_string()),
        format!("route {kind} node '{endpoint}' is absent from the wiring graph"),
    )
}

fn missing_compatible_port(
    edge_index: usize,
    instance: &str,
    direction: &str,
    transport: TransportKind,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "missing-compatible-port",
        format!("/edges/{edge_index}"),
        Some(instance.to_string()),
        format!("facility instance '{instance}' has no {direction} {transport:?} port"),
    )
}

fn solver_domain_error(field: &str) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "solver-domain-out-of-range",
        format!("/{field}"),
        None,
        format!("layout {field} does not fit the solver's 32-bit integer domain"),
    )
}

struct Candidate {
    rotation: i64,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    occupied_cells: Vec<usize>,
    port_connections: BTreeMap<String, usize>,
    selected: DomainId,
}

struct ModelInstance {
    input: InstanceInput,
    candidates: Vec<Candidate>,
}

struct EndpointOption {
    endpoint: IntegratedRouteEndpoint,
    cell: usize,
    selected: DomainId,
}

#[derive(Clone, Copy)]
struct Arc {
    from: usize,
    to: usize,
    selected: DomainId,
}

struct ModelRoute {
    source_options: Vec<EndpointOption>,
    target_options: Vec<EndpointOption>,
    arcs: Vec<Arc>,
}

fn solve(mut input: ModelInput, time_limit: Option<Duration>) -> IntegratedLayoutReport {
    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let cell_count = (input.width as usize) * (input.height as usize);
    let mut occupancy = vec![Vec::<DomainId>::new(); cell_count];
    let mut model_instances = Vec::with_capacity(input.instances.len());

    for instance in std::mem::take(&mut input.instances) {
        let candidates = generate_candidates(&mut solver, &instance, input.width, input.height);
        if candidates.is_empty() {
            return IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Infeasible,
                IntegratedLayoutDiagnostic::error(
                    "facility-has-no-placement-candidate",
                    "/",
                    Some(instance.id),
                    "facility has no rotation and origin within the hard layout bounds",
                ),
            );
        }
        post_equals_one(
            &mut solver,
            candidates.iter().map(|candidate| candidate.selected),
            tag,
        );
        for candidate in &candidates {
            for cell in &candidate.occupied_cells {
                occupancy[*cell].push(candidate.selected);
            }
        }
        model_instances.push(ModelInstance {
            input: instance,
            candidates,
        });
    }

    for cell_candidates in &occupancy {
        post_at_most_one(&mut solver, cell_candidates.iter().copied(), tag);
    }

    let mut model_routes = Vec::with_capacity(input.edges.len());
    let mut route_cells_by_grid = vec![Vec::<DomainId>::new(); cell_count];
    let mut route_arc_variables = Vec::new();
    for (edge_index, edge) in input.edges.iter().enumerate() {
        let source_options = model_endpoint_options(
            &mut solver,
            edge_index,
            "source",
            &edge.source,
            &model_instances,
            input.width,
            input.height,
            tag,
        );
        let target_options = model_endpoint_options(
            &mut solver,
            edge_index,
            "target",
            &edge.target,
            &model_instances,
            input.width,
            input.height,
            tag,
        );

        let (arcs, incoming, outgoing) =
            grid_arcs(&mut solver, edge_index, input.width, input.height);
        let mut source_by_cell = vec![Vec::<DomainId>::new(); cell_count];
        let mut target_by_cell = vec![Vec::<DomainId>::new(); cell_count];
        for option in &source_options {
            source_by_cell[option.cell].push(option.selected);
        }
        for option in &target_options {
            target_by_cell[option.cell].push(option.selected);
        }

        for cell in 0..cell_count {
            let route_cell =
                solver.new_named_bounded_integer(0, 1, format!("route-{edge_index}-cell-{cell}"));
            route_cells_by_grid[cell].push(route_cell);

            let mut conservation = Vec::new();
            conservation.extend(outgoing[cell].iter().map(|variable| variable.scaled(1)));
            conservation.extend(incoming[cell].iter().map(|variable| variable.scaled(-1)));
            conservation.extend(
                source_by_cell[cell]
                    .iter()
                    .map(|variable| variable.scaled(-1)),
            );
            conservation.extend(
                target_by_cell[cell]
                    .iter()
                    .map(|variable| variable.scaled(1)),
            );
            solver
                .add_constraint(pumpkin_solver::equals(conservation, 0, tag))
                .post();

            post_at_most_one(&mut solver, incoming[cell].iter().copied(), tag);
            post_at_most_one(&mut solver, outgoing[cell].iter().copied(), tag);

            post_at_most_one(
                &mut solver,
                source_by_cell[cell]
                    .iter()
                    .chain(target_by_cell[cell].iter())
                    .copied(),
                tag,
            );

            let mut route_definition = vec![route_cell.scaled(1)];
            route_definition.extend(outgoing[cell].iter().map(|variable| variable.scaled(-1)));
            route_definition.extend(
                target_by_cell[cell]
                    .iter()
                    .map(|variable| variable.scaled(-1)),
            );
            solver
                .add_constraint(pumpkin_solver::equals(route_definition, 0, tag))
                .post();
        }

        route_arc_variables.extend(arcs.iter().map(|arc| arc.selected));
        model_routes.push(ModelRoute {
            source_options,
            target_options,
            arcs,
        });
    }

    for cell in 0..cell_count {
        let mut exclusion = occupancy[cell]
            .iter()
            .chain(route_cells_by_grid[cell].iter())
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        if exclusion.len() > 1 {
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    std::mem::take(&mut exclusion),
                    1,
                    tag,
                ))
                .post();
        }
    }

    let route_length =
        solver.new_named_bounded_integer(0, route_arc_variables.len() as i32, "total-route-length");
    let mut route_length_definition = route_arc_variables
        .iter()
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    route_length_definition.push(route_length.scaled(-1));
    solver
        .add_constraint(pumpkin_solver::equals(route_length_definition, 0, tag))
        .post();

    let mut brancher = solver.default_brancher();
    let mut resolver = ResolutionResolver::default();
    let callback = |_: &Solver,
                    _: SolutionReference,
                    _: &DefaultBrancher,
                    _: &ResolutionResolver|
     -> ControlFlow<()> { ControlFlow::Continue(()) };
    let mut termination = time_limit.map(TimeBudget::starting_now);
    let result = solver.optimise(
        &mut brancher,
        &mut termination,
        &mut resolver,
        LinearSatUnsat::new(OptimisationDirection::Minimise, route_length, callback),
    );

    match result {
        OptimisationResult::Optimal(solution) => extract_report(
            &solution,
            IntegratedLayoutStatus::Optimal,
            &input,
            &model_instances,
            &model_routes,
        ),
        OptimisationResult::Satisfiable(solution) | OptimisationResult::Stopped(solution, ()) => {
            extract_report(
                &solution,
                IntegratedLayoutStatus::Feasible,
                &input,
                &model_instances,
                &model_routes,
            )
        }
        OptimisationResult::Unsatisfiable => IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::Infeasible,
            IntegratedLayoutDiagnostic::error(
                "integrated-layout-infeasible",
                "/",
                None,
                "facility placement, port selection, and route constraints are infeasible",
            ),
        ),
        OptimisationResult::Unknown => IntegratedLayoutReport::failure(
            IntegratedLayoutStatus::Unknown,
            IntegratedLayoutDiagnostic::error(
                "integrated-layout-unknown",
                "/",
                None,
                "solver terminated without a solution or proof",
            ),
        ),
    }
}

fn generate_candidates(
    solver: &mut Solver,
    instance: &InstanceInput,
    max_width: i32,
    max_height: i32,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for rotation in &instance.definition.allowed_rotations {
        let source_width = instance.definition.footprint.width as i32;
        let source_height = instance.definition.footprint.height as i32;
        let (width, height) = if matches!(rotation, 90 | 270) {
            (source_height, source_width)
        } else {
            (source_width, source_height)
        };
        if width > max_width || height > max_height {
            continue;
        }
        for y in 0..=(max_height - height) {
            for x in 0..=(max_width - width) {
                let Some(port_connections) = candidate_port_connections(
                    &instance.definition,
                    *rotation,
                    x,
                    y,
                    max_width,
                    max_height,
                ) else {
                    continue;
                };
                let occupied_cells = (y..(y + height))
                    .flat_map(|occupied_y| {
                        (x..(x + width))
                            .map(move |occupied_x| grid_index(occupied_x, occupied_y, max_width))
                    })
                    .collect();
                candidates.push(Candidate {
                    rotation: *rotation,
                    x,
                    y,
                    width,
                    height,
                    occupied_cells,
                    port_connections,
                    selected: solver.new_named_bounded_integer(
                        0,
                        1,
                        format!("place-{}-{rotation}-{x}-{y}", instance.id),
                    ),
                });
            }
        }
    }
    candidates
}

fn candidate_port_connections(
    definition: &FacilityDefinition,
    rotation: i64,
    origin_x: i32,
    origin_y: i32,
    max_width: i32,
    max_height: i32,
) -> Option<BTreeMap<String, usize>> {
    let mut connections = BTreeMap::new();
    for port in &definition.ports {
        let (position, edge) = rotate_port(
            &port.position,
            port.edge,
            rotation,
            definition.footprint.width,
            definition.footprint.height,
        );
        let port_x = origin_x + position.x as i32;
        let port_y = origin_y + position.y as i32;
        let (connection_x, connection_y) = match edge {
            FacilityPortEdge::North => (port_x, port_y - 1),
            FacilityPortEdge::East => (port_x + 1, port_y),
            FacilityPortEdge::South => (port_x, port_y + 1),
            FacilityPortEdge::West => (port_x - 1, port_y),
        };
        if connection_x < 0
            || connection_y < 0
            || connection_x >= max_width
            || connection_y >= max_height
        {
            return None;
        }
        connections.insert(
            port.id.clone(),
            grid_index(connection_x, connection_y, max_width),
        );
    }
    Some(connections)
}

fn rotate_port(
    position: &FacilityPortPosition,
    edge: FacilityPortEdge,
    rotation: i64,
    width: i64,
    height: i64,
) -> (FacilityPortPosition, FacilityPortEdge) {
    let position = match rotation {
        0 => position.clone(),
        90 => FacilityPortPosition {
            x: height - 1 - position.y,
            y: position.x,
        },
        180 => FacilityPortPosition {
            x: width - 1 - position.x,
            y: height - 1 - position.y,
        },
        270 => FacilityPortPosition {
            x: position.y,
            y: width - 1 - position.x,
        },
        _ => unreachable!("validated facility rotation"),
    };
    (position, edge.rotated_clockwise(rotation))
}

fn endpoint_options(
    solver: &mut Solver,
    edge_index: usize,
    endpoint_kind: &str,
    instance: &ModelInstance,
    ports: &[FacilityPortDefinition],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<EndpointOption> {
    let mut options = Vec::new();
    for (candidate_index, candidate) in instance.candidates.iter().enumerate() {
        let mut candidate_options = Vec::new();
        for port in ports {
            let selected = solver.new_named_bounded_integer(
                0,
                1,
                format!(
                    "edge-{edge_index}-{endpoint_kind}-{}-{}-{candidate_index}",
                    instance.input.id, port.id
                ),
            );
            candidate_options.push(selected);
            options.push(EndpointOption {
                endpoint: IntegratedRouteEndpoint::Facility {
                    instance: instance.input.id.clone(),
                    port: port.id.clone(),
                },
                cell: candidate.port_connections[&port.id],
                selected,
            });
        }
        let mut definition = candidate_options
            .iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        definition.push(candidate.selected.scaled(-1));
        solver
            .add_constraint(pumpkin_solver::equals(definition, 0, tag))
            .post();
    }
    post_equals_one(solver, options.iter().map(|option| option.selected), tag);
    options
}

#[allow(clippy::too_many_arguments)]
fn model_endpoint_options(
    solver: &mut Solver,
    edge_index: usize,
    endpoint_kind: &str,
    endpoint: &EndpointInput,
    instances: &[ModelInstance],
    width: i32,
    height: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<EndpointOption> {
    match endpoint {
        EndpointInput::Facility { instance, ports } => {
            let instance = instances
                .iter()
                .find(|model_instance| model_instance.input.id == *instance)
                .expect("prepared endpoint instance exists");
            endpoint_options(solver, edge_index, endpoint_kind, instance, ports, tag)
        }
        EndpointInput::Boundary { node } => {
            boundary_endpoint_options(solver, edge_index, endpoint_kind, node, width, height, tag)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn boundary_endpoint_options(
    solver: &mut Solver,
    edge_index: usize,
    endpoint_kind: &str,
    node: &str,
    width: i32,
    height: i32,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Vec<EndpointOption> {
    let mut positions = Vec::new();
    positions.extend((0..width).map(|x| (BoundarySide::North, x, 0)));
    positions.extend((0..height).map(|y| (BoundarySide::East, width - 1, y)));
    positions.extend((0..width).map(|x| (BoundarySide::South, x, height - 1)));
    positions.extend((0..height).map(|y| (BoundarySide::West, 0, y)));

    let options = positions
        .into_iter()
        .enumerate()
        .map(|(option_index, (side, x, y))| EndpointOption {
            endpoint: IntegratedRouteEndpoint::Boundary {
                node: node.to_string(),
                side,
            },
            cell: grid_index(x, y, width),
            selected: solver.new_named_bounded_integer(
                0,
                1,
                format!("edge-{edge_index}-{endpoint_kind}-boundary-{option_index}"),
            ),
        })
        .collect::<Vec<_>>();
    post_equals_one(solver, options.iter().map(|option| option.selected), tag);
    options
}

fn grid_arcs(
    solver: &mut Solver,
    edge_index: usize,
    width: i32,
    height: i32,
) -> (Vec<Arc>, Vec<Vec<DomainId>>, Vec<Vec<DomainId>>) {
    let cell_count = (width as usize) * (height as usize);
    let mut arcs = Vec::new();
    let mut incoming = vec![Vec::new(); cell_count];
    let mut outgoing = vec![Vec::new(); cell_count];
    for y in 0..height {
        for x in 0..width {
            let from = grid_index(x, y, width);
            for (to_x, to_y) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                if to_x < 0 || to_y < 0 || to_x >= width || to_y >= height {
                    continue;
                }
                let to = grid_index(to_x, to_y, width);
                let selected = solver.new_named_bounded_integer(
                    0,
                    1,
                    format!("route-{edge_index}-arc-{from}-{to}"),
                );
                arcs.push(Arc { from, to, selected });
                outgoing[from].push(selected);
                incoming[to].push(selected);
            }
        }
    }
    (arcs, incoming, outgoing)
}

fn post_equals_one(
    solver: &mut Solver,
    variables: impl Iterator<Item = DomainId>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    solver
        .add_constraint(pumpkin_solver::equals(
            variables
                .map(|variable| variable.scaled(1))
                .collect::<Vec<_>>(),
            1,
            tag,
        ))
        .post();
}

fn post_at_most_one(
    solver: &mut Solver,
    variables: impl Iterator<Item = DomainId>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    let terms = variables
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    if terms.len() > 1 {
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(terms, 1, tag))
            .post();
    }
}

fn extract_report(
    solution: &impl ProblemSolution,
    status: IntegratedLayoutStatus,
    input: &ModelInput,
    instances: &[ModelInstance],
    model_routes: &[ModelRoute],
) -> IntegratedLayoutReport {
    let mut placements = Vec::new();
    for instance in instances {
        let candidate = instance
            .candidates
            .iter()
            .find(|candidate| solution.get_integer_value(candidate.selected) == 1)
            .expect("exactly one placement candidate is selected");
        placements.push(FacilityPlacement {
            instance: instance.input.id.clone(),
            recipe: instance.input.recipe.clone(),
            facility: instance.input.facility.clone(),
            x: i64::from(candidate.x),
            y: i64::from(candidate.y),
            width: i64::from(candidate.width),
            height: i64::from(candidate.height),
            rotation: candidate.rotation,
        });
    }
    placements.sort_by(|left, right| left.instance.cmp(&right.instance));

    let mut used_width = placements
        .iter()
        .map(|placement| placement.x + placement.width)
        .max()
        .unwrap_or(0);
    let mut used_height = placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .max()
        .unwrap_or(0);
    let routes = input
        .edges
        .iter()
        .zip(model_routes)
        .map(|(edge, model_route)| {
            let source = selected_endpoint(solution, &model_route.source_options);
            let target = selected_endpoint(solution, &model_route.target_options);
            let cells = extract_path(
                solution,
                source.cell,
                target.cell,
                &model_route.arcs,
                input.width,
            );
            for cell in &cells {
                used_width = used_width.max(cell.x + 1);
                used_height = used_height.max(cell.y + 1);
            }
            IntegratedRoute {
                source: source.endpoint.clone(),
                target: target.endpoint.clone(),
                item: edge.edge.item.clone(),
                rate: edge.edge.rate,
                transport: edge.transport,
                cells,
            }
        })
        .collect();

    IntegratedLayoutReport {
        success: true,
        status,
        bounds: Some(FacilityPlacementBounds {
            width: used_width,
            height: used_height,
        }),
        placements,
        routes,
        diagnostics: vec![IntegratedLayoutDiagnostic::info(
            if status == IntegratedLayoutStatus::Optimal {
                "integrated-layout-optimal"
            } else {
                "integrated-layout-feasible"
            },
            if status == IntegratedLayoutStatus::Optimal {
                "facility placement, port selection, and routes are solved with proven minimum total route length"
            } else {
                "facility placement, port selection, and routing are feasible but not proven optimal"
            },
        )],
    }
}

fn selected_endpoint<'a>(
    solution: &impl ProblemSolution,
    options: &'a [EndpointOption],
) -> &'a EndpointOption {
    options
        .iter()
        .find(|option| solution.get_integer_value(option.selected) == 1)
        .expect("exactly one endpoint option is selected")
}

fn extract_path(
    solution: &impl ProblemSolution,
    source: usize,
    target: usize,
    arcs: &[Arc],
    width: i32,
) -> Vec<WorldGridPosition> {
    let mut next_by_cell = BTreeMap::new();
    for arc in arcs {
        if solution.get_integer_value(arc.selected) == 1 {
            next_by_cell.insert(arc.from, arc.to);
        }
    }
    let mut cells = vec![world_position(source, width)];
    let mut current = source;
    let mut seen = BTreeSet::from([source]);
    while current != target {
        current = *next_by_cell.get(&current).unwrap_or_else(|| {
            panic!(
                "solver route stops before target: source={source}, target={target}, current={current}, arcs={next_by_cell:?}"
            )
        });
        assert!(seen.insert(current), "solver route contains a cycle");
        cells.push(world_position(current, width));
    }
    cells
}

fn grid_index(x: i32, y: i32, width: i32) -> usize {
    (y as usize) * (width as usize) + (x as usize)
}

fn world_position(index: usize, width: i32) -> WorldGridPosition {
    WorldGridPosition {
        x: (index % width as usize) as i64,
        y: (index / width as usize) as i64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityCatalog, FacilityFootprint};
    use crate::logistics::{
        ItemCatalog, ItemDefinition, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
        SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION, TransportCapacity, TransportCatalog,
        TransportDefinition,
    };

    fn facility(
        id: &str,
        port_id: &str,
        direction: FacilityPortDirection,
        edge: FacilityPortEdge,
    ) -> FacilityDefinition {
        FacilityDefinition {
            id: id.to_string(),
            footprint: FacilityFootprint {
                width: 1,
                height: 1,
            },
            allowed_rotations: vec![0],
            ports: vec![FacilityPortDefinition {
                id: port_id.to_string(),
                direction,
                transport: TransportKind::Belt,
                position: FacilityPortPosition { x: 0, y: 0 },
                edge,
            }],
        }
    }

    fn facility_with_ports(
        id: &str,
        ports: &[(&str, FacilityPortDirection, FacilityPortEdge)],
    ) -> FacilityDefinition {
        FacilityDefinition {
            id: id.to_string(),
            footprint: FacilityFootprint {
                width: 1,
                height: 1,
            },
            allowed_rotations: vec![0],
            ports: ports
                .iter()
                .map(|(port_id, direction, edge)| FacilityPortDefinition {
                    id: (*port_id).to_string(),
                    direction: *direction,
                    transport: TransportKind::Belt,
                    position: FacilityPortPosition { x: 0, y: 0 },
                    edge: *edge,
                })
                .collect(),
        }
    }

    fn wiring() -> FacilityInstanceWiringReport {
        FacilityInstanceWiringReport {
            success: true,
            nodes: vec![
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:source#1".to_string(),
                    recipe: "source".to_string(),
                    facility: "source-machine".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:target#1".to_string(),
                    recipe: "target".to_string(),
                    facility: "target-machine".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
            ],
            edges: vec![FacilityInstanceWiringEdge {
                source: "recipe:source#1".to_string(),
                target: "recipe:target#1".to_string(),
                kind: "intermediate".to_string(),
                item: "part".to_string(),
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
            }],
            diagnostics: Vec::new(),
        }
    }

    fn transport_catalog() -> ValidatedTransportCatalog {
        ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
            schema_version: SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION,
            transports: vec![
                TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 2,
                        duration_ms: 1000,
                    },
                },
                TransportDefinition {
                    kind: TransportKind::Pipe,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 500,
                    },
                },
            ],
        })
        .expect("transport catalog should validate")
    }

    fn catalogs() -> (
        ValidatedFacilityCatalog,
        ValidatedItemCatalog,
        ValidatedTransportCatalog,
    ) {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility(
                    "source-machine",
                    "output",
                    FacilityPortDirection::Output,
                    FacilityPortEdge::East,
                ),
                facility(
                    "target-machine",
                    "input",
                    FacilityPortDirection::Input,
                    FacilityPortEdge::West,
                ),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![ItemDefinition {
                id: "part".to_string(),
                transport: TransportKind::Belt,
            }],
        })
        .expect("item catalog should validate");
        (facilities, items, transport_catalog())
    }

    #[test]
    fn jointly_places_selects_ports_and_routes_one_edge() {
        let (facilities, items, transports) = catalogs();
        let report = solve_integrated_layout(
            &wiring(),
            &facilities,
            &items,
            &transports,
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 1,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.status, IntegratedLayoutStatus::Optimal);
        assert_eq!(report.routes.len(), 1);
        assert_eq!(report.routes[0].transport, TransportKind::Belt);
        assert!(matches!(
            &report.routes[0].source,
            IntegratedRouteEndpoint::Facility { port, .. } if port == "output"
        ));
        assert!(matches!(
            &report.routes[0].target,
            IntegratedRouteEndpoint::Facility { port, .. } if port == "input"
        ));
        assert_eq!(report.routes[0].cells.len(), 2);
        assert_eq!(report.placements.len(), 2);
    }

    #[test]
    fn handles_grid_cells_without_endpoint_candidates() {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility(
                    "source-machine",
                    "output",
                    FacilityPortDirection::Output,
                    FacilityPortEdge::East,
                ),
                facility(
                    "target-machine",
                    "input",
                    FacilityPortDirection::Input,
                    FacilityPortEdge::East,
                ),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![ItemDefinition {
                id: "part".to_string(),
                transport: TransportKind::Belt,
            }],
        })
        .expect("item catalog should validate");

        let report = solve_integrated_layout(
            &wiring(),
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 2,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.routes.len(), 1);
    }

    #[test]
    fn jointly_routes_multiple_edges_without_shared_cells() {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility(
                    "source-machine",
                    "output",
                    FacilityPortDirection::Output,
                    FacilityPortEdge::East,
                ),
                facility_with_ports(
                    "middle-machine",
                    &[
                        (
                            "input",
                            FacilityPortDirection::Input,
                            FacilityPortEdge::West,
                        ),
                        (
                            "output",
                            FacilityPortDirection::Output,
                            FacilityPortEdge::East,
                        ),
                    ],
                ),
                facility(
                    "target-machine",
                    "input",
                    FacilityPortDirection::Input,
                    FacilityPortEdge::West,
                ),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "part-a".to_string(),
                    transport: TransportKind::Belt,
                },
                ItemDefinition {
                    id: "part-b".to_string(),
                    transport: TransportKind::Belt,
                },
            ],
        })
        .expect("item catalog should validate");
        let mut wiring = wiring();
        wiring.nodes.insert(
            1,
            FacilityInstanceWiringNode::Facility {
                id: "recipe:middle#1".to_string(),
                recipe: "middle".to_string(),
                facility: "middle-machine".to_string(),
                index: 1,
                runs_per_second: Rate {
                    numerator: 1,
                    denominator: 1,
                },
                work_seconds_per_second: Rate {
                    numerator: 1,
                    denominator: 1,
                },
                unused_capacity: Rate::zero(),
            },
        );
        wiring.edges = vec![
            FacilityInstanceWiringEdge {
                source: "recipe:source#1".to_string(),
                target: "recipe:middle#1".to_string(),
                kind: "intermediate".to_string(),
                item: "part-a".to_string(),
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
            },
            FacilityInstanceWiringEdge {
                source: "recipe:middle#1".to_string(),
                target: "recipe:target#1".to_string(),
                kind: "intermediate".to_string(),
                item: "part-b".to_string(),
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
            },
        ];

        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 7,
                max_height: 1,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.status, IntegratedLayoutStatus::Optimal);
        assert_eq!(report.placements.len(), 3);
        assert_eq!(report.routes.len(), 2);
        assert_eq!(
            report
                .routes
                .iter()
                .map(|route| route.cells.len())
                .sum::<usize>(),
            4
        );
        let route_cells = report
            .routes
            .iter()
            .flat_map(|route| route.cells.iter().map(|cell| (cell.x, cell.y)))
            .collect::<BTreeSet<_>>();
        assert_eq!(route_cells.len(), 4);
    }

    #[test]
    fn jointly_routes_a_two_facility_cycle() {
        let cycle_ports = [
            (
                "input",
                FacilityPortDirection::Input,
                FacilityPortEdge::West,
            ),
            (
                "output",
                FacilityPortDirection::Output,
                FacilityPortEdge::East,
            ),
        ];
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                facility_with_ports("planter", &cycle_ports),
                facility_with_ports("seed-collector", &cycle_ports),
            ],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "crop".to_string(),
                    transport: TransportKind::Belt,
                },
                ItemDefinition {
                    id: "seed".to_string(),
                    transport: TransportKind::Belt,
                },
            ],
        })
        .expect("item catalog should validate");
        let wiring = FacilityInstanceWiringReport {
            success: true,
            nodes: vec![
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:grow#1".to_string(),
                    recipe: "grow".to_string(),
                    facility: "planter".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:collect#1".to_string(),
                    recipe: "collect".to_string(),
                    facility: "seed-collector".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
            ],
            edges: vec![
                FacilityInstanceWiringEdge {
                    source: "recipe:grow#1".to_string(),
                    target: "recipe:collect#1".to_string(),
                    kind: "intermediate".to_string(),
                    item: "crop".to_string(),
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                },
                FacilityInstanceWiringEdge {
                    source: "recipe:collect#1".to_string(),
                    target: "recipe:grow#1".to_string(),
                    kind: "intermediate".to_string(),
                    item: "seed".to_string(),
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                },
            ],
            diagnostics: Vec::new(),
        };

        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 6,
                max_height: 2,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.routes.len(), 2);
        let route_cells = report
            .routes
            .iter()
            .flat_map(|route| route.cells.iter().map(|cell| (cell.x, cell.y)))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            route_cells.len(),
            report
                .routes
                .iter()
                .map(|route| route.cells.len())
                .sum::<usize>()
        );
    }

    #[test]
    fn routes_external_input_and_target_to_boundary_terminals() {
        let facilities = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![facility_with_ports(
                "processor",
                &[
                    (
                        "input",
                        FacilityPortDirection::Input,
                        FacilityPortEdge::West,
                    ),
                    (
                        "output",
                        FacilityPortDirection::Output,
                        FacilityPortEdge::East,
                    ),
                ],
            )],
        })
        .expect("facility catalog should validate");
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![
                ItemDefinition {
                    id: "ore".to_string(),
                    transport: TransportKind::Belt,
                },
                ItemDefinition {
                    id: "product".to_string(),
                    transport: TransportKind::Belt,
                },
            ],
        })
        .expect("item catalog should validate");
        let mut wiring = FacilityInstanceWiringReport {
            success: true,
            nodes: vec![
                FacilityInstanceWiringNode::External {
                    id: "external:ore".to_string(),
                    item: "ore".to_string(),
                },
                FacilityInstanceWiringNode::Facility {
                    id: "recipe:process#1".to_string(),
                    recipe: "process".to_string(),
                    facility: "processor".to_string(),
                    index: 1,
                    runs_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    work_seconds_per_second: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                    unused_capacity: Rate::zero(),
                },
                FacilityInstanceWiringNode::Target {
                    id: "target:product".to_string(),
                    item: "product".to_string(),
                },
            ],
            edges: vec![
                FacilityInstanceWiringEdge {
                    source: "external:ore".to_string(),
                    target: "recipe:process#1".to_string(),
                    kind: "external-input".to_string(),
                    item: "ore".to_string(),
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                },
                FacilityInstanceWiringEdge {
                    source: "recipe:process#1".to_string(),
                    target: "target:product".to_string(),
                    kind: "target".to_string(),
                    item: "product".to_string(),
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                },
            ],
            diagnostics: Vec::new(),
        };

        let report = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 5,
                max_height: 1,
            },
        );

        assert!(report.success, "{:#?}", report.diagnostics);
        assert_eq!(report.routes.len(), 2);
        assert!(matches!(
            &report.routes[0].source,
            IntegratedRouteEndpoint::Boundary { node, .. } if node == "external:ore"
        ));
        assert!(matches!(
            &report.routes[0].target,
            IntegratedRouteEndpoint::Facility { instance, port }
                if instance == "recipe:process#1" && port == "input"
        ));
        assert!(matches!(
            &report.routes[1].source,
            IntegratedRouteEndpoint::Facility { instance, port }
                if instance == "recipe:process#1" && port == "output"
        ));
        assert!(matches!(
            &report.routes[1].target,
            IntegratedRouteEndpoint::Boundary { node, .. } if node == "target:product"
        ));

        let FacilityInstanceWiringNode::External { item, .. } = &mut wiring.nodes[0] else {
            unreachable!("the first test node is external")
        };
        *item = "product".to_string();
        let mismatch = solve_integrated_layout(
            &wiring,
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 5,
                max_height: 1,
            },
        );
        assert!(!mismatch.success);
        assert_eq!(mismatch.diagnostics[0].code, "boundary-item-mismatch");
    }

    #[test]
    fn rejects_item_port_transport_mismatch() {
        let (facilities, _, _) = catalogs();
        let items = ValidatedItemCatalog::try_from_catalog(ItemCatalog {
            schema_version: SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION,
            items: vec![ItemDefinition {
                id: "part".to_string(),
                transport: TransportKind::Pipe,
            }],
        })
        .expect("item catalog should validate");

        let report = solve_integrated_layout(
            &wiring(),
            &facilities,
            &items,
            &transport_catalog(),
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 1,
            },
        );

        assert!(!report.success);
        assert_eq!(report.diagnostics[0].code, "missing-compatible-port");
    }

    #[test]
    fn rejects_route_rate_above_transport_capacity() {
        let (facilities, items, _) = catalogs();
        let transports = ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
            schema_version: SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION,
            transports: vec![
                TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 2000,
                    },
                },
                TransportDefinition {
                    kind: TransportKind::Pipe,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 500,
                    },
                },
            ],
        })
        .expect("transport catalog should validate");

        let report = solve_integrated_layout(
            &wiring(),
            &facilities,
            &items,
            &transports,
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 1,
            },
        );

        assert!(!report.success);
        assert_eq!(report.diagnostics[0].code, "route-capacity-exceeded");
    }
}
