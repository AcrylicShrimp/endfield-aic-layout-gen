//! Geometry identity only: a bridge coordinate contains two independent vertices.
use super::{ConstructiveNode, LogisticsComponentKind, TransportKind, WorldGridPosition, xy};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub(super) fn between(a: &WorldGridPosition, b: &WorldGridPosition) -> Self {
        if a.y == b.y {
            Self::Horizontal
        } else {
            Self::Vertical
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Vertex {
    pub(super) position: (i64, i64),
    pub(super) axis: Option<Axis>,
}

impl Vertex {
    pub(super) fn ordinary(position: (i64, i64)) -> Self {
        Self {
            position,
            axis: None,
        }
    }

    // Callers validate unit adjacency separately. No route or flow state is shared.
    pub(super) fn segment(
        at: &WorldGridPosition,
        other: &WorldGridPosition,
        bridges: &BTreeSet<(i64, i64)>,
    ) -> Self {
        Self {
            position: xy(at),
            axis: bridges.contains(&xy(at)).then(|| Axis::between(at, other)),
        }
    }
}

pub(super) fn bridge_cells(
    node: &ConstructiveNode,
    transport: TransportKind,
) -> BTreeSet<(i64, i64)> {
    node.logistics_components
        .iter()
        .filter(|c| c.transport == transport && c.kind == LogisticsComponentKind::Bridge)
        .map(|c| xy(&c.position))
        .collect()
}
