import fs from 'node:fs';
import { spawn } from 'node:child_process';

export async function executeLookaheadProcess(command, args, logPath, { timeoutMs = 180000, graceMs = 1000 } = {}) {
  if (process.platform === 'win32' || !Number.isSafeInteger(timeoutMs) || timeoutMs < 1 || !Number.isSafeInteger(graceMs) || graceMs < 0)
    throw new Error('Lookahead requires POSIX process groups and positive timeout/nonnegative grace budgets');
  const fd = fs.openSync(logPath, 'wx'), start = performance.now();
  return new Promise((resolve, reject) => {
    let child;
    try { child = spawn(command, args, { detached: true, stdio: ['ignore', fd, fd] }); }
    catch (e) { fs.closeSync(fd); reject(e); return; }
    let timedOut = false, forced = false, forceTimer, spawnError = null, signalError = null;
    const signal = sig => {
      if (!child.pid) return;
      try { process.kill(-child.pid, sig); } catch (e) { if (e.code !== 'ESRCH') signalError = { code: e.code, message: e.message }; }
    };
    const guard = setTimeout(() => {
      timedOut = true; signal('SIGTERM');
      forceTimer = setTimeout(() => { forced = true; signal('SIGKILL'); }, graceMs);
    }, timeoutMs);
    child.on('error', e => { spawnError = { code: e.code, message: e.message }; });
    child.on('close', (code, terminationSignal) => {
      clearTimeout(guard); clearTimeout(forceTimer);
      // The group can outlive its leader, including a validator ignoring SIGTERM.
      if (timedOut) { forced = true; signal('SIGKILL'); }
      fs.closeSync(fd);
      resolve({ code, signal: terminationSignal, timed_out: timedOut, forced, spawn_error: spawnError, signal_error: signalError, elapsed_ms: performance.now() - start });
    });
  });
}
