//! Retained declarative candidates and typed numerical learning inputs.

mod input;
pub(crate) mod runtime;
pub(super) mod trace;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU64, Ordering};

use aic_data::facilities::FacilityPortDirection;
use sha2::{Digest, Sha256};

use super::*;

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub translation_steps: Vec<i64>,
    pub band_counts: Vec<i64>,
    pub masking: Option<String>,
}

#[derive(Clone)]
struct Root {
    kind: usize,
    invalid: bool,
    owner: usize,
    x: i64,
    y: i64,
    rotation: i64,
    axis: Axis,
    at: i64,
    count: i64,
}

struct Group {
    node: usize,
    ports: Vec<String>,
    features: BTreeMap<i64, (usize, usize)>,
}

struct Facility {
    rotations: Vec<i64>,
    ports: BTreeMap<i64, Vec<catalog::Port>>,
    groups: Vec<Group>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct Input {
    pub pending: usize,
    pub conflicts: usize,
    pub node_static: Vec<[f32; 14]>,
    pub node_state: Vec<[f32; 10]>,
    pub edge_index: Vec<[i64; 2]>,
    pub edge_static: Vec<[f32; 10]>,
    pub edge_state: Vec<[f32; 4]>,
    pub path_cells: Vec<[f32; 2]>,
    pub path_edges: Vec<i64>,
    pub global_features: Vec<f32>,
    pub action_features: Vec<[f32; 12]>,
    pub action_references: Vec<[i64; 2]>,
    pub root_meta: Vec<[i64; 3]>,
    pub kind_log_counts: Vec<f32>,
    pub root_invalid: Vec<bool>,
    pub port_features: Vec<[f32; 12]>,
}

pub(super) struct Learning {
    options: Options,
    facilities: Vec<Facility>,
    owners: BTreeMap<String, usize>,
    fingerprints: BTreeMap<String, [f64; 4]>,
    roots: Vec<Root>,
    handle: u64,
    revision: u64,
    attempts: u64,
}

fn floats<const N: usize>(row: [f64; N]) -> [f32; N] {
    row.map(|v| v as f32)
}

fn angle(rotation: i64) -> (f64, f64) {
    let value = rotation as f64 * (std::f64::consts::PI / 180.0);
    (value.sin(), value.cos())
}

impl Learning {
    pub(super) fn new(session: &Session, options: Options) -> Result<Self> {
        if options.band_counts.iter().any(|n| *n <= 0)
            || !matches!(
                options.masking.as_deref(),
                None | Some("structural-bands-v1")
            )
        {
            return Err(error(
                "learning-config",
                "Invalid declarative candidate configuration",
                "actions",
            ));
        }
        let mut facilities = vec![];
        let mut owners = BTreeMap::new();
        let mut names = BTreeSet::new();
        let mut offset = 0;
        for (owner, placement) in session.blueprint.facilities.iter().enumerate() {
            owners.insert(placement.instance.clone(), owner);
            names.insert(placement.recipe.clone());
            let definition = session
                .catalogs
                .data
                .facilities
                .facility(&placement.facility)
                .unwrap();
            let rotations = definition.allowed_rotations.clone();
            let ports = rotations
                .iter()
                .map(|&rotation| {
                    (
                        rotation,
                        catalog::rotate(definition, rotation, Point::default()).2,
                    )
                })
                .collect();
            let mut groups = vec![];
            for node in &session.blueprint.nodes {
                let Some(terminal) = &node.terminal else {
                    continue;
                };
                if !matches!(&terminal.endpoint, TransportNetworkEndpoint::Facility { instance, .. }
                    if instance == &placement.instance)
                {
                    continue;
                }
                let choices: Vec<_> = definition
                    .ports
                    .iter()
                    .filter(|port| {
                        port.direction == terminal.direction
                            && json!(port.transport)
                                == session.blueprint.networks[node.network]["transport"]
                    })
                    .map(|port| port.id.clone())
                    .collect();
                names.extend(choices.iter().cloned());
                let mut features = BTreeMap::new();
                for &rotation in &rotations {
                    features.insert(rotation, (offset, offset + choices.len()));
                    offset += choices.len();
                }
                groups.push(Group {
                    node: node.id,
                    ports: choices,
                    features,
                });
            }
            facilities.push(Facility {
                rotations,
                ports,
                groups,
            });
        }
        for network in &session.blueprint.networks {
            names.insert(network["item"].as_str().unwrap().to_owned());
        }
        let fingerprints = names
            .into_iter()
            .map(|name| {
                let digest = Sha256::digest(name.as_bytes());
                let values = std::array::from_fn(|i| f64::from(digest[i]) / 255.);
                (name, values)
            })
            .collect();
        Ok(Self {
            options,
            facilities,
            owners,
            fingerprints,
            roots: vec![],
            handle: 0,
            revision: 0,
            attempts: 0,
        })
    }

    fn position(&self, session: &Session, node: &Node) -> Point {
        if let Some(terminal) = &node.terminal
            && let TransportNetworkEndpoint::Facility { instance, .. } = &terminal.endpoint
        {
            let owner = self.owners[instance];
            let placement = &session.state.geometry.placements[owner];
            let name = session.state.ports[node.id].as_ref().unwrap();
            let port = self.facilities[owner].ports[&placement.rotation]
                .iter()
                .find(|p| &p.id == name)
                .unwrap();
            return port.connection.shifted(placement.x, placement.y);
        }
        session.state.anchors[node.id]
    }

    pub(super) fn observe(&mut self, session: &Session) -> Input {
        let result = self.build(session);
        self.handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        self.revision = session.revision;
        self.attempts = session.attempts;
        result
    }

    pub(super) fn selected(
        &self,
        session: &Session,
        handle: u64,
        root: usize,
        ports: &[usize],
    ) -> Result<Decision> {
        if handle == 0
            || handle != self.handle
            || self.revision != session.revision
            || self.attempts != session.attempts
        {
            return Err(error(
                "learning-handle",
                "Stale, consumed or foreign candidate handle",
                "selection",
            ));
        }
        let Some(root) = self.roots.get(root) else {
            return Err(error(
                "learning-selection",
                "Root index is outside the retained domain",
                root,
            ));
        };
        if root.invalid {
            return Err(error(
                "learning-selection",
                "Selected root is structurally masked",
                "root",
            ));
        }
        let action = if root.kind == 0 {
            let groups = &self.facilities[root.owner].groups;
            if ports.len() != groups.len() {
                return Err(error(
                    "learning-selection",
                    "Selection must assign every original terminal",
                    "ports",
                ));
            }
            let mut used = BTreeSet::new();
            let mut choices = vec![];
            for (group, &index) in groups.iter().zip(ports) {
                let Some(port) = group.ports.get(index) else {
                    return Err(error(
                        "learning-selection",
                        "Port index is outside its compatible domain",
                        group.node,
                    ));
                };
                if !used.insert(port) {
                    return Err(error(
                        "learning-selection",
                        "Distinct terminals cannot reuse a port",
                        group.node,
                    ));
                }
                choices.push(PortChoice {
                    node: group.node,
                    port: port.clone(),
                });
            }
            Edit::Place {
                instance: session.blueprint.facilities[root.owner].instance.clone(),
                x: root.x,
                y: root.y,
                rotation: root.rotation,
                ports: choices,
            }
        } else {
            if !ports.is_empty() {
                return Err(error(
                    "learning-selection",
                    "Band actions have no port choices",
                    "ports",
                ));
            }
            if root.kind == 1 {
                Edit::InsertBand {
                    axis: root.axis,
                    at: root.at,
                    count: root.count,
                }
            } else {
                Edit::DeleteBand {
                    axis: root.axis,
                    at: root.at,
                    count: root.count,
                }
            }
        };
        Ok(Decision {
            blueprint_sha256: session.blueprint_sha256.clone(),
            revision: session.revision,
            action,
        })
    }

    // Explicit diagnostic export only; normal collection retains these in Rust.
}
