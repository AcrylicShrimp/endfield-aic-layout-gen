import test from 'node:test';
import assert from 'node:assert/strict';
import { reservationConflicts } from './reservations.mjs';

function boundary(requirement, instance, transport, connection, position) {
  return { requirement, attachments: [{ kind: 'new-port',
    port: { instance, port: 'input', transport, connection, position } }] };
}
test('distinct future port corridor can occupy a selected endpoint', () => {
  const n = { boundary_requirements: [
    boundary('target', 'a', 'belt', { x: 3, y: 2 }, { x: 4, y: 2 }),
    boundary('owner', 'b', 'belt', { x: 3, y: 1 }, { x: 3, y: 0 }) ] };
  assert.equal(reservationConflicts(n, 1).length, 0);
  const c = reservationConflicts(n, 3);
  assert.equal(c.length, 1);
  assert.deepEqual(c[0].requirements, ['target']);
  assert.deepEqual(c[0].reservation_owners[0].requirements, ['owner']);
});
test('shared port identity and independent layers are not conflicts', () => {
  const n = { boundary_requirements: [
    boundary('a', 'same', 'belt', { x: 0, y: 1 }, { x: 0, y: 0 }),
    boundary('b', 'same', 'belt', { x: 0, y: 1 }, { x: 0, y: 0 }),
    boundary('c', 'other', 'pipe', { x: 0, y: 1 }, { x: 0, y: 0 }) ] };
  assert.deepEqual(reservationConflicts(n, 3), []);
  assert.throws(() => reservationConflicts(n, 0), /length/);
});
