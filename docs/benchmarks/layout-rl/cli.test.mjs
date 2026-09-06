import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import test from 'node:test';

const directory = fileURLToPath(new URL('.', import.meta.url));
const invoke = (script, args) => spawnSync(process.execPath, [path.join(directory, script), ...args], { encoding: 'utf8', timeout: 5000 });
const diagnostic = result => {
  assert.equal(result.status, 1); assert.equal(result.error, undefined);
  const value = JSON.parse(result.stderr.trim());
  assert.equal(value.severity, 'error'); assert.equal(typeof value.message, 'string');
  assert.equal(typeof value.stage, 'string'); assert.ok(Object.hasOwn(value, 'entity'));
  return value;
};

test('CLI rejects missing, duplicate and unknown arguments with structured diagnostics', () => {
  assert.equal(diagnostic(invoke('import.mjs', [])).code, 'rl-input-invalid');
  assert.equal(diagnostic(invoke('server.mjs', ['--unknown', 'x'])).code, 'rl-input-invalid');
  assert.equal(diagnostic(invoke('server.mjs', ['--roots', 'x', '--roots', 'y'])).code, 'rl-input-invalid');
});

test('startup rejects stale provenance before making output and never overwrites a directory', () => {
  const temp = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-rl-cli-test-'));
  try {
    const roots = path.join(temp, 'roots.json'), output = path.join(temp, 'output');
    const dataset = { schema_version: 1, factory_validated: true, roots: [], provenance: [{ path: path.join(directory, 'plan.json'), sha256: 'incorrect' }] };
    fs.writeFileSync(roots, JSON.stringify(dataset), { flag: 'wx' });
    assert.equal(diagnostic(invoke('server.mjs', ['--roots', roots, '--output-directory', output])).code, 'rl-stale-input');
    assert.equal(fs.existsSync(output), false);
    dataset.provenance = []; fs.writeFileSync(roots, JSON.stringify(dataset));
    fs.mkdirSync(output); const sentinel = path.join(output, 'sentinel'); fs.writeFileSync(sentinel, 'preserve');
    assert.match(diagnostic(invoke('server.mjs', ['--roots', roots, '--output-directory', output])).message, /EEXIST/);
    assert.equal(fs.readFileSync(sentinel, 'utf8'), 'preserve');
  } finally { fs.rmSync(temp, { recursive: true, force: true }); }
});
