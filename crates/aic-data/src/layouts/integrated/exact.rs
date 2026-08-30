use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::sync::Arc as Shared;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pumpkin_solver::Solver;
use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::DefaultBrancher;
use pumpkin_solver::core::optimisation::OptimisationDirection;
use pumpkin_solver::core::optimisation::linear_sat_unsat::LinearSatUnsat;
use pumpkin_solver::core::results::{OptimisationResult, SolutionReference};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::{
    EndpointInput, ExactModelMetrics, FacilityPortEdge, InstanceInput, IntegratedLayoutDiagnostic,
    IntegratedLayoutReport, IntegratedLayoutStatus, IntegratedRouteEndpoint, ModelInput,
    TransportKind, ValidatedLogisticsComponentCatalog, witness,
};

mod extract;
mod formulation;
mod metrics;

use extract::extract_report;
use formulation::{
    external_endpoint_options, generate_candidates, grid_arcs, model_facility_endpoint_options,
    post_acyclic_route_ordering, post_at_most_one, post_equals_one,
};
use metrics::{elapsed_millis, finish_report};

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
    external_side: Option<FacilityPortEdge>,
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

pub(super) fn solve(
    input: ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    time_limit: Option<Duration>,
) -> IntegratedLayoutReport {
    let construction_started = Instant::now();
    let mut model_metrics = ExactModelMetrics {
        facility_count: input.instances.len(),
        route_requirement_count: input.edges.len(),
        grid_cell_count: input.cell_count as usize,
        ..ExactModelMetrics::default()
    };
    let mut solver = Solver::default();
    let tag = solver.new_constraint_tag();
    let cell_count = input.cell_count as usize;
    let mut occupancy = vec![Vec::<DomainId>::new(); cell_count];
    let mut model_instances = Vec::with_capacity(input.instances.len());

    for instance in &input.instances {
        let candidates = generate_candidates(&mut solver, &instance, input.width, input.height);
        model_metrics.placement_variables += candidates.len();
        if candidates.is_empty() {
            return IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Infeasible,
                IntegratedLayoutDiagnostic::error(
                    "facility-has-no-placement-candidate",
                    "/",
                    Some(instance.id.clone()),
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
            input: instance.clone(),
            candidates,
        });
    }

    for cell_candidates in &occupancy {
        post_at_most_one(&mut solver, cell_candidates.iter().copied(), tag);
    }

    let mut model_routes = Vec::with_capacity(input.edges.len());
    let mut belt_route_cells_by_grid = vec![Vec::<DomainId>::new(); cell_count];
    let mut pipe_route_cells_by_grid = vec![Vec::<DomainId>::new(); cell_count];
    let mut route_arc_variables = Vec::new();
    for (edge_index, edge) in input.edges.iter().enumerate() {
        let (source_options, target_options) = match (&edge.source, &edge.target) {
            (EndpointInput::Facility { .. }, EndpointInput::Facility { .. }) => (
                model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "source",
                    &edge.source,
                    &model_instances,
                    tag,
                ),
                model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "target",
                    &edge.target,
                    &model_instances,
                    tag,
                ),
            ),
            (EndpointInput::External { node }, EndpointInput::Facility { .. }) => {
                let target = model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "target",
                    &edge.target,
                    &model_instances,
                    tag,
                );
                (external_endpoint_options(node, &target), target)
            }
            (EndpointInput::Facility { .. }, EndpointInput::External { node }) => {
                let source = model_facility_endpoint_options(
                    &mut solver,
                    edge_index,
                    "source",
                    &edge.source,
                    &model_instances,
                    tag,
                );
                let target = external_endpoint_options(node, &source);
                (source, target)
            }
            (EndpointInput::External { .. }, EndpointInput::External { .. }) => unreachable!(
                "external-to-external requirements are rejected during model preparation"
            ),
        };
        model_metrics.endpoint_variables += match (&edge.source, &edge.target) {
            (EndpointInput::Facility { .. }, EndpointInput::Facility { .. }) => {
                source_options.len() + target_options.len()
            }
            (EndpointInput::External { .. }, EndpointInput::Facility { .. }) => {
                target_options.len()
            }
            (EndpointInput::Facility { .. }, EndpointInput::External { .. }) => {
                source_options.len()
            }
            (EndpointInput::External { .. }, EndpointInput::External { .. }) => 0,
        };

        let (arcs, incoming, outgoing) =
            grid_arcs(&mut solver, edge_index, input.width, input.height);
        model_metrics.route_arc_variables += arcs.len();
        model_metrics.route_cell_variables += cell_count;
        model_metrics.route_order_variables += cell_count;
        model_metrics.acyclicity_constraints += arcs.len();
        post_acyclic_route_ordering(&mut solver, edge_index, &arcs, input.cell_count, tag);
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
            match edge.transport {
                TransportKind::Belt => belt_route_cells_by_grid[cell].push(route_cell),
                TransportKind::Pipe => pipe_route_cells_by_grid[cell].push(route_cell),
            }

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
        for layer in [
            &belt_route_cells_by_grid[cell],
            &pipe_route_cells_by_grid[cell],
        ] {
            let exclusion = occupancy[cell]
                .iter()
                .chain(layer.iter())
                .map(|variable| variable.scaled(1))
                .collect::<Vec<_>>();
            if exclusion.len() > 1 {
                solver
                    .add_constraint(pumpkin_solver::less_than_or_equals(exclusion, 1, tag))
                    .post();
            }
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

    let construction_ms = elapsed_millis(construction_started.elapsed());
    let mut brancher = solver.default_brancher();
    let mut resolver = ResolutionResolver::default();
    let incumbent_count = Shared::new(AtomicUsize::new(0));
    let callback_incumbent_count = Shared::clone(&incumbent_count);
    let callback = move |_: &Solver,
                         _: SolutionReference,
                         _: &DefaultBrancher,
                         _: &ResolutionResolver|
          -> ControlFlow<()> {
        callback_incumbent_count.fetch_add(1, Ordering::Relaxed);
        ControlFlow::Continue(())
    };
    let mut termination = time_limit.map(TimeBudget::starting_now);
    let search_started = Instant::now();
    let result = solver.optimise(
        &mut brancher,
        &mut termination,
        &mut resolver,
        LinearSatUnsat::new(OptimisationDirection::Minimise, route_length, callback),
    );
    let search_ms = elapsed_millis(search_started.elapsed());

    let mut report = match result {
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
    };
    let validation = if report.success {
        match witness::validate(&input, logistics_components, &report) {
            Ok(()) => super::ExactValidationStatus::Passed,
            Err(diagnostic) => {
                report.success = false;
                report.status = IntegratedLayoutStatus::Unknown;
                report.diagnostics.push(diagnostic);
                super::ExactValidationStatus::Failed
            }
        }
    } else {
        super::ExactValidationStatus::NotAttempted
    };
    let observed_incumbents = incumbent_count.load(Ordering::Relaxed);
    finish_report(
        report,
        model_metrics,
        construction_ms,
        search_ms,
        observed_incumbents,
        validation,
    )
}
