//! Compile original directed chains once; actions cannot mutate their incidence.

use aic_data::layouts::{FactoryEditCatalogs, TransportNetworkEndpoint, TransportNetworkSegment};
use indexmap::{IndexMap, IndexSet};
use serde_json::json;

use super::super::{catalog, chains};
use super::*;

pub(super) fn compile(w: &Witness) -> Result<(Blueprint, Draft)> {
    let decomposition = chains::extract(w, &IndexSet::new())?;
    let mut nodes = Vec::<Node>::new();
    let mut keys = IndexMap::<(usize, Point, Option<String>), usize>::new();
    let mut edges = Vec::new();
    let mut paths = Vec::new();
    for c in decomposition.chains {
        let mut endpoints = Vec::new();
        for (position, terminals) in [(c.start(), &c.start_terminals), (c.end(), &c.end_terminals)]
        {
            let components: Vec<_> = w
                .logistics_components
                .iter()
                .filter(|p| {
                    p.transport == c.transport
                        && Point::from(&p.position) == position
                        && w.transport_networks[c.network]
                            .component_ids
                            .contains(&p.id)
                })
                .map(|p| p.id.clone())
                .collect();
            if terminals.len() > 1 || (!terminals.is_empty() && !components.is_empty()) {
                return Err(fail(
                    "unsupported-anchor",
                    "V1 requires separate terminal and component anchors",
                    c.id,
                ));
            }
            let terminal = terminals.first().cloned();
            let key = (c.network, position, terminal.as_ref().map(|t| t.id.clone()));
            let id = *keys.entry(key).or_insert_with(|| {
                let id = nodes.len();
                nodes.push(Node {
                    id,
                    network: c.network,
                    initial_position: position,
                    terminal,
                    components,
                });
                id
            });
            endpoints.push(id);
        }
        if endpoints[0] == endpoints[1] {
            return Err(fail(
                "unsupported-cycle",
                "V1 does not edit a chain returning to the same anchor",
                c.id,
            ));
        }
        edges.push(Edge {
            id: edges.len(),
            network: c.network,
            source: endpoints[0],
            target: endpoints[1],
            rate: c.rate,
            first: c.first,
            last: c.last,
            zero_length: c.zero_length,
        });
        paths.push(Some(c.cells));
    }
    let blueprint = Blueprint { schema: CONTRACT, facilities: w.placements.clone(), utilizations: w.utilizations.clone(),
        networks: w.transport_networks.iter().map(|n| json!({"id": n.id, "requirement_ids": n.requirement_ids,
            "item": n.item, "transport": n.transport, "terminals": n.terminals, "component_ids": n.component_ids})).collect(),
        components: w.logistics_components.clone(), nodes, edges };
    let draft = Draft {
        placements: w.placements.clone(),
        paths,
    };
    let rebuilt = rebuild(w, &blueprint, &draft, None)?;
    // Admission must retain every segment and cell, including bridge ownership.
    for (a, b) in w.transport_networks.iter().zip(&rebuilt.transport_networks) {
        let normalize = |n: &aic_data::layouts::TransportNetwork| {
            let mut segments: Vec<_> = n
                .segments
                .iter()
                .map(|s| {
                    (
                        Point::from(&s.from),
                        Point::from(&s.to),
                        s.rate.numerator,
                        s.rate.denominator,
                    )
                })
                .collect();
            segments.sort();
            (
                n.cells
                    .iter()
                    .map(Point::from)
                    .collect::<std::collections::BTreeSet<_>>(),
                segments,
            )
        };
        if normalize(a) != normalize(b) {
            return Err(fail(
                "unsupported-decomposition",
                "Chain extraction does not reproduce the original network exactly",
                a.id.clone(),
            ));
        }
    }
    Ok((blueprint, draft))
}

pub(super) fn position(
    node: &Node,
    draft: &Draft,
    catalogs: &FactoryEditCatalogs,
) -> Result<Point> {
    if let Some(t) = &node.terminal
        && let TransportNetworkEndpoint::Facility { instance, port } = &t.endpoint
    {
        let p = draft
            .placements
            .iter()
            .find(|p| &p.instance == instance)
            .ok_or_else(|| {
                fail(
                    "missing-facility",
                    "Blueprint terminal has no draft facility",
                    instance.clone(),
                )
            })?;
        return catalog::placement_ports(p, catalogs)?
            .into_iter()
            .find(|p| &p.id == port)
            .map(|p| p.connection)
            .ok_or_else(|| {
                fail(
                    "missing-port",
                    "Original blueprint port is absent from catalog",
                    port.clone(),
                )
            });
    }
    Ok(node.initial_position)
}

pub(super) fn rebuild(
    original: &Witness,
    bp: &Blueprint,
    draft: &Draft,
    catalogs: Option<&FactoryEditCatalogs>,
) -> Result<Witness> {
    let mut w = original.clone();
    w.placements = draft.placements.clone();
    for n in &mut w.transport_networks {
        n.cells.clear();
        n.segments.clear();
    }
    for edge in &bp.edges {
        let cells = draft.paths[edge.id].as_ref().ok_or_else(|| {
            fail(
                "pending-edge",
                "Cannot export an unfinished blueprint edge",
                edge.id,
            )
        })?;
        let n = &mut w.transport_networks[edge.network];
        for &p in cells {
            n.cells.push(p.into());
        }
        for pair in cells.windows(2) {
            n.segments.push(TransportNetworkSegment {
                from: pair[0].into(),
                to: pair[1].into(),
                rate: edge.rate,
            });
        }
    }
    for n in &mut w.transport_networks {
        let cells: IndexSet<_> = n.cells.iter().map(Point::from).collect();
        n.cells = cells.into_iter().map(Into::into).collect();
    }
    if let Some(catalogs) = catalogs {
        for node in &bp.nodes {
            if let Some(t) = &node.terminal {
                let position = position(node, draft, catalogs)?;
                let target = w.transport_networks[node.network]
                    .terminals
                    .iter_mut()
                    .find(|v| v.id == t.id)
                    .unwrap();
                target.position = position.into();
            }
        }
    }
    Ok(w)
}
