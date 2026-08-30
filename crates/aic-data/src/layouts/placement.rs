use std::cmp::Reverse;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::facilities::{FacilityFootprint, ValidatedFacilityCatalog};
use crate::recipes::{FacilityInstanceWiringNode, FacilityInstanceWiringReport};

const STAGE: &str = "facility-placement";

pub const SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FacilityPlacementRequest {
    pub schema_version: u32,
    pub max_width: i64,
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
    fn feasible(bounds: FacilityPlacementBounds, placements: Vec<FacilityPlacement>) -> Self {
        Self {
            success: true,
            status: FacilityPlacementStatus::Feasible,
            bounds: Some(bounds),
            placements,
            diagnostics: vec![FacilityPlacementDiagnostic::info(
                "facility-placement-feasible",
                "/",
                None,
                "facility placement is feasible but not proven optimal",
            )],
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

    match solve_with_shelves(instances, request.max_width) {
        Ok((bounds, placements)) => FacilityPlacementReport::feasible(bounds, placements),
        Err(PlacementFailure::Invalid(diagnostic)) => FacilityPlacementReport::invalid(diagnostic),
        Err(PlacementFailure::Infeasible(diagnostic)) => {
            FacilityPlacementReport::infeasible(diagnostic)
        }
    }
}

#[derive(Debug)]
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

#[derive(Debug, Clone, Copy)]
struct Orientation {
    rotation: i64,
    width: i64,
    height: i64,
}

#[derive(Debug)]
struct Shelf {
    y: i64,
    height: i64,
    used_width: i64,
}

fn solve_with_shelves(
    mut instances: Vec<InstanceSpec>,
    max_width: i64,
) -> Result<(FacilityPlacementBounds, Vec<FacilityPlacement>), PlacementFailure> {
    instances.sort_by_key(|instance| {
        (
            Reverse(instance.footprint.width.max(instance.footprint.height)),
            Reverse(i128::from(instance.footprint.width) * i128::from(instance.footprint.height)),
            instance.instance.clone(),
        )
    });

    let mut shelves = Vec::<Shelf>::new();
    let mut placements = Vec::with_capacity(instances.len());
    let mut next_shelf_y = 0_i64;
    let mut used_width = 0_i64;

    for instance in instances {
        let orientations = orientations(&instance);
        let mut selected = None;

        for (shelf_index, shelf) in shelves.iter().enumerate() {
            let orientation = orientations
                .iter()
                .copied()
                .filter(|orientation| orientation.height <= shelf.height)
                .filter_map(|orientation| {
                    let right = shelf.used_width.checked_add(orientation.width)?;
                    (right <= max_width).then_some((max_width - right, orientation))
                })
                .min_by_key(|(remaining_width, orientation)| {
                    (*remaining_width, orientation.height, orientation.rotation)
                });

            if let Some((_, orientation)) = orientation {
                selected = Some((shelf_index, orientation));
                break;
            }
        }

        let (shelf_index, orientation) = match selected {
            Some(selected) => selected,
            None => {
                let Some(orientation) = orientations
                    .iter()
                    .copied()
                    .filter(|orientation| orientation.width <= max_width)
                    .min_by_key(|orientation| {
                        (orientation.height, orientation.width, orientation.rotation)
                    })
                else {
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
                };

                let shelf_index = shelves.len();
                shelves.push(Shelf {
                    y: next_shelf_y,
                    height: orientation.height,
                    used_width: 0,
                });
                next_shelf_y = next_shelf_y
                    .checked_add(orientation.height)
                    .ok_or_else(|| {
                        PlacementFailure::Invalid(placement_overflow(
                            &instance.instance,
                            "layout height overflowed",
                        ))
                    })?;
                (shelf_index, orientation)
            }
        };

        let shelf = &mut shelves[shelf_index];
        let x = shelf.used_width;
        shelf.used_width = shelf
            .used_width
            .checked_add(orientation.width)
            .ok_or_else(|| {
                PlacementFailure::Invalid(placement_overflow(
                    &instance.instance,
                    "layout width overflowed",
                ))
            })?;
        used_width = used_width.max(shelf.used_width);

        placements.push(FacilityPlacement {
            instance: instance.instance,
            recipe: instance.recipe,
            facility: instance.facility,
            x,
            y: shelf.y,
            width: orientation.width,
            height: orientation.height,
            rotation: orientation.rotation,
        });
    }

    placements.sort_by(|left, right| left.instance.cmp(&right.instance));

    Ok((
        FacilityPlacementBounds {
            width: used_width,
            height: next_shelf_y,
        },
        placements,
    ))
}

fn orientations(instance: &InstanceSpec) -> Vec<Orientation> {
    let mut rotations = instance.allowed_rotations.clone();
    rotations.sort_unstable();

    rotations
        .into_iter()
        .map(|rotation| {
            let (width, height) = match rotation {
                90 | 270 => (instance.footprint.height, instance.footprint.width),
                _ => (instance.footprint.width, instance.footprint.height),
            };
            Orientation {
                rotation,
                width,
                height,
            }
        })
        .collect()
}

fn placement_overflow(entity: &str, message: &str) -> FacilityPlacementDiagnostic {
    FacilityPlacementDiagnostic::error(
        "placement-arithmetic-overflow",
        "/placements",
        Some(entity.to_string()),
        message,
    )
}

enum PlacementFailure {
    Invalid(FacilityPlacementDiagnostic),
    Infeasible(FacilityPlacementDiagnostic),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facilities::{FacilityCatalog, FacilityDefinition};
    use crate::recipes::{FacilityInstanceWiringNode, Rate};

    fn request(max_width: i64) -> FacilityPlacementRequest {
        FacilityPlacementRequest {
            schema_version: SUPPORTED_FACILITY_PLACEMENT_SCHEMA_VERSION,
            max_width,
        }
    }

    fn catalog(
        footprint: FacilityFootprint,
        allowed_rotations: Vec<i64>,
    ) -> ValidatedFacilityCatalog {
        ValidatedFacilityCatalog::try_from_catalog(FacilityCatalog {
            schema_version: 2,
            facilities: vec![FacilityDefinition {
                id: "assembler".to_string(),
                footprint,
                allowed_rotations,
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
        assert_eq!(report.status, FacilityPlacementStatus::Feasible);
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
        assert_eq!(report.placements[0].y, 0);
        assert_eq!(report.placements[1].y, 2);
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
            schema_version: 2,
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
        };
        let invalid_report = solve_facility_placement(
            &wiring(vec![facility_node("assemble-casing:0")]),
            &validated,
            &invalid_request,
        );

        assert!(!invalid_report.success);
        assert_eq!(invalid_report.diagnostics.len(), 2);

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
    fn rejects_unknown_request_fields() {
        let error = serde_json::from_str::<FacilityPlacementRequest>(
            r#"{ "schema_version": 1, "max_width": 12, "extra": true }"#,
        )
        .expect_err("unknown placement request fields should be rejected");

        assert!(error.to_string().contains("unknown field"));
    }
}
