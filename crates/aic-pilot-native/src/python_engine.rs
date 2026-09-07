//! Explicit native engine boundary. JSON is restricted to setup and diagnostic exports.

use std::path::Path;
use std::time::Instant;

use numpy::IntoPyArray;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde::{Serialize, de::DeserializeOwned};

use crate::acceptance::{self, Acceptance};
use crate::diagnostic::Diagnostic;
use crate::engine::{Engine, Handle, SNAPSHOT_VERSION};
use crate::observation::Observation;
use crate::transition::Outcome;

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

#[pyclass(frozen, name = "CandidateHandle")]
pub struct PythonHandle {
    pub value: Handle,
}

fn outcome<'py>(py: Python<'py>, value: Outcome) -> PyResult<Bound<'py, PyDict>> {
    let output = PyDict::new(py);
    output.set_item("applied", value.applied)?;
    output.set_item("complete", value.complete)?;
    output.set_item("new_completion", value.new_completion)?;
    output.set_item("improved", value.improved)?;
    let diagnostics = PyList::empty(py);
    for d in value.diagnostics {
        let item = PyDict::new(py);
        item.set_item("code", d.code)?;
        item.set_item("severity", d.severity)?;
        item.set_item("stage", d.stage)?;
        item.set_item("message", d.message)?;
        item.set_item("entity", d.entity)?;
        diagnostics.append(item)?;
    }
    output.set_item("diagnostics", diagnostics)?;
    Ok(output)
}

fn observation<'py>(py: Python<'py>, value: Observation) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("raster", value.raster.into_pyarray(py))?;
    result.set_item("objects", value.objects.into_pyarray(py))?;
    result.set_item("stats", value.stats.into_pyarray(py))?;
    result.set_item("candidates", value.candidates.into_pyarray(py))?;
    result.set_item("points", value.points.into_pyarray(py))?;
    Ok(result)
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

    #[staticmethod]
    fn build_info_json() -> &'static str {
        env!("AIC_PILOT_BUILD_INFO")
    }

    #[getter]
    fn initial_score(&self) -> Vec<i64> {
        self.engine.initial_score.to_vec()
    }

    #[getter]
    fn best_score(&self) -> Vec<i64> {
        self.engine.best_score.to_vec()
    }

    fn trace_json(&self) -> PyResult<String> {
        encode(&self.engine.trace)
    }

    fn performance_json(&self) -> PyResult<String> {
        encode(&serde_json::json!({"seconds":self.engine.seconds,"counters":self.engine.counters}))
    }

    fn restart_measurements(&mut self) {
        self.engine.seconds.clear();
        self.engine.counters.clear();
    }

    fn episode_json(&self) -> PyResult<String> {
        encode(
            &serde_json::json!({"steps":self.engine.steps,"revision":self.engine.revision,
            "best_score":self.engine.best_score,"base_score":self.engine.base_score,
            "counts":self.engine.episode_counts,
            "draft_acceptance":Acceptance::from_result(acceptance::check(&self.engine.catalog,&self.engine.draft))}),
        )
    }

    fn inspect_layout_json(&self, layout_json: &str) -> PyResult<String> {
        let layout = decode(layout_json, "layout.malformed")?;
        let state = crate::state::State::import(&self.engine.catalog, layout).map_err(py_error)?;
        let accepted = Acceptance::from_result(acceptance::check(&self.engine.catalog, &state));
        let mut boxes = std::collections::BTreeMap::new();
        if accepted.valid {
            for (fi, &pose) in state.poses.iter().enumerate() {
                let body = crate::state::body(&self.engine.catalog, fi, pose).map_err(py_error)?;
                let [x0, y0, x1, y1] = crate::state::bounds(&body).map_err(py_error)?;
                boxes.insert(
                    self.engine.catalog.case.facilities[fi].id.clone(),
                    [x0, y0, x1 - x0 + 1, y1 - y0 + 1],
                );
            }
        }
        encode(&serde_json::json!({"acceptance":accepted,"facility_boxes":boxes}))
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

    fn candidates(&mut self) -> PyResult<(PythonHandle, usize)> {
        self.engine
            .candidates()
            .map(|(value, count)| (PythonHandle { value }, count))
            .map_err(py_error)
    }

    fn observe<'py>(&mut self, py: Python<'py>) -> PyResult<(Bound<'py, PyDict>, PythonHandle)> {
        let (value, _) = self.engine.candidates().map_err(py_error)?;
        let data = self.engine.observation(value).map_err(py_error)?;
        let started = Instant::now();
        let output = observation(py, data)?;
        *self
            .engine
            .seconds
            .entry("array_export".into())
            .or_default() += started.elapsed().as_secs_f64();
        Ok((output, PythonHandle { value }))
    }

    fn observe_candidates<'py>(
        &mut self,
        py: Python<'py>,
        handle: &PythonHandle,
    ) -> PyResult<Bound<'py, PyDict>> {
        let data = self.engine.observation(handle.value).map_err(py_error)?;
        let started = Instant::now();
        let output = observation(py, data)?;
        *self
            .engine
            .seconds
            .entry("array_export".into())
            .or_default() += started.elapsed().as_secs_f64();
        Ok(output)
    }

    fn export_candidates_json(&self, handle: &PythonHandle) -> PyResult<String> {
        encode(
            &self
                .engine
                .export_candidates(handle.value)
                .map_err(py_error)?,
        )
    }

    fn step<'py>(
        &mut self,
        py: Python<'py>,
        handle: &PythonHandle,
        action_index: usize,
    ) -> PyResult<(f64, bool, Bound<'py, PyDict>)> {
        let (reward, done, result) = self
            .engine
            .step(handle.value, action_index)
            .map_err(py_error)?;
        Ok((reward, done, outcome(py, result)?))
    }

    fn diagnostic_step_json<'py>(
        &mut self,
        py: Python<'py>,
        command_json: &str,
    ) -> PyResult<(f64, bool, Bound<'py, PyDict>)> {
        let (reward, done, result) = self
            .engine
            .diagnostic_step(decode(command_json, "command.malformed")?)
            .map_err(py_error)?;
        Ok((reward, done, outcome(py, result)?))
    }
}
