use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::candidates::Action;
use super::types::*;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Memory {
    pub seen: BTreeSet<String>,
    pub failed: BTreeMap<String, BTreeSet<String>>,
}
impl Memory {
    pub fn new(initial: &str) -> Self {
        Self {
            seen: BTreeSet::from([initial.into()]),
            failed: BTreeMap::new(),
        }
    }
    pub fn mask(
        &self,
        current: &str,
        keys: &[String],
        actions: &[Action],
        enabled: bool,
        restore: bool,
    ) -> Result<(Vec<bool>, bool)> {
        let blocked = self.failed.get(current).filter(|_| enabled);
        let mut allowed: Vec<_> = keys
            .iter()
            .map(|k| blocked.is_none_or(|b| !b.contains(k)))
            .collect();
        let exhausted = !allowed.iter().any(|b| *b);
        if exhausted {
            allowed[actions
                .iter()
                .position(|a| matches!(a, Action::RestoreBest { .. }))
                .unwrap()] = true;
        }
        if !restore {
            for (ok, a) in allowed.iter_mut().zip(actions) {
                if matches!(a, Action::RestoreBest { .. }) {
                    *ok = false;
                }
            }
        }
        if !allowed.iter().any(|b| *b) {
            return Err(Diagnostic::new(
                "input-contract.action-exhausted",
                "No action remains under the configured input contract",
                current,
            ));
        }
        Ok((allowed, exhausted))
    }
    pub fn record(&mut self, before: &str, after: &str, action: &str) -> (bool, bool) {
        let unchanged = before == after;
        let revisited = !unchanged && self.seen.contains(after);
        if unchanged || revisited {
            self.failed
                .entry(before.into())
                .or_default()
                .insert(action.into());
        }
        self.seen.insert(after.into());
        (unchanged, revisited)
    }
}
