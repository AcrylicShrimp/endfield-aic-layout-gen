import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { constructInterface, orderedApproach } from './model.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
import { treeModule } from './trees.mjs';
import { rate, multiply, capacity, dto, parseRate, add, compare } from '../constructive-interconnect/rates.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k =>
  [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const request = name => read(`./${name}.json`);
const result = name => constructInterface(catalogs, request(name));

test('geometry-free recipe input discovers removable and unavoidable local crossings', () => {
  const four = result('four'), five = result('five');
  assert.equal(four.witness.logistics_components.filter(c => c.kind === 'bridge').length, 1);
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
  assert.equal(constructInterface(catalogs, q).witness.logistics_components.filter(c => c.kind === 'bridge').length, 1);
  const carbon = result('carbon');
  assert.equal(carbon.witness.logistics_components.filter(c => c.kind === 'bridge').length, 1);
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

test('four consumers share three source capacity ports through fixed splitter trees', () => {
  const r = result('port-pressure');
  assert.equal(r.port_groups.sources.length, 3);
  assert.equal(r.port_groups.targets.length, 8);
  assert.equal(r.witness.logistics_components.filter(c => c.kind === 'splitter').length, 5);
  assert.equal(r.witness.logistics_components.filter(c => c.kind === 'bridge').length, 6);
  assert.equal(r.accepted, false);
  assert.equal(new Set(r.witness.transport_networks.flatMap(n => n.terminals.filter(t => t.endpoint.kind === 'facility').map(t => t.id))).size,
    r.witness.transport_networks.reduce((s, n) => s + n.terminals.filter(t => t.endpoint.kind === 'facility').length, 0));
});

test('merge and combined recipes generate capacity-safe roots and exact leaf sums', () => {
  const merge = result('merge'), combined = result('combined');
  assert.equal(merge.witness.logistics_components.filter(c => c.kind === 'converger').length, 1);
  assert.equal(merge.witness.logistics_components.filter(c => c.kind === 'splitter').length, 0);
  assert.ok(['splitter', 'converger', 'bridge'].every(kind => combined.witness.logistics_components.some(c => c.kind === kind)));
  const cap = capacity(catalogs.transports.transports.find(t => t.kind === 'belt').capacity);
  for (const r of [merge, combined]) for (const g of [...r.port_groups.sources, ...r.port_groups.targets]) {
    const total = r.bindings.filter(l => g.lanes.includes(l.id)).reduce((s, l) => add(s, parseRate(l.rate)), rate(0));
    assert.deepEqual(dto(total), g.rate); assert.ok(compare(total, cap) <= 0);
  }
});

test('fixed combs support 1..12 leaves on both layers and reject invalid capacity/catalogs', () => {
  for (const transport of ['belt', 'pipe']) for (const kind of ['splitter', 'converger']) for (let count = 1; count <= 12; count++) {
    const cap = capacity(catalogs.transports.transports.find(t => t.kind === transport).capacity);
    const item = catalogs.items.items.find(i => i.transport === transport).id;
    const group = { id: 'tree', item, lanes: Array.from({ length: count }, (_, i) => ({ id: `leaf${i}`, item, transport, rate: dto(multiply(cap, rate(1, count))) })) };
    const tree = treeModule(group, kind, { x: 3, y: 2 }, catalogs);
    assert.equal(tree.components.length, count - 1); assert.deepEqual(tree.root_rate, dto(cap));
    assert.ok(tree.paths.every(p => p.cells.every(c => c.x >= tree.x && c.x < tree.x + tree.width && c.y >= tree.y && c.y < tree.y + tree.height)));
    const overload = structuredClone(group); overload.lanes[0].rate = dto(multiply(cap, rate(2)));
    assert.throws(() => treeModule(overload, kind, { x: 3, y: 2 }, catalogs), { code: 'interface-tree-capacity' });
    if (count > 1) assert.throws(() => treeModule(group, kind, { x: 3, y: 2 }, { ...catalogs, components: { components: [] } }), { code: 'interface-tree-catalog-unsupported' });
  }
});

test('aggregate capacity pressure stays explicit after sharing is introduced', () => {
  const changed = structuredClone(catalogs), q = request('four');
  const facility = changed.facilities.facilities.find(f => f.id === changed.recipes.recipes.find(r => r.id === q.producers[0].recipe).facility);
  let kept = false; facility.ports = facility.ports.filter(p => p.direction !== 'output' || p.transport !== 'belt' || (!kept && (kept = true)));
  assert.throws(() => constructInterface(changed, q), { code: 'interface-aggregate-port-pressure' });
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
