//! One update-local heap index: no repeated full-buffer scans during eviction.
use super::replay::{Entry, Retention};
use anyhow::{Result, ensure};
use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BinaryHeap},
};

#[derive(Clone, Copy, PartialEq)]
struct Score(f64);
impl Eq for Score {}
impl PartialOrd for Score {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Score {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}
type Key = (Score, u64, String, u64);
#[derive(Default)]
struct Stratum {
    count: usize,
    mature: BinaryHeap<Reverse<Key>>,
    oldest: BinaryHeap<Reverse<Key>>,
}
pub struct Index {
    strata: BTreeMap<String, Stratum>,
    present: BTreeMap<String, u64>,
    next_token: u64,
    ordinal: u64,
    config: Retention,
}
impl Index {
    pub fn new(entries: &BTreeMap<String, Entry>, ordinal: u64, config: &Retention) -> Self {
        let mut result = Self {
            strata: BTreeMap::new(),
            present: BTreeMap::new(),
            next_token: 0,
            ordinal,
            config: config.clone(),
        };
        for entry in entries.values() {
            result.add(entry);
        }
        result
    }
    pub fn add(&mut self, entry: &Entry) {
        let token = self.next_token;
        self.next_token += 1;
        self.present.insert(entry.id.clone(), token);
        let row = self.strata.entry(entry.stratum.clone()).or_default();
        row.count += 1;
        row.oldest.push(Reverse((
            Score(0.),
            entry.admitted_update,
            entry.id.clone(),
            token,
        )));
        if let Some(utility) = &entry.utility {
            if utility.draws >= self.config.minimum_draws
                && utility.updates >= self.config.minimum_updates
            {
                let score = utility.ema
                    / (1.
                        + self.ordinal.saturating_sub(utility.last_update) as f64
                            / self.config.stale_updates as f64);
                row.mature.push(Reverse((
                    Score(score),
                    entry.admitted_update,
                    entry.id.clone(),
                    token,
                )));
            }
        }
    }
    pub fn choose(&mut self, incoming: &str) -> Result<String> {
        let name = self
            .strata
            .iter()
            .filter(|(_, s)| s.count > 0)
            .max_by_key(|(name, s)| {
                (
                    s.count + usize::from(name.as_str() == incoming),
                    name.as_str() == incoming,
                    name.as_str(),
                )
            })
            .map(|(n, _)| n.clone())
            .ok_or_else(|| {
                anyhow::anyhow!("aic-learning.replay-eviction: no removable sequence")
            })?;
        let row = self.strata.get_mut(&name).unwrap();
        let prune = |heap: &mut BinaryHeap<Reverse<Key>>| {
            while heap
                .peek()
                .is_some_and(|Reverse((_, _, id, token))| self.present.get(id) != Some(token))
            {
                heap.pop();
            }
        };
        prune(&mut row.mature);
        prune(&mut row.oldest);
        let Reverse((_, _, id, token)) =
            row.mature
                .pop()
                .or_else(|| row.oldest.pop())
                .ok_or_else(|| {
                    anyhow::anyhow!("aic-learning.replay-eviction: empty indexed stratum {name}")
                })?;
        ensure!(
            self.present.remove(&id) == Some(token),
            "aic-learning.replay-eviction: stale identity"
        );
        row.count -= 1;
        Ok(id)
    }
}
