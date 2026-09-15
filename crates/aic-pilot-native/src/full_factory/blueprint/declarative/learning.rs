//! Retained declarative candidates and typed numerical learning inputs.

mod input;
mod order;
pub mod schema;
mod view;
use schema::*;
pub(crate) mod runtime;
pub(super) mod trace;

use aic_data::facilities::FacilityPortDirection;

use super::*;

pub use schema::GLOBAL_FEATURES;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub translation_steps: Vec<i64>,
    pub band_counts: Vec<i64>,
    pub masking: Option<String>,
}

#[derive(Clone)]
#[allow(dead_code)] // Selection metadata is consumed by the optional learning runtime.
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
    equivalence: usize,
    port_equivalences: Vec<usize>,
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
    pub node_static: Vec<[f32; NODE_STATIC]>,
    pub node_state: Vec<[f32; NODE_STATE]>,
    pub edge_index: Vec<[i64; 2]>,
    #[serde(with = "schema::rows")]
    pub edge_static: Vec<[f32; EDGE_STATIC]>,
    pub edge_state: Vec<[f32; EDGE_STATE]>,
    pub path_cells: Vec<[f32; PATH_FEATURES]>,
    pub path_edges: Vec<i64>,
    pub global_features: Vec<f32>,
    pub action_features: Vec<[f32; ACTION_FEATURES]>,
    pub action_references: Vec<[i64; 2]>,
    pub root_meta: Vec<[i64; 3]>,
    pub kind_log_counts: Vec<f32>,
    pub root_invalid: Vec<bool>,
    pub port_features: Vec<[f32; PORT_FEATURES]>,
}

pub(super) struct Learning {
    options: Options,
    facilities: Vec<Facility>,
    owners: BTreeMap<String, usize>,
    tables: view::Tables,
    cached: Option<(u64, view::PhysicalView)>,
    roots: Vec<Root>,
    candidates: crate::physical_editor::PhysicalCandidateAuthority,
    handle: u64,
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
        if options.band_counts.iter().any(|n| !(1..=GUARD).contains(n))
            || options
                .translation_steps
                .iter()
                .any(|n| !(-GUARD..=GUARD).contains(n))
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
        for (owner, placement) in session.blueprint.facilities.iter().enumerate() {
            owners.insert(placement.instance.clone(), owner);
            let definition = session.common_facility(&placement.facility);
            let mut rotations = definition.allowed_rotations.clone();
            rotations.sort_unstable();
            let ports = rotations
                .iter()
                .map(|&rotation| {
                    (
                        rotation,
                        catalog::rotate(definition, rotation, Point::default()).2,
                    )
                })
                .collect();
            let groups = crate::physical_editor::canonical_block_port_domains(
                &session.common_problem,
                owner,
            )
            .map_err(|failure| error("learning-domain", failure.message, failure.entity))?
            .into_iter()
            .map(|domain| Group {
                node: domain.node,
                equivalence: 0,
                port_equivalences: vec![],
                ports: domain.ports,
                features: BTreeMap::new(),
            })
            .collect();
            facilities.push(Facility {
                rotations,
                ports,
                groups,
            });
        }
        order::configure(session, &mut facilities);
        let tables = view::Tables::new(session, &facilities)?;
        Ok(Self {
            options,
            facilities,
            owners,
            tables,
            cached: None,
            roots: vec![],
            candidates: crate::physical_editor::PhysicalCandidateAuthority::default(),
            handle: 0,
        })
    }

    fn position(&self, session: &Session, node: &Node) -> Result<Point> {
        if let Some(terminal) = &node.terminal
            && let TransportNetworkEndpoint::Facility { instance, .. } = &terminal.endpoint
        {
            let owner = self.owners[instance];
            let placement = &session.state.geometry.placements[owner];
            let name = session.state.ports[node.id]
                .as_ref()
                .ok_or_else(|| error("learning-port", "Missing selected port", node.id))?;
            let port = self.facilities[owner].ports[&placement.rotation]
                .iter()
                .find(|p| &p.id == name)
                .ok_or_else(|| error("learning-port", "Missing catalog port", node.id))?;
            return Ok(port.connection.shifted(placement.x, placement.y));
        }
        Ok(session.state.anchors[node.id])
    }

    pub(super) fn observe(&mut self, session: &Session) -> Result<Arc<Input>> {
        let result = self.build(session)?;
        self.handle = self
            .candidates
            .refresh_context(session.revision, session.attempts);
        Ok(result)
    }

    pub(super) fn selected(
        &self,
        session: &Session,
        handle: u64,
        root: usize,
        ports: &[usize],
    ) -> Result<Decision> {
        let selected = self
            .candidates
            .select(handle, session.revision, session.attempts, root, ports)
            .map_err(|failure| error("learning-selection", failure.message, failure.entity))?;
        let action = match selected {
            crate::physical_editor::PhysicalEdit::Place {
                block,
                position,
                rotation,
                ports,
            } => Edit::Place {
                instance: block,
                x: position.x,
                y: position.y,
                rotation,
                ports: ports
                    .into_iter()
                    .map(|choice| PortChoice {
                        node: choice.node,
                        port: choice.port,
                    })
                    .collect(),
            },
            crate::physical_editor::PhysicalEdit::InsertBand { axis, at, count } => {
                Edit::InsertBand {
                    axis: match axis {
                        crate::physical_editor::Axis::X => Axis::X,
                        crate::physical_editor::Axis::Y => Axis::Y,
                    },
                    at,
                    count,
                }
            }
            crate::physical_editor::PhysicalEdit::DeleteBand { axis, at, count } => {
                Edit::DeleteBand {
                    axis: match axis {
                        crate::physical_editor::Axis::X => Axis::X,
                        crate::physical_editor::Axis::Y => Axis::Y,
                    },
                    at,
                    count,
                }
            }
        };
        Ok(Decision {
            schema: DecisionSchema::Local,
            blueprint_sha256: session.blueprint_sha256.clone(),
            revision: session.revision,
            action,
        })
    }

    // Explicit diagnostic export only; normal collection retains these in Rust.
}

impl Input {
    pub fn bytes(&self, allocated: bool) -> u64 {
        macro_rules! banks { ($($name:ident),* ) => { 0usize $(+ row_bytes(&self.$name, allocated))* }; }
        banks!(
            node_static,
            node_state,
            edge_index,
            edge_static,
            edge_state,
            path_cells,
            path_edges,
            global_features,
            action_features,
            action_references,
            root_meta,
            kind_log_counts,
            root_invalid,
            port_features
        ) as u64
    }
}
