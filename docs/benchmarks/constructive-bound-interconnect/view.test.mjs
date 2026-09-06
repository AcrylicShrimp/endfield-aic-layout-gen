import test from 'node:test';
import assert from 'node:assert/strict';
import vm from 'node:vm';
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

function viewportFixture() {
  const w = { placements: [{ instance: 'a', facility: 'geometry-only', x: 0, y: 0, width: 5, height: 5 },
    { instance: 'frame', facility: 'geometry-only', x: 30, y: 40, width: 5, height: 5 }], transport_networks: [], logistics_components: [] };
  const html = render({ witness: w, catalog_ports: { a: [], frame: [] } }, { recipe_terminals: [], covered_geometry_validated: true }, {});
  const node = () => ({ style: {}, attributes: {}, children: [], textContent: '', clientWidth: 1000,
    setAttribute(k, v) { this.attributes[k] = v; }, append(...children) { this.children.push(...children); },
    replaceChildren() { this.children = []; }, addEventListener() {} });
  const nodes = Object.fromEntries(['data', 'diagram', 'status', 'scope', 'explanation', 'fit', 'normal', 'overload', 'itemA', 'itemB', 'legend'].map(id => [id, node()]));
  nodes.data.textContent = html.match(/id="data">([^<]*)<\/script>/)[1];
  let notify;
  const context = vm.createContext({ document: { getElementById: id => nodes[id], createElement: node, createElementNS: node, querySelector: () => nodes.legend },
    ResizeObserver: class { constructor(callback) { notify = callback; } observe() {} } });
  vm.runInContext(html.match(/<\/script><script>([\s\S]*)<\/script>/)[1], context);
  return { nodes, notify, run: s => vm.runInContext(s, context) };
}

test('viewport focus survives height-only resize notifications and does not change diagram data', () => {
  const { nodes, notify, run } = viewportFixture(), full = nodes.diagram.attributes.viewBox, height = nodes.diagram.style.height, before = run('JSON.stringify(w)');
  assert.equal(height, '820px'); assert.equal(run('focusLabels(["a"])'), true);
  const focused = nodes.diagram.attributes.viewBox; assert.notEqual(focused, full);
  notify(); notify(); assert.equal(nodes.diagram.attributes.viewBox, focused); assert.equal(nodes.diagram.style.height, height);
  assert.equal(run('JSON.stringify(w)'), before); nodes.fit.onclick(); assert.equal(nodes.diagram.attributes.viewBox, full);
});

test('viewport redraws for a real width change but not its own height update', () => {
  const { nodes, notify, run } = viewportFixture(); nodes.diagram.clientWidth = 300; notify();
  const narrow = nodes.diagram.attributes.viewBox, height = parseFloat(nodes.diagram.style.height);
  assert.equal(Number(narrow.split(' ')[2]), 300); assert.ok(height > 0 && height < 820);
  run('focusLabels(["a"])'); const focused = nodes.diagram.attributes.viewBox; notify(); assert.equal(nodes.diagram.attributes.viewBox, focused);
  nodes.normal.onclick(); assert.equal(nodes.diagram.attributes.viewBox, narrow);
});
