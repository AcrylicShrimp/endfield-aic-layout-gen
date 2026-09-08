//! Ordered adjacency over an immutable segment slice, independent of grid extent and capacity.
use super::channels::Vertex;
use super::{ProfileOp, TransportNetworkSegment, WorldGridPosition, profile_span, xy};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct DirectedPaths<'a> {
    // Keep the topology borrowed for the entire index lifetime.
    _segments: &'a [TransportNetworkSegment],
    vertices: BTreeMap<Vertex, usize>,
    bridges: BTreeSet<(i64, i64)>,
    outgoing: Vec<Vec<(usize, usize)>>,
    previous: Vec<Option<(usize, usize)>>,
    queue: Vec<usize>,
}

impl<'a> DirectedPaths<'a> {
    pub(super) fn new(
        segments: &'a [TransportNetworkSegment],
        bridges: &BTreeSet<(i64, i64)>,
    ) -> Self {
        let span = profile_span(ProfileOp::PathIndexBuild);
        let mut vertices = BTreeMap::new();
        for segment in segments {
            for (position, other) in [(&segment.from, &segment.to), (&segment.to, &segment.from)] {
                let next = vertices.len();
                vertices
                    .entry(Vertex::segment(position, other, bridges))
                    .or_insert(next);
            }
        }
        let mut outgoing = vec![Vec::new(); vertices.len()];
        for (edge, segment) in segments.iter().enumerate() {
            let from = vertices[&Vertex::segment(&segment.from, &segment.to, bridges)];
            let to = vertices[&Vertex::segment(&segment.to, &segment.from, bridges)];
            // Preserve the original segment iteration order at every source vertex.
            outgoing[from].push((edge, to));
        }
        span.add_work("vertices-indexed", vertices.len() as u64);
        span.add_work("segments-indexed", segments.len() as u64);
        Self {
            _segments: segments,
            previous: vec![None; vertices.len()],
            queue: Vec::with_capacity(vertices.len()),
            vertices,
            bridges: bridges.clone(),
            outgoing,
        }
    }

    pub(super) fn path(
        &mut self,
        start: &WorldGridPosition,
        end: &WorldGridPosition,
    ) -> Option<Vec<usize>> {
        let span = profile_span(ProfileOp::SharedPath);
        let mut visited = 0;
        let mut examined = 0;
        let result = self.search(start, end, &mut visited, &mut examined);
        span.add_work("vertices-visited", visited);
        span.add_work("edges-examined", examined);
        result
    }

    fn search(
        &mut self,
        start: &WorldGridPosition,
        end: &WorldGridPosition,
        visited: &mut u64,
        examined: &mut u64,
    ) -> Option<Vec<usize>> {
        // A bridge axis is never an attachment endpoint; no implicit axis selection.
        if self.bridges.contains(&xy(start)) || self.bridges.contains(&xy(end)) {
            return None;
        }
        if start == end {
            *visited = 1;
            return Some(Vec::new());
        }
        let start = *self.vertices.get(&Vertex::ordinary(xy(start)))?;
        let end = *self.vertices.get(&Vertex::ordinary(xy(end)))?;
        self.previous.fill(None);
        self.queue.clear();
        self.previous[start] = Some((start, usize::MAX));
        self.queue.push(start);
        let mut head = 0;
        while head < self.queue.len() {
            let current = self.queue[head];
            head += 1;
            *visited += 1;
            if current == end {
                break;
            }
            for &(edge, to) in &self.outgoing[current] {
                *examined += 1;
                if self.previous[to].is_none() {
                    self.previous[to] = Some((current, edge));
                    self.queue.push(to);
                }
            }
        }
        let mut cursor = end;
        let mut path = Vec::new();
        while cursor != start {
            let (previous, edge) = self.previous[cursor]?;
            path.push(edge);
            cursor = previous;
        }
        path.reverse();
        Some(path)
    }
}

#[cfg(test)]
mod tests;
