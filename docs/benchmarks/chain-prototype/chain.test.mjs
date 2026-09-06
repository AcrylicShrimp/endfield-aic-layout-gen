import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { prepare, checkState } from './input.mjs';
import { construct } from './construct.mjs';
import { occupancy } from './geometry.mjs';
const root = fileURLToPath(new URL('../../../', import.meta.url)), read = p => JSON.parse(fs.readFileSync(p));
const keys = ['recipes', 'facilities', 'items', 'transports', 'components'];
const paths = Object.fromEntries(keys.map(k => [k, path.join(root, 'data/game/normalized', `${k === 'components' ? 'logistics-components' : k}.json`)]));
const catalogs = Object.fromEntries(keys.map(k => [k, read(paths[k])]));
const request = name => read(new URL(`./${name}-chain.json`, import.meta.url));
function validator(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-chain-test-')), calls = [];
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
const geometry = w => w.placements.map(p => [p.recipe, p.rotation, p.x, p.y, p.width, p.height]);
function retainedGeometry(before, after) {
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
test('real belt and pipe chain families grow and validate one routed facility at a time', async t => {
  for (const [name, expected] of [['battery', [[21,1,0],[28,2,0],[28,2,0]]], ['iron', [[72,2,0],[77,6,0],[77,5,0]]], ['pipe', [[55,1,0],[77,3,0],[77,3,0]]]]) {
    const input = request(name), before = JSON.stringify(input), graph = prepare(input, catalogs), v = validator(t), r = await construct(graph, v.validate);
    assert.equal(JSON.stringify(input), before);
    assert.equal(r.factory_validated, false); assert.equal(r.status, 'internal-chain-validated');
    assert.deepEqual(r.families.map(f => f.score.tuple), expected);
    assert.equal(r.families.length, 3);
    for (const f of r.families) {
      assert.equal(f.history.length, input.nodes.length);
      for (let i = 0; i < f.history.length; i++) {
        const call = v.calls[f.history[i].artifact];
        assert.equal(call.witness.placements.length, i + 1);
        assert.equal(call.witness.transport_networks.reduce((s, n) => s + n.requirement_ids.length, 0), i);
        assert.equal(call.report.covered_geometry_validated, true);
        assert.equal(checkState(graph, call.witness, call.report), true);
        if (i) retainedGeometry(v.calls[f.history[i - 1].artifact].witness, call.witness);
      }
    }
    assert.deepEqual([...new Set(r.decisions.map(d => d.gap))], [1, 2]);
    assert.equal(r.budget.used.validations, v.calls.length);
    assert.equal(r.budget.used.proposals, r.decisions.length);
    assert.equal(r.budget.used.route_expansions, r.decisions.reduce((n, d) => n + (d.route?.expanded ?? 0), 0));
    if (name === 'pipe') assert.ok(r.selected.witness.transport_networks.every(n => n.transport === 'pipe'));
  }
});
test('same input, reversed arrays and renamed node IDs preserve generated geometry and work counts', async t => {
  const input = request('iron'), v = validator(t), a = await construct(prepare(input, catalogs), v.validate);
  const reordered = structuredClone(input); reordered.nodes.reverse(); reordered.edges.reverse(); reordered.external.reverse();
  const b = await construct(prepare(reordered, catalogs), v.validate);
  assert.deepEqual(a.selected.witness, b.selected.witness); assert.deepEqual(a.budget.used, b.budget.used);
  const renamed = structuredClone(input), ids = new Map(renamed.nodes.map((n, i) => [n.id, ['__proto__', 'constructor', 'other'][i]]));
  for (const n of renamed.nodes) n.id = ids.get(n.id);
  for (const e of renamed.edges) { e.source = ids.get(e.source); e.target = ids.get(e.target); }
  for (const e of renamed.external) e.instance = ids.get(e.instance);
  const c = await construct(prepare(renamed, catalogs), v.validate);
  assert.deepEqual(geometry(a.selected.witness), geometry(c.selected.witness)); assert.deepEqual(a.budget.used, c.budget.used);
});
test('tight shared budgets preserve validated prefixes and never select a smaller partial chain', async t => {
  for (const [kind, amount] of [['proposals', 1], ['validations', 2], ['route_expansions', 1]]) {
    const input = request('iron'); input.budget[kind] = amount;
    const v = validator(t), r = await construct(prepare(input, catalogs), v.validate);
    assert.equal(r.factory_validated, false); assert.equal(r.selected, null);
    assert.equal(r.budget.exhausted, kind); assert.ok(r.budget.used[kind] <= amount);
    assert.equal(r.families.length, 1); assert.equal(r.families[0].score, null);
    assert.ok(r.families[0].history.every(h => v.calls[h.artifact].report.covered_geometry_validated));
    assert.deepEqual(r.unattempted_families, ['left', 'right']);
  }
});
test('failed independent validation cannot commit or expand a candidate', async t => {
  const input = request('iron'), v = validator(t);
  const r = await construct(prepare(input, catalogs), async (w, meta) => {
    const checked = await v.validate(w, meta);
    return meta.depth === 0 ? checked : { ...checked, status: 2 };
  });
  assert.equal(r.selected, null); assert.ok(r.families.every(f => f.history.length === 1));
  assert.ok(v.calls.every(c => c.meta.depth <= 1)); assert.ok(r.decisions.every(d => d.outcome === 'validation-failed'));
});
test('validation finishing after the shared deadline is not accepted', async t => {
  const input = request('battery'), v = validator(t); let time = 0;
  const r = await construct(prepare(input, catalogs), async (w, meta) => { const checked = await v.validate(w, meta); time = input.budget.elapsed_ms + 1; return checked; }, { now: () => time });
  assert.equal(r.selected, null); assert.equal(r.seed.accepted, false); assert.equal(r.budget.exhausted, 'elapsed_ms');
  assert.equal(r.families.length, 0); assert.equal(v.calls.length, 1);
});
test('fixed geometry, broken ledgers and over-capacity edges fail instead of changing the request', () => {
  for (const mutate of [s => s.max_width = 10, s => s.nodes[0].x = 0, s => s.edges[0].source_port = 'output-belt-1',
    s => s.edges[0].rate.denominator = 0, s => s.external.pop(), s => s.nodes[0].utilization.numerator = 2,
    s => s.nodes[0].recipe = 'unknown', s => s.policy.families = ['straight', 'straight'], s => s.budget.validations = 0]) {
    const s = request('battery'); mutate(s); assert.throws(() => prepare(s, catalogs), { code: 'chain-input-invalid' });
  }
  const s = request('battery'); s.edges[0].rate.numerator = 2;
  assert.throws(() => prepare(s, catalogs), { code: 'chain-unsupported-lanes' });
});
test('the citron cycle/branch is rejected as unsupported topology, not factory infeasibility', () => {
  const reference = read(new URL('../user-layout-reconstruction/citron-powder-03.json', import.meta.url));
  const input = request('battery');
  input.nodes = reference.placements.map(p => ({ id: p.id, recipe: p.recipe, utilization: p.utilization }));
  input.edges = reference.routes.map(r => ({ id: r.id, source: r.source[0], target: r.target[0], item: r.item, rate: r.rate }));
  input.external = reference.expected_pending.map((t, i) => ({ id: `external-${i}`, instance: t.instance, direction: t.direction, item: t.item, rate: t.rate }));
  assert.throws(() => prepare(input, catalogs), { code: 'chain-unsupported-topology' });
});
test('pipe-after-belt growth is explicitly unsupported in this sink-first slice', () => {
  const c = structuredClone(catalogs), input = request('iron');
  c.items.items.find(i => i.id === 'item-iron-nugget').transport = 'pipe';
  assert.throws(() => prepare(input, c), { code: 'chain-unsupported-pipe-order' });
  c.items.items.find(i => i.id === 'item-iron-nugget').transport = 'belt';
  c.items.items.find(i => i.id === 'item-iron-powder').transport = 'pipe';
  assert.deepEqual(prepare(input, c).growthEdges.map(e => e.transport), ['pipe', 'belt']);
});
test('same-layer transport blocks routing while bodies block both layers', () => {
  const w = { placements: [{ x: 0, y: 0, width: 1, height: 1 }], transport_networks: [{ transport: 'belt', cells: [{ x: 1, y: 0 }] }, { transport: 'pipe', cells: [{ x: 2, y: 0 }] }] };
  assert.deepEqual([...occupancy(w, 'belt')], ['0,0', '1,0']);
  assert.deepEqual([...occupancy(w, 'pipe')], ['0,0', '2,0']);
  assert.deepEqual([...occupancy(w)], ['0,0', '1,0', '2,0']);
});
test('prefix graph checks reject missing edges and incomplete independent ledgers', async t => {
  const input = request('battery'), graph = prepare(input, catalogs), v = validator(t), r = await construct(graph, v.validate);
  const w = structuredClone(r.selected.witness); w.transport_networks = [];
  const checked = await v.validate(w, { depth: 1 });
  assert.equal(checked.report.covered_geometry_validated, true);
  assert.throws(() => checkState(graph, w, checked.report), { code: 'chain-margin-mismatch' });
  const valid = structuredClone(v.calls[r.selected.artifact].report); valid.recipe_terminals.pop();
  assert.throws(() => checkState(graph, r.selected.witness, valid), { code: 'chain-margin-mismatch' });
});
test('CLI budget exhaustion writes a stopped report and preserves its validated prefix', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-chain-cli-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const input = request('iron'); input.budget.proposals = 1;
  const requestPath = path.join(directory, 'request.json'), output = path.join(directory, 'output');
  fs.writeFileSync(requestPath, JSON.stringify(input), { flag: 'wx' });
  const args = [fileURLToPath(new URL('./run.mjs', import.meta.url)), '--request', requestPath, ...keys.flatMap(k => [`--${k}`, paths[k]]),
    '--validator', path.join(root, 'target/release/aic-witness-probe'), '--output-directory', output];
  const run = () => spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 30000 });
  const result = run(); assert.ifError(result.error); assert.equal(result.status, 2, result.stderr);
  const report = read(path.join(output, 'report.json'));
  assert.equal(report.status, 'chain-stopped'); assert.equal(report.selected, null); assert.equal(report.budget.exhausted, 'proposals');
  assert.equal(report.families[0].covered, 2); assert.equal(report.factory_validated, false);
  assert.equal(fs.existsSync(path.join(output, 'selected-witness.json')), false);
  assert.equal(read(path.join(report.families[0].artifact.directory, 'validation.json')).covered_geometry_validated, true);
  const before = fs.readFileSync(path.join(output, 'report.json'), 'utf8');
  assert.equal(run().status, 1); assert.equal(fs.readFileSync(path.join(output, 'report.json'), 'utf8'), before);
});
test('successful CLI pages hide an unused legend and preserve the incomplete-factory scope after redraw', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-chain-page-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const output = path.join(directory, 'output');
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('./run.mjs', import.meta.url)),
    '--request', fileURLToPath(new URL('./battery-chain.json', import.meta.url)), ...keys.flatMap(k => [`--${k}`, paths[k]]),
    '--validator', path.join(root, 'target/release/aic-witness-probe'), '--output-directory', output], { encoding: 'utf8', timeout: 30000 });
  assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
  for (const family of ['straight', 'left', 'right']) {
    const html = fs.readFileSync(path.join(output, `${family}.html`), 'utf8');
    const script = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)].at(-1)[1];
    for (const sources of [['one'], ['one', 'two']]) {
      const elements = new Map(), element = id => {
        if (!elements.has(id)) elements.set(id, { style: {}, remove() { this.removed = true; } });
        return elements.get(id);
      };
      const context = vm.createContext({ sources, document: { querySelector: element, getElementById: element }, draw() { element('scope').textContent = 'old scope'; } });
      vm.runInContext(script, context); vm.runInContext('draw()', context);
      assert.equal(element('itemB').style.display, sources.length === 1 ? 'none' : '');
      assert.match(element('scope').textContent, /not a complete factory/);
      assert.equal(element('normal').removed, true); assert.equal(element('overload').removed, true);
    }
  }
});
