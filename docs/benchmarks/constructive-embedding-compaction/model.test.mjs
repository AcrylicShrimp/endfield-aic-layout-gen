import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { compactEmbedding, solveDifferences } from './model.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
const catalogs = { facilities: JSON.parse(fs.readFileSync(new URL('../../../data/game/normalized/facilities.json', import.meta.url))) };
const rate = { numerator: 1, denominator: 1 }, key = p => `${p.x},${p.y}`;
const network = (id, corners, transport = 'belt') => {
  const cells = polyline(corners); return { id, item: id, transport, requirement_ids: [], component_ids: [], cells,
    segments: cells.slice(1).map((p, i) => ({ from: cells[i], to: p, rate: { ...rate } })), terminals: [] };
};
const detail = networks => ({ schema_version: 1, case: 'synthetic', embedding: 'test', accepted: true, factory_validated: true,
  witness: { schema_version: 1, placements: [], utilizations: {}, logistics_components: [], transport_networks: networks } });

test('difference solver matches exhaustive least solutions for 500 feasible tiny systems', () => {
  let seed = 17; const rand = n => { seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0; return seed % n; };
  for (let sample = 0; sample < 500; sample++) {
    const original = Array.from({ length: 3 }, () => rand(4)), lower = original.map(v => rand(v + 1)), edges = [];
    for (let from = 0; from < 3; from++) for (let to = 0; to < 3; to++) if (from !== to && rand(2))
      edges.push({ from, to, weight: original[to] - original[from] - rand(3) });
    const feasible = [];
    for (let a = 0; a <= 3; a++) for (let b = 0; b <= 3; b++) for (let c = 0; c <= 3; c++) {
      const v = [a, b, c]; if (v.every((x, i) => x >= lower[i]) && edges.every(e => v[e.to] >= v[e.from] + e.weight)) feasible.push(v);
    }
    assert.deepEqual(solveDifferences(lower, edges, original).values, lower.map((_, i) => Math.min(...feasible.map(v => v[i]))));
  }
  assert.throws(() => solveDifferences([0, 0], [{ from: 0, to: 1, weight: 2 }], [0, 1]), { code: 'compaction-original-certificate' });
  assert.throws(() => solveDifferences([0, 0], [{ from: 0, to: 1, weight: 1 }], [0, 1], 0), { code: 'compaction-relaxation-budget' });
});

test('unrelated rows compact independently, with reverse flow and detached idempotent output', () => {
  const d = detail([network('a', [[0, 0], [20, 0], [20, 2]]), network('b', [[30, 4], [30, 6], [10, 6]])]);
  const before = structuredClone(d), c = compactEmbedding(d, catalogs);
  assert.deepEqual(d, before); assert.equal(c.accepted, false); assert.equal(c.factory_validated, false);
  assert.equal(c.compaction_certificate.resulting_extent, 2);
  assert.ok(c.witness.transport_networks[1].segments.some(s => s.to.x < s.from.x));
  const again = compactEmbedding(c, catalogs);
  assert.deepEqual(again.pre_embedding_compaction, c.pre_embedding_compaction);
  assert.equal(Object.hasOwn(again.pre_embedding_compaction, 'compaction_certificate'), false);
  const normalized = w => w.transport_networks.map(n => ({ cells: n.cells.map(key).sort(), segments: n.segments.map(s => `${key(s.from)}>${key(s.to)}`).sort() }));
  assert.deepEqual(normalized(again.witness), normalized(c.witness));
  c.witness.transport_networks[0].segments[0].rate.numerator = 20;
  c.pre_embedding_compaction.embedding = 'mutated'; assert.deepEqual(d, before);
});

test('rigid catalog bodies and terminal connection offsets survive both axes', () => {
  let checked = 0;
  for (const f of catalogs.facilities.facilities) for (const rotation of f.allowed_rotations) {
  const s = rotateFacility(f, rotation);
  for (const port of s.ports) {
  const p = { instance: 'facility', facility: f.id, rotation, x: 15, y: 15, width: s.width, height: s.height };
  const dir = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] }[port.edge];
  const connection = { x: p.x + port.position.x + dir[0], y: p.y + port.position.y + dir[1] };
  const corners = [[connection.x + 3 * dir[0], connection.y + 3 * dir[1]], [connection.x, connection.y]];
  const n = network('a', port.direction === 'input' ? corners : corners.reverse(), port.transport);
  n.terminals.push({ id: 'input', rate, position: connection, endpoint: { kind: 'facility', instance: p.instance, port: port.id } });
  const d = detail([n]); d.witness.placements.push(p); d.witness.utilizations[p.instance] = rate;
  for (const axis of ['x', 'y']) for (const layerPolicy of ['coupled', 'independent']) {
    const c = compactEmbedding(d, catalogs, axis, { layerPolicy }), q = c.witness.placements[0], t = c.witness.transport_networks[0].terminals[0];
    assert.equal(q.width, p.width); assert.equal(q.height, p.height);
    assert.deepEqual(t.position, { x: q.x + port.position.x + dir[0], y: q.y + port.position.y + dir[1] });
    assert.deepEqual(c.catalog_ports[p.instance].find(t => t.id === port.id).connection, t.position);
    checked++;
  }
  }
  }
  assert.equal(checked, catalogs.facilities.facilities.reduce((n, f) => n + 4 * f.allowed_rotations.length * f.ports.length, 0));
});

test('independent layers may coincide, but same-layer paths and shared facility bodies stay separated', () => {
  for (const axis of ['x', 'y']) {
    const corners = axis === 'x' ? [[[0, 0], [0, 2]], [[20, 1], [20, 3]]] : [[[0, 0], [2, 0]], [[1, 20], [3, 20]]];
    const d = detail([network('belt', corners[0]), network('pipe', corners[1], 'pipe')]), before = structuredClone(d);
    const coupled = compactEmbedding(d, catalogs, axis), independent = compactEmbedding(d, catalogs, axis, { layerPolicy: 'independent' });
    assert.equal(coupled.compaction_certificate.resulting_extent, 2);
    assert.equal(independent.compaction_certificate.resulting_extent, 1);
    assert.deepEqual(d, before);
    d.witness.transport_networks[1].transport = 'belt';
    assert.equal(compactEmbedding(d, catalogs, axis, { layerPolicy: 'independent' }).compaction_certificate.resulting_extent, 2);
  }
  const f = catalogs.facilities.facilities[0], shape = rotateFacility(f, f.allowed_rotations[0]);
  const p = { instance: 'body', facility: f.id, rotation: f.allowed_rotations[0], x: 10, y: 0, width: shape.width, height: shape.height };
  const d = detail([network('left', [[0, 0], [0, shape.height]]), network('right', [[30, 0], [30, shape.height]], 'pipe')]);
  d.witness.placements = [p];
  const c = compactEmbedding(d, catalogs, 'x', { layerPolicy: 'independent' }), q = c.witness.placements[0];
  assert.ok(c.witness.transport_networks[0].cells.every(p => p.x < q.x));
  assert.ok(c.witness.transport_networks[1].cells.every(p => p.x >= q.x + q.width));
  assert.throws(() => compactEmbedding(d, catalogs, 'x', { layerPolicy: 'unknown' }), { code: 'compaction-layer-policy' });
});

test('row obstacles, rate transitions, layer coincidences and external sides are preserved', () => {
  const a = network('a', [[0, 0], [8, 0]]), b = network('b', [[4, 0], [4, 4]], 'pipe');
  a.segments.slice(4).forEach(s => { s.rate = { numerator: 2, denominator: 1 }; });
  a.terminals = [0, 8].map(x => ({ id: String(x), rate, endpoint: { kind: 'external' }, position: { x, y: 0 } }));
  const c = compactEmbedding(detail([a, b]), catalogs), [ca, cb] = c.witness.transport_networks;
  assert.equal(ca.terminals[0].position.x, 0); assert.equal(ca.terminals[1].position.x, c.compaction_certificate.resulting_extent - 1);
  assert.equal(ca.segments.length, 2); assert.deepEqual(ca.segments.map(s => s.rate.numerator), [1, 2]);
  assert.equal(ca.segments[0].to.x, cb.cells[0].x);
  const withComponent = detail([a, b]); withComponent.witness.logistics_components.push({ id: 'crossing', position: { x: 4, y: 0 } });
  assert.deepEqual(compactEmbedding(withComponent, catalogs).witness.logistics_components[0].position, cb.cells[0]);
  assert.throws(() => compactEmbedding(withComponent, catalogs, 'x', { pointBudget: 1 }), { code: 'compaction-point-budget' });
  assert.throws(() => compactEmbedding(withComponent, catalogs, 'x', { cellBudget: 1 }), { code: 'compaction-cell-budget' });
});
