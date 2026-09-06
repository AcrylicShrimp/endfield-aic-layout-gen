import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { constructCycle } from '../constructive-cycle-blocks/model.mjs';
import { compactInterface } from '../constructive-auto-interface/compact.mjs';
import { movementPlan } from './movement.mjs';
import { extractChains, reembed } from './model.mjs';
import { connectedGroups, dependencyCones, emptyRectangles } from './groups.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = normalizeGraph(read('../constructive-chain-blocks/gap-1-placement.json'), catalogs), blocks = extractCycles(graph, catalogs).blocks;

test('dependency cones retain complete branch layers, stop at budgets and ignore external terminals', () => {
  const nodes = ['sink', 'a', 'b', 'aa', 'bb', 'source'].map(id => ({ id }));
  const edges = [['a', 'sink'], ['b', 'sink'], ['aa', 'a'], ['bb', 'b'], ['source', 'aa'], ['source', 'bb']]
    .map(([source, target]) => ({ source, target, internal: true }));
  const g = { nodes, edges: [...edges, { source: 'external', target: 'source', internal: false }] }, snapshot = structuredClone(g);
  const cones = dependencyCones(g);
  assert.ok(cones.some(c => c.instances.join(',') === 'a,aa,b,bb,sink' && c.kind === 'upstream-cone'));
  assert.ok(cones.some(c => c.instances.join(',') === 'a,aa,b,bb,source' && c.kind === 'downstream-cone'));
  assert.ok(!dependencyCones(g, { maxMembers: 2 }).some(c => c.kind === 'upstream-cone' && c.instances.includes('sink')));
  assert.ok(dependencyCones(g, { maxDepth: 1 }).every(c => c.instances.length <= 3));
  assert.deepEqual(g, snapshot);
  assert.deepEqual(dependencyCones({ nodes: [...nodes].reverse(), edges: [...g.edges].reverse() }), cones);
  assert.throws(() => dependencyCones(g, { maxDepth: 0 }));
  assert.throws(() => dependencyCones(g, { maxMembers: 17 }));
  assert.throws(() => dependencyCones({ nodes, edges: [{ source: 'absent', target: 'sink', internal: true }] }));
});

test('dependency cones terminate on cycles and include the actual seven-facility product fork', () => {
  const cycle = { nodes: ['a', 'b', 'c'].map(id => ({ id })), edges: [['a', 'b'], ['b', 'c'], ['c', 'a']].map(([source, target]) => ({ source, target, internal: true })) };
  assert.equal(dependencyCones(cycle).length, 4);
  const groups = connectedGroups(graph, catalogs, { cones: true });
  const labels = new Map(graph.nodes.map(n => [n.id, n.label]));
  assert.ok(groups.some(g => g.instances.map(id => labels.get(id)).sort().join(',') === 'F03,F07,F08,F12,F13,F18,F19'));
  assert.ok(groups.every(g => g.instances.length <= 16 && new Set(g.instances).size === g.instances.length));
});

test('groups are connected and include actual cycles, independent of labels and graph row order', () => {
  const groups = connectedGroups(graph, catalogs);
  for (const b of blocks) assert.ok(groups.some(g => JSON.stringify(g.instances) === JSON.stringify([...b.members].sort())));
  for (const g of groups) {
    const seen = new Set([g.instances[0]]); let changed;
    do { changed = false; for (const e of graph.edges.filter(e => e.internal && g.instances.includes(e.source) && g.instances.includes(e.target)))
      if (seen.has(e.source) !== seen.has(e.target)) { seen.add(e.source); seen.add(e.target); changed = true; }
    } while (changed);
    assert.equal(seen.size, g.instances.length);
  }
  const permuted = structuredClone(graph); permuted.nodes.reverse(); permuted.edges.reverse(); permuted.nodes.forEach(n => { n.label = 'irrelevant'; });
  assert.deepEqual(connectedGroups(permuted, catalogs).map(g => g.instances.join('|')).sort(), groups.map(g => g.instances.join('|')).sort());
});

test('whole cyclic groups translate all internal paths and junctions without mutating inputs', () => {
  for (const b of blocks) {
    const d = structuredClone(compactInterface(constructCycle(catalogs, graph, b, 'root'), catalogs)), snapshot = structuredClone(d), { chains, anchors } = extractChains(d.witness);
    const p = movementPlan(d.witness, chains, anchors, [{ instances: b.members, dx: 7, dy: 9 }]);
    assert.equal(p.fixedChains.size, chains.length); assert.equal(p.changed.size, 0);
    assert.deepEqual(p.components.sort(), d.witness.logistics_components.map(c => c.id).sort());
    for (const c of chains) assert.deepEqual(p.fixedChains.get(c.id), c.cells.map(v => ({ x: v.x + 7, y: v.y + 9 })));
    assert.deepEqual(d, snapshot);
  }
});

test('unidentified shared terminals remain coupled and reject partial ownership', () => {
  const position = { x: 1, y: 0 }, rate = { numerator: 1, denominator: 2 };
  const w = { placements: [{ instance: 'a', x: 0, y: 0, width: 1, height: 1 }, { instance: 'b', x: 2, y: 0, width: 1, height: 1 }],
    logistics_components: [], transport_networks: [{ id: 'n', transport: 'belt', cells: [position], segments: [], terminals: [
      { endpoint: { kind: 'facility', instance: 'a' }, direction: 'output', position, rate },
      { endpoint: { kind: 'facility', instance: 'b' }, direction: 'input', position, rate }] }] };
  const { chains, anchors } = extractChains(w), snapshot = structuredClone(w);
  assert.throws(() => movementPlan(w, chains, anchors, [{ instances: ['a'], dx: 0, dy: 3 }]), { code: 'local-shared-terminal-move' });
  const p = movementPlan(w, chains, anchors, [{ instances: ['a', 'b'], dx: 0, dy: 3 }]);
  assert.equal(p.movable('belt', position), true); assert.deepEqual(p.shiftAnchor('belt', position), { x: 1, y: 3 }); assert.deepEqual(w, snapshot);
  assert.throws(() => movementPlan(w, chains, anchors, { instance: 'a', x: 0, y: 3 }), { code: 'local-move-input' });
  assert.throws(() => movementPlan(w, chains, anchors, [{ instances: ['a', 'a'], dx: 0, dy: 3 }]), { code: 'local-move-input' });
});

test('downstream junction ownership carries all arms only when every output owner moves', () => {
  // An anchor-graph fixture, not a game geometry or recipe validity claim.
  const center = { x: 3, y: 2 }, ps = [{ x: 1, y: 0 }, { x: 1, y: 4 }, { x: 5, y: 0 }, { x: 5, y: 4 }];
  const ids = ['a', 'b', 'c', 'd'];
  const terminals = ps.map((position, i) => ({ position, direction: i < 2 ? 'output' : 'input', endpoint: { kind: 'facility', instance: ids[i] } }));
  const w = { placements: ids.map((instance, i) => ({ instance, x: i < 2 ? 0 : 6, y: (i % 2) * 4, width: 1, height: 1 })),
    logistics_components: [{ id: 'junction', transport: 'belt', position: center }],
    transport_networks: [{ transport: 'belt', terminals, cells: [...ps, center] }] };
  const chains = ps.map((p, i) => ({ id: i, transport: 'belt', cells: i < 2 ? [p, center] : [center, p] }));
  w.transport_networks[0].segments = chains.map(c => ({ from: c.cells[0], to: c.cells[1] }));
  const anchors = { belt: new Set([...ps, center].map(p => `${p.x},${p.y}`)), pipe: new Set() };
  const move = { instances: ['c', 'd'], dx: 0, dy: 8 }, snapshot = structuredClone(w);
  const retained = movementPlan(w, chains, anchors, [move]);
  assert.deepEqual(retained.components, []);
  const carried = movementPlan(w, chains, anchors, [move], { junctionPolicy: 'downstream' });
  assert.deepEqual(carried.components, ['junction']);
  assert.deepEqual([...carried.fixedChains.keys()], [2, 3]);
  assert.deepEqual([...carried.changed], [0, 1]);
  const separated = movementPlan(w, chains, anchors, [{ instances: ['c'], dx: 0, dy: 8 }, { instances: ['d'], dx: 0, dy: 9 }], { junctionPolicy: 'downstream' });
  assert.deepEqual(separated.components, []);
  assert.deepEqual([...separated.changed], [2, 3]);
  assert.equal(carried.movable('belt', ps[0]), false);
  assert.deepEqual(movementPlan(w, chains, anchors, [{ ...move, instances: ['c'] }], { junctionPolicy: 'downstream' }).components, []);
  const external = structuredClone(w); external.transport_networks[0].terminals[3].endpoint = { kind: 'external' };
  assert.deepEqual(movementPlan(external, chains, anchors, [move], { junctionPolicy: 'downstream' }).components, []);
  assert.throws(() => movementPlan(w, chains, anchors, [move], { junctionPolicy: 'unknown' }), { code: 'local-junction-policy' });
  assert.deepEqual(w, snapshot);
});

test('current-extent move bounds reject carried paths rather than silently expanding the canvas', () => {
  const b = blocks[0], d = compactInterface(constructCycle(catalogs, graph, b, 'root'), catalogs);
  assert.throws(() => reembed(d, catalogs, { move: { instances: b.members, dx: 0, dy: 0 } }), { code: 'local-move-input' });
  assert.throws(() => reembed(d, catalogs, { moves: {} }), { code: 'local-move-input' });
  assert.throws(() => reembed(d, catalogs, { moves: [{ instances: b.members, dx: 100, dy: 0 }] }), { code: 'local-move-bounds' });
});

test('largest empty rectangle matches brute force for every tiny occupancy mask', () => {
  for (let mask = 0; mask < 1024; mask++) {
    const cells = [{ x: 0, y: 0 }, { x: 3, y: 2 }];
    const domain = Array.from({ length: 12 }, (_, i) => ({ x: i % 4, y: Math.floor(i / 4) })).filter(p => !(p.x === 0 && p.y === 0) && !(p.x === 3 && p.y === 2));
    for (let i = 0; i < domain.length; i++) if (mask & (1 << i)) cells.push(domain[i]);
    const w = { placements: [], logistics_components: [], transport_networks: [{ cells }] };
    const result = emptyRectangles(w), occupied = new Set(cells.map(p => `${p.x},${p.y}`)); let expected = 0;
    for (let x = 0; x < 4; x++) for (let y = 0; y < 3; y++) for (let xx = x; xx < 4; xx++) for (let yy = y; yy < 3; yy++) {
      let empty = true; for (let px = x; px <= xx; px++) for (let py = y; py <= yy; py++) if (occupied.has(`${px},${py}`)) empty = false;
      if (empty) expected = Math.max(expected, (xx - x + 1) * (yy - y + 1));
    }
    assert.equal(result[0]?.area ?? 0, expected);
    for (const r of result) for (let x = r.x; x < r.x + r.width; x++) for (let y = r.y; y < r.y + r.height; y++) assert.ok(!occupied.has(`${x},${y}`));
  }
});
