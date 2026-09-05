import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { constructInterface } from '../constructive-auto-interface/model.mjs';
import { inspectBlockCoverage } from './model.mjs';
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
  const baseline = read(options.baseline), selected = [...baseline.selected].sort((a, b) => b.facilities - a.facilities)[0];
  if (!selected || baseline.status !== 'covered-cycle-blocks-validated') throw new Error('Expected completed covered-cycle baseline');
  const original = read(path.join(selected.directory, 'detail.json')), controls = [];
  const broken = structuredClone(original); broken.witness.transport_networks.find(n => n.item === broken.block.return_item).segments.pop();
  controls.push({ name: 'broken-return-segment', detail: broken, geometry: false, accepted: false });
  const removed = structuredClone(original), network = removed.witness.transport_networks.find(n => n.item === removed.block.return_item);
  removed.witness.transport_networks = removed.witness.transport_networks.filter(n => n !== network);
  removed.witness.logistics_components = removed.witness.logistics_components.filter(c => !network.component_ids.includes(c.id));
  controls.push({ name: 'missing-return-network', detail: removed, geometry: true, accepted: false });
  const fake = structuredClone(original); fake.witness = constructInterface(catalogs, fake.block.request).witness;
  controls.push({ name: 'fake-external-return', detail: fake, geometry: true, accepted: false });
  const missingCut = structuredClone(original); missingCut.block.pending_edges = [];
  controls.push({ name: 'undeclared-pending-output', detail: missingCut, geometry: true, accepted: false });
  const duplicate = structuredClone(original); duplicate.witness.placements.push(duplicate.witness.placements[0]);
  controls.push({ name: 'duplicate-facility-owner', detail: duplicate, geometry: false, accepted: false });
  const missing = structuredClone(original); missing.witness.placements.pop(); delete missing.witness.utilizations[original.witness.placements.at(-1).instance];
  controls.push({ name: 'missing-facility-owner', detail: missing, geometry: false, accepted: false });
  const utilization = structuredClone(original); utilization.witness.utilizations[original.witness.placements[0].instance].denominator *= 2;
  controls.push({ name: 'changed-utilization', detail: utilization, geometry: false, accepted: false });
  controls.push({ name: 'unchanged-cycle', detail: original, geometry: true, accepted: true });
  const results = [];
  for (const c of controls) {
    const witnessPath = path.join(output, c.name + '.json'), validationPath = path.join(output, c.name + '-validation.json');
    write(witnessPath, c.detail.witness);
    const result = spawnSync(path.resolve(options.validator), ['--witness', witnessPath,
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', validationPath],
      { encoding: 'utf8', timeout: 30000, maxBuffer: 1024 * 1024 });
    fs.writeFileSync(path.join(output, c.name + '.log'), (result.stdout ?? '') + (result.stderr ?? ''), { flag: 'wx' });
    const validation = read(validationPath), coverage = inspectBlockCoverage(c.detail, validation, catalogs), accepted = coverage.accepted;
    results.push({ name: c.name, passed: result.status === (c.geometry ? 0 : 2) && validation.covered_geometry_validated === c.geometry && accepted === c.accepted,
      covered_geometry_validated: validation.covered_geometry_validated, cycle_coverage_accepted: accepted,
      diagnostics: [...validation.diagnostics, ...coverage.diagnostics], witness_sha256: hash(witnessPath) });
  }
  const report = { schema_version: 1, all_passed: results.every(r => r.passed), cases: results,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['audit.mjs', 'model.mjs', 'graph.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'audit.json'), report);
  console.log(JSON.stringify({ status: report.all_passed ? 'cycle-controls-passed' : 'cycle-controls-failed', cases: results.length, output }));
  if (!report.all_passed) process.exitCode = 1;
} catch (e) { console.error(JSON.stringify({ code: 'cycle-audit-failed', severity: 'error', stage: 'audit', message: e.message })); process.exitCode = 1; }
