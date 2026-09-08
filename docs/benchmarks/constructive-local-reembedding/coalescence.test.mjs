import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { releaseBridgeDraft, bridgeTopologyView } from './components.mjs';
import { extractChains, reembed } from './model.mjs';
import { movementPlan } from './movement.mjs';
import { terminalLinks, coalescenceCandidates } from './coalescence.mjs';
const catalogs = { components: JSON.parse(fs.readFileSync(new URL('../../../data/game/normalized/logistics-components.json', import.meta.url))), facilities: { facilities: [] }, recipes: { recipes: [] } };
const rate = { numerator: 1, denominator: 3 };
const net = (id, cells) => ({ id, item: id, transport: 'belt', cells, component_ids: [], requirement_ids: [],
  segments: cells.slice(1).map((to, i) => ({ from: cells[i], to, rate })),
  terminals: [cells[0], cells.at(-1)].map((position, i) => ({ id: `${id}-${i}`, position, direction: i ? 'input' : 'output', endpoint: { kind: 'external', node: `${id}-${i}` }, rate })) });
function bridge() {
  const def = catalogs.components.components.find(c => c.kind === 'bridge' && c.transport === 'belt');
  const networks = [net('horizontal', Array.from({ length: 7 }, (_, x) => ({ x, y: 3 }))), net('vertical', Array.from({ length: 7 }, (_, y) => ({ x: 3, y })))];
  for (const n of networks) n.component_ids.push('bridge');
  return { labels: {}, witness: { placements: [], logistics_components: [{ id: 'bridge', component: def.id, kind: def.kind, transport: def.transport, rotation: def.allowed_rotations[0], position: { x: 3, y: 3 } }], transport_networks: networks } };
}

test('all-bridge topology discovery has no eight-bridge factory ceiling', () => {
  const detail = bridge(), w = { placements: [], logistics_components: [], transport_networks: [] };
  for (let i = 0; i < 9; i++) {
    const shift = p => ({ x: p.x + i * 10, y: p.y });
    w.logistics_components.push({ ...detail.witness.logistics_components[0], id: `bridge-${i}`, position: shift(detail.witness.logistics_components[0].position) });
    for (const n of detail.witness.transport_networks) w.transport_networks.push({ ...n, id: `${i}-${n.id}`, component_ids: [`bridge-${i}`],
      cells: n.cells.map(shift), segments: n.segments.map(s => ({ ...s, from: shift(s.from), to: shift(s.to) })),
      terminals: n.terminals.map(t => ({ ...t, id: `${i}-${t.id}`, position: shift(t.position) })) });
  }
  const before = structuredClone(w);
  assert.throws(() => releaseBridgeDraft(w, catalogs, w.logistics_components.map(c => c.id)), { code: 'local-bridge-release' });
  const topology = bridgeTopologyView(w, catalogs);
  assert.equal(topology.released.size, 9); assert.equal(extractChains(topology.witness, topology.released).chains.length, 18);
  assert.deepEqual(topology.witness.transport_networks.map(n => n.segments), w.transport_networks.map(n => n.segments));
  assert.deepEqual(topology.witness.transport_networks.map(n => n.terminals), w.transport_networks.map(n => n.terminals));
  const r = coalescenceCandidates({ witness: w, labels: {} }, { nodes: [], edges: [] }, catalogs);
  assert.deepEqual(r.candidates, []); assert.deepEqual(w, before);
});

test('topology projection retains the structural bridge checks used by bounded release', () => {
  const detail = bridge(), topology = bridgeTopologyView(detail.witness, catalogs);
  assert.deepEqual(topology, releaseBridgeDraft(detail.witness, catalogs, ['bridge']));
  const invalid = [
    w => w.transport_networks.pop(),
    w => w.transport_networks[0].terminals.push({ ...w.transport_networks[0].terminals[0], position: { x: 3, y: 3 } }),
    w => { w.transport_networks[0].segments[2].rate = { numerator: 2, denominator: 3 }; },
  ];
  for (const mutate of invalid) {
    const w = structuredClone(detail.witness); mutate(w);
    assert.throws(() => bridgeTopologyView(w, catalogs), { code: 'local-bridge-release' });
    assert.throws(() => releaseBridgeDraft(w, catalogs, ['bridge']), { code: 'local-bridge-release' });
  }
});

test('an over-budget incident bridge group is a pair diagnostic, not a factory ceiling', () => {
  const def = catalogs.components.components.find(c => c.kind === 'bridge' && c.transport === 'belt');
  const horizontal = net('horizontal', Array.from({ length: 31 }, (_, x) => ({ x, y: 3 })));
  horizontal.terminals.forEach((t, i) => { t.endpoint = { kind: 'facility', instance: i ? 'b' : 'a' }; });
  const w = { placements: [{ instance: 'a', x: -1, y: 3, width: 1, height: 1 }, { instance: 'b', x: 31, y: 3, width: 1, height: 1 }],
    logistics_components: [], transport_networks: [horizontal] };
  for (let i = 1; i <= 9; i++) {
    const id = `bridge-${i}`, position = { x: i * 3, y: 3 }, n = net(`vertical-${i}`, Array.from({ length: 7 }, (_, y) => ({ x: position.x, y })));
    n.component_ids.push(id); horizontal.component_ids.push(id); w.transport_networks.push(n);
    w.logistics_components.push({ id, position, kind: def.kind, component: def.id, transport: def.transport, rotation: def.allowed_rotations[0] });
  }
  const other = net('other', Array.from({ length: 9 }, (_, x) => ({ x: x + 1, y: 10 })));
  other.terminals.forEach((t, i) => { t.endpoint = { kind: 'facility', instance: i ? 'd' : 'c' }; });
  w.transport_networks.push(other);
  w.placements.push({ instance: 'c', x: 0, y: 10, width: 1, height: 1 }, { instance: 'd', x: 10, y: 10, width: 1, height: 1 });
  const graph = { nodes: w.placements.map(p => ({ id: p.instance, recipe: 'geometry-only' })), edges: [] }, before = structuredClone(w);
  const r = coalescenceCandidates({ witness: w, labels: { a: 'A', b: 'B' } }, graph, catalogs);
  assert.ok(r.diagnostics.some(d => d.code === 'local-bridge-release-budget' && d.entity.bridges.length === 9));
  assert.ok(r.candidates.some(p => p.link.source === 'other-0'));
  assert.ok(r.candidates.every(p => p.releaseBridges.length <= 8));
  assert.ok(r.rejected['local-bridge-release-budget'] > 0); assert.deepEqual(w, before);
});

test('bridge release suppresses only approved crossing anchors and restores both owner paths', () => {
  const detail = bridge(), before = structuredClone(detail), original = extractChains(detail.witness);
  const r = releaseBridgeDraft(detail.witness, catalogs, ['bridge']), rebuilt = extractChains(r.witness, r.released);
  assert.equal(original.chains.length, 4); assert.equal(rebuilt.chains.length, 2);
  assert.equal(rebuilt.anchors.belt.has('3,3'), false); assert.deepEqual(detail, before);
  assert.equal(reembed(detail, catalogs, { releaseBridges: ['bridge'], endpointPolicy: 'terminal-flexible' }).status, 'rejected');
  const candidate = reembed(detail, catalogs, { releaseBridges: ['bridge'], endpointPolicy: 'terminal-flexible', maxNewCrossings: 1 });
  assert.equal(candidate.status, 'candidate'); assert.equal(candidate.routes.length, 2);
  assert.equal(candidate.detail.witness.logistics_components.length, 1);
  assert.notEqual(candidate.detail.witness.logistics_components[0].id, 'bridge');
  assert.deepEqual(candidate.detail.witness.transport_networks.map(n => n.terminals), before.witness.transport_networks.map(n => n.terminals));
  assert.deepEqual(detail, before);
});

test('bridge release rejects missing IDs, branch components and unsupported ownership', () => {
  const { witness: w } = bridge();
  for (const ids of [null, ['absent'], ['bridge', 'bridge']]) assert.throws(() => releaseBridgeDraft(w, catalogs, ids), { code: 'local-bridge-release' });
  w.logistics_components[0].kind = 'splitter';
  assert.throws(() => releaseBridgeDraft(w, catalogs, ['bridge']), { code: 'local-bridge-release' });
  w.logistics_components[0].kind = 'bridge'; w.transport_networks.pop();
  assert.throws(() => releaseBridgeDraft(w, catalogs, ['bridge']), { code: 'local-bridge-release' });
});

test('released owner paths are not carried rigidly when their two endpoints translate equally', () => {
  const { witness: w } = bridge();
  w.placements = [{ instance: 'a', x: -1, y: 3, width: 1, height: 1 }, { instance: 'b', x: 7, y: 3, width: 1, height: 1 }];
  w.transport_networks[0].terminals.forEach((t, i) => { t.endpoint = { kind: 'facility', instance: i ? 'b' : 'a' }; });
  const draft = releaseBridgeDraft(w, catalogs, ['bridge']), { chains, anchors } = extractChains(draft.witness, draft.released);
  const rerouteChains = new Set(chains.map(c => c.id));
  const plan = movementPlan(draft.witness, chains, anchors, [{ instances: ['a', 'b'], dx: 0, dy: 1 }], { rerouteChains });
  assert.equal(plan.fixedChains.size, 0);
});

test('coalescence derives relative attachment offsets, respects bounds and reports enumeration limits', () => {
  const n = net('connection', [...Array.from({ length: 9 }, (_, x) => ({ x: x + 1, y: 0 })), ...Array.from({ length: 6 }, (_, y) => ({ x: 9, y: y + 1 }))]);
  n.terminals.forEach((t, i) => { t.endpoint = { kind: 'facility', instance: i ? 'b' : 'a' }; });
  const w = { logistics_components: [], transport_networks: [n], placements: [{ instance: 'a', x: 0, y: 0, width: 1, height: 1 }, { instance: 'b', x: 10, y: 6, width: 1, height: 1 }, { instance: 'c', x: 20, y: 0, width: 1, height: 1 }] };
  const d = { witness: w, labels: { a: 'A', b: 'B', c: 'C' } }, graph = { nodes: w.placements.map(p => ({ id: p.instance, recipe: 'geometry-only' })), edges: [] }, before = structuredClone(d);
  const links = terminalLinks(extractChains(w).chains); assert.equal(links.length, 1); assert.equal(links[0].source.endpoint.instance, 'a');
  const r = coalescenceCandidates(d, graph, catalogs, { limit: 16 });
  assert.ok(r.candidates.length > 0); assert.ok(r.candidates.some(p => p.body_envelope_area < 147));
  assert.deepEqual(r, coalescenceCandidates(d, graph, catalogs, { limit: 16 })); assert.deepEqual(d, before);
  const bounded = coalescenceCandidates(d, graph, catalogs, { poseBudget: 1 });
  assert.equal(bounded.pose_budget_exhausted, true); assert.equal(bounded.pairs_considered, 1);
  assert.ok(bounded.pairs_considered <= bounded.pairs_available);
  assert.throws(() => coalescenceCandidates(d, graph, catalogs, { poseBudget: 0 }), { code: 'coalescence-budget' });
});

test('a shared pose budget samples later terminal relationships before deepening the first', () => {
  const networks = [0, 10].map((y, i) => {
    const n = net(`link-${i}`, Array.from({ length: 9 }, (_, x) => ({ x: x + 1, y })));
    n.terminals.forEach((t, j) => { t.endpoint = { kind: 'facility', instance: `${i}-${j}` }; });
    return n;
  });
  const placements = [0, 10].flatMap((y, i) => [0, 10].map((x, j) => ({ instance: `${i}-${j}`, x, y, width: 1, height: 1 })));
  placements.push({ instance: 'frame', x: 20, y: 20, width: 1, height: 1 });
  const detail = { witness: { placements, logistics_components: [], transport_networks: networks }, labels: Object.fromEntries(placements.map(p => [p.instance, p.instance])) };
  const graph = { nodes: placements.map(p => ({ id: p.instance, recipe: 'geometry-only' })), edges: [] }, before = structuredClone(detail);
  const r = coalescenceCandidates(detail, graph, catalogs, { poseBudget: 2 });
  assert.equal(r.links_available, 2); assert.equal(r.links_sampled, 2); assert.equal(r.pairs_considered, 2);
  assert.equal(r.poses_examined, 2); assert.equal(r.pose_budget_exhausted, true);
  assert.deepEqual(r.link_coverage.map(l => l.poses), [1, 1]);
  assert.deepEqual(detail, before);
  assert.deepEqual(r, coalescenceCandidates(detail, graph, catalogs, { poseBudget: 2 }));
});

test('one commodity owns both bridge channels without false turns or merged release paths', () => {
  const detail = bridge(), [horizontal, vertical] = detail.witness.transport_networks;
  const secondRate = { numerator: 2, denominator: 5 };
  vertical.segments.forEach(s => { s.rate = secondRate; });
  vertical.terminals.forEach(t => { t.rate = secondRate; });
  detail.witness.transport_networks = [{ ...horizontal,
    cells: [...new Map([...horizontal.cells, ...vertical.cells].map(p => [`${p.x},${p.y}`, p])).values()],
    segments: [...horizontal.segments, ...vertical.segments], terminals: [...horizontal.terminals, ...vertical.terminals] }];
  const before = structuredClone(detail), draft = releaseBridgeDraft(detail.witness, catalogs, ['bridge']);
  const rebuilt = extractChains(draft.witness, draft.released);
  assert.equal(rebuilt.chains.length, 2); assert.equal(rebuilt.anchors.belt.has('3,3'), false);
  assert.deepEqual(rebuilt.chains.map(c => [c.cells[0], c.cells.at(-1), c.rate]), [
    [{ x: 0, y: 3 }, { x: 6, y: 3 }, rate], [{ x: 3, y: 0 }, { x: 3, y: 6 }, secondRate] ]);
  const result = reembed(detail, catalogs, { releaseBridges: ['bridge'], endpointPolicy: 'terminal-flexible', maxNewCrossings: 1 });
  assert.equal(result.status, 'candidate'); assert.equal(result.score.turns, 0);
  assert.equal(result.detail.witness.logistics_components.length, 1);
  assert.deepEqual(result.detail.witness.transport_networks[0].component_ids, [result.detail.witness.logistics_components[0].id]);
  assert.deepEqual(result.detail.witness.transport_networks[0].terminals, detail.witness.transport_networks[0].terminals);
  assert.deepEqual(detail, before);
});
