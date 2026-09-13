//! Explicit experimental API; existing learners cannot select it implicitly.

use std::path::Path;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

use super::*;
use crate::python_factory::{decode, load_catalogs};

fn diagnostic(value: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(value.to_string())
}
fn native_error(value: Diagnostic) -> PyErr {
    diagnostic(serde_json::to_string(&value).unwrap())
}
fn encode(value: &Value) -> PyResult<String> {
    serde_json::to_string(value).map_err(diagnostic)
}

#[pyclass(name = "DeclarativeEditSession")]
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
        let request = decode(request_json).map_err(diagnostic)?;
        let initial = decode(initial_json).map_err(diagnostic)?;
        let config = decode(config_json).map_err(diagnostic)?;
        let session = py
            .detach(|| Session::new(request, initial, catalogs, config))
            .map_err(native_error)?;
        Ok(Self { session })
    }
    #[staticmethod]
    fn build_info_json() -> &'static str {
        env!("AIC_PILOT_BUILD_INFO")
    }
    fn observation_json(&self, py: Python<'_>) -> PyResult<String> {
        encode(
            &py.detach(|| self.session.observation())
                .map_err(native_error)?,
        )
    }
    fn step_json(&mut self, py: Python<'_>, action_json: &str) -> PyResult<String> {
        encode(&py.detach(|| self.session.step_json(action_json)))
    }
    fn finish_json(&mut self, py: Python<'_>) -> PyResult<String> {
        encode(&py.detach(|| self.session.finish()))
    }
    fn checkpoint_json(&self, py: Python<'_>) -> PyResult<String> {
        encode(&py.detach(|| self.session.checkpoint()))
    }
    fn validated_draft_json(&self, py: Python<'_>) -> PyResult<String> {
        encode(
            &py.detach(|| self.session.validated_draft())
                .map_err(native_error)?,
        )
    }
    #[staticmethod]
    fn from_checkpoint(
        py: Python<'_>,
        root: &str,
        catalogs_json: &str,
        checkpoint_json: &str,
    ) -> PyResult<Self> {
        let (catalogs, _) = load_catalogs(Path::new(root), catalogs_json)?;
        let session = py
            .detach(|| Session::from_checkpoint(checkpoint_json, catalogs))
            .map_err(native_error)?;
        Ok(Self { session })
    }
}
