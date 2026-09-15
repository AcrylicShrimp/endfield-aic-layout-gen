//! Shared fixed-ownership worker scheduling with bounded queues.

use crossbeam_channel::{Receiver, Sender, bounded};
use std::{
    collections::BTreeSet,
    sync::Arc,
    thread::{self, JoinHandle},
};

type Result<T> = std::result::Result<T, String>;
type Work<C> = Vec<(usize, C)>;
type Replies<R> = Vec<(usize, R)>;

struct Worker<C> {
    sender: Sender<Option<Work<C>>>,
    thread: Option<JoinHandle<()>>,
}

pub(crate) struct FixedWorkerPool<C, R> {
    workers: Vec<Worker<C>>,
    replies: Receiver<(usize, Replies<R>, Option<String>)>,
    failed_replies: Replies<R>,
    streams: usize,
    failed: bool,
    recoverable: Arc<dyn Fn(&C) -> bool + Send + Sync>,
    namespace: &'static str,
    failed_message: &'static str,
}

impl<C: Send + 'static, R: Send + 'static> FixedWorkerPool<C, R> {
    pub(crate) fn new<S: Send + 'static>(
        workers: usize,
        streams: usize,
        mut make_state: impl FnMut(usize) -> Result<S>,
        execute: impl Fn(&mut S, usize, C) -> Result<R> + Send + Sync + 'static,
        recoverable: impl Fn(&C) -> bool + Send + Sync + 'static,
        namespace: &'static str,
        failed_message: &'static str,
        thread_name: &'static str,
    ) -> Result<Self> {
        if workers == 0 || workers > streams {
            return Err(format!("{namespace}-size: require 1 <= workers <= streams"));
        }
        let (send, replies) = bounded(workers);
        let execute = Arc::new(execute);
        let mut pool = Self {
            workers: Vec::with_capacity(workers),
            replies,
            failed_replies: Vec::new(),
            streams,
            failed: false,
            recoverable: Arc::new(recoverable),
            namespace,
            failed_message,
        };
        for index in 0..workers {
            let mut state = make_state(index)?;
            let (sender, receiver) = bounded::<Option<Work<C>>>(1);
            let send = send.clone();
            let execute = execute.clone();
            let thread = thread::Builder::new()
                .name(format!("{thread_name}-{index}"))
                .spawn(move || {
                    while let Ok(Some(work)) = receiver.recv() {
                        let mut completed = Vec::with_capacity(work.len());
                        let error = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            for (stream, command) in work {
                                completed.push((stream, execute(&mut state, stream, command)?));
                            }
                            Ok::<(), String>(())
                        }))
                        .unwrap_or_else(|_| Err(format!("{namespace}-panic: worker {index}")))
                        .err();
                        if send.send((index, completed, error)).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|error| format!("{namespace}-spawn: {error}"))?;
            pool.workers.push(Worker {
                sender,
                thread: Some(thread),
            });
        }
        Ok(pool)
    }

    pub(crate) fn exchange(&mut self, commands: Work<C>) -> Result<Replies<R>> {
        if self.failed
            && commands
                .iter()
                .any(|(_, command)| !(self.recoverable)(command))
        {
            return Err(format!(
                "{}-failed: {}",
                self.namespace, self.failed_message
            ));
        }
        let mut ids = BTreeSet::new();
        for (stream, _) in &commands {
            if *stream >= self.streams || !ids.insert(*stream) {
                return Err(format!(
                    "{}-ownership: unknown or duplicate stream",
                    self.namespace
                ));
            }
        }
        let mut batches: Vec<Work<C>> = (0..self.workers.len()).map(|_| Vec::new()).collect();
        for (stream, command) in commands {
            batches[stream % self.workers.len()].push((stream, command));
        }
        let mut waiting = BTreeSet::new();
        for (worker, work) in batches.into_iter().enumerate() {
            if work.is_empty() {
                continue;
            }
            self.workers[worker]
                .sender
                .send(Some(work))
                .map_err(|_| format!("{}-disconnected: submit", self.namespace))?;
            waiting.insert(worker);
        }
        let mut output = Vec::new();
        let mut error = None;
        while !waiting.is_empty() {
            let (worker, mut rows, failure) = self
                .replies
                .recv()
                .map_err(|_| format!("{}-disconnected: receive", self.namespace))?;
            if !waiting.remove(&worker) {
                return Err(format!(
                    "{}-protocol: duplicate worker response",
                    self.namespace
                ));
            }
            output.append(&mut rows);
            if let Some(failure) = failure {
                self.failed = true;
                error = Some(failure);
            }
        }
        if let Some(error) = error {
            self.failed_replies.append(&mut output);
            return Err(error);
        }
        output.sort_by_key(|(stream, _)| *stream);
        Ok(output)
    }

    pub(crate) fn take_failed_replies(&mut self) -> Replies<R> {
        std::mem::take(&mut self.failed_replies)
    }
}

impl<C, R> Drop for FixedWorkerPool<C, R> {
    fn drop(&mut self) {
        for worker in &self.workers {
            let _ = worker.sender.send(None);
        }
        for worker in &mut self.workers {
            if let Some(thread) = worker.thread.take() {
                let _ = thread.join();
            }
        }
    }
}
