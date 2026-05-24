use crate::recipes::RecipeGraphError;

pub const STABLE_ID_PATTERN: &str = "^[a-z0-9]+(-[a-z0-9]+)*$";

pub fn is_stable_id(value: &str) -> bool {
    if value.is_empty() || value.starts_with('-') || value.ends_with('-') {
        return false;
    }

    let mut previous_was_hyphen = false;

    for byte in value.bytes() {
        let is_segment_char = byte.is_ascii_lowercase() || byte.is_ascii_digit();
        let is_hyphen = byte == b'-';

        if !is_segment_char && !is_hyphen {
            return false;
        }

        if is_hyphen && previous_was_hyphen {
            return false;
        }

        previous_was_hyphen = is_hyphen;
    }

    true
}

pub fn validate_target_item_id(target_item: &str) -> Result<(), RecipeGraphError> {
    if is_stable_id(target_item) {
        Ok(())
    } else {
        Err(RecipeGraphError::InvalidTargetId {
            target_item: target_item.to_string(),
        })
    }
}
