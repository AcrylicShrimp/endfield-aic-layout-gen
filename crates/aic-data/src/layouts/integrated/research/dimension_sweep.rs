use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use serde::Serialize;

use crate::facilities::ValidatedFacilityCatalog;
use crate::layouts::FacilityPlacementRequest;
use crate::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use crate::recipes::FacilityInstanceWiringReport;

use super::super::{
    ExactProofStatus, ExactValidationStatus, IntegratedLayoutDiagnostic, IntegratedLayoutReport,
    IntegratedLayoutStatus, exact, harness, prepare_exact_model,
};
use super::{
    ExactDimensionLowerBoundsReport, ExactUsedDimensionCandidate,
    enumerate_exact_dimension_candidates, exact_dimension_lower_bounds,
};

pub const PARALLEL_EXACT_DIMENSION_SWEEP_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExactDimensionCaseDisposition {
    Executed,
    SkippedAboveUpperBound,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExactDimensionCaseOutcome {
    ValidatedFeasible,
    ProvenInfeasible,
    Unknown,
    InvalidWitness,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ParallelExactDimensionCaseReport {
    pub candidate_index: usize,
    pub candidate: ExactUsedDimensionCandidate,
    pub disposition: ExactDimensionCaseDisposition,
    pub worker_index: usize,
    pub completion_order: usize,
    pub outcome: Option<ExactDimensionCaseOutcome>,
    pub layout: Option<IntegratedLayoutReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExactDimensionUpperBoundImprovement {
    pub completion_order: usize,
    pub worker_index: usize,
    pub candidate: ExactUsedDimensionCandidate,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ParallelExactDimensionSweepReport {
    pub schema_version: u32,
    pub selected_network_indices: Vec<usize>,
    pub selected_networks: Vec<String>,
    pub request_width: i32,
    pub request_height: i32,
    pub lower_bounds: ExactDimensionLowerBoundsReport,
    pub candidates: Vec<ExactUsedDimensionCandidate>,
    pub requested_worker_count: usize,
    pub actual_worker_count: usize,
    pub search_budget_ms_per_case: u64,
    pub outer_wall_ms: u64,
    pub cases: Vec<ParallelExactDimensionCaseReport>,
    pub upper_bound_improvements: Vec<ExactDimensionUpperBoundImprovement>,
    pub feasible_upper_bound_area: Option<i64>,
    pub unresolved_smaller_candidates: Vec<ExactUsedDimensionCandidate>,
    pub primary_area_optimum_proven: bool,
    pub complete_infeasibility_proven: bool,
    pub secondary_objectives_proven: bool,
    pub selected_incumbent: Option<IntegratedLayoutReport>,
    pub diagnostic_only: bool,
}

struct WorkItem {
    candidate_index: usize,
    candidate: ExactUsedDimensionCandidate,
}

struct CompletionEvent {
    candidate_index: usize,
    candidate: ExactUsedDimensionCandidate,
    disposition: ExactDimensionCaseDisposition,
    worker_index: usize,
    outcome: Option<ExactDimensionCaseOutcome>,
    layout: Option<IntegratedLayoutReport>,
    improved_upper_bound: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn sweep_first_integrated_layout_phase_fixed_dimensions(
    instance_wiring: &FacilityInstanceWiringReport,
    facilities: &ValidatedFacilityCatalog,
    items: &ValidatedItemCatalog,
    transports: &ValidatedTransportCatalog,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    request: &FacilityPlacementRequest,
    network_indices: &[usize],
    worker_count: usize,
    search_budget: Duration,
) -> Result<ParallelExactDimensionSweepReport, IntegratedLayoutReport> {
    if worker_count == 0 {
        return Err(invalid_sweep_input(
            "/worker_count",
            "parallel exact dimension sweep requires at least one worker",
        ));
    }
    if search_budget.is_zero() {
        return Err(invalid_sweep_input(
            "/search_budget",
            "parallel exact dimension sweep requires a positive per-case budget",
        ));
    }

    let first_phase_wiring = harness::first_iterative_scc_wiring(instance_wiring)?;
    let input = prepare_exact_model(
        &first_phase_wiring,
        facilities,
        items,
        transports,
        logistics_components,
        request,
    )?;
    let (input, selected_networks) = input
        .select_network_indices(network_indices)
        .map_err(IntegratedLayoutReport::invalid)?;
    let lower_bounds = exact_dimension_lower_bounds(&input)?;
    let candidates = enumerate_exact_dimension_candidates(input.width, input.height, &lower_bounds);
    let actual_worker_count = worker_count.min(candidates.len().max(1));
    let (work_sender, work_receiver) = unbounded::<WorkItem>();
    let (completion_sender, completion_receiver) = unbounded::<CompletionEvent>();
    for (candidate_index, candidate) in candidates.iter().cloned().enumerate() {
        work_sender
            .send(WorkItem {
                candidate_index,
                candidate,
            })
            .expect("fresh dimension work queue remains connected");
    }
    drop(work_sender);

    let best_area = AtomicI64::new(i64::MAX);
    let started = Instant::now();
    let mut cases = Vec::with_capacity(candidates.len());
    let mut upper_bound_improvements = Vec::new();
    let mut worker_failure = None;
    std::thread::scope(|scope| {
        for worker_index in 0..actual_worker_count {
            let work_receiver = work_receiver.clone();
            let completion_sender = completion_sender.clone();
            let input = &input;
            let best_area = &best_area;
            scope.spawn(move || {
                let result = catch_unwind(AssertUnwindSafe(|| {
                    run_worker(
                        worker_index,
                        work_receiver,
                        completion_sender.clone(),
                        input,
                        logistics_components,
                        search_budget,
                        best_area,
                    );
                }));
                if result.is_err() {
                    let _ = completion_sender.send(CompletionEvent {
                        candidate_index: usize::MAX,
                        candidate: ExactUsedDimensionCandidate {
                            width: 0,
                            height: 0,
                            area: 0,
                        },
                        disposition: ExactDimensionCaseDisposition::Executed,
                        worker_index,
                        outcome: None,
                        layout: None,
                        improved_upper_bound: false,
                    });
                }
            });
        }
        drop(completion_sender);

        for (completion_order, event) in completion_receiver.iter().enumerate() {
            if event.candidate_index == usize::MAX {
                worker_failure = Some(event.worker_index);
                continue;
            }
            if event.improved_upper_bound {
                upper_bound_improvements.push(ExactDimensionUpperBoundImprovement {
                    completion_order,
                    worker_index: event.worker_index,
                    candidate: event.candidate.clone(),
                });
            }
            cases.push(ParallelExactDimensionCaseReport {
                candidate_index: event.candidate_index,
                candidate: event.candidate,
                disposition: event.disposition,
                worker_index: event.worker_index,
                completion_order,
                outcome: event.outcome,
                layout: event.layout,
            });
        }
    });
    if let Some(worker_index) = worker_failure {
        return Err(invalid_sweep_input(
            "/workers",
            format!("parallel exact dimension worker {worker_index} panicked"),
        ));
    }
    cases.sort_by_key(|case| case.candidate_index);

    let feasible_upper_bound_area = match best_area.load(Ordering::Acquire) {
        i64::MAX => None,
        area => Some(area),
    };
    let unresolved_smaller_candidates =
        unresolved_smaller_candidates(&candidates, &cases, feasible_upper_bound_area);
    let primary_area_optimum_proven =
        feasible_upper_bound_area.is_some() && unresolved_smaller_candidates.is_empty();
    let complete_infeasibility_proven = feasible_upper_bound_area.is_none()
        && cases
            .iter()
            .all(|case| case.outcome == Some(ExactDimensionCaseOutcome::ProvenInfeasible));
    let selected_incumbent = select_incumbent(&cases, feasible_upper_bound_area);

    Ok(ParallelExactDimensionSweepReport {
        schema_version: PARALLEL_EXACT_DIMENSION_SWEEP_SCHEMA_VERSION,
        selected_network_indices: network_indices.to_vec(),
        selected_networks,
        request_width: input.width,
        request_height: input.height,
        lower_bounds,
        candidates,
        requested_worker_count: worker_count,
        actual_worker_count,
        search_budget_ms_per_case: millis(search_budget),
        outer_wall_ms: millis(started.elapsed()),
        cases,
        upper_bound_improvements,
        feasible_upper_bound_area,
        unresolved_smaller_candidates,
        primary_area_optimum_proven,
        complete_infeasibility_proven,
        secondary_objectives_proven: false,
        selected_incumbent,
        diagnostic_only: true,
    })
}

fn run_worker(
    worker_index: usize,
    work_receiver: Receiver<WorkItem>,
    completion_sender: Sender<CompletionEvent>,
    input: &super::super::ModelInput,
    logistics_components: &ValidatedLogisticsComponentCatalog,
    search_budget: Duration,
    best_area: &AtomicI64,
) {
    while let Ok(work) = work_receiver.recv() {
        if work.candidate.area > best_area.load(Ordering::Acquire) {
            if completion_sender
                .send(CompletionEvent {
                    candidate_index: work.candidate_index,
                    candidate: work.candidate,
                    disposition: ExactDimensionCaseDisposition::SkippedAboveUpperBound,
                    worker_index,
                    outcome: None,
                    layout: None,
                    improved_upper_bound: false,
                })
                .is_err()
            {
                return;
            }
            continue;
        }

        let layout =
            exact::shared_layer::solve_factored_endpoints_fixed_dimensions_feasibility_only(
                input.clone(),
                logistics_components,
                Some(search_budget),
                exact::shared_layer::FixedUsedDimensions {
                    width: work.candidate.width,
                    height: work.candidate.height,
                },
            );
        let outcome = classify_outcome(&layout);
        let improved_upper_bound = outcome == ExactDimensionCaseOutcome::ValidatedFeasible
            && lower_upper_bound(best_area, work.candidate.area);
        if completion_sender
            .send(CompletionEvent {
                candidate_index: work.candidate_index,
                candidate: work.candidate,
                disposition: ExactDimensionCaseDisposition::Executed,
                worker_index,
                outcome: Some(outcome),
                layout: Some(layout),
                improved_upper_bound,
            })
            .is_err()
        {
            return;
        }
    }
}

fn classify_outcome(layout: &IntegratedLayoutReport) -> ExactDimensionCaseOutcome {
    if layout.success
        && layout
            .exact
            .as_ref()
            .is_some_and(|exact| exact.validation == ExactValidationStatus::Passed)
    {
        ExactDimensionCaseOutcome::ValidatedFeasible
    } else if layout.status == IntegratedLayoutStatus::Infeasible
        && layout
            .exact
            .as_ref()
            .is_some_and(|exact| exact.proof == ExactProofStatus::ProvenInfeasible)
    {
        ExactDimensionCaseOutcome::ProvenInfeasible
    } else if layout
        .exact
        .as_ref()
        .is_some_and(|exact| exact.validation == ExactValidationStatus::Failed)
    {
        ExactDimensionCaseOutcome::InvalidWitness
    } else {
        ExactDimensionCaseOutcome::Unknown
    }
}

fn lower_upper_bound(best_area: &AtomicI64, candidate_area: i64) -> bool {
    let mut observed = best_area.load(Ordering::Acquire);
    loop {
        if candidate_area >= observed {
            return false;
        }
        match best_area.compare_exchange_weak(
            observed,
            candidate_area,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return true,
            Err(current) => observed = current,
        }
    }
}

fn unresolved_smaller_candidates(
    candidates: &[ExactUsedDimensionCandidate],
    cases: &[ParallelExactDimensionCaseReport],
    upper_bound: Option<i64>,
) -> Vec<ExactUsedDimensionCandidate> {
    let Some(upper_bound) = upper_bound else {
        return Vec::new();
    };
    candidates
        .iter()
        .filter(|candidate| candidate.area < upper_bound)
        .filter(|candidate| {
            cases
                .iter()
                .find(|case| case.candidate_index == candidate_index(candidates, candidate))
                .is_none_or(|case| {
                    case.outcome != Some(ExactDimensionCaseOutcome::ProvenInfeasible)
                })
        })
        .cloned()
        .collect()
}

fn candidate_index(
    candidates: &[ExactUsedDimensionCandidate],
    candidate: &ExactUsedDimensionCandidate,
) -> usize {
    candidates
        .iter()
        .position(|item| item == candidate)
        .expect("candidate originates from complete dimension list")
}

fn select_incumbent(
    cases: &[ParallelExactDimensionCaseReport],
    upper_bound: Option<i64>,
) -> Option<IntegratedLayoutReport> {
    let upper_bound = upper_bound?;
    cases
        .iter()
        .filter(|case| {
            case.candidate.area == upper_bound
                && case.outcome == Some(ExactDimensionCaseOutcome::ValidatedFeasible)
        })
        .filter_map(|case| case.layout.as_ref())
        .min_by_key(|layout| {
            layout
                .exact
                .as_ref()
                .and_then(|exact| exact.objective)
                .map(|objective| {
                    (
                        objective.physical_transport_tiles,
                        objective.total_route_turns,
                        objective.maximum_used_side,
                        objective.logistics_component_count,
                    )
                })
                .unwrap_or((usize::MAX, usize::MAX, i64::MAX, usize::MAX))
        })
        .cloned()
}

fn invalid_sweep_input(
    path: impl Into<String>,
    message: impl Into<String>,
) -> IntegratedLayoutReport {
    IntegratedLayoutReport::invalid(IntegratedLayoutDiagnostic::error(
        "invalid-parallel-exact-dimension-sweep",
        path,
        None,
        message,
    ))
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(width: i32, height: i32) -> ExactUsedDimensionCandidate {
        ExactUsedDimensionCandidate {
            width,
            height,
            area: i64::from(width) * i64::from(height),
        }
    }

    fn case(
        index: usize,
        candidate: ExactUsedDimensionCandidate,
        disposition: ExactDimensionCaseDisposition,
        outcome: Option<ExactDimensionCaseOutcome>,
    ) -> ParallelExactDimensionCaseReport {
        ParallelExactDimensionCaseReport {
            candidate_index: index,
            candidate,
            disposition,
            worker_index: 0,
            completion_order: index,
            outcome,
            layout: None,
        }
    }

    #[test]
    fn upper_bound_updates_are_visible_immediately_and_only_improve() {
        let upper_bound = AtomicI64::new(i64::MAX);
        assert!(lower_upper_bound(&upper_bound, 42));
        assert_eq!(upper_bound.load(Ordering::Acquire), 42);
        assert!(!lower_upper_bound(&upper_bound, 49));
        assert!(!lower_upper_bound(&upper_bound, 42));
        assert!(lower_upper_bound(&upper_bound, 35));
        assert_eq!(upper_bound.load(Ordering::Acquire), 35);
    }

    #[test]
    fn only_proven_infeasible_smaller_cases_close_the_primary_gap() {
        let candidates = vec![candidate(5, 6), candidate(6, 5), candidate(5, 7)];
        let cases = vec![
            case(
                0,
                candidates[0].clone(),
                ExactDimensionCaseDisposition::Executed,
                Some(ExactDimensionCaseOutcome::ProvenInfeasible),
            ),
            case(
                1,
                candidates[1].clone(),
                ExactDimensionCaseDisposition::Executed,
                Some(ExactDimensionCaseOutcome::Unknown),
            ),
            case(
                2,
                candidates[2].clone(),
                ExactDimensionCaseDisposition::Executed,
                Some(ExactDimensionCaseOutcome::ValidatedFeasible),
            ),
        ];

        assert_eq!(
            unresolved_smaller_candidates(&candidates, &cases, Some(35)),
            vec![candidate(6, 5)]
        );
    }

    #[test]
    fn skipped_cases_above_the_upper_bound_do_not_block_primary_proof() {
        let candidates = vec![candidate(5, 6), candidate(5, 7), candidate(6, 6)];
        let cases = vec![
            case(
                0,
                candidates[0].clone(),
                ExactDimensionCaseDisposition::Executed,
                Some(ExactDimensionCaseOutcome::ProvenInfeasible),
            ),
            case(
                1,
                candidates[1].clone(),
                ExactDimensionCaseDisposition::Executed,
                Some(ExactDimensionCaseOutcome::ValidatedFeasible),
            ),
            case(
                2,
                candidates[2].clone(),
                ExactDimensionCaseDisposition::SkippedAboveUpperBound,
                None,
            ),
        ];

        assert!(unresolved_smaller_candidates(&candidates, &cases, Some(35)).is_empty());
    }
}
