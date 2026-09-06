import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import assert from 'node:assert/strict';
import { setTimeout as delay } from 'node:timers/promises';
import { selectLookaheadSeeds, selectGlobalFactory, mapLookaheadBranches } from './lookahead.mjs';
import { executeLookaheadProcess } from './lookahead-process.mjs';

// Geometry-only fixtures isolate ranking; they make no factory-validity claim.
const witness = (width, height, cells = 0, name = 'body') => ({
  placements: [{ instance: name, x: 0, y: 0, width, height }], logistics_components: [],
  transport_networks: [{ transport: 'belt', cells: Array.from({ length: cells }, (_, x) => ({ x, y: 0 })), segments: [] }]
});
const record = (w, validated = true) => ({ witness: w, factory_validated: validated, score: { tuple: [-1, -1, -1] } });

test('lookahead ranks recomputed geometry, deduplicates exact content and breaks ties by source stage', () => {
  const a = witness(5, 5), b = witness(4, 6), c = witness(4, 6, 0, 'other');
  const entries = [{ stage: 9, witness: a }, { stage: 7, witness: b }, { stage: 3, witness: structuredClone(b) }, { stage: 2, witness: c }];
  const before = structuredClone(entries), r = selectLookaheadSeeds(entries, 3);
  assert.deepEqual([r.eligible_records, r.unique_witnesses, r.duplicates], [4, 3, 1]);
  assert.deepEqual(r.seeds.map(s => s.stage), [2, 3, 9]);
  assert.deepEqual(r.seeds.map(s => s.score.tuple), [[24, 0, 0], [24, 0, 0], [25, 0, 0]]);
  assert.deepEqual(selectLookaheadSeeds([...entries].reverse(), 2).seeds, r.seeds.slice(0, 2));
  assert.deepEqual(entries, before);
});

test('lookahead rejects invalid seed and worker budgets and source stages', async () => {
  for (const limit of [0, 5, 1.5, NaN, '2']) assert.throws(() => selectLookaheadSeeds([], limit), { code: 'lookahead-input' });
  for (const stage of [-1, 0.5, undefined]) assert.throws(() => selectLookaheadSeeds([{ stage, witness: witness(2, 2) }]), { code: 'lookahead-input' });
  for (const workers of [0, 5, 1.5, NaN]) await assert.rejects(mapLookaheadBranches([], workers, async () => 0), { code: 'lookahead-input' });
  assert.deepEqual(await mapLookaheadBranches([], 2, async () => assert.fail()), []);
});

test('global lookahead gate retains root for worse, invalid and tied candidates, ignoring reported scores', () => {
  const root = record(witness(5, 5)), before = structuredClone(root);
  const r = selectGlobalFactory(root, [record(witness(5, 6)), record(witness(1, 1), false), record(witness(1, 1), 'true'), record(witness(5, 5))]);
  assert.equal(r.selected, root); assert.deepEqual(r.score.tuple, [25, 0, 0]);
  assert.ok(r.comparisons.every(c => !c.improves_current)); assert.deepEqual(root, before);
  assert.throws(() => selectGlobalFactory(record(witness(5, 5), false), []), { code: 'lookahead-input' });
});

test('global lookahead gate gives area priority over transport and stable first-winner tie resolution', () => {
  const root = record(witness(5, 5)), winner = record(witness(4, 6, 4)), tied = record(witness(4, 6, 4));
  const r = selectGlobalFactory(root, [winner, record(witness(5, 5)), tied]);
  assert.equal(r.selected, winner); assert.deepEqual(r.score.tuple, [24, 4, 0]);
  assert.deepEqual(r.comparisons.map(c => c.improves_current), [true, false, false]);
  assert.deepEqual(r.comparisons.map(c => c.improves_global_root), [true, false, true]);
});

test('lookahead worker pool bounds concurrency and retains input order despite reversed completion', async () => {
  let live = 0, peak = 0; const finished = [];
  const result = await mapLookaheadBranches([30, 1, 1, 1], 2, async (ms, index) => {
    live++; peak = Math.max(peak, live); await delay(ms); live--; finished.push(index); return index;
  });
  assert.equal(peak, 2); assert.equal(live, 0); assert.deepEqual(result, [0, 1, 2, 3]); assert.notEqual(finished[0], 0);
});

test('lookahead waits for other owned workers before surfacing a worker failure', async () => {
  let finished = false;
  await assert.rejects(mapLookaheadBranches([0, 1], 2, async (_, index) => {
    if (!index) throw new Error('controlled worker failure'); await delay(20); finished = true;
  }), /controlled worker failure/);
  assert.equal(finished, true);
});

test('lookahead process records success and spawn failure without overwriting logs', async t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-lookahead-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const log = path.join(directory, 'success.log');
  const r = await executeLookaheadProcess(process.execPath, ['-e', 'console.log("finished")'], log, { timeoutMs: 5000 });
  assert.equal(r.code, 0); assert.equal(r.timed_out, false); assert.equal(r.spawn_error, null); assert.match(fs.readFileSync(log, 'utf8'), /finished/);
  await assert.rejects(executeLookaheadProcess(process.execPath, [], log), { code: 'EEXIST' });
  const missing = await executeLookaheadProcess(path.join(directory, 'not-an-executable'), [], path.join(directory, 'missing.log'));
  assert.equal(missing.spawn_error.code, 'ENOENT'); assert.equal(missing.timed_out, false);
  await assert.rejects(executeLookaheadProcess(process.execPath, [], path.join(directory, 'invalid.log'), { timeoutMs: 0 }));
  assert.equal(fs.existsSync(path.join(directory, 'invalid.log')), false);
});

test('lookahead timeout kills an owned group, including a resistant descendant after leader exit', { skip: process.platform === 'win32' }, async t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-lookahead-group-test-')), log = path.join(directory, 'timeout.log');
  let descendant;
  t.after(() => {
    if (descendant) { try { process.kill(descendant, 'SIGKILL'); } catch (e) { if (e.code !== 'ESRCH') throw e; } }
    fs.rmSync(directory, { recursive: true, force: true });
  });
  const childCode = 'process.on("SIGTERM",()=>{}); console.log("descendant="+process.pid); setInterval(()=>{},1000)';
  const parentCode = `const {spawn}=require('node:child_process'); spawn(process.execPath,['-e',${JSON.stringify(childCode)}],{stdio:['ignore',1,2]}); setInterval(()=>{},1000);`;
  const r = await executeLookaheadProcess(process.execPath, ['-e', parentCode], log, { timeoutMs: 1000, graceMs: 50 });
  assert.equal(r.timed_out, true); assert.equal(r.forced, true); assert.equal(r.signal, 'SIGTERM');
  descendant = Number(fs.readFileSync(log, 'utf8').match(/descendant=(\d+)/)?.[1]); assert.ok(descendant > 0);
  // Give init time to reap the orphan on POSIX systems.
  for (let i = 0; i < 100; i++) {
    try { process.kill(descendant, 0); } catch (e) { assert.equal(e.code, 'ESRCH'); descendant = null; break; }
    await delay(10);
  }
  assert.equal(descendant, null, 'the timed-out descendant must not remain live');
});

test('lookahead escalates when the group leader ignores SIGTERM', { skip: process.platform === 'win32' }, async t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-lookahead-force-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const r = await executeLookaheadProcess(process.execPath, ['-e', 'process.on("SIGTERM",()=>{}); setInterval(()=>{},1000)'], path.join(directory, 'force.log'), { timeoutMs: 500, graceMs: 50 });
  assert.equal(r.timed_out, true); assert.equal(r.forced, true); assert.equal(r.signal, 'SIGKILL');
});
