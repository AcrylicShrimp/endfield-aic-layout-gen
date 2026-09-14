//! Full production-witness transactions. Historical single-path tensors are not adapted.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aic_data::facilities::ValidatedFacilityCatalog;
use aic_data::layouts::FactoryEditCatalogs;
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use aic_data::recipes::ValidatedRecipeBook;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

const MAX_JSON_BYTES: usize = 16 * 1024 * 1024;

fn error(message: impl std::fmt::Display) -> String {
    format!("factory.input: {message}")
}

pub fn decode<T: DeserializeOwned>(value: &str) -> std::result::Result<T, String> {
    if value.len() > MAX_JSON_BYTES {
        return Err("JSON exceeds the 16 MiB input work guard".into());
    }
    serde_json::from_str(value).map_err(|e| e.to_string())
}

pub fn read<T: DeserializeOwned>(root: &Path, path: &Path) -> Result<T, String> {
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
    Ok(value)
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPaths {
    pub facilities: PathBuf,
    pub recipes: PathBuf,
    pub items: PathBuf,
    pub transports: PathBuf,
    pub components: PathBuf,
}

pub fn load_catalogs(root: &Path, paths: &CatalogPaths) -> Result<Arc<Catalogs>, String> {
    let catalogs = FactoryEditCatalogs {
        facilities: ValidatedFacilityCatalog::try_from_catalog(read(root, &paths.facilities)?)
            .map_err(|e| error(format!("{}: {e:?}", paths.facilities.display())))?,
        recipes: ValidatedRecipeBook::try_from_recipe_book(read(root, &paths.recipes)?)
            .map_err(|e| error(format!("{}: {e:?}", paths.recipes.display())))?,
        items: ValidatedItemCatalog::try_from_catalog(read(root, &paths.items)?)
            .map_err(|e| error(format!("{}: {e:?}", paths.items.display())))?,
        transports: ValidatedTransportCatalog::try_from_catalog(read(root, &paths.transports)?)
            .map_err(|e| error(format!("{}: {e:?}", paths.transports.display())))?,
        components: ValidatedLogisticsComponentCatalog::try_from_catalog(read(
            root,
            &paths.components,
        )?)
        .map_err(|e| error(format!("{}: {e:?}", paths.components.display())))?,
    };
    Ok(Arc::new(Catalogs::new(catalogs)))
}

/// Source identity for external data and native checkpoints.
pub fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Catalog validation and identity are shared by every session in the pool.
pub struct Catalogs {
    pub data: Arc<FactoryEditCatalogs>,
    pub identity: String,
}
impl Catalogs {
    pub fn new(data: FactoryEditCatalogs) -> Self {
        let identity = crate::full_factory::identity::digest(&[
            format!("{:?}", data.facilities.catalog()),
            format!("{:?}", data.recipes.recipe_book()),
            format!("{:?}", data.items.catalog()),
            format!("{:?}", data.transports.catalog()),
            format!("{:?}", data.components.catalog()),
        ]);
        Self {
            data: Arc::new(data),
            identity,
        }
    }
}
