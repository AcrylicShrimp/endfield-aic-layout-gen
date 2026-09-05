//! Independent directed coverage over already checked witness cells and segments.
use super::*;

pub(super) struct Reachability<'a> {
    // No constructor index or cached validity is accepted. Rebuild for each immutable witness.
    _segments: &'a [TransportNetworkSegment],
    vertices: BTreeMap<(i64, i64), usize>,
    outgoing: Vec<Vec<usize>>,
    incoming: Vec<Vec<usize>>,
    seen: Vec<bool>,
    queue: Vec<usize>,
}

impl<'a> Reachability<'a> {
    pub(super) fn new(
        cells: &BTreeSet<(i64, i64)>,
        segments: &'a [TransportNetworkSegment],
    ) -> Self {
        let span = profile_span(ProfileOp::ValidationIndexBuild);
        let vertices = cells
            .iter()
            .enumerate()
            .map(|(i, &p)| (p, i))
            .collect::<BTreeMap<_, _>>();
        let mut outgoing = vec![Vec::new(); cells.len()];
        let mut incoming = vec![Vec::new(); cells.len()];
        // Segment membership, adjacency, rates and duplicates were checked before this stage.
        for segment in segments {
            let from = vertices[&xy(&segment.from)];
            let to = vertices[&xy(&segment.to)];
            outgoing[from].push(to);
            incoming[to].push(from);
        }
        span.add_work("vertices-indexed", cells.len() as u64);
        span.add_work("segments-indexed", segments.len() as u64);
        Self {
            _segments: segments,
            vertices,
            outgoing,
            incoming,
            seen: vec![false; cells.len()],
            queue: Vec::with_capacity(cells.len()),
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
            let root = self.vertices[&p];
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
