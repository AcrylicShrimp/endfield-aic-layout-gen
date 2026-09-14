use super::types::*;
use aic_data::facilities::ValidatedFacilityCatalog;
use aic_data::layouts::FactoryEditCatalogs;
use aic_data::logistics::{
    ValidatedItemCatalog, ValidatedLogisticsComponentCatalog, ValidatedTransportCatalog,
};
use aic_data::recipes::ValidatedRecipeBook;
use serde_json::{Value, json};
use std::path::PathBuf;
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}
pub(super) fn fixture() -> (Witness, FactoryEditCatalogs, Value) {
    let root = root();
    let data = root.join("data/game/normalized");
    let paths = json!({"facilities": data.join("facilities.json"), "recipes": data.join("recipes.json"), "items": data.join("items.json"),
        "transports": data.join("transports.json"), "components": data.join("logistics-components.json")});
    let read = |key: &str| std::fs::read(paths[key].as_str().unwrap()).unwrap();
    let catalogs = FactoryEditCatalogs {
        facilities: ValidatedFacilityCatalog::try_from_catalog(
            serde_json::from_slice(&read("facilities")).unwrap(),
        )
        .unwrap(),
        recipes: ValidatedRecipeBook::try_from_recipe_book(
            serde_json::from_slice(&read("recipes")).unwrap(),
        )
        .unwrap(),
        items: ValidatedItemCatalog::try_from_catalog(
            serde_json::from_slice(&read("items")).unwrap(),
        )
        .unwrap(),
        transports: ValidatedTransportCatalog::try_from_catalog(
            serde_json::from_slice(&read("transports")).unwrap(),
        )
        .unwrap(),
        components: ValidatedLogisticsComponentCatalog::try_from_catalog(
            serde_json::from_slice(&read("components")).unwrap(),
        )
        .unwrap(),
    };
    let w = serde_json::from_slice(
        &std::fs::read(root.join(
            "docs/benchmarks/constructive-bound-interconnect/fixtures/crossing-witness.json",
        ))
        .unwrap(),
    )
    .unwrap();
    (w, catalogs, paths)
}

pub(super) fn fixture_request(w: &Witness) -> aic_data::layouts::FactoryEditRequest {
    use aic_data::layouts::{
        FactoryEditEndpoint, FactoryEditInstance, FactoryEditRequest, FactoryEditTerminal,
        TransportNetworkEndpoint,
    };
    // Fixed external fixture obligations are captured before any test mutation.
    let mut terminals = std::collections::BTreeMap::new();
    for n in &w.transport_networks {
        for t in &n.terminals {
            let endpoint = match &t.endpoint {
                TransportNetworkEndpoint::Facility { instance, .. } => {
                    FactoryEditEndpoint::Facility {
                        instance: instance.clone(),
                    }
                }
                TransportNetworkEndpoint::External { node, .. } => {
                    FactoryEditEndpoint::External { node: node.clone() }
                }
            };
            let rate = terminals
                .entry((endpoint, t.direction, n.item.clone()))
                .or_insert(aic_data::recipes::Rate::zero());
            *rate = rate.checked_add(t.rate).unwrap();
        }
    }
    FactoryEditRequest {
        schema_version: 1,
        instances: w
            .placements
            .iter()
            .map(|p| FactoryEditInstance {
                instance: p.instance.clone(),
                recipe: p.recipe.clone(),
                utilization: w.utilizations[&p.instance],
            })
            .collect(),
        terminals: terminals
            .into_iter()
            .map(|((endpoint, direction, item), rate)| FactoryEditTerminal {
                endpoint,
                direction,
                item,
                rate,
            })
            .collect(),
    }
}

#[test]
#[ignore = "requires AIC_COLLECTION_CONFIG and AIC_COLLECTION_RESULTS"]
fn retained_best_layouts_pass_explicit_physical_audit() {
    let config_path = PathBuf::from(std::env::var("AIC_COLLECTION_CONFIG").unwrap());
    let config: Value = serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    let paths = serde_json::from_value(config["catalogs"].clone()).unwrap();
    let catalogs = crate::input::load_catalogs(
        std::path::Path::new(config["catalog_root"].as_str().unwrap()),
        &paths,
    )
    .unwrap();
    let results: Vec<Value> = serde_json::from_slice(
        &std::fs::read(std::env::var("AIC_COLLECTION_RESULTS").unwrap()).unwrap(),
    )
    .unwrap();
    for row in results {
        let stream = row["stream"].as_u64().unwrap() as usize;
        let request = serde_json::from_slice(
            &std::fs::read(
                config_path.parent().unwrap().join(
                    config["episodes"][stream]["request"]["path"]
                        .as_str()
                        .unwrap(),
                ),
            )
            .unwrap(),
        )
        .unwrap();
        let witness = serde_json::from_value(row["native"]["best"].clone()).unwrap();
        assert!(
            aic_data::layouts::validate_factory_layout(&request, &witness, &catalogs.data)
                .factory_validated,
            "stream {stream}"
        );
    }
}
