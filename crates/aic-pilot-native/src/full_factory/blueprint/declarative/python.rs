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
    learning: Option<learning::Learning>,
    learning_metadata: Option<Py<PyAny>>,
    learning_chunk: Option<learning::trace::Chunk>,
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
        Ok(Self {
            session,
            learning: None,
            learning_metadata: None,
            learning_chunk: None,
        })
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
    fn prepare_learning(&mut self, py: Python<'_>, options_json: &str) -> PyResult<()> {
        let options = decode(options_json).map_err(diagnostic)?;
        let learning = py
            .detach(|| learning::Learning::new(&self.session, options))
            .map_err(native_error)?;
        self.learning_metadata = Some(
            crate::full_factory::python::value(py, &learning.metadata(&self.session))?.unbind(),
        );
        self.learning = Some(learning);
        Ok(())
    }
    fn observe<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let learning = self.learning.as_mut().ok_or_else(|| {
            diagnostic("declarative.learning-config: call prepare_learning at admission")
        })?;
        let input = py.detach(|| learning.observe(&self.session));
        learning::python::export(
            py,
            input,
            learning,
            &self.session,
            self.learning_metadata.as_ref().unwrap().bind(py),
        )
    }
    fn step_index<'py>(
        &mut self,
        py: Python<'py>,
        handle: u64,
        root: usize,
        ports: Vec<usize>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let learning = self.learning.as_ref().ok_or_else(|| {
            diagnostic("declarative.learning-config: call prepare_learning at admission")
        })?;
        let decision = learning
            .selected(&self.session, handle, root, &ports)
            .map_err(native_error)?;
        let action = json!(decision.action);
        let outcome = py
            .detach(|| {
                if let Some(chunk) = &mut self.learning_chunk {
                    chunk.step(&mut self.session, decision)
                } else {
                    Ok(self.session.step(|| Ok(decision), None))
                }
            })
            .map_err(native_error)?;
        crate::full_factory::python::value(
            py,
            &json!({"action":action,"outcome":outcome,"work":self.session.work}),
        )
    }
    fn begin_learning_chunk(&mut self, limit: usize, full: bool) -> PyResult<()> {
        if self.learning_chunk.is_some() {
            return Err(diagnostic(
                "declarative.learning-chunk: drain the existing chunk first",
            ));
        }
        self.learning_chunk = Some(learning::trace::Chunk::new(limit, full).map_err(native_error)?);
        Ok(())
    }
    fn learning_roots_json(&self) -> PyResult<String> {
        let learning = self.learning.as_ref().ok_or_else(|| {
            diagnostic("declarative.learning-config: call prepare_learning at admission")
        })?;
        encode(&learning.diagnostic_roots(&self.session))
    }
    fn take_learning_chunk<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let chunk = self
            .learning_chunk
            .take()
            .ok_or_else(|| diagnostic("declarative.learning-chunk: no active chunk"))?;
        let value = py
            .detach(|| chunk.export(&self.session))
            .map_err(native_error)?;
        crate::full_factory::python::value(py, &value)
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
        Ok(Self {
            session,
            learning: None,
            learning_metadata: None,
            learning_chunk: None,
        })
    }
}
