import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { growCandidates, ready, finishOutputs, inspectCoverage, better } from './model.mjs';
import { witnessScore, compactInterface } from '../constructive-auto-interface/compact.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
let output;
try {
  const keys = ['graph', 'baseline', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'candidate-budget', 'route-family', 'placement-family'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const placementFamily = options['placement-family'] ?? 'forward';
  if (!['forward', 'two-sided'].includes(placementFamily)) throw new Error('Expected --placement-family forward or two-sided');
  output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const graph = normalizeGraph(read(options.graph), catalogs), baseline = read(options.baseline);
  let current = read(path.join(baseline.selected_directory, 'detail.json')); const stages = [], start = performance.now(), published = new Set();
  let selectedOutputMs = 0;
  const publish = result => {
    if (published.has(result.directory)) return;
    const start = performance.now();
    write(path.join(result.directory, 'detail.json'), result.detail);
    fs.writeFileSync(path.join(result.directory, 'index.html'), render(result.detail, read(path.join(result.directory, 'validation.json')), catalogs), { flag: 'wx' });
    selectedOutputMs += performance.now() - start; published.add(result.directory);
  };
  const validate = async (detail, name, complete = false) => {
    const ioStart = performance.now();
    const directory = path.join(output, name); fs.mkdirSync(directory);
    write(path.join(directory, 'witness.json'), detail.witness);
    const inputIoMs = performance.now() - ioStart, processStart = performance.now();
    const code = await new Promise((resolve, reject) => {
      const log = fs.createWriteStream(path.join(directory, 'validator.log'), { flags: 'wx' });
      const child = spawn(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
        ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')]);
      const guard = setTimeout(() => child.kill(), 30000); child.stdout.pipe(log, { end: false }); child.stderr.pipe(log, { end: false });
      child.on('error', e => { clearTimeout(guard); log.end(); reject(e); });
      child.on('close', code => { clearTimeout(guard); log.end(); resolve(code); });
    });
    const validationProcessMs = performance.now() - processStart, renderStart = performance.now();
    const validation = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(graph, detail, validation, complete);
    detail = { ...detail, accepted: code === 0 && coverage.accepted, factory_validated: code === 0 && coverage.factory_validated };
    write(path.join(directory, 'coverage.json'), coverage);
    return { directory, detail, accepted: detail.accepted, factory_validated: detail.factory_validated, score: witnessScore(detail.witness),
      validator_ms: validation.elapsed_ms, input_io_ms: inputIoMs, validation_process_ms: validationProcessMs, coverage_io_ms: performance.now() - renderStart,
      diagnostics: [...validation.diagnostics, ...coverage.diagnostics], witness_sha256: hash(path.join(directory, 'witness.json')) };
  };
  let selected = await validate(current, 'baseline');
  if (!selected.accepted) throw new Error('Baseline failed independent geometry or original graph coverage');
  const failures = [];
  while (current.witness.placements.length < graph.nodes.length) {
    const frontier = ready(graph, current.witness).sort((a, b) => a.id.localeCompare(b.id));
    if (!frontier.length) { failures.push({ code: 'closure-no-ready-frontier', severity: 'error', stage: 'frontier' }); break; }
    let accepted = false;
    for (const node of frontier) {
      let generated;
      try { generated = growCandidates(catalogs, graph, current, node, Number(options['candidate-budget'] ?? 64), options['route-family'] ?? 'simple', placementFamily); }
      catch (e) { stages.push({ node: node.id, label: node.label, candidates: [], selected_directory: null,
        failures: [{ code: e.code ?? 'closure-generation-failed', severity: 'error', stage: e.stage ?? 'generation', message: e.message, entity: e.entity }] }); continue; }
      const validationStart = performance.now();
      const checked = [];
      for (let i = 0; i < generated.candidates.length; i += 4) checked.push(...await Promise.all(generated.candidates.slice(i, i + 4).map((d, j) => validate(d, `stage-${stages.length}-candidate-${i + j}`))));
      const valid = checked.filter(c => c.accepted).sort((a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : 0);
      stages.push({ node: node.id, label: node.label, trials: generated.trials, copying: generated.copying, construction_ms: generated.construction_ms, validation_output_ms: performance.now() - validationStart, failures: generated.failures,
        candidates: checked.map(({ detail, ...r }) => r), selected_directory: valid[0]?.directory ?? null });
      if (valid.length) { selected = valid[0]; current = selected.detail; publish(selected); accepted = true; break; }
    }
    if (!accepted) { failures.push({ code: 'closure-all-frontiers-exhausted', severity: 'error', stage: 'frontier' }); break; }
  }
  if (current.witness.placements.length === graph.nodes.length) {
    try {
      const detail = finishOutputs(catalogs, graph, current, options['route-family'] ?? 'simple'), candidates = [detail, compactInterface(detail, catalogs)];
      const checked = await Promise.all(candidates.map((d, i) => validate(d, `complete-${i}`, true)));
      const valid = checked.filter(c => c.factory_validated).sort((a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : 0);
      stages.push({ node: null, label: 'external-outputs', construction_ms: detail.construction_ms, candidates: checked.map(({ detail, ...r }) => r), selected_directory: valid[0]?.directory ?? null });
      if (valid.length) { selected = valid[0]; current = selected.detail; }
    } catch (e) { failures.push({ code: e.code ?? 'closure-output-failed', severity: 'error', stage: e.stage ?? 'outputs', message: e.message, entity: e.entity }); }
  }
  publish(selected);
  const report = { schema_version: 1, status: selected.factory_validated ? 'complete-graph-validated' : 'factory-closure-stopped', factory_validated: selected.factory_validated,
    covered_facilities: current.witness.placements.length, total_facilities: graph.nodes.length, selected_directory: selected.directory,
    elapsed_ms: performance.now() - start, route_family: options['route-family'] ?? 'simple', placement_family: placementFamily, selected_output_ms: selectedOutputMs, score: selected.score, stages, failures,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['model.mjs', 'output-space.mjs', 'run.mjs', '../constructive-cluster-packing/pockets.mjs', '../constructive-cycle-blocks/graph.mjs', '../constructive-auto-interface/compact.mjs', '../constructive-bound-interconnect/view.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), report); fs.copyFileSync(path.join(selected.directory, 'index.html'), path.join(output, 'index.html'), fs.constants.COPYFILE_EXCL);
  console.log(JSON.stringify({ status: report.status, facilities: report.covered_facilities, score: report.score.tuple, output }));
  if (!report.factory_validated) process.exitCode = 2;
} catch (e) {
  const failure = { code: e.code ?? 'closure-execution-failed', severity: 'error', stage: e.stage ?? 'execution', message: e.message, entity: e.entity };
  if (output && fs.existsSync(output)) write(path.join(output, 'failure.json'), failure);
  console.error(JSON.stringify({ code: failure.code, message: failure.message })); process.exitCode = 1;
}
