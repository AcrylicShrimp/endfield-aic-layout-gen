import fs from 'node:fs';
import assert from 'node:assert/strict';
import test from 'node:test';
import { reconstruct, verifyReferenceMargins } from './model.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const spec = read('./medium-battery-01.json');
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k,
  read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const buildings = read('../../../data/game/source/table-cfg/FactoryBuildingTable.json');
const citron = read('./citron-powder-03.json'), iron = read('./enriched-iron-01.json');
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
test('split seed routes use explicit rates without duplicating the full source output', () => {
  const d = reconstruct(citron, catalogs, {});
  assert.equal(d.witness.placements.length, 4);
  assert.deepEqual(d.reconstruction.metrics.bounds, { x: 0, y: 0, width: 10, height: 12 });
  const seed = d.witness.transport_networks.find(n => n.item === 'item-plant-moss-seed-2');
  assert.equal(seed.terminals.filter(t => t.direction === 'output').length, 2);
  assert.ok(seed.terminals.every(t => t.rate.numerator === 1 && t.rate.denominator === 2));
});
test('mixed utilization preserves the observed nine-facility iron inventory', () => {
  const d = reconstruct(iron, catalogs, {});
  assert.equal(d.witness.placements.length, 9);
  assert.equal(d.reconstruction.metrics.area, 208);
  assert.deepEqual(d.witness.utilizations.seed, { numerator: 1, denominator: 3 });
  assert.deepEqual(d.witness.utilizations.thickener, { numerator: 1, denominator: 1 });
  assert.deepEqual(d.reconstruction.routes.find(r => r.id === 'product-seed').rate, { numerator: 1, denominator: 6 });
  assert.deepEqual(d.reconstruction.routes.find(r => r.id === 'sand-thickener').rate, { numerator: 1, denominator: 2 });
});
test('schema 1, implicit rates, invalid rationals and overallocated margins reject', () => {
  for (const mutate of [s => s.schema_version = 1, s => s.utilization_assumption = { numerator: 1, denominator: 1 },
    s => delete s.placements[0].utilization, s => delete s.routes[0].rate, s => delete s.routes[0].item,
    s => s.placements[0].utilization.numerator = 2, s => s.routes[0].rate.denominator = 0,
    s => s.routes[0].rate.numerator = 0.1, s => s.routes[0].rate.numerator = Number.MAX_SAFE_INTEGER + 1,
    s => s.routes[0].rate = { numerator: 1, denominator: 1 }, s => delete s.expected_pending]) {
    const s = structuredClone(citron); mutate(s);
    assert.throws(() => reconstruct(s, catalogs, {}), { code: 'reconstruction-input' });
  }
  const s = structuredClone(citron);
  // Each downstream planter could accept this amount; together they exceed seed utilization.
  s.placements[0].utilization = { numerator: 3, denominator: 4 };
  s.routes[0].rate = { numerator: 3, denominator: 8 };
  assert.throws(() => reconstruct(s, catalogs, {}), /exceed the utilized recipe margin/);
});
test('reference acceptance checks exact expected margins beyond geometry validation', () => {
  const v = { covered_geometry_validated: true, recipe_terminals: citron.expected_pending.map(t => ({ ...t, pending_rate: t.rate })) };
  assert.equal(verifyReferenceMargins(citron, v).status, 'internal-reference-validated');
  const equivalent = structuredClone(v); equivalent.recipe_terminals[0].pending_rate = { numerator: 2, denominator: 2 };
  assert.equal(verifyReferenceMargins(citron, equivalent).status, 'internal-reference-validated');
  for (const mutate of [v => v.covered_geometry_validated = false,
    v => v.recipe_terminals[0].pending_rate = { numerator: 1, denominator: 2 },
    v => v.recipe_terminals.push({ instance: 'plant-product', direction: 'input', item: 'item-plant-moss-seed-2', pending_rate: { numerator: 1, denominator: 2 } }),
    v => v.recipe_terminals.push(v.recipe_terminals[0])]) {
    const changed = structuredClone(v); mutate(changed);
    assert.throws(() => verifyReferenceMargins(citron, changed), { code: 'reconstruction-input' });
  }
});
