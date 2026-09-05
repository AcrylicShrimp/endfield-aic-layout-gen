import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

test('comparison ignores timings but catches trial, candidate, selected-geometry and empty-portfolio changes', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-closure-comparison-')); let ordinal = 0;
  try {
    const fixture = name => {
      const p = path.join(dir, name); fs.mkdirSync(p); fs.writeFileSync(path.join(p, 'witness.json'), '{}\n');
      return { status: 'fixture', factory_validated: false, covered_facilities: 1, total_facilities: 2, score: { tuple: [1, 0, 0] },
        failures: [], selected_directory: p, elapsed_ms: name === 'before' ? 100 : 50,
        stages: [{ node: 'n', label: 'N', trials: 1, failures: [], selected_directory: p,
          candidates: [{ directory: p, accepted: true, factory_validated: false, score: { tuple: [1, 0, 0] }, witness_sha256: 'fixture-hash', diagnostics: [] }] }] };
    };
    const a = fixture('before'), b = fixture('after');
    const run = (a, b) => {
      const before = path.join(dir, 'before.json'), after = path.join(dir, 'after.json'), output = path.join(dir, `comparison-${ordinal++}.json`);
      fs.writeFileSync(before, JSON.stringify(a)); fs.writeFileSync(after, JSON.stringify(b));
      const p = spawnSync(process.execPath, [new URL('./compare-runs.mjs', import.meta.url).pathname, '--before', before, '--after', after, '--output', output], { encoding: 'utf8' });
      return { code: p.status, result: JSON.parse(fs.readFileSync(output)) };
    };
    assert.equal(run(a, b).code, 0);
    const changed = structuredClone(b); changed.stages[0].trials++;
    assert.equal(run(a, changed).result.all_passed, false);
    changed.stages[0].trials--; changed.stages[0].candidates[0].witness_sha256 = 'changed';
    assert.equal(run(a, changed).code, 2);
    fs.writeFileSync(path.join(b.selected_directory, 'witness.json'), '{"changed":true}\n');
    assert.equal(run(a, b).result.all_passed, false);
    assert.equal(run({ packings: [] }, { packings: [] }).result.all_passed, false);
  } finally { fs.rmSync(dir, { recursive: true }); }
});
