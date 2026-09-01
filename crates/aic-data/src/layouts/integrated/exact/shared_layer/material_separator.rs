use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};
use serde::Serialize;

use super::*;

#[derive(Debug, Clone)]
pub(in crate::layouts::integrated) struct MaterialSeparatorRestriction {
    pub network_id: String,
    pub source_terminal: String,
    pub demand_terminal: String,
    pub source_cell: usize,
    pub source_continuation_cell: usize,
    pub demand_cell: usize,
    pub separator_after_row: usize,
    pub selected_case_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct MaterialSeparatorArcBuildCertificate {
    pub case_index: usize,
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
    pub selected_item_code: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct MaterialSeparatorBuildCertificate {
    pub network_id: String,
    pub network_index: usize,
    pub transport: TransportKind,
    pub item: String,
    pub selected_item_code: i32,
    pub source_terminal: String,
    pub source_flow_units: i32,
    pub source_cell: usize,
    pub source_continuation_cell: usize,
    pub demand_terminal: String,
    pub demand_flow_units: i32,
    pub demand_cell: usize,
    pub width: i32,
    pub height: i32,
    pub separator_after_row: usize,
    pub candidates: Vec<MaterialSeparatorArcBuildCertificate>,
    pub selected_case_index: Option<usize>,
    pub preceding_case_indices: Vec<usize>,
    pub posted_selected_unary_constraints: usize,
    pub posted_exclusion_clauses: usize,
}

pub(super) type MaterialSeparatorBuildCertificateCollector =
    SyncArc<Mutex<Vec<MaterialSeparatorBuildCertificate>>>;

#[derive(Debug, Clone)]
pub(super) struct MaterialSeparatorArcProbe {
    pub case_index: usize,
    pub from: usize,
    pub to: usize,
    pub route_selected: DomainId,
    pub flow: DomainId,
    pub from_item: DomainId,
    pub selected_item_code: i32,
}

#[derive(Debug, Clone)]
pub(super) struct MaterialSeparatorProbe {
    pub network_id: String,
    pub network_index: usize,
    pub transport: TransportKind,
    pub item: String,
    pub selected_item_code: i32,
    pub separator_after_row: usize,
    pub selected_case_index: Option<usize>,
    pub candidates: Vec<MaterialSeparatorArcProbe>,
}

pub(super) fn post_material_separator_restriction(
    solver: &mut RecordedModel,
    input: &ModelInput,
    terminals: &[Vec<SharedTerminal>],
    layers: &[SharedLayer],
    restriction: &MaterialSeparatorRestriction,
    certificates: Option<&MaterialSeparatorBuildCertificateCollector>,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<MaterialSeparatorProbe, IntegratedLayoutDiagnostic> {
    let invalid = |message: String| {
        IntegratedLayoutDiagnostic::error(
            "material-separator-restriction-invalid",
            "/material_separator_restriction",
            Some(restriction.network_id.clone()),
            message,
        )
    };
    let width = usize::try_from(input.width)
        .map_err(|_| invalid("grid width must be positive".to_string()))?;
    let height = usize::try_from(input.height)
        .map_err(|_| invalid("grid height must be positive".to_string()))?;
    if width == 0 || height < 2 || restriction.separator_after_row >= height.saturating_sub(1) {
        return Err(invalid(
            "separator row must have a complete row below it".to_string(),
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
            "selected network must contain exactly one source and one demand".to_string(),
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
            "separator requires one balanced positive source/demand pair".to_string(),
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
    let row = |cell: usize| cell / width;
    if restriction.source_cell >= width * height
        || restriction.source_continuation_cell >= width * height
        || restriction.demand_cell >= width * height
        || row(restriction.source_cell) > restriction.separator_after_row
        || row(restriction.source_continuation_cell) > restriction.separator_after_row
        || row(restriction.demand_cell) <= restriction.separator_after_row
    {
        return Err(invalid(
            "separator does not place the fixed source above and demand below the cut".to_string(),
        ));
    }

    let item_upper = i32::try_from(layer.network_indices.len())
        .map_err(|_| invalid("layer item count exceeds solver integer range".to_string()))?;
    let mut probes = Vec::with_capacity(width);
    let mut arc_certificates = Vec::with_capacity(width);
    for x in 0..width {
        let from = restriction.separator_after_row * width + x;
        let to = (restriction.separator_after_row + 1) * width + x;
        let arc = layer
            .arcs
            .iter()
            .find(|arc| arc.from == from && arc.to == to)
            .ok_or_else(|| {
                invalid(format!(
                    "south-directed separator arc {from} -> {to} is not modeled"
                ))
            })?;
        let from_direction = direction_index(direction_between(from, to, input.width));
        if from_direction != direction_index(CardinalDirection::South) {
            return Err(invalid(format!(
                "separator arc {from} -> {to} is not south-directed"
            )));
        }
        let from_item = layer.arm_items[from][from_direction];
        let route = solver.variable_descriptor(arc.selected);
        let flow = solver.variable_descriptor(arc.flow);
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
                "separator arc {from} -> {to} has unexpected declared domains"
            )));
        }
        let case_index = x;
        probes.push(MaterialSeparatorArcProbe {
            case_index,
            from,
            to,
            route_selected: arc.selected,
            flow: arc.flow,
            from_item,
            selected_item_code,
        });
        arc_certificates.push(MaterialSeparatorArcBuildCertificate {
            case_index,
            from,
            to,
            direction: "south".to_string(),
            route_selected_domain_id: arc.selected.id(),
            route_selected_family: route.family.name().to_string(),
            route_selected_name: route.name,
            route_selected_declared_lower_bound: route.declared_lower_bound,
            route_selected_declared_upper_bound: route.declared_upper_bound,
            route_selected_declared_cardinality: route.declared_cardinality,
            flow_domain_id: arc.flow.id(),
            flow_family: flow.family.name().to_string(),
            flow_name: flow.name,
            flow_declared_lower_bound: flow.declared_lower_bound,
            flow_declared_upper_bound: flow.declared_upper_bound,
            from_item_domain_id: from_item.id(),
            from_item_family: item.family.name().to_string(),
            from_item_name: item.name,
            from_item_declared_lower_bound: item.declared_lower_bound,
            from_item_declared_upper_bound: item.declared_upper_bound,
            selected_item_code,
        });
    }

    let selected_case_index = restriction.selected_case_index;
    if let Some(selected_index) = selected_case_index {
        let selected = probes.get(selected_index).ok_or_else(|| {
            invalid(format!(
                "selected separator case {selected_index} is outside the complete cut"
            ))
        })?;
        solver.post_equals(
            ConstraintFamily::MaterialSeparator,
            vec![selected.route_selected.scaled(1)],
            1,
            1,
            tag,
        );
        solver.post_equals(
            ConstraintFamily::MaterialSeparator,
            vec![selected.from_item.scaled(1)],
            selected_item_code,
            1,
            tag,
        );
        for earlier in probes.iter().take(selected_index) {
            solver.post_predicate_clause(
                ConstraintFamily::MaterialSeparator,
                &[earlier.route_selected, earlier.from_item],
                vec![
                    earlier.route_selected.equality_predicate(0),
                    earlier.from_item.disequality_predicate(selected_item_code),
                ],
                tag,
            );
        }
    }

    let (selected_unary_constraints, exclusion_clauses) = restriction_counts(selected_case_index);
    let certificate = MaterialSeparatorBuildCertificate {
        network_id: restriction.network_id.clone(),
        network_index,
        transport: network.transport(),
        item: network.item().to_string(),
        selected_item_code,
        source_terminal: source.id.clone(),
        source_flow_units: source.flow_units,
        source_cell: restriction.source_cell,
        source_continuation_cell: restriction.source_continuation_cell,
        demand_terminal: demand.id.clone(),
        demand_flow_units: demand.flow_units,
        demand_cell: restriction.demand_cell,
        width: input.width,
        height: input.height,
        separator_after_row: restriction.separator_after_row,
        candidates: arc_certificates,
        selected_case_index,
        preceding_case_indices: selected_case_index
            .map(|index| (0..index).collect())
            .unwrap_or_default(),
        posted_selected_unary_constraints: selected_unary_constraints,
        posted_exclusion_clauses: exclusion_clauses,
    };
    if let Some(certificates) = certificates {
        certificates
            .lock()
            .expect("material-separator certificate collector is not poisoned")
            .push(certificate);
    }
    Ok(MaterialSeparatorProbe {
        network_id: restriction.network_id.clone(),
        network_index,
        transport: network.transport(),
        item: network.item().to_string(),
        selected_item_code,
        separator_after_row: restriction.separator_after_row,
        selected_case_index,
        candidates: probes,
    })
}

fn restriction_counts(selected_case_index: Option<usize>) -> (usize, usize) {
    selected_case_index
        .map(|index| (2, index))
        .unwrap_or((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_prefix_constraint_counts_are_exact() {
        assert_eq!(restriction_counts(None), (0, 0));
        for selected in 0..16 {
            assert_eq!(restriction_counts(Some(selected)), (2, selected));
        }
    }
}
