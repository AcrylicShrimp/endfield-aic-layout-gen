import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { planFacilityRotations } from './facility-rotation.mjs';
import { movementPlan } from './movement.mjs';
import { reembed, extractChains } from './model.mjs';
import { deflectionCandidates } from './deflection.mjs';
import { rotationCandidates } from './rotation-candidates.mjs';
import { cooperativePortCandidates } from './cooperative.mjs';
import { vacantPortCandidates } from './ports.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { key } from './path.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['facilities', 'components'].map(k => [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const axes = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const rate = { numerator: 1, denominator: 3 };
const spec = catalogs.facilities.facilities.find(f => f.footprint.width > f.footprint.height && [0, 90, 180, 270].every(r => f.allowed_rotations.includes(r)) && f.ports.some(p => p.transport === 'belt' && p.direction === 'output'));
function network(id, transport, cells, terminals) {
  return { id, item: `geometry-only-${id}`, transport, cells, terminals, component_ids: [], requirement_ids: [],
    segments: cells.slice(1).map((to, i) => ({ from: cells[i], to, rate })) };
}
const external = (id, position, direction) => ({ id, direction, position, rate, endpoint: { kind: 'external', node: id } });
function fixture() {
  const p = { instance: 'a', facility: spec.id, rotation: 0, x: 10, y: 10, ...spec.footprint };
  const port = spec.ports.find(p => p.transport === 'belt' && p.direction === 'output'), axis = axes[port.edge];
  const start = { x: p.x + port.position.x + axis[0], y: p.y + port.position.y + axis[1] };
  const cells = Array.from({ length: 7 }, (_, i) => ({ x: start.x + axis[0] * i, y: start.y + axis[1] * i }));
  const t = { id: 'facility-terminal', direction: 'output', position: start, rate, endpoint: { kind: 'facility', instance: 'a', port: port.id } };
  const frame = [...Array.from({ length: 41 }, (_, x) => ({ x, y: 0 })), ...Array.from({ length: 40 }, (_, y) => ({ x: 40, y: y + 1 }))];
  return { labels: { a: 'A' }, witness: { placements: [p], logistics_components: [], transport_networks: [
    network('main', 'belt', cells, [t, external('target', cells.at(-1), 'input')]),
    network('frame', 'pipe', frame, [external('frame-start', frame[0], 'output'), external('frame-end', frame.at(-1), 'input')]) ] } };
}
test('catalog rotation projects non-square bodies and all world ports without changing terminal semantics', () => {
  const d = fixture(), w = d.witness, p = w.placements[0], before = structuredClone(w), moves = [{ instances: ['a'], dx: 3, dy: -2 }];
  const c = planFacilityRotations(w, catalogs, moves, [{ instance: 'a', rotation: 90 }]).get('a');
  assert.deepEqual(c.pose, { x: 13, y: 8, width: spec.footprint.height, height: spec.footprint.width, rotation: 90 });
  for (const port of spec.ports) {
    const old = axes[port.edge], q = c.ports.find(q => q.id === port.id);
    const position = { x: 13 + spec.footprint.height - 1 - port.position.y, y: 8 + port.position.x };
    assert.deepEqual(q.position, position); assert.deepEqual(q.connection, { x: position.x - old[1], y: position.y + old[0] });
    assert.equal(q.direction, port.direction); assert.equal(q.transport, port.transport);
  }
  const t = w.transport_networks[0].terminals[0]; assert.deepEqual(c.terminalPositions.get(t.id), c.ports.find(q => q.id === t.endpoint.port).connection);
  assert.equal(c.turns, 1); assert.deepEqual(w, before);
  c.pose.x++; c.terminalPositions.get(t.id).x++; assert.deepEqual(w, before);
  assert.equal(planFacilityRotations(w, catalogs, moves, [{ instance: 'a', rotation: p.rotation }]).size, 0);
});
test('rotation input requires selected unique catalog-backed facilities and compatible port identity', () => {
  const w = fixture().witness, moves = [{ instances: ['a'], dx: 0, dy: 0 }], rotation = { instance: 'a', rotation: 90 };
  for (const rs of [null, [rotation, rotation], [{ ...rotation, instance: 'missing' }], [{ ...rotation, rotation: 45 }], Array(17).fill(rotation)])
    assert.throws(() => planFacilityRotations(w, catalogs, moves, rs), { code: 'local-facility-rotation' });
  assert.throws(() => planFacilityRotations(w, catalogs, [], [rotation]), { code: 'local-facility-rotation' });
  const restricted = structuredClone(catalogs); restricted.facilities.facilities.find(f => f.id === spec.id).allowed_rotations = [0];
  assert.throws(() => planFacilityRotations(w, restricted, moves, [rotation]), { code: 'local-facility-rotation' });
  w.transport_networks[0].terminals[0].endpoint.port = 'missing';
  assert.throws(() => planFacilityRotations(w, catalogs, moves, [rotation]), { code: 'local-facility-rotation' });
});
test('in-place rotation reroutes real catalog endpoints, rotates retained arms, preserves externals and cannot report unchanged', () => {
  const d = fixture(), before = structuredClone(d), moves = [{ instances: ['a'], dx: 0, dy: 0 }], facilityRotations = [{ instance: 'a', rotation: 90 }];
  const { chains, anchors } = extractChains(d.witness), c = chains[0], changes = planFacilityRotations(d.witness, catalogs, moves, facilityRotations);
  const m = movementPlan(d.witness, chains, anchors, moves, { facilityChanges: changes });
  assert.equal(m.fixedChains.size, 0); assert.ok(m.changed.has(c.id)); assert.equal(m.anchorTurns.get(`belt:${key(c.cells[0])}`), 1);
  const r = reembed(d, catalogs, { moves, facilityRotations }); assert.equal(r.status, 'candidate');
  assert.equal(r.detail.factory_validated, false); assert.equal(r.detail.reembedding.move_schema_version, 4); assert.deepEqual(r.detail.reembedding.facility_rotations, facilityRotations);
  assert.equal(r.routes[0].first, (c.first + 1) % 4); assert.equal(r.routes[0].last, c.last);
  const w = r.detail.witness; assert.deepEqual(w.placements[0], { ...d.witness.placements[0], ...changes.get('a').pose });
  assert.deepEqual(w.transport_networks[0].terminals[0].endpoint, d.witness.transport_networks[0].terminals[0].endpoint);
  assert.deepEqual(w.transport_networks[0].terminals[0].rate, rate);
  assert.deepEqual(w.transport_networks[0].terminals[1], d.witness.transport_networks[0].terminals[1]);
  assert.deepEqual(w.transport_networks[1], d.witness.transport_networks[1]);
  assert.deepEqual(r.detail.catalog_ports, project(w, catalogs)); assert.deepEqual(d, before);
  assert.equal(reembed(d, catalogs, { moves, facilityRotations: [{ instance: 'a', rotation: 0 }] }).status, 'unchanged');
});
test('rotated footprint collisions and extent overflow are rejected before candidate publication', () => {
  const d = fixture(), p = d.witness.placements[0], moves = [{ instances: ['a'], dx: 0, dy: 0 }], facilityRotations = [{ instance: 'a', rotation: 90 }];
  d.witness.placements.push({ ...p, instance: 'obstacle', x: p.x, y: p.y + p.height });
  assert.throws(() => reembed(d, catalogs, { moves, facilityRotations }), { code: 'local-body-collision' });
  const isolated = fixture(); isolated.witness.transport_networks = [];
  assert.throws(() => reembed(isolated, catalogs, { moves, facilityRotations }), { code: 'local-move-bounds' });
});
test('vacant and cooperative port domains use the rotated body and endpoint positions', () => {
  const d = fixture(), w = d.witness, moves = [{ instances: ['a'], dx: 1, dy: 2 }], facilityRotations = [{ instance: 'a', rotation: 90 }], { chains } = extractChains(w);
  const changes = planFacilityRotations(w, catalogs, moves, facilityRotations), c = changes.get('a'), t = w.transport_networks[0].terminals[0];
  const r = vacantPortCandidates(w, chains, catalogs, moves, { chainIds: [0], facilityRotations });
  for (const p of r.candidates) {
    const q = c.ports.find(q => q.id === p.portAssignments[0].port), end = chains[0].cells.at(-1);
    assert.equal(p.endpoint_distance, Math.abs(q.connection.x - end.x) + Math.abs(q.connection.y - end.y));
  }
  const q = c.ports.find(q => q.direction === t.direction && q.transport === 'belt' && q.id !== t.endpoint.port);
  assert.ok(q); w.placements.push({ ...w.placements[0], instance: 'obstacle', x: q.connection.x, y: q.connection.y, width: 1, height: 1 });
  const ports = cooperativePortCandidates(w, catalogs, moves, [t.id], { facilityRotations });
  assert.ok(!ports.domains[0].ports.includes(q.id));
});
test('catalog deflection interleaves actual rotations and respects non-square pose geometry', () => {
  const p = fixture().witness.placements[0], d = { labels: {}, witness: { placements: [
    { ...p, instance: 'a', x: 10, y: 20 }, { ...p, instance: 'b', x: 10 + p.width, y: 20 }, { ...p, instance: 'frame', x: 40, y: 0 }
  ], transport_networks: [], logistics_components: [] } }, before = structuredClone(d);
  const r = deflectionCandidates(d, catalogs, { rotationPolicy: 'catalog', limit: 512 });
  assert.ok(r.candidates.some(p => p.facilityRotations.length));
  for (const candidate of r.candidates) {
    const changes = planFacilityRotations(d.witness, catalogs, candidate.moves, candidate.facilityRotations), shifts = new Map(candidate.moves.flatMap(m => m.instances.map(id => [id, m]))), occupied = new Set();
    for (const p of d.witness.placements) {
      const delta = shifts.get(p.instance), q = { ...p, ...(changes.get(p.instance)?.pose ?? { x: p.x + (delta?.dx ?? 0), y: p.y + (delta?.dy ?? 0) }) };
      for (let dy = 0; dy < q.height; dy++) for (let dx = 0; dx < q.width; dx++) { const k = `${q.x + dx},${q.y + dy}`; assert.ok(!occupied.has(k)); occupied.add(k); }
    }
  }
  assert.deepEqual(d, before); assert.ok(r.poses_examined <= r.pose_budget);
});

test('shared anchors compare actual terminal displacement and reject incompatible arm rotations', () => {
  // Deliberately synthetic ownership geometry isolates the movement rule from catalog projection.
  const p = { instance: 'a', x: 1, y: 1, width: 1, height: 1, rotation: 0 }, position = { x: 5, y: 5 };
  const terminals = ['a', 'b'].map(instance => ({ id: instance, position, rate, direction: 'output', endpoint: { kind: 'facility', instance, port: 'synthetic' } }));
  const w = { placements: [p, { ...p, instance: 'b', x: 9 }], logistics_components: [], transport_networks: [network('shared', 'belt', [position, { x: 6, y: 5 }], terminals)] };
  const { chains, anchors } = extractChains(w), moves = [{ instances: ['a', 'b'], dx: 0, dy: 0 }];
  const changes = new Map(w.placements.map(p => [p.instance, { pose: p, turns: 1, terminalPositions: new Map([[p.instance, { x: 5, y: 6 }]]) }]));
  const m = movementPlan(w, chains, anchors, moves, { facilityChanges: changes });
  assert.deepEqual(m.shiftAnchor('belt', position), { x: 5, y: 6 }); assert.equal(m.anchorTurns.get('belt:5,5'), 1);
  changes.get('b').terminalPositions.set('b', { x: 5, y: 7 });
  assert.throws(() => movementPlan(w, chains, anchors, moves, { facilityChanges: changes }), { code: 'local-shared-terminal-move' });
  changes.get('b').terminalPositions.set('b', { x: 5, y: 6 }); changes.get('b').turns = 2;
  assert.throws(() => movementPlan(w, chains, anchors, moves, { facilityChanges: changes }), { code: 'local-shared-terminal-rotation' });
});

test('standalone rotation proposals are bounded, deterministic, catalog-backed and leave the parent immutable', () => {
  const d = fixture(), before = structuredClone(d), r = rotationCandidates(d, catalogs, { limit: 512 });
  assert.ok(r.candidates.length); assert.equal(r.poses_examined, 27); assert.equal(r.pose_budget_exhausted, false);
  assert.deepEqual(rotationCandidates(d, catalogs, { limit: 512 }), r);
  for (const p of r.candidates) {
    assert.equal(p.moves.length, 1); assert.equal(p.facilityRotations.length, 1); assert.equal(p.kind, 'catalog-rotation');
    assert.notEqual(p.facilityRotations[0].rotation, d.witness.placements[0].rotation);
    const { chains, anchors } = extractChains(d.witness), facilityChanges = planFacilityRotations(d.witness, catalogs, p.moves, p.facilityRotations);
    assert.doesNotThrow(() => movementPlan(d.witness, chains, anchors, p.moves, { facilityChanges, junctionPolicy: 'downstream' }));
    assert.ok(p.releaseBridges.length <= 8);
  }
  const small = rotationCandidates(d, catalogs, { poseBudget: 1, limit: 1 }); assert.equal(small.poses_examined, 1); assert.equal(small.pose_budget_exhausted, true); assert.ok(small.candidates.length <= 1);
  for (const options of [{ limit: 0 }, { poseBudget: 0 }, { stepLimit: -1 }, { junctionPolicy: 'unknown' }])
    assert.throws(() => rotationCandidates(d, catalogs, options), { code: 'rotation-invalid-input' });
  assert.deepEqual(d, before);
});
