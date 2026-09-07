//! Complete owned observation arrays with the established arithmetic/channel contract.

use std::collections::BTreeMap;
use std::time::Instant;

use num_traits::ToPrimitive;
use numpy::ndarray::{Array1, Array2, Array3};

use crate::cache::Cache;
use crate::catalog::Catalog;
use crate::command::{Command, OPERATIONS};
use crate::diagnostic::{Diagnostic, Result};
use crate::engine::{Config, Engine, Handle};
use crate::state::{self, ARMS, Binding, Index, Point, SIDES, Score, State};

pub struct Tables {
    pub object_template: Array2<f32>,
    pub object_dynamic: usize,
    pub category_offsets: [usize; 8],
    pub feature_numeric: usize,
    pub per_state: usize,
}

impl Tables {
    pub fn new(catalog: &Catalog, scale: f64) -> Result<Self> {
        let (nf, nn, nt) = (
            catalog.types.len(),
            catalog.rates.len(),
            catalog.terminals.len(),
        );
        let counts = [OPERATIONS.len(), nf, nf, nn, nt, 4, 2, 4];
        let mut category_offsets = [0; 8];
        let mut feature_numeric = 0usize;
        for (i, count) in counts.into_iter().enumerate() {
            category_offsets[i] = feature_numeric;
            feature_numeric = feature_numeric
                .checked_add(count)
                .ok_or_else(|| state::range_error("Feature dimension overflow"))?;
        }
        let object_dynamic = nn
            .checked_add(nf)
            .and_then(|v| v.checked_add(2))
            .ok_or_else(|| state::range_error("Object dimension overflow"))?;
        let object_width = object_dynamic
            .checked_add(17)
            .ok_or_else(|| state::range_error("Object dimension overflow"))?;
        let object_rows = nt
            .checked_mul(2)
            .ok_or_else(|| state::range_error("Object row count overflow"))?;
        let mut template = Array2::from_shape_vec(
            (object_rows, object_width),
            zeros(product(&[object_rows, object_width])?)?,
        )
        .map_err(|e| state::range_error(&e.to_string()))?;
        for (ti, t) in catalog.terminals.iter().enumerate() {
            let ratio = (&catalog.rates[t.network] / &catalog.capacity)
                .to_f64()
                .ok_or_else(|| state::range_error("Rate cannot be represented in observation"))?;
            let capacity = catalog.capacity.to_f64().ok_or_else(|| {
                state::range_error("Capacity cannot be represented in observation")
            })?;
            for row in [ti, nt + ti] {
                template[[row, t.network]] = 1.0;
                if let Some(fi) = t.facility {
                    template[[row, nn + fi]] = 1.0;
                }
                template[[row, nn + nf]] = f32::from(t.facility.is_none());
                template[[row, nn + nf + 1]] = f32::from(t.source);
                template[[row, object_dynamic + 7]] = ratio as f32;
                template[[row, object_dynamic + 8]] = capacity as f32;
                if let Some(fi) = t.facility {
                    let size = &catalog.types[fi].footprint;
                    template[[row, object_dynamic + 9]] = (size.width as f64 / scale) as f32;
                    template[[row, object_dynamic + 10]] = (size.height as f64 / scale) as f32;
                }
            }
        }
        let per_state = nf
            .checked_add(nn)
            .and_then(|v| v.checked_add(8))
            .and_then(|v| v.checked_add(nt))
            .ok_or_else(|| state::range_error("Raster channel overflow"))?;
        Ok(Self {
            object_template: template,
            object_dynamic,
            category_offsets,
            feature_numeric,
            per_state,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FeatureKey {
    pub command: Command,
    pub context: Option<Point>,
}

pub struct CandidateFeature {
    pub feature: Vec<f32>,
    pub points: [i64; 4],
}

pub struct Observation {
    pub raster: Array3<f32>,
    pub objects: Array2<f32>,
    pub stats: Array1<f32>,
    pub candidates: Array2<f32>,
    pub points: Array2<i64>,
}

fn product(dimensions: &[usize]) -> Result<usize> {
    dimensions.iter().try_fold(1usize, |n, &d| {
        n.checked_mul(d)
            .ok_or_else(|| state::range_error("Array dimension overflow"))
    })
}

fn zeros<T: Clone + Default>(count: usize) -> Result<Vec<T>> {
    if count > isize::MAX as usize / size_of::<T>() {
        return Err(state::range_error("Array allocation size overflow"));
    }
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|e| {
        Diagnostic::new(
            "observation.resource_guard",
            "observation",
            e.to_string(),
            None,
        )
    })?;
    values.resize(count, T::default());
    Ok(values)
}

fn context(catalog: &Catalog, s: &State, command: &Command) -> Result<Option<Point>> {
    match command {
        Command::Unbind(ti) => {
            Ok(state::location(catalog, s, *ti, s.bindings[*ti].as_ref())?.map(|v| v.0))
        }
        Command::Bind(ti, binding) => {
            Ok(state::location(catalog, s, *ti, Some(binding))?.map(|v| v.0))
        }
        Command::Discard => Ok(Some([s.poses[0][0], s.poses[0][1]])),
        _ => Ok(None),
    }
}

fn encode_candidate(
    catalog: &Catalog,
    tables: &Tables,
    scale: f64,
    key: &FeatureKey,
) -> Result<CandidateFeature> {
    let command = &key.command;
    let (a, b) = if let Some(p) = key.context {
        (p, p)
    } else {
        match command {
            Command::SetPlacements(assignments) => {
                let a = assignments[0].1;
                let b = assignments[assignments.len() - 1].1;
                ([a[0], a[1]], [b[0], b[1]])
            }
            Command::Erase(_, cells) => (cells[0], cells[cells.len() - 1]),
            Command::Extend {
                anchor,
                direction,
                length,
                ..
            } => {
                let d = ARMS[*direction];
                (
                    *anchor,
                    state::add(*anchor, [d[0] * length, d[1] * length])?,
                )
            }
            Command::Connect { source, target, .. } => (*source, *target),
            _ => {
                return Err(Diagnostic::new(
                    "observation.command",
                    "observation",
                    "Candidate has no spatial context",
                    None,
                ));
            }
        }
    };
    let mut categories = [None; 8];
    categories[0] = Some(command.operation());
    let mut length = 0usize;
    let mut assignment_count = 0usize;
    match command {
        Command::SetPlacements(assignments) => {
            categories[1] = Some(assignments[0].0);
            if assignments.len() > 1 {
                categories[2] = Some(assignments[1].0);
            }
            categories[7] = Some((assignments[0].1[2] / 90) as usize);
            assignment_count = assignments.len();
        }
        Command::Bind(ti, binding) => {
            categories[3] = Some(catalog.terminals[*ti].network);
            categories[4] = Some(*ti);
            if let Binding::External { side, .. } = binding {
                categories[5] = SIDES.iter().position(|s| *s == side);
            }
        }
        Command::Unbind(ti) => {
            categories[3] = Some(catalog.terminals[*ti].network);
            categories[4] = Some(*ti);
        }
        Command::Erase(ni, cells) => {
            categories[3] = Some(*ni);
            length = cells.len();
        }
        Command::Extend {
            network,
            backward,
            direction,
            length: n,
            ..
        } => {
            categories[3] = Some(*network);
            categories[5] = Some(*direction);
            categories[6] = Some(usize::from(*backward));
            length =
                usize::try_from(*n).map_err(|_| state::range_error("Candidate length overflow"))?;
        }
        Command::Connect { network, .. } => categories[3] = Some(*network),
        Command::Discard => {}
    }
    let mut feature = zeros(tables.feature_numeric + 8)?;
    for (offset, category) in tables.category_offsets.into_iter().zip(categories) {
        if let Some(column) = category {
            feature[offset + column] = 1.0;
        }
    }
    let numeric = [
        a[0] as f64 / scale,
        a[1] as f64 / scale,
        b[0] as f64 / scale,
        b[1] as f64 / scale,
        (b[0] as i128 - a[0] as i128) as f64 / scale,
        (b[1] as i128 - a[1] as i128) as f64 / scale,
        length as f64 / 2.0,
        assignment_count as f64 / 2.0,
    ];
    for (slot, n) in feature[tables.feature_numeric..].iter_mut().zip(numeric) {
        *slot = n as f32;
    }
    Ok(CandidateFeature {
        feature,
        points: [a[0], a[1], b[0], b[1]],
    })
}

struct View<'a> {
    catalog: &'a Catalog,
    config: &'a Config,
    tables: &'a Tables,
    scale: f64,
    states: [&'a State; 2],
    indexes: [&'a Index; 2],
    commands: &'a [Command],
    initial_score: Score,
    base_score: Score,
    best_score: Score,
    steps: u64,
}

/// NumPy arange uses float32(start + 1) - float32(start) as its actual increment.
/// Keep separate multiply/add operations, followed by float32 scalar division.
fn coordinate(start: i64, index: usize, scale: f64) -> Result<f32> {
    let first = (start as f64) as f32;
    let next = (start
        .checked_add(1)
        .ok_or_else(|| state::range_error("Coordinate channel overflow"))? as f64)
        as f32;
    let delta = next - first;
    Ok((first + (index as f32) * delta) / (scale as f32))
}

fn encode(
    view: View<'_>,
    cache: &mut Cache<FeatureKey, CandidateFeature>,
    counters: &mut BTreeMap<String, u64>,
) -> Result<Observation> {
    let View {
        catalog,
        config,
        tables,
        scale,
        states,
        indexes,
        commands,
        initial_score,
        base_score,
        best_score,
        steps,
    } = view;
    let used = state::bounds(indexes[0].occupied.iter().chain(&indexes[1].occupied))?;
    let origin = state::add([used[0], used[1]], [-config.halo, -config.halo])?;
    let high = state::add([used[2], used[3]], [config.halo, config.halo])?;
    let (w, h) = (
        high[0] as i128 - origin[0] as i128 + 1,
        high[1] as i128 - origin[1] as i128 + 1,
    );
    if w <= 0
        || h <= 0
        || w.checked_mul(h)
            .is_none_or(|n| n > config.raster_cell_guard as i128)
    {
        return Err(Diagnostic::new(
            "observation.resource_guard",
            "observation",
            format!("{w}x{h} exceeds declared raster-work guard"),
            None,
        ));
    }
    let (w, h) = (w as usize, h as usize);
    let channels = tables
        .per_state
        .checked_mul(2)
        .and_then(|v| v.checked_add(3))
        .ok_or_else(|| state::range_error("Raster channel overflow"))?;
    let mut raster = Array3::from_shape_vec((channels, h, w), zeros(product(&[channels, h, w])?)?)
        .map_err(|e| state::range_error(&e.to_string()))?;
    let mut objects = tables.object_template.clone();
    let pixel = |p: Point| -> Result<(usize, usize)> {
        let (x, y) = (
            p[0] as i128 - origin[0] as i128,
            p[1] as i128 - origin[1] as i128,
        );
        if x < 0 || y < 0 || x >= w as i128 || y >= h as i128 {
            return Err(state::range_error(
                "Observed geometry is outside used bounds",
            ));
        }
        Ok((y as usize, x as usize))
    };
    for state_index in 0..2 {
        let s = states[state_index];
        let index = indexes[state_index];
        let offset = state_index * tables.per_state;
        for (fi, body) in index.bodies.iter().enumerate() {
            for &p in body {
                let (y, x) = pixel(p)?;
                raster[[offset + fi, y, x]] = 1.0;
            }
        }
        let net_offset = offset + catalog.types.len();
        let arm_offset = net_offset + catalog.rates.len();
        for (ni, n) in s.networks.iter().enumerate() {
            for &p in &n.cells {
                let (y, x) = pixel(p)?;
                raster[[net_offset + ni, y, x]] = 1.0;
                for (io, arms) in [&index.incoming[ni], &index.outgoing[ni]]
                    .into_iter()
                    .enumerate()
                {
                    for arm in arms.get(&p).into_iter().flatten() {
                        let d = ARMS
                            .iter()
                            .position(|a| a == arm)
                            .ok_or_else(|| state::range_error("Noncardinal observation arm"))?;
                        raster[[arm_offset + io * 4 + d, y, x]] = 1.0;
                    }
                }
            }
        }
        for (ti, t) in catalog.terminals.iter().enumerate() {
            let loc = state::location(catalog, s, ti, s.bindings[ti].as_ref())?;
            let position = loc.map_or([0, 0], |v| v.0);
            let row = state_index * catalog.terminals.len() + ti;
            let dynamic = tables.object_dynamic;
            if let Some((position, arm)) = loc {
                let (y, x) = pixel(position)?;
                raster[[arm_offset + 8 + ti, y, x]] = 1.0;
                let direction = ARMS
                    .iter()
                    .position(|a| *a == arm)
                    .ok_or_else(|| state::range_error("Noncardinal terminal arm"))?;
                objects[[row, dynamic + 3 + direction]] = 1.0;
            }
            objects[[row, dynamic]] = f32::from(s.bindings[ti].is_some());
            objects[[row, dynamic + 1]] = (position[0] as f64 / scale) as f32;
            objects[[row, dynamic + 2]] = (position[1] as f64 / scale) as f32;
            if let Some(fi) = t.facility {
                let [x, y, r] = s.poses[fi];
                objects[[row, dynamic + 11]] = (x as f64 / scale) as f32;
                objects[[row, dynamic + 12]] = (y as f64 / scale) as f32;
                objects[[row, dynamic + 13 + (r / 90) as usize]] = 1.0;
            }
        }
    }
    let x_values: Vec<_> = (0..w)
        .map(|i| coordinate(origin[0], i, scale))
        .collect::<Result<_>>()?;
    let y_values: Vec<_> = (0..h)
        .map(|i| coordinate(origin[1], i, scale))
        .collect::<Result<_>>()?;
    for y in 0..h {
        for x in 0..w {
            raster[[channels - 3, y, x]] = x_values[x];
            raster[[channels - 2, y, x]] = y_values[y];
            raster[[channels - 1, y, x]] = 1.0;
        }
    }
    let mut features = zeros(product(&[commands.len(), tables.feature_numeric + 8])?)?;
    let mut points = zeros::<i64>(product(&[commands.len(), 4])?)?;
    let relative_origin = [origin[0], origin[1], origin[0], origin[1]];
    for (ci, command) in commands.iter().enumerate() {
        let key = FeatureKey {
            command: command.clone(),
            context: context(catalog, states[0], command)?,
        };
        let encoded = if let Some(encoded) = cache.get(&key) {
            *counters.entry("encoding_cache_hits".into()).or_default() += 1;
            encoded
        } else {
            *counters.entry("encoding_cache_misses".into()).or_default() += 1;
            cache.insert(key.clone(), encode_candidate(catalog, tables, scale, &key)?);
            cache
                .get(&key)
                .ok_or_else(|| state::range_error("Inserted candidate cache entry is absent"))?
        };
        let width = tables.feature_numeric + 8;
        features[ci * width..(ci + 1) * width].copy_from_slice(&encoded.feature);
        for j in 0..4 {
            points[ci * 4 + j] = encoded.points[j]
                .checked_sub(relative_origin[j])
                .ok_or_else(|| state::range_error("Relative candidate point overflow"))?;
        }
    }
    counters.insert("encoding_cache_entries".into(), cache.len() as u64);
    let area = initial_score[0] as f64;
    let stats = [
        (config.horizon - steps) as f64 / config.horizon as f64,
        best_score[0] as f64 / area,
        base_score[0] as f64 / area,
        best_score[1] as f64 / area,
        base_score[1] as f64 / area,
        best_score[2] as f64 / area,
        base_score[2] as f64 / area,
        states[0].bindings.iter().filter(|b| b.is_none()).count() as f64
            / catalog.terminals.len() as f64,
    ];
    Ok(Observation {
        raster,
        objects,
        stats: Array1::from_vec(stats.into_iter().map(|n| n as f32).collect()),
        candidates: Array2::from_shape_vec((commands.len(), tables.feature_numeric + 8), features)
            .map_err(|e| state::range_error(&e.to_string()))?,
        points: Array2::from_shape_vec((commands.len(), 4), points)
            .map_err(|e| state::range_error(&e.to_string()))?,
    })
}

impl Engine {
    pub fn observation(&mut self, handle: Handle) -> Result<Observation> {
        self.domain(handle)?;
        let started = Instant::now();
        let domain = self
            .domain
            .as_ref()
            .ok_or_else(|| state::range_error("Candidate domain disappeared"))?;
        let base_index = Index::build(&self.catalog, &self.base)?;
        let view = View {
            catalog: &self.catalog,
            config: &self.config,
            tables: &self.observation_tables,
            scale: self.scale,
            states: [&self.draft, &self.base],
            indexes: [&domain.index, &base_index],
            commands: &domain.commands,
            initial_score: self.initial_score,
            base_score: self.base_score,
            best_score: self.best_score,
            steps: self.steps,
        };
        let observation = encode(view, &mut self.feature_cache, &mut self.counters)?;
        *self.seconds.entry("observation".into()).or_default() += started.elapsed().as_secs_f64();
        Ok(observation)
    }
}
