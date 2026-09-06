import test from 'node:test';
import assert from 'node:assert/strict';
import { rotateRegion, sourcePortfolio, eastInterface, frontierGroups } from './model.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
const catalogs = {
  facilities: { facilities: [{ id: 'fixture-facility', footprint: { width: 2, height: 3 }, allowed_rotations: [0, 90, 180, 270], ports: [
    { id: 'in', direction: 'input', transport: 'belt', edge: 'west', position: { x: 0, y: 1 } },
    { id: 'out', direction: 'output', transport: 'belt', edge: 'south', position: { x: 1, y: 2 } }] }] },
  components: { components: [{ id: 'fixture-splitter', allowed_rotations: [0, 90, 180, 270] }] },
  recipes: { recipes: [{ id: 'fixture-recipe', facility: 'fixture-facility', inputs: [{ item: 'fixture-item' }], outputs: [{ item: 'fixture-item' }] }] },
  items: { items: [{ id: 'fixture-item', transport: 'belt' }] },
  transports: { transports: [{ kind: 'belt', capacity: { quantity: 1, duration_ms: 1000 } }] },
};
const rate = { numerator: 1, denominator: 2 };
const region = ids => ({ labels: Object.fromEntries(ids.map(id => [id, id])), block: { members: ids }, witness: {
  schema_version: 1, placements: ids.map((instance, i) => ({ instance, recipe: 'fixture-recipe', facility: 'fixture-facility', rotation: 0, x: i * 4 + 3, y: 4, width: 2, height: 3 })),
  utilizations: Object.fromEntries(ids.map(id => [id, rate])), transport_networks: [], logistics_components: [] } });
const graphOf = ids => ({ nodes: ids.map(id => ({ id, label: id, recipe: 'fixture-recipe' })), edges: [] });
const edge = (source, target) => ({ id: `${source}:${target}`, source, target, item: 'fixture-item', transport: 'belt', internal: true, rate });

test('quarter-turn poses keep non-square bodies, terminal ports, segment direction and component orientation coherent', () => {
  const d = region(['a']); d.witness.logistics_components.push({ id: 'c', component: 'fixture-splitter', kind: 'splitter', transport: 'belt', rotation: 270, position: { x: 6, y: 4 } });
  d.witness.transport_networks.push({ id: 'n', item: 'fixture-item', transport: 'belt', requirement_ids: ['e'], component_ids: ['c'], cells: [{ x: 2, y: 5 }, { x: 4, y: 7 }],
    segments: [{ from: { x: 2, y: 5 }, to: { x: 2, y: 6 }, rate }], terminals: [
      { endpoint: { kind: 'facility', instance: 'a', port: 'in' }, position: { x: 2, y: 5 }, direction: 'input', rate },
      { endpoint: { kind: 'facility', instance: 'a', port: 'out' }, position: { x: 4, y: 7 }, direction: 'output', rate }] });
  const before = structuredClone(d), zero = rotateRegion(d, 0, catalogs), turned = rotateRegion(d, 90, catalogs);
  assert.deepEqual(turned.witness.placements[0], { ...d.witness.placements[0], x: 1, y: 1, width: 3, height: 2, rotation: 90 });
  assert.deepEqual(turned.witness.logistics_components[0].position, { x: 3, y: 4 }); assert.equal(turned.witness.logistics_components[0].rotation, 0);
  for (const angle of [0, 90, 180, 270]) {
    const r = rotateRegion(d, angle, catalogs), ports = project(r.witness, catalogs);
    for (const n of r.witness.transport_networks) for (const t of n.terminals) assert.deepEqual(t.position, ports[t.endpoint.instance].find(p => p.id === t.endpoint.port).connection);
    assert.deepEqual(r.witness.transport_networks[0].segments[0].rate, rate);
  }
  let roundtrip = zero; for (let i = 0; i < 4; i++) roundtrip = rotateRegion(roundtrip, 90, catalogs);
  assert.deepEqual(roundtrip.witness, zero.witness); assert.deepEqual(d, before);
});

test('poses reject unsupported catalog rotations and routed external boundaries', () => {
  const d = region(['a']), restricted = structuredClone(catalogs); restricted.facilities.facilities[0].allowed_rotations = [0];
  assert.throws(() => rotateRegion(d, 90, restricted), { code: 'frontier-pose-facility-rotation' });
  assert.throws(() => rotateRegion(d, 45, catalogs), { code: 'frontier-pose-angle' });
  d.witness.logistics_components.push({ id: 'c', component: 'fixture-splitter', rotation: 0, transport: 'belt', position: { x: 8, y: 4 } });
  const components = structuredClone(catalogs); components.components.components[0].allowed_rotations = [0];
  assert.throws(() => rotateRegion(d, 90, components), { code: 'frontier-pose-component-rotation' });
  d.witness.transport_networks.push({ terminals: [{ endpoint: { kind: 'external' } }] });
  assert.throws(() => rotateRegion(d, 0, catalogs), { code: 'frontier-pose-external' });
});

test('source cuts explicitly retain the smallest or largest east-compatible membership', () => {
  const graph = graphOf(['a', 'b']); graph.edges = [edge('a', 'external'), edge('b', 'external')];
  const alternatives = [['a'], ['a', 'b']].map(ids => ({ directory: 'fixture', detail: region(ids) })), before = structuredClone(alternatives);
  const min = sourcePortfolio(graph, alternatives, catalogs, { cut: 'min' }), max = sourcePortfolio(graph, alternatives, catalogs, { cut: 'max' });
  assert.equal(min.covered, 1); assert.equal(max.covered, 2); assert.deepEqual(min.available_cuts, [1, 2]);
  assert.ok(min.exclusions.length > 0); assert.ok(max.variants.every(v => eastInterface(graph, v.detail, catalogs).accepted));
  assert.ok(max.variants.every(v => v.detail.source_pose.angle === 270)); assert.deepEqual(alternatives, before);
  assert.throws(() => sourcePortfolio(graph, alternatives, catalogs, { cut: 'unknown' }), { code: 'frontier-source-policy' });
});

test('frontier grouping joins only shared consumers and is not hardwired to two clusters', () => {
  const graph = graphOf(['a', 'b', 'c', 'ta', 'tb', 'tc', 'shared']); graph.edges = [edge('a', 'ta'), edge('b', 'tb'), edge('c', 'tc')];
  const banks = ['a', 'b', 'c'].map(id => ({ id, variants: [{ detail: region([id]) }] }));
  assert.equal(frontierGroups(graph, banks, catalogs).length, 3);
  graph.edges.push(edge('a', 'shared'), edge('c', 'shared'));
  const groups = frontierGroups(graph, banks, catalogs);
  assert.equal(groups.length, 2); assert.deepEqual(groups.find(g => g.targets.includes('shared')).banks.map(b => b.id), ['a', 'c']);
  graph.edges.push(edge('b', 'shared')); assert.equal(frontierGroups(graph, banks, catalogs).length, 1);
  assert.equal(new Set(frontierGroups(graph, banks, catalogs)[0].targets).size, 4);
});

test('equal-size source alternatives cannot silently substitute facility ownership', () => {
  const graph = graphOf(['a', 'b', 'c']); graph.edges = ['a', 'b', 'c'].map(id => edge(id, 'external'));
  const alternatives = [['a', 'b'], ['a', 'c']].map(ids => ({ directory: 'fixture', detail: region(ids) }));
  assert.throws(() => sourcePortfolio(graph, alternatives, catalogs), { code: 'frontier-source-membership' });
});
