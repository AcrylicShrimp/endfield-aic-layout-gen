import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { inspectBlockCoverage } from '../constructive-cycle-blocks/model.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
try {
  const keys = ['baseline', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const baseline = read(options.baseline);
  if (baseline.status !== 'composition-frontiers-validated' || !baseline.selected_directory) throw new Error('Expected a validated composition baseline');
  const original = read(path.join(baseline.selected_directory, 'detail.json')), controls = [];
  const plan = original.composition_plan ?? original.pre_compaction_plan?.composition_plan;
  const retained = new Set(plan.consumed_edges.map(e => e.source));
  const missingCut = structuredClone(original);
  missingCut.block.pending_edges = missingCut.block.pending_edges.filter(e => !retained.has(e.source));
  if (missingCut.block.pending_edges.length === original.block.pending_edges.length) throw new Error('Baseline must leave a shared supplier remainder pending');
  controls.push({ name: 'missing-shared-supplier-remainder', detail: missingCut, geometry: true, accepted: false });
  const broken = structuredClone(original); broken.witness.transport_networks.find(n => n.segments.length).segments.pop();
  controls.push({ name: 'broken-retained-interior', detail: broken, geometry: false, accepted: false });
  const missing = structuredClone(original), network = missing.witness.transport_networks.find(n => plan.consumed_edges.some(e => e.item === n.item));
  missing.witness.transport_networks = missing.witness.transport_networks.filter(n => n !== network);
  missing.witness.logistics_components = missing.witness.logistics_components.filter(c => !network.component_ids.includes(c.id));
  controls.push({ name: 'missing-frontier-commodity', detail: missing, geometry: true, accepted: false });
  const duplicate = structuredClone(original); duplicate.witness.placements.push(duplicate.witness.placements[0]);
  controls.push({ name: 'duplicate-retained-owner', detail: duplicate, geometry: false, accepted: false });
  const utilization = structuredClone(original); utilization.witness.utilizations[plan.frontier[0]].denominator *= 2;
  controls.push({ name: 'changed-frontier-utilization', detail: utilization, geometry: false, accepted: false });
  controls.push({ name: 'unchanged-composition', detail: original, geometry: true, accepted: true });
  const results = [];
  for (const c of controls) {
    const witnessPath = path.join(output, c.name + '.json'), validationPath = path.join(output, c.name + '-validation.json');
    write(witnessPath, c.detail.witness);
    const result = spawnSync(path.resolve(options.validator), ['--witness', witnessPath,
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', validationPath],
    { encoding: 'utf8', timeout: 30000, maxBuffer: 1024 * 1024 });
    fs.writeFileSync(path.join(output, c.name + '.log'), (result.stdout ?? '') + (result.stderr ?? ''), { flag: 'wx' });
    const validation = read(validationPath), coverage = inspectBlockCoverage(c.detail, validation, catalogs);
    results.push({ name: c.name, passed: result.status === (c.geometry ? 0 : 2) && validation.covered_geometry_validated === c.geometry && coverage.accepted === c.accepted,
      covered_geometry_validated: validation.covered_geometry_validated, coverage_accepted: coverage.accepted,
      diagnostics: [...validation.diagnostics, ...coverage.diagnostics], witness_sha256: hash(witnessPath) });
  }
  const report = { schema_version: 1, all_passed: results.every(r => r.passed), cases: results,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['audit.mjs', '../constructive-cycle-blocks/model.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'audit.json'), report);
  console.log(JSON.stringify({ status: report.all_passed ? 'composition-controls-passed' : 'composition-controls-failed', cases: results.length, output }));
  if (!report.all_passed) process.exitCode = 1;
} catch (e) { console.error(JSON.stringify({ code: 'composition-audit-failed', severity: 'error', stage: 'audit', message: e.message })); process.exitCode = 1; }
