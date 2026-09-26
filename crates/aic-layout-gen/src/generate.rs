use crate::{
    config::{Config, failure},
    packing::{self, Packing},
};
use aic_layout::{
    facilities::FacilityPortDirection,
    geometry::{Bounds, Diagnostic, Point, layer},
    physical::{
        Bridge, BridgeAxis, BridgeChannel, Connection, Layout, Metrics, Problem, Route, SCHEMA,
        Solution,
    },
    rate::Rate,
    rng::Rng,
    validation::{Catalogs, validate},
};
use aic_routing::{Carrier, PathOptions, RouteJob, crossing_options, shortest_path};
use indexmap::IndexSet;
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    time::Instant,
};

#[derive(Clone, Debug, Serialize)]
pub struct Attempt {
    pub ordinal: usize,
    pub seed: u64,
    pub status: String,
    pub blocks: usize,
    pub target_connections: usize,
    pub connected: usize,
    pub weak_components: usize,
    pub pair_trials: usize,
    pub route_calls: usize,
    pub route_expansions: usize,
    pub long_distance_threshold: i64,
    pub long_connections: usize,
    pub routing_statuses: BTreeMap<String, usize>,
    pub elapsed_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct Generated {
    pub seed: u64,
    pub layout: Layout,
    pub metrics: Metrics,
    pub sequence_pair: [Vec<usize>; 2],
    pub attempts: Vec<Attempt>,
    pub generation_ms: f64,
    pub validation_ms: f64,
}

#[derive(Debug, Serialize)]
pub struct Failure {
    pub seed: u64,
    pub diagnostic: Diagnostic,
    pub attempts: Vec<Attempt>,
}

/// A retained generator borrows validated catalogs; individual calls own only
/// their current packing, occupancy and bounded routing work.
pub struct Generator<'a> {
    catalogs: &'a Catalogs,
    config: Config,
}

impl<'a> Generator<'a> {
    pub fn new(catalogs: &'a Catalogs, config: Config) -> Result<Self, Diagnostic> {
        config.validate(catalogs)?;
        Ok(Self { catalogs, config })
    }

    pub(crate) fn from_validated(catalogs: &'a Catalogs, config: Config) -> Self {
        Self { catalogs, config }
    }

    /// Return a fully routed, independently validated layout or explicit exhaustion.
    /// Retry seeds and all unsuccessful attempt summaries are retained.
    pub fn generate(&self, seed: u64) -> Result<Generated, Failure> {
        self.generate_until(seed, || false)
    }

    /// Generate while checking cancellation between bounded construction units.
    pub fn generate_until(
        &self,
        seed: u64,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Generated, Failure> {
        let started = Instant::now();
        let mut seeds = Rng::new(seed);
        let mut attempts = vec![];
        for ordinal in 0..self.config.max_attempts {
            if cancelled() {
                return Err(interrupted(seed, attempts));
            }
            let attempt_seed = seeds.next_u64();
            let mut rng = Rng::new(attempt_seed);
            let n = self.config.blocks[0]
                + rng.index(self.config.blocks[1] - self.config.blocks[0] + 1);
            let unit = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
            let density = self.config.edges_per_block[0]
                + unit * (self.config.edges_per_block[1] - self.config.edges_per_block[0]);
            let target = (n as f64 * density).ceil() as usize;
            let mut attempt = Attempt {
                ordinal,
                seed: attempt_seed,
                status: "pending".into(),
                blocks: n,
                target_connections: target,
                connected: 0,
                weak_components: n,
                pair_trials: 0,
                route_calls: 0,
                route_expansions: 0,
                long_distance_threshold: 0,
                long_connections: 0,
                routing_statuses: BTreeMap::new(),
                elapsed_ms: 0.0,
            };
            let clock = Instant::now();
            let packing = match packing::sample(self.catalogs, &self.config, n, &mut rng) {
                Ok(p) => p,
                Err(e) => {
                    attempt.status = e.code;
                    attempt.elapsed_ms = clock.elapsed().as_secs_f64() * 1000.0;
                    attempts.push(attempt);
                    continue;
                }
            };
            if cancelled() {
                attempt.status = "interrupted".into();
                attempt.elapsed_ms = clock.elapsed().as_secs_f64() * 1000.0;
                attempts.push(attempt);
                return Err(interrupted(seed, attempts));
            }
            let result = self.connect(&packing, target, &mut rng, &mut attempt, &mut cancelled);
            attempt.elapsed_ms = clock.elapsed().as_secs_f64() * 1000.0;
            match result {
                Ok(Some(layout)) => {
                    if cancelled() {
                        attempt.status = "interrupted".into();
                        attempts.push(attempt);
                        return Err(interrupted(seed, attempts));
                    }
                    let before = Instant::now();
                    let metrics = match validate(&layout, self.catalogs) {
                        Ok(v) => v,
                        Err(diagnostic) => {
                            attempt.status = "validation-error".into();
                            attempts.push(attempt);
                            return Err(Failure {
                                seed,
                                diagnostic,
                                attempts,
                            });
                        }
                    };
                    let validation_ms = before.elapsed().as_secs_f64() * 1000.0;
                    if cancelled() {
                        attempt.status = "interrupted".into();
                        attempts.push(attempt);
                        return Err(interrupted(seed, attempts));
                    }
                    if metrics.branch_blocks < self.config.min_branch_blocks
                        || metrics.merge_blocks < self.config.min_merge_blocks
                    {
                        attempt.status = "shape-target-missed".into();
                        attempts.push(attempt);
                        continue;
                    }
                    attempt.status = "complete".into();
                    attempts.push(attempt);
                    return Ok(Generated {
                        seed,
                        layout,
                        metrics,
                        sequence_pair: [packing.first, packing.second],
                        attempts,
                        generation_ms: started.elapsed().as_secs_f64() * 1000.0 - validation_ms,
                        validation_ms,
                    });
                }
                Ok(None) => attempts.push(attempt),
                Err(diagnostic) => {
                    attempt.status = "routing-error".into();
                    attempts.push(attempt);
                    return Err(Failure {
                        seed,
                        diagnostic,
                        attempts,
                    });
                }
            }
        }
        Err(Failure {
            seed,
            diagnostic: failure(
                "generation-exhausted",
                seed,
                "Requested routed graph was not constructed within the attempt/work limits",
            ),
            attempts,
        })
    }

    fn connect(
        &self,
        packing: &Packing,
        target: usize,
        rng: &mut Rng,
        attempt: &mut Attempt,
        cancelled: &mut dyn FnMut() -> bool,
    ) -> Result<Option<Layout>, Diagnostic> {
        let c = &self.config;
        let n = packing.blocks.len();
        let body: IndexSet<_> = packing
            .poses
            .iter()
            .zip(&packing.sizes)
            .flat_map(|(p, &(w, h))| {
                (p.position.y..p.position.y + h).flat_map(move |y| {
                    (p.position.x..p.position.x + w).map(move |x| Point { x, y })
                })
            })
            .collect();
        let mut reserved: [IndexSet<Point>; 2] = Default::default();
        for ports in &packing.ports {
            for port in ports {
                reserved[layer(port.transport)].insert(port.connection);
            }
        }
        let mut blocked = [body.clone(), body.clone()];
        for t in 0..2 {
            blocked[t].extend(reserved[t].iter().copied());
        }
        let mut occupied: [IndexSet<Point>; 2] = Default::default();
        let mut used: Vec<Vec<bool>> = packing.ports.iter().map(|p| vec![false; p.len()]).collect();
        let mut parent: Vec<_> = (0..n).collect();
        let mut degrees = vec![0; n];
        let mut order: Vec<_> = (0..n).collect();
        rng.shuffle(&mut order);
        let mut rank = vec![0; n];
        for (i, &v) in order.iter().enumerate() {
            rank[v] = i;
        }
        let mut pairs = vec![];
        for s in 0..n {
            for t in 0..n {
                if s != t && (!c.acyclic || rank[s] < rank[t]) {
                    pairs.push((
                        s,
                        t,
                        packing.poses[s]
                            .position
                            .distance(packing.poses[t].position),
                    ));
                }
            }
        }
        let mut distances: Vec<_> = pairs.iter().map(|p| p.2).collect();
        distances.sort_unstable();
        let long_distance = distances[distances.len() * 3 / 4];
        attempt.long_distance_threshold = long_distance;
        let mut eligible = Vec::with_capacity(pairs.len());
        let mut tried = HashSet::new();
        let mut existing = HashSet::new();
        let mut connections: Vec<Connection> = vec![];
        let mut routes: Vec<Route> = vec![];
        let mut bridges: Vec<Bridge> = vec![];
        let mut transport_counts = BTreeMap::new();
        let bounds = Bounds {
            min_x: 0,
            min_y: 0,
            max_x: packing.width - 1,
            max_y: packing.height - 1,
        };
        for _ in 0..c.max_pair_trials {
            if cancelled() {
                attempt.status = "interrupted".into();
                return Ok(None);
            }
            if connections.len() >= target {
                attempt.status = "graph-target-missed".into();
                break;
            }
            let remaining = c
                .route_work_per_attempt
                .saturating_sub(attempt.route_expansions);
            if remaining == 0 {
                attempt.status = "route-work-budget".into();
                break;
            }
            let joining = components(&parent) > 1;
            let required_transport = c
                .min_connections
                .iter()
                .find(|(t, n)| transport_counts.get(*t).copied().unwrap_or(0) < **n)
                .map(|(t, _)| *t);
            let long = rng.index(10_000) < (c.long_link_fraction * 10_000.0) as usize;
            eligible.clear();
            for (i, &(s, t, d)) in pairs.iter().enumerate() {
                if degrees[s] >= c.max_degree
                    || degrees[t] >= c.max_degree
                    || existing.contains(&(s, t))
                    || joining && root(&parent, s) == root(&parent, t)
                    || long && d < long_distance
                    || c.partition_distance_classes && !long && d >= long_distance
                {
                    continue;
                }
                eligible.push(i);
            }
            attempt.pair_trials += 1;
            if eligible.is_empty() {
                continue;
            }
            let (s, t, pair_distance) = pairs[eligible[rng.index(eligible.len())]];
            let mut port_pairs = vec![];
            for (sp, a) in packing.ports[s].iter().enumerate() {
                if used[s][sp]
                    || a.direction != FacilityPortDirection::Output
                    || !c.transports.contains(&a.transport)
                    || required_transport.is_some_and(|kind| a.transport != kind)
                {
                    continue;
                }
                for (tp, b) in packing.ports[t].iter().enumerate() {
                    if !used[t][tp]
                        && b.direction == FacilityPortDirection::Input
                        && a.transport == b.transport
                        && !tried.contains(&(s, sp, t, tp))
                    {
                        port_pairs.push((sp, tp));
                    }
                }
            }
            rng.shuffle(&mut port_pairs);
            for (sp, tp) in port_pairs.into_iter().take(c.ports_per_pair) {
                if cancelled() {
                    attempt.status = "interrupted".into();
                    return Ok(None);
                }
                tried.insert((s, sp, t, tp));
                let (a, b) = (&packing.ports[s][sp], &packing.ports[t][tp]);
                let tr = layer(a.transport);
                if body.contains(&a.connection)
                    || body.contains(&b.connection)
                    || occupied[tr].contains(&a.connection)
                    || occupied[tr].contains(&b.connection)
                {
                    continue;
                }
                let remaining = c
                    .route_work_per_attempt
                    .saturating_sub(attempt.route_expansions);
                if remaining == 0 {
                    break;
                }
                let capacity = self.catalogs.transports.capacity(a.transport);
                let full =
                    Rate::from_quantity_per_duration_ms(capacity.quantity, capacity.duration_ms)
                        .map_err(|e| failure("capacity", "catalog", e.message))?;
                let rate = full
                    .checked_mul_i64(1 + rng.index(4) as i64)
                    .and_then(|r| r.checked_div_i64(4))
                    .map_err(|e| failure("capacity", "catalog", e.message))?;
                let carriers: Vec<_> = routes
                    .iter()
                    .enumerate()
                    .map(|(i, r)| Carrier {
                        id: i,
                        network: i,
                        transport: connections[i].transport,
                        rate: connections[i].rate,
                        cells: &r.cells,
                    })
                    .collect();
                let mut excluded = reserved[tr].clone();
                excluded.extend(bridges.iter().map(|b| b.position));
                let job = RouteJob {
                    id: connections.len(),
                    transport: a.transport,
                    rate,
                };
                let sites = if c.max_bridges_per_route > 0 {
                    crossing_options(&carriers, &job, &self.catalogs.components, &excluded)
                } else {
                    Default::default()
                };
                blocked[tr].shift_remove(&a.connection);
                blocked[tr].shift_remove(&b.connection);
                let result = shortest_path(
                    a.connection,
                    b.connection,
                    &blocked[tr],
                    bounds,
                    PathOptions {
                        first: None,
                        last: None,
                        work_budget: remaining.min(c.route_work_per_call),
                        scan_budget: usize::MAX,
                        frontier_budget: usize::MAX,
                        deadline: None,
                        crossings: &sites,
                        crossing_limit: c.max_bridges_per_route,
                        prefer_crossings: false,
                        allow_zero_length: true,
                        cell_penalties: None,
                        edge_penalties: None,
                    },
                );
                blocked[tr].insert(a.connection);
                blocked[tr].insert(b.connection);
                let result = result?;
                attempt.route_calls += 1;
                attempt.route_expansions += result.expanded;
                *attempt
                    .routing_statuses
                    .entry(result.status.into())
                    .or_default() += 1;
                if cancelled() {
                    attempt.status = "interrupted".into();
                    return Ok(None);
                }
                if result.status != "found" {
                    continue;
                }
                let cells = result.cells.unwrap();
                let id = format!("edge-{}", connections.len());
                for site in result.crossings.unwrap_or_default() {
                    let new_horizontal = cells
                        .windows(3)
                        .any(|window| window[1] == site.position && window[0].y == window[2].y);
                    let new_channel = BridgeChannel {
                        axis: if new_horizontal {
                            BridgeAxis::Horizontal
                        } else {
                            BridgeAxis::Vertical
                        },
                        commodity: format!("flow-{}", connections.len()),
                        connections: vec![id.clone()],
                    };
                    let carrier_channel = BridgeChannel {
                        axis: if new_horizontal {
                            BridgeAxis::Vertical
                        } else {
                            BridgeAxis::Horizontal
                        },
                        commodity: connections[site.carrier_chain].commodity.clone(),
                        connections: vec![connections[site.carrier_chain].id.clone()],
                    };
                    let channels = if new_horizontal {
                        [new_channel, carrier_channel]
                    } else {
                        [carrier_channel, new_channel]
                    };
                    bridges.push(Bridge {
                        id: format!("bridge-{}", bridges.len()),
                        component: site.component,
                        position: site.position,
                        rotation: site.rotation,
                        channels,
                    });
                }
                blocked[tr].extend(cells.iter().copied());
                occupied[tr].extend(cells.iter().copied());
                used[s][sp] = true;
                used[t][tp] = true;
                degrees[s] += 1;
                degrees[t] += 1;
                let (rs, rt) = (root(&parent, s), root(&parent, t));
                parent[rs] = rt;
                existing.insert((s, t));
                *transport_counts.entry(a.transport).or_insert(0) += 1;
                connections.push(Connection {
                    id: id.clone(),
                    source: packing.blocks[s].id.clone(),
                    target: packing.blocks[t].id.clone(),
                    commodity: format!("flow-{}", connections.len()),
                    transport: a.transport,
                    rate,
                });
                routes.push(Route {
                    connection: id,
                    source_port: Some(a.id.clone()),
                    target_port: Some(b.id.clone()),
                    cells,
                });
                attempt.connected = connections.len();
                attempt.long_connections += usize::from(pair_distance >= long_distance);
                attempt.weak_components = components(&parent);
                break;
            }
        }
        // Check after the loop as well: the last permitted trial may complete it.
        if connections.len() == target
            && components(&parent) == 1
            && c.min_connections
                .iter()
                .all(|(t, n)| transport_counts.get(t).copied().unwrap_or(0) >= *n)
        {
            attempt.status = "routed".into();
            return Ok(Some(Layout {
                schema: SCHEMA.into(),
                catalog_sha256: self.catalogs.identity.clone(),
                problem: Problem {
                    blocks: packing.blocks.clone(),
                    terminals: vec![],
                    connections,
                },
                solution: Solution {
                    poses: packing.poses.clone(),
                    terminal_poses: vec![],
                    routes,
                    bridges,
                    junctions: vec![],
                },
            }));
        }
        if attempt.status == "pending" {
            attempt.status = "pair-trials-exhausted".into();
        }
        Ok(None)
    }
}

fn interrupted(seed: u64, attempts: Vec<Attempt>) -> Failure {
    Failure {
        seed,
        diagnostic: failure(
            "generation-interrupted",
            seed,
            "Generation was interrupted before a complete source was admitted",
        ),
        attempts,
    }
}

fn root(parent: &[usize], mut i: usize) -> usize {
    while parent[i] != i {
        i = parent[i];
    }
    i
}
fn components(parent: &[usize]) -> usize {
    (0..parent.len())
        .map(|i| root(parent, i))
        .collect::<BTreeSet<_>>()
        .len()
}
