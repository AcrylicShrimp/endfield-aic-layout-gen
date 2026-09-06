import test from 'node:test';
import assert from 'node:assert/strict';
import { usedBounds, render } from './view.mjs';
test('displayed used bounds exclude unused origin margin and include components and both layers', () => {
  const w = { placements: [{ x: 10, y: 20, width: 5, height: 3 }], transport_networks: [{ transport: 'pipe', cells: [{ x: 16, y: 20 }] }],
    logistics_components: [{ position: { x: 8, y: 24 } }] };
  const before = JSON.stringify(w);
  assert.deepEqual(usedBounds(w), { x: 8, y: 20, width: 9, height: 5 });
  const html = render({ witness: w, accepted: false }, {}, {}), data = JSON.parse(html.match(/id="data">([^<]*)<\/script>/)[1]);
  assert.deepEqual(data.used_bounds, usedBounds(w)); assert.match(html, /planner REJECTED/);
  assert.equal(JSON.stringify(w), before);
});
