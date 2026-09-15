//! Persistent typed workers for procedural physical-editor sessions.

use super::{
    PhysicalCandidateOptions, PhysicalEditorDiagnostic, PhysicalSessionLimits,
    PhysicalSessionOutcome, PhysicalSessionSnapshot, ProceduralAdmission, ProceduralSession,
    learning::Observation,
};
use crate::worker_pool::FixedWorkerPool;
use aic_layout::validation::Catalogs;
use std::{collections::BTreeMap, sync::Arc};

type Result<T> = std::result::Result<T, String>;

pub enum PhysicalCommand {
    Open {
        admission: ProceduralAdmission,
        limits: PhysicalSessionLimits,
        options: PhysicalCandidateOptions,
    },
    Observe,
    Step {
        source_sha256: String,
        revision: u64,
        handle: u64,
        root: usize,
        ports: Vec<usize>,
    },
    Snapshot,
    Close,
}

pub enum PhysicalReply {
    Ready,
    Observation(Observation),
    Step {
        outcome: PhysicalSessionOutcome,
        next: std::result::Result<Observation, PhysicalEditorDiagnostic>,
    },
    Snapshot(PhysicalSessionSnapshot),
    Closed,
}

struct PhysicalWorker {
    sessions: BTreeMap<usize, (ProceduralSession, PhysicalCandidateOptions)>,
    catalogs: Arc<Catalogs>,
}

pub struct PhysicalPool {
    inner: FixedWorkerPool<PhysicalCommand, PhysicalReply>,
}

impl PhysicalPool {
    pub fn new(catalogs: Arc<Catalogs>, workers: usize, streams: usize) -> Result<Self> {
        let state_catalogs = catalogs.clone();
        let inner = FixedWorkerPool::new(
            workers,
            streams,
            move |_| {
                Ok(PhysicalWorker {
                    sessions: BTreeMap::new(),
                    catalogs: state_catalogs.clone(),
                })
            },
            execute,
            |command| matches!(command, PhysicalCommand::Snapshot),
            "physical-editor.pool",
            "only snapshots may be recovered",
            "aic-physical",
        )?;
        Ok(Self { inner })
    }

    pub fn exchange(
        &mut self,
        commands: Vec<(usize, PhysicalCommand)>,
    ) -> Result<Vec<(usize, PhysicalReply)>> {
        self.inner.exchange(commands)
    }

    pub fn take_failed_replies(&mut self) -> Vec<(usize, PhysicalReply)> {
        self.inner.take_failed_replies()
    }
}

fn diagnostic(failure: PhysicalEditorDiagnostic) -> String {
    serde_json::to_string(&failure).unwrap_or_else(|_| failure.to_string())
}

fn execute(
    worker: &mut PhysicalWorker,
    stream: usize,
    command: PhysicalCommand,
) -> Result<PhysicalReply> {
    match command {
        PhysicalCommand::Open {
            admission,
            limits,
            options,
        } => {
            if worker.sessions.contains_key(&stream) {
                return Err("physical-editor.pool-admission: stream already has a root".into());
            }
            let session =
                ProceduralSession::from_admission(admission, limits).map_err(diagnostic)?;
            worker.sessions.insert(stream, (session, options));
            Ok(PhysicalReply::Ready)
        }
        PhysicalCommand::Close => {
            let (session, _) = worker
                .sessions
                .get(&stream)
                .ok_or("physical-editor.pool-admission: unknown session")?;
            if !session.done() {
                return Err("physical-editor.pool-admission: unfinished root cannot close".into());
            }
            worker.sessions.remove(&stream);
            Ok(PhysicalReply::Closed)
        }
        command => {
            let (session, options) = worker
                .sessions
                .get_mut(&stream)
                .ok_or("physical-editor.pool-admission: unknown session")?;
            match command {
                PhysicalCommand::Observe => session
                    .observe_learning(options)
                    .map(PhysicalReply::Observation)
                    .map_err(diagnostic),
                PhysicalCommand::Step {
                    source_sha256,
                    revision,
                    handle,
                    root,
                    ports,
                } => {
                    let outcome = session.step_candidate(
                        &source_sha256,
                        revision,
                        handle,
                        root,
                        &ports,
                        &worker.catalogs,
                    );
                    let next = session.observe_learning(options);
                    Ok(PhysicalReply::Step { outcome, next })
                }
                PhysicalCommand::Snapshot => Ok(PhysicalReply::Snapshot(session.checkpoint())),
                PhysicalCommand::Open { .. } | PhysicalCommand::Close => unreachable!(),
            }
        }
    }
}
