use aic_data::layouts::{FactoryEditCatalogs, TransportNetworkEndpoint};
use aic_data::logistics::{LogisticsComponentKind, TransportKind};
use indexmap::{IndexMap, IndexSet};
use serde::Serialize;
use serde_json::Value;

use super::catalog::{self, Port};
use super::chains::{self, Decomposition};
use super::changes::{ComponentPose, PortAssignment};
use super::movement::MovementIndex;
use super::poses::{self, PoseAction, VocabularyOptions};
use super::routing::{self, Edit, Execution};
use super::strips::{self, Axis};
use super::types::*;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    Pose(PoseAction),
    Port {
        terminal: String,
        instance: String,
        port: String,
        x: i64,
        y: i64,
        transport: TransportKind,
    },
    Reroute {
        chain: usize,
        network: usize,
        x: i64,
        y: i64,
        transport: TransportKind,
    },
    Component {
        component: String,
        x: i64,
        y: i64,
        dx: i64,
        dy: i64,
        rotation: i64,
        transport: TransportKind,
    },
    ReleaseBridge {
        component: String,
        x: i64,
        y: i64,
        transport: TransportKind,
    },
    Insert {
        axis: Axis,
        position: i64,
        x: i64,
        y: i64,
    },
    Contract {
        axis: Axis,
        position: i64,
        x: i64,
        y: i64,
    },
    RestoreBest {
        x: i64,
        y: i64,
    },
}
impl Action {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Pose(_) => "pose",
            Self::Port { .. } => "port",
            Self::Reroute { .. } => "reroute",
            Self::Component { .. } => "component",
            Self::ReleaseBridge { .. } => "release_bridge",
            Self::Insert { .. } => "insert",
            Self::Contract { .. } => "contract",
            Self::RestoreBest { .. } => "restore_best",
        }
    }
    pub fn point(&self) -> Point {
        match self {
            Self::Pose(p) => Point { x: p.x, y: p.y },
            Self::Port { x, y, .. }
            | Self::Reroute { x, y, .. }
            | Self::Component { x, y, .. }
            | Self::ReleaseBridge { x, y, .. }
            | Self::Insert { x, y, .. }
            | Self::Contract { x, y, .. }
            | Self::RestoreBest { x, y } => Point { x: *x, y: *y },
        }
    }
}
pub struct Domain {
    pub ports: IndexMap<String, Vec<Port>>,
    pub decomposition: Decomposition,
    pub actions: Vec<Action>,
    pub report: Value,
}
pub fn describe(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    opts: &VocabularyOptions,
) -> Result<Domain> {
    let ports = catalog::project(w, catalogs)?;
    let decomposition = chains::extract(w, &IndexSet::new())?;
    let movement = MovementIndex::new(w, &decomposition.chains);
    let (poses, report) = poses::actions(w, catalogs, opts, &decomposition, &movement)?;
    let mut actions: Vec<_> = poses.into_iter().map(Action::Pose).collect();
    let used: IndexSet<_> = w
        .transport_networks
        .iter()
        .flat_map(|n| {
            n.terminals.iter().filter_map(|t| {
                if let TransportNetworkEndpoint::Facility { instance, port } = &t.endpoint {
                    Some((instance.as_str(), port.as_str()))
                } else {
                    None
                }
            })
        })
        .collect();
    for n in &w.transport_networks {
        for t in &n.terminals {
            if let TransportNetworkEndpoint::Facility { instance, .. } = &t.endpoint {
                for p in &ports[instance] {
                    if p.direction == t.direction
                        && p.transport == n.transport
                        && !used.contains(&(instance.as_str(), p.id.as_str()))
                    {
                        actions.push(Action::Port {
                            terminal: t.id.clone(),
                            instance: instance.clone(),
                            port: p.id.clone(),
                            x: p.connection.x,
                            y: p.connection.y,
                            transport: n.transport,
                        });
                    }
                }
            }
        }
    }
    for c in &decomposition.chains {
        actions.push(Action::Reroute {
            chain: c.id,
            network: c.network,
            x: c.start().x,
            y: c.start().y,
            transport: c.transport,
        });
    }
    for c in &w.logistics_components {
        for (dx, dy) in poses::LOCAL_STEPS {
            actions.push(Action::Component {
                component: c.id.clone(),
                x: c.position.x + dx,
                y: c.position.y + dy,
                dx,
                dy,
                rotation: c.rotation,
                transport: c.transport,
            });
        }
        if c.kind == LogisticsComponentKind::Bridge {
            actions.push(Action::ReleaseBridge {
                component: c.id.clone(),
                x: c.position.x,
                y: c.position.y,
                transport: c.transport,
            });
        }
    }
    for axis in [Axis::X, Axis::Y] {
        let strips = strips::candidates(w, axis)?;
        for position in strips.insert {
            let (x, y) = if axis == Axis::X {
                (position, 0)
            } else {
                (0, position)
            };
            actions.push(Action::Insert {
                axis,
                position,
                x,
                y,
            });
        }
        for position in strips.remove {
            let (x, y) = if axis == Axis::X {
                (position, 0)
            } else {
                (0, position)
            };
            actions.push(Action::Contract {
                axis,
                position,
                x,
                y,
            });
        }
    }
    actions.push(Action::RestoreBest { x: 0, y: 0 });
    Ok(Domain {
        ports,
        decomposition,
        actions,
        report,
    })
}

pub fn execute(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    action: &Action,
    work: usize,
) -> Result<Execution> {
    if !(1..=200000).contains(&work) {
        return Err(Diagnostic::new(
            "factory.edit-parameters",
            "Invalid route work budget",
            Value::Null,
        ));
    }
    let mut edit = Edit::default();
    match action {
        Action::Pose(p) => {
            let (moves, rotations, _) = poses::options(w, catalogs, &p.poses)?;
            edit.moves = moves;
            edit.facility_rotations = rotations;
        }
        Action::Port { terminal, port, .. } => edit.port_assignments.push(PortAssignment {
            terminal: terminal.clone(),
            port: port.clone(),
        }),
        Action::Reroute { chain, .. } => edit.chain_ids.push(*chain),
        Action::Component {
            component,
            x,
            y,
            rotation,
            ..
        } => edit.component_poses.push(ComponentPose {
            id: component.clone(),
            x: *x,
            y: *y,
            rotation: *rotation,
        }),
        Action::ReleaseBridge { component, .. } => edit.release_bridges.push(component.clone()),
        Action::Insert { axis, position, .. } | Action::Contract { axis, position, .. } => {
            let witness =
                strips::edit(w, *axis, *position, matches!(action, Action::Insert { .. }))?;
            return Ok(Execution {
                status: "candidate",
                witness: Some(witness),
                diagnostics: vec![],
                routes: vec![],
                route_expansions: 0,
            });
        }
        Action::RestoreBest { .. } => {
            return Err(Diagnostic::new(
                "factory.restore-selection",
                "Restore is an episode transaction, not a geometry proposal",
                Value::Null,
            ));
        }
    }
    routing::reembed(w, catalogs, &edit, work)
}
