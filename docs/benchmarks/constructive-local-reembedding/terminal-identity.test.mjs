import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { detachablePairs, joinableChains, terminalJoinPlan } from './terminal-identity.mjs';
import { extractChains, reembed } from './model.mjs';
import { movementPlan } from './movement.mjs';
import { planFacilityRotations } from './facility-rotation.mjs';
import { closeTerminalGroups } from './compound.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { key } from './path.mjs';
const catalogs = Object.fromEntries(['facilities', 'components'].map(k => [k, JSON.parse(fs.readFileSync(new URL(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`, import.meta.url)))]));
function fixture() {
  const variant = catalogs.facilities.facilities.flatMap(f => f.allowed_rotations.map(rotation => ({ f, rotation, shape: rotateFacility(f, rotation) }))).find(({ shape }) =>
    shape.ports.filter(p => p.edge === 'east' && p.direction === 'output' && p.transport === 'belt').length >= 2 && shape.ports.some(p => p.edge === 'west' && p.direction === 'input' && p.transport === 'belt'));
  assert.ok(variant); const { f, rotation, shape } = variant;
  const outputs = shape.ports.filter(p => p.edge === 'east' && p.direction === 'output' && p.transport === 'belt'), input = shape.ports.find(p => p.edge === 'west' && p.direction === 'input' && p.transport === 'belt');
  const source = { instance: 'source', facility: f.id, rotation, x: 10, y: 10, width: shape.width, height: shape.height };
  const position = { x: source.x + outputs[0].position.x + 1, y: source.y + outputs[0].position.y };
  const target = { ...source, instance: 'target', x: position.x - input.position.x + 1, y: position.y - input.position.y };
  const rate = { numerator: 1, denominator: 3 };
  const n = { id: 'coincident', item: 'geometry-only', transport: 'belt', cells: [position], segments: [], component_ids: [], requirement_ids: [], terminals: [
    { id: 'source-terminal', endpoint: { kind: 'facility', instance: source.instance, port: outputs[0].id }, direction: 'output', position, rate },
    { id: 'target-terminal', endpoint: { kind: 'facility', instance: target.instance, port: input.id }, direction: 'input', position, rate }] };
  // An unrelated valid unit-path geometry supplies room for translated synthetic fixtures.
  const xmax = target.x + shape.width + 10, ymax = target.y + shape.height + 10;
  const frame = [...Array.from({ length: xmax + 1 }, (_, x) => ({ x, y: 0 })), ...Array.from({ length: ymax }, (_, y) => ({ x: xmax, y: y + 1 }))];
  const pipe = { id: 'frame', item: 'geometry-only-frame', transport: 'pipe', cells: frame, component_ids: [], requirement_ids: [],
    segments: frame.slice(1).map((to, i) => ({ from: frame[i], to, rate })), terminals: [frame[0], frame.at(-1)].map((position, i) => ({ id: `frame-${i}`, direction: i ? 'input' : 'output', position, rate, endpoint: { kind: 'external', node: `frame-${i}` } })) };
  return { detail: { labels: { source: 'S', target: 'T' }, witness: { placements: [source, target], logistics_components: [], transport_networks: [n, pipe] } }, outputs };
}

test('isolated cell becomes a directed identity-based chain and no longer closes mandatory groups', () => {
  const { detail: d } = fixture(), { chains } = extractChains(d.witness), c = chains.find(c => c.zero_length);
  assert.equal(detachablePairs(d.witness).size, 1); assert.ok(c); assert.equal(c.cells.length, 1); assert.equal(c.first, null); assert.equal(c.last, null);
  assert.equal(c.startTerminals[0].id, 'source-terminal'); assert.equal(c.endTerminals[0].id, 'target-terminal');
  assert.deepEqual(closeTerminalGroups(d.witness, []).map(g => g.instances), [['source'], ['target']]);
});

test('different owner shifts restore exact flow without leaving the old cell or mutating the source', () => {
  const { detail: d } = fixture(), before = structuredClone(d), original = d.witness.transport_networks[0], old = key(original.cells[0]);
  const moves = [{ instances: ['source'], dx: 0, dy: -2 }, { instances: ['target'], dx: 0, dy: -4 }];
  const r = reembed(d, catalogs, { moves }); assert.equal(r.status, 'candidate');
  const n = r.detail.witness.transport_networks[0]; assert.equal(n.segments.length, 2); assert.ok(n.cells.every(p => key(p) !== old));
  assert.ok(n.segments.every(s => JSON.stringify(s.rate) === JSON.stringify(original.terminals[0].rate)));
  assert.deepEqual(n.terminals.map(t => t.endpoint), original.terminals.map(t => t.endpoint));
  assert.deepEqual(n.terminals.map(t => t.rate), original.terminals.map(t => t.rate));
  assert.deepEqual(r.detail.witness.transport_networks[1], before.witness.transport_networks[1]); assert.deepEqual(d, before);
  const one = reembed(d, catalogs, { moves: moves.slice(0, 1) }); assert.equal(one.status, 'candidate');
  assert.deepEqual(one.detail.witness.transport_networks[0].terminals[1], original.terminals[1]);
});

test('same-shift coincident connection stays one cell and port assignment addresses only the selected identity', () => {
  const { detail: d, outputs } = fixture(), before = structuredClone(d);
  const same = reembed(d, catalogs, { moves: [{ instances: ['source', 'target'], dx: 0, dy: -2 }] });
  assert.equal(same.status, 'candidate'); assert.equal(same.detail.witness.transport_networks[0].segments.length, 0); assert.equal(same.detail.witness.transport_networks[0].cells.length, 1);
  const changed = reembed(d, catalogs, { portAssignments: [{ terminal: 'source-terminal', port: outputs[1].id }] });
  assert.equal(changed.status, 'candidate'); const n = changed.detail.witness.transport_networks[0];
  assert.equal(n.terminals[0].endpoint.port, outputs[1].id); assert.deepEqual(n.terminals[1], d.witness.transport_networks[0].terminals[1]);
  assert.ok(n.segments.length > 0); assert.deepEqual(d, before);
});

test('ambiguous rates, duplicate identities and incident branches are not detached', () => {
  for (const mutate of [w => { w.transport_networks[0].terminals[1].rate = { numerator: 2, denominator: 3 }; },
    w => { w.transport_networks[1].terminals[0].id = 'source-terminal'; },
    w => { const n = w.transport_networks[0], p = n.cells[0]; n.segments.push({ from: p, to: { x: p.x, y: p.y - 1 }, rate: n.terminals[0].rate }); }]) {
    const { detail: d } = fixture(); mutate(d.witness); assert.equal(detachablePairs(d.witness).size, 0);
  }
  const { detail: d } = fixture(); d.witness.transport_networks[0].terminals[1].rate = { numerator: 2, denominator: 3 };
  const { chains, anchors } = extractChains(d.witness);
  assert.throws(() => movementPlan(d.witness, chains, anchors, [{ instances: ['source'], dx: 0, dy: -2 }]), { code: 'local-shared-terminal-move' });
});

test('separated terminal body intrusion is rejected before publishing a candidate', () => {
  const { detail: d } = fixture(), { chains, anchors } = extractChains(d.witness);
  assert.throws(() => movementPlan(d.witness, chains, anchors, [{ instances: ['source'], dx: 1, dy: 0 }]), { code: 'local-endpoint-body' });
});

test('explicit rip-up of a recognized coincident connection restores the unchanged one-cell route', () => {
  const { detail: d } = fixture(), before = structuredClone(d), { chains } = extractChains(d.witness);
  const c = chains.find(c => c.zero_length), result = reembed(d, catalogs, { chainIds: [c.id] });
  assert.equal(result.status, 'unchanged'); assert.equal(result.routes.length, 1);
  assert.equal(result.routes[0].status, 'found'); assert.equal(result.routes[0].steps, 0);
  assert.equal(result.routes[0].turns, 0); assert.deepEqual(d, before);
});

test('catalog rotation separates a zero-length pair by identity and restores its complete directed connection', () => {
  const { detail: d } = fixture(), before = structuredClone(d), p = d.witness.placements[0];
  const moves = [{ instances: ['source'], dx: 0, dy: -4 }], facilityRotations = [{ instance: 'source', rotation: (p.rotation + 90) % 360 }];
  const changes = planFacilityRotations(d.witness, catalogs, moves, facilityRotations), { chains, anchors } = extractChains(d.witness);
  const c = chains.find(c => c.zero_length), m = movementPlan(d.witness, chains, anchors, moves, { facilityChanges: changes });
  assert.ok(m.separated.has(`belt:${key(c.cells[0])}`)); assert.ok(m.changed.has(c.id));
  const result = reembed(d, catalogs, { moves, facilityRotations }); assert.equal(result.status, 'candidate');
  const n = result.detail.witness.transport_networks[0]; assert.ok(n.segments.length > 0);
  assert.deepEqual(n.terminals[0].position, changes.get('source').terminalPositions.get('source-terminal'));
  assert.deepEqual(n.terminals[1], d.witness.transport_networks[0].terminals[1]);
  assert.deepEqual(n.segments[0].from, n.terminals[0].position); assert.deepEqual(n.segments.at(-1).to, n.terminals[1].position);
  assert.ok(n.segments.every(s => JSON.stringify(s.rate) === JSON.stringify(n.terminals[0].rate)));
  assert.deepEqual(d, before);
});


test('separated direct terminals rejoin without retaining old path cells', () => {
  for (const moves of [[{ instances: ['source'], dx: 0, dy: -2 }],
    [{ instances: ['source'], dx: 0, dy: -2 }, { instances: ['target'], dx: 0, dy: -4 }]]) {
    const { detail: original } = fixture(), frozen = structuredClone(original);
    const separated = reembed(original, catalogs, { moves, endpointPolicy: 'terminal-flexible' });
    assert.equal(separated.status, 'candidate'); const before = structuredClone(separated.detail);
    const { chains } = extractChains(before.witness); assert.equal(joinableChains(before.witness, chains).length, 1);
    const joined = reembed(separated.detail, catalogs, { moves: moves.map(m => ({ ...m, dx: -m.dx, dy: -m.dy })), endpointPolicy: 'terminal-flexible' });
    assert.equal(joined.status, 'candidate'); assert.equal(joined.detail.reembedding.joined_chains.length, 1);
    assert.deepEqual(joined.detail.witness.transport_networks[0], original.witness.transport_networks[0]);
    assert.deepEqual(joined.detail.witness.transport_networks[1], original.witness.transport_networks[1]);
    assert.deepEqual(joined.detail.witness.placements, original.witness.placements);
    assert.deepEqual(separated.detail, before); assert.deepEqual(original, frozen);
  }
});

test('rejoining eligibility rejects mismatched rates, external ends, components and extra incident chains', () => {
  const { detail: original } = fixture();
  const separated = reembed(original, catalogs, { moves: [{ instances: ['source'], dx: 0, dy: -2 }] }).detail.witness;
  for (const mutate of [w => { w.transport_networks[0].terminals[0].rate = { numerator: 2, denominator: 3 }; },
    w => { w.transport_networks[0].terminals[0].endpoint = { kind: 'external', node: 'outside' }; },
    w => { w.logistics_components.push({ transport: 'belt', position: w.transport_networks[0].terminals[0].position }); },
    w => { w.transport_networks[1].terminals[0].id = w.transport_networks[0].terminals[0].id; }]) {
    const w = structuredClone(separated); mutate(w); assert.equal(joinableChains(w, extractChains(w).chains).length, 0);
  }
  const { chains } = extractChains(separated);
  assert.equal(joinableChains(separated, [...chains, chains[0]]).length, 0);
});

test('a third final terminal or component prevents rejoining and anchor identity exceptions stay narrow', () => {
  const { detail: original } = fixture();
  const w = reembed(original, catalogs, { moves: [{ instances: ['source'], dx: 0, dy: -2 }] }).detail.witness;
  const eligible = joinableChains(w, extractChains(w).chains), destination = eligible[0].endTerminals[0].position;
  const shift = t => t.endpoint.instance === 'source' ? destination : t.position;
  const plan = terminalJoinPlan(w, eligible, shift, c => c.position);
  assert.equal(plan.chains.size, 1);
  assert.equal(plan.allows('belt', key(destination), key(eligible[0].startTerminals[0].position)), true);
  assert.equal(plan.allows('belt', key(destination), '999,999'), false);
  assert.equal(plan.allows('pipe', key(destination), key(destination)), false);
  const third = structuredClone(w); third.transport_networks[1].transport = 'belt';
  third.transport_networks[1].terminals[0].position = destination;
  assert.equal(terminalJoinPlan(third, eligible, shift, c => c.position).chains.size, 0);
  const component = structuredClone(w); component.logistics_components.push({ transport: 'belt', position: destination });
  assert.equal(terminalJoinPlan(component, eligible, shift, c => c.position).chains.size, 0);
});
