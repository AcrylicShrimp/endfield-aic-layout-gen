use std::collections::BTreeSet;
use std::time::Instant;

use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::termination::TerminationCondition;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::formulation::{generate_candidates, post_at_most_one, post_equals_one};
use super::recorder::{ConstraintFamily, RecordedModel, VariableFamily};
use super::{Candidate, InstanceInput};
use crate::facilities::FacilityDefinition;
use crate::layouts::FacilityPlacementRequest;
use crate::layouts::integrated::research::{
    PHYSICAL_OCCUPANCY_PROBE_SCHEMA_VERSION, PhysicalOccupancyCaseReport,
    PhysicalOccupancyDomainSnapshot, PhysicalOccupancyEncoding, PhysicalOccupancyProbeReport,
    PhysicalOccupancyRestriction,
};
use crate::research::BenchmarkRequestBounds;

struct ProbeModel {
    solver: RecordedModel,
    candidates: Vec<Candidate>,
    placement_choice: DomainId,
    belt_cells: Vec<DomainId>,
    pipe_cells: Vec<DomainId>,
    occupancy: Vec<Vec<DomainId>>,
    target_cell: usize,
    selected_footprint: BTreeSet<usize>,
    same_footprint_candidates: BTreeSet<usize>,
    non_covering_candidates: BTreeSet<usize>,
}

#[derive(Clone)]
struct RawSnapshot {
    public: PhysicalOccupancyDomainSnapshot,
    candidate_domains: Vec<[bool; 2]>,
    belt_domains: Vec<[bool; 2]>,
    pipe_domains: Vec<[bool; 2]>,
}

struct StopAfterRootPropagation {
    polls: u8,
}

impl TerminationCondition for StopAfterRootPropagation {
    fn should_stop(&mut self) -> bool {
        self.polls += 1;
        self.polls > 1
    }
}

pub(in crate::layouts::integrated) fn probe_candidate_collision_occupancy(
    facility: &FacilityDefinition,
    request: &FacilityPlacementRequest,
) -> Result<PhysicalOccupancyProbeReport, String> {
    let width = i32::try_from(request.max_width)
        .map_err(|_| "physical occupancy probe max_width does not fit i32".to_string())?;
    let height = i32::try_from(request.max_height)
        .map_err(|_| "physical occupancy probe max_height does not fit i32".to_string())?;
    if facility.footprint.width != 5 || facility.footprint.height != 5 {
        return Err(format!(
            "physical occupancy probe requires a 5 by 5 facility, found {} by {} for '{}'",
            facility.footprint.width, facility.footprint.height, facility.id
        ));
    }
    if width < 7 || height < 7 {
        return Err("physical occupancy probe requires bounds of at least 7 by 7".to_string());
    }

    let target = [width / 2, height / 2];
    let same_origin = [target[0] - 4, target[1] - 4];
    let control_origin = [0, 0];
    let base = build_model(facility, width, height, target, same_origin, control_origin)?;
    let candidate_count = base.candidates.len();
    let target_covering = base.occupancy[base.target_cell].len();
    let collision_rows = width as usize * height as usize * 2;
    let collision_terms = base
        .occupancy
        .iter()
        .map(|covering| (covering.len() + 1) * 2)
        .sum();
    drop(base);

    let restrictions = [
        PhysicalOccupancyRestriction::None,
        PhysicalOccupancyRestriction::BeltUsed,
        PhysicalOccupancyRestriction::PipeUsed,
        PhysicalOccupancyRestriction::ExactPlacement,
        PhysicalOccupancyRestriction::SameFootprintDomain,
        PhysicalOccupancyRestriction::NonCoveringControl,
    ];
    let mut cases = Vec::with_capacity(restrictions.len());
    for restriction in restrictions {
        let mut model = build_model(facility, width, height, target, same_origin, control_origin)?;
        propagate_root(&mut model.solver);
        let before = snapshot(&model);
        apply_restriction(&mut model, restriction);
        let started = Instant::now();
        propagate_root(&mut model.solver);
        let propagation_time_us = started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        let after = snapshot(&model);
        cases.push(compare_case(
            &model,
            restriction,
            before,
            after,
            propagation_time_us,
        ));
    }

    Ok(PhysicalOccupancyProbeReport {
        schema_version: PHYSICAL_OCCUPANCY_PROBE_SCHEMA_VERSION,
        encoding: PhysicalOccupancyEncoding::CandidateCollision,
        facility_id: facility.id.clone(),
        request_bounds: BenchmarkRequestBounds {
            max_width: request.max_width as u32,
            max_height: request.max_height as u32,
        },
        target_cell: target,
        same_footprint_origin: same_origin,
        non_covering_origin: control_origin,
        candidate_count,
        analytically_target_covering_candidates: target_covering,
        collision_rows,
        collision_terms,
        cases,
    })
}

fn build_model(
    facility: &FacilityDefinition,
    width: i32,
    height: i32,
    target: [i32; 2],
    same_origin: [i32; 2],
    control_origin: [i32; 2],
) -> Result<ProbeModel, String> {
    let mut solver = RecordedModel::default();
    let tag = solver.new_constraint_tag();
    let instance = InstanceInput {
        id: "occupancy-probe-facility".to_string(),
        recipe: "occupancy-probe".to_string(),
        facility: facility.id.clone(),
        definition: facility.clone(),
    };
    let candidates = generate_candidates(&mut solver, &instance, width, height);
    if candidates.is_empty() {
        return Err("physical occupancy probe generated no legal placement candidates".to_string());
    }
    post_equals_one(
        &mut solver,
        ConstraintFamily::PlacementChoice,
        candidates.iter().map(|candidate| candidate.selected),
        tag,
    );

    let cell_count = width as usize * height as usize;
    let mut occupancy = vec![Vec::new(); cell_count];
    for candidate in &candidates {
        for cell in &candidate.occupied_cells {
            occupancy[*cell].push(candidate.selected);
        }
    }
    for covering in &occupancy {
        post_at_most_one(
            &mut solver,
            ConstraintFamily::FacilityNonOverlap,
            covering.iter().copied(),
            tag,
        );
    }

    let upper_bound = i32::try_from(candidates.len() - 1)
        .map_err(|_| "physical occupancy candidate index does not fit i32".to_string())?;
    let placement_choice = solver.new_variable(
        VariableFamily::Placement,
        0,
        upper_bound,
        "occupancy-probe-placement-choice",
    );
    let mut placement_definition = vec![placement_choice.scaled(1)];
    placement_definition.extend(
        candidates
            .iter()
            .enumerate()
            .skip(1)
            .map(|(index, candidate)| {
                candidate
                    .selected
                    .scaled(-i32::try_from(index).expect("candidate index fits i32"))
            }),
    );
    solver.post_equals(
        ConstraintFamily::PlacementChoice,
        placement_definition,
        0,
        upper_bound as u64,
        tag,
    );

    let belt_cells = (0..cell_count)
        .map(|cell| {
            solver.new_variable(
                VariableFamily::RouteCell,
                0,
                1,
                format!("occupancy-probe-belt-{cell}"),
            )
        })
        .collect::<Vec<_>>();
    let pipe_cells = (0..cell_count)
        .map(|cell| {
            solver.new_variable(
                VariableFamily::RouteCell,
                0,
                1,
                format!("occupancy-probe-pipe-{cell}"),
            )
        })
        .collect::<Vec<_>>();
    for cell in 0..cell_count {
        for transport_cell in [belt_cells[cell], pipe_cells[cell]] {
            solver.post_less_than_or_equals(
                ConstraintFamily::TransportCollision,
                occupancy[cell]
                    .iter()
                    .map(|candidate| candidate.scaled(1))
                    .chain(std::iter::once(transport_cell.scaled(1)))
                    .collect(),
                1,
                1,
                tag,
            );
        }
    }

    let candidate_indices_at = |origin: [i32; 2]| {
        candidates
            .iter()
            .enumerate()
            .filter_map(|(index, candidate)| {
                (candidate.x == origin[0] && candidate.y == origin[1]).then_some(index)
            })
            .collect::<BTreeSet<_>>()
    };
    let same_footprint_candidates = candidate_indices_at(same_origin);
    let non_covering_candidates = candidate_indices_at(control_origin);
    if same_footprint_candidates.len() != facility.allowed_rotations.len()
        || non_covering_candidates.len() != facility.allowed_rotations.len()
    {
        return Err("probe origins do not retain exactly one candidate per rotation".to_string());
    }
    let target_cell = target[1] as usize * width as usize + target[0] as usize;
    let selected_footprint = candidates[*same_footprint_candidates
        .first()
        .expect("same-footprint candidate set is non-empty")]
    .occupied_cells
    .iter()
    .copied()
    .collect();

    Ok(ProbeModel {
        solver,
        candidates,
        placement_choice,
        belt_cells,
        pipe_cells,
        occupancy,
        target_cell,
        selected_footprint,
        same_footprint_candidates,
        non_covering_candidates,
    })
}

fn apply_restriction(model: &mut ProbeModel, restriction: PhysicalOccupancyRestriction) {
    let tag = model.solver.new_constraint_tag();
    match restriction {
        PhysicalOccupancyRestriction::None => {}
        PhysicalOccupancyRestriction::BeltUsed => model.solver.add_clause(
            [model.belt_cells[model.target_cell].equality_predicate(1)],
            tag,
        ),
        PhysicalOccupancyRestriction::PipeUsed => model.solver.add_clause(
            [model.pipe_cells[model.target_cell].equality_predicate(1)],
            tag,
        ),
        PhysicalOccupancyRestriction::ExactPlacement => {
            let index = *model
                .same_footprint_candidates
                .first()
                .expect("same-footprint candidates exist");
            model.solver.add_clause(
                [model.candidates[index].selected.equality_predicate(1)],
                tag,
            );
        }
        PhysicalOccupancyRestriction::SameFootprintDomain => {
            for (index, candidate) in model.candidates.iter().enumerate() {
                if !model.same_footprint_candidates.contains(&index) {
                    model
                        .solver
                        .add_clause([candidate.selected.equality_predicate(0)], tag);
                }
            }
        }
        PhysicalOccupancyRestriction::NonCoveringControl => {
            for (index, candidate) in model.candidates.iter().enumerate() {
                if !model.non_covering_candidates.contains(&index) {
                    model
                        .solver
                        .add_clause([candidate.selected.equality_predicate(0)], tag);
                }
            }
        }
    }
}

fn propagate_root(solver: &mut RecordedModel) {
    let mut brancher = solver.default_brancher();
    let mut resolver = ResolutionResolver::default();
    let mut termination = StopAfterRootPropagation { polls: 0 };
    let result = solver.satisfy(&mut brancher, &mut termination, &mut resolver);
    drop(result);
}

fn snapshot(model: &ProbeModel) -> RawSnapshot {
    let candidate_domains = model
        .candidates
        .iter()
        .map(|candidate| domain(&model.solver, candidate.selected))
        .collect::<Vec<_>>();
    let supported = candidate_domains
        .iter()
        .enumerate()
        .filter_map(|(index, values)| values[1].then_some(index))
        .collect::<Vec<_>>();
    let distinct_x_values = supported
        .iter()
        .map(|index| model.candidates[*index].x)
        .collect::<BTreeSet<_>>()
        .len();
    let distinct_y_values = supported
        .iter()
        .map(|index| model.candidates[*index].y)
        .collect::<BTreeSet<_>>()
        .len();
    let distinct_rotation_values = supported
        .iter()
        .map(|index| model.candidates[*index].rotation)
        .collect::<BTreeSet<_>>()
        .len();
    let belt_domains = model
        .belt_cells
        .iter()
        .map(|cell| domain(&model.solver, *cell))
        .collect::<Vec<_>>();
    let pipe_domains = model
        .pipe_cells
        .iter()
        .map(|cell| domain(&model.solver, *cell))
        .collect::<Vec<_>>();
    let (belt_true, belt_false, belt_free) = transport_counts(&belt_domains);
    let (pipe_true, pipe_false, pipe_free) = transport_counts(&pipe_domains);
    RawSnapshot {
        public: PhysicalOccupancyDomainSnapshot {
            supported_placement_candidates: supported.len(),
            fixed_false_placement_candidates: candidate_domains
                .iter()
                .filter(|values| !values[1])
                .count(),
            fixed_true_placement_candidates: candidate_domains
                .iter()
                .filter(|values| !values[0] && values[1])
                .count(),
            supported_placement_choice_values: (0..model.candidates.len())
                .filter(|index| {
                    model.solver.contains(
                        &model.placement_choice,
                        i32::try_from(*index).expect("candidate index fits i32"),
                    )
                })
                .count(),
            distinct_x_values,
            distinct_y_values,
            distinct_rotation_values,
            belt_cells_fixed_true: belt_true,
            belt_cells_fixed_false: belt_false,
            belt_cells_free: belt_free,
            pipe_cells_fixed_true: pipe_true,
            pipe_cells_fixed_false: pipe_false,
            pipe_cells_free: pipe_free,
            target_belt_domain: values(belt_domains[model.target_cell]),
            target_pipe_domain: values(pipe_domains[model.target_cell]),
        },
        candidate_domains,
        belt_domains,
        pipe_domains,
    }
}

fn compare_case(
    model: &ProbeModel,
    restriction: PhysicalOccupancyRestriction,
    before: RawSnapshot,
    after: RawSnapshot,
    propagation_time_us: u64,
) -> PhysicalOccupancyCaseReport {
    let removed_candidates = before
        .candidate_domains
        .iter()
        .zip(&after.candidate_domains)
        .enumerate()
        .filter_map(|(index, (old, new))| (old[1] && !new[1]).then_some(index))
        .collect::<BTreeSet<_>>();
    let target_covering = model.occupancy[model.target_cell]
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    let candidate_index_by_variable = model
        .candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.selected, index))
        .collect::<std::collections::BTreeMap<_, _>>();
    let target_covering_indices = target_covering
        .iter()
        .map(|variable| candidate_index_by_variable[variable])
        .collect::<BTreeSet<_>>();
    let removed_target_covering_candidates = removed_candidates
        .intersection(&target_covering_indices)
        .count();
    let newly_forbidden = |old: &[[bool; 2]], new: &[[bool; 2]]| {
        model
            .selected_footprint
            .iter()
            .filter(|cell| old[**cell][1] && !new[**cell][1])
            .count()
    };
    let changed_collision_rows = (0..model.belt_cells.len())
        .map(|cell| {
            let candidate_changed = model.occupancy[cell].iter().any(|variable| {
                let index = candidate_index_by_variable[variable];
                before.candidate_domains[index] != after.candidate_domains[index]
            });
            usize::from(candidate_changed || before.belt_domains[cell] != after.belt_domains[cell])
                + usize::from(
                    candidate_changed || before.pipe_domains[cell] != after.pipe_domains[cell],
                )
        })
        .sum();
    let fully_decided_collision_rows = (0..model.belt_cells.len())
        .map(|cell| {
            let candidates_fixed = model.occupancy[cell].iter().all(|variable| {
                let index = candidate_index_by_variable[variable];
                after.candidate_domains[index][0] != after.candidate_domains[index][1]
            });
            usize::from(candidates_fixed && fixed(after.belt_domains[cell]))
                + usize::from(candidates_fixed && fixed(after.pipe_domains[cell]))
        })
        .sum();
    let (incident_collision_rows, incident_collision_terms) = match restriction {
        PhysicalOccupancyRestriction::BeltUsed | PhysicalOccupancyRestriction::PipeUsed => {
            (1, model.occupancy[model.target_cell].len() + 1)
        }
        PhysicalOccupancyRestriction::ExactPlacement
        | PhysicalOccupancyRestriction::SameFootprintDomain
        | PhysicalOccupancyRestriction::NonCoveringControl => {
            let rows = model.selected_footprint.len() * 2;
            let terms = model
                .selected_footprint
                .iter()
                .map(|cell| (model.occupancy[*cell].len() + 1) * 2)
                .sum();
            (rows, terms)
        }
        PhysicalOccupancyRestriction::None => (0, 0),
    };
    let verdict = match restriction {
        PhysicalOccupancyRestriction::BeltUsed
            if removed_target_covering_candidates == target_covering_indices.len()
                && after.public.target_pipe_domain == [0, 1] =>
        {
            "strong transport-to-placement propagation".to_string()
        }
        PhysicalOccupancyRestriction::PipeUsed
            if removed_target_covering_candidates == target_covering_indices.len()
                && after.public.target_belt_domain == [0, 1] =>
        {
            "strong transport-to-placement propagation".to_string()
        }
        PhysicalOccupancyRestriction::ExactPlacement
            if newly_forbidden(&before.belt_domains, &after.belt_domains)
                == model.selected_footprint.len()
                && newly_forbidden(&before.pipe_domains, &after.pipe_domains)
                    == model.selected_footprint.len() =>
        {
            "strong exact-placement-to-transport propagation".to_string()
        }
        PhysicalOccupancyRestriction::SameFootprintDomain
            if newly_forbidden(&before.belt_domains, &after.belt_domains) == 0
                && newly_forbidden(&before.pipe_domains, &after.pipe_domains) == 0 =>
        {
            "partial placement domain does not propagate mandatory occupancy".to_string()
        }
        PhysicalOccupancyRestriction::NonCoveringControl
            if after.public.target_belt_domain == [0, 1]
                && after.public.target_pipe_domain == [0, 1] =>
        {
            "control target remains available".to_string()
        }
        PhysicalOccupancyRestriction::None => "base root fixed point".to_string(),
        _ => "unexpected propagation result".to_string(),
    };

    PhysicalOccupancyCaseReport {
        restriction,
        before: before.public,
        after: after.public,
        removed_target_covering_candidates,
        removed_non_covering_candidates: removed_candidates.len()
            - removed_target_covering_candidates,
        newly_forbidden_belt_cells_inside_selected_footprint: newly_forbidden(
            &before.belt_domains,
            &after.belt_domains,
        ),
        newly_forbidden_pipe_cells_inside_selected_footprint: newly_forbidden(
            &before.pipe_domains,
            &after.pipe_domains,
        ),
        changed_collision_rows,
        fully_decided_collision_rows,
        incident_collision_rows,
        incident_collision_terms,
        propagation_time_us,
        inconsistent: model.solver.is_inconsistent(),
        verdict,
    }
}

fn domain(solver: &RecordedModel, variable: DomainId) -> [bool; 2] {
    [solver.contains(&variable, 0), solver.contains(&variable, 1)]
}

fn values(domain: [bool; 2]) -> Vec<i32> {
    [0, 1]
        .into_iter()
        .zip(domain)
        .filter_map(|(value, supported)| supported.then_some(value))
        .collect()
}

fn fixed(domain: [bool; 2]) -> bool {
    domain[0] != domain[1]
}

fn transport_counts(domains: &[[bool; 2]]) -> (usize, usize, usize) {
    let fixed_true = domains
        .iter()
        .filter(|domain| !domain[0] && domain[1])
        .count();
    let fixed_false = domains
        .iter()
        .filter(|domain| domain[0] && !domain[1])
        .count();
    let free = domains
        .iter()
        .filter(|domain| domain[0] && domain[1])
        .count();
    (fixed_true, fixed_false, free)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityFootprint, FacilityPortDefinition};

    fn facility() -> FacilityDefinition {
        FacilityDefinition {
            id: "test-five-by-five".to_string(),
            footprint: FacilityFootprint {
                width: 5,
                height: 5,
            },
            allowed_rotations: vec![0, 90, 180, 270],
            ports: Vec::<FacilityPortDefinition>::new(),
        }
    }

    #[test]
    fn baseline_exposes_partial_domain_propagation_gap() {
        let report = probe_candidate_collision_occupancy(
            &facility(),
            &FacilityPlacementRequest {
                schema_version: 1,
                max_width: 12,
                max_height: 12,
            },
        )
        .expect("probe should run");
        assert_eq!(report.candidate_count, 256);
        assert_eq!(report.analytically_target_covering_candidates, 100);
        let partial = report
            .cases
            .iter()
            .find(|case| case.restriction == PhysicalOccupancyRestriction::SameFootprintDomain)
            .expect("partial-domain case exists");
        assert_eq!(partial.after.target_belt_domain, vec![0, 1]);
        assert_eq!(partial.after.target_pipe_domain, vec![0, 1]);
    }
}
