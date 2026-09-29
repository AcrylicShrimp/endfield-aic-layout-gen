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

pub const MAX_GENERATED_INPUT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CHECKPOINT_BYTES: usize = 16 * 1024 * 1024;

fn error(message: impl std::fmt::Display) -> String {
    format!("factory.input: {message}")
}

#[derive(Debug)]
pub enum CheckpointDecodeError {
    TooLarge { bytes: usize, limit: usize },
    Invalid(String),
}

pub fn decode_checkpoint<T: DeserializeOwned>(
    value: &str,
) -> std::result::Result<T, CheckpointDecodeError> {
    decode_with_limit(value, MAX_CHECKPOINT_BYTES)
}

fn decode_with_limit<T: DeserializeOwned>(
    value: &str,
    limit: usize,
) -> Result<T, CheckpointDecodeError> {
    if value.len() > limit {
        return Err(CheckpointDecodeError::TooLarge {
            bytes: value.len(),
            limit,
        });
    }
    serde_json::from_str(value).map_err(|e| CheckpointDecodeError::Invalid(e.to_string()))
}

pub fn read_generated_bytes(path: &Path) -> Result<Vec<u8>, String> {
    read_bytes_with_limit(path, MAX_GENERATED_INPUT_BYTES)
}

fn read_bytes_with_limit(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(&path).map_err(|e| error(format!("{}: {e}", path.display())))?;
    let declared = file
        .metadata()
        .map_err(|e| error(format!("{}: {e}", path.display())))?
        .len();
    if declared > limit as u64 {
        return Err(error(format!(
            "{}: {} bytes exceeds {}-byte input limit",
            path.display(),
            declared,
            limit
        )));
    }
    let mut bytes = Vec::new();
    use std::io::Read;
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| error(format!("{}: {e}", path.display())))?;
    if bytes.len() > limit {
        let actual = std::fs::metadata(path)
            .map(|metadata| metadata.len().max(bytes.len() as u64))
            .unwrap_or(bytes.len() as u64);
        return Err(error(format!(
            "{}: {} bytes exceeds {}-byte input limit",
            path.display(),
            actual,
            limit
        )));
    }
    Ok(bytes)
}

pub fn read<T: DeserializeOwned>(root: &Path, path: &Path) -> Result<T, String> {
    let path = root.join(path);
    let bytes = read_generated_bytes(&path)?;
    let value =
        serde_json::from_slice(&bytes).map_err(|e| error(format!("{}: {e}", path.display())))?;
    Ok(value)
}

pub fn hash_file(path: &Path) -> Result<String, String> {
    hash_file_with_limit(path, MAX_GENERATED_INPUT_BYTES)
}

fn hash_file_with_limit(path: &Path, limit: usize) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;

    let file = std::fs::File::open(path).map_err(|e| error(format!("{}: {e}", path.display())))?;
    let declared = file
        .metadata()
        .map_err(|e| error(format!("{}: {e}", path.display())))?
        .len();
    if declared > limit as u64 {
        return Err(error(format!(
            "{}: {} bytes exceeds {}-byte input limit",
            path.display(),
            declared,
            limit
        )));
    }
    let mut reader = std::io::BufReader::new(file).take(limit as u64 + 1);
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let count = reader
            .read(&mut bytes)
            .map_err(|e| error(format!("{}: {e}", path.display())))?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > limit as u64 {
            let actual = std::fs::metadata(path)
                .map(|metadata| metadata.len().max(total))
                .unwrap_or(total);
            return Err(error(format!(
                "{}: {} bytes exceeds {}-byte input limit",
                path.display(),
                actual,
                limit
            )));
        }
        digest.update(&bytes[..count]);
    }
    if total != declared {
        return Err(error(format!(
            "{}: input changed length while hashing ({} to {} bytes)",
            path.display(),
            declared,
            total
        )));
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_file_identity_and_checkpoint_decode_use_independent_byte_limits() {
        let path = std::env::temp_dir().join(format!(
            "aic-input-boundary-{}-{:?}.json",
            std::process::id(),
            std::thread::current().id()
        ));
        let original = br#"{"a":1}"#;
        std::fs::write(&path, original).unwrap();
        let bytes = read_bytes_with_limit(&path, original.len()).unwrap();
        assert_eq!(
            hash_file_with_limit(&path, original.len()).unwrap(),
            hash(&bytes)
        );
        let decoded: serde_json::Value =
            decode_with_limit(std::str::from_utf8(&bytes).unwrap(), original.len()).unwrap();
        std::fs::write(&path, br#"{"a":2}"#).unwrap();
        assert_eq!(decoded["a"], 1);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["a"],
            1
        );
        std::fs::write(&path, b"{\"a\":1} ").unwrap();
        let actual = original.len() + 1;
        for result in [
            read_bytes_with_limit(&path, original.len()).map(|_| ()),
            hash_file_with_limit(&path, original.len()).map(|_| ()),
        ] {
            let diagnostic = result.unwrap_err();
            assert!(diagnostic.contains(&path.display().to_string()));
            assert!(diagnostic.contains(&format!("{actual} bytes")));
            assert!(diagnostic.contains(&format!("{}-byte", original.len())));
        }
        assert!(matches!(
            decode_with_limit::<serde_json::Value>("{\"a\":1} ", original.len()),
            Err(CheckpointDecodeError::TooLarge { bytes, limit })
                if bytes == actual && limit == original.len()
        ));
        std::fs::remove_file(path).unwrap();
    }
}
