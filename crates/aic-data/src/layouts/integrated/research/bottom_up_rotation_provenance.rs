use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

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

pub const BOTTOM_UP_ROTATION_PROVENANCE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpRotationProvenanceReport {
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
    pub search_budget_ms: u64,
    pub maximum_detailed_decisions: usize,
    pub partitioned_rotation_domains: BTreeMap<String, Vec<i64>>,
    pub expected_case_count: usize,
    pub partition_complete: bool,
    pub cases_pairwise_disjoint: bool,
    pub parent: BottomUpRotationProvenanceCaseReport,
    pub cases: Vec<BottomUpRotationProvenanceCaseReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpRotationProvenanceCaseReport {
    pub case_index: Option<usize>,
    pub fixed_rotations: BTreeMap<String, i64>,
    pub rung: exact::ladder::BottomUpRungReport,
    pub trace: exact::ladder::BottomUpSearchProvenanceTrace,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_bottom_up_rotation_provenance(
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
    search_budget: Duration,
    maximum_detailed_decisions: usize,
) -> Result<BottomUpRotationProvenanceReport, IntegratedLayoutReport> {
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
    let target_instance = &partition_facility_ids[0];
    let (parent_rung, parent_trace) = exact::ladder::solve_facility_ports_search_provenance(
        prepared.input.clone(),
        search_budget,
        endpoint_clearance_priority,
        endpoint_clearance_counters_enabled,
        endpoint_clearance_false_event_filter_enabled,
        &BTreeMap::new(),
        target_instance,
        maximum_detailed_decisions,
    );
    let parent = BottomUpRotationProvenanceCaseReport {
        case_index: None,
        fixed_rotations: BTreeMap::new(),
        rung: parent_rung,
        trace: parent_trace,
    };

    let mut cases = Vec::new();
    cases.try_reserve_exact(expected_case_count).map_err(|_| {
        IntegratedLayoutReport::invalid(super::super::IntegratedLayoutDiagnostic::error(
            "bottom-up-rotation-provenance-allocation-failed",
            "/partition_facility_ids",
            None,
            "rotation provenance cannot reserve the requested case report storage",
        ))
    })?;
    for case_index in 0..expected_case_count {
        let fixed_rotations = rotation_case_at(case_index, partition_facility_ids, &domains);
        let (rung, trace) = exact::ladder::solve_facility_ports_search_provenance(
            prepared.input.clone(),
            search_budget,
            endpoint_clearance_priority,
            endpoint_clearance_counters_enabled,
            endpoint_clearance_false_event_filter_enabled,
            &fixed_rotations,
            target_instance,
            maximum_detailed_decisions,
        );
        cases.push(BottomUpRotationProvenanceCaseReport {
            case_index: Some(case_index),
            fixed_rotations,
            rung,
            trace,
        });
    }
    let partition_complete = cases.len() == expected_case_count
        && cases.iter().enumerate().all(|(case_index, case)| {
            case.case_index == Some(case_index)
                && case.fixed_rotations
                    == rotation_case_at(case_index, partition_facility_ids, &domains)
        });
    let cases_pairwise_disjoint = cases
        .iter()
        .map(|case| &case.fixed_rotations)
        .collect::<BTreeSet<_>>()
        .len()
        == cases.len();

    Ok(BottomUpRotationProvenanceReport {
        schema_version: BOTTOM_UP_ROTATION_PROVENANCE_SCHEMA_VERSION,
        workload_id: None,
        workload_manifest_sha256: None,
        target_phase_index: prepared.target_phase_index,
        total_phase_count: prepared.total_phase_count,
        cumulative_facility_count: prepared.cumulative_facility_count,
        introduced_facility_ids: prepared.introduced_facility_ids,
        endpoint_clearance_priority,
        endpoint_clearance_counters_enabled,
        endpoint_clearance_false_event_filter_enabled,
        search_budget_ms: u64::try_from(search_budget.as_millis()).unwrap_or(u64::MAX),
        maximum_detailed_decisions,
        partitioned_rotation_domains: domains,
        expected_case_count,
        partition_complete,
        cases_pairwise_disjoint,
        parent,
        cases,
    })
}
