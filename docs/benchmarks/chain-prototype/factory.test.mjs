import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import test from 'node:test';
import { prepareFactory } from './factory-input.mjs';
import { checkFactoryState } from './factory-check.mjs';
import { constructFactory } from './factory.mjs';
import { pendingPortAccess } from './factory-access.mjs';
import { routeConnection } from './factory-routing.mjs';
import { placement, ports } from './geometry.mjs';
import { Work } from './work.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url)), read = p => JSON.parse(fs.readFileSync(p));
const keys = ['recipes', 'facilities', 'items', 'transports', 'components'];
const paths = Object.fromEntries(keys.map(k => [k, path.join(root, 'data/game/normalized', `${k === 'components' ? 'logistics-components' : k}.json`)]));
const catalogs = Object.fromEntries(keys.map(k => [k, read(paths[k])]));
const request = (name, first = null) => {
  const input = read(new URL(`./${name}-factory.json`, import.meta.url));
  if (first) input.policy.families = [first, ...input.policy.families.filter(f => f !== first)];
  return input;
};
function validator(t) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-factory-test-')), calls = [];
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
test('all three recipe families and both iron inventories reach independently validated full factories', async t => {
  for (const [name, first, count, lanes] of [['battery', 'right', 8, 6], ['citron', 'fold', 4, 2], ['iron-nine', 'fold', 9, 3], ['iron-eight', 'fold', 8, 3]]) {
    const input = request(name, first), before = JSON.stringify(input), graph = prepareFactory(input, catalogs), v = validator(t), r = await constructFactory(graph, v.validate);
    assert.equal(JSON.stringify(input), before); assert.equal(r.status, 'factory-validated'); assert.equal(r.factory_validated, true);
    const final = v.calls[r.selected.artifact];
    assert.equal(final.report.covered_geometry_validated, true); assert.equal(final.report.recipe_coverage_complete, true);
    assert.equal(final.report.factory_validated, false, 'the Rust probe does not own the original graph');
    const coverage = checkFactoryState(graph, final.witness, final.report, { complete: true });
    assert.equal(coverage.factory_validated, true); assert.equal(coverage.covered, count); assert.deepEqual(coverage.external_lanes, { required: lanes, completed: lanes });
    let previous = null;
    for (const commit of r.commits) {
      const call = v.calls[commit.artifact], state = checkFactoryState(graph, call.witness, call.report);
      assert.equal(state.output_closed, true);
      if (previous) retainedGeometry(previous, call.witness);
      for (const h of commit.history) {
        const step = v.calls[h.artifact]; assert.equal(checkFactoryState(graph, step.witness, step.report).accepted, true);
        if (h.depth !== undefined) assert.equal(step.witness.placements.length, h.depth + 1);
      }
      previous = call.witness;
    }
    assert.equal(r.deepest_private, null); assert.equal(r.retained.validation.coverage.factory_validated, false);
    assert.equal(r.budget.used.proposals, r.decisions.length); assert.equal(r.budget.used.validations, v.calls.length);
    assert.equal(r.budget.used.route_expansions, r.decisions.reduce((s, d) => s + (d.route?.expanded ?? 0) + (d.access_trials ?? []).reduce((n, a) => n + a.expanded, 0), 0));
    assert.equal(r.commits.at(-1).kind, 'external-closure');
  }
});
test('every accepted private cycle step is routed before expansion and only the closed pair commits', async t => {
  const graph = prepareFactory(request('iron-nine'), catalogs), v = validator(t), r = await constructFactory(graph, v.validate);
  const cycle = r.commits.find(c => c.added?.length === 2); assert.ok(cycle); assert.equal(cycle.history.length, 2);
  const a = v.calls[cycle.history[0].artifact], b = v.calls[cycle.history[1].artifact];
  assert.equal(checkFactoryState(graph, a.witness, a.report).output_closed, false);
  assert.equal(checkFactoryState(graph, b.witness, b.report).output_closed, true);
  assert.equal(b.witness.placements.length, a.witness.placements.length + 1); retainedGeometry(a.witness, b.witness);
  assert.equal(b.witness.transport_networks.flatMap(n => n.requirement_ids).length, a.witness.transport_networks.flatMap(n => n.requirement_ids).length + 2);
});
test('the two powder export lanes are distinct and preserve the exact original aggregate rate', async t => {
  const graph = prepareFactory(request('citron', 'fold'), catalogs), v = validator(t), r = await constructFactory(graph, v.validate);
  const network = r.selected.witness.transport_networks.find(n => n.item === 'item-plant-moss-powder-2');
  assert.deepEqual(network.requirement_ids, ['powder-export']); assert.equal(network.terminals.length, 4);
  assert.equal(new Set(network.terminals.map(t => t.id)).size, 4);
  const external = network.terminals.filter(t => t.endpoint.kind === 'external');
  assert.equal(external.length, 2); assert.ok(external.every(t => t.direction === 'input' && t.rate.numerator === 1 && t.rate.denominator === 2));
  assert.equal(new Set(network.terminals.filter(t => t.endpoint.kind === 'facility').map(t => t.endpoint.port)).size, 2);
});
test('arrays and arbitrary facility IDs do not select fixed poses or change deterministic work', async t => {
  const input = request('battery', 'right'), v = validator(t), a = await constructFactory(prepareFactory(input, catalogs), v.validate);
  const renamed = structuredClone(input), ids = new Map(renamed.nodes.map((n, i) => [n.id, ['__proto__', 'constructor', 'other', 'four', 'five', 'six', 'seven', 'eight'][i]]));
  renamed.nodes.forEach(n => n.id = ids.get(n.id)); renamed.edges.forEach(e => { e.source = ids.get(e.source); e.target = ids.get(e.target); });
  renamed.external.forEach(e => e.instance = ids.get(e.instance)); renamed.nodes.reverse(); renamed.edges.reverse(); renamed.external.reverse();
  const b = await constructFactory(prepareFactory(renamed, catalogs), v.validate);
  const geometry = w => w.placements.map(p => [p.recipe, p.rotation, p.x, p.y, p.width, p.height]);
  assert.deepEqual(geometry(a.selected.witness), geometry(b.selected.witness)); assert.deepEqual(a.budget.used, b.budget.used);
});
test('the narrow battery channel has reachable ports but fails the multi-lane clearance heuristic', async t => {
  const graph = prepareFactory(request('battery'), catalogs), v = validator(t), work = new Work(graph.budget, () => 0);
  // A generated failure fixture, not a source screenshot or a constructor pose hint.
  let w = { schema_version: 1, placements: [['P', 8, 2], ['C1', 4, 1], ['F1', 0, 0], ['C2', 4, 4], ['F2', 0, 3]].map(([id, x, y]) => placement(graph.nodes.get(id), 90, x, y)),
    utilizations: Object.fromEntries(['P', 'C1', 'F1', 'C2', 'F2'].map(id => [id, graph.nodes.get(id).utilization])), transport_networks: [], logistics_components: [] };
  for (const [id, sourcePort, targetPort] of [['C1-P', 1, 0], ['F1-C1', 1, 0], ['C2-P', 0, 2], ['F2-C2', 1, 0]]) {
    const e = graph.lanes.find(e => e.id === id), source = w.placements.find(p => p.instance === e.source), target = w.placements.find(p => p.instance === e.target);
    w = routeConnection(w, e, { source: ports(source, graph.nodes.get(e.source)).find(p => p.id === `output-belt-${sourcePort}`),
      target: ports(target, graph.nodes.get(e.target)).find(p => p.id === `input-belt-${targetPort}`) }, {}, work);
    assert.ok(w);
  }
  const checked = await v.validate(w, { kind: 'narrow-channel-control' }); assert.equal(checkFactoryState(graph, w, checked.report).accepted, true);
  const access = pendingPortAccess(graph, w, work), rootInput = access.demands.find(d => d.instance === 'P' && d.direction === 'input');
  assert.equal(rootInput.required, 3); assert.equal(rootInput.ports.filter(p => p.reachable).length, 3);
  assert.equal(rootInput.available, 1); assert.equal(access.accepted, false);
});
test('shared budget failures retain accepted components without pretending that the factory is complete', async t => {
  for (const [kind, amount] of [['proposals', 50], ['route_expansions', 1], ['validations', 3]]) {
    const input = request('iron-nine'); input.budget[kind] = amount;
    const v = validator(t), r = await constructFactory(prepareFactory(input, catalogs), v.validate);
    assert.equal(r.factory_validated, false); assert.equal(r.selected, null); assert.equal(r.budget.exhausted, kind); assert.ok(r.budget.used[kind] <= amount);
    assert.ok(r.retained); assert.equal(v.calls[r.retained.artifact].report.covered_geometry_validated, true);
    assert.ok(r.commits.every(c => c.kind !== 'external-closure'));
  }
});
test('a rejected cycle step cannot replace earlier unrelated committed supplier geometry', async t => {
  const input = request('iron-nine'); input.budget.proposals = 180;
  const graph = prepareFactory(input, catalogs), v = validator(t), r = await constructFactory(graph, async (w, meta) => {
    const checked = await v.validate(w, meta); return meta.group?.length === 2 ? { ...checked, status: 2 } : checked;
  });
  assert.equal(r.factory_validated, false); assert.deepEqual(r.retained.witness.placements.map(p => p.instance), ['T', 'GP', 'P2']);
  assert.ok(v.calls.every(c => c.witness.placements.length <= 4));
  assert.equal(checkFactoryState(graph, r.retained.witness, v.calls[r.retained.artifact].report).output_closed, true);
});
test('failed external closure preserves the complete internal witness and never emits a final score', async t => {
  const input = request('citron', 'fold'); input.budget.proposals = 40;
  const v = validator(t), r = await constructFactory(prepareFactory(input, catalogs), async (w, meta) => {
    const checked = await v.validate(w, meta); return meta.kind.startsWith('boundary') ? { ...checked, status: 2 } : checked;
  });
  assert.equal(r.selected, null); assert.equal(r.retained.witness.placements.length, 4);
  assert.ok(r.retained.witness.transport_networks.every(n => n.terminals.every(t => t.endpoint.kind === 'facility')));
  assert.ok(r.commits.every(c => c.kind !== 'external-closure')); assert.equal(r.factory_validated, false);
});
test('late validation cannot commit an otherwise valid growth state', async t => {
  const input = request('citron', 'fold'), v = validator(t); let time = 0;
  const r = await constructFactory(prepareFactory(input, catalogs), async (w, meta) => {
    const checked = await v.validate(w, meta); if (meta.kind === 'private-growth') time = input.budget.elapsed_ms + 1; return checked;
  }, { now: () => time });
  assert.equal(r.selected, null); assert.equal(r.retained.witness.placements.length, 1); assert.equal(r.budget.exhausted, 'elapsed_ms');
});
test('coverage rejects omitted internal/external routes and undeclared external identities despite partial geometry passes', async t => {
  const graph = prepareFactory(request('citron', 'fold'), catalogs), v = validator(t), r = await constructFactory(graph, v.validate);
  for (const item of ['item-plant-moss-powder-2', 'item-plant-moss-2']) {
    const w = structuredClone(r.selected.witness); w.transport_networks = w.transport_networks.filter(n => n.item !== item);
    const checked = await v.validate(w, { kind: 'omitted-network', item });
    assert.equal(checked.report.covered_geometry_validated, true);
    assert.throws(() => checkFactoryState(graph, w, checked.report, { complete: true }), { code: 'chain-factory-coverage-invalid' });
  }
  const w = structuredClone(r.selected.witness), external = w.transport_networks.flatMap(n => n.terminals).find(t => t.endpoint.kind === 'external');
  external.endpoint.node = external.node = 'undeclared-export';
  const checked = await v.validate(w, { kind: 'wrong-external-identity' });
  assert.equal(checked.report.covered_geometry_validated, true); assert.equal(checked.report.recipe_coverage_complete, true);
  assert.throws(() => checkFactoryState(graph, w, checked.report, { complete: true }), { code: 'chain-factory-coverage-invalid' });
});
test('wrong boundary sides and utilization are independently rejected; the two iron inventories are not interchangeable', async t => {
  const graph = prepareFactory(request('iron-nine', 'fold'), catalogs), v = validator(t), r = await constructFactory(graph, v.validate);
  assert.throws(() => checkFactoryState(prepareFactory(request('iron-eight'), catalogs), r.selected.witness, v.calls[r.selected.artifact].report, { complete: true }), { code: 'chain-factory-coverage-invalid' });
  const w = structuredClone(r.selected.witness), external = w.transport_networks.flatMap(n => n.terminals).find(t => t.endpoint.kind === 'external');
  external.endpoint.side = external.position.y === 0 ? 'south' : 'north';
  assert.equal((await v.validate(w, { kind: 'wrong-side' })).report.covered_geometry_validated, false);
  const u = structuredClone(r.selected.witness); u.utilizations.T = { numerator: 1, denominator: 2 };
  assert.equal((await v.validate(u, { kind: 'wrong-utilization' })).report.covered_geometry_validated, false);
});
test('invalid schema, fixed geometry, missing margins and unsupported reconvergence reject explicitly', () => {
  for (const mutate of [s => s.schema_version = 1, s => s.max_width = 10, s => s.nodes[0].x = 0, s => s.external.pop(),
    s => s.boundary.margins = [0], s => s.policy.slides_per_pose = 0, s => s.edges[0].rate.denominator = 0]) {
    const input = request('battery'); mutate(input); assert.throws(() => prepareFactory(input, catalogs));
  }
  const shared = request('battery'); shared.nodes = shared.nodes.filter(n => n.id !== 'F2');
  shared.external = shared.external.filter(e => e.instance !== 'F2');
  shared.nodes.forEach(n => { if (n.id !== 'F1') n.utilization.denominator *= 2; });
  shared.edges.forEach(e => { e.rate.denominator *= 2; if (e.source === 'F2') e.source = 'F1'; });
  shared.external.forEach(e => { if (e.instance !== 'F1') e.rate.denominator *= 2; });
  assert.throws(() => prepareFactory(shared, catalogs), { code: 'chain-unsupported-factory-topology' });
});
test('standalone verification uses only the original request and witness, and rejects output reuse or missing exports', async t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-factory-verify-')); t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const input = request('citron', 'fold'), v = validator(t), r = await constructFactory(prepareFactory(input, catalogs), v.validate);
  const requestPath = path.join(directory, 'request.json'); fs.writeFileSync(requestPath, JSON.stringify(input), { flag: 'wx' });
  for (const complete of [true, false]) {
    const w = structuredClone(r.selected.witness); if (!complete) w.transport_networks = w.transport_networks.filter(n => n.item !== 'item-plant-moss-powder-2');
    const witnessPath = path.join(directory, `${complete}-witness.json`), output = path.join(directory, `${complete}-verification`);
    fs.writeFileSync(witnessPath, JSON.stringify(w), { flag: 'wx' });
    const args = [fileURLToPath(new URL('./verify-factory.mjs', import.meta.url)), '--request', requestPath, '--witness', witnessPath,
      ...keys.flatMap(k => [`--${k}`, paths[k]]), '--validator', path.join(root, 'target/release/aic-witness-probe'), '--output-directory', output];
    const run = () => spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 30000 });
    const result = run(); assert.ifError(result.error); assert.equal(result.status, complete ? 0 : 2, result.stderr);
    const report = read(path.join(output, 'report.json')); assert.equal(report.factory_validated, complete);
    assert.equal(report.independent.probe_factory_validated, false);
    const before = fs.readFileSync(path.join(output, 'report.json'), 'utf8'); assert.equal(run().status, 1); assert.equal(fs.readFileSync(path.join(output, 'report.json'), 'utf8'), before);
  }
});
test('original-request coverage rejects changed inventory and lane rates beyond partial geometry', async t => {
  const graph = prepareFactory(request('citron', 'fold'), catalogs), v = validator(t), r = await constructFactory(graph, v.validate);
  for (const change of ['missing-facility', 'extra-facility', 'changed-rate']) {
    const w = structuredClone(r.selected.witness);
    if (change === 'missing-facility') {
      w.placements = w.placements.filter(p => p.instance !== 'P1'); delete w.utilizations.P1;
      w.transport_networks = w.transport_networks.filter(n => !n.terminals.some(t => t.node === 'P1'));
    } else if (change === 'extra-facility') {
      const p = { ...w.placements[0], instance: 'undeclared', x: 100, y: 100 };
      // Remove boundary lanes: otherwise moving the used extreme is already a geometry error.
      w.transport_networks = w.transport_networks.filter(n => n.terminals.every(t => t.endpoint.kind === 'facility'));
      w.placements.push(p); w.utilizations.undeclared = w.utilizations[w.placements[0].instance];
    } else {
      for (const n of w.transport_networks.filter(n => n.item === 'item-plant-moss-powder-2')) for (const t of n.terminals) t.rate.denominator *= 2;
    }
    const checked = await v.validate(w, { kind: change });
    assert.equal(checked.report.covered_geometry_validated, true, change);
    assert.throws(() => checkFactoryState(graph, w, checked.report, { complete: true }), { code: 'chain-factory-coverage-invalid' });
  }
});
test('factory CLI exports only complete witnesses and preserves immutable stopped artifacts', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-factory-cli-')); t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  for (const complete of [true, false]) {
    const input = request('citron', 'fold'); if (!complete) input.budget.proposals = 1;
    const requestPath = path.join(directory, `${complete}-request.json`), output = path.join(directory, `${complete}-run`);
    fs.writeFileSync(requestPath, JSON.stringify(input), { flag: 'wx' });
    const args = [fileURLToPath(new URL('./run-factory.mjs', import.meta.url)), '--request', requestPath,
      ...keys.flatMap(k => [`--${k}`, paths[k]]), '--validator', path.join(root, 'target/release/aic-witness-probe'), '--output-directory', output];
    const run = () => spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 30000 });
    const result = run(); assert.ifError(result.error); assert.equal(result.status, complete ? 0 : 2, result.stderr);
    const reportPath = path.join(output, 'report.json'), report = read(reportPath), before = fs.readFileSync(reportPath, 'utf8');
    assert.equal(report.factory_validated, complete); assert.ok(fs.existsSync(path.join(output, 'retained-witness.json')));
    assert.equal(fs.existsSync(path.join(output, 'factory-witness.json')), complete); assert.equal(fs.existsSync(path.join(output, 'index.html')), complete);
    if (!complete) {
      assert.equal(report.selected, null); assert.equal(report.budget.exhausted, 'proposals');
      assert.deepEqual(report.commits.map(c => c.kind), ['seed', 'supplier-component']);
      assert.equal(report.retained.validation.coverage.covered, 2);
    }
    assert.equal(run().status, 1); assert.equal(fs.readFileSync(reportPath, 'utf8'), before);
  }
});
