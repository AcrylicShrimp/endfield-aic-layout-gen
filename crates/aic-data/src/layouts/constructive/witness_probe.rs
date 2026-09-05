//! Read-only diagnostic entry point for independently constructed physical witnesses.
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use super::{ConstructiveFrontierDiagnostic, ConstructiveNode, network};
use crate::facilities::{FacilityPortDirection, ValidatedFacilityCatalog};
use crate::layouts::{
    FacilityPlacement, FacilityPlacementBounds, PlacedLogisticsComponent, TransportNetwork,
    TransportNetworkEndpoint,
};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::{Rate, ValidatedRecipeBook};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WitnessProbeInput {
    pub schema_version: u32,
    pub placements: Vec<FacilityPlacement>,
    pub utilizations: BTreeMap<String, Rate>,
    pub transport_networks: Vec<TransportNetwork>,
    pub logistics_components: Vec<PlacedLogisticsComponent>,
}

#[derive(Debug, Serialize)]
pub struct WitnessRecipeTerminal {
    pub instance: String,
    pub direction: FacilityPortDirection,
    pub item: String,
    pub required_rate: Rate,
    pub routed_rate: Rate,
    pub pending_rate: Rate,
}

#[derive(Debug, Serialize)]
pub struct WitnessProbeReport {
    pub schema_version: u32,
    pub status: &'static str,
    pub covered_geometry_validated: bool,
    pub recipe_coverage_complete: bool,
    pub factory_validated: bool,
    pub bounds: Option<FacilityPlacementBounds>,
    pub recipe_terminals: Vec<WitnessRecipeTerminal>,
    pub diagnostics: Vec<ConstructiveFrontierDiagnostic>,
    pub elapsed_ms: f64,
}

fn error(entity: &str, message: &str) -> ConstructiveFrontierDiagnostic {
    ConstructiveFrontierDiagnostic::error(
        "witness-probe-invalid",
        "/witness",
        Some(entity.into()),
        message,
    )
}

fn inspect(
    input: &WitnessProbeInput,
    facilities: &ValidatedFacilityCatalog,
    recipes: &ValidatedRecipeBook,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
) -> Result<(FacilityPlacementBounds, Vec<WitnessRecipeTerminal>), ConstructiveFrontierDiagnostic> {
    let members = input
        .placements
        .iter()
        .map(|p| p.instance.clone())
        .collect::<BTreeSet<_>>();
    if input.schema_version != 1
        || input.placements.is_empty()
        || input.placements.len() > 256
        || input.transport_networks.len() > 1024
        || input.logistics_components.len() > 10000
        || members.len() != input.placements.len()
        || members != input.utilizations.keys().cloned().collect()
    {
        return Err(error(
            "input",
            "Unsupported schema, duplicate membership or diagnostic count bound",
        ));
    }
    let valid_rate = |r: Rate| r.numerator > 0 && r.denominator > 0;
    let arithmetic = |_| error("rate", "Invalid or overflowing exact rate arithmetic");
    let mut expected = BTreeMap::new();
    let mut width = 0;
    let mut height = 0;
    for p in &input.placements {
        if p.x < 0
            || p.y < 0
            || p.x > 1_000_000
            || p.y > 1_000_000
            || p.width <= 0
            || p.height <= 0
            || p.width > 1000
            || p.height > 1000
            || p.width * p.height > 10000
        {
            return Err(error(
                &p.instance,
                "Placement exceeds diagnostic coordinate/materialization guard",
            ));
        }
        width = width.max(p.x + p.width);
        height = height.max(p.y + p.height);
        let r = recipes
            .recipe_book()
            .recipes
            .iter()
            .find(|r| r.id == p.recipe)
            .ok_or_else(|| error(&p.instance, "Unknown recipe"))?;
        let u = input.utilizations[&p.instance];
        if r.facility != p.facility || !valid_rate(u) || u.numerator > u.denominator {
            return Err(error(
                &p.instance,
                "Recipe/facility mismatch or utilization outside (0,1]",
            ));
        }
        for (direction, amounts) in [
            (FacilityPortDirection::Input, &r.inputs),
            (FacilityPortDirection::Output, &r.outputs),
        ] {
            for amount in amounts {
                let key = (p.instance.clone(), direction, amount.item.clone());
                let rate = Rate::from_quantity_per_duration_ms(amount.quantity, r.duration_ms)
                    .map_err(arithmetic)?
                    .checked_mul_i64(u.numerator)
                    .map_err(arithmetic)?
                    .checked_div_i64(u.denominator)
                    .map_err(arithmetic)?;
                let previous = expected.get(&key).copied().unwrap_or(Rate::zero());
                expected.insert(key, previous.checked_add(rate).map_err(arithmetic)?);
            }
        }
    }
    let mut routed = BTreeMap::new();
    let mut count = 0usize;
    let mut network_ids = BTreeSet::new();
    let mut terminal_ids = BTreeSet::new();
    let mut requirements = BTreeSet::new();
    for n in &input.transport_networks {
        count = count
            .saturating_add(n.cells.len())
            .saturating_add(n.segments.len())
            .saturating_add(n.terminals.len());
        if count > 200000
            || !network_ids.insert(&n.id)
            || n.requirement_ids.is_empty()
            || n.requirement_ids
                .iter()
                .any(|id| !requirements.insert(id.clone()))
        {
            return Err(error(
                &n.id,
                "Duplicate identities, empty requirement set or diagnostic geometry budget",
            ));
        }
        if items.item(&n.item).map(|i| i.transport) != Some(n.transport) {
            return Err(error(&n.id, "Unknown item or incompatible transport"));
        }
        for p in n
            .cells
            .iter()
            .chain(n.segments.iter().flat_map(|s| [&s.from, &s.to]))
            .chain(n.terminals.iter().map(|t| &t.position))
        {
            if p.x < 0 || p.y < 0 || p.x > 1_000_000 || p.y > 1_000_000 {
                return Err(error(&n.id, "Network exceeds diagnostic coordinate guard"));
            }
            width = width.max(p.x + 1);
            height = height.max(p.y + 1);
        }
        if n.segments.iter().any(|s| !valid_rate(s.rate)) {
            return Err(error(&n.id, "Nonpositive segment rate"));
        }
        for t in &n.terminals {
            if !valid_rate(t.rate) || !terminal_ids.insert(&t.id) {
                return Err(error(&t.id, "Nonpositive terminal rate or duplicate ID"));
            }
            if let TransportNetworkEndpoint::Facility { instance, .. } = &t.endpoint {
                let key = (instance.clone(), t.direction, n.item.clone());
                if !expected.contains_key(&key) {
                    return Err(error(
                        &t.id,
                        "Terminal commodity/direction is absent from its recipe",
                    ));
                }
                let previous = routed.get(&key).copied().unwrap_or(Rate::zero());
                routed.insert(key, previous.checked_add(t.rate).map_err(arithmetic)?);
            }
        }
    }
    let mut ledger = Vec::new();
    for ((instance, direction, item), required_rate) in expected {
        let routed_rate = routed
            .get(&(instance.clone(), direction, item.clone()))
            .copied()
            .unwrap_or(Rate::zero());
        if routed_rate > required_rate {
            return Err(error(&instance, "Routed flow exceeds recipe utilization"));
        }
        ledger.push(WitnessRecipeTerminal {
            instance,
            direction,
            item,
            required_rate,
            routed_rate,
            pending_rate: required_rate.checked_sub(routed_rate).map_err(arithmetic)?,
        });
    }
    let bounds = FacilityPlacementBounds { width, height };
    let node = ConstructiveNode {
        id: "independent-witness".into(),
        bounds: bounds.clone(),
        member_instances: members.into_iter().collect(),
        internal_requirements: requirements.into_iter().collect(),
        external_requirements: vec![],
        placements: input.placements.clone(),
        transport_networks: input.transport_networks.clone(),
        logistics_components: input.logistics_components.clone(),
        network_capacity: vec![],
        attachment_events: vec![],
        boundary_requirements: vec![],
    };
    network::validate(&node, facilities, transports, components)?;
    Ok((bounds, ledger))
}

pub fn probe_constructive_witness(
    input: &WitnessProbeInput,
    facilities: &ValidatedFacilityCatalog,
    recipes: &ValidatedRecipeBook,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    components: &ValidatedLogisticsComponentCatalog,
) -> WitnessProbeReport {
    let start = Instant::now();
    let result = inspect(input, facilities, recipes, items, transports, components);
    let (bounds, recipe_terminals, diagnostics) = match result {
        Ok((bounds, ledger)) => (Some(bounds), ledger, vec![]),
        Err(e) => (None, vec![], vec![e]),
    };
    WitnessProbeReport {
        schema_version: 1,
        status: if diagnostics.is_empty() {
            "covered-witness-valid"
        } else {
            "witness-rejected"
        },
        covered_geometry_validated: diagnostics.is_empty(),
        recipe_coverage_complete: diagnostics.is_empty()
            && recipe_terminals.iter().all(|t| t.pending_rate.is_zero()),
        factory_validated: false,
        bounds,
        recipe_terminals,
        diagnostics,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    }
}
