use aic_data::layouts::{TransportNetworkEndpoint, TransportNetworkSegment};
use indexmap::{IndexMap, IndexSet};
use serde::{Deserialize, Serialize};

use super::types::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
}
impl Axis {
    pub fn coordinate(self, p: Point) -> i64 {
        match self {
            Self::X => p.x,
            Self::Y => p.y,
        }
    }
    fn cross(self, p: Point) -> i64 {
        match self {
            Self::X => p.y,
            Self::Y => p.x,
        }
    }
    fn start(self, p: &aic_data::layouts::FacilityPlacement) -> i64 {
        match self {
            Self::X => p.x,
            Self::Y => p.y,
        }
    }
    fn size(self, p: &aic_data::layouts::FacilityPlacement) -> i64 {
        match self {
            Self::X => p.width,
            Self::Y => p.height,
        }
    }
}
pub struct Strips {
    pub remove: Vec<i64>,
    pub insert: Vec<i64>,
}
pub fn candidates(w: &Witness, axis: Axis) -> Result<Strips> {
    let mut critical = IndexSet::new();
    for p in &w.placements {
        critical.extend(axis.start(p)..axis.start(p) + axis.size(p));
    }
    for c in &w.logistics_components {
        critical.insert(axis.coordinate(Point::from(&c.position)));
    }
    for n in &w.transport_networks {
        let mut ins = IndexMap::<Point, Vec<&TransportNetworkSegment>>::new();
        let mut outs = IndexMap::<Point, Vec<&TransportNetworkSegment>>::new();
        for s in &n.segments {
            ins.entry(Point::from(&s.to)).or_default().push(s);
            outs.entry(Point::from(&s.from)).or_default().push(s);
        }
        for t in &n.terminals {
            critical.insert(axis.coordinate(Point::from(&t.position)));
        }
        for p in n.cells.iter().map(Point::from) {
            let a = ins.get(&p).map(Vec::as_slice).unwrap_or_default();
            let b = outs.get(&p).map(Vec::as_slice).unwrap_or_default();
            if a.len() != 1
                || b.len() != 1
                || axis.cross(Point::from(&a[0].from)) != axis.cross(p)
                || axis.cross(Point::from(&b[0].to)) != axis.cross(p)
                || (axis.coordinate(p) - axis.coordinate(Point::from(&a[0].from))).signum()
                    != (axis.coordinate(Point::from(&b[0].to)) - axis.coordinate(p)).signum()
                || !equal_rate(a[0].rate, b[0].rate)
            {
                critical.insert(axis.coordinate(p));
            }
        }
    }
    let placements: IndexMap<_, _> = w.placements.iter().map(|p| (&p.instance, p)).collect();
    let mut cuts = IndexSet::new();
    for n in &w.transport_networks {
        for t in &n.terminals {
            if let TransportNetworkEndpoint::Facility { instance, .. } = &t.endpoint {
                let p = placements[instance];
                let terminal = axis.coordinate(Point::from(&t.position));
                let body = terminal.clamp(axis.start(p), axis.start(p) + axis.size(p) - 1);
                cuts.extend(body.min(terminal) + 1..=body.max(terminal));
            }
        }
    }
    let b = extent(w)?;
    let (low, high) = match axis {
        Axis::X => (b.min_x, b.max_x),
        Axis::Y => (b.min_y, b.max_y),
    };
    Ok(Strips {
        remove: (low..=high).filter(|p| !critical.contains(p)).collect(),
        insert: (low + 1..=high)
            .filter(|p| {
                !cuts.contains(p)
                    && !w
                        .placements
                        .iter()
                        .any(|q| axis.start(q) < *p && *p < axis.start(q) + axis.size(q))
            })
            .collect(),
    })
}
/// One transaction: insert a corridor or compact all removable strips on an axis.
/// `position` is absent for axis-wide contraction.
pub fn edit(w: &Witness, axis: Axis, position: Option<i64>, count: i64) -> Result<Witness> {
    let error = |message| {
        let mut e = Diagnostic::new("factory.edit-parameters", message, serde_json::Value::Null);
        e.stage = "factory-edit";
        e.severity = "info";
        e
    };
    let choices = candidates(w, axis)?;
    let insert = position.is_some();
    if count <= 0
        || match position {
            Some(p) => !choices.insert.contains(&p) || !insertion_counts(w, axis).contains(&count),
            None => count != choices.remove.len() as i64,
        }
    {
        return Err(error(
            "Strip count or cut does not match the current compact vocabulary",
        ));
    }
    let removed = |p: Point| !insert && choices.remove.binary_search(&axis.coordinate(p)).is_ok();
    let shift = |p: Point| {
        let coordinate = axis.coordinate(p);
        let delta = if let Some(position) = position {
            if coordinate >= position { count } else { 0 }
        } else {
            -(choices.remove.partition_point(|&q| q < coordinate) as i64)
        };
        match axis {
            Axis::X => p.shifted(delta, 0),
            Axis::Y => p.shifted(0, delta),
        }
    };
    let mut result = w.clone();
    for p in &mut result.placements {
        let q = shift(Point { x: p.x, y: p.y });
        p.x = q.x;
        p.y = q.y;
    }
    for c in &mut result.logistics_components {
        c.position = shift(Point::from(&c.position)).into();
    }
    for n in &mut result.transport_networks {
        let mut cells: Vec<_> = n
            .cells
            .iter()
            .map(Point::from)
            .filter(|&p| !removed(p))
            .map(shift)
            .collect();
        let mut segments = Vec::new();
        let out: IndexMap<_, _> = n
            .segments
            .iter()
            .map(|s| (Point::from(&s.from), s))
            .collect();
        for s in &n.segments {
            if removed(Point::from(&s.from)) {
                continue;
            }
            let from = shift(Point::from(&s.from));
            let mut end = Point::from(&s.to);
            while removed(end) {
                let next = out
                    .get(&end)
                    .ok_or_else(|| error("Missing straight continuation"))?;
                if !equal_rate(next.rate, s.rate) {
                    return Err(error("Missing straight continuation"));
                }
                end = Point::from(&next.to);
            }
            let to = shift(end);
            let distance = from.distance(to);
            if distance == 0 || (from.x != to.x && from.y != to.y) {
                return Err(error("Strip edit produced a noncardinal segment"));
            }
            let dx = (to.x - from.x).signum();
            let dy = (to.y - from.y).signum();
            let mut previous = from;
            for step in 1..=distance {
                let next = from.shifted(dx * step, dy * step);
                if step < distance {
                    cells.push(next);
                }
                segments.push(TransportNetworkSegment {
                    from: previous.into(),
                    to: next.into(),
                    rate: s.rate,
                });
                previous = next;
            }
        }

        if cells.iter().collect::<IndexSet<_>>().len() != cells.len() {
            return Err(error(
                "Strip insertion produced duplicate same-network cells",
            ));
        }
        n.cells = cells.into_iter().map(Into::into).collect();
        n.segments = segments;
        for t in &mut n.terminals {
            t.position = shift(Point::from(&t.position)).into();
        }
    }
    Ok(result)
}

/// Two geometry-derived corridor scales; no game data is embedded here.
pub fn insertion_counts(w: &Witness, axis: Axis) -> [i64; 2] {
    [
        1,
        w.placements.iter().map(|p| axis.size(p)).max().unwrap_or(1) + 2,
    ]
}

pub fn insertion_cuts(choices: &[i64]) -> Vec<i64> {
    const LIMIT: usize = 32;
    if choices.len() <= LIMIT {
        choices.to_vec()
    } else {
        (0..LIMIT)
            .map(|i| choices[i * (choices.len() - 1) / (LIMIT - 1)])
            .collect()
    }
}
