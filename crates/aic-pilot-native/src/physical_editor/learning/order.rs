//! Admission-only physical factor order; opaque names remain lookup keys.

use std::collections::BTreeMap;

use aic_layout::{
    editor::{CompiledProblemV1, CompiledTerminal, PhysicalDraftV1},
    facilities::FacilityPortDirection,
    projection::{self, Port},
    rate::Rate,
};

use super::{Facility, view};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PortKey {
    physical: [i64; 5],
    original_membership: Vec<bool>,
}

fn physical(port: &Port) -> [i64; 5] {
    [
        port.position.x,
        port.position.y,
        projection::edge_index(port.edge) as i64,
        i64::from(port.direction == FacilityPortDirection::Output),
        view::transport(port.transport) as i64,
    ]
}

fn equivalences<K: Eq>(keys: &[K]) -> Vec<usize> {
    let mut class = 0;
    keys.iter()
        .enumerate()
        .map(|(index, key)| {
            if index > 0 && key != &keys[index - 1] {
                class += 1;
            }
            class
        })
        .collect()
}

fn terminal_key(port: &Port, rate: Rate, incidence: &mut [Vec<i64>]) -> Vec<i64> {
    let mut key = physical(port).to_vec();
    key.extend([rate.numerator, rate.denominator]);
    incidence.sort();
    for row in incidence {
        key.push(row.len() as i64);
        key.extend_from_slice(row);
    }
    key
}

pub(super) fn configure(
    problem: &CompiledProblemV1,
    initial: &PhysicalDraftV1,
    facilities: &mut [Facility],
) {
    let base: Vec<_> = problem
        .blocks
        .iter()
        .map(|block| {
            let definition = super::facility(problem, &block.facility);
            projection::rotate(definition, 0, aic_layout::geometry::Point::default()).2
        })
        .collect();
    let ports: Vec<BTreeMap<_, _>> = base
        .iter()
        .map(|rows| rows.iter().map(|port| (port.id.as_str(), port)).collect())
        .collect();
    let terminal_owner: Vec<_> = problem
        .nodes
        .iter()
        .map(|node| match &node.terminal {
            Some(CompiledTerminal::Facility { block, .. }) => Some(*block),
            _ => None,
        })
        .collect();
    let original = |node: usize| match problem.nodes[node].terminal.as_ref().unwrap() {
        CompiledTerminal::Facility { original_port, .. } => original_port.as_str(),
        CompiledTerminal::External { .. } => unreachable!(),
    };
    let mut incidence = vec![Vec::<Vec<i64>>::new(); problem.nodes.len()];
    for edge in &problem.edges {
        let headings = [edge.first, edge.last];
        for (side, node_id) in [edge.source, edge.target].into_iter().enumerate() {
            let Some(owner) = terminal_owner[node_id] else {
                continue;
            };
            let placement = initial.poses[owner];
            let mut row = vec![
                side as i64,
                edge.rate.numerator,
                edge.rate.denominator,
                i64::from(edge.zero_length),
            ];
            for (other_side, endpoint) in [edge.source, edge.target].into_iter().enumerate() {
                let node = &problem.nodes[endpoint];
                let other = terminal_owner[endpoint].map(|index| &problem.blocks[index]);
                let baseline = super::super::endpoint_rule(
                    headings[other_side],
                    other.map(|value| (value.baseline_rotation, value.baseline_rotation, 0)),
                    other_side,
                    false,
                )
                .1;
                row.extend([
                    node.baseline_position.x - placement.position.x,
                    node.baseline_position.y - placement.position.y,
                    baseline.map_or(-1, |direction| direction as i64),
                ]);
            }
            incidence[node_id].push(row);
        }
    }
    let mut offset = 0;
    for (owner, facility) in facilities.iter_mut().enumerate() {
        let mut terminal_keys = BTreeMap::new();
        for group in &facility.groups {
            let rate = match problem.nodes[group.node].terminal.as_ref().unwrap() {
                CompiledTerminal::Facility { rate, .. } => *rate,
                CompiledTerminal::External { .. } => unreachable!(),
            };
            terminal_keys.insert(
                group.node,
                terminal_key(
                    ports[owner][original(group.node)],
                    rate,
                    &mut incidence[group.node],
                ),
            );
        }
        debug_assert!(
            facility
                .groups
                .windows(2)
                .all(|pair| terminal_keys[&pair[0].node] <= terminal_keys[&pair[1].node])
        );
        let classes = equivalences(
            &facility
                .groups
                .iter()
                .map(|group| &terminal_keys[&group.node])
                .collect::<Vec<_>>(),
        );
        let keys: BTreeMap<_, _> = base[owner]
            .iter()
            .map(|port| {
                (
                    port.id.as_str(),
                    PortKey {
                        physical: physical(port),
                        original_membership: facility
                            .groups
                            .iter()
                            .map(|group| original(group.node) == port.id)
                            .collect(),
                    },
                )
            })
            .collect();
        for (group, class) in facility.groups.iter_mut().zip(classes) {
            group.equivalence = class;
            debug_assert!(
                group
                    .ports
                    .windows(2)
                    .all(|pair| keys[pair[0].as_str()] <= keys[pair[1].as_str()])
            );
            group.port_equivalences = equivalences(
                &group
                    .ports
                    .iter()
                    .map(|port| &keys[port.as_str()])
                    .collect::<Vec<_>>(),
            );
            for &rotation in &facility.rotations {
                group
                    .features
                    .insert(rotation, (offset, offset + group.ports.len()));
                offset += group.ports.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalence_classes_change_only_at_distinct_ordered_keys() {
        assert_eq!(equivalences(&[1, 1, 2, 4, 4]), vec![0, 0, 1, 2, 2]);
    }
}
