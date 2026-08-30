use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;

use pumpkin_solver::Solver;
use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::DefaultBrancher;
use pumpkin_solver::core::optimisation::OptimisationDirection;
use pumpkin_solver::core::optimisation::linear_sat_unsat::LinearSatUnsat;
use pumpkin_solver::core::results::{OptimisationResult, ProblemSolution, SolutionReference};
use pumpkin_solver::core::termination::Indefinite;
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
use crate::logistics::{TransportKind, ValidatedItemCatalog};
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
pub struct IntegratedRouteEndpoint {
    pub instance: String,
    pub port: String,
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
    fn error(
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
    request: &FacilityPlacementRequest,
) -> IntegratedLayoutReport {
    match prepare_model(instance_wiring, facilities, items, request) {
        Ok(input) => solve(input),
        Err(diagnostic) => {
            IntegratedLayoutReport::failure(IntegratedLayoutStatus::InvalidInput, diagnostic)
        }
    }
}

struct ModelInput {
    width: i32,
    height: i32,
    instances: Vec<InstanceInput>,
    edge: FacilityInstanceWiringEdge,
    source_instance: String,
    target_instance: String,
    source_ports: Vec<FacilityPortDefinition>,
    target_ports: Vec<FacilityPortDefinition>,
    transport: TransportKind,
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
    if instance_wiring.edges.len() != 1 {
        return Err(IntegratedLayoutDiagnostic::error(
            "unsupported-routing-edge-count",
            "/edges",
            None,
            format!(
                "the first integrated routing slice requires exactly one logical edge, found {}",
                instance_wiring.edges.len()
            ),
        ));
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

    let edge = instance_wiring.edges[0].clone();
    let source = instances
        .iter()
        .find(|instance| instance.id == edge.source)
        .ok_or_else(|| unsupported_external_endpoint("source", &edge.source))?;
    let target = instances
        .iter()
        .find(|instance| instance.id == edge.target)
        .ok_or_else(|| unsupported_external_endpoint("target", &edge.target))?;
    if source.id == target.id {
        return Err(IntegratedLayoutDiagnostic::error(
            "unsupported-self-route",
            "/edges/0",
            Some(source.id.clone()),
            "the first integrated routing slice does not support a self-route",
        ));
    }

    let item = items.item(&edge.item).ok_or_else(|| {
        IntegratedLayoutDiagnostic::error(
            "missing-item-definition",
            "/edges/0/item",
            Some(edge.item.clone()),
            format!(
                "item '{}' is absent from the validated item catalog",
                edge.item
            ),
        )
    })?;
    let source_ports = compatible_ports(
        &source.definition,
        FacilityPortDirection::Output,
        item.transport,
    );
    let target_ports = compatible_ports(
        &target.definition,
        FacilityPortDirection::Input,
        item.transport,
    );
    if source_ports.is_empty() {
        return Err(missing_compatible_port(
            &source.id,
            "output",
            item.transport,
        ));
    }
    if target_ports.is_empty() {
        return Err(missing_compatible_port(&target.id, "input", item.transport));
    }

    let width = i32::try_from(request.max_width).map_err(|_| solver_domain_error("max_width"))?;
    let height =
        i32::try_from(request.max_height).map_err(|_| solver_domain_error("max_height"))?;

    let source_instance = source.id.clone();
    let target_instance = target.id.clone();

    Ok(ModelInput {
        width,
        height,
        instances,
        edge,
        source_instance,
        target_instance,
        source_ports,
        target_ports,
        transport: item.transport,
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

fn unsupported_external_endpoint(kind: &str, endpoint: &str) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "unsupported-external-route-endpoint",
        format!("/edges/0/{kind}"),
        Some(endpoint.to_string()),
        format!("the first integrated routing slice requires a facility {kind} endpoint"),
    )
}

fn missing_compatible_port(
    instance: &str,
    direction: &str,
    transport: TransportKind,
) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "missing-compatible-port",
        "/edges/0",
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
    port: String,
    cell: usize,
    selected: DomainId,
}

#[derive(Clone, Copy)]
struct Arc {
    from: usize,
    to: usize,
    selected: DomainId,
}

fn solve(mut input: ModelInput) -> IntegratedLayoutReport {
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

    let source_index = model_instances
        .iter()
        .position(|instance| instance.input.id == input.source_instance)
        .expect("prepared source instance exists");
    let target_index = model_instances
        .iter()
        .position(|instance| instance.input.id == input.target_instance)
        .expect("prepared target instance exists");
    let source_options = endpoint_options(
        &mut solver,
        &model_instances[source_index],
        &input.source_ports,
        tag,
    );
    let target_options = endpoint_options(
        &mut solver,
        &model_instances[target_index],
        &input.target_ports,
        tag,
    );

    let (arcs, incoming, outgoing) = grid_arcs(&mut solver, input.width, input.height);
    let mut source_by_cell = vec![Vec::<DomainId>::new(); cell_count];
    let mut target_by_cell = vec![Vec::<DomainId>::new(); cell_count];
    for option in &source_options {
        source_by_cell[option.cell].push(option.selected);
    }
    for option in &target_options {
        target_by_cell[option.cell].push(option.selected);
    }

    let mut route_cells = Vec::with_capacity(cell_count);
    for cell in 0..cell_count {
        let route_cell = solver.new_named_bounded_integer(0, 1, format!("route-cell-{cell}"));
        route_cells.push(route_cell);

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

        let mut distinct_endpoints = Vec::new();
        distinct_endpoints.extend(
            source_by_cell[cell]
                .iter()
                .map(|variable| variable.scaled(1)),
        );
        distinct_endpoints.extend(
            target_by_cell[cell]
                .iter()
                .map(|variable| variable.scaled(1)),
        );
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(
                distinct_endpoints,
                1,
                tag,
            ))
            .post();

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

        let mut exclusion = occupancy[cell]
            .iter()
            .map(|variable| variable.scaled(1))
            .collect::<Vec<_>>();
        exclusion.push(route_cell.scaled(1));
        solver
            .add_constraint(pumpkin_solver::less_than_or_equals(exclusion, 1, tag))
            .post();
    }

    let route_length = solver.new_named_bounded_integer(0, arcs.len() as i32, "route-length");
    let mut route_length_definition = arcs
        .iter()
        .map(|arc| arc.selected.scaled(1))
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
    let result = solver.optimise(
        &mut brancher,
        &mut Indefinite,
        &mut resolver,
        LinearSatUnsat::new(OptimisationDirection::Minimise, route_length, callback),
    );

    match result {
        OptimisationResult::Optimal(solution) => extract_report(
            &solution,
            IntegratedLayoutStatus::Optimal,
            &input,
            &model_instances,
            &source_options,
            &target_options,
            &arcs,
        ),
        OptimisationResult::Satisfiable(solution) | OptimisationResult::Stopped(solution, ()) => {
            extract_report(
                &solution,
                IntegratedLayoutStatus::Feasible,
                &input,
                &model_instances,
                &source_options,
                &target_options,
                &arcs,
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
                    "endpoint-{}-{}-{candidate_index}",
                    instance.input.id, port.id
                ),
            );
            candidate_options.push(selected);
            options.push(EndpointOption {
                port: port.id.clone(),
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

fn grid_arcs(
    solver: &mut Solver,
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
                let selected =
                    solver.new_named_bounded_integer(0, 1, format!("route-arc-{from}-{to}"));
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
    source_options: &[EndpointOption],
    target_options: &[EndpointOption],
    arcs: &[Arc],
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

    let source = selected_endpoint(solution, source_options);
    let target = selected_endpoint(solution, target_options);
    let cells = extract_path(solution, source.cell, target.cell, arcs, input.width);
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
    for cell in &cells {
        used_width = used_width.max(cell.x + 1);
        used_height = used_height.max(cell.y + 1);
    }

    IntegratedLayoutReport {
        success: true,
        status,
        bounds: Some(FacilityPlacementBounds {
            width: used_width,
            height: used_height,
        }),
        placements,
        routes: vec![IntegratedRoute {
            source: IntegratedRouteEndpoint {
                instance: input.source_instance.clone(),
                port: source.port.clone(),
            },
            target: IntegratedRouteEndpoint {
                instance: input.target_instance.clone(),
                port: target.port.clone(),
            },
            item: input.edge.item.clone(),
            rate: input.edge.rate,
            transport: input.transport,
            cells,
        }],
        diagnostics: vec![IntegratedLayoutDiagnostic::info(
            if status == IntegratedLayoutStatus::Optimal {
                "integrated-layout-optimal"
            } else {
                "integrated-layout-feasible"
            },
            if status == IntegratedLayoutStatus::Optimal {
                "facility placement, port selection, and route length are solved with proven minimum route length"
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
    use crate::logistics::{ItemCatalog, ItemDefinition, SUPPORTED_ITEM_CATALOG_SCHEMA_VERSION};

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

    fn catalogs() -> (ValidatedFacilityCatalog, ValidatedItemCatalog) {
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
        (facilities, items)
    }

    #[test]
    fn jointly_places_selects_ports_and_routes_one_edge() {
        let (facilities, items) = catalogs();
        let report = solve_integrated_layout(
            &wiring(),
            &facilities,
            &items,
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
        assert_eq!(report.routes[0].source.port, "output");
        assert_eq!(report.routes[0].target.port, "input");
        assert_eq!(report.routes[0].cells.len(), 2);
        assert_eq!(report.placements.len(), 2);
    }

    #[test]
    fn rejects_item_port_transport_mismatch() {
        let (facilities, _) = catalogs();
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
            &FacilityPlacementRequest {
                schema_version: 2,
                max_width: 4,
                max_height: 1,
            },
        );

        assert!(!report.success);
        assert_eq!(report.diagnostics[0].code, "missing-compatible-port");
    }
}
