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
test('relaxed diagnostic remains separate from validated geometry and escapes owner text', () => {
  const w = { placements: [{ x: 0, y: 0, width: 1, height: 1 }], transport_networks: [], logistics_components: [] };
  const relaxed = [{ id: 1, cells: [{ x: 1, y: 0 }], blockers: [{ position: { x: 1, y: 0 }, owners: [{ instance: '</script>' }] }] }];
  const html = render({ witness: w, move_diagnostic: { moves: [], relaxed_routes: relaxed } }, {}, {});
  const data = JSON.parse(html.match(/id="data">([^<]*)<\/script>/)[1]);
  assert.deepEqual(data.detail.move_diagnostic.relaxed_routes, relaxed);
  assert.deepEqual(data.detail.witness, w); assert.match(html, /NOT a valid route/); assert.match(html, /data-terminal-obstruction/);
});
test('rotated proposal poses and world port stubs remain separate from the validated parent', () => {
  const w = { placements: [{ instance: 'a', x: 1, y: 1, width: 6, height: 4, rotation: 0 }], transport_networks: [], logistics_components: [] };
  const poses = [{ instance: 'a', x: 2, y: 3, width: 4, height: 6, rotation: 90, ports: [{ id: 'p', position: { x: 2, y: 3 }, connection: { x: 1, y: 3 } }] }];
  const html = render({ witness: w, move_diagnostic: { moves: [{ instances: ['a'], dx: 1, dy: 2 }], facility_poses: poses } }, {}, {});
  const data = JSON.parse(html.match(/id="data">([^<]*)<\/script>/)[1]);
  assert.deepEqual(data.detail.witness, w); assert.deepEqual(data.detail.move_diagnostic.facility_poses, poses); assert.match(html, /data-proposed-port/);
});
