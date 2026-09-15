//! Bounded external input loading and single-copy dataset publication.
use aic_layout::{
    facilities::ValidatedFacilityCatalog,
    logistics::{ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog},
    validation::Catalogs,
};
use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Read, Write},
    path::Path,
};

pub fn bytes(path: &Path) -> Result<Vec<u8>> {
    let mut bytes = vec![];
    File::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "{} exceeds the 16 MiB input guard",
        path.display()
    );
    Ok(bytes)
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(&bytes(path)?).with_context(|| format!("parse {}", path.display()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
pub fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub struct LoadedCatalogs {
    pub catalogs: Catalogs,
    pub sources: Vec<(&'static str, Vec<u8>)>,
}

pub fn load_catalogs(dir: &Path) -> Result<LoadedCatalogs> {
    let names = [
        "facilities.json",
        "transports.json",
        "logistics-components.json",
    ];
    let sources = names
        .into_iter()
        .map(|name| Ok((name, bytes(&dir.join(name))?)))
        .collect::<Result<Vec<_>>>()?;
    let mut identity = Sha256::new();
    for (name, bytes) in &sources {
        identity.update((name.len() as u64).to_le_bytes());
        identity.update(name.as_bytes());
        identity.update((bytes.len() as u64).to_le_bytes());
        identity.update(bytes);
    }
    let facilities = ValidatedFacilityCatalog::try_from_catalog(
        serde_json::from_slice(&sources[0].1).context("parse facilities.json")?,
    )
    .map_err(|e| anyhow::anyhow!("{}: {e:?}", dir.join(names[0]).display()))?;
    let transports = ValidatedTransportCatalog::try_from_catalog(
        serde_json::from_slice(&sources[1].1).context("parse transports.json")?,
    )
    .map_err(|e| anyhow::anyhow!("{}: {e:?}", dir.join(names[1]).display()))?;
    let components = ValidatedLogisticsComponentCatalog::try_from_catalog(
        serde_json::from_slice(&sources[2].1).context("parse logistics-components.json")?,
    )
    .map_err(|e| anyhow::anyhow!("{}: {e:?}", dir.join(names[2]).display()))?;
    Ok(LoadedCatalogs {
        catalogs: Catalogs {
            identity: hex(&identity.finalize()),
            facilities,
            transports,
            components,
        },
        sources,
    })
}

pub fn write(path: &Path, content: &[u8]) -> Result<()> {
    let mut out = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .with_context(|| format!("create {}", path.display()))?,
    );
    out.write_all(content)?;
    out.flush()?;
    Ok(())
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    anyhow::ensure!(!path.exists(), "output already exists: {}", path.display());
    let temporary = path.with_extension("json.tmp");
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .with_context(|| format!("create {}", temporary.display()))?;
    let mut out = BufWriter::new(file);
    serde_json::to_writer_pretty(&mut out, value)?;
    out.write_all(b"\n")?;
    out.flush()?;
    // Output directories are exclusively created by this command.
    fs::rename(&temporary, path).with_context(|| format!("publish {}", path.display()))?;
    Ok(())
}
