//! Scale-independent public generator intent and its versioned resolution policy.

use crate::{Failure, Generated, Generator, MechanismConfig};
use aic_layout::{logistics::TransportKind, rng::Rng, validation::Catalogs};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA: &str = "aic-layout-generator-v2";
pub const SCALE_POLICY: &str = "aic-layout-generator-scale-policy-v4";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TopologyProfile {
    Tree,
    LightCycles,
    Branching,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackingProfile {
    Compact,
    Balanced,
    Spacious,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Effort {
    Fast,
    Standard,
    Thorough,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Crossings {
    Allowed,
    Forbidden,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub topology_profiles: Vec<TopologyProfile>,
    pub packing_profiles: Vec<PackingProfile>,
    pub long_connection_preference: [f64; 2],
    pub effort: Effort,
    pub crossings: Crossings,
    pub minimum_connections: BTreeMap<TransportKind, usize>,
    pub transports: Vec<TransportKind>,
    pub facility_ids: Vec<String>,
    pub acyclic: bool,
}

impl Default for Intent {
    fn default() -> Self {
        Self {
            topology_profiles: vec![
                TopologyProfile::Tree,
                TopologyProfile::LightCycles,
                TopologyProfile::Branching,
            ],
            packing_profiles: vec![
                PackingProfile::Compact,
                PackingProfile::Balanced,
                PackingProfile::Spacious,
            ],
            long_connection_preference: [0.1, 0.4],
            effort: Effort::Standard,
            crossings: Crossings::Allowed,
            minimum_connections: BTreeMap::new(),
            transports: vec![TransportKind::Belt, TransportKind::Pipe],
            facility_ids: vec![],
            acyclic: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema: String,
    pub facilities: [usize; 2],
    #[serde(flatten)]
    pub intent: Intent,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema: SCHEMA.into(),
            facilities: [12, 20],
            intent: Intent::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub facilities: usize,
    pub topology: TopologyProfile,
    pub packing: PackingProfile,
    pub long_connection_preference: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResolvedPolicy {
    pub schema: &'static str,
    pub request: Request,
    pub target_connections: usize,
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
    pub partition_distance_classes: bool,
    pub min_branch_blocks: usize,
    pub min_merge_blocks: usize,
}

#[derive(Debug)]
pub struct IntentGenerated {
    pub request_seed: u64,
    pub resolved: ResolvedPolicy,
    pub generated: Generated,
}

#[derive(Debug)]
pub struct IntentFailure {
    pub request_seed: u64,
    pub resolved: ResolvedPolicy,
    pub failure: Failure,
}

pub struct IntentGenerator<'a> {
    catalogs: &'a Catalogs,
    config: Config,
}

impl Config {
    pub fn validate(&self, catalogs: &Catalogs) -> anyhow::Result<()> {
        anyhow::ensure!(self.schema == SCHEMA, "expected generator schema {SCHEMA}");
        anyhow::ensure!(
            self.facilities[0] >= 2
                && self.facilities[0] <= self.facilities[1]
                && self.facilities[1] <= 256,
            "facilities must be an inclusive range within 2..256"
        );
        self.intent.validate()?;
        for facilities in self.facilities[0]..=self.facilities[1] {
            for &topology in &self.intent.topology_profiles {
                for &packing in &self.intent.packing_profiles {
                    let request = Request {
                        facilities,
                        topology,
                        packing,
                        long_connection_preference: self.intent.long_connection_preference[0],
                    };
                    let (_, mechanism) = resolve(&self.intent, request, false)?;
                    mechanism
                        .validate(catalogs)
                        .map_err(|d| anyhow::anyhow!(serde_json::to_string(&d).unwrap()))?;
                }
            }
        }
        Ok(())
    }

    pub fn sample(&self, seed: u64) -> (Request, u64) {
        let mut rng = Rng::new(seed);
        let facilities =
            self.facilities[0] + rng.index(self.facilities[1] - self.facilities[0] + 1);
        let topology =
            self.intent.topology_profiles[rng.index(self.intent.topology_profiles.len())];
        let packing = self.intent.packing_profiles[rng.index(self.intent.packing_profiles.len())];
        let unit = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        let long_connection_preference = self.intent.long_connection_preference[0]
            + unit
                * (self.intent.long_connection_preference[1]
                    - self.intent.long_connection_preference[0]);
        (
            Request {
                facilities,
                topology,
                packing,
                long_connection_preference,
            },
            rng.next_u64(),
        )
    }

    pub fn conservative_workspace_bytes(&self) -> anyhow::Result<u64> {
        self.intent.validate()?;
        let mut maximum = 0u64;
        for &facilities in &[self.facilities[0], self.facilities[1]] {
            for &topology in &self.intent.topology_profiles {
                for &packing in &self.intent.packing_profiles {
                    let request = Request {
                        facilities,
                        topology,
                        packing,
                        long_connection_preference: self.intent.long_connection_preference[1],
                    };
                    let (resolved, _) = resolve(&self.intent, request, false)?;
                    let route = u64::try_from(resolved.route_work_per_call)?
                        .checked_mul(640)
                        .context("route workspace estimate overflow")?;
                    let canvas = u64::try_from(resolved.max_cells)?
                        .checked_mul(256)
                        .context("canvas workspace estimate overflow")?;
                    let estimate = route
                        .checked_add(canvas)
                        .context("workspace estimate overflow")?;
                    maximum = maximum.max(estimate);
                }
            }
        }
        Ok(maximum)
    }

    pub fn resolve_request(
        &self,
        request: Request,
        one_attempt: bool,
    ) -> anyhow::Result<(ResolvedPolicy, MechanismConfig)> {
        resolve(&self.intent, request, one_attempt)
    }
}

pub(crate) fn admit_workspace(
    workers: usize,
    per_worker_bytes: u64,
    budget_gib: u64,
) -> anyhow::Result<u64> {
    let budget_bytes = budget_gib
        .checked_mul(1024 * 1024 * 1024)
        .context("workspace budget GiB overflow")?;
    let estimate = u64::try_from(workers)?
        .checked_mul(per_worker_bytes)
        .context("total workspace estimate overflow")?;
    anyhow::ensure!(
        budget_bytes > 0 && estimate <= budget_bytes,
        "estimated workspace {estimate} bytes for {workers} workers exceeds the configured {budget_gib} GiB budget; adjust workers or --workspace-budget-gib"
    );
    Ok(estimate)
}

impl Intent {
    pub(crate) fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            !self.topology_profiles.is_empty()
                && self.topology_profiles.len() <= 3
                && unique(&self.topology_profiles),
            "topology_profiles must be nonempty and unique"
        );
        anyhow::ensure!(
            !self.packing_profiles.is_empty()
                && self.packing_profiles.len() <= 3
                && unique(&self.packing_profiles),
            "packing_profiles must be nonempty and unique"
        );
        anyhow::ensure!(
            self.long_connection_preference
                .iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                && self.long_connection_preference[0] <= self.long_connection_preference[1],
            "long_connection_preference must be an ordered range within 0..1"
        );
        anyhow::ensure!(
            !self.transports.is_empty() && self.transports.len() <= 2 && unique(&self.transports),
            "transports must be nonempty and unique"
        );
        anyhow::ensure!(
            self.minimum_connections
                .iter()
                .all(|(transport, _)| self.transports.contains(transport)),
            "minimum_connections must refer to enabled transports"
        );
        Ok(())
    }
}

impl<'a> IntentGenerator<'a> {
    pub fn new(catalogs: &'a Catalogs, config: Config) -> anyhow::Result<Self> {
        config.validate(catalogs)?;
        Ok(Self { catalogs, config })
    }

    pub fn generate(&self, request_seed: u64) -> Result<IntentGenerated, IntentFailure> {
        let (request, construction_seed) = self.config.sample(request_seed);
        self.generate_request(request_seed, construction_seed, request, false, || false)
    }

    pub fn generate_once_until(
        &self,
        request_seed: u64,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<IntentGenerated, IntentFailure> {
        let (request, construction_seed) = self.config.sample(request_seed);
        self.generate_request(
            request_seed,
            construction_seed,
            request,
            true,
            &mut cancelled,
        )
    }

    fn generate_request(
        &self,
        request_seed: u64,
        construction_seed: u64,
        request: Request,
        one_attempt: bool,
        cancelled: impl FnMut() -> bool,
    ) -> Result<IntentGenerated, IntentFailure> {
        let (resolved, mechanism) = resolve(&self.config.intent, request, one_attempt)
            .expect("validated semantic generator request");
        let generator = Generator::from_validated(self.catalogs, mechanism);
        match generator.generate_until(construction_seed, cancelled) {
            Ok(generated) => Ok(IntentGenerated {
                request_seed,
                resolved,
                generated,
            }),
            Err(failure) => Err(IntentFailure {
                request_seed,
                resolved,
                failure,
            }),
        }
    }
}

pub fn resolve(
    intent: &Intent,
    request: Request,
    one_attempt: bool,
) -> anyhow::Result<(ResolvedPolicy, MechanismConfig)> {
    intent.validate()?;
    anyhow::ensure!(
        request.facilities >= 2 && request.facilities <= 256,
        "invalid facility count"
    );
    anyhow::ensure!(
        intent.topology_profiles.contains(&request.topology)
            && intent.packing_profiles.contains(&request.packing),
        "request profile is outside the configured intent"
    );
    anyhow::ensure!(
        request.long_connection_preference.is_finite()
            && request.long_connection_preference >= intent.long_connection_preference[0]
            && request.long_connection_preference <= intent.long_connection_preference[1],
        "request long-connection preference is outside the configured range"
    );
    let n = request.facilities;
    let extra = match request.topology {
        TopologyProfile::Tree => 0,
        TopologyProfile::LightCycles => n.div_ceil(4),
        TopologyProfile::Branching => n.div_ceil(2),
    };
    let target_connections = (n - 1)
        .checked_add(extra)
        .ok_or_else(|| anyhow::anyhow!("target connection overflow"))?;
    if intent.acyclic {
        let maximum = n * (n - 1) / 2;
        anyhow::ensure!(
            target_connections <= maximum,
            "acyclic {:?} request with {n} facilities requires {target_connections} connections but permits at most {maximum}",
            request.topology
        );
    }
    let (gap, max_degree, branch, merge) = match request.topology {
        TopologyProfile::Tree => (packing_gap(request.packing), 4, 0, 0),
        TopologyProfile::LightCycles => (packing_gap(request.packing), 5, 0, 0),
        TopologyProfile::Branching => {
            let shaped = if n >= 3 { (n / 10).max(1) } else { 0 };
            (packing_gap(request.packing), 6, shaped, shaped)
        }
    };
    // Routing searches a two-dimensional packing whose useful area grows with
    // the square of its facility count. Packing profiles scale the coefficient
    // because their sampled gaps enlarge that search area.
    let packing_work = match request.packing {
        PackingProfile::Compact => 12usize,
        PackingProfile::Balanced => 20,
        PackingProfile::Spacious => 40,
    };
    let base_call = n
        .checked_mul(n)
        .and_then(|v| v.checked_mul(packing_work))
        .context("route work overflow")?
        .max(12_000);
    let (route_work_per_call, attempts, pair_factor, ports) = match intent.effort {
        Effort::Fast => ((base_call / 2).max(6_000), 1usize, 32usize, 4usize),
        Effort::Standard => (base_call, 4, 192, 8),
        Effort::Thorough => (
            base_call
                .checked_mul(2)
                .context("per-call route work overflow")?,
            8,
            288,
            16,
        ),
    };
    let route_work_per_attempt = route_work_per_call
        .checked_mul(target_connections)
        .and_then(|v| v.checked_mul(2))
        .context("per-attempt route work overflow")?
        .max(route_work_per_call);
    let max_pair_trials = target_connections
        .checked_mul(pair_factor)
        .context("pair-trial overflow")?
        .max(512);
    let margin = 6 + ((n as f64).sqrt().floor() as i64) / 2;
    let max_cells = n
        .checked_mul(n)
        .and_then(|v| v.checked_mul(16))
        .context("canvas guard overflow")?
        .max(65_536);
    let density = (target_connections as f64 - 0.5) / n as f64;
    let max_attempts = if one_attempt { 1 } else { attempts };
    let mechanism = MechanismConfig {
        schema: "aic-layout-generator-v1".into(),
        blocks: [n, n],
        edges_per_block: [density, density],
        gap,
        margin,
        max_cells,
        max_attempts,
        max_pair_trials,
        ports_per_pair: ports,
        route_work_per_call,
        route_work_per_attempt,
        max_bridges_per_route: match intent.crossings {
            Crossings::Allowed => 4,
            Crossings::Forbidden => 0,
        },
        max_degree,
        long_link_fraction: request.long_connection_preference,
        partition_distance_classes: true,
        min_branch_blocks: branch,
        min_merge_blocks: merge,
        min_connections: intent.minimum_connections.clone(),
        transports: intent.transports.clone(),
        facility_ids: intent.facility_ids.clone(),
        acyclic: intent.acyclic,
    };
    Ok((
        ResolvedPolicy {
            schema: SCALE_POLICY,
            request,
            target_connections,
            gap,
            margin,
            max_cells,
            max_attempts,
            max_pair_trials,
            ports_per_pair: ports,
            route_work_per_call,
            route_work_per_attempt,
            max_bridges_per_route: match intent.crossings {
                Crossings::Allowed => 4,
                Crossings::Forbidden => 0,
            },
            max_degree,
            partition_distance_classes: true,
            min_branch_blocks: branch,
            min_merge_blocks: merge,
        },
        mechanism,
    ))
}

fn packing_gap(profile: PackingProfile) -> [i64; 2] {
    match profile {
        PackingProfile::Compact => [1, 1],
        PackingProfile::Balanced => [1, 5],
        PackingProfile::Spacious => [5, 10],
    }
}

fn unique<T: PartialEq>(values: &[T]) -> bool {
    values
        .iter()
        .enumerate()
        .all(|(index, value)| !values[..index].contains(value))
}

trait OptionContext<T> {
    fn context(self, message: &'static str) -> anyhow::Result<T>;
}

impl<T> OptionContext<T> for Option<T> {
    fn context(self, message: &'static str) -> anyhow::Result<T> {
        self.ok_or_else(|| anyhow::anyhow!(message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io;
    use std::path::Path;

    fn request(facilities: usize, topology: TopologyProfile) -> Request {
        Request {
            facilities,
            topology,
            packing: PackingProfile::Balanced,
            long_connection_preference: 0.25,
        }
    }

    #[test]
    fn scale_policy_preserves_meaning_and_grows_derived_work() {
        let intent = Intent::default();
        let (two, two_mechanism) =
            resolve(&intent, request(2, TopologyProfile::Tree), false).unwrap();
        let (twenty, _) = resolve(&intent, request(20, TopologyProfile::Tree), false).unwrap();
        let (hundred, _) = resolve(&intent, request(100, TopologyProfile::Tree), false).unwrap();
        assert_eq!(
            [
                two.target_connections,
                twenty.target_connections,
                hundred.target_connections
            ],
            [1, 19, 99]
        );
        assert!(
            two.route_work_per_attempt < twenty.route_work_per_attempt
                && twenty.route_work_per_attempt < hundred.route_work_per_attempt
        );
        assert!(two.max_cells <= twenty.max_cells && twenty.max_cells <= hundred.max_cells);
        let (light, _) = resolve(&intent, request(20, TopologyProfile::LightCycles), true).unwrap();
        let (branching, _) =
            resolve(&intent, request(20, TopologyProfile::Branching), true).unwrap();
        assert_eq!(light.target_connections, 24);
        assert_eq!(branching.target_connections, 29);
        assert_eq!(light.max_attempts, 1);
        assert_eq!(branching.min_branch_blocks, 2);
        assert_eq!(branching.min_merge_blocks, 2);
        assert!(two.partition_distance_classes);
        assert!(two_mechanism.partition_distance_classes);
        assert!(!MechanismConfig::default().partition_distance_classes);

        let mut compact_request = request(100, TopologyProfile::Tree);
        compact_request.packing = PackingProfile::Compact;
        let mut spacious_request = compact_request.clone();
        spacious_request.packing = PackingProfile::Spacious;
        let (compact, compact_mechanism) = resolve(&intent, compact_request, false).unwrap();
        let (spacious, _) = resolve(&intent, spacious_request, false).unwrap();
        assert_eq!(compact.route_work_per_call, 120_000);
        assert_eq!(spacious.route_work_per_call, 400_000);
        assert_eq!(compact_mechanism.blocks, [100, 100]);
        assert_eq!(compact_mechanism.long_link_fraction, 0.25);
    }

    #[test]
    fn request_sampling_is_deterministic_and_scale_free() {
        let config = Config {
            facilities: [2, 100],
            ..Config::default()
        };
        assert_eq!(config.sample(42), config.sample(42));
        let (request, _) = config.sample(42);
        assert!((2..=100).contains(&request.facilities));
        assert!(config.intent.topology_profiles.contains(&request.topology));
        assert!(config.intent.packing_profiles.contains(&request.packing));
    }

    #[test]
    fn every_declared_scale_resolves_and_thorough_keeps_standard_work() {
        let loaded = io::load_catalogs(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/game/normalized"),
        )
        .unwrap();
        let mut intent = Intent::default();
        for effort in [Effort::Fast, Effort::Standard, Effort::Thorough] {
            intent.effort = effort;
            Config {
                schema: SCHEMA.into(),
                facilities: [2, 256],
                intent: intent.clone(),
            }
            .validate(&loaded.catalogs)
            .unwrap();
        }
        for acyclic in [false, true] {
            intent.acyclic = acyclic;
            for n in 2..=256 {
                for topology in [
                    TopologyProfile::Tree,
                    TopologyProfile::LightCycles,
                    TopologyProfile::Branching,
                ] {
                    for packing in [
                        PackingProfile::Compact,
                        PackingProfile::Balanced,
                        PackingProfile::Spacious,
                    ] {
                        let mut request = request(n, topology);
                        request.packing = packing;
                        intent.effort = Effort::Standard;
                        let standard = resolve(&intent, request.clone(), false);
                        if acyclic
                            && ((n == 2 && topology != TopologyProfile::Tree)
                                || (n == 3 && topology == TopologyProfile::Branching))
                        {
                            assert!(standard.unwrap_err().to_string().contains("acyclic"));
                            continue;
                        }
                        let (standard, mechanism) = standard.unwrap();
                        mechanism.validate(&loaded.catalogs).unwrap();
                        intent.effort = Effort::Thorough;
                        let (thorough, mechanism) = resolve(&intent, request, false).unwrap();
                        mechanism.validate(&loaded.catalogs).unwrap();
                        assert!(thorough.route_work_per_call >= standard.route_work_per_call);
                        assert!(thorough.route_work_per_attempt >= standard.route_work_per_attempt);
                    }
                }
            }
        }
        let upper_request = Request {
            facilities: 256,
            topology: TopologyProfile::Branching,
            packing: PackingProfile::Spacious,
            long_connection_preference: 0.25,
        };
        intent.effort = Effort::Standard;
        let (standard_upper, _) = resolve(&intent, upper_request.clone(), false).unwrap();
        assert_eq!(standard_upper.max_pair_trials, 73_536);
        assert_eq!(standard_upper.route_work_per_attempt, 2_008_023_040);
        intent.effort = Effort::Thorough;
        let (upper, _) = resolve(&intent, upper_request, false).unwrap();
        assert_eq!(upper.max_cells, 1_048_576);
        assert_eq!(upper.route_work_per_call, 5_242_880);
        assert_eq!(upper.max_pair_trials, 110_304);
        assert_eq!(upper.route_work_per_attempt, 4_016_046_080);
    }

    #[test]
    fn upper_scale_workspace_is_admitted_with_an_explicit_resource_budget() {
        let config = Config {
            schema: SCHEMA.into(),
            facilities: [256, 256],
            intent: Intent {
                effort: Effort::Thorough,
                ..Intent::default()
            },
        };
        let estimate = config.conservative_workspace_bytes().unwrap();
        assert!(estimate > 2 * 1024 * 1024 * 1024);
        assert_eq!(admit_workspace(1, estimate, 8).unwrap(), estimate);
        assert!(admit_workspace(1, estimate, 2).is_err());
    }
}
