//! Directed chain decomposition with explicit terminal/component anchors.

use aic_data::facilities::FacilityPortDirection;
use aic_data::layouts::{TransportNetworkEndpoint, TransportNetworkTerminal};
use aic_data::logistics::TransportKind;
use aic_data::recipes::Rate;
use indexmap::{IndexMap, IndexSet};
use serde::Serialize;

use super::types::*;

#[derive(Clone, Debug, Serialize)]
pub struct Chain {
    pub id: usize,
    pub network: usize,
    pub transport: TransportKind,
    pub item: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub zero_length: bool,
    pub cells: Vec<Point>,
    pub rate: Rate,
    pub first: Option<usize>,
    pub last: Option<usize>,
    #[serde(rename = "startTerminals")]
    pub start_terminals: Vec<TransportNetworkTerminal>,
    #[serde(rename = "endTerminals")]
    pub end_terminals: Vec<TransportNetworkTerminal>,
}
impl Chain {
    pub fn start(&self) -> Point {
        self.cells[0]
    }
    pub fn end(&self) -> Point {
        *self.cells.last().expect("chains contain at least one cell")
    }
    pub fn tag(&self, p: Point) -> LayerPoint {
        (layer(self.transport), p)
    }
}

#[derive(Clone, Debug)]
pub struct Pair {
    pub network: usize,
    pub source: TransportNetworkTerminal,
    pub target: TransportNetworkTerminal,
}
pub struct Decomposition {
    pub chains: Vec<Chain>,
    pub anchors: [IndexSet<Point>; 2],
}

pub fn detachable_pairs(w: &Witness) -> IndexMap<LayerPoint, Pair> {
    let mut slots: IndexMap<LayerPoint, Vec<(usize, &TransportNetworkTerminal)>> = IndexMap::new();
    let mut used = IndexSet::new();
    let mut ids = IndexMap::<&str, usize>::new();
    let components: IndexSet<_> = w
        .logistics_components
        .iter()
        .map(|c| (layer(c.transport), Point::from(&c.position)))
        .collect();
    for (ni, n) in w.transport_networks.iter().enumerate() {
        for s in &n.segments {
            used.insert((layer(n.transport), Point::from(&s.from)));
            used.insert((layer(n.transport), Point::from(&s.to)));
        }
        for t in &n.terminals {
            *ids.entry(&t.id).or_default() += 1;
            slots
                .entry((layer(n.transport), Point::from(&t.position)))
                .or_default()
                .push((ni, t));
        }
    }
    let mut result = IndexMap::new();
    for (tag, entries) in slots {
        if entries.len() != 2
            || used.contains(&tag)
            || components.contains(&tag)
            || entries[0].0 != entries[1].0
        {
            continue;
        }
        let (a, b) = (entries[0], entries[1]);
        let (source, target) = if a.1.direction == FacilityPortDirection::Output {
            (a, b)
        } else {
            (b, a)
        };
        let (s, t) = (source.1, target.1);
        if s.direction != FacilityPortDirection::Output
            || t.direction != FacilityPortDirection::Input
            || !matches!(s.endpoint, TransportNetworkEndpoint::Facility { .. })
            || !matches!(t.endpoint, TransportNetworkEndpoint::Facility { .. })
            || ids[s.id.as_str()] != 1
            || ids[t.id.as_str()] != 1
            || !positive_rate(s.rate)
            || !positive_rate(t.rate)
            || !equal_rate(s.rate, t.rate)
        {
            continue;
        }
        result.insert(
            tag,
            Pair {
                network: source.0,
                source: s.clone(),
                target: t.clone(),
            },
        );
    }
    result
}

pub fn extract(w: &Witness, released: &IndexSet<LayerPoint>) -> Result<Decomposition> {
    let mut chains = Vec::new();
    let mut anchors: [IndexSet<Point>; 2] = Default::default();
    let pairs = detachable_pairs(w);
    for c in &w.logistics_components {
        anchors[layer(c.transport)].insert(Point::from(&c.position));
    }
    let mut owners = IndexMap::<LayerPoint, usize>::new();
    for n in &w.transport_networks {
        for p in &n.cells {
            *owners
                .entry((layer(n.transport), Point::from(p)))
                .or_default() += 1;
        }
    }
    for (ni, n) in w.transport_networks.iter().enumerate() {
        let tr = layer(n.transport);
        let mut ins = IndexMap::<Point, Vec<usize>>::new();
        let mut outs = IndexMap::<Point, Vec<usize>>::new();
        let mut local = IndexSet::new();
        let mut terminals = IndexMap::<Point, Vec<TransportNetworkTerminal>>::new();
        let fail = |code, message| Diagnostic::new(code, message, n.id.clone());
        for (si, s) in n.segments.iter().enumerate() {
            if direction(Point::from(&s.from), Point::from(&s.to)).is_none() {
                return Err(fail(
                    "local-nonunit-segment",
                    "Expected unit directed segment",
                ));
            }
            ins.entry(Point::from(&s.to)).or_default().push(si);
            outs.entry(Point::from(&s.from)).or_default().push(si);
        }
        for t in &n.terminals {
            local.insert(Point::from(&t.position));
            terminals
                .entry(Point::from(&t.position))
                .or_default()
                .push(t.clone());
        }
        for p in &n.cells {
            let p = Point::from(p);
            if released.contains(&(tr, p)) {
                continue;
            }
            let a = ins.get(&p).map(Vec::as_slice).unwrap_or_default();
            let b = outs.get(&p).map(Vec::as_slice).unwrap_or_default();
            if anchors[tr].contains(&p)
                || owners[&(tr, p)] > 1
                || a.len() != 1
                || b.len() != 1
                || !equal_rate(n.segments[a[0]].rate, n.segments[b[0]].rate)
            {
                local.insert(p);
            }
        }
        let mut visited = IndexSet::new();
        for (si, s) in n.segments.iter().enumerate() {
            if !local.contains(&Point::from(&s.from)) {
                continue;
            }
            let mut path = vec![Point::from(&s.from)];
            let mut current = Some(si);
            while let Some(i) = current {
                if !visited.insert(i) {
                    return Err(fail("local-chain-cycle", "Chain repeats an edge"));
                }
                let edge = &n.segments[i];
                let end = Point::from(&edge.to);
                path.push(end);
                if local.contains(&end) {
                    break;
                }
                let following = outs.get(&end).map(Vec::as_slice).unwrap_or_default();
                current = if released.contains(&(tr, end)) {
                    following.iter().copied().find(|&j| {
                        direction(
                            Point::from(&n.segments[j].from),
                            Point::from(&n.segments[j].to),
                        ) == direction(Point::from(&edge.from), end)
                    })
                } else {
                    following.first().copied()
                };
            }
            let end = *path.last().expect("nonempty path");
            if !local.contains(&end) {
                return Err(fail("local-open-chain", "Chain has no fixed endpoint"));
            }
            chains.push(Chain {
                id: chains.len(),
                network: ni,
                transport: n.transport,
                item: n.item.clone(),
                zero_length: false,
                rate: s.rate,
                first: direction(path[0], path[1]),
                last: direction(path[path.len() - 2], end),
                start_terminals: terminals.get(&path[0]).cloned().unwrap_or_default(),
                end_terminals: terminals.get(&end).cloned().unwrap_or_default(),
                cells: path,
            });
        }
        if visited.len() != n.segments.len() {
            return Err(fail(
                "local-unanchored-cycle",
                "A directed cycle has no terminal or component anchor",
            ));
        }
        for p in pairs.values().filter(|p| p.network == ni) {
            chains.push(Chain {
                id: chains.len(),
                network: ni,
                transport: n.transport,
                item: n.item.clone(),
                zero_length: true,
                cells: vec![Point::from(&p.source.position)],
                rate: p.source.rate,
                first: None,
                last: None,
                start_terminals: vec![p.source.clone()],
                end_terminals: vec![p.target.clone()],
            });
        }
        anchors[tr].extend(local);
    }
    Ok(Decomposition { chains, anchors })
}

fn terminal_counts(
    w: &Witness,
) -> (
    IndexMap<LayerPoint, usize>,
    IndexMap<&str, usize>,
    IndexSet<LayerPoint>,
) {
    let mut slots = IndexMap::<LayerPoint, usize>::new();
    let mut ids = IndexMap::<&str, usize>::new();
    for n in &w.transport_networks {
        for t in &n.terminals {
            *slots
                .entry((layer(n.transport), Point::from(&t.position)))
                .or_default() += 1;
            *ids.entry(&t.id).or_default() += 1;
        }
    }
    let components = w
        .logistics_components
        .iter()
        .map(|c| (layer(c.transport), Point::from(&c.position)))
        .collect();
    (slots, ids, components)
}
fn incident_counts(chains: &[Chain]) -> IndexMap<LayerPoint, usize> {
    let mut incident = IndexMap::<LayerPoint, usize>::new();
    for c in chains {
        for p in [c.start(), c.end()] {
            *incident.entry(c.tag(p)).or_default() += 1;
        }
    }
    incident
}
pub fn flexible_terminals(w: &Witness, chains: &[Chain]) -> IndexSet<LayerPoint> {
    let (slots, _, components) = terminal_counts(w);
    let incident = incident_counts(chains);
    slots
        .into_iter()
        .filter(|(k, n)| *n == 1 && incident.get(k) == Some(&1) && !components.contains(k))
        .map(|(k, _)| k)
        .collect()
}
pub fn joinable_chains(w: &Witness, chains: &[Chain]) -> Vec<usize> {
    let (slots, ids, components) = terminal_counts(w);
    let incident = incident_counts(chains);
    chains
        .iter()
        .filter(|c| {
            if c.zero_length
                || c.cells.len() < 2
                || c.start_terminals.len() != 1
                || c.end_terminals.len() != 1
            {
                return false;
            }
            let a = &c.start_terminals[0];
            let b = &c.end_terminals[0];
            a.direction == FacilityPortDirection::Output
                && b.direction == FacilityPortDirection::Input
                && [a, b].iter().all(|t| {
                    let tag = c.tag(Point::from(&t.position));
                    matches!(t.endpoint, TransportNetworkEndpoint::Facility { .. })
                        && ids[t.id.as_str()] == 1
                        && slots.get(&tag) == Some(&1)
                        && incident.get(&tag) == Some(&1)
                        && !components.contains(&tag)
                        && positive_rate(t.rate)
                        && positive_rate(c.rate)
                        && equal_rate(t.rate, c.rate)
                })
        })
        .map(|c| c.id)
        .collect()
}
