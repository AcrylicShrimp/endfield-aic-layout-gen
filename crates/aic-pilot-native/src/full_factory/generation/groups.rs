use aic_data::facilities::FacilityPortDirection;
use aic_data::layouts::TransportNetworkEndpoint;
use aic_data::logistics::LogisticsComponentKind;
use indexmap::{IndexMap, IndexSet};
use serde::Serialize;

use super::chains::detachable_pairs;
use super::types::*;

#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub members: Vec<usize>,
    pub source: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct Omitted {
    pub members: Vec<usize>,
    pub source: String,
    pub code: String,
}
pub struct Groups {
    pub groups: Vec<Group>,
    pub omitted: Vec<Omitted>,
}

pub fn production_edges(w: &Witness) -> Vec<(usize, usize)> {
    let indices: IndexMap<_, _> = w
        .placements
        .iter()
        .enumerate()
        .map(|(i, p)| (p.instance.as_str(), i))
        .collect();
    let mut edges = IndexSet::new();
    for n in &w.transport_networks {
        let bridges: IndexSet<_> = w
            .logistics_components
            .iter()
            .filter(|c| c.kind == LogisticsComponentKind::Bridge && c.transport == n.transport)
            .map(|c| Point::from(&c.position))
            .collect();
        let vertex = |p: Point, horizontal: bool| {
            (
                p,
                if bridges.contains(&p) {
                    if horizontal { 1 } else { 2 }
                } else {
                    0
                },
            )
        };
        let mut out = IndexMap::<(Point, usize), Vec<(Point, usize)>>::new();
        let mut sinks = IndexMap::<(Point, usize), Vec<usize>>::new();
        for s in &n.segments {
            let horizontal = s.from.y == s.to.y;
            out.entry(vertex(Point::from(&s.from), horizontal))
                .or_default()
                .push(vertex(Point::from(&s.to), horizontal));
        }
        for t in &n.terminals {
            if let TransportNetworkEndpoint::Facility { instance, .. } = &t.endpoint {
                if t.direction == FacilityPortDirection::Input {
                    sinks
                        .entry((Point::from(&t.position), 0))
                        .or_default()
                        .push(indices[instance.as_str()]);
                }
            }
        }
        for t in &n.terminals {
            let TransportNetworkEndpoint::Facility { instance, .. } = &t.endpoint else {
                continue;
            };
            if t.direction != FacilityPortDirection::Output {
                continue;
            }
            let source = indices[instance.as_str()];
            let start = (Point::from(&t.position), 0);
            let mut seen = IndexSet::from([start]);
            let mut queue = vec![start];
            let mut i = 0;
            while i < queue.len() {
                if let Some(ss) = sinks.get(&queue[i]) {
                    for &sink in ss {
                        if source != sink {
                            edges.insert((source, sink));
                        }
                    }
                }
                if let Some(vs) = out.get(&queue[i]) {
                    for &next in vs {
                        if seen.insert(next) {
                            queue.push(next);
                        }
                    }
                }
                i += 1;
            }
        }
    }
    edges.into_iter().collect()
}

fn components(count: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    struct Tarjan {
        adjacent: Vec<Vec<usize>>,
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        stack: Vec<usize>,
        active: Vec<bool>,
        result: Vec<Vec<usize>>,
        serial: usize,
    }
    impl Tarjan {
        fn visit(&mut self, v: usize) {
            self.index[v] = Some(self.serial);
            self.low[v] = self.serial;
            self.serial += 1;
            self.stack.push(v);
            self.active[v] = true;
            for next in self.adjacent[v].clone() {
                if self.index[next].is_none() {
                    self.visit(next);
                    self.low[v] = self.low[v].min(self.low[next]);
                } else if self.active[next] {
                    self.low[v] = self.low[v].min(self.index[next].unwrap());
                }
            }
            if Some(self.low[v]) == self.index[v] {
                let mut members = Vec::new();
                loop {
                    let next = self.stack.pop().unwrap();
                    self.active[next] = false;
                    members.push(next);
                    if next == v {
                        break;
                    }
                }
                self.result.push(members);
            }
        }
    }
    let mut t = Tarjan {
        adjacent: vec![vec![]; count],
        index: vec![None; count],
        low: vec![0; count],
        stack: vec![],
        active: vec![false; count],
        result: vec![],
        serial: 0,
    };
    for &(a, b) in edges {
        t.adjacent[a].push(b);
    }
    for i in 0..count {
        if t.index[i].is_none() {
            t.visit(i);
        }
    }
    t.result
}
fn find(parent: &mut [usize], i: usize) -> usize {
    if parent[i] != i {
        parent[i] = find(parent, parent[i]);
    }
    parent[i]
}
fn closure(parent: &mut [usize], members: &[usize]) -> Vec<usize> {
    let roots: IndexSet<_> = members.iter().map(|&i| find(parent, i)).collect();
    (0..parent.len())
        .filter(|&i| roots.contains(&find(parent, i)))
        .collect()
}
fn add(
    groups: &mut Groups,
    seen: &mut IndexSet<Vec<usize>>,
    frozen: &IndexSet<usize>,
    mut members: Vec<usize>,
    source: &str,
) {
    members.sort_unstable();
    members.dedup();
    if !seen.insert(members.clone()) {
        return;
    }
    if members.len() > 16 || members.iter().any(|i| frozen.contains(i)) {
        let code = if members.len() > 16 {
            "factory.group-member-budget"
        } else {
            "factory.group-external-anchor"
        };
        groups.omitted.push(Omitted {
            members,
            source: source.into(),
            code: code.into(),
        });
    } else {
        groups.groups.push(Group {
            members,
            source: source.into(),
        });
    }
}
pub fn pose_groups(w: &Witness, include_connected: bool) -> Groups {
    let indices: IndexMap<_, _> = w
        .placements
        .iter()
        .enumerate()
        .map(|(i, p)| (p.instance.as_str(), i))
        .collect();
    let mut parent: Vec<_> = (0..w.placements.len()).collect();
    let mut frozen = IndexSet::new();
    let mut slots =
        IndexMap::<LayerPoint, Vec<&aic_data::layouts::TransportNetworkTerminal>>::new();
    let pairs = detachable_pairs(w);
    for n in &w.transport_networks {
        for t in &n.terminals {
            slots
                .entry((layer(n.transport), Point::from(&t.position)))
                .or_default()
                .push(t);
        }
    }
    for (tag, ts) in slots {
        if ts.len() < 2 || pairs.contains_key(&tag) {
            continue;
        }
        let members: Vec<_> = ts
            .iter()
            .filter_map(|t| match &t.endpoint {
                TransportNetworkEndpoint::Facility { instance, .. } => {
                    Some(indices[instance.as_str()])
                }
                _ => None,
            })
            .collect();
        for &i in members.iter().skip(1) {
            let a = find(&mut parent, members[0]);
            let b = find(&mut parent, i);
            if a != b {
                parent[b] = a;
            }
        }
        if ts
            .iter()
            .any(|t| matches!(t.endpoint, TransportNetworkEndpoint::External { .. }))
        {
            frozen.extend(members);
        }
    }
    let mut result = Groups {
        groups: vec![],
        omitted: vec![],
    };
    let mut seen = IndexSet::new();
    for i in 0..w.placements.len() {
        let required = closure(&mut parent, &[i]);
        if required.len() == 1 {
            add(&mut result, &mut seen, &frozen, required, "singleton");
        } else if include_connected {
            add(
                &mut result,
                &mut seen,
                &frozen,
                required,
                "terminal_closure",
            );
        } else {
            result.omitted.push(Omitted {
                members: vec![i],
                source: "singleton".into(),
                code: "factory.group-required-closure".into(),
            });
        }
    }
    if include_connected {
        let edges = production_edges(w);
        for scc in components(w.placements.len(), &edges) {
            if scc.len() > 1 {
                add(
                    &mut result,
                    &mut seen,
                    &frozen,
                    closure(&mut parent, &scc),
                    "cycle",
                );
            }
        }
        for (a, b) in edges {
            add(
                &mut result,
                &mut seen,
                &frozen,
                closure(&mut parent, &[a, b]),
                "edge",
            );
        }
    }
    result
}

pub fn compound_groups(w: &Witness) -> Vec<Group> {
    let mut groups = IndexMap::new();
    let mut add = |mut members: Vec<usize>, source: String| {
        members.sort_unstable();
        members.dedup();
        if (2..=4).contains(&members.len()) {
            groups
                .entry(members.clone())
                .or_insert(Group { members, source });
        }
    };
    for g in pose_groups(w, true).groups {
        add(g.members, g.source);
    }
    let mut adjacent = vec![IndexSet::new(); w.placements.len()];
    for (a, b) in production_edges(w) {
        adjacent[a].insert(b);
        adjacent[b].insert(a);
    }
    for axis_x in [true, false] {
        let mut lines = IndexMap::<(i64, i64), Vec<usize>>::new();
        for (i, p) in w.placements.iter().enumerate() {
            lines
                .entry(if axis_x {
                    (p.y, p.height)
                } else {
                    (p.x, p.width)
                })
                .or_default()
                .push(i);
        }
        for line in lines.values_mut() {
            line.sort_by_key(|&i| {
                (
                    if axis_x {
                        w.placements[i].x
                    } else {
                        w.placements[i].y
                    },
                    i,
                )
            });
            for i in 0..line.len() {
                for j in i + 1..(i + 4).min(line.len()) {
                    let a = &w.placements[line[j - 1]];
                    let b = &w.placements[line[j]];
                    if if axis_x {
                        a.x + a.width != b.x
                    } else {
                        a.y + a.height != b.y
                    } {
                        break;
                    }
                    let members = &line[i..=j];
                    let selected: IndexSet<_> = members.iter().copied().collect();
                    let mut seen = IndexSet::from([members[0]]);
                    let mut queue = vec![members[0]];
                    let mut k = 0;
                    while k < queue.len() {
                        for &next in &adjacent[queue[k]] {
                            if selected.contains(&next) && seen.insert(next) {
                                queue.push(next);
                            }
                        }
                        k += 1;
                    }
                    if seen.len() == members.len() {
                        add(
                            members.to_vec(),
                            if axis_x { "aligned-x" } else { "aligned-y" }.into(),
                        );
                    }
                }
            }
        }
    }
    groups.into_values().collect()
}
