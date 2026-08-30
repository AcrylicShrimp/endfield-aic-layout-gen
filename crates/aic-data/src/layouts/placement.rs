use std::collections::BTreeSet;
use std::ops::ControlFlow;

use pumpkin_solver::Solver;
use pumpkin_solver::conflict_resolvers::resolvers::ResolutionResolver;
use pumpkin_solver::core::DefaultBrancher;
use pumpkin_solver::core::constraints::NegatableConstraint;
use pumpkin_solver::core::optimisation::OptimisationDirection;
use pumpkin_solver::core::optimisation::linear_sat_unsat::LinearSatUnsat;
use pumpkin_solver::core::results::{OptimisationResult, ProblemSolution, SolutionReference};
use pumpkin_solver::core::termination::Indefinite;
use pumpkin_solver::core::variables::{DomainId, Literal, TransformableVariable};
use serde::{Deserialize, Serialize};

use crate::facilities::{FacilityFootprint, ValidatedFacilityCatalog};
use crate::recipes::{FacilityInstanceWiringNode, FacilityInstanceWiringReport};

const STAGE: &str = "facility-placement";

pub const SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacilityPlacementRequest {
    pub schema_version: u32,
    pub max_width: i64,
    pub max_height: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FacilityPlacementStatus {
    Optimal,
    Feasible,
    Infeasible,
    InvalidInput,
    Unknown,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityPlacementBounds {
    pub width: i64,
    pub height: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityPlacement {
    pub instance: String,
    pub recipe: String,
    pub facility: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub rotation: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityPlacementReport {
    pub success: bool,
    pub status: FacilityPlacementStatus,
    pub bounds: Option<FacilityPlacementBounds>,
    pub placements: Vec<FacilityPlacement>,
    pub diagnostics: Vec<FacilityPlacementDiagnostic>,
}

impl FacilityPlacementReport {
    fn solved(
        status: FacilityPlacementStatus,
        bounds: FacilityPlacementBounds,
        placements: Vec<FacilityPlacement>,
        code: &'static str,
        message: &'static str,
    ) -> Self {
        Self {
            success: true,
            status,
            bounds: Some(bounds),
            placements,
            diagnostics: vec![FacilityPlacementDiagnostic::info(code, "/", None, message)],
        }
    }

    pub fn invalid(diagnostic: FacilityPlacementDiagnostic) -> Self {
        Self::invalid_many(vec![diagnostic])
    }

    pub fn invalid_many(diagnostics: Vec<FacilityPlacementDiagnostic>) -> Self {
        Self::failure(FacilityPlacementStatus::InvalidInput, diagnostics)
    }

    fn infeasible(diagnostic: FacilityPlacementDiagnostic) -> Self {
        Self::failure(FacilityPlacementStatus::Infeasible, vec![diagnostic])
    }

    fn unknown(diagnostic: FacilityPlacementDiagnostic) -> Self {
        Self::failure(FacilityPlacementStatus::Unknown, vec![diagnostic])
    }

    fn failure(
        status: FacilityPlacementStatus,
        diagnostics: Vec<FacilityPlacementDiagnostic>,
    ) -> Self {
        Self {
            success: false,
            status,
            bounds: None,
            placements: Vec::new(),
            diagnostics,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FacilityPlacementDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub entity: Option<String>,
    pub message: String,
}

impl FacilityPlacementDiagnostic {
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

    fn info(
        code: &'static str,
        path: impl Into<String>,
        entity: Option<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            stage: STAGE,
            severity: "info",
            code,
            path: path.into(),
            entity,
            message: message.into(),
        }
    }
}

pub fn validate_facility_placement_request(
    request: &FacilityPlacementRequest,
) -> Vec<FacilityPlacementDiagnostic> {
    let mut diagnostics = Vec::new();

    if request.schema_version != SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION {
        diagnostics.push(FacilityPlacementDiagnostic::error(
            "unsupported-facility-placement-schema-version",
            "/schema_version",
            None,
            format!(
                "facility placement schema_version {} is unsupported; expected {}",
                request.schema_version, SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION
            ),
        ));
    }

    if request.max_width <= 0 {
        diagnostics.push(FacilityPlacementDiagnostic::error(
            "non-positive-layout-max-width",
            "/max_width",
            None,
            format!(
                "facility placement max_width must be positive, found {}",
                request.max_width
            ),
        ));
    }

    if request.max_height <= 0 {
        diagnostics.push(FacilityPlacementDiagnostic::error(
            "non-positive-layout-max-height",
            "/max_height",
            None,
            format!(
                "facility placement max_height must be positive, found {}",
                request.max_height
            ),
        ));
    }

    diagnostics
}

pub fn solve_facility_placement(
    instance_wiring: &FacilityInstanceWiringReport,
    catalog: &ValidatedFacilityCatalog,
    request: &FacilityPlacementRequest,
) -> FacilityPlacementReport {
    if !instance_wiring.success {
        return FacilityPlacementReport::invalid(FacilityPlacementDiagnostic::error(
            "upstream-instance-wiring-failed",
            "/",
            None,
            "facility placement requires successful facility instance wiring",
        ));
    }

    let request_diagnostics = validate_facility_placement_request(request);
    if !request_diagnostics.is_empty() {
        return FacilityPlacementReport::invalid_many(request_diagnostics);
    }

    let instances = match collect_instances(instance_wiring, catalog) {
        Ok(instances) => instances,
        Err(diagnostic) => return FacilityPlacementReport::invalid(diagnostic),
    };

    match solve_optimally(instances, request.max_width, request.max_height) {
        Ok(report) => report,
        Err(PlacementFailure::Invalid(diagnostic)) => FacilityPlacementReport::invalid(diagnostic),
        Err(PlacementFailure::Infeasible(diagnostic)) => {
            FacilityPlacementReport::infeasible(diagnostic)
        }
    }
}

#[derive(Debug, Clone)]
struct InstanceSpec {
    instance: String,
    recipe: String,
    facility: String,
    footprint: FacilityFootprint,
    allowed_rotations: Vec<i64>,
}

fn collect_instances(
    instance_wiring: &FacilityInstanceWiringReport,
    catalog: &ValidatedFacilityCatalog,
) -> Result<Vec<InstanceSpec>, FacilityPlacementDiagnostic> {
    let mut seen_instances = BTreeSet::new();
    let mut instances = Vec::new();

    for (node_index, node) in instance_wiring.nodes.iter().enumerate() {
        let FacilityInstanceWiringNode::Facility {
            id,
            recipe,
            facility,
            ..
        } = node
        else {
            continue;
        };

        if !seen_instances.insert(id.as_str()) {
            return Err(FacilityPlacementDiagnostic::error(
                "duplicate-facility-instance",
                format!("/nodes/{node_index}/id"),
                Some(id.clone()),
                format!("facility instance '{id}' appears more than once"),
            ));
        }

        let Some(definition) = catalog.facility(facility) else {
            return Err(FacilityPlacementDiagnostic::error(
                "missing-facility-definition",
                format!("/nodes/{node_index}/facility"),
                Some(facility.clone()),
                format!(
                    "facility instance '{id}' references facility '{facility}' which is absent from the validated catalog"
                ),
            ));
        };

        instances.push(InstanceSpec {
            instance: id.clone(),
            recipe: recipe.clone(),
            facility: facility.clone(),
            footprint: definition.footprint.clone(),
            allowed_rotations: definition.allowed_rotations.clone(),
        });
    }

    Ok(instances)
}

fn solve_optimally(
    mut instances: Vec<InstanceSpec>,
    max_width: i64,
    max_height: i64,
) -> Result<FacilityPlacementReport, PlacementFailure> {
    if instances.is_empty() {
        return Ok(FacilityPlacementReport::solved(
            FacilityPlacementStatus::Optimal,
            FacilityPlacementBounds {
                width: 0,
                height: 0,
            },
            Vec::new(),
            "facility-placement-optimal",
            "facility placement is proven to have minimum height",
        ));
    }

    instances.sort_by(|left, right| left.instance.cmp(&right.instance));
    let (mut solver, used_height, model_instances) =
        build_model(instances, max_width, max_height, 0)?;

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
        LinearSatUnsat::new(OptimisationDirection::Minimise, used_height, callback),
    );

    match result {
        OptimisationResult::Optimal(solution) => solved_report(
            &solution,
            &model_instances,
            FacilityPlacementStatus::Optimal,
            "facility-placement-optimal",
            "facility placement is proven to have minimum height",
        ),
        OptimisationResult::Satisfiable(solution) | OptimisationResult::Stopped(solution, ()) => {
            solved_report(
                &solution,
                &model_instances,
                FacilityPlacementStatus::Feasible,
                "facility-placement-feasible",
                "facility placement is feasible but not proven optimal",
            )
        }
        OptimisationResult::Unsatisfiable => Ok(FacilityPlacementReport::infeasible(
            FacilityPlacementDiagnostic::error(
                "facility-placement-infeasible",
                "/",
                None,
                "facility placement constraints are infeasible",
            ),
        )),
        OptimisationResult::Unknown => Ok(FacilityPlacementReport::unknown(
            FacilityPlacementDiagnostic::error(
                "facility-placement-unknown",
                "/",
                None,
                "facility placement solver terminated without a solution or proof",
            ),
        )),
    }
}

fn build_model(
    instances: Vec<InstanceSpec>,
    max_width: i64,
    max_height: i64,
    minimum_clearance: i64,
) -> Result<(Solver, DomainId, Vec<ModelInstance>), PlacementFailure> {
    let max_width = solver_i32(max_width, None, "layout max_width")?;
    let max_height = solver_i32(max_height, None, "layout max_height")?;
    let minimum_clearance = solver_i32(minimum_clearance, None, "minimum clearance")?;
    if minimum_clearance < 0 {
        return Err(PlacementFailure::Invalid(
            FacilityPlacementDiagnostic::error(
                "negative-placement-clearance",
                "/",
                None,
                format!(
                    "minimum placement clearance must be non-negative, found {minimum_clearance}"
                ),
            ),
        ));
    }
    let mut solver = Solver::default();
    let constraint_tag = solver.new_constraint_tag();
    let used_height = solver.new_named_bounded_integer(0, max_height, "used-height");
    let mut model_instances = Vec::with_capacity(instances.len());

    for instance in instances {
        let x = solver.new_named_bounded_integer(0, max_width, format!("{}-x", instance.instance));
        let y = solver.new_named_bounded_integer(0, max_height, format!("{}-y", instance.instance));
        let orientations = solver_orientations(&mut solver, &instance, max_width)?;
        post_exactly_one_orientation(&mut solver, &orientations, constraint_tag);

        for orientation in &orientations {
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![x.scaled(-1)],
                    0,
                    constraint_tag,
                ))
                .implied_by(orientation.literal);
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![x.scaled(1)],
                    max_width - orientation.width,
                    constraint_tag,
                ))
                .implied_by(orientation.literal);
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![y.scaled(-1)],
                    0,
                    constraint_tag,
                ))
                .implied_by(orientation.literal);
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![y.scaled(1)],
                    max_height - orientation.height,
                    constraint_tag,
                ))
                .implied_by(orientation.literal);
            solver
                .add_constraint(pumpkin_solver::less_than_or_equals(
                    vec![y.scaled(1), used_height.scaled(-1)],
                    -orientation.height,
                    constraint_tag,
                ))
                .implied_by(orientation.literal);
        }

        model_instances.push(ModelInstance {
            spec: instance,
            x,
            y,
            orientations,
        });
    }

    post_pairwise_non_overlap(
        &mut solver,
        &model_instances,
        minimum_clearance,
        constraint_tag,
    );
    Ok((solver, used_height, model_instances))
}

struct ModelInstance {
    spec: InstanceSpec,
    x: DomainId,
    y: DomainId,
    orientations: Vec<ModelOrientation>,
}

#[derive(Clone, Copy)]
struct ModelOrientation {
    rotation: i64,
    width: i32,
    height: i32,
    literal: Literal,
}

fn solver_orientations(
    solver: &mut Solver,
    instance: &InstanceSpec,
    max_width: i32,
) -> Result<Vec<ModelOrientation>, PlacementFailure> {
    let width = solver_i32(
        instance.footprint.width,
        Some(&instance.instance),
        "facility width",
    )?;
    let height = solver_i32(
        instance.footprint.height,
        Some(&instance.instance),
        "facility height",
    )?;
    let mut rotations = instance.allowed_rotations.clone();
    rotations.sort_unstable();

    let orientations = rotations
        .into_iter()
        .filter_map(|rotation| {
            let (oriented_width, oriented_height) = match rotation {
                90 | 270 => (height, width),
                _ => (width, height),
            };
            (oriented_width <= max_width).then(|| ModelOrientation {
                rotation,
                width: oriented_width,
                height: oriented_height,
                literal: solver.new_literal(),
            })
        })
        .collect::<Vec<_>>();

    if orientations.is_empty() {
        return Err(PlacementFailure::Infeasible(
            FacilityPlacementDiagnostic::error(
                "facility-does-not-fit-layout-width",
                "/max_width",
                Some(instance.instance.clone()),
                format!(
                    "facility instance '{}' has no allowed rotation that fits max_width {max_width}",
                    instance.instance
                ),
            ),
        ));
    }

    Ok(orientations)
}

fn post_exactly_one_orientation(
    solver: &mut Solver,
    orientations: &[ModelOrientation],
    constraint_tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    solver.add_clause(
        orientations
            .iter()
            .map(|orientation| orientation.literal.get_true_predicate()),
        constraint_tag,
    );

    for left_index in 0..orientations.len() {
        for right_index in (left_index + 1)..orientations.len() {
            solver.add_clause(
                [
                    orientations[left_index].literal.get_false_predicate(),
                    orientations[right_index].literal.get_false_predicate(),
                ],
                constraint_tag,
            );
        }
    }
}

fn post_pairwise_non_overlap(
    solver: &mut Solver,
    instances: &[ModelInstance],
    minimum_clearance: i32,
    constraint_tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for left_index in 0..instances.len() {
        for right_index in (left_index + 1)..instances.len() {
            let left = &instances[left_index];
            let right = &instances[right_index];

            for left_orientation in &left.orientations {
                for right_orientation in &right.orientations {
                    let left_of = reify_less_or_equal(
                        solver,
                        vec![left.x.scaled(1), right.x.scaled(-1)],
                        -left_orientation.width - minimum_clearance,
                        constraint_tag,
                    );
                    let right_of = reify_less_or_equal(
                        solver,
                        vec![right.x.scaled(1), left.x.scaled(-1)],
                        -right_orientation.width - minimum_clearance,
                        constraint_tag,
                    );
                    let below = reify_less_or_equal(
                        solver,
                        vec![left.y.scaled(1), right.y.scaled(-1)],
                        -left_orientation.height - minimum_clearance,
                        constraint_tag,
                    );
                    let above = reify_less_or_equal(
                        solver,
                        vec![right.y.scaled(1), left.y.scaled(-1)],
                        -right_orientation.height - minimum_clearance,
                        constraint_tag,
                    );

                    solver.add_clause(
                        [
                            left_orientation.literal.get_false_predicate(),
                            right_orientation.literal.get_false_predicate(),
                            left_of.get_true_predicate(),
                            right_of.get_true_predicate(),
                            below.get_true_predicate(),
                            above.get_true_predicate(),
                        ],
                        constraint_tag,
                    );
                }
            }
        }
    }
}

fn reify_less_or_equal(
    solver: &mut Solver,
    terms: Vec<pumpkin_solver::core::variables::AffineView<DomainId>>,
    rhs: i32,
    constraint_tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Literal {
    let literal = solver.new_literal();
    pumpkin_solver::less_than_or_equals(terms, rhs, constraint_tag).reify(solver, literal);
    literal
}

fn solved_report(
    solution: &impl ProblemSolution,
    instances: &[ModelInstance],
    status: FacilityPlacementStatus,
    code: &'static str,
    message: &'static str,
) -> Result<FacilityPlacementReport, PlacementFailure> {
    let mut placements = Vec::with_capacity(instances.len());
    let mut used_width = 0_i64;
    let mut used_height = 0_i64;

    for instance in instances {
        let orientation = instance
            .orientations
            .iter()
            .find(|orientation| solution.get_literal_value(orientation.literal))
            .ok_or_else(|| {
                PlacementFailure::Invalid(FacilityPlacementDiagnostic::error(
                    "solver-model-error",
                    "/placements",
                    Some(instance.spec.instance.clone()),
                    "solver solution has no selected facility rotation",
                ))
            })?;
        let x = i64::from(solution.get_integer_value(instance.x));
        let y = i64::from(solution.get_integer_value(instance.y));
        let width = i64::from(orientation.width);
        let height = i64::from(orientation.height);
        used_width = used_width.max(x + width);
        used_height = used_height.max(y + height);

        placements.push(FacilityPlacement {
            instance: instance.spec.instance.clone(),
            recipe: instance.spec.recipe.clone(),
            facility: instance.spec.facility.clone(),
            x,
            y,
            width,
            height,
            rotation: orientation.rotation,
        });
    }

    placements.sort_by(|left, right| left.instance.cmp(&right.instance));

    Ok(FacilityPlacementReport::solved(
        status,
        FacilityPlacementBounds {
            width: used_width,
            height: used_height,
        },
        placements,
        code,
        message,
    ))
}

fn solver_i32(value: i64, entity: Option<&str>, field: &str) -> Result<i32, PlacementFailure> {
    i32::try_from(value).map_err(|_| {
        PlacementFailure::Invalid(FacilityPlacementDiagnostic::error(
            "solver-domain-out-of-range",
            "/",
            entity.map(str::to_string),
            format!("{field} value {value} does not fit the solver's 32-bit integer domain"),
        ))
    })
}

enum PlacementFailure {
    Invalid(FacilityPlacementDiagnostic),
    Infeasible(FacilityPlacementDiagnostic),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{
        FacilityCatalog, FacilityDefinition, FacilityPortDefinition, FacilityPortDirection,
        FacilityPortEdge, FacilityPortPosition,
    };
    use crate::logistics::TransportKind;
    use crate::recipes::{FacilityInstanceWiringNode, Rate};

    fn request(max_width: i64) -> FacilityPlacementRequest {
        request_with_bounds(max_width, 100)
    }

    fn request_with_bounds(max_width: i64, max_height: i64) -> FacilityPlacementRequest {
        FacilityPlacementRequest {
            schema_version: SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
            max_width,
            max_height,
        }
    }

    fn catalog(
        footprint: FacilityFootprint,
        allowed_rotations: Vec<i64>,
    ) -> ValidatedFacilityCatalog {
        ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![FacilityDefinition {
                id: "assembler".to_string(),
                footprint,
                allowed_rotations,
                ports: Vec::new(),
            }],
        })
        .expect("test catalog should validate")
    }

    fn facility_node(id: &str) -> FacilityInstanceWiringNode {
        FacilityInstanceWiringNode::Facility {
            id: id.to_string(),
            recipe: "assemble-casing".to_string(),
            facility: "assembler".to_string(),
            index: 0,
            runs_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            work_seconds_per_second: Rate {
                numerator: 1,
                denominator: 1,
            },
            unused_capacity: Rate {
                numerator: 0,
                denominator: 1,
            },
        }
    }

    fn wiring(nodes: Vec<FacilityInstanceWiringNode>) -> FacilityInstanceWiringReport {
        FacilityInstanceWiringReport {
            schema_version: crate::recipes::FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: true,
            nodes,
            edges: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn rotates_facility_to_fit_requested_width() {
        let report = solve_facility_placement(
            &wiring(vec![facility_node("assemble-casing:0")]),
            &catalog(
                FacilityFootprint {
                    width: 4,
                    height: 2,
                },
                vec![0, 90],
            ),
            &request(3),
        );

        assert!(report.success);
        assert_eq!(report.status, FacilityPlacementStatus::Optimal);
        assert_eq!(
            report.bounds,
            Some(FacilityPlacementBounds {
                width: 2,
                height: 4
            })
        );
        assert_eq!(report.placements[0].rotation, 90);
        assert_eq!(
            (report.placements[0].width, report.placements[0].height),
            (2, 4)
        );
    }

    #[test]
    fn placement_does_not_reserve_outer_space_for_unused_ports() {
        let validated = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![FacilityDefinition {
                id: "assembler".to_string(),
                footprint: FacilityFootprint {
                    width: 2,
                    height: 2,
                },
                allowed_rotations: vec![0],
                ports: vec![FacilityPortDefinition {
                    id: "output".to_string(),
                    direction: FacilityPortDirection::Output,
                    transport: TransportKind::Belt,
                    position: FacilityPortPosition { x: 1, y: 0 },
                    edge: FacilityPortEdge::North,
                }],
            }],
        })
        .expect("test catalog should validate");
        let request = request_with_bounds(2, 2);

        let report = solve_facility_placement(
            &wiring(vec![facility_node("assemble-casing:0")]),
            &validated,
            &request,
        );

        assert!(report.success, "{report:?}");
        assert_eq!(report.placements[0].rotation, 0);
        assert_eq!(report.placements[0].y, 0);
        let projected = super::super::project_facility_ports(&report, &validated, &request);
        assert!(!projected.success);
        assert_eq!(
            projected.diagnostics[0].code,
            "facility-port-connection-out-of-bounds"
        );
    }

    #[test]
    fn creates_multiple_non_overlapping_shelves() {
        let report = solve_facility_placement(
            &wiring(vec![
                facility_node("assemble-casing:1"),
                facility_node("assemble-casing:0"),
            ]),
            &catalog(
                FacilityFootprint {
                    width: 3,
                    height: 2,
                },
                vec![0],
            ),
            &request(4),
        );

        assert!(report.success);
        assert_eq!(
            report.bounds,
            Some(FacilityPlacementBounds {
                width: 3,
                height: 4
            })
        );
        let mut y_coordinates = report
            .placements
            .iter()
            .map(|placement| placement.y)
            .collect::<Vec<_>>();
        y_coordinates.sort_unstable();
        assert_eq!(y_coordinates, vec![0, 2]);
        assert_eq!(report.placements[0].instance, "assemble-casing:0");
        assert_eq!(report.placements[1].instance, "assemble-casing:1");
    }

    #[test]
    fn produces_zero_bounds_for_empty_facility_layout() {
        let report = solve_facility_placement(
            &wiring(Vec::new()),
            &catalog(
                FacilityFootprint {
                    width: 3,
                    height: 2,
                },
                vec![0],
            ),
            &request(4),
        );

        assert!(report.success);
        assert_eq!(
            report.bounds,
            Some(FacilityPlacementBounds {
                width: 0,
                height: 0
            })
        );
        assert!(report.placements.is_empty());
    }

    #[test]
    fn rejects_missing_facility_definition() {
        let validated = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: Vec::new(),
        })
        .expect("empty catalog should validate");

        let report = solve_facility_placement(
            &wiring(vec![facility_node("assemble-casing:0")]),
            &validated,
            &request(4),
        );

        assert!(!report.success);
        assert_eq!(report.diagnostics[0].code, "missing-facility-definition");
    }

    #[test]
    fn rejects_failed_instance_wiring() {
        let failed_wiring = FacilityInstanceWiringReport {
            schema_version: crate::recipes::FACILITY_INSTANCE_WIRING_SCHEMA_VERSION,
            success: false,
            nodes: Vec::new(),
            edges: Vec::new(),
            diagnostics: Vec::new(),
        };

        let report = solve_facility_placement(
            &failed_wiring,
            &catalog(
                FacilityFootprint {
                    width: 3,
                    height: 2,
                },
                vec![0],
            ),
            &request(4),
        );

        assert!(!report.success);
        assert_eq!(report.status, FacilityPlacementStatus::InvalidInput);
        assert_eq!(
            report.diagnostics[0].code,
            "upstream-instance-wiring-failed"
        );
    }

    #[test]
    fn rejects_invalid_request_and_duplicate_instances() {
        let validated = catalog(
            FacilityFootprint {
                width: 3,
                height: 2,
            },
            vec![0],
        );
        let invalid_request = FacilityPlacementRequest {
            schema_version: 99,
            max_width: 0,
            max_height: 0,
        };
        let invalid_report = solve_facility_placement(
            &wiring(vec![facility_node("assemble-casing:0")]),
            &validated,
            &invalid_request,
        );

        assert!(!invalid_report.success);
        assert_eq!(invalid_report.diagnostics.len(), 3);

        let duplicate_report = solve_facility_placement(
            &wiring(vec![
                facility_node("assemble-casing:0"),
                facility_node("assemble-casing:0"),
            ]),
            &validated,
            &request(4),
        );
        assert_eq!(
            duplicate_report.diagnostics[0].code,
            "duplicate-facility-instance"
        );
    }

    #[test]
    fn rejects_facility_that_cannot_fit_width() {
        let report = solve_facility_placement(
            &wiring(vec![facility_node("assemble-casing:0")]),
            &catalog(
                FacilityFootprint {
                    width: 4,
                    height: 2,
                },
                vec![0, 180],
            ),
            &request(3),
        );

        assert!(!report.success);
        assert_eq!(report.status, FacilityPlacementStatus::Infeasible);
        assert_eq!(
            report.diagnostics[0].code,
            "facility-does-not-fit-layout-width"
        );
    }

    #[test]
    fn proves_layout_infeasible_when_height_bound_is_too_small() {
        let report = solve_facility_placement(
            &wiring(vec![
                facility_node("assemble-casing:0"),
                facility_node("assemble-casing:1"),
            ]),
            &catalog(
                FacilityFootprint {
                    width: 3,
                    height: 2,
                },
                vec![0],
            ),
            &request_with_bounds(3, 3),
        );

        assert!(!report.success);
        assert_eq!(report.status, FacilityPlacementStatus::Infeasible);
        assert_eq!(report.diagnostics[0].code, "facility-placement-infeasible");
    }

    #[test]
    fn rejects_bounds_outside_solver_integer_domain() {
        let report = solve_facility_placement(
            &wiring(vec![facility_node("assemble-casing:0")]),
            &catalog(
                FacilityFootprint {
                    width: 3,
                    height: 2,
                },
                vec![0],
            ),
            &request_with_bounds(i64::MAX, 10),
        );

        assert!(!report.success);
        assert_eq!(report.status, FacilityPlacementStatus::InvalidInput);
        assert_eq!(report.diagnostics[0].code, "solver-domain-out-of-range");
    }

    #[test]
    fn proves_height_lower_than_previous_shelf_solution() {
        let validated = ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 3,
            facilities: vec![
                FacilityDefinition {
                    id: "wide-tall".to_string(),
                    footprint: FacilityFootprint {
                        width: 6,
                        height: 4,
                    },
                    allowed_rotations: vec![0],
                    ports: Vec::new(),
                },
                FacilityDefinition {
                    id: "wide-short".to_string(),
                    footprint: FacilityFootprint {
                        width: 6,
                        height: 2,
                    },
                    allowed_rotations: vec![0],
                    ports: Vec::new(),
                },
                FacilityDefinition {
                    id: "narrow".to_string(),
                    footprint: FacilityFootprint {
                        width: 4,
                        height: 3,
                    },
                    allowed_rotations: vec![0],
                    ports: Vec::new(),
                },
            ],
        })
        .expect("test catalog should validate");
        let nodes = [
            ("facility-instance:a:0", "wide-tall"),
            ("facility-instance:b:0", "wide-short"),
            ("facility-instance:c:0", "narrow"),
            ("facility-instance:c:1", "narrow"),
        ]
        .into_iter()
        .map(|(id, facility)| {
            let mut node = facility_node(id);
            let FacilityInstanceWiringNode::Facility {
                facility: node_facility,
                ..
            } = &mut node
            else {
                unreachable!()
            };
            *node_facility = facility.to_string();
            node
        })
        .collect();

        let report =
            solve_facility_placement(&wiring(nodes), &validated, &request_with_bounds(10, 20));

        assert!(report.success);
        assert_eq!(report.status, FacilityPlacementStatus::Optimal);
        assert_eq!(report.bounds.expect("bounds should exist").height, 6);
    }

    #[test]
    fn rejects_unknown_request_fields() {
        let error = serde_json::from_str::<FacilityPlacementRequest>(
            r#"{ "schema_version": 2, "max_width": 12, "max_height": 8, "extra": true }"#,
        )
        .expect_err("unknown placement request fields should be rejected");

        assert!(error.to_string().contains("unknown field"));
    }
}
