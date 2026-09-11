//! Restore a private draft only after rechecking geometry and its immutable base.

use super::*;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Checkpoint {
    schema: String,
    blueprint_sha256: String,
    catalog_sha256: String,
    request: FactoryEditRequest,
    original: Witness,
    config: Config,
    draft: Draft,
    best_draft: Draft,
    attempts: u64,
    revision: u64,
    closed: bool,
    draft_validated: bool,
    elapsed_ns: u64,
}

impl Session {
    pub fn checkpoint(&self) -> Value {
        json!(Checkpoint {
            schema: "blueprint-edit-checkpoint-v1".into(),
            blueprint_sha256: self.blueprint_sha256.clone(),
            catalog_sha256: self.catalog_sha256.clone(),
            request: self.request.clone(),
            original: self.original.clone(),
            config: self.config.clone(),
            draft: self.draft.clone(),
            best_draft: self.best_draft.clone(),
            attempts: self.attempts,
            revision: self.revision,
            closed: self.closed,
            draft_validated: self.draft_validated,
            elapsed_ns: u64::try_from(self.elapsed().as_nanos()).unwrap_or(u64::MAX)
        })
    }

    fn check_draft(&self, draft: &Draft) -> Result<()> {
        if draft.placements.len() != self.blueprint.facilities.len()
            || draft.paths.len() != self.blueprint.edges.len()
        {
            return Err(fail(
                "checkpoint-shape",
                "Draft must retain every original facility and edge slot",
                "checkpoint",
            ));
        }
        let mut canonical = draft.clone();
        canonical.paths.fill(None);
        for (p, original) in draft.placements.iter().zip(&self.blueprint.facilities) {
            if p.instance != original.instance
                || p.facility != original.facility
                || p.recipe != original.recipe
            {
                return Err(fail(
                    "checkpoint-facility",
                    "Draft changes original facility identity, definition or recipe",
                    p.instance.clone(),
                ));
            }
            geometry::pose(
                &self.blueprint,
                &mut canonical,
                &self.catalogs,
                &p.instance,
                p.x,
                p.y,
                p.rotation,
            )?;
        }
        if json!(canonical.placements) != json!(draft.placements) {
            return Err(fail(
                "checkpoint-pose",
                "Draft dimensions disagree with catalog-derived poses",
                "checkpoint",
            ));
        }
        for edge in &self.blueprint.edges {
            if let Some(path) = &draft.paths[edge.id] {
                geometry::check_path(&self.blueprint, draft, &self.catalogs, edge, path)?;
            }
        }
        Ok(())
    }

    fn validate_draft(&self, draft: &Draft) -> Result<(Witness, FactoryLayoutMetrics)> {
        if !geometry::conflicts(&self.blueprint, draft).is_empty() {
            return Err(fail(
                "checkpoint-invalid-witness",
                "Validated checkpoint state contains overlap",
                "checkpoint",
            ));
        }
        let witness =
            topology::rebuild(&self.original, &self.blueprint, draft, Some(&self.catalogs))?;
        let validation = validate_factory_layout(&self.request, &witness, &self.catalogs);
        if !validation.factory_validated {
            return Err(fail(
                "checkpoint-invalid-witness",
                "Checkpoint witness fails full physical validation",
                json!(validation.diagnostics),
            ));
        }
        Ok((witness, validation.metrics.unwrap()))
    }

    pub fn from_checkpoint(input: &str, catalogs: Arc<FactoryEditCatalogs>) -> Result<Self> {
        let saved: Checkpoint = crate::python_factory::decode(input)
            .map_err(|e| fail("checkpoint-schema", e, "checkpoint"))?;
        if saved.schema != "blueprint-edit-checkpoint-v1"
            || saved.attempts > saved.config.max_actions
            || saved.revision > saved.attempts
        {
            return Err(fail(
                "checkpoint-counters",
                "Unsupported checkpoint schema or invalid counters",
                "checkpoint",
            ));
        }
        let mut session = Self::new(saved.request, saved.original, catalogs, saved.config)?;
        if saved.blueprint_sha256 != session.blueprint_sha256
            || saved.catalog_sha256 != session.catalog_sha256
        {
            return Err(fail(
                "checkpoint-identity",
                "Original blueprint or runtime catalog differs from checkpoint",
                "checkpoint",
            ));
        }
        session.check_draft(&saved.draft)?;
        session.check_draft(&saved.best_draft)?;
        let (best, metrics) = session.validate_draft(&saved.best_draft)?;
        if metrics.score > session.initial_metrics.score {
            return Err(fail(
                "checkpoint-best",
                "Retained best is worse than the validated initial factory",
                "checkpoint",
            ));
        }
        if saved.draft_validated {
            session.validate_draft(&saved.draft)?;
        }
        session.draft = saved.draft;
        session.best_draft = saved.best_draft;
        session.best = best;
        session.best_metrics = metrics;
        session.attempts = saved.attempts;
        session.revision = saved.revision;
        session.closed = saved.closed;
        session.draft_validated = saved.draft_validated;
        session.elapsed_before = Duration::from_nanos(saved.elapsed_ns);
        session.started = Instant::now();
        Ok(session)
    }
}
