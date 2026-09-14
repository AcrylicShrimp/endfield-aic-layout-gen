//! Owned NumPy buffers and compact selected-action results; cold JSON imports only.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use aic_data::layouts::FactoryEditCatalogs;
use numpy::{
    IntoPyArray,
    ndarray::{Array2, Array3},
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde::Serialize;
use serde_json::{Value, json};

use super::engine::{Compact, Episode, Handle};
use super::identity;
use super::observation::{self as obs, Best, Current};
use crate::python_factory::{decode, load_catalogs, read};

fn error(value: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(value.to_string())
}
fn diagnostic(value: super::types::Diagnostic) -> PyErr {
    error(serde_json::to_string(&value).unwrap())
}
fn encode(value: &impl Serialize) -> PyResult<String> {
    serde_json::to_string(value).map_err(error)
}

#[pyclass(name = "FullFactoryRuntime")]
pub struct PythonRuntime {
    root: PathBuf,
    catalogs: Arc<FactoryEditCatalogs>,
    metadata: Value,
}
#[pyclass(name = "FullFactoryEpisode")]
pub struct PythonEpisode {
    episode: Episode,
}
#[pyclass(frozen, name = "FullFactoryCandidateHandle")]
pub struct PythonHandle {
    handle: Handle,
}

pub(crate) fn value<'py>(py: Python<'py>, v: &Value) -> PyResult<Bound<'py, PyAny>> {
    Ok(match v {
        Value::Null => py.None().into_bound(py),
        Value::Bool(b) => b.into_pyobject(py)?.to_owned().into_any(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.into_pyobject(py)?.into_any()
            } else if let Some(u) = n.as_u64() {
                u.into_pyobject(py)?.into_any()
            } else {
                n.as_f64().unwrap().into_pyobject(py)?.into_any()
            }
        }
        Value::String(s) => s.into_pyobject(py)?.into_any(),
        Value::Array(a) => {
            let result = PyList::empty(py);
            for v in a {
                result.append(value(py, v)?)?;
            }
            result.into_any()
        }
        Value::Object(m) => {
            let result = PyDict::new(py);
            for (k, v) in m {
                result.set_item(k, value(py, v)?)?;
            }
            result.into_any()
        }
    })
}
fn matrix<'py, T: numpy::Element + Clone, const N: usize>(
    py: Python<'py>,
    rows: &[[T; N]],
) -> Bound<'py, numpy::PyArray2<T>> {
    Array2::from_shape_vec((rows.len(), N), rows.iter().flatten().cloned().collect())
        .unwrap()
        .into_pyarray(py)
}
fn world<'py>(py: Python<'py>, w: &obs::World) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item(
        "raster",
        Array3::from_shape_vec((28, w.height, w.width), w.raster.clone())
            .unwrap()
            .into_pyarray(py),
    )?;
    result.set_item("nodes", matrix(py, &w.nodes))?;
    result.set_item("node_xy", matrix(py, &w.node_xy))?;
    result.set_item("edges", matrix(py, &w.edges))?;
    result.set_item("edge_values", matrix(py, &w.edge_values))?;
    result.set_item("messages", w.messages)?;
    Ok(result)
}
fn candidates<'py>(py: Python<'py>, a: &obs::Candidates) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("actions", matrix(py, &a.actions))?;
    result.set_item("operands", matrix(py, &a.operands))?;
    result.set_item("action_xy", matrix(py, &a.action_xy))?;
    result.set_item("member_nodes", a.member_nodes.clone().into_pyarray(py))?;
    result.set_item("member_actions", a.member_actions.clone().into_pyarray(py))?;
    result.set_item("member_xy", matrix(py, &a.member_xy))?;
    result.set_item("member_pose", matrix(py, &a.member_pose))?;
    result.set_item("schema", obs::SCHEMA)?;
    Ok(result)
}
fn compact<'py>(py: Python<'py>, c: Compact) -> PyResult<Bound<'py, PyDict>> {
    let result = PyDict::new(py);
    result.set_item("handle", PythonHandle { handle: c.handle })?;
    result.set_item("allowed", c.allowed.into_pyarray(py))?;
    let ids: Vec<_> = c
        .kinds
        .iter()
        .map(|kind| obs::KINDS.iter().position(|k| k == kind).unwrap() as i64)
        .collect();
    result.set_item("kind_ids", ids.into_pyarray(py))?;
    result.set_item("memory_exhausted", c.exhausted)?;
    Ok(result)
}

#[pymethods]
impl PythonRuntime {
    #[staticmethod]
    fn open(root: &str, catalog_paths_json: &str) -> PyResult<Self> {
        let began = Instant::now();
        let root = PathBuf::from(root);
        let (catalogs, files) = load_catalogs(&root, catalog_paths_json)?;
        let build: Value = serde_json::from_str(env!("AIC_PILOT_BUILD_INFO")).map_err(error)?;
        // Locations are diagnostic; byte identities identify the executed runtime.
        let identities: Vec<_> = files.iter().map(|f| f["sha256"].clone()).collect();
        let provenance = identity::digest(&json!({"catalogs": identities, "build": build}));
        let metadata = json!({"schema_version": 2, "backend": "retained-full-factory-native",
            "repair_contract": super::routing::CONTRACT, "repair_limits": super::routing::Limits::default(),
            "observation_schema": obs::SCHEMA, "kinds": obs::KINDS, "catalog_provenance": files, "native_build": build, "provenance": provenance,
            "catalog_seconds": began.elapsed().as_secs_f64()});
        Ok(Self {
            root,
            catalogs,
            metadata,
        })
    }
    fn open_episode(
        &self,
        py: Python<'_>,
        request_path: &str,
        witness_path: &str,
        config_json: &str,
    ) -> PyResult<PythonEpisode> {
        let config = decode(config_json).map_err(error)?;
        let mut files = vec![];
        let request = read(&self.root, Path::new(request_path), &mut files)?;
        let witness = read(&self.root, Path::new(witness_path), &mut files)?;
        let episode = py
            .detach(|| Episode::new(request, witness, self.catalogs.clone(), config))
            .map_err(|d| {
                error(format!(
                    "{}; request={request_path}, witness={witness_path}",
                    serde_json::to_string(&d).unwrap()
                ))
            })?;
        Ok(PythonEpisode { episode })
    }
    fn open_episode_json(
        &self,
        py: Python<'_>,
        request_json: &str,
        witness_json: &str,
        config_json: &str,
    ) -> PyResult<PythonEpisode> {
        let request =
            decode(request_json).map_err(|e| error(format!("full-factory.request-json: {e}")))?;
        let witness =
            decode(witness_json).map_err(|e| error(format!("full-factory.witness-json: {e}")))?;
        let config = decode(config_json).map_err(error)?;
        let episode = py
            .detach(|| Episode::new(request, witness, self.catalogs.clone(), config))
            .map_err(diagnostic)?;
        Ok(PythonEpisode { episode })
    }
    fn metadata_json(&self) -> PyResult<String> {
        encode(&self.metadata)
    }
    #[staticmethod]
    fn build_info_json() -> &'static str {
        env!("AIC_PILOT_BUILD_INFO")
    }
}

#[pymethods]
impl PythonEpisode {
    #[getter]
    fn done(&self) -> bool {
        self.episode.done()
    }
    #[getter]
    fn selected(&self) -> u64 {
        self.episode.selected
    }
    #[getter]
    fn elapsed_seconds(&self) -> f64 {
        self.episode.elapsed().as_secs_f64()
    }
    fn candidates<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let c = py
            .detach(|| self.episode.prepare(false, false))
            .map_err(diagnostic)?;
        compact(py, c)
    }
    #[pyo3(signature = (include_critic=false))]
    fn observe<'py>(
        &mut self,
        py: Python<'py>,
        include_critic: bool,
    ) -> PyResult<Bound<'py, PyDict>> {
        let c = py
            .detach(|| self.episode.prepare(true, include_critic))
            .map_err(diagnostic)?;
        let began = Instant::now();
        let output = compact(py, c)?;
        let v = self.episode.view();
        let current = world(py, v.current)?;
        let needs_best = include_critic || v.actor_contract.best == Best::Layout;
        let best = if needs_best {
            Some(if v.best_is_current {
                current.clone()
            } else {
                world(py, v.best.expect("prepare best world"))?
            })
        } else {
            None
        };
        let base = candidates(py, v.candidates)?;
        let actor = base.copy()?;
        let actor_current = match v.actor_contract.current {
            Current::None => None,
            Current::Full => Some(current.clone()),
            Current::Geometry => Some(world(py, &v.current.geometry())?),
        };
        actor.set_item("current", &actor_current)?;
        actor.set_item(
            "best",
            if v.actor_contract.best == Best::Layout {
                best.as_ref()
            } else {
                None
            },
        )?;
        actor.set_item(
            "best_is_current",
            v.actor_contract.best == Best::Layout && v.best_is_current,
        )?;
        actor.set_item("context", v.actor_context.to_vec().into_pyarray(py))?;
        actor.set_item("history", matrix(py, &v.history))?;
        if actor_current.is_none() {
            actor.set_item(
                "operands",
                Array2::<i64>::zeros((v.candidates.actions.len(), 2)).into_pyarray(py),
            )?;
            actor.set_item(
                "action_xy",
                Array2::<i64>::zeros((v.candidates.actions.len(), 2)).into_pyarray(py),
            )?;
            actor.set_item(
                "member_nodes",
                vec![0i64; v.candidates.member_nodes.len()].into_pyarray(py),
            )?;
            actor.set_item(
                "member_xy",
                Array2::<i64>::zeros((v.candidates.member_nodes.len(), 2)).into_pyarray(py),
            )?;
        }
        output.set_item("actor", actor)?;
        if include_critic {
            base.set_item("current", current)?;
            base.set_item("best", best)?;
            base.set_item("best_is_current", v.best_is_current)?;
            base.set_item("context", v.critic_context.to_vec().into_pyarray(py))?;
            base.set_item("history", Array2::<f32>::zeros((0, 53)).into_pyarray(py))?;
            output.set_item("critic", base)?;
        } else {
            output.set_item("critic", py.None())?;
        }
        self.episode.performance.time("array_export", began);
        Ok(output)
    }
    fn step<'py>(
        &mut self,
        py: Python<'py>,
        handle: &PythonHandle,
        index: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let result = py
            .detach(|| self.episode.step(handle.handle, index))
            .map_err(diagnostic)?;
        value(py, &result)
    }
    fn export_layout(&self, which: &str) -> PyResult<String> {
        let s = self.episode.session.snapshot();
        encode(match which {
            "initial" => s.initial,
            "current" => s.current,
            "best" => s.best,
            _ => return Err(error(format!("factory.layout-selection: {which}"))),
        })
    }
    fn summary<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        value(py, &self.episode.summary())
    }
    fn performance_json(&self) -> PyResult<String> {
        encode(&self.episode.performance)
    }
    fn diagnostic_domain_json(&mut self, py: Python<'_>) -> PyResult<String> {
        let result = py
            .detach(|| self.episode.diagnostic_domain())
            .map_err(diagnostic)?;
        encode(&result)
    }
}
