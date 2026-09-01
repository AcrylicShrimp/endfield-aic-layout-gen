use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::FacilityPlacementRequest;
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{IntegratedLayoutReport, exact};
use super::bottom_up_ladder::prepare_bottom_up_phase;
use super::bottom_up_rotation_partition::{
    checked_rotation_case_count, rotation_case_at, validate_partition_selection,
    validated_rotation_domains,
};

pub const BOTTOM_UP_ROTATION_ROOT_COMPARISON_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpRotationRootComparisonReport {
    pub schema_version: u32,
    pub workload_id: Option<String>,
    pub workload_manifest_sha256: Option<String>,
    pub target_phase_index: usize,
    pub total_phase_count: usize,
    pub cumulative_facility_count: usize,
    pub introduced_facility_ids: Vec<String>,
    pub endpoint_clearance_priority: exact::ladder::EndpointClearanceSchedulingPriority,
    pub endpoint_clearance_counters_enabled: bool,
    pub endpoint_clearance_false_event_filter_enabled: bool,
    pub partitioned_rotation_domains: BTreeMap<String, Vec<i64>>,
    pub expected_case_count: usize,
    pub partition_complete: bool,
    pub cases_pairwise_disjoint: bool,
    pub parent: exact::ladder::BottomUpRootDomainSnapshot,
    pub cases: Vec<BottomUpRotationRootCaseReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpRotationRootCaseReport {
    pub case_index: usize,
    pub fixed_rotations: BTreeMap<String, i64>,
    pub snapshot: exact::ladder::BottomUpRootDomainSnapshot,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_bottom_up_rotation_root_comparison(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    target_phase_index: usize,
    partition_facility_ids: &[String],
    endpoint_clearance_priority: exact::ladder::EndpointClearanceSchedulingPriority,
    endpoint_clearance_counters_enabled: bool,
    endpoint_clearance_false_event_filter_enabled: bool,
) -> Result<BottomUpRotationRootComparisonReport, IntegratedLayoutReport> {
    validate_partition_selection(partition_facility_ids, 1)
        .map_err(IntegratedLayoutReport::invalid)?;
    let prepared = prepare_bottom_up_phase(
        instance_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
        target_phase_index,
    )?;
    let domains = validated_rotation_domains(&prepared, partition_facility_ids)
        .map_err(IntegratedLayoutReport::invalid)?;
    let expected_case_count =
        checked_rotation_case_count(&domains).map_err(IntegratedLayoutReport::invalid)?;
    let parent = exact::ladder::snapshot_facility_ports_propagated_root(
        prepared.input.clone(),
        endpoint_clearance_priority,
        endpoint_clearance_counters_enabled,
        endpoint_clearance_false_event_filter_enabled,
        &BTreeMap::new(),
    )
    .map_err(IntegratedLayoutReport::invalid)?;
    let mut cases = Vec::new();
    cases.try_reserve_exact(expected_case_count).map_err(|_| {
        IntegratedLayoutReport::invalid(super::super::IntegratedLayoutDiagnostic::error(
            "bottom-up-rotation-root-allocation-failed",
            "/partition_facility_ids",
            None,
            "rotation root comparison cannot reserve the requested case report storage",
        ))
    })?;
    for case_index in 0..expected_case_count {
        let fixed_rotations = rotation_case_at(case_index, partition_facility_ids, &domains);
        let snapshot = exact::ladder::snapshot_facility_ports_propagated_root(
            prepared.input.clone(),
            endpoint_clearance_priority,
            endpoint_clearance_counters_enabled,
            endpoint_clearance_false_event_filter_enabled,
            &fixed_rotations,
        )
        .map_err(IntegratedLayoutReport::invalid)?;
        cases.push(BottomUpRotationRootCaseReport {
            case_index,
            fixed_rotations,
            snapshot,
        });
    }
    let partition_complete = cases.len() == expected_case_count
        && cases.iter().enumerate().all(|(case_index, case)| {
            case.case_index == case_index
                && case.fixed_rotations
                    == rotation_case_at(case_index, partition_facility_ids, &domains)
        });
    let cases_pairwise_disjoint = cases
        .iter()
        .map(|case| &case.fixed_rotations)
        .collect::<BTreeSet<_>>()
        .len()
        == cases.len();

    Ok(BottomUpRotationRootComparisonReport {
        schema_version: BOTTOM_UP_ROTATION_ROOT_COMPARISON_SCHEMA_VERSION,
        workload_id: None,
        workload_manifest_sha256: None,
        target_phase_index: prepared.target_phase_index,
        total_phase_count: prepared.total_phase_count,
        cumulative_facility_count: prepared.cumulative_facility_count,
        introduced_facility_ids: prepared.introduced_facility_ids,
        endpoint_clearance_priority,
        endpoint_clearance_counters_enabled,
        endpoint_clearance_false_event_filter_enabled,
        partitioned_rotation_domains: domains,
        expected_case_count,
        partition_complete,
        cases_pairwise_disjoint,
        parent,
        cases,
    })
}
