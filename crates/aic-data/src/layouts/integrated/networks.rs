use std::collections::BTreeMap;

use crate::facilities::FacilityPortDirection;
use crate::logistics::{LogisticsComponentKind, TransportKind};
use crate::recipes::Rate;

use super::{ComponentCapacityRates, EdgeInput, EndpointInput, IntegratedLayoutDiagnostic};

#[derive(Clone)]
pub(super) struct RoutingNetworkInput {
    id: String,
    item: String,
    transport: TransportKind,
    line_capacity_rate: Rate,
    flow_scale: i64,
    line_capacity_units: i32,
    component_capacity_units: ComponentCapacityUnits,
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

    pub(super) fn flow_scale(&self) -> i64 {
        self.flow_scale
    }

    pub(super) fn line_capacity_units(&self) -> i32 {
        self.line_capacity_units
    }

    pub(super) fn line_capacity_rate(&self) -> Rate {
        self.line_capacity_rate
    }

    pub(super) fn component_capacity_units(&self, kind: LogisticsComponentKind) -> i32 {
        self.component_capacity_units.get(kind)
    }

    pub(super) fn terminals(&self) -> &[RoutingTerminalInput] {
        &self.terminals
    }

    pub(super) fn total_terminal_flow_units(&self) -> i64 {
        self.terminals
            .iter()
            .map(|terminal| i64::from(terminal.flow_units))
            .sum()
    }

    pub(super) fn flow_units_for_hint(&self, rate: Rate) -> Option<i32> {
        rate_to_flow_units(&self.id, "prior solution hint", rate, self.flow_scale).ok()
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
}

#[derive(Clone)]
pub(super) struct RoutingTerminalInput {
    id: String,
    route_index: usize,
    direction: FacilityPortDirection,
    endpoint: EndpointInput,
    rate: Rate,
    flow_units: i32,
}

impl RoutingTerminalInput {
    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn route_index(&self) -> usize {
        self.route_index
    }

    pub(super) fn direction(&self) -> FacilityPortDirection {
        self.direction
    }

    pub(super) fn rate(&self) -> Rate {
        self.rate
    }

    pub(super) fn flow_units(&self) -> i32 {
        self.flow_units
    }
}

struct NetworkBuilder {
    id: String,
    item: String,
    transport: TransportKind,
    capacity_rate: Rate,
    component_capacity_rates: ComponentCapacityRates,
    terminals: Vec<RoutingTerminalInput>,
    route_indices: Vec<usize>,
}

#[derive(Clone, Copy)]
struct ComponentCapacityUnits {
    splitter: i32,
    converger: i32,
    bridge: i32,
}

impl ComponentCapacityUnits {
    fn get(self, kind: LogisticsComponentKind) -> i32 {
        match kind {
            LogisticsComponentKind::Splitter => self.splitter,
            LogisticsComponentKind::Converger => self.converger,
            LogisticsComponentKind::Bridge => self.bridge,
        }
    }
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
                capacity_rate: edge.capacity_rate,
                component_capacity_rates: edge.component_capacity_rates,
                terminals: Vec::new(),
                route_indices: Vec::new(),
            });
        if builder.capacity_rate != edge.capacity_rate {
            return Err(IntegratedLayoutDiagnostic::error(
                "routing-network-capacity-mismatch",
                "/edges",
                Some(builder.id.clone()),
                format!(
                    "routing network '{}' contains inconsistent transport capacities",
                    builder.id
                ),
            ));
        }
        if builder.component_capacity_rates != edge.component_capacity_rates {
            return Err(IntegratedLayoutDiagnostic::error(
                "routing-network-component-capacity-mismatch",
                "/edges",
                Some(builder.id.clone()),
                format!(
                    "routing network '{}' contains inconsistent logistics component capacities",
                    builder.id
                ),
            ));
        }
        builder.route_indices.push(route_index);
        add_terminal(
            builder,
            &edge.requirement_id,
            route_index,
            FacilityPortDirection::Output,
            &edge.source,
            edge.edge.rate,
        )?;
        add_terminal(
            builder,
            &edge.requirement_id,
            route_index,
            FacilityPortDirection::Input,
            &edge.target,
            edge.edge.rate,
        )?;
    }

    builders
        .into_values()
        .map(|builder| {
            let flow_scale = checked_flow_scale(&builder)?;
            let line_capacity_units = rate_to_flow_units(
                &builder.id,
                "transport capacity",
                builder.capacity_rate,
                flow_scale,
            )?;
            if line_capacity_units > i32::MAX / 4 {
                return Err(IntegratedLayoutDiagnostic::error(
                    "routing-network-component-bound-out-of-range",
                    "/edges",
                    Some(builder.id.clone()),
                    format!(
                        "routing network '{}' line capacity is too large for exact component constraints",
                        builder.id
                    ),
                ));
            }
            let component_capacity_units = ComponentCapacityUnits {
                splitter: rate_to_flow_units(
                    &builder.id,
                    "splitter capacity",
                    builder.component_capacity_rates.splitter,
                    flow_scale,
                )?,
                converger: rate_to_flow_units(
                    &builder.id,
                    "converger capacity",
                    builder.component_capacity_rates.converger,
                    flow_scale,
                )?,
                bridge: rate_to_flow_units(
                    &builder.id,
                    "bridge capacity",
                    builder.component_capacity_rates.bridge,
                    flow_scale,
                )?,
            };
            let terminals = builder
                .terminals
                .into_iter()
                .map(|mut terminal| {
                    terminal.flow_units = rate_to_flow_units(
                        &builder.id,
                        &format!("terminal '{}'", terminal.id),
                        terminal.rate,
                        flow_scale,
                    )?;
                    if terminal.flow_units > line_capacity_units {
                        return Err(IntegratedLayoutDiagnostic::error(
                            "routing-terminal-exceeds-line-capacity",
                            "/edges",
                            Some(terminal.id.clone()),
                            format!(
                                "routing terminal '{}' requires {} flow units but one {:?} line carries at most {}",
                                terminal.id,
                                terminal.flow_units,
                                builder.transport,
                                line_capacity_units
                            ),
                        ));
                    }
                    Ok(terminal)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let supplied = sum_terminal_units(
                &builder.id,
                &terminals,
                FacilityPortDirection::Output,
            )?;
            let demanded = sum_terminal_units(
                &builder.id,
                &terminals,
                FacilityPortDirection::Input,
            )?;
            if supplied != demanded {
                return Err(IntegratedLayoutDiagnostic::error(
                    "routing-network-flow-imbalance",
                    "/edges",
                    Some(builder.id.clone()),
                    format!(
                        "routing network '{}' supplies {supplied} flow units but demands {demanded}",
                        builder.id
                    ),
                ));
            }
            Ok(RoutingNetworkInput {
                id: builder.id,
                item: builder.item,
                transport: builder.transport,
                line_capacity_rate: builder.capacity_rate,
                flow_scale,
                line_capacity_units,
                component_capacity_units,
                terminals,
                route_indices: builder.route_indices,
            })
        })
        .collect()
}

fn add_terminal(
    builder: &mut NetworkBuilder,
    requirement_id: &str,
    route_index: usize,
    direction: FacilityPortDirection,
    endpoint: &EndpointInput,
    rate: Rate,
) -> Result<(), IntegratedLayoutDiagnostic> {
    if rate.is_zero() {
        return Err(IntegratedLayoutDiagnostic::error(
            "zero-routing-terminal-rate",
            "/edges",
            Some(requirement_id.to_string()),
            "routing terminal rates must be positive",
        ));
    }
    let role = match direction {
        FacilityPortDirection::Input => "demand",
        FacilityPortDirection::Output => "supply",
    };
    builder.terminals.push(RoutingTerminalInput {
        id: format!("{requirement_id}:{role}"),
        route_index,
        direction,
        endpoint: endpoint.clone(),
        rate,
        flow_units: 0,
    });
    Ok(())
}

fn checked_flow_scale(builder: &NetworkBuilder) -> Result<i64, IntegratedLayoutDiagnostic> {
    std::iter::once(builder.capacity_rate)
        .chain(builder.component_capacity_rates.values())
        .chain(builder.terminals.iter().map(|terminal| terminal.rate))
        .try_fold(1_i64, |scale, rate| {
            checked_lcm(scale, rate.denominator).ok_or_else(|| {
                IntegratedLayoutDiagnostic::error(
                    "routing-network-flow-scale-overflow",
                    "/edges",
                    Some(builder.id.clone()),
                    format!(
                        "routing network '{}' has no representable common flow denominator",
                        builder.id
                    ),
                )
            })
        })
}

fn checked_lcm(left: i64, right: i64) -> Option<i64> {
    if left <= 0 || right <= 0 {
        return None;
    }
    let divisor = gcd(left, right);
    left.checked_div(divisor)?.checked_mul(right)
}

fn gcd(mut left: i64, mut right: i64) -> i64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn rate_to_flow_units(
    network: &str,
    subject: &str,
    rate: Rate,
    flow_scale: i64,
) -> Result<i32, IntegratedLayoutDiagnostic> {
    if rate.numerator <= 0 || rate.denominator <= 0 || flow_scale % rate.denominator != 0 {
        return Err(IntegratedLayoutDiagnostic::error(
            "invalid-routing-network-rate",
            "/edges",
            Some(network.to_string()),
            format!("routing network '{network}' has an invalid rate for {subject}"),
        ));
    }
    let multiplier = flow_scale / rate.denominator;
    let units = rate.numerator.checked_mul(multiplier).ok_or_else(|| {
        IntegratedLayoutDiagnostic::error(
            "routing-network-flow-units-out-of-range",
            "/edges",
            Some(network.to_string()),
            format!("routing network '{network}' cannot represent {subject} in solver flow units"),
        )
    })?;
    i32::try_from(units).map_err(|_| {
        IntegratedLayoutDiagnostic::error(
            "routing-network-flow-units-out-of-range",
            "/edges",
            Some(network.to_string()),
            format!(
                "routing network '{network}' cannot represent {subject} in 32-bit solver flow units"
            ),
        )
    })
}

fn sum_terminal_units(
    network: &str,
    terminals: &[RoutingTerminalInput],
    direction: FacilityPortDirection,
) -> Result<i64, IntegratedLayoutDiagnostic> {
    terminals
        .iter()
        .filter(|terminal| terminal.direction == direction)
        .try_fold(0_i64, |total, terminal| {
            total
                .checked_add(i64::from(terminal.flow_units))
                .ok_or_else(|| {
                    IntegratedLayoutDiagnostic::error(
                        "routing-network-flow-units-out-of-range",
                        "/edges",
                        Some(network.to_string()),
                        format!("routing network '{network}' terminal flow units overflow"),
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
        capacity_rate: Rate,
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
            capacity_rate,
            component_capacity_rates: ComponentCapacityRates {
                splitter: capacity_rate,
                converger: capacity_rate,
                bridge: capacity_rate,
            },
        }
    }

    #[test]
    fn preserves_independent_terminal_lanes_in_fungible_networks() {
        let belt_capacity = Rate {
            numerator: 3,
            denominator: 1,
        };
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
                belt_capacity,
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
                belt_capacity,
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
                Rate {
                    numerator: 1,
                    denominator: 1,
                },
            ),
        ])
        .expect("balanced routes should normalize");

        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].id, "network:belt:part");
        assert_eq!(networks[0].route_indices(), &[0, 1]);
        assert_eq!(networks[0].terminal_count(), 4);
        assert_eq!(networks[0].flow_scale(), 1);
        assert_eq!(networks[0].line_capacity_units(), 3);
        assert_eq!(networks[0].terminals[0].flow_units, 1);
        assert_eq!(networks[0].terminals[2].flow_units, 2);
        assert!(matches!(
            (&networks[0].terminals[0].endpoint, &networks[0].terminals[2].endpoint),
            (
                EndpointInput::External { node: left },
                EndpointInput::External { node: right }
            ) if left == right && left == "source"
        ));
        assert_ne!(networks[0].terminals[0].id, networks[0].terminals[2].id);
        assert_eq!(networks[1].id, "network:pipe:fluid");
        assert_eq!(networks[1].flow_scale(), 2);
        assert_eq!(networks[1].line_capacity_units(), 2);
    }

    #[test]
    fn reports_common_flow_scale_overflow() {
        let report = normalize(&[
            edge(
                "source",
                "target-a",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: 1,
                    denominator: i64::MAX,
                },
                Rate {
                    numerator: 1,
                    denominator: i64::MAX - 1,
                },
            ),
            edge(
                "source-b",
                "target-b",
                "part",
                TransportKind::Belt,
                Rate {
                    numerator: 1,
                    denominator: i64::MAX - 1,
                },
                Rate {
                    numerator: 1,
                    denominator: i64::MAX - 1,
                },
            ),
        ]);
        let Err(report) = report else {
            panic!("overflow must be diagnosed");
        };

        assert_eq!(report.code, "routing-network-flow-scale-overflow");
        assert_eq!(report.entity.as_deref(), Some("network:belt:part"));
    }

    #[test]
    fn rejects_a_terminal_lane_above_line_capacity() {
        let report = normalize(&[edge(
            "source",
            "target",
            "part",
            TransportKind::Belt,
            Rate {
                numerator: 2,
                denominator: 1,
            },
            Rate {
                numerator: 1,
                denominator: 1,
            },
        )]);
        let Err(report) = report else {
            panic!("an oversized terminal lane must be diagnosed");
        };

        assert_eq!(report.code, "routing-terminal-exceeds-line-capacity");
    }
}
