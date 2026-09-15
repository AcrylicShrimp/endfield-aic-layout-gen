//! Factory adapter over the common compiled physical learning input.

pub(crate) mod runtime;
pub(super) mod trace;

use super::*;

pub use crate::physical_editor::learning::{GLOBAL_FEATURES, Input, schema};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    pub translation_steps: Vec<i64>,
    pub band_counts: Vec<i64>,
    pub masking: Option<String>,
}

pub(super) struct Learning {
    options: crate::physical_editor::PhysicalCandidateOptions,
    shared: crate::physical_editor::learning::Learning,
}

fn diagnostic(failure: crate::physical_editor::PhysicalEditorDiagnostic) -> Diagnostic {
    error("learning-common", failure.message, failure.entity)
}

impl Learning {
    pub(super) fn new(session: &Session, options: Options) -> Result<Self> {
        if options.band_counts.iter().any(|n| !(1..=GUARD).contains(n))
            || options
                .translation_steps
                .iter()
                .any(|n| !(-GUARD..=GUARD).contains(n))
            || !matches!(
                options.masking.as_deref(),
                None | Some("structural-bands-v1")
            )
        {
            return Err(error(
                "learning-config",
                "Invalid declarative candidate configuration",
                "actions",
            ));
        }
        let options = crate::physical_editor::PhysicalCandidateOptions {
            translation_steps: options.translation_steps,
            band_counts: options.band_counts,
            structural_band_mask: options.masking.is_some(),
        };
        let shared = crate::physical_editor::learning::Learning::new(
            session.common_problem.clone(),
            &session.common_state,
        )
        .map_err(diagnostic)?;
        Ok(Self { options, shared })
    }

    fn context<'a>(session: &'a Session) -> crate::physical_editor::learning::Context<'a> {
        crate::physical_editor::learning::Context {
            source_sha256: &session.blueprint_sha256,
            initial_score: session.initial_metrics.score,
            best_score: session.best_metrics.score,
            attempts: session.attempts,
            max_actions: session.config.max_actions,
            route_work: session.work.route_expansions,
            max_total_route_work: session.config.max_total_route_expansions,
            per_action_route_work: session.config.route_expansions,
            search_padding: session.config.search_padding,
            done: session.done(),
        }
    }

    pub(super) fn observe_common(
        &mut self,
        session: &Session,
    ) -> Result<crate::physical_editor::learning::Observation> {
        self.shared
            .observe(
                &session.common_state,
                session.revision,
                Self::context(session),
                &self.options,
            )
            .map_err(diagnostic)
    }

    pub(super) fn observe(&mut self, session: &Session) -> Result<Arc<Input>> {
        Ok(self.observe_common(session)?.input)
    }

    pub(super) fn selected(
        &self,
        session: &Session,
        handle: u64,
        root: usize,
        ports: &[usize],
    ) -> Result<Decision> {
        let selected = self
            .shared
            .select(handle, session.revision, session.attempts, root, ports)
            .map_err(diagnostic)?;
        let action = match selected {
            crate::physical_editor::PhysicalEdit::Place {
                block,
                position,
                rotation,
                ports,
            } => Edit::Place {
                instance: block,
                x: position.x,
                y: position.y,
                rotation,
                ports: ports
                    .into_iter()
                    .map(|choice| PortChoice {
                        node: choice.node,
                        port: choice.port,
                    })
                    .collect(),
            },
            crate::physical_editor::PhysicalEdit::InsertBand { axis, at, count } => {
                Edit::InsertBand {
                    axis: match axis {
                        crate::physical_editor::Axis::X => Axis::X,
                        crate::physical_editor::Axis::Y => Axis::Y,
                    },
                    at,
                    count,
                }
            }
            crate::physical_editor::PhysicalEdit::DeleteBand { axis, at, count } => {
                Edit::DeleteBand {
                    axis: match axis {
                        crate::physical_editor::Axis::X => Axis::X,
                        crate::physical_editor::Axis::Y => Axis::Y,
                    },
                    at,
                    count,
                }
            }
        };
        Ok(Decision {
            schema: DecisionSchema::Local,
            blueprint_sha256: session.blueprint_sha256.clone(),
            revision: session.revision,
            action,
        })
    }

    #[cfg(test)]
    pub(super) fn action_domain(&self) -> crate::physical_editor::PhysicalActionDomain {
        self.shared.action_domain()
    }
}
