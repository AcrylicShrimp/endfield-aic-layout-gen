//! Transactional application of the seven existing pilot edit operations.

use std::collections::BTreeSet;

use crate::acceptance;
use crate::catalog::Catalog;
use crate::command::Command;
use crate::diagnostic::{Diagnostic, Result};
use crate::engine::Config;
use crate::state::{self, ARMS, State};

pub fn apply(
    catalog: &Catalog,
    config: &Config,
    state: &State,
    base: &State,
    command: &Command,
) -> Result<State> {
    let error =
        |message: &str| Diagnostic::new("edit.rejected", "edit", message, command.entity(catalog));
    if matches!(command, Command::Discard) {
        return Ok(base.clone());
    }
    let mut result = state.clone();
    match command {
        Command::SetPlacements(assignments) => {
            if !(1..=2).contains(&assignments.len()) {
                return Err(error("Placement group limit is two"));
            }
            for &(fi, pose) in assignments {
                if catalog
                    .terminals
                    .iter()
                    .enumerate()
                    .any(|(ti, t)| t.facility == Some(fi) && result.bindings[ti].is_some())
                {
                    return Err(error("Unbind a facility before moving it"));
                }
                result.poses[fi] = pose;
            }
        }
        Command::Unbind(ti) => {
            if result.bindings[*ti].is_none() {
                return Err(error("Terminal is already unbound"));
            }
            result.bindings[*ti] = None;
        }
        Command::Bind(ti, binding) => {
            if result.bindings[*ti].is_some() {
                return Err(error("Unbind a terminal before rebinding"));
            }
            let ni = catalog.terminals[*ti].network;
            let (cell, _) = state::location(catalog, &result, *ti, Some(binding))?
                .ok_or_else(|| error("Missing terminal location"))?;
            result.bindings[*ti] = Some(binding.clone());
            result.networks[ni].cells.insert(cell);
        }
        Command::Erase(ni, cells) => {
            let cells: BTreeSet<_> = cells.iter().copied().collect();
            let network = &mut result.networks[*ni];
            if !(1..=2).contains(&cells.len()) || !cells.is_subset(&network.cells) {
                return Err(error("Unknown cells or erase length outside domain"));
            }
            if cells.len() == 2 {
                let points: Vec<_> = cells.iter().copied().collect();
                if state::arm(points[0], points[1]).is_err() {
                    return Err(error("Erase cells must be adjacent"));
                }
            }
            network.cells.retain(|c| !cells.contains(c));
            network
                .edges
                .retain(|[a, b]| !cells.contains(a) && !cells.contains(b));
        }
        Command::Extend {
            network,
            anchor,
            backward,
            direction,
            length,
        } => {
            let n = &mut result.networks[*network];
            if !config.segment_lengths.contains(length) || !n.cells.contains(anchor) {
                return Err(error("Invalid extension anchor or length"));
            }
            let d = ARMS[*direction];
            let mut path = vec![*anchor];
            for i in 1..=*length {
                let p = state::add(*anchor, [d[0] * i, d[1] * i])?;
                if n.cells.contains(&p) {
                    return Err(error("Extension cannot silently connect existing cells"));
                }
                path.push(p);
            }
            n.cells.extend(&path[1..]);
            n.edges.extend(path.windows(2).map(|p| {
                if *backward {
                    [p[1], p[0]]
                } else {
                    [p[0], p[1]]
                }
            }));
        }
        Command::Connect {
            network,
            source,
            target,
        } => {
            if !result.networks[*network].edges.insert([*source, *target]) {
                return Err(error("Connection already exists"));
            }
        }
        Command::Discard => unreachable!("discard returned the validated base above"),
    }
    acceptance::mechanical(catalog, &result)?;
    Ok(result)
}
