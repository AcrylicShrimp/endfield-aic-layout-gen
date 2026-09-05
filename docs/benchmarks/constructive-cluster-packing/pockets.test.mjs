import test from 'node:test';
import assert from 'node:assert/strict';
import { probeSpace, spaceFailures } from './pockets.mjs';
import { consumerSpaceDemands } from './nesting.mjs';
const bounds = { minX: -2, minY: -2, maxX: 12, maxY: 8 };
const pocket = (x, y) => !(x >= 0 && x <= 3 && y >= 0 && y <= 1);
const group = (minimum_cells, ports = [{ id: 'p', x: 0, y: 0 }]) => ({ instance: 'supplier', transport: 'belt', ports,
  demands: [{ target: 'consumer', lanes: 1, minimum_cells, escape: false }] });

test('sealed eight-cell output pocket rejects a 25-cell consumer but not a sufficient threshold', () => {
  const p = probeSpace({ x: 0, y: 0 }, pocket, bounds, 26);
  assert.equal(p.count, 8); assert.equal(p.escaped, false); assert.equal(p.capped, false);
  const failures = spaceFailures([group(26)], pocket, bounds);
  assert.equal(failures[0].code, 'nesting-output-pocket-too-small'); assert.equal(failures[0].ports[0].count, 8);
  assert.equal(spaceFailures([group(9)], pocket, bounds).length, 1);
  assert.deepEqual(spaceFailures([group(8)], pocket, bounds), []);
});

test('halo escape and exhausted budgets remain eligible; blocked starts never do', () => {
  assert.deepEqual(spaceFailures([group(26)], () => false, bounds), []);
  assert.deepEqual(spaceFailures([group(26)], pocket, bounds, 4), []);
  const external = group(null); external.demands[0].escape = true;
  assert.equal(spaceFailures([external], pocket, bounds).length, 1);
  assert.deepEqual(spaceFailures([external], pocket, bounds, 4), []);
  assert.deepEqual(spaceFailures([external], () => false, bounds), []);
  assert.equal(spaceFailures([group(1, [{ id: 'blocked', x: 4, y: 4 }])], pocket, bounds).length, 1);
});

test('nested-threshold matching reserves distinct large-pocket ports for larger demands', () => {
  const blocked = (x, y) => !((x >= 0 && x < 2 && y >= 0 && y < 2) || (x >= 5 && x < 9 && y >= 0 && y < 2));
  const g = group(8, [{ id: 'small', x: 0, y: 0 }, { id: 'large', x: 5, y: 0 }]);
  g.demands.push({ target: 'second', lanes: 1, minimum_cells: 5, escape: false });
  const f = spaceFailures([g], blocked, bounds)[0];
  assert.equal(f.required_ports_at_threshold, 2); assert.equal(f.available, 1);
  g.demands[1].minimum_cells = 4; assert.deepEqual(spaceFailures([g], blocked, bounds), []);
  g.demands.reverse(); assert.deepEqual(spaceFailures([g], blocked, bounds), []);
  g.demands[0].lanes = 2; assert.equal(spaceFailures([g], blocked, bounds).length, 1);
});

test('opposite-layer geometry is supplied by the caller and cached regions never cross transport kinds', () => {
  const belt = group(26), pipe = { ...group(26), transport: 'pipe' };
  const f = spaceFailures([belt, pipe], (x, y, layer) => layer === 'belt' ? pocket(x, y) : false, bounds);
  assert.equal(f.length, 1); assert.equal(f[0].transport, 'belt');
});

test('sorted threshold matching agrees with exhaustive distinct-port assignment', () => {
  let seed = 91; const next = n => ((seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0) % n);
  for (let sample = 0; sample < 200; sample++) {
    const sizes = Array.from({ length: 3 }, () => 1 + next(12)), needs = Array.from({ length: 3 }, () => 1 + next(12));
    const g = group(0, sizes.map((_, i) => ({ id: `p${i}`, x: i * 20, y: 0 })));
    g.demands = needs.map((n, i) => ({ target: `t${i}`, lanes: 1, minimum_cells: n, escape: false }));
    const blocked = (x, y) => y !== 0 || !sizes.some((n, i) => x >= i * 20 && x < i * 20 + n);
    const search = (i, used) => i === needs.length || sizes.some((n, p) => !used.has(p) && n >= needs[i] && search(i + 1, new Set([...used, p])));
    assert.equal(spaceFailures([g], blocked, { minX: -2, maxX: 60, minY: -2, maxY: 2 }).length === 0, search(0, new Set()));
  }
});

test('every constructed rectangular consumer plus adjacent connection passes the necessary size condition', () => {
  for (let width = 1; width <= 5; width++) for (let height = 1; height <= 5; height++) {
    const blocked = (x, y) => !((x >= 1 && x <= width && y >= 0 && y < height) || (x === 0 && y === 0));
    assert.deepEqual(spaceFailures([group(width * height + 1)], blocked, bounds), []);
  }
});

test('consumer requirements come from recipe facility geometry and exact runtime transport capacity', () => {
  const catalogs = { recipes: { recipes: [{ id: 'recipe', facility: 'fixture' }] },
    facilities: { facilities: [{ id: 'fixture', footprint: { width: 3, height: 7 } }] },
    transports: { transports: [{ kind: 'belt', capacity: { quantity: 1, duration_ms: 1000 } }] } };
  const edge = target => ({ source: 'supplier', target, transport: 'belt', rate: { numerator: 4, denominator: 3 } });
  const graph = { nodes: [{ id: 'consumer', recipe: 'recipe' }], edges: [edge('consumer'), edge('external'), edge('already-owned')] };
  const d = consumerSpaceDemands(graph, [group(0)], new Set(['already-owned']), catalogs)[0].demands;
  assert.deepEqual(d, [{ target: 'consumer', minimum_cells: 22, escape: false, lanes: 2 },
    { target: 'external', minimum_cells: null, escape: true, lanes: 2 }]);
  assert.throws(() => probeSpace({ x: 0, y: 0 }, pocket, bounds, 0), { code: 'nesting-space-schema' });
});
