//! Fixed native workers with bounded queues; one ordered batch per worker/round.
//! Workers retain logical sessions. Observations move by Arc, with no serializer.
use super::{Limits, Observation, Options, Runtime};
use aic_data::layouts::{FactoryEditRequest, WitnessProbeInput};
use crossbeam_channel::{Receiver, Sender, bounded};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    thread::{self, JoinHandle},
};

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
type Work = Vec<(usize, Command)>;
type Replies = Vec<(usize, Reply)>;
struct Worker {
    sender: Sender<Option<Work>>,
    thread: Option<JoinHandle<()>>,
}
pub struct Pool {
    workers: Vec<Worker>,
    replies: Receiver<(usize, Replies, Option<String>)>,
    failed_replies: Replies,
    streams: usize,
    failed: bool,
}
impl Pool {
    pub fn new(
        catalogs: Arc<crate::input::Catalogs>,
        workers: usize,
        streams: usize,
    ) -> Result<Self> {
        if workers == 0 || workers > streams {
            return Err("declarative.pool-size: require 1 <= workers <= streams".into());
        }
        let (send, replies) = bounded(workers);
        let mut pool = Self {
            workers: Vec::new(),
            replies,
            streams,
            failed: false,
            failed_replies: Vec::new(),
        };
        for index in 0..workers {
            let (sender, receiver) = bounded::<Option<Work>>(1);
            let catalogs = catalogs.clone();
            let send = send.clone();
            let thread = thread::Builder::new()
                .name(format!("aic-native-{index}"))
                .spawn(move || {
                    let mut sessions = BTreeMap::new();
                    while let Ok(Some(work)) = receiver.recv() {
                        let mut completed = Vec::with_capacity(work.len());
                        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            for (id, command) in work {
                                completed
                                    .push((id, execute(&mut sessions, id, command, &catalogs)?));
                            }
                            Ok::<(), String>(())
                        }))
                        .unwrap_or_else(|_| Err(format!("declarative.pool-panic: worker {index}")))
                        .err();
                        if send.send((index, completed, error)).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|e| format!("declarative.pool-spawn: {e}"))?;
            pool.workers.push(Worker {
                sender,
                thread: Some(thread),
            });
        }
        Ok(pool)
    }
    pub fn exchange(&mut self, commands: Work) -> Result<Replies> {
        if self.failed
            && commands
                .iter()
                .any(|(_, c)| !matches!(c, Command::Drain | Command::Snapshot))
        {
            return Err(
                "declarative.pool-failed: only failure evidence and snapshots may be recovered"
                    .into(),
            );
        }
        let mut ids = BTreeSet::new();
        for (id, _) in &commands {
            if *id >= self.streams || !ids.insert(*id) {
                return Err("declarative.pool-ownership: unknown or duplicate stream".into());
            }
        }
        let mut batches: Vec<Work> = (0..self.workers.len()).map(|_| Vec::new()).collect();
        for (id, c) in commands {
            batches[id % self.workers.len()].push((id, c));
        }
        let mut waiting = BTreeSet::new();
        for (worker, work) in batches.into_iter().enumerate() {
            if work.is_empty() {
                continue;
            }
            self.workers[worker]
                .sender
                .send(Some(work))
                .map_err(|_| "declarative.pool-disconnected: submit")?;
            waiting.insert(worker);
        }
        let mut output = Vec::new();
        let mut error = None;
        while !waiting.is_empty() {
            let (worker, mut rows, failure) = self
                .replies
                .recv()
                .map_err(|_| "declarative.pool-disconnected: receive")?;
            if !waiting.remove(&worker) {
                return Err("declarative.pool-protocol: duplicate worker response".into());
            }
            output.append(&mut rows);
            if let Some(e) = failure {
                self.failed = true;
                error = Some(e);
            }
        }
        if let Some(e) = error {
            self.failed_replies.append(&mut output);
            return Err(e);
        }
        output.sort_by_key(|(id, _)| *id);
        Ok(output)
    }
    /// Available replies from an incomplete exchange, including workers that
    /// succeeded before another failed. Consume only for cold failure evidence.
    pub fn take_failed_replies(&mut self) -> Replies {
        std::mem::take(&mut self.failed_replies)
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        for w in &self.workers {
            let _ = w.sender.send(None);
        }
        for w in &mut self.workers {
            if let Some(t) = w.thread.take() {
                let _ = t.join();
            }
        }
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
                Command::Observe => Ok(Reply::Observation(session.observe())),
                Command::Begin { actions } => {
                    session.begin_chunk(actions)?;
                    Ok(Reply::Observation(session.observe()))
                }
                Command::Step {
                    handle,
                    root,
                    ports,
                } => {
                    let result = session.step(handle, root, &ports)?;
                    Ok(Reply::Step {
                        result,
                        next: session.observe(),
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
