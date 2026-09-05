import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { inspectCoverage } from './model.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
try {
  const keys = ['graph', 'baseline', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const graph = normalizeGraph(read(options.graph), catalogs), report = read(options.baseline);
  if (!report.factory_validated) throw new Error('Expected a complete original-graph baseline');
  const original = read(path.join(report.selected_directory, 'detail.json')), controls = [];
  const unknown = structuredClone(original), t = unknown.witness.transport_networks.flatMap(n => n.terminals).find(t => t.endpoint.kind === 'external');
  t.node = t.endpoint.node = 'undeclared-external'; controls.push({ name: 'undeclared-external-substitution', detail: unknown, geometry: true });
  const missing = structuredClone(original), n = missing.witness.transport_networks.find(n => n.terminals.some(t => t.endpoint.kind === 'external' && t.direction === 'input'));
  missing.witness.transport_networks = missing.witness.transport_networks.filter(q => q !== n);
  missing.witness.logistics_components = missing.witness.logistics_components.filter(c => !n.component_ids.includes(c.id));
  controls.push({ name: 'omitted-export-network', detail: missing, geometry: true });
  const broken = structuredClone(original); broken.witness.transport_networks.find(n => n.transport === 'pipe' && n.segments.length).segments.pop();
  controls.push({ name: 'broken-pipe-segment', detail: broken, geometry: false });
  const duplicate = structuredClone(original); duplicate.witness.placements.push(duplicate.witness.placements[0]);
  controls.push({ name: 'duplicate-facility', detail: duplicate, geometry: false });
  const u = structuredClone(original); u.witness.utilizations[u.witness.placements.at(-1).instance].denominator *= 2;
  controls.push({ name: 'changed-final-utilization', detail: u, geometry: false });
  controls.push({ name: 'omitted-terminal-report', detail: original, geometry: true, mutate: v => { v.recipe_terminals.pop(); } });
  controls.push({ name: 'inflated-terminal-report', detail: original, geometry: true, mutate: v => { v.recipe_terminals[0].required_rate.numerator *= 2; } });
  controls.push({ name: 'unchanged-complete-factory', detail: original, geometry: true, accepted: true });
  const results = [];
  for (const c of controls) {
    const wp = path.join(output, c.name + '.json'), vp = path.join(output, c.name + '-validation.json'); write(wp, c.detail.witness);
    const child = spawnSync(path.resolve(options.validator), ['--witness', wp, ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', vp],
      { encoding: 'utf8', timeout: 30000, maxBuffer: 1024 * 1024 });
    fs.writeFileSync(path.join(output, c.name + '.log'), (child.stdout ?? '') + (child.stderr ?? ''), { flag: 'wx' });
    const v = read(vp); c.mutate?.(v); if (c.mutate) write(path.join(output, c.name + '-mutated-report.json'), v);
    const coverage = inspectCoverage(graph, c.detail, v, true);
    results.push({ name: c.name, passed: child.status === (c.geometry ? 0 : 2) && v.covered_geometry_validated === c.geometry && coverage.factory_validated === (c.accepted ?? false),
      geometry: v.covered_geometry_validated, factory_validated: coverage.factory_validated, diagnostics: [...v.diagnostics, ...coverage.diagnostics], witness_sha256: hash(wp) });
  }
  const result = { schema_version: 1, all_passed: results.every(r => r.passed), cases: results,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['audit.mjs', 'model.mjs', '../constructive-cycle-blocks/graph.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'audit.json'), result);
  console.log(JSON.stringify({ status: result.all_passed ? 'factory-controls-passed' : 'factory-controls-failed', cases: results.length, output }));
  if (!result.all_passed) process.exitCode = 1;
} catch (e) { console.error(JSON.stringify({ code: 'factory-audit-failed', message: e.message })); process.exitCode = 1; }
