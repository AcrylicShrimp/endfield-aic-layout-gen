use std::collections::{BTreeMap, BTreeSet};

use pumpkin_solver::core::proof::ConstraintTag;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::recorder::{ConstraintFamily, RecordedModel};
use super::{ModelBranchComponent, ModelBridge, ModelInstance, ModelNetwork};
use crate::layouts::integrated::{
    ExactAblationFixation, IntegratedLayoutDiagnostic, IntegratedLayoutReport, ModelInput,
    TransportNetwork, WorldGridPosition, world_position,
};
use crate::logistics::LogisticsComponentKind;

pub(super) fn post_research_fixation(
    solver: &mut RecordedModel,
    input: &ModelInput,
    instances: &[ModelInstance],
    networks: &[ModelNetwork],
    branch_components: &[ModelBranchComponent],
    bridges: &[ModelBridge],
    reference: Option<&IntegratedLayoutReport>,
    fixation: &ExactAblationFixation,
    tag: ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    if matches!(fixation, ExactAblationFixation::None) {
        return Ok(());
    }
    match fixation {
        ExactAblationFixation::None => {}
        ExactAblationFixation::Placements => {
            let reference = successful_reference(reference)?;
            fix_placements(solver, instances, reference, tag)?;
        }
        ExactAblationFixation::PlacementsAndTerminals => {
            let reference = successful_reference(reference)?;
            fix_placements(solver, instances, reference, tag)?;
            for network in networks {
                fix_network_terminals(solver, input, instances, network, reference, tag)?;
            }
        }
        ExactAblationFixation::NetworkRoute { network_id } => {
            let reference = successful_reference(reference)?;
            fix_network_route(
                solver,
                input,
                instances,
                networks,
                branch_components,
                bridges,
                reference,
                network_id,
                tag,
            )?;
        }
        ExactAblationFixation::ZeroNetworkArcs { network_ids } => {
            zero_network_arcs(solver, input, networks, network_ids, tag)?;
        }
        ExactAblationFixation::ReferenceWithZeroNetworkArcs {
            placements,
            terminals,
            network_ids,
        } => {
            let reference = successful_reference(reference)?;
            if *placements {
                fix_placements(solver, instances, reference, tag)?;
            }
            if *terminals {
                for network in networks {
                    fix_network_terminals(solver, input, instances, network, reference, tag)?;
                }
            }
            zero_network_arcs(solver, input, networks, network_ids, tag)?;
        }
    }
    Ok(())
}

fn zero_network_arcs(
    solver: &mut RecordedModel,
    input: &ModelInput,
    networks: &[ModelNetwork],
    network_ids: &[String],
    tag: ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let selected = network_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if selected.len() != network_ids.len() {
        return Err(IntegratedLayoutDiagnostic::error(
            "duplicate-zero-arc-research-network",
            "/fixation/network_ids",
            None,
            "zero-arc research fixation contains a duplicate network ID",
        ));
    }
    for network_id in &selected {
        if !networks
            .iter()
            .any(|network| input.networks[network.input_index].id() == *network_id)
        {
            return Err(missing_reference("selected model network", network_id));
        }
    }
    for network in networks
        .iter()
        .filter(|network| selected.contains(input.networks[network.input_index].id()))
    {
        for arc in &network.arcs {
            fix_value(solver, arc.selected, 0, tag);
            fix_value(solver, arc.flow, 0, tag);
        }
    }
    Ok(())
}

fn successful_reference(
    reference: Option<&IntegratedLayoutReport>,
) -> Result<&IntegratedLayoutReport, IntegratedLayoutDiagnostic> {
    reference.filter(|report| report.success).ok_or_else(|| {
        IntegratedLayoutDiagnostic::error(
            "missing-successful-research-reference",
            "/reference",
            None,
            "research fixation requires a successful validated reference layout",
        )
    })
}

fn fix_placements(
    solver: &mut RecordedModel,
    instances: &[ModelInstance],
    reference: &IntegratedLayoutReport,
    tag: ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    for instance in instances {
        let placement = reference
            .placements
            .iter()
            .find(|placement| placement.instance == instance.input.id)
            .ok_or_else(|| missing_reference("placement", &instance.input.id))?;
        let candidate = instance
            .candidates
            .iter()
            .find(|candidate| {
                candidate.rotation == placement.rotation
                    && i64::from(candidate.x) == placement.x
                    && i64::from(candidate.y) == placement.y
            })
            .ok_or_else(|| missing_reference("placement candidate", &instance.input.id))?;
        fix_value(solver, candidate.selected, 1, tag);
    }
    Ok(())
}

fn fix_network_terminals(
    solver: &mut RecordedModel,
    input: &ModelInput,
    instances: &[ModelInstance],
    model_network: &ModelNetwork,
    reference: &IntegratedLayoutReport,
    tag: ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let network_id = input.networks[model_network.input_index].id();
    let prior_network = reference_network(reference, network_id)?;
    let reference_placements = selected_reference_placements(instances, reference)?;
    for terminal in &model_network.terminals {
        let prior_terminal = prior_network
            .terminals
            .iter()
            .find(|candidate| candidate.id == terminal.id)
            .ok_or_else(|| missing_reference("network terminal", &terminal.id))?;
        if matches!(
            prior_terminal.endpoint,
            crate::layouts::integrated::TransportNetworkEndpoint::External { .. }
        ) {
            continue;
        }
        let option = terminal
            .options
            .iter()
            .find(|option| {
                option.endpoint == prior_terminal.endpoint
                    && world_position(option.cell, input.width) == prior_terminal.position
                    && reference_placements.contains(&option.placement)
            })
            .ok_or_else(|| missing_reference("terminal option", &terminal.id))?;
        fix_value(solver, option.selected, 1, tag);
    }
    Ok(())
}

fn selected_reference_placements(
    instances: &[ModelInstance],
    reference: &IntegratedLayoutReport,
) -> Result<BTreeSet<DomainId>, IntegratedLayoutDiagnostic> {
    let mut selected = BTreeSet::new();
    for instance in instances {
        let placement = reference
            .placements
            .iter()
            .find(|placement| placement.instance == instance.input.id)
            .ok_or_else(|| missing_reference("placement", &instance.input.id))?;
        let candidate = instance
            .candidates
            .iter()
            .find(|candidate| {
                candidate.rotation == placement.rotation
                    && i64::from(candidate.x) == placement.x
                    && i64::from(candidate.y) == placement.y
            })
            .ok_or_else(|| missing_reference("placement candidate", &instance.input.id))?;
        selected.insert(candidate.selected);
    }
    Ok(selected)
}

#[allow(clippy::too_many_arguments)]
fn fix_network_route(
    solver: &mut RecordedModel,
    input: &ModelInput,
    instances: &[ModelInstance],
    networks: &[ModelNetwork],
    branch_components: &[ModelBranchComponent],
    bridges: &[ModelBridge],
    reference: &IntegratedLayoutReport,
    network_id: &str,
    tag: ConstraintTag,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let model_network = networks
        .iter()
        .find(|network| input.networks[network.input_index].id() == network_id)
        .ok_or_else(|| missing_reference("selected model network", network_id))?;
    let network_input = &input.networks[model_network.input_index];
    let prior_network = reference_network(reference, network_id)?;
    fix_network_terminals(solver, input, instances, model_network, reference, tag)?;

    let prior_cells = prior_network
        .cells
        .iter()
        .filter_map(|position| position_cell(position, input))
        .collect::<BTreeSet<_>>();
    for (cell, variable) in model_network.route_cells.iter().enumerate() {
        fix_value(
            solver,
            *variable,
            i32::from(prior_cells.contains(&cell)),
            tag,
        );
    }

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
        let flow = prior_segments
            .get(&(arc.from, arc.to))
            .copied()
            .and_then(|rate| network_input.flow_units_for_hint(rate))
            .unwrap_or(0);
        fix_value(solver, arc.selected, i32::from(flow > 0), tag);
        fix_value(solver, arc.flow, flow, tag);
    }

    let prior_component_ids = prior_network
        .component_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    for component in branch_components
        .iter()
        .filter(|component| component.network_index == model_network.input_index)
    {
        let selected = reference.logistics_components.iter().any(|prior| {
            prior_component_ids.contains(prior.id.as_str())
                && prior.kind == component.kind
                && prior.transport == component.transport
                && prior.component == component.component
                && prior.position == world_position(component.cell, input.width)
                && prior.rotation == component.rotation
        });
        fix_value(solver, component.selected, i32::from(selected), tag);
    }

    let transport = network_input.transport();
    let same_transport_networks = networks
        .iter()
        .filter(|network| input.networks[network.input_index].transport() == transport)
        .count();
    if same_transport_networks == 1 {
        for bridge in bridges
            .iter()
            .filter(|bridge| bridge.transport == transport)
        {
            let prior_bridge = reference.logistics_components.iter().find(|prior| {
                prior.kind == LogisticsComponentKind::Bridge
                    && prior.transport == bridge.transport
                    && prior.component == bridge.component
                    && prior.position == world_position(bridge.cell, input.width)
            });
            fix_value(
                solver,
                bridge.selected,
                i32::from(prior_bridge.is_some()),
                tag,
            );
            for (rotation, variable) in &bridge.rotations {
                fix_value(
                    solver,
                    *variable,
                    i32::from(prior_bridge.is_some_and(|prior| prior.rotation == *rotation)),
                    tag,
                );
            }
        }
    }
    Ok(())
}

fn reference_network<'a>(
    reference: &'a IntegratedLayoutReport,
    network_id: &str,
) -> Result<&'a TransportNetwork, IntegratedLayoutDiagnostic> {
    reference
        .transport_networks
        .iter()
        .find(|network| network.id == network_id)
        .ok_or_else(|| missing_reference("transport network", network_id))
}

fn position_cell(position: &WorldGridPosition, input: &ModelInput) -> Option<usize> {
    let x = i32::try_from(position.x).ok()?;
    let y = i32::try_from(position.y).ok()?;
    if x < 0 || y < 0 || x >= input.width || y >= input.height {
        return None;
    }
    Some(usize::try_from(y.checked_mul(input.width)?.checked_add(x)?).ok()?)
}

fn fix_value(solver: &mut RecordedModel, variable: DomainId, value: i32, tag: ConstraintTag) {
    solver.post_equals(
        ConstraintFamily::ResearchFixation,
        vec![variable.scaled(1)],
        value,
        value.unsigned_abs() as u64,
        tag,
    );
}

fn missing_reference(kind: &str, entity: &str) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "research-reference-mismatch",
        "/reference",
        Some(entity.to_string()),
        format!("successful research reference has no matching {kind} for '{entity}'"),
    )
}
