use std::time::Duration;

use super::super::{
    ExactModelMetrics, ExactProofStatus, ExactSolveReport, ExactTerminationReason,
    ExactValidationStatus, IntegratedLayoutReport, IntegratedLayoutStatus,
};

pub(super) fn finish_report(
    mut report: IntegratedLayoutReport,
    model: ExactModelMetrics,
    construction_ms: u64,
    search_ms: u64,
    observed_incumbents: usize,
    validation: ExactValidationStatus,
) -> IntegratedLayoutReport {
    let (termination, proof) = match report.status {
        IntegratedLayoutStatus::Optimal => (
            ExactTerminationReason::Optimal,
            ExactProofStatus::ProvenOptimal,
        ),
        IntegratedLayoutStatus::Feasible => {
            (ExactTerminationReason::Feasible, ExactProofStatus::Unproven)
        }
        IntegratedLayoutStatus::Infeasible => (
            ExactTerminationReason::Infeasible,
            ExactProofStatus::ProvenInfeasible,
        ),
        IntegratedLayoutStatus::Unknown | IntegratedLayoutStatus::InvalidInput => {
            (ExactTerminationReason::Unknown, ExactProofStatus::Unproven)
        }
    };
    let objective_route_cells = report.success.then(|| {
        report
            .transport_networks
            .iter()
            .map(|network| network.cells.len())
            .sum()
    });
    report.exact = Some(ExactSolveReport {
        formulation: "joint-commodity-flow-v2",
        model,
        construction_ms,
        search_ms,
        incumbent_count: if report.success {
            observed_incumbents.max(1)
        } else {
            observed_incumbents
        },
        objective_route_cells,
        best_bound_route_cells: (proof == ExactProofStatus::ProvenOptimal)
            .then_some(objective_route_cells)
            .flatten(),
        termination,
        proof,
        validation,
    });
    report
}

pub(super) fn elapsed_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
