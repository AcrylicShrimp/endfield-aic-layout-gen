use std::ops::ControlFlow;
use std::sync::Arc as Shared;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use pumpkin_solver::Solver;
use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::DefaultBrancher;
use pumpkin_solver::core::optimisation::OptimisationDirection;
use pumpkin_solver::core::optimisation::linear_sat_unsat::LinearSatUnsat;
use pumpkin_solver::core::proof::ConstraintTag;
use pumpkin_solver::core::results::{
    OptimisationResult, ProblemSolution, Solution, SolutionReference,
};
use pumpkin_solver::core::termination::TimeBudget;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::{Arc, ModelBranchComponent, ModelBridge, ModelNetwork, post_presence};
use crate::layouts::integrated::{
    ExactModelMetrics, ExactObjectiveKind, ExactObjectiveStageReport, ExactProofStatus,
    IntegratedLayoutDiagnostic, IntegratedLayoutReport, IntegratedLayoutStatus, ModelInput,
    TransportKind,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct ExactObjectives {
    pub(super) used_bounding_box_area: DomainId,
    pub(super) physical_transport_tiles: DomainId,
    pub(super) total_route_turns: DomainId,
    pub(super) maximum_used_side: DomainId,
    pub(super) logistics_component_count: DomainId,
}

pub(super) struct ObjectiveSearchResult {
    pub(super) report: IntegratedLayoutReport,
    pub(super) stages: Vec<ExactObjectiveStageReport>,
    pub(super) search_ms: u64,
    pub(super) incumbent_count: usize,
}

pub(super) fn optimise_lexicographically(
    solver: &mut Solver,
    objectives: ExactObjectives,
    time_limit: Option<Duration>,
    tag: ConstraintTag,
    mut extract: impl FnMut(&Solution, IntegratedLayoutStatus) -> IntegratedLayoutReport,
) -> ObjectiveSearchResult {
    let stages = [
        (
            ExactObjectiveKind::UsedBoundingBoxArea,
            objectives.used_bounding_box_area,
        ),
        (
            ExactObjectiveKind::PhysicalTransportTiles,
            objectives.physical_transport_tiles,
        ),
        (
            ExactObjectiveKind::TotalRouteTurns,
            objectives.total_route_turns,
        ),
        (
            ExactObjectiveKind::MaximumUsedSide,
            objectives.maximum_used_side,
        ),
        (
            ExactObjectiveKind::LogisticsComponentCount,
            objectives.logistics_component_count,
        ),
    ];
    let incumbent_count = Shared::new(AtomicUsize::new(0));
    let mut termination = time_limit.map(TimeBudget::starting_now);
    let search_started = Instant::now();
    let mut stage_reports = Vec::with_capacity(stages.len());
    let mut last_report = None;
    let mut terminal_failure = None;

    for (stage_index, (objective_kind, objective_variable)) in stages.iter().copied().enumerate() {
        let mut brancher = solver.default_brancher();
        let mut resolver = ResolutionResolver::default();
        let callback_incumbent_count = Shared::clone(&incumbent_count);
        let callback = move |_: &Solver,
                             _: SolutionReference,
                             _: &DefaultBrancher,
                             _: &ResolutionResolver|
              -> ControlFlow<()> {
            callback_incumbent_count.fetch_add(1, Ordering::Relaxed);
            ControlFlow::Continue(())
        };
        let stage_started = Instant::now();
        let result = solver.optimise(
            &mut brancher,
            &mut termination,
            &mut resolver,
            LinearSatUnsat::new(
                OptimisationDirection::Minimise,
                objective_variable,
                callback,
            ),
        );
        let stage_ms = super::metrics::elapsed_millis(stage_started.elapsed());
        match result {
            OptimisationResult::Optimal(solution) => {
                let value = solution.get_integer_value(objective_variable);
                stage_reports.push(ExactObjectiveStageReport {
                    objective: objective_kind,
                    incumbent: Some(i64::from(value)),
                    best_bound: Some(i64::from(value)),
                    search_ms: stage_ms,
                    proof: ExactProofStatus::ProvenOptimal,
                });
                last_report = Some(extract(
                    &solution,
                    if stage_index + 1 == stages.len() {
                        IntegratedLayoutStatus::Optimal
                    } else {
                        IntegratedLayoutStatus::Feasible
                    },
                ));
                drop(solution);
                if stage_index + 1 < stages.len() {
                    solver
                        .add_constraint(pumpkin_solver::equals(
                            [objective_variable.scaled(1)],
                            value,
                            tag,
                        ))
                        .post();
                }
            }
            OptimisationResult::Satisfiable(solution)
            | OptimisationResult::Stopped(solution, ()) => {
                let value = solution.get_integer_value(objective_variable);
                stage_reports.push(ExactObjectiveStageReport {
                    objective: objective_kind,
                    incumbent: Some(i64::from(value)),
                    best_bound: None,
                    search_ms: stage_ms,
                    proof: ExactProofStatus::Unproven,
                });
                last_report = Some(extract(&solution, IntegratedLayoutStatus::Feasible));
                break;
            }
            OptimisationResult::Unsatisfiable if stage_index == 0 => {
                terminal_failure = Some(IntegratedLayoutReport::failure(
                    IntegratedLayoutStatus::Infeasible,
                    IntegratedLayoutDiagnostic::error(
                        "integrated-layout-infeasible",
                        "/",
                        None,
                        "facility placement, port selection, and route constraints are infeasible",
                    ),
                ));
                break;
            }
            OptimisationResult::Unsatisfiable => {
                terminal_failure = Some(IntegratedLayoutReport::failure(
                    IntegratedLayoutStatus::Unknown,
                    IntegratedLayoutDiagnostic::error(
                        "exact-objective-stage-inconsistent",
                        "/",
                        None,
                        format!(
                            "objective stage {objective_kind:?} became infeasible after fixing a proven earlier optimum"
                        ),
                    ),
                ));
                break;
            }
            OptimisationResult::Unknown if last_report.is_some() => break,
            OptimisationResult::Unknown => {
                terminal_failure = Some(IntegratedLayoutReport::failure(
                    IntegratedLayoutStatus::Unknown,
                    IntegratedLayoutDiagnostic::error(
                        "integrated-layout-unknown",
                        "/",
                        None,
                        "solver terminated without a solution or proof",
                    ),
                ));
                break;
            }
        }
    }

    ObjectiveSearchResult {
        report: terminal_failure.or(last_report).unwrap_or_else(|| {
            IntegratedLayoutReport::failure(
                IntegratedLayoutStatus::Unknown,
                IntegratedLayoutDiagnostic::error(
                    "integrated-layout-unknown",
                    "/",
                    None,
                    "solver stopped without a reportable result",
                ),
            )
        }),
        stages: stage_reports,
        search_ms: super::metrics::elapsed_millis(search_started.elapsed()),
        incumbent_count: incumbent_count.load(Ordering::Relaxed),
    }
}

pub(super) fn build_objectives(
    solver: &mut Solver,
    input: &ModelInput,
    facility_occupancy: &[Vec<DomainId>],
    networks: &[ModelNetwork],
    branch_components: &[ModelBranchComponent],
    bridges: &[ModelBridge],
    metrics: &mut ExactModelMetrics,
    tag: ConstraintTag,
) -> Result<ExactObjectives, IntegratedLayoutDiagnostic> {
    let cell_count = input.cell_count as usize;
    let used_cells = (0..cell_count)
        .map(|cell| {
            let variables = facility_occupancy[cell]
                .iter()
                .copied()
                .chain(networks.iter().map(|network| network.route_cells[cell]));
            post_presence(solver, format!("used-geometry-cell-{cell}"), variables, tag)
        })
        .collect::<Vec<_>>();
    metrics.objective_variables += used_cells.len() + 2;

    require_canonical_origin(solver, input, &used_cells, tag);
    let used_width = solver.new_named_bounded_integer(1, input.width, "used-bounding-box-width");
    let used_height = solver.new_named_bounded_integer(1, input.height, "used-bounding-box-height");
    solver
        .add_constraint(pumpkin_solver::maximum(
            used_cells.iter().enumerate().map(|(cell, used)| {
                let x = i32::try_from(cell).expect("grid index fits i32") % input.width;
                used.scaled(x + 1)
            }),
            used_width,
            tag,
        ))
        .post();
    solver
        .add_constraint(pumpkin_solver::maximum(
            used_cells.iter().enumerate().map(|(cell, used)| {
                let y = i32::try_from(cell).expect("grid index fits i32") / input.width;
                used.scaled(y + 1)
            }),
            used_height,
            tag,
        ))
        .post();
    let used_bounding_box_area =
        solver.new_named_bounded_integer(1, input.cell_count, "used-bounding-box-area");
    solver
        .add_constraint(pumpkin_solver::times(
            used_width,
            used_height,
            used_bounding_box_area,
            tag,
        ))
        .post();
    let maximum_used_side =
        solver.new_named_bounded_integer(1, input.width.max(input.height), "maximum-used-side");
    solver
        .add_constraint(pumpkin_solver::maximum(
            [used_width, used_height],
            maximum_used_side,
            tag,
        ))
        .post();
    metrics.objective_variables += 4;

    let mut physical_tile_variables = Vec::new();
    for transport in [TransportKind::Belt, TransportKind::Pipe] {
        let transport_networks = networks
            .iter()
            .filter(|network| input.networks[network.input_index].transport() == transport)
            .collect::<Vec<_>>();
        if transport_networks.is_empty() {
            continue;
        }
        for cell in 0..cell_count {
            physical_tile_variables.push(post_presence(
                solver,
                format!("{:?}-physical-tile-{cell}", transport).to_lowercase(),
                transport_networks
                    .iter()
                    .map(|network| network.route_cells[cell]),
                tag,
            ));
        }
    }
    metrics.objective_variables += physical_tile_variables.len();
    let physical_transport_tiles = post_sum_variable(
        solver,
        "physical-transport-tiles",
        &physical_tile_variables,
        tag,
    )?;
    metrics.objective_variables += 1;

    let mut turn_variables = Vec::new();
    for (network_index, network) in networks.iter().enumerate() {
        for cell in 0..cell_count {
            turn_variables.push(post_network_turn(
                solver,
                network_index,
                cell,
                input.width,
                &network.arcs,
                tag,
                metrics,
            ));
        }
    }
    let total_route_turns = post_sum_variable(solver, "total-route-turns", &turn_variables, tag)?;
    metrics.objective_variables += 1;

    let logistics_component_variables = branch_components
        .iter()
        .map(|component| component.selected)
        .chain(bridges.iter().map(|bridge| bridge.selected))
        .collect::<Vec<_>>();
    let logistics_component_count = post_sum_variable(
        solver,
        "logistics-component-count",
        &logistics_component_variables,
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

fn require_canonical_origin(
    solver: &mut Solver,
    input: &ModelInput,
    used_cells: &[DomainId],
    tag: ConstraintTag,
) {
    let left_edge = post_presence(
        solver,
        "used-geometry-touches-left-edge".to_string(),
        used_cells
            .iter()
            .enumerate()
            .filter(|(cell, _)| {
                i32::try_from(*cell).expect("grid index fits i32") % input.width == 0
            })
            .map(|(_, used)| *used),
        tag,
    );
    let top_edge = post_presence(
        solver,
        "used-geometry-touches-top-edge".to_string(),
        used_cells
            .iter()
            .enumerate()
            .filter(|(cell, _)| {
                i32::try_from(*cell).expect("grid index fits i32") / input.width == 0
            })
            .map(|(_, used)| *used),
        tag,
    );
    solver
        .add_constraint(pumpkin_solver::equals([left_edge.scaled(1)], 1, tag))
        .post();
    solver
        .add_constraint(pumpkin_solver::equals([top_edge.scaled(1)], 1, tag))
        .post();
}

fn post_sum_variable(
    solver: &mut Solver,
    name: &str,
    variables: &[DomainId],
    tag: ConstraintTag,
) -> Result<DomainId, IntegratedLayoutDiagnostic> {
    let upper_bound = i32::try_from(variables.len()).map_err(|_| {
        IntegratedLayoutDiagnostic::error(
            "exact-objective-domain-overflow",
            "/",
            None,
            format!("objective {name} has more terms than the solver integer domain supports"),
        )
    })?;
    let total = solver.new_named_bounded_integer(0, upper_bound, name);
    let mut definition = variables
        .iter()
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    definition.push(total.scaled(-1));
    solver
        .add_constraint(pumpkin_solver::equals(definition, 0, tag))
        .post();
    Ok(total)
}

fn post_network_turn(
    solver: &mut Solver,
    network_index: usize,
    cell: usize,
    width: i32,
    arcs: &[Arc],
    tag: ConstraintTag,
    metrics: &mut ExactModelMetrics,
) -> DomainId {
    let incoming = arcs.iter().filter(|arc| arc.to == cell).collect::<Vec<_>>();
    let outgoing = arcs
        .iter()
        .filter(|arc| arc.from == cell)
        .collect::<Vec<_>>();
    let incoming_count = post_count(
        solver,
        format!("network-{network_index}-cell-{cell}-incoming-segment-count"),
        incoming.iter().map(|arc| arc.selected),
        tag,
    );
    let outgoing_count = post_count(
        solver,
        format!("network-{network_index}-cell-{cell}-outgoing-segment-count"),
        outgoing.iter().map(|arc| arc.selected),
        tag,
    );
    let exactly_one_incoming = post_exactly_one_indicator(
        solver,
        format!("network-{network_index}-cell-{cell}-exactly-one-incoming"),
        incoming_count,
        incoming.len(),
        tag,
    );
    let exactly_one_outgoing = post_exactly_one_indicator(
        solver,
        format!("network-{network_index}-cell-{cell}-exactly-one-outgoing"),
        outgoing_count,
        outgoing.len(),
        tag,
    );
    let mut orthogonal_pairs = Vec::new();
    for incoming_arc in &incoming {
        for outgoing_arc in &outgoing {
            if same_axis(cell, incoming_arc.from, outgoing_arc.to, width) {
                continue;
            }
            orthogonal_pairs.push(post_and(
                solver,
                format!(
                    "network-{network_index}-cell-{cell}-turn-pair-{}-{}",
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
        format!("network-{network_index}-cell-{cell}-has-orthogonal-pair"),
        orthogonal_pairs.iter().copied(),
        tag,
    );
    let exact_segments = post_and(
        solver,
        format!("network-{network_index}-cell-{cell}-exact-segments"),
        exactly_one_incoming,
        exactly_one_outgoing,
        tag,
    );
    metrics.objective_variables += 7 + orthogonal_pairs.len();
    post_and(
        solver,
        format!("network-{network_index}-cell-{cell}-turn"),
        exact_segments,
        has_orthogonal_pair,
        tag,
    )
}

fn post_count(
    solver: &mut Solver,
    name: String,
    variables: impl Iterator<Item = DomainId>,
    tag: ConstraintTag,
) -> DomainId {
    let variables = variables.collect::<Vec<_>>();
    let count = solver.new_named_bounded_integer(
        0,
        i32::try_from(variables.len()).expect("a grid cell has at most four neighbors"),
        name,
    );
    let mut definition = variables
        .iter()
        .map(|variable| variable.scaled(1))
        .collect::<Vec<_>>();
    definition.push(count.scaled(-1));
    solver
        .add_constraint(pumpkin_solver::equals(definition, 0, tag))
        .post();
    count
}

fn post_exactly_one_indicator(
    solver: &mut Solver,
    name: String,
    count: DomainId,
    count_upper_bound: usize,
    tag: ConstraintTag,
) -> DomainId {
    let indicator = solver.new_named_bounded_integer(0, 1, name);
    let rows = (0..=count_upper_bound)
        .map(|value| {
            vec![
                i32::try_from(value).expect("neighbor count fits i32"),
                i32::from(value == 1),
            ]
        })
        .collect::<Vec<_>>();
    solver
        .add_constraint(pumpkin_solver::table([count, indicator], rows, tag))
        .post();
    indicator
}

fn post_and(
    solver: &mut Solver,
    name: String,
    left: DomainId,
    right: DomainId,
    tag: ConstraintTag,
) -> DomainId {
    let conjunction = solver.new_named_bounded_integer(0, 1, name);
    solver
        .add_constraint(pumpkin_solver::less_than_or_equals(
            [conjunction.scaled(1), left.scaled(-1)],
            0,
            tag,
        ))
        .post();
    solver
        .add_constraint(pumpkin_solver::less_than_or_equals(
            [conjunction.scaled(1), right.scaled(-1)],
            0,
            tag,
        ))
        .post();
    solver
        .add_constraint(pumpkin_solver::greater_than_or_equals(
            [conjunction.scaled(1), left.scaled(-1), right.scaled(-1)],
            -1,
            tag,
        ))
        .post();
    conjunction
}

fn same_axis(cell: usize, incoming_from: usize, outgoing_to: usize, width: i32) -> bool {
    let width = usize::try_from(width).expect("validated width is positive");
    let incoming_horizontal = cell / width == incoming_from / width;
    let outgoing_horizontal = cell / width == outgoing_to / width;
    incoming_horizontal == outgoing_horizontal
}
