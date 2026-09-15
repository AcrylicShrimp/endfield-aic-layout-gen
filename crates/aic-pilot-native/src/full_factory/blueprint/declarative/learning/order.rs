//! Admission-only physical factor order; opaque names remain exact lookup keys.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PortKey {
    physical: [i64; 5],
    // Identity only through incidence membership, never through a name/hash.
    original_membership: Vec<bool>,
}
fn physical(p: &catalog::Port) -> [i64; 5] {
    [
        p.position.x,
        p.position.y,
        catalog::edge_index(p.edge) as i64,
        i64::from(p.direction == FacilityPortDirection::Output),
        view::transport(p.transport) as i64,
    ]
}
fn equivalences<K: Eq>(keys: &[K]) -> Vec<usize> {
    let mut class = 0;
    keys.iter()
        .enumerate()
        .map(|(i, key)| {
            if i > 0 && key != &keys[i - 1] {
                class += 1;
            }
            class
        })
        .collect()
}

fn terminal_key(port: &catalog::Port, rate: Rate, incidence: &mut [Vec<i64>]) -> Vec<i64> {
    let mut key = physical(port).to_vec();
    key.extend([rate.numerator, rate.denominator]);
    incidence.sort();
    for row in incidence {
        key.push(row.len() as i64);
        key.extend_from_slice(row);
    }
    key
}
pub(super) fn configure(session: &Session, facilities: &mut [Facility]) {
    let bp = &session.blueprint;
    let owners: BTreeMap<_, _> = bp
        .facilities
        .iter()
        .enumerate()
        .map(|(owner, p)| (p.instance.as_str(), owner))
        .collect();
    let base: Vec<_> = bp
        .facilities
        .iter()
        .map(|p| {
            let definition = session
                .catalogs
                .data
                .facilities
                .facility(&p.facility)
                .unwrap();
            catalog::rotate(definition, 0, Point::default()).2
        })
        .collect();
    let ports: Vec<BTreeMap<_, _>> = base
        .iter()
        .map(|ps| ps.iter().map(|p| (p.id.as_str(), p)).collect())
        .collect();
    let terminal_owner: Vec<_> = bp
        .nodes
        .iter()
        .map(|n| {
            n.terminal.as_ref().and_then(|t| match &t.endpoint {
                TransportNetworkEndpoint::Facility { instance, .. } => {
                    Some(owners[instance.as_str()])
                }
                _ => None,
            })
        })
        .collect();
    let original = |id: usize| match &bp.nodes[id].terminal.as_ref().unwrap().endpoint {
        TransportNetworkEndpoint::Facility { port, .. } => port.as_str(),
        _ => unreachable!(),
    };
    // Build immutable directed adjacency once, outside observation/sampling.
    let mut incidence = vec![Vec::<Vec<i64>>::new(); bp.nodes.len()];
    for edge in &bp.edges {
        let headings = [edge.first, edge.last];
        for (side, id) in [edge.source, edge.target].into_iter().enumerate() {
            let Some(owner) = terminal_owner[id] else {
                continue;
            };
            let placement = &bp.facilities[owner];
            let mut row = vec![
                side as i64,
                edge.rate.numerator,
                edge.rate.denominator,
                i64::from(edge.zero_length),
            ];
            for (j, endpoint) in [edge.source, edge.target].into_iter().enumerate() {
                let node = &bp.nodes[endpoint];
                let facility = terminal_owner[endpoint].map(|i| &bp.facilities[i]);
                let baseline = crate::physical_editor::endpoint_rule(
                    headings[j],
                    facility.map(|p| (p.rotation, p.rotation, 0)),
                    j,
                    false,
                )
                .1;
                row.extend([
                    node.initial_position.x - placement.x,
                    node.initial_position.y - placement.y,
                    baseline.map_or(-1, |d| d as i64),
                ]);
            }
            incidence[id].push(row);
        }
    }
    let mut offset = 0;
    for (owner, facility) in facilities.iter_mut().enumerate() {
        let groups = &mut facility.groups;
        let mut terminal_keys = BTreeMap::new();
        for g in groups.iter() {
            let terminal = bp.nodes[g.node].terminal.as_ref().unwrap();
            terminal_keys.insert(
                g.node,
                terminal_key(
                    ports[owner][original(g.node)],
                    terminal.rate,
                    &mut incidence[g.node],
                ),
            );
        }
        debug_assert!(
            groups
                .windows(2)
                .all(|pair| terminal_keys[&pair[0].node] <= terminal_keys[&pair[1].node])
        );
        let classes = equivalences(
            &groups
                .iter()
                .map(|g| &terminal_keys[&g.node])
                .collect::<Vec<_>>(),
        );
        let keys: BTreeMap<_, _> = base[owner]
            .iter()
            .map(|p| {
                (
                    p.id.as_str(),
                    PortKey {
                        physical: physical(p),
                        original_membership: groups
                            .iter()
                            .map(|g| original(g.node) == p.id)
                            .collect(),
                    },
                )
            })
            .collect();
        for (g, class) in groups.iter_mut().zip(classes) {
            g.equivalence = class;
            debug_assert!(
                g.ports
                    .windows(2)
                    .all(|pair| keys[pair[0].as_str()] <= keys[pair[1].as_str()])
            );
            g.port_equivalences = equivalences(
                &g.ports
                    .iter()
                    .map(|p| &keys[p.as_str()])
                    .collect::<Vec<_>>(),
            );
            for &rotation in &facility.rotations {
                g.features
                    .insert(rotation, (offset, offset + g.ports.len()));
                offset += g.ports.len();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identical_geometry_does_not_merge_original_membership_or_baseline_rules() {
        let physical = [1, 0, 3, 0, 0];
        let a = PortKey {
            physical,
            original_membership: vec![true, false],
        };
        let b = PortKey {
            physical,
            original_membership: vec![false, true],
        };
        let c = PortKey {
            physical,
            original_membership: vec![false, false],
        };
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_eq!(equivalences(&[c.clone(), c, a, b]), vec![0, 0, 1, 2]);
        let session = super::super::super::tests::session();
        let definition = session
            .catalogs
            .data
            .facilities
            .facility(&session.blueprint.facilities[0].facility)
            .unwrap();
        let port = &catalog::rotate(definition, 0, Point::default()).2[0];
        let rate = session.blueprint.edges[0].rate;
        let baseline = |heading| {
            crate::physical_editor::endpoint_rule(Some(heading), Some((90, 0, 0)), 0, false)
                .1
                .unwrap() as i64
        };
        let first = terminal_key(port, rate, &mut [vec![0, 1, 1, baseline(1)]]);
        let second = terminal_key(port, rate, &mut [vec![0, 1, 1, baseline(3)]]);
        assert_eq!(equivalences(&[first, second]), vec![0, 1]);
    }
}
