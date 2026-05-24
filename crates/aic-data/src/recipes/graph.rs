use std::collections::{BTreeSet, HashSet};

use crate::recipes::{
    Recipe, RecipeGraph, ValidatedRecipeBook,
    id::{STABLE_ID_PATTERN, validate_target_item_id},
    index::RecipeIndex,
};

#[derive(Debug)]
pub enum RecipeGraphError {
    InvalidTargetId { target_item: String },
    UnknownTargetItem { target_item: String },
}

impl std::fmt::Display for RecipeGraphError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTargetId { target_item } => {
                write!(
                    formatter,
                    "target item '{target_item}' must match {STABLE_ID_PATTERN}"
                )
            }
            Self::UnknownTargetItem { target_item } => {
                write!(
                    formatter,
                    "target item '{target_item}' is neither external nor recipe-produced"
                )
            }
        }
    }
}

impl std::error::Error for RecipeGraphError {}

impl ValidatedRecipeBook {
    pub fn resolve_graph(&self, target_item: &str) -> Result<RecipeGraph, RecipeGraphError> {
        validate_target_item_id(target_item)?;

        if !self.index().is_external_item(target_item)
            && self.index().producer_for(target_item).is_none()
        {
            return Err(RecipeGraphError::UnknownTargetItem {
                target_item: target_item.to_string(),
            });
        }

        let mut resolver = GraphResolver::new(target_item, self.index());
        resolver.resolve_item(target_item);
        Ok(resolver.into_graph())
    }
}

struct GraphResolver<'a> {
    target_item: String,
    index: &'a RecipeIndex,
    seen_external_items: BTreeSet<String>,
    seen_recipe_ids: HashSet<String>,
    recipes: Vec<Recipe>,
}

impl<'a> GraphResolver<'a> {
    fn new(target_item: &str, index: &'a RecipeIndex) -> Self {
        Self {
            target_item: target_item.to_string(),
            index,
            seen_external_items: BTreeSet::new(),
            seen_recipe_ids: HashSet::new(),
            recipes: Vec::new(),
        }
    }

    fn resolve_item(&mut self, item: &str) {
        if self.index.is_external_item(item) {
            self.seen_external_items.insert(item.to_string());
            return;
        }

        if let Some(recipe) = self.index.producer_for(item) {
            self.resolve_recipe(recipe);
        }
    }

    fn resolve_recipe(&mut self, recipe: &Recipe) {
        if !self.seen_recipe_ids.insert(recipe.id.clone()) {
            return;
        }

        for input in &recipe.inputs {
            self.resolve_item(&input.item);
        }

        self.recipes.push(recipe.clone());
    }

    fn into_graph(self) -> RecipeGraph {
        RecipeGraph {
            target_item: self.target_item,
            external_items: self.seen_external_items.into_iter().collect(),
            recipes: self.recipes,
        }
    }
}
