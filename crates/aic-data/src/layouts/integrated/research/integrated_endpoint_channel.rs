use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::{FacilityPlacementRequest, plan_facility_growth};
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::coordinate_partition::{classify_outcome, invalid_input, prepare_target_input};
use super::{
    EndpointChannelEncoding, EndpointSupportPropagationStatistics, ExactDimensionCaseOutcome,
};
use crate::layouts::integrated::{ExactSearchStatistics, IntegratedLayoutReport, exact};

pub const INTEGRATED_ENDPOINT_CHANNEL_COMPARISON_SCHEMA_VERSION: u32 = 3;
const MAX_NEW_FACILITIES_PER_GROWTH_PHASE: usize = 1;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegratedEndpointTableScale {
    pub facility_endpoint_tables: usize,
    pub legal_tuple_rows: usize,
    pub estimated_hidden_row_literals: usize,
    pub estimated_table_clauses: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct IntegratedEndpointChannelCaseReport {
    pub schema_version: u32,
    pub target_phase_index: usize,
    pub encoding: EndpointChannelEncoding,
    pub outcome: ExactDimensionCaseOutcome,
    pub fixed_dimensions: [i32; 2],
    pub case_search_budget_ms: u64,
    pub row_selector_tracking: bool,
    pub endpoint_table_scale: IntegratedEndpointTableScale,
    pub endpoint_support_propagation: EndpointSupportPropagationStatistics,
    pub outer_wall_ms: u64,
    pub construction_ms: u64,
    pub search_ms: u64,
    pub first_incumbent_ms: Option<u64>,
    pub search_statistics: ExactSearchStatistics,
    pub layout: IntegratedLayoutReport,
    pub diagnostic_only: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn run_integrated_endpoint_channel_case(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    fixed_width: i32,
    fixed_height: i32,
    encoding: EndpointChannelEncoding,
    track_row_selectors: bool,
    case_search_budget: Duration,
) -> Result<IntegratedEndpointChannelCaseReport, IntegratedLayoutReport> {
    if case_search_budget.is_zero() {
        return Err(invalid_input(
            "/case_search_budget",
            "integrated endpoint-channel comparison requires a positive search budget",
        ));
    }
    if fixed_width <= 0 || fixed_height <= 0 {
        return Err(invalid_input(
            "/fixed_dimensions",
            "integrated endpoint-channel comparison requires positive fixed dimensions",
        ));
    }
    if !matches!(
        encoding,
        EndpointChannelEncoding::NestedElement
            | EndpointChannelEncoding::PositiveTable
            | EndpointChannelEncoding::SparseSupport
    ) {
        return Err(invalid_input(
            "/encoding",
            "integrated endpoint-channel comparison supports nested-element, positive-table, or sparse-support",
        ));
    }
    if track_row_selectors && encoding != EndpointChannelEncoding::PositiveTable {
        return Err(invalid_input(
            "/track_row_selectors",
            "row-selector tracking requires the positive-table endpoint encoding",
        ));
    }
    let growth = plan_facility_growth(instance_wiring, MAX_NEW_FACILITIES_PER_GROWTH_PHASE);
    if !growth.success || target_phase_index >= growth.phases.len() {
        return Err(invalid_input(
            "/target_phase_index",
            "facility growth planning failed or target phase is out of range",
        ));
    }
    let input = prepare_target_input(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        &growth,
        target_phase_index,
    )?;
    let (facility_endpoint_tables, legal_tuple_rows, estimated_table_clauses) =
        exact::shared_layer::positive_table_endpoint_scale(&input);
    let (estimated_hidden_row_literals, estimated_table_clauses) =
        encoding_table_overhead(encoding, legal_tuple_rows, estimated_table_clauses);
    let endpoint_table_scale = IntegratedEndpointTableScale {
        facility_endpoint_tables,
        legal_tuple_rows,
        estimated_hidden_row_literals,
        estimated_table_clauses,
    };
    let fixed_dimensions = exact::shared_layer::FixedUsedDimensions {
        width: fixed_width,
        height: fixed_height,
    };
    let started = Instant::now();
    let (layout, endpoint_support_propagation) = match (encoding, track_row_selectors) {
        (EndpointChannelEncoding::NestedElement, false) => {
            let layout = exact::shared_layer::solve_factored_endpoints_fixed_dimensions_feasibility_only_with_prior_and_local_continuation_guarded_intersection_propagation(
                input,
                logistics_components,
                Some(case_search_budget),
                fixed_dimensions,
                None,
            )
            .0;
            (layout, EndpointSupportPropagationStatistics::default())
        }
        (EndpointChannelEncoding::PositiveTable, false) => {
            let layout = exact::shared_layer::solve_positive_table_endpoints_fixed_dimensions_feasibility_only_with_local_continuation_guarded_intersection_propagation(
                input,
                logistics_components,
                Some(case_search_budget),
                fixed_dimensions,
            )
            .0;
            (layout, EndpointSupportPropagationStatistics::default())
        }
        (EndpointChannelEncoding::PositiveTable, true) => {
            let layout = exact::shared_layer::solve_tracked_positive_table_endpoints_fixed_dimensions_feasibility_only_with_local_continuation_guarded_intersection_propagation(
                input,
                logistics_components,
                Some(case_search_budget),
                fixed_dimensions,
            )
            .0;
            (layout, EndpointSupportPropagationStatistics::default())
        }
        (EndpointChannelEncoding::SparseSupport, false) => {
            let (layout, _, support) = exact::shared_layer::solve_sparse_support_endpoints_fixed_dimensions_feasibility_only_with_local_continuation_guarded_intersection_propagation(
                input,
                logistics_components,
                Some(case_search_budget),
                fixed_dimensions,
            );
            (layout, support)
        }
        _ => unreachable!("encoding and tracking mode were validated before the exact solve"),
    };
    let outer_wall_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let exact = layout
        .exact
        .as_ref()
        .expect("a constructed integrated endpoint-channel case has exact metrics");
    Ok(IntegratedEndpointChannelCaseReport {
        schema_version: INTEGRATED_ENDPOINT_CHANNEL_COMPARISON_SCHEMA_VERSION,
        target_phase_index,
        encoding,
        outcome: classify_outcome(&layout),
        fixed_dimensions: [fixed_width, fixed_height],
        case_search_budget_ms: case_search_budget.as_millis().min(u128::from(u64::MAX)) as u64,
        row_selector_tracking: track_row_selectors,
        endpoint_table_scale,
        endpoint_support_propagation,
        outer_wall_ms,
        construction_ms: exact.construction_ms,
        search_ms: exact.search_ms,
        first_incumbent_ms: exact.first_incumbent_ms,
        search_statistics: exact.search_statistics.clone(),
        layout,
        diagnostic_only: true,
    })
}

fn encoding_table_overhead(
    encoding: EndpointChannelEncoding,
    legal_tuple_rows: usize,
    estimated_table_clauses: usize,
) -> (usize, usize) {
    match encoding {
        EndpointChannelEncoding::NestedElement => (0, 0),
        EndpointChannelEncoding::PositiveTable => (legal_tuple_rows, estimated_table_clauses),
        EndpointChannelEncoding::SparseSupport => (0, 0),
        _ => unreachable!("integrated comparison validates its endpoint encoding"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_overhead_is_attributed_only_to_the_positive_table_encoding() {
        assert_eq!(
            encoding_table_overhead(EndpointChannelEncoding::NestedElement, 29_568, 107_608),
            (0, 0)
        );
        assert_eq!(
            encoding_table_overhead(EndpointChannelEncoding::PositiveTable, 29_568, 107_608),
            (29_568, 107_608)
        );
        assert_eq!(
            encoding_table_overhead(EndpointChannelEncoding::SparseSupport, 29_568, 107_608),
            (0, 0)
        );
    }
}
