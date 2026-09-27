use aic_layout::{geometry::Diagnostic, logistics::TransportKind, validation::Catalogs};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub blocks: [usize; 2],
    pub edges_per_block: [f64; 2],
    pub gap: [i64; 2],
    pub margin: i64,
    pub max_cells: usize,
    pub max_attempts: usize,
    pub max_pair_trials: usize,
    pub ports_per_pair: usize,
    pub route_work_per_call: usize,
    pub route_work_per_attempt: usize,
    pub max_bridges_per_route: usize,
    pub max_degree: usize,
    pub long_link_fraction: f64,
    #[serde(default)]
    pub partition_distance_classes: bool,
    pub min_branch_blocks: usize,
    pub min_merge_blocks: usize,
    pub min_connections: BTreeMap<TransportKind, usize>,
    pub transports: Vec<TransportKind>,
    pub facility_ids: Vec<String>,
    pub acyclic: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: "aic-layout-generator-v1".into(),
            blocks: [12, 20],
            edges_per_block: [1.25, 1.5],
            gap: [1, 5],
            margin: 6,
            max_cells: 65_536,
            max_attempts: 12,
            max_pair_trials: 512,
            ports_per_pair: 8,
            route_work_per_call: 12_000,
            route_work_per_attempt: 600_000,
            max_bridges_per_route: 4,
            max_degree: 5,
            long_link_fraction: 0.4,
            partition_distance_classes: false,
            min_branch_blocks: 1,
            min_merge_blocks: 1,
            min_connections: BTreeMap::new(),
            transports: vec![TransportKind::Belt, TransportKind::Pipe],
            facility_ids: vec![],
            acyclic: false,
        }
    }
}

pub(crate) fn failure(
    code: &str,
    entity: impl Into<serde_json::Value>,
    message: impl Into<String>,
) -> Diagnostic {
    let mut d = Diagnostic::new(format!("aic-layout-gen.{code}"), message, entity);
    d.stage = "procedural-generation";
    d
}

impl Config {
    pub fn validate(&self, catalogs: &Catalogs) -> Result<(), Diagnostic> {
        let range = |a: usize, b: usize, lo, hi| a >= lo && b >= a && b <= hi;
        if self.schema != "aic-layout-generator-v1"
            || !range(self.blocks[0], self.blocks[1], 2, 256)
            || !self
                .edges_per_block
                .iter()
                .all(|v| v.is_finite() && *v > 0.0 && *v <= 8.0)
            || self.edges_per_block[0] > self.edges_per_block[1]
            || self.gap[0] < 1
            || self.gap[1] < self.gap[0]
            || self.gap[1] > 128
            || !(1..=128).contains(&self.margin)
            || !(16..=250_000_000).contains(&self.max_cells)
            || !(1..=64).contains(&self.max_attempts)
            || !(1..=131_072).contains(&self.max_pair_trials)
            || !(1..=64).contains(&self.ports_per_pair)
            || self.route_work_per_call == 0
            || self.route_work_per_attempt < self.route_work_per_call
            || self.route_work_per_attempt > u32::MAX as usize
            || self.max_bridges_per_route > 8
            || !(2..=32).contains(&self.max_degree)
            || !self.long_link_fraction.is_finite()
            || !(0.0..=1.0).contains(&self.long_link_fraction)
            || self.min_branch_blocks > self.blocks[0]
            || self.min_merge_blocks > self.blocks[0]
            || self.transports.is_empty()
            || self.transports.len() > 2
            || self.transports.len() == 2 && self.transports[0] == self.transports[1]
            || self
                .min_connections
                .iter()
                .any(|(t, n)| !self.transports.contains(t) || *n > self.blocks[0] * 8)
            || self
                .facility_ids
                .iter()
                .any(|id| catalogs.facilities.facility(id).is_none())
        {
            return Err(failure(
                "config",
                "config",
                "Invalid schema, ranges, catalog IDs, transport choices or resource limits",
            ));
        }
        if self.edges_per_block[1] * 2.0 > self.max_degree as f64 {
            return Err(failure(
                "config",
                "max_degree",
                "Maximum edge density exceeds the configured degree capacity",
            ));
        }
        for n in self.blocks[0]..=self.blocks[1] {
            let minimum = (n as f64 * self.edges_per_block[0]).ceil() as usize;
            let maximum = (n as f64 * self.edges_per_block[1]).ceil() as usize;
            let pair_limit = n * (n - 1) / if self.acyclic { 2 } else { 1 };
            if minimum < n - 1
                || maximum > pair_limit.min(n * self.max_degree / 2)
                || self.min_connections.values().sum::<usize>() > minimum
                || self.min_branch_blocks * 2 > minimum
                || self.min_merge_blocks * 2 > minimum
            {
                return Err(failure(
                    "config",
                    "graph-targets",
                    "Requested counts exceed the simple-graph/degree capacity or cannot fit the minimum edge target",
                ));
            }
        }
        let pool: Vec<_> = catalogs
            .facilities
            .catalog()
            .facilities
            .iter()
            .filter(|f| {
                (self.facility_ids.is_empty() || self.facility_ids.contains(&f.id))
                    && f.ports
                        .iter()
                        .filter(|p| self.transports.contains(&p.transport))
                        .count()
                        >= 2
            })
            .collect();
        if pool.is_empty()
            || !self.transports.iter().all(|t| {
                [
                    aic_layout::facilities::FacilityPortDirection::Input,
                    aic_layout::facilities::FacilityPortDirection::Output,
                ]
                .iter()
                .all(|d| {
                    pool.iter().any(|f| {
                        f.ports
                            .iter()
                            .any(|p| p.transport == *t && p.direction == *d)
                    })
                })
            })
        {
            return Err(failure(
                "catalog-domain",
                "facilities",
                "Requested transports need input and output ports in the facility pool",
            ));
        }
        Ok(())
    }
}
