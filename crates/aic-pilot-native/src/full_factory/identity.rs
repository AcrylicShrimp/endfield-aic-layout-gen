//! Python-compatible cold DTO identities, cached by retained state revision.
use serde::Serialize;
use serde_json::Value;

// serde_json maps are ordered by key. Python's original identity also used
// ensure_ascii=True, including UTF-16 surrogate escapes for non-BMP codepoints.
pub fn ascii_json(value: &Value) -> String {
    let encoded = serde_json::to_string(value).expect("typed finite DTO serialization");
    let mut output = String::with_capacity(encoded.len());
    for c in encoded.chars() {
        if c.is_ascii() && c != '\u{7f}' {
            output.push(c);
        } else {
            use std::fmt::Write;
            for u in c.encode_utf16(&mut [0; 2]) {
                write!(output, "\\u{u:04x}").unwrap();
            }
        }
    }
    output
}
pub fn digest(value: &impl Serialize) -> String {
    crate::input::hash(
        ascii_json(&serde_json::to_value(value).expect("typed finite DTO")).as_bytes(),
    )
}
