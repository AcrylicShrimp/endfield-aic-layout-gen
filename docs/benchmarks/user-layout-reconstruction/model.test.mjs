import fs from 'node:fs';
import assert from 'node:assert/strict';
import test from 'node:test';
import { reconstruct } from './model.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const spec = read('./medium-battery-01.json');
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k,
  read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const buildings = read('../../../data/game/source/table-cfg/FactoryBuildingTable.json');
test('transcribes eight poses and seven routes without moving or filling boundaries', () => {
  const before = JSON.stringify({ spec, catalogs, buildings }), d = reconstruct(spec, catalogs, buildings);
  assert.equal(JSON.stringify({ spec, catalogs, buildings }), before);
  assert.equal(d.factory_validated, false);
  assert.equal(d.witness.placements.length, 8);
  assert.equal(d.reconstruction.routes.length, 7);
  assert.deepEqual(d.reconstruction.metrics, { bounds: { x: 0, y: 0, width: 10, height: 15 }, area: 150, facility_cells: 87, physical_belt_cells: 32, turns_including_port_arms: 7, annotation_cells: 4 });
  assert.deepEqual(d.reconstruction.routes.map(r => r.turns_including_port_arms), [0, 0, 1, 1, 1, 1, 3]);
  assert.deepEqual(d.reconstruction.routes.map(r => r.physical_cells), [1, 1, 10, 6, 6, 4, 4]);
  assert.ok(d.witness.transport_networks.every(n => n.terminals.every(t => t.endpoint.kind === 'facility')));
  assert.ok(d.reconstruction.routes.every(r => r.rate.numerator === 1 && r.rate.denominator === 2));
  assert.deepEqual(d.witness.placements.map(p => [p.instance, p.x, p.y, p.rotation]), spec.placements.map(p => [p.id, ...p.position, p.rotation]));
});
test('annotation uses source footprint without becoming a production facility', () => {
  const changed = structuredClone(buildings); changed.power_diffuser_1.range.width = 1;
  const a = reconstruct(spec, catalogs, buildings), b = reconstruct(spec, catalogs, changed);
  assert.deepEqual(a.witness, b.witness);
  assert.equal(b.reconstruction.annotations[0].width, 1);
  assert.equal(b.reconstruction.annotations[0].validated_by_witness, false);
});
test('bad poses and port reuse reject instead of being repaired', () => {
  for (const mutate of [s => { s.placements[0].rotation = 45; }, s => { s.placements[0].recipe = 'missing'; },
    s => { s.placements[1].id = 'F1'; }, s => { s.routes[1].source = s.routes[0].source; }, s => { s.routes[0].corners = [[4, 1]]; }]) {
    const s = structuredClone(spec); mutate(s); assert.throws(() => reconstruct(s, catalogs, buildings), { code: 'reconstruction-input' });
  }
});
test('separate annotation overlap and missing source footprint are explicit failures', () => {
  const s = structuredClone(spec); s.annotations[0].position = [0, 0];
  assert.throws(() => reconstruct(s, catalogs, buildings), { code: 'reconstruction-input' });
  assert.throws(() => reconstruct(spec, catalogs, {}), { code: 'reconstruction-input' });
});
