//! Explicit native engine boundary. JSON is restricted to setup and diagnostic exports.

use std::path::Path;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde::{Serialize, de::DeserializeOwned};

use crate::acceptance::{self, Acceptance};
use crate::diagnostic::Diagnostic;
use crate::engine::{Engine, SNAPSHOT_VERSION};

fn py_error(error: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(error.to_string())
}

fn encode(value: &impl Serialize) -> PyResult<String> {
    serde_json::to_string(value).map_err(py_error)
}

fn decode<T: DeserializeOwned>(value: &str, code: &str) -> PyResult<T> {
    serde_json::from_str(value).map_err(|e| py_error(format!("{code}: {e}")))
}

#[pyclass(name = "Engine")]
pub struct PythonEngine {
    pub engine: Engine,
}

#[pymethods]
impl PythonEngine {
    #[staticmethod]
    fn open(root: &str, case_path: &str, config_json: &str) -> PyResult<Self> {
        Engine::open(
            Path::new(root),
            Path::new(case_path),
            decode(config_json, "input.config")?,
        )
        .map(|engine| Self { engine })
        .map_err(py_error)
    }

    fn reset(&mut self) -> PyResult<()> {
        self.engine.reset().map_err(py_error)
    }

    fn metadata_json(&self) -> PyResult<String> {
        encode(&self.engine.metadata())
    }

    fn export_layout_json(&self, which: &str) -> PyResult<String> {
        encode(&self.engine.export_layout(which).map_err(py_error)?)
    }

    fn export_state_json(&self) -> PyResult<String> {
        encode(&self.engine.snapshot())
    }

    fn restore_state_json(&mut self, snapshot_json: &str) -> PyResult<()> {
        let raw: serde_json::Value = decode(snapshot_json, "snapshot.malformed")?;
        if raw.get("schema_version").and_then(|v| v.as_u64()) != Some(SNAPSHOT_VERSION as u64) {
            return Err(py_error(
                "snapshot.version: incompatible native environment snapshot; legacy Python states cannot be resumed",
            ));
        }
        self.engine
            .restore(
                serde_json::from_value(raw)
                    .map_err(|e| py_error(format!("snapshot.malformed: {e}")))?,
            )
            .map_err(py_error)
    }

    fn check_layout_json(&self, layout_json: &str) -> PyResult<String> {
        let result = match serde_json::from_str(layout_json) {
            Ok(layout) => acceptance::check_layout(&self.engine.catalog, layout),
            Err(e) => Acceptance::from_result(Err(Diagnostic::acceptance(
                "layout.malformed",
                &e.to_string(),
                None,
            ))),
        };
        encode(&result)
    }
}
