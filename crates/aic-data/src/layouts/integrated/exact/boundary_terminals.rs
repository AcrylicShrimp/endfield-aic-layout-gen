use std::collections::BTreeSet;

use pumpkin_solver::core::predicates::PredicateConstructor;
use pumpkin_solver::core::variables::{DomainId, TransformableVariable};

use super::recorder::{ConstraintFamily, RecordedModel, VariableFamily};
use crate::facilities::FacilityPortEdge;
use crate::layouts::integrated::{
    ExactModelMetrics, IntegratedLayoutDiagnostic, IntegratedLayoutReport, ModelInput,
    TransportNetworkEndpoint,
};

#[derive(Clone, Copy)]
pub(super) struct UsedBoundsVariables {
    pub(super) width: DomainId,
    pub(super) height: DomainId,
}

pub(super) struct BoundaryTerminalSelector {
    pub(super) key: DomainId,
    pub(super) reachable_keys: Vec<i32>,
    pub(super) domain: BoundaryTerminalDomainCertificate,
}

#[derive(Clone)]
pub(super) struct BoundaryTerminalDomainCertificate {
    pub(super) kind: &'static str,
    pub(super) lower_bound: i32,
    pub(super) upper_bound: i32,
    pub(super) declared_values: Vec<i32>,
    pub(super) unary_table_projection: Vec<i32>,
}

pub(super) fn new_used_bounds(
    solver: &mut RecordedModel,
    input: &ModelInput,
) -> UsedBoundsVariables {
    UsedBoundsVariables {
        width: solver.new_variable(
            VariableFamily::Objective,
            1,
            input.width,
            "used-bounding-box-width",
        ),
        height: solver.new_variable(
            VariableFamily::Objective,
            1,
            input.height,
            "used-bounding-box-height",
        ),
    }
}

pub(super) fn build_selector(
    solver: &mut RecordedModel,
    input: &ModelInput,
    edge_index: usize,
    endpoint_kind: &str,
    used_bounds: UsedBoundsVariables,
    sparse_legal_key_domain: bool,
    restricted_keys: Option<&[i32]>,
    metrics: &mut ExactModelMetrics,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> BoundaryTerminalSelector {
    let full_reachable_keys = reachable_boundary_keys(input.width, input.height);
    let reachable_keys = restricted_keys.map_or_else(
        || full_reachable_keys.clone(),
        |restricted| {
            assert!(
                !restricted.is_empty()
                    && restricted
                        .iter()
                        .all(|key| full_reachable_keys.binary_search(key).is_ok()),
                "restricted boundary keys must be a non-empty subset of legal keys"
            );
            let mut keys = restricted.to_vec();
            keys.sort_unstable();
            keys.dedup();
            keys
        },
    );
    let key_upper = input
        .cell_count
        .checked_mul(4)
        .and_then(|value| value.checked_sub(1))
        .expect("validated boundary key domain fits i32");
    let name = format!("edge-{edge_index}-{endpoint_kind}-boundary-key");
    let declared_values = if sparse_legal_key_domain {
        reachable_keys.clone()
    } else {
        (0..=key_upper).collect()
    };
    let declared_lower_bound = *declared_values
        .first()
        .expect("boundary key domain is non-empty");
    let declared_upper_bound = *declared_values
        .last()
        .expect("boundary key domain is non-empty");
    let key = if sparse_legal_key_domain {
        solver.new_sparse_variable(
            VariableFamily::BoundaryTerminal,
            declared_values.clone(),
            name,
        )
    } else {
        solver.new_variable(VariableFamily::BoundaryTerminal, 0, key_upper, name)
    };
    metrics.boundary_terminal_variables += 1;
    solver.post_table(
        ConstraintFamily::BoundaryTerminal,
        vec![key],
        reachable_keys.iter().map(|key| vec![*key]).collect(),
        tag,
    );

    for candidate in &reachable_keys {
        let cell = candidate / 4;
        let direction = candidate % 4;
        let x = cell % input.width;
        let y = cell / input.width;
        let expected_bound = match direction {
            1 => Some((used_bounds.width, x + 1)),
            2 => Some((used_bounds.height, y + 1)),
            0 | 3 => None,
            _ => unreachable!("boundary direction key is cardinal"),
        };
        let Some((bound, value)) = expected_bound else {
            continue;
        };
        let selected = solver
            .solver_mut()
            .new_literal_for_predicate(key.equality_predicate(*candidate), tag);
        solver.post_implied_equals(
            ConstraintFamily::BoundaryTerminal,
            vec![bound.scaled(1)],
            value,
            1,
            selected,
            key,
            tag,
        );
    }

    BoundaryTerminalSelector {
        key,
        reachable_keys: reachable_keys.clone(),
        domain: BoundaryTerminalDomainCertificate {
            kind: if sparse_legal_key_domain {
                "sparse-legal"
            } else {
                "bounded"
            },
            lower_bound: declared_lower_bound,
            upper_bound: declared_upper_bound,
            declared_values,
            unary_table_projection: reachable_keys,
        },
    }
}

fn reachable_boundary_keys(width: i32, height: i32) -> Vec<i32> {
    let mut keys = BTreeSet::new();
    for x in 0..width {
        keys.insert(geometry_key(x, 0, width, 0));
    }
    for y in 0..height {
        keys.insert(geometry_key(0, y, width, 3));
    }
    for y in 0..height {
        for x in 0..width {
            keys.insert(geometry_key(x, y, width, 1));
            keys.insert(geometry_key(x, y, width, 2));
        }
    }
    keys.into_iter().collect()
}

fn geometry_key(x: i32, y: i32, width: i32, direction: i32) -> i32 {
    (y * width + x) * 4 + direction
}

pub(super) fn validate_witness(
    report: &IntegratedLayoutReport,
) -> Result<(), IntegratedLayoutDiagnostic> {
    let bounds = report.bounds.as_ref().ok_or_else(|| {
        invalid(
            "/bounds",
            "shared boundary terminals require exact used bounds",
        )
    })?;
    for (network_index, network) in report.transport_networks.iter().enumerate() {
        for (terminal_index, terminal) in network.terminals.iter().enumerate() {
            let TransportNetworkEndpoint::External { side, .. } = &terminal.endpoint else {
                continue;
            };
            let on_selected_side = match side {
                FacilityPortEdge::North => terminal.position.y == 0,
                FacilityPortEdge::East => terminal.position.x + 1 == bounds.width,
                FacilityPortEdge::South => terminal.position.y + 1 == bounds.height,
                FacilityPortEdge::West => terminal.position.x == 0,
            };
            if !on_selected_side {
                return Err(invalid(
                    format!(
                        "/transport_networks/{network_index}/terminals/{terminal_index}/position"
                    ),
                    "external terminal is not on its selected used-bounds side",
                ));
            }
        }
    }
    Ok(())
}

fn invalid(path: impl Into<String>, message: impl Into<String>) -> IntegratedLayoutDiagnostic {
    IntegratedLayoutDiagnostic::error(
        "invalid-shared-boundary-terminal-witness",
        path,
        None,
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boundary_domain_contains_fixed_minimum_and_variable_maximum_sides() {
        let keys = reachable_boundary_keys(3, 2);
        assert_eq!(keys.len(), 17);
        assert!(keys.contains(&geometry_key(2, 0, 3, 0)));
        assert!(keys.contains(&geometry_key(0, 1, 3, 3)));
        assert!(keys.contains(&geometry_key(1, 1, 3, 1)));
        assert!(keys.contains(&geometry_key(2, 0, 3, 2)));
        assert!(!keys.contains(&geometry_key(1, 1, 3, 0)));
        assert!(!keys.contains(&geometry_key(1, 1, 3, 3)));
    }
}
