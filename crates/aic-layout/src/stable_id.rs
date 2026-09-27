pub const STABLE_ID_PATTERN: &str = "^[a-z0-9]+(-[a-z0-9]+)*$";

#[derive(Default)]
pub struct StableIdCursor {
    next_byte: usize,
    previous_was_hyphen: bool,
}

pub enum StableIdAdvance {
    Pending,
    Complete(bool),
}

impl StableIdCursor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Admit one bounded chunk before scanning it. The cursor keeps the byte
    /// index, so a page stop never repeats accepted input work.
    pub fn advance(&mut self, value: &str, mut admit: impl FnMut() -> bool) -> StableIdAdvance {
        if value.is_empty() || value.starts_with('-') || value.ends_with('-') {
            return StableIdAdvance::Complete(false);
        }
        let bytes = value.as_bytes();
        while self.next_byte < bytes.len() {
            if !admit() {
                return StableIdAdvance::Pending;
            }
            let end = self.next_byte.saturating_add(64).min(bytes.len());
            while self.next_byte < end {
                let byte = bytes[self.next_byte];
                let is_segment_char = byte.is_ascii_lowercase() || byte.is_ascii_digit();
                let is_hyphen = byte == b'-';
                if !is_segment_char && !is_hyphen || is_hyphen && self.previous_was_hyphen {
                    return StableIdAdvance::Complete(false);
                }
                self.previous_was_hyphen = is_hyphen;
                self.next_byte += 1;
            }
        }
        StableIdAdvance::Complete(true)
    }
}

pub fn is_stable_id(value: &str) -> bool {
    match StableIdCursor::new().advance(value, || true) {
        StableIdAdvance::Complete(valid) => valid,
        StableIdAdvance::Pending => unreachable!("unbounded stable ID scan cannot yield"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_id_chunks_resume_across_a_hyphen_boundary() {
        for value in [
            format!("{}-b", "a".repeat(64)),
            format!("{}--b", "a".repeat(64)),
        ] {
            let mut cursor = StableIdCursor::new();
            let mut charged = 0;
            let actual = loop {
                let mut page_credit = 1;
                match cursor.advance(&value, || {
                    if page_credit == 0 {
                        false
                    } else {
                        page_credit -= 1;
                        charged += 1;
                        true
                    }
                }) {
                    StableIdAdvance::Pending => {}
                    StableIdAdvance::Complete(valid) => break valid,
                }
            };
            assert_eq!(actual, is_stable_id(&value));
            assert_eq!(charged, 2);
        }
    }
}
