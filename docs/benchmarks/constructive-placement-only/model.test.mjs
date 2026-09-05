import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { placeOnly, rotateFacility } from './model.mjs';
import { render } from './render.mjs';
const base = JSON.parse(fs.readFileSync(new URL('../../../data/examples/constructive-network.facilities.json', import.meta.url)));
const items = { schema_version: 1, items: [{ id: 'solid', transport: 'belt' }, { id: 'fluid', transport: 'pipe' }] };
const wiring = pairs => ({ schema_version: 1, success: true,
  nodes: [...new Set(pairs.flatMap(p => p.slice(0, 2)))].map(id => ({ id, kind: 'facility', recipe: 'fixture', facility: base.facilities[0].id })),
  edges: pairs.map(([source, target, item = 'solid'], i) => ({ id: `edge-${i}`, source, target, item,
    rate: { numerator: 1, denominator: 5 } })),
});
test('one-to-one chains remain top-aligned with one empty cell despite port offsets', () => {
  const catalog = structuredClone(base), f = catalog.facilities[0];
  f.footprint = { width: 3, height: 5 };
  f.ports[0].position = { x: 0, y: 4 };
  f.ports[1].position = { x: 2, y: 0 };
  const w = wiring([['a', 'b'], ['b', 'c'], ['c', 'd']]);
  const r = placeOnly(w, catalog, items, 'd');
  assert.equal(r.placements.length, 4);
  assert.equal(new Set(r.placements.map(p => p.y)).size, 1);
  for (let i = 1; i < 4; i++) assert.equal(r.placements[i].x + r.placements[i].width + 1, r.placements[i - 1].x);
  assert.equal(r.routing_performed, false);
  assert.equal(r.factory_validated, false);
  assert.deepEqual(r.transport_networks, []);
  assert.deepEqual(r, placeOnly(w, catalog, items, 'd'));
});
test('breadth-first belts precede pipes; branches slide only to clear rows', () => {
  const w = wiring([['pipe', 'target', 'fluid'], ['belt', 'target'], ['upstream', 'belt'], ['other', 'target']]);
  const r = placeOnly(w, base, items, 'target');
  assert.deepEqual(r.placements.map(p => p.instance), ['target', 'belt', 'other', 'pipe', 'upstream']);
  assert.equal(r.placement_decisions[1].row_shift, 0);
  assert.notEqual(r.placement_decisions[2].row_shift, 0);
  assert.equal(r.placement_decisions[4].row_shift, 0);
  assert.equal(r.placement_geometry_valid, true);
});
test('cycles, shared consumers and disconnected components all terminate with full coverage', () => {
  const w = wiring([['a', 'b'], ['b', 'a'], ['a', 'c'], ['d', 'e'], ['self', 'self']]);
  const r = placeOnly(w, base, items, 'c');
  assert.equal(r.placements.length, w.nodes.length);
  assert.equal(new Set(r.placements.map(p => p.instance)).size, w.nodes.length);
  assert(r.non_forward_requirements.length >= 2);
  assert.deepEqual(r, placeOnly(w, base, items, 'c'));
});
test('belt-input orientation wins over pipe output and mixed faces remain explicit', () => {
  const catalog = structuredClone(base), f = catalog.facilities[0];
  f.ports[0].edge = 'south';
  f.ports[1].transport = 'pipe';
  const r = placeOnly(wiring([['a', 'b'], ['b', 'c', 'fluid']]), catalog, items, 'c');
  const p = r.placements.find(p => p.instance === 'b');
  assert.equal(p.rotation, 90);
  assert.equal(p.alignment.belt_input.aligned, 1);
  assert.equal(p.alignment.pipe_output.aligned, 0);
  assert(r.diagnostics.some(d => d.entity === 'b' && d.code === 'placement-probe-mixed-port-faces'));
});
test('non-square rotations preserve catalog edge positions', () => {
  const f = { footprint: { width: 3, height: 5 }, ports: [{ id: 'p', position: { x: 2, y: 4 }, edge: 'east' }] };
  const r = rotateFacility(f, 90);
  assert.deepEqual([r.width, r.height, r.ports[0].position, r.ports[0].edge], [5, 3, { x: 0, y: 2 }, 'south']);
});
test('invalid references and schemas fail explicitly without inventing catalog geometry', () => {
  const w = wiring([['a', 'b']]);
  assert.throws(() => placeOnly(w, base, items, 'missing'), { code: 'placement-probe-target-invalid' });
  assert.throws(() => placeOnly({ ...w, schema_version: 2 }, base, items, 'a'), { code: 'placement-probe-schema-invalid' });
  const invalid = structuredClone(w); invalid.edges[0].item = 'missing';
  assert.throws(() => placeOnly(invalid, base, items, 'a'), { code: 'placement-probe-edge-invalid' });
  const c = structuredClone(base); c.facilities[0].ports[0].position.x = -1;
  assert.throws(() => placeOnly(w, c, items, 'a'), { code: 'placement-probe-port-invalid' });
});
test('HTML labels unrouted geometry and safely embeds instance IDs', () => {
  const r = placeOnly(wiring([['a</script>', 'b']]), base, items, 'b');
  const html = render(r);
  assert(html.includes('NOT A VALIDATED FACTORY'));
  assert(!html.includes('a</script>'));
  assert(html.includes('Logical edges (NOT routes)'));
  assert(!/id="show-graph"[^>]*checked/.test(html));
});
