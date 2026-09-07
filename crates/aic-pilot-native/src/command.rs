//! Typed native commands and the exact Python canonical JSON ordering contract.

use serde_json::{Value, json};

use crate::catalog::Catalog;
use crate::diagnostic::{Diagnostic, Result};
use crate::state::{Binding, Point, Pose, SIDES};

pub const OPERATIONS: [&str; 7] = [
    "SET_PLACEMENTS",
    "UNBIND_TERMINALS",
    "BIND_TERMINALS",
    "ERASE_TRANSPORT",
    "EXTEND",
    "CONNECT",
    "DISCARD_EDIT",
];

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Command {
    SetPlacements(Vec<(usize, Pose)>),
    Unbind(usize),
    Bind(usize, Binding),
    Erase(usize, Vec<Point>),
    Extend {
        network: usize,
        anchor: Point,
        backward: bool,
        direction: usize,
        length: i64,
    },
    Connect {
        network: usize,
        source: Point,
        target: Point,
    },
    Discard,
}

impl Command {
    pub fn operation(&self) -> usize {
        match self {
            Self::SetPlacements(_) => 0,
            Self::Unbind(_) => 1,
            Self::Bind(..) => 2,
            Self::Erase(..) => 3,
            Self::Extend { .. } => 4,
            Self::Connect { .. } => 5,
            Self::Discard => 6,
        }
    }

    pub fn entity<'a>(&self, catalog: &'a Catalog) -> Option<&'a str> {
        match self {
            Self::Unbind(ti) | Self::Bind(ti, _) => Some(&catalog.terminals[*ti].id),
            Self::Erase(ni, _)
            | Self::Extend { network: ni, .. }
            | Self::Connect { network: ni, .. } => Some(&catalog.case.networks[*ni].id),
            _ => None,
        }
    }

    pub fn export(&self, catalog: &Catalog) -> Value {
        let op = OPERATIONS[self.operation()];
        match self {
            Self::SetPlacements(assignments) => {
                let assignments: serde_json::Map<_, _> = assignments
                    .iter()
                    .map(|(fi, p)| (catalog.case.facilities[*fi].id.clone(), json!(p)))
                    .collect();
                json!({"op":op,"assignments":assignments})
            }
            Self::Unbind(ti) => json!({"op":op,"terminal":catalog.terminals[*ti].id}),
            Self::Bind(ti, binding) => {
                json!({"op":op,"terminal":catalog.terminals[*ti].id,"binding":binding})
            }
            Self::Erase(ni, cells) => {
                json!({"op":op,"network":catalog.case.networks[*ni].id,"cells":cells})
            }
            Self::Extend {
                network,
                anchor,
                backward,
                direction,
                length,
            } => {
                json!({"op":op,"network":catalog.case.networks[*network].id,"anchor":anchor,"growth":if *backward { "backward" } else { "forward" },"direction":SIDES[*direction],"length":length})
            }
            Self::Connect {
                network,
                source,
                target,
            } => {
                json!({"op":op,"network":catalog.case.networks[*network].id,"source":source,"target":target})
            }
            Self::Discard => json!({"op":op}),
        }
    }

    pub fn import(catalog: &Catalog, value: &Value) -> Result<Self> {
        let error = |message: &str| Diagnostic::new("command.malformed", "edit", message, None);
        let get_str = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| error(&format!("Expected string {key}")))
        };
        let terminal = || {
            catalog
                .terminal_ids
                .get(get_str("terminal")?)
                .copied()
                .ok_or_else(|| error("Unknown terminal"))
        };
        let network = || {
            catalog
                .network_ids
                .get(get_str("network")?)
                .copied()
                .ok_or_else(|| error("Unknown network"))
        };
        let point = |key: &str| {
            serde_json::from_value::<Point>(
                value
                    .get(key)
                    .cloned()
                    .ok_or_else(|| error(&format!("Missing {key}")))?,
            )
            .map_err(|e| error(&e.to_string()))
        };
        Ok(match get_str("op")? {
            "SET_PLACEMENTS" => {
                let map = value
                    .get("assignments")
                    .and_then(Value::as_object)
                    .ok_or_else(|| error("Expected assignments object"))?;
                let mut assignments = Vec::new();
                for (fid, p) in map {
                    let fi = catalog
                        .facility_ids
                        .get(fid)
                        .copied()
                        .ok_or_else(|| error("Unknown facility"))?;
                    let pose = serde_json::from_value(p.clone())
                        .map_err(|_| error("Placement coordinates must be integers"))?;
                    assignments.push((fi, pose));
                }
                Self::SetPlacements(assignments)
            }
            "UNBIND_TERMINALS" => Self::Unbind(terminal()?),
            "BIND_TERMINALS" => Self::Bind(
                terminal()?,
                serde_json::from_value(
                    value
                        .get("binding")
                        .cloned()
                        .ok_or_else(|| error("Missing binding"))?,
                )
                .map_err(|e| error(&e.to_string()))?,
            ),
            "ERASE_TRANSPORT" => Self::Erase(
                network()?,
                serde_json::from_value(
                    value
                        .get("cells")
                        .cloned()
                        .ok_or_else(|| error("Missing cells"))?,
                )
                .map_err(|e| error(&e.to_string()))?,
            ),
            "EXTEND" => {
                let direction = SIDES
                    .iter()
                    .position(|s| *s == get_str("direction").unwrap_or(""))
                    .ok_or_else(|| error("Unknown direction"))?;
                let backward = match get_str("growth")? {
                    "forward" => false,
                    "backward" => true,
                    _ => return Err(error("Invalid growth direction")),
                };
                Self::Extend {
                    network: network()?,
                    anchor: point("anchor")?,
                    backward,
                    direction,
                    length: value
                        .get("length")
                        .and_then(Value::as_i64)
                        .ok_or_else(|| error("Expected integer length"))?,
                }
            }
            "CONNECT" => Self::Connect {
                network: network()?,
                source: point("source")?,
                target: point("target")?,
            },
            "DISCARD_EDIT" => Self::Discard,
            _ => return Err(error("Unknown operation")),
        })
    }
}

/// Python JSONEncoder(sort_keys=True): ASCII escaping and default separator spaces.
/// Candidate values contain only strings, integers, lists and objects.
pub fn canonical(value: &Value) -> String {
    fn string(value: &str, output: &mut String) {
        use std::fmt::Write;
        output.push('"');
        for c in value.chars() {
            match c {
                '"' => output.push_str("\\\""),
                '\\' => output.push_str("\\\\"),
                '\u{8}' => output.push_str("\\b"),
                '\u{c}' => output.push_str("\\f"),
                '\n' => output.push_str("\\n"),
                '\r' => output.push_str("\\r"),
                '\t' => output.push_str("\\t"),
                ' '..='~' => output.push(c),
                c => {
                    for unit in c.encode_utf16(&mut [0; 2]) {
                        let _ = write!(output, "\\u{unit:04x}");
                    }
                }
            }
        }
        output.push('"');
    }
    fn write(value: &Value, output: &mut String) {
        match value {
            Value::String(s) => string(s, output),
            Value::Array(values) => {
                output.push('[');
                for (i, value) in values.iter().enumerate() {
                    if i != 0 {
                        output.push_str(", ");
                    }
                    write(value, output);
                }
                output.push(']');
            }
            Value::Object(values) => {
                output.push('{');
                // serde_json's default map order is Unicode scalar order, as in Python.
                for (i, (key, value)) in values.iter().enumerate() {
                    if i != 0 {
                        output.push_str(", ");
                    }
                    string(key, output);
                    output.push_str(": ");
                    write(value, output);
                }
                output.push('}');
            }
            _ => output.push_str(&value.to_string()),
        }
    }
    let mut output = String::new();
    write(value, &mut output);
    output
}
