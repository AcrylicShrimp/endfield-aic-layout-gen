import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { composeBlocks, readyConsumers } from '../constructive-block-composition/model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { inspectCoverage, better } from '../constructive-factory-closure/model.mjs';
import { discoverClusters, packingCandidates } from './model.mjs';
import { nestingCandidates } from './nesting.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
async function execute(command, args, logPath, timeout) {
  return new Promise((resolve, reject) => {
    const log = fs.createWriteStream(logPath, { flags: 'wx' }), p = spawn(command, args), guard = setTimeout(() => p.kill(), timeout);
    p.stdout.pipe(log, { end: false }); p.stderr.pipe(log, { end: false });
    p.on('error', e => { clearTimeout(guard); log.end(); reject(e); }); p.on('close', code => { clearTimeout(guard); log.end(); resolve(code); });
  });
}
let output;
try {
  const keys = ['graph', 'blocks', 'incumbent', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'packing-family', 'nesting-limit'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const packingFamily = options['packing-family'] ?? 'shelf';
  if (!['shelf', 'nesting'].includes(packingFamily)) throw new Error('Expected --packing-family shelf or nesting');
  output = path.resolve(options['output-directory']); fs.mkdirSync(output); const start = performance.now();
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const graph = normalizeGraph(read(options.graph), catalogs), baseline = read(options.incumbent), blocks = read(options.blocks);
  const catalogArgs = ['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]);
  const validate = async (detail, directory, complete = false) => {
    fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), detail.witness);
    const code = await execute(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'), ...catalogArgs, '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'), 30000);
    const validation = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(graph, detail, validation, complete);
    write(path.join(directory, 'coverage.json'), coverage);
    return { detail, directory, accepted: code === 0 && coverage.accepted, factory_validated: code === 0 && coverage.factory_validated,
      score: witnessScore(detail.witness), witness_sha256: hash(path.join(directory, 'witness.json')), validator_ms: validation.elapsed_ms,
      diagnostics: [...validation.diagnostics, ...coverage.diagnostics] };
  };
  const publish = r => {
    write(path.join(r.directory, 'detail.json'), { ...r.detail, accepted: r.accepted, factory_validated: r.factory_validated });
    fs.writeFileSync(path.join(r.directory, 'index.html'), render({ ...r.detail, accepted: r.accepted, factory_validated: r.factory_validated }, read(path.join(r.directory, 'validation.json')), catalogs), { flag: 'wx' });
  };
  const incumbent = await validate(read(path.join(baseline.selected_directory, 'detail.json')), path.join(output, 'incumbent'), true);
  if (!incumbent.factory_validated) throw new Error('Complete incumbent failed independent current-catalog validation'); publish(incumbent);
  const sources = blocks.selected.map(b => ({ id: b.block, detail: read(path.join(b.directory, 'detail.json')) }));
  const inventory = discoverClusters(graph, sources, catalogs), clusters = [], clusterReports = [];
  for (const c of inventory) {
    let regions = c.regions, targets = c.frontier; const stages = [];
    while (targets.length && stages.length < 32) {
      const generated = composeBlocks(catalogs, graph, regions, targets, { lanePolicy: 'portfolio' }), candidates = [...generated.candidates];
      candidates.push(...generated.candidates.map(d => compactInterface(d, catalogs)));
      const checked = await Promise.all(candidates.map((d, i) => validate(d, path.join(output, `${c.id}-stage-${stages.length}-${i}`))));
      const valid = checked.filter(r => r.accepted).sort((a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : 0);
      stages.push({ frontier: targets, construction_ms: generated.construction_ms, failures: generated.failures, candidates: checked.map(({ detail, ...r }) => r), selected_directory: valid[0]?.directory });
      if (!valid.length) throw new Error(`No valid structured interface for ${c.id}`);
      const selected = valid[0]; publish(selected); regions = [{ id: c.id, detail: selected.detail }];
      targets = readyConsumers(graph, selected.detail.witness.placements.map(p => p.instance), catalogs);
    }
    if (targets.length || regions.length !== 1) throw new Error(`Cluster growth did not close for ${c.id}`);
    clusters.push(regions[0]); clusterReports.push({ id: c.id, sources: c.regions.map(r => r.id), members: regions[0].detail.witness.placements.map(p => p.instance), stages });
  }
  const packStart = performance.now(), packed = packingFamily === 'shelf' ? packingCandidates(graph, clusters, catalogs) : nestingCandidates(graph, clusters, catalogs, Number(options['nesting-limit'] ?? 12)), packMs = performance.now() - packStart;
  const packReports = [];
  for (let i = 0; i < packed.length; i++) {
    const p = await validate(packed[i], path.join(output, `packing-${i}`));
    if (p.accepted) { publish(p); write(path.join(p.directory, 'baseline.json'), { schema_version: 1, factory_validated: false, selected_directory: p.directory }); }
    packReports.push({ ...p, packing_plan: packed[i].packing_plan });
  }
  let index = 0;
  await Promise.all(Array.from({ length: 2 }, async () => {
    while (index < packReports.length) {
      const p = packReports[index++]; if (!p.accepted) continue;
      p.closures = []; p.complete = false;
      for (const family of ['simple', 'perimeter']) {
        const closureStart = performance.now(), directory = path.join(p.directory, `closure-${family}`);
        const code = await execute(process.execPath, [new URL('../constructive-factory-closure/run.mjs', import.meta.url).pathname,
          '--graph', options.graph, '--baseline', path.join(p.directory, 'baseline.json'), '--route-family', family, ...catalogArgs,
          '--validator', options.validator, '--output-directory', directory], path.join(p.directory, `closure-${family}.log`), 120000);
        const attempt = { family, elapsed_ms: performance.now() - closureStart, exit_code: code, complete: false }; p.closures.push(attempt);
        if (fs.existsSync(path.join(directory, 'report.json'))) {
          const r = read(path.join(directory, 'report.json')); attempt.report = path.join(directory, 'report.json');
          attempt.covered_facilities = r.covered_facilities; attempt.partial_or_complete_score = r.score; attempt.failures = r.failures;
          if (code === 0 && r.factory_validated) {
            const check = await validate(read(path.join(r.selected_directory, 'detail.json')), path.join(p.directory, `complete-check-${family}`), true);
            attempt.complete = check.factory_validated; attempt.diagnostics = check.diagnostics;
            if (check.factory_validated && (!p.complete || better(check.score.tuple, p.complete_score.tuple))) {
              p.complete = true; p.complete_score = check.score; p.complete_hash = check.witness_sha256; p.complete_directory = r.selected_directory; p.selected_family = family;
            }
          }
        } else attempt.failures = fs.existsSync(path.join(directory, 'failure.json')) ? [read(path.join(directory, 'failure.json'))] : [{ code: 'cluster-closure-process-failed' }];
      }
    }
  }));
  const improved = packReports.filter(p => p.complete && better(p.complete_score.tuple, incumbent.score.tuple))
    .sort((a, b) => better(a.complete_score.tuple, b.complete_score.tuple) ? -1 : better(b.complete_score.tuple, a.complete_score.tuple) ? 1 : 0);
  const selectedDirectory = improved[0]?.complete_directory ?? incumbent.directory, score = improved[0]?.complete_score ?? incumbent.score;
  const report = { schema_version: 1, status: improved.length ? 'complete-factory-improved' : 'complete-incumbent-retained', factory_validated: true,
    selected_directory: selectedDirectory, score, incumbent_score: incumbent.score, packing_family: packingFamily, elapsed_ms: performance.now() - start, packing_construction_ms: packMs,
    clusters: clusterReports, packings: packReports.map(({ detail, ...r }) => r),
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['run.mjs', 'model.mjs', 'nesting.mjs', '../constructive-block-composition/model.mjs', '../constructive-factory-closure/model.mjs', '../constructive-factory-closure/run.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), report); fs.copyFileSync(path.join(selectedDirectory, 'index.html'), path.join(output, 'index.html'), fs.constants.COPYFILE_EXCL);
  console.log(JSON.stringify({ status: report.status, clusters: clusters.length, packings: packed.length, score: score.tuple, output }));
} catch (e) {
  const failure = { code: e.code ?? 'cluster-packing-execution-failed', severity: 'error', stage: e.stage ?? 'execution', message: e.message, entity: e.entity };
  if (output && fs.existsSync(output)) write(path.join(output, 'failure.json'), failure);
  console.error(JSON.stringify({ code: failure.code, message: failure.message })); process.exitCode = 1;
}
