//! Full production-witness transactions. Historical single-path tensors are not adapted.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use aic_data::facilities::ValidatedFacilityCatalog;
use aic_data::layouts::{FactoryEditCatalogs, FactoryEditSession};
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use aic_data::recipes::ValidatedRecipeBook;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;

fn error(message: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(format!("factory.input: {message}"))
}

pub(crate) fn decode<T: DeserializeOwned>(value: &str) -> std::result::Result<T, String> {
    if value.len() > MAX_JSON_BYTES {
        return Err("JSON exceeds the 16 MiB input work guard".into());
    }
    serde_json::from_str(value).map_err(|e| e.to_string())
}

fn encode(value: &impl Serialize) -> PyResult<String> {
    serde_json::to_string(value).map_err(error)
}

pub(crate) fn read<T: DeserializeOwned>(
    root: &Path,
    path: &Path,
    provenance: &mut Vec<Value>,
) -> PyResult<T> {
    let path = root.join(path);
    let file = std::fs::File::open(&path).map_err(|e| error(format!("{}: {e}", path.display())))?;
    let mut bytes = Vec::new();
    use std::io::Read;
    file.take(MAX_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error(format!("{}: {e}", path.display())))?;
    if bytes.len() > MAX_JSON_BYTES {
        return Err(error(format!("{}: input exceeds 16 MiB", path.display())));
    }
    let value =
        serde_json::from_slice(&bytes).map_err(|e| error(format!("{}: {e}", path.display())))?;
    provenance.push(json!({"path": path, "sha256": crate::catalog::hash(&bytes)}));
    Ok(value)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Paths {
    facilities: PathBuf,
    recipes: PathBuf,
    items: PathBuf,
    transports: PathBuf,
    components: PathBuf,
}

pub(crate) fn load_catalogs(
    root: &Path,
    catalog_paths_json: &str,
) -> PyResult<(Arc<FactoryEditCatalogs>, Vec<Value>)> {
    let paths: Paths = decode(catalog_paths_json).map_err(error)?;
    let mut provenance = Vec::new();
    let catalogs = FactoryEditCatalogs {
        facilities: ValidatedFacilityCatalog::try_from_catalog(read(
            root,
            &paths.facilities,
            &mut provenance,
        )?)
        .map_err(|e| error(format!("{}: {e:?}", paths.facilities.display())))?,
        recipes: ValidatedRecipeBook::try_from_recipe_book(read(
            root,
            &paths.recipes,
            &mut provenance,
        )?)
        .map_err(|e| error(format!("{}: {e:?}", paths.recipes.display())))?,
        items: ValidatedItemCatalog::try_from_catalog(read(root, &paths.items, &mut provenance)?)
            .map_err(|e| error(format!("{}: {e:?}", paths.items.display())))?,
        transports: ValidatedTransportCatalog::try_from_catalog(read(
            root,
            &paths.transports,
            &mut provenance,
        )?)
        .map_err(|e| error(format!("{}: {e:?}", paths.transports.display())))?,
        components: ValidatedLogisticsComponentCatalog::try_from_catalog(read(
            root,
            &paths.components,
            &mut provenance,
        )?)
        .map_err(|e| error(format!("{}: {e:?}", paths.components.display())))?,
    };
    Ok((Arc::new(catalogs), provenance))
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
