//! Full production-witness transactions. Historical single-path tensors are not adapted.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use aic_data::facilities::ValidatedFacilityCatalog;
use aic_data::layouts::FactoryEditCatalogs;
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use aic_data::recipes::ValidatedRecipeBook;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};

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

pub fn read<T: DeserializeOwned>(
    root: &Path,
    path: &Path,
    provenance: &mut Vec<Value>,
) -> Result<T, String> {
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
    provenance.push(json!({"path": path, "sha256": hash(&bytes)}));
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

pub fn load_catalogs(
    root: &Path,
    catalog_paths_json: &str,
) -> Result<(Arc<FactoryEditCatalogs>, Vec<Value>), String> {
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

/// Shared source-identity encoding; used by both native and Python entry points.
pub fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
