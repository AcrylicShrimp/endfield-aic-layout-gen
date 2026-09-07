import test from 'node:test';
import assert from 'node:assert/strict';
import { read } from '../layout-rl/io.mjs';
import { makeRequest, validatePlan } from './corpus.mjs';
import { prepareFactory } from '../chain-prototype/factory-input.mjs';

const plan = read(new URL('./corpus.json', import.meta.url));
const catalogs = Object.fromEntries(Object.entries(plan.catalogs).map(([k, p]) => [k, read(new URL(p, import.meta.url))]));

test('catalog-derived requests preserve exact recipe ledgers and preassigned family splits', () => {
  validatePlan(plan);
  let supported = 0, medium = 0, unsupported = 0;
  const snapshot = JSON.stringify([plan, catalogs]);
  for (const family of plan.families) for (const depth of plan.depths) for (const u of plan.utilizations) {
    try {
      const { request, procurement } = makeRequest(plan, family, depth, u, 'straight', catalogs);
      const graph = prepareFactory(request, catalogs);
      assert.equal(graph.nodes.size, request.nodes.length);
      assert.equal(graph.order.length, request.nodes.length);
      assert.equal(procurement.length, request.external.length);
      assert.equal(request.nodes[0].recipe, family.recipe);
      assert.ok(request.nodes.every(n => n.utilization.numerator <= n.utilization.denominator));
      assert.ok(request.nodes.length <= 16);
      assert.equal(request.edges.length, request.nodes.length - 1);
      assert.ok(graph.lanes.every(l => l.transport === 'belt'));
      supported++; medium += Number(request.nodes.length >= 10);
    } catch (e) {
      assert.equal(e.code, 'rl-next-unsupported-request', e.message); unsupported++;
    }
  }
  assert.ok(supported >= 20); assert.ok(medium >= 2); assert.ok(unsupported > 0);
  assert.equal(JSON.stringify([plan, catalogs]), snapshot);
});

test('constructor variants change proposal ordering but not the declared factory', () => {
  const family = plan.families.find(f => f.id === 'advanced-battery');
  const a = makeRequest(plan, family, 4, plan.utilizations[0], 'straight', catalogs).request;
  const b = makeRequest(plan, family, 4, plan.utilizations[0], 'fold', catalogs).request;
  for (const key of ['nodes', 'edges', 'external', 'boundary', 'budget']) assert.deepEqual(a[key], b[key]);
  assert.equal(a.nodes.length, 12);
  assert.throws(() => makeRequest(plan, family, 4, plan.utilizations[1], 'fold', catalogs), { code: 'rl-next-unsupported-request' });
});

test('invalid splits, duplicated families and oversized matrices fail before construction', () => {
  for (const changed of [{ ...plan, schema_version: 9 }, { ...plan, families: [...plan.families, plan.families[0]] },
    { ...plan, families: [{ ...plan.families[0], split: 'mixed' }] }, { ...plan, depths: [99] },
    { ...plan, utilizations: [{ numerator: 2, denominator: 1 }] }, { ...plan, total_elapsed_ms: 0 }])
    assert.throws(() => validatePlan(changed), { code: 'rl-next-input-invalid' });
});
