use aic_layout::rng::Rng;
use aic_pilot_native::physical_editor::learning::{Observation, schema::PORT_FEATURES};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PortMode {
    RetainCompatible,
    RandomCompatible,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub terminal: i64,
    pub port: [f32; PORT_FEATURES],
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SelectedAction {
    pub root: usize,
    pub family: usize,
    pub requested_port_mode: Option<PortMode>,
    pub actual_port_mode: Option<PortMode>,
    pub ports: Vec<usize>,
    pub assignments: Vec<Assignment>,
}

pub fn choose(observation: &Observation, rng: &mut Rng) -> Result<SelectedAction> {
    let mut families: [Vec<usize>; 3] = Default::default();
    for (root, (&invalid, meta)) in observation
        .input
        .root_invalid
        .iter()
        .zip(&observation.input.root_meta)
        .enumerate()
    {
        ensure!(
            meta[0] >= 0 && meta[0] < 3,
            "aic-learning.outcomes-action: invalid action family"
        );
        if !invalid {
            families[meta[0] as usize].push(root);
        }
    }
    let available: Vec<_> = families
        .iter()
        .enumerate()
        .filter_map(|(family, roots)| (!roots.is_empty()).then_some(family))
        .collect();
    ensure!(
        !available.is_empty(),
        "aic-learning.outcomes-action: no available action family"
    );
    let family = available[rng.index(available.len())];
    let root = families[family][rng.index(families[family].len())];
    if family != 0 {
        return Ok(SelectedAction {
            root,
            family,
            requested_port_mode: None,
            actual_port_mode: None,
            ports: vec![],
            assignments: vec![],
        });
    }

    let meta = observation.input.root_meta[root];
    let groups = observation
        .domain
        .groups
        .get(meta[1] as usize)
        .ok_or_else(|| anyhow::anyhow!("aic-learning.outcomes-action: invalid port owner"))?;
    let requested = if rng.index(2) == 0 {
        PortMode::RetainCompatible
    } else {
        PortMode::RandomCompatible
    };
    let retained = (requested == PortMode::RetainCompatible)
        .then(|| retain(observation, groups))
        .transpose()?
        .flatten();
    let (actual, ports) = match retained {
        Some(ports) => (PortMode::RetainCompatible, ports),
        None => (PortMode::RandomCompatible, random(groups, rng)?),
    };
    let mut assignments = Vec::with_capacity(groups.len());
    for (group, &port) in groups.iter().zip(&ports) {
        let &(start, end) = group.features.get(&meta[2]).ok_or_else(|| {
            anyhow::anyhow!("aic-learning.outcomes-action: missing rotation bank")
        })?;
        ensure!(
            end.saturating_sub(start) == group.ports.len() && port < group.ports.len(),
            "aic-learning.outcomes-action: inconsistent port feature bank"
        );
        assignments.push(Assignment {
            terminal: group.graph_node as i64,
            port: observation.input.port_features[start + port],
        });
    }
    Ok(SelectedAction {
        root,
        family,
        requested_port_mode: Some(requested),
        actual_port_mode: Some(actual),
        ports,
        assignments,
    })
}

fn retain(
    observation: &Observation,
    groups: &[aic_pilot_native::physical_editor::learning::PortGroup],
) -> Result<Option<Vec<usize>>> {
    let mut used = BTreeSet::new();
    let mut ports = Vec::with_capacity(groups.len());
    for group in groups {
        let Some(Some(current)) = observation.current_ports.get(group.graph_node) else {
            return Ok(None);
        };
        let Some(port) = group
            .ports
            .iter()
            .position(|candidate| candidate == current)
        else {
            return Ok(None);
        };
        if !used.insert(current.as_str()) {
            return Ok(None);
        }
        ports.push(port);
    }
    Ok(Some(ports))
}

fn random(
    groups: &[aic_pilot_native::physical_editor::learning::PortGroup],
    rng: &mut Rng,
) -> Result<Vec<usize>> {
    let mut used = BTreeSet::new();
    let mut ports = Vec::with_capacity(groups.len());
    for group in groups {
        let available: Vec<_> = group
            .ports
            .iter()
            .enumerate()
            .filter_map(|(index, port)| (!used.contains(port.as_str())).then_some(index))
            .collect();
        ensure!(
            !available.is_empty(),
            "aic-learning.outcomes-action: empty compatible port domain"
        );
        let index = available[rng.index(available.len())];
        used.insert(group.ports[index].as_str());
        ports.push(index);
    }
    Ok(ports)
}
