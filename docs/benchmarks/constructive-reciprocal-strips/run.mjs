import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { stripPortfolio } from './model.mjs';
import { normalizeGraph, extractCycles } from '../constructive-cycle-blocks/graph.mjs';
import { inspectBlockCoverage } from '../constructive-cycle-blocks/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
let output;
try {
  const keys = ['graph', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const directory = path.resolve(options['output-directory']); fs.mkdirSync(directory); output = directory;
  const began = performance.now(), catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const graph = normalizeGraph(read(options.graph), catalogs), inventory = extractCycles(graph, catalogs), jobs = [], neighborhoods = [], failures = [...inventory.failures];
  const start = performance.now();
  for (const block of inventory.blocks) {
    try {
      const r = stripPortfolio(graph, catalogs, block); jobs.push(...r.candidates);
      neighborhoods.push({ block: block.id, families: r.families, layouts: r.layouts, rejected: r.rejected, candidates: r.candidates.length, enumeration_complete: r.enumeration_complete });
      failures.push(...r.diagnostics.map(d => ({ block: block.id, ...d })));
    } catch (e) { if (!e.code) throw e; failures.push({ block: block.id, code: e.code, severity: e.severity, stage: e.stage, message: e.message, entity: e.entity }); }
  }
  const constructionMs = performance.now() - start, results = []; let next = 0;
  await Promise.all(Array.from({ length: Math.min(4, jobs.length) }, async () => {
    while (next < jobs.length) {
      const detail = jobs[next++], directory = path.join(output, `${detail.block.id}-${detail.embedding}`); fs.mkdirSync(directory);
      write(path.join(directory, 'witness.json'), detail.witness);
      const proc = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
        ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
      write(path.join(directory, 'process.json'), proc);
      if (proc.timed_out || !fs.existsSync(path.join(directory, 'validation.json'))) throw new Error(`Validator timed out or exited without a report in ${directory}; inspect process.json and validator.log`);
      const v = read(path.join(directory, 'validation.json')), coverage = inspectBlockCoverage(detail, v, catalogs);
      detail.accepted = proc.code === 0 && coverage.accepted;
      write(path.join(directory, 'detail.json'), detail); write(path.join(directory, 'coverage.json'), coverage);
      results.push({ block: detail.block.id, directory, accepted: detail.accepted, score: witnessScore(detail.witness), embedding: detail.embedding,
        pending_terminals: v.recipe_terminals.filter(t => t.pending_rate.numerator > 0).length, diagnostics: [...v.diagnostics, ...coverage.diagnostics], witness_sha256: hash(path.join(directory, 'witness.json')) });
    }
  }));
  const compare = (a, b) => { for (let i = 0; i < 3; i++) if (a.score.tuple[i] !== b.score.tuple[i]) return a.score.tuple[i] - b.score.tuple[i]; return a.embedding.localeCompare(b.embedding); };
  const selected = [...new Set(results.map(r => r.block))].sort().map(id => results.filter(r => r.block === id && r.accepted).sort(compare)[0]).filter(Boolean);
  for (const r of selected) fs.writeFileSync(path.join(output, `${r.block}.html`), render(read(path.join(r.directory, 'detail.json')), read(path.join(r.directory, 'validation.json')), catalogs), { flag: 'wx' });
  const report = { schema_version: 1, status: selected.length ? 'partial-reciprocal-strips-validated' : 'no-validated-strip', factory_validated: false,
    construction_ms: constructionMs, elapsed_ms: performance.now() - began, neighborhoods, selected, candidates: results.sort((a, b) => a.directory.localeCompare(b.directory)), failures,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['model.mjs', 'allocation.mjs', 'run.mjs', '../constructive-cycle-blocks/graph.mjs', '../constructive-cycle-blocks/model.mjs', '../constructive-factory-closure/model.mjs',
      '../constructive-placement-only/model.mjs', '../constructive-interconnect/rates.mjs', '../constructive-auto-interface/compact.mjs', '../constructive-bound-interconnect/view.mjs',
      '../constructive-local-reembedding/path.mjs', '../constructive-local-reembedding/movement.mjs', '../constructive-embedding-compaction/validator.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), report);
  console.log(JSON.stringify({ status: report.status, selected: selected.length, candidates: results.length, construction_ms: constructionMs, elapsed_ms: report.elapsed_ms, output }));
  if (!selected.length || results.some(r => !r.accepted)) process.exitCode = 2;
} catch (e) {
  const d = { code: e.code ?? 'strip-execution-failed', severity: 'error', stage: e.stage ?? 'reciprocal-strip', message: e.message };
  if (output) write(path.join(output, 'failure.json'), d);
  console.error(JSON.stringify(d)); process.exitCode = 1;
}
