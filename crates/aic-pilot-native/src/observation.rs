//! Owned schema-2 tensors with semantic entities and fixed-width candidate operands.

mod candidates;
mod entities;

use std::collections::BTreeMap;
use std::time::Instant;

use numpy::ndarray::{Array1, Array2, Array3};
use serde_json::{Value, json};

use crate::cache::Cache;
use crate::catalog::Catalog;
use crate::command::Command;
use crate::diagnostic::{Diagnostic, Result};
use crate::engine::{Config, Engine, Handle};
use crate::state::{self, ARMS, Index, Point, Score, State};

pub use entities::Tables;
pub const SCHEMA_VERSION: u32 = 2;
pub const ENTITY_FEATURES: usize = 42;
pub const CANDIDATE_FEATURES: usize = 15;
pub const OPERAND_FEATURES: usize = 15;
pub const RASTER_CHANNELS: usize = 23;

pub fn dimensions() -> Value {
    json!({"schema_version": SCHEMA_VERSION, "raster_channels": RASTER_CHANNELS,
           "entity_features": ENTITY_FEATURES, "entity_maps": 6, "edge_types": 8,
           "candidate_features": CANDIDATE_FEATURES, "operand_features": OPERAND_FEATURES,
           "operand_slots": 2, "stats": 8})
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FeatureKey {
    pub command: Command,
    pub context: Option<(Point, Point)>,
}

pub struct CandidateFeature {
    pub feature: [f32; CANDIDATE_FEATURES],
    pub operands: [[f32; OPERAND_FEATURES]; 2],
    pub entities: [i64; 2],
    pub points: [Point; 2],
    pub mask: [bool; 2],
}

pub struct Observation {
    pub raster: Array3<f32>,
    pub entity_maps: Array3<i64>,
    pub entities: Array2<f32>,
    pub edges: Array2<i64>,
    pub edge_types: Array1<i64>,
    pub stats: Array1<f32>,
    pub candidates: Array2<f32>,
    pub operands: Array3<f32>,
    pub operand_entities: Array2<i64>,
    pub operand_mask: Array2<bool>,
    pub points: Array3<i64>,
}

fn product(dimensions: &[usize]) -> Result<usize> {
    dimensions.iter().try_fold(1usize, |n, &d| {
        n.checked_mul(d)
            .ok_or_else(|| state::range_error("Array dimension overflow"))
    })
}

fn zeros<T: Clone + Default>(count: usize) -> Result<Vec<T>> {
    if count > isize::MAX as usize / size_of::<T>() {
        return Err(state::range_error(
            "Observation array exceeds addressable bytes",
        ));
    }
    Ok(vec![T::default(); count])
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

/// Match NumPy's float32 arange increment and scalar division.
fn coordinate(start: i64, index: usize, scale: f64) -> Result<f32> {
    let first = (start as f64) as f32;
    let next = (start
        .checked_add(1)
        .ok_or_else(|| state::range_error("Coordinate channel overflow"))? as f64)
        as f32;
    Ok((first + (index as f32) * (next - first)) / (scale as f32))
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
    let mut raster = Array3::from_shape_vec(
        (RASTER_CHANNELS, h, w),
        zeros(product(&[RASTER_CHANNELS, h, w])?)?,
    )
    .map_err(|e| state::range_error(&e.to_string()))?;
    let mut map_data: Vec<i64> = zeros(product(&[6, h, w])?)?;
    map_data.fill(-1);
    let mut entity_maps = Array3::from_shape_vec((6, h, w), map_data)
        .map_err(|e| state::range_error(&e.to_string()))?;
    let entities = entities::observe(catalog, tables, states, indexes, scale)?;
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
    let (nf, nn) = (catalog.types.len(), catalog.rates.len());
    for snapshot in 0..2 {
        let state = states[snapshot];
        let index = indexes[snapshot];
        let offset = snapshot * tables.per_state;
        let channel = snapshot * 10;
        for (fi, body) in index.bodies.iter().enumerate() {
            for &point in body {
                let (y, x) = pixel(point)?;
                raster[[channel, y, x]] = 1.0;
                entity_maps[[snapshot * 3, y, x]] = (offset + fi) as i64;
            }
        }
        for (ni, network) in state.networks.iter().enumerate() {
            for &point in &network.cells {
                let (y, x) = pixel(point)?;
                raster[[channel + 1, y, x]] = 1.0;
                entity_maps[[snapshot * 3 + 1, y, x]] = (offset + nf + ni) as i64;
                for (io, arms) in [&index.incoming[ni], &index.outgoing[ni]]
                    .into_iter()
                    .enumerate()
                {
                    for arm in arms.get(&point).into_iter().flatten() {
                        let direction = ARMS
                            .iter()
                            .position(|a| a == arm)
                            .ok_or_else(|| state::range_error("Noncardinal observation arm"))?;
                        raster[[channel + 2 + io * 4 + direction, y, x]] = 1.0;
                    }
                }
            }
        }
        for ti in 0..catalog.terminals.len() {
            if let Some((point, _)) =
                state::location(catalog, state, ti, state.bindings[ti].as_ref())?
            {
                let (y, x) = pixel(point)?;
                entity_maps[[snapshot * 3 + 2, y, x]] = (offset + nf + nn + ti) as i64;
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
            raster[[RASTER_CHANNELS - 3, y, x]] = x_values[x];
            raster[[RASTER_CHANNELS - 2, y, x]] = y_values[y];
            raster[[RASTER_CHANNELS - 1, y, x]] = 1.0;
        }
    }
    let mut features = zeros(product(&[commands.len(), CANDIDATE_FEATURES])?)?;
    let mut operands = zeros(product(&[commands.len(), 2, OPERAND_FEATURES])?)?;
    let mut operand_entities = zeros::<i64>(product(&[commands.len(), 2])?)?;
    let mut operand_mask = zeros::<bool>(product(&[commands.len(), 2])?)?;
    let mut points = zeros::<i64>(product(&[commands.len(), 2, 2])?)?;
    for (ci, command) in commands.iter().enumerate() {
        let key = FeatureKey {
            command: command.clone(),
            context: candidates::context(catalog, states[0], command)?,
        };
        let encoded = if let Some(encoded) = cache.get(&key) {
            *counters.entry("encoding_cache_hits".into()).or_default() += 1;
            encoded
        } else {
            *counters.entry("encoding_cache_misses".into()).or_default() += 1;
            cache.insert(key.clone(), candidates::encode(catalog, scale, &key)?);
            cache
                .get(&key)
                .ok_or_else(|| state::range_error("Inserted candidate cache entry is absent"))?
        };
        features[ci * CANDIDATE_FEATURES..(ci + 1) * CANDIDATE_FEATURES]
            .copy_from_slice(&encoded.feature);
        for slot in 0..2 {
            let start = (ci * 2 + slot) * OPERAND_FEATURES;
            operands[start..start + OPERAND_FEATURES].copy_from_slice(&encoded.operands[slot]);
            operand_entities[ci * 2 + slot] = encoded.entities[slot];
            operand_mask[ci * 2 + slot] = encoded.mask[slot];
            for coordinate in 0..2 {
                if !encoded.mask[slot] {
                    continue;
                }
                points[(ci * 2 + slot) * 2 + coordinate] = encoded.points[slot][coordinate]
                    .checked_sub(origin[coordinate])
                    .ok_or_else(|| state::range_error("Relative candidate point overflow"))?;
            }
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
        entity_maps,
        entities,
        edges: tables.edges.clone(),
        edge_types: tables.edge_types.clone(),
        stats: Array1::from_vec(stats.into_iter().map(|n| n as f32).collect()),
        candidates: Array2::from_shape_vec((commands.len(), CANDIDATE_FEATURES), features)
            .map_err(|e| state::range_error(&e.to_string()))?,
        operands: Array3::from_shape_vec((commands.len(), 2, OPERAND_FEATURES), operands)
            .map_err(|e| state::range_error(&e.to_string()))?,
        operand_entities: Array2::from_shape_vec((commands.len(), 2), operand_entities)
            .map_err(|e| state::range_error(&e.to_string()))?,
        operand_mask: Array2::from_shape_vec((commands.len(), 2), operand_mask)
            .map_err(|e| state::range_error(&e.to_string()))?,
        points: Array3::from_shape_vec((commands.len(), 2, 2), points)
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
