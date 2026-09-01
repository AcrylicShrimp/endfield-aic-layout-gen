use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};
use serde::Serialize;

use super::*;

#[derive(Debug, Clone)]
pub(in crate::layouts::integrated) struct MaterialJunctionRestriction {
    pub network_id: String,
    pub source_terminal: String,
    pub demand_terminal: String,
    pub source_cell: usize,
    pub demand_cell: usize,
    pub incoming: DirectedGridArcRestriction,
    pub junction_cell: usize,
    pub candidates: Vec<DirectedGridArcRestriction>,
    pub selected_case_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct MaterialJunctionArcBuildCertificate {
    pub case_index: Option<usize>,
    pub from: usize,
    pub to: usize,
    pub direction: String,
    pub route_selected_domain_id: u32,
    pub route_selected_family: String,
    pub route_selected_name: String,
    pub route_selected_declared_lower_bound: i32,
    pub route_selected_declared_upper_bound: i32,
    pub route_selected_declared_cardinality: u64,
    pub flow_domain_id: u32,
    pub flow_family: String,
    pub flow_name: String,
    pub flow_declared_lower_bound: i32,
    pub flow_declared_upper_bound: i32,
    pub from_item_domain_id: u32,
    pub from_item_family: String,
    pub from_item_name: String,
    pub from_item_declared_lower_bound: i32,
    pub from_item_declared_upper_bound: i32,
    pub from_item_declared_cardinality: u64,
    pub selected_item_code: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct MaterialJunctionBuildCertificate {
    pub network_id: String,
    pub network_index: usize,
    pub transport: TransportKind,
    pub item: String,
    pub selected_item_code: i32,
    pub source_terminal: String,
    pub source_flow_units: i32,
    pub source_cell: usize,
    pub demand_terminal: String,
    pub demand_flow_units: i32,
    pub demand_cell: usize,
    pub network_terminal_count: usize,
    pub width: i32,
    pub height: i32,
    pub junction_cell: usize,
    pub junction_is_west_boundary: bool,
    pub junction_is_not_selected_terminal: bool,
    pub actual_outgoing_cells: Vec<usize>,
    pub incoming: MaterialJunctionArcBuildCertificate,
    pub candidates: Vec<MaterialJunctionArcBuildCertificate>,
    pub selected_case_index: Option<usize>,
    pub preceding_case_indices: Vec<usize>,
    pub posted_selected_unary_constraints: usize,
    pub posted_exclusion_clauses: usize,
}

pub(super) type MaterialJunctionBuildCertificateCollector =
    SyncArc<Mutex<Vec<MaterialJunctionBuildCertificate>>>;

#[derive(Debug, Clone)]
pub(super) struct MaterialJunctionArcProbe {
    pub case_index: Option<usize>,
    pub from: usize,
    pub to: usize,
    pub direction: String,
    pub route_selected: DomainId,
    pub flow: DomainId,
    pub from_item: DomainId,
    pub selected_item_code: i32,
}

#[derive(Debug, Clone)]
pub(super) struct MaterialJunctionProbe {
    pub network_id: String,
    pub network_index: usize,
    pub transport: TransportKind,
    pub item: String,
    pub selected_item_code: i32,
    pub junction_cell: usize,
    pub selected_case_index: Option<usize>,
    pub incoming: MaterialJunctionArcProbe,
    pub candidates: Vec<MaterialJunctionArcProbe>,
}

pub(super) fn post_material_junction_restriction(
    solver: &mut RecordedModel,
    input: &ModelInput,
    terminals: &[Vec<SharedTerminal>],
    layers: &[SharedLayer],
    restriction: &MaterialJunctionRestriction,
    certificates: Option<&MaterialJunctionBuildCertificateCollector>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<MaterialJunctionProbe, IntegratedLayoutDiagnostic> {
    let invalid = |message: String| {
        IntegratedLayoutDiagnostic::error(
            "material-junction-restriction-invalid",
            "/material_junction_restriction",
            Some(restriction.network_id.clone()),
            message,
        )
    };
    let width = usize::try_from(input.width)
        .map_err(|_| invalid("grid width must be positive".to_string()))?;
    let height = usize::try_from(input.height)
        .map_err(|_| invalid("grid height must be positive".to_string()))?;
    let cell_count = width
        .checked_mul(height)
        .ok_or_else(|| invalid("grid area exceeds addressable memory".to_string()))?;
    if width == 0
        || height == 0
        || restriction.junction_cell >= cell_count
        || restriction.source_cell >= cell_count
        || restriction.demand_cell >= cell_count
    {
        return Err(invalid(
            "junction cells must be inside the grid".to_string(),
        ));
    }
    if restriction.candidates.is_empty() {
        return Err(invalid(
            "material junction requires at least one continuation candidate".to_string(),
        ));
    }

    let network_index = input
        .networks
        .iter()
        .position(|network| network.id() == restriction.network_id)
        .ok_or_else(|| invalid("selected network does not exist".to_string()))?;
    let network = &input.networks[network_index];
    let layer = layers
        .iter()
        .find(|layer| {
            layer.transport == network.transport() && layer.network_indices.contains(&network_index)
        })
        .ok_or_else(|| invalid("selected network has no shared transport layer".to_string()))?;
    let selected_item_code = *layer
        .item_codes
        .get(&network_index)
        .ok_or_else(|| invalid("selected network has no layer-local item code".to_string()))?;
    let network_terminals = &terminals[network_index];
    if network_terminals.len() != 2 {
        return Err(invalid(
            "material junction requires exactly one source and one demand".to_string(),
        ));
    }
    let source = network_terminals
        .iter()
        .find(|terminal| terminal.id == restriction.source_terminal)
        .ok_or_else(|| invalid("selected source terminal does not exist".to_string()))?;
    let demand = network_terminals
        .iter()
        .find(|terminal| terminal.id == restriction.demand_terminal)
        .ok_or_else(|| invalid("selected demand terminal does not exist".to_string()))?;
    if source.direction != FacilityPortDirection::Output
        || demand.direction != FacilityPortDirection::Input
        || source.flow_units <= 0
        || source.flow_units != demand.flow_units
    {
        return Err(invalid(
            "material junction requires one balanced positive source/demand pair".to_string(),
        ));
    }
    if !source
        .routing_options
        .iter()
        .any(|option| option.cell == restriction.source_cell)
        || !demand
            .routing_options
            .iter()
            .any(|option| option.cell == restriction.demand_cell)
    {
        return Err(invalid(
            "requested source or demand cell is not a legal terminal option".to_string(),
        ));
    }
    if restriction.source_cell == restriction.junction_cell
        || restriction.demand_cell == restriction.junction_cell
    {
        return Err(invalid(
            "junction cell must not be the selected source or demand cell".to_string(),
        ));
    }
    if restriction.incoming.to != restriction.junction_cell {
        return Err(invalid(
            "incoming arc must terminate at the material junction".to_string(),
        ));
    }
    if restriction
        .candidates
        .iter()
        .any(|candidate| candidate.from != restriction.junction_cell)
    {
        return Err(invalid(
            "every continuation candidate must originate at the junction".to_string(),
        ));
    }
    let unique_candidates = restriction
        .candidates
        .iter()
        .map(|candidate| (candidate.from, candidate.to))
        .collect::<BTreeSet<_>>();
    if unique_candidates.len() != restriction.candidates.len() {
        return Err(invalid(
            "material continuation candidates must be distinct".to_string(),
        ));
    }

    let item_upper = i32::try_from(layer.network_indices.len())
        .map_err(|_| invalid("layer item count exceeds solver integer range".to_string()))?;
    let resolve = |arc: &DirectedGridArcRestriction, case_index: Option<usize>| {
        let modeled = layer
            .arcs
            .iter()
            .find(|modeled| modeled.from == arc.from && modeled.to == arc.to)
            .ok_or_else(|| {
                invalid(format!(
                    "material junction arc {} -> {} is not modeled",
                    arc.from, arc.to
                ))
            })?;
        let direction = direction_between(arc.from, arc.to, input.width);
        let from_direction = direction_index(direction);
        let from_item = layer.arm_items[arc.from][from_direction];
        let route = solver.variable_descriptor(modeled.selected);
        let flow = solver.variable_descriptor(modeled.flow);
        let item = solver.variable_descriptor(from_item);
        if route.family != VariableFamily::RouteArc
            || route.declared_lower_bound != 0
            || route.declared_upper_bound != 1
            || route.declared_cardinality != 2
            || flow.family != VariableFamily::Flow
            || flow.declared_lower_bound != 0
            || flow.declared_upper_bound != layer.maximum_capacity
            || flow.declared_cardinality
                != u64::try_from(layer.maximum_capacity + 1)
                    .expect("positive maximum capacity fits u64")
            || item.family != VariableFamily::ArmItem
            || item.declared_lower_bound != 0
            || item.declared_upper_bound != item_upper
            || item.declared_cardinality
                != u64::try_from(item_upper + 1).expect("non-negative item count fits u64")
        {
            return Err(invalid(format!(
                "material junction arc {} -> {} has unexpected declared domains",
                arc.from, arc.to
            )));
        }
        let direction = direction_name(from_direction).to_string();
        Ok((
            MaterialJunctionArcProbe {
                case_index,
                from: arc.from,
                to: arc.to,
                direction: direction.clone(),
                route_selected: modeled.selected,
                flow: modeled.flow,
                from_item,
                selected_item_code,
            },
            MaterialJunctionArcBuildCertificate {
                case_index,
                from: arc.from,
                to: arc.to,
                direction,
                route_selected_domain_id: modeled.selected.id(),
                route_selected_family: route.family.name().to_string(),
                route_selected_name: route.name,
                route_selected_declared_lower_bound: route.declared_lower_bound,
                route_selected_declared_upper_bound: route.declared_upper_bound,
                route_selected_declared_cardinality: route.declared_cardinality,
                flow_domain_id: modeled.flow.id(),
                flow_family: flow.family.name().to_string(),
                flow_name: flow.name,
                flow_declared_lower_bound: flow.declared_lower_bound,
                flow_declared_upper_bound: flow.declared_upper_bound,
                from_item_domain_id: from_item.id(),
                from_item_family: item.family.name().to_string(),
                from_item_name: item.name,
                from_item_declared_lower_bound: item.declared_lower_bound,
                from_item_declared_upper_bound: item.declared_upper_bound,
                from_item_declared_cardinality: item.declared_cardinality,
                selected_item_code,
            },
        ))
    };

    let (incoming_probe, incoming_certificate) = resolve(&restriction.incoming, None)?;
    let mut probes = Vec::with_capacity(restriction.candidates.len());
    let mut candidate_certificates = Vec::with_capacity(restriction.candidates.len());
    for (case_index, candidate) in restriction.candidates.iter().enumerate() {
        let (probe, certificate) = resolve(candidate, Some(case_index))?;
        probes.push(probe);
        candidate_certificates.push(certificate);
    }

    if let Some(selected_index) = restriction.selected_case_index {
        let selected = probes.get(selected_index).ok_or_else(|| {
            invalid(format!(
                "selected material junction case {selected_index} is outside the candidate set"
            ))
        })?;
        solver.post_equals(
            ConstraintFamily::MaterialJunction,
            vec![selected.route_selected.scaled(1)],
            1,
            1,
            tag,
        );
        solver.post_equals(
            ConstraintFamily::MaterialJunction,
            vec![selected.from_item.scaled(1)],
            selected_item_code,
            1,
            tag,
        );
        for earlier in probes.iter().take(selected_index) {
            solver.post_predicate_clause(
                ConstraintFamily::MaterialJunction,
                &[earlier.route_selected, earlier.from_item],
                vec![
                    earlier.route_selected.equality_predicate(0),
                    earlier.from_item.disequality_predicate(selected_item_code),
                ],
                tag,
            );
        }
    }

    let (posted_selected_unary_constraints, posted_exclusion_clauses) = restriction
        .selected_case_index
        .map(|index| (2, index))
        .unwrap_or((0, 0));
    let mut actual_outgoing_cells = layer
        .arcs
        .iter()
        .filter_map(|arc| (arc.from == restriction.junction_cell).then_some(arc.to))
        .collect::<Vec<_>>();
    actual_outgoing_cells.sort_unstable();
    if let Some(certificates) = certificates {
        certificates
            .lock()
            .expect("material-junction certificate collector is not poisoned")
            .push(MaterialJunctionBuildCertificate {
                network_id: restriction.network_id.clone(),
                network_index,
                transport: network.transport(),
                item: network.item().to_string(),
                selected_item_code,
                source_terminal: source.id.clone(),
                source_flow_units: source.flow_units,
                source_cell: restriction.source_cell,
                demand_terminal: demand.id.clone(),
                demand_flow_units: demand.flow_units,
                demand_cell: restriction.demand_cell,
                network_terminal_count: network_terminals.len(),
                width: input.width,
                height: input.height,
                junction_cell: restriction.junction_cell,
                junction_is_west_boundary: restriction.junction_cell % width == 0,
                junction_is_not_selected_terminal: restriction.source_cell
                    != restriction.junction_cell
                    && restriction.demand_cell != restriction.junction_cell,
                actual_outgoing_cells,
                incoming: incoming_certificate,
                candidates: candidate_certificates,
                selected_case_index: restriction.selected_case_index,
                preceding_case_indices: restriction
                    .selected_case_index
                    .map(|index| (0..index).collect())
                    .unwrap_or_default(),
                posted_selected_unary_constraints,
                posted_exclusion_clauses,
            });
    }

    Ok(MaterialJunctionProbe {
        network_id: restriction.network_id.clone(),
        network_index,
        transport: network.transport(),
        item: network.item().to_string(),
        selected_item_code,
        junction_cell: restriction.junction_cell,
        selected_case_index: restriction.selected_case_index,
        incoming: incoming_probe,
        candidates: probes,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn canonical_two_way_counts_match_the_contract() {
        let count = |selected: Option<usize>| selected.map(|index| (2, index)).unwrap_or((0, 0));
        assert_eq!(count(None), (0, 0));
        assert_eq!(count(Some(0)), (2, 0));
        assert_eq!(count(Some(1)), (2, 1));
    }
}
