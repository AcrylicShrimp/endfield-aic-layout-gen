//! Shared complete-action domains and retained selection handles.

use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicU64, Ordering},
};

use aic_layout::{
    editor::{CompiledProblemV1, PhysicalDraftV1, occupied_extents},
    geometry::Point,
};
use serde::{Deserialize, Serialize};

use super::{
    Axis, PhysicalEdit, PortDomain, PortSelection, Result, band_allowed,
    canonical_block_port_domains, compiled_error, error,
};

const GUARD: i64 = 1_000_000;
static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PhysicalCandidateOptions {
    pub translation_steps: Vec<i64>,
    pub band_counts: Vec<i64>,
    pub structural_band_mask: bool,
}

impl PhysicalCandidateOptions {
    fn validate(&self) -> Result<()> {
        if self
            .band_counts
            .iter()
            .any(|count| !(1..=GUARD).contains(count))
            || self
                .translation_steps
                .iter()
                .any(|step| !(-GUARD..=GUARD).contains(step))
        {
            return Err(error(
                "candidate-options",
                "actions",
                "translation steps or band counts exceed the action guard",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PhysicalActionDomain {
    pub schema: &'static str,
    pub handle: u64,
    pub revision: u64,
    pub attempts: u64,
    pub port_groups: Vec<Vec<PortDomain>>,
    pub roots: Vec<PhysicalActionRoot>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct PhysicalActionRoot {
    pub invalid: bool,
    pub action: PhysicalRootAction,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhysicalRootAction {
    Place {
        block: usize,
        position: Point,
        rotation: i64,
        port_group: usize,
    },
    InsertBand {
        axis: Axis,
        at: i64,
        count: i64,
    },
    DeleteBand {
        axis: Axis,
        at: i64,
        count: i64,
    },
}

#[derive(Default)]
pub struct PhysicalCandidateAuthority {
    handle: u64,
    revision: u64,
    attempts: u64,
    block_ids: Vec<String>,
    port_groups: Vec<Vec<PortDomain>>,
    roots: Vec<PhysicalActionRoot>,
}

impl PhysicalCandidateAuthority {
    #[cfg(test)]
    pub(crate) fn publish(
        &mut self,
        problem: &CompiledProblemV1,
        revision: u64,
        attempts: u64,
        port_groups: Vec<Vec<PortDomain>>,
        roots: Vec<PhysicalActionRoot>,
    ) -> PhysicalActionDomain {
        self.retain(problem, revision, attempts, port_groups, roots);
        PhysicalActionDomain {
            schema: "physical-action-domain-v1",
            handle: self.handle,
            revision,
            attempts,
            port_groups: self.port_groups.clone(),
            roots: self.roots.clone(),
        }
    }

    pub(crate) fn retain(
        &mut self,
        problem: &CompiledProblemV1,
        revision: u64,
        attempts: u64,
        port_groups: Vec<Vec<PortDomain>>,
        roots: Vec<PhysicalActionRoot>,
    ) -> u64 {
        self.handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
        self.revision = revision;
        self.attempts = attempts;
        self.block_ids = problem
            .blocks
            .iter()
            .map(|block| block.id.clone())
            .collect();
        self.port_groups = port_groups;
        self.roots = roots;
        self.handle
    }

    pub fn enumerate(
        &mut self,
        problem: &CompiledProblemV1,
        draft: &PhysicalDraftV1,
        options: &PhysicalCandidateOptions,
        revision: u64,
        attempts: u64,
    ) -> Result<PhysicalActionDomain> {
        let port_groups = (0..problem.blocks.len())
            .map(|owner| canonical_block_port_domains(problem, owner))
            .collect::<Result<Vec<_>>>()?;
        self.enumerate_with_port_groups(problem, draft, options, revision, attempts, port_groups)?;
        Ok(PhysicalActionDomain {
            schema: "physical-action-domain-v1",
            handle: self.handle,
            revision,
            attempts,
            port_groups: self.port_groups.clone(),
            roots: self.roots.clone(),
        })
    }

    pub(crate) fn enumerate_with_port_groups(
        &mut self,
        problem: &CompiledProblemV1,
        draft: &PhysicalDraftV1,
        options: &PhysicalCandidateOptions,
        revision: u64,
        attempts: u64,
        port_groups: Vec<Vec<PortDomain>>,
    ) -> Result<u64> {
        options.validate()?;
        let (low, high) = occupied_extents(problem, draft).map_err(compiled_error)?;
        let mut roots = Vec::new();
        for (owner, block) in problem.blocks.iter().enumerate() {
            let mut destinations = BTreeSet::from([draft.poses[owner].position]);
            for &step in &options.translation_steps {
                if let Some(x) = draft.poses[owner].position.x.checked_add(step) {
                    destinations.insert(Point {
                        x,
                        y: draft.poses[owner].position.y,
                    });
                }
                if let Some(y) = draft.poses[owner].position.y.checked_add(step) {
                    destinations.insert(Point {
                        x: draft.poses[owner].position.x,
                        y,
                    });
                }
            }
            destinations.extend(draft.poses.iter().map(|pose| pose.position));
            let mut rotations = problem
                .used_catalog
                .facilities
                .iter()
                .find(|definition| definition.id == block.facility)
                .expect("validated compiled facility")
                .allowed_rotations
                .clone();
            rotations.sort_unstable();
            for position in destinations {
                for &rotation in &rotations {
                    roots.push(PhysicalActionRoot {
                        invalid: false,
                        action: PhysicalRootAction::Place {
                            block: owner,
                            position,
                            rotation,
                            port_group: owner,
                        },
                    });
                }
            }
        }
        for delete in [false, true] {
            for axis in [Axis::X, Axis::Y] {
                let (low, high) = match axis {
                    Axis::X => (low.x, high.x),
                    Axis::Y => (low.y, high.y),
                };
                for at in low..=high {
                    for &count in &options.band_counts {
                        let invalid = options.structural_band_mask
                            && !band_allowed(problem, draft, axis, at, count, delete)?;
                        roots.push(PhysicalActionRoot {
                            invalid,
                            action: if delete {
                                PhysicalRootAction::DeleteBand { axis, at, count }
                            } else {
                                PhysicalRootAction::InsertBand { axis, at, count }
                            },
                        });
                    }
                }
            }
        }
        Ok(self.retain(problem, revision, attempts, port_groups, roots))
    }

    pub(crate) fn roots(&self) -> &[PhysicalActionRoot] {
        &self.roots
    }

    #[cfg(test)]
    pub(crate) fn port_groups(&self) -> &[Vec<PortDomain>] {
        &self.port_groups
    }

    pub(crate) fn refresh_context(&mut self, revision: u64, attempts: u64) -> u64 {
        if self.revision != revision || self.attempts != attempts {
            self.handle = NEXT_HANDLE.fetch_add(1, Ordering::Relaxed);
            self.revision = revision;
            self.attempts = attempts;
        }
        self.handle
    }

    pub fn select(
        &self,
        handle: u64,
        revision: u64,
        attempts: u64,
        root: usize,
        ports: &[usize],
    ) -> Result<PhysicalEdit> {
        if handle == 0
            || handle != self.handle
            || revision != self.revision
            || attempts != self.attempts
        {
            return Err(error(
                "candidate-handle",
                "selection",
                "candidate handle is stale, consumed, or foreign",
            ));
        }
        let root = self.roots.get(root).ok_or_else(|| {
            error(
                "candidate-root",
                "selection",
                "root index is outside the retained domain",
            )
        })?;
        if root.invalid {
            return Err(error(
                "candidate-root",
                "selection",
                "selected root is structurally masked",
            ));
        }
        Ok(match &root.action {
            PhysicalRootAction::Place {
                block,
                position,
                rotation,
                port_group,
            } => {
                let block_id = self.block_ids.get(*block).ok_or_else(|| {
                    error(
                        "candidate-root",
                        "selection",
                        "place root references an unknown block",
                    )
                })?;
                let domains = self.port_groups.get(*port_group).ok_or_else(|| {
                    error(
                        "candidate-root",
                        "selection",
                        "place root references an unknown port group",
                    )
                })?;
                if ports.len() != domains.len() {
                    return Err(error(
                        "candidate-ports",
                        "selection",
                        "selection must assign every block terminal",
                    ));
                }
                let mut used = BTreeSet::new();
                let ports = domains
                    .iter()
                    .zip(ports)
                    .map(|(domain, &index)| {
                        let port = domain.ports.get(index).ok_or_else(|| {
                            error(
                                "candidate-ports",
                                domain.node.to_string(),
                                "port index is outside its compatible domain",
                            )
                        })?;
                        if !used.insert(port.as_str()) {
                            return Err(error(
                                "candidate-ports",
                                domain.node.to_string(),
                                "distinct terminals cannot reuse a port",
                            ));
                        }
                        Ok(PortSelection {
                            node: domain.node,
                            port: port.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                PhysicalEdit::Place {
                    block: block_id.clone(),
                    position: *position,
                    rotation: *rotation,
                    ports,
                }
            }
            PhysicalRootAction::InsertBand { axis, at, count } => {
                if !ports.is_empty() {
                    return Err(error(
                        "candidate-ports",
                        "selection",
                        "band actions have no ports",
                    ));
                }
                PhysicalEdit::InsertBand {
                    axis: *axis,
                    at: *at,
                    count: *count,
                }
            }
            PhysicalRootAction::DeleteBand { axis, at, count } => {
                if !ports.is_empty() {
                    return Err(error(
                        "candidate-ports",
                        "selection",
                        "band actions have no ports",
                    ));
                }
                PhysicalEdit::DeleteBand {
                    axis: *axis,
                    at: *at,
                    count: *count,
                }
            }
        })
    }
}
