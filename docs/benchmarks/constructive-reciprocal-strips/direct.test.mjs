import { loadGraphSnapshot } from '../fixtures/constructive.mjs';
import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { stripPortfolio } from './model.mjs';
import { applyReallocation } from './graph-delta.mjs';
import { directAttachments } from './direct.mjs';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { bodyCells } from '../constructive-local-reembedding/movement.mjs';
import { key } from '../constructive-local-reembedding/path.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const original = normalizeGraph(loadGraphSnapshot(), catalogs), block = extractCycles(original, catalogs).blocks[0];
const core = stripPortfolio(original, catalogs, block).candidates.find(d => d.reallocation && d.witness.placements.every(p => p.rotation === 0 || p.rotation === 180));
const graph = applyReallocation(original, core.reallocation), target = block.consumers.find(id => !block.cycle.includes(id));
function freeze(x) { if (x && typeof x === 'object' && !Object.isFrozen(x)) { Object.freeze(x); Object.values(x).forEach(freeze); } return x; }
const snapshot = structuredClone({ core, graph, catalogs }); freeze(core); freeze(graph); freeze(catalogs);

test('north-flow reciprocal core can directly feed an east-flow processor at a shared corner', () => {
  const ds = directAttachments(catalogs, graph, core, target);
  assert.ok(ds.length > 0); assert.ok(ds.some(d => d.direct_attachment.physical_cells === 1));
  assert.ok(ds.some(d => d.direct_attachment.physical_cells === 2));
  assert.ok(ds.some(d => witnessScore(d.witness).area === 126 && witnessScore(d.witness).physical_tiles === 7));
  for (const d of ds) {
    assert.equal(d.factory_validated, false); assert.equal(d.witness.placements.length, 4);
    const cells = d.witness.placements.flatMap(bodyCells).map(key), bodies = new Set(cells);
    assert.equal(cells.length, bodies.size);
    for (const n of d.witness.transport_networks) assert.ok(n.cells.every(p => !bodies.has(key(p)) && p.x >= 0 && p.y >= 0));
    const p = d.witness.placements.find(p => p.instance === target); assert.equal(p.rotation, 90);
    assert.equal(d.block.pending_edges.length, 1); assert.equal(d.block.pending_edges[0].source, target);
  }
  ds[0].witness.placements[0].x++; ds[0].witness.transport_networks[0].terminals[0].rate.numerator++;
  assert.deepEqual({ core, graph, catalogs }, snapshot);
});

test('direct family does not hide multiple incoming edges or reuse occupied supplier ports', () => {
  const retained = stripPortfolio(original, catalogs, block).candidates.find(d => !d.reallocation);
  assert.equal(directAttachments(catalogs, original, retained, target).length, 0);
  const blocked = structuredClone(core), source = graph.edges.find(e => e.target === target).source;
  for (const p of blocked.catalog_ports[source].filter(p => p.direction === 'output' && p.transport === 'belt'))
    blocked.witness.transport_networks[0].terminals.push({ endpoint: { kind: 'facility', instance: source, port: p.id } });
  assert.equal(directAttachments(catalogs, graph, blocked, target).length, 0);
});
