import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import test from 'node:test';
import { prepareGroup, checkGroup } from './group-input.mjs';
import { constructGroup } from './group.mjs';
import { checkState } from './input.mjs';
const root = fileURLToPath(new URL('../../../', import.meta.url)), read = p => JSON.parse(fs.readFileSync(p));
const keys = ['recipes', 'facilities', 'items', 'transports', 'components'];
const paths = Object.fromEntries(keys.map(k => [k, path.join(root, 'data/game/normalized', `${k === 'components' ? 'logistics-components' : k}.json`)]));
const catalogs = Object.fromEntries(keys.map(k => [k, read(paths[k])]));
const request = () => read(new URL('./citron-group.json', import.meta.url));
function validator(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-group-test-')), calls = [];
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const validate = async (w, meta) => {
    const input = path.join(directory, `${calls.length}-witness.json`), output = path.join(directory, `${calls.length}-report.json`);
    fs.writeFileSync(input, JSON.stringify(w) + '\n', { flag: 'wx' });
    const p = spawnSync(path.join(root, 'target/release/aic-witness-probe'), ['--witness', input, ...keys.flatMap(k => [`--${k}`, paths[k]]), '--report-output', output], { encoding: 'utf8', timeout: 30000 });
    assert.ifError(p.error); assert.notEqual(p.status, 1, p.stderr);
    const report = read(output), artifact = calls.length;
    calls.push({ witness: structuredClone(w), meta, report, status: p.status });
    return { status: p.status, report, artifact };
  };
  return { calls, validate };
}
const shape = w => w.placements.map(p => [p.recipe, p.rotation, p.x, p.y, p.width, p.height]);
function retained(before, after) {
  const anchor = before.placements[0], next = after.placements.find(p => p.instance === anchor.instance), dx = next.x - anchor.x, dy = next.y - anchor.y;
  const move = p => ({ ...p, x: p.x + dx, y: p.y + dy });
  for (const p of before.placements) assert.deepEqual(after.placements.find(q => q.instance === p.instance), move(p));
  for (const n of before.transport_networks) {
    const q = after.transport_networks.find(q => q.id === n.id);
    for (const p of n.cells) assert.ok(q.cells.some(c => c.x === p.x + dx && c.y === p.y + dy));
    for (const s of n.segments) assert.ok(q.segments.some(t => JSON.stringify(t) === JSON.stringify({ ...s, from: move(s.from), to: move(s.to) })));
    for (const t of n.terminals) assert.deepEqual(q.terminals.find(u => u.id === t.id), { ...t, position: move(t.position) });
  }
}
test('citron cycle and branch grow privately with both cycle directions closed before commit', async t => {
  const input = request(), before = JSON.stringify(input), graph = prepareGroup(input, catalogs), v = validator(t), r = await constructGroup(graph, v.validate);
  assert.equal(JSON.stringify(input), before); assert.equal(r.status, 'internal-group-validated'); assert.equal(r.factory_validated, false);
  assert.deepEqual(r.order, ['G', 'P2', 'S', 'P1']); assert.deepEqual(r.commits.map(c => c.members.length), [1, 4]);
  assert.deepEqual(r.selected.history.map(h => h.committed), [true, false, false, true]);
  assert.equal(r.deepest_private.covered, 3); assert.equal(r.deepest_private.committed, false);
  for (const [i, h] of r.selected.history.entries()) {
    const call = v.calls[h.artifact]; assert.equal(call.witness.placements.length, i + 1); assert.equal(checkState(graph, call.witness, call.report), true);
    assert.equal(call.witness.transport_networks.reduce((n, net) => n + net.requirement_ids.length, 0), [0, 1, 2, 4][i]);
    if (i) retained(v.calls[r.selected.history[i - 1].artifact].witness, call.witness);
  }
  const final = v.calls[r.selected.artifact]; assert.equal(checkGroup(graph, final.witness, final.report), true);
  const pending = final.report.recipe_terminals.filter(t => t.pending_rate.numerator);
  assert.equal(pending.length, 1); assert.equal(pending[0].instance, 'G'); assert.deepEqual(pending[0].pending_rate, { numerator: 1, denominator: 1 });
  const seed = final.report.recipe_terminals.find(t => t.instance === 'S' && t.direction === 'output');
  assert.deepEqual(seed.routed_rate, { numerator: 1, denominator: 1 });
  assert.equal(final.witness.transport_networks.length, 2);
  assert.equal(r.budget.used.proposals, r.decisions.length);
  assert.equal(r.budget.used.validations, v.calls.length);
  assert.equal(r.budget.used.route_expansions, r.decisions.reduce((n, d) => n + (d.route?.expanded ?? 0), 0));
});
test('each approach priority is a checked proposal policy, including body contact and 180-degree folding', async t => {
  for (const first of ['straight', 'left', 'right', 'fold']) {
    const input = request(); input.policy.families = [first, ...input.policy.families.filter(f => f !== first)];
    const v = validator(t), r = await constructGroup(prepareGroup(input, catalogs), v.validate);
    t.diagnostic(JSON.stringify({ first, status: r.status, score: r.selected?.score, used: r.budget.used, selected: r.decisions.filter(d => d.kind === 'placement' && d.outcome === 'selected-group-path').map(d => [d.family, d.gap]) }));
    assert.equal(r.status, 'internal-group-validated');
    const firstSelected = r.decisions.find(d => d.kind === 'placement' && d.outcome === 'selected-group-path');
    assert.equal(firstSelected.family, first);
    if (first !== 'straight') assert.equal(firstSelected.gap, 0);
    if (first !== 'straight') {
      const firstPair = v.calls[r.selected.history[1].artifact].witness.placements;
      const [a, b] = firstPair;
      const touching = ((a.x + a.width === b.x || b.x + b.width === a.x) && Math.max(a.y, b.y) < Math.min(a.y + a.height, b.y + b.height))
        || ((a.y + a.height === b.y || b.y + b.height === a.y) && Math.max(a.x, b.x) < Math.min(a.x + a.width, b.x + b.width));
      assert.equal(touching, true, 'zero-gap selected bodies share an edge, not just a policy label');
      assert.equal((a.rotation - b.rotation + 360) % 360, first === 'fold' ? 180 : first === 'left' ? 90 : 270);
    }
    assert.ok(r.selected.history.every(h => v.calls[h.artifact].report.covered_geometry_validated));
  }
});
test('renaming IDs and reordering graph arrays preserve generated geometry and deterministic work', async t => {
  const input = request(), v = validator(t), a = await constructGroup(prepareGroup(input, catalogs), v.validate);
  const renamed = structuredClone(input), ids = new Map(renamed.nodes.map((n, i) => [n.id, ['__proto__', 'constructor', 'other', 'last'][i]]));
  renamed.nodes.forEach(n => n.id = ids.get(n.id));
  renamed.edges.forEach(e => { e.source = ids.get(e.source); e.target = ids.get(e.target); });
  renamed.external.forEach(e => e.instance = ids.get(e.instance));
  renamed.nodes.reverse(); renamed.edges.reverse(); renamed.external.reverse();
  const b = await constructGroup(prepareGroup(renamed, catalogs), v.validate);
  assert.deepEqual(shape(a.selected.witness), shape(b.selected.witness)); assert.deepEqual(a.budget.used, b.budget.used);
});
test('a private retry can mix approaches instead of forcing one bend family across the group', async t => {
  const input = request(); input.policy = { families: ['left', 'straight'], gaps: [0, 1], ports_per_side: 2 };
  const v = validator(t), r = await constructGroup(prepareGroup(input, catalogs), async (w, meta) => {
    const checked = await v.validate(w, meta);
    // Synthetic rejection exercises retry control, not a new game rule or quality heuristic.
    if (meta.depth === 2 && w.placements.find(p => p.instance === 'S').rotation !== w.placements.find(p => p.instance === 'P2').rotation) return { ...checked, status: 2 };
    return checked;
  });
  assert.equal(r.status, 'internal-group-validated');
  const choices = r.decisions.filter(d => d.kind === 'placement' && d.outcome === 'selected-group-path');
  assert.equal(choices[0].family, 'left'); assert.equal(choices[1].family, 'straight');
  for (const h of r.selected.history) assert.equal(v.calls[h.artifact].report.covered_geometry_validated, true);
});
test('proposal, route and validation exhaustion retain only the seed as a committed state', async t => {
  for (const [kind, amount] of [['proposals', 45], ['route_expansions', 1], ['validations', 3]]) {
    const input = request(); input.budget[kind] = amount;
    const v = validator(t), r = await constructGroup(prepareGroup(input, catalogs), v.validate);
    assert.equal(r.selected, null); assert.equal(r.status, 'group-stopped'); assert.equal(r.budget.exhausted, kind);
    assert.ok(r.budget.used[kind] <= amount); assert.deepEqual(r.commits.map(c => c.members.length), [1]);
    assert.equal(v.calls[r.base.artifact].report.covered_geometry_validated, true);
    assert.equal(r.deepest_private.covered, 3); assert.equal(r.deepest_private.committed, false);
  }
});
test('a rejected private branch cannot expand, and a rejected closure cannot commit', async t => {
  for (const rejectedDepth of [2, 3]) {
    const input = request(); input.budget.proposals = 150;
    const v = validator(t), r = await constructGroup(prepareGroup(input, catalogs), async (w, meta) => {
      const checked = await v.validate(w, meta); return meta.depth === rejectedDepth ? { ...checked, status: 2 } : checked;
    });
    assert.equal(r.selected, null); assert.deepEqual(r.commits.map(c => c.members.length), [1]);
    assert.ok(v.calls.every(c => c.meta.depth <= rejectedDepth));
    assert.ok(r.decisions.some(d => d.validation?.accepted === false));
  }
});
test('failed closing-port validation retries privately and retains both validation artifacts', async t => {
  const input = request(), v = validator(t); let rejected = false;
  const r = await constructGroup(prepareGroup(input, catalogs), async (w, meta) => {
    const checked = await v.validate(w, meta);
    if (meta.depth === 3 && !rejected) { rejected = true; return { ...checked, status: 2 }; }
    return checked;
  });
  assert.equal(r.status, 'internal-group-validated'); assert.equal(rejected, true);
  assert.ok(r.decisions.some(d => d.validation_trials?.some(v => !v.accepted)));
  const failed = r.decisions.find(d => d.kind === 'closing-ports' && d.validation?.accepted === false);
  const selected = r.decisions.find(d => d.kind === 'closing-ports' && d.outcome === 'selected-group-path');
  assert.ok(failed); assert.ok(selected); assert.notEqual(failed.validation.artifact, selected.validation.artifact);
});
test('deadline expiration cannot accept a late private result', async t => {
  const input = request(), v = validator(t); let time = 0;
  const r = await constructGroup(prepareGroup(input, catalogs), async (w, meta) => {
    const checked = await v.validate(w, meta); if (meta.depth === 1) time = input.budget.elapsed_ms + 1; return checked;
  }, { now: () => time });
  assert.equal(r.selected, null); assert.equal(r.budget.exhausted, 'elapsed_ms');
  assert.deepEqual(r.commits.map(c => c.members.length), [1]); assert.equal(r.deepest_private, null);
});
test('a graph-margin rejection remains distinguishable from independent process failure', async t => {
  const input = request(); input.budget.proposals = 100;
  const v = validator(t), r = await constructGroup(prepareGroup(input, catalogs), async (w, meta) => {
    const checked = await v.validate(w, meta);
    if (meta.depth === 3) checked.report.recipe_terminals = checked.report.recipe_terminals.slice(1);
    return checked;
  });
  assert.equal(r.selected, null); assert.deepEqual(r.commits.map(c => c.members.length), [1]);
  assert.ok(r.decisions.some(d => d.validation?.diagnostic?.code === 'chain-margin-mismatch'));
});
test('unsupported topologies and broken rate ledgers reject without silently cutting internal edges', () => {
  assert.throws(() => prepareGroup(read(new URL('./battery-chain.json', import.meta.url)), catalogs), { code: 'chain-unsupported-topology' });
  for (const mutate of [s => s.nodes[0].rotation = 0, s => s.policy.gaps = [-1], s => s.edges.pop(), s => s.external.pop(), s => s.edges[1].rate.numerator = 2]) {
    const input = request(); mutate(input); assert.throws(() => prepareGroup(input, catalogs));
  }
  const input = request(); input.edges[1].rate = { numerator: 1, denominator: 4 }; input.edges.push({ ...input.edges[1], id: 'parallel' });
  assert.throws(() => prepareGroup(input, catalogs), { code: 'chain-unsupported-topology' });
});
test('a missing closing edge can pass partial geometry but fails the graph closure gate', async t => {
  const input = request(), graph = prepareGroup(input, catalogs), v = validator(t), r = await constructGroup(graph, v.validate);
  const w = structuredClone(r.selected.witness), n = w.transport_networks.find(n => n.requirement_ids.includes('cycle'));
  const original = v.calls[r.selected.history[2].artifact].witness.transport_networks.find(n => n.requirement_ids.includes('branch'));
  // Remove the cycle's physical path by using the earlier branch network, translated into the final frame.
  const oldAnchor = v.calls[r.selected.history[2].artifact].witness.placements[0], newAnchor = w.placements[0], dx = newAnchor.x - oldAnchor.x, dy = newAnchor.y - oldAnchor.y;
  const move = p => ({ ...p, x: p.x + dx, y: p.y + dy });
  Object.assign(n, structuredClone(original), { cells: original.cells.map(move), segments: original.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })), terminals: original.terminals.map(t => ({ ...t, position: move(t.position) })) });
  const checked = await v.validate(w, { kind: 'missing-cycle' });
  assert.equal(checked.report.covered_geometry_validated, true);
  assert.throws(() => checkGroup(graph, w, checked.report), { code: 'chain-margin-mismatch' });
});
test('CLI success and failure preserve immutable evidence and distinguish private from committed geometry', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-group-cli-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  for (const stopped of [false, true]) {
    const input = request(); if (stopped) input.budget.proposals = 45;
    const requestPath = path.join(directory, `${stopped}-request.json`), output = path.join(directory, `${stopped}-output`);
    fs.writeFileSync(requestPath, JSON.stringify(input), { flag: 'wx' });
    const args = [fileURLToPath(new URL('./run-group.mjs', import.meta.url)), '--request', requestPath, ...keys.flatMap(k => [`--${k}`, paths[k]]),
      '--validator', path.join(root, 'target/release/aic-witness-probe'), '--output-directory', output];
    const run = () => spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 30000 });
    const result = run(); assert.ifError(result.error); assert.equal(result.status, stopped ? 2 : 0, result.stderr);
    const report = read(path.join(output, 'report.json'));
    assert.equal(report.factory_validated, false); assert.equal(report.commits.length, stopped ? 1 : 2);
    assert.equal(report.deepest_private.committed, false);
    assert.equal(read(path.join(report.deepest_private.artifact.directory, 'validation.json')).covered_geometry_validated, true);
    assert.equal(fs.existsSync(path.join(output, 'selected-witness.json')), !stopped);
    if (!stopped) assert.match(fs.readFileSync(path.join(output, 'index.html'), 'utf8'), /not a complete factory/);
    const before = fs.readFileSync(path.join(output, 'report.json'), 'utf8');
    assert.equal(run().status, 1); assert.equal(fs.readFileSync(path.join(output, 'report.json'), 'utf8'), before);
  }
});
