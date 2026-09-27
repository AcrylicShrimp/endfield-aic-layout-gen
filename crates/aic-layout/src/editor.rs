//! Bounded, recipe-independent state consumed by physical layout editors.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    facilities::{FacilityDefinition, FacilityPortDirection, FacilityPortEdge},
    geometry::Point,
    logistics::{
        LogisticsComponentDefinition, LogisticsComponentKind, TransportDefinition, TransportKind,
    },
    rate::Rate,
    stable_id::is_stable_id,
};

pub const COMPILED_SCHEMA: &str = "aic-compiled-physical-problem-v1";
pub const DRAFT_SCHEMA: &str = "aic-physical-draft-v1";
pub const PHYSICAL_SPAN_GUARD: i64 = 1_000_000;
const MAX_COMPILED_ENTITIES: usize = 250_000;
const MAX_MATERIALIZED_CELLS: i128 = 2_000_000;

/// The immutable catalog subset used by one compiled problem.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct UsedDefinitionCatalog {
    pub facilities: Vec<FacilityDefinition>,
    pub components: Vec<LogisticsComponentDefinition>,
    pub transports: Vec<TransportDefinition>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompiledProblemV1 {
    pub schema: String,
    /// Identity of the validated external catalogs used at admission.
    pub catalog_sha256: String,
    pub used_catalog: UsedDefinitionCatalog,
    pub blocks: Vec<CompiledBlock>,
    pub networks: Vec<CompiledNetwork>,
    pub nodes: Vec<CompiledNode>,
    pub edges: Vec<CompiledEdge>,
    pub components: Vec<CompiledComponent>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompiledBlock {
    pub id: String,
    pub facility: String,
    /// The source rotation used to interpret recorded endpoint headings.
    pub baseline_rotation: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompiledNetwork {
    pub id: String,
    pub commodity: String,
    pub transport: TransportKind,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompiledNode {
    pub id: usize,
    pub source_id: String,
    pub network: usize,
    pub baseline_position: Point,
    pub terminal: Option<CompiledTerminal>,
    /// Dense component indices whose channels are attached at this node.
    pub components: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum CompiledTerminal {
    Facility {
        block: usize,
        source_terminal: String,
        original_port: String,
        direction: FacilityPortDirection,
        rate: Rate,
    },
    External {
        source_terminal: String,
        direction: FacilityPortDirection,
        rate: Rate,
        /// Fixed boundary normal when the source has one.
        side: Option<FacilityPortEdge>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompiledEdge {
    pub id: usize,
    pub source_id: String,
    pub network: usize,
    pub source: usize,
    pub target: usize,
    pub rate: Rate,
    pub first: Option<usize>,
    pub last: Option<usize>,
    pub zero_length: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CompiledComponent {
    pub id: String,
    pub definition: String,
    pub kind: LogisticsComponentKind,
    pub transport: TransportKind,
    pub rotation: i64,
    /// One entry per independently owned channel, even when positions coincide.
    pub channels: Vec<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PhysicalDraftV1 {
    pub schema: String,
    pub poses: Vec<PhysicalPose>,
    pub selected_ports: Vec<Option<String>>,
    pub anchors: Vec<Point>,
    pub component_positions: Vec<Point>,
    pub paths: Vec<Option<Vec<Point>>>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PhysicalPose {
    pub position: Point,
    pub rotation: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledValidationError {
    pub code: &'static str,
    pub entity: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct CompiledIdentity {
    pub physical: crate::physical_identity::PhysicalIdentity,
    /// Canonical mutable geometry. It contains no source-local names.
    pub draft_sha256: String,
}

impl std::fmt::Display for CompiledValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} [{}]: {}",
            self.code, self.entity, self.message
        )
    }
}

impl std::error::Error for CompiledValidationError {}

fn invalid(
    code: &'static str,
    entity: impl Into<String>,
    message: impl Into<String>,
) -> CompiledValidationError {
    CompiledValidationError {
        code,
        entity: entity.into(),
        message: message.into(),
    }
}

fn validate_used_definitions(problem: &CompiledProblemV1) -> Result<(), CompiledValidationError> {
    let mut capabilities = BTreeSet::new();
    let mut total_ports = 0usize;
    for definition in &problem.used_catalog.facilities {
        let area = i128::from(definition.footprint.width)
            .checked_mul(i128::from(definition.footprint.height));
        if !is_stable_id(&definition.id)
            || definition.footprint.width <= 0
            || definition.footprint.height <= 0
            || area.is_none_or(|area| !(1..=MAX_MATERIALIZED_CELLS).contains(&area))
            || definition.allowed_rotations.is_empty()
            || definition
                .allowed_rotations
                .iter()
                .any(|rotation| !matches!(rotation, 0 | 90 | 180 | 270))
            || definition
                .allowed_rotations
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                != definition.allowed_rotations.len()
        {
            return Err(invalid(
                "invalid-facility-definition",
                &definition.id,
                "facility ID, footprint, or rotations are invalid",
            ));
        }
        total_ports = total_ports
            .checked_add(definition.ports.len())
            .ok_or_else(|| invalid("compiled-size", "used_catalog", "port count overflow"))?;
        if total_ports > MAX_COMPILED_ENTITIES {
            return Err(invalid(
                "compiled-size",
                "used_catalog",
                "embedded facility ports exceed the compiled problem guard",
            ));
        }
        let mut ports = BTreeSet::new();
        for port in &definition.ports {
            let inside = port.position.x >= 0
                && port.position.y >= 0
                && port.position.x < definition.footprint.width
                && port.position.y < definition.footprint.height;
            let on_edge = match port.edge {
                FacilityPortEdge::North => port.position.y == 0,
                FacilityPortEdge::East => port.position.x == definition.footprint.width - 1,
                FacilityPortEdge::South => port.position.y == definition.footprint.height - 1,
                FacilityPortEdge::West => port.position.x == 0,
            };
            if !is_stable_id(&port.id) || !ports.insert(port.id.as_str()) || !inside || !on_edge {
                return Err(invalid(
                    "invalid-facility-definition",
                    &definition.id,
                    "facility port ID, position, or edge is invalid",
                ));
            }
        }
    }
    for definition in &problem.used_catalog.components {
        let rotations = definition
            .allowed_rotations
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let inputs = definition
            .input_directions
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let outputs = definition
            .output_directions
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let topology = match definition.kind {
            LogisticsComponentKind::Splitter => inputs.len() == 1 && outputs.len() == 3,
            LogisticsComponentKind::Converger => inputs.len() == 3 && outputs.len() == 1,
            LogisticsComponentKind::Bridge => inputs.len() == 4 && outputs.len() == 4,
        };
        if !is_stable_id(&definition.id)
            || definition.footprint.width != 1
            || definition.footprint.height != 1
            || rotations.is_empty()
            || rotations.len() != definition.allowed_rotations.len()
            || rotations
                .iter()
                .any(|rotation| !matches!(rotation, 0 | 90 | 180 | 270))
            || inputs.len() != definition.input_directions.len()
            || outputs.len() != definition.output_directions.len()
            || !topology
            || definition.capacity.quantity <= 0
            || definition.capacity.duration_ms <= 0
            || !capabilities.insert((definition.transport, definition.kind))
        {
            return Err(invalid(
                "invalid-component-definition",
                &definition.id,
                "component ID, capability, geometry, directions, rotations, or capacity are invalid",
            ));
        }
    }
    for definition in &problem.used_catalog.transports {
        if definition.capacity.quantity <= 0 || definition.capacity.duration_ms <= 0 {
            return Err(invalid(
                "invalid-transport-definition",
                format!("{:?}", definition.kind).to_lowercase(),
                "transport capacity must be positive",
            ));
        }
    }
    Ok(())
}

fn occupied_extents_validated(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
) -> Result<(Point, Point), CompiledValidationError> {
    let facilities: BTreeMap<_, _> = problem
        .used_catalog
        .facilities
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect();
    let components: BTreeMap<_, _> = problem
        .used_catalog
        .components
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect();
    let mut low = [i128::MAX; 2];
    let mut high = [i128::MIN; 2];
    let mut include = |x: i128, y: i128| {
        low[0] = low[0].min(x);
        low[1] = low[1].min(y);
        high[0] = high[0].max(x);
        high[1] = high[1].max(y);
    };
    for (owner, (block, pose)) in problem.blocks.iter().zip(&draft.poses).enumerate() {
        let definition = facilities[block.facility.as_str()];
        let (width, height) = if pose.rotation % 180 == 0 {
            (definition.footprint.width, definition.footprint.height)
        } else {
            (definition.footprint.height, definition.footprint.width)
        };
        include(i128::from(pose.position.x), i128::from(pose.position.y));
        include(
            i128::from(pose.position.x) + i128::from(width) - 1,
            i128::from(pose.position.y) + i128::from(height) - 1,
        );
        for node in &problem.nodes {
            let Some(CompiledTerminal::Facility { block, .. }) = &node.terminal else {
                continue;
            };
            if *block != owner {
                continue;
            }
            let selected = draft.selected_ports[node.id]
                .as_deref()
                .expect("validated port");
            let port = definition
                .ports
                .iter()
                .find(|port| port.id == selected)
                .expect("validated port definition");
            let mut x = i128::from(port.position.x);
            let mut y = i128::from(port.position.y);
            let mut local_width = i128::from(definition.footprint.width);
            let mut local_height = i128::from(definition.footprint.height);
            let mut edge = port.edge;
            for _ in 0..pose.rotation / 90 {
                (x, y) = (local_height - 1 - y, x);
                std::mem::swap(&mut local_width, &mut local_height);
                edge = edge.rotated_clockwise(90);
            }
            x += i128::from(pose.position.x);
            y += i128::from(pose.position.y);
            include(x, y);
            let (dx, dy) = match edge {
                FacilityPortEdge::North => (0, -1),
                FacilityPortEdge::East => (1, 0),
                FacilityPortEdge::South => (0, 1),
                FacilityPortEdge::West => (-1, 0),
            };
            include(x + dx, y + dy);
        }
    }
    for (index, component) in problem.components.iter().enumerate() {
        let definition = components[component.definition.as_str()];
        let (width, height) = if component.rotation % 180 == 0 {
            (definition.footprint.width, definition.footprint.height)
        } else {
            (definition.footprint.height, definition.footprint.width)
        };
        let point = draft.component_positions[index];
        include(i128::from(point.x), i128::from(point.y));
        include(
            i128::from(point.x) + i128::from(width) - 1,
            i128::from(point.y) + i128::from(height) - 1,
        );
    }
    for node in &problem.nodes {
        if !matches!(node.terminal, Some(CompiledTerminal::Facility { .. })) {
            let point = draft.anchors[node.id];
            include(i128::from(point.x), i128::from(point.y));
        }
    }
    for point in draft.paths.iter().flatten().flatten() {
        include(i128::from(point.x), i128::from(point.y));
    }
    if low[0] == i128::MAX
        || high[0] - low[0] > i128::from(PHYSICAL_SPAN_GUARD)
        || high[1] - low[1] > i128::from(PHYSICAL_SPAN_GUARD)
        || low
            .iter()
            .chain(&high)
            .any(|&value| i64::try_from(value).is_err())
    {
        return Err(invalid(
            "coordinate-guard",
            "draft",
            "occupied coordinates overflow or span exceeds the physical editor guard",
        ));
    }
    Ok((
        Point {
            x: low[0] as i64,
            y: low[1] as i64,
        },
        Point {
            x: high[0] as i64,
            y: high[1] as i64,
        },
    ))
}

/// Returns occupied bounds after checking only the inputs used by the extent
/// calculation. Session-owned hot paths use this after the complete compiled
/// boundary was admitted once; malformed standalone values still return an
/// error instead of reaching the validated implementation's indexed lookups.
pub fn occupied_extents_checked(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
) -> Result<(Point, Point), CompiledValidationError> {
    if problem.schema != COMPILED_SCHEMA
        || draft.schema != DRAFT_SCHEMA
        || draft.poses.len() != problem.blocks.len()
        || draft.selected_ports.len() != problem.nodes.len()
        || draft.anchors.len() != problem.nodes.len()
        || draft.component_positions.len() != problem.components.len()
        || draft.paths.len() != problem.edges.len()
    {
        return Err(invalid(
            "extent-shape",
            "draft",
            "compiled problem and draft shapes differ",
        ));
    }
    let facilities: BTreeMap<_, _> = problem
        .used_catalog
        .facilities
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect();
    let components: BTreeMap<_, _> = problem
        .used_catalog
        .components
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect();
    for (block, pose) in problem.blocks.iter().zip(&draft.poses) {
        let Some(definition) = facilities.get(block.facility.as_str()) else {
            return Err(invalid(
                "extent-facility",
                &block.id,
                "block facility definition is absent",
            ));
        };
        if !definition.allowed_rotations.contains(&pose.rotation) {
            return Err(invalid(
                "extent-rotation",
                &block.id,
                "block rotation is outside the facility domain",
            ));
        }
    }
    for node in &problem.nodes {
        if node.id >= problem.nodes.len() {
            return Err(invalid(
                "extent-node",
                node.id.to_string(),
                "node ID is outside the dense node bank",
            ));
        }
        if let Some(CompiledTerminal::Facility { block, .. }) = &node.terminal {
            let Some(owner) = problem.blocks.get(*block) else {
                return Err(invalid(
                    "extent-owner",
                    node.id.to_string(),
                    "terminal block owner is absent",
                ));
            };
            let definition = facilities.get(owner.facility.as_str()).ok_or_else(|| {
                invalid(
                    "extent-facility",
                    &owner.id,
                    "terminal facility definition is absent",
                )
            })?;
            let selected = draft.selected_ports[node.id].as_deref().ok_or_else(|| {
                invalid(
                    "extent-port",
                    node.id.to_string(),
                    "facility terminal has no selected port",
                )
            })?;
            if !definition.ports.iter().any(|port| port.id == selected) {
                return Err(invalid(
                    "extent-port",
                    node.id.to_string(),
                    "selected port is absent from its facility definition",
                ));
            }
        }
    }
    for component in &problem.components {
        let Some(definition) = components.get(component.definition.as_str()) else {
            return Err(invalid(
                "extent-component",
                &component.id,
                "component definition is absent",
            ));
        };
        if !definition.allowed_rotations.contains(&component.rotation) {
            return Err(invalid(
                "extent-rotation",
                &component.id,
                "component rotation is outside the definition domain",
            ));
        }
    }
    occupied_extents_validated(problem, draft)
}

/// Returns the complete occupied bounds used by every physical action domain.
pub fn occupied_extents(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
) -> Result<(Point, Point), CompiledValidationError> {
    validate_compiled(problem, draft)?;
    occupied_extents_validated(problem, draft)
}

/// Checks the complete serialized editor boundary using only its embedded catalog.
pub fn validate_compiled(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
) -> Result<(), CompiledValidationError> {
    if problem.schema != COMPILED_SCHEMA || draft.schema != DRAFT_SCHEMA {
        return Err(invalid(
            "schema",
            "root",
            "unsupported compiled problem or draft schema",
        ));
    }
    let entity_counts = [
        problem.used_catalog.facilities.len(),
        problem.used_catalog.components.len(),
        problem.used_catalog.transports.len(),
        problem.blocks.len(),
        problem.networks.len(),
        problem.nodes.len(),
        problem.edges.len(),
        problem.components.len(),
    ];
    if entity_counts
        .iter()
        .any(|&count| count > MAX_COMPILED_ENTITIES)
        || entity_counts
            .iter()
            .try_fold(0usize, |total, &count| total.checked_add(count))
            .is_none_or(|total| total > MAX_COMPILED_ENTITIES)
    {
        return Err(invalid(
            "compiled-size",
            "root",
            "compiled entity count exceeds the materialization guard",
        ));
    }
    validate_used_definitions(problem)?;
    let facilities: BTreeMap<_, _> = problem
        .used_catalog
        .facilities
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect();
    let component_definitions: BTreeMap<_, _> = problem
        .used_catalog
        .components
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect();
    let transports: BTreeMap<_, _> = problem
        .used_catalog
        .transports
        .iter()
        .map(|definition| (definition.kind, definition))
        .collect();
    if facilities.len() != problem.used_catalog.facilities.len()
        || component_definitions.len() != problem.used_catalog.components.len()
        || transports.len() != problem.used_catalog.transports.len()
    {
        return Err(invalid(
            "duplicate-definition",
            "used_catalog",
            "used definitions must be uniquely keyed",
        ));
    }
    if draft.poses.len() != problem.blocks.len()
        || draft.selected_ports.len() != problem.nodes.len()
        || draft.anchors.len() != problem.nodes.len()
        || draft.component_positions.len() != problem.components.len()
        || draft.paths.len() != problem.edges.len()
    {
        return Err(invalid(
            "draft-shape",
            "draft",
            "draft vectors must align with their dense problem entities",
        ));
    }

    let mut block_ids = BTreeSet::new();
    let mut body_cells = 0i128;
    for (index, (block, pose)) in problem.blocks.iter().zip(&draft.poses).enumerate() {
        if !block_ids.insert(block.id.as_str()) {
            return Err(invalid(
                "duplicate-block",
                &block.id,
                "block ID is repeated",
            ));
        }
        let definition = facilities.get(block.facility.as_str()).ok_or_else(|| {
            invalid(
                "missing-facility-definition",
                &block.id,
                "block references a definition outside the embedded catalog",
            )
        })?;
        body_cells +=
            i128::from(definition.footprint.width) * i128::from(definition.footprint.height);
        if body_cells > MAX_MATERIALIZED_CELLS {
            return Err(invalid(
                "body-size",
                "blocks",
                "total facility body cells exceed the materialization guard",
            ));
        }
        if !definition
            .allowed_rotations
            .contains(&block.baseline_rotation)
            || !definition.allowed_rotations.contains(&pose.rotation)
        {
            return Err(invalid(
                "block-rotation",
                index.to_string(),
                "baseline or current rotation is not allowed",
            ));
        }
    }

    let mut network_ids = BTreeSet::new();
    for network in &problem.networks {
        if !network_ids.insert(network.id.as_str()) || !transports.contains_key(&network.transport)
        {
            return Err(invalid(
                "network",
                &network.id,
                "network ID is repeated or transport definition is missing",
            ));
        }
    }

    let node_component_references = problem.nodes.iter().try_fold(0usize, |total, node| {
        total.checked_add(node.components.len())
    });
    let component_channel_references = problem
        .components
        .iter()
        .try_fold(0usize, |total, component| {
            total.checked_add(component.channels.len())
        });
    if node_component_references.is_none_or(|count| count > MAX_MATERIALIZED_CELLS as usize)
        || component_channel_references.is_none_or(|count| count > MAX_MATERIALIZED_CELLS as usize)
    {
        return Err(invalid(
            "compiled-size",
            "relationships",
            "component relationship count exceeds the materialization guard",
        ));
    }
    let mut terminal_ports = BTreeSet::new();
    let mut node_source_ids = BTreeSet::new();
    for (index, node) in problem.nodes.iter().enumerate() {
        if node.id != index || node.network >= problem.networks.len() {
            return Err(invalid(
                "node-index",
                index.to_string(),
                "node ID or network index is outside the dense problem",
            ));
        }
        if !node_source_ids.insert(node.source_id.as_str()) {
            return Err(invalid(
                "duplicate-node-source",
                &node.source_id,
                "node source identity is repeated",
            ));
        }
        if node
            .components
            .iter()
            .any(|&id| id >= problem.components.len())
        {
            return Err(invalid(
                "node-component",
                index.to_string(),
                "node references an unknown component",
            ));
        }
        match &node.terminal {
            Some(CompiledTerminal::Facility {
                block,
                original_port,
                direction,
                rate,
                ..
            }) => {
                let owner = problem.blocks.get(*block).ok_or_else(|| {
                    invalid(
                        "terminal-block",
                        index.to_string(),
                        "terminal owner is unknown",
                    )
                })?;
                let definition = facilities[owner.facility.as_str()];
                let capacity = &transports[&problem.networks[node.network].transport].capacity;
                if !crate::validation::within_capacity(
                    *rate,
                    capacity.quantity,
                    capacity.duration_ms,
                ) {
                    return Err(invalid(
                        "terminal-rate",
                        index.to_string(),
                        "facility terminal rate exceeds transport capacity",
                    ));
                }
                let selected = draft.selected_ports[index].as_deref().ok_or_else(|| {
                    invalid(
                        "missing-selected-port",
                        index.to_string(),
                        "facility terminal has no port",
                    )
                })?;
                for port_id in [original_port.as_str(), selected] {
                    let port = definition.ports.iter().find(|port| port.id == port_id);
                    if port.is_none_or(|port| {
                        port.direction != *direction
                            || port.transport != problem.networks[node.network].transport
                    }) {
                        return Err(invalid(
                            "incompatible-port",
                            index.to_string(),
                            "terminal port is absent or has incompatible direction/transport",
                        ));
                    }
                }
                if !terminal_ports.insert((*block, selected)) {
                    return Err(invalid(
                        "reused-port",
                        index.to_string(),
                        "facility terminals must select distinct ports",
                    ));
                }
            }
            Some(CompiledTerminal::External { rate, .. }) => {
                let capacity = &transports[&problem.networks[node.network].transport].capacity;
                if !crate::validation::within_capacity(
                    *rate,
                    capacity.quantity,
                    capacity.duration_ms,
                ) {
                    return Err(invalid(
                        "terminal-rate",
                        index.to_string(),
                        "external terminal rate exceeds transport capacity",
                    ));
                }
                if draft.selected_ports[index].is_some() {
                    return Err(invalid(
                        "unexpected-selected-port",
                        index.to_string(),
                        "only facility terminals select catalog ports",
                    ));
                }
            }
            None => {
                if draft.selected_ports[index].is_some() {
                    return Err(invalid(
                        "unexpected-selected-port",
                        index.to_string(),
                        "only facility terminals select catalog ports",
                    ));
                }
            }
        }
    }

    let mut component_ids = BTreeSet::new();
    for (index, component) in problem.components.iter().enumerate() {
        if !component_ids.insert(component.id.as_str()) {
            return Err(invalid(
                "duplicate-component",
                &component.id,
                "component ID is repeated",
            ));
        }
        let definition = component_definitions
            .get(component.definition.as_str())
            .ok_or_else(|| {
                invalid(
                    "missing-component-definition",
                    &component.id,
                    "component definition is outside the embedded catalog",
                )
            })?;
        if definition.kind != component.kind
            || definition.transport != component.transport
            || !definition.allowed_rotations.contains(&component.rotation)
            || component.channels.iter().any(|&id| {
                id >= problem.nodes.len() || !problem.nodes[id].components.contains(&index)
            })
        {
            return Err(invalid(
                "component",
                &component.id,
                "component definition, rotation, or channel ownership is inconsistent",
            ));
        }
        if component.channels.iter().any(|&node| {
            problem.networks[problem.nodes[node].network].transport != component.transport
                || draft.anchors[node] != draft.component_positions[index]
        }) {
            return Err(invalid(
                "component-channel",
                &component.id,
                "component channels must use its transport and remain attached",
            ));
        }
        if problem
            .nodes
            .iter()
            .any(|node| node.components.contains(&index) && !component.channels.contains(&node.id))
        {
            return Err(invalid(
                "component-channel",
                &component.id,
                "node and component channel ownership must be symmetric",
            ));
        }
    }

    let mut edge_source_ids = BTreeSet::new();
    let mut total_path_cells = 0usize;
    for (index, edge) in problem.edges.iter().enumerate() {
        if edge.id != index
            || edge.network >= problem.networks.len()
            || edge.source >= problem.nodes.len()
            || edge.target >= problem.nodes.len()
            || edge.source == edge.target
            || problem.nodes[edge.source].network != edge.network
            || problem.nodes[edge.target].network != edge.network
            || edge.first.is_some_and(|direction| direction >= 4)
            || edge.last.is_some_and(|direction| direction >= 4)
        {
            return Err(invalid(
                "edge-index",
                index.to_string(),
                "edge incidence or endpoint direction is inconsistent",
            ));
        }
        if !edge_source_ids.insert(edge.source_id.as_str()) {
            return Err(invalid(
                "duplicate-edge-source",
                &edge.source_id,
                "edge source identity is repeated",
            ));
        }
        let transport_capacity = &transports[&problem.networks[edge.network].transport].capacity;
        if !crate::validation::within_capacity(
            edge.rate,
            transport_capacity.quantity,
            transport_capacity.duration_ms,
        ) {
            return Err(invalid(
                "edge-rate",
                index.to_string(),
                "edge rate exceeds transport capacity",
            ));
        }
        for component in [edge.source, edge.target]
            .into_iter()
            .flat_map(|node| &problem.nodes[node].components)
        {
            let definition =
                component_definitions[&problem.components[*component].definition.as_str()];
            if !crate::validation::within_capacity(
                edge.rate,
                definition.capacity.quantity,
                definition.capacity.duration_ms,
            ) {
                return Err(invalid(
                    "edge-component-rate",
                    index.to_string(),
                    "edge rate exceeds an endpoint component capacity",
                ));
            }
        }
        if let Some(path) = &draft.paths[index]
            && path.is_empty()
        {
            return Err(invalid("path-size", index.to_string(), "path is empty"));
        }
        total_path_cells = total_path_cells
            .checked_add(draft.paths[index].as_ref().map_or(0, Vec::len))
            .ok_or_else(|| invalid("path-size", "paths", "path cell count overflow"))?;
        if total_path_cells > 2_000_000 {
            return Err(invalid(
                "path-size",
                "paths",
                "total path cells exceed the materialization guard",
            ));
        }
    }
    occupied_extents_validated(problem, draft)?;
    Ok(())
}

/// Compute name-independent problem identity and canonical mutable-state identity.
pub fn compiled_identity(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
    limits: &crate::physical_identity::Limits,
) -> Result<CompiledIdentity, Box<dyn std::error::Error + Send + Sync>> {
    compiled_identity_until(problem, draft, limits, || false).map(|(identity, _)| identity)
}

/// Compute the compiled identity and charged canonicalization visits with cancellation.
pub fn compiled_identity_until(
    problem: &CompiledProblemV1,
    draft: &PhysicalDraftV1,
    limits: &crate::physical_identity::Limits,
    mut cancelled: impl FnMut() -> bool,
) -> Result<(CompiledIdentity, u64), Box<dyn std::error::Error + Send + Sync>> {
    validate_compiled(problem, draft)?;
    use crate::physical_identity::{Graph, Kind, Relation};
    let mut graph = Graph::default();
    let transport = |kind: TransportKind| i64::from(kind == TransportKind::Pipe);
    let direction =
        |value: FacilityPortDirection| i64::from(value == FacilityPortDirection::Output);
    let mut blocks = Vec::new();
    let mut ports = Vec::new();
    for block in &problem.blocks {
        let definition = facilities(problem)[block.facility.as_str()];
        let rotations = definition
            .allowed_rotations
            .iter()
            .fold(0i64, |mask, rotation| mask | 1 << (rotation / 90));
        let owner = graph.vertex(
            Kind::Facility,
            vec![
                definition.footprint.width,
                definition.footprint.height,
                rotations,
                block.baseline_rotation,
            ],
            vec![],
        );
        blocks.push(owner);
        let local: BTreeMap<_, _> = definition
            .ports
            .iter()
            .map(|port| {
                let vertex = graph.vertex(
                    Kind::Port,
                    vec![
                        direction(port.direction),
                        transport(port.transport),
                        port.position.x,
                        port.position.y,
                        crate::projection::edge_index(port.edge) as i64,
                    ],
                    vec![],
                );
                graph.relation(owner, Relation::OwnsPort, vertex);
                (port.id.as_str(), vertex)
            })
            .collect();
        ports.push(local);
    }
    let mut commodities = BTreeMap::new();
    let mut networks = Vec::new();
    for network in &problem.networks {
        let vertex = graph.vertex(Kind::Network, vec![transport(network.transport)], vec![]);
        let commodity = *commodities
            .entry(network.commodity.as_str())
            .or_insert_with(|| graph.vertex(Kind::Commodity, vec![], vec![]));
        graph.relation(vertex, Relation::Commodity, commodity);
        networks.push(vertex);
    }
    let mut nodes = Vec::new();
    for node in &problem.nodes {
        let (kind, structural, exact) = match &node.terminal {
            Some(CompiledTerminal::Facility {
                direction: value,
                rate,
                ..
            }) => (
                Kind::Terminal,
                vec![
                    direction(*value),
                    node.baseline_position.x,
                    node.baseline_position.y,
                ],
                vec![rate.numerator, rate.denominator],
            ),
            Some(CompiledTerminal::External {
                direction: value,
                rate,
                side,
                ..
            }) => (
                Kind::External,
                vec![
                    direction(*value),
                    node.baseline_position.x,
                    node.baseline_position.y,
                    side.map_or(4, crate::projection::edge_index) as i64,
                ],
                vec![rate.numerator, rate.denominator],
            ),
            None => (
                Kind::Cell,
                vec![node.baseline_position.x, node.baseline_position.y],
                vec![],
            ),
        };
        let vertex = graph.vertex(kind, structural, exact);
        graph.relation(networks[node.network], Relation::NetworkTerminal, vertex);
        if let Some(CompiledTerminal::Facility {
            block,
            original_port,
            ..
        }) = &node.terminal
        {
            graph.relation(vertex, Relation::TerminalEndpoint, blocks[*block]);
            graph.relation(
                vertex,
                Relation::TerminalCell,
                ports[*block][original_port.as_str()],
            );
        }
        nodes.push(vertex);
    }
    let mut components = Vec::new();
    for component in &problem.components {
        let definition = component_definitions(problem)[component.definition.as_str()];
        let vertex = graph.vertex(
            Kind::Component,
            vec![
                transport(component.transport),
                match component.kind {
                    LogisticsComponentKind::Splitter => 0,
                    LogisticsComponentKind::Converger => 1,
                    LogisticsComponentKind::Bridge => 2,
                },
                definition.footprint.width,
                definition.footprint.height,
                component.rotation,
            ],
            vec![
                definition.capacity.quantity,
                definition.capacity.duration_ms,
            ],
        );
        for &node in &component.channels {
            graph.relation(vertex, Relation::NetworkComponent, nodes[node]);
        }
        components.push(vertex);
    }
    for edge in &problem.edges {
        let segment = graph.vertex(
            Kind::Segment,
            vec![
                edge.first.unwrap_or(4) as i64,
                edge.last.unwrap_or(4) as i64,
                i64::from(edge.zero_length),
            ],
            vec![edge.rate.numerator, edge.rate.denominator],
        );
        graph.relation(networks[edge.network], Relation::NetworkSegment, segment);
        graph.relation(segment, Relation::SegmentFrom, nodes[edge.source]);
        graph.relation(segment, Relation::SegmentTo, nodes[edge.target]);
    }
    let (physical, identity_work) = crate::physical_identity::identities_until(
        &graph,
        limits,
        "compiled-problem",
        &mut cancelled,
    )?;
    let selected_ports: Vec<_> = problem
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| match &node.terminal {
            Some(CompiledTerminal::Facility { block, .. }) => draft.selected_ports[index]
                .as_deref()
                .and_then(|selected| {
                    facilities(problem)[problem.blocks[*block].facility.as_str()]
                        .ports
                        .iter()
                        .find(|port| port.id == selected)
                })
                .map(|port| {
                    (
                        direction(port.direction),
                        transport(port.transport),
                        port.position.x,
                        port.position.y,
                        crate::projection::edge_index(port.edge),
                    )
                }),
            _ => None,
        })
        .collect();
    let state = (
        &draft.poses,
        selected_ports,
        &draft.anchors,
        &draft.component_positions,
        &draft.paths,
    );
    let draft_sha256 = Sha256::digest(serde_json::to_vec(&state)?)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((
        CompiledIdentity {
            physical,
            draft_sha256,
        },
        identity_work,
    ))
}

fn facilities(problem: &CompiledProblemV1) -> BTreeMap<&str, &FacilityDefinition> {
    problem
        .used_catalog
        .facilities
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect()
}

fn component_definitions(
    problem: &CompiledProblemV1,
) -> BTreeMap<&str, &LogisticsComponentDefinition> {
    problem
        .used_catalog
        .components
        .iter()
        .map(|definition| (definition.id.as_str(), definition))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        facilities::{
            FacilityFootprint, FacilityPortDefinition, FacilityPortEdge, FacilityPortPosition,
        },
        logistics::TransportCapacity,
    };

    fn fixture() -> (CompiledProblemV1, PhysicalDraftV1) {
        let facility = FacilityDefinition {
            id: "machine".into(),
            footprint: FacilityFootprint {
                width: 1,
                height: 1,
            },
            allowed_rotations: vec![0],
            ports: vec![FacilityPortDefinition {
                id: "out".into(),
                direction: FacilityPortDirection::Output,
                transport: TransportKind::Belt,
                position: FacilityPortPosition { x: 0, y: 0 },
                edge: FacilityPortEdge::East,
            }],
        };
        let problem = CompiledProblemV1 {
            schema: COMPILED_SCHEMA.into(),
            catalog_sha256: "catalog".into(),
            used_catalog: UsedDefinitionCatalog {
                facilities: vec![facility],
                components: vec![],
                transports: vec![TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 1,
                    },
                }],
            },
            blocks: vec![CompiledBlock {
                id: "a".into(),
                facility: "machine".into(),
                baseline_rotation: 0,
            }],
            networks: vec![CompiledNetwork {
                id: "n".into(),
                commodity: "ore".into(),
                transport: TransportKind::Belt,
            }],
            nodes: vec![CompiledNode {
                id: 0,
                source_id: "terminal".into(),
                network: 0,
                baseline_position: Point { x: 1, y: 0 },
                terminal: Some(CompiledTerminal::Facility {
                    block: 0,
                    source_terminal: "terminal".into(),
                    original_port: "out".into(),
                    direction: FacilityPortDirection::Output,
                    rate: Rate {
                        numerator: 1,
                        denominator: 1,
                    },
                }),
                components: vec![],
            }],
            edges: vec![],
            components: vec![],
        };
        let draft = PhysicalDraftV1 {
            schema: DRAFT_SCHEMA.into(),
            poses: vec![PhysicalPose {
                position: Point { x: 0, y: 0 },
                rotation: 0,
            }],
            selected_ports: vec![Some("out".into())],
            anchors: vec![Point { x: 1, y: 0 }],
            component_positions: vec![],
            paths: vec![],
        };
        (problem, draft)
    }

    #[test]
    fn validates_without_external_catalogs() {
        let (problem, draft) = fixture();
        validate_compiled(&problem, &draft).unwrap();
    }

    #[test]
    fn admits_a_structural_path_longer_than_the_old_per_path_guard() {
        let (mut problem, mut draft) = fixture();
        let far = Point { x: 250_001, y: 0 };
        problem.nodes.push(CompiledNode {
            id: 1,
            source_id: "far-node".into(),
            network: 0,
            baseline_position: far,
            terminal: None,
            components: vec![],
        });
        problem.edges.push(CompiledEdge {
            id: 0,
            source_id: "long-edge".into(),
            network: 0,
            source: 0,
            target: 1,
            rate: Rate {
                numerator: 1,
                denominator: 1,
            },
            first: None,
            last: None,
            zero_length: false,
        });
        draft.selected_ports.push(None);
        draft.anchors.push(far);
        draft
            .paths
            .push(Some((1..=250_001).map(|x| Point { x, y: 0 }).collect()));
        validate_compiled(&problem, &draft).unwrap();

        draft.paths[0] = Some(vec![]);
        assert_eq!(
            validate_compiled(&problem, &draft).unwrap_err().code,
            "path-size"
        );
    }

    #[test]
    fn rejects_misaligned_dense_state() {
        let (problem, mut draft) = fixture();
        draft.anchors.clear();
        assert_eq!(
            validate_compiled(&problem, &draft).unwrap_err().code,
            "draft-shape"
        );
    }

    #[test]
    fn rejects_invalid_embedded_definitions() {
        let (mut problem, draft) = fixture();
        problem.used_catalog.facilities[0].footprint.width = 0;
        assert_eq!(
            validate_compiled(&problem, &draft).unwrap_err().code,
            "invalid-facility-definition"
        );

        let (mut problem, draft) = fixture();
        problem.used_catalog.transports[0].capacity.duration_ms = 0;
        assert_eq!(
            validate_compiled(&problem, &draft).unwrap_err().code,
            "invalid-transport-definition"
        );

        let (mut problem, draft) = fixture();
        problem.used_catalog.facilities[0].footprint.width = 2_000;
        problem.used_catalog.facilities[0].footprint.height = 2_000;
        problem.used_catalog.facilities[0].ports[0].position.x = 1_999;
        assert_eq!(
            validate_compiled(&problem, &draft).unwrap_err().code,
            "invalid-facility-definition"
        );
    }

    #[test]
    fn rejects_overflowing_or_oversized_occupied_coordinates() {
        let (problem, mut draft) = fixture();
        draft.poses[0].position.x = i64::MAX;
        assert_eq!(
            validate_compiled(&problem, &draft).unwrap_err().code,
            "coordinate-guard"
        );
    }

    #[test]
    fn extent_only_boundary_rejects_malformed_indexed_inputs() {
        let (problem, mut draft) = fixture();
        draft.selected_ports.clear();
        assert_eq!(
            occupied_extents_checked(&problem, &draft).unwrap_err().code,
            "extent-shape"
        );

        let (mut problem, draft) = fixture();
        problem.nodes[0].id = usize::MAX;
        assert_eq!(
            occupied_extents_checked(&problem, &draft).unwrap_err().code,
            "extent-node"
        );

        let (problem, mut draft) = fixture();
        draft.selected_ports[0] = Some("absent".into());
        assert_eq!(
            occupied_extents_checked(&problem, &draft).unwrap_err().code,
            "extent-port"
        );
    }
}
