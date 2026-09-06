import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { planComponentPoses, componentPoseCandidates } from './components.mjs';
import { movementPlan } from './movement.mjs';
import { extractChains, extentOf, reembed } from './model.mjs';
import { directions, key } from './path.mjs';
const catalogs = { components: JSON.parse(fs.readFileSync(new URL('../../../data/game/normalized/logistics-components.json', import.meta.url))), facilities: { facilities: [] } };
const axis = { east: 0, south: 1, west: 2, north: 3 };

function fixture() {
  // Geometry-only star; component arms/rotation come from the runtime catalog.
  const def = catalogs.components.components.find(c => c.kind === 'splitter' && c.transport === 'belt');
  const rotation = def.allowed_rotations[0], position = { x: 5, y: 5 }, paths = [], terminals = [];
  for (const [input, names] of [[true, def.input_directions], [false, def.output_directions]]) for (const name of names) {
    const d = directions[(axis[name] + rotation / 90) % 4];
    const cells = Array.from({ length: 6 }, (_, i) => ({ x: position.x + d[0] * i, y: position.y + d[1] * i }));
    const endpoint = cells.at(-1), id = `t${terminals.length}`;
    const rate = { numerator: 1, denominator: input ? 1 : def.output_directions.length };
    terminals.push({ id, position: endpoint, direction: input ? 'output' : 'input', rate, endpoint: { kind: 'external', node: id } });
    paths.push({ cells: input ? cells.reverse() : cells, rate });
  }
  return { labels: {}, witness: { placements: [], logistics_components: [{ id: 'junction', component: def.id, kind: def.kind, transport: def.transport, rotation, position }],
    transport_networks: [{ id: 'star', item: 'geometry-only', transport: 'belt', terminals, component_ids: ['junction'], requirement_ids: [],
      cells: [...new Map(paths.flatMap(p => p.cells).map(p => [key(p), p])).values()],
      segments: paths.flatMap(p => p.cells.slice(1).map((q, i) => ({ from: p.cells[i], to: q, rate: p.rate }))) }] } };
}

test('component poses validate identity, uniqueness and catalog rules without mutation', () => {
  const { witness: w } = fixture(), before = structuredClone(w), c = w.logistics_components[0];
  const pose = { id: c.id, ...c.position, rotation: c.rotation };
  assert.equal(planComponentPoses(w, catalogs, [pose]).size, 1);
  assert.equal(reembed({ witness: w, labels: {} }, catalogs, { componentPoses: [pose] }).status, 'unchanged');
  for (const invalid of [null, [pose, pose], [{ ...pose, id: 'absent' }], [{ ...pose, rotation: 45 }], [{ ...pose, x: NaN }], Array(9).fill(pose)])
    assert.throws(() => planComponentPoses(w, catalogs, invalid), { code: 'local-component-pose' });
  const wide = structuredClone(catalogs); wide.components.components.find(d => d.id === c.component).footprint.width = 2;
  assert.throws(() => planComponentPoses(w, wide, [pose]), { code: 'local-component-pose' });
  assert.deepEqual(w, before);
});

test('in-place rotation invalidates incident chains and rotates both directed endpoint arms', () => {
  const detail = fixture(), w = detail.witness, before = structuredClone(detail), { chains, anchors } = extractChains(w);
  const pose = { id: 'junction', x: 5, y: 5, rotation: 90 }, changes = planComponentPoses(w, catalogs, [pose]);
  const m = movementPlan(w, chains, anchors, [], { componentChanges: changes });
  assert.equal(m.changed.size, chains.length); assert.equal(m.fixedChains.size, 0);
  assert.equal(m.anchorTurns.get('belt:5,5'), 1);
  for (const c of chains) {
    const result = reembed(detail, catalogs, { componentPoses: [pose], routePriority: [c.id], endpointPolicy: 'terminal-flexible' });
    const first = result.routes[0]; assert.equal(first.id, c.id);
    if (key(c.cells[0]) === '5,5') assert.equal(first.first, (c.first + 1) % 4);
    else assert.equal(first.last, (c.last + 1) % 4);
  }
  assert.deepEqual(detail, before);
});

test('translated junction reroutes its star, rejects collisions and leaves semantic margins intact', () => {
  const detail = fixture(), pose = { id: 'junction', x: 6, y: 5, rotation: 0 };
  const other = structuredClone(detail.witness.transport_networks[0]);
  other.id = 'unselected-layer'; other.transport = 'pipe'; other.component_ids = [];
  detail.witness.transport_networks.push(other);
  const before = structuredClone(detail);
  const r = reembed(detail, catalogs, { componentPoses: [pose], endpointPolicy: 'terminal-flexible' });
  assert.equal(r.status, 'candidate'); assert.equal(r.detail.factory_validated, false);
  assert.deepEqual(r.detail.witness.logistics_components[0].position, { x: 6, y: 5 });
  const n = r.detail.witness.transport_networks[0], old = detail.witness.transport_networks[0];
  assert.deepEqual(n.terminals, old.terminals); assert.deepEqual(n.component_ids, old.component_ids);
  assert.deepEqual(n.requirement_ids, old.requirement_ids);
  assert.deepEqual(r.detail.witness.transport_networks[1], other);
  assert.deepEqual(detail, before);
  assert.throws(() => reembed(detail, catalogs, { componentPoses: [{ ...pose, x: 5, y: 0 }] }), { code: 'local-terminal-collision' });
  assert.throws(() => reembed(detail, catalogs, { componentPoses: [{ ...pose, x: 11 }] }), { code: 'local-move-bounds' });
  detail.witness.placements.push({ instance: 'obstacle', x: 6, y: 5, width: 1, height: 1 });
  assert.throws(() => reembed(detail, catalogs, { componentPoses: [pose] }), { code: 'local-endpoint-body' });
});

test('explicit junction pose overrides implicit downstream translation in one final-state plan', () => {
  const { witness: w } = fixture();
  w.placements.push({ instance: 'consumer', x: 0, y: 0, width: 1, height: 1 });
  for (const t of w.transport_networks[0].terminals) if (t.direction === 'input') t.endpoint = { kind: 'facility', instance: 'consumer' };
  const { chains, anchors } = extractChains(w), moves = [{ instances: ['consumer'], dx: -1, dy: 0 }];
  const implicit = movementPlan(w, chains, anchors, moves, { junctionPolicy: 'downstream' });
  assert.deepEqual(implicit.shiftAnchor('belt', { x: 5, y: 5 }), { x: 4, y: 5 });
  const changes = planComponentPoses(w, catalogs, [{ id: 'junction', x: 6, y: 6, rotation: 90 }]);
  const explicit = movementPlan(w, chains, anchors, moves, { junctionPolicy: 'downstream', componentChanges: changes });
  assert.deepEqual(explicit.shiftAnchor('belt', { x: 5, y: 5 }), { x: 6, y: 6 });
  assert.equal(explicit.changed.size, chains.length); assert.equal(explicit.fixedChains.size, 0);
  const pin = planComponentPoses(w, catalogs, [{ id: 'junction', x: 5, y: 5, rotation: 0 }]);
  const pinned = movementPlan(w, chains, anchors, moves, { junctionPolicy: 'downstream', componentChanges: pin });
  assert.deepEqual(pinned.shiftAnchor('belt', { x: 5, y: 5 }), { x: 5, y: 5 });
});

test('pose proposals are bounded, deterministic, catalog-backed and exclude blocked arms', () => {
  const { witness: w } = fixture(), { chains } = extractChains(w), bounds = extentOf(w);
  w.placements.push({ instance: 'obstacle', x: 6, y: 6, width: 1, height: 1 });
  const before = structuredClone(w), a = componentPoseCandidates(w, chains, catalogs, [], bounds, { limit: 12 });
  assert.ok(a.examined > a.candidates.length); assert.equal(a.candidates.length, 12);
  assert.deepEqual(a, componentPoseCandidates(w, chains, catalogs, [], bounds, { limit: 12 }));
  assert.deepEqual(componentPoseCandidates(w, chains, catalogs, [], bounds, { affectedOnly: true }).candidates, []);
  for (const p of a.candidates) {
    const q = p.componentPoses[0]; assert.ok(q.x >= 0 && q.y >= 0 && q.x <= 10 && q.y <= 10);
    assert.notEqual(`${q.x},${q.y}`, '6,6');
    assert.ok(directions.every(d => `${q.x + d[0]},${q.y + d[1]}` !== '6,6'));
    assert.equal(planComponentPoses(w, catalogs, [q]).size, 1);
  }
  assert.deepEqual(w, before);
  assert.throws(() => componentPoseCandidates(w, chains, catalogs, [], bounds, { limit: 0 }), { code: 'local-component-pose' });
});
