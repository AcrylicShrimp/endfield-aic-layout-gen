import test from 'node:test';
import assert from 'node:assert/strict';
import { geometryAudit, frontierAudit } from './audit.mjs';

const node = () => ({ id: 'test', bounds: { width: 5, height: 3 }, member_instances: ['a', 'b'],
  internal_requirements: ['done'], external_requirements: [], boundary_requirements: [],
  placements: [{ instance: 'a', x: -2, y: 0, width: 2, height: 2 }],
  transport_networks: [{ id: 'net', cells: [{ x: 2, y: 2 }], segments: [],
    terminals: [{ id: 'ext', endpoint: { kind: 'external' }, position: { x: 2, y: 2 } }] }],
  logistics_components: [{ id: 'bridge', component: 'bridge-type', position: { x: 2, y: 2 } }] });
const catalog = { components: [{ id: 'bridge-type', footprint: { width: 1, height: 1 } }] };

test('inclusive translated bounds and all co-owning boundary objects', () => {
  const r = geometryAudit(node(), catalog, 2);
  assert.equal(r.area, 15);
  assert.equal(r.sides.x_min.value, -2);
  assert.deepEqual(r.sides.x_max.owners.map(o => o.kind), ['network-cell', 'external-terminal', 'component']);
  assert.equal(r.sides.y_min.strip_objects.filter(o => o.kind === 'facility').length, 1);
});
test('segments also hold a boundary and strip selection includes their intersection', () => {
  const n = node();
  n.transport_networks[0].segments.push({ from: { x: 1, y: 2 }, to: { x: 2, y: 2 } });
  assert(geometryAudit(n, catalog).sides.x_max.owners.some(o => o.id === 'net/segment:0'));
});
test('unknown component geometry and inconsistent bounds fail visibly', () => {
  assert.throws(() => geometryAudit(node(), { components: [] }), /Unsupported component/);
  const n = node(); n.bounds.width = 99;
  assert.throws(() => geometryAudit(n, catalog), /bounds/);
  assert.throws(() => geometryAudit(node(), catalog, 0), /strip/);
});
test('original unresolved ledger includes both-placed and external demands', () => {
  const n = node();
  const edges = [ { id: 'done', source: 'a', target: 'b' },
    { id: 'missing', source: 'a', target: 'b' }, { id: 'ext', source: 'outside', target: 'a' },
    { id: 'later', source: 'unplaced1', target: 'unplaced2' } ];
  const r = frontierAudit(n, { edges });
  assert.equal(r.unresolved_incident_requirements, 2);
  assert.equal(r.active_facilities, 2);
  assert.equal(r.both_directions, 1);
  assert.equal(r.input_only, 1);
  assert.equal(r.requirements[0].both_placed, true);
  assert.equal(r.requirements[0].projected, false);
});
test('shared attachment is not counted as a new port; closure is incident-only', () => {
  const n = node();
  n.boundary_requirements = [{ requirement: 'pending', direction: 'output',
    attachments: [{ kind: 'existing-network', port: { edge: 'north' } }] }];
  const r = frontierAudit(n, { edges: [{ id: 'pending', source: 'a', target: 'outside' }] });
  assert.equal(r.incident_closed_facilities, 1);
  assert.equal(r.output_only, 1);
  assert.deepEqual(r.requirements[0].attachment_kinds, ['existing-network']);
  assert.deepEqual(r.requirements[0].new_port_edges, []);
});
