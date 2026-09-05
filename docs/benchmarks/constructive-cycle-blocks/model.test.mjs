import fs from 'node:fs';
import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeGraph, extractCycles } from './graph.mjs';
import { constructCycle, inspectBlockCoverage } from './model.mjs';
import { rate, add, multiply, parseRate, dto } from '../constructive-interconnect/rates.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k =>
  [k, read(`../../../data/game/normalized/${k === 'components' ? 'logistics-components' : k}.json`)]));
const snapshot = read('../constructive-chain-blocks/gap-1-placement.json');
const graph = normalizeGraph(snapshot, catalogs), inventory = extractCycles(graph, catalogs);

test('full graph terminal margins recover consistent exact utilizations and SCC ownership', () => {
  assert.equal(graph.nodes.length, 59); assert.equal(graph.edges.length, 96);
  assert.equal(inventory.cyclic_components, 8); assert.equal(inventory.blocks.length, 8);
  assert.equal(inventory.owned_facilities, 35); assert.equal(inventory.unowned_nodes.length, 24); assert.deepEqual(inventory.failures, []);
  const ids = inventory.blocks.flatMap(b => b.members);
  assert.equal(new Set(ids).size, ids.length);
  assert.equal(inventory.blocks.filter(b => b.members.length === 7).length, 1);
  for (const b of inventory.blocks) assert.ok(b.pending_edges.every(e => b.members.includes(e.source) && !b.members.includes(e.target)));
});

test('normalization rejects changed margins, missing members and unknown commodities', () => {
  const changed = structuredClone(snapshot); changed.internal_requirements[0].rate.numerator++;
  assert.throws(() => normalizeGraph(changed, catalogs), { code: 'cycle-graph-margin' });
  const dropped = structuredClone(snapshot); dropped.placements.pop();
  assert.throws(() => normalizeGraph(dropped, catalogs), { code: 'cycle-graph-edge' });
  const unknown = structuredClone(snapshot); unknown.internal_requirements[0].item = 'missing';
  assert.throws(() => normalizeGraph(unknown, catalogs), { code: 'cycle-graph-edge' });
  const duplicate = structuredClone(snapshot); duplicate.placements.push(duplicate.placements[0]);
  assert.throws(() => normalizeGraph(duplicate, catalogs), { code: 'cycle-graph-duplicate' });
  for (const value of [undefined, null, '', 42]) {
    const invalid = structuredClone(snapshot); invalid.external_requirements[0].source = value;
    assert.throws(() => normalizeGraph(invalid, catalogs), { code: 'cycle-graph-edge' });
  }
});

test('geometry and ownership do not depend on frozen coordinates or labels', () => {
  const changed = structuredClone(snapshot);
  changed.placements.forEach((p, i) => { p.x = 1000 + i; p.y = 2000 + i; p.rotation = 0; p.label = `node-${i}`; p.catalog_ports = []; });
  const g = normalizeGraph(changed, catalogs), extracted = extractCycles(g, catalogs);
  assert.deepEqual(extracted.blocks, inventory.blocks);
  assert.deepEqual(constructCycle(catalogs, g, extracted.blocks[0]).witness, constructCycle(catalogs, graph, inventory.blocks[0]).witness);
});

test('all actual cycles close return paths without external terminals in both geometry families', () => {
  for (const b of inventory.blocks) for (const family of ['leaf', 'root']) {
    const d = constructCycle(catalogs, graph, b, family), c = compactInterface(d, catalogs);
    assert.equal(d.factory_validated, false); assert.equal(d.accepted, false);
    assert.deepEqual([...d.witness.placements.map(p => p.instance)].sort(), b.members);
    assert.equal(d.witness.transport_networks.length, 2);
    assert.ok(d.witness.transport_networks.every(n => n.terminals.every(t => t.endpoint.kind === 'facility')));
    assert.ok(witnessScore(c.witness).area <= witnessScore(d.witness).area);
    assert.deepEqual(compactInterface(c, catalogs).witness, c.witness);
    for (const g of [...c.port_groups.sources, ...c.port_groups.targets]) {
      const p = c.catalog_ports[g.node].find(p => p.id === g.port);
      assert.deepEqual({ x: g.port_x, y: g.port_y }, p.connection);
    }
    assert.equal(Object.hasOwn(c, 'return_plan'), false); assert.deepEqual(c.pre_compaction_plan.return_plan, d.return_plan);
  }
});

test('coverage diagnostics reject independent mutations of an otherwise accepted local report', () => {
  const d = constructCycle(catalogs, graph, inventory.blocks[0]);
  // Coverage-contract unit fixture only. Actual Rust-validated controls live in audit.mjs.
  const report = { covered_geometry_validated: true, recipe_terminals: d.witness.placements.flatMap(p => {
    const recipe = catalogs.recipes.recipes.find(r => r.id === p.recipe);
    return ['input', 'output'].flatMap(direction => recipe[direction + 's'].map(a => {
      const total = multiply(rate(BigInt(a.quantity) * 1000n, recipe.duration_ms), parseRate(d.witness.utilizations[p.instance]));
      const pending = d.block.pending_edges.filter(e => direction === 'output' && e.source === p.instance && e.item === a.item).reduce((s, e) => add(s, parseRate(e.rate)), rate(0));
      return { instance: p.instance, direction, item: a.item, required_rate: dto(total), pending_rate: dto(pending),
        routed_rate: dto(rate(total.n * pending.d - pending.n * total.d, total.d * pending.d)) };
    }));
  }) };
  assert.equal(inspectBlockCoverage(d, report, catalogs).accepted, true);
  for (const [code, mutate] of [
    ['cycle-membership-mismatch', x => x.witness.placements.pop()],
    ['cycle-membership-mismatch', x => x.witness.placements.push(x.witness.placements[0])],
    ['cycle-recipe-mismatch', x => { x.witness.utilizations[x.witness.placements[0].instance].denominator *= 2; }],
    ['cycle-pending-mismatch', x => { x.block.pending_edges[0].rate.numerator++; }],
  ]) {
    const changed = structuredClone(d); mutate(changed);
    const result = inspectBlockCoverage(changed, report, catalogs);
    assert.equal(result.accepted, false); assert.ok(result.diagnostics.some(d => d.code === code), JSON.stringify({ expected: code, diagnostics: result.diagnostics }));
  }
  const missing = structuredClone(report); missing.recipe_terminals.pop();
  assert.ok(inspectBlockCoverage(d, missing, catalogs).diagnostics.some(d => d.code === 'cycle-terminal-report-shape'));
});

test('root return geometry delays splitting and reduces large-cycle footprint', () => {
  const b = inventory.blocks.find(b => b.members.length === 7);
  const leaf = constructCycle(catalogs, graph, b, 'leaf'), root = constructCycle(catalogs, graph, b, 'root');
  assert.equal(leaf.return_plan.perimeter_channels, 6); assert.equal(root.return_plan.perimeter_channels, 4);
  assert.ok(witnessScore(compactInterface(root, catalogs).witness).area < witnessScore(compactInterface(leaf, catalogs).witness).area);
});
