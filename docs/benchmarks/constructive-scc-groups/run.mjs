import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { sourceSccs, projectRegion, concreteAllocation, readyAtoms } from './project.mjs';
import { regionInterface } from './interface.mjs';
import { applyReallocation } from '../constructive-reciprocal-strips/graph-delta.mjs';
import { directAttachments } from '../constructive-reciprocal-strips/direct.mjs';
import { diverse, fillMetrics } from '../constructive-shape-packing/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { inspectCoverage } from '../constructive-factory-closure/model.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
let output;
try {
  const keys = ['graph', 'blocks', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'growth-depth', 'beam-width'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const depth = Number(options['growth-depth'] ?? 2), beamWidth = Number(options['beam-width'] ?? 4);
  if (!Number.isSafeInteger(depth) || depth < 0 || depth > 2 || !Number.isSafeInteger(beamWidth) || beamWidth < 1 || beamWidth > 8) throw new Error('Expected growth depth 0..2 and beam width 1..8');
  const destination = path.resolve(options['output-directory']); fs.mkdirSync(destination); output = destination;
  const began = performance.now(), timing = { candidate_generation_ms: 0, seed_projection_ms: 0, interface_ms: 0, validator_task_ms: 0, render_ms: 0 };
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const snapshot = read(options.graph), parents = read(options.blocks).selected.map(s => ({ ...s, detail: read(path.join(s.directory, 'detail.json')) }));
  let graph = normalizeGraph(snapshot, catalogs);
  const banks = [], failures = [], allocations = []; let ordinal = 0, generated = 0;
  const validate = async (detail, name, checkInterface = false) => {
    const directory = path.join(output, name); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), detail.witness);
    const validatorStart = performance.now();
    const proc = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    timing.validator_task_ms += performance.now() - validatorStart;
    if (proc.timed_out || !fs.existsSync(path.join(directory, 'validation.json'))) throw new Error(`Validator did not finish in ${directory}`);
    const v = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(graph, detail, v, false);
    const interfaceStart = performance.now(), boundary = checkInterface ? regionInterface(graph, detail.witness, catalogs) : null;
    timing.interface_ms += performance.now() - interfaceStart;
    detail.accepted = proc.code === 0 && coverage.accepted && (boundary?.accepted ?? true); detail.factory_validated = false;
    if (boundary) write(path.join(directory, 'interface.json'), boundary);
    write(path.join(directory, 'detail.json'), detail); write(path.join(directory, 'coverage.json'), coverage);
    const renderStart = performance.now();
    if (detail.accepted) fs.writeFileSync(path.join(directory, 'index.html'), render(detail, v, catalogs), { flag: 'wx' });
    timing.render_ms += performance.now() - renderStart;
    const fill = fillMetrics(detail.witness);
    return { directory, accepted: detail.accepted, score: witnessScore(detail.witness), fill: { ...fill, facility_fill_ratio: fill.facility_xy / fill.area },
      members: detail.witness.placements.map(p => p.instance), diagnostics: [...v.diagnostics, ...coverage.diagnostics, ...(boundary?.diagnostics ?? [])], witness_sha256: hash(path.join(directory, 'witness.json')) };
  };
  for (const [i, parent] of parents.entries()) {
    const checked = await validate(structuredClone(parent.detail), `allocation-parent-${i}`);
    if (!checked.accepted) throw new Error(`Retained allocation parent rejected: ${parent.block}`);
    if (new Set(parent.detail.witness.transport_networks.map(n => n.item)).size !== parent.detail.witness.transport_networks.length)
      throw Object.assign(new Error('Allocation projection currently requires one retained network per item'), { code: 'scc-allocation-network-scope' });
    const own = new Set(checked.members);
    for (const n of parent.detail.witness.transport_networks) {
      if (parent.detail.witness.logistics_components.some(c => n.component_ids.includes(c.id) && c.kind === 'bridge')) throw new Error('Retained bridge requires channel-aware flow decomposition');
      const replacement_edges = concreteAllocation(n).map((e, j) => ({ ...e, id: JSON.stringify(['concrete-source-allocation', parent.block, n.item, j]) }));
      const delta = { item: n.item, replaced_edge_ids: graph.edges.filter(e => e.internal && e.item === n.item && own.has(e.source) && own.has(e.target)).map(e => e.id), replacement_edges };
      graph = applyReallocation(graph, delta); allocations.push({ parent: parent.block, ...delta });
    }
  }
  const emitted = { ...snapshot, internal_requirements: graph.edges.filter(e => e.internal), external_requirements: graph.edges.filter(e => !e.internal) };
  graph = normalizeGraph(emitted, catalogs); write(path.join(output, 'graph.json'), emitted);
  const seeds = sourceSccs(graph);
  for (const seed of seeds) {
    const parent = parents.find(p => seed.members.every(id => p.detail.witness.placements.some(q => q.instance === id)));
    if (!parent) throw new Error(`No retained source geometry for ${seed.id}`);
    const checkedParent = await validate(structuredClone(parent.detail), `${seed.id}-parent`);
    if (!checkedParent.accepted) throw new Error(`Retained parent rejected for ${seed.id}`);
    const projectionStart = performance.now(), detail = projectRegion(graph, parent.detail, seed.members, catalogs, seed.id);
    timing.seed_projection_ms += performance.now() - projectionStart;
    const base = await validate(detail, `${seed.id}-seed`, true);
    if (!base.accepted) throw new Error(`Source SCC rejected for ${seed.id}; inspect validation.json`);
    const bank = { id: seed.id, required_members: seed.members, parent_directory: parent.directory, levels: [[base]], variants: [base] }; banks.push(bank);
    let current = [{ detail, record: base }];
    for (let level = 1; level <= depth; level++) {
      const generationStart = performance.now(), candidates = [], seen = new Set();
      for (const state of current) for (const atom of readyAtoms(graph, state.record.members)) {
        if (!atom.every(id => parent.detail.witness.placements.some(p => p.instance === id))) continue;
        const options = atom.length === 1 ? directAttachments(catalogs, graph, state.detail, atom[0], { orientationPolicy: 'all' }) : [];
        try { options.push(projectRegion(graph, parent.detail, [...state.record.members, ...atom], catalogs, seed.id)); }
        catch (e) { failures.push({ code: e.code, stage: e.stage, message: e.message, seed: seed.id, level, atom }); }
        for (const d of options) {
          d.growth = { depth: level, added_atom: atom, parent_directory: state.record.directory };
          const signature = createHash('sha256').update(JSON.stringify(d.witness)).digest('hex'); if (seen.has(signature)) continue; seen.add(signature);
          const score = witnessScore(d.witness); candidates.push({ detail: d, signature, ...score, tiles: score.physical_tiles });
        }
      }
      generated += candidates.length;
      // Keep stop states at every depth; diversity is heuristic, not a dominance claim.
      const selected = diverse(candidates, beamWidth);
      timing.candidate_generation_ms += performance.now() - generationStart;
      const checked = await Promise.all(selected.map(async c => ({ detail: c.detail, record: await validate(c.detail, `${seed.id}-level-${level}-${ordinal++}`, true) })));
      current = checked.filter(c => c.record.accepted); bank.levels.push(checked.map(c => c.record)); bank.variants.push(...current.map(c => c.record));
      if (!current.length) break;
    }
  }
  const report = { schema_version: 1, status: 'scc-stop-grow-regions-validated', factory_validated: false, banks, allocations, failures, generated_candidates: generated,
    growth_depth: depth, beam_width: beamWidth, elapsed_ms: performance.now() - began, timing,
    scope: { seed_rule: 'source-scc', extension_ownership: 'retained-parent', assembly_implemented: false, fill_selection_implemented: false },
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['run.mjs', 'project.mjs', 'interface.mjs', '../constructive-cycle-blocks/graph.mjs', '../constructive-reciprocal-strips/direct.mjs', '../constructive-reciprocal-strips/graph-delta.mjs', '../constructive-shape-packing/model.mjs',
      '../constructive-factory-closure/output-space.mjs', '../constructive-cluster-packing/pockets.mjs',
      '../constructive-factory-closure/model.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'regions.json'), report);
  console.log(JSON.stringify({ status: report.status, banks: banks.length, variants: banks.reduce((s, b) => s + b.variants.length, 0), output }));
} catch (e) {
  const d = { code: e.code ?? 'scc-groups-execution-failed', severity: 'error', stage: e.stage ?? 'execution', message: e.message, entity: e.entity };
  if (output) write(path.join(output, 'failure.json'), d); console.error(JSON.stringify({ code: d.code, message: d.message })); process.exitCode = 1;
}
