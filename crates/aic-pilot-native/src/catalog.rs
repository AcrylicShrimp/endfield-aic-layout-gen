//! Runtime-owned pilot catalogs and exact rational production obligations.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use num_rational::BigRational;
use num_traits::{One, Zero};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

use crate::diagnostic::{Diagnostic, Result, require};
use crate::state::{Binding, Point, Pose};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Case {
    pub schema_version: u32,
    pub id: String,
    pub catalogs: BTreeMap<String, String>,
    pub target: Target,
    pub facilities: Vec<Instance>,
    pub networks: Vec<NetworkSpec>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target {
    pub item: String,
    pub rate: [i64; 2],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Instance {
    pub id: String,
    pub recipe: String,
    pub utilization: [i64; 2],
    pub pose: Pose,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum Endpoint {
    Facility { facility: String },
    External { external: String },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct NetworkSpec {
    pub id: String,
    pub item: String,
    pub rate: [i64; 2],
    pub source: Endpoint,
    pub target: Endpoint,
    pub source_binding: Option<Binding>,
    pub target_binding: Option<Binding>,
    pub corners: Vec<Point>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Footprint {
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Position {
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Port {
    pub id: String,
    pub direction: String,
    pub transport: String,
    pub position: Position,
    pub edge: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FacilityType {
    pub id: String,
    pub footprint: Footprint,
    pub allowed_rotations: Vec<i64>,
    pub ports: Vec<Port>,
}

#[derive(Deserialize)]
struct FacilityFile {
    schema_version: u32,
    facilities: Vec<FacilityType>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Amount {
    pub item: String,
    pub quantity: i64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Recipe {
    pub id: String,
    pub facility: String,
    pub inputs: Vec<Amount>,
    pub outputs: Vec<Amount>,
    pub duration_ms: i64,
}

#[derive(Deserialize)]
struct RecipeFile {
    schema_version: u32,
    external_items: BTreeSet<String>,
    recipes: Vec<Recipe>,
}

#[derive(Deserialize)]
struct Capacity {
    quantity: i64,
    duration_ms: i64,
}

#[derive(Deserialize)]
struct Transport {
    kind: String,
    capacity: Capacity,
}

#[derive(Deserialize)]
struct TransportFile {
    schema_version: u32,
    transports: Vec<Transport>,
}

#[derive(Clone, Debug)]
pub struct Terminal {
    pub id: String,
    pub network: usize,
    pub source: bool,
    pub facility: Option<usize>,
}

pub struct Catalog {
    pub case: Case,
    pub case_json: serde_json::Value,
    pub path: PathBuf,
    pub provenance: BTreeMap<String, String>,
    pub types: Vec<FacilityType>,
    pub recipes: Vec<Recipe>,
    pub rates: Vec<BigRational>,
    pub capacity: BigRational,
    pub terminals: Vec<Terminal>,
    pub facility_ids: BTreeMap<String, usize>,
    pub network_ids: BTreeMap<String, usize>,
    pub terminal_ids: BTreeMap<String, usize>,
}

pub fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn read<T: DeserializeOwned>(path: &Path) -> Result<(T, String)> {
    let entity = path.to_string_lossy();
    let bytes = std::fs::read(path)
        .map_err(|e| Diagnostic::new("input.read", "admission", e.to_string(), Some(&entity)))?;
    let parsed = serde_json::from_slice(&bytes)
        .map_err(|e| Diagnostic::new("input.schema", "admission", e.to_string(), Some(&entity)))?;
    Ok((parsed, hash(&bytes)))
}

pub fn rational(pair: [i64; 2], entity: &str) -> Result<BigRational> {
    require(
        pair[1] != 0,
        "input.rational",
        "Zero rational denominator",
        Some(entity),
    )?;
    Ok(BigRational::new(pair[0].into(), pair[1].into()))
}

impl Catalog {
    pub fn load(root: &Path, case_path: &Path) -> Result<Self> {
        let (case_json, case_hash): (serde_json::Value, _) = read(case_path)?;
        let case: Case = serde_json::from_value(case_json.clone()).map_err(|e| {
            Diagnostic::new(
                "input.schema",
                "admission",
                e.to_string(),
                Some(&case_path.to_string_lossy()),
            )
        })?;
        require(
            case.schema_version == 1,
            "input.schema",
            "Unsupported case schema",
            Some(&case_path.to_string_lossy()),
        )?;
        let path_for = |name: &str| -> Result<PathBuf> {
            case.catalogs
                .get(name)
                .map(|p| root.join(p))
                .ok_or_else(|| {
                    Diagnostic::new(
                        "input.catalog",
                        "admission",
                        "Missing catalog path",
                        Some(name),
                    )
                })
        };
        let mut provenance = BTreeMap::from([("case".into(), case_hash)]);
        let (facilities, digest): (FacilityFile, _) = read(&path_for("facilities")?)?;
        provenance.insert("facilities".into(), digest);
        let (recipes, digest): (RecipeFile, _) = read(&path_for("recipes")?)?;
        provenance.insert("recipes".into(), digest);
        let (transports, digest): (TransportFile, _) = read(&path_for("transports")?)?;
        provenance.insert("transports".into(), digest);
        require(
            facilities.schema_version == 3
                && recipes.schema_version == 1
                && transports.schema_version == 1,
            "input.schema",
            "Unsupported catalog schema",
            Some(&case_path.to_string_lossy()),
        )?;
        let type_map: BTreeMap<_, _> = facilities.facilities.iter().map(|f| (&f.id, f)).collect();
        let recipe_map: BTreeMap<_, _> = recipes.recipes.iter().map(|r| (&r.id, r)).collect();
        require(
            type_map.len() == facilities.facilities.len()
                && recipe_map.len() == recipes.recipes.len(),
            "input.duplicate",
            "Duplicate catalog identifiers",
            None,
        )?;
        let belt = transports
            .transports
            .iter()
            .find(|t| t.kind == "belt")
            .ok_or_else(|| {
                Diagnostic::new(
                    "input.transport",
                    "admission",
                    "Missing belt transport",
                    None,
                )
            })?;
        let capacity = rational([belt.capacity.quantity, belt.capacity.duration_ms], "belt")?
            * BigRational::from_integer(1000.into());
        require(
            capacity > BigRational::zero(),
            "input.capacity",
            "Belt capacity must be positive",
            None,
        )?;
        let facility_ids: BTreeMap<_, _> = case
            .facilities
            .iter()
            .enumerate()
            .map(|(i, f)| (f.id.clone(), i))
            .collect();
        let network_ids: BTreeMap<_, _> = case
            .networks
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        require(
            facility_ids.len() == case.facilities.len(),
            "input.duplicate",
            "Duplicate facilities",
            None,
        )?;
        require(
            network_ids.len() == case.networks.len(),
            "input.duplicate",
            "Duplicate networks",
            None,
        )?;
        require(
            !facility_ids.is_empty() && !network_ids.is_empty(),
            "input.inventory",
            "Pilot requires facilities and networks",
            None,
        )?;
        require(
            case.networks.iter().all(|n| !n.id.contains(':')),
            "input.identifier",
            "Network identifiers cannot contain a terminal separator",
            None,
        )?;
        let mut expected = BTreeMap::<(String, bool, String), BigRational>::new();
        let mut observed = BTreeMap::<(String, bool, String), BigRational>::new();
        let mut instance_types = Vec::new();
        let mut instance_recipes = Vec::new();
        for f in &case.facilities {
            let recipe = recipe_map.get(&f.recipe).ok_or_else(|| {
                Diagnostic::new("request.recipe", "admission", "Unknown recipe", Some(&f.id))
            })?;
            let definition = type_map.get(&recipe.facility).ok_or_else(|| {
                Diagnostic::new(
                    "request.facility_type",
                    "admission",
                    "Unknown facility type",
                    Some(&f.id),
                )
            })?;
            require(
                definition.footprint.width > 0
                    && definition.footprint.height > 0
                    && definition
                        .allowed_rotations
                        .iter()
                        .all(|r| [0, 90, 180, 270].contains(r)),
                "input.geometry",
                "Invalid footprint or rotation domain",
                Some(&f.id),
            )?;
            let ports: BTreeSet<_> = definition.ports.iter().map(|p| &p.id).collect();
            require(
                ports.len() == definition.ports.len(),
                "input.duplicate",
                "Duplicate physical ports",
                Some(&f.id),
            )?;
            for p in &definition.ports {
                require(
                    ["north", "east", "south", "west"].contains(&p.edge.as_str())
                        && p.position.x >= 0
                        && p.position.y >= 0
                        && p.position.x < definition.footprint.width
                        && p.position.y < definition.footprint.height,
                    "input.geometry",
                    "Invalid catalog port geometry",
                    Some(&p.id),
                )?;
            }
            let utilization = rational(f.utilization, &f.id)?;
            require(
                utilization > BigRational::zero() && utilization <= BigRational::one(),
                "request.utilization",
                "Utilization outside (0,1]",
                Some(&f.id),
            )?;
            require(
                recipe.duration_ms > 0,
                "input.recipe",
                "Recipe duration must be positive",
                Some(&recipe.id),
            )?;
            for (source, entries) in [(false, &recipe.inputs), (true, &recipe.outputs)] {
                for entry in entries {
                    let rate = &utilization
                        * rational([entry.quantity, recipe.duration_ms], &recipe.id)?
                        * BigRational::from_integer(1000.into());
                    *expected
                        .entry((f.id.clone(), source, entry.item.clone()))
                        .or_default() += rate;
                }
            }
            instance_types.push((*definition).clone());
            instance_recipes.push((*recipe).clone());
        }
        let mut rates = Vec::new();
        let mut terminals = Vec::new();
        let mut exports = BTreeMap::<String, BigRational>::new();
        for (ni, n) in case.networks.iter().enumerate() {
            let rate = rational(n.rate, &n.id)?;
            require(
                rate > BigRational::zero() && rate <= capacity,
                "request.capacity",
                "Network exceeds catalog belt capacity",
                Some(&n.id),
            )?;
            for (source, endpoint) in [(true, &n.source), (false, &n.target)] {
                let facility = match endpoint {
                    Endpoint::Facility { facility } => {
                        let fi = facility_ids.get(facility).copied().ok_or_else(|| {
                            Diagnostic::acceptance(
                                "request.facility",
                                "Unknown facility",
                                Some(&n.id),
                            )
                        })?;
                        *observed
                            .entry((facility.clone(), source, n.item.clone()))
                            .or_default() += &rate;
                        Some(fi)
                    }
                    Endpoint::External { .. } => {
                        if source {
                            require(
                                recipes.external_items.contains(&n.item),
                                "request.external",
                                "Undeclared external input item",
                                Some(&n.id),
                            )?;
                        } else {
                            *exports.entry(n.item.clone()).or_default() += &rate;
                        }
                        None
                    }
                };
                terminals.push(Terminal {
                    id: format!("{}:{}", n.id, if source { "source" } else { "target" }),
                    network: ni,
                    source,
                    facility,
                });
            }
            rates.push(rate);
        }
        require(
            expected == observed,
            "request.coverage",
            "Networks do not exactly cover recipe obligations",
            None,
        )?;
        require(
            exports
                == BTreeMap::from([(
                    case.target.item.clone(),
                    rational(case.target.rate, "target")?,
                )]),
            "request.target",
            "External output differs from target",
            None,
        )?;
        let terminal_ids = terminals
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.clone(), i))
            .collect();
        Ok(Self {
            case,
            case_json,
            path: case_path.to_owned(),
            provenance,
            types: instance_types,
            recipes: instance_recipes,
            rates,
            capacity,
            terminals,
            facility_ids,
            network_ids,
            terminal_ids,
        })
    }
}
