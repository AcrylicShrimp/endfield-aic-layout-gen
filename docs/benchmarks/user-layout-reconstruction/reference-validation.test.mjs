// Integration contract: build the independent release probe before running this suite.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import test from 'node:test';
import { reconstruct, verifyReferenceMargins } from './model.mjs';
const root = fileURLToPath(new URL('../../../', import.meta.url));
const read = p => JSON.parse(fs.readFileSync(p));
const keys = ['recipes', 'facilities', 'items', 'transports', 'components'];
const paths = Object.fromEntries(keys.map(k => [k, path.join(root, 'data/game/normalized', `${k === 'components' ? 'logistics-components' : k}.json`)]));
const catalogs = Object.fromEntries(keys.map(k => [k, read(paths[k])]));
const buildings = read(path.join(root, 'data/game/source/table-cfg/FactoryBuildingTable.json'));
const probe = path.join(root, 'target/release/aic-witness-probe');
const specFor = name => read(new URL(`./${name}.json`, import.meta.url));
function validate(t, witness) {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-reference-test-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const input = path.join(directory, 'witness.json'), output = path.join(directory, 'validation.json');
  fs.writeFileSync(input, JSON.stringify(witness) + '\n', { flag: 'wx' });
  const process = spawnSync(probe, ['--witness', input, ...keys.flatMap(k => [`--${k}`, paths[k]]), '--report-output', output], { encoding: 'utf8', timeout: 30000 });
  assert.ifError(process.error); assert.notEqual(process.status, 1, process.stderr);
  return { status: process.status, report: read(output) };
}
test('three recipe families independently validate with exactly the declared external margins', t => {
  for (const [name, count, margins] of [['medium-battery-01', 8, 6], ['citron-powder-03', 4, 1], ['enriched-iron-01', 9, 3]]) {
    const s = specFor(name), d = reconstruct(s, catalogs, buildings), checked = validate(t, d.witness);
    assert.equal(checked.status, 0);
    assert.equal(checked.report.covered_geometry_validated, true);
    assert.equal(d.witness.placements.length, count);
    assert.equal(verifyReferenceMargins(s, checked.report).expected_pending.length, margins);
    assert.equal(checked.report.recipe_coverage_complete, false);
    assert.equal(checked.report.factory_validated, false);
    if (name === 'medium-battery-01') assert.equal(createHash('sha256').update(JSON.stringify(d.witness, null, 2) + '\n').digest('hex'),
      '5c313c90670156cba95fcc0e6fe3954b43532c2dd370fb7a06550ca9b7b39480', 'schema cutover must preserve the original witness byte for byte');
  }
});
test('omitting an internal route passes partial geometry but fails the reference boundary check', t => {
  const s = specFor('citron-powder-03'); s.routes = s.routes.filter(r => r.id !== 'product-seed');
  const checked = validate(t, reconstruct(s, catalogs, {}).witness);
  assert.equal(checked.status, 0);
  assert.equal(checked.report.covered_geometry_validated, true);
  assert.throws(() => verifyReferenceMargins(s, checked.report), /declared external obligations/);
});
test('the independent validator rejects overallocated split flow and body overlap', t => {
  const s = specFor('citron-powder-03'), original = reconstruct(s, catalogs, {}).witness;
  const overlap = structuredClone(original); overlap.placements[1].x = 0;
  assert.equal(validate(t, overlap).status, 2);
  const overflow = structuredClone(original);
  overflow.utilizations.seed = { numerator: 3, denominator: 4 };
  const checked = validate(t, overflow);
  assert.equal(checked.status, 2);
  assert.equal(checked.report.covered_geometry_validated, false);
});
test('CLI rejects incorrect expected boundaries despite valid covered geometry', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-reference-cli-'));
  t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const s = specFor('citron-powder-03'); s.expected_pending[0].rate.denominator = 2;
  const input = path.join(directory, 'case.json'), source = path.join(directory, 'source-provenance.bin'), output = path.join(directory, 'output');
  fs.writeFileSync(input, JSON.stringify(s), { flag: 'wx' });
  // The harness hashes but does not decode the image; this is a provenance-only test input.
  fs.writeFileSync(source, 'synthetic provenance input, not a screenshot', { flag: 'wx' });
  const result = spawnSync(process.execPath, [fileURLToPath(new URL('./run.mjs', import.meta.url)), '--case', input, '--source-image', source,
    ...keys.flatMap(k => [`--${k}`, paths[k]]), '--validator', probe, '--output-directory', output], { encoding: 'utf8', timeout: 30000 });
  assert.ifError(result.error); assert.equal(result.status, 2, result.stderr);
  const r = read(path.join(output, 'report.json'));
  assert.equal(r.schema_version, 2);
  assert.equal(r.status, 'reconstruction-rejected');
  assert.equal(r.factory_validated, false);
  assert.equal(r.reference_margin_check.status, 'reference-margins-rejected');
  assert.equal(read(path.join(output, 'validation.json')).covered_geometry_validated, true);
});
