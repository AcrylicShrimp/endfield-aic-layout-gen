use std::collections::BTreeSet;
use std::time::Instant;

use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::termination::TerminationCondition;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::InstanceInput;
use super::formulation::{
    generate_candidate_geometries, generate_candidates, post_at_most_one, post_equals_one,
};
use super::recorder::{ConstraintFamily, RecordedModel, VariableFamily};
use crate::facilities::FacilityDefinition;
use crate::layouts::FacilityPlacementRequest;
use crate::layouts::integrated::research::{
    PHYSICAL_OCCUPANCY_PROBE_SCHEMA_VERSION, PhysicalOccupancyCaseReport,
    PhysicalOccupancyDomainSnapshot, PhysicalOccupancyEncoding, PhysicalOccupancyProbeReport,
    PhysicalOccupancyRestriction,
};
use crate::research::BenchmarkRequestBounds;

struct ProbeCandidate {
    rotation: i64,
    x: i32,
    y: i32,
    occupied_cells: Vec<usize>,
    selected: Option<DomainId>,
}

struct ProbeModel {
    encoding: PhysicalOccupancyEncoding,
    solver: RecordedModel,
    candidates: Vec<ProbeCandidate>,
    placement_choice: DomainId,
    facility_cells: Option<Vec<DomainId>>,
    belt_cells: Vec<DomainId>,
    pipe_cells: Vec<DomainId>,
    covering_candidates: Vec<Vec<usize>>,
    target_cell: usize,
    selected_footprint: BTreeSet<usize>,
    same_footprint_candidates: BTreeSet<usize>,
    non_covering_candidates: BTreeSet<usize>,
}

#[derive(Clone)]
struct RawSnapshot {
    public: PhysicalOccupancyDomainSnapshot,
    candidate_domains: Vec<[bool; 2]>,
    facility_domains: Vec<[bool; 2]>,
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

pub(in crate::layouts::integrated) fn probe_physical_occupancy(
    facility: &FacilityDefinition,
    request: &FacilityPlacementRequest,
    encoding: PhysicalOccupancyEncoding,
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
    if width < 9 || height < 9 {
        return Err("physical occupancy probe requires bounds of at least 9 by 9".to_string());
    }

    let target = [width / 2, height / 2];
    let same_origin = [target[0] - 4, target[1] - 4];
    let control_origin = [0, 0];
    let base = build_model(
        facility,
        width,
        height,
        target,
        same_origin,
        control_origin,
        encoding,
    )?;
    let candidate_count = base.candidates.len();
    let target_covering = base.covering_candidates[base.target_cell].len();
    let collision_rows = width as usize * height as usize * 2;
    let collision_terms = match encoding {
        PhysicalOccupancyEncoding::CandidateCollision => base
            .covering_candidates
            .iter()
            .map(|covering| (covering.len() + 1) * 2)
            .sum(),
        PhysicalOccupancyEncoding::CanonicalSharedOccupancy => collision_rows * 2,
    };
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
        let mut model = build_model(
            facility,
            width,
            height,
            target,
            same_origin,
            control_origin,
            encoding,
        )?;
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
        encoding,
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

#[allow(clippy::too_many_arguments)]
fn build_model(
    facility: &FacilityDefinition,
    width: i32,
    height: i32,
    target: [i32; 2],
    same_origin: [i32; 2],
    control_origin: [i32; 2],
    encoding: PhysicalOccupancyEncoding,
) -> Result<ProbeModel, String> {
    let mut solver = RecordedModel::default();
    let tag = solver.new_constraint_tag();
    let instance = InstanceInput {
        id: "occupancy-probe-facility".to_string(),
        recipe: "occupancy-probe".to_string(),
        facility: facility.id.clone(),
        definition: facility.clone(),
    };

    let (candidates, candidate_variables) = match encoding {
        PhysicalOccupancyEncoding::CandidateCollision => {
            let candidates = generate_candidates(&mut solver, &instance, width, height);
            if candidates.is_empty() {
                return Err(
                    "physical occupancy probe generated no legal placement candidates".to_string(),
                );
            }
            post_equals_one(
                &mut solver,
                ConstraintFamily::PlacementChoice,
                candidates.iter().map(|candidate| candidate.selected),
                tag,
            );
            let variables = candidates
                .iter()
                .map(|candidate| candidate.selected)
                .collect::<Vec<_>>();
            let candidates = candidates
                .into_iter()
                .map(|candidate| ProbeCandidate {
                    rotation: candidate.rotation,
                    x: candidate.x,
                    y: candidate.y,
                    occupied_cells: candidate.occupied_cells,
                    selected: Some(candidate.selected),
                })
                .collect::<Vec<_>>();
            (candidates, Some(variables))
        }
        PhysicalOccupancyEncoding::CanonicalSharedOccupancy => {
            let candidates = generate_candidate_geometries(&instance, width, height)
                .into_iter()
                .map(|candidate| ProbeCandidate {
                    rotation: candidate.rotation,
                    x: candidate.x,
                    y: candidate.y,
                    occupied_cells: candidate.occupied_cells,
                    selected: None,
                })
                .collect::<Vec<_>>();
            if candidates.is_empty() {
                return Err(
                    "physical occupancy probe generated no legal placement candidates".to_string(),
                );
            }
            (candidates, None)
        }
    };

    let cell_count = width as usize * height as usize;
    let mut covering_candidates = vec![Vec::new(); cell_count];
    for (candidate_index, candidate) in candidates.iter().enumerate() {
        for cell in &candidate.occupied_cells {
            covering_candidates[*cell].push(candidate_index);
        }
    }

    let upper_bound = i32::try_from(candidates.len() - 1)
        .map_err(|_| "physical occupancy candidate index does not fit i32".to_string())?;
    let placement_choice = solver.new_variable(
        VariableFamily::Placement,
        0,
        upper_bound,
        "occupancy-probe-placement-choice",
    );
    if let Some(candidate_variables) = &candidate_variables {
        for covering in &covering_candidates {
            post_at_most_one(
                &mut solver,
                ConstraintFamily::FacilityNonOverlap,
                covering.iter().map(|index| candidate_variables[*index]),
                tag,
            );
        }
        let mut placement_definition = vec![placement_choice.scaled(1)];
        placement_definition.extend(candidate_variables.iter().enumerate().skip(1).map(
            |(index, candidate)| {
                candidate.scaled(-i32::try_from(index).expect("candidate index fits i32"))
            },
        ));
        solver.post_equals(
            ConstraintFamily::PlacementChoice,
            placement_definition,
            0,
            upper_bound as u64,
            tag,
        );
    }

    let facility_cells = if matches!(
        encoding,
        PhysicalOccupancyEncoding::CanonicalSharedOccupancy
    ) {
        Some(
            (0..cell_count)
                .map(|cell| {
                    let instance_occupied = solver.new_variable(
                        VariableFamily::PhysicalOccupancy,
                        0,
                        1,
                        format!("occupancy-probe-instance-{cell}"),
                    );
                    let values = candidates
                        .iter()
                        .map(|candidate| i32::from(candidate.occupied_cells.contains(&cell)))
                        .collect::<Vec<_>>();
                    solver.post_constant_element(
                        ConstraintFamily::OccupancyChannel,
                        placement_choice,
                        values,
                        instance_occupied,
                        tag,
                    );
                    let facility_occupied = solver.new_variable(
                        VariableFamily::PhysicalOccupancy,
                        0,
                        1,
                        format!("occupancy-probe-facility-{cell}"),
                    );
                    solver.post_equals(
                        ConstraintFamily::OccupancyChannel,
                        vec![facility_occupied.scaled(1), instance_occupied.scaled(-1)],
                        0,
                        1,
                        tag,
                    );
                    facility_occupied
                })
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };

    let belt_cells = new_transport_cells(&mut solver, cell_count, "belt");
    let pipe_cells = new_transport_cells(&mut solver, cell_count, "pipe");
    for cell in 0..cell_count {
        for transport_cell in [belt_cells[cell], pipe_cells[cell]] {
            let terms = match (&facility_cells, &candidate_variables) {
                (Some(facility_cells), None) => {
                    vec![facility_cells[cell].scaled(1), transport_cell.scaled(1)]
                }
                (None, Some(candidate_variables)) => covering_candidates[cell]
                    .iter()
                    .map(|index| candidate_variables[*index].scaled(1))
                    .chain(std::iter::once(transport_cell.scaled(1)))
                    .collect(),
                _ => unreachable!("occupancy encoding selects exactly one collision channel"),
            };
            solver.post_less_than_or_equals(ConstraintFamily::TransportCollision, terms, 1, 1, tag);
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
        encoding,
        solver,
        candidates,
        placement_choice,
        facility_cells,
        belt_cells,
        pipe_cells,
        covering_candidates,
        target_cell,
        selected_footprint,
        same_footprint_candidates,
        non_covering_candidates,
    })
}

fn new_transport_cells(
    solver: &mut RecordedModel,
    cell_count: usize,
    transport: &str,
) -> Vec<DomainId> {
    (0..cell_count)
        .map(|cell| {
            solver.new_variable(
                VariableFamily::TransportOccupancy,
                0,
                1,
                format!("occupancy-probe-{transport}-{cell}"),
            )
        })
        .collect()
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
            restrict_exact_placement(model, index, tag);
        }
        PhysicalOccupancyRestriction::SameFootprintDomain => {
            restrict_placement_domain(model, model.same_footprint_candidates.clone(), tag);
        }
        PhysicalOccupancyRestriction::NonCoveringControl => {
            restrict_placement_domain(model, model.non_covering_candidates.clone(), tag);
        }
    }
}

fn restrict_exact_placement(
    model: &mut ProbeModel,
    index: usize,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    match model.encoding {
        PhysicalOccupancyEncoding::CandidateCollision => model.solver.add_clause(
            [model.candidates[index]
                .selected
                .expect("candidate encoding has selection variables")
                .equality_predicate(1)],
            tag,
        ),
        PhysicalOccupancyEncoding::CanonicalSharedOccupancy => model.solver.add_clause(
            [model
                .placement_choice
                .equality_predicate(i32::try_from(index).expect("candidate index fits i32"))],
            tag,
        ),
    }
}

fn restrict_placement_domain(
    model: &mut ProbeModel,
    retained: BTreeSet<usize>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for index in 0..model.candidates.len() {
        if retained.contains(&index) {
            continue;
        }
        let predicate = match model.encoding {
            PhysicalOccupancyEncoding::CandidateCollision => model.candidates[index]
                .selected
                .expect("candidate encoding has selection variables")
                .equality_predicate(0),
            PhysicalOccupancyEncoding::CanonicalSharedOccupancy => model
                .placement_choice
                .disequality_predicate(i32::try_from(index).expect("candidate index fits i32")),
        };
        model.solver.add_clause([predicate], tag);
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
    let supported_choice_values = (0..model.candidates.len())
        .map(|index| {
            model.solver.contains(
                &model.placement_choice,
                i32::try_from(index).expect("candidate index fits i32"),
            )
        })
        .collect::<Vec<_>>();
    let supported_choice_count = supported_choice_values
        .iter()
        .filter(|supported| **supported)
        .count();
    let candidate_domains = match model.encoding {
        PhysicalOccupancyEncoding::CandidateCollision => model
            .candidates
            .iter()
            .map(|candidate| {
                domain(
                    &model.solver,
                    candidate
                        .selected
                        .expect("candidate encoding has selection variables"),
                )
            })
            .collect::<Vec<_>>(),
        PhysicalOccupancyEncoding::CanonicalSharedOccupancy => supported_choice_values
            .iter()
            .map(|supported| [!*supported || supported_choice_count > 1, *supported])
            .collect::<Vec<_>>(),
    };
    let supported = candidate_domains
        .iter()
        .enumerate()
        .filter_map(|(index, values)| values[1].then_some(index))
        .collect::<Vec<_>>();
    let facility_domains = model.facility_cells.as_ref().map_or_else(
        || derive_facility_domains(&candidate_domains, &model.covering_candidates),
        |cells| {
            cells
                .iter()
                .map(|cell| domain(&model.solver, *cell))
                .collect()
        },
    );
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
    let (facility_true, facility_false, facility_free) = binary_counts(&facility_domains);
    let (belt_true, belt_false, belt_free) = binary_counts(&belt_domains);
    let (pipe_true, pipe_false, pipe_free) = binary_counts(&pipe_domains);
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
            supported_placement_choice_values: supported_choice_count,
            distinct_x_values: supported
                .iter()
                .map(|index| model.candidates[*index].x)
                .collect::<BTreeSet<_>>()
                .len(),
            distinct_y_values: supported
                .iter()
                .map(|index| model.candidates[*index].y)
                .collect::<BTreeSet<_>>()
                .len(),
            distinct_rotation_values: supported
                .iter()
                .map(|index| model.candidates[*index].rotation)
                .collect::<BTreeSet<_>>()
                .len(),
            facility_cells_fixed_true: facility_true,
            facility_cells_fixed_false: facility_false,
            facility_cells_free: facility_free,
            belt_cells_fixed_true: belt_true,
            belt_cells_fixed_false: belt_false,
            belt_cells_free: belt_free,
            pipe_cells_fixed_true: pipe_true,
            pipe_cells_fixed_false: pipe_false,
            pipe_cells_free: pipe_free,
            target_facility_domain: values(facility_domains[model.target_cell]),
            target_belt_domain: values(belt_domains[model.target_cell]),
            target_pipe_domain: values(pipe_domains[model.target_cell]),
        },
        candidate_domains,
        facility_domains,
        belt_domains,
        pipe_domains,
    }
}

fn derive_facility_domains(
    candidate_domains: &[[bool; 2]],
    covering_candidates: &[Vec<usize>],
) -> Vec<[bool; 2]> {
    covering_candidates
        .iter()
        .map(|covering| {
            let can_be_occupied = covering.iter().any(|index| candidate_domains[*index][1]);
            let can_be_empty = candidate_domains
                .iter()
                .enumerate()
                .any(|(index, domain)| domain[1] && !covering.contains(&index));
            [can_be_empty, can_be_occupied]
        })
        .collect()
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
    let target_covering_indices = model.covering_candidates[model.target_cell]
        .iter()
        .copied()
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
            usize::from(
                before.facility_domains[cell] != after.facility_domains[cell]
                    || before.belt_domains[cell] != after.belt_domains[cell],
            ) + usize::from(
                before.facility_domains[cell] != after.facility_domains[cell]
                    || before.pipe_domains[cell] != after.pipe_domains[cell],
            )
        })
        .sum();
    let fully_decided_collision_rows = (0..model.belt_cells.len())
        .map(|cell| {
            usize::from(fixed(after.facility_domains[cell]) && fixed(after.belt_domains[cell]))
                + usize::from(
                    fixed(after.facility_domains[cell]) && fixed(after.pipe_domains[cell]),
                )
        })
        .sum();
    let terms_per_cell = |cell: usize| match model.encoding {
        PhysicalOccupancyEncoding::CandidateCollision => model.covering_candidates[cell].len() + 1,
        PhysicalOccupancyEncoding::CanonicalSharedOccupancy => 2,
    };
    let (incident_collision_rows, incident_collision_terms) = match restriction {
        PhysicalOccupancyRestriction::BeltUsed | PhysicalOccupancyRestriction::PipeUsed => {
            (1, terms_per_cell(model.target_cell))
        }
        PhysicalOccupancyRestriction::ExactPlacement
        | PhysicalOccupancyRestriction::SameFootprintDomain
        | PhysicalOccupancyRestriction::NonCoveringControl => (
            model.selected_footprint.len() * 2,
            model
                .selected_footprint
                .iter()
                .map(|cell| terms_per_cell(*cell) * 2)
                .sum(),
        ),
        PhysicalOccupancyRestriction::None => (0, 0),
    };
    let newly_forbidden_belt = newly_forbidden(&before.belt_domains, &after.belt_domains);
    let newly_forbidden_pipe = newly_forbidden(&before.pipe_domains, &after.pipe_domains);
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
            if newly_forbidden_belt == model.selected_footprint.len()
                && newly_forbidden_pipe == model.selected_footprint.len() =>
        {
            "strong exact-placement-to-transport propagation".to_string()
        }
        PhysicalOccupancyRestriction::SameFootprintDomain
            if newly_forbidden_belt == model.selected_footprint.len()
                && newly_forbidden_pipe == model.selected_footprint.len() =>
        {
            "mandatory occupancy propagates from the partial placement domain".to_string()
        }
        PhysicalOccupancyRestriction::SameFootprintDomain
            if newly_forbidden_belt == 0 && newly_forbidden_pipe == 0 =>
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
        newly_forbidden_belt_cells_inside_selected_footprint: newly_forbidden_belt,
        newly_forbidden_pipe_cells_inside_selected_footprint: newly_forbidden_pipe,
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

fn binary_counts(domains: &[[bool; 2]]) -> (usize, usize, usize) {
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
    use pumpkin_solver::core::results::SatisfactionResultUnderAssumptions;
    use pumpkin_solver::core::termination::Indefinite;

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

    fn request() -> FacilityPlacementRequest {
        FacilityPlacementRequest {
            schema_version: 1,
            max_width: 12,
            max_height: 12,
        }
    }

    #[test]
    fn baseline_exposes_partial_domain_propagation_gap() {
        let report = probe_physical_occupancy(
            &facility(),
            &request(),
            PhysicalOccupancyEncoding::CandidateCollision,
        )
        .expect("probe should run");
        assert_eq!(report.candidate_count, 256);
        assert_eq!(report.analytically_target_covering_candidates, 100);
        let partial = case(&report, PhysicalOccupancyRestriction::SameFootprintDomain);
        assert_eq!(partial.after.target_belt_domain, vec![0, 1]);
        assert_eq!(partial.after.target_pipe_domain, vec![0, 1]);
    }

    #[test]
    fn canonical_occupancy_propagates_the_partial_domain() {
        let report = probe_physical_occupancy(
            &facility(),
            &request(),
            PhysicalOccupancyEncoding::CanonicalSharedOccupancy,
        )
        .expect("probe should run");
        let partial = case(&report, PhysicalOccupancyRestriction::SameFootprintDomain);
        assert_eq!(partial.after.target_facility_domain, vec![1]);
        assert_eq!(
            partial.newly_forbidden_belt_cells_inside_selected_footprint,
            25
        );
        assert_eq!(
            partial.newly_forbidden_pipe_cells_inside_selected_footprint,
            25
        );
    }

    #[test]
    fn candidate_and_canonical_encodings_accept_the_same_controlled_states() {
        for encoding in [
            PhysicalOccupancyEncoding::CandidateCollision,
            PhysicalOccupancyEncoding::CanonicalSharedOccupancy,
        ] {
            let mut model = build_model(&facility(), 12, 12, [6, 6], [2, 2], [0, 0], encoding)
                .expect("equivalence model should build");
            for candidate_index in 0..model.candidates.len() {
                let covers_target = model.candidates[candidate_index]
                    .occupied_cells
                    .contains(&model.target_cell);
                for belt in 0..=1 {
                    for pipe in 0..=1 {
                        let expected = !covers_target || (belt == 0 && pipe == 0);
                        let placement = match encoding {
                            PhysicalOccupancyEncoding::CandidateCollision => model.candidates
                                [candidate_index]
                                .selected
                                .expect("candidate encoding has selection variables")
                                .equality_predicate(1),
                            PhysicalOccupancyEncoding::CanonicalSharedOccupancy => model
                                .placement_choice
                                .equality_predicate(candidate_index as i32),
                        };
                        let assumptions = [
                            placement,
                            model.belt_cells[model.target_cell].equality_predicate(belt),
                            model.pipe_cells[model.target_cell].equality_predicate(pipe),
                        ];
                        let mut brancher = model.solver.default_brancher();
                        let mut resolver = ResolutionResolver::default();
                        let result = model.solver.satisfy_under_assumptions(
                            &mut brancher,
                            &mut Indefinite,
                            &mut resolver,
                            &assumptions,
                        );
                        let actual =
                            matches!(result, SatisfactionResultUnderAssumptions::Satisfiable(_));
                        assert_eq!(
                            actual, expected,
                            "encoding={encoding:?} candidate={candidate_index} belt={belt} pipe={pipe}"
                        );
                    }
                }
            }
        }
    }

    fn case(
        report: &PhysicalOccupancyProbeReport,
        restriction: PhysicalOccupancyRestriction,
    ) -> &PhysicalOccupancyCaseReport {
        report
            .cases
            .iter()
            .find(|case| case.restriction == restriction)
            .expect("probe case exists")
    }
}
