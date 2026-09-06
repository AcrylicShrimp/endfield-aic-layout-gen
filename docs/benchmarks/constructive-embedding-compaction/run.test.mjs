import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import assert from 'node:assert/strict';
import { executeValidator } from './validator.mjs';

test('existing output directory is untouched and always yields structured stderr', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-compaction-cli-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const invoke = () => spawnSync(process.execPath, [new URL('./run.mjs', import.meta.url).pathname,
    ...['graph', 'baseline', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator'].flatMap(k => [`--${k}`, 'unused']),
    '--output-directory', directory], { encoding: 'utf8', timeout: 10000 });
  let r = invoke(); assert.equal(r.status, 1); assert.equal(r.stdout, '');
  assert.equal(JSON.parse(r.stderr).code, 'EEXIST'); assert.deepEqual(fs.readdirSync(directory), []);
  fs.writeFileSync(path.join(directory, 'failure.json'), 'user-owned sentinel');
  r = invoke(); assert.equal(r.status, 1); assert.equal(JSON.parse(r.stderr).code, 'EEXIST');
  assert.equal(fs.readFileSync(path.join(directory, 'failure.json'), 'utf8'), 'user-owned sentinel');
});

test('validator completion flushes logs; an ignored SIGTERM is followed by SIGKILL', { timeout: 5000 }, async t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-validator-limit-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const normalLog = path.join(directory, 'normal.log');
  const normal = await executeValidator(process.execPath, ['-e', 'console.log("complete")'], normalLog);
  assert.equal(normal.code, 0); assert.equal(normal.timed_out, false); assert.equal(fs.readFileSync(normalLog, 'utf8'), 'complete\n');
  const stuckLog = path.join(directory, 'stuck.log');
  const stuck = await executeValidator(process.execPath, ['-e', 'process.on("SIGTERM",()=>{}); console.log("ready"); setInterval(()=>{},1000)'], stuckLog, { timeoutMs: 500, graceMs: 50 });
  assert.equal(fs.readFileSync(stuckLog, 'utf8'), 'ready\n');
  assert.equal(stuck.timed_out, true); assert.equal(stuck.forced, true); assert.equal(stuck.signal, 'SIGKILL');
  await assert.rejects(executeValidator(process.execPath, [], path.join(directory, 'invalid.log'), { timeoutMs: NaN }));
});
