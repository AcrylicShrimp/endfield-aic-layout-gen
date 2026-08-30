use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const ITERATIVE_OPTIMIZATION_CONFIG_SCHEMA_VERSION: u32 = 1;
pub const CANDIDATE_POLICY_TABLE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IterativeOptimizationConfig {
    pub schema_version: u32,
    pub total_time_limit_ms: u64,
    pub final_refinement_reserve_percent: u8,
    pub minimum_phase_attempt_ms: u64,
    pub max_new_facilities_per_phase: usize,
    pub candidate_attempts_per_neighborhood: usize,
    pub same_neighborhood_restart_limit: usize,
}

impl Default for IterativeOptimizationConfig {
    fn default() -> Self {
        Self {
            schema_version: ITERATIVE_OPTIMIZATION_CONFIG_SCHEMA_VERSION,
            total_time_limit_ms: 30_000,
            final_refinement_reserve_percent: 20,
            minimum_phase_attempt_ms: 250,
            max_new_facilities_per_phase: 8,
            candidate_attempts_per_neighborhood: 3,
            same_neighborhood_restart_limit: 1,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CandidatePolicyTable {
    pub schema_version: u32,
    pub policies: Vec<CandidatePolicy>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CandidatePolicy {
    pub id: String,
    pub placement_policy: PlacementPolicy,
    pub routing_order_policy: RoutingOrderPolicy,
    pub max_candidate_yields: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PlacementPolicy {
    PriorHint,
    CompactShelf,
    AlternatingShelf,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RoutingOrderPolicy {
    FacilityFirst,
    ExternalFirst,
    NetworkFirst,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OptimizationConfigDiagnostic {
    pub stage: &'static str,
    pub severity: &'static str,
    pub code: &'static str,
    pub path: String,
    pub message: String,
}

impl OptimizationConfigDiagnostic {
    fn error(code: &'static str, path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            stage: "iterative-optimization-config",
            severity: "error",
            code,
            path: path.into(),
            message: message.into(),
        }
    }
}

pub fn validate_iterative_optimization_config(
    config: &IterativeOptimizationConfig,
) -> Result<(), Vec<OptimizationConfigDiagnostic>> {
    let mut diagnostics = Vec::new();
    if config.schema_version != ITERATIVE_OPTIMIZATION_CONFIG_SCHEMA_VERSION {
        diagnostics.push(OptimizationConfigDiagnostic::error(
            "unsupported-iterative-optimization-config-schema-version",
            "/schema_version",
            format!(
                "iterative optimization config schema version {} is unsupported; expected {}",
                config.schema_version, ITERATIVE_OPTIMIZATION_CONFIG_SCHEMA_VERSION,
            ),
        ));
    }
    require_positive(
        config.total_time_limit_ms,
        "/total_time_limit_ms",
        "total-time-limit-must-be-positive",
        &mut diagnostics,
    );
    if config.final_refinement_reserve_percent > 50 {
        diagnostics.push(OptimizationConfigDiagnostic::error(
            "final-refinement-reserve-percent-out-of-range",
            "/final_refinement_reserve_percent",
            "final refinement reserve percent must be between 0 and 50 inclusive",
        ));
    }
    require_positive(
        config.minimum_phase_attempt_ms,
        "/minimum_phase_attempt_ms",
        "minimum-phase-attempt-must-be-positive",
        &mut diagnostics,
    );
    require_positive(
        config.max_new_facilities_per_phase,
        "/max_new_facilities_per_phase",
        "max-new-facilities-per-phase-must-be-positive",
        &mut diagnostics,
    );
    require_positive(
        config.candidate_attempts_per_neighborhood,
        "/candidate_attempts_per_neighborhood",
        "candidate-attempts-per-neighborhood-must-be-positive",
        &mut diagnostics,
    );
    if config.same_neighborhood_restart_limit > 1 {
        diagnostics.push(OptimizationConfigDiagnostic::error(
            "same-neighborhood-restart-limit-out-of-range",
            "/same_neighborhood_restart_limit",
            "same-neighborhood restart limit must be zero or one for the deterministic MVP",
        ));
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

pub fn validate_candidate_policy_table(
    table: &CandidatePolicyTable,
) -> Result<(), Vec<OptimizationConfigDiagnostic>> {
    let mut diagnostics = Vec::new();
    if table.schema_version != CANDIDATE_POLICY_TABLE_SCHEMA_VERSION {
        diagnostics.push(OptimizationConfigDiagnostic::error(
            "unsupported-candidate-policy-table-schema-version",
            "/schema_version",
            format!(
                "candidate policy table schema version {} is unsupported; expected {}",
                table.schema_version, CANDIDATE_POLICY_TABLE_SCHEMA_VERSION,
            ),
        ));
    }
    if table.policies.is_empty() {
        diagnostics.push(OptimizationConfigDiagnostic::error(
            "candidate-policy-table-empty",
            "/policies",
            "candidate policy table must contain at least one policy",
        ));
    }
    let mut ids = BTreeSet::new();
    for (index, policy) in table.policies.iter().enumerate() {
        if policy.id.is_empty() {
            diagnostics.push(OptimizationConfigDiagnostic::error(
                "candidate-policy-id-empty",
                format!("/policies/{index}/id"),
                "candidate policy ID must not be empty",
            ));
        } else if !ids.insert(policy.id.as_str()) {
            diagnostics.push(OptimizationConfigDiagnostic::error(
                "candidate-policy-id-duplicate",
                format!("/policies/{index}/id"),
                format!("candidate policy ID '{}' appears more than once", policy.id),
            ));
        }
        if policy.max_candidate_yields == 0 {
            diagnostics.push(OptimizationConfigDiagnostic::error(
                "candidate-policy-yield-limit-must-be-positive",
                format!("/policies/{index}/max_candidate_yields"),
                "candidate policy max candidate yields must be positive",
            ));
        }
    }
    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(diagnostics)
    }
}

fn require_positive<T>(
    value: T,
    path: &'static str,
    code: &'static str,
    diagnostics: &mut Vec<OptimizationConfigDiagnostic>,
) where
    T: Default + PartialEq,
{
    if value == T::default() {
        diagnostics.push(OptimizationConfigDiagnostic::error(
            code,
            path,
            "value must be positive",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_matches_the_accepted_budget_contract() {
        let config = IterativeOptimizationConfig::default();
        assert_eq!(config.total_time_limit_ms, 30_000);
        assert_eq!(config.final_refinement_reserve_percent, 20);
        assert_eq!(config.minimum_phase_attempt_ms, 250);
        assert_eq!(config.max_new_facilities_per_phase, 8);
        assert_eq!(config.candidate_attempts_per_neighborhood, 3);
        assert_eq!(config.same_neighborhood_restart_limit, 1);
        assert!(validate_iterative_optimization_config(&config).is_ok());
    }

    #[test]
    fn reports_every_invalid_config_field_in_one_machine_readable_pass() {
        let diagnostics = validate_iterative_optimization_config(&IterativeOptimizationConfig {
            schema_version: 99,
            total_time_limit_ms: 0,
            final_refinement_reserve_percent: 51,
            minimum_phase_attempt_ms: 0,
            max_new_facilities_per_phase: 0,
            candidate_attempts_per_neighborhood: 0,
            same_neighborhood_restart_limit: 2,
        })
        .expect_err("invalid config should fail");
        assert_eq!(diagnostics.len(), 7);
        assert!(diagnostics.iter().all(|diagnostic| diagnostic.stage
            == "iterative-optimization-config"
            && diagnostic.severity == "error"
            && diagnostic.path.starts_with('/')));
    }

    #[test]
    fn rejects_unknown_config_fields_during_deserialization() {
        let error = serde_json::from_str::<IterativeOptimizationConfig>(
            r#"{
            "schema_version": 1,
            "total_time_limit_ms": 30000,
            "final_refinement_reserve_percent": 20,
            "minimum_phase_attempt_ms": 250,
            "max_new_facilities_per_phase": 8,
            "candidate_attempts_per_neighborhood": 3,
            "same_neighborhood_restart_limit": 1,
            "extra": true
        }"#,
        )
        .expect_err("unknown config fields must be rejected");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn validates_policy_identity_order_and_yield_quotas() {
        let table = CandidatePolicyTable {
            schema_version: CANDIDATE_POLICY_TABLE_SCHEMA_VERSION,
            policies: vec![
                CandidatePolicy {
                    id: "compact".to_string(),
                    placement_policy: PlacementPolicy::CompactShelf,
                    routing_order_policy: RoutingOrderPolicy::FacilityFirst,
                    max_candidate_yields: 1,
                },
                CandidatePolicy {
                    id: "compact".to_string(),
                    placement_policy: PlacementPolicy::PriorHint,
                    routing_order_policy: RoutingOrderPolicy::NetworkFirst,
                    max_candidate_yields: 0,
                },
            ],
        };
        let diagnostics =
            validate_candidate_policy_table(&table).expect_err("invalid policy table should fail");
        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].code, "candidate-policy-id-duplicate");
        assert_eq!(
            diagnostics[1].code,
            "candidate-policy-yield-limit-must-be-positive"
        );
    }
}
