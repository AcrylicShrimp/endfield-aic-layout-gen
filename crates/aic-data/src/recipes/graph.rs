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
    UnknownExternalInput { item: String },
    AmbiguousProducer { item: String, recipes: Vec<String> },
    RecipeCycle { recipes: Vec<String> },
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
            Self::UnknownExternalInput { item } => {
                write!(
                    formatter,
                    "external input item '{item}' is neither external nor recipe-produced"
                )
            }
            Self::AmbiguousProducer { item, recipes } => write!(
                formatter,
                "item '{item}' is produced by multiple recipes: {}",
                recipes.join(", ")
            ),
            Self::RecipeCycle { recipes } => write!(
                formatter,
                "cyclic recipe dependency encountered: {}",
                recipes.join(" -> ")
            ),
        }
    }
}

impl std::error::Error for RecipeGraphError {}

impl ValidatedRecipeBook {
    pub fn resolve_graph(&self, target_item: &str) -> Result<RecipeGraph, RecipeGraphError> {
        self.resolve_graph_with_external_inputs(target_item, &[])
    }

    pub fn resolve_graph_with_external_inputs(
        &self,
        target_item: &str,
        external_inputs: &[String],
    ) -> Result<RecipeGraph, RecipeGraphError> {
        validate_target_item_id(target_item)?;

        for item in external_inputs {
            if !self.index().is_external_item(item) && self.index().producer_ids_for(item).is_none()
            {
                return Err(RecipeGraphError::UnknownExternalInput { item: item.clone() });
            }
        }

        let external_inputs = external_inputs.iter().cloned().collect::<BTreeSet<_>>();

        if !self.index().is_external_item(target_item)
            && !external_inputs.contains(target_item)
            && self.index().producer_ids_for(target_item).is_none()
        {
            return Err(RecipeGraphError::UnknownTargetItem {
                target_item: target_item.to_string(),
            });
        }

        let mut resolver = GraphResolver::new(target_item, self.index(), external_inputs);
        resolver.resolve_item(target_item)?;
        Ok(resolver.into_graph())
    }
}

struct GraphResolver<'a> {
    target_item: String,
    index: &'a RecipeIndex,
    seen_external_items: BTreeSet<String>,
    resolved_recipe_ids: HashSet<String>,
    visiting_recipe_ids: HashSet<String>,
    recipe_stack: Vec<String>,
    recipes: Vec<Recipe>,
    additional_external_items: BTreeSet<String>,
}

impl<'a> GraphResolver<'a> {
    fn new(
        target_item: &str,
        index: &'a RecipeIndex,
        additional_external_items: BTreeSet<String>,
    ) -> Self {
        Self {
            target_item: target_item.to_string(),
            index,
            seen_external_items: BTreeSet::new(),
            resolved_recipe_ids: HashSet::new(),
            visiting_recipe_ids: HashSet::new(),
            recipe_stack: Vec::new(),
            recipes: Vec::new(),
            additional_external_items,
        }
    }

    fn resolve_item(&mut self, item: &str) -> Result<(), RecipeGraphError> {
        if self.index.is_external_item(item) || self.additional_external_items.contains(item) {
            self.seen_external_items.insert(item.to_string());
            return Ok(());
        }

        let Some(producer_ids) = self.index.producer_ids_for(item) else {
            return Ok(());
        };
        if producer_ids.len() != 1 {
            return Err(RecipeGraphError::AmbiguousProducer {
                item: item.to_string(),
                recipes: producer_ids.to_vec(),
            });
        }

        let recipe = self
            .index
            .recipe(&producer_ids[0])
            .expect("validated recipe index contains every producer recipe");
        self.resolve_recipe(recipe)
    }

    fn resolve_recipe(&mut self, recipe: &Recipe) -> Result<(), RecipeGraphError> {
        if self.resolved_recipe_ids.contains(&recipe.id) {
            return Ok(());
        }
        if self.visiting_recipe_ids.contains(&recipe.id) {
            let start = self
                .recipe_stack
                .iter()
                .position(|recipe_id| recipe_id == &recipe.id)
                .unwrap_or(0);
            let mut recipes = self.recipe_stack[start..].to_vec();
            recipes.push(recipe.id.clone());
            return Err(RecipeGraphError::RecipeCycle { recipes });
        }

        self.visiting_recipe_ids.insert(recipe.id.clone());
        self.recipe_stack.push(recipe.id.clone());

        for input in &recipe.inputs {
            self.resolve_item(&input.item)?;
        }

        self.recipe_stack.pop();
        self.visiting_recipe_ids.remove(&recipe.id);
        self.resolved_recipe_ids.insert(recipe.id.clone());
        self.recipes.push(recipe.clone());
        Ok(())
    }

    fn into_graph(self) -> RecipeGraph {
        RecipeGraph {
            target_item: self.target_item,
            external_items: self.seen_external_items.into_iter().collect(),
            recipes: self.recipes,
        }
    }
}
