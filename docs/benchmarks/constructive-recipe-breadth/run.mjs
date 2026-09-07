import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { graphFromWiring, externalSeed, regionsForConsumer, joinSupplierRegions, frontierDepth, inspectFutureEscape } from './model.mjs';
import { extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { constructCycle } from '../constructive-cycle-blocks/model.mjs';
import { composeBlocks, readyConsumers } from '../constructive-block-composition/model.mjs';
import { inspectCoverage, finishBoundary, growCandidates, better } from '../constructive-factory-closure/model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { mapLookaheadBranches } from '../constructive-local-reembedding/lookahead.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const diagnostic = e => ({ code: e.code ?? 'breadth-generation-failed', severity: 'error', stage: e.stage ?? 'recipe-breadth', message: e.message, entity: e.entity ?? null });
const publicRecord = ({ detail, ...r }) => r;
const sourceDirectory = fileURLToPath(new URL('.', import.meta.url));
const engineHashes = () => {
  const visited = new Map(), visit = file => {
    if (visited.has(file)) return; visited.set(file, hash(file));
    const text = fs.readFileSync(file, 'utf8');
    for (const m of text.matchAll(/\b(?:import|export)\s+[^;]*?\bfrom\s+['"](\.{1,2}\/[^'"]+\.mjs)['"]/g)) visit(path.resolve(path.dirname(file), m[1]));
  };
  visit(fileURLToPath(import.meta.url)); return Object.fromEntries([...visited].map(([file, h]) => [path.relative(sourceDirectory, file), h]));
};
let output;
try {
  const keys = ['wiring', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'max-steps'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const maxSteps = Number(options['max-steps'] ?? 256);
  if (!Number.isSafeInteger(maxSteps) || maxSteps < 1 || maxSteps > 256) throw new Error('Expected --max-steps 1..256');
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const began = performance.now(), graph = graphFromWiring(read(options.wiring), catalogs), inventory = extractCycles(graph, catalogs), stages = [], failures = [];
  const provenance = keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) }));
  const sourceHashes = engineHashes();
  output = path.resolve(options['output-directory']); fs.mkdirSync(output); write(path.join(output, 'graph.json'), graph); write(path.join(output, 'inventory.json'), inventory);
  const validate = async (detail, name, complete = false) => {
    const directory = path.join(output, name); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), detail.witness);
    const processResult = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    write(path.join(directory, 'validator-process.json'), processResult);
    if (processResult.timed_out || !fs.existsSync(path.join(directory, 'validation.json'))) throw new Error(`Validator did not finish for ${name}`);
    const validation = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(graph, detail, validation, complete, { pendingExternalInputs: !complete });
    const accepted = processResult.code === 0 && coverage.accepted, checked = { ...detail, accepted, factory_validated: accepted && complete };
    write(path.join(directory, 'detail.json'), checked); write(path.join(directory, 'coverage.json'), coverage);
    fs.writeFileSync(path.join(directory, 'index.html'), render(checked, validation, catalogs), { flag: 'wx' });
    return { directory, detail: checked, accepted, factory_validated: checked.factory_validated, score: witnessScore(detail.witness),
      witness_sha256: hash(path.join(directory, 'witness.json')), validator_ms: processResult.elapsed_ms, diagnostics: [...validation.diagnostics, ...coverage.diagnostics] };
  };
  const variants = detail => {
    try { return [detail, compactInterface(detail, catalogs)]; }
    catch (e) { failures.push({ ...diagnostic(e), severity: 'warning', stage: 'compaction' }); return [detail]; }
  };
  const check = async (candidates, prefix, complete = false) => {
    const checked = await mapLookaheadBranches(candidates, 4, async (d, i) => {
      try {
        if (!complete) { const escape = inspectFutureEscape(catalogs, graph, d); if (!escape.accepted) return { accepted: false, diagnostics: escape.diagnostics, score: witnessScore(d.witness) }; }
        return await validate(d, `${prefix}-${i}`, complete);
      } catch (e) { return { accepted: false, diagnostics: [diagnostic(e)] }; }
    });
    const valid = checked.filter(r => r.accepted).sort((a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : 0);
    return { checked, selected: valid[0] };
  };
  const initialJobs = inventory.blocks.map(block => ({ id: block.id, generate: () => ['leaf', 'root'].flatMap(f => {
    try { return variants(constructCycle(catalogs, graph, block, f)); } catch (e) { failures.push(diagnostic(e)); return []; }
  }) }));
  const covered = new Set(inventory.blocks.flatMap(b => b.members));
  for (const n of graph.nodes.filter(n => !covered.has(n.id) && graph.edges.some(e => e.target === n.id) && graph.edges.filter(e => e.target === n.id).every(e => !e.internal)))
    initialJobs.push({ id: `external-${n.label}`, generate: () => [externalSeed(catalogs, graph, n)] });
  let regions = [];
  // Each initial region is a connected atomic SCC or one explicit external-demand seed.
  for (const job of initialJobs) {
    try {
      const start = performance.now(), candidates = job.generate(), generationMs = performance.now() - start;
      const { checked, selected } = await check(candidates, `seed-${job.id}`);
      stages.push({ stage: 'seed', region: job.id, generation_ms: generationMs, candidates: checked.map(publicRecord), selected_directory: selected?.directory ?? null });
      if (selected) regions.push({ id: job.id, detail: selected.detail, directory: selected.directory });
    } catch (e) { failures.push(diagnostic(e)); }
  }
  failures.push(...inventory.failures);
  const depths = new Map(regions.flatMap(r => r.detail.witness.placements.map(p => [p.instance, 0])));
  let growth = 0;
  for (; growth < maxSteps; growth++) {
    const members = regions.flatMap(r => r.detail.witness.placements.map(p => p.instance));
    if (members.length === graph.nodes.length) break;
    const targets = readyConsumers(graph, members, catalogs).sort((a, b) => frontierDepth(graph, a, depths) - frontierDepth(graph, b, depths) || a.localeCompare(b)); let committed = false;
    for (const target of targets) {
      try {
        const needed = regionsForConsumer(graph, regions, target), start = performance.now();
        let generated, local;
        try { generated = composeBlocks(catalogs, graph, needed, [target], { orderBudget: 128, lanePolicy: 'portfolio' }); }
        catch (e) { generated = { candidates: [], failures: [diagnostic(e)] }; }
        try { local = growCandidates(catalogs, graph, joinSupplierRegions(catalogs, graph, needed), graph.nodes.find(n => n.id === target), 128, 'search', 'two-sided'); }
        catch (e) { local = { candidates: [], failures: [diagnostic(e)], trials: 0 }; }
        const candidates = [...generated.candidates, ...local.candidates].flatMap(variants), generationMs = performance.now() - start;
        const { checked, selected } = await check(candidates, `growth-${growth}-${graph.nodes.find(n => n.id === target).label}`);
        stages.push({ stage: 'growth', transaction: growth, consumer: target, source_regions: needed.map(r => r.id), generation_ms: generationMs,
          candidates: checked.map(publicRecord), selected_directory: selected?.directory ?? null,
          proposal_failures: generated.failures, local_trials: local.trials, local_route_search: local.route_search, local_failures: local.failures });
        if (!selected) continue;
        depths.set(target, frontierDepth(graph, target, depths));
        regions = [...regions.filter(r => !needed.includes(r)), { id: `growth-${growth}`, detail: selected.detail, directory: selected.directory }];
        committed = true; break;
      } catch (e) { failures.push(diagnostic(e)); }
    }
    if (!committed) { failures.push({ code: 'breadth-no-valid-frontier', severity: 'error', stage: 'region-growth', message: 'No ready consumer could extend its retained supplier regions', entity: targets }); break; }
  }
  let final;
  if (regions.length === 1 && regions[0].detail.witness.placements.length === graph.nodes.length) {
    try {
      const start = performance.now(), raw = finishBoundary(catalogs, graph, regions[0].detail, 'search'), candidates = variants(raw), generationMs = performance.now() - start;
      const { checked, selected } = await check(candidates, 'complete', true); final = selected;
      stages.push({ stage: 'boundary', generation_ms: generationMs, candidates: checked.map(publicRecord), selected_directory: selected?.directory ?? null });
    } catch (e) { failures.push(diagnostic(e)); }
  }
  const report = { schema_version: 1, status: final?.factory_validated ? 'complete-factory-validated' : 'region-construction-stopped', factory_validated: final?.factory_validated ?? false,
    total_facilities: graph.nodes.length, covered_facilities: regions.reduce((n, r) => n + r.detail.witness.placements.length, 0), required_edges: graph.edges.length,
    max_steps: maxSteps, growth_transactions: stages.filter(s => s.stage === 'growth' && s.selected_directory).length,
    selected_directory: final?.directory ?? null, score: final?.score ?? null, witness_sha256: final?.witness_sha256 ?? null,
    retained_regions: regions.map(r => ({ id: r.id, directory: r.directory, facilities: r.detail.witness.placements.length, score: witnessScore(r.detail.witness) })),
    stages, failures, elapsed_ms: performance.now() - began, provenance,
    source_hashes: sourceHashes };
  for (const input of provenance) if (hash(input.path) !== input.sha256) throw new Error(`Input changed during construction: ${input.path}`);
  for (const [file, expected] of Object.entries(sourceHashes)) if (hash(path.resolve(sourceDirectory, file)) !== expected) throw new Error(`Engine changed during construction: ${file}`);
  write(path.join(output, 'report.json'), report); if (final) fs.copyFileSync(path.join(final.directory, 'index.html'), path.join(output, 'index.html'));
  console.log(JSON.stringify({ status: report.status, total: report.total_facilities, covered: report.covered_facilities, score: report.score?.tuple, elapsed_ms: report.elapsed_ms, output }));
  if (!report.factory_validated) process.exitCode = 2;
} catch (e) { const d = diagnostic(e); if (output) write(path.join(output, 'failure.json'), d); console.error(JSON.stringify(d)); process.exitCode = 1; }
