import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { releaseBridgeDraft } from './components.mjs';
import { reembed } from './model.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { inspectCoverage } from '../constructive-factory-closure/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
try {
  const options = {}, args = process.argv.slice(2), keys = ['baseline', 'graph', 'catalog-directory', 'validator', 'output-directory'];
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2); if (!args[i].startsWith('--') || !keys.includes(k) || k in options || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const paths = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, path.join(options['catalog-directory'], `${k === 'components' ? 'logistics-components' : k}.json`)]));
  const catalogs = Object.fromEntries(Object.entries(paths).map(([k, p]) => [k, read(p)])), graph = normalizeGraph(read(options.graph), catalogs);
  const baseline = read(options.baseline), source = path.join(baseline.selected_directory, 'detail.json'), detail = read(source), snapshot = JSON.stringify(detail);
  if (!baseline.factory_validated) throw new Error('Expected a validated complete baseline');
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const validate = async (d, name) => {
    const directory = path.join(output, name); fs.mkdirSync(directory); const wp = path.join(directory, 'witness.json'), vp = path.join(directory, 'validation.json');
    write(wp, d.witness);
    const process = await executeValidator(path.resolve(options.validator), ['--witness', wp, ...Object.entries(paths).flatMap(([k, p]) => [`--${k}`, p]), '--report-output', vp], path.join(directory, 'validator.log'));
    const v = read(vp), coverage = inspectCoverage(graph, d, v, true); write(path.join(directory, 'coverage.json'), coverage);
    const valid = coverage.factory_validated && process.code === 0 && !process.timed_out;
    const result = { ...d, accepted: valid, factory_validated: valid }; write(path.join(directory, 'detail.json'), result);
    if (valid) fs.writeFileSync(path.join(directory, 'index.html'), render(result, v, catalogs), { flag: 'wx' });
    return { directory, geometry: v.covered_geometry_validated, factory_validated: valid, code: process.code, score: witnessScore(d.witness), witness_sha256: hash(wp) };
  };
  const initial = await validate(detail, 'unchanged'), records = [], ids = detail.witness.logistics_components.filter(c => c.kind === 'bridge').map(c => c.id);
  for (const [i, releaseBridges] of [...ids.map(id => [id]), ids].entries()) {
    const draft = releaseBridgeDraft(detail.witness, catalogs, releaseBridges);
    records.push({ releaseBridges, kind: 'unrestored-negative', ...await validate({ ...detail, witness: draft.witness }, `unrestored-${i}`) });
    for (const order of ['near', 'far']) {
      const r = reembed(detail, catalogs, { releaseBridges, order, endpointPolicy: 'terminal-flexible', maxNewCrossings: 8 });
      const record = { releaseBridges, order, kind: 'restored', status: r.status, diagnostics: r.diagnostics, routes: r.routes };
      if (r.status === 'candidate') Object.assign(record, await validate(r.detail, `restored-${i}-${order}`));
      records.push(record);
    }
  }
  const restored = records.filter(r => r.kind === 'restored' && r.status === 'candidate');
  const passed = initial.factory_validated && restored.length > 0 && restored.every(r => r.factory_validated) &&
    records.filter(r => r.kind === 'unrestored-negative').every(r => r.code === 2 && !r.geometry && !r.factory_validated) && JSON.stringify(detail) === snapshot;
  write(path.join(output, 'audit.json'), { schema_version: 1, all_passed: passed, restored_candidates: restored.length, initial, records,
    source_sha256: hash(source), provenance: [options.baseline, options.graph, options.validator, ...Object.values(paths)].map(p => ({ path: path.resolve(p), sha256: hash(p) })),
    source_hashes: Object.fromEntries(['bridge-release-audit.mjs', 'components.mjs', 'model.mjs', 'movement.mjs', 'path.mjs', 'crossing.mjs', '../constructive-factory-closure/model.mjs', '../constructive-bound-interconnect/view.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) });
  console.log(JSON.stringify({ status: passed ? 'bridge-release-controls-passed' : 'bridge-release-controls-failed', restored: restored.length, output }));
  if (!passed) process.exitCode = 1;
} catch (e) { console.error(JSON.stringify({ code: 'bridge-release-audit-failed', message: e.message })); process.exitCode = 1; }
