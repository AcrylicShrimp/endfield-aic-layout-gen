import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { planPortAssignments, boundaryPortSwaps, vacantPortCandidates } from './ports.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { key } from './path.mjs';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
const catalogs = { facilities: JSON.parse(fs.readFileSync(new URL('../../../data/game/normalized/facilities.json', import.meta.url))) };
function fixture() {
  const f = catalogs.facilities.facilities.find(f => f.ports.filter(p => p.transport === 'belt' && p.direction === 'input').length >= 2);
  const w = { placements: [{ instance: 'p', facility: f.id, rotation: f.allowed_rotations[0], x: 10, y: 10 }], logistics_components: [], transport_networks: [] };
  const ports = project(w, catalogs).p.filter(p => p.transport === 'belt' && p.direction === 'input');
  const terminals = ports.slice(0, 2).map((p, i) => ({ id: `t${i}`, position: p.connection, direction: 'input', endpoint: { kind: 'facility', instance: 'p', port: p.id }, rate: { numerator: 1, denominator: 3 } }));
  w.transport_networks = [{ item: 'runtime-fixture', transport: 'belt', terminals }];
  return { w, terminals, swap: [{ terminal: 't0', port: terminals[1].endpoint.port }, { terminal: 't1', port: terminals[0].endpoint.port }], eligible: new Set(terminals.map(t => t.id)) };
}

test('simultaneous port swaps preserve identities and rates, not old positions, without mutation', () => {
  const { w, terminals, swap, eligible } = fixture(), before = structuredClone(w);
  const r = planPortAssignments(w, catalogs, swap, eligible), ts = r.networks[0].terminals;
  assert.deepEqual(ts.map(t => t.id), terminals.map(t => t.id));
  assert.deepEqual(ts.map(t => t.rate), terminals.map(t => t.rate));
  assert.deepEqual(ts.map(t => t.position), [...terminals].reverse().map(t => t.position));
  assert.deepEqual(w, before); assert.equal(r.positions.size, 2);
  assert.throws(() => planPortAssignments(w, catalogs, swap.slice(0, 1), eligible), { code: 'local-port-assignment' });
  assert.throws(() => planPortAssignments(w, catalogs, swap, new Set()), { code: 'local-port-assignment' });
  assert.throws(() => planPortAssignments(w, catalogs, [{ terminal: 'absent', port: swap[0].port }], eligible), { code: 'local-port-assignment' });
  assert.throws(() => planPortAssignments(w, catalogs, [{ terminal: 't0', port: 'absent' }], eligible), { code: 'local-port-assignment' });
  const external = structuredClone(w); external.transport_networks[0].terminals[0].endpoint = { kind: 'external' };
  assert.throws(() => planPortAssignments(external, catalogs, swap, eligible), { code: 'local-port-assignment' });
});

test('boundary pair proposals require the same owner, item, direction and exact rate', () => {
  const { w, terminals, swap } = fixture(), c = { item: 'same', transport: 'belt', startTerminals: [], endTerminals: terminals }, move = { instances: ['p'] };
  assert.deepEqual(boundaryPortSwaps(w, [c], [move]), [swap]);
  assert.deepEqual(boundaryPortSwaps(w, [c], [{ instances: ['other'] }]), []);
  const unequal = structuredClone(c); unequal.endTerminals[1].rate.denominator = 4;
  assert.deepEqual(boundaryPortSwaps(w, [unequal], [move]), []);
  assert.deepEqual(boundaryPortSwaps(w, terminals.map((t, i) => ({ ...c, item: String(i), endTerminals: [t] })), [move]), []);
});

function vacantFixture() {
  const f = catalogs.facilities.facilities.find(f => f.ports.filter(p => p.transport === 'belt' && p.direction === 'input').length >= 3 && f.ports.filter(p => p.transport === 'belt' && p.direction === 'output').length >= 2);
  assert.ok(f); const rotation = f.allowed_rotations[0], shape = rotateFacility(f, rotation);
  const w = { placements: ['a', 'b'].map((instance, i) => ({ instance, facility: f.id, rotation, x: 20 + 30 * i, y: 20, width: shape.width, height: shape.height })), logistics_components: [], transport_networks: [] };
  const ps = project(w, catalogs), source = ps.a.find(p => p.transport === 'belt' && p.direction === 'output'), targets = ps.b.filter(p => p.transport === 'belt' && p.direction === 'input');
  const terminal = (id, instance, p) => ({ id, position: p.connection, direction: p.direction, endpoint: { kind: 'facility', instance, port: p.id }, rate: { numerator: 1, denominator: 1 } });
  const a = terminal('source', 'a', source), b = terminal('target', 'b', targets[0]), occupied = terminal('occupied', 'b', targets[1]);
  // Domain-only fixture; the third terminal reserves a port, not a balanced factory flow.
  w.transport_networks = [{ transport: 'belt', terminals: [a, b, occupied] }];
  return { w, chains: [{ id: 0, transport: 'belt', cells: [a.position, b.position], startTerminals: [a], endTerminals: [b] }], occupied };
}

test('vacant endpoint combinations use compatible unclaimed catalog slots and preserve input geometry', () => {
  const { w, chains, occupied } = vacantFixture(), before = structuredClone(w), moves = [{ instances: ['a'], dx: 3, dy: -2 }];
  const r = vacantPortCandidates(w, chains, catalogs, moves, { chainIds: [0] });
  assert.ok(r.candidates.some(p => p.portAssignments.length === 1));
  assert.ok(r.candidates.some(p => p.portAssignments.length === 2));
  assert.equal(r.combination_budget_reached, false);
  const projected = project({ ...w, placements: w.placements.map(p => p.instance === 'a' ? { ...p, x: p.x + 3, y: p.y - 2 } : p) }, catalogs);
  for (const [i, c] of r.candidates.entries()) {
    if (i) assert.ok(c.endpoint_distance >= r.candidates[i - 1].endpoint_distance);
    for (const assignment of c.portAssignments) {
      const t = w.transport_networks[0].terminals.find(t => t.id === assignment.terminal), q = projected[t.endpoint.instance].find(p => p.id === assignment.port);
      assert.equal(q.transport, 'belt'); assert.equal(q.direction, t.direction);
      assert.ok(t.endpoint.instance !== 'b' || q.id !== occupied.endpoint.port);
      assert.notEqual(q.id, t.endpoint.port);
    }
  }
  assert.deepEqual(w, before); assert.deepEqual(r, vacantPortCandidates(w, chains, catalogs, moves, { chainIds: [0] }));
});

test('vacant proposal limits, body screens and ambiguous endpoints remain explicit', () => {
  const { w, chains } = vacantFixture();
  assert.throws(() => vacantPortCandidates(w, chains, catalogs, [], { chainIds: [7] }), { code: 'local-port-assignment' });
  assert.throws(() => vacantPortCandidates(w, chains, catalogs, [], { chainIds: [0], limit: 0 }), { code: 'local-port-assignment' });
  const limited = vacantPortCandidates(w, chains, catalogs, [], { chainIds: [0], combinationBudget: 1 });
  assert.equal(limited.combination_budget_reached, true); assert.equal(limited.combinations_examined, 1);
  assert.deepEqual(limited.candidates, []);
  const ambiguous = [{ ...chains[0], startTerminals: [chains[0].startTerminals[0], chains[0].startTerminals[0]], endTerminals: [] }];
  assert.deepEqual(vacantPortCandidates(w, ambiguous, catalogs, [], { chainIds: [0] }).candidates, []);
  const blocked = structuredClone(w), before = vacantPortCandidates(w, chains, catalogs, [], { chainIds: [0] });
  const a = before.candidates[0].portAssignments[0], t = w.transport_networks[0].terminals.find(t => t.id === a.terminal), p = project(w, catalogs)[t.endpoint.instance].find(p => p.id === a.port).connection;
  const other = blocked.placements.find(q => q.instance !== t.endpoint.instance); other.x = p.x; other.y = p.y;
  assert.ok(vacantPortCandidates(blocked, chains, catalogs, [], { chainIds: [0] }).candidates.every(c => c.portAssignments.every(q => q.terminal !== a.terminal || q.port !== a.port)));
});
