//! Cold restoration validates state and bound inputs; no legacy adapters.

use super::*;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema: String,
    blueprint_sha256: String,
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
    closed: bool,
    elapsed_ns: u64,
}

impl Session {
    pub fn checkpoint(&self) -> Value {
        json!(Checkpoint {
            schema: "declarative-edit-checkpoint-v1".into(),
            blueprint_sha256: self.blueprint_sha256.clone(),
            catalog_sha256: self.catalog_sha256.clone(),
            input_sha256: identity::digest(
                &json!({"request":self.request,"original":self.original})
            ),
            request: self.request.clone(),
            original: self.original.clone(),
            config: self.config.clone(),
            state: self.state.clone(),
            best_state: self.best_state.clone(),
            attempts: self.attempts,
            revision: self.revision,
            work: self.work.clone(),
            validated: self.validated,
            closed: self.closed,
            elapsed_ns: u64::try_from(self.elapsed().as_nanos()).unwrap_or(u64::MAX),
        })
    }

    fn validate_state(&self, state: &State) -> Result<(Witness, FactoryLayoutMetrics)> {
        self.check_state(state)?;
        if !geometry::conflicts(&self.blueprint_at(state), &state.geometry).is_empty() {
            return Err(error(
                "checkpoint-invalid-witness",
                "Validated state contains overlapping bodies",
                "checkpoint",
            ));
        }
        let witness = self.witness(state)?;
        let validation = validate_factory_layout(&self.request, &witness, &self.catalogs);
        if !validation.factory_validated {
            return Err(error(
                "checkpoint-invalid-witness",
                "Retained witness fails physical validation",
                json!(validation.diagnostics),
            ));
        }
        Ok((witness, validation.metrics.unwrap()))
    }

    pub fn from_checkpoint(input: &str, catalogs: Arc<FactoryEditCatalogs>) -> Result<Self> {
        let saved: Checkpoint =
            crate::input::decode(input).map_err(|e| error("checkpoint-schema", e, "checkpoint"))?;
        if saved.schema != "declarative-edit-checkpoint-v1"
            || saved.attempts > saved.config.max_actions
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
        let mut session = Self::new(saved.request, saved.original, catalogs, saved.config)?;
        if saved.blueprint_sha256 != session.blueprint_sha256
            || saved.catalog_sha256 != session.catalog_sha256
        {
            return Err(error(
                "checkpoint-identity",
                "Original blueprint or runtime catalogs differ from checkpoint",
                "checkpoint",
            ));
        }
        session.check_state(&saved.state)?;
        let (best, metrics) = session.validate_state(&saved.best_state)?;
        if metrics.score > session.initial_metrics.score {
            return Err(error(
                "checkpoint-best",
                "Retained best cannot be worse than the initial factory",
                "checkpoint",
            ));
        }
        if saved.validated {
            session.validate_state(&saved.state)?;
        }
        session.state = saved.state;
        session.best_state = saved.best_state;
        session.best = best;
        session.best_metrics = metrics;
        session.attempts = saved.attempts;
        session.revision = saved.revision;
        session.work = saved.work;
        session.validated = saved.validated;
        session.closed = saved.closed;
        session.elapsed_before = Duration::from_nanos(saved.elapsed_ns);
        session.started = Instant::now();
        Ok(session)
    }
}
