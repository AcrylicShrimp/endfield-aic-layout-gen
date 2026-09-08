//! Catalog-backed, independent-axis crossings with independent commodity channels.
use super::*;

pub(in crate::layouts::constructive) fn options(
    node: &ConstructiveNode,
    transport: TransportKind,
    rate: Rate,
    components: &ValidatedLogisticsComponentCatalog,
) -> BTreeMap<(i64, i64), (bool, PlacedLogisticsComponent)> {
    let _span = profile_span(ProfileOp::CrossingOptions);
    let Some(def) = components.catalog().components.iter().find(|d| {
        d.kind == LogisticsComponentKind::Bridge
            && d.transport == transport
            && d.footprint.width == 1
            && d.footprint.height == 1
            && d.input_directions.len() == 4
            && d.output_directions.len() == 4
            && component_capacity(d).is_some_and(|cap| cap >= rate)
    }) else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::new();
    for n in node
        .transport_networks
        .iter()
        .filter(|n| n.transport == transport)
    {
        for p in &n.cells {
            if n.terminals.iter().any(|t| t.position == *p)
                || node
                    .logistics_components
                    .iter()
                    .any(|c| c.transport == transport && c.position == *p)
            {
                continue;
            }
            let (inputs, outputs, load) = arms(n, p);
            if inputs.len() != 1
                || outputs.len() != 1
                || rotate(inputs[0], 180) != outputs[0]
                || component_capacity(def).is_none_or(|cap| cap < load)
            {
                continue;
            }
            let horizontal = matches!(
                inputs[0],
                CardinalDirection::North | CardinalDirection::South
            );
            result.insert(
                xy(p),
                (
                    horizontal,
                    PlacedLogisticsComponent {
                        id: format!(
                            "crossing:{transport:?}:{}:{}:{}",
                            node.logistics_components.len(),
                            p.x,
                            p.y
                        ),
                        component: def.id.clone(),
                        kind: LogisticsComponentKind::Bridge,
                        transport,
                        position: p.clone(),
                        rotation: def.allowed_rotations[0],
                    },
                ),
            );
        }
    }
    result
}

pub(in crate::layouts::constructive) fn install(
    node: &mut ConstructiveNode,
    transport: TransportKind,
    item: &str,
    path: &[WorldGridPosition],
    choices: &BTreeMap<(i64, i64), (bool, PlacedLogisticsComponent)>,
) {
    for p in path {
        let Some((_, component)) = choices.get(&xy(p)) else {
            continue;
        };
        for n in node
            .transport_networks
            .iter_mut()
            .filter(|n| n.transport == transport && (n.item == item || n.cells.contains(p)))
        {
            n.component_ids.push(component.id.clone());
            n.component_ids.sort();
            n.component_ids.dedup();
        }
        node.logistics_components.push(component.clone());
    }
}
