use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::layouts::FacilityPlacement;

use super::IntegratedLayoutReport;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct LayoutScore {
    pub used_bounding_box_area: u64,
    pub physical_transport_tiles: usize,
    pub total_route_turns: usize,
    pub maximum_used_side: i64,
    pub logistics_component_count: usize,
    pub moved_prior_facility_count: usize,
    pub total_prior_facility_manhattan_displacement: u64,
    pub rotation_change_count: usize,
    pub total_route_cells: usize,
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
            .transport_networks
            .iter()
            .flat_map(|network| {
                network
                    .cells
                    .iter()
                    .map(move |cell| (network.transport, cell.x, cell.y))
            })
            .collect::<BTreeSet<_>>()
            .len();
        Some(Self {
            used_bounding_box_area: u64::try_from(bounds.width)
                .ok()?
                .checked_mul(u64::try_from(bounds.height).ok()?)?,
            physical_transport_tiles,
            total_route_turns: report
                .transport_networks
                .iter()
                .map(network_turn_count)
                .sum(),
            maximum_used_side: bounds.width.max(bounds.height),
            logistics_component_count: report.logistics_components.len(),
            moved_prior_facility_count,
            total_prior_facility_manhattan_displacement,
            rotation_change_count,
            total_route_cells: report
                .transport_networks
                .iter()
                .map(|network| network.cells.len())
                .sum(),
        })
    }
}

fn network_turn_count(network: &super::TransportNetwork) -> usize {
    network
        .cells
        .iter()
        .filter(|cell| {
            let incoming = network
                .segments
                .iter()
                .filter(|segment| segment.to == **cell)
                .collect::<Vec<_>>();
            let outgoing = network
                .segments
                .iter()
                .filter(|segment| segment.from == **cell)
                .collect::<Vec<_>>();
            if incoming.len() != 1 || outgoing.len() != 1 {
                return false;
            }
            let incoming_direction = (cell.x - incoming[0].from.x, cell.y - incoming[0].from.y);
            let outgoing_direction = (outgoing[0].to.x - cell.x, outgoing[0].to.y - cell.y);
            incoming_direction != outgoing_direction
        })
        .count()
}

#[cfg(test)]
mod tests {
    use crate::layouts::{
        FacilityPlacementBounds, IntegratedLayoutStatus, TransportNetwork, TransportNetworkSegment,
        WorldGridPosition,
    };
    use crate::logistics::TransportKind;
    use crate::recipes::Rate;

    use super::*;

    #[test]
    fn score_is_lexicographic_in_the_documented_priority_order() {
        let base = LayoutScore {
            used_bounding_box_area: 100,
            physical_transport_tiles: 9,
            total_route_turns: 4,
            maximum_used_side: 10,
            logistics_component_count: 2,
            moved_prior_facility_count: 1,
            total_prior_facility_manhattan_displacement: 3,
            rotation_change_count: 1,
            total_route_cells: 10,
        };
        let mut more_tiles = base;
        more_tiles.physical_transport_tiles += 1;
        let mut bendier = base;
        bendier.total_route_turns += 1;
        let mut larger = base;
        larger.used_bounding_box_area += 1;
        assert!(base < more_tiles);
        assert!(base < bendier);
        assert!(base < larger);
        assert!(larger > more_tiles, "used area outranks every later field");
        assert!(more_tiles > bendier, "physical tiles outrank route turns");
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
            transport_networks: vec![network(TransportKind::Belt), network(TransportKind::Belt)],
            phases: Vec::new(),
            exact: None,
            diagnostics: Vec::new(),
        };
        report.transport_networks.push(network(TransportKind::Pipe));

        let score = LayoutScore::from_report(&report, &[prior]).expect("report is scoreable");

        assert_eq!(score.total_route_cells, 6);
        assert_eq!(score.physical_transport_tiles, 4);
        assert_eq!(score.moved_prior_facility_count, 1);
        assert_eq!(score.total_prior_facility_manhattan_displacement, 3);
        assert_eq!(score.rotation_change_count, 1);
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

    fn network(transport: TransportKind) -> TransportNetwork {
        TransportNetwork {
            id: format!("network:{transport:?}:item"),
            requirement_ids: vec![format!("requirement:{transport:?}")],
            item: "item".to_string(),
            transport,
            cells: vec![
                WorldGridPosition { x: 0, y: 0 },
                WorldGridPosition { x: 1, y: 0 },
            ],
            segments: vec![TransportNetworkSegment {
                from: WorldGridPosition { x: 0, y: 0 },
                to: WorldGridPosition { x: 1, y: 0 },
                rate: Rate {
                    numerator: 1,
                    denominator: 1,
                },
            }],
            terminals: Vec::new(),
            component_ids: Vec::new(),
        }
    }
}
