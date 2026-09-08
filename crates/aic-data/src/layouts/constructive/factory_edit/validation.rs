//! Original request ledger around the existing independent production geometry validator.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::layouts::TransportNetworkEndpoint;

// Operational materialization guards, inherited from the production witness probe.
const MAX_INSTANCES: usize = 256;
const MAX_TERMINALS: usize = 8192;
const MAX_GEOMETRY: usize = 200_000;
const MAX_SPAN: i128 = 1_000_000;

type Key = (FactoryEditEndpoint, FacilityPortDirection, String);
type Ledger = BTreeMap<Key, Rate>;

fn positive(rate: Rate) -> bool {
    rate.numerator > 0 && rate.denominator > 0
}

fn add(ledger: &mut Ledger, key: Key, rate: Rate) -> Result<()> {
    let value = ledger.entry(key).or_insert(Rate::zero());
    *value = value.checked_add(rate).map_err(|_| {
        error(
            "factory.rate-overflow",
            "terminals",
            "Exact terminal rate sum overflow",
        )
    })?;
    Ok(())
}

fn equal(a: &Ledger, b: &Ledger) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|((ka, va), (kb, vb))| ka == kb && va.cmp(vb).is_eq())
}

pub(super) fn request(request: &FactoryEditRequest, catalogs: &FactoryEditCatalogs) -> Result<()> {
    if request.schema_version != 1
        || request.instances.is_empty()
        || request.instances.len() > MAX_INSTANCES
        || request.terminals.len() > MAX_TERMINALS
    {
        return Err(error(
            "factory.request-schema",
            "request",
            "Unsupported schema or request materialization guard",
        ));
    }
    let mut ids = BTreeSet::new();
    let mut recipes = Ledger::new();
    for instance in &request.instances {
        let id = &instance.instance;
        if id.is_empty()
            || !ids.insert(id)
            || !positive(instance.utilization)
            || instance.utilization.numerator > instance.utilization.denominator
        {
            return Err(error(
                "factory.request-instance",
                id,
                "Duplicate/empty instance or utilization outside (0,1]",
            ));
        }
        let recipe = catalogs
            .recipes
            .recipe_book()
            .recipes
            .iter()
            .find(|r| r.id == instance.recipe)
            .ok_or_else(|| error("factory.request-recipe", id, "Unknown requested recipe"))?;
        if catalogs.facilities.facility(&recipe.facility).is_none() {
            return Err(error(
                "factory.request-recipe",
                id,
                "Requested recipe has no loaded facility definition",
            ));
        }
        for (direction, entries) in [
            (FacilityPortDirection::Input, &recipe.inputs),
            (FacilityPortDirection::Output, &recipe.outputs),
        ] {
            for amount in entries {
                let u = instance.utilization;
                let rate = Rate::from_quantity_per_duration_ms(amount.quantity, recipe.duration_ms)
                    .and_then(|r| r.checked_mul_i64(u.numerator))
                    .and_then(|r| r.checked_div_i64(u.denominator))
                    .map_err(|_| {
                        error(
                            "factory.rate-overflow",
                            id,
                            "Recipe obligation arithmetic overflow",
                        )
                    })?;
                add(
                    &mut recipes,
                    (
                        FactoryEditEndpoint::Facility {
                            instance: id.clone(),
                        },
                        direction,
                        amount.item.clone(),
                    ),
                    rate,
                )?;
            }
        }
    }
    let mut declared = Ledger::new();
    let mut unique = BTreeSet::new();
    for terminal in &request.terminals {
        let key = (
            terminal.endpoint.clone(),
            terminal.direction,
            terminal.item.clone(),
        );
        if !positive(terminal.rate)
            || !unique.insert(key.clone())
            || catalogs.items.item(&terminal.item).is_none()
        {
            return Err(error(
                "factory.request-terminal",
                &terminal.item,
                "Unknown item, duplicate terminal obligation or nonpositive rate",
            ));
        }
        match &terminal.endpoint {
            FactoryEditEndpoint::Facility { instance } => {
                if !ids.contains(instance) {
                    return Err(error(
                        "factory.request-terminal",
                        instance,
                        "Unknown facility endpoint",
                    ));
                }
                add(&mut declared, key, terminal.rate)?;
            }
            FactoryEditEndpoint::External { node } => {
                if node.is_empty() || ids.contains(node) {
                    return Err(error(
                        "factory.request-terminal",
                        node,
                        "Empty external node or identity shared with a facility",
                    ));
                }
            }
        }
    }
    if !equal(&recipes, &declared) {
        return Err(error(
            "factory.request-recipe-coverage",
            "request",
            "Requested facility terminal rates do not exactly match recipes and utilizations",
        ));
    }
    Ok(())
}

fn canonical(
    input: &WitnessProbeInput,
    catalogs: &FactoryEditCatalogs,
) -> Result<WitnessProbeInput> {
    if input.schema_version != 1
        || input.placements.is_empty()
        || input.placements.len() > MAX_INSTANCES
        || input.transport_networks.len() > 1024
        || input.logistics_components.len() > 10000
    {
        return Err(error(
            "factory.geometry-guard",
            "layout",
            "Unsupported physical schema or materialization count guard",
        ));
    }
    let geometry = input.transport_networks.iter().fold(0usize, |total, n| {
        total
            .saturating_add(n.cells.len())
            .saturating_add(n.segments.len())
            .saturating_add(n.terminals.len())
    });
    if geometry > MAX_GEOMETRY {
        return Err(error(
            "factory.geometry-guard",
            "layout",
            "Network geometry exceeds the materialization work guard",
        ));
    }
    let mut min_x = i128::MAX;
    let mut min_y = i128::MAX;
    let mut max_x = i128::MIN;
    let mut max_y = i128::MIN;
    let mut include = |x: i128, y: i128| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    };
    for p in &input.placements {
        if p.width <= 0
            || p.height <= 0
            || p.width > 1000
            || p.height > 1000
            || p.width * p.height > 10000
        {
            return Err(error(
                "factory.geometry-guard",
                &p.instance,
                "Facility footprint exceeds the materialization work guard",
            ));
        }
        include(p.x.into(), p.y.into());
        include(
            i128::from(p.x) + i128::from(p.width) - 1,
            i128::from(p.y) + i128::from(p.height) - 1,
        );
    }
    for n in &input.transport_networks {
        for p in n
            .cells
            .iter()
            .chain(n.segments.iter().flat_map(|s| [&s.from, &s.to]))
            .chain(n.terminals.iter().map(|t| &t.position))
        {
            include(p.x.into(), p.y.into());
        }
    }
    for c in &input.logistics_components {
        let definition = catalogs
            .components
            .component(&c.component)
            .ok_or_else(|| error("factory.component", &c.id, "Unknown logistics component"))?;
        if definition.footprint.width != 1 || definition.footprint.height != 1 {
            return Err(error(
                "factory.component-footprint-unsupported",
                &c.id,
                "The current production network validator supports single-cell logistics components",
            ));
        }
        include(c.position.x.into(), c.position.y.into());
    }
    if max_x - min_x > MAX_SPAN || max_y - min_y > MAX_SPAN {
        return Err(error(
            "factory.geometry-guard",
            "layout",
            "Used span exceeds the coordinate arithmetic/materialization guard; not a game canvas limit",
        ));
    }
    let mut result = input.clone();
    let shift = |p: &mut crate::layouts::WorldGridPosition| {
        p.x = (i128::from(p.x) - min_x) as i64;
        p.y = (i128::from(p.y) - min_y) as i64;
    };
    for p in &mut result.placements {
        p.x = (i128::from(p.x) - min_x) as i64;
        p.y = (i128::from(p.y) - min_y) as i64;
    }
    for n in &mut result.transport_networks {
        for p in &mut n.cells {
            shift(p);
        }
        for s in &mut n.segments {
            shift(&mut s.from);
            shift(&mut s.to);
        }
        for t in &mut n.terminals {
            shift(&mut t.position);
        }
    }
    for c in &mut result.logistics_components {
        shift(&mut c.position);
    }
    Ok(result)
}

pub(super) fn accept(
    request: &FactoryEditRequest,
    input: &WitnessProbeInput,
    catalogs: &FactoryEditCatalogs,
) -> Result<(WitnessProbeInput, FactoryLayoutMetrics)> {
    if input.placements.len() != request.instances.len()
        || input.utilizations.len() != request.instances.len()
    {
        return Err(error(
            "factory.inventory",
            "layout",
            "Candidate inventory differs from the original request",
        ));
    }
    for instance in &request.instances {
        let Some(p) = input
            .placements
            .iter()
            .find(|p| p.instance == instance.instance)
        else {
            return Err(error(
                "factory.inventory",
                &instance.instance,
                "Missing original facility instance",
            ));
        };
        let Some(u) = input.utilizations.get(&instance.instance) else {
            return Err(error(
                "factory.inventory",
                &instance.instance,
                "Missing original utilization",
            ));
        };
        if p.recipe != instance.recipe || !positive(*u) || !u.cmp(&instance.utilization).is_eq() {
            return Err(error(
                "factory.inventory",
                &instance.instance,
                "Recipe or utilization differs from the original request",
            ));
        }
    }
    let input = canonical(input, catalogs)?;
    let probe = super::super::probe_constructive_witness(
        &input,
        &catalogs.facilities,
        &catalogs.recipes,
        &catalogs.items,
        &catalogs.transports,
        &catalogs.components,
    );
    if !probe.covered_geometry_validated {
        let diagnostic = &probe.diagnostics[0];
        return Err(error(
            "factory.geometry-invalid",
            diagnostic.entity.clone().unwrap_or_else(|| "layout".into()),
            format!("{}: {}", diagnostic.code, diagnostic.message),
        ));
    }
    if !probe.recipe_coverage_complete {
        return Err(error(
            "factory.recipe-coverage",
            "layout",
            "Candidate leaves pending original recipe obligations",
        ));
    }
    let mut expected = Ledger::new();
    let mut actual = Ledger::new();
    for t in &request.terminals {
        add(
            &mut expected,
            (t.endpoint.clone(), t.direction, t.item.clone()),
            t.rate,
        )?;
    }
    for n in &input.transport_networks {
        for t in &n.terminals {
            let endpoint = match &t.endpoint {
                TransportNetworkEndpoint::Facility { instance, .. } => {
                    FactoryEditEndpoint::Facility {
                        instance: instance.clone(),
                    }
                }
                TransportNetworkEndpoint::External { node, .. } => {
                    FactoryEditEndpoint::External { node: node.clone() }
                }
            };
            add(&mut actual, (endpoint, t.direction, n.item.clone()), t.rate)?;
        }
    }
    if !equal(&expected, &actual) {
        return Err(error(
            "factory.original-terminals",
            "layout",
            "Candidate terminal commodity, direction, external identity or rate differs from the original request",
        ));
    }
    let metrics = metrics::measure(&input, catalogs)?;
    Ok((input, metrics))
}
