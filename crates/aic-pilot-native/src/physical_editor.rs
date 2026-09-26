//! Source admission for the common physical editor state.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use aic_layout::{
    editor::{
        COMPILED_SCHEMA, CompiledBlock, CompiledComponent, CompiledEdge, CompiledNetwork,
        CompiledNode, CompiledProblemV1, CompiledTerminal, DRAFT_SCHEMA, PhysicalDraftV1,
        PhysicalPose, UsedDefinitionCatalog, validate_compiled,
    },
    geometry::{Point, direction},
    physical::{Layout, Metrics, Route, SCHEMA, Solution},
    validation,
};
use serde::{Deserialize, Serialize};

mod candidates;
mod core;
pub mod learning;
pub mod pool;
mod session;
pub use candidates::{
    PhysicalActionDomain, PhysicalActionRoot, PhysicalCandidateAuthority, PhysicalCandidateOptions,
    PhysicalRootAction,
};
pub use core::{
    Axis, EditWork, PhysicalEdit, PhysicalEditLimits, PortDomain, PortSelection, RouteWork,
    apply_edit, band_allowed, block_port_domains, canonical_block_port_domains, conflicts,
    edge_headings, endpoint_rule, node_position, query_directions, repair_pending, validate_draft,
};
pub(crate) use core::{apply_edit_in_place, conflicts_validated, repair_pending_validated_until};
pub use session::{
    PhysicalSessionLimits, PhysicalSessionOutcome, PhysicalSessionSnapshot, ProceduralSession,
    SessionWork,
};

const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PhysicalEditorDiagnostic {
    pub code: String,
    pub severity: &'static str,
    pub stage: &'static str,
    pub source: &'static str,
    pub entity: String,
    pub message: String,
}

impl std::fmt::Display for PhysicalEditorDiagnostic {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} [{}]: {}",
            self.code, self.entity, self.message
        )
    }
}

impl std::error::Error for PhysicalEditorDiagnostic {}

pub type Result<T> = std::result::Result<T, PhysicalEditorDiagnostic>;

fn error(
    code: &str,
    entity: impl Into<String>,
    message: impl Into<String>,
) -> PhysicalEditorDiagnostic {
    let stage = if code.starts_with("checkpoint") {
        "checkpoint-restore"
    } else if code.starts_with("candidate") || code == "port-domain" {
        "candidate-selection"
    } else if code.starts_with("source")
        || matches!(
            code,
            "catalog-binding" | "bridge-channels" | "bridge-order" | "compiled-validation"
        )
    {
        "procedural-admission"
    } else if code.starts_with("export") || code == "pending-edge" {
        "procedural-export"
    } else if matches!(code, "budget-exhausted" | "elapsed" | "stale-envelope") {
        "session"
    } else if code.starts_with("routing") || code.starts_with("route-") {
        "routing"
    } else {
        "physical-edit"
    };
    diagnostic(stage, code, entity, message)
}

fn diagnostic(
    stage: &'static str,
    code: &str,
    entity: impl Into<String>,
    message: impl Into<String>,
) -> PhysicalEditorDiagnostic {
    PhysicalEditorDiagnostic {
        code: format!("physical-editor.{code}"),
        severity: "error",
        stage,
        source: "aic-physical-layout-v3",
        entity: entity.into(),
        message: message.into(),
    }
}

fn compiled_error(
    failure: aic_layout::editor::CompiledValidationError,
) -> PhysicalEditorDiagnostic {
    diagnostic(
        "compiled-boundary",
        failure.code,
        failure.entity,
        failure.message,
    )
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ProceduralSourceOwner {
    source_sha256: String,
    source_origin: Point,
    original: Layout,
    /// Ordered common edge IDs for every original connection.
    connection_segments: Vec<Vec<usize>>,
    /// Common component index for every original bridge.
    bridge_components: Vec<usize>,
    source_nodes: Vec<(usize, usize)>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ProceduralAdmission {
    pub problem: Arc<CompiledProblemV1>,
    pub draft: PhysicalDraftV1,
    pub initial_metrics: Metrics,
    pub owner: ProceduralSourceOwner,
}

impl ProceduralAdmission {
    /// Decode, validate and compile one bounded source buffer exactly once.
    pub fn compile(source: &[u8], catalogs: &validation::Catalogs) -> Result<Self> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(error(
                "source-size",
                "source",
                "procedural JSON exceeds the 16 MiB admission guard",
            ));
        }
        let original: Layout = serde_json::from_slice(source).map_err(|failure| {
            let is_v2 = serde_json::from_slice::<serde_json::Value>(source)
                .ok()
                .and_then(|value| value.get("schema")?.as_str().map(str::to_owned))
                .is_some_and(|schema| schema == "aic-physical-layout-v2");
            if is_v2 {
                error(
                    "source-schema-migration",
                    "source",
                    "physical-v2 input cannot be decoded as physical-v3; migrate bridges to axis channels and declare catalog junctions",
                )
            } else {
                error(
                    "source-json",
                    "source",
                    format!("cannot decode procedural layout: {failure}"),
                )
            }
        })?;
        if original.schema == "aic-physical-layout-v2" {
            return Err(error(
                "source-schema-migration",
                "source",
                "physical-v2 input must be regenerated as physical-v3 with explicit junctions and axis-owned bridge channels",
            ));
        }
        if !original.problem.terminals.is_empty() || !original.solution.terminal_poses.is_empty() {
            return Err(error(
                "source-terminals",
                "source",
                "the procedural physical editor does not accept boundary-terminal layouts",
            ));
        }
        if !original.solution.junctions.is_empty() {
            return Err(error(
                "source-junctions",
                "source",
                "the procedural physical editor cannot preserve catalog junctions",
            ));
        }
        if original.solution.bridges.iter().any(|bridge| {
            bridge
                .channels
                .iter()
                .any(|channel| channel.connections.len() != 1)
        }) {
            return Err(error(
                "source-bridge-ownership",
                "source",
                "the procedural physical editor supports exactly one route per bridge axis",
            ));
        }
        let initial_metrics = validation::validate(&original, catalogs).map_err(|failure| {
            error(
                "source-validation",
                failure.entity.to_string(),
                failure.message,
            )
        })?;
        let source_sha256 = crate::input::hash(source);
        Self::from_validated(original, initial_metrics, source_sha256, catalogs)
    }

    fn from_validated(
        original: Layout,
        initial_metrics: Metrics,
        source_sha256: String,
        catalogs: &validation::Catalogs,
    ) -> Result<Self> {
        let source_origin = occupied_origin(&original, catalogs)?;
        let canonical = |point: Point| Point {
            x: point.x - source_origin.x,
            y: point.y - source_origin.y,
        };
        let block_indices: BTreeMap<_, _> = original
            .problem
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (block.id.as_str(), index))
            .collect();
        let connection_count = original.problem.connections.len();
        let mut parents: Vec<_> = (0..connection_count).collect();
        let mut component_sizes = vec![1usize; connection_count];
        let mut endpoint_owner = BTreeMap::new();
        for (index, connection) in original.problem.connections.iter().enumerate() {
            for block in [&connection.source, &connection.target] {
                let key = (
                    connection.commodity.clone(),
                    connection.transport,
                    block.clone(),
                );
                if let Some(&other) = endpoint_owner.get(&key) {
                    union(&mut parents, &mut component_sizes, index, other);
                } else {
                    endpoint_owner.insert(key, index);
                }
            }
        }
        let mut network_indices = BTreeMap::new();
        let mut networks = Vec::new();
        let mut connection_networks = Vec::new();
        for (index, connection) in original.problem.connections.iter().enumerate() {
            let key = (
                connection.commodity.clone(),
                connection.transport,
                root(&mut parents, index),
            );
            let network = *network_indices.entry(key.clone()).or_insert_with(|| {
                let index = networks.len();
                networks.push(CompiledNetwork {
                    id: format!("network-{index}"),
                    commodity: key.0,
                    transport: key.1,
                });
                index
            });
            connection_networks.push(network);
        }
        let mut used_facilities = BTreeSet::new();
        let facilities = original
            .problem
            .blocks
            .iter()
            .filter(|block| used_facilities.insert(block.facility.as_str()))
            .map(|block| {
                catalogs
                    .facilities
                    .facility(&block.facility)
                    .expect("validated source facility")
                    .clone()
            })
            .collect();
        let mut used_component_definitions = BTreeSet::new();
        let component_definitions = original
            .solution
            .bridges
            .iter()
            .filter(|bridge| used_component_definitions.insert(bridge.component.as_str()))
            .map(|bridge| {
                catalogs
                    .components
                    .component(&bridge.component)
                    .expect("validated bridge definition")
                    .clone()
            })
            .collect();
        let mut used_transports = BTreeSet::from([
            aic_layout::logistics::TransportKind::Belt,
            aic_layout::logistics::TransportKind::Pipe,
        ]);
        used_transports.extend(
            original
                .problem
                .connections
                .iter()
                .map(|connection| connection.transport),
        );
        let transports = catalogs
            .transports
            .catalog()
            .transports
            .iter()
            .filter(|definition| used_transports.contains(&definition.kind))
            .cloned()
            .collect();

        let mut components: Vec<_> = original
            .solution
            .bridges
            .iter()
            .map(|bridge| {
                let definition = catalogs
                    .components
                    .component(&bridge.component)
                    .expect("validated bridge definition");
                CompiledComponent {
                    id: bridge.id.clone(),
                    definition: bridge.component.clone(),
                    kind: definition.kind,
                    transport: definition.transport,
                    rotation: bridge.rotation,
                    channels: Vec::new(),
                }
            })
            .collect();
        let bridge_indices: BTreeMap<_, _> = original
            .solution
            .bridges
            .iter()
            .enumerate()
            .map(|(index, bridge)| (bridge.id.as_str(), index))
            .collect();
        let bridge_components: Vec<_> = (0..components.len()).collect();
        let routes: BTreeMap<_, _> = original
            .solution
            .routes
            .iter()
            .map(|route| (route.connection.as_str(), route))
            .collect();
        let mut bridges_by_connection: BTreeMap<&str, Vec<(Point, &str)>> = BTreeMap::new();
        for bridge in &original.solution.bridges {
            for channel in &bridge.channels {
                for connection in &channel.connections {
                    bridges_by_connection
                        .entry(connection.as_str())
                        .or_default()
                        .push((bridge.position, bridge.id.as_str()));
                }
            }
        }

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut paths = Vec::new();
        let mut selected_ports = Vec::new();
        let mut anchors = Vec::new();
        let mut connection_segments = vec![Vec::new(); original.problem.connections.len()];
        let mut source_nodes = Vec::new();
        for (connection_index, connection) in original.problem.connections.iter().enumerate() {
            let route = routes[connection.id.as_str()];
            let network = connection_networks[connection_index];
            let source_node = nodes.len();
            nodes.push(CompiledNode {
                id: source_node,
                source_id: format!("{}:source", connection.id),
                network,
                baseline_position: canonical(route.cells[0]),
                terminal: Some(CompiledTerminal::Facility {
                    block: block_indices[connection.source.as_str()],
                    source_terminal: format!("{}:source", connection.id),
                    original_port: route.source_port.clone().ok_or_else(|| {
                        error(
                            "terminal-endpoint",
                            &connection.id,
                            "physical editor does not edit boundary-terminal routes",
                        )
                    })?,
                    direction: aic_layout::facilities::FacilityPortDirection::Output,
                    rate: connection.rate,
                }),
                components: Vec::new(),
            });
            selected_ports.push(Some(route.source_port.clone().ok_or_else(|| {
                error(
                    "terminal-endpoint",
                    &connection.id,
                    "physical editor does not edit boundary-terminal routes",
                )
            })?));
            anchors.push(canonical(route.cells[0]));

            let target_node = source_node + 1;
            nodes.push(CompiledNode {
                id: target_node,
                source_id: format!("{}:target", connection.id),
                network,
                baseline_position: canonical(*route.cells.last().unwrap()),
                terminal: Some(CompiledTerminal::Facility {
                    block: block_indices[connection.target.as_str()],
                    source_terminal: format!("{}:target", connection.id),
                    original_port: route.target_port.clone().ok_or_else(|| {
                        error(
                            "terminal-endpoint",
                            &connection.id,
                            "physical editor does not edit boundary-terminal routes",
                        )
                    })?,
                    direction: aic_layout::facilities::FacilityPortDirection::Input,
                    rate: connection.rate,
                }),
                components: Vec::new(),
            });
            selected_ports.push(Some(route.target_port.clone().ok_or_else(|| {
                error(
                    "terminal-endpoint",
                    &connection.id,
                    "physical editor does not edit boundary-terminal routes",
                )
            })?));
            anchors.push(canonical(*route.cells.last().unwrap()));
            source_nodes.push((source_node, target_node));

            let declared_bridges = bridges_by_connection
                .get(connection.id.as_str())
                .map_or(&[][..], Vec::as_slice);
            let mut bridges_at = BTreeMap::<Point, Vec<&str>>::new();
            for &(position, bridge_id) in declared_bridges {
                bridges_at.entry(position).or_default().push(bridge_id);
            }
            let mut cuts = Vec::new();
            for (cell_index, point) in route.cells.iter().enumerate() {
                if let Some(bridges) = bridges_at.get(point) {
                    for &bridge_id in bridges {
                        cuts.push((cell_index, bridge_id));
                    }
                }
            }
            cuts.sort_by_key(|&(cell_index, _)| cell_index);
            if cuts.len() != declared_bridges.len() {
                return Err(error(
                    "bridge-order",
                    &connection.id,
                    "declared bridge use is missing or ambiguous in the route",
                ));
            }
            let mut ordered_nodes = vec![(0usize, source_node)];
            for (cell_index, bridge_id) in cuts {
                let component = bridge_indices[bridge_id];
                let node = nodes.len();
                nodes.push(CompiledNode {
                    id: node,
                    source_id: format!("{bridge_id}:{}", connection.id),
                    network,
                    baseline_position: canonical(route.cells[cell_index]),
                    terminal: None,
                    components: vec![component],
                });
                selected_ports.push(None);
                anchors.push(canonical(route.cells[cell_index]));
                components[component].channels.push(node);
                ordered_nodes.push((cell_index, node));
            }
            ordered_nodes.push((route.cells.len() - 1, target_node));
            for pair in ordered_nodes.windows(2) {
                let ((begin, source), (end, target)) = (pair[0], pair[1]);
                let cells: Vec<_> = route.cells[begin..=end]
                    .iter()
                    .copied()
                    .map(canonical)
                    .collect();
                let first = cells
                    .windows(2)
                    .next()
                    .and_then(|pair| direction(pair[0], pair[1]));
                let last = cells
                    .windows(2)
                    .last()
                    .and_then(|pair| direction(pair[0], pair[1]));
                let edge = edges.len();
                edges.push(CompiledEdge {
                    id: edge,
                    source_id: format!("{}:{begin}-{end}", connection.id),
                    network,
                    source,
                    target,
                    rate: connection.rate,
                    first,
                    last,
                    zero_length: cells.len() == 1,
                });
                paths.push(Some(cells));
                connection_segments[connection_index].push(edge);
            }
        }
        if components
            .iter()
            .any(|component| component.channels.len() != 2)
        {
            return Err(error(
                "bridge-channels",
                "bridges",
                "each declared bridge must compile to two distinct channel nodes",
            ));
        }

        let poses_by_block: BTreeMap<_, _> = original
            .solution
            .poses
            .iter()
            .map(|pose| (pose.block.as_str(), pose))
            .collect();
        let problem = CompiledProblemV1 {
            schema: COMPILED_SCHEMA.into(),
            catalog_sha256: catalogs.identity.clone(),
            used_catalog: UsedDefinitionCatalog {
                facilities,
                components: component_definitions,
                transports,
            },
            blocks: original
                .problem
                .blocks
                .iter()
                .map(|block| CompiledBlock {
                    id: block.id.clone(),
                    facility: block.facility.clone(),
                    baseline_rotation: poses_by_block[block.id.as_str()].rotation,
                })
                .collect(),
            networks,
            nodes,
            edges,
            components,
        };
        let draft = PhysicalDraftV1 {
            schema: DRAFT_SCHEMA.into(),
            poses: original
                .problem
                .blocks
                .iter()
                .map(|block| {
                    let pose = poses_by_block[block.id.as_str()];
                    PhysicalPose {
                        position: canonical(pose.position),
                        rotation: pose.rotation,
                    }
                })
                .collect(),
            selected_ports,
            anchors,
            component_positions: original
                .solution
                .bridges
                .iter()
                .map(|bridge| canonical(bridge.position))
                .collect(),
            paths,
        };
        validate_compiled(&problem, &draft).map_err(compiled_error)?;
        let owner = ProceduralSourceOwner {
            source_sha256,
            source_origin,
            original,
            connection_segments,
            bridge_components,
            source_nodes,
        };
        let admission = Self {
            problem: Arc::new(problem),
            draft,
            initial_metrics,
            owner,
        };
        admission.export(catalogs)?;
        Ok(admission)
    }

    /// Rebuild and independently validate a complete procedural source layout.
    pub fn export(&self, catalogs: &validation::Catalogs) -> Result<Layout> {
        validate_compiled(&self.problem, &self.draft).map_err(compiled_error)?;
        let restore = |point: Point| -> Result<Point> {
            Ok(Point {
                x: point
                    .x
                    .checked_add(self.owner.source_origin.x)
                    .ok_or_else(|| {
                        error("coordinate-overflow", "source", "source-frame x overflow")
                    })?,
                y: point
                    .y
                    .checked_add(self.owner.source_origin.y)
                    .ok_or_else(|| {
                        error("coordinate-overflow", "source", "source-frame y overflow")
                    })?,
            })
        };
        let mut poses = self.owner.original.solution.poses.clone();
        let pose_indices: BTreeMap<_, _> = poses
            .iter()
            .enumerate()
            .map(|(index, pose)| (pose.block.clone(), index))
            .collect();
        for (block, pose) in self.problem.blocks.iter().zip(&self.draft.poses) {
            let target = &mut poses[pose_indices[&block.id]];
            target.position = restore(pose.position)?;
            target.rotation = pose.rotation;
        }
        let mut routes = Vec::with_capacity(self.owner.connection_segments.len());
        for (connection, segments) in self
            .owner
            .original
            .problem
            .connections
            .iter()
            .zip(&self.owner.connection_segments)
        {
            let mut cells = Vec::new();
            for &edge in segments {
                let path = self.draft.paths[edge].as_ref().ok_or_else(|| {
                    error(
                        "pending-edge",
                        &self.problem.edges[edge].source_id,
                        "cannot export an unfinished procedural draft",
                    )
                })?;
                for &point in path.iter().skip(usize::from(!cells.is_empty())) {
                    cells.push(restore(point)?);
                }
            }
            let (source_node, target_node) = self.owner.source_nodes[routes.len()];
            routes.push(Route {
                connection: connection.id.clone(),
                source_port: Some(self.draft.selected_ports[source_node].clone().ok_or_else(
                    || error("missing-port", &connection.id, "source port is missing"),
                )?),
                target_port: Some(self.draft.selected_ports[target_node].clone().ok_or_else(
                    || error("missing-port", &connection.id, "target port is missing"),
                )?),
                cells,
            });
        }
        let mut bridges = self.owner.original.solution.bridges.clone();
        for (bridge, &component) in bridges.iter_mut().zip(&self.owner.bridge_components) {
            bridge.position = restore(self.draft.component_positions[component])?;
            bridge.rotation = self.problem.components[component].rotation;
        }
        let layout = Layout {
            schema: SCHEMA.into(),
            catalog_sha256: self.problem.catalog_sha256.clone(),
            problem: self.owner.original.problem.clone(),
            solution: Solution {
                poses,
                terminal_poses: vec![],
                routes,
                bridges,
                junctions: vec![],
            },
        };
        validation::validate(&layout, catalogs).map_err(|failure| {
            error(
                "export-validation",
                failure.entity.to_string(),
                failure.message,
            )
        })?;
        Ok(layout)
    }

    pub fn source_sha256(&self) -> &str {
        &self.owner.source_sha256
    }

    pub fn source_origin(&self) -> Point {
        self.owner.source_origin
    }

    /// Apply one complete physical action and run the shared bounded repair pass.
    pub fn edit(&mut self, action: PhysicalEdit, limits: PhysicalEditLimits) -> Result<EditWork> {
        apply_edit(&self.problem, &mut self.draft, action)?;
        repair_pending(&self.problem, &mut self.draft, limits)
    }
}

fn occupied_origin(layout: &Layout, catalogs: &validation::Catalogs) -> Result<Point> {
    let poses: BTreeMap<_, _> = layout
        .solution
        .poses
        .iter()
        .map(|pose| (pose.block.as_str(), pose))
        .collect();
    let mut low = Point {
        x: i64::MAX,
        y: i64::MAX,
    };
    let mut include = |point: Point| {
        low.x = low.x.min(point.x);
        low.y = low.y.min(point.y);
    };
    for block in &layout.problem.blocks {
        let pose = poses[block.id.as_str()];
        let definition = catalogs
            .facilities
            .facility(&block.facility)
            .expect("validated block facility");
        let (width, height) = if pose.rotation % 180 == 0 {
            (definition.footprint.width, definition.footprint.height)
        } else {
            (definition.footprint.height, definition.footprint.width)
        };
        include(pose.position);
        include(Point {
            x: pose.position.x + width - 1,
            y: pose.position.y + height - 1,
        });
    }
    for point in layout
        .solution
        .routes
        .iter()
        .flat_map(|route| route.cells.iter().copied())
        .chain(layout.solution.bridges.iter().map(|bridge| bridge.position))
    {
        include(point);
    }
    if low.x == i64::MAX {
        return Err(error(
            "empty-frame",
            "source",
            "procedural source has no occupied geometry",
        ));
    }
    Ok(low)
}

fn root(parents: &mut [usize], mut index: usize) -> usize {
    let mut representative = index;
    while parents[representative] != representative {
        representative = parents[representative];
    }
    while parents[index] != index {
        let next = parents[index];
        parents[index] = representative;
        index = next;
    }
    representative
}

fn union(parents: &mut [usize], sizes: &mut [usize], a: usize, b: usize) {
    let (mut a, mut b) = (root(parents, a), root(parents, b));
    if a != b {
        if sizes[a] < sizes[b] {
            std::mem::swap(&mut a, &mut b);
        }
        parents[b] = a;
        sizes[a] += sizes[b];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aic_layout::{
        facilities::{FacilityCatalog, ValidatedFacilityCatalog},
        logistics::{
            LogisticsComponentCatalog, TransportCatalog, ValidatedLogisticsComponentCatalog,
            ValidatedTransportCatalog,
        },
    };
    use sha2::{Digest, Sha256};
    use std::{fs, path::Path, time::Instant};

    const FACILITIES: &[u8] = include_bytes!("../../../data/game/normalized/facilities.json");

    #[test]
    fn disjoint_set_compresses_adversarial_connection_unions() {
        let count = 32_768;
        let mut parents = (0..count).collect::<Vec<_>>();
        let mut sizes = vec![1usize; count];
        for index in 1..count {
            union(&mut parents, &mut sizes, index, index - 1);
        }
        let representative = root(&mut parents, count - 1);
        for index in 0..count {
            assert_eq!(root(&mut parents, index), representative);
        }
        assert!(parents.iter().all(|&parent| parent == representative));
    }
    const TRANSPORTS: &[u8] = include_bytes!("../../../data/game/normalized/transports.json");
    const COMPONENTS: &[u8] =
        include_bytes!("../../../data/game/normalized/logistics-components.json");
    const NO_BRIDGE: &[u8] = include_bytes!("../tests/fixtures/procedural-no-bridge.json");
    const BRIDGES: &[u8] = include_bytes!("../tests/fixtures/procedural-bridges.json");

    #[test]
    fn physical_v2_source_reports_explicit_migration_diagnostic() {
        let mut old: serde_json::Value = serde_json::from_slice(NO_BRIDGE).unwrap();
        old["schema"] = "aic-physical-layout-v2".into();
        old["solution"].as_object_mut().unwrap().remove("junctions");
        let source = serde_json::to_vec(&old).unwrap();
        let diagnostic = ProceduralAdmission::compile(&source, &catalogs())
            .err()
            .unwrap();
        assert_eq!(diagnostic.code, "physical-editor.source-schema-migration");
    }

    fn catalogs() -> validation::Catalogs {
        let sources = [
            ("facilities.json", FACILITIES),
            ("transports.json", TRANSPORTS),
            ("logistics-components.json", COMPONENTS),
        ];
        let mut identity = Sha256::new();
        for (name, bytes) in sources {
            identity.update((name.len() as u64).to_le_bytes());
            identity.update(name.as_bytes());
            identity.update((bytes.len() as u64).to_le_bytes());
            identity.update(bytes);
        }
        validation::Catalogs {
            identity: identity
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            facilities: ValidatedFacilityCatalog::try_from_catalog(
                serde_json::from_slice::<FacilityCatalog>(FACILITIES).unwrap(),
            )
            .unwrap(),
            transports: ValidatedTransportCatalog::try_from_catalog(
                serde_json::from_slice::<TransportCatalog>(TRANSPORTS).unwrap(),
            )
            .unwrap(),
            components: ValidatedLogisticsComponentCatalog::try_from_catalog(
                serde_json::from_slice::<LogisticsComponentCatalog>(COMPONENTS).unwrap(),
            )
            .unwrap(),
        }
    }

    fn translated(source: &[u8], x: i64, y: i64) -> Vec<u8> {
        let mut layout: Layout = serde_json::from_slice(source).unwrap();
        let shift = |point: &mut Point| {
            point.x += x;
            point.y += y;
        };
        for pose in &mut layout.solution.poses {
            shift(&mut pose.position);
        }
        for route in &mut layout.solution.routes {
            for point in &mut route.cells {
                shift(point);
            }
        }
        for bridge in &mut layout.solution.bridges {
            shift(&mut bridge.position);
        }
        serde_json::to_vec(&layout).unwrap()
    }

    fn renamed(source: &[u8]) -> Vec<u8> {
        let mut layout: Layout = serde_json::from_slice(source).unwrap();
        let blocks: BTreeMap<_, _> = layout
            .problem
            .blocks
            .iter_mut()
            .enumerate()
            .map(|(index, block)| {
                let old = std::mem::replace(&mut block.id, format!("renamed-block-{index}"));
                (old, block.id.clone())
            })
            .collect();
        let connections: BTreeMap<_, _> = layout
            .problem
            .connections
            .iter_mut()
            .enumerate()
            .map(|(index, connection)| {
                connection.source = blocks[&connection.source].clone();
                connection.target = blocks[&connection.target].clone();
                let old =
                    std::mem::replace(&mut connection.id, format!("renamed-connection-{index}"));
                (old, connection.id.clone())
            })
            .collect();
        for pose in &mut layout.solution.poses {
            pose.block = blocks[&pose.block].clone();
        }
        for route in &mut layout.solution.routes {
            route.connection = connections[&route.connection].clone();
        }
        for (index, bridge) in layout.solution.bridges.iter_mut().enumerate() {
            bridge.id = format!("renamed-bridge-{index}");
            for channel in &mut bridge.channels {
                for connection in &mut channel.connections {
                    *connection = connections[connection].clone();
                }
            }
        }
        serde_json::to_vec(&layout).unwrap()
    }

    fn pending_session(source: &[u8], catalogs: &validation::Catalogs) -> ProceduralSession {
        let admitted = ProceduralAdmission::compile(source, catalogs).unwrap();
        let max_x = admitted
            .draft
            .paths
            .iter()
            .flatten()
            .flatten()
            .map(|point| point.x)
            .max()
            .unwrap();
        let action = (1..max_x)
            .map(|at| PhysicalEdit::InsertBand {
                axis: Axis::X,
                at,
                count: 1,
            })
            .find(|action| {
                let mut draft = admitted.draft.clone();
                apply_edit(&admitted.problem, &mut draft, action.clone()).is_ok()
                    && draft.paths.iter().any(Option::is_none)
            })
            .expect("fixture must expose a routing edit");
        let limits = PhysicalSessionLimits {
            max_actions: 10,
            elapsed_ms: 60_000,
            route_expansions: 1,
            max_total_route_expansions: 100,
            search_padding: 8,
        };
        let mut session = ProceduralSession::new(source, catalogs, limits).unwrap();
        let source_sha256 = session.source_sha256().to_owned();
        let outcome = session.step(&source_sha256, session.revision(), action, catalogs);
        assert_eq!(outcome.status, "draft_accepted");
        assert!(session.current().paths.iter().any(Option::is_none));
        session
    }

    #[test]
    fn procedural_sources_round_trip_through_the_common_boundary() {
        let catalogs = catalogs();
        for source in [NO_BRIDGE, BRIDGES] {
            let original: Layout = serde_json::from_slice(source).unwrap();
            let admission = ProceduralAdmission::compile(source, &catalogs).unwrap();
            assert_eq!(admission.export(&catalogs).unwrap(), original);
            assert_eq!(admission.source_sha256(), crate::input::hash(source));
        }
    }

    #[test]
    fn bridge_channels_are_distinct_and_translation_is_not_identity() {
        let catalogs = catalogs();
        let original = ProceduralAdmission::compile(BRIDGES, &catalogs).unwrap();
        for component in &original.problem.components {
            assert_eq!(component.channels.len(), 2);
            assert_ne!(component.channels[0], component.channels[1]);
            assert_eq!(
                original.draft.anchors[component.channels[0]],
                original.draft.anchors[component.channels[1]]
            );
        }
        let shifted_bytes = translated(BRIDGES, 17, 23);
        let shifted = ProceduralAdmission::compile(&shifted_bytes, &catalogs).unwrap();
        assert_eq!(shifted.problem, original.problem);
        assert_eq!(shifted.draft, original.draft);
        assert_ne!(shifted.source_sha256(), original.source_sha256());
        assert_eq!(
            shifted.export(&catalogs).unwrap(),
            serde_json::from_slice(&shifted_bytes).unwrap()
        );
    }

    #[test]
    fn opaque_source_relabeling_preserves_compiled_identity() {
        let catalogs = catalogs();
        let original = ProceduralAdmission::compile(NO_BRIDGE, &catalogs).unwrap();
        let renamed = ProceduralAdmission::compile(&renamed(NO_BRIDGE), &catalogs).unwrap();
        assert_ne!(renamed.problem, original.problem);
        let limits = aic_layout::physical_identity::Limits {
            max_individualization_nodes: 1_000_000,
        };
        let a = aic_layout::editor::compiled_identity(&original.problem, &original.draft, &limits)
            .unwrap();
        let b = aic_layout::editor::compiled_identity(&renamed.problem, &renamed.draft, &limits)
            .unwrap();
        assert_eq!(a, b);
        assert_ne!(renamed.source_sha256(), original.source_sha256());
    }

    fn learning_limits() -> PhysicalSessionLimits {
        PhysicalSessionLimits {
            max_actions: 10,
            elapsed_ms: 60_000,
            route_expansions: 1_000_000,
            max_total_route_expansions: 10_000_000,
            search_padding: 8,
        }
    }

    fn learning_options() -> PhysicalCandidateOptions {
        PhysicalCandidateOptions {
            translation_steps: vec![-1, 1],
            band_counts: vec![1],
            structural_band_mask: true,
        }
    }

    #[test]
    fn procedural_learning_is_translation_and_opaque_relabel_invariant() {
        let catalogs = catalogs();
        let translated = translated(NO_BRIDGE, 17, 23);
        let renamed = renamed(NO_BRIDGE);
        let mut sessions = [
            ProceduralSession::new(NO_BRIDGE, &catalogs, learning_limits()).unwrap(),
            ProceduralSession::new(&translated, &catalogs, learning_limits()).unwrap(),
            ProceduralSession::new(&renamed, &catalogs, learning_limits()).unwrap(),
        ];
        for session in &sessions {
            let kinds: BTreeSet<_> = session
                .problem()
                .used_catalog
                .transports
                .iter()
                .map(|definition| definition.kind)
                .collect();
            assert_eq!(
                kinds,
                BTreeSet::from([
                    aic_layout::logistics::TransportKind::Belt,
                    aic_layout::logistics::TransportKind::Pipe,
                ])
            );
        }
        let options = learning_options();
        let observations = sessions
            .iter_mut()
            .map(|session| session.observe_learning(&options).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(*observations[0].input, *observations[1].input);
        assert_eq!(*observations[0].input, *observations[2].input);
        assert_eq!(*observations[0].domain, *observations[1].domain);
        assert_eq!(*observations[0].domain, *observations[2].domain);
        assert_ne!(observations[0].source_sha256, observations[1].source_sha256);
        assert_ne!(observations[0].source_sha256, observations[2].source_sha256);
    }

    #[test]
    fn procedural_learning_reuses_one_domain_and_refreshes_attempt_context() {
        let catalogs = catalogs();
        let mut session = ProceduralSession::new(NO_BRIDGE, &catalogs, learning_limits()).unwrap();
        let options = learning_options();
        let first = session.observe_learning(&options).unwrap();
        let public = session.observe_actions(&options).unwrap();
        assert_eq!(first.handle, public.handle);
        assert_eq!(first.revision, public.revision);
        assert_eq!(first.attempts, public.attempts);

        let rejected = session.step_candidate(
            &first.source_sha256,
            first.revision,
            first.handle,
            usize::MAX,
            &[],
            &catalogs,
        );
        assert!(rejected.charged);
        assert_eq!(rejected.status, "rejected");
        assert_eq!(session.revision(), first.revision);

        let refreshed = session.observe_learning(&options).unwrap();
        assert_ne!(refreshed.handle, first.handle);
        assert_eq!(refreshed.attempts, first.attempts + 1);
        assert_eq!(refreshed.input.action_features, first.input.action_features);
        assert_eq!(
            refreshed.input.action_references,
            first.input.action_references
        );
        assert_eq!(refreshed.input.root_meta, first.input.root_meta);
        assert_eq!(refreshed.input.root_invalid, first.input.root_invalid);
        assert_ne!(refreshed.input.global_features, first.input.global_features);

        let changed = PhysicalCandidateOptions {
            translation_steps: vec![-2, 2],
            ..options
        };
        let changed_domain = session.observe_actions(&changed).unwrap();
        assert_ne!(changed_domain.handle, refreshed.handle);
        let stale = session.step_learning(&refreshed, 0, &[], &catalogs);
        assert!(stale.charged);
        assert_eq!(stale.status, "rejected");
    }

    fn normalized_domain(
        mut domain: PhysicalActionDomain,
        _problem: &CompiledProblemV1,
    ) -> PhysicalActionDomain {
        domain.handle = 0;
        domain
    }

    #[test]
    fn procedural_candidates_share_complete_domains_masks_and_stale_handles() {
        let catalogs = catalogs();
        let limits = PhysicalSessionLimits {
            max_actions: 10,
            elapsed_ms: 60_000,
            route_expansions: 1_000_000,
            max_total_route_expansions: 10_000_000,
            search_padding: 8,
        };
        let options = PhysicalCandidateOptions {
            translation_steps: vec![-1, 0, 1],
            band_counts: vec![1],
            structural_band_mask: true,
        };
        let mut session = ProceduralSession::new(NO_BRIDGE, &catalogs, limits).unwrap();
        let domain = session.observe_actions(&options).unwrap();
        assert!(
            domain
                .roots
                .iter()
                .any(|root| matches!(root.action, PhysicalRootAction::Place { .. }))
        );
        assert!(
            domain
                .roots
                .iter()
                .any(|root| matches!(root.action, PhysicalRootAction::InsertBand { .. }))
        );
        assert!(
            domain
                .roots
                .iter()
                .any(|root| matches!(root.action, PhysicalRootAction::DeleteBand { .. }))
        );
        assert!(domain.roots.iter().any(|root| root.invalid));

        let (place_index, place_ports) = domain
            .roots
            .iter()
            .enumerate()
            .find_map(|(index, root)| match &root.action {
                PhysicalRootAction::Place { port_group, .. } if !root.invalid => {
                    let choices = domain.port_groups[*port_group]
                        .iter()
                        .map(|port_domain| {
                            let selected = session.current().selected_ports[port_domain.node]
                                .as_ref()
                                .unwrap();
                            port_domain.ports.iter().position(|port| port == selected)
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some((index, choices))
                }
                _ => None,
            })
            .unwrap();
        let mut selection_authority = PhysicalCandidateAuthority::default();
        let selection_domain = selection_authority.publish(
            session.problem(),
            0,
            0,
            domain.port_groups.clone(),
            domain.roots.clone(),
        );
        assert!(matches!(
            selection_authority
                .select(selection_domain.handle, 0, 0, place_index, &place_ports)
                .unwrap(),
            PhysicalEdit::Place { .. }
        ));

        let band_index = domain
            .roots
            .iter()
            .position(|root| {
                !root.invalid
                    && matches!(
                        root.action,
                        PhysicalRootAction::InsertBand { .. }
                            | PhysicalRootAction::DeleteBand { .. }
                    )
            })
            .unwrap();
        let source = session.source_sha256().to_string();
        let outcome = session.step_candidate(
            &source,
            domain.revision,
            domain.handle,
            band_index,
            &[],
            &catalogs,
        );
        assert!(outcome.charged);
        assert_eq!(outcome.attempts, 1);
        let stale = session.step_candidate(
            &source,
            domain.revision,
            domain.handle,
            band_index,
            &[],
            &catalogs,
        );
        assert!(stale.charged);
        assert_eq!(
            stale.diagnostics[0].code,
            "physical-editor.candidate-handle"
        );

        let shifted_source = translated(NO_BRIDGE, 17, 23);
        let renamed_source = renamed(NO_BRIDGE);
        let admitted = ProceduralAdmission::compile(NO_BRIDGE, &catalogs).unwrap();
        let shifted = ProceduralAdmission::compile(&shifted_source, &catalogs).unwrap();
        let renamed = ProceduralAdmission::compile(&renamed_source, &catalogs).unwrap();
        let enumerate = |admission: &ProceduralAdmission| {
            let mut authority = PhysicalCandidateAuthority::default();
            authority
                .enumerate(&admission.problem, &admission.draft, &options, 0, 0)
                .unwrap()
        };
        let base = normalized_domain(enumerate(&admitted), &admitted.problem);
        assert_eq!(
            base,
            normalized_domain(enumerate(&shifted), &shifted.problem)
        );
        assert_eq!(
            base,
            normalized_domain(enumerate(&renamed), &renamed.problem)
        );
    }

    #[test]
    fn incomplete_procedural_draft_is_never_exported() {
        let catalogs = catalogs();
        let mut admission = ProceduralAdmission::compile(NO_BRIDGE, &catalogs).unwrap();
        admission.draft.paths[0] = None;
        let failure = admission.export(&catalogs).unwrap_err();
        assert_eq!(failure.code, "physical-editor.pending-edge");
    }

    #[test]
    fn procedural_source_executes_shared_band_and_routing_engine() {
        let catalogs = catalogs();
        let original: Layout = serde_json::from_slice(NO_BRIDGE).unwrap();
        let admitted = ProceduralAdmission::compile(NO_BRIDGE, &catalogs).unwrap();
        let max_x = admitted
            .draft
            .poses
            .iter()
            .map(|pose| pose.position.x)
            .chain(
                admitted
                    .draft
                    .paths
                    .iter()
                    .flatten()
                    .flatten()
                    .map(|point| point.x),
            )
            .max()
            .unwrap();
        let mut accepted = None;
        for at in 1..max_x {
            let mut candidate = admitted.clone();
            let result = candidate.edit(
                PhysicalEdit::InsertBand {
                    axis: Axis::X,
                    at,
                    count: 1,
                },
                PhysicalEditLimits {
                    route_expansions: 1_000_000,
                    search_padding: 8,
                },
            );
            if let Ok(work) = result
                && work.committed
                && candidate.export(&catalogs).is_ok()
                && candidate.draft != admitted.draft
            {
                accepted = Some((candidate, work));
                break;
            }
        }
        let (edited, work) = accepted.expect("fixture must have one editable empty band boundary");
        assert!(work.route_calls > 0);
        assert_ne!(edited.export(&catalogs).unwrap(), original);
    }

    #[test]
    fn procedural_v4_snapshot_retains_pending_state_and_rejects_wrong_source() {
        let catalogs = catalogs();
        let session = pending_session(NO_BRIDGE, &catalogs);
        let encoded = serde_json::to_vec(&session.checkpoint()).unwrap();
        let snapshot: PhysicalSessionSnapshot = serde_json::from_slice(&encoded).unwrap();
        let restored = ProceduralSession::from_checkpoint(snapshot, NO_BRIDGE, &catalogs).unwrap();
        assert_eq!(restored.current(), session.current());
        assert_eq!(restored.best(), session.best());
        assert_eq!(restored.revision(), session.revision());

        let shifted = translated(NO_BRIDGE, 1, 1);
        let snapshot: PhysicalSessionSnapshot = serde_json::from_slice(&encoded).unwrap();
        let failure = ProceduralSession::from_checkpoint(snapshot, &shifted, &catalogs)
            .err()
            .unwrap();
        assert_eq!(failure.code, "physical-editor.checkpoint-binding");
    }

    #[test]
    fn procedural_v4_snapshot_rejects_oversized_pending_state_before_enumeration() {
        let catalogs = catalogs();
        let session = pending_session(NO_BRIDGE, &catalogs);
        let mut snapshot = session.checkpoint();
        snapshot.current.poses[0].position.x += 1_000_001;
        let failure = ProceduralSession::from_checkpoint(snapshot, NO_BRIDGE, &catalogs)
            .err()
            .unwrap();
        assert_eq!(failure.code, "physical-editor.coordinate-guard");
        assert_eq!(failure.stage, "compiled-boundary");
    }

    #[test]
    fn common_multi_edge_repair_stops_at_the_between_edge_deadline() {
        let catalogs = catalogs();
        let session = pending_session(NO_BRIDGE, &catalogs);
        let problem = session.problem().clone();
        let mut draft = session.current().clone();
        draft.paths.fill(None);
        let before = draft.clone();
        assert!(draft.paths.iter().filter(|path| path.is_none()).count() > 1);
        let mut checks = 0;
        let work = repair_pending_validated_until(
            &problem,
            &mut draft,
            PhysicalEditLimits {
                route_expansions: 100_000,
                search_padding: 5,
            },
            || {
                checks += 1;
                checks > 1
            },
        )
        .unwrap();
        assert_eq!(checks, 2);
        assert_eq!(work.route_calls, 1);
        assert!(work.expanded > 0);
        assert_eq!(work.routes.last().unwrap().status, "elapsed");
        assert!(!work.committed);
        assert_eq!(draft, before);
    }

    #[test]
    #[ignore = "explicit two-process checkpoint save/restore probe"]
    fn procedural_checkpoint_process_probe() {
        let mode = std::env::var("AIC_PROCEDURAL_CHECKPOINT_MODE").unwrap();
        let path = std::env::var("AIC_PROCEDURAL_CHECKPOINT_PATH").unwrap();
        let catalogs = catalogs();
        match mode.as_str() {
            "save" => {
                let session = pending_session(NO_BRIDGE, &catalogs);
                fs::write(
                    &path,
                    serde_json::to_vec_pretty(&session.checkpoint()).unwrap(),
                )
                .unwrap();
                println!(
                    "{}",
                    serde_json::json!({
                        "mode": mode,
                        "source_sha256": session.source_sha256(),
                        "revision": session.revision(),
                        "pending_paths": session.current().paths.iter().filter(|path| path.is_none()).count(),
                        "snapshot_bytes": fs::metadata(&path).unwrap().len(),
                    })
                );
            }
            "restore" => {
                let encoded = fs::read(&path).unwrap();
                let snapshot: PhysicalSessionSnapshot = serde_json::from_slice(&encoded).unwrap();
                let restored =
                    ProceduralSession::from_checkpoint(snapshot, NO_BRIDGE, &catalogs).unwrap();
                assert!(restored.current().paths.iter().any(Option::is_none));
                let shifted = translated(NO_BRIDGE, 1, 1);
                let snapshot: PhysicalSessionSnapshot = serde_json::from_slice(&encoded).unwrap();
                let failure = ProceduralSession::from_checkpoint(snapshot, &shifted, &catalogs)
                    .err()
                    .unwrap();
                assert_eq!(failure.code, "physical-editor.checkpoint-binding");
                println!(
                    "{}",
                    serde_json::json!({
                        "mode": mode,
                        "source_sha256": restored.source_sha256(),
                        "revision": restored.revision(),
                        "pending_paths": restored.current().paths.iter().filter(|path| path.is_none()).count(),
                        "wrong_source_code": failure.code,
                    })
                );
            }
            other => panic!("unsupported probe mode {other}"),
        }
    }

    #[test]
    #[ignore = "writes an explicit bounded release-mode procedural continuity report"]
    fn procedural_reference() {
        let output = std::env::var("AIC_PROCEDURAL_REFERENCE_REPORT").unwrap();
        let generated = std::env::var("AIC_PROCEDURAL_16_SOURCE").unwrap();
        let generated = fs::read(generated).unwrap();
        let catalogs = catalogs();
        let mut reports = Vec::new();
        for (name, source) in [("blocks-8", NO_BRIDGE), ("blocks-16", generated.as_slice())] {
            let started = Instant::now();
            let layout: Layout = serde_json::from_slice(source).unwrap();
            let limits = PhysicalSessionLimits {
                max_actions: 32,
                elapsed_ms: 60_000,
                route_expansions: 1_000_000,
                max_total_route_expansions: 32_000_000,
                search_padding: 8,
            };
            let mut session = ProceduralSession::new(source, &catalogs, limits).unwrap();
            let source_sha256 = session.source_sha256().to_owned();
            let problem_bytes = serde_json::to_vec(session.problem()).unwrap().len();
            let initial_draft_bytes = serde_json::to_vec(session.current()).unwrap().len();
            let mut statuses = BTreeMap::<String, usize>::new();
            for attempt in 0..32 {
                let axis = if attempt % 2 == 0 { Axis::X } else { Axis::Y };
                let points = session
                    .current()
                    .poses
                    .iter()
                    .map(|pose| pose.position)
                    .chain(session.current().paths.iter().flatten().flatten().copied())
                    .chain(session.current().component_positions.iter().copied());
                let (mut low, mut high) = (i64::MAX, i64::MIN);
                for point in points {
                    let coordinate = match axis {
                        Axis::X => point.x,
                        Axis::Y => point.y,
                    };
                    low = low.min(coordinate);
                    high = high.max(coordinate);
                }
                let candidates: Vec<_> = (low..=high)
                    .map(|at| PhysicalEdit::InsertBand { axis, at, count: 1 })
                    .collect();
                let action = candidates
                    .into_iter()
                    .find(|action| {
                        let mut draft = session.current().clone();
                        apply_edit(session.problem(), &mut draft, action.clone()).is_ok()
                            && draft != *session.current()
                    })
                    .unwrap_or(PhysicalEdit::InsertBand {
                        axis,
                        at: high.saturating_add(1),
                        count: 1,
                    });
                let outcome = session.step(&source_sha256, session.revision(), action, &catalogs);
                *statuses.entry(outcome.status.into()).or_default() += 1;
            }
            let exported = session.export_best(&catalogs).unwrap();
            validation::validate(&exported, &catalogs).unwrap();
            let snapshot = session.checkpoint();
            reports.push(serde_json::json!({
                "case": name,
                "source_sha256": crate::input::hash(source),
                "blocks": layout.problem.blocks.len(),
                "connections": layout.problem.connections.len(),
                "bridges": layout.solution.bridges.len(),
                "actions": snapshot.attempts,
                "statuses": statuses,
                "route_expansions": snapshot.work.route_expansions,
                "preflight_expansions": snapshot.work.preflight_expansions,
                "route_calls": snapshot.work.route_calls,
                "validations": snapshot.work.validations,
                "compiled_bytes": problem_bytes,
                "initial_draft_bytes": initial_draft_bytes,
                "final_draft_bytes": serde_json::to_vec(session.current()).unwrap().len(),
                "snapshot_bytes": serde_json::to_vec(&snapshot).unwrap().len(),
                "elapsed_ns": started.elapsed().as_nanos(),
            }));
        }
        let report = serde_json::json!({
            "schema": "common-physical-editor-procedural-reference-v1",
            "profile": "release",
            "cases": reports,
        });
        let path = Path::new(&output);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        println!("{}", report);
    }

    #[test]
    fn common_router_preserves_distinct_zero_length_nodes() {
        let catalogs = catalogs();
        let admitted = ProceduralAdmission::compile(NO_BRIDGE, &catalogs).unwrap();
        let mut problem = admitted.problem.as_ref().clone();
        let mut draft = admitted.draft;
        let network = problem.edges[0].network;
        let mut nodes = problem.nodes[..2].to_vec();
        for (index, node) in nodes.iter_mut().enumerate() {
            node.id = index;
            node.network = 0;
            node.baseline_position = Point::default();
            node.terminal = None;
            node.components.clear();
        }
        problem.blocks.clear();
        problem.networks = vec![problem.networks[network].clone()];
        problem.nodes = nodes;
        problem.components.clear();
        let mut edge = problem.edges[0].clone();
        edge.id = 0;
        edge.network = 0;
        edge.source = 0;
        edge.target = 1;
        edge.first = None;
        edge.last = None;
        edge.zero_length = true;
        problem.edges = vec![edge];
        draft.poses.clear();
        draft.selected_ports = vec![None, None];
        draft.anchors = vec![Point::default(), Point::default()];
        draft.component_positions.clear();
        draft.paths = vec![None];
        let work = repair_pending(
            &problem,
            &mut draft,
            PhysicalEditLimits {
                route_expansions: 100,
                search_padding: 1,
            },
        )
        .unwrap();
        assert!(work.committed);
        assert_eq!(
            draft.paths[0].as_deref(),
            Some([Point::default()].as_slice())
        );
        validate_draft(&problem, &draft).unwrap();
    }
}
