import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { loadGraphSnapshot, loadBankGraph, loadPlacementFixture, loadOrderFixture } from './constructive.mjs';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';

const read = name => JSON.parse(fs.readFileSync(new URL(name, import.meta.url)));
const manifest = read('./manifest.json');
const catalogs = Object.fromEntries(['recipes', 'items', 'facilities'].map(k => [k,
  read(`../../../data/game/normalized/${k}.json`)]));
const canonical = v => Array.isArray(v) ? v.map(canonical) : v && typeof v === 'object'
  ? Object.fromEntries(Object.keys(v).sort().map(k => [k, canonical(v[k])])) : v;
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const digest = v => sha(JSON.stringify(canonical(v)));
const pick = (o, keys) => Object.fromEntries(keys.filter(k => Object.hasOwn(o, k)).map(k => [k, o[k]]));

test('catalog inputs and expanded graph identities, ordered edges and exact rates retain their frozen semantics', () => {
  for (const [name, expected] of Object.entries(manifest.catalogs))
    assert.equal(sha(fs.readFileSync(new URL(`../../../data/game/normalized/${name}.json`, import.meta.url))), expected, name);
  const graph = normalizeGraph(loadGraphSnapshot(), catalogs), bank = loadBankGraph(catalogs);
  assert.equal(digest(graph), manifest.semantic_sha256.graph);
  assert.equal(digest(bank), manifest.semantic_sha256.banks);
  assert.deepEqual([graph.nodes.length, graph.edges.length, bank.nodes.length, bank.edges.length], [59, 96, 59, 80]);
  // Recheck all recipe margins in the bank input without replaying historical allocation reports.
  const snapshot = loadGraphSnapshot();
  snapshot.internal_requirements = bank.edges.filter(e => e.internal);
  snapshot.external_requirements = bank.edges.filter(e => !e.internal);
  assert.deepEqual(normalizeGraph(snapshot, catalogs).nodes, graph.nodes);
});

test('catalog-derived geometry retains each frozen target, placement order, rotation, port, alignment and rigid block', () => {
  for (const name of ['heavy', 'gap6']) {
    const p = loadPlacementFixture(name);
    const geometry = {
      target_instance: p.target_instance,
      placements: p.placements.map(p => pick(p, ['instance', 'label', 'recipe', 'facility', 'x', 'y', 'width', 'height',
        'rotation', 'catalog_ports', 'alignment', 'block'])),
      bounds: p.bounds, pipe_relations: p.pipe_relations,
      ...(p.block_experiment ? { blocks: p.block_experiment.blocks } : {}) };
    assert.equal(digest(geometry), manifest.semantic_sha256[name], name);
    const other = p.placements.find(q => q.instance !== p.target_instance).instance;
    assert.notEqual(digest({ ...geometry, target_instance: other }), manifest.semantic_sha256[name], `${name}: changed target`);
    assert.equal(p.routing_performed, false); assert.equal(p.factory_validated, false);
  }
  assert.equal(digest(loadOrderFixture()), manifest.semantic_sha256.order);
});

test('a repeated member within one rigid block is rejected', t => {
  const file = new URL('./gap6-layout.json', import.meta.url), layout = read('./gap6-layout.json');
  const block = layout.blocks[0]; block.members.push(block.members[0]);
  const originalRead = fs.readFileSync;
  t.mock.method(fs, 'readFileSync', (name, ...options) => String(name) === file.href
    ? Buffer.from(JSON.stringify(layout)) : originalRead(name, ...options));
  assert.throws(() => loadPlacementFixture('gap6'), { message: `Constructive test fixture: duplicate member in ${block.id}` });
});

test('fixture loads are detached and each retained source file remains below 100 KiB', () => {
  const p = loadPlacementFixture('gap6');
  p.placements[0].catalog_ports[0].position.x++;
  p.internal_requirements[0].rate.numerator++;
  assert.notDeepEqual(p, loadPlacementFixture('gap6'));
  assert.deepEqual(loadGraphSnapshot(), loadGraphSnapshot());
  assert.throws(() => loadPlacementFixture('../historical-report'), /unknown placement variant/);
  for (const file of fs.readdirSync(new URL('.', import.meta.url))) {
    const stat = fs.statSync(new URL(file, import.meta.url));
    if (stat.isFile()) assert.ok(stat.size < 100 * 1024, `${file}: ${stat.size} bytes`);
  }
});
