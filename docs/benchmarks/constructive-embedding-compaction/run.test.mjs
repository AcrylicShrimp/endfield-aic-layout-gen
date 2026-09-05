import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import assert from 'node:assert/strict';

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
