import test from 'node:test';
import assert from 'node:assert/strict';
import { closeGeometry, pushCandidates, boundaryCohorts } from './push.mjs';
const body = (instance, x, y) => ({ instance, x, y, width: 1, height: 1 });
const terminal = (id, instance, x, y, direction = 'output') => ({ id, position: { x, y }, direction,
  endpoint: instance === null ? { kind: 'external', node: id } : { kind: 'facility', instance, port: id }, rate: { numerator: 1, denominator: 1 } });
const network = (terminals, transport = 'belt') => ({ id: terminals[0]?.id ?? 'empty', item: 'geometry-only', transport, terminals, cells: terminals.map(t => t.position), segments: [], component_ids: [] });
const witness = (placements, networks = []) => ({ placements, transport_networks: networks, logistics_components: [] });

test('body pushing discovers bounded cascades with reasons and immutable deterministic input', () => {
  const w = witness([body('a', 0, 0), body('b', 1, 0), body('c', 2, 0)]), before = structuredClone(w);
  const r = closeGeometry(w, ['a'], 1, 0);
  assert.equal(r.status, 'closed'); assert.deepEqual(r.instances, ['a', 'b', 'c']);
  assert.deepEqual(r.additions.map(a => [a.instance, a.reason]), [['b', 'body-body'], ['c', 'body-body']]);
  assert.deepEqual(closeGeometry(w, ['a'], 1, 0), r); assert.deepEqual(w, before);
  const limited = closeGeometry(w, ['a'], 1, 0, { maxMembers: 2 });
  assert.equal(limited.status, 'rejected'); assert.equal(limited.diagnostic.code, 'push-member-budget'); assert.equal(limited.instances.length, 2);
});

test('translated bodies collect terminal owners, and translated terminals collect body owners', () => {
  const w = witness([body('a', 0, 0), body('b', 4, 4)], [network([terminal('b-port', 'b', 1, 0)])]);
  assert.equal(closeGeometry(w, ['a'], 1, 0).additions[0].reason, 'body-terminal');
  const q = witness([body('a', 0, 0), body('b', 2, 1)], [network([terminal('a-port', 'a', 1, 1)], 'pipe')]);
  assert.equal(closeGeometry(q, ['a'], 1, 0).additions[0].reason, 'terminal-body');
});

test('terminal contacts respect layers and fixed external obligations', () => {
  const make = transport => witness([body('a', 0, 0), body('b', 4, 4)], [network([terminal('a-port', 'a', 1, 1)]), network([terminal('b-port', 'b', 2, 1)], transport)]);
  assert.deepEqual(closeGeometry(make('pipe'), ['a'], 1, 0).instances, ['a']);
  const r = closeGeometry(make('belt'), ['a'], 1, 0);
  assert.deepEqual(r.instances, ['a', 'b']); assert.equal(r.additions[0].reason, 'terminal-terminal');
  const ext = witness([body('a', 0, 0)], [network([terminal('external', null, 1, 0)])]);
  assert.equal(closeGeometry(ext, ['a'], 1, 0).diagnostic.code, 'push-fixed-external');
  const extTerminal = witness([body('a', 0, 0)], [network([terminal('a', 'a', 1, 1)]), network([terminal('external', null, 2, 1)])]);
  assert.equal(closeGeometry(extTerminal, ['a'], 1, 0).diagnostic.code, 'push-fixed-external');
});

test('non-detachable shared ownership closes but recognized zero-segment pairs can separate', () => {
  const ps = [body('a', 0, 0), body('b', 4, 4)], a = terminal('a-port', 'a', 1, 1), b = terminal('b-port', 'b', 1, 1, 'input');
  const shared = witness(ps, [network([a]), network([b])]);
  const r = closeGeometry(shared, ['a'], 1, 0); assert.deepEqual(r.instances, ['a', 'b']); assert.equal(r.additions[0].reason, 'shared-terminal');
  const detached = witness(ps, [{ ...network([a, b]), cells: [a.position] }]);
  assert.deepEqual(closeGeometry(detached, ['a'], 1, 0).instances, ['a']);
});

test('closure rejects invalid seeds and budgets explicitly', () => {
  const w = witness([body('a', 0, 0)]);
  for (const [ids, dx, dy, options] of [[[], 1, 0, {}], [['absent'], 1, 0, {}], [['a', 'a'], 1, 0, {}], [['a'], 0, 0, {}], [['a'], 0.5, 0, {}], [['a'], 1, 0, { maxMembers: 17 }]])
    assert.throws(() => closeGeometry(w, ids, dx, dy, options), { code: 'push-invalid-input' });
});

test('boundary cohorts derive separated support bodies from actual extents and diagnose oversized seeds', () => {
  const w = witness([body('a', 10, 20), { ...body('b', 13, 19), height: 2 }, body('c', 16, 18), body('d', 12, 10)]), before = structuredClone(w);
  const r = boundaryCohorts(w);
  assert.ok(r.seeds.some(g => g.boundary.side === 'maxY' && g.boundary.band === 1 && g.instances.join(',') === 'a,b'));
  assert.ok(r.seeds.some(g => g.boundary.side === 'maxY' && g.boundary.band === 3 && g.instances.join(',') === 'a,b,c'));
  assert.equal(new Set(r.seeds.map(g => JSON.stringify(g.instances))).size, r.seeds.length);
  const limited = boundaryCohorts(w, { maxMembers: 1 });
  assert.ok(limited.diagnostics.some(d => d.code === 'push-boundary-seed-budget')); assert.ok(limited.seeds.every(g => g.instances.length === 1));
  assert.deepEqual(w, before); assert.deepEqual(boundaryCohorts(w), r);
  assert.throws(() => boundaryCohorts(w, { bandLimit: 0 }), { code: 'push-invalid-input' });
});

test('push portfolio charges poses and exposes deterministic bounded proposed geometry', () => {
  const w = witness([body('a', 0, 0), body('b', 1, 0), body('frame', 8, 8)]), detail = { witness: w, labels: {} };
  const graph = { nodes: w.placements.map(p => ({ id: p.instance, recipe: 'geometry-only' })), edges: [] };
  const catalogs = { recipes: { recipes: [] }, facilities: { facilities: [] }, components: { components: [] } }, before = structuredClone(detail);
  const limited = pushCandidates(detail, graph, catalogs, { poseBudget: 1 });
  assert.equal(limited.poses_examined, 1); assert.equal(limited.pose_budget_exhausted, true);
  const r = pushCandidates(detail, graph, catalogs, { limit: 8 });
  assert.ok(r.candidates.length > 0 && r.candidates.length <= 8); assert.ok(r.grown_closures > 0);
  assert.ok(r.candidates.every(c => c.moves.length === 1 && c.moves[0].instances.length <= 16));
  assert.deepEqual(r, pushCandidates(detail, graph, catalogs, { limit: 8 })); assert.deepEqual(detail, before);
  assert.throws(() => pushCandidates(detail, graph, catalogs, { poseBudget: 0 }), { code: 'push-invalid-input' });
});
