use crate::logistics::{TransportKind, ValidatedTransportCatalog};
use crate::recipes::Rate;

use super::ConstructiveFrontierDiagnostic;

pub(super) fn split_rate_into_lanes(
    rate: Rate,
    transport: TransportKind,
    transports: &ValidatedTransportCatalog,
    entity: &str,
) -> Result<Vec<Rate>, ConstructiveFrontierDiagnostic> {
    let definition = transports.capacity(transport);
    let capacity = Rate::from_quantity_per_duration_ms(definition.quantity, definition.duration_ms)
        .map_err(|error| {
            ConstructiveFrontierDiagnostic::error(
                "constructive-transport-capacity-out-of-range",
                "/transport_catalog",
                Some(entity.to_string()),
                error.message,
            )
        })?;
    let mut remaining = rate;
    let mut lanes = Vec::new();
    while !remaining.is_zero() {
        let lane = remaining.min(capacity);
        lanes.push(lane);
        remaining = remaining.checked_sub(lane).map_err(|error| {
            ConstructiveFrontierDiagnostic::error(
                "constructive-lane-rate-arithmetic-overflow",
                "/rate",
                Some(entity.to_string()),
                error.message,
            )
        })?;
    }
    if lanes.is_empty() {
        return Err(ConstructiveFrontierDiagnostic::error(
            "constructive-zero-rate-requirement",
            "/rate",
            Some(entity.to_string()),
            "constructive routing requires a positive material rate",
        ));
    }
    Ok(lanes)
}

pub(super) fn lane_id(requirement: &str, lane_index: usize) -> String {
    format!("{requirement}:lane:{lane_index:04}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logistics::{
        SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION, TransportCapacity, TransportCatalog,
        TransportDefinition,
    };

    fn transports() -> ValidatedTransportCatalog {
        ValidatedTransportCatalog::try_from_catalog(TransportCatalog {
            schema_version: SUPPORTED_TRANSPORT_CATALOG_SCHEMA_VERSION,
            transports: vec![
                TransportDefinition {
                    kind: TransportKind::Belt,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 2_000,
                    },
                },
                TransportDefinition {
                    kind: TransportKind::Pipe,
                    capacity: TransportCapacity {
                        quantity: 1,
                        duration_ms: 500,
                    },
                },
            ],
        })
        .expect("transport catalog validates")
    }

    #[test]
    fn splits_rate_into_minimum_capacity_bounded_lanes() {
        let lanes = split_rate_into_lanes(
            Rate {
                numerator: 5,
                denominator: 4,
            },
            TransportKind::Belt,
            &transports(),
            "edge",
        )
        .expect("rate splits");

        assert_eq!(
            lanes,
            vec![
                Rate {
                    numerator: 1,
                    denominator: 2,
                },
                Rate {
                    numerator: 1,
                    denominator: 2,
                },
                Rate {
                    numerator: 1,
                    denominator: 4,
                },
            ]
        );
    }
}
