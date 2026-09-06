import fs from 'node:fs';
import { spawn } from 'node:child_process';

// Keep the owned process handle until close; SIGTERM alone is not a hard resource limit.
export async function executeValidator(command, args, logPath, { timeoutMs = 30000, graceMs = 1000 } = {}) {
  if (![timeoutMs, graceMs].every(v => Number.isSafeInteger(v) && v >= 0)) throw new Error('Validator time budgets must be nonnegative integers');
  return new Promise((resolve, reject) => {
    const start = performance.now(), log = fs.createWriteStream(logPath, { flags: 'wx' }), p = spawn(command, args);
    let forced = false, timedOut = false, forceTimer;
    const guard = setTimeout(() => {
      timedOut = true; p.kill('SIGTERM');
      forceTimer = setTimeout(() => { forced = true; p.kill('SIGKILL'); }, graceMs);
    }, timeoutMs);
    const clear = () => { clearTimeout(guard); clearTimeout(forceTimer); };
    p.stdout.pipe(log, { end: false }); p.stderr.pipe(log, { end: false });
    log.on('error', e => { clear(); p.kill('SIGKILL'); reject(e); });
    p.on('error', e => { clear(); log.end(); reject(e); });
    p.on('close', (code, signal) => {
      clear(); log.end(() => resolve({ code, signal, timed_out: timedOut, forced, elapsed_ms: performance.now() - start }));
    });
  });
}
