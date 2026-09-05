import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import assert from 'node:assert/strict';

const root = fileURLToPath(new URL('../../../', import.meta.url));
function invoke(t, missingComponents) {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-interface-cli-'));
  t.after(() => fs.rmSync(temporary, { recursive: true, force: true }));
  const components = path.join(temporary, 'components.json');
  fs.writeFileSync(components, JSON.stringify({ components: [] }));
  const output = path.join(temporary, 'result');
  const args = ['docs/benchmarks/constructive-auto-interface/run.mjs', '--request', 'docs/benchmarks/constructive-auto-interface/four.json',
    ...['recipes', 'facilities', 'items', 'transports'].flatMap(k => [`--${k}`, `data/game/normalized/${k}.json`]),
    '--components', missingComponents ? components : 'data/game/normalized/logistics-components.json',
    // Node is intentionally not a witness validator: it rejects --witness without a report.
    '--validator', process.execPath, '--output-directory', output];
  const result = spawnSync(process.execPath, args, { cwd: root, encoding: 'utf8', timeout: 10000, maxBuffer: 1024 * 1024 });
  assert.equal(result.status, 2, result.stderr); assert.equal(result.stdout.trim().split('\n').length, 1);
  assert.equal(fs.existsSync(path.join(output, 'witness.json')), false);
  return JSON.parse(fs.readFileSync(path.join(output, 'report.json')));
}
test('no generated candidates is not described as independent validation rejection', t => {
  const report = invoke(t, true);
  assert.equal(report.status, 'interface-generation-failed'); assert.equal(report.candidates.length, 0);
  assert.equal(report.failures.length, 2); assert.equal(report.accepted, false);
});
test('validator process failure cannot select even the smallest candidate', t => {
  const report = invoke(t, false);
  assert.equal(report.status, 'independent-validation-rejected'); assert.equal(report.selected_embedding, null);
  assert.equal(report.candidates.length, 4); assert.equal(report.accepted, false);
  assert.ok(report.candidates.every(c => !c.accepted && c.diagnostics[0].code === 'interface-validator-process-failed'));
});
