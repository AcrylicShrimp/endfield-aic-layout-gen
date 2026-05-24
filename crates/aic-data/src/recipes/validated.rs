use crate::recipes::{RecipeBook, ValidationReport, index::RecipeIndex, validate_recipe_book};

#[derive(Debug, Clone)]
pub struct ValidatedRecipeBook {
    book: RecipeBook,
    index: RecipeIndex,
}

impl ValidatedRecipeBook {
    pub fn try_from_recipe_book(book: RecipeBook) -> Result<Self, ValidationReport> {
        let report = validate_recipe_book(&book);
        if !report.valid {
            return Err(report);
        }

        let index = RecipeIndex::from_validated(&book);
        Ok(Self { book, index })
    }

    pub fn recipe_book(&self) -> &RecipeBook {
        &self.book
    }

    pub(crate) fn index(&self) -> &RecipeIndex {
        &self.index
    }
}
