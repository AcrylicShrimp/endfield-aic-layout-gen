//! Typed Rust entry point over the same session, candidate and evidence authority.
use super::*;

#[derive(Debug, Serialize)]
pub struct PortGroup {
    pub graph_node: usize,
    pub ports: Vec<String>,
    pub features: BTreeMap<i64, (usize, usize)>,
}
#[derive(Debug, Serialize)]
pub struct Domain {
    pub groups: Vec<Vec<PortGroup>>,
}
#[derive(Clone)]
pub struct Observation {
    pub input: Arc<Input>,
    pub domain: Arc<Domain>,
    pub handle: u64,
    pub blueprint_sha256: String,
    pub revision: u64,
    pub attempts: u64,
    pub done: bool,
}
pub struct Runtime {
    session: Session,
    learning: Learning,
    domain: Arc<Domain>,
    chunk: Option<trace::Chunk>,
}
fn diagnostic(d: Diagnostic) -> String {
    serde_json::to_string(&d).unwrap_or(d.message)
}
impl Runtime {
    pub fn replaceable(&self) -> bool {
        self.session.done() && self.chunk.is_none()
    }
    fn from_session(session: Session, options: Options) -> std::result::Result<Self, String> {
        let learning = Learning::new(&session, options).map_err(diagnostic)?;
        let domain = Arc::new(Domain {
            groups: learning
                .facilities
                .iter()
                .map(|f| {
                    f.groups
                        .iter()
                        .map(|g| PortGroup {
                            graph_node: session.blueprint.facilities.len() + g.node,
                            ports: g.ports.clone(),
                            features: g.features.clone(),
                        })
                        .collect()
                })
                .collect(),
        });
        Ok(Self {
            session,
            learning,
            domain,
            chunk: None,
        })
    }
    pub fn open(
        request: FactoryEditRequest,
        original: Witness,
        catalogs: Arc<crate::input::Catalogs>,
        limits: Limits,
        options: Options,
    ) -> std::result::Result<Self, String> {
        Self::from_session(
            Session::new(request, original, catalogs, limits).map_err(diagnostic)?,
            options,
        )
    }
    pub fn restore(
        checkpoint: &str,
        catalogs: Arc<crate::input::Catalogs>,
        options: Options,
    ) -> std::result::Result<Self, String> {
        Self::from_session(
            Session::from_checkpoint(checkpoint, catalogs).map_err(diagnostic)?,
            options,
        )
    }
    pub fn observe(&mut self) -> Observation {
        let input = self.learning.observe(&self.session);
        Observation {
            input: Arc::new(input),
            domain: self.domain.clone(),
            handle: self.learning.handle,
            blueprint_sha256: self.session.blueprint_sha256.clone(),
            revision: self.session.revision,
            attempts: self.session.attempts,
            done: self.session.done(),
        }
    }
    pub fn begin_chunk(&mut self, limit: usize) -> std::result::Result<(), String> {
        if self.chunk.is_some() {
            return Err("declarative.learning-chunk: drain the existing chunk first".into());
        }
        self.chunk = Some(trace::Chunk::new(limit).map_err(diagnostic)?);
        Ok(())
    }
    pub fn step(
        &mut self,
        handle: u64,
        root: usize,
        ports: &[usize],
    ) -> std::result::Result<Value, String> {
        let decision = self
            .learning
            .selected(&self.session, handle, root, ports)
            .map_err(diagnostic)?;
        let action = json!(decision.action);
        let outcome = if let Some(chunk) = &mut self.chunk {
            chunk
                .step(&mut self.session, decision)
                .map_err(diagnostic)?
        } else {
            self.session.step(|| Ok(decision), None)
        };
        Ok(json!({"action": action, "outcome":outcome, "work":self.session.work}))
    }
    pub fn take_chunk(&mut self) -> std::result::Result<Value, String> {
        self.chunk
            .take()
            .ok_or("declarative.learning-chunk: no active chunk")?
            .export(&self.session)
            .map_err(diagnostic)
    }
    /// A cold native snapshot also available after a partial failure. Pending
    /// evidence must be drained separately; never advertise this as a full wave.
    pub fn checkpoint(&self) -> Value {
        self.session.checkpoint()
    }
    pub fn result(&self) -> Value {
        self.session.retained_result()
    }
}
