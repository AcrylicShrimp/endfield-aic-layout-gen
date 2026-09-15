//! Semantic compatibility is independent of tensor dimensions and build hashes.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LearningContract {
    pub observation: String,
    pub execution: String,
    pub candidates: String,
    pub mask: String,
    pub family_prior: String,
    pub port_prefix: String,
    pub reward: String,
    pub frozen_rows: String,
}

pub fn current_contract() -> LearningContract {
    LearningContract {
        observation: "absolute-origin-graph-v1".into(),
        execution: "declarative-edit-v1".into(),
        candidates: "absolute-origin-candidates-v1".into(),
        mask: "structural-bands-v1".into(),
        family_prior: "all-root-count-v1".into(),
        port_prefix: "mean-port-features-v1".into(),
        reward: "canonical-area-delta-v1".into(),
        frozen_rows: "owned-graph-row-v1".into(),
    }
}

/// Inspect the bounded JSON header before deserializing any learned payload.
pub fn validate_envelope(value: &Value, schema: &str, path: &Path) -> Result<(), String> {
    let expected = serde_json::to_value(current_contract()).map_err(|e| e.to_string())?;
    if value["schema"] != schema || value["contract"] != expected {
        return Err(format!(
            "aic-learning.io-contract: {}: expected schema={schema} contract={expected}; actual schema={} contract={}; use the artifact's historical build or start fresh training with the current configuration",
            path.display(),
            value["schema"],
            value["contract"]
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_shapes_do_not_make_different_semantics_compatible() {
        let path = Path::new("checkpoint/manifest.json");
        let valid = serde_json::json!({"schema":"test-v2", "contract":current_contract(), "shape":[24,14,7,12,12]});
        assert!(validate_envelope(&valid, "test-v2", path).is_ok());
        for key in ["observation", "family_prior"] {
            let mut wrong = valid.clone();
            wrong["contract"][key] = "other-v1".into();
            let error = validate_envelope(&wrong, "test-v2", path).unwrap_err();
            assert!(error.contains("aic-learning.io-contract"));
            assert!(error.contains("checkpoint/manifest.json"));
            assert!(error.contains("expected") && error.contains("actual"));
        }
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("contract");
        assert!(validate_envelope(&missing, "test-v2", path).is_err());
        assert!(validate_envelope(&valid, "test-v1", path).is_err());
    }
}
