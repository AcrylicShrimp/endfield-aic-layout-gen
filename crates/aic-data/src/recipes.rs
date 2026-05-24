mod graph;
mod id;
mod index;
mod load;
mod model;
mod throughput;
mod validate;
mod validated;

pub use graph::RecipeGraphError;
pub use id::validate_target_item_id;
pub use load::{LoadRecipeBookError, load_recipe_book};
pub use model::{ItemAmount, Recipe, RecipeBook, RecipeGraph};
pub use throughput::{
    ItemRate, Rate, RecipeRunRate, RecipeThroughputReport, RecipeThroughputRequest,
    SUPPORTED_THROUGHPUT_REQUEST_SCHEMA_VERSION, ThroughputDiagnostic, ThroughputTarget,
    validate_throughput_request,
};
pub use validate::{
    SUPPORTED_SCHEMA_VERSION, ValidationDiagnostic, ValidationReport, validate_recipe_book,
};
pub use validated::ValidatedRecipeBook;

#[cfg(test)]
mod tests;
