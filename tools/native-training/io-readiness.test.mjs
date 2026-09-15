import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { Gate, admissions, publishResult, splitAudit, retentionProjection } from './io-readiness.mjs';

test('budget stops owned descendants and preserves actual evidence', async () => {
  const output = await fs.mkdtemp(path.join(os.tmpdir(), 'aic-io-budget-'));
  const gate = new Gate(output, { seconds: 1, rss_bytes: 1024 ** 3, output_bytes: 1024 ** 2, min_free_bytes: 0 });
  const code = `const fs=require('fs');const {spawn}=require('child_process');fs.writeFileSync('actual.json',JSON.stringify({actual:true}));spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore'});setInterval(()=>{},1000);`;
  await assert.rejects(gate.run('failure', process.execPath, ['-e', code], output), /budget-time/);
  assert.deepEqual(JSON.parse(await fs.readFile(path.join(output, 'actual.json'))), { actual: true });
  assert.equal(gate.remaining().length, 0);
  assert.equal(gate.invocations.length, 1);
  await fs.rm(output, { recursive: true });
});
test('missing kept checkpoint cannot produce an admission audit', async () => {
  const output = await fs.mkdtemp(path.join(os.tmpdir(), 'aic-io-evidence-'));
  await assert.rejects(admissions(output, 2, 2, path.join(output, 'actual')), /incomplete-evidence/);
  await fs.rm(output, { recursive: true });
});
test('baseline descendant reserves its actual earlier root', () => {
  const root = { wave: 0, geometry_sha256: 'old-id', request_sha256: 'request', area: 10 };
  const starts = [{ id: '0/0', depth: 0, root }, { id: '1/0', depth: 1, root }];
  const identities = { rows: [{ id: '0/0', admission: { physical: { structural: 'reserved' } } }, { id: '1/0', admission: { physical: { structural: 'changed' } } }] };
  const panel = { cases: [{ admission: { physical: { structural: 'reserved' }, root: { structural: 'reserved' } } }] };
  assert.deepEqual(splitAudit(starts, identities, panel).overlaps, ['0/0', '1/0']);
  assert.throws(() => splitAudit(starts.slice(1), { rows: identities.rows.slice(1) }, panel), /lineage-missing/);
});

test('retention estimate scales measured streams, waves and all four arms', () => {
  assert.deepEqual(retentionProjection(33848304), { measured_bytes: 33848304, scale: 512, estimated_bytes: 17330331648 });
});

test('unlisted checkpoint state cannot count as verified evidence', async () => {
  const output = await fs.mkdtemp(path.join(os.tmpdir(), 'aic-io-unlisted-'));
  await fs.mkdir(path.join(output, 'checkpoints/00000001'), { recursive: true });
  await fs.writeFile(path.join(output, 'checkpoints/00000001/manifest.json'), JSON.stringify({schema:'aic-training-checkpoint-v8', binding:'a'.repeat(64), files:{}, replay:{}}));
  await assert.rejects(admissions(output, 1, 2, path.join(output, 'actual')), /incomplete-evidence: manifest/);
  await fs.rm(output, { recursive: true });
});

test('combined output accounting includes every retained setup root', async () => {
  const parent = await fs.mkdtemp(path.join(os.tmpdir(), 'aic-io-combined-'));
  const output = path.join(parent, 'scientific');
  const first = path.join(parent, 'first');
  const second = path.join(parent, 'second');
  await Promise.all([output, first, second].map(directory => fs.mkdir(directory)));
  await fs.writeFile(path.join(output, 'new'), '12345');
  await fs.writeFile(path.join(first, 'old'), '123');
  await fs.writeFile(path.join(second, 'old'), '1234');
  const gate = new Gate(output, { seconds: 10, rss_bytes: 48 * 1024 ** 3,
    output_bytes: 12, min_free_bytes: 0 }, [first, second]);
  await gate.enforce();
  assert.equal(gate.peakSetupBytes, 7);
  assert.equal(gate.peakScientificBytes, 5);
  assert.equal(gate.peakCombinedBytes, 12);
  await fs.writeFile(path.join(second, 'overflow'), 'x');
  await assert.rejects(gate.enforce(), /budget-output/);
  await fs.rm(parent, { recursive: true });
});

test('published resource bytes include the final result file', async () => {
  const output = await fs.mkdtemp(path.join(os.tmpdir(), 'aic-io-result-'));
  const result = { status: 'completed', resources: {
    scientific_root_bytes: 0, combined_retained_bytes: 0,
    peak_scientific_root_bytes: 0, combined_peak_output_bytes: 0,
  } };
  await publishResult(output, result, 7, { output_bytes: 1024 * 1024 });
  const resultBytes = (await fs.stat(path.join(output, 'result.json'))).size;
  assert.equal(result.resources.scientific_root_bytes, resultBytes);
  assert.equal(result.resources.combined_retained_bytes, resultBytes + 7);
  const published = JSON.parse(await fs.readFile(path.join(output, 'result.json')));
  assert.equal(published.resources.scientific_root_bytes, resultBytes);
  await fs.rm(output, { recursive: true });
});
