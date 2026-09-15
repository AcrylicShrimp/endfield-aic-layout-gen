//! Fixed native workers with bounded queues; one ordered batch per worker/round.
//! Workers retain logical sessions. Observations move by Arc, with no serializer.
use super::{Limits, Observation, Options, Runtime};
use crate::worker_pool::FixedWorkerPool;
use aic_data::layouts::{FactoryEditRequest, WitnessProbeInput};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};

type Result<T> = std::result::Result<T, String>;
pub enum Command {
    /// Install a fresh/inherited start after the previous episode was drained.
    Next {
        request: FactoryEditRequest,
        original: WitnessProbeInput,
        limits: Limits,
        options: Options,
    },
    /// Construct/randomize a canonical fresh slot on this persistent worker.
    Generate {
        source: Arc<crate::generation::source::Source>,
        slot: u64,
        directory: std::path::PathBuf,
        limits: Limits,
        options: Options,
    },
    Open {
        request: FactoryEditRequest,
        original: WitnessProbeInput,
        limits: Limits,
        options: Options,
    },
    Restore {
        checkpoint: String,
        options: Options,
    },
    Begin {
        actions: usize,
    },
    /// Observe without collecting training evidence (held-out inference).
    Observe,
    Step {
        handle: u64,
        root: usize,
        ports: Vec<usize>,
    },
    Drain,
    Snapshot,
    Result,
}
pub enum Reply {
    Ready,
    Observation(Observation),
    Step { result: Value, next: Observation },
    Data(Value),
}
type Replies = Vec<(usize, Reply)>;
pub struct Pool {
    inner: FixedWorkerPool<Command, Reply>,
}
impl Pool {
    pub fn new(
        catalogs: Arc<crate::input::Catalogs>,
        workers: usize,
        streams: usize,
    ) -> Result<Self> {
        let state_catalogs = catalogs.clone();
        let inner = FixedWorkerPool::new(
            workers,
            streams,
            move |_| Ok((BTreeMap::new(), state_catalogs.clone())),
            |state, id, command| execute(&mut state.0, id, command, &state.1),
            |command| matches!(command, Command::Drain | Command::Snapshot),
            "declarative.pool",
            "only failure evidence and snapshots may be recovered",
            "aic-native",
        )?;
        Ok(Self { inner })
    }
    pub fn exchange(&mut self, commands: Vec<(usize, Command)>) -> Result<Replies> {
        self.inner.exchange(commands)
    }
    /// Available replies from an incomplete exchange, including workers that
    /// succeeded before another failed. Consume only for cold failure evidence.
    pub fn take_failed_replies(&mut self) -> Replies {
        self.inner.take_failed_replies()
    }
}
fn execute(
    sessions: &mut BTreeMap<usize, Runtime>,
    id: usize,
    command: Command,
    catalogs: &Arc<crate::input::Catalogs>,
) -> Result<Reply> {
    match command {
        Command::Next {
            request,
            original,
            limits,
            options,
        } => {
            if sessions.get(&id).is_some_and(|s| !s.replaceable()) {
                return Err(
                    "declarative.pool-admission: unfinished episode cannot be replaced".into(),
                );
            }
            let session = Runtime::open(request, original, catalogs.clone(), limits, options)?;
            sessions.insert(id, session);
            Ok(Reply::Ready)
        }
        Command::Generate {
            source,
            slot,
            directory,
            limits,
            options,
        } => {
            if sessions.get(&id).is_some_and(|s| !s.replaceable()) {
                return Err(
                    "declarative.pool-admission: generation would replace unfinished episode"
                        .into(),
                );
            }
            let generated = source.generate(slot, &directory)?;
            let session = Runtime::open(
                generated.request.clone(),
                generated.original.clone(),
                catalogs.clone(),
                limits,
                options,
            )?;
            sessions.insert(id, session);
            Ok(Reply::Data(
                serde_json::to_value(generated).map_err(|e| e.to_string())?,
            ))
        }
        Command::Open {
            request,
            original,
            limits,
            options,
        } => {
            if sessions.contains_key(&id) {
                return Err("declarative.pool-admission: stream already has an episode".into());
            }
            let session = Runtime::open(request, original, catalogs.clone(), limits, options)?;
            sessions.insert(id, session);
            Ok(Reply::Ready)
        }
        Command::Restore {
            checkpoint,
            options,
        } => {
            if sessions.contains_key(&id) {
                return Err(
                    "declarative.pool-admission: restore would replace live episode".into(),
                );
            }
            let session = Runtime::restore(&checkpoint, catalogs.clone(), options)?;
            sessions.insert(id, session);
            Ok(Reply::Ready)
        }
        command => {
            let session = sessions
                .get_mut(&id)
                .ok_or("declarative.pool-admission: unknown session")?;
            match command {
                Command::Observe => Ok(Reply::Observation(session.observe()?)),
                Command::Begin { actions } => {
                    session.begin_chunk(actions)?;
                    Ok(Reply::Observation(session.observe()?))
                }
                Command::Step {
                    handle,
                    root,
                    ports,
                } => {
                    let result = session.step(handle, root, &ports)?;
                    Ok(Reply::Step {
                        result,
                        next: session.observe()?,
                    })
                }
                Command::Drain => session.take_chunk().map(Reply::Data),
                Command::Snapshot => Ok(Reply::Data(session.checkpoint())),
                Command::Result => Ok(Reply::Data(session.result())),
                _ => unreachable!(),
            }
        }
    }
}
