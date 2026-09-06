import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { normalizeGraph, components } from '../constructive-cycle-blocks/graph.mjs';
import { bankCandidates } from './model.mjs';
import { projectRegion } from '../constructive-scc-groups/project.mjs';
import { regionInterface } from '../constructive-scc-groups/interface.mjs';
import { eastInterface } from '../constructive-frontier-assembly/model.mjs';
import { inspectCoverage, better } from '../constructive-factory-closure/model.mjs';
import { composeBlocks } from '../constructive-block-composition/model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
let output;
try {
  const keys = ['graph', 'blocks', 'block', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2); if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`); options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const destination = path.resolve(options['output-directory']); fs.mkdirSync(destination); output = destination; const start = performance.now();
  const sourcePaths = ['run.mjs', 'model.mjs', '../constructive-reciprocal-strips/model.mjs', '../constructive-reciprocal-strips/graph-delta.mjs',
    '../constructive-scc-groups/project.mjs', '../constructive-scc-groups/interface.mjs', '../constructive-block-composition/model.mjs',
    '../constructive-auto-interface/embedding.mjs', '../constructive-auto-interface/compact.mjs', '../constructive-interconnect/geometry.mjs', '../constructive-interconnect/tracks.mjs'];
  const sourceHashes = Object.fromEntries(sourcePaths.map(p => [p, hash(new URL(p, import.meta.url))]));
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const snapshot = read(options.graph), graph = normalizeGraph(snapshot, catalogs), blocks = read(options.blocks), selectedBlock = blocks.selected.find(b => b.block === options.block);
  if (!selectedBlock) throw new Error(`Unknown retained block ${options.block}`);
  const parent = read(path.join(selectedBlock.directory, 'detail.json')), members = parent.witness.placements.map(p => p.instance), checked = [], failures = [];
  const validate = async (detail, g, name) => {
    const directory = path.join(output, name); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), detail.witness);
    const proc = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    write(path.join(directory, 'validator-process.json'), proc);
    if (proc.timed_out || !fs.existsSync(path.join(directory, 'validation.json'))) throw new Error(`Validator failed in ${directory}`);
    const v = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(g, detail, v, false), boundary = regionInterface(g, detail.witness, catalogs);
    const accepted = proc.code === 0 && coverage.accepted && boundary.accepted;
    detail = { ...detail, accepted, factory_validated: false }; write(path.join(directory, 'detail.json'), detail);
    write(path.join(directory, 'coverage.json'), coverage); write(path.join(directory, 'interface.json'), boundary);
    fs.writeFileSync(path.join(directory, 'index.html'), render(detail, v, catalogs), { flag: 'wx' });
    const record = { directory, accepted, score: witnessScore(detail.witness), members: detail.witness.placements.length,
      witness_sha256: hash(path.join(directory, 'witness.json')), diagnostics: [...v.diagnostics, ...coverage.diagnostics, ...boundary.diagnostics] };
    checked.push(record); return { ...record, detail };
  };
  const control = await validate(parent, graph, 'retained-parent'); if (!control.accepted) throw new Error('Retained source bank failed current validation');
  const began = performance.now(), generated = bankCandidates(graph, catalogs, members), generationMs = performance.now() - began;
  const candidates = [], coreReports = [];
  for (const [i, c] of generated.candidates.entries()) {
    const core = await validate(c.detail, c.graph, `core-${i}`), record = { core: core.directory, accepted: core.accepted, score: core.score, east_interface: eastInterface(c.graph, c.detail, catalogs).accepted, prefixes: [] };
    coreReports.push(record); if (!core.accepted || !record.east_interface) continue;
    let owned = [], valid = true;
    while (owned.length < c.family.members.length) {
      const atoms = components(c.graph).filter(a => a.every(id => c.family.members.includes(id) && !owned.includes(id)) &&
        c.graph.edges.filter(e => a.includes(e.target)).every(e => e.internal && (a.includes(e.source) || owned.includes(e.source))));
      if (!atoms.length) throw new Error('Rebuilt source family has no input-closed next atom');
      owned = [...owned, ...atoms[0]];
      const projected = projectRegion(c.graph, c.detail, owned, catalogs, options.block);
      const prefix = await validate(projected, c.graph, `core-${i}-prefix-${record.prefixes.length}`);
      record.prefixes.push({ directory: prefix.directory, members: owned.length, accepted: prefix.accepted });
      if (!prefix.accepted) { valid = false; break; }
    }
    if (!valid) continue;
    try {
      const composed = composeBlocks(catalogs, c.graph, [{ id: options.block, detail: c.detail }], c.family.consumers, { lanePolicy: 'portfolio' });
      failures.push(...composed.failures);
      const alternatives = [...composed.candidates, ...composed.candidates.map(d => compactInterface(d, catalogs))];
      for (const [j, detail] of alternatives.entries()) {
        detail.reallocation = c.detail.reallocation;
        const bound = await validate(detail, c.graph, `bound-${i}-${j}`);
        if (bound.accepted) candidates.push({ ...bound, graph: c.graph, core: core.directory });
      }
    } catch (e) { if (!e.code) throw e; failures.push({ code: e.code, message: e.message, entity: e.entity }); }
  }
  candidates.sort((a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : a.directory.localeCompare(b.directory));
  const selected = candidates[0]; if (!selected) throw new Error('No fully bound replacement bank; see candidate files');
  const allocation = JSON.stringify(selected.detail.reallocation), variants = candidates.filter(c => JSON.stringify(c.detail.reallocation) === allocation);
  const record = c => ({ block: options.block, directory: c.directory, accepted: true, score: c.score, witness_sha256: c.witness_sha256 });
  // Independently recheck every unaffected bank; retain its original witness/path.
  for (const [i, b] of blocks.selected.entries()) if (b.block !== options.block) {
    const r = await validate(read(path.join(b.directory, 'detail.json')), selected.graph, `unchanged-${i}`);
    if (!r.accepted) throw new Error(`Unrelated source bank changed validity: ${b.block}`);
  }
  write(path.join(output, 'graph.json'), { ...snapshot, internal_requirements: selected.graph.edges.filter(e => e.internal), external_requirements: selected.graph.edges.filter(e => !e.internal) });
  write(path.join(output, 'blocks.json'), { schema_version: 1, selected: blocks.selected.map(b => b.block === options.block ? record(selected) : b),
    candidates: [...blocks.candidates.filter(c => c.block !== options.block), ...variants.map(record)] });
  const report = { schema_version: 1, status: 'replacement-bank-validated', factory_validated: false, block: options.block, selected_directory: selected.directory, core_directory: selected.core,
    score: selected.score, previous_score: control.score, generation_ms: generationMs, elapsed_ms: performance.now() - start,
    counts: generated.counts, core_candidates: generated.candidates.length, bound_candidates: candidates.length, retained_variants: variants.length,
    core_reports: coreReports, candidates: checked, failures, reallocation: selected.detail.reallocation,
    provenance: keys.filter(k => !['output-directory', 'block'].includes(k)).map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })), source_hashes: sourceHashes };
  write(path.join(output, 'report.json'), report); console.log(JSON.stringify({ status: report.status, score: report.score.tuple, output }));
} catch (e) {
  const failure = { code: e.code ?? 'reciprocal-bank-failed', severity: 'error', stage: e.stage ?? 'execution', message: e.message };
  if (output) write(path.join(output, 'failure.json'), failure); console.error(JSON.stringify(failure)); process.exitCode = 1;
}
