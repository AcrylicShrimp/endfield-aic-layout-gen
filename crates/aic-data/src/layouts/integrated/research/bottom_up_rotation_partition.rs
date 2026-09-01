use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::FacilityPlacementRequest;
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{IntegratedLayoutDiagnostic, IntegratedLayoutReport, exact};
use super::bottom_up_ladder::prepare_bottom_up_phase;

pub const BOTTOM_UP_ROTATION_PARTITION_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpRotationPartitionReport {
    pub schema_version: u32,
    pub workload_id: Option<String>,
    pub workload_manifest_sha256: Option<String>,
    pub target_phase_index: usize,
    pub total_phase_count: usize,
    pub cumulative_facility_count: usize,
    pub introduced_facility_ids: Vec<String>,
    pub partitioned_rotation_domains: BTreeMap<String, Vec<i64>>,
    pub expected_case_count: usize,
    pub partition_complete: bool,
    pub cases_pairwise_disjoint: bool,
    pub worker_count: usize,
    pub case_search_budget_ms: u64,
    pub wall_time_ms: u64,
    pub first_feasible_wall_ms: Option<u64>,
    pub combined_outcome: exact::ladder::BottomUpRungOutcome,
    pub feasible_cases: usize,
    pub infeasible_cases: usize,
    pub unknown_cases: usize,
    pub invalid_cases: usize,
    pub cases: Vec<BottomUpRotationPartitionCaseReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BottomUpRotationPartitionCaseReport {
    pub case_index: usize,
    pub fixed_rotations: BTreeMap<String, i64>,
    pub rung: exact::ladder::BottomUpRungReport,
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose_bottom_up_rotation_partition(
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
    case_search_budget: Duration,
    worker_count: usize,
) -> Result<BottomUpRotationPartitionReport, IntegratedLayoutReport> {
    validate_partition_selection(partition_facility_ids, worker_count)
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
    let introduced = prepared
        .introduced_facility_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    validate_introduced_selection(partition_facility_ids, &introduced)
        .map_err(IntegratedLayoutReport::invalid)?;
    let mut domains = BTreeMap::new();
    for instance_id in partition_facility_ids {
        let Some(instance) = prepared
            .input
            .instances
            .iter()
            .find(|instance| instance.id == *instance_id)
        else {
            return Err(IntegratedLayoutReport::invalid(
                IntegratedLayoutDiagnostic::error(
                    "bottom-up-rotation-partition-facility-missing",
                    "/partition_facility_ids",
                    Some(instance_id.clone()),
                    "rotation partition facility is absent from the cumulative model",
                ),
            ));
        };
        let mut rotations = instance.definition.allowed_rotations.clone();
        rotations.sort_unstable();
        rotations.dedup();
        if rotations.is_empty() {
            return Err(IntegratedLayoutReport::invalid(
                IntegratedLayoutDiagnostic::error(
                    "bottom-up-rotation-partition-empty-domain",
                    "/partition_facility_ids",
                    Some(instance_id.clone()),
                    "rotation partition facility has no validated directional rotation",
                ),
            ));
        }
        domains.insert(instance_id.clone(), rotations);
    }

    let expected_case_count = domains
        .values()
        .try_fold(1_usize, |product, values| product.checked_mul(values.len()))
        .ok_or_else(|| {
            IntegratedLayoutReport::invalid(IntegratedLayoutDiagnostic::error(
                "bottom-up-rotation-partition-case-count-overflow",
                "/partition_facility_ids",
                None,
                "rotation partition case count exceeds the platform range",
            ))
        })?;

    let started = Instant::now();
    let next_case = AtomicUsize::new(0);
    let first_feasible_wall_ms = AtomicU64::new(u64::MAX);
    let mut result_storage = Vec::new();
    result_storage
        .try_reserve_exact(expected_case_count)
        .map_err(|_| {
            IntegratedLayoutReport::invalid(IntegratedLayoutDiagnostic::error(
                "bottom-up-rotation-partition-allocation-failed",
                "/partition_facility_ids",
                None,
                "rotation partition report storage cannot represent the requested case count",
            ))
        })?;
    let results = Mutex::new(result_storage);
    std::thread::scope(|scope| {
        for _ in 0..worker_count.min(expected_case_count) {
            let input = prepared.input.clone();
            let domains = &domains;
            let next_case = &next_case;
            let first_feasible_wall_ms = &first_feasible_wall_ms;
            let results = &results;
            scope.spawn(move || {
                loop {
                    let case_index = next_case.fetch_add(1, Ordering::Relaxed);
                    if case_index >= expected_case_count {
                        break;
                    }
                    let fixed_rotations =
                        rotation_case_at(case_index, partition_facility_ids, &domains);
                    let rung =
                        exact::ladder::solve_facility_ports_propagated_rung_with_fixed_rotations(
                            input.clone(),
                            case_search_budget,
                            endpoint_clearance_priority,
                            endpoint_clearance_counters_enabled,
                            endpoint_clearance_false_event_filter_enabled,
                            &fixed_rotations,
                        );
                    if rung.outcome == exact::ladder::BottomUpRungOutcome::Feasible {
                        first_feasible_wall_ms.fetch_min(
                            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                            Ordering::Relaxed,
                        );
                    }
                    results
                        .lock()
                        .expect("rotation partition result lock should not be poisoned")
                        .push(BottomUpRotationPartitionCaseReport {
                            case_index,
                            fixed_rotations,
                            rung,
                        });
                }
            });
        }
    });
    let mut cases = results
        .into_inner()
        .expect("rotation partition result lock should not be poisoned");
    cases.sort_by_key(|case| case.case_index);

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
    if !partition_complete || !cases_pairwise_disjoint {
        return Err(IntegratedLayoutReport::invalid(
            IntegratedLayoutDiagnostic::error(
                "bottom-up-rotation-partition-certificate-failed",
                "/cases",
                None,
                "rotation partition results do not cover the complete ordered Cartesian product",
            ),
        ));
    }

    let feasible_cases = count_outcome(&cases, exact::ladder::BottomUpRungOutcome::Feasible);
    let infeasible_cases = count_outcome(&cases, exact::ladder::BottomUpRungOutcome::Infeasible);
    let unknown_cases = count_outcome(&cases, exact::ladder::BottomUpRungOutcome::Unknown);
    let invalid_cases = count_outcome(&cases, exact::ladder::BottomUpRungOutcome::InvalidWitness);
    let combined_outcome = combine_partition_outcomes(
        expected_case_count,
        feasible_cases,
        infeasible_cases,
        invalid_cases,
    );

    Ok(BottomUpRotationPartitionReport {
        schema_version: BOTTOM_UP_ROTATION_PARTITION_SCHEMA_VERSION,
        workload_id: None,
        workload_manifest_sha256: None,
        target_phase_index: prepared.target_phase_index,
        total_phase_count: prepared.total_phase_count,
        cumulative_facility_count: prepared.cumulative_facility_count,
        introduced_facility_ids: prepared.introduced_facility_ids,
        partitioned_rotation_domains: domains,
        expected_case_count,
        partition_complete,
        cases_pairwise_disjoint,
        worker_count: worker_count.min(expected_case_count),
        case_search_budget_ms: u64::try_from(case_search_budget.as_millis()).unwrap_or(u64::MAX),
        wall_time_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        first_feasible_wall_ms: match first_feasible_wall_ms.load(Ordering::Relaxed) {
            u64::MAX => None,
            milliseconds => Some(milliseconds),
        },
        combined_outcome,
        feasible_cases,
        infeasible_cases,
        unknown_cases,
        invalid_cases,
        cases,
    })
}

fn validate_partition_selection(
    facility_ids: &[String],
    worker_count: usize,
) -> Result<(), IntegratedLayoutDiagnostic> {
    if facility_ids.is_empty() {
        return Err(IntegratedLayoutDiagnostic::error(
            "bottom-up-rotation-partition-empty",
            "/partition_facility_ids",
            None,
            "rotation partition requires at least one selected facility",
        ));
    }
    if worker_count == 0 {
        return Err(IntegratedLayoutDiagnostic::error(
            "bottom-up-rotation-partition-zero-workers",
            "/worker_count",
            None,
            "rotation partition worker count must be positive",
        ));
    }
    let selected = facility_ids.iter().collect::<BTreeSet<_>>();
    if selected.len() != facility_ids.len() {
        return Err(IntegratedLayoutDiagnostic::error(
            "bottom-up-rotation-partition-duplicate-facility",
            "/partition_facility_ids",
            None,
            "rotation partition facility IDs must be unique",
        ));
    }
    Ok(())
}

fn validate_introduced_selection(
    facility_ids: &[String],
    introduced: &BTreeSet<String>,
) -> Result<(), IntegratedLayoutDiagnostic> {
    if let Some(instance_id) = facility_ids
        .iter()
        .find(|instance_id| !introduced.contains(*instance_id))
    {
        return Err(IntegratedLayoutDiagnostic::error(
            "bottom-up-rotation-partition-facility-not-introduced",
            "/partition_facility_ids",
            Some(instance_id.clone()),
            "rotation partition facilities must be introduced by the selected phase",
        ));
    }
    Ok(())
}

fn combine_partition_outcomes(
    expected_case_count: usize,
    feasible_cases: usize,
    infeasible_cases: usize,
    invalid_cases: usize,
) -> exact::ladder::BottomUpRungOutcome {
    if feasible_cases > 0 {
        exact::ladder::BottomUpRungOutcome::Feasible
    } else if infeasible_cases == expected_case_count {
        exact::ladder::BottomUpRungOutcome::Infeasible
    } else if invalid_cases > 0 {
        exact::ladder::BottomUpRungOutcome::InvalidWitness
    } else {
        exact::ladder::BottomUpRungOutcome::Unknown
    }
}

fn rotation_case_at(
    case_index: usize,
    facility_ids: &[String],
    domains: &BTreeMap<String, Vec<i64>>,
) -> BTreeMap<String, i64> {
    let mut remaining = case_index;
    let mut case = BTreeMap::new();
    for facility_id in facility_ids.iter().rev() {
        let domain = &domains[facility_id];
        let value_index = remaining % domain.len();
        remaining /= domain.len();
        case.insert(facility_id.clone(), domain[value_index]);
    }
    debug_assert_eq!(remaining, 0);
    case
}

fn count_outcome(
    cases: &[BottomUpRotationPartitionCaseReport],
    outcome: exact::ladder::BottomUpRungOutcome,
) -> usize {
    cases
        .iter()
        .filter(|case| case.rung.outcome == outcome)
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_cases_are_the_complete_ordered_cartesian_product() {
        let facilities = vec!["seed".to_string(), "planter".to_string()];
        let domains = BTreeMap::from([
            ("seed".to_string(), vec![0, 90, 180, 270]),
            ("planter".to_string(), vec![0, 90]),
        ]);

        let cases = (0..8)
            .map(|case_index| rotation_case_at(case_index, &facilities, &domains))
            .collect::<Vec<_>>();

        assert_eq!(cases.len(), 8);
        assert_eq!(cases.iter().collect::<BTreeSet<_>>().len(), 8);
        assert_eq!(cases[0]["seed"], 0);
        assert_eq!(cases[0]["planter"], 0);
        assert_eq!(cases[7]["seed"], 270);
        assert_eq!(cases[7]["planter"], 90);
    }

    #[test]
    fn partition_selection_rejects_empty_duplicate_and_zero_worker_requests() {
        assert_eq!(
            validate_partition_selection(&[], 1)
                .expect_err("empty selection must fail")
                .code,
            "bottom-up-rotation-partition-empty"
        );
        assert_eq!(
            validate_partition_selection(&["seed".to_string()], 0)
                .expect_err("zero workers must fail")
                .code,
            "bottom-up-rotation-partition-zero-workers"
        );
        assert_eq!(
            validate_partition_selection(&["seed".to_string(), "seed".to_string()], 1)
                .expect_err("duplicate facilities must fail")
                .code,
            "bottom-up-rotation-partition-duplicate-facility"
        );
    }

    #[test]
    fn partition_selection_rejects_a_facility_outside_the_phase_frontier() {
        let introduced = BTreeSet::from(["seed".to_string()]);
        let diagnostic = validate_introduced_selection(
            &["seed".to_string(), "planter".to_string()],
            &introduced,
        )
        .expect_err("non-introduced facility must fail");

        assert_eq!(
            diagnostic.code,
            "bottom-up-rotation-partition-facility-not-introduced"
        );
        assert_eq!(diagnostic.entity.as_deref(), Some("planter"));
    }

    #[test]
    fn partition_outcome_requires_every_child_to_prove_infeasibility() {
        use exact::ladder::BottomUpRungOutcome;

        assert_eq!(
            combine_partition_outcomes(4, 1, 0, 1),
            BottomUpRungOutcome::Feasible
        );
        assert_eq!(
            combine_partition_outcomes(4, 0, 4, 0),
            BottomUpRungOutcome::Infeasible
        );
        assert_eq!(
            combine_partition_outcomes(4, 0, 3, 0),
            BottomUpRungOutcome::Unknown
        );
        assert_eq!(
            combine_partition_outcomes(4, 0, 3, 1),
            BottomUpRungOutcome::InvalidWitness
        );
    }
}
