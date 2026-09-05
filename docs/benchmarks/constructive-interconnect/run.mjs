import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { census } from './census.mjs';
import { recipeInterface } from './fixture.mjs';
import { render } from './view.mjs';
try {
  const inputKeys = ['recipes', 'items', 'facilities', 'transports', 'components', 'fixture'];
  const keys = [...inputKeys, 'report-output', 'visualization-output'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = path.resolve(args[i + 1]);
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const outputs = ['report-output', 'visualization-output'].map(k => options[k]);
  if (new Set(outputs).size !== 2 || outputs.some(p => fs.existsSync(p))) throw new Error('Output paths must be distinct new files');
  const inputs = {}, catalogs = {};
  for (const k of inputKeys) {
    const bytes = fs.readFileSync(options[k]); catalogs[k] = JSON.parse(bytes);
    inputs[k] = { path: options[k], sha256: createHash('sha256').update(bytes).digest('hex') };
  }
  const start = performance.now(), catalogCensus = census(catalogs), censusMs = performance.now() - start;
  const fixture = catalogs.fixture;
  if (fixture.schema_version !== 1) throw new Error('Unsupported fixture schema');
  const minimum = catalogCensus.recipes.find(r => r.recipe === fixture.minimum_case.recipe);
  const maximum = catalogCensus.recipes.find(r => r.recipe === catalogCensus.maxima[fixture.catalog_max_case.select]?.witnesses[0]);
  if (!minimum || !maximum) throw new Error('Fixture references unknown recipe or census metric');
  const geometryStart = performance.now();
  const examples = { minimum: recipeInterface(minimum, fixture.minimum_case, catalogs), maximum: recipeInterface(maximum, fixture.catalog_max_case, catalogs) };
  const result = { schema_version: 1, status: 'structural-interconnect-research', factory_validated: false,
    census: catalogCensus, examples, timing_ms: { census: censusMs, geometry_and_checker: performance.now() - geometryStart },
    provenance: { inputs, node_version: process.version, source_sha256: Object.fromEntries(['census.mjs', 'rates.mjs', 'geometry.mjs', 'fixture.mjs', 'run.mjs'].map(name =>
      [name, createHash('sha256').update(fs.readFileSync(new URL(name, import.meta.url))).digest('hex')])) } };
  fs.writeFileSync(outputs[0], JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  fs.writeFileSync(outputs[1], render(result), { flag: 'wx' });
  console.log(JSON.stringify({ status: result.status, recipes: catalogCensus.counts.recipes,
    maximum_input_items: catalogCensus.maxima.input_distinct_items.maximum,
    examples: Object.fromEntries(Object.entries(examples).map(([k, f]) => [k, { lanes: f.geometry.lanes.length, bridges: f.geometry.components.length, geometry_valid: f.geometry.certificate.valid }])),
    timing_ms: result.timing_ms, report_output: outputs[0] }));
} catch (error) {
  console.error(JSON.stringify({ code: error.code ?? 'interconnect-diagnostic-error', severity: 'error', stage: error.stage ?? 'structural-interconnect', entity: error.entity ?? null, message: error.message }));
  process.exitCode = 1;
}
