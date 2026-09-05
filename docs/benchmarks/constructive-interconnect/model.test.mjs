import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { census } from './census.mjs';
import { permutationModule, checkGeometry } from './geometry.mjs';
import { recipeInterface } from './fixture.mjs';
import { dto, capacity, rate, lanesFor } from './rates.mjs';
const load = n => JSON.parse(fs.readFileSync(new URL('../../../data/game/normalized/' + n + '.json', import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'items', 'facilities', 'transports', 'components'].map(k => [k, load(k === 'components' ? 'logistics-components' : k)]));
const beltItems = catalogs.items.items.filter(i => i.transport === 'belt');
const cap = capacity(catalogs.transports.transports.find(t => t.kind === 'belt').capacity);
const lane = (id, item) => ({ id, item: item ?? beltItems[Number(id)].id, transport: 'belt', rate: dto(cap) });
function* permutations(xs) { if (!xs.length) yield []; else for (let i = 0; i < xs.length; i++) for (const rest of permutations(xs.filter((_, j) => i !== j))) yield [xs[i], ...rest]; }
test('every distinct-item permutation through six lanes passes independent cell checking', () => {
  let checked = 0;
  for (let n = 1; n <= 6; n++) {
    const lanes = Array.from({ length: n }, (_, i) => lane(String(i)));
    for (const target of permutations(lanes.map(l => l.id))) {
      const g = permutationModule(lanes, target, catalogs);
      assert.equal(checkGeometry(g, catalogs).valid, true);
      assert.deepEqual(g.lanes.map(l => l.rate), lanes.map(l => l.rate));
      const inversions = target.reduce((s, t, i) => s + target.slice(i + 1).filter(u => Number(u) < Number(t)).length, 0);
      assert.equal(g.modules.length, inversions); checked++;
    }
  }
  assert.equal(checked, 873);
});
test('stable same-commodity lanes reorder across other items without self-crossing', () => {
  const a = beltItems[0].id, b = beltItems[1].id;
  const lanes = [lane('0', a), lane('1', a), lane('2', b), lane('3', b)];
  const g = permutationModule(lanes, ['0', '2', '1', '3'], catalogs);
  assert.equal(g.modules.length, 1);
  assert.throws(() => permutationModule(lanes, ['1', '0', '2', '3'], catalogs), e => e.code === 'interconnect-same-item-order');
});
test('all binary commodity words through six lanes respect stable item order', () => {
  for (let n = 2; n <= 6; n++) for (let count = 1; count < n; count++) {
    const lanes = Array.from({ length: n }, (_, i) => lane(String(i), beltItems[i < count ? 0 : 1].id));
    for (let mask = 0; mask < 2 ** n; mask++) {
      const bits = Array.from({ length: n }, (_, i) => mask >> i & 1);
      if (bits.filter(Boolean).length !== count) continue;
      let a = 0, b = count;
      const target = bits.map(bit => String(bit ? a++ : b++));
      assert.equal(permutationModule(lanes, target, catalogs).certificate.valid, true);
    }
  }
});
test('catalog and geometry mutations are independently rejected', () => {
  const g = permutationModule([lane('0'), lane('1')], ['1', '0'], catalogs);
  const bad = change => { const copy = structuredClone(g); change(copy); assert.equal(checkGeometry(copy, catalogs).valid, false); };
  bad(c => { c.components = []; });
  bad(c => { c.components[0].rotation = 45; });
  bad(c => { c.components[0].component = 'unknown'; });
  bad(c => { c.components[0].position.x++; });
  bad(c => { c.modules = []; });
  bad(c => { c.modules[0].width = c.width + 1; });
  bad(c => { c.modules[0].lanes[0] = 'unknown'; });
  bad(c => { c.lanes[1].id = c.lanes[0].id; });
  bad(c => { c.lanes[0].cells.splice(1, 1); });
  bad(c => { c.lanes[0].rate.numerator *= 2; });
  bad(c => { c.lanes[0].item = 'unknown'; });
  bad(c => { c.target_order.reverse(); });
  const lower = structuredClone(catalogs);
  lower.components.components.find(c => c.transport === 'belt' && c.kind === 'bridge').capacity.duration_ms *= 2;
  assert.equal(checkGeometry(g, lower).valid, false);
  assert.throws(() => permutationModule([lane('0'), lane('1')], ['1', '0'], catalogs, 0), e => e.code === 'interconnect-swap-budget');
  const wide = Array.from({ length: 31 }, (_, i) => lane(String(i)));
  assert.throws(() => permutationModule(wide, wide.map(l => l.id).reverse(), catalogs), e => e.code === 'interconnect-cell-budget');
});
test('pipe geometry uses the pipe catalog and mixed layers are rejected', () => {
  const items = catalogs.items.items.filter(i => i.transport === 'pipe').slice(0, 2);
  const pipeCap = capacity(catalogs.transports.transports.find(t => t.kind === 'pipe').capacity);
  const lanes = items.map((i, j) => ({ id: String(j), item: i.id, transport: 'pipe', rate: dto(pipeCap) }));
  assert.equal(permutationModule(lanes, ['1', '0'], catalogs).certificate.valid, true);
  lanes[0] = lane('0');
  assert.throws(() => permutationModule(lanes, ['1', '0'], catalogs), e => e.code === 'interconnect-mixed-layer');
});
test('census distinguishes two recipe items from five full-rate belt lanes', () => {
  const r = census(catalogs);
  assert.equal(r.counts.recipes, 305); assert.equal(r.maxima.input_distinct_items.maximum, 2);
  assert.equal(r.maxima.input_belt_minimum_lanes.maximum, 5);
  assert.equal(r.replicated_group_crossing_upper_bound, null);
  assert.equal(lanesFor(rate(1, 3), rate(1, 2)), 1);
  assert.equal(lanesFor(rate(3, 2), rate(1, 2)), 3);
  const spec = JSON.parse(fs.readFileSync(new URL('./fixture.json', import.meta.url)));
  const small = recipeInterface(r.recipes.find(r => r.recipe === spec.minimum_case.recipe), spec.minimum_case, catalogs);
  assert.equal(small.geometry.lanes.length, 4); assert.equal(small.geometry.modules.length, 1);
  const row = r.recipes.find(row => row.recipe === r.maxima.input_belt_minimum_lanes.witnesses[0]);
  const large = recipeInterface(row, spec.catalog_max_case, catalogs);
  assert.equal(large.geometry.lanes.length, 10); assert.equal(large.fanout_fanin_realized, false);
});
test('census rejects unknown items, absent facilities and invalid capacities', () => {
  const bad = change => { const c = structuredClone(catalogs); change(c); assert.throws(() => census(c)); };
  bad(c => { c.recipes.recipes[0].inputs[0].item = 'unknown'; });
  bad(c => { c.facilities.facilities = []; });
  bad(c => { c.transports.transports[0].capacity.quantity = 0; });
});
