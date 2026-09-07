//! Stable errors shared by catalog admission, state validation and engine APIs.

use serde::{Deserialize, Serialize};

pub type Result<T> = std::result::Result<T, Diagnostic>;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub stage: String,
    pub message: String,
    pub entity: Option<String>,
}

impl Diagnostic {
    pub fn new(code: &str, stage: &str, message: impl Into<String>, entity: Option<&str>) -> Self {
        Self {
            code: code.into(),
            severity: "error".into(),
            stage: stage.into(),
            message: message.into(),
            entity: entity.map(str::to_owned),
        }
    }

    pub fn acceptance(code: &str, message: &str, entity: Option<&str>) -> Self {
        Self::new(code, "acceptance", message, entity)
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)?;
        if let Some(entity) = &self.entity {
            write!(f, " [{entity}]")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostic {}

pub fn require(condition: bool, code: &str, message: &str, entity: Option<&str>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Diagnostic::acceptance(code, message, entity))
    }
}
