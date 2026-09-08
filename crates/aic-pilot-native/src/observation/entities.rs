//! Fixed semantic entity templates and immutable typed relationships.

use num_traits::ToPrimitive;
use numpy::ndarray::{Array1, Array2};

use crate::catalog::Catalog;
use crate::diagnostic::Result;
use crate::state::{self, ARMS, Index, State};

use super::ENTITY_FEATURES;

pub struct Tables {
    pub template: Array2<f32>,
    pub edges: Array2<i64>,
    pub edge_types: Array1<i64>,
    pub per_state: usize,
}

impl Tables {
    pub fn new(catalog: &Catalog, _scale: f64) -> Result<Self> {
        let (nf, nn, nt) = (
            catalog.types.len(),
            catalog.rates.len(),
            catalog.terminals.len(),
        );
        let per_state = nf
            .checked_add(nn)
            .and_then(|n| n.checked_add(nt))
            .ok_or_else(|| state::range_error("Entity count overflow"))?;
        let rows = per_state
            .checked_mul(2)
            .ok_or_else(|| state::range_error("Entity count overflow"))?;
        let mut template = Array2::from_shape_vec(
            (rows, ENTITY_FEATURES),
            super::zeros(super::product(&[rows, ENTITY_FEATURES])?)?,
        )
        .map_err(|e| state::range_error(&e.to_string()))?;
        let capacity = catalog
            .capacity
            .to_f64()
            .ok_or_else(|| state::range_error("Capacity cannot be observed"))?;
        let mut edges = Vec::new();
        let mut edge_types = Vec::new();
        for snapshot in 0..2 {
            let offset = snapshot * per_state;
            for entity in 0..per_state {
                template[[offset + entity, 3 + snapshot]] = 1.0;
            }
            for (fi, definition) in catalog.types.iter().enumerate() {
                let mut row = template.row_mut(offset + fi);
                row[0] = 1.0;
                for rotation in &definition.allowed_rotations {
                    row[13 + (rotation / 90) as usize] = 1.0;
                }
                let utilization = catalog.case.facilities[fi].utilization;
                row[27] = (utilization[0] as f64 / utilization[1] as f64) as f32;
                for (column, source) in [(28, false), (29, true)] {
                    row[column] = (catalog
                        .terminals
                        .iter()
                        .filter(|t| t.facility == Some(fi) && t.source == source)
                        .count() as f64)
                        .ln_1p() as f32;
                }
                for (i, (transport, direction)) in [
                    ("belt", "input"),
                    ("belt", "output"),
                    ("pipe", "input"),
                    ("pipe", "output"),
                ]
                .iter()
                .enumerate()
                {
                    row[30 + i] = (definition
                        .ports
                        .iter()
                        .filter(|p| p.transport == *transport && p.direction == *direction)
                        .count() as f64)
                        .ln_1p() as f32;
                }
            }
            for (ni, rate) in catalog.rates.iter().enumerate() {
                let mut row = template.row_mut(offset + nf + ni);
                row[1] = 1.0;
                row[34] = 1.0;
                row[25] = (rate / &catalog.capacity)
                    .to_f64()
                    .ok_or_else(|| state::range_error("Rate cannot be observed"))?
                    as f32;
                row[26] = capacity.ln_1p() as f32;
                row[28] = 1.0_f64.ln_1p() as f32;
                row[29] = 1.0_f64.ln_1p() as f32;
            }
            for (ti, terminal) in catalog.terminals.iter().enumerate() {
                let entity = offset + nf + nn + ti;
                let network = offset + nf + terminal.network;
                let mut row = template.row_mut(entity);
                row[2] = 1.0;
                row[if terminal.source { 17 } else { 18 }] = 1.0;
                row[34] = 1.0;
                row[19] = f32::from(terminal.facility.is_none());
                row[25] = (&catalog.rates[terminal.network] / &catalog.capacity)
                    .to_f64()
                    .ok_or_else(|| state::range_error("Rate cannot be observed"))?
                    as f32;
                row[26] = capacity.ln_1p() as f32;
                if let Some(fi) = terminal.facility {
                    let owner = offset + fi;
                    edges.extend([owner as i64, entity as i64, entity as i64, owner as i64]);
                    edge_types.extend([0, 1]);
                }
                edges.extend([entity as i64, network as i64, network as i64, entity as i64]);
                edge_types.extend(if terminal.source { [2, 3] } else { [4, 5] });
            }
        }
        for i in 0..per_state {
            edges.extend([
                i as i64,
                (per_state + i) as i64,
                (per_state + i) as i64,
                i as i64,
            ]);
            edge_types.extend([6, 7]);
        }
        Ok(Self {
            template,
            edges: Array2::from_shape_vec((edge_types.len(), 2), edges)
                .map_err(|e| state::range_error(&e.to_string()))?,
            edge_types: Array1::from_vec(edge_types),
            per_state,
        })
    }
}

pub fn observe(
    catalog: &Catalog,
    tables: &Tables,
    states: [&State; 2],
    indexes: [&Index; 2],
    scale: f64,
) -> Result<Array2<f32>> {
    let (nf, nn) = (catalog.types.len(), catalog.rates.len());
    let mut entities = tables.template.clone();
    for snapshot in 0..2 {
        let offset = snapshot * tables.per_state;
        let state = states[snapshot];
        for (fi, pose) in state.poses.iter().enumerate() {
            let mut row = entities.row_mut(offset + fi);
            let footprint = &catalog.types[fi].footprint;
            let (width, height) = if pose[2] % 180 == 0 {
                (footprint.width, footprint.height)
            } else {
                (footprint.height, footprint.width)
            };
            row[5] = (pose[0] as f64 / scale) as f32;
            row[6] = (pose[1] as f64 / scale) as f32;
            row[7] = (width as f64 / scale) as f32;
            row[8] = (height as f64 / scale) as f32;
            row[9 + (pose[2] / 90) as usize] = 1.0;
            row[36] = (indexes[snapshot].bodies[fi].len() as f64).ln_1p() as f32;
            let incident: Vec<_> = catalog
                .terminals
                .iter()
                .enumerate()
                .filter(|(_, t)| t.facility == Some(fi))
                .collect();
            row[37] = (incident
                .iter()
                .filter(|(ti, _)| state.bindings[*ti].is_some())
                .count() as f64
                / incident.len().max(1) as f64) as f32;
        }
        for (ni, network) in state.networks.iter().enumerate() {
            let mut row = entities.row_mut(offset + nf + ni);
            if !network.cells.is_empty() {
                let bounds = state::bounds(network.cells.iter())?;
                row[5] = (bounds[0] as f64 / scale) as f32;
                row[6] = (bounds[1] as f64 / scale) as f32;
                row[7] = ((bounds[2] as i128 - bounds[0] as i128 + 1) as f64 / scale) as f32;
                row[8] = ((bounds[3] as i128 - bounds[1] as i128 + 1) as f64 / scale) as f32;
            }
            row[36] = (network.cells.len() as f64).ln_1p() as f32;
            let terminals: Vec<_> = catalog
                .terminals
                .iter()
                .enumerate()
                .filter(|(_, t)| t.network == ni)
                .collect();
            row[37] = (terminals
                .iter()
                .filter(|(ti, _)| state.bindings[*ti].is_some())
                .count() as f64
                / terminals.len().max(1) as f64) as f32;
            for (ti, terminal) in terminals {
                if let Some((point, _)) =
                    state::location(catalog, state, ti, state.bindings[ti].as_ref())?
                {
                    let column = if terminal.source { 38 } else { 40 };
                    row[column] = (point[0] as f64 / scale) as f32;
                    row[column + 1] = (point[1] as f64 / scale) as f32;
                }
            }
        }
        for (ti, _) in catalog.terminals.iter().enumerate() {
            let mut row = entities.row_mut(offset + nf + nn + ti);
            row[20] = f32::from(state.bindings[ti].is_some());
            if let Some((point, arm)) =
                state::location(catalog, state, ti, state.bindings[ti].as_ref())?
            {
                row[5] = (point[0] as f64 / scale) as f32;
                row[6] = (point[1] as f64 / scale) as f32;
                let direction = ARMS
                    .iter()
                    .position(|a| *a == arm)
                    .ok_or_else(|| state::range_error("Noncardinal terminal arm"))?;
                row[21 + direction] = 1.0;
            }
        }
    }
    Ok(entities)
}
