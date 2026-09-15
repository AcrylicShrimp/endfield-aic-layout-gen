//! Typed witness-to-identity projection, shared by all learning admission paths.
use aic_data::{
    facilities::FacilityPortDirection,
    layouts::{
        FactoryEditCatalogs, FactoryEditRequest, TransportNetworkEndpoint, WitnessProbeInput,
    },
    logistics::{LogisticsComponentKind, TransportKind},
    recipes::Rate,
};
pub use aic_layout::physical_identity::{Limits, PhysicalIdentity, SEMANTICS};
use aic_layout::{
    geometry::Point,
    physical_identity::{Graph, Kind as K, Relation as R},
    projection::{cardinal_index, edge_index, rotate},
};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, String>;
#[cfg(test)]
mod tests;
fn medium(t: TransportKind) -> i64 {
    i64::from(t == TransportKind::Pipe)
}
fn direction(d: FacilityPortDirection) -> i64 {
    i64::from(d == FacilityPortDirection::Output)
}
fn rate(r: Rate) -> Result<Vec<i64>> {
    if r.denominator <= 0 || r.numerator < 0 {
        return Err("aic-learning.physical-identity-rate: invalid rational rate".into());
    }
    let (mut a, mut b) = (r.numerator, r.denominator);
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Ok(vec![r.numerator / a, r.denominator / a])
}
fn capacity(c: &aic_data::logistics::TransportCapacity) -> Result<Vec<i64>> {
    rate(Rate::from_quantity_per_duration_ms(c.quantity, c.duration_ms).map_err(|e| e.message)?)
}
fn rotations(values: &[i64]) -> i64 {
    values.iter().fold(0, |mask, &r| mask | (1 << (r / 90)))
}
fn get<K: Ord, V: Copy>(map: &BTreeMap<K, V>, key: &K, entity: &str) -> Result<V> {
    map.get(key)
        .copied()
        .ok_or_else(|| format!("aic-learning.physical-identity-reference: missing {entity}"))
}

/// Validated catalog handle is shared. Call after actual layout validation; no source
/// data is rewritten by normalization. No model/float representation participates.
pub fn identify(
    witness: &WitnessProbeInput,
    catalogs: &FactoryEditCatalogs,
    limits: &Limits,
    entity: &str,
) -> Result<(PhysicalIdentity, u64)> {
    let graph = graph(witness, catalogs).map_err(|e| format!("{e}; input={entity}"))?;
    aic_layout::physical_identity::identities(&graph, limits, entity).map_err(|e| e.to_string())
}

/// External actual-input admission normalizes with checked wide arithmetic BEFORE
/// factory validation can perform world-space port/footprint arithmetic.
pub fn validate_and_identify(
    request: &FactoryEditRequest,
    witness: &WitnessProbeInput,
    catalogs: &FactoryEditCatalogs,
    limits: &Limits,
    entity: &str,
) -> Result<PhysicalIdentity> {
    let working = working(witness, catalogs).map_err(|e| format!("{e}; input={entity}"))?;
    let validation = aic_data::layouts::validate_factory_layout(request, &working, catalogs);
    if !validation.factory_validated {
        return Err(format!(
            "aic-learning.physical-identity-input: {entity}: {}",
            serde_json::to_string(&validation.diagnostics).unwrap()
        ));
    }
    let graph = graph_working(&working, catalogs).map_err(|e| format!("{e}; input={entity}"))?;
    aic_layout::physical_identity::identities(&graph, limits, entity)
        .map(|(keys, _)| keys)
        .map_err(|e| e.to_string())
}

fn working(
    witness: &WitnessProbeInput,
    catalogs: &FactoryEditCatalogs,
) -> Result<WitnessProbeInput> {
    crate::full_factory::blueprint::declarative::frame::working_witness(witness, catalogs)
        .map_err(|e| serde_json::to_string(&e).unwrap())
}

fn graph(witness: &WitnessProbeInput, catalogs: &FactoryEditCatalogs) -> Result<Graph> {
    graph_working(&working(witness, catalogs)?, catalogs)
}
fn graph_working(w: &WitnessProbeInput, catalogs: &FactoryEditCatalogs) -> Result<Graph> {
    let mut g = Graph::default();
    let mut ports = BTreeMap::new();
    let recipes: BTreeMap<_, _> = catalogs
        .recipes
        .recipe_book()
        .recipes
        .iter()
        .map(|r| (r.id.as_str(), r))
        .collect();
    let mut facility_recipes = BTreeMap::new();
    for p in &w.placements {
        let d = catalogs
            .facilities
            .facility(&p.facility)
            .ok_or("Missing facility definition")?;
        let recipe = recipes
            .get(p.recipe.as_str())
            .ok_or("Missing recipe definition")?;
        facility_recipes.insert(p.instance.as_str(), *recipe);
        let mut exact = rate(
            *w.utilizations
                .get(&p.instance)
                .ok_or("Missing utilization")?,
        )?;
        exact.push(recipe.duration_ms);
        let owner = g.vertex(
            K::Facility,
            vec![
                p.x,
                p.y,
                p.width,
                p.height,
                p.rotation,
                d.footprint.width,
                d.footprint.height,
                rotations(&d.allowed_rotations),
            ],
            exact,
        );
        for port in rotate(d, p.rotation, Point { x: p.x, y: p.y }).2 {
            let id = g.vertex(
                K::Port,
                vec![
                    port.position.x,
                    port.position.y,
                    edge_index(port.edge) as i64,
                    direction(port.direction),
                    medium(port.transport),
                ],
                capacity(catalogs.transports.capacity(port.transport))?,
            );
            g.relation(owner, R::OwnsPort, id);
            ports.insert((p.instance.as_str(), port.id), id);
        }
    }
    let mut components = BTreeMap::new();
    for c in &w.logistics_components {
        let d = catalogs
            .components
            .component(&c.component)
            .ok_or("Missing component definition")?;
        let mask = |ds: &[aic_data::logistics::CardinalDirection]| {
            ds.iter().fold(0i64, |bits, &v| {
                bits | (1 << ((cardinal_index(v) + (c.rotation / 90) as usize) % 4))
            })
        };
        let id = g.vertex(
            K::Component,
            vec![
                c.position.x,
                c.position.y,
                c.rotation,
                d.footprint.width,
                d.footprint.height,
                medium(d.transport),
                match d.kind {
                    LogisticsComponentKind::Splitter => 0,
                    LogisticsComponentKind::Converger => 1,
                    LogisticsComponentKind::Bridge => 2,
                },
                rotations(&d.allowed_rotations),
                mask(&d.input_directions),
                mask(&d.output_directions),
            ],
            capacity(&d.capacity)?,
        );
        components.insert(c.id.as_str(), id);
    }
    let mut commodities = BTreeMap::new();
    let mut external = BTreeMap::new();
    for n in &w.transport_networks {
        let commodity = *commodities
            .entry(n.item.as_str())
            .or_insert_with(|| g.vertex(K::Commodity, vec![], vec![]));
        let network = g.vertex(
            K::Network,
            vec![medium(n.transport)],
            capacity(catalogs.transports.capacity(n.transport))?,
        );
        g.relation(network, R::Commodity, commodity);
        // World positions identify a network's cells; distinct networks retain separate channels.
        let mut cells = BTreeMap::new();
        for p in &n.cells {
            let cell = g.vertex(K::Cell, vec![p.x, p.y], vec![]);
            if cells.insert((p.x, p.y), cell).is_some() {
                return Err("Repeated network cell".into());
            }
            g.relation(network, R::NetworkCell, cell);
        }
        for s in &n.segments {
            let segment = g.vertex(K::Segment, vec![], rate(s.rate)?);
            g.relation(network, R::NetworkSegment, segment);
            g.relation(
                segment,
                R::SegmentFrom,
                get(&cells, &(s.from.x, s.from.y), "segment source")?,
            );
            g.relation(
                segment,
                R::SegmentTo,
                get(&cells, &(s.to.x, s.to.y), "segment target")?,
            );
        }
        for t in &n.terminals {
            let mut attributes = vec![direction(t.direction)];
            let mut exact = rate(t.rate)?;
            let endpoint = match &t.endpoint {
                TransportNetworkEndpoint::Facility { instance, port } => {
                    attributes.push(-1);
                    let recipe = facility_recipes
                        .get(instance.as_str())
                        .ok_or("Missing terminal facility")?;
                    let amounts = if t.direction == FacilityPortDirection::Input {
                        &recipe.inputs
                    } else {
                        &recipe.outputs
                    };
                    exact.push(
                        amounts
                            .iter()
                            .find(|a| a.item == n.item)
                            .ok_or("Missing terminal recipe commodity")?
                            .quantity,
                    );
                    get(&ports, &(instance.as_str(), port.clone()), "terminal port")?
                }
                TransportNetworkEndpoint::External { node, side } => {
                    attributes.push(edge_index(*side) as i64);
                    *external
                        .entry(node.as_str())
                        .or_insert_with(|| g.vertex(K::External, vec![], vec![]))
                }
            };
            let terminal = g.vertex(K::Terminal, attributes, exact);
            g.relation(network, R::NetworkTerminal, terminal);
            g.relation(terminal, R::TerminalEndpoint, endpoint);
            g.relation(
                terminal,
                R::TerminalCell,
                get(&cells, &(t.position.x, t.position.y), "terminal position")?,
            );
        }
        for name in &n.component_ids {
            g.relation(
                network,
                R::NetworkComponent,
                get(&components, &name.as_str(), "component channel")?,
            );
        }
    }
    Ok(g)
}
