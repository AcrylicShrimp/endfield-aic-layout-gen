use std::collections::BTreeSet;
use std::sync::{Arc as SyncArc, Mutex};

use pumpkin_solver::core::predicates::{Predicate, PredicateConstructor};
use pumpkin_solver::core::variables::DomainId;
use serde::Serialize;

use super::{
    FactoredEndpointKind, ModelInput, ModelInstance, RecordedModel, SharedLayer, SharedTerminal,
    SharedTerminalEndpoint, UsedBoundsVariables, direction_between, direction_index,
};
use crate::layouts::integrated::IntegratedLayoutDiagnostic;
use crate::layouts::integrated::exact::recorder::{ConstraintFamily, VariableFamily};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(in crate::layouts::integrated) enum NativePredicateRelation {
    Equal,
    NotEqual,
    GreaterThanOrEqual,
    LessThanOrEqual,
}

impl NativePredicateRelation {
    fn predicate(self, domain: DomainId, value: i32) -> Predicate {
        match self {
            Self::Equal => domain.equality_predicate(value),
            Self::NotEqual => domain.disequality_predicate(value),
            Self::GreaterThanOrEqual => domain.lower_bound_predicate(value),
            Self::LessThanOrEqual => domain.upper_bound_predicate(value),
        }
    }

    fn complement(self, value: i32) -> Result<(NativePredicateRelation, i32), GuardedCoreError> {
        match self {
            Self::Equal => Ok((Self::NotEqual, value)),
            Self::NotEqual => Ok((Self::Equal, value)),
            Self::GreaterThanOrEqual => value
                .checked_sub(1)
                .map(|complement| (Self::LessThanOrEqual, complement))
                .ok_or_else(|| GuardedCoreError::new("lower-bound complement underflows i32")),
            Self::LessThanOrEqual => value
                .checked_add(1)
                .map(|complement| (Self::GreaterThanOrEqual, complement))
                .ok_or_else(|| GuardedCoreError::new("upper-bound complement overflows i32")),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(in crate::layouts::integrated) enum GuardedCoreAtom {
    UsedWidth {
        value: i32,
    },
    UsedHeight {
        value: i32,
    },
    Placement {
        instance: String,
        x: i32,
        y: i32,
        rotation: i64,
    },
    FacilityPort {
        terminal: String,
        port: String,
    },
    ExternalBoundaryKey {
        terminal: String,
        key: i32,
    },
    MaterialArcSelected {
        network: String,
        from: usize,
        to: usize,
    },
    MaterialArcItem {
        network: String,
        from: usize,
        to: usize,
        item: String,
    },
    MaterialArcFlowAtLeast {
        network: String,
        from: usize,
        to: usize,
        minimum: i32,
    },
    MaterialArcFlowEquals {
        network: String,
        from: usize,
        to: usize,
        value: i32,
    },
}

impl GuardedCoreAtom {
    pub(in crate::layouts::integrated) fn stable_id(&self) -> String {
        match self {
            Self::UsedWidth { value } => format!("used-width={value}"),
            Self::UsedHeight { value } => format!("used-height={value}"),
            Self::Placement {
                instance,
                x,
                y,
                rotation,
            } => format!("placement:{instance}@{x},{y},r{rotation}"),
            Self::FacilityPort { terminal, port } => {
                format!("facility-port:{terminal}={port}")
            }
            Self::ExternalBoundaryKey { terminal, key } => {
                format!("external-boundary-key:{terminal}={key}")
            }
            Self::MaterialArcSelected { network, from, to } => {
                format!("material-arc-selected:{network}:{from}->{to}")
            }
            Self::MaterialArcItem {
                network,
                from,
                to,
                item,
            } => format!("material-arc-item:{network}:{from}->{to}={item}"),
            Self::MaterialArcFlowAtLeast {
                network,
                from,
                to,
                minimum,
            } => format!("material-arc-flow:{network}:{from}->{to}>={minimum}"),
            Self::MaterialArcFlowEquals {
                network,
                from,
                to,
                value,
            } => format!("material-arc-flow:{network}:{from}->{to}={value}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct GuardedCoreAtomCertificate {
    pub atom_index: usize,
    pub stable_id: String,
    pub domain_id: u32,
    pub variable_family: String,
    pub variable_name: String,
    pub declared_lower_bound: i32,
    pub declared_upper_bound: i32,
    pub declared_cardinality: u64,
    pub relation: NativePredicateRelation,
    pub value: i32,
    pub complement_relation: NativePredicateRelation,
    pub complement_value: i32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct GuardedCoreClauseCertificate {
    pub variable_count_delta: usize,
    pub clause_count_delta: usize,
    pub clause_arity: usize,
    pub atoms: Vec<GuardedCoreAtomCertificate>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(in crate::layouts::integrated) enum GuardedCorePosting {
    Assumptions,
    #[allow(
        dead_code,
        reason = "wired by the guarded replay slice after the initial proof gate"
    )]
    ReplayClause,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(in crate::layouts::integrated) struct GuardedCoreBuildCertificate {
    pub posting: GuardedCorePosting,
    pub atoms: Vec<GuardedCoreAtomCertificate>,
    pub clause: Option<GuardedCoreClauseCertificate>,
}

pub(super) type GuardedCoreBuildCertificateCollector =
    SyncArc<Mutex<Vec<GuardedCoreBuildCertificate>>>;

#[derive(Clone)]
pub(super) struct GuardedCoreRequest {
    pub atoms: Vec<GuardedCoreAtom>,
    pub posting: GuardedCorePosting,
    pub certificates: GuardedCoreBuildCertificateCollector,
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedGuardedCoreAtom {
    pub domain: DomainId,
    pub relation: NativePredicateRelation,
    pub value: i32,
    pub certificate: GuardedCoreAtomCertificate,
}

impl ResolvedGuardedCoreAtom {
    fn predicate(&self) -> Predicate {
        self.relation.predicate(self.domain, self.value)
    }

    fn complement_predicate(&self) -> Predicate {
        self.certificate
            .complement_relation
            .predicate(self.domain, self.certificate.complement_value)
    }
}

#[derive(Debug)]
struct GuardedCoreError {
    message: String,
}

impl GuardedCoreError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn diagnostic(self, stable_id: Option<String>) -> IntegratedLayoutDiagnostic {
        IntegratedLayoutDiagnostic::error(
            "guarded-core-atom-invalid",
            "/guarded_core/atoms",
            stable_id,
            self.message,
        )
    }
}

pub(super) struct GuardedCoreResolutionContext<'a> {
    pub input: &'a ModelInput,
    pub used_bounds: UsedBoundsVariables,
    pub instances: &'a [ModelInstance],
    pub terminals: &'a [Vec<SharedTerminal>],
    pub layers: &'a [SharedLayer],
}

pub(super) fn resolve_atoms(
    solver: &RecordedModel,
    context: GuardedCoreResolutionContext<'_>,
    atoms: &[GuardedCoreAtom],
) -> Result<Vec<ResolvedGuardedCoreAtom>, IntegratedLayoutDiagnostic> {
    let mut stable_ids = BTreeSet::new();
    atoms
        .iter()
        .enumerate()
        .map(|(atom_index, atom)| {
            let stable_id = atom.stable_id();
            if !stable_ids.insert(stable_id.clone()) {
                return Err(
                    GuardedCoreError::new("duplicate semantic atom").diagnostic(Some(stable_id))
                );
            }
            resolve_atom(solver, &context, atom_index, atom.clone())
                .map_err(|error| error.diagnostic(Some(stable_id)))
        })
        .collect()
}

fn resolve_atom(
    solver: &RecordedModel,
    context: &GuardedCoreResolutionContext<'_>,
    atom_index: usize,
    atom: GuardedCoreAtom,
) -> Result<ResolvedGuardedCoreAtom, GuardedCoreError> {
    let (domain, expected_family, relation, value) = match &atom {
        GuardedCoreAtom::UsedWidth { value } => (
            context.used_bounds.width,
            VariableFamily::Objective,
            NativePredicateRelation::Equal,
            *value,
        ),
        GuardedCoreAtom::UsedHeight { value } => (
            context.used_bounds.height,
            VariableFamily::Objective,
            NativePredicateRelation::Equal,
            *value,
        ),
        GuardedCoreAtom::Placement {
            instance,
            x,
            y,
            rotation,
        } => {
            let model_instance = context
                .instances
                .iter()
                .find(|candidate| candidate.input.id == *instance)
                .ok_or_else(|| GuardedCoreError::new("placement instance is not modeled"))?;
            let candidates = model_instance
                .candidates
                .iter()
                .filter(|candidate| {
                    candidate.x == *x && candidate.y == *y && candidate.rotation == *rotation
                })
                .collect::<Vec<_>>();
            if candidates.len() != 1 {
                return Err(GuardedCoreError::new(format!(
                    "placement atom resolved to {} candidates instead of one",
                    candidates.len()
                )));
            }
            (
                candidates[0].selected,
                VariableFamily::Placement,
                NativePredicateRelation::Equal,
                1,
            )
        }
        GuardedCoreAtom::FacilityPort { terminal, port } => {
            let terminal = unique_terminal(context.terminals, terminal)?;
            let SharedTerminalEndpoint::Factored {
                kind:
                    FactoredEndpointKind::Facility {
                        port_choice,
                        port_ids,
                        ..
                    },
                ..
            } = &terminal.endpoint
            else {
                return Err(GuardedCoreError::new(
                    "facility-port atom resolved to a non-facility endpoint",
                ));
            };
            let port_index = port_ids
                .iter()
                .position(|candidate| candidate == port)
                .ok_or_else(|| GuardedCoreError::new("facility port is not in the port domain"))?;
            (
                *port_choice,
                VariableFamily::Endpoint,
                NativePredicateRelation::Equal,
                i32::try_from(port_index)
                    .map_err(|_| GuardedCoreError::new("facility port index exceeds i32"))?,
            )
        }
        GuardedCoreAtom::ExternalBoundaryKey { terminal, key } => {
            let terminal = unique_terminal(context.terminals, terminal)?;
            let SharedTerminalEndpoint::Factored {
                key: domain,
                kind: FactoredEndpointKind::External { .. },
            } = &terminal.endpoint
            else {
                return Err(GuardedCoreError::new(
                    "boundary-key atom resolved to a non-external endpoint",
                ));
            };
            let reachable = terminal.routing_options.iter().any(|option| {
                let option_key = i32::try_from(option.cell)
                    .ok()
                    .and_then(|cell| cell.checked_mul(4))
                    .and_then(|cell| {
                        i32::try_from(direction_index(option.arm_direction))
                            .ok()
                            .and_then(|direction| cell.checked_add(direction))
                    });
                option_key == Some(*key)
            });
            if !reachable {
                return Err(GuardedCoreError::new(
                    "boundary key has no materialized routing option",
                ));
            }
            (
                *domain,
                VariableFamily::BoundaryTerminal,
                NativePredicateRelation::Equal,
                *key,
            )
        }
        GuardedCoreAtom::MaterialArcSelected { network, from, to } => {
            let (_, layer) = resolve_layer(context, network)?;
            let arc = resolve_arc(layer, *from, *to)?;
            (
                arc.selected,
                VariableFamily::RouteArc,
                NativePredicateRelation::Equal,
                1,
            )
        }
        GuardedCoreAtom::MaterialArcItem {
            network,
            from,
            to,
            item,
        } => {
            let (network_index, layer) = resolve_layer(context, network)?;
            if context.input.networks[network_index].item() != item {
                return Err(GuardedCoreError::new(
                    "material-arc item does not match the selected network item",
                ));
            }
            resolve_arc(layer, *from, *to)?;
            let from_direction =
                direction_index(direction_between(*from, *to, context.input.width));
            (
                layer.arm_items[*from][from_direction],
                VariableFamily::ArmItem,
                NativePredicateRelation::Equal,
                *layer.item_codes.get(&network_index).ok_or_else(|| {
                    GuardedCoreError::new("selected network has no layer-local item code")
                })?,
            )
        }
        GuardedCoreAtom::MaterialArcFlowAtLeast {
            network,
            from,
            to,
            minimum,
        } => {
            let (_, layer) = resolve_layer(context, network)?;
            let arc = resolve_arc(layer, *from, *to)?;
            (
                arc.flow,
                VariableFamily::Flow,
                NativePredicateRelation::GreaterThanOrEqual,
                *minimum,
            )
        }
        GuardedCoreAtom::MaterialArcFlowEquals {
            network,
            from,
            to,
            value,
        } => {
            let (_, layer) = resolve_layer(context, network)?;
            let arc = resolve_arc(layer, *from, *to)?;
            (
                arc.flow,
                VariableFamily::Flow,
                NativePredicateRelation::Equal,
                *value,
            )
        }
    };

    let descriptor = solver.variable_descriptor(domain);
    if descriptor.family != expected_family {
        return Err(GuardedCoreError::new(format!(
            "resolved variable family is {}, expected {}",
            descriptor.family.name(),
            expected_family.name()
        )));
    }
    if !solver.contains(&domain, value) {
        return Err(GuardedCoreError::new(format!(
            "predicate value {value} is absent from the declared domain"
        )));
    }
    let (complement_relation, complement_value) = relation.complement(value)?;
    let stable_id = atom.stable_id();
    Ok(ResolvedGuardedCoreAtom {
        domain,
        relation,
        value,
        certificate: GuardedCoreAtomCertificate {
            atom_index,
            stable_id,
            domain_id: domain.id(),
            variable_family: descriptor.family.name().to_string(),
            variable_name: descriptor.name,
            declared_lower_bound: descriptor.declared_lower_bound,
            declared_upper_bound: descriptor.declared_upper_bound,
            declared_cardinality: descriptor.declared_cardinality,
            relation,
            value,
            complement_relation,
            complement_value,
        },
    })
}

fn unique_terminal<'a>(
    terminals: &'a [Vec<SharedTerminal>],
    terminal_id: &str,
) -> Result<&'a SharedTerminal, GuardedCoreError> {
    let matches = terminals
        .iter()
        .flatten()
        .filter(|terminal| terminal.id == terminal_id)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(GuardedCoreError::new(format!(
            "terminal resolved {} times instead of once",
            matches.len()
        )));
    }
    Ok(matches[0])
}

fn resolve_layer<'a>(
    context: &'a GuardedCoreResolutionContext<'_>,
    network_id: &str,
) -> Result<(usize, &'a SharedLayer), GuardedCoreError> {
    let network_index = context
        .input
        .networks
        .iter()
        .position(|network| network.id() == network_id)
        .ok_or_else(|| GuardedCoreError::new("material network is not modeled"))?;
    let layer = context
        .layers
        .iter()
        .find(|layer| layer.network_indices.contains(&network_index))
        .ok_or_else(|| GuardedCoreError::new("material network has no shared layer"))?;
    Ok((network_index, layer))
}

fn resolve_arc(
    layer: &SharedLayer,
    from: usize,
    to: usize,
) -> Result<&crate::layouts::integrated::exact::Arc, GuardedCoreError> {
    let matches = layer
        .arcs
        .iter()
        .filter(|arc| arc.from == from && arc.to == to)
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(GuardedCoreError::new(format!(
            "directed material arc resolved {} times instead of once",
            matches.len()
        )));
    }
    Ok(matches[0])
}

pub(super) fn post_assumptions(
    solver: &mut RecordedModel,
    atoms: &[ResolvedGuardedCoreAtom],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) {
    for atom in atoms {
        solver.post_predicate_clause(
            ConstraintFamily::GuardedCore,
            &[atom.domain],
            vec![atom.predicate()],
            tag,
        );
    }
}

pub(super) fn post_guarded_clause(
    solver: &mut RecordedModel,
    atoms: &[ResolvedGuardedCoreAtom],
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<GuardedCoreClauseCertificate, IntegratedLayoutDiagnostic> {
    if atoms.is_empty() {
        return Err(IntegratedLayoutDiagnostic::error(
            "guarded-core-empty-replay",
            "/guarded_core/atoms",
            None,
            "an empty infeasible core proves the base model infeasible and cannot be replayed as a useful guarded clause",
        ));
    }
    solver.post_predicate_clause(
        ConstraintFamily::GuardedCore,
        &atoms.iter().map(|atom| atom.domain).collect::<Vec<_>>(),
        atoms
            .iter()
            .map(ResolvedGuardedCoreAtom::complement_predicate)
            .collect(),
        tag,
    );
    Ok(GuardedCoreClauseCertificate {
        variable_count_delta: 0,
        clause_count_delta: 1,
        clause_arity: atoms.len(),
        atoms: atoms.iter().map(|atom| atom.certificate.clone()).collect(),
    })
}

pub(super) fn post_request(
    solver: &mut RecordedModel,
    context: GuardedCoreResolutionContext<'_>,
    request: &GuardedCoreRequest,
    tag: pumpkin_solver::core::proof::ConstraintTag,
) -> Result<Vec<ResolvedGuardedCoreAtom>, IntegratedLayoutDiagnostic> {
    let atoms = resolve_atoms(solver, context, &request.atoms)?;
    let clause = match request.posting {
        GuardedCorePosting::Assumptions => {
            post_assumptions(solver, &atoms, tag);
            None
        }
        GuardedCorePosting::ReplayClause => Some(post_guarded_clause(solver, &atoms, tag)?),
    };
    request
        .certificates
        .lock()
        .expect("guarded-core certificate collector is not poisoned")
        .push(GuardedCoreBuildCertificate {
            posting: request.posting,
            atoms: atoms.iter().map(|atom| atom.certificate.clone()).collect(),
            clause,
        });
    Ok(atoms)
}

#[cfg(test)]
mod tests {
    use pumpkin_solver::core::predicates::PredicateConstructor;
    use pumpkin_solver::core::variables::DomainId;

    use super::NativePredicateRelation;

    #[test]
    fn exact_predicate_complements_match_pumpkin() {
        let domain = DomainId::new(7);
        for (relation, value) in [
            (NativePredicateRelation::Equal, 4),
            (NativePredicateRelation::NotEqual, 4),
            (NativePredicateRelation::GreaterThanOrEqual, 1),
            (NativePredicateRelation::LessThanOrEqual, 8),
        ] {
            let predicate = relation.predicate(domain, value);
            let (complement_relation, complement_value) = relation
                .complement(value)
                .expect("fixture relation has a representable complement");
            assert_eq!(
                !predicate,
                complement_relation.predicate(domain, complement_value)
            );
        }
        assert_eq!(
            !domain.lower_bound_predicate(1),
            domain.upper_bound_predicate(0)
        );
    }

    #[test]
    fn predicate_complement_overflow_is_fail_closed() {
        assert!(
            NativePredicateRelation::GreaterThanOrEqual
                .complement(i32::MIN)
                .is_err()
        );
        assert!(
            NativePredicateRelation::LessThanOrEqual
                .complement(i32::MAX)
                .is_err()
        );
    }
}
