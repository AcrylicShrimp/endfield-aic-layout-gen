import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import test from 'node:test';
import { prepareFactory } from './factory-input.mjs';
import { prepareReplayPolicy } from './replay-policy.mjs';
import { checkFactoryState } from './factory-check.mjs';
import { constructFactory } from './factory.mjs';
import { constructReplay } from './replay.mjs';
import { preservedPrefix, rollbackWindow } from './replay-preservation.mjs';
import { completionScore, compareTuple } from './replay-score.mjs';
import { remainingFit } from './remaining-fit.mjs';
import { WorkSlice } from './replay-work.mjs';
import { Work } from './work.mjs';
import { placement, occupancy } from './geometry.mjs';
import { reconstruct } from '../user-layout-reconstruction/model.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url)), read = p => JSON.parse(fs.readFileSync(p));
const keys = ['recipes', 'facilities', 'items', 'transports', 'components'];
const paths = Object.fromEntries(keys.map(k => [k, path.join(root, 'data/game/normalized', `${k === 'components' ? 'logistics-components' : k}.json`)]));
const catalogs = Object.fromEntries(keys.map(k => [k, read(paths[k])]));
const request = name => read(new URL(`./${name}-factory.json`, import.meta.url));
const policy = (overrides = {}) => prepareReplayPolicy({ ...read(new URL('./replay-policy.json', import.meta.url)), ...overrides });
function validator(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-replay-test-')), calls = [];
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const validate = async (w, meta) => {
    const input = path.join(directory, `${calls.length}-witness.json`), output = path.join(directory, `${calls.length}-report.json`);
    fs.writeFileSync(input, JSON.stringify(w) + '\n', { flag: 'wx' });
    const p = spawnSync(path.join(root, 'target/release/aic-witness-probe'), ['--witness', input, ...keys.flatMap(k => [`--${k}`, paths[k]]), '--report-output', output], { encoding: 'utf8', timeout: 30000 });
    assert.ifError(p.error); assert.notEqual(p.status, 1, p.stderr);
    const report = read(output), artifact = calls.length;
    calls.push({ witness: structuredClone(w), meta, report, status: p.status }); return { status: p.status, report, artifact };
  };
  return { calls, validate };
}
test('shared kernel preserves the four M3 selected geometries and exact work counts', async t => {
  for (const [name, first, tuple, counts] of [['battery', 'right', [240, 33, 3], [684, 1004, 15]], ['citron', 'fold', [152, 20, 0], [7, 212, 7]],
    ['iron-nine', 'fold', [341, 48, 2], [573, 9622, 13]], ['iron-eight', 'fold', [286, 44, 2], [572, 8252, 12]]]) {
    const input = request(name); input.policy.families = [first, ...input.policy.families.filter(f => f !== first)];
    const v = validator(t), r = await constructFactory(prepareFactory(input, catalogs), v.validate);
    assert.deepEqual(r.selected.score.tuple, tuple); assert.deepEqual(Object.values(r.budget.used), counts);
  }
});
test('all recipe families and both iron inventories complete through explicit replay controls with exact shared accounting', async t => {
  for (const name of ['battery', 'citron', 'iron-nine', 'iron-eight']) {
    const input = request(name), graph = prepareFactory(input, catalogs), v = validator(t), p = policy(name === 'battery' ? { proposal_order: 'exhaustive', ranking: 'partial-area' } : {});
    const before = JSON.stringify(input), r = await constructReplay(graph, p, v.validate);
    assert.equal(JSON.stringify(input), before); assert.equal(r.factory_validated, true);
    assert.equal(checkFactoryState(graph, r.selected.witness, v.calls[r.selected.artifact].report, { complete: true }).factory_validated, true);
    assert.equal(r.budget.used.proposals, r.decisions.length); assert.equal(r.budget.used.validations, v.calls.length);
    assert.equal(r.budget.used.route_expansions, r.decisions.reduce((s, d) => s + (d.route?.expanded ?? 0) + (d.access_trials ?? []).reduce((n, a) => n + a.expanded, 0), 0));
    assert.equal(r.budget.used.fit_checks, r.pools.flatMap(p => p.candidates).reduce((s, c) => s + (c.fit?.checks ?? 0), 0));
    for (const k of Object.keys(r.budget.used)) assert.ok(r.budget.used[k] <= r.budget.limits[k]);
    for (const pool of r.pools) for (const c of pool.candidates) {
      const call = v.calls[c.artifact]; assert.equal(checkFactoryState(graph, call.witness, call.report).output_closed, true);
      for (const h of c.history) {
        const step = v.calls[h.artifact]; assert.equal(checkFactoryState(graph, step.witness, step.report).accepted, true);
        assert.equal(step.witness.placements.length, h.depth + 1);
      }
    }
    assert.ok(r.rollbacks.length > 0);
    for (const b of r.rollbacks) {
      assert.ok(b.removed.length <= p.rollback_facilities); assert.ok(b.restart_group >= b.fixed_episode_floor); assert.equal(b.fixed_episode_floor, r.rollback_floor);
      const frozen = v.calls[b.restored_artifact].witness, next = r.trials[b.after_trial + 1];
      assert.equal(preservedPrefix(frozen, v.calls[next.internal_artifact].witness).accepted, true);
    }
    for (let i = 1; i < r.incumbent_history.length; i++) assert.ok(compareTuple(r.incumbent_history[i].score.tuple, r.incumbent_history[i - 1].score.tuple) < 0);
  }
});
test('two-facility iron-chain replay keeps the plant cycle and the unrelated first iron chain unchanged', async t => {
  const graph = prepareFactory(request('iron-eight'), catalogs), v = validator(t), r = await constructReplay(graph, policy(), v.validate);
  const b = r.rollbacks.find(b => b.removed.length === 2); assert.deepEqual(b.removed, ['G2', 'F2']);
  assert.deepEqual(b.frozen_members, ['T', 'GP', 'P', 'S', 'G1', 'F1']);
  const before = v.calls[b.restored_artifact].witness, next = v.calls[r.trials[b.after_trial + 1].internal_artifact].witness;
  assert.equal(preservedPrefix(before, next).accepted, true);
  const changed = structuredClone(next); changed.placements.find(p => p.instance === 'F1').x++;
  assert.throws(() => preservedPrefix(before, changed), { code: 'chain-replay-preservation-invalid' });
  const current = { witness: next, artifact: 0 }, base = { witness: before, artifact: b.restored_artifact };
  assert.equal(rollbackWindow(graph, base, current, 1), null);
});
test('actual completion replaces an incumbent only on a better full score and survives a later boundary failure', async t => {
  const graph = prepareFactory(request('citron'), catalogs), v = validator(t), r = await constructReplay(graph, policy(), v.validate);
  assert.equal(r.incumbent_history[0].score.area, 176); assert.equal(r.selected.score.area, 162);
  assert.ok(r.trials.some(t => t.complete && !t.selected && t.score.area > r.selected.score.area));
  const battery = prepareFactory(request('battery'), catalogs), b = validator(t), stopped = await constructReplay(battery, policy({ ranking: 'partial-area', proposal_order: 'exhaustive' }), b.validate);
  assert.equal(stopped.trials[0].complete, true); assert.equal(stopped.trials[1].complete, false); assert.equal(stopped.trials[1].stopped.stage, 'boundary');
  assert.equal(stopped.selected.trial, 0); assert.equal(stopped.factory_validated, true);
});
test('fit ranking is an explicit fallible heuristic: the battery control stops without widening its rollback suffix', async t => {
  const graph = prepareFactory(request('battery'), catalogs), v = validator(t), r = await constructReplay(graph, policy(), v.validate);
  assert.equal(r.factory_validated, false); assert.equal(r.retained.witness.placements.length, 7); assert.equal(r.selected, null);
  assert.equal(r.rollbacks.length, 0); assert.equal(r.rollback_floor, 6); assert.equal(r.budget.exhausted, null);
  assert.ok(r.diagnostics.some(d => d.code === 'chain-replay-window-exhausted'));
  const first = r.pools[0], smallest = [...first.candidates].sort((a, b) => a.partial_score.area - b.partial_score.area)[0];
  assert.notEqual(first.ranked_candidate_ids[0], smallest.id, 'fit ranking is actually used, not merely reported');
});
test('real routed continuation reverses a partial-area ordering and accepts fewer turns only after area and cells tie', async t => {
  const v = validator(t), r = await constructReplay(prepareFactory(request('iron-eight'), catalogs), policy({ ranking: 'partial-area' }), v.validate);
  const partial = trial => { const p = trial.path.at(-1); return r.pools[p.pool].candidates[p.candidate].partial_score.area; };
  assert.equal(partial(r.trials[1]), 230); assert.equal(r.trials[1].score.area, 288);
  assert.equal(partial(r.trials[3]), 240); assert.equal(r.trials[3].score.area, 280);
  assert.equal(r.selected.score.area, 280); assert.ok(r.trials[3].selected);
  const n = validator(t), tied = await constructReplay(prepareFactory(request('iron-nine'), catalogs), policy(), n.validate);
  assert.deepEqual(tied.incumbent_history.map(h => h.score.tuple), [[340, 40, 15], [340, 40, 12]]);
  assert.equal(tied.termination.code, 'chain-replay-trial-limit');
});
test('equal empty area does not imply that the remaining catalog-backed body fits the same envelope', () => {
  const original = prepareFactory(request('battery'), catalogs), a = { ...original.nodes.get('F1'), id: 'A' }, b = { ...original.nodes.get('F1'), id: 'B' };
  const graph = { nodes: new Map([['A', a], ['B', b]]) }, placed = placement(a, 0); assert.equal(placed.width, 3); assert.equal(placed.height, 3);
  const row = Array.from({ length: 9 }, (_, x) => ({ x, y: 3 }));
  const mask = cells => ({ placements: [placed], transport_networks: [{ transport: 'belt', cells: [...row, ...cells] }] });
  const good = mask([{ x: 5, y: 0 }, { x: 5, y: 1 }, { x: 5, y: 2 }]), bad = mask([{ x: 4, y: 0 }, { x: 6, y: 1 }, { x: 8, y: 2 }]);
  assert.equal(occupancy(good).size, occupancy(bad).size);
  const run = w => { const work = new Work({ fit_checks: 1024, elapsed_ms: 30000 }, () => 0); work.used.fit_checks = 0; return remainingFit(graph, w, work); };
  const before = JSON.stringify([good, bad]), fits = [run(good), run(bad)];
  assert.deepEqual(fits.map(f => f.estimated_area), [36, 48]); assert.ok(fits.every(f => f.complete && !f.factory_validated));
  assert.equal(JSON.stringify([good, bad]), before);
});
test('fit exhaustion is separate from routed work and never injects temporary bodies into a witness', async t => {
  const graph = prepareFactory(request('citron'), catalogs), v = validator(t), r = await constructReplay(graph, policy({ fit_checks: 1 }), v.validate);
  assert.equal(r.budget.used.fit_checks, 1); assert.equal(r.factory_validated, true);
  assert.ok(r.pools.flatMap(p => p.candidates).some(c => c.fit.status === 'fit-work-exhausted' && c.fit.estimated_area === null));
  for (const call of v.calls) assert.ok(call.witness.placements.length <= graph.nodes.size);
});
test('slice counters share direct routing work while local exhaustion does not reset or consume a new global allowance', () => {
  const work = new Work({ proposals: 9, route_expansions: 20, validations: 9, elapsed_ms: 30000 }, () => 0);
  const first = new WorkSlice(work, { proposals: 2, route_expansions: 4 });
  assert.equal(first.take('proposals', 2), true); first.used.route_expansions += 4;
  assert.equal(first.take('proposals'), false); assert.equal(first.report().scope, 'slice'); assert.equal(work.reason, null);
  const second = new WorkSlice(work, { proposals: 20, route_expansions: 20 });
  assert.equal(second.take('proposals', 7), true); assert.equal(second.take('proposals'), false); assert.equal(second.report().scope, 'global');
  assert.equal(second.limits.route_expansions, 20); assert.equal(work.used.route_expansions, 4); assert.equal(work.used.proposals, 9);
});
test('tight global work and a late validation retain the already completed incumbent', async t => {
  const input = request('citron'), p = policy(), baselineValidator = validator(t), baseline = await constructReplay(prepareFactory(input, catalogs), p, baselineValidator.validate);
  const first = baseline.trials[0], expected = baselineValidator.calls[baseline.incumbent_history[0].artifact].witness;
  for (const kind of ['proposals', 'route_expansions', 'validations']) {
    const limited = structuredClone(input); limited.budget[kind] = first.work_at_end[kind] + 1;
    const v = validator(t), r = await constructReplay(prepareFactory(limited, catalogs), p, v.validate);
    assert.equal(r.factory_validated, true, kind); assert.deepEqual(r.selected.witness, expected, kind); assert.equal(r.budget.exhausted, kind);
  }
  const v = validator(t); let time = 0;
  const late = await constructReplay(prepareFactory(input, catalogs), p, async (w, meta) => {
    const result = await v.validate(w, meta); if (v.calls.length > first.work_at_end.validations) time = input.budget.elapsed_ms + 1; return result;
  }, { now: () => time });
  assert.equal(late.factory_validated, true); assert.deepEqual(late.selected.witness, expected); assert.equal(late.budget.exhausted, 'elapsed_ms');
});
test('port-arm-inclusive scoring matches the three reference path counts and keeps their original scopes', () => {
  for (const [name, reference, turns] of [['battery', 'medium-battery-01', 7], ['citron', 'citron-powder-03', 5], ['iron-nine', 'enriched-iron-01', 12]]) {
    const spec = read(new URL(`../user-layout-reconstruction/${reference}.json`, import.meta.url)); spec.annotations = [];
    const reconstructed = reconstruct(spec, catalogs, []);
    const referenceGraph = { nodes: new Map(reconstructed.witness.placements.map(p => [p.instance, { facility_data: catalogs.facilities.facilities.find(f => f.id === p.facility) }])) };
    const score = completionScore(referenceGraph, reconstructed.witness);
    assert.equal(score.turns, turns); assert.equal(score.turn_definition, 'transport-cells-including-facility-arms-v1');
    assert.equal(reconstructed.factory_validated, false);
  }
});
test('invalid replay controls reject explicitly instead of broadening geometry or work bounds', () => {
  for (const change of [{ schema_version: 2 }, { rollback_facilities: 0 }, { ranking: 'optimal' }, { fit_checks: 0 }, { max_width: 15 }, { frontier_work: { proposals: 1 } }]) {
    assert.throws(() => policy(change));
  }
});
test('renamed IDs and reversed arrays preserve replay geometry, fit estimates and deterministic work', async t => {
  const input = request('iron-eight'), v = validator(t), p = policy({ trials: 2 }), a = await constructReplay(prepareFactory(input, catalogs), p, v.validate);
  const renamed = structuredClone(input), names = ['__proto__', 'constructor', 'three', 'four', 'five', 'six', 'seven', 'eight'], ids = new Map(renamed.nodes.map((n, i) => [n.id, names[i]]));
  renamed.nodes.forEach(n => n.id = ids.get(n.id)); renamed.edges.forEach(e => { e.source = ids.get(e.source); e.target = ids.get(e.target); });
  renamed.external.forEach(e => e.instance = ids.get(e.instance)); renamed.nodes.reverse(); renamed.edges.reverse(); renamed.external.reverse();
  const b = await constructReplay(prepareFactory(renamed, catalogs), p, v.validate);
  const geometry = w => w.placements.map(p => [p.recipe, p.rotation, p.x, p.y, p.width, p.height]);
  assert.deepEqual(geometry(a.selected.witness), geometry(b.selected.witness)); assert.deepEqual(a.budget.used, b.budget.used);
  assert.deepEqual(a.pools.map(p => p.candidates.map(c => c.fit.estimated_area)), b.pools.map(p => p.candidates.map(c => c.fit.estimated_area)));
});
test('a rejected cycle cannot enter a shortlist and rollback cannot split an atomic cycle', async t => {
  const graph = prepareFactory(request('citron'), catalogs), v = validator(t), r = await constructReplay(graph, policy({ trials: 1 }), async (w, meta) => {
    const result = await v.validate(w, meta); return meta.group?.length === 2 ? { ...result, status: 2 } : result;
  });
  assert.equal(r.factory_validated, false); assert.equal(r.retained.witness.placements.length, 2);
  assert.ok(r.pools.filter(p => p.group.length === 2).every(p => p.candidates.length === 0));
  assert.ok(v.calls.every(c => c.witness.placements.length <= 3));
  const all = { witness: { placements: graph.order.map(instance => ({ instance })) }, artifact: 1 };
  const half = { witness: { placements: graph.order.slice(0, -1).map(instance => ({ instance })) }, artifact: 0 };
  assert.equal(rollbackWindow(graph, half, all, 4), null);
});
test('replay CLI preserves immutable success/stopped artifacts and standalone verification accepts only its complete witness', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-replay-cli-')); t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const common = [...keys.flatMap(k => [`--${k}`, paths[k]]), '--validator', path.join(root, 'target/release/aic-witness-probe')];
  const policyPath = path.join(directory, 'policy.json'); fs.writeFileSync(policyPath, JSON.stringify(policy({ trials: 2 })), { flag: 'wx' });
  for (const complete of [true, false]) {
    const input = request('citron'); if (!complete) input.budget.proposals = 1;
    const requestPath = path.join(directory, `${complete}-request.json`), output = path.join(directory, `${complete}-run`);
    fs.writeFileSync(requestPath, JSON.stringify(input), { flag: 'wx' });
    const args = [fileURLToPath(new URL('./run-replay.mjs', import.meta.url)), '--request', requestPath, '--policy', policyPath, ...common, '--output-directory', output];
    const run = () => spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 30000 });
    const result = run(); assert.ifError(result.error); assert.equal(result.status, complete ? 0 : 2, result.stderr);
    const reportPath = path.join(output, 'report.json'), before = fs.readFileSync(reportPath, 'utf8'); assert.equal(read(reportPath).factory_validated, complete);
    assert.equal(fs.existsSync(path.join(output, 'factory-witness.json')), complete); assert.equal(fs.existsSync(path.join(output, 'index.html')), complete);
    assert.equal(run().status, 1); assert.equal(fs.readFileSync(reportPath, 'utf8'), before);
    const verify = spawnSync(process.execPath, [fileURLToPath(new URL('./verify-factory.mjs', import.meta.url)), '--request', requestPath,
      '--witness', path.join(output, complete ? 'factory-witness.json' : 'retained-witness.json'), ...common, '--output-directory', path.join(directory, `${complete}-verified`)], { encoding: 'utf8', timeout: 30000 });
    assert.ifError(verify.error); assert.equal(verify.status, complete ? 0 : 2, verify.stderr);
  }
});
