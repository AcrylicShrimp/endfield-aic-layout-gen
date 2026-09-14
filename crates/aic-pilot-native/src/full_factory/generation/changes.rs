//! Typed private pose/port/bridge changes, before physical acceptance.

use aic_data::layouts::{FacilityPlacement, FactoryEditCatalogs, TransportNetworkEndpoint};
use aic_data::logistics::LogisticsComponentKind;
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use super::catalog::{project, rotate};
use super::types::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Move {
    pub instances: Vec<String>,
    pub dx: i64,
    pub dy: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacilityRotation {
    pub instance: String,
    pub rotation: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComponentPose {
    pub id: String,
    pub x: i64,
    pub y: i64,
    pub rotation: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortAssignment {
    pub terminal: String,
    pub port: String,
}

#[derive(Clone)]
pub struct FacilityChange {
    pub pose: FacilityPlacement,
    pub terminal_positions: IndexMap<String, Point>,
    pub turns: usize,
}
#[derive(Clone)]
pub struct ComponentChange {
    pub position: Point,
    pub rotation: i64,
    pub turns: usize,
}

fn fail(
    code: &str,
    stage: &'static str,
    message: &str,
    entity: impl Into<serde_json::Value>,
) -> Diagnostic {
    let mut e = Diagnostic::new(code, message, entity);
    e.stage = stage;
    e.severity = "info";
    e
}
pub fn facility_rotations(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    moves: &[Move],
    rotations: &[FacilityRotation],
) -> Result<IndexMap<String, FacilityChange>> {
    let error = |message, entity| {
        fail(
            "local-facility-rotation",
            "local-facility-pose",
            message,
            entity,
        )
    };
    if rotations.len() > 16
        || rotations
            .iter()
            .map(|r| &r.instance)
            .collect::<IndexSet<_>>()
            .len()
            != rotations.len()
    {
        return Err(error(
            "Expected up to sixteen unique facility rotations",
            serde_json::Value::Null,
        ));
    }
    let shifts: IndexMap<_, _> = moves
        .iter()
        .flat_map(|m| m.instances.iter().map(move |id| (id, m)))
        .collect();
    let mut changes = IndexMap::new();
    for r in rotations {
        let entity = serde_json::to_value(r).expect("typed rotation");
        let bad = || {
            error(
                "Unknown/nonselected facility or unsupported catalog rotation",
                entity.clone(),
            )
        };
        let p = w
            .placements
            .iter()
            .find(|p| p.instance == r.instance)
            .ok_or_else(bad)?;
        let spec = catalogs.facilities.facility(&p.facility).ok_or_else(bad)?;
        let shift = shifts.get(&r.instance).ok_or_else(bad)?;
        if ![0, 90, 180, 270].contains(&r.rotation) || !spec.allowed_rotations.contains(&r.rotation)
        {
            return Err(bad());
        }
        let (width, height, ports) = rotate(
            spec,
            r.rotation,
            Point {
                x: p.x + shift.dx,
                y: p.y + shift.dy,
            },
        );
        let mut pose = p.clone();
        pose.x += shift.dx;
        pose.y += shift.dy;
        pose.width = width;
        pose.height = height;
        pose.rotation = r.rotation;
        let mut positions = IndexMap::new();
        for n in &w.transport_networks {
            for t in &n.terminals {
                if let TransportNetworkEndpoint::Facility { instance, port } = &t.endpoint {
                    if *instance != r.instance {
                        continue;
                    }
                    let q = ports
                        .iter()
                        .find(|q| {
                            q.id == *port
                                && q.direction == t.direction
                                && q.transport == n.transport
                        })
                        .ok_or_else(|| {
                            error(
                                "Rotated facility terminal has no compatible catalog port",
                                t.id.clone().into(),
                            )
                        })?;
                    positions.insert(t.id.clone(), q.connection);
                }
            }
        }
        if r.rotation != p.rotation {
            changes.insert(
                r.instance.clone(),
                FacilityChange {
                    pose,
                    terminal_positions: positions,
                    turns: ((r.rotation - p.rotation) / 90 + 4) as usize % 4,
                },
            );
        }
    }
    Ok(changes)
}

pub fn component_poses(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    poses: &[ComponentPose],
) -> Result<IndexMap<String, ComponentChange>> {
    let error = |message, entity| fail("local-component-pose", "component-pose", message, entity);
    if poses.len() > 8 || poses.iter().map(|p| &p.id).collect::<IndexSet<_>>().len() != poses.len()
    {
        return Err(error(
            "Expected up to eight unique component poses",
            serde_json::Value::Null,
        ));
    }
    let mut changes = IndexMap::new();
    for p in poses {
        let bad = || {
            error(
                "Unknown component, unsupported footprint or illegal runtime rotation",
                serde_json::to_value(p).expect("typed pose"),
            )
        };
        let c = w
            .logistics_components
            .iter()
            .find(|c| c.id == p.id)
            .ok_or_else(bad)?;
        let d = catalogs
            .components
            .component(&c.component)
            .ok_or_else(bad)?;
        if d.kind != c.kind
            || d.transport != c.transport
            || d.footprint.width != 1
            || d.footprint.height != 1
            || !d.allowed_rotations.contains(&p.rotation)
        {
            return Err(bad());
        }
        if w.transport_networks.iter().any(|n| {
            n.transport == c.transport && n.terminals.iter().any(|t| t.position == c.position)
        }) {
            return Err(error(
                "Component anchor must not own a facility/external terminal",
                p.id.clone().into(),
            ));
        }
        changes.insert(
            c.id.clone(),
            ComponentChange {
                position: Point { x: p.x, y: p.y },
                rotation: p.rotation,
                turns: ((p.rotation - c.rotation) / 90 + 4) as usize % 4,
            },
        );
    }
    Ok(changes)
}

pub fn release_bridges(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    ids: &[String],
) -> Result<(Witness, IndexSet<LayerPoint>)> {
    if ids.len() > 8 || ids.iter().collect::<IndexSet<_>>().len() != ids.len() {
        return Err(fail(
            "local-bridge-release",
            "bridge-release",
            "Expected up to eight unique existing bridge IDs",
            serde_json::Value::Null,
        ));
    }
    bridge_view(w, catalogs, ids)
}
pub fn bridge_view(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    ids: &[String],
) -> Result<(Witness, IndexSet<LayerPoint>)> {
    let error = |message| {
        fail(
            "local-bridge-release",
            "bridge-release",
            message,
            serde_json::Value::Null,
        )
    };
    let mut released = IndexSet::new();
    for id in ids {
        let c = w
            .logistics_components
            .iter()
            .find(|c| c.id == *id)
            .ok_or_else(|| error("Only existing catalog bridges may be released"))?;
        let def = catalogs
            .components
            .component(&c.component)
            .ok_or_else(|| error("Only existing catalog bridges may be released"))?;
        if c.kind != LogisticsComponentKind::Bridge || def.kind != LogisticsComponentKind::Bridge {
            return Err(error("Only existing catalog bridges may be released"));
        }
        let mut axes = IndexSet::new();
        for n in w
            .transport_networks
            .iter()
            .filter(|n| n.component_ids.contains(id))
        {
            let ins: Vec<_> = n.segments.iter().filter(|s| s.to == c.position).collect();
            let outs: Vec<_> = n.segments.iter().filter(|s| s.from == c.position).collect();
            if n.transport != c.transport
                || n.terminals.iter().any(|t| t.position == c.position)
                || ins.is_empty()
                || ins.len() != outs.len()
            {
                return Err(error(
                    "A bridge requires terminal-free independent channels",
                ));
            }
            for incoming in ins {
                let d = direction(Point::from(&incoming.from), Point::from(&incoming.to));
                let straight: Vec<_> = outs
                    .iter()
                    .filter(|s| direction(Point::from(&s.from), Point::from(&s.to)) == d)
                    .collect();
                if d.is_none()
                    || axes.contains(&(d.unwrap_or_default() % 2))
                    || straight.len() != 1
                    || !equal_rate(incoming.rate, straight[0].rate)
                    || i128::from(incoming.rate.numerator) * i128::from(def.capacity.duration_ms)
                        > i128::from(def.capacity.quantity)
                            * 1000
                            * i128::from(incoming.rate.denominator)
                {
                    return Err(error(
                        "A bridge requires two straight equal-rate channels within per-axis capacity",
                    ));
                }
                axes.insert(d.expect("checked direction") % 2);
            }
        }
        if axes.len() != 2 {
            return Err(error(
                "A bridge requires exactly two perpendicular channels",
            ));
        }
        released.insert((layer(c.transport), Point::from(&c.position)));
    }
    let mut draft = w.clone();
    draft.logistics_components.retain(|c| !ids.contains(&c.id));
    for n in &mut draft.transport_networks {
        n.component_ids.retain(|id| !ids.contains(id));
    }
    Ok((draft, released))
}

pub fn assign_ports(
    w: &mut Witness,
    catalogs: &FactoryEditCatalogs,
    assignments: &[PortAssignment],
    eligible: &IndexSet<String>,
) -> Result<IndexMap<String, Point>> {
    let error = |message, entity| fail("local-port-assignment", "local-ports", message, entity);
    if assignments.len() > 16
        || assignments
            .iter()
            .map(|a| &a.terminal)
            .collect::<IndexSet<_>>()
            .len()
            != assignments.len()
    {
        return Err(error(
            "Expected at most sixteen unique terminal assignments",
            serde_json::Value::Null,
        ));
    }
    let ports = project(w, catalogs)?;
    let mut changes = IndexMap::new();
    let mut positions = IndexMap::new();
    for a in assignments {
        let matches: Vec<_> = w
            .transport_networks
            .iter()
            .flat_map(|n| {
                n.terminals
                    .iter()
                    .filter(|t| t.id == a.terminal)
                    .map(move |t| (n, t))
            })
            .collect();
        if matches.len() != 1 {
            return Err(error(
                "Assignment must identify one existing terminal",
                a.terminal.clone().into(),
            ));
        }
        let (n, t) = matches[0];
        let TransportNetworkEndpoint::Facility { instance, port } = &t.endpoint else {
            return Err(error(
                "Only independently identified ordinary facility terminals can change ports",
                a.terminal.clone().into(),
            ));
        };
        if !eligible.contains(&t.id) {
            return Err(error(
                "Only independently identified ordinary facility terminals can change ports",
                a.terminal.clone().into(),
            ));
        }
        let q = ports[instance]
            .iter()
            .find(|q| q.id == a.port && q.direction == t.direction && q.transport == n.transport)
            .ok_or_else(|| {
                error(
                    "Assigned port has incompatible identity, direction or transport",
                    serde_json::to_value(a).expect("typed assignment"),
                )
            })?;
        if a.port != *port {
            let mut changed = t.clone();
            changed.endpoint = TransportNetworkEndpoint::Facility {
                instance: instance.clone(),
                port: a.port.clone(),
            };
            changed.position = q.connection.into();
            changes.insert(t.id.clone(), changed);
            positions.insert(t.id.clone(), q.connection);
        }
    }
    let mut networks = w.transport_networks.clone();
    let mut used = IndexSet::new();
    let mut counts = IndexMap::<LayerPoint, usize>::new();
    for n in &mut networks {
        for t in &mut n.terminals {
            if let Some(changed) = changes.get(&t.id) {
                *t = changed.clone();
            }
            *counts
                .entry((layer(n.transport), Point::from(&t.position)))
                .or_default() += 1;
            if let TransportNetworkEndpoint::Facility { instance, port } = &t.endpoint {
                if !used.insert((instance.clone(), port.clone())) {
                    return Err(error(
                        "Two terminals claim the same facility port",
                        t.id.clone().into(),
                    ));
                }
            }
        }
    }
    for n in &networks {
        for t in &n.terminals {
            if changes.contains_key(&t.id)
                && (counts[&(layer(n.transport), Point::from(&t.position))] != 1
                    || w.logistics_components
                        .iter()
                        .any(|c| c.transport == n.transport && c.position == t.position))
            {
                return Err(error(
                    "Assigned port collides with another terminal or component",
                    t.id.clone().into(),
                ));
            }
        }
    }
    w.transport_networks = networks;
    Ok(positions)
}
