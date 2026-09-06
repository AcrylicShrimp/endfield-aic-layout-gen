import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { constructCycle } from '../constructive-cycle-blocks/model.mjs';
import { compactInterface } from '../constructive-auto-interface/compact.mjs';
import { movementPlan } from './movement.mjs';
import { extractChains, reembed } from './model.mjs';
import { connectedGroups, emptyRectangles } from './groups.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const graph = normalizeGraph(read('../constructive-chain-blocks/gap-1-placement.json'), catalogs), blocks = extractCycles(graph, catalogs).blocks;

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
    const p = movementPlan(d.witness, chains, anchors, { instances: b.members, dx: 7, dy: 9 });
    assert.equal(p.fixedChains.size, chains.length); assert.equal(p.changed.size, 0);
    assert.deepEqual(p.components.sort(), d.witness.logistics_components.map(c => c.id).sort());
    for (const c of chains) assert.deepEqual(p.fixedChains.get(c.id), c.cells.map(v => ({ x: v.x + 7, y: v.y + 9 })));
    assert.deepEqual(d, snapshot);
  }
});

test('zero-length shared terminals move together and reject partial ownership', () => {
  const position = { x: 1, y: 0 }, rate = { numerator: 1, denominator: 2 };
  const w = { placements: [{ instance: 'a', x: 0, y: 0, width: 1, height: 1 }, { instance: 'b', x: 2, y: 0, width: 1, height: 1 }],
    logistics_components: [], transport_networks: [{ id: 'n', transport: 'belt', cells: [position], segments: [], terminals: [
      { endpoint: { kind: 'facility', instance: 'a' }, direction: 'output', position, rate },
      { endpoint: { kind: 'facility', instance: 'b' }, direction: 'input', position, rate }] }] };
  const { chains, anchors } = extractChains(w), snapshot = structuredClone(w);
  assert.throws(() => movementPlan(w, chains, anchors, { instances: ['a'], dx: 0, dy: 3 }), { code: 'local-shared-terminal-move' });
  const p = movementPlan(w, chains, anchors, { instances: ['a', 'b'], dx: 0, dy: 3 });
  assert.equal(p.movable('belt', position), true); assert.deepEqual(p.translate(position), { x: 1, y: 3 }); assert.deepEqual(w, snapshot);
  assert.throws(() => movementPlan(w, chains, anchors, { instance: 'a', x: 0, y: 3 }), { code: 'local-move-input' });
  assert.throws(() => movementPlan(w, chains, anchors, { instances: ['a', 'a'], dx: 0, dy: 3 }), { code: 'local-move-input' });
});

test('current-extent move bounds reject carried paths rather than silently expanding the canvas', () => {
  const b = blocks[0], d = compactInterface(constructCycle(catalogs, graph, b, 'root'), catalogs);
  assert.throws(() => reembed(d, catalogs, { move: { instances: b.members, dx: 100, dy: 0 } }), { code: 'local-move-bounds' });
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
