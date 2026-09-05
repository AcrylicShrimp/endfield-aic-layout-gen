use super::*;
use std::collections::HashSet;

pub(in crate::layouts::constructive::production) fn blocked_boundaries(
    node: &ConstructiveNode,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
) -> Vec<String> {
    let mut blocked_requirements = Vec::new();
    let facilities = super::first_pipe_frontier::occupied_cells(&node.placements);
    let max_x = node
        .placements
        .iter()
        .map(|p| p.x + p.width)
        .chain(
            node.transport_networks
                .iter()
                .flat_map(|n| n.cells.iter().map(|p| p.x + 1)),
        )
        .max()
        .unwrap_or(0)
        + 3;
    let max_y = node
        .placements
        .iter()
        .map(|p| p.y + p.height)
        .chain(
            node.transport_networks
                .iter()
                .flat_map(|n| n.cells.iter().map(|p| p.y + 1)),
        )
        .max()
        .unwrap_or(0)
        + 3;
    let commodities = node
        .boundary_requirements
        .iter()
        .map(|b| (b.transport, b.item.clone()))
        .collect::<BTreeSet<_>>();
    for (transport, item) in commodities {
        let mut blocked = facilities.clone();
        blocked.extend(
            node.transport_networks
                .iter()
                .filter(|n| n.transport == transport)
                .flat_map(|n| n.cells.iter().map(|p| (p.x, p.y))),
        );
        let cap = transports.capacity(transport);
        let rate = Rate::from_quantity_per_duration_ms(cap.quantity, cap.duration_ms).unwrap();
        let crossings = network::crossings::options(node, transport, &item, rate, components);
        let mut seen = HashSet::from([(-1, -1)]);
        let mut queue = VecDeque::from([(-1, -1)]);
        while let Some((x, y)) = queue.pop_front() {
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                let mut p = (x + dx, y + dy);
                while crossings
                    .get(&p)
                    .is_some_and(|(horizontal, _)| *horizontal == (dy == 0))
                {
                    p = (p.0 + dx, p.1 + dy);
                }
                if p.0 >= -1
                    && p.1 >= -1
                    && p.0 <= max_x
                    && p.1 <= max_y
                    && !blocked.contains(&p)
                    && seen.insert(p)
                {
                    queue.push_back(p);
                }
            }
        }
        for b in node
            .boundary_requirements
            .iter()
            .filter(|b| b.transport == transport && b.item == item)
        {
            if !b.attachments.iter().any(|a| match a {
                ConstructiveBoundaryAttachment::NewPort { port } => {
                    seen.contains(&(port.connection.x, port.connection.y))
                }
                ConstructiveBoundaryAttachment::ExistingNetwork { branch_cell, .. } => {
                    seen.contains(&(branch_cell.x, branch_cell.y))
                }
            }) {
                blocked_requirements.push(b.requirement.clone());
            }
        }
    }
    blocked_requirements.sort();
    blocked_requirements.dedup();
    blocked_requirements
}
