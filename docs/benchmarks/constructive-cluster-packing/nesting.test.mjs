import test from 'node:test';
import assert from 'node:assert/strict';
import { footprint, collides, blockedAt, pendingPorts, accessFailures, nestingCandidates } from './nesting.mjs';

const empty = () => ({ placements: [], transport_networks: [], logistics_components: [] });
const network = (transport, cells) => ({ transport, cells, terminals: [] });
function expanded(w) {
  return [...w.placements.flatMap(p => Array.from({ length: p.width * p.height }, (_, i) =>
    ({ x: p.x + i % p.width, y: p.y + Math.floor(i / p.width), layer: 'body' }))),
  ...w.transport_networks.flatMap(n => n.cells.map(p => ({ ...p, layer: n.transport }))),
  ...w.logistics_components.map(c => ({ ...c.position, layer: c.transport }))];
}

test('bitset collision matches independent cell-pair oracle for all sampled positive and negative translations', () => {
  let seed = 723; const next = n => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) % n);
  for (let sample = 0; sample < 25; sample++) {
    const make = origin => ({ placements: [{ x: origin + next(3), y: origin + next(3), width: 1 + next(3), height: 1 + next(3) }],
      transport_networks: ['belt', 'pipe'].map(t => network(t, Array.from({ length: 4 }, () => ({ x: origin + next(5), y: origin + next(5) })))),
      logistics_components: [{ position: { x: origin - 1, y: origin }, transport: sample % 2 ? 'belt' : 'pipe' }] });
    const aw = make(-7), bw = make(13), a = footprint(aw), b = footprint(bw);
    const ac = expanded(aw).map(p => ({ ...p, x: p.x - a.x, y: p.y - a.y })), bc = expanded(bw).map(p => ({ ...p, x: p.x - b.x, y: p.y - b.y }));
    for (let dy = -7; dy <= 7; dy++) for (let dx = -7; dx <= 7; dx++) {
      const expected = ac.some(p => bc.some(q => p.x === q.x + dx && p.y === q.y + dy && (p.layer === 'body' || q.layer === 'body' || p.layer === q.layer)));
      assert.equal(collides(a, b, dx, dy), expected, JSON.stringify({ sample, dx, dy }));
      assert.equal(collides(b, a, -dx, -dy), expected);
    }
  }
});

test('independent layers may overlap while bodies and same layers block connections', () => {
  const a = footprint({ ...empty(), transport_networks: [network('belt', [{ x: 0, y: 0 }])] });
  const b = footprint({ ...empty(), transport_networks: [network('pipe', [{ x: 0, y: 0 }])] });
  assert.equal(collides(a, b, 0, 0), false); assert.equal(collides(a, a, 0, 0), true);
  assert.equal(blockedAt(a, 0, 0, 'pipe'), false); assert.equal(blockedAt(a, 0, 0, 'belt'), true);
  assert.equal(blockedAt(a, -1, 0, 'belt'), false); assert.equal(blockedAt(a, 0, 1, 'belt'), false);
});

test('pending access uses exact per-edge lane counts, excludes used ports and respects translated layers', () => {
  const catalogs = { facilities: { facilities: [{ id: 'fixture', footprint: { width: 1, height: 3 }, ports: [0, 1, 2].map(y =>
    ({ id: `p${y}`, position: { x: 0, y }, direction: 'output', transport: 'belt', edge: 'east' })) }] },
  transports: { transports: [{ kind: 'belt', capacity: { quantity: 1, duration_ms: 1000 } }] } };
  const w = { ...empty(), placements: [{ instance: 'source', facility: 'fixture', x: 7, y: 8, width: 1, height: 3, rotation: 0 }],
    transport_networks: [{ ...network('belt', [{ x: 8, y: 8 }]), terminals: [{ endpoint: { kind: 'facility', instance: 'source', port: 'p0' } }] }] };
  const graph = { edges: ['consumer-a', 'consumer-b'].map(target => ({ source: 'source', target, transport: 'belt', rate: { numerator: 2, denominator: 3 } })) };
  const req = pendingPorts(graph, { detail: { witness: w } }, catalogs);
  assert.equal(req[0].required, 2); assert.deepEqual(req[0].ports.map(p => p.id), ['p1', 'p2']);
  const obstacle = footprint({ ...empty(), transport_networks: [network('belt', [{ x: 0, y: 0 }])] });
  assert.equal(accessFailures(req, obstacle, 1, 1)[0].available, 1);
  assert.deepEqual(accessFailures(req, obstacle, 2, 1), []);
  const pipe = footprint({ ...empty(), transport_networks: [network('pipe', [{ x: 0, y: 0 }])] });
  assert.deepEqual(accessFailures(req, pipe, 1, 1), []);
  graph.edges[0].rate = { numerator: 4, denominator: 3 };
  assert.equal(pendingPorts(graph, { detail: { witness: w } }, catalogs)[0].required, 3);
  w.transport_networks[0].cells.push({ x: 8, y: 9 });
  assert.deepEqual(pendingPorts(graph, { detail: { witness: w } }, catalogs)[0].ports.map(p => p.id), ['p2']);
});

test('nesting budgets and geometry reject unsupported inputs with stable diagnostics', () => {
  assert.throws(() => footprint(empty()), { code: 'nesting-coordinate-schema' });
  assert.throws(() => footprint({ ...empty(), placements: [{ x: 0, y: 0, width: 0, height: 1 }] }), { code: 'nesting-footprint-schema' });
  assert.throws(() => footprint({ ...empty(), transport_networks: [network('unknown', [{ x: 0, y: 0 }])] }), { code: 'nesting-layer-schema' });
  assert.throws(() => footprint({ ...empty(), placements: [{ x: 0, y: 0, width: 1025, height: 1 }] }), { code: 'nesting-row-budget' });
  assert.throws(() => nestingCandidates({}, [], {}), { code: 'nesting-cluster-count' });
  assert.throws(() => nestingCandidates({}, [{}, {}], {}, 0), { code: 'nesting-candidate-budget' });
  assert.throws(() => collides({}, {}, 0.1, 0), { code: 'nesting-offset-schema' });
});
