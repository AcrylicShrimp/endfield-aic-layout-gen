//! Unranked translation and rigid-rotation vocabulary, retaining oracle order.

use aic_data::layouts::{FacilityPlacement, FactoryEditCatalogs, TransportNetworkEndpoint};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::chains::Decomposition;
use super::changes::{self, FacilityChange, FacilityRotation, Move};
use super::groups::{self, Group, Groups, Omitted};
use super::movement::{self, MovementIndex};
use super::spatial::{self, Prefix, Rectangle};
use super::types::*;

pub const LOCAL_STEPS: [(i64, i64); 8] = [
    (1, 0),
    (-1, 0),
    (0, 1),
    (0, -1),
    (2, 0),
    (-2, 0),
    (0, 2),
    (0, -2),
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pose {
    pub member: usize,
    pub x: i64,
    pub y: i64,
    pub rotation: i64,
}
#[derive(Clone, Debug, Serialize)]
pub struct PoseAction {
    pub poses: Vec<Pose>,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub body_area: i64,
    pub source_x: i64,
    pub source_y: i64,
}
pub fn action(w: &Witness, mut poses: Vec<Pose>) -> PoseAction {
    let bodies: Vec<_> = poses
        .iter()
        .map(|q| {
            let p = &w.placements[q.member];
            let swap = (q.rotation - p.rotation).abs() % 180 != 0;
            Rectangle {
                x: q.x,
                y: q.y,
                width: if swap { p.height } else { p.width },
                height: if swap { p.width } else { p.height },
            }
        })
        .collect();
    let b = spatial::envelope(bodies.iter().copied());
    let source_x = poses
        .iter()
        .map(|q| w.placements[q.member].x)
        .min()
        .unwrap();
    let source_y = poses
        .iter()
        .map(|q| w.placements[q.member].y)
        .min()
        .unwrap();
    poses.sort_by_key(|p| p.member);
    PoseAction {
        poses,
        x: b.x,
        y: b.y,
        width: b.width,
        height: b.height,
        body_area: bodies.iter().map(|p| p.width * p.height).sum(),
        source_x,
        source_y,
    }
}
fn fail(code: &str, message: &str) -> Diagnostic {
    let mut e = Diagnostic::new(code, message, Value::Null);
    e.stage = "factory-pose-vocabulary";
    e
}
pub fn options(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    poses: &[Pose],
) -> Result<(
    Vec<Move>,
    Vec<FacilityRotation>,
    IndexMap<String, FacilityChange>,
)> {
    if poses.is_empty()
        || poses.len() > 16
        || poses
            .iter()
            .any(|q| q.member >= w.placements.len() || ![0, 90, 180, 270].contains(&q.rotation))
        || poses
            .iter()
            .map(|q| q.member)
            .collect::<IndexSet<_>>()
            .len()
            != poses.len()
    {
        return Err(fail(
            "factory.pose-parameters",
            "Pose requires 1..16 distinct current placement indices, integer destinations and catalog rotations",
        ));
    }
    let mut vectors = IndexMap::<Point, Move>::new();
    let mut changed = false;
    for q in poses {
        let p = &w.placements[q.member];
        let dx = q.x - p.x;
        let dy = q.y - p.y;
        changed |= dx != 0 || dy != 0 || q.rotation != p.rotation;
        vectors
            .entry(Point { x: dx, y: dy })
            .or_insert_with(|| Move {
                instances: vec![],
                dx,
                dy,
            })
            .instances
            .push(p.instance.clone());
    }
    if !changed || vectors.len() > 4 {
        return Err(fail(
            "factory.pose-parameters",
            "Pose must change geometry within four distinct translation vectors",
        ));
    }
    let moves: Vec<_> = vectors.into_values().collect();
    let rotations: Vec<_> = poses
        .iter()
        .map(|q| FacilityRotation {
            instance: w.placements[q.member].instance.clone(),
            rotation: q.rotation,
        })
        .collect();
    let changes = changes::facility_rotations(w, catalogs, &moves, &rotations)?;
    Ok((moves, rotations, changes))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct VocabularyOptions {
    pub mode: String,
    pub scans_per_group: usize,
    pub candidates_per_group: usize,
    pub pose_domain: String,
    pub compound_scans: usize,
    pub compound_candidates: usize,
    pub compound_preflights: usize,
}
impl Default for VocabularyOptions {
    fn default() -> Self {
        Self {
            mode: "groups".into(),
            scans_per_group: 256,
            candidates_per_group: 32,
            pose_domain: "compound".into(),
            compound_scans: 256,
            compound_candidates: 8,
            compound_preflights: 512,
        }
    }
}
impl VocabularyOptions {
    pub fn validate(&self) -> Result<()> {
        if !["local", "filtered-local", "distant", "groups"].contains(&self.mode.as_str())
            || !(8..=4096).contains(&self.scans_per_group)
            || !(8..=256).contains(&self.candidates_per_group)
        {
            return Err(fail(
                "factory.vocabulary-options",
                "Invalid relocation mode or scan/candidate bounds",
            ));
        }
        if !["basic", "compound"].contains(&self.pose_domain.as_str())
            || !(8..=4096).contains(&self.compound_scans)
            || !(1..=32).contains(&self.compound_candidates)
            || !(1..=4096).contains(&self.compound_preflights)
        {
            return Err(fail(
                "factory.vocabulary-options",
                "Invalid pose domain or compound scan/candidate/preflight bounds",
            ));
        }
        Ok(())
    }
}
struct Preflight {
    bounds: Bounds,
    box_: Bounds,
    bodies: Vec<FacilityPlacement>,
    occupied: Prefix,
    body: Prefix,
    fixed: [IndexSet<Point>; 2],
    moving: [IndexSet<Point>; 2],
    join_vectors: Vec<Point>,
    join_origins: IndexMap<(usize, Point, Point), Point>,
}
impl Preflight {
    fn new(
        w: &Witness,
        group: &Group,
        d: &Decomposition,
        bounds: Bounds,
        index: &MovementIndex,
    ) -> Result<Self> {
        let ids: Vec<_> = group
            .members
            .iter()
            .map(|&i| w.placements[i].instance.clone())
            .collect();
        let selected: IndexSet<_> = ids.iter().cloned().collect();
        let plan = movement::plan(
            w,
            &d.chains,
            &d.anchors,
            &[Move {
                instances: ids,
                dx: 0,
                dy: 0,
            }],
            &IndexMap::new(),
            &IndexMap::new(),
            &IndexSet::new(),
            index,
            None,
        )?;
        let bodies: Vec<_> = w
            .placements
            .iter()
            .filter(|p| selected.contains(&p.instance))
            .cloned()
            .collect();
        let stationary: Vec<_> = w
            .placements
            .iter()
            .filter(|p| !selected.contains(&p.instance))
            .flat_map(body_cells)
            .collect();
        let mut fixed: [IndexSet<Point>; 2] = Default::default();
        let mut moving: [IndexSet<Point>; 2] = Default::default();
        for tr in 0..2 {
            for &p in &d.anchors[tr] {
                if !plan.moved.contains(&(tr, p)) {
                    fixed[tr].insert(p);
                }
            }
        }
        for n in &w.transport_networks {
            for t in &n.terminals {
                if matches!(&t.endpoint, TransportNetworkEndpoint::Facility { instance, .. } if selected.contains(instance))
                {
                    moving[layer(n.transport)].insert(Point::from(&t.position));
                }
            }
        }
        for c in &w.logistics_components {
            if plan.components.contains(&c.id) {
                moving[layer(c.transport)].insert(Point::from(&c.position));
            }
        }
        for c in &d.chains {
            if let Some(path) = plan.fixed_chains.get(&c.id) {
                moving[layer(c.transport)].extend(path.iter().copied());
            }
        }
        let mut vectors = IndexSet::new();
        let mut origins = IndexMap::new();
        for &id in &index.joinable {
            let c = &d.chains[id];
            let a = &c.start_terminals[0];
            let b = &c.end_terminals[0];
            let own = |t: &aic_data::layouts::TransportNetworkTerminal| matches!(&t.endpoint, TransportNetworkEndpoint::Facility { instance, .. } if selected.contains(instance));
            if own(a) == own(b) {
                continue;
            }
            let (own, other) = if own(a) { (a, b) } else { (b, a) };
            let p = Point::from(&own.position);
            let q = Point::from(&other.position);
            let delta = Point {
                x: q.x - p.x,
                y: q.y - p.y,
            };
            vectors.insert(delta);
            origins.insert((layer(c.transport), p, delta), q);
        }
        let box_ = spatial::envelope(
            bodies
                .iter()
                .map(|p| Rectangle {
                    x: p.x,
                    y: p.y,
                    width: p.width,
                    height: p.height,
                })
                .chain(moving.iter().flatten().map(|p| Rectangle {
                    x: p.x,
                    y: p.y,
                    width: 1,
                    height: 1,
                })),
        );
        let occupied = Prefix::new(
            bounds,
            stationary
                .iter()
                .copied()
                .chain(fixed.iter().flatten().copied()),
        );
        let body = Prefix::new(bounds, stationary);
        Ok(Self {
            bounds,
            box_: Bounds {
                min_x: box_.x,
                min_y: box_.y,
                max_x: box_.x + box_.width - 1,
                max_y: box_.y + box_.height - 1,
            },
            bodies,
            occupied,
            body,
            fixed,
            moving,
            join_vectors: vectors.into_iter().collect(),
            join_origins: origins,
        })
    }
    fn check(&self, delta: Point) -> Option<&'static str> {
        let (dx, dy) = (delta.x, delta.y);
        let b = self.box_;
        if b.min_x + dx < self.bounds.min_x
            || b.max_x + dx > self.bounds.max_x
            || b.min_y + dy < self.bounds.min_y
            || b.max_y + dy > self.bounds.max_y
        {
            return Some("bounds");
        }
        if self.bodies.iter().any(|p| {
            self.occupied
                .occupied(p.x + dx, p.y + dy, p.width, p.height)
        }) {
            return Some("body_or_anchor");
        }
        for tr in 0..2 {
            for &p in &self.moving[tr] {
                let q = p.shifted(dx, dy);
                if self.body.occupied(q.x, q.y, 1, 1)
                    || (self.fixed[tr].contains(&q)
                        && self.join_origins.get(&(tr, p, delta)) != Some(&q))
                {
                    return Some("carried_or_terminal");
                }
            }
        }
        None
    }
}

fn reject(counts: &mut serde_json::Map<String, Value>, code: &str) {
    let count = counts.get(code).and_then(Value::as_u64).unwrap_or_default() + 1;
    counts.insert(code.into(), count.into());
}
fn translations(
    w: &Witness,
    opts: &VocabularyOptions,
    d: &Decomposition,
    bounds: Bounds,
    index: &MovementIndex,
) -> (Vec<PoseAction>, Value) {
    let generated = if opts.mode == "local" {
        Groups {
            groups: (0..w.placements.len())
                .map(|i| Group {
                    members: vec![i],
                    source: "singleton".into(),
                })
                .collect(),
            omitted: vec![],
        }
    } else {
        groups::pose_groups(w, opts.mode == "groups")
    };
    let group_count = generated.groups.len().min(512);
    let mut diagnostics = generated.omitted;
    let mut actions = Vec::new();
    let mut rejected = serde_json::Map::new();
    let mut examined = 0;
    let mut capped = 0;
    let mut join_examined = 0;
    for group in generated.groups.iter().take(512) {
        let preflight = if opts.mode == "local" {
            None
        } else {
            match Preflight::new(w, group, d, bounds, index) {
                Ok(p) => Some(p),
                Err(e) => {
                    diagnostics.push(Omitted {
                        members: group.members.clone(),
                        source: group.source.clone(),
                        code: e.code,
                    });
                    continue;
                }
            }
        };
        let mut retained = 0;
        let mut seen = IndexSet::new();
        let mut examine = |delta: Point, retained: &mut usize| {
            if seen.contains(&delta) || delta == Point::default() {
                return;
            }
            seen.insert(delta);
            examined += 1;
            if let Some(code) = preflight.as_ref().and_then(|p| p.check(delta)) {
                reject(&mut rejected, code);
                return;
            }
            actions.push(action(
                w,
                group
                    .members
                    .iter()
                    .map(|&member| {
                        let p = &w.placements[member];
                        Pose {
                            member,
                            x: p.x + delta.x,
                            y: p.y + delta.y,
                            rotation: p.rotation,
                        }
                    })
                    .collect(),
            ));
            *retained += 1;
        };
        for (dx, dy) in LOCAL_STEPS {
            examine(Point { x: dx, y: dy }, &mut retained);
        }
        if opts.mode != "distant" && opts.mode != "groups" {
            continue;
        }
        let preflight = preflight.as_ref().unwrap();
        for &delta in &preflight.join_vectors {
            if retained >= opts.candidates_per_group {
                break;
            }
            join_examined += 1;
            examine(delta, &mut retained);
        }
        let b = preflight.box_;
        let lo_x = bounds.min_x - b.min_x;
        let lo_y = bounds.min_y - b.min_y;
        let nx = bounds.max_x - b.max_x - lo_x + 1;
        let ny = bounds.max_y - b.max_y - lo_y + 1;
        let total = nx * ny;
        if total <= 0 {
            continue;
        }
        let stride = spatial::stride(total);
        let mut i = 0;
        while i < total && i < opts.scans_per_group as i64 && retained < opts.candidates_per_group {
            let position = (i * stride) % total;
            examine(
                Point {
                    x: lo_x + position % nx,
                    y: lo_y + position / nx,
                },
                &mut retained,
            );
            i += 1;
        }
        if i < total {
            capped += 1;
        }
    }
    let omitted: Vec<_> = diagnostics.iter().map(|d| json!({"members": d.members, "source": d.source, "code": d.code, "severity": "info", "stage": "factory-pose-vocabulary",
        "message": "Group omitted from the bounded retained-terminal relocation vocabulary", "entity": {"members": d.members, "source": d.source}})).collect();
    let report = json!({"mode": opts.mode, "groups": group_count, "groups_generated": generated.groups.len(), "groups_budget_omitted": generated.groups.len() - group_count,
        "omitted": omitted, "examined": examined, "static_rejected": rejected, "capped_groups": capped, "join_vectors_examined": join_examined,
        "actions": actions.len(), "scans_per_group": opts.scans_per_group, "candidates_per_group": opts.candidates_per_group});
    (actions, report)
}

fn rigid(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    group: &Group,
    turns: i64,
    origin: Point,
) -> Result<Vec<FacilityPlacement>> {
    let ps: Vec<_> = group.members.iter().map(|&i| &w.placements[i]).collect();
    let old = spatial::envelope(ps.iter().map(|p| Rectangle {
        x: p.x,
        y: p.y,
        width: p.width,
        height: p.height,
    }));
    ps.into_iter()
        .map(|p| {
            let x = p.x - old.x;
            let y = p.y - old.y;
            let (rx, ry, width, height) = match turns {
                1 => (old.height - y - p.height, x, p.height, p.width),
                2 => (
                    old.width - x - p.width,
                    old.height - y - p.height,
                    p.width,
                    p.height,
                ),
                _ => (y, old.width - x - p.width, p.height, p.width),
            };
            let rotation = (p.rotation + turns * 90) % 360;
            if !catalogs
                .facilities
                .facility(&p.facility)
                .is_some_and(|s| s.allowed_rotations.contains(&rotation))
            {
                let mut e = fail(
                    "block-rotation-catalog",
                    "A group member does not allow the proposed catalog rotation",
                );
                e.entity = p.instance.clone().into();
                return Err(e);
            }
            let mut p = p.clone();
            p.x = origin.x + rx;
            p.y = origin.y + ry;
            p.width = width;
            p.height = height;
            p.rotation = rotation;
            Ok(p)
        })
        .collect()
}
fn origins(box_: Rectangle, bounds: Bounds, voids: &[Rectangle], limit: usize) -> Vec<Point> {
    let mut seen = IndexSet::new();
    let mut offer = |x, y| {
        if seen.len() < limit {
            seen.insert(Point { x, y });
        }
    };
    offer(box_.x, box_.y);
    for (dx, dy) in LOCAL_STEPS {
        offer(box_.x + dx, box_.y + dy);
    }
    for r in voids {
        if r.width >= box_.width && r.height >= box_.height {
            for y in [r.y, r.y + 1, r.y + r.height - box_.height] {
                for x in [r.x, r.x + 1, r.x + r.width - box_.width] {
                    offer(x, y);
                }
            }
        }
    }
    let nx = bounds.max_x - bounds.min_x - box_.width + 2;
    let ny = bounds.max_y - bounds.min_y - box_.height + 2;
    let total = nx * ny;
    if nx > 0 && ny > 0 {
        let stride = spatial::stride(total);
        for i in 0..total {
            if seen.len() >= limit {
                break;
            }
            let p = (i * stride) % total;
            seen.insert(Point {
                x: bounds.min_x + p % nx,
                y: bounds.min_y + p / nx,
            });
        }
    }
    seen.into_iter().collect()
}

pub fn actions(
    w: &Witness,
    catalogs: &FactoryEditCatalogs,
    opts: &VocabularyOptions,
    d: &Decomposition,
    index: &MovementIndex,
) -> Result<(Vec<PoseAction>, Value)> {
    opts.validate()?;
    let bounds = extent(w)?;
    if i128::from(bounds.width()) * i128::from(bounds.height()) > 200000 {
        return Err(fail(
            "factory.vocabulary-area-budget",
            "Current observation/enumeration area exceeds the explicit 200000-cell guard",
        ));
    }
    let (mut actions, translation) = translations(w, opts, d, bounds, index);
    let mut rejected = serde_json::Map::new();
    let preflight = |poses: &[Pose], rejected: &mut serde_json::Map<String, Value>| -> bool {
        let attempt = || -> Result<bool> {
            let (moves, _, changes) = options(w, catalogs, poses)?;
            let p = movement::plan(
                w,
                &d.chains,
                &d.anchors,
                &moves,
                &IndexMap::new(),
                &changes,
                &IndexSet::new(),
                index,
                None,
            )?;
            Ok(!w.transport_networks.iter().any(|n| {
                n.terminals
                    .iter()
                    .any(|t| !bounds.contains(p.shift_terminal(t)))
            }) && !w.logistics_components.iter().any(|c| {
                !bounds.contains(p.shift_anchor(layer(c.transport), Point::from(&c.position)))
            }) && !p
                .fixed_chains
                .values()
                .flatten()
                .any(|&p| !bounds.contains(p)))
        };
        match attempt() {
            Ok(true) => true,
            Ok(false) => {
                reject(rejected, "factory.pose-carried-bounds");
                false
            }
            Err(e) => {
                reject(rejected, &e.code);
                false
            }
        }
    };
    let mut single_examined = 0;
    let mut single_retained = 0;
    for (member, p) in w.placements.iter().enumerate() {
        let stationary = Prefix::new(
            bounds,
            w.placements
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != member)
                .flat_map(|(_, p)| body_cells(p)),
        );
        let spec = catalogs.facilities.facility(&p.facility).ok_or_else(|| {
            fail(
                "factory-unknown-facility",
                "Facility is absent from the runtime catalog",
            )
        })?;
        for &rotation in &spec.allowed_rotations {
            if rotation == p.rotation {
                continue;
            }
            single_examined += 1;
            let a = action(
                w,
                vec![Pose {
                    member,
                    x: p.x,
                    y: p.y,
                    rotation,
                }],
            );
            if stationary.occupied(a.x, a.y, a.width, a.height) {
                reject(&mut rejected, "factory.pose-body-or-bounds");
                continue;
            }
            if preflight(&a.poses, &mut rejected) {
                actions.push(a);
                single_retained += 1;
            }
        }
    }
    struct Stream {
        group: Group,
        stationary: Prefix,
        template: Vec<FacilityPlacement>,
        origin: Point,
        origins: Vec<Point>,
        examined: usize,
        retained: usize,
        done: bool,
    }
    let mut streams = Vec::new();
    let mut group_count = 0;
    let mut group_omitted = 0;
    let mut examined = 0;
    let mut preflights = 0;
    let mut retained = 0;
    if opts.pose_domain == "compound" {
        let all = groups::compound_groups(w);
        group_count = all.len().min(128);
        group_omitted = all.len() - group_count;
        let voids = spatial::empty_rectangles(w, bounds);
        for group in all.into_iter().take(128) {
            let origin = Point {
                x: group
                    .members
                    .iter()
                    .map(|&i| w.placements[i].x)
                    .min()
                    .unwrap(),
                y: group
                    .members
                    .iter()
                    .map(|&i| w.placements[i].y)
                    .min()
                    .unwrap(),
            };
            for turns in [1, 2, 3] {
                match rigid(w, catalogs, &group, turns, origin) {
                    Err(e) => reject(&mut rejected, &e.code),
                    Ok(template) => {
                        let b = spatial::envelope(template.iter().map(|p| Rectangle {
                            x: p.x,
                            y: p.y,
                            width: p.width,
                            height: p.height,
                        }));
                        streams.push(Stream {
                            stationary: Prefix::new(
                                bounds,
                                w.placements
                                    .iter()
                                    .enumerate()
                                    .filter(|(i, _)| !group.members.contains(i))
                                    .flat_map(|(_, p)| body_cells(p)),
                            ),
                            group: group.clone(),
                            template,
                            origin,
                            origins: origins(b, bounds, &voids, opts.compound_scans),
                            examined: 0,
                            retained: 0,
                            done: false,
                        });
                    }
                }
            }
        }
        let mut active = true;
        while active && preflights < opts.compound_preflights {
            active = false;
            for s in &mut streams {
                if s.done || s.retained >= opts.compound_candidates {
                    continue;
                }
                active = true;
                loop {
                    let Some(&next) = s.origins.get(s.examined) else {
                        s.done = true;
                        break;
                    };
                    s.examined += 1;
                    examined += 1;
                    let dx = next.x - s.origin.x;
                    let dy = next.y - s.origin.y;
                    if s.template
                        .iter()
                        .any(|p| s.stationary.occupied(p.x + dx, p.y + dy, p.width, p.height))
                    {
                        reject(&mut rejected, "factory.pose-body-or-bounds");
                        continue;
                    }
                    let poses: Vec<_> = s
                        .template
                        .iter()
                        .enumerate()
                        .map(|(i, p)| Pose {
                            member: s.group.members[i],
                            x: p.x + dx,
                            y: p.y + dy,
                            rotation: p.rotation,
                        })
                        .collect();
                    preflights += 1;
                    if preflight(&poses, &mut rejected) {
                        actions.push(action(w, poses));
                        retained += 1;
                        s.retained += 1;
                    }
                    break;
                }
                if preflights >= opts.compound_preflights {
                    break;
                }
            }
        }
    }
    let capped = streams
        .iter()
        .filter(|s| !s.done || s.examined >= opts.compound_scans)
        .count();
    let count = actions.len();
    let mut seen = IndexSet::new();
    actions.retain(|a| seen.insert(a.poses.clone()));
    let report = json!({"pose_domain": opts.pose_domain, "single_rotations_examined": single_examined, "single_rotations": single_retained, "compound_groups": group_count,
        "compound_groups_omitted": group_omitted, "compound_examined": examined, "compound_preflights": preflights, "compound_actions": retained,
        "compound_streams": streams.len(), "compound_streams_capped": capped, "compound_preflight_limit": opts.compound_preflights,
        "compound_scans": opts.compound_scans, "compound_candidates": opts.compound_candidates, "static_rejected": rejected,
        "translation": translation, "duplicates": count - actions.len(), "actions": actions.len()});
    Ok((actions, report))
}
