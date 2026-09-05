//! Exact wiring coverage and terminal-rate audit, independent of construction choices.
use super::*;

pub(super) fn validate_coverage(
    node: &ConstructiveNode,
    wiring: &FacilityInstanceWiringReport,
    items: &ValidatedItemCatalog,
) -> Result<(), ConstructiveFrontierDiagnostic> {
    let routed = node
        .internal_requirements
        .iter()
        .chain(&node.external_requirements)
        .collect::<BTreeSet<_>>();
    if routed.len() != node.internal_requirements.len() + node.external_requirements.len() {
        return Err(failure(
            "production-coverage-invalid",
            &node.id,
            "duplicate resolved requirement",
        ));
    }
    let mut expected = BTreeMap::<(String, String, TransportKind, u8), Rate>::new();
    let mut actual = BTreeMap::<(String, String, TransportKind, u8), Rate>::new();
    for id in &routed {
        let Some(edge) = wiring.edges.iter().find(|e| &e.id == *id) else {
            return Err(failure(
                "production-coverage-invalid",
                id,
                "unknown resolved requirement",
            ));
        };
        let Some(item) = items.item(&edge.item) else {
            return Err(failure("production-coverage-invalid", id, "unknown item"));
        };
        for (endpoint, direction) in [(&edge.source, 0), (&edge.target, 1)] {
            let key = (
                endpoint.clone(),
                edge.item.clone(),
                item.transport,
                direction,
            );
            let rate = expected.entry(key).or_insert(Rate::zero());
            *rate = rate
                .checked_add(edge.rate)
                .map_err(|_| failure("production-coverage-invalid", id, "rate overflow"))?;
        }
        let internal = node.member_instances.contains(&edge.source)
            && node.member_instances.contains(&edge.target);
        if internal != node.internal_requirements.contains(&edge.id) {
            return Err(failure(
                "production-coverage-invalid",
                id,
                "wrong internal/external classification",
            ));
        }
    }
    for edge in &wiring.edges {
        if node.member_instances.contains(&edge.source)
            && node.member_instances.contains(&edge.target)
            && !routed.contains(&edge.id)
        {
            return Err(failure(
                "production-coverage-invalid",
                &edge.id,
                "unrouted induced internal edge",
            ));
        }
    }
    for n in &node.transport_networks {
        for t in &n.terminals {
            let key = (
                t.node.clone(),
                n.item.clone(),
                n.transport,
                u8::from(t.direction == FacilityPortDirection::Input),
            );
            let rate = actual.entry(key).or_insert(Rate::zero());
            *rate = rate.checked_add(t.rate).map_err(|_| {
                failure(
                    "production-coverage-invalid",
                    &t.id,
                    "terminal rate overflow",
                )
            })?;
        }
    }
    if expected != actual {
        return Err(failure(
            "production-coverage-invalid",
            &node.id,
            "physical terminal commodity rates do not match original resolved wiring",
        ));
    }
    Ok(())
}
