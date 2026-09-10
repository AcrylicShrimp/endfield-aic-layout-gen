mod components;

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
        count: i64,
        x: i64,
        y: i64,
    },
    Contract {
        axis: Axis,
        count: i64,
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
    let (poses, mut report) = poses::actions(w, catalogs, opts, &decomposition, &movement)?;
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
    let (component_actions, component_report) = components::actions(w, catalogs)?;
    actions.extend(component_actions);
    report["components"] = component_report;
    for c in &w.logistics_components {
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
        for position in strips::insertion_cuts(&strips.insert) {
            let (x, y) = if axis == Axis::X {
                (position, 0)
            } else {
                (0, position)
            };
            for count in strips::insertion_counts(w, axis) {
                actions.push(Action::Insert {
                    axis,
                    position,
                    count,
                    x,
                    y,
                });
            }
        }
        if !strips.remove.is_empty() {
            actions.push(Action::Contract {
                axis,
                count: strips.remove.len() as i64,
                x: 0,
                y: 0,
            });
        }
    }
    report["strips"] = serde_json::json!({
        "contraction": "all-removable-on-axis",
        "contract_actions": actions.iter().filter(|a| matches!(a, Action::Contract { .. })).count(),
        "insert_actions": actions.iter().filter(|a| matches!(a, Action::Insert { .. })).count(),
        "insert_cuts_per_axis_limit": 32,
        "insert_counts_per_cut": 2,
    });
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
    limits: &routing::Limits,
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
        Action::Insert { axis, count, .. } | Action::Contract { axis, count, .. } => {
            let position = match action {
                Action::Insert { position, .. } => Some(*position),
                _ => None,
            };
            let witness = strips::edit(w, *axis, position, *count)?;
            return Ok(Execution {
                status: "candidate",
                witness: Some(witness),
                diagnostics: vec![],
                routes: vec![],
                route_expansions: 0,
                region: None,
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
    routing::reembed(w, catalogs, &edit, work, limits)
}
