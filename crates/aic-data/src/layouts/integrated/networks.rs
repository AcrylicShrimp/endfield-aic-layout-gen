use std::collections::BTreeMap;

use crate::facilities::FacilityPortDirection;
use crate::logistics::TransportKind;
use crate::recipes::Rate;

use super::{EdgeInput, EndpointInput, IntegratedLayoutDiagnostic};

pub(super) struct RoutingNetworkInput {
    id: String,
    item: String,
    transport: TransportKind,
    terminals: Vec<RoutingTerminalInput>,
    route_indices: Vec<usize>,
}

impl RoutingNetworkInput {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn route_indices(&self) -> &[usize] {
        &self.route_indices
    }

    pub(super) fn item(&self) -> &str {
        &self.item
    }

    pub(super) fn transport(&self) -> TransportKind {
        self.transport
    }

    pub(super) fn terminal_count(&self) -> usize {
        self.terminals.len()
    }

    pub(super) fn external_terminal_count(&self) -> usize {
        self.terminals
            .iter()
            .filter(|terminal| matches!(terminal.endpoint, EndpointInput::External { .. }))
            .count()
    }

    fn supplied_rate(&self) -> Result<Rate, IntegratedLayoutDiagnostic> {
        sum_terminal_rates(&self.id, &self.terminals, FacilityPortDirection::Output)
    }

    fn demanded_rate(&self) -> Result<Rate, IntegratedLayoutDiagnostic> {
        sum_terminal_rates(&self.id, &self.terminals, FacilityPortDirection::Input)
    }
}

struct RoutingTerminalInput {
    node: String,
    direction: FacilityPortDirection,
    endpoint: EndpointInput,
    rate: Rate,
}

struct NetworkBuilder {
    id: String,
    item: String,
    transport: TransportKind,
    terminals: BTreeMap<(String, bool), RoutingTerminalInput>,
    route_indices: Vec<usize>,
}

pub(super) fn normalize(
    edges: &[EdgeInput],
) -> Result<Vec<RoutingNetworkInput>, IntegratedLayoutDiagnostic> {
    let mut builders = BTreeMap::<String, NetworkBuilder>::new();

    for (route_index, edge) in edges.iter().enumerate() {
        let id = network_id(edge.transport, &edge.edge.item);
        let builder = builders
            .entry(id.clone())
            .or_insert_with(|| NetworkBuilder {
                id,
                item: edge.edge.item.clone(),
                transport: edge.transport,
                terminals: BTreeMap::new(),
                route_indices: Vec::new(),
            });
        builder.route_indices.push(route_index);
        add_terminal(
            builder,
            &edge.edge.source,
            FacilityPortDirection::Output,
            &edge.source,
            edge.edge.rate,
        )?;
        add_terminal(
            builder,
            &edge.edge.target,
            FacilityPortDirection::Input,
            &edge.target,
            edge.edge.rate,
        )?;
    }

    builders
        .into_values()
        .map(|builder| {
            let network = RoutingNetworkInput {
                id: builder.id,
                item: builder.item,
                transport: builder.transport,
                terminals: builder.terminals.into_values().collect(),
                route_indices: builder.route_indices,
            };
            let supplied = network.supplied_rate()?;
            let demanded = network.demanded_rate()?;
            if supplied != demanded {
                return Err(IntegratedLayoutDiagnostic::error(
                    "routing-network-flow-imbalance",
                    "/edges",
                    Some(network.id.clone()),
                    format!(
                        "routing network '{}' supplies {}/{} but demands {}/{}",
                        network.id,
                        supplied.numerator,
                        supplied.denominator,
                        demanded.numerator,
                        demanded.denominator
                    ),
                ));
            }
            Ok(network)
        })
        .collect()
}

fn add_terminal(
    builder: &mut NetworkBuilder,
    node: &str,
    direction: FacilityPortDirection,
    endpoint: &EndpointInput,
    rate: Rate,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let is_output = direction == FacilityPortDirection::Output;
    let terminal = builder
        .terminals
        .entry((node.to_string(), is_output))
        .or_insert_with(|| RoutingTerminalInput {
            node: node.to_string(),
            direction,
            endpoint: endpoint.clone(),
            rate: Rate::zero(),
        });
    terminal.rate = terminal.rate.checked_add(rate).map_err(|_| {
        IntegratedLayoutDiagnostic::error(
            "routing-network-rate-overflow",
            "/edges",
            Some(builder.id.clone()),
            format!(
                "routing network '{}' cannot aggregate the rate at terminal '{}'",
                builder.id, terminal.node
            ),
        )
    })?;
    Ok(())
}

fn sum_terminal_rates(
    network: &str,
    terminals: &[RoutingTerminalInput],
    direction: FacilityPortDirection,
) -> Result<Rate, IntegratedLayoutDiagnostic> {
    terminals
        .iter()
        .filter(|terminal| terminal.direction == direction)
        .try_fold(Rate::zero(), |total, terminal| {
            total.checked_add(terminal.rate).map_err(|_| {
                IntegratedLayoutDiagnostic::error(
                    "routing-network-rate-overflow",
                    "/edges",
                    Some(network.to_string()),
                    format!("routing network '{network}' terminal rates overflow"),
                )
            })
        })
}

fn network_id(transport: TransportKind, item: &str) -> String {
    let transport = match transport {
        TransportKind::Belt => "belt",
        TransportKind::Pipe => "pipe",
    };
    format!("network:{transport}:{item}")
}

#[cfg(test)]
mod tests {
    use crate::recipes::FacilityInstanceWiringEdge;

    use super::*;

    fn edge(
        source: &str,
        target: &str,
        item: &str,
        transport: TransportKind,
        rate: Rate,
    ) -> EdgeInput {
        let edge = FacilityInstanceWiringEdge::original(source, target, "intermediate", item, rate);
        EdgeInput {
            requirement_id: format!("{}:lane:0000", edge.id),
            edge,
            source: EndpointInput::External {
                node: source.to_string(),
            },
            target: EndpointInput::External {
                node: target.to_string(),
            },
            transport,
        }
    }

    #[test]
    fn aggregates_fungible_flow_into_item_transport_networks() {
        let networks = normalize(&[
            edge(
                "source",
                "target-a",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
            edge(
                "source",
                "target-b",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: 2,
                    denominator: 1,
                },
            ),
            edge(
                "liquid-source",
                "liquid-target",
                "fluid",
                TransportKind::Pipe,
                Rate {
                    numerator: 1,
                    denominator: 2,
                },
            ),
        ])
        .expect("balanced routes should normalize");

        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].id, "network:belt:part");
        assert_eq!(networks[0].route_indices(), &[0, 1]);
        assert_eq!(networks[0].terminal_count(), 3);
        assert_eq!(
            networks[0].supplied_rate().expect("rate should add"),
            Rate {
                numerator: 3,
                denominator: 1,
            }
        );
        assert_eq!(networks[1].id, "network:pipe:fluid");
    }

    #[test]
    fn reports_terminal_rate_overflow() {
        let report = normalize(&[
            edge(
                "source",
                "target-a",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: i64::MAX,
                    denominator: 1,
                },
            ),
            edge(
                "source",
                "target-b",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: i64::MAX,
                    denominator: 1,
                },
            ),
        ]);
        let Err(report) = report else {
            panic!("overflow must be diagnosed");
        };

        assert_eq!(report.code, "routing-network-rate-overflow");
        assert_eq!(report.entity.as_deref(), Some("network:belt:part"));
    }
}
