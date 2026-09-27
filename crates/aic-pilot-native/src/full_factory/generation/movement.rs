//! Immutable movement indexes and private simultaneous-movement transactions.

use aic_data::layouts::{FacilityPlacement, TransportNetworkEndpoint, TransportNetworkTerminal};
use indexmap::{IndexMap, IndexSet};

use super::chains::{Chain, Pair, detachable_pairs, joinable_chains};
use super::changes::{ComponentChange, FacilityChange, Move};
use super::types::*;

pub struct Cluster {
    pub anchors: Vec<LayerPoint>,
    pub neighbors: Vec<TransportNetworkTerminal>,
}
pub struct MovementIndex {
    pub terminals: IndexMap<LayerPoint, Vec<TransportNetworkTerminal>>,
    pub clusters: Vec<Cluster>,
    pub network_cells: IndexMap<LayerPoint, Vec<usize>>,
    pub cell_chains: [IndexMap<Point, Vec<usize>>; 2],
    pub pairs: IndexMap<LayerPoint, Pair>,
    pub joinable: Vec<usize>,
    pub body_keys: IndexMap<String, Vec<Point>>,
}
impl MovementIndex {
    pub fn new(w: &Witness, chains: &[Chain]) -> Self {
        let mut adjacency = IndexMap::<LayerPoint, IndexSet<LayerPoint>>::new();
        let mut cell_chains: [IndexMap<Point, Vec<usize>>; 2] = Default::default();
        for c in chains {
            let a = c.tag(c.start());
            let b = c.tag(c.end());
            adjacency.entry(a).or_default().insert(b);
            adjacency.entry(b).or_default().insert(a);
            for p in &c.cells {
                cell_chains[layer(c.transport)]
                    .entry(*p)
                    .or_default()
                    .push(c.id);
            }
        }
        let mut terminals = IndexMap::<LayerPoint, Vec<TransportNetworkTerminal>>::new();
        let mut network_cells = IndexMap::<LayerPoint, Vec<usize>>::new();
        let mut ordinal = 0;
        for n in &w.transport_networks {
            for t in &n.terminals {
                terminals
                    .entry((layer(n.transport), Point::from(&t.position)))
                    .or_default()
                    .push(t.clone());
            }
            for p in &n.cells {
                network_cells
                    .entry((layer(n.transport), Point::from(p)))
                    .or_default()
                    .push(ordinal);
                ordinal += 1;
            }
        }
        let mut visited = IndexSet::new();
        let mut clusters = Vec::new();
        for &start in adjacency.keys() {
            if terminals.contains_key(&start) || visited.contains(&start) {
                continue;
            }
            let mut queue = vec![start];
            let mut neighbors = IndexSet::new();
            visited.insert(start);
            let mut i = 0;
            while i < queue.len() {
                if let Some(vs) = adjacency.get(&queue[i]) {
                    for &v in vs {
                        if terminals.contains_key(&v) {
                            neighbors.insert(v);
                        } else if visited.insert(v) {
                            queue.push(v);
                        }
                    }
                }
                i += 1;
            }
            clusters.push(Cluster {
                anchors: queue,
                neighbors: neighbors
                    .iter()
                    .flat_map(|v| terminals[v].clone())
                    .collect(),
            });
        }
        Self {
            terminals,
            clusters,
            network_cells,
            cell_chains,
            pairs: detachable_pairs(w),
            joinable: joinable_chains(w, chains),
            body_keys: w
                .placements
                .iter()
                .map(|p| (p.instance.clone(), body_cells(p).collect()))
                .collect(),
        }
    }
}

#[derive(Default)]
pub struct Movement {
    pub selected: IndexSet<String>,
    pub moved: IndexSet<LayerPoint>,
    pub joined: IndexSet<usize>,
    pub separated: IndexSet<LayerPoint>,
    pub components: Vec<String>,
    pub anchor_turns: IndexMap<LayerPoint, usize>,
    pub fixed_chains: IndexMap<usize, Vec<Point>>,
    pub changed: IndexSet<usize>,
    pub body: IndexSet<Point>,
    shifts: IndexMap<String, Point>,
    anchor_shifts: IndexMap<LayerPoint, Point>,
    facility_changes: IndexMap<String, FacilityChange>,
}
impl Movement {
    pub fn shift_facility(&self, p: &FacilityPlacement) -> FacilityPlacement {
        if let Some(change) = self.facility_changes.get(&p.instance) {
            return change.pose.clone();
        }
        let mut p = p.clone();
        if let Some(d) = self.shifts.get(&p.instance) {
            p.x += d.x;
            p.y += d.y;
        }
        p
    }
    pub fn shift_anchor(&self, tr: usize, p: Point) -> Point {
        self.anchor_shifts
            .get(&(tr, p))
            .map_or(p, |d| p.shifted(d.x, d.y))
    }
    fn terminal_delta(&self, t: &TransportNetworkTerminal) -> Option<Point> {
        let TransportNetworkEndpoint::Facility { instance, .. } = &t.endpoint else {
            return None;
        };
        if let Some(p) = self
            .facility_changes
            .get(instance)
            .and_then(|c| c.terminal_positions.get(&t.id))
        {
            return Some(Point {
                x: p.x - t.position.x,
                y: p.y - t.position.y,
            });
        }
        self.shifts.get(instance).copied()
    }
    fn terminal_turns(&self, t: &TransportNetworkTerminal) -> usize {
        if let TransportNetworkEndpoint::Facility { instance, .. } = &t.endpoint {
            self.facility_changes.get(instance).map_or(0, |c| c.turns)
        } else {
            0
        }
    }
    pub fn shift_terminal(&self, t: &TransportNetworkTerminal) -> Point {
        let p = Point::from(&t.position);
        self.terminal_delta(t).map_or(p, |d| p.shifted(d.x, d.y))
    }
    pub fn shift_endpoint(&self, c: &Chain, end: bool) -> Point {
        if c.zero_length {
            self.shift_terminal(if end {
                &c.end_terminals[0]
            } else {
                &c.start_terminals[0]
            })
        } else {
            self.shift_anchor(layer(c.transport), if end { c.end() } else { c.start() })
        }
    }
    fn owns(&self, t: &TransportNetworkTerminal) -> bool {
        matches!(&t.endpoint, TransportNetworkEndpoint::Facility { instance, .. } if self.selected.contains(instance))
    }
    fn common(&self, ts: &[TransportNetworkTerminal]) -> Option<Point> {
        let first = ts.first()?;
        let d = self.terminal_delta(first)?;
        ts.iter()
            .all(|t| self.owns(t) && self.terminal_delta(t) == Some(d))
            .then_some(d)
    }
}

fn error(code: &str, message: &str, entity: impl Into<serde_json::Value>) -> Diagnostic {
    let mut e = Diagnostic::new(code, message, entity);
    e.stage = "local-movement";
    e.severity = "info";
    e
}
fn point_entity(p: Point) -> serde_json::Value {
    format!("{},{}", p.x, p.y).into()
}
fn tag_entity((tr, p): LayerPoint) -> serde_json::Value {
    format!("{}:{},{}", if tr == 0 { "belt" } else { "pipe" }, p.x, p.y).into()
}

pub fn plan(
    w: &Witness,
    chains: &[Chain],
    anchors: &[IndexSet<Point>; 2],
    moves: &[Move],
    component_changes: &IndexMap<String, ComponentChange>,
    facility_changes: &IndexMap<String, FacilityChange>,
    reroute: &IndexSet<usize>,
    index: &MovementIndex,
    joinable: Option<&[usize]>,
) -> Result<Movement> {
    let instances: Vec<_> = moves
        .iter()
        .flat_map(|m| m.instances.iter().cloned())
        .collect();
    let entity = || serde_json::to_value(&instances).expect("string list");
    if moves.len() > 4
        || moves.iter().any(|m| {
            m.instances.is_empty()
                || m.instances
                    .iter()
                    .any(|id| !w.placements.iter().any(|p| p.instance == *id))
        })
    {
        return Err(error(
            "local-move-input",
            "Expected move schema 3: up to four translation primitives",
            serde_json::to_value(moves).expect("typed moves"),
        ));
    }
    let selected: IndexSet<_> = instances.iter().cloned().collect();
    if selected.len() != instances.len() {
        return Err(error(
            "local-move-input",
            "Expected distinct selected facilities",
            serde_json::to_value(moves).expect("typed moves"),
        ));
    }
    if facility_changes.keys().any(|id| !selected.contains(id)) {
        return Err(error(
            "local-facility-rotation",
            "Rotated facilities must belong to explicit translation primitives",
            serde_json::Value::Null,
        ));
    }
    let mut m = Movement {
        selected,
        shifts: moves
            .iter()
            .flat_map(|m| {
                m.instances
                    .iter()
                    .map(move |id| (id.clone(), Point { x: m.dx, y: m.dy }))
            })
            .collect(),
        facility_changes: facility_changes.clone(),
        ..Default::default()
    };
    for (&tag, ts) in &index.terminals {
        if !ts.iter().any(|t| m.owns(t)) {
            continue;
        }
        let delta = m.common(ts);
        let turns = m.terminal_turns(&ts[0]);
        let same_turns = ts.iter().all(|t| m.terminal_turns(t) == turns);
        if (delta.is_none() || !same_turns) && index.pairs.contains_key(&tag) {
            m.separated.insert(tag);
            continue;
        }
        let Some(delta) = delta else {
            return Err(error(
                "local-shared-terminal-move",
                "Non-detachable shared terminal owners must receive identical translations",
                tag_entity(tag),
            ));
        };
        if !same_turns {
            return Err(error(
                "local-shared-terminal-rotation",
                "Shared terminal owners have incompatible retained arm rotations in this neighborhood",
                tag_entity(tag),
            ));
        }
        m.moved.insert(tag);
        m.anchor_shifts.insert(tag, delta);
        if turns != 0 {
            m.anchor_turns.insert(tag, turns);
        }
    }
    // The full-factory contract uses internal junction ownership exclusively.
    for cluster in &index.clusters {
        if let Some(delta) = m.common(&cluster.neighbors) {
            for &v in &cluster.anchors {
                m.moved.insert(v);
                m.anchor_shifts.insert(v, delta);
            }
        }
    }
    for c in &w.logistics_components {
        if let Some(pose) = component_changes.get(&c.id) {
            let tag = (layer(c.transport), Point::from(&c.position));
            m.moved.insert(tag);
            m.anchor_shifts.insert(
                tag,
                Point {
                    x: pose.position.x - c.position.x,
                    y: pose.position.y - c.position.y,
                },
            );
            m.anchor_turns.insert(tag, pose.turns);
        }
    }
    let bodies: Vec<_> = w
        .placements
        .iter()
        .filter(|p| m.selected.contains(&p.instance))
        .map(|p| m.shift_facility(p))
        .collect();
    let cells: IndexSet<_> = bodies.iter().flat_map(body_cells).collect();
    let stationary: IndexSet<_> = w
        .placements
        .iter()
        .filter(|p| !m.selected.contains(&p.instance))
        .flat_map(|p| index.body_keys[&p.instance].iter().copied())
        .collect();
    if cells.len() as i64 != bodies.iter().map(|p| p.width * p.height).sum::<i64>()
        || cells.iter().any(|p| stationary.contains(p))
    {
        return Err(error(
            "local-body-collision",
            "Simultaneous translated bodies overlap each other or a retained facility",
            entity(),
        ));
    }

    let components: IndexSet<_> = w
        .logistics_components
        .iter()
        .map(|c| {
            (
                layer(c.transport),
                m.shift_anchor(layer(c.transport), Point::from(&c.position)),
            )
        })
        .collect();
    let mut slots = IndexMap::<LayerPoint, Vec<&str>>::new();
    for n in &w.transport_networks {
        for t in &n.terminals {
            slots
                .entry((layer(n.transport), m.shift_terminal(t)))
                .or_default()
                .push(&t.id);
        }
    }
    let mut joined_origins = IndexMap::<LayerPoint, IndexSet<Point>>::new();
    for &id in joinable.unwrap_or(&index.joinable) {
        let c = &chains[id];
        let a = &c.start_terminals[0];
        let b = &c.end_terminals[0];
        let tag = c.tag(m.shift_terminal(a));
        let ts = slots.get(&tag).expect("terminal slot indexed");
        if tag != c.tag(m.shift_terminal(b))
            || components.contains(&tag)
            || ts.len() != 2
            || !ts.contains(&a.id.as_str())
            || !ts.contains(&b.id.as_str())
        {
            continue;
        }
        m.joined.insert(c.id);
        joined_origins.insert(
            tag,
            IndexSet::from([Point::from(&a.position), Point::from(&b.position)]),
        );
    }
    let allows = |tr, destination, original| {
        joined_origins
            .get(&(tr, destination))
            .is_some_and(|ps| ps.contains(&original))
    };
    let mut origins = IndexMap::<LayerPoint, Point>::new();
    let mut carry = |tr: usize, before: Point, after: Point| -> Result<()> {
        if let Some(&previous) = origins.get(&(tr, after)) {
            if previous != before && !(allows(tr, after, before) && allows(tr, after, previous)) {
                return Err(error(
                    "local-carried-collision",
                    "Different carried cells collide on one layer",
                    serde_json::json!({"transport": if tr == 0 { "belt" } else { "pipe" }, "position": after}),
                ));
            }
        }
        origins.insert((tr, after), before);
        Ok(())
    };
    let mut moved_anchors: [IndexSet<Point>; 2] = Default::default();
    let mut moved_cells: Vec<_> = m
        .moved
        .iter()
        .flat_map(|&tag| {
            index
                .network_cells
                .get(&tag)
                .into_iter()
                .flatten()
                .map(move |&ordinal| (ordinal, tag))
        })
        .collect();
    moved_cells.sort_by_key(|p| p.0);
    for (_, (tr, p)) in moved_cells {
        let q = m.shift_anchor(tr, p);
        carry(tr, p, q)?;
        moved_anchors[tr].insert(q);
    }
    for &tag in &m.separated {
        let pair = &index.pairs[&tag];
        for t in [&pair.source, &pair.target] {
            let q = m.shift_terminal(t);
            carry(tag.0, Point::from(&t.position), q)?;
            moved_anchors[tag.0].insert(q);
        }
    }
    for tr in 0..2 {
        for &p in &anchors[tr] {
            if m.moved.contains(&(tr, p)) || m.separated.contains(&(tr, p)) {
                continue;
            }
            if cells.contains(&p) {
                return Err(error(
                    "local-anchor-collision",
                    "Translated body intersects a fixed anchor",
                    point_entity(p),
                ));
            }
            if moved_anchors[tr].contains(&p) && !allows(tr, p, p) {
                return Err(error(
                    "local-terminal-collision",
                    "Translated anchor intersects a retained anchor",
                    point_entity(p),
                ));
            }
        }
    }
    for ps in &moved_anchors {
        for &p in ps {
            if stationary.contains(&p) || cells.contains(&p) {
                return Err(error(
                    "local-endpoint-body",
                    "Translated anchor intersects a facility body",
                    point_entity(p),
                ));
            }
        }
    }
    m.components = w
        .logistics_components
        .iter()
        .filter(|c| {
            m.moved
                .contains(&(layer(c.transport), Point::from(&c.position)))
        })
        .map(|c| c.id.clone())
        .collect();
    for c in chains {
        let start = c.tag(c.start());
        let end = c.tag(c.end());
        if reroute.contains(&c.id)
            || m.anchor_turns.get(&start).copied().unwrap_or_default() != 0
            || m.anchor_turns.get(&end).copied().unwrap_or_default() != 0
        {
            continue;
        }
        let Some(&d) = m.anchor_shifts.get(&start) else {
            continue;
        };
        if m.anchor_shifts.get(&end) != Some(&d) {
            continue;
        }
        let path: Vec<_> = c.cells.iter().map(|p| p.shifted(d.x, d.y)).collect();
        if path
            .iter()
            .any(|p| stationary.contains(p) || cells.contains(p))
        {
            return Err(error(
                "local-internal-body-collision",
                "Translated internal route intersects a body",
                c.id,
            ));
        }
        for (&before, &after) in c.cells.iter().zip(&path) {
            carry(layer(c.transport), before, after)?;
        }
        m.fixed_chains.insert(c.id, path);
    }
    let mut carried = moved_anchors;
    for c in chains {
        if let Some(path) = m.fixed_chains.get(&c.id) {
            carried[layer(c.transport)].extend(path.iter().copied());
        }
    }
    for tr in 0..2 {
        for &p in &anchors[tr] {
            if !m.moved.contains(&(tr, p))
                && !m.separated.contains(&(tr, p))
                && carried[tr].contains(&p)
                && !allows(tr, p, p)
            {
                return Err(error(
                    "local-internal-anchor-collision",
                    "Translated internal route intersects a retained anchor",
                    point_entity(p),
                ));
            }
        }
    }
    let mut intersects = IndexSet::<usize>::new();
    for tr in 0..2 {
        for p in cells.iter().chain(&carried[tr]) {
            if let Some(ids) = index.cell_chains[tr].get(p) {
                intersects.extend(ids);
            }
        }
    }
    for c in chains {
        let start = c.tag(c.start());
        let end = c.tag(c.end());
        if !m.fixed_chains.contains_key(&c.id)
            && (m.separated.contains(&start)
                || m.moved.contains(&start)
                || m.moved.contains(&end)
                || intersects.contains(&c.id))
        {
            m.changed.insert(c.id);
        }
    }
    m.body = cells;
    Ok(m)
}
