use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::layouts::FacilityPlacement;

use super::IntegratedLayoutReport;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct LayoutScore {
    pub total_route_cells: usize,
    pub total_route_turns: usize,
    pub used_bounding_box_area: u64,
    pub maximum_used_side: i64,
    pub physical_transport_tiles: usize,
    pub logistics_component_count: usize,
    pub moved_prior_facility_count: usize,
    pub total_prior_facility_manhattan_displacement: u64,
    pub rotation_change_count: usize,
}

impl LayoutScore {
    pub fn from_report(
        report: &IntegratedLayoutReport,
        prior_reference: &[FacilityPlacement],
    ) -> Option<Self> {
        if !report.success {
            return None;
        }
        let bounds = report.bounds.as_ref()?;
        let prior_by_instance = prior_reference
            .iter()
            .map(|placement| (placement.instance.as_str(), placement))
            .collect::<BTreeMap<_, _>>();
        let mut moved_prior_facility_count = 0;
        let mut total_prior_facility_manhattan_displacement = 0_u64;
        let mut rotation_change_count = 0;
        for placement in &report.placements {
            let Some(prior) = prior_by_instance.get(placement.instance.as_str()) else {
                continue;
            };
            let displacement = placement.x.abs_diff(prior.x) + placement.y.abs_diff(prior.y);
            if displacement > 0 {
                moved_prior_facility_count += 1;
                total_prior_facility_manhattan_displacement += displacement;
            }
            rotation_change_count += usize::from(placement.rotation != prior.rotation);
        }
        let physical_transport_tiles = report
            .routes
            .iter()
            .flat_map(|route| {
                route
                    .cells
                    .iter()
                    .map(move |cell| (route.transport, cell.x, cell.y))
            })
            .collect::<BTreeSet<_>>()
            .len();
        Some(Self {
            total_route_cells: report.routes.iter().map(|route| route.cells.len()).sum(),
            total_route_turns: report.routes.iter().map(super::route_turn_count).sum(),
            used_bounding_box_area: u64::try_from(bounds.width)
                .ok()?
                .checked_mul(u64::try_from(bounds.height).ok()?)?,
            maximum_used_side: bounds.width.max(bounds.height),
            physical_transport_tiles,
            logistics_component_count: report.logistics_components.len(),
            moved_prior_facility_count,
            total_prior_facility_manhattan_displacement,
            rotation_change_count,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "kebab-case")]
pub enum RefinementKind {
    IncumbentExtension,
    GrowthNeighborhood,
    FinalGlobal,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct DeterministicCandidateKey {
    pub phase_index: usize,
    pub refinement_kind: RefinementKind,
    pub neighborhood_rank: usize,
    pub restart_index: usize,
    pub policy_index: usize,
    pub attempt_index: usize,
    pub yield_index: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CandidateRank {
    pub score: LayoutScore,
    pub deterministic_candidate_key: DeterministicCandidateKey,
}

#[cfg(test)]
mod tests {
    use crate::layouts::{
        FacilityPlacementBounds, IntegratedLayoutStatus, IntegratedRoute, IntegratedRouteEndpoint,
        RouteRequirementFingerprint, WorldGridPosition,
    };
    use crate::logistics::TransportKind;
    use crate::recipes::{FacilityInstanceWiringProjection, Rate};

    use super::*;

    #[test]
    fn score_is_lexicographic_in_the_documented_priority_order() {
        let base = LayoutScore {
            total_route_cells: 10,
            total_route_turns: 4,
            used_bounding_box_area: 100,
            maximum_used_side: 10,
            physical_transport_tiles: 9,
            logistics_component_count: 2,
            moved_prior_facility_count: 1,
            total_prior_facility_manhattan_displacement: 3,
            rotation_change_count: 1,
        };
        let mut longer = base;
        longer.total_route_cells += 1;
        let mut bendier = base;
        bendier.total_route_turns += 1;
        let mut larger = base;
        larger.used_bounding_box_area += 1;
        assert!(base < longer);
        assert!(base < bendier);
        assert!(base < larger);
        assert!(longer > bendier, "route length outranks every later field");
    }

    #[test]
    fn candidate_key_breaks_only_equal_score_ties() {
        let score = LayoutScore {
            total_route_cells: 1,
            total_route_turns: 0,
            used_bounding_box_area: 1,
            maximum_used_side: 1,
            physical_transport_tiles: 1,
            logistics_component_count: 0,
            moved_prior_facility_count: 0,
            total_prior_facility_manhattan_displacement: 0,
            rotation_change_count: 0,
        };
        let first = CandidateRank {
            score,
            deterministic_candidate_key: key(0),
        };
        let second = CandidateRank {
            score,
            deterministic_candidate_key: key(1),
        };
        assert!(first < second);
    }

    #[test]
    fn computes_unique_transport_tiles_and_fixed_reference_movement() {
        let prior = placement("facility", 2, 3, 0);
        let mut report = IntegratedLayoutReport {
            schema_version: super::super::INTEGRATED_LAYOUT_SCHEMA_VERSION,
            success: true,
            status: IntegratedLayoutStatus::Feasible,
            bounds: Some(FacilityPlacementBounds {
                width: 4,
                height: 5,
            }),
            placements: vec![placement("facility", 4, 4, 90), placement("new", 0, 0, 0)],
            logistics_components: Vec::new(),
            routes: vec![route(TransportKind::Belt), route(TransportKind::Belt)],
            phases: Vec::new(),
            diagnostics: Vec::new(),
        };
        report.routes.push(route(TransportKind::Pipe));

        let score = LayoutScore::from_report(&report, &[prior]).expect("report is scoreable");

        assert_eq!(score.total_route_cells, 6);
        assert_eq!(score.physical_transport_tiles, 4);
        assert_eq!(score.moved_prior_facility_count, 1);
        assert_eq!(score.total_prior_facility_manhattan_displacement, 3);
        assert_eq!(score.rotation_change_count, 1);
    }

    fn key(yield_index: usize) -> DeterministicCandidateKey {
        DeterministicCandidateKey {
            phase_index: 0,
            refinement_kind: RefinementKind::GrowthNeighborhood,
            neighborhood_rank: 0,
            restart_index: 0,
            policy_index: 0,
            attempt_index: 0,
            yield_index,
        }
    }

    fn placement(instance: &str, x: i64, y: i64, rotation: i64) -> FacilityPlacement {
        FacilityPlacement {
            instance: instance.to_string(),
            recipe: "recipe".to_string(),
            facility: "facility".to_string(),
            x,
            y,
            width: 1,
            height: 1,
            rotation,
        }
    }

    fn route(transport: TransportKind) -> IntegratedRoute {
        IntegratedRoute {
            requirement_id: format!("route:{transport:?}"),
            requirement_fingerprint: RouteRequirementFingerprint {
                source: "source".to_string(),
                target: "target".to_string(),
                item: "item".to_string(),
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
                transport,
                projection: FacilityInstanceWiringProjection::Original,
            },
            source: IntegratedRouteEndpoint::External {
                node: "source".to_string(),
                side: crate::facilities::FacilityPortEdge::East,
            },
            target: IntegratedRouteEndpoint::External {
                node: "target".to_string(),
                side: crate::facilities::FacilityPortEdge::West,
            },
            item: "item".to_string(),
            rate: Rate {
                numerator: 1,
                denominator: 1,
            },
            transport,
            cells: vec![
                WorldGridPosition { x: 0, y: 0 },
                WorldGridPosition { x: 1, y: 0 },
            ],
        }
    }
}
