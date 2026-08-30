use std::collections::{BTreeMap, BTreeSet};

use pumpkin_solver::core::results::ProblemSolution;

use super::super::{
    FacilityPlacement, INTEGRATED_LAYOUT_SCHEMA_VERSION, IntegratedLayoutDiagnostic,
    IntegratedLayoutReport, IntegratedLayoutStatus, IntegratedRoute, ModelInput, WorldGridPosition,
    canonicalize_report_geometry, world_position,
};
use super::{Arc, EndpointOption, ModelInstance, ModelRoute};

pub(in crate::layouts::integrated) fn extract_report(
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
            IntegratedRoute {
                requirement_id: edge.requirement_id.clone(),
                requirement_fingerprint: edge.requirement_fingerprint.clone(),
                source: source.endpoint.clone(),
                target: target.endpoint.clone(),
                item: edge.edge.item.clone(),
                rate: edge.edge.rate,
                transport: edge.transport,
                cells,
            }
        })
        .collect();

    let mut report = IntegratedLayoutReport {
        schema_version: INTEGRATED_LAYOUT_SCHEMA_VERSION,
        success: true,
        status,
        bounds: None,
        placements,
        logistics_components: Vec::new(),
        routes,
        phases: Vec::new(),
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
    };
    canonicalize_report_geometry(&mut report);
    report
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
