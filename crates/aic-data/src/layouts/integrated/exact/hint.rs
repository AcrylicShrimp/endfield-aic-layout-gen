use std::collections::{BTreeMap, BTreeSet};

use pumpkin_solver::core::variables::DomainId;

use super::{ModelBranchComponent, ModelBridge, ModelInstance, ModelNetwork};
use crate::layouts::integrated::{
    ExactModelMetrics, IntegratedLayoutReport, ModelInput, TransportNetwork, WorldGridPosition,
    world_position,
};
use crate::logistics::LogisticsComponentKind;

#[derive(Debug, Default)]
pub(super) struct SolverHint {
    pub(super) assignments: BTreeMap<DomainId, i32>,
}

pub(super) fn build_solver_hint(
    prior: Option<&IntegratedLayoutReport>,
    input: &ModelInput,
    instances: &[ModelInstance],
    networks: &[ModelNetwork],
    branch_components: &[ModelBranchComponent],
    bridges: &[ModelBridge],
    metrics: &mut ExactModelMetrics,
) -> SolverHint {
    let Some(prior) = prior.filter(|report| report.success) else {
        return SolverHint::default();
    };
    let mut hint = SolverHint::default();

    for instance in instances {
        let Some(placement) = prior
            .placements
            .iter()
            .find(|placement| placement.instance == instance.input.id)
        else {
            continue;
        };
        let mut matched = false;
        for candidate in &instance.candidates {
            let selected = candidate.rotation == placement.rotation
                && i64::from(candidate.x) == placement.x
                && i64::from(candidate.y) == placement.y;
            hint.push(candidate.selected, i32::from(selected));
            matched |= selected;
        }
        metrics.hinted_placements += usize::from(matched);
    }

    let prior_networks = prior
        .transport_networks
        .iter()
        .map(|network| (network.id.as_str(), network))
        .collect::<BTreeMap<_, _>>();
    for model_network in networks {
        let network_input = &input.networks[model_network.input_index];
        let Some(prior_network) = prior_networks.get(network_input.id()).copied() else {
            continue;
        };
        metrics.hinted_networks += 1;
        hint_network_cells(&mut hint, input, model_network, prior_network);
        hint_network_arcs(
            &mut hint,
            input,
            network_input,
            model_network,
            prior_network,
        );
        for terminal in &model_network.terminals {
            let Some(prior_terminal) = prior_network
                .terminals
                .iter()
                .find(|candidate| candidate.id == terminal.id)
            else {
                continue;
            };
            let mut matched = false;
            for option in &terminal.options {
                let selected = option.endpoint == prior_terminal.endpoint
                    && world_position(option.cell, input.width) == prior_terminal.position;
                hint.push(option.selected, i32::from(selected));
                matched |= selected;
            }
            metrics.hinted_terminals += usize::from(matched);
        }
    }

    hint_branch_components(
        &mut hint,
        prior,
        input,
        branch_components,
        &prior_networks,
        metrics,
    );
    hint_bridges(&mut hint, prior, input, bridges, metrics);
    metrics.hint_variables = hint.assignments.len();
    hint
}

impl SolverHint {
    fn push(&mut self, variable: DomainId, value: i32) {
        if let Some(previous) = self.assignments.insert(variable, value) {
            assert_eq!(
                previous, value,
                "one prior solution cannot hint conflicting values for one solver variable"
            );
        }
    }
}

fn hint_network_cells(
    hint: &mut SolverHint,
    input: &ModelInput,
    model_network: &ModelNetwork,
    prior_network: &TransportNetwork,
) {
    let prior_cells = prior_network
        .cells
        .iter()
        .filter_map(|position| position_cell(position, input))
        .collect::<BTreeSet<_>>();
    for (cell, variable) in model_network.route_cells.iter().enumerate() {
        hint.push(*variable, i32::from(prior_cells.contains(&cell)));
    }
}

fn hint_network_arcs(
    hint: &mut SolverHint,
    input: &ModelInput,
    network_input: &super::super::networks::RoutingNetworkInput,
    model_network: &ModelNetwork,
    prior_network: &TransportNetwork,
) {
    let prior_segments = prior_network
        .segments
        .iter()
        .filter_map(|segment| {
            Some((
                (
                    position_cell(&segment.from, input)?,
                    position_cell(&segment.to, input)?,
                ),
                segment.rate,
            ))
        })
        .collect::<BTreeMap<_, _>>();
    for arc in &model_network.arcs {
        let rate = prior_segments.get(&(arc.from, arc.to)).copied();
        let flow = rate
            .and_then(|rate| network_input.flow_units_for_hint(rate))
            .unwrap_or(0);
        hint.push(arc.selected, i32::from(flow > 0));
        hint.push(arc.flow, flow);
    }
}

fn hint_branch_components(
    hint: &mut SolverHint,
    prior: &IntegratedLayoutReport,
    input: &ModelInput,
    branch_components: &[ModelBranchComponent],
    prior_networks: &BTreeMap<&str, &TransportNetwork>,
    metrics: &mut ExactModelMetrics,
) {
    let mut matched_component_ids = BTreeSet::new();
    for model_component in branch_components {
        let network_id = input.networks[model_component.network_index].id();
        let selected = prior_networks
            .get(network_id)
            .into_iter()
            .flat_map(|network| &network.component_ids)
            .filter_map(|id| {
                prior
                    .logistics_components
                    .iter()
                    .find(|component| component.id == *id)
            })
            .any(|component| {
                component.kind == model_component.kind
                    && component.transport == model_component.transport
                    && component.component == model_component.component
                    && component.position == world_position(model_component.cell, input.width)
                    && component.rotation == model_component.rotation
            });
        hint.push(model_component.selected, i32::from(selected));
        if selected {
            matched_component_ids.insert((network_id.to_string(), model_component.cell));
        }
    }
    metrics.hinted_components += matched_component_ids.len();
}

fn hint_bridges(
    hint: &mut SolverHint,
    prior: &IntegratedLayoutReport,
    input: &ModelInput,
    bridges: &[ModelBridge],
    metrics: &mut ExactModelMetrics,
) {
    for bridge in bridges {
        let prior_bridge = prior.logistics_components.iter().find(|component| {
            component.kind == LogisticsComponentKind::Bridge
                && component.transport == bridge.transport
                && component.component == bridge.component
                && component.position == world_position(bridge.cell, input.width)
        });
        hint.push(bridge.selected, i32::from(prior_bridge.is_some()));
        for (rotation, variable) in &bridge.rotations {
            hint.push(
                *variable,
                i32::from(prior_bridge.is_some_and(|component| component.rotation == *rotation)),
            );
        }
        metrics.hinted_components += usize::from(prior_bridge.is_some());
    }
}

fn position_cell(position: &WorldGridPosition, input: &ModelInput) -> Option<usize> {
    let x = i32::try_from(position.x).ok()?;
    let y = i32::try_from(position.y).ok()?;
    if x < 0 || y < 0 || x >= input.width || y >= input.height {
        return None;
    }
    Some(usize::try_from(y.checked_mul(input.width)?.checked_add(x)?).ok()?)
}
