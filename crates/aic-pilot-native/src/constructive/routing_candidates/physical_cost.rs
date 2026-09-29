//! Exact observable physical deltas for one natively replayed macro witness.

use std::collections::{BTreeMap, BTreeSet};

use aic_layout::{
    geometry::{Point, direction, layer},
    logistics::{LogisticsComponentKind, TransportKind},
    rate::Rate,
};
use serde::{Deserialize, Serialize};

use super::{Session, error};
use crate::constructive::Diagnostic;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct ComponentUse {
    pub catalog_id: String,
    pub kind: LogisticsComponentKind,
    pub transport: TransportKind,
    pub position: Point,
    pub rotation: i64,
    /// -1 removed, 0 reused, +1 installed.
    pub delta: i8,
    pub footprint_cells: i64,
}

/// Field order is the exact lexicographic comparison order for witnesses
/// with one semantic identity. Rates compare as rationals, not rounded floats.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub struct PhysicalCost {
    pub route_steps_delta: i64,
    pub route_turns_delta: i64,
    pub occupied_transport_cells_delta: i64,
    pub component_count_delta: i64,
    pub component_footprint_cells_delta: i64,
    pub component_uses: Vec<ComponentUse>,
    /// Maximum exact load/capacity ratio over transport resources touched by
    /// this macro, before and after native replay. A bridge axis is separate.
    pub max_touched_utilization_before: Rate,
    pub max_touched_utilization_after: Rate,
    pub touched_resource_count: usize,
}

impl Default for PhysicalCost {
    fn default() -> Self {
        Self {
            route_steps_delta: 0,
            route_turns_delta: 0,
            occupied_transport_cells_delta: 0,
            component_count_delta: 0,
            component_footprint_cells_delta: 0,
            component_uses: Vec::new(),
            max_touched_utilization_before: Rate::zero(),
            max_touched_utilization_after: Rate::zero(),
            touched_resource_count: 0,
        }
    }
}

fn cost_error(message: impl Into<String>) -> Diagnostic {
    error("route-physical-cost", "candidate", message)
}

fn route_turns(cells: &[Point]) -> i64 {
    cells
        .windows(3)
        .filter(|points| direction(points[0], points[1]) != direction(points[1], points[2]))
        .count() as i64
}

fn component_at(
    session: &Session,
    transport: TransportKind,
    point: Point,
) -> Result<Option<ComponentUse>, Diagnostic> {
    let (catalog_id, rotation) = if let Some(junction) = session.junctions.get(&(transport, point))
    {
        (&junction.component, junction.rotation)
    } else if let Some(bridge) = session.bridges.get(&(transport, point)) {
        (&bridge.component, bridge.rotation)
    } else {
        return Ok(None);
    };
    let definition = session
        .catalogs
        .components
        .component(catalog_id)
        .ok_or_else(|| {
            cost_error(format!(
                "committed component {catalog_id} is missing from the catalog"
            ))
        })?;
    let footprint_cells = definition
        .footprint
        .width
        .checked_mul(definition.footprint.height)
        .ok_or_else(|| cost_error("component footprint area overflow"))?;
    Ok(Some(ComponentUse {
        catalog_id: catalog_id.to_owned(),
        kind: definition.kind,
        transport,
        position: point,
        rotation,
        delta: 0,
        footprint_cells,
    }))
}

fn ratio(load: Rate, quantity: i64, duration_ms: i64) -> Result<Rate, Diagnostic> {
    let capacity = Rate::from_quantity_per_duration_ms(quantity, duration_ms)
        .map_err(|failure| cost_error(failure.message))?;
    Rate::normalize(
        i128::from(load.numerator) * i128::from(capacity.denominator),
        i128::from(load.denominator) * i128::from(capacity.numerator),
        "physical-capacity-ratio",
    )
    .map_err(|failure| cost_error(failure.message))
}

fn add_load(load: &mut Rate, next: Rate) -> Result<(), Diagnostic> {
    *load = load
        .checked_add(next)
        .map_err(|failure| cost_error(failure.message))?;
    Ok(())
}

fn utilization_at(
    session: &Session,
    transport: TransportKind,
    point: Point,
) -> Result<(Rate, usize), Diagnostic> {
    let line = session.catalogs.transports.capacity(transport);
    if let Some(bridge) = session.bridges.get(&(transport, point)) {
        let definition = session
            .catalogs
            .components
            .component(&bridge.component)
            .ok_or_else(|| {
                cost_error(format!(
                    "committed bridge {} is missing from the catalog",
                    bridge.component
                ))
            })?;
        let mut maximum = Rate::zero();
        for channel in &bridge.channels {
            let mut load = Rate::zero();
            for id in &channel.connections {
                let &index = session.connections.get(id).ok_or_else(|| {
                    cost_error(format!("bridge owner {id} is not a logical connection"))
                })?;
                add_load(&mut load, session.problem.connections[index].rate)?;
            }
            maximum = maximum.max(ratio(load, line.quantity, line.duration_ms)?);
            maximum = maximum.max(ratio(
                load,
                definition.capacity.quantity,
                definition.capacity.duration_ms,
            )?);
        }
        return Ok((maximum, 2));
    }
    let Some(users) = session.occupied[layer(transport)].get(&point) else {
        return Ok((Rate::zero(), 0));
    };
    let mut load = Rate::zero();
    for &index in users {
        add_load(&mut load, session.problem.connections[index].rate)?;
    }
    let mut maximum = ratio(load, line.quantity, line.duration_ms)?;
    if let Some(junction) = session.junctions.get(&(transport, point)) {
        let definition = session
            .catalogs
            .components
            .component(&junction.component)
            .ok_or_else(|| {
                cost_error(format!(
                    "committed junction {} is missing from the catalog",
                    junction.component
                ))
            })?;
        maximum = maximum.max(ratio(
            load,
            definition.capacity.quantity,
            definition.capacity.duration_ms,
        )?);
    }
    Ok((maximum, 1))
}

struct SiteCost {
    occupied: bool,
    component: Option<ComponentUse>,
    utilization: Rate,
    resources: usize,
}

fn site_cost(
    session: &Session,
    transport: TransportKind,
    point: Point,
) -> Result<SiteCost, Diagnostic> {
    let (utilization, resources) = utilization_at(session, transport, point)?;
    Ok(SiteCost {
        occupied: session.occupied[layer(transport)].contains_key(&point),
        component: component_at(session, transport, point)?,
        utilization,
        resources,
    })
}

/// Compact pre-replay view of only the cells a route member can change. A
/// repair draft mutates in place, so this avoids copying the entire factory
/// for each replacement while retaining exact before/after capacity use.
pub(super) struct PhysicalCostSnapshot {
    connection: String,
    transport: TransportKind,
    before_steps: i64,
    before_turns: i64,
    sites: BTreeMap<Point, SiteCost>,
}

impl PhysicalCostSnapshot {
    pub(super) fn capture(
        before: &Session,
        connection: &str,
        proposed_cells: &[Point],
    ) -> Result<Self, Diagnostic> {
        let &index = before.connections.get(connection).ok_or_else(|| {
            cost_error(format!(
                "changed route {connection} is not a logical connection"
            ))
        })?;
        let transport = before.problem.connections[index].transport;
        let previous = before.routes[index].as_ref();
        let cells = proposed_cells
            .iter()
            .copied()
            .chain(
                previous
                    .into_iter()
                    .flat_map(|route| route.cells.iter().copied()),
            )
            .collect::<BTreeSet<_>>();
        let sites = cells
            .into_iter()
            .map(|point| Ok((point, site_cost(before, transport, point)?)))
            .collect::<Result<BTreeMap<_, _>, Diagnostic>>()?;
        Ok(Self {
            connection: connection.to_owned(),
            transport,
            before_steps: previous.map_or(0, |route| route.cells.len().saturating_sub(1) as i64),
            before_turns: previous.map_or(0, |route| route_turns(&route.cells)),
            sites,
        })
    }

    pub(super) fn finish(self, after: &Session) -> Result<PhysicalCost, Diagnostic> {
        let &index = after
            .connections
            .get(&self.connection)
            .ok_or_else(|| cost_error(format!("changed route {} disappeared", self.connection)))?;
        let route = after.routes[index].as_ref().ok_or_else(|| {
            cost_error(format!(
                "changed route {} was not committed",
                self.connection
            ))
        })?;
        if route
            .cells
            .iter()
            .any(|point| !self.sites.contains_key(point))
        {
            return Err(cost_error(
                "committed route exceeds pre-replay cost snapshot",
            ));
        }
        let mut cost = PhysicalCost {
            route_steps_delta: route.cells.len().saturating_sub(1) as i64 - self.before_steps,
            route_turns_delta: route_turns(&route.cells) - self.before_turns,
            ..PhysicalCost::default()
        };
        for (point, old) in self.sites {
            let new = site_cost(after, self.transport, point)?;
            cost.occupied_transport_cells_delta +=
                i64::from(new.occupied) - i64::from(old.occupied);
            if old.component != new.component {
                if let Some(mut removed) = old.component {
                    removed.delta = -1;
                    cost.component_uses.push(removed);
                }
                if let Some(mut installed) = new.component {
                    installed.delta = 1;
                    cost.component_uses.push(installed);
                }
            } else if let Some(reused) = old.component {
                cost.component_uses.push(reused);
            }
            cost.max_touched_utilization_before =
                cost.max_touched_utilization_before.max(old.utilization);
            cost.max_touched_utilization_after =
                cost.max_touched_utilization_after.max(new.utilization);
            cost.touched_resource_count += old.resources.max(new.resources);
        }
        cost.component_count_delta = cost
            .component_uses
            .iter()
            .map(|item| i64::from(item.delta))
            .sum();
        cost.component_footprint_cells_delta = cost
            .component_uses
            .iter()
            .map(|item| i64::from(item.delta) * item.footprint_cells)
            .sum();
        Ok(cost)
    }
}

/// Compare only the named changed routes and their affected sites. Existing
/// native occupancy indexes make this proportional to touched geometry rather
/// than rescanning every route for every candidate.
pub(super) fn physical_cost_between(
    before: &Session,
    after: &Session,
    changed_connections: &[String],
) -> Result<PhysicalCost, Diagnostic> {
    let mut touched = BTreeSet::<(TransportKind, Point)>::new();
    let mut steps = 0i64;
    let mut turns = 0i64;
    let changed = changed_connections
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for id in &changed {
        let &index = before
            .connections
            .get(*id)
            .ok_or_else(|| cost_error(format!("changed route {id} is not a logical connection")))?;
        let transport = before.problem.connections[index].transport;
        for (session, sign) in [(before, -1), (after, 1)] {
            if let Some(route) = &session.routes[index] {
                steps += sign * route.cells.len().saturating_sub(1) as i64;
                turns += sign * route_turns(&route.cells);
                touched.extend(route.cells.iter().copied().map(|point| (transport, point)));
            }
        }
    }
    for session in [before, after] {
        for (&site, junction) in &session.junctions {
            if junction
                .connections
                .iter()
                .any(|id| changed.contains(id.as_str()))
            {
                touched.insert(site);
            }
        }
        for (&site, bridge) in &session.bridges {
            if bridge.channels.iter().any(|channel| {
                channel
                    .connections
                    .iter()
                    .any(|id| changed.contains(id.as_str()))
            }) {
                touched.insert(site);
            }
        }
    }
    let mut occupied_delta = 0i64;
    let mut component_uses = Vec::new();
    let mut max_before = Rate::zero();
    let mut max_after = Rate::zero();
    let mut resource_count = 0usize;
    for &(transport, point) in &touched {
        let before_occupied = before.occupied[layer(transport)].contains_key(&point);
        let after_occupied = after.occupied[layer(transport)].contains_key(&point);
        occupied_delta += i64::from(after_occupied) - i64::from(before_occupied);
        let old = component_at(before, transport, point)?;
        let new = component_at(after, transport, point)?;
        if old != new {
            if let Some(mut old) = old {
                old.delta = -1;
                component_uses.push(old);
            }
            if let Some(mut new) = new {
                new.delta = 1;
                component_uses.push(new);
            }
        } else if let Some(old) = old {
            component_uses.push(old);
        }
        let (old_utilization, old_count) = utilization_at(before, transport, point)?;
        let (new_utilization, new_count) = utilization_at(after, transport, point)?;
        max_before = max_before.max(old_utilization);
        max_after = max_after.max(new_utilization);
        resource_count += old_count.max(new_count);
    }
    let component_count_delta = component_uses
        .iter()
        .map(|use_item| i64::from(use_item.delta))
        .sum();
    let component_footprint_cells_delta = component_uses
        .iter()
        .map(|use_item| i64::from(use_item.delta) * use_item.footprint_cells)
        .sum();
    Ok(PhysicalCost {
        route_steps_delta: steps,
        route_turns_delta: turns,
        occupied_transport_cells_delta: occupied_delta,
        component_count_delta,
        component_footprint_cells_delta,
        component_uses,
        max_touched_utilization_before: max_before,
        max_touched_utilization_after: max_after,
        touched_resource_count: resource_count,
    })
}
