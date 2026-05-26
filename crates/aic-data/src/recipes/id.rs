use crate::recipes::RecipeGraphError;

pub use crate::stable_id::{STABLE_ID_PATTERN, is_stable_id};

pub fn validate_target_item_id(target_item: &str) -> Result<(), RecipeGraphError> {
    if is_stable_id(target_item) {
        Ok(())
    } else {
        Err(RecipeGraphError::InvalidTargetId {
            target_item: target_item.to_string(),
        })
    }
}
