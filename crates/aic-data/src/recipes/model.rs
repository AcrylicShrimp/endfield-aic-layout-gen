use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RecipeBook {
    pub schema_version: u32,
    pub external_items: Vec<String>,
    pub recipes: Vec<Recipe>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub id: String,
    pub facility: String,
    pub inputs: Vec<ItemAmount>,
    pub outputs: Vec<ItemAmount>,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ItemAmount {
    pub item: String,
    pub quantity: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RecipeGraph {
    pub target_item: String,
    pub external_items: Vec<String>,
    pub recipes: Vec<Recipe>,
}
