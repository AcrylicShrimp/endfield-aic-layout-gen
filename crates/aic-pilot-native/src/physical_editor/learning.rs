//! Recipe-independent numerical observations over the compiled physical editor.

mod frame;
mod input;
mod order;
pub mod schema;
mod view;

use std::{collections::BTreeMap, sync::Arc};

use aic_layout::{
    editor::{CompiledProblemV1, CompiledTerminal, PhysicalDraftV1},
    facilities::FacilityPortDirection,
    geometry::Point,
    projection::{self, Port},
};
use serde::{Deserialize, Serialize};

use super::{
    PhysicalActionDomain, PhysicalCandidateAuthority, PhysicalCandidateOptions, PhysicalEdit,
    PortDomain, Result, canonical_block_port_domains, error,
};
use schema::*;

pub use schema::GLOBAL_FEATURES;

#[derive(Default, Clone, Serialize, Deserialize, Debug, PartialEq)]
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortGroup {
    pub graph_node: usize,
    pub equivalence: usize,
    pub port_equivalences: Vec<usize>,
    pub ports: Vec<String>,
    pub features: BTreeMap<i64, (usize, usize)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Domain {
    pub groups: Vec<Vec<PortGroup>>,
}

#[derive(Clone)]
pub struct Observation {
    pub input: Arc<Input>,
    pub domain: Arc<Domain>,
    pub current_ports: Arc<Vec<Option<String>>>,
    pub handle: u64,
    pub source_sha256: String,
    pub revision: u64,
    pub attempts: u64,
    pub done: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct Context<'a> {
    pub source_sha256: &'a str,
    pub initial_score: [u64; 3],
    pub best_score: [u64; 3],
    pub attempts: u64,
    pub max_actions: u64,
    pub route_work: usize,
    pub max_total_route_work: usize,
    pub per_action_route_work: usize,
    pub search_padding: i64,
    pub done: bool,
}

#[derive(Clone)]
pub(crate) struct PreparedDomain {
    pub actions: PhysicalActionDomain,
    pub prefix: Arc<Domain>,
    pub rebuilt: bool,
}

#[derive(Clone)]
struct Root {
    kind: usize,
    invalid: bool,
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
    ports: BTreeMap<i64, Vec<Port>>,
    groups: Vec<Group>,
}

pub(crate) struct Learning {
    problem: Arc<CompiledProblemV1>,
    facilities: Vec<Facility>,
    tables: view::Tables,
    domain: Arc<Domain>,
    roots: Vec<Root>,
    candidates: PhysicalCandidateAuthority,
    options: Option<PhysicalCandidateOptions>,
    domain_revision: Option<u64>,
    domain_attempts: u64,
    cached: Option<(u64, PhysicalCandidateOptions, view::PhysicalView)>,
}

fn floats<const N: usize>(row: [f64; N]) -> [f32; N] {
    row.map(|value| value as f32)
}

fn angle(rotation: i64) -> (f64, f64) {
    let value = rotation as f64 * (std::f64::consts::PI / 180.0);
    (value.sin(), value.cos())
}

fn facility<'a>(
    problem: &'a CompiledProblemV1,
    id: &str,
) -> &'a aic_layout::facilities::FacilityDefinition {
    problem
        .used_catalog
        .facilities
        .iter()
        .find(|definition| definition.id == id)
        .expect("validated compiled facility")
}

fn component<'a>(
    problem: &'a CompiledProblemV1,
    id: &str,
) -> &'a aic_layout::logistics::LogisticsComponentDefinition {
    problem
        .used_catalog
        .components
        .iter()
        .find(|definition| definition.id == id)
        .expect("validated compiled component")
}

impl Learning {
    pub(crate) fn new(problem: Arc<CompiledProblemV1>, initial: &PhysicalDraftV1) -> Result<Self> {
        aic_layout::editor::validate_compiled(&problem, initial).map_err(super::compiled_error)?;
        let mut facilities = Vec::with_capacity(problem.blocks.len());
        for (owner, block) in problem.blocks.iter().enumerate() {
            let definition = facility(&problem, &block.facility);
            let mut rotations = definition.allowed_rotations.clone();
            rotations.sort_unstable();
            let ports = rotations
                .iter()
                .map(|&rotation| {
                    (
                        rotation,
                        projection::rotate(definition, rotation, Point::default()).2,
                    )
                })
                .collect();
            let groups = canonical_block_port_domains(&problem, owner)?
                .into_iter()
                .map(|domain| Group {
                    node: domain.node,
                    equivalence: 0,
                    port_equivalences: Vec::new(),
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
        order::configure(&problem, initial, &mut facilities);
        let tables = view::Tables::new(&problem, &facilities)?;
        let domain = Arc::new(Domain {
            groups: facilities
                .iter()
                .map(|owner| {
                    owner
                        .groups
                        .iter()
                        .map(|group| PortGroup {
                            graph_node: tables.nodes[group.node],
                            equivalence: group.equivalence,
                            port_equivalences: group.port_equivalences.clone(),
                            ports: group.ports.clone(),
                            features: group.features.clone(),
                        })
                        .collect()
                })
                .collect(),
        });
        Ok(Self {
            problem,
            facilities,
            tables,
            domain,
            roots: Vec::new(),
            candidates: PhysicalCandidateAuthority::default(),
            options: None,
            domain_revision: None,
            domain_attempts: 0,
            cached: None,
        })
    }

    pub(crate) fn prepare_domain(
        &mut self,
        draft: &PhysicalDraftV1,
        revision: u64,
        attempts: u64,
        requested: &PhysicalCandidateOptions,
    ) -> Result<PreparedDomain> {
        let rebuilt =
            self.domain_revision != Some(revision) || self.options.as_ref() != Some(requested);
        if rebuilt {
            let port_groups = self
                .facilities
                .iter()
                .map(|owner| {
                    owner
                        .groups
                        .iter()
                        .map(|group| PortDomain {
                            node: group.node,
                            ports: group.ports.clone(),
                        })
                        .collect()
                })
                .collect();
            self.candidates.enumerate_with_port_groups(
                &self.problem,
                draft,
                requested,
                revision,
                attempts,
                port_groups,
            )?;
            self.options = Some(requested.clone());
            self.domain_revision = Some(revision);
            self.domain_attempts = attempts;
            self.cached = None;
        } else if self.domain_attempts != attempts {
            self.candidates.refresh_context(revision, attempts);
            self.domain_attempts = attempts;
        }
        Ok(PreparedDomain {
            actions: self.candidates.domain(),
            prefix: self.domain.clone(),
            rebuilt,
        })
    }

    pub(crate) fn observe(
        &mut self,
        draft: &PhysicalDraftV1,
        revision: u64,
        context: Context<'_>,
        options: &PhysicalCandidateOptions,
    ) -> Result<Observation> {
        let prepared = self.prepare_domain(draft, revision, context.attempts, options)?;
        let input = self.build(draft, revision, context, options)?;
        let mut current_ports = vec![None; input.node_static.len()];
        for (compiled, selected) in draft.selected_ports.iter().enumerate() {
            current_ports[self.tables.nodes[compiled]] = selected.clone();
        }
        Ok(Observation {
            input,
            domain: prepared.prefix,
            current_ports: Arc::new(current_ports),
            handle: prepared.actions.handle,
            source_sha256: context.source_sha256.to_owned(),
            revision,
            attempts: context.attempts,
            done: context.done,
        })
    }

    pub(crate) fn select(
        &self,
        handle: u64,
        revision: u64,
        attempts: u64,
        root: usize,
        ports: &[usize],
    ) -> Result<PhysicalEdit> {
        self.candidates
            .select(handle, revision, attempts, root, ports)
    }

    #[cfg(test)]
    pub(crate) fn action_domain(&self) -> PhysicalActionDomain {
        self.candidates.domain()
    }
}

impl Input {
    pub fn bytes(&self, allocated: bool) -> u64 {
        macro_rules! banks { ($($name:ident),* ) => { 0usize $(+ schema::row_bytes(&self.$name, allocated))* }; }
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
