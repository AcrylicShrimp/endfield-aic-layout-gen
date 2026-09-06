import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { cooperativePortCandidates as enumerate, cooperativeControls } from './cooperative.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
const catalogs = { facilities: JSON.parse(fs.readFileSync(new URL('../../../data/game/normalized/facilities.json', import.meta.url))) };
test('cooperative controls pair each selected obstruction with a matching retained failure', () => {
  const request = { moves: [{ instances: ['a'], dx: 0, dy: -1 }], portAssignments: [], maxNewCrossings: 2 };
  const report = { terminal_access: [2, 7].map(id => ({ id, status: 'found', blockers: [] })), records: [
    { status: 'rejected', request: { ...request, portAssignments: [{ terminal: 'changed', port: 'p' }] }, routes: [{ id: 7, status: 'no-path' }] },
    { status: 'rejected', request: { ...request, order: 'near' }, routes: [{ id: 2, status: 'no-path' }] },
    { status: 'rejected', request: { maxNewCrossings: 2, portAssignments: [], moves: request.moves, order: 'far', chainIds: [2, 7], routePriority: [7] }, routes: [{ id: 2, status: 'found' }, { id: 7, status: 'no-path' }] }
  ] }, before = structuredClone(report);
  assert.deepEqual(cooperativeControls(report).map(c => c.index), [1]);
  assert.deepEqual(cooperativeControls(report, [7, 2]).map(c => [c.id, c.index]), [[7, 2], [2, 1]]);
  for (const ids of [[], [7, 7], [-1], [0], [2.5], Array.from({ length: 9 }, (_, i) => i)]) assert.throws(() => cooperativeControls(report, ids));
  const mismatch = structuredClone(report); mismatch.records[2].request.maxNewCrossings = 3;
  assert.throws(() => cooperativeControls(mismatch, [2, 7]), /disagree/);
  const missing = structuredClone(report); missing.terminal_access[1].status = 'work-budget';
  assert.throws(() => cooperativeControls(missing, [7]), /relaxed terminal-access/);
  const success = structuredClone(report); success.records[2].routes[1].status = 'found';
  assert.throws(() => cooperativeControls(success, [7]), /failed retained replay/);
  const changed = structuredClone(report); changed.records[2].request.portAssignments = [{ terminal: 'changed', port: 'p' }];
  assert.throws(() => cooperativeControls(changed, [7]), /failed retained replay/);
  assert.deepEqual(report, before);
});
function fixture() {
  const f = catalogs.facilities.facilities.find(f => f.ports.filter(p => p.direction === 'input' && p.transport === 'belt').length >= 3);
  const rotation = f.allowed_rotations[0], shape = rotateFacility(f, rotation);
  const w = { placements: [{ instance: 'a', facility: f.id, rotation, x: 10, y: 10, width: shape.width, height: shape.height }], logistics_components: [], transport_networks: [] };
  const ports = project(w, catalogs).a.filter(p => p.direction === 'input' && p.transport === 'belt');
  const terminals = ports.slice(0, 3).map((q, i) => ({ id: `t${i}`, direction: 'input', position: q.connection, endpoint: { kind: 'facility', instance: 'a', port: q.id }, rate: { numerator: i + 1, denominator: 3 } }));
  w.transport_networks = [{ transport: 'belt', item: 'fixture', terminals }];
  return { w, ports, terminals };
}
test('cooperative assignments equal exhaustive injective domains with reserved slots and simultaneous swaps', () => {
  const { w, ports, terminals } = fixture(), before = structuredClone(w), r = enumerate(w, catalogs, [], ['t0', 't1']);
  assert.equal(r.termination, 'exhausted');
  const expected = [];
  for (const a of ports) for (const b of ports) if (a.id !== b.id && [a, b].every(q => q.id !== terminals[2].endpoint.port)) expected.push([a.id, b.id].join(','));
  const actual = r.candidates.map(c => terminals.slice(0, 2).map(t => c.portAssignments.find(a => a.terminal === t.id)?.port ?? t.endpoint.port).join(','));
  assert.deepEqual(actual.sort(), expected.sort()); assert.equal(new Set(actual).size, actual.length);
  assert.ok(actual.includes([terminals[1].endpoint.port, terminals[0].endpoint.port].join(',')));
  assert.deepEqual(w, before); assert.deepEqual(enumerate(w, catalogs, [], ['t0', 't1']), r);
  assert.ok(r.candidates.some(c => c.portAssignments.length === 0));
});
test('cooperative enumeration charges partial nodes and reports candidate truncation', () => {
  const { w } = fixture(), nodes = enumerate(w, catalogs, [], ['t0', 't1'], { nodeBudget: 1 });
  assert.equal(nodes.nodes, 1); assert.equal(nodes.termination, 'node-budget'); assert.equal(nodes.candidates.length, 0);
  const limited = enumerate(w, catalogs, [], ['t0', 't1'], { limit: 1 });
  assert.equal(limited.termination, 'candidate-limit'); assert.equal(limited.candidates.length, 1);
  for (const ids of [[], ['missing'], ['t0', 't0']]) assert.throws(() => enumerate(w, catalogs, [], ids), { code: 'local-cooperative-ports' });
  for (const options of [{ limit: 0 }, { limit: 4097 }, { nodeBudget: 0 }, { nodeBudget: 65537 }])
    assert.throws(() => enumerate(w, catalogs, [], ['t0'], options), { code: 'local-cooperative-ports' });
});
test('cooperative domains screen projected bodies, not old coordinates', () => {
  const { w, ports } = fixture(), moved = { instances: ['a'], dx: 2, dy: 3 }, p = ports[0].connection;
  w.placements.push({ ...w.placements[0], instance: 'obstacle', x: p.x + 2, y: p.y + 3, width: 1, height: 1 });
  const r = enumerate(w, catalogs, [moved], ['t0', 't1']);
  assert.ok(r.domains.every(d => !d.ports.includes(ports[0].id)));
});
