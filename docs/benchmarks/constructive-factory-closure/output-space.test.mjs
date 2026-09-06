import test from 'node:test';
import assert from 'node:assert/strict';
import { pocketFit, pendingOutputSpace } from './output-space.mjs';
const rectangle = (x, y, width, height) => Array.from({ length: width * height }, (_, i) => `${x + i % width},${y + Math.floor(i / width)}`);

test('a closed 26-cell strip cannot fit a 5x5 consumer despite passing area-only screening', () => {
  const cells = [...rectangle(36, 39, 11, 2), ...rectangle(36, 41, 4, 1)], size = [{ width: 5, height: 5 }];
  assert.equal(cells.length, 26);
  for (const y of [39, 40, 41]) assert.equal(pocketFit(cells, size, { x: 36, y }), null);
});

test('rectangle fit excludes the source connection and all body-blocking transport layers', () => {
  const cells = rectangle(2, 3, 4, 3), sizes = [{ width: 3, height: 3 }], source = { x: 2, y: 3 };
  assert.deepEqual(pocketFit(cells, sizes, source), { x: 3, y: 3, width: 3, height: 3 });
  assert.equal(pocketFit(cells, sizes, source, (x, y) => x === 4 && y === 4), null);
  assert.equal(pocketFit(rectangle(2, 3, 3, 3), sizes, source), null);
  assert.deepEqual(pocketFit(rectangle(2, 3, 3, 3), [{ width: 3, height: 2 }, { width: 2, height: 3 }], source),
    { x: 2, y: 4, width: 3, height: 2 });
});

function fixture() {
  const empty = new Set(rectangle(2, 2, 7, 2)), body = new Set();
  for (const cell of rectangle(1, 1, 9, 4)) if (!empty.has(cell)) body.add(cell);
  const occupied = { body, layers: { belt: new Set(), pipe: new Set() } };
  const w = { placements: [{ instance: 'source', x: 1, y: 1, width: 9, height: 4 }], transport_networks: [], logistics_components: [] };
  const graph = { nodes: [{ id: 'target', recipe: 'fixture-recipe' }], edges: [{ source: 'source', target: 'target', item: 'fixture-item', transport: 'belt' }] };
  const catalogs = { recipes: { recipes: [{ id: 'fixture-recipe', facility: 'fixture-machine' }] }, facilities: { facilities: [
    { id: 'fixture-machine', footprint: { width: 3, height: 3 }, allowed_rotations: [0, 90] }] } };
  const access = [{ instance: 'source', transport: 'belt', pending_items: [{ item: 'fixture-item' }], ports: [2, 3].map(y =>
    ({ port: `p${y}`, connection: { x: 2, y }, obstruction: null })) }];
  return { graph, w, catalogs, access, occupied };
}

test('closed pocket failure is cached by component; capped or open space is not claimed impossible', () => {
  const f = fixture(), before = JSON.stringify(f.access);
  const run = options => pendingOutputSpace(f.graph, f.w, f.catalogs, f.access, f.occupied, options);
  const r = run(); assert.equal(r.failures.length, 1); assert.equal(r.probes, 1); assert.equal(r.explored, 14);
  assert.equal(r.failures[0].target, 'target'); assert.equal(r.failures[0].ports.length, 2);
  assert.equal(run({ cellBudget: 3 }).failures.length, 0);
  f.occupied.body.delete('1,2'); assert.equal(run().failures.length, 0);
  assert.equal(JSON.stringify(f.access), before);
});

test('external exports cannot end in a closed pocket and only actual pending demands are checked', () => {
  const f = fixture(); f.graph.nodes = [];
  const run = () => pendingOutputSpace(f.graph, f.w, f.catalogs, f.access, f.occupied);
  assert.equal(run().failures[0].requires_escape, true);
  f.access[0].pending_items = []; assert.equal(run().probes, 0);
});
