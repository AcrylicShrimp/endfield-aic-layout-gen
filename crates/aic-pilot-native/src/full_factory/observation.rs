//! Stable float32 policy inputs; geometry never crosses the learning boundary.

use aic_data::facilities::{FacilityPortDirection, FacilityPortEdge};
use aic_data::layouts::TransportNetworkEndpoint;
use aic_data::logistics::{LogisticsComponentKind, TransportKind};
use aic_data::recipes::Rate;
use indexmap::IndexMap;
use serde::Serialize;

use super::candidates::{Action, Domain};
use super::catalog::Port;
use super::strips::Axis;
use super::types::*;

pub const SCHEMA: &str = "factory-input-contract-screen-v1";
pub const KINDS: [&str; 8] = [
    "pose",
    "port",
    "reroute",
    "component",
    "release_bridge",
    "insert",
    "contract",
    "restore_best",
];

#[derive(Clone, Serialize)]
pub struct World {
    pub width: usize,
    pub height: usize,
    pub raster: Vec<f32>,
    pub nodes: Vec<[f32; 32]>,
    pub node_xy: Vec<[i64; 2]>,
    pub edges: Vec<[i64; 2]>,
    pub edge_values: Vec<[f32; 16]>,
    pub messages: bool,
}
#[derive(Default)]
struct References {
    facilities: IndexMap<String, usize>,
    networks: Vec<usize>,
    cells: IndexMap<(usize, Point), usize>,
    terminals: IndexMap<String, usize>,
    components: IndexMap<String, usize>,
    ports: IndexMap<(String, String), usize>,
}
pub struct WorldInput {
    pub world: World,
    refs: References,
}

fn ratio(n: i64, d: i64) -> f32 {
    (n as f64 / d as f64) as f32
}
fn rate(r: Rate) -> [f32; 3] {
    [
        ratio(r.numerator, r.denominator),
        (r.numerator as f64).ln_1p() as f32,
        (r.denominator as f64).ln_1p() as f32,
    ]
}
fn angle(degrees: i64) -> (f32, f32) {
    let a = degrees as f64 * (std::f64::consts::PI / 180.0);
    (a.sin() as f32, a.cos() as f32)
}
fn clipped(p: Point, width: usize, height: usize) -> [i64; 2] {
    [
        p.x.clamp(0, width as i64 - 1),
        p.y.clamp(0, height as i64 - 1),
    ]
}
impl World {
    fn node(&mut self, kind: usize, p: Point, transport: Option<TransportKind>) -> usize {
        let mut f = [0.; 32];
        f[kind] = 1.;
        f[7] = ratio(p.x, (self.width as i64 - 1).max(1));
        f[8] = ratio(p.y, (self.height as i64 - 1).max(1));
        if let Some(t) = transport {
            f[11 + layer(t)] = 1.;
        }
        let i = self.nodes.len();
        self.nodes.push(f);
        self.node_xy.push(clipped(p, self.width, self.height));
        i
    }
    fn edge(&mut self, a: usize, b: usize, kind: usize, r: Option<Rate>, vector: Option<Point>) {
        let mut f = [0.; 16];
        f[kind] = 1.;
        if let Some(r) = r {
            f[11..14].copy_from_slice(&rate(r));
        }
        if let Some(p) = vector {
            f[14] = p.x as f32;
            f[15] = p.y as f32;
        }
        self.edges.push([a as i64, b as i64]);
        self.edge_values.push(f);
    }
    fn pair(&mut self, a: usize, b: usize, kind: usize) {
        self.edge(a, b, kind, None, None);
        self.edge(b, a, kind + 1, None, None);
    }
    fn paint(&mut self, channel: usize, p: Point, value: f32) {
        self.raster[(channel * self.height + p.y as usize) * self.width + p.x as usize] = value;
    }
    pub fn geometry(&self) -> Self {
        let mut value = self.clone();
        value.messages = false;
        value.raster[3 * self.width * self.height..19 * self.width * self.height].fill(0.);
        for n in &mut value.nodes {
            n[17..24].fill(0.);
            n[31] = 0.;
        }
        value.edges.clear();
        value.edge_values.clear();
        value
    }
}

pub fn world(w: &Witness, ports: &IndexMap<String, Vec<Port>>) -> Result<WorldInput> {
    let bounds = extent(w)?;
    if bounds.min_x != 0 || bounds.min_y != 0 {
        return Err(Diagnostic::new(
            "factory.observation-origin",
            "Accepted geometry must have canonical origin",
            serde_json::Value::Null,
        ));
    }
    let (width, height) = (bounds.width() as usize, bounds.height() as usize);
    let mut v = World {
        width,
        height,
        raster: vec![0.; 28 * width * height],
        nodes: vec![],
        node_xy: vec![],
        edges: vec![],
        edge_values: vec![],
        messages: true,
    };
    let mut refs = References::default();
    for y in 0..height {
        for x in 0..width {
            let p = Point {
                x: x as i64,
                y: y as i64,
            };
            v.paint(25, p, ratio(p.x, (width as i64 - 1).max(1)));
            v.paint(26, p, ratio(p.y, (height as i64 - 1).max(1)));
            v.paint(27, p, 1.);
        }
    }
    for p in &w.placements {
        let i = v.node(0, Point { x: p.x, y: p.y }, None);
        refs.facilities.insert(p.instance.clone(), i);
        v.nodes[i][9..11]
            .copy_from_slice(&[ratio(p.width, width as i64), ratio(p.height, height as i64)]);
        let (sin, cos) = angle(p.rotation);
        v.nodes[i][15..17].copy_from_slice(&[sin, cos]);
        v.nodes[i][17..20].copy_from_slice(&rate(w.utilizations[&p.instance]));
        for cell in body_cells(p) {
            v.paint(0, cell, 1.);
        }
    }
    for (ni, n) in w.transport_networks.iter().enumerate() {
        let l = layer(n.transport);
        let i = v.node(1, Point::default(), Some(n.transport));
        refs.networks.push(i);
        v.nodes[i][20..22].copy_from_slice(&[
            (n.cells.len() as f64).ln_1p() as f32,
            (n.terminals.len() as f64).ln_1p() as f32,
        ]);
        for p in &n.cells {
            let p = Point::from(p);
            let c = v.node(2, p, Some(n.transport));
            refs.cells.insert((ni, p), c);
            v.pair(c, i, 2);
            v.paint(1 + l, p, 1.);
        }
        for s in &n.segments {
            let (a, b) = (Point::from(&s.from), Point::from(&s.to));
            let (ca, cb) = (refs.cells[&(ni, a)], refs.cells[&(ni, b)]);
            let delta = Point {
                x: b.x - a.x,
                y: b.y - a.y,
            };
            let d = match (delta.x, delta.y) {
                (1, 0) => 0,
                (-1, 0) => 1,
                (0, 1) => 2,
                (0, -1) => 3,
                _ => {
                    return Err(Diagnostic::new(
                        "factory.observation-segment",
                        "Segment is not a unit grid edge",
                        n.id.clone(),
                    ));
                }
            };
            v.edge(ca, cb, 0, Some(s.rate), Some(delta));
            v.edge(
                cb,
                ca,
                1,
                Some(s.rate),
                Some(Point {
                    x: -delta.x,
                    y: -delta.y,
                }),
            );
            v.nodes[ca][20] += 1.;
            v.nodes[cb][21] += 1.;
            v.nodes[ca][22] += ratio(s.rate.numerator, s.rate.denominator);
            v.nodes[cb][23] += ratio(s.rate.numerator, s.rate.denominator);
            v.paint(3 + l * 8 + d, a, 1.);
            v.paint(7 + l * 8 + d, b, 1.);
        }
        for t in &n.terminals {
            let p = Point::from(&t.position);
            let ti = v.node(
                if matches!(&t.endpoint, TransportNetworkEndpoint::Facility { .. }) {
                    3
                } else {
                    6
                },
                p,
                Some(n.transport),
            );
            refs.terminals.insert(t.id.clone(), ti);
            v.nodes[ti][13 + usize::from(t.direction == FacilityPortDirection::Output)] = 1.;
            v.nodes[ti][17..20].copy_from_slice(&rate(t.rate));
            v.pair(ti, refs.cells[&(ni, p)], 4);
            if let TransportNetworkEndpoint::Facility { instance, .. } = &t.endpoint {
                v.pair(ti, refs.facilities[instance], 6);
            }
        }
    }
    for c in &w.logistics_components {
        let p = Point::from(&c.position);
        let i = v.node(4, p, Some(c.transport));
        refs.components.insert(c.id.clone(), i);
        let kind = match c.kind {
            LogisticsComponentKind::Splitter => 0,
            LogisticsComponentKind::Converger => 1,
            LogisticsComponentKind::Bridge => 2,
        };
        v.nodes[i][24 + kind] = 1.;
        let (sin, cos) = angle(c.rotation);
        v.nodes[i][15..17].copy_from_slice(&[sin, cos]);
        v.paint(19 + 3 * layer(c.transport) + kind, p, 1.);
        for (ni, n) in w.transport_networks.iter().enumerate() {
            if n.component_ids.contains(&c.id) {
                v.pair(i, refs.cells[&(ni, p)], 8);
            }
        }
    }
    for (instance, ps) in ports {
        for p in ps {
            let i = v.node(5, p.connection, Some(p.transport));
            refs.ports.insert((instance.clone(), p.id.clone()), i);
            v.nodes[i][13 + usize::from(p.direction == FacilityPortDirection::Output)] = 1.;
            let edge = match p.edge {
                FacilityPortEdge::East => 0,
                FacilityPortEdge::West => 1,
                FacilityPortEdge::South => 2,
                FacilityPortEdge::North => 3,
            };
            v.nodes[i][27 + edge] = 1.;
            v.pair(i, refs.facilities[instance], 6);
        }
    }
    for n in &w.transport_networks {
        for t in &n.terminals {
            if let TransportNetworkEndpoint::Facility { instance, port } = &t.endpoint {
                let p = refs.ports[&(instance.clone(), port.clone())];
                v.nodes[p][31] = 1.;
                v.pair(refs.terminals[&t.id], p, 8);
            }
        }
    }
    for (ai, a) in w.transport_networks.iter().enumerate() {
        for (bi, b) in w.transport_networks[..ai].iter().enumerate() {
            if a.item == b.item && a.transport == b.transport {
                v.edge(refs.networks[ai], refs.networks[bi], 10, None, None);
                v.edge(refs.networks[bi], refs.networks[ai], 10, None, None);
            }
        }
    }
    Ok(WorldInput { world: v, refs })
}

#[derive(Clone, Default, Serialize)]
pub struct Candidates {
    pub actions: Vec<[f32; 32]>,
    pub operands: Vec<[i64; 2]>,
    pub action_xy: Vec<[i64; 2]>,
    pub member_nodes: Vec<i64>,
    pub member_actions: Vec<i64>,
    pub member_xy: Vec<[i64; 2]>,
    pub member_pose: Vec<[f32; 10]>,
}
impl Candidates {
    pub fn token(&self, index: usize) -> [f32; 42] {
        let mut result = [0.; 42];
        result[..32].copy_from_slice(&self.actions[index]);
        let mut count = 0;
        for (&ai, pose) in self.member_actions.iter().zip(&self.member_pose) {
            if ai == index as i64 {
                count += 1;
                for j in 0..10 {
                    result[32 + j] += pose[j];
                }
            }
        }
        if count > 0 {
            for x in &mut result[32..] {
                *x /= count as f32;
            }
        }
        result
    }
}
pub fn candidates(w: &Witness, domain: &Domain, world: &WorldInput) -> Candidates {
    let mut v = Candidates::default();
    let (width, height) = (world.world.width as i64, world.world.height as i64);
    let (sx, sy) = ((width - 1).max(1), (height - 1).max(1));
    let refs = &world.refs;
    for (ai, a) in domain.actions.iter().enumerate() {
        let mut f = [0.; 32];
        f[KINDS.iter().position(|k| *k == a.kind()).unwrap()] = 1.;
        let p = a.point();
        f[9] = ratio(p.x, sx);
        f[10] = ratio(p.y, sy);
        f[14] = 1.;
        let (mut op, mut secondary) = (-1, -1);
        match a {
            Action::Pose(a) => {
                f[24..30].copy_from_slice(&[
                    (a.poses.len() as f64).ln_1p() as f32,
                    ratio(a.width, width),
                    ratio(a.height, height),
                    ratio(a.body_area, width * height),
                    ratio(a.source_x, sx),
                    ratio(a.source_y, sy),
                ]);
                for q in &a.poses {
                    let p = &w.placements[q.member];
                    let (sin, cos) = angle(q.rotation);
                    let (dsin, dcos) = angle(q.rotation - p.rotation);
                    let (pw, ph) = if (q.rotation - p.rotation).abs() % 180 != 0 {
                        (p.height, p.width)
                    } else {
                        (p.width, p.height)
                    };
                    v.member_nodes.push(refs.facilities[&p.instance] as i64);
                    v.member_actions.push(ai as i64);
                    v.member_xy.push(clipped(
                        Point {
                            x: q.x + (pw - 1) / 2,
                            y: q.y + (ph - 1) / 2,
                        },
                        width as usize,
                        height as usize,
                    ));
                    v.member_pose.push([
                        ratio(q.x, sx),
                        ratio(q.y, sy),
                        ratio(q.x - p.x, sx),
                        ratio(q.y - p.y, sy),
                        ratio(pw, width),
                        ratio(ph, height),
                        sin,
                        cos,
                        dsin,
                        dcos,
                    ]);
                }
            }
            Action::Port {
                terminal,
                instance,
                port,
                transport,
                ..
            } => {
                op = refs.terminals[terminal] as i64;
                secondary = refs.ports[&(instance.clone(), port.clone())] as i64;
                f[15 + layer(*transport)] = 1.;
            }
            Action::Reroute {
                chain, transport, ..
            } => {
                let c = &domain.decomposition.chains[*chain];
                op = refs.cells[&(c.network, c.start())] as i64;
                secondary = refs.cells[&(c.network, c.end())] as i64;
                f[15 + layer(*transport)] = 1.;
                f[20..23].copy_from_slice(&rate(c.rate));
                f[23] = (c.cells.len() as f64).ln_1p() as f32;
            }
            Action::Component {
                component,
                dx,
                dy,
                rotation,
                transport,
                ..
            } => {
                op = refs.components[component] as i64;
                f[11] = ratio(*dx, sx);
                f[12] = ratio(*dy, sy);
                let (sin, cos) = angle(*rotation);
                f[13] = sin;
                f[14] = cos;
                f[15 + layer(*transport)] = 1.;
            }
            Action::ReleaseBridge {
                component,
                transport,
                ..
            } => {
                op = refs.components[component] as i64;
                f[15 + layer(*transport)] = 1.;
            }
            Action::Insert { axis, position, .. } | Action::Contract { axis, position, .. } => {
                let y = matches!(axis, Axis::Y);
                f[17 + usize::from(y)] = 1.;
                f[19] = ratio(*position, if y { sy } else { sx });
            }
            Action::RestoreBest { .. } => {}
        }
        v.actions.push(f);
        v.operands.push([op + 1, secondary + 1]);
        v.action_xy
            .push(clipped(p, width as usize, height as usize));
    }
    v
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Current {
    None,
    Geometry,
    Full,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Best {
    None,
    Scores2,
    Scores3,
    Layout,
}
#[derive(Clone, Copy)]
pub struct Contract {
    pub current: Current,
    pub history: usize,
    pub outcomes: bool,
    pub best: Best,
    pub restore: bool,
}
impl Contract {
    pub fn named(name: &str) -> Result<Self> {
        let mut c = Self {
            current: Current::Full,
            history: 0,
            outcomes: false,
            best: Best::None,
            restore: true,
        };
        match name {
            "action-only" => c.current = Current::None,
            "geometry-current" => c.current = Current::Geometry,
            "full-current" => {}
            "current-history-4" => c.history = 4,
            "current-history-16" => c.history = 16,
            "current-history-16-outcomes" => {
                c.history = 16;
                c.outcomes = true;
            }
            "current-best-2" => c.best = Best::Scores2,
            "current-best-3" => c.best = Best::Scores3,
            "current-best-layout" => c.best = Best::Layout,
            "current-best-layout-history" => {
                c.best = Best::Layout;
                c.history = 16;
                c.outcomes = true;
            }
            "current-no-restore" => c.restore = false,
            "history-no-restore" => {
                c.restore = false;
                c.history = 16;
                c.outcomes = true;
            }
            _ => {
                return Err(Diagnostic::new(
                    "factory.input-contract",
                    "Unknown input contract",
                    name,
                ));
            }
        }
        Ok(c)
    }
}
#[derive(Clone)]
pub struct History {
    pub action: [f32; 42],
    pub outcome: [f32; 10],
}
pub fn history(records: &[History], contract: Contract) -> Vec<[f32; 53]> {
    let selected = &records[records.len().saturating_sub(contract.history)..];
    selected
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut row = [0.; 53];
            row[..42].copy_from_slice(&r.action);
            row[42] = (selected.len() - i) as f32 / contract.history as f32;
            if contract.outcomes {
                row[43..].copy_from_slice(&r.outcome);
            }
            row
        })
        .collect()
}
pub fn context(
    width: usize,
    height: usize,
    facilities: usize,
    remaining: f64,
    initial: [i64; 3],
    current: [i64; 3],
    best: [i64; 3],
    contract: Contract,
) -> [f32; 9] {
    let mut result = [
        remaining as f32,
        (width as f64).ln_1p() as f32,
        (height as f64).ln_1p() as f32,
        (facilities as f64).ln_1p() as f32,
        0.,
        0.,
        0.,
        0.,
        0.,
    ];
    if contract.current != Current::None {
        for i in 0..2 {
            result[4 + i] = ratio(current[i], initial[i].max(1));
        }
    }
    if contract.best != Best::None {
        for i in 0..2 {
            result[6 + i] = ratio(best[i], initial[i].max(1));
        }
    }
    if matches!(contract.best, Best::Scores3 | Best::Layout) {
        result[8] = ratio(best[2], initial[2].max(1));
    }
    result
}
