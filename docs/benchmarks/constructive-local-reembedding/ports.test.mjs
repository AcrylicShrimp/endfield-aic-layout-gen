import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { planPortAssignments, boundaryPortSwaps } from './ports.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { key } from './path.mjs';
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
