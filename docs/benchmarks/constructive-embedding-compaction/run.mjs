import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { compactEmbedding } from './model.mjs';
import { executeValidator } from './validator.mjs';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { inspectCoverage, better } from '../constructive-factory-closure/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
let output, outputCreated = false;
try {
  const keys = ['graph', 'baseline', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'rounds', 'axis-order'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const rounds = Number(options.rounds ?? 8), order = options['axis-order'] ?? 'xy';
  if (!Number.isSafeInteger(rounds) || rounds < 1 || rounds > 32 || !['xy', 'yx'].includes(order)) throw new Error('Expected --rounds 1..32 and --axis-order xy or yx');
  output = path.resolve(options['output-directory']); fs.mkdirSync(output); outputCreated = true; const start = performance.now();
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const graph = normalizeGraph(read(options.graph), catalogs), baseline = read(options.baseline), stages = [], failures = [];
  const validate = async (detail, name) => {
    const began = performance.now(), directory = path.join(output, name); fs.mkdirSync(directory);
    write(path.join(directory, 'witness.json'), detail.witness);
    const processResult = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    write(path.join(directory, 'validator-process.json'), processResult);
    if (processResult.timed_out) throw Object.assign(new Error(`Validator time limit exhausted in ${directory}`), { code: 'compaction-validator-timeout', stage: 'validation', entity: directory });
    if (!fs.existsSync(path.join(directory, 'validation.json'))) throw new Error(`Validator produced no report in ${directory}`);
    const v = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(graph, detail, v, true);
    detail.accepted = detail.factory_validated = processResult.code === 0 && coverage.factory_validated;
    write(path.join(directory, 'coverage.json'), coverage); write(path.join(directory, 'detail.json'), detail);
    fs.writeFileSync(path.join(directory, 'index.html'), render(detail, v, catalogs), { flag: 'wx' });
    const { coordinates, lower, original, edges, values, ...certificate } = detail.compaction_certificate ?? {};
    return { detail, directory, factory_validated: detail.factory_validated, score: witnessScore(detail.witness), witness_sha256: hash(path.join(directory, 'witness.json')),
      validation_output_ms: performance.now() - began, validator_ms: v.elapsed_ms, timings: detail.compaction_timings, certificate,
      diagnostics: [...v.diagnostics, ...coverage.diagnostics] };
  };
  let selected = await validate(read(path.join(baseline.selected_directory, 'detail.json')), 'incumbent');
  if (!selected.factory_validated) throw new Error('Complete incumbent failed independent original-graph validation');
  const initialScore = selected.score;
  let termination = 'round-budget';
  for (let round = 0; round < rounds; round++) {
    let improved = false;
    for (const axis of order) {
      try {
        const candidate = compactEmbedding(selected.detail, catalogs, axis), r = await validate(candidate, `round-${round}-${axis}`);
        const accepted = r.factory_validated && better(r.score.tuple, selected.score.tuple);
        stages.push({ round, axis, ...Object.fromEntries(Object.entries(r).filter(([k]) => k !== 'detail')), selected: accepted });
        if (accepted) { selected = r; improved = true; }
      } catch (e) { failures.push({ round, axis, code: e.code ?? 'compaction-candidate-failed', severity: 'error', stage: e.stage ?? 'embedding-compaction', message: e.message, entity: e.entity ?? null }); }
    }
    if (!improved) { termination = failures.length ? 'no-improvement-with-diagnostics' : 'unimproved-axis-pair'; break; }
  }
  const report = { schema_version: 1, factory_validated: true, status: better(selected.score.tuple, initialScore.tuple) ? 'complete-factory-improved' : 'complete-incumbent-retained',
    selected_directory: selected.directory, initial_score: initialScore, score: selected.score, axis_order: order, round_budget: rounds, termination,
    elapsed_ms: performance.now() - start, stages, failures,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['model.mjs', 'length.mjs', 'validator.mjs', 'run.mjs', '../constructive-placement-only/model.mjs', '../constructive-bound-interconnect/view.mjs', '../constructive-auto-interface/compact.mjs', '../constructive-factory-closure/model.mjs', '../constructive-cycle-blocks/graph.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), report); fs.copyFileSync(path.join(selected.directory, 'index.html'), path.join(output, 'index.html'));
  console.log(JSON.stringify({ status: report.status, score: report.score.tuple, stages: stages.length, elapsed_ms: report.elapsed_ms, output }));
} catch (e) {
  const diagnostic = { code: e.code ?? 'compaction-execution-failed', severity: 'error', stage: e.stage ?? 'embedding-compaction', message: e.message, entity: e.entity ?? null };
  if (outputCreated) try { write(path.join(output, 'failure.json'), diagnostic); }
  catch (reportError) { diagnostic.report_error = reportError.message; }
  console.error(JSON.stringify(diagnostic)); process.exitCode = 1;
}
