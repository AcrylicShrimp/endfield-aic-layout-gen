import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { applyReallocation } from './graph-delta.mjs';
import { directAttachments } from './direct.mjs';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { composeBlocks } from '../constructive-block-composition/model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { inspectCoverage, better } from '../constructive-factory-closure/model.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
let output;
try {
  const keys = ['graph', 'cores', 'blocks', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'validation-policy'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const validationPolicy = options['validation-policy'] ?? 'ranked';
  if (!['ranked', 'exhaustive'].includes(validationPolicy)) throw new Error('Expected --validation-policy ranked|exhaustive');
  const directory = path.resolve(options['output-directory']); fs.mkdirSync(directory); output = directory;
  const began = performance.now(), catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const snapshot = read(options.graph), graph = normalizeGraph(snapshot, catalogs), inventory = extractCycles(graph, catalogs), coreReport = read(options.cores), retained = read(options.blocks);
  const candidates = [], failures = [], jobs = []; let ordinal = 0;
  const validate = async (detail, g, name) => {
    const directory = path.join(output, name); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), detail.witness);
    const proc = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    write(path.join(directory, 'process.json'), proc);
    if (proc.timed_out || !fs.existsSync(path.join(directory, 'validation.json'))) throw new Error(`Validator failed in ${directory}; inspect process.json and validator.log`);
    const v = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(g, detail, v, false);
    detail.accepted = proc.code === 0 && coverage.accepted; detail.factory_validated = false;
    write(path.join(directory, 'detail.json'), detail); write(path.join(directory, 'coverage.json'), coverage);
    return { block: detail.block.id, directory, accepted: detail.accepted, facilities: detail.witness.placements.length, score: witnessScore(detail.witness),
      reallocation: detail.reallocation ?? null, diagnostics: [...v.diagnostics, ...coverage.diagnostics], witness_sha256: hash(path.join(directory, 'witness.json')) };
  };
  const start = performance.now();
  for (const b of inventory.blocks) for (const c of coreReport.candidates.filter(c => c.block === `${b.id}-core` && c.accepted)) {
    const core = read(path.join(c.directory, 'detail.json')), g = applyReallocation(graph, core.reallocation), targets = b.consumers.filter(id => !b.cycle.includes(id));
    if (targets.length === 1) for (const detail of directAttachments(catalogs, g, core, targets[0])) {
      detail.block.id = b.id; jobs.push({ detail, graph: g, name: `candidate-${ordinal++}`, core_directory: c.directory });
    }
    try {
      const generated = composeBlocks(catalogs, g, [{ id: b.id, detail: core }], targets, { lanePolicy: 'portfolio' });
      for (const d of generated.candidates) for (const detail of [d, compactInterface(d, catalogs)]) {
        detail.block.id = b.id; detail.reallocation = core.reallocation ?? null;
        jobs.push({ detail, graph: g, name: `candidate-${ordinal++}`, core_directory: c.directory });
      }
      failures.push(...generated.failures.map(d => ({ block: b.id, core_directory: c.directory, ...d })));
    } catch (e) { if (!e.code) throw e; failures.push({ block: b.id, core_directory: c.directory, code: e.code, message: e.message, entity: e.entity }); }
  }
  const constructionMs = performance.now() - start; let next = 0;
  const compareCandidates = (a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : a.directory.localeCompare(b.directory);
  const queues = validationPolicy === 'exhaustive' ? jobs.map(j => [j]) : inventory.blocks.map(b => jobs.filter(j => j.detail.block.id === b.id)
    .map(j => ({ ...j, score: witnessScore(j.detail.witness), directory: path.join(output, j.name) })).sort(compareCandidates));
  await Promise.all(Array.from({ length: Math.min(4, queues.length) }, async () => {
    while (next < queues.length) for (const j of queues[next++]) {
      const c = { ...await validate(j.detail, j.graph, j.name), core_directory: j.core_directory }; candidates.push(c);
      if (validationPolicy === 'ranked' && c.accepted) break;
    }
  }));
  const selected = [];
  for (const b of inventory.blocks) {
    const best = candidates.filter(c => c.block === b.id && c.accepted).sort(compareCandidates)[0];
    const fallback = retained.selected.find(c => c.block === b.id);
    if (!best && !fallback) throw new Error(`No validated candidate or retained block for ${b.id}`);
    selected.push(best ?? { ...fallback, reallocation: null, retained: true });
  }
  let combined = graph;
  for (const c of selected) combined = applyReallocation(combined, c.reallocation);
  const combinedSnapshot = { ...snapshot, internal_requirements: combined.edges.filter(e => e.internal), external_requirements: combined.edges.filter(e => !e.internal) };
  const verifiedGraph = normalizeGraph(combinedSnapshot, catalogs), verified = [];
  for (const s of selected) {
    const d = read(path.join(s.directory, 'detail.json')); d.block.id = s.block;
    const v = await validate(d, verifiedGraph, `selected-${s.block}`); if (!v.accepted) throw new Error(`Selected block failed combined-graph validation: ${s.block}`);
    verified.push({ ...v, retained: s.retained ?? false });
    fs.writeFileSync(path.join(output, `${s.block}.html`), render(d, read(path.join(v.directory, 'validation.json')), catalogs), { flag: 'wx' });
  }
  write(path.join(output, 'graph.json'), combinedSnapshot);
  const report = { schema_version: 1, status: 'covered-cycle-blocks-validated', factory_validated: false, selected: verified,
    candidates: candidates.sort((a, b) => a.directory.localeCompare(b.directory)), validation_policy: validationPolicy, generated_candidates: jobs.length,
    skipped_candidates: jobs.length - candidates.length, failures, construction_ms: constructionMs, elapsed_ms: performance.now() - began,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['attach.mjs', 'graph-delta.mjs', 'direct.mjs', '../constructive-block-composition/model.mjs', '../constructive-auto-interface/compact.mjs',
      '../constructive-factory-closure/model.mjs', '../constructive-cycle-blocks/graph.mjs', '../constructive-embedding-compaction/validator.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'blocks.json'), report);
  console.log(JSON.stringify({ status: report.status, blocks: verified.length, replaced: verified.filter(s => !s.retained).length, elapsed_ms: report.elapsed_ms, output }));
} catch (e) {
  const d = { code: e.code ?? 'strip-attachment-failed', severity: 'error', stage: e.stage ?? 'reciprocal-integration', message: e.message };
  if (output) write(path.join(output, 'failure.json'), d); console.error(JSON.stringify(d)); process.exitCode = 1;
}
