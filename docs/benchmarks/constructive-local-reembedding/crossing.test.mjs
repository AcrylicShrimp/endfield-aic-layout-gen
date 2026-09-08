import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { crossingOptions } from './crossing.mjs';
import { shortestPath, key, distance } from './path.mjs';
const catalogs = { components: JSON.parse(fs.readFileSync(new URL('../../../data/game/normalized/logistics-components.json', import.meta.url))) };
const rate = { numerator: 1, denominator: 3 };
const carrier = { id: 3, network: 0, transport: 'belt', item: 'carrier', rate, cells: Array.from({ length: 5 }, (_, y) => ({ x: 2, y })) };
const job = { transport: 'belt', item: 'routed', rate };

test('crossing sites use catalog axes and capacity and reject bends, endpoints, self-carriers and anchors', () => {
  const before = structuredClone(carrier), options = crossingOptions([carrier], job, catalogs, new Set(['2,1']));
  assert.equal(options.size, 4); assert.ok(options.has('2,2:0')); assert.ok(options.has('2,2:2')); assert.ok(!options.has('2,2:1'));
  assert.equal(crossingOptions([carrier, carrier], job, catalogs, new Set()).size, 0);
  assert.equal(crossingOptions([carrier], { ...job, item: carrier.item }, catalogs, new Set()).size, 6);
  assert.equal(crossingOptions([carrier], { ...job, id: carrier.id }, catalogs, new Set()).size, 0);
  assert.equal(crossingOptions([carrier], { ...job, transport: 'pipe' }, catalogs, new Set()).size, 0);
  const bent = { ...carrier, cells: [{ x: 1, y: 1 }, { x: 2, y: 1 }, { x: 2, y: 2 }] };
  assert.equal(crossingOptions([bent], job, catalogs, new Set()).size, 0);
  const low = structuredClone(catalogs); low.components.components.forEach(c => { c.capacity = { quantity: 1, duration_ms: 100000 }; });
  assert.equal(crossingOptions([carrier], job, low, new Set()).size, 0);
  assert.equal(crossingOptions([{ ...carrier, rate: { numerator: 100, denominator: 1 } }], job, catalogs, new Set()).size, 0);
  const directed = structuredClone(catalogs); directed.components.components = directed.components.components.filter(c => c.kind === 'bridge' && c.transport === 'belt');
  for (const c of directed.components.components) { c.allowed_rotations = [90]; c.input_directions = ['west', 'north']; c.output_directions = ['east', 'south']; }
  const poses = crossingOptions([carrier], job, directed, new Set());
  assert.equal(poses.size, 3); assert.ok([...poses.values()].every(p => p.rotation === 90 && p.direction === 2));
  assert.deepEqual(carrier, before);
});

test('preplanned bridge states cross barriers with unit cells, straight axes and bounded resources', () => {
  const bounds = { minX: 0, minY: 0, maxX: 4, maxY: 4 }, start = { x: 0, y: 2 }, end = { x: 4, y: 2 };
  const blocked = new Set(carrier.cells.map(key)), options = crossingOptions([carrier], job, catalogs, new Set());
  assert.equal(shortestPath(start, end, blocked, bounds).status, 'no-path');
  const r = shortestPath(start, end, blocked, bounds, { crossingOptions: options, crossingLimit: 1 });
  assert.equal(r.status, 'found'); assert.equal(r.crossings.length, 1); assert.deepEqual([r.steps, r.turns], [4, 0]);
  assert.ok(r.cells.slice(1).every((p, i) => distance(r.cells[i], p) === 1));
  assert.deepEqual(r.crossings[0].position, { x: 2, y: 2 });
  assert.equal(shortestPath(start, end, blocked, bounds, { crossingOptions: options, crossingLimit: 0 }).status, 'no-path');
  const malformed = new Map(options); malformed.set('2,2:0', { position: { x: 99, y: 99 }, direction: 0 });
  assert.throws(() => shortestPath(start, end, blocked, bounds, { crossingOptions: malformed, crossingLimit: 1 }), { code: 'path-invalid-crossing' });
  assert.throws(() => shortestPath(start, end, blocked, bounds, { crossingLimit: 9 }), { code: 'path-invalid-input' });
});

test('two crossing barriers require two budgeted modules and never permit a turn within a bridge', () => {
  const second = { ...carrier, id: 4, network: 1, item: 'another', cells: carrier.cells.map(p => ({ ...p, x: 4 })) };
  const bounds = { minX: 0, minY: 0, maxX: 6, maxY: 4 }, start = { x: 0, y: 2 }, end = { x: 6, y: 2 };
  const blocked = new Set([...carrier.cells, ...second.cells].map(key)), options = crossingOptions([carrier, second], job, catalogs, new Set());
  assert.equal(shortestPath(start, end, blocked, bounds, { crossingOptions: options, crossingLimit: 1 }).status, 'no-path');
  const r = shortestPath(start, end, blocked, bounds, { crossingOptions: options, crossingLimit: 2 });
  assert.equal(r.status, 'found'); assert.equal(r.crossings.length, 2); assert.deepEqual([r.steps, r.turns], [6, 0]);
  for (const c of r.crossings) { const i = r.cells.findIndex(p => key(p) === key(c.position)); assert.equal(r.cells[i - 1].y, r.cells[i + 1].y); }
});

test('adjacent catalog bridge cells do not require an invented spacing tile', () => {
  const second = { ...carrier, id: 4, network: 1, item: 'another', cells: carrier.cells.map(p => ({ ...p, x: 3 })) };
  const blocked = new Set([...carrier.cells, ...second.cells].map(key)), options = crossingOptions([carrier, second], job, catalogs, new Set());
  const r = shortestPath({ x: 0, y: 2 }, { x: 5, y: 2 }, blocked, { minX: 0, minY: 0, maxX: 5, maxY: 4 }, { crossingOptions: options, crossingLimit: 2 });
  assert.equal(r.status, 'found'); assert.equal(r.crossings.length, 2); assert.deepEqual([r.steps, r.turns], [5, 0]);
  assert.equal(distance(r.crossings[0].position, r.crossings[1].position), 1);
});
