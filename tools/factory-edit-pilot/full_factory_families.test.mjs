import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { validateStudy, wiringRequest } from './full_factory_families.mjs';

const plan = () => JSON.parse(fs.readFileSync(new URL('./full-factory-family-study.json', import.meta.url)));
test('opened family assignments and rate variants are valid before generation', () => {
  assert.doesNotThrow(() => validateStudy(plan()));
  for (const mutation of [
    p => { p.families[1].split = 'test'; },
    p => { p.families[1].id = p.families[0].id; },
    p => { p.families[1].sources[0].path = p.families[0].sources[0].path; },
    p => { p.families[0].rates.push({ quantity: 2, duration_ms: 200000 }); },
    p => { p.construction.elapsed_ms = 0; },
    p => { p.max_facilities = 257; },
  ]) {
    const p = plan(); mutation(p); assert.throws(() => validateStudy(p));
  }
});

test('original graph extraction preserves external identities and every required edge', () => {
  const wiring = { schema_version: 1, success: true, nodes: [
    { kind: 'external', id: 'water' }, { kind: 'target', id: 'target' },
    { kind: 'surplus', id: 'surplus' },
    { kind: 'facility', id: 'a', recipe: 'r', facility: 'f' },
    { kind: 'facility', id: 'b', recipe: 's', facility: 'g' },
  ], edges: [
    { id: 'input', source: 'water', target: 'a' },
    { id: 'forward', source: 'a', target: 'b' },
    { id: 'return', source: 'b', target: 'a' },
    { id: 'output', source: 'b', target: 'target' },
    { id: 'byproduct', source: 'b', target: 'surplus' },
  ] };
  const original = structuredClone(wiring), r = wiringRequest(wiring);
  assert.deepEqual(r.internal_requirements.map(e => e.id), ['forward', 'return']);
  assert.deepEqual(r.external_requirements.map(e => e.id), ['input', 'output', 'byproduct']);
  assert.equal(r.placements.length, 2);
  assert.deepEqual(wiring, original);
  assert.throws(() => wiringRequest({ ...wiring, success: false }));
});
