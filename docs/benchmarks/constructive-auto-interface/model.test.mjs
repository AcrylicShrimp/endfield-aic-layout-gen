import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { constructInterface, orderedApproach } from './model.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { renderFailure } from './failure-view.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k =>
  [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const request = name => read(`./${name}.json`);
const result = name => constructInterface(catalogs, request(name));

test('geometry-free recipe input discovers removable and unavoidable local crossings', () => {
  const four = result('four'), five = result('five');
  assert.equal(four.witness.logistics_components.length, 1);
  assert.equal(five.witness.logistics_components.length, 0);
  assert.equal(four.bindings.length, 4); assert.equal(five.bindings.length, 6);
  assert.equal(five.decisions.producer_order[1], 'B');
  assert.deepEqual([...five.decisions.producer_order].sort(), ['A0', 'A1', 'B']);
  assert.equal(four.accepted, false); assert.equal(five.factory_validated, false);
  for (const r of [four, five]) for (const n of request(r.bindings.length === 4 ? 'four' : 'five').producers)
    assert.deepEqual(r.witness.utilizations[n.id], n.utilization);
});

test('input array order does not change the constructed witness', () => {
  const q = request('five'), first = constructInterface(catalogs, q);
  q.producers.reverse(); q.consumers.reverse();
  assert.deepEqual(constructInterface(catalogs, q).witness, first.witness);
});

test('IDs and recipe family do not select hardcoded geometry', () => {
  const q = request('four');
  [...q.producers, ...q.consumers].forEach((n, i) => { n.id = `renamed-${9 - i}`; });
  assert.equal(constructInterface(catalogs, q).witness.logistics_components.length, 1);
  const carbon = result('carbon');
  assert.equal(carbon.witness.logistics_components.length, 1);
  assert.ok(carbon.witness.placements.some(p => p.facility.startsWith('furnance')));
});

test('ordered approaches are disjoint for 3136 mixed upward/downward endpoint sequences', () => {
  const sequences = [];
  for (let a = 0; a < 8; a++) for (let b = a + 1; b < 8; b++) for (let c = b + 1; c < 8; c++) sequences.push([a, b, c]);
  let count = 0;
  for (const a of sequences) for (const b of sequences) {
    const paths = orderedApproach(a.map((y, i) => ({ id: `${i}`, start: { x: 0, y }, end: { x: 5, y: b[i] } })));
    const used = new Set();
    for (const points of paths.values()) for (const p of polyline(points)) {
      const k = `${p.x},${p.y}`; assert.ok(!used.has(k)); used.add(k);
    }
    count++;
  }
  assert.equal(count, 3136);
  assert.throws(() => orderedApproach([{ id: 'a', start: { x: 0, y: 1 }, end: { x: 1, y: 1 } }]), { code: 'interface-approach-order' });
});

test('four consumer fragments expose port pressure, not a false global impossibility', () => {
  const q = request('four');
  q.consumers = Array.from({ length: 4 }, (_, i) => ({ ...q.consumers[0], id: `C${i}`, utilization: { numerator: 1, denominator: 4 } }));
  assert.throws(() => constructInterface(catalogs, q), e => e.code === 'interface-port-pressure' && e.entity.length === 2 && e.entity.every(p => p.count === 4));
  try { constructInterface(catalogs, q); } catch (e) {
    assert.equal(e.evidence.geometry_constructed, false);
    assert.equal(e.evidence.global_infeasibility_proven, false);
    assert.equal(e.evidence.lanes.length, 8);
    assert.ok(e.entity.every(p => p.available_ports < p.count));
    const html = renderFailure({ message: '</script><script>unsafe()</script>', code: e.code, entity: e.entity, evidence: e.evidence });
    assert.ok(html.includes('\\u003c/script>')); assert.ok(!html.includes('<script>unsafe()'));
  }
});

test('invalid, fixed-geometry and imbalanced requests fail explicitly', () => {
  const q = request('four');
  assert.throws(() => constructInterface(catalogs, { ...q, max_width: 100 }), { code: 'interface-invalid-request' });
  const geometry = structuredClone(q); geometry.producers[0].position = [0, 0];
  assert.throws(() => constructInterface(catalogs, geometry), { code: 'interface-invalid-instance' });
  const badRate = structuredClone(q); badRate.producers[0].utilization.numerator = 0;
  assert.throws(() => constructInterface(catalogs, badRate), { code: 'interface-invalid-instance' });
  const imbalance = structuredClone(q); imbalance.producers[0].utilization.denominator = 2;
  assert.throws(() => constructInterface(catalogs, imbalance), { code: 'interface-item-imbalance' });
  const duplicate = structuredClone(q); duplicate.consumers[0].id = duplicate.producers[0].id;
  assert.throws(() => constructInterface(catalogs, duplicate), { code: 'interface-invalid-instance' });
  const missing = structuredClone(q); missing.producers[0].recipe = 'unknown';
  assert.throws(() => constructInterface(catalogs, missing), { code: 'interface-unknown-recipe' });
  const liquid = structuredClone(q), pipeRecipe = catalogs.recipes.recipes.find(r => [...r.inputs, ...r.outputs]
    .some(a => catalogs.items.items.find(i => i.id === a.item)?.transport === 'pipe'));
  liquid.producers[0].recipe = pipeRecipe.id;
  assert.throws(() => constructInterface(catalogs, liquid), { code: 'interface-unsupported-transport' });
});
