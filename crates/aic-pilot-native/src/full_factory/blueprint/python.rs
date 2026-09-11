//! Cold JSON boundary for the new contract; no legacy checkpoint adaptation.

use std::path::Path;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde_json::Value;

use super::*;
use crate::python_factory::{decode, load_catalogs};

fn error(value: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(value.to_string())
}
fn diagnostic(value: Diagnostic) -> PyErr {
    error(serde_json::to_string(&value).unwrap())
}
fn encode(value: &Value) -> PyResult<String> {
    serde_json::to_string(value).map_err(error)
}

#[pyclass(name = "BlueprintEditSession")]
pub struct PythonSession {
    session: Session,
}

#[pymethods]
impl PythonSession {
    #[staticmethod]
    fn open(
        py: Python<'_>,
        root: &str,
        catalogs_json: &str,
        request_json: &str,
        initial_json: &str,
        config_json: &str,
    ) -> PyResult<Self> {
        let (catalogs, _) = load_catalogs(Path::new(root), catalogs_json)?;
        let request = decode(request_json).map_err(error)?;
        let initial = decode(initial_json).map_err(error)?;
        let config = decode(config_json).map_err(error)?;
        let session = py
            .detach(|| Session::new(request, initial, catalogs, config))
            .map_err(diagnostic)?;
        Ok(Self { session })
    }

    #[staticmethod]
    fn build_info_json() -> &'static str {
        env!("AIC_PILOT_BUILD_INFO")
    }

    fn observation_json(&self, py: Python<'_>) -> PyResult<String> {
        encode(
            &py.detach(|| self.session.observation())
                .map_err(diagnostic)?,
        )
    }

    fn step_json(&mut self, py: Python<'_>, action_json: &str) -> PyResult<String> {
        encode(&py.detach(|| self.session.step_json(action_json)))
    }

    fn finish_json(&mut self, py: Python<'_>) -> PyResult<String> {
        encode(&py.detach(|| self.session.finish()))
    }

    fn validated_draft_json(&self, py: Python<'_>) -> PyResult<String> {
        encode(
            &py.detach(|| self.session.validated_draft())
                .map_err(diagnostic)?,
        )
    }
}
