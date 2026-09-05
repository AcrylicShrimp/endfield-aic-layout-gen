import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { inspectBlockCoverage } from '../constructive-cycle-blocks/model.mjs';
import { composeBlocks, readyConsumers } from './model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
let failureDirectory;
try {
  const keys = ['graph', 'blocks', 'frontier', 'steps', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'];
  const args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'order-budget', 'lane-policy'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const steps = Number(options.steps);
  if (!Number.isSafeInteger(steps) || steps < 1 || steps > 32) throw new Error('Expected --steps 1..32');
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output); failureDirectory = output;
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const graph = normalizeGraph(read(options.graph), catalogs), blockReport = read(options.blocks);
  let regions = blockReport.selected.map(r => ({ id: r.block, detail: read(path.join(r.directory, 'detail.json')) }));
  let targets;
  if (options.frontier === 'ready') targets = readyConsumers(graph, regions.flatMap(r => r.detail.witness.placements.map(p => p.instance)), catalogs);
  else {
    targets = options.frontier.split(',').map(value => { const found = graph.nodes.filter(n => n.id === value || n.label === value);
      if (found.length !== 1) throw new Error(`Unknown or ambiguous frontier ${value}`); return found[0].id; });
    const needed = new Set(graph.edges.filter(e => targets.includes(e.target)).map(e => e.source));
    regions = regions.filter(r => r.detail.witness.placements.some(p => needed.has(p.instance)));
  }
  const results = [], failures = [], start = performance.now();
  const validate = async (detail, stage) => {
    const directory = path.join(output, `stage-${stage}-${detail.embedding}`); fs.mkdirSync(directory);
    write(path.join(directory, 'detail.json'), detail); write(path.join(directory, 'witness.json'), detail.witness);
    const code = await new Promise((resolve, reject) => {
      const log = fs.createWriteStream(path.join(directory, 'validator.log'), { flags: 'wx' });
      const child = spawn(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
        ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')]);
      const guard = setTimeout(() => child.kill(), 30000); child.stdout.pipe(log, { end: false }); child.stderr.pipe(log, { end: false });
      child.on('error', e => { clearTimeout(guard); log.end(); reject(e); });
      child.on('close', code => { clearTimeout(guard); log.end(); resolve(code); });
    });
    const validation = read(path.join(directory, 'validation.json')), coverage = inspectBlockCoverage(detail, validation, catalogs);
    write(path.join(directory, 'coverage.json'), coverage);
    fs.writeFileSync(path.join(directory, 'index.html'), render(detail, validation, catalogs), { flag: 'wx' });
    return { detail, directory, accepted: code === 0 && coverage.accepted, score: witnessScore(detail.witness), timings: detail.timings,
      validator_ms: validation.elapsed_ms, pending: validation.recipe_terminals.filter(t => t.pending_rate.numerator > 0),
      diagnostics: [...validation.diagnostics, ...coverage.diagnostics], witness_sha256: hash(path.join(directory, 'witness.json')) };
  };
  for (let stage = 0; stage < steps; stage++) {
    if (!targets.length) { failures.push({ code: 'composition-no-ready-belt-frontier', severity: 'info', stage: 'frontier', message: 'No further closed-input belt-only frontier in the retained graph' }); break; }
    let generated;
    try { generated = composeBlocks(catalogs, graph, regions, targets, { orderBudget: Number(options['order-budget'] ?? 128), lanePolicy: options['lane-policy'] ?? 'portfolio' }); }
    catch (e) { failures.push({ code: e.code ?? 'composition-generation-failed', severity: 'error', stage: e.stage ?? 'generation', message: e.message, entity: e.entity }); break; }
    const candidates = generated.candidates ? [...generated.candidates] : [generated];
    failures.push(...(generated.failures ?? []));
    for (const raw of [...candidates]) {
      try { candidates.push(compactInterface(raw, catalogs)); }
      catch (e) { failures.push({ code: 'composition-compaction-failed', severity: 'warning', stage: 'compaction', message: e.message }); }
    }
    const checked = await Promise.all(candidates.map(async detail => {
      try { return await validate(detail, stage); }
      catch (e) { return { detail, accepted: false, diagnostics: [{ code: 'composition-candidate-processing-failed', severity: 'error', stage: 'validation', message: e.message }] }; }
    }));
    const valid = checked.filter(r => r.accepted).sort((a, b) => { for (let i = 0; i < 3; i++) if (a.score.tuple[i] !== b.score.tuple[i]) return a.score.tuple[i] - b.score.tuple[i]; return 0; });
    const selected = valid[0];
    results.push({ stage, construction_ms: generated.construction_ms, compaction_ms: candidates.reduce((s, d) => s + (d.compaction?.elapsed_ms ?? 0), 0),
      frontier: targets.map(id => graph.nodes.find(n => n.id === id).label),
      composition_plan: selected?.detail.composition_plan ?? selected?.detail.pre_compaction_plan?.composition_plan ?? candidates[0]?.composition_plan,
      selected_directory: selected?.directory ?? null,
      candidates: checked.map(({ detail, ...r }) => ({ ...r, embedding: detail.embedding, facilities: detail.witness.placements.length })) });
    if (!selected) break;
    fs.copyFileSync(path.join(selected.directory, 'index.html'), path.join(output, `stage-${stage}.html`), fs.constants.COPYFILE_EXCL);
    regions = [{ id: `retained-${stage}`, detail: selected.detail }];
    targets = readyConsumers(graph, selected.detail.witness.placements.map(p => p.instance), catalogs);
  }
  const last = [...results].reverse().find(r => r.selected_directory), selected = last?.candidates.find(c => c.directory === last.selected_directory);
  const report = { schema_version: 1, status: results.length === steps && results.every(r => r.selected_directory) ? 'composition-frontiers-validated' : 'composition-frontier-stopped',
    factory_validated: false, requested_steps: steps, completed_steps: results.filter(r => r.selected_directory).length,
    covered_facilities: selected?.facilities ?? 0, total_facilities: graph.nodes.length, selected_directory: last?.selected_directory ?? null,
    elapsed_ms: performance.now() - start, stages: results, failures,
    provenance: ['graph', 'blocks', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator'].map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['model.mjs', 'run.mjs', '../constructive-auto-interface/embedding.mjs', '../constructive-auto-interface/compact.mjs',
      '../constructive-auto-interface/allocation.mjs', '../constructive-cycle-blocks/model.mjs', '../constructive-cycle-blocks/graph.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), report);
  console.log(JSON.stringify({ status: report.status, stages: report.completed_steps, facilities: report.covered_facilities, output }));
  if (report.status !== 'composition-frontiers-validated') process.exitCode = 2;
} catch (e) {
  const failure = { code: e.code ?? 'composition-execution-failed', severity: 'error', stage: e.stage ?? 'execution', message: e.message, entity: e.entity ?? null };
  if (failureDirectory) write(path.join(failureDirectory, 'failure.json'), failure);
  console.error(JSON.stringify({ code: failure.code, message: failure.message })); process.exitCode = 1;
}
