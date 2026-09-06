import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { rigidBlockPose, blockRotationGroups, blockRotationCandidates } from './block-rotation.mjs';
import { planFacilityRotations } from './facility-rotation.mjs';
import { movementPlan, bodyCells } from './movement.mjs';
import { extractChains, extentOf } from './model.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { key } from './path.mjs';
const catalogs = Object.fromEntries(['facilities', 'components', 'recipes'].map(k => [k, JSON.parse(fs.readFileSync(new URL(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`, import.meta.url)))]));
const spec = catalogs.facilities.facilities.find(f => f.footprint.width !== f.footprint.height && [0, 90, 180, 270].every(r => f.allowed_rotations.includes(r)));
function placement(instance, x, y, rotation = 0) {
  const shape = rotateFacility(spec, rotation); return { instance, facility: spec.id, x, y, rotation, width: shape.width, height: shape.height };
}
function fixture() {
  const h = spec.footprint.height, w = { placements: [placement('a', 10, 10), placement('b', 10, 10 + h), placement('c', 10, 10 + 2 * h), placement('frame', 30 + 4 * spec.footprint.width, 0)],
    logistics_components: [], transport_networks: [] };
  const graph = { nodes: w.placements.map(p => ({ id: p.instance })), edges: [['b', 'a'], ['b', 'c']].map(([source, target]) => ({ source, target, internal: true })) };
  return { detail: { labels: {}, witness: w }, graph };
}
test('rigid group quarter turns preserve every body cell and world port under a common transform', () => {
  const w = { placements: [placement('a', 10, 10), placement('b', 21, 16, 90), placement('c', 12, 30, 180)], transport_networks: [], logistics_components: [] }, before = structuredClone(w);
  const minX = Math.min(...w.placements.map(p => p.x)), minY = Math.min(...w.placements.map(p => p.y));
  const oldWidth = Math.max(...w.placements.map(p => p.x + p.width)) - minX, oldHeight = Math.max(...w.placements.map(p => p.y + p.height)) - minY, oldPorts = project(w, catalogs);
  for (const turns of [1, 2, 3]) {
    const origin = { x: 40, y: 30 }, r = rigidBlockPose(w, catalogs, ['a', 'b', 'c'], turns, origin), projected = project({ ...w, placements: r.placements }, catalogs);
    const transform = p => { let x = p.x - minX, y = p.y - minY, width = oldWidth, height = oldHeight;
      for (let i = 0; i < turns; i++) { [x, y] = [height - 1 - y, x]; [width, height] = [height, width]; }
      return { x: origin.x + x, y: origin.y + y }; };
    assert.deepEqual(new Set(r.placements.flatMap(bodyCells).map(key)), new Set(w.placements.flatMap(bodyCells).map(transform).map(key)));
    for (const p of w.placements) for (const old of oldPorts[p.instance]) {
      const q = projected[p.instance].find(q => q.id === old.id);
      assert.deepEqual(q.position, transform(old.position)); assert.deepEqual(q.connection, transform(old.connection));
      assert.equal(q.direction, old.direction); assert.equal(q.transport, old.transport);
    }
    const changes = planFacilityRotations(w, catalogs, r.moves, r.facilityRotations);
    for (const p of r.placements) assert.deepEqual(changes.get(p.instance).pose, { x: p.x, y: p.y, width: p.width, height: p.height, rotation: p.rotation });
    assert.ok(r.moves.length <= 3);
  }
  assert.deepEqual(w, before);
});
test('rigid transforms reject invalid membership, origins, turns and unsupported catalog angles', () => {
  const { detail: { witness: w } } = fixture(), origin = { x: 10, y: 10 };
  for (const ids of [[], ['a'], ['a', 'a'], ['a', 'unknown'], ['a', 'b', 'c', 'frame', 'fifth']])
    assert.throws(() => rigidBlockPose(w, catalogs, ids, 1, origin), { code: 'block-rotation-input' });
  for (const turns of [0, 4, 1.5]) assert.throws(() => rigidBlockPose(w, catalogs, ['a', 'b'], turns, origin), { code: 'block-rotation-input' });
  assert.throws(() => rigidBlockPose(w, catalogs, ['a', 'b'], 1, { x: 1.5, y: 0 }), { code: 'block-rotation-input' });
  const restricted = structuredClone(catalogs); restricted.facilities.facilities.find(f => f.id === spec.id).allowed_rotations = [0];
  assert.throws(() => rigidBlockPose(w, restricted, ['a', 'b'], 1, origin), { code: 'block-rotation-catalog' });
});
test('aligned-run discovery includes connected triples but never couples an unrelated adjacent facility', () => {
  const { detail: { witness: w }, graph } = fixture();
  const groups = blockRotationGroups(w, graph, catalogs);
  assert.ok(groups.some(g => g.kind === 'aligned-y' && g.instances.join() === 'a,b,c'));
  assert.ok(groups.every(g => !g.instances.includes('frame')));
  graph.edges = graph.edges.filter(e => e.source !== 'b');
  assert.ok(blockRotationGroups(w, graph, catalogs).every(g => !g.instances.includes('c')));
});
test('block and boundary poses share one bounded simultaneous transaction with immutable deterministic proposals', () => {
  const { detail, graph } = fixture(), before = structuredClone(detail), r = blockRotationCandidates(detail, graph, catalogs, { limit: 512 });
  assert.ok(r.candidates.length); assert.ok(r.candidates.some(p => p.boundary_closure && p.moves.flatMap(m => m.instances).some(id => !p.group.instances.includes(id))));
  assert.deepEqual(blockRotationCandidates(detail, graph, catalogs, { limit: 512 }), r);
  const w = detail.witness, { chains, anchors } = extractChains(w), b = extentOf(w);
  for (const p of r.candidates) {
    assert.ok(p.moves.length <= 4); const ids = p.moves.flatMap(m => m.instances); assert.ok(ids.length <= 16); assert.equal(new Set(ids).size, ids.length);
    assert.ok(p.facilityRotations.length >= 2); assert.ok(p.releaseBridges.length <= 8);
    const facilityChanges = planFacilityRotations(w, catalogs, p.moves, p.facilityRotations), m = movementPlan(w, chains, anchors, p.moves, { facilityChanges });
    for (const q of w.placements.map(q => ({ ...q, ...m.shiftFacility(q) }))) assert.ok(q.x >= b.minX && q.y >= b.minY && q.x + q.width - 1 <= b.maxX && q.y + q.height - 1 <= b.maxY);
  }
  const small = blockRotationCandidates(detail, graph, catalogs, { limit: 1, poseBudget: 1 }); assert.equal(small.poses_examined, 1); assert.equal(small.pose_budget_exhausted, true); assert.ok(small.candidates.length <= 1);
  for (const options of [{ limit: 0 }, { poseBudget: 0 }, { junctionPolicy: 'unknown' }]) assert.throws(() => blockRotationCandidates(detail, graph, catalogs, options), { code: 'block-rotation-budget' });
  assert.deepEqual(detail, before);
});
