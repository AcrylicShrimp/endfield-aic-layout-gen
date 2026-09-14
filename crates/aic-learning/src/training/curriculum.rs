//! Complete-wave strict-improvement donors, with exact 80/10/10 quota rotation.
use aic_pilot_native::generation::{geometry_identity, rng::Rng, source::Generated};
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub selection_seed: u64,
    pub percentages: [u8; 3],
    pub donors: String,
}
impl Config {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema == "aic-best-starts-v1"
                && self.percentages == [80, 10, 10]
                && self.donors == "strict-area-improvement",
            "aic-learning.curriculum-config: explicit 80/10/10 strict area improvement required"
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Root {
    pub wave: u64,
    pub area: u64,
    pub request_sha256: String,
    pub geometry_sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Parent {
    pub stream: usize,
    pub wave: u64,
    pub depth: u8,
    pub root: Root,
    pub start_area: u64,
    pub best_area: u64,
    pub complete: bool,
    pub validated: bool,
    pub best: Generated,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Start {
    pub stream: usize,
    pub wave: u64,
    pub depth: u8,
    pub requested_depth: u8,
    pub parent: Option<[u64; 3]>,
    pub root: Option<Root>,
    pub inherited: Option<Generated>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Plan {
    pub wave: u64,
    pub requested: [usize; 3],
    pub actual: [usize; 3],
    pub slots: Vec<Start>,
    pub diagnostics: Vec<Value>,
}
pub fn quotas(count: usize, wave: u64) -> Result<[usize; 3]> {
    ensure!(
        wave > 0,
        "aic-learning.curriculum-wave: positive wave required"
    );
    let q = |w: u64| {
        if w == 0 {
            0
        } else {
            ((count as u128 * w as u128) / 10 - (count as u128 * (w - 1) as u128) / 10) as usize
        }
    };
    let (one, two) = (q(wave), q(wave - 1));
    ensure!(
        one + two <= count,
        "aic-learning.curriculum-quota: inconsistent quota"
    );
    Ok([count - one - two, one, two])
}
fn rng(config: &Config, seed: u64, wave: u64, purpose: &str) -> Rng {
    let hash = Sha256::digest(format!("{}:{seed}:{wave}:{purpose}", config.selection_seed));
    Rng::new(u64::from_le_bytes(hash[..8].try_into().unwrap()))
}
pub fn choose(
    config: &Config,
    seed: u64,
    wave: u64,
    count: usize,
    parents: &[Parent],
    reserved: &BTreeSet<String>,
) -> Result<Plan> {
    config.validate()?;
    let requested = quotas(count, wave)?;
    ensure!(
        parents
            .iter()
            .all(|p| p.wave + 1 == wave && p.depth <= 2 && p.complete && p.validated),
        "aic-learning.curriculum-parent: incomplete, invalid or nonconsecutive parent"
    );
    let mut seen = BTreeSet::new();
    ensure!(
        parents.iter().all(|p| seen.insert(p.stream)),
        "aic-learning.curriculum-parent: duplicate donor stream"
    );
    let fresh = |depth| Start {
        stream: 0,
        wave,
        depth: 0,
        requested_depth: depth,
        parent: None,
        root: None,
        inherited: None,
    };
    let mut plan = Plan {
        wave,
        requested,
        actual: [0; 3],
        slots: vec![],
        diagnostics: vec![],
    };
    plan.slots.extend((0..requested[0]).map(|_| fresh(0)));
    for depth in 1..=2 {
        let mut pool: Vec<_> = parents
            .iter()
            .filter(|p| p.depth as usize + 1 == depth && p.best_area < p.start_area)
            .collect();
        pool.sort_by_key(|p| p.stream);
        rng(config, seed, wave, &format!("donors:{depth}")).shuffle(&mut pool);
        let selected = requested[depth].min(pool.len());
        for p in pool.into_iter().take(selected) {
            let geometry = geometry_identity(&p.best.original);
            if reserved.contains(&geometry) {
                plan.slots.push(fresh(depth as u8));
                plan.diagnostics.push(json!({"code":"aic-learning.curriculum-reserved-geometry","severity":"info","stage":"curriculum",
                    "entity":format!("{seed}/{wave}/{}",p.stream),"message":"Selected inherited geometry is reserved; this slot uses a fresh start"}));
            } else {
                ensure!(
                    p.root.wave + depth as u64 == wave,
                    "aic-learning.curriculum-lineage: donor root depth differs"
                );
                plan.slots.push(Start {
                    stream: 0,
                    wave,
                    depth: depth as u8,
                    requested_depth: depth as u8,
                    parent: Some([p.wave, p.stream as u64, p.depth as u64]),
                    root: Some(p.root.clone()),
                    inherited: Some(p.best.clone()),
                });
            }
        }
        let missing = requested[depth] - selected;
        plan.slots.extend((0..missing).map(|_| fresh(depth as u8)));
        if missing > 0 {
            plan.diagnostics.push(json!({"code":"aic-learning.curriculum-success-shortage","severity":"info","stage":"curriculum",
            "entity":format!("{seed}/{wave}/{depth}"),"message":format!("{missing} slots use fresh starts because strictly improving eligible donors are unavailable")}));
        }
    }
    rng(config, seed, wave, "slots").shuffle(&mut plan.slots);
    for (stream, slot) in plan.slots.iter_mut().enumerate() {
        slot.stream = stream;
        plan.actual[slot.depth as usize] += 1;
    }
    Ok(plan)
}
