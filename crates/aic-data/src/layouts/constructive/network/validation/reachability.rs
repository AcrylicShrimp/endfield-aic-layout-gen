//! Independent directed coverage over already checked witness cells and segments.
use super::*;

pub(super) struct Reachability<'a> {
    // No constructor index or cached validity is accepted. Rebuild for each immutable witness.
    _segments: &'a [TransportNetworkSegment],
    vertices: BTreeMap<Vertex, usize>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
    seen: Vec<bool>,
    queue: Vec<usize>,
}

impl<'a> Reachability<'a> {
    pub(super) fn new(
        cells: &BTreeSet<(i64, i64)>,
        segments: &'a [TransportNetworkSegment],
        bridges: &BTreeSet<(i64, i64)>,
    ) -> Self {
        let span = profile_span(ProfileOp::ValidationIndexBuild);
        let positions = cells
            .iter()
            .filter(|p| !bridges.contains(p))
            .map(|&p| Vertex::ordinary(p))
            .chain(segments.iter().flat_map(|s| {
                [
                    Vertex::segment(&s.from, &s.to, bridges),
                    Vertex::segment(&s.to, &s.from, bridges),
                ]
            }))
            .collect::<BTreeSet<_>>();
        let vertices = positions
            .into_iter()
            .enumerate()
            .map(|(i, p)| (p, i))
            .collect::<BTreeMap<_, _>>();
        let count = vertices.len();
        let mut outgoing = vec![Vec::new(); count];
        let mut incoming = vec![Vec::new(); count];
        // Segment membership, adjacency, rates and duplicates were checked before this stage.
        for segment in segments {
            let from = vertices[&Vertex::segment(&segment.from, &segment.to, bridges)];
            let to = vertices[&Vertex::segment(&segment.to, &segment.from, bridges)];
            outgoing[from].push(to);
            incoming[to].push(from);
        }
        span.add_work("vertices-indexed", count as u64);
        span.add_work("segments-indexed", segments.len() as u64);
        Self {
            _segments: segments,
            vertices,
            outgoing,
            incoming,
            seen: vec![false; count],
            queue: Vec::with_capacity(count),
        }
    }

    /// Returns coverage and actual dequeued-vertex/outgoing-entry work; roots are checked cells.
    pub(super) fn covers(
        &mut self,
        roots: impl Iterator<Item = (i64, i64)>,
        reverse: bool,
    ) -> (bool, u64, u64) {
        self.seen.fill(false);
        self.queue.clear();
        for p in roots {
            let Some(&root) = self.vertices.get(&Vertex::ordinary(p)) else {
                return (false, 0, 0);
            };
            if !self.seen[root] {
                self.seen[root] = true;
                self.queue.push(root);
            }
        }
        let adjacency = if reverse {
            &self.incoming
        } else {
            &self.outgoing
        };
        let mut head = 0;
        let mut examined = 0;
        while head < self.queue.len() {
            let current = self.queue[head];
            head += 1;
            for &next in &adjacency[current] {
                examined += 1;
                if !self.seen[next] {
                    self.seen[next] = true;
                    self.queue.push(next);
                }
            }
        }
        (
            self.queue.len() == self.vertices.len(),
            head as u64,
            examined,
        )
    }
}

#[cfg(test)]
mod tests;
