import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { selectLookaheadSeeds, selectGlobalFactory, mapLookaheadBranches } from './lookahead.mjs';
import { executeLookaheadProcess } from './lookahead-process.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { inspectCoverage } from '../constructive-factory-closure/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const diagnostic = (code, message, entity = null) => ({ code, severity: 'error', stage: 'lookahead', message, entity });
const fail = (code, message, entity) => { throw Object.assign(new Error(message), diagnostic(code, message, entity)); };
const sourceDirectory = fileURLToPath(new URL('.', import.meta.url)), benchmarkRoot = path.resolve(sourceDirectory, '..');
const publicRecord = ({ witness, detail, ...record }) => record;
let output, created = false;
try {
  const keys = ['graph', 'baseline', 'seed-report', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'];
  const args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'seed-limit', 'rounds', 'workers'].includes(k) || Object.hasOwn(options, k) || !args[i + 1]) fail('lookahead-input', `Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) fail('lookahead-input', `Missing --${k}`);
  const limit = Number(options['seed-limit'] ?? 2), rounds = Number(options.rounds ?? 3), workers = Number(options.workers ?? 2);
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 4 || !Number.isSafeInteger(rounds) || rounds < 1 || rounds > 3 || !Number.isSafeInteger(workers) || workers < 1 || workers > 4 || process.platform === 'win32')
    fail('lookahead-input', 'Expected POSIX, seed limit/workers 1..4 and rounds 1..3');
  const report = read(options['seed-report']), inputKeys = ['graph', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator'];
  if (report.chain_schema_version !== 2 || report.move_schema_version !== 4 || !Array.isArray(report.stages) || !Array.isArray(report.provenance)) fail('lookahead-seed-schema', 'Expected a schema-4 local run with recorded stages and provenance');
  for (const k of inputKeys) if (report.provenance.find(p => p.kind === k)?.sha256 !== hash(options[k])) fail('lookahead-input-mismatch', 'Seed report inputs differ from the requested graph/catalogs/validator', k);
  for (const required of ['model.mjs', 'movement.mjs', 'block-rotation.mjs', 'facility-rotation.mjs', 'components.mjs', 'run.mjs'])
    if (!report.source_hashes?.[required]) fail('lookahead-source-mismatch', 'Seed report is missing required engine provenance', required);
  const sources = {};
  for (const [name, expected] of Object.entries(report.source_hashes)) {
    const p = path.resolve(sourceDirectory, name);
    if (!p.startsWith(benchmarkRoot + path.sep) || hash(p) !== expected) fail('lookahead-source-mismatch', 'Seed report does not match the current benchmark engine', name);
    sources[name] = expected;
  }
  for (const name of ['lookahead.mjs', 'lookahead-process.mjs', 'lookahead-run.mjs']) sources[name] = hash(path.join(sourceDirectory, name));
  const provenance = keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) }));
  output = path.resolve(options['output-directory']); fs.mkdirSync(output); created = true; const began = performance.now();
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const graph = normalizeGraph(read(options.graph), catalogs);
  const validate = async (detail, name) => {
    const directory = path.join(output, name); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), detail.witness);
    const processResult = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    write(path.join(directory, 'validator-process.json'), processResult);
    if (processResult.timed_out || !fs.existsSync(path.join(directory, 'validation.json'))) return { directory, factory_validated: false, diagnostics: [diagnostic('lookahead-validation-process', 'Independent validator did not complete', name)] };
    const validation = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(graph, detail, validation, true);
    const factory_validated = processResult.code === 0 && coverage.factory_validated;
    const checked = { ...detail, accepted: factory_validated, factory_validated }; write(path.join(directory, 'coverage.json'), coverage); write(path.join(directory, 'detail.json'), checked);
    fs.writeFileSync(path.join(directory, 'index.html'), render(checked, validation, catalogs), { flag: 'wx' });
    return { directory, factory_validated, score: witnessScore(detail.witness), witness_sha256: hash(path.join(directory, 'witness.json')), witness: detail.witness, detail: checked, diagnostics: [...validation.diagnostics, ...coverage.diagnostics] };
  };
  const baseline = read(options.baseline), root = await validate(read(path.join(baseline.selected_directory, 'detail.json')), 'global-root');
  if (!root.factory_validated) fail('lookahead-root-invalid', 'Global root failed fresh original-graph validation', root.directory);
  const entries = [], seedDiagnostics = [];
  for (const [stage, s] of report.stages.entries()) if (s.status === 'unimproved-validated' && s.factory_validated === true && s.block_transform) {
    try { const detail = read(path.join(s.directory, 'detail.json')); entries.push({ stage, source_directory: s.directory, witness: detail.witness, detail }); }
    catch (e) { seedDiagnostics.push(diagnostic('lookahead-seed-artifact', e.message, stage)); }
  }
  const selection = selectLookaheadSeeds(entries, limit);
  const branches = await mapLookaheadBranches(selection.seeds, workers, async (seed, index) => {
    const record = { index, source_stage: seed.stage, source_directory: seed.source_directory, seed_signature: seed.signature, proposed_seed_score: seed.score };
    const validatedCandidates = [];
    try {
      const checked = await validate(seed.detail, `seed-${index}`); record.seed = publicRecord(checked);
      if (!checked.factory_validated) return { ...record, status: 'seed-validation-rejected', candidates: [] };
      validatedCandidates.push(checked);
      const seedPath = path.join(output, `seed-${index}.json`), branchDirectory = path.join(output, `branch-${index}`);
      write(seedPath, { schema_version: 1, status: 'private-lookahead-seed', factory_validated: true, selected_directory: checked.directory, global_root: publicRecord(root) });
      record.process = await executeLookaheadProcess(process.execPath, [path.join(sourceDirectory, 'run.mjs'),
        ...inputKeys.flatMap(k => [`--${k}`, options[k]]), '--baseline', seedPath, '--output-directory', branchDirectory,
        '--rounds', String(rounds), '--mode', 'joint', '--move-limit', '512', '--move-family', 'push', '--push-seeds', 'boundary',
        '--endpoint-policy', report.endpoint_policy, '--junction-policy', report.junction_policy, '--max-new-crossings', String(report.max_new_crossings),
        '--port-policy', report.port_policy, '--layer-policy', report.layer_policy, '--bridge-policy', 'release', '--audit-unimproved', '4'], path.join(output, `branch-${index}.log`));
      record.branch_report = path.join(branchDirectory, 'report.json');
      if (record.process.code !== 0 || record.process.timed_out || record.process.spawn_error || !fs.existsSync(record.branch_report))
        return { ...record, status: 'branch-process-failed', diagnostics: [diagnostic('lookahead-branch-process', 'Private branch failed or exhausted its wall-time budget', index)], candidates: [checked] };
      const child = read(record.branch_report);
      if (child.factory_validated !== true || typeof child.selected_directory !== 'string') fail('lookahead-branch-report', 'Private runner did not report a complete factory', index);
      record.branch_local = { initial_score: child.initial_score, score: child.score, termination: child.termination, attempts: child.stages.length, elapsed_ms: child.elapsed_ms };
      const final = await validate(read(path.join(child.selected_directory, 'detail.json')), `branch-final-${index}`); record.final = publicRecord(final);
      return { ...record, status: final.factory_validated ? 'branch-validated' : 'branch-validation-rejected', candidates: [checked, ...(final.factory_validated ? [final] : [])] };
    } catch (e) { return { ...record, status: 'branch-failed', diagnostics: [diagnostic(e.code ?? 'lookahead-branch-failed', e.message, index)], candidates: validatedCandidates }; }
  });
  for (const p of provenance) if (hash(p.path) !== p.sha256) fail('lookahead-input-changed', 'Input changed during private exploration', p.path);
  for (const [name, expected] of Object.entries(sources)) if (hash(path.resolve(sourceDirectory, name)) !== expected) fail('lookahead-source-changed', 'Engine changed during private exploration', name);
  const candidates = branches.flatMap(b => b.candidates), chosen = selectGlobalFactory(root, candidates), selected = chosen.selected;
  const result = { schema_version: 1, status: selected === root ? 'complete-incumbent-retained' : 'complete-factory-improved', factory_validated: true,
    selected_directory: selected.directory, initial_score: root.score, score: chosen.score, witness_sha256: selected.witness_sha256,
    seed_limit: limit, rounds, workers, branch_timeout_ms: 180000, branch_grace_ms: 1000, root: publicRecord(root),
    seed_selection: { ...selection, seeds: selection.seeds.map(({ witness, detail, ...s }) => s) }, seed_diagnostics: seedDiagnostics,
    branches: branches.map(({ candidates, ...b }) => b), global_comparisons: chosen.comparisons,
    termination: branches.length ? 'bounded-portfolio-complete' : 'no-eligible-seeds', elapsed_ms: performance.now() - began, provenance, source_hashes: sources };
  write(path.join(output, 'report.json'), result); fs.copyFileSync(path.join(selected.directory, 'index.html'), path.join(output, 'index.html'));
  console.log(JSON.stringify({ status: result.status, score: result.score.tuple, seeds: selection.seeds.length, workers, elapsed_ms: result.elapsed_ms, output }));
} catch (e) {
  const d = diagnostic(e.code ?? 'lookahead-execution-failed', e.message, e.entity);
  if (created) write(path.join(output, 'failure.json'), d);
  console.error(JSON.stringify(d)); process.exitCode = 1;
}
