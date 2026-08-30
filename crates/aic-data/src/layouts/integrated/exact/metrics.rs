use std::time::Duration;

use super::super::{
    ExactModelMetrics, ExactObjectiveStageReport, ExactObjectiveValue, ExactProofStatus,
    ExactSolveReport, ExactTerminationReason, ExactValidationStatus, IntegratedLayoutReport,
    IntegratedLayoutStatus, LayoutScore,
};
use crate::research::ModelComplexityMetrics;

pub(super) fn finish_report(
    mut report: IntegratedLayoutReport,
    model: ExactModelMetrics,
    model_complexity: ModelComplexityMetrics,
    construction_ms: u64,
    search_ms: u64,
    first_incumbent_ms: Option<u64>,
    observed_incumbents: usize,
    validation: ExactValidationStatus,
    objective_stages: Vec<ExactObjectiveStageReport>,
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
    let objective = LayoutScore::from_report(&report, &[]).map(|score| ExactObjectiveValue {
        used_bounding_box_area: score.used_bounding_box_area,
        physical_transport_tiles: score.physical_transport_tiles,
        total_route_turns: score.total_route_turns,
        maximum_used_side: score.maximum_used_side,
        logistics_component_count: score.logistics_component_count,
    });
    report.exact = Some(ExactSolveReport {
        formulation: "joint-lexicographic-layout-v4",
        model,
        model_complexity,
        construction_ms,
        search_ms,
        first_incumbent_ms,
        incumbent_count: if report.success {
            observed_incumbents.max(1)
        } else {
            observed_incumbents
        },
        objective,
        objective_stages,
        termination,
        proof,
        validation,
    });
    report
}

pub(super) fn elapsed_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
