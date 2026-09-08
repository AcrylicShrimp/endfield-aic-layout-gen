//! Fixed-width operation and symmetric operand features.

use crate::catalog::Catalog;
use crate::command::Command;
use crate::diagnostic::Result;
use crate::state::{self, ARMS, Point, State};

use super::{CANDIDATE_FEATURES, CandidateFeature, FeatureKey};

pub fn context(
    catalog: &Catalog,
    state: &State,
    command: &Command,
) -> Result<Option<(Point, Point)>> {
    match command {
        Command::Unbind(ti) => state::location(catalog, state, *ti, state.bindings[*ti].as_ref()),
        Command::Bind(ti, binding) => state::location(catalog, state, *ti, Some(binding)),
        _ => Ok(None),
    }
}

fn operand(
    value: &mut CandidateFeature,
    slot: usize,
    entity: usize,
    role: usize,
    point: Point,
    rotation: Option<i64>,
    arm: Option<Point>,
    scale: f64,
) -> Result<()> {
    value.entities[slot] = entity as i64;
    value.mask[slot] = true;
    value.operands[slot][role] = 1.0;
    value.operands[slot][5] = (point[0] as f64 / scale) as f32;
    value.operands[slot][6] = (point[1] as f64 / scale) as f32;
    if let Some(rotation) = rotation {
        value.operands[slot][7 + (rotation / 90) as usize] = 1.0;
    }
    if let Some(arm) = arm {
        let direction = ARMS
            .iter()
            .position(|a| *a == arm)
            .ok_or_else(|| state::range_error("Noncardinal operand arm"))?;
        value.operands[slot][11 + direction] = 1.0;
    }
    value.points[slot] = point;
    Ok(())
}

pub fn encode(catalog: &Catalog, scale: f64, key: &FeatureKey) -> Result<CandidateFeature> {
    let nf = catalog.types.len();
    let nn = catalog.rates.len();
    let mut value = CandidateFeature {
        feature: [0.0; CANDIDATE_FEATURES],
        operands: [[0.0; super::OPERAND_FEATURES]; 2],
        entities: [-1; 2],
        points: [[0; 2]; 2],
        mask: [false; 2],
    };
    value.feature[key.command.operation()] = 1.0;
    match &key.command {
        Command::SetPlacements(assignments) => {
            if assignments.len() > 2 {
                return Err(state::range_error(
                    "Observation supports the admitted single/pair placement domain",
                ));
            }
            value.feature[14] = assignments.len() as f32;
            for (slot, (fi, pose)) in assignments.iter().enumerate() {
                operand(
                    &mut value,
                    slot,
                    *fi,
                    0,
                    [pose[0], pose[1]],
                    Some(pose[2]),
                    None,
                    scale,
                )?;
            }
        }
        Command::Bind(ti, _) | Command::Unbind(ti) => {
            let (point, arm) = key
                .context
                .ok_or_else(|| state::range_error("Terminal operand has no location"))?;
            operand(
                &mut value,
                0,
                nf + nn + ti,
                4,
                point,
                None,
                Some(arm),
                scale,
            )?;
        }
        Command::Erase(ni, cells) => {
            if cells.len() > 2 {
                return Err(state::range_error(
                    "Observation supports the admitted single/pair erase domain",
                ));
            }
            value.feature[13] = (cells.len() as f64).ln_1p() as f32;
            for (slot, cell) in cells.iter().enumerate() {
                operand(&mut value, slot, nf + ni, 3, *cell, None, None, scale)?;
            }
        }
        Command::Extend {
            network,
            anchor,
            backward,
            direction,
            length,
        } => {
            value.feature[7 + direction] = 1.0;
            value.feature[11 + usize::from(*backward)] = 1.0;
            value.feature[13] = (*length as f64).ln_1p() as f32;
            let d = ARMS[*direction];
            let endpoint = state::add(*anchor, [d[0] * length, d[1] * length])?;
            operand(&mut value, 0, nf + network, 1, *anchor, None, None, scale)?;
            operand(&mut value, 1, nf + network, 2, endpoint, None, None, scale)?;
        }
        Command::Connect {
            network,
            source,
            target,
        } => {
            operand(&mut value, 0, nf + network, 1, *source, None, None, scale)?;
            operand(&mut value, 1, nf + network, 2, *target, None, None, scale)?;
        }
        Command::Discard => {}
    }
    Ok(value)
}
