import test from 'node:test';
import assert from 'node:assert/strict';
import { interfaceMapping } from './interfaces.mjs';
const items = { schema_version: 1, items: [{ id: 'a', transport: 'belt' }, { id: 'b', transport: 'belt' }] };
const transports = { schema_version: 1, transports: [{ kind: 'belt', capacity: { quantity: 1, duration_ms: 2000 } }] };
function fixture() {
  return { block_experiment: {}, placements: ['s1', 's2', 't'].map((id, index) => ({ instance: id, label: id, block: id,
    catalog_ports: ['input', 'output'].flatMap(direction => Array.from({ length: 3 }, (_, i) => ({ id: direction + i,
      direction, transport: 'belt', edge: direction === 'input' ? 'west' : 'east', connection: { x: index * 5, y: i } }))) })),
  internal_requirements: ['s1', 's2'].map(source => ({ id: source, source, target: 't', item: 'a', rate: { numerator: 1, denominator: 4 } })), external_requirements: [] };
}
test('two quarter-rate suppliers can share one half-rate input but have two separate edge lanes', () => {
  const r = interfaceMapping(fixture(), items, transports), t = r.terminal_groups.find(g => g.label === 't');
  assert.deepEqual(t.rate, { numerator: 1, denominator: 2 }); assert.equal(t.minimum_shared_lanes, 1); assert.equal(t.separate_edge_lanes, 2);
});
test('different items require distinct port budgets and multi-lane demand is rounded exactly', () => {
  const p = fixture(); p.internal_requirements[0].rate = { numerator: 1, denominator: 1 }; p.internal_requirements[1].item = 'b';
  const t = interfaceMapping(p, items, transports).facility_interfaces.find(g => g.label === 't');
  assert.equal(t.minimum_ports, 3); assert.equal(t.capacity_count_fits_catalog, true);
  p.internal_requirements[0].rate.numerator = 2;
  assert.equal(interfaceMapping(p, items, transports).facility_interfaces.find(g => g.label === 't').capacity_count_fits_catalog, false);
});
test('external and internal-chain obligations remain in terminal budgets', () => {
  const p = fixture(); p.placements[0].block = 't';
  p.external_requirements.push({ id: 'outside', source: 'outside', target: 't', item: 'a', rate: { numerator: 1, denominator: 4 } });
  const r = interfaceMapping(p, items, transports), t = r.terminal_groups.find(g => g.label === 't');
  assert.equal(t.requirements.length, 3); assert.equal(t.boundary_requirements.length, 2); assert.equal(t.minimum_shared_lanes, 2);
  assert.equal(r.pairs.length, 2);
});
