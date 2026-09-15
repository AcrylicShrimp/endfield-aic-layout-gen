//! Explicit legacy panel migration. Original archives and scientific evidence stay immutable.
use super::*;
use crate::collection::storage::write;
use aic_pilot_native::input::{CatalogPaths, load_catalogs};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCase {
    id: String,
    product: String,
    stratum: String,
    action_seed: u64,
    geometry_sha256: String,
    input: Archive,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyPanel {
    schema: String,
    cases: Vec<LegacyCase>,
}
#[cfg(test)]
mod tests;

// This historical digest exists only to verify v1 evidence before migration.
// It is never used for new admission or partition membership.
fn legacy_digest(witness: &impl Serialize) -> String {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.remove("rate");
                map.remove("utilizations");
                for value in map.values_mut() {
                    strip(value);
                }
            }
            Value::Array(rows) => {
                for row in rows.iter_mut() {
                    strip(row);
                }
                rows.sort_by_key(ascii_json);
            }
            _ => {}
        }
    }
    let mut value = serde_json::to_value(witness).unwrap();
    strip(&mut value);
    aic_pilot_native::input::hash(ascii_json(&value).as_bytes())
}
fn ascii_json(value: &Value) -> String {
    // Matches the legacy Python-compatible ASCII JSON encoding, including surrogate pairs.
    let mut result = String::new();
    for c in serde_json::to_string(value).unwrap().chars() {
        if c.is_ascii() && c != '\u{7f}' {
            result.push(c);
        } else {
            for unit in c.encode_utf16(&mut [0; 2]) {
                result.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    result
}

pub fn migrate(
    input: &Path,
    output: &Path,
    identity_limits: &Path,
    catalog_root: &Path,
) -> Result<Value> {
    ensure!(
        !output.exists(),
        "aic-learning.panel-output: new output directory required: {}",
        output.display()
    );
    let limits: IdentityLimits = aic_pilot_native::input::read(Path::new("."), identity_limits)
        .map_err(anyhow::Error::msg)?;
    let catalogs = load_catalogs(
        catalog_root,
        &CatalogPaths {
            facilities: "facilities.json".into(),
            recipes: "recipes.json".into(),
            items: "items.json".into(),
            transports: "transports.json".into(),
            components: "logistics-components.json".into(),
        },
    )
    .map_err(anyhow::Error::msg)?;
    migrate_with(input, output, &limits, &catalogs)
}

pub(super) fn migrate_with(
    input: &Path,
    output: &Path,
    limits: &IdentityLimits,
    catalogs: &Catalogs,
) -> Result<Value> {
    ensure!(
        !output.exists() && fs::metadata(input)?.len() <= 4 * 1024 * 1024,
        "aic-learning.panel-output: new output directory and bounded input required"
    );
    let legacy: LegacyPanel = serde_json::from_slice(&fs::read(input)?)?;
    ensure!(
        legacy.schema == "aic-evaluation-panel-v1"
            && !legacy.cases.is_empty()
            && legacy.cases.len() <= 4096,
        "aic-learning.evaluation-panel: expected bounded legacy v1 panel at {}",
        input.display()
    );
    let mut manifest = Manifest {
        schema: "aic-evaluation-panel-v2".into(),
        identity_semantics: SEMANTICS.into(),
        cases: vec![],
    };
    let mut ids = BTreeSet::new();
    let mut reservations = std::collections::BTreeMap::<String, String>::new();
    let mut archives = vec![];
    for (index, case) in legacy.cases.into_iter().enumerate() {
        ensure!(
            !case.id.is_empty()
                && ids.insert(case.id.clone())
                && !case.product.is_empty()
                && !case.stratum.is_empty(),
            "aic-learning.evaluation-identity: invalid/duplicate case {}",
            case.id
        );
        let source = input
            .parent()
            .unwrap_or(Path::new("."))
            .join(&case.input.path);
        ensure!(
            fs::metadata(&source)?.len() <= 16 * 1024 * 1024
                && sha256(&source)? == case.input.sha256,
            "aic-learning.evaluation-input: changed/oversized {}",
            source.display()
        );
        let mut value: Generated =
            aic_pilot_native::input::read(Path::new("."), &source).map_err(anyhow::Error::msg)?;
        ensure!(
            legacy_digest(&value.original) == case.geometry_sha256
                && value.metadata["draw"]["product"] == case.product,
            "aic-learning.evaluation-geometry: changed legacy case {}",
            case.id
        );
        let episode = match (
            value.metadata["source"].as_str(),
            value.metadata["slot"].as_u64(),
        ) {
            (Some(source), Some(slot)) => format!("{source}/{slot}"),
            _ => format!("legacy-panel/{}", case.id),
        };
        value
            .admit(
                catalogs,
                limits,
                &format!("{} / {}", source.display(), case.id),
                Some(episode),
            )
            .map_err(anyhow::Error::msg)?;
        let admission = value.admitted().map_err(anyhow::Error::msg)?.clone();
        for key in [&admission.physical.structural, &admission.root.structural] {
            if let Some(previous) = reservations.insert(key.clone(), case.id.clone()) {
                ensure!(
                    previous == case.id,
                    "aic-learning.evaluation-duplicate: legacy cases {} and {} become equivalent under {}",
                    previous,
                    case.id,
                    SEMANTICS
                );
            }
        }
        let path = format!("inputs/{index:04}.json");
        archives.push((source, path.clone()));
        manifest.cases.push(Case {
            id: case.id,
            product: case.product,
            stratum: case.stratum,
            action_seed: case.action_seed,
            admission,
            input: Archive {
                path,
                sha256: case.input.sha256,
            },
        });
    }
    // Keep only path metadata during admission. Copy the original bytes; no seed replay,
    // JSON reserialization, or input metadata rewrite is part of migration.
    fs::create_dir(output)?;
    fs::create_dir(output.join("inputs"))?;
    for (source, path) in archives {
        fs::copy(source, output.join(path))?;
    }
    for case in &manifest.cases {
        ensure!(
            sha256(&output.join(&case.input.path))? == case.input.sha256,
            "aic-learning.evaluation-input: source changed while copying case {}",
            case.id
        );
    }
    write(&output.join("panel.pending"), &manifest)?;
    fs::rename(output.join("panel.pending"), output.join("panel.json"))?;
    Ok(
        json!({"schema":"aic-panel-migration-v1","status":"completed","source_panel":input,
        "panel":output.join("panel.json"),"cases":manifest.cases.len(),"identity_semantics":SEMANTICS}),
    )
}
