import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { chainBlocks } from './model.mjs';
import { placeOnly } from '../constructive-placement-only/model.mjs';
import { render } from '../constructive-placement-only/render.mjs';
const fixture = JSON.parse(fs.readFileSync(new URL('../../../data/examples/constructive-network.facilities.json', import.meta.url)));
const items = { schema_version: 1, items: [{ id: 'solid', transport: 'belt' }, { id: 'fluid', transport: 'pipe' }] };
function input(pairs) {
  const catalog = structuredClone(fixture);
  const f = catalog.facilities[0]; f.allowed_rotations = [0];
  f.ports.push({ id: 'pipe-in', transport: 'pipe', direction: 'input', position: { x: 0, y: 0 }, edge: 'north' },
    { id: 'pipe-out', transport: 'pipe', direction: 'output', position: { x: 0, y: 0 }, edge: 'north' });
  const w = { schema_version: 1, success: true,
    nodes: [...new Set(pairs.flatMap(p => p.slice(0, 2)))].map(id => ({ id, kind: 'facility', facility: f.id, recipe: 'fixture' })),
    edges: pairs.map(([source, target, item = 'solid'], i) => ({ id: `e${i}`, source, target, item, rate: { numerator: 1, denominator: 5 } })) };
  return placeOnly(w, catalog, items, pairs[0][1]);
}
test('a maximal chain is one block with adjacent opposed candidate cells', () => {
  const source = input([['a', 'b'], ['b', 'c'], ['c', 'd']]);
  const report = chainBlocks(source, items), b = report.block_experiment.blocks[0];
  assert.deepEqual(b.members, ['a', 'b', 'c', 'd']); assert.equal(b.links.length, 3);
  for (let i = 0; i < b.members.length - 1; i++) {
    const a = report.placements.find(p => p.instance === b.members[i]), c = report.placements.find(p => p.instance === b.members[i + 1]);
    assert.equal(a.x + a.width + 1, c.x);
    assert.deepEqual(a.catalog_ports.find(p => p.direction === 'output' && p.transport === 'belt').connection,
      c.catalog_ports.find(p => p.direction === 'input' && p.transport === 'belt').connection);
  }
  assert.equal(report.routing_performed, false); assert.equal(report.factory_validated, false);
});
test('shared two-producer two-consumer topology stays four independently owned blocks', () => {
  const report = chainBlocks(input([['a', 'c'], ['a', 'd'], ['b', 'c'], ['b', 'd']]), items);
  assert.equal(report.block_experiment.blocks.length, 4);
  assert.equal(new Set(report.block_experiment.blocks.flatMap(b => b.members)).size, 4);
  assert.equal(report.block_experiment.blocks.flatMap(b => b.boundaries).length, 8);
});
test('directed cycles are not contracted, including a pipe return edge', () => {
  for (const kind of ['solid', 'fluid']) {
    const report = chainBlocks(input([['a', 'b'], ['b', 'c'], ['c', 'a', kind]]), items);
    assert.equal(report.block_experiment.summary.contracted_edges, 0);
  }
});
test('a pipe side input remains an explicit boundary on a chain interior member', () => {
  const report = chainBlocks(input([['a', 'b'], ['b', 'c'], ['pipe', 'b', 'fluid']]), items);
  const b = report.block_experiment.blocks.find(b => b.members.includes('b'));
  assert.deepEqual(b.members, ['a', 'b', 'c']);
  assert.equal(b.boundaries.find(e => e.transport === 'pipe').instance, 'b');
  assert.deepEqual(b.boundaries.find(e => e.transport === 'pipe').candidate_ports, ['pipe-in']);
});
test('external belt obligations prevent treating a shared output as private', () => {
  const source = input([['a', 'b'], ['b', 'c']]);
  source.external_requirements.push({ id: 'external', source: 'a', target: 'surplus', item: 'solid', rate: { numerator: 1, denominator: 5 } });
  const report = chainBlocks(source, items);
  assert.equal(report.block_experiment.extraction.find(e => e.source === 'a').reason, 'belt-fan-out');
  assert.deepEqual(report.external_requirements, source.external_requirements);
});
test('singleton control and shuffled input preserve deterministic geometry and all IDs', () => {
  const source = input([['a', 'b'], ['b', 'c'], ['x', 'c']]);
  const control = chainBlocks(source, items, { contract: false });
  assert.equal(control.block_experiment.blocks.length, source.placements.length);
  const reversed = structuredClone(source); reversed.internal_requirements.reverse(); reversed.placements.reverse();
  const a = chainBlocks(source, items), b = chainBlocks(reversed, items);
  assert.deepEqual(a.placements, b.placements);
  assert.deepEqual(a.block_experiment.blocks, b.block_experiment.blocks);
  assert.deepEqual(a.internal_requirements, source.internal_requirements);
});
test('invalid input and resource parameters fail explicitly', () => {
  const source = input([['a', 'b']]);
  for (const minimumGap of [0, 17, NaN, 1.5]) assert.throws(() => chainBlocks(source, items, { minimumGap }), /gap/);
  source.internal_requirements[0].target = 'missing';
  assert.throws(() => chainBlocks(source, items), /endpoints/);
});
test('parallel demands and non-opposed belt interfaces are not silently contracted', () => {
  let source = input([['a', 'b'], ['a', 'b']]);
  assert.equal(chainBlocks(source, items).block_experiment.summary.contracted_edges, 0);
  source = input([['a', 'b']]);
  source.placements.find(p => p.instance === 'a').catalog_ports.filter(p => p.direction === 'output' && p.transport === 'belt')
    .forEach(p => { p.edge = 'north'; });
  assert.equal(chainBlocks(source, items).block_experiment.extraction[0].reason, 'non-opposed-belt-ports');
});
test('full graph preserves all catalog-relative ports, identities and channel clearance', () => {
  const source = JSON.parse(fs.readFileSync(new URL('../constructive-supplier-groups/heavy.json', import.meta.url)));
  const catalogItems = JSON.parse(fs.readFileSync(new URL('../../../data/game/normalized/items.json', import.meta.url)));
  const report = chainBlocks(source, catalogItems, { minimumGap: 1 });
  assert.equal(report.block_experiment.summary.chains, 7);
  assert.equal(report.block_experiment.summary.chain_facilities, 20);
  assert.equal(report.block_experiment.blocks.length, 46);
  for (const p of report.placements) {
    const old = source.placements.find(q => q.instance === p.instance);
    for (const key of ['recipe', 'facility', 'rotation', 'width', 'height', 'label']) assert.equal(p[key], old[key]);
    const relative = p => p.catalog_ports.map(q => ({ ...q,
      position: { x: q.position.x - p.x, y: q.position.y - p.y }, connection: { x: q.connection.x - p.x, y: q.connection.y - p.y } }));
    assert.deepEqual(relative(p), relative(old));
  }
  const blocks = report.block_experiment.blocks;
  for (let i = 0; i < blocks.length; i++) for (let j = i + 1; j < blocks.length; j++) {
    const a = blocks[i], b = blocks[j], g = Math.max(a.channel, b.channel);
    assert(a.x + a.width + g <= b.x || b.x + b.width + g <= a.x || a.y + a.height + g <= b.y || b.y + b.height + g <= a.y);
  }
  assert.deepEqual(report.external_requirements, source.external_requirements);
  const internal = blocks.flatMap(b => b.internal_requirements), boundary = blocks.flatMap(b => b.boundaries.map(e => e.requirement));
  for (const e of source.internal_requirements) assert.equal(boundary.filter(id => id === e.id).length, internal.includes(e.id) ? 0 : 2);
  const html = render(report);
  assert(html.includes('Same-color members')); assert(html.includes('NOT A VALIDATED FACTORY'));
});
test('explicit CLI validates runtime catalog provenance and uses compact completion', () => {
  const folder = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-chain-cli-'));
  const args = ['docs/benchmarks/constructive-chain-blocks/place.mjs',
    '--placement', 'docs/benchmarks/constructive-supplier-groups/heavy.json',
    '--facilities', 'data/game/normalized/facilities.json', '--items', 'data/game/normalized/items.json',
    '--minimum-gap', '1', '--report-output', path.join(folder, 'result.json'), '--visualization-output', path.join(folder, 'result.html')];
  const result = spawnSync(process.execPath, args, { encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr); assert(result.stdout.length < 1024);
  assert.equal(JSON.parse(result.stdout).status, 'placement-only-unrouted');
  assert.equal(spawnSync(process.execPath, args, { encoding: 'utf8' }).status, 1);
  const rejected = spawnSync(process.execPath, [...args.slice(0, -4), '--contract', 'maybe', ...args.slice(-4)], { encoding: 'utf8' });
  assert.equal(rejected.status, 1); assert.match(rejected.stderr, /must be true or false/);
});
