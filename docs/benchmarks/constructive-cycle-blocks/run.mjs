import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { normalizeGraph, extractCycles } from './graph.mjs';
import { constructCycle, inspectCycleCoverage } from './model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
let failureDirectory;

try {
  const keys = ['graph', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(key) || options[key] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[key] = args[i + 1];
  }
  for (const key of keys) if (!options[key]) throw new Error(`Missing --${key}`);
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output); failureDirectory = output;
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const start = performance.now(), graph = normalizeGraph(read(options.graph), catalogs), inventory = extractCycles(graph, catalogs);
  const graphMs = performance.now() - start, inventoryIoStart = performance.now();
  write(path.join(output, 'graph.json'), graph); write(path.join(output, 'inventory.json'), inventory);
  const inventoryIoMs = performance.now() - inventoryIoStart, constructionStart = performance.now();
  const candidates = [], results = [], failures = [];
  for (const block of inventory.blocks) for (const family of ['leaf', 'root']) {
    try {
      const detail = constructCycle(catalogs, graph, block, family); candidates.push(detail);
      try { candidates.push(compactInterface(detail, catalogs)); }
      catch (e) { failures.push({ block: block.id, family, code: 'cycle-compaction-failed', severity: 'error', stage: 'compaction', message: e.message }); }
    }
    catch (e) { failures.push({ block: block.id, family, code: e.code ?? 'cycle-generation-failed', severity: 'error', stage: e.stage ?? 'cycle-block', message: e.message, entity: e.entity }); }
  }
  const constructionMs = performance.now() - constructionStart;
  const validate = async detail => {
    const directory = path.join(output, `${detail.block.id}-${detail.embedding}`); fs.mkdirSync(directory);
    write(path.join(directory, 'detail.json'), detail); write(path.join(directory, 'witness.json'), detail.witness);
    const code = await new Promise((resolve, reject) => {
      const log = fs.createWriteStream(path.join(directory, 'validator.log'), { flags: 'wx' });
      const child = spawn(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
        ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')]);
      const guard = setTimeout(() => child.kill(), 30000);
      child.stdout.pipe(log, { end: false }); child.stderr.pipe(log, { end: false });
      child.on('error', e => { clearTimeout(guard); log.end(); reject(e); });
      child.on('close', code => { clearTimeout(guard); log.end(); resolve(code); });
    });
    const validation = read(path.join(directory, 'validation.json')), coverage = inspectCycleCoverage(detail, validation, catalogs), accepted = code === 0 && coverage.accepted;
    write(path.join(directory, 'coverage.json'), coverage);
    fs.writeFileSync(path.join(directory, 'index.html'), render(detail, validation, catalogs), { flag: 'wx' });
    return { block: detail.block.id, embedding: detail.embedding, directory, accepted, facilities: detail.witness.placements.length,
      score: witnessScore(detail.witness), pending_terminals: validation.recipe_terminals.filter(t => t.pending_rate.numerator > 0).length,
      validator_ms: validation.elapsed_ms, diagnostics: [...validation.diagnostics, ...coverage.diagnostics], witness_sha256: hash(path.join(directory, 'witness.json')) };
  };
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(4, candidates.length) }, async () => {
    while (next < candidates.length) { const detail = candidates[next++];
      try { results.push(await validate(detail)); }
      catch (e) { failures.push({ block: detail.block.id, embedding: detail.embedding, code: 'cycle-validation-process', severity: 'error', stage: 'validation', message: e.message }); }
    }
  }));
  const selected = inventory.blocks.map(b => results.filter(r => r.block === b.id && r.accepted).sort((a, b) => {
    for (let i = 0; i < 3; i++) if (a.score.tuple[i] !== b.score.tuple[i]) return a.score.tuple[i] - b.score.tuple[i]; return a.embedding.localeCompare(b.embedding);
  })[0]).filter(Boolean);
  for (const r of selected) fs.copyFileSync(path.join(r.directory, 'index.html'), path.join(output, `${r.block}.html`), fs.constants.COPYFILE_EXCL);
  const report = { schema_version: 1, status: selected.length === inventory.blocks.length && !inventory.failures.length && inventory.blocks.length ? 'covered-cycle-blocks-validated' : 'cycle-blocks-incomplete',
    factory_validated: false, graph_preparation_ms: graphMs, inventory_io_ms: inventoryIoMs, construction_ms: constructionMs, elapsed_ms: performance.now() - start,
    total_facilities: graph.nodes.length, covered_facilities: selected.reduce((s, r) => s + r.facilities, 0),
    remaining_facilities: graph.nodes.length - selected.reduce((s, r) => s + r.facilities, 0),
    selected, candidates: results.sort((a, b) => (a.block + a.embedding).localeCompare(b.block + b.embedding)), failures: [...inventory.failures, ...failures],
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['graph.mjs', 'model.mjs', 'run.mjs', '../constructive-auto-interface/compact.mjs', '../constructive-auto-interface/trees.mjs',
      '../constructive-auto-interface/model.mjs', '../constructive-auto-interface/embedding.mjs', '../constructive-auto-interface/lower.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), report);
  console.log(JSON.stringify({ status: report.status, blocks: selected.length, covered_facilities: report.covered_facilities, remaining_facilities: report.remaining_facilities, output }));
  if (report.status !== 'covered-cycle-blocks-validated') process.exitCode = 2;
} catch (e) {
  const failure = { code: e.code ?? 'cycle-execution-failed', severity: 'error', stage: e.stage ?? 'execution', message: e.message, entity: e.entity ?? null };
  if (failureDirectory) write(path.join(failureDirectory, 'failure.json'), failure);
  console.error(JSON.stringify({ code: failure.code, message: failure.message })); process.exitCode = 1;
}
