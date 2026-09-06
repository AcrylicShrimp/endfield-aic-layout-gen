import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { detachablePairs } from './terminal-identity.mjs';
import { extractChains, reembed } from './model.mjs';
import { movementPlan } from './movement.mjs';
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
