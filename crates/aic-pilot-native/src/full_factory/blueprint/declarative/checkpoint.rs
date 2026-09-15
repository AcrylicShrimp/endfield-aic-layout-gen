//! Direct current snapshots and one-time conversion of bound physical v2 snapshots.

use super::*;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<CheckpointSource>,
    blueprint_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    common_problem_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    common_problem: Option<aic_layout::editor::CompiledProblemV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    common_draft: Option<aic_layout::editor::PhysicalDraftV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    common_best_draft: Option<aic_layout::editor::PhysicalDraftV1>,
    catalog_sha256: String,
    input_sha256: String,
    request: FactoryEditRequest,
    original: Witness,
    config: Limits,
    state: State,
    best_state: State,
    attempts: u64,
    revision: u64,
    work: Work,
    validated: bool,
    elapsed_ns: u64,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum CheckpointSource {
    Factory { input_sha256: String },
}

impl Session {
    pub fn checkpoint(&self) -> Value {
        let input_sha256 =
            identity::digest(&json!({"request":self.request,"original":self.original}));
        json!(Checkpoint {
            schema: "declarative-edit-checkpoint-v4".into(),
            source: Some(CheckpointSource::Factory {
                input_sha256: input_sha256.clone()
            }),
            blueprint_sha256: self.blueprint_sha256.clone(),
            common_problem_sha256: Some(self.common_problem_sha256.clone()),
            common_problem: Some(self.common_problem.as_ref().clone()),
            common_draft: Some(self.common_state.clone()),
            common_best_draft: Some(self.common_best_state.clone()),
            catalog_sha256: self.catalog_sha256.clone(),
            input_sha256,
            request: self.request.clone(),
            original: self.original.clone(),
            config: self.config.clone(),
            state: self.state.clone(),
            best_state: self.best_state.clone(),
            attempts: self.attempts,
            revision: self.revision,
            work: self.work.clone(),
            validated: self.validated,
            elapsed_ns: u64::try_from(self.elapsed().as_nanos()).unwrap_or(u64::MAX),
        })
    }

    fn validate_state(&self, state: &State) -> Result<(Witness, FactoryLayoutMetrics)> {
        self.check_state(state)?;
        if crate::physical_editor::conflicts(&self.common_problem, &common::draft(state))
            .map_err(|failure| error("common-state", failure.message, failure.entity))?
            != 0
        {
            return Err(error(
                "checkpoint-invalid-witness",
                "Validated state contains overlapping bodies",
                "checkpoint",
            ));
        }
        let witness = self.witness(state)?;
        let validation = validate_factory_layout(&self.request, &witness, &self.catalogs.data);
        if !validation.factory_validated {
            return Err(error(
                "checkpoint-invalid-witness",
                "Retained witness fails physical validation",
                json!(validation.diagnostics),
            ));
        }
        Ok((witness, validation.metrics.unwrap()))
    }

    pub fn from_checkpoint(input: &str, catalogs: Arc<crate::input::Catalogs>) -> Result<Self> {
        let mut saved: Checkpoint =
            crate::input::decode(input).map_err(|e| error("checkpoint-schema", e, "checkpoint"))?;
        let legacy = saved.schema == "declarative-edit-checkpoint-v2";
        let current = saved.schema == "declarative-edit-checkpoint-v4";
        if !legacy && !current && saved.schema != "declarative-edit-checkpoint-v3" {
            return Err(error(
                "checkpoint-schema",
                "Expected physical checkpoint v2, v3 or v4",
                saved.schema,
            ));
        }
        if saved.attempts > saved.config.max_actions
            || saved.revision > saved.attempts
            || saved.work.route_expansions > saved.config.max_total_route_expansions
            || saved.work.preflight_expansions > saved.work.route_expansions
        {
            return Err(error(
                "checkpoint-counters",
                "Checkpoint schema or resource counters are invalid",
                "checkpoint",
            ));
        }
        if saved.input_sha256
            != identity::digest(&json!({"request":saved.request,"original":saved.original}))
        {
            return Err(error(
                "checkpoint-input",
                "Checkpoint no longer matches its actual request and initial layout",
                "checkpoint",
            ));
        }
        if current {
            match &saved.source {
                Some(CheckpointSource::Factory { input_sha256 })
                    if input_sha256 == &saved.input_sha256 => {}
                _ => {
                    return Err(error(
                        "checkpoint-source",
                        "Physical v4 checkpoint requires its bound factory source owner",
                        "checkpoint",
                    ));
                }
            }
        } else if saved.source.is_some() {
            return Err(error(
                "checkpoint-schema",
                "Physical v2/v3 checkpoint cannot contain a v4 source owner",
                "checkpoint",
            ));
        }
        if legacy {
            if saved.config.schema != "declarative-edit-v1" {
                return Err(error(
                    "checkpoint-schema",
                    "Physical v2 snapshot requires its original limits schema",
                    "checkpoint",
                ));
            }
            // V2 used only bounded nonnegative geometry. Check that before any
            // original-coordinate chain arithmetic, then verify its recorded hash.
            legacy_geometry(
                &saved.original.placements,
                saved
                    .original
                    .transport_networks
                    .iter()
                    .flat_map(|n| {
                        n.cells
                            .iter()
                            .chain(n.segments.iter().flat_map(|s| [&s.from, &s.to]))
                            .chain(n.terminals.iter().map(|t| &t.position))
                    })
                    .map(Point::from)
                    .chain(
                        saved
                            .original
                            .logistics_components
                            .iter()
                            .map(|c| Point::from(&c.position)),
                    ),
            )?;
            saved.config.schema = SCHEMA.into();
        }
        let mut session = Self::new(saved.request, saved.original, catalogs, saved.config)?;
        let legacy_blueprint = if legacy {
            Some(identity::digest(&topology::compile(&session.original)?.0))
        } else {
            None
        };
        if saved.blueprint_sha256
            != legacy_blueprint
                .as_ref()
                .unwrap_or(&session.blueprint_sha256)
                .as_str()
            || saved.catalog_sha256 != session.catalog_sha256
        {
            return Err(error(
                "checkpoint-identity",
                "Original blueprint or runtime catalogs differ from checkpoint",
                "checkpoint",
            ));
        }
        if current {
            let saved_problem = saved.common_problem.as_ref().ok_or_else(|| {
                error(
                    "checkpoint-common-problem",
                    "Physical v4 checkpoint is missing its common problem",
                    "checkpoint",
                )
            })?;
            let saved_problem_sha256 = saved.common_problem_sha256.as_deref().ok_or_else(|| {
                error(
                    "checkpoint-common-problem",
                    "Physical v4 checkpoint is missing its common problem identity",
                    "checkpoint",
                )
            })?;
            if saved_problem != session.common_problem.as_ref()
                || saved_problem_sha256 != session.common_problem_sha256
                || identity::digest(saved_problem) != saved_problem_sha256
            {
                return Err(error(
                    "checkpoint-common-problem",
                    "Common problem differs from the bound source or its recorded identity",
                    "checkpoint",
                ));
            }
            let saved_draft = saved.common_draft.as_ref().ok_or_else(|| {
                error(
                    "checkpoint-common-draft",
                    "Physical v4 checkpoint is missing its common draft",
                    "checkpoint",
                )
            })?;
            if saved_draft != &common::draft(&saved.state) {
                return Err(error(
                    "checkpoint-common-draft",
                    "Common draft differs from the checkpoint editor state",
                    "checkpoint",
                ));
            }
            let saved_best_draft = saved.common_best_draft.as_ref().ok_or_else(|| {
                error(
                    "checkpoint-common-draft",
                    "Physical v4 checkpoint is missing its common best draft",
                    "checkpoint",
                )
            })?;
            if saved_best_draft != &common::draft(&saved.best_state) {
                return Err(error(
                    "checkpoint-common-draft",
                    "Common best draft differs from the checkpoint best editor state",
                    "checkpoint",
                ));
            }
            aic_layout::editor::validate_compiled(saved_problem, saved_draft).map_err(
                |failure| {
                    error(
                        "checkpoint-common-draft",
                        "Common checkpoint state failed validation",
                        failure.to_string(),
                    )
                },
            )?;
            aic_layout::editor::validate_compiled(saved_problem, saved_best_draft).map_err(
                |failure| {
                    error(
                        "checkpoint-common-draft",
                        "Common checkpoint best state failed validation",
                        failure.to_string(),
                    )
                },
            )?;
        } else if saved.common_problem.is_some()
            || saved.common_problem_sha256.is_some()
            || saved.common_draft.is_some()
            || saved.common_best_draft.is_some()
        {
            return Err(error(
                "checkpoint-schema",
                "Physical v2/v3 checkpoint cannot contain v4 common state",
                "checkpoint",
            ));
        }
        let converted_best = if legacy {
            for state in [&saved.state, &saved.best_state] {
                legacy_geometry(
                    &state.geometry.placements,
                    state
                        .anchors
                        .iter()
                        .copied()
                        .chain(state.components.iter().map(Point::from))
                        .chain(state.geometry.paths.iter().flatten().flatten().copied()),
                )?;
            }
            // Validate the actual saved physical states before conversion.
            let (_, metrics) = session.validate_state(&saved.best_state)?;
            if saved.state != saved.best_state {
                if saved.validated {
                    session.validate_state(&saved.state)?;
                } else {
                    session.check_state(&saved.state)?;
                }
            }
            frame::canonicalize(&session.blueprint, &mut saved.state, &session.catalogs.data)?;
            frame::canonicalize(
                &session.blueprint,
                &mut saved.best_state,
                &session.catalogs.data,
            )?;
            Some((session.witness(&saved.best_state)?, metrics))
        } else {
            for state in [&saved.state, &saved.best_state] {
                if frame::from_state(&session.blueprint, state, &session.catalogs.data)?.origin
                    != [0, 0]
                {
                    return Err(error(
                        "checkpoint-frame",
                        "Current physical snapshot must use canonical occupied coordinates",
                        "checkpoint",
                    ));
                }
            }
            None
        };
        // Admission already validated the initial state. Reuse validation when
        // current/best/initial snapshots identify the same state.
        let (best, metrics) = if let Some(converted) = converted_best {
            converted
        } else if saved.best_state == session.state {
            (session.best.clone(), session.best_metrics.clone())
        } else {
            session.validate_state(&saved.best_state)?
        };
        if metrics.score > session.initial_metrics.score {
            return Err(error(
                "checkpoint-best",
                "Retained best cannot be worse than the initial factory",
                "checkpoint",
            ));
        }
        if !legacy && saved.state != saved.best_state && saved.state != session.state {
            if saved.validated {
                session.validate_state(&saved.state)?;
            } else {
                session.check_state(&saved.state)?;
            }
        }
        session.state = saved.state;
        session.best_state = saved.best_state;
        session.common_state = common::draft(&session.state);
        session.common_best_state = common::draft(&session.best_state);
        session.best = best;
        session.best_metrics = metrics;
        session.attempts = saved.attempts;
        session.revision = saved.revision;
        session.work = saved.work;
        session.validated = saved.validated;
        session.elapsed_before = Duration::from_nanos(saved.elapsed_ns);
        session.started = Instant::now();
        Ok(session)
    }
}

fn legacy_geometry(
    placements: &[aic_data::layouts::FacilityPlacement],
    points: impl IntoIterator<Item = Point>,
) -> Result<()> {
    let bounded = |x: i128| (0..=i128::from(GUARD)).contains(&x);
    if placements.iter().any(|p| {
        !bounded(p.x.into())
            || !bounded(p.y.into())
            || !bounded(i128::from(p.x) + i128::from(p.width))
            || !bounded(i128::from(p.y) + i128::from(p.height))
    }) || points
        .into_iter()
        .any(|p| !bounded(p.x.into()) || !bounded(p.y.into()))
    {
        return Err(error(
            "checkpoint-coordinate",
            "Legacy snapshot exceeds its recorded coordinate guard",
            "checkpoint",
        ));
    }
    Ok(())
}
