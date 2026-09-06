import test from 'node:test';
import assert from 'node:assert/strict';
import { deflectionCandidates } from './deflection.mjs';
const body = (instance, x, y, width = 1, height = 1) => ({ instance, x, y, width, height });
const fixture = () => ({ witness: { placements: [body('a', 0, 8), body('b', 1, 8), body('c', 2, 8), body('frame', 8, 0)], transport_networks: [], logistics_components: [] }, labels: {} });
const catalogs = { components: { components: [] } };
test('deflection separates one member while preserving bounded simultaneous body geometry', () => {
  const d = fixture(), before = structuredClone(d), r = deflectionCandidates(d, catalogs, { limit: 64 });
  assert.ok(r.candidates.length > 0); assert.ok(r.candidates.length <= 64);
  assert.ok(r.candidates.some(p => p.void_candidate));
  assert.ok(r.groups > 0); assert.ok(r.boundary_seeds > 0); assert.ok(r.poses_examined <= r.pose_budget);
  for (const p of r.candidates) {
    assert.equal(p.moves.length, 2); assert.equal(p.moves[1].instances.length, 1); assert.equal(p.deflected_instance, p.moves[1].instances[0]);
    assert.notDeepEqual([p.moves[0].dx, p.moves[0].dy], [p.moves[1].dx, p.moves[1].dy]);
    const ids = p.moves.flatMap(m => m.instances); assert.equal(new Set(ids).size, ids.length); assert.ok(ids.length <= 16);
    const shifts = new Map(p.moves.flatMap(m => m.instances.map(id => [id, m]))), cells = new Set();
    for (const b of d.witness.placements) for (let dy = 0; dy < b.height; dy++) for (let dx = 0; dx < b.width; dx++) {
      const x = b.x + dx + (shifts.get(b.instance)?.dx ?? 0), y = b.y + dy + (shifts.get(b.instance)?.dy ?? 0), key = `${x},${y}`;
      assert.ok(x >= 0 && x <= 8 && y >= 0 && y <= 8); assert.ok(!cells.has(key)); cells.add(key);
    }
  }
  assert.deepEqual(d, before); assert.deepEqual(deflectionCandidates(d, catalogs, { limit: 64 }), r);
});
test('deflection charges sampled poses and rejects invalid budgets', () => {
  const d = fixture(), r = deflectionCandidates(d, catalogs, { poseBudget: 1 });
  assert.equal(r.poses_examined, 1); assert.equal(r.pose_budget_exhausted, true);
  for (const options of [{ limit: 0 }, { limit: 513 }, { poseBudget: 0 }, { poseBudget: 65537 }, { stepLimit: 0 }, { stepLimit: 9 }, { maxMembers: 1 }, { maxMembers: 17 }, { junctionPolicy: 'unknown' }])
    assert.throws(() => deflectionCandidates(d, catalogs, options), { code: 'deflection-invalid-input' });
});
test('deflection cannot separate retained non-detachable shared-terminal owners', () => {
  const d = fixture();
  d.witness.transport_networks.push({ item: 'synthetic-shared', transport: 'belt', cells: [{ x: 1, y: 7 }], segments: [], component_ids: [], terminals:
    ['a', 'b', 'c'].map((instance, i) => ({ id: `terminal-${instance}`, endpoint: { kind: 'facility', instance, port: `port-${instance}` },
      position: { x: 1, y: 7 }, direction: i ? 'input' : 'output', rate: { numerator: i ? 1 : 2, denominator: 1 } })) });
  const r = deflectionCandidates(d, catalogs);
  assert.ok(!r.candidates.some(p => ['a', 'b', 'c'].includes(p.deflected_instance)));
  assert.ok(r.rejected['local-shared-terminal-move'] > 0);
});
