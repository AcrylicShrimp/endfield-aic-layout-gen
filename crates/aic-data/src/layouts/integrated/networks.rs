use std::collections::BTreeMap;

use crate::facilities::FacilityPortDirection;
use crate::logistics::{LogisticsComponentKind, TransportKind, ValidatedLogisticsComponentCatalog};
use crate::recipes::Rate;

use super::{EdgeInput, EndpointInput, IntegratedLayoutDiagnostic};

pub(super) struct RoutingTopologyPlan {
    network_count: usize,
    requirements: Vec<ComponentRequirement>,
}

impl RoutingTopologyPlan {
    pub(super) fn network_count(&self) -> usize {
        self.network_count
    }

    pub(super) fn component_count(&self, kind: LogisticsComponentKind) -> usize {
        self.requirements
            .iter()
            .filter(|requirement| requirement.kind == kind)
            .map(|requirement| requirement.component_count)
            .sum()
    }

    pub(super) fn shared_bundle_count(&self) -> usize {
        self.requirements.len()
    }

    pub(super) fn max_branch_count(&self) -> usize {
        self.requirements
            .iter()
            .map(|requirement| requirement.route_indices.len())
            .max()
            .unwrap_or(0)
    }

    pub(super) fn referenced_terminal_count(&self) -> usize {
        self.requirements
            .iter()
            .map(|requirement| (&requirement.network, &requirement.terminal))
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }
}

struct ComponentRequirement {
    network: String,
    terminal: String,
    kind: LogisticsComponentKind,
    route_indices: Vec<usize>,
    component_count: usize,
}

pub(super) struct RoutingNetworkInput {
    id: String,
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

    pub(super) fn terminal_count(&self) -> usize {
        self.terminals.len()
    }

    pub(super) fn boundary_terminal_count(&self) -> usize {
        self.terminals
            .iter()
            .filter(|terminal| matches!(terminal.endpoint, EndpointInput::Boundary { .. }))
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

pub(super) fn plan_topology(
    networks: &[RoutingNetworkInput],
    edges: &[EdgeInput],
    components: &ValidatedLogisticsComponentCatalog,
) -> Result<RoutingTopologyPlan, IntegratedLayoutDiagnostic> {
    let mut requirements = Vec::new();

    for network in networks {
        for (direction, kind) in [
            (
                FacilityPortDirection::Output,
                LogisticsComponentKind::Splitter,
            ),
            (
                FacilityPortDirection::Input,
                LogisticsComponentKind::Converger,
            ),
        ] {
            let definition = components
                .component_by_kind(edges[network.route_indices[0]].transport, kind)
                .expect("validated catalog has every component capability");
            let capacity = Rate::from_quantity_per_duration_ms(
                definition.capacity.quantity,
                definition.capacity.duration_ms,
            )
            .map_err(|_| {
                IntegratedLayoutDiagnostic::error(
                    "routing-component-capacity-out-of-range",
                    "/logistics_components",
                    Some(definition.id.clone()),
                    format!(
                        "logistics component '{}' capacity cannot be represented in the exact rate domain",
                        definition.id
                    ),
                )
            })?;
            let branch_arity = match kind {
                LogisticsComponentKind::Splitter => definition.output_directions.len(),
                LogisticsComponentKind::Converger => definition.input_directions.len(),
                LogisticsComponentKind::Bridge => unreachable!(),
            };
            let mut terminal_routes = BTreeMap::<String, Vec<usize>>::new();
            for route_index in network.route_indices() {
                let edge = &edges[*route_index];
                let terminal = match direction {
                    FacilityPortDirection::Output => &edge.edge.source,
                    FacilityPortDirection::Input => &edge.edge.target,
                };
                terminal_routes
                    .entry(terminal.clone())
                    .or_default()
                    .push(*route_index);
            }
            for (terminal, route_indices) in terminal_routes {
                for bundle in capacity_bundles(&route_indices, edges, capacity)? {
                    if bundle.len() <= 1 {
                        continue;
                    }
                    requirements.push(ComponentRequirement {
                        network: network.id.clone(),
                        terminal: terminal.clone(),
                        component_count: tree_component_count(bundle.len(), branch_arity),
                        kind,
                        route_indices: bundle,
                    });
                }
            }
        }
    }

    Ok(RoutingTopologyPlan {
        network_count: networks.len(),
        requirements,
    })
}

fn capacity_bundles(
    route_indices: &[usize],
    edges: &[EdgeInput],
    capacity: Rate,
) -> Result<Vec<Vec<usize>>, IntegratedLayoutDiagnostic> {
    let mut route_indices = route_indices.to_vec();
    route_indices.sort_by(|left, right| {
        edges[*right]
            .edge
            .rate
            .cmp(&edges[*left].edge.rate)
            .then_with(|| left.cmp(right))
    });
    let mut bundles = Vec::<(Rate, Vec<usize>)>::new();

    for route_index in route_indices {
        let rate = edges[route_index].edge.rate;
        if rate > capacity {
            return Err(IntegratedLayoutDiagnostic::error(
                "routing-component-capacity-exceeded",
                format!("/edges/{route_index}/rate"),
                Some(edges[route_index].edge.item.clone()),
                "capacity-split route exceeds the selected logistics component capacity",
            ));
        }
        let mut selected = None;
        for (bundle_index, (bundle_rate, _)) in bundles.iter().enumerate() {
            let combined = bundle_rate.checked_add(rate).map_err(|_| {
                IntegratedLayoutDiagnostic::error(
                    "routing-network-rate-overflow",
                    format!("/edges/{route_index}/rate"),
                    Some(edges[route_index].edge.item.clone()),
                    "routing topology bundle rate overflowed",
                )
            })?;
            if combined <= capacity {
                selected = Some((bundle_index, combined));
                break;
            }
        }
        if let Some((bundle_index, combined)) = selected {
            bundles[bundle_index].0 = combined;
            bundles[bundle_index].1.push(route_index);
        } else {
            bundles.push((rate, vec![route_index]));
        }
    }

    Ok(bundles
        .into_iter()
        .map(|(_, route_indices)| route_indices)
        .collect())
}

fn tree_component_count(branches: usize, branch_arity: usize) -> usize {
    debug_assert!(branches > 1);
    debug_assert!(branch_arity > 1);
    (branches - 1).div_ceil(branch_arity - 1)
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
        EdgeInput {
            edge: FacilityInstanceWiringEdge {
                source: source.to_string(),
                target: target.to_string(),
                kind: "intermediate".to_string(),
                item: item.to_string(),
                rate,
            },
            source: EndpointInput::Boundary {
                node: source.to_string(),
            },
            target: EndpointInput::Boundary {
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
        assert_eq!(networks[0].id(), "network:belt:part");
        assert_eq!(networks[0].route_indices(), &[0, 1]);
        assert_eq!(networks[0].terminal_count(), 3);
        assert_eq!(
            networks[0].supplied_rate().expect("rate should add"),
            Rate {
                numerator: 3,
                denominator: 1,
            }
        );
        assert_eq!(networks[1].id(), "network:pipe:fluid");
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

    #[test]
    fn packs_shared_branches_without_exceeding_line_capacity() {
        let edges = [
            edge(
                "source",
                "target-a",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: 1,
                    denominator: 4,
                },
            ),
            edge(
                "source",
                "target-b",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: 1,
                    denominator: 4,
                },
            ),
            edge(
                "source",
                "target-c",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: 1,
                    denominator: 4,
                },
            ),
            edge(
                "source",
                "target-d",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: 1,
                    denominator: 4,
                },
            ),
        ];
        let bundles = capacity_bundles(
            &[0, 1, 2, 3],
            &edges,
            Rate {
                numerator: 1,
                denominator: 2,
            },
        )
        .expect("quarter-rate branches should pack exactly");

        assert_eq!(bundles, vec![vec![0, 1], vec![2, 3]]);
        assert_eq!(tree_component_count(4, 3), 2);
        assert_eq!(tree_component_count(3, 3), 1);
        assert_eq!(tree_component_count(2, 3), 1);
    }
}
