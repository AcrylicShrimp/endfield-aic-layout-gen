//! Geometry checks never change the identity of a blueprint edge.

use aic_data::layouts::FactoryEditCatalogs;
use indexmap::{IndexMap, IndexSet};
use serde_json::json;

use super::super::path;
use super::*;

use super::declarative::frame::PhysicalFrame;

pub(super) fn conflicts(bp: &Blueprint, draft: &Draft) -> Vec<Value> {
    let mut conflicts = Vec::new();
    for (i, a) in draft.placements.iter().enumerate() {
        for b in &draft.placements[i + 1..] {
            let width = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
            let height = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
            if width > 0 && height > 0 {
                conflicts.push(json!({"kind": "facility_overlap", "facilities": [a.instance, b.instance], "area": width * height}));
            }
        }
        for c in &bp.components {
            if c.position.x >= a.x
                && c.position.x < a.x + a.width
                && c.position.y >= a.y
                && c.position.y < a.y + a.height
            {
                conflicts.push(
                    json!({"kind": "component_overlap", "facility": a.instance, "component": c.id}),
                );
            }
        }
    }
    conflicts
}

pub(super) fn blocked(
    bp: &Blueprint,
    draft: &Draft,
    catalogs: &FactoryEditCatalogs,
    edge: &Edge,
) -> Result<IndexSet<Point>> {
    let positions: Vec<_> = bp
        .nodes
        .iter()
        .map(|n| topology::position(n, draft, catalogs))
        .collect::<Result<_>>()?;
    let permits = |other: usize, p: Point| {
        [edge.source, edge.target]
            .iter()
            .any(|&id| positions[id] == p && topology::compatible(&bp.nodes[id], &bp.nodes[other]))
    };
    let mut blocked: IndexSet<_> = draft.placements.iter().flat_map(body_cells).collect();
    let transport = &bp.networks[edge.network]["transport"];
    for node in &bp.nodes {
        if &bp.networks[node.network]["transport"] == transport
            && !permits(node.id, positions[node.id])
        {
            blocked.insert(positions[node.id]);
        }
    }
    for component in &bp.components {
        if serde_json::to_value(component.transport).unwrap() != *transport {
            continue;
        }
        let p = Point::from(&component.position);
        if ![edge.source, edge.target]
            .iter()
            .any(|&id| positions[id] == p && bp.nodes[id].components.contains(&component.id))
        {
            blocked.insert(p);
        }
    }
    for other in &bp.edges {
        if other.id == edge.id || bp.networks[other.network]["transport"] != *transport {
            continue;
        }
        if let Some(cells) = &draft.paths[other.id] {
            for (i, &p) in cells.iter().enumerate() {
                if !((i == 0 && permits(other.source, p))
                    || (i + 1 == cells.len() && permits(other.target, p)))
                {
                    blocked.insert(p);
                }
            }
        }
    }
    Ok(blocked)
}

pub(super) fn check_path(
    bp: &Blueprint,
    draft: &Draft,
    catalogs: &FactoryEditCatalogs,
    edge: &Edge,
    cells: &[Point],
) -> Result<()> {
    if cells.is_empty() || cells.len() > 250_000 {
        return Err(fail(
            "route-geometry",
            "Route is empty or exceeds the path work guard",
            edge.id,
        ));
    }
    PhysicalFrame::from_points(cells.iter().copied())?;
    let start = topology::position(&bp.nodes[edge.source], draft, catalogs)?;
    let end = topology::position(&bp.nodes[edge.target], draft, catalogs)?;
    if cells[0] != start || cells[cells.len() - 1] != end {
        return Err(fail(
            "route-endpoint",
            "Path must connect the original edge's exact current endpoints",
            edge.id,
        ));
    }
    let (first, last) = topology::directions(edge, bp, draft, catalogs)?;
    if (cells.len() == 1 && (!edge.zero_length || first != last))
        || (cells.len() > 1
            && (first.is_some_and(|d| direction(cells[0], cells[1]) != Some(d))
                || last.is_some_and(|d| direction(cells[cells.len() - 2], end) != Some(d))))
    {
        return Err(fail(
            "route-direction",
            "Path changes an original port or component channel direction",
            edge.id,
        ));
    }
    if cells.iter().collect::<IndexSet<_>>().len() != cells.len()
        || cells.windows(2).any(|p| direction(p[0], p[1]).is_none())
    {
        return Err(fail(
            "route-geometry",
            "Path must be simple and use directed adjacent cells",
            edge.id,
        ));
    }
    let blocked = blocked(bp, draft, catalogs, edge)?;
    if let Some(p) = cells.iter().find(|p| blocked.contains(*p)) {
        return Err(fail(
            "route-intersection",
            format!(
                "Path intersects a body, unrelated anchor or unrelated route at ({}, {})",
                p.x, p.y
            ),
            edge.id,
        ));
    }
    Ok(())
}

pub(super) fn repair(
    bp: &Blueprint,
    draft: &Draft,
    catalogs: &FactoryEditCatalogs,
    edge: &Edge,
    work: usize,
    padding: i64,
) -> Result<path::PathResult> {
    let start = topology::position(&bp.nodes[edge.source], draft, catalogs)?;
    let end = topology::position(&bp.nodes[edge.target], draft, catalogs)?;
    let mut points: Vec<_> = bp
        .nodes
        .iter()
        .map(|n| topology::position(n, draft, catalogs))
        .collect::<Result<_>>()?;
    points.extend(draft.placements.iter().flat_map(|p| {
        [
            Point { x: p.x, y: p.y },
            Point {
                x: p.x + p.width - 1,
                y: p.y + p.height - 1,
            },
        ]
    }));
    points.extend(bp.components.iter().map(|c| Point::from(&c.position)));
    let bounds = PhysicalFrame::from_points(
        points
            .into_iter()
            .chain(draft.paths.iter().flatten().flatten().copied()),
    )?
    .padded(padding)?;
    if i128::from(bounds.width()) * i128::from(bounds.height()) > 250_000 {
        return Err(fail(
            "routing-area-guard",
            "Local A* region exceeds its 250000-cell materialization guard",
            edge.id,
        ));
    }
    if work == 0 {
        return Ok(path::PathResult::stopped("work-budget", 0));
    }
    let blocked = blocked(bp, draft, catalogs, edge)?;
    let (first, last) = topology::query_directions(
        edge.zero_length,
        start,
        end,
        topology::directions(edge, bp, draft, catalogs)?,
    );
    path::shortest_path(
        start,
        end,
        &blocked,
        bounds,
        path::PathOptions {
            first,
            last,
            work_budget: work,
            crossings: &IndexMap::new(),
            crossing_limit: 0,
            allow_zero_length: edge.zero_length,
        },
    )
}
