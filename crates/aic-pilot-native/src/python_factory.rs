//! Full production-witness transactions. Historical single-path tensors are not adapted.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use aic_data::layouts::{FactoryEditCatalogs, FactoryEditSession};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn error(message: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("factory.input: {message}"))
}
pub(crate) use crate::input::decode;
fn encode(value: &impl Serialize) -> PyResult<String> {
    serde_json::to_string(value).map_err(error)
}
pub(crate) fn read<T: DeserializeOwned>(
    root: &Path,
    path: &Path,
    provenance: &mut Vec<Value>,
) -> PyResult<T> {
    crate::input::read(root, path, provenance).map_err(PyValueError::new_err)
}
pub(crate) fn load_catalogs(
    root: &Path,
    paths: &str,
) -> PyResult<(Arc<FactoryEditCatalogs>, Vec<Value>)> {
    crate::input::load_catalogs(root, paths).map_err(PyValueError::new_err)
}

#[pyclass(name = "FactoryEditSession")]
pub struct PythonFactory {
    session: FactoryEditSession,
    metadata: Value,
}

#[pymethods]
impl PythonFactory {
    #[staticmethod]
    fn open(
        root: &str,
        request_path: &str,
        witness_path: &str,
        catalog_paths_json: &str,
        budget_json: &str,
    ) -> PyResult<Self> {
        let root = Path::new(root);
        let budget = decode(budget_json).map_err(error)?;
        let (catalogs, mut provenance) = load_catalogs(root, catalog_paths_json)?;
        let request = read(root, Path::new(request_path), &mut provenance)?;
        let witness = read(root, Path::new(witness_path), &mut provenance)?;
        let session = FactoryEditSession::new(request, witness, catalogs, budget)
            .map_err(|d| error(serde_json::to_string(&d).unwrap_or_else(|_| d.message.clone())))?;
        let build: Value = serde_json::from_str(env!("AIC_PILOT_BUILD_INFO")).map_err(error)?;
        let metadata = json!({"schema_version": 1, "backend": "production-physical-witness",
            "observation_schema": null, "checkpoint_compatibility": "none",
            "catalog_provenance": provenance, "native_build": build,
            "scope": "complete steady-state factory material and physical acceptance; excludes power and startup"});
        Ok(Self { session, metadata })
    }

    fn propose_json(&mut self, py: Python<'_>, proposal_json: &str) -> PyResult<String> {
        let outcome = py.detach(|| self.session.propose_json(proposal_json));
        encode(&outcome)
    }

    fn snapshot_json(&self) -> PyResult<String> {
        encode(&self.session.snapshot())
    }

    fn metadata_json(&self) -> PyResult<String> {
        encode(&self.metadata)
    }

    #[staticmethod]
    fn dimensions() -> BTreeMap<String, u32> {
        BTreeMap::from([
            ("physical_schema_version".into(), 1),
            ("request_schema_version".into(), 1),
        ])
    }
}
