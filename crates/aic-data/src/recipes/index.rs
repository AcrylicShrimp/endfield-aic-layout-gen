use std::collections::{BTreeSet, HashMap, HashSet};

use crate::recipes::{Recipe, RecipeBook};

pub(crate) struct RecipeAnalysis<'a> {
    pub external_items: HashSet<&'a str>,
    pub output_producers: HashMap<&'a str, Vec<&'a Recipe>>,
}

impl<'a> RecipeAnalysis<'a> {
    pub fn from_raw(book: &'a RecipeBook) -> Self {
        let external_items = book
            .external_items
            .iter()
            .map(String::as_str)
            .collect::<HashSet<_>>();
        let mut output_producers = HashMap::<&str, Vec<&Recipe>>::new();

        for recipe in &book.recipes {
            for output in &recipe.outputs {
                output_producers
                    .entry(output.item.as_str())
                    .or_default()
                    .push(recipe);
            }
        }

        Self {
            external_items,
            output_producers,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RecipeIndex {
    external_items: BTreeSet<String>,
    output_producer_by_item: HashMap<String, String>,
    recipe_by_id: HashMap<String, Recipe>,
}

impl RecipeIndex {
    pub fn from_validated(book: &RecipeBook) -> Self {
        let analysis = RecipeAnalysis::from_raw(book);
        let external_items = analysis
            .external_items
            .into_iter()
            .map(str::to_string)
            .collect::<BTreeSet<_>>();
        let mut output_producer_by_item = HashMap::new();

        for (item, producers) in analysis.output_producers {
            if let Some(recipe) = producers.first() {
                output_producer_by_item.insert(item.to_string(), recipe.id.clone());
            }
        }

        let recipe_by_id = book
            .recipes
            .iter()
            .map(|recipe| (recipe.id.clone(), recipe.clone()))
            .collect();

        Self {
            external_items,
            output_producer_by_item,
            recipe_by_id,
        }
    }

    pub fn is_external_item(&self, item: &str) -> bool {
        self.external_items.contains(item)
    }

    pub fn producer_for(&self, item: &str) -> Option<&Recipe> {
        self.output_producer_by_item
            .get(item)
            .and_then(|recipe_id| self.recipe_by_id.get(recipe_id))
    }
}
