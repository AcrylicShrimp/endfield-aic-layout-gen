//! Lightweight live limits. Polling stays off the collection/optimizer thread.
use super::config::Resources;
use anyhow::{Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{Sender, channel},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

struct Shared {
    failed: AtomicBool,
    error: Mutex<Option<String>>,
    peak_rss: AtomicU64,
    deadline: Instant,
}
#[derive(Clone)]
pub struct Check(Arc<Shared>);
impl Check {
    pub fn run(&self) -> Result<()> {
        if self.0.failed.load(Ordering::Relaxed) {
            anyhow::bail!(
                self.0
                    .error
                    .lock()
                    .unwrap()
                    .clone()
                    .unwrap_or_else(|| "aic-learning.resource: guard failed".into())
            );
        }
        ensure!(
            Instant::now() < self.0.deadline,
            "aic-learning.elapsed-budget: training finish grace exhausted"
        );
        Ok(())
    }
    pub fn native(&self) -> std::result::Result<(), String> {
        self.run().map_err(|e| e.to_string())
    }
}
pub struct Guard {
    shared: Check,
    stop: Sender<()>,
    worker: Option<JoinHandle<()>>,
}
impl Guard {
    pub fn new(directory: &Path, resources: &Resources, seconds: u64) -> Result<Self> {
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(seconds))
            .ok_or_else(|| anyhow::anyhow!("aic-learning.elapsed-budget: invalid clock bound"))?;
        let shared = Check(Arc::new(Shared {
            failed: AtomicBool::new(false),
            error: Mutex::new(None),
            peak_rss: AtomicU64::new(0),
            deadline,
        }));
        let (stop, receiver) = channel();
        let value = shared.clone();
        let directory = directory.to_path_buf();
        let resources = resources.clone();
        let inspect = move |disk: bool| -> Result<()> {
            let rss = tree_rss()?;
            value.0.peak_rss.fetch_max(rss, Ordering::Relaxed);
            ensure!(
                rss <= resources.max_rss_bytes,
                "aic-learning.rss-budget: process tree uses {rss} bytes, limit {}",
                resources.max_rss_bytes
            );
            if disk && resources.min_free_bytes > 0 {
                let free = available(&directory)?;
                ensure!(
                    free >= resources.min_free_bytes,
                    "aic-learning.free-space: {} has {free} free bytes, requires {}",
                    directory.display(),
                    resources.min_free_bytes
                );
            }
            Ok(())
        };
        inspect(true)?;
        let value = shared.clone();
        let worker = std::thread::Builder::new()
            .name("aic-resource-guard".into())
            .spawn(move || {
                let mut tick = 0;
                while receiver.recv_timeout(Duration::from_secs(1)).is_err() {
                    tick += 1;
                    if let Err(error) = inspect(tick % 10 == 0) {
                        *value.0.error.lock().unwrap() = Some(format!("{error:#}"));
                        value.0.failed.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            })?;
        Ok(Self {
            shared,
            stop,
            worker: Some(worker),
        })
    }
    pub fn check(&self) -> Check {
        self.shared.clone()
    }
    pub fn peak_rss(&self) -> u64 {
        self.shared.0.peak_rss.load(Ordering::Relaxed)
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn available(path: &Path) -> Result<u64> {
    let output = Command::new("df").arg("-Pk").arg(path).output()?;
    ensure!(
        output.status.success(),
        "aic-learning.free-space: df failed for {}",
        path.display()
    );
    let text = String::from_utf8(output.stdout)?;
    let columns: Vec<_> = text
        .lines()
        .last()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    ensure!(
        columns.len() >= 6,
        "aic-learning.free-space: malformed filesystem response"
    );
    Ok(columns[3]
        .parse::<u64>()?
        .checked_mul(1024)
        .ok_or_else(|| anyhow::anyhow!("aic-learning.free-space: byte overflow"))?)
}
#[cfg(target_os = "linux")]
fn tree_rss() -> Result<u64> {
    let root = std::process::id();
    let mut pending = VecDeque::from([root]);
    let mut seen = BTreeSet::new();
    let mut sum = 0u64;
    while let Some(pid) = pending.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        let directory = PathBuf::from(format!("/proc/{pid}"));
        let status = match fs::read_to_string(directory.join("status")) {
            Ok(s) => s,
            Err(_) if pid != root => continue,
            Err(e) => return Err(e.into()),
        };
        if let Some(line) = status.lines().find(|s| s.starts_with("VmRSS:")) {
            sum += line.split_whitespace().nth(1).unwrap().parse::<u64>()? * 1024;
        }
        if let Ok(tasks) = fs::read_dir(directory.join("task")) {
            for task in tasks.flatten() {
                if let Ok(children) = fs::read_to_string(task.path().join("children")) {
                    pending.extend(
                        children
                            .split_whitespace()
                            .filter_map(|p| p.parse::<u32>().ok()),
                    );
                }
            }
        }
    }
    Ok(sum)
}
#[cfg(not(target_os = "linux"))]
fn tree_rss() -> Result<u64> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,rss="])
        .output()?;
    ensure!(
        output.status.success(),
        "aic-learning.rss-budget: cannot inspect process tree"
    );
    let text = String::from_utf8(output.stdout)?;
    let mut rows = BTreeMap::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() == 3 {
            rows.insert(
                fields[0].parse::<u32>()?,
                (fields[1].parse::<u32>()?, fields[2].parse::<u64>()?),
            );
        }
    }
    let mut pending = VecDeque::from([std::process::id()]);
    let mut seen = BTreeSet::new();
    let mut sum = 0;
    while let Some(pid) = pending.pop_front() {
        if seen.insert(pid) {
            if let Some((_, rss)) = rows.get(&pid) {
                sum += rss * 1024;
            }
            pending.extend(rows.iter().filter(|(_, r)| r.0 == pid).map(|(&id, _)| id));
        }
    }
    Ok(sum)
}

/// Account only known owned files during normal writes; one cold scan on resume.
#[derive(Default)]
pub struct Ledger {
    pub files: BTreeMap<PathBuf, u64>,
    pub bytes: u64,
}
impl Ledger {
    pub fn scan(root: &Path) -> Result<Self> {
        let mut result = Self::default();
        let mut pending = vec![root.to_path_buf()];
        while let Some(path) = pending.pop() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    pending.push(entry.path());
                } else if entry.file_type()?.is_file() {
                    result.track(&entry.path())?;
                }
            }
        }
        Ok(result)
    }
    pub fn track(&mut self, path: &Path) -> Result<()> {
        let bytes = fs::metadata(path)?.len();
        self.bytes = self.bytes - self.files.insert(path.to_path_buf(), bytes).unwrap_or(0) + bytes;
        Ok(())
    }
    pub fn remove(&mut self, path: &Path) -> Result<()> {
        fs::remove_file(path)?;
        self.bytes -= self.files.remove(path).unwrap_or(0);
        Ok(())
    }
    pub fn enforce(&self, limit: u64) -> Result<()> {
        ensure!(
            self.bytes <= limit,
            "aic-learning.output-budget: {} retained bytes exceed {limit}",
            self.bytes
        );
        Ok(())
    }
}
