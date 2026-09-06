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
import { packShapes, diverse, fillMetrics, growthCut } from '../constructive-shape-packing/model.mjs';
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
    if (!args[i].startsWith('--') || ![...keys, 'packing-family', 'nesting-limit', 'nesting-screen', 'refine-from', 'refine-radius', 'placement-family', 'source-shapes', 'shape-limit', 'shape-beam', 'growth-fill'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const packingFamily = options['packing-family'] ?? 'shelf';
  if (!['shelf', 'nesting'].includes(packingFamily)) throw new Error('Expected --packing-family shelf or nesting');
  const placementFamily = options['placement-family'] ?? 'forward';
  if (!['forward', 'two-sided'].includes(placementFamily)) throw new Error('Expected --placement-family forward or two-sided');
  const shapeLimit = Number(options['shape-limit'] ?? 12), shapeBeam = Number(options['shape-beam'] ?? 8);
  if (![shapeLimit, shapeBeam].every(n => Number.isSafeInteger(n) && n >= 1 && n <= 32) ||
      ((options['shape-limit'] || options['shape-beam']) && !options['source-shapes'])) throw new Error('Shape limits require --source-shapes and values 1..32');
  const growthFill = Number(options['growth-fill'] ?? 0);
  if (!Number.isFinite(growthFill) || growthFill < 0 || growthFill > 1) throw new Error('Expected --growth-fill in 0..1');
  const nestingLimit = Number(options['nesting-limit'] ?? (options['refine-from'] ? 32 : 12)), nestingScreen = options['nesting-screen'] ?? 'space';
  if (!Number.isSafeInteger(nestingLimit) || nestingLimit < 1 || nestingLimit > 32) throw new Error('Expected --nesting-limit 1..32');
  if (!['ports', 'space'].includes(nestingScreen)) throw new Error('Expected --nesting-screen ports or space');
  const refineRadius = Number(options['refine-radius'] ?? 2);
  if (![1, 2].includes(refineRadius) || (options['refine-radius'] && !options['refine-from']) || (options['refine-from'] && packingFamily !== 'nesting')) throw new Error('Refinement requires nesting, --refine-from and radius 1 or 2');
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
  const banks = [], shapeChecks = [];
  if (options['source-shapes']) {
    const shapeReport = read(options['source-shapes']);
    for (const s of blocks.selected) {
      const retained = read(path.join(s.directory, 'detail.json')), seen = new Set(), alternatives = [];
      for (const c of [s, ...shapeReport.candidates.filter(c => c.block === s.block && c.accepted)]) {
        if (seen.has(c.witness_sha256)) continue; seen.add(c.witness_sha256);
        const detail = c.directory === s.directory ? retained : read(path.join(c.directory, 'detail.json'));
        // Hold logical allocation fixed while varying physical shape in this control.
        if (JSON.stringify(detail.reallocation ?? null) !== JSON.stringify(retained.reallocation ?? null)) continue;
        const score = witnessScore(detail.witness);
        alternatives.push({ directory: c.directory, detail, ...score, tiles: score.physical_tiles, signature: c.directory });
      }
      const portfolio = diverse(alternatives, shapeLimit), variants = [];
      for (let i = 0; i < portfolio.length; i++) {
        const p = portfolio[i], checked = await validate(p.detail, path.join(output, `shape-${s.block}-${i}`));
        shapeChecks.push({ region: s.block, source_directory: p.directory, ...fillMetrics(p.detail.witness), ...((({ detail, ...r }) => r)(checked)) });
        if (checked.accepted) variants.push({ directory: p.directory, detail: p.detail });
      }
      if (!variants.length) throw new Error(`No currently valid source shape for ${s.block}`);
      banks.push({ id: s.block, variants });
    }
  }
  const inventory = discoverClusters(graph, sources, catalogs), clusters = [], clusterReports = [];
  for (const c of inventory) {
    let regions = c.regions, targets = c.frontier; const stages = [], growthDecisions = []; let fillStopped = false;
    while (targets.length && stages.length < 32) {
      const generated = composeBlocks(catalogs, graph, regions, targets, { lanePolicy: 'portfolio' });
      let shapeSearch = null;
      if (banks.length && stages.length === 0 && regions.length > 1) {
        const packed = packShapes(graph, banks.filter(b => regions.some(r => r.id === b.id)), catalogs, { beamWidth: shapeBeam, limit: shapeBeam });
        shapeSearch = packed.search;
        for (const p of packed.candidates) {
          try {
            const g = composeBlocks(catalogs, graph, p.regions, targets, { lanePolicy: 'portfolio', regionOffsets: p.offsets });
            generated.candidates.push(...g.candidates.map(d => ({ ...d, source_shape_plan: p.plan })));
            generated.failures.push(...g.failures); generated.construction_ms += g.construction_ms;
          } catch (e) {
            if (!e.code) throw e;
            generated.failures.push({ code: e.code, stage: e.stage, message: e.message, entity: e.entity, source_shape_plan: p.plan });
          }
        }
      }
      const candidates = [...generated.candidates];
      candidates.push(...generated.candidates.map(d => compactInterface(d, catalogs)));
      const checked = await Promise.all(candidates.map((d, i) => validate(d, path.join(output, `${c.id}-stage-${stages.length}-${i}`))));
      const valid = checked.filter(r => r.accepted).sort((a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : 0);
      if (!valid.length) throw new Error(`No valid structured interface for ${c.id}`);
      const selected = valid[0]; publish(selected);
      const stage = { frontier: targets, construction_ms: generated.construction_ms, shape_search: shapeSearch, failures: generated.failures,
        candidates: checked.map(({ detail, ...r }) => ({ ...r, fill: fillMetrics(detail.witness), source_shape_plan: detail.source_shape_plan ?? null })), selected_directory: selected.directory };
      const cut = growthCut(stages.length ? regions[0].detail.witness : null, selected.detail.witness, growthFill);
      if (cut) {
        growthDecisions.push({ ...cut, rejected_growth: stage });
        fillStopped = true; break;
      }
      stages.push(stage); regions = [{ id: c.id, detail: selected.detail }];
      targets = readyConsumers(graph, selected.detail.witness.placements.map(p => p.instance), catalogs);
    }
    if ((!fillStopped && targets.length) || regions.length !== 1) throw new Error(`Cluster growth did not close for ${c.id}`);
    clusters.push(regions[0]); clusterReports.push({ id: c.id, sources: c.regions.map(r => r.id), members: regions[0].detail.witness.placements.map(p => p.instance),
      stages, growth_decisions: growthDecisions, deferred_frontier: fillStopped ? targets : [] });
  }
  let neighborhood = null;
  if (options['refine-from']) {
    const previous = read(options['refine-from']), winner = previous.packings?.find(p => p.complete && p.complete_directory === previous.selected_directory);
    if (!previous.factory_validated || !winner?.packing_plan?.offset) throw new Error('Refinement report must select a complete nested packing');
    const clusterHashes = cs => cs.map(c => [c.id, c.stages.at(-1).candidates.find(p => p.directory === c.stages.at(-1).selected_directory)?.witness_sha256]);
    if (JSON.stringify(clusterHashes(previous.clusters)) !== JSON.stringify(clusterHashes(clusterReports))) throw new Error('Refinement requires identical retained cluster witness hashes');
    // Recheck the complete reference independently rather than trusting its report flags.
    const reference = await validate(read(path.join(previous.selected_directory, 'detail.json')), path.join(output, 'refinement-reference'), true);
    if (!reference.factory_validated) throw new Error('Refinement reference failed complete independent validation');
    neighborhood = { dx: winner.packing_plan.offset.dx, dy: winner.packing_plan.offset.dy, radius: refineRadius };
  }
  const packStart = performance.now(), packed = packingFamily === 'shelf' ? packingCandidates(graph, clusters, catalogs) : nestingCandidates(graph, clusters, catalogs, nestingLimit, nestingScreen, neighborhood), packMs = performance.now() - packStart;
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
          '--graph', options.graph, '--baseline', path.join(p.directory, 'baseline.json'), '--route-family', family, '--placement-family', placementFamily, ...catalogArgs,
          '--validator', options.validator, '--output-directory', directory], path.join(p.directory, `closure-${family}.log`), 120000);
        const attempt = { family, placement_family: placementFamily, elapsed_ms: performance.now() - closureStart, exit_code: code, complete: false }; p.closures.push(attempt);
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
    selected_directory: selectedDirectory, score, incumbent_score: incumbent.score, packing_family: packingFamily, placement_family: placementFamily, elapsed_ms: performance.now() - start, packing_construction_ms: packMs,
    clusters: clusterReports, growth_fill: growthFill, source_shapes: { enabled: Boolean(options['source-shapes']), shape_limit: shapeLimit, beam_width: shapeBeam, checks: shapeChecks }, packings: packReports.map(({ detail, ...r }) => r),
    provenance: [...keys.filter(k => k !== 'output-directory'), ...['refine-from', 'source-shapes'].filter(k => options[k])].map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['run.mjs', 'model.mjs', 'nesting.mjs', 'pockets.mjs', '../constructive-block-composition/model.mjs', '../constructive-factory-closure/model.mjs', '../constructive-factory-closure/output-space.mjs', '../constructive-factory-closure/run.mjs',
      '../constructive-interconnect/geometry.mjs', '../constructive-auto-interface/embedding.mjs', '../constructive-auto-interface/model.mjs', '../constructive-shape-packing/model.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), report); fs.copyFileSync(path.join(selectedDirectory, 'index.html'), path.join(output, 'index.html'), fs.constants.COPYFILE_EXCL);
  console.log(JSON.stringify({ status: report.status, clusters: clusters.length, packings: packed.length, score: score.tuple, output }));
} catch (e) {
  const failure = { code: e.code ?? 'cluster-packing-execution-failed', severity: 'error', stage: e.stage ?? 'execution', message: e.message, entity: e.entity };
  if (output && fs.existsSync(output)) write(path.join(output, 'failure.json'), failure);
  console.error(JSON.stringify({ code: failure.code, message: failure.message })); process.exitCode = 1;
}
