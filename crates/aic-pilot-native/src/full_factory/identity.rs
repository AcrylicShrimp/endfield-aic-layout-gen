//! Python-compatible cold DTO identities, cached by retained state revision.
use serde::Serialize;
use serde_json::Value;

use super::candidates::{Action, Domain};
use super::types::Witness;

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
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(m) => Value::Object(m.into_iter().map(|(k, v)| (k, canonical(v))).collect()),
        Value::Array(a) => {
            let mut keyed: Vec<_> = a
                .into_iter()
                .map(|v| {
                    let v = canonical(v);
                    (ascii_json(&v), v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Array(keyed.into_iter().map(|(_, v)| v).collect())
        }
        scalar => scalar,
    }
}
pub fn digest(value: &impl Serialize) -> String {
    crate::catalog::hash(
        ascii_json(&serde_json::to_value(value).expect("typed finite DTO")).as_bytes(),
    )
}
#[derive(Clone)]
pub struct Identity {
    pub ordered: String,
    pub physical: String,
}
impl Identity {
    pub fn new(w: &Witness) -> Self {
        let value = serde_json::to_value(w).expect("typed witness DTO");
        Self {
            ordered: crate::catalog::hash(ascii_json(&value).as_bytes()),
            physical: crate::catalog::hash(ascii_json(&canonical(value)).as_bytes()),
        }
    }
}
pub fn action_key(a: &Action, domain: &Domain, w: &Witness, best_key: &str) -> String {
    let mut value = serde_json::to_value(a).expect("typed action DTO");
    match a {
        Action::Pose(_) => {
            let ps = value["poses"].as_array_mut().unwrap();
            for p in ps.iter_mut() {
                p["member"] = Value::String(
                    w.placements[p["member"].as_u64().unwrap() as usize]
                        .instance
                        .clone(),
                );
            }
            ps.sort_by(|a, b| a["member"].as_str().cmp(&b["member"].as_str()));
        }
        Action::Reroute { chain, .. } => {
            let c = &domain.decomposition.chains[*chain];
            value.as_object_mut().unwrap().remove("chain");
            value["network"] = Value::String(w.transport_networks[c.network].id.clone());
            value["directed_cells"] = serde_json::to_value(&c.cells).unwrap();
            value["rate"] = serde_json::to_value(c.rate).unwrap();
        }
        Action::RestoreBest { .. } => {
            value["best_key"] = Value::String(best_key.into());
        }
        _ => {}
    }
    crate::catalog::hash(ascii_json(&value).as_bytes())
}
