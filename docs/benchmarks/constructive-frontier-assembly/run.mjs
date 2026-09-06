import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { composeBlocks, placeRegions } from '../constructive-block-composition/model.mjs';
import { packShapes, diverse } from '../constructive-shape-packing/model.mjs';
import { packingCandidates } from '../constructive-cluster-packing/model.mjs';
import { inspectCoverage, better } from '../constructive-factory-closure/model.mjs';
import { compactInterface, witnessScore } from '../constructive-auto-interface/compact.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
import { regionInterface } from '../constructive-scc-groups/interface.mjs';
import { sourcePortfolio, eastInterface, frontierGroups } from './model.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const signature = w => createHash('sha256').update(JSON.stringify(w)).digest('hex');
const sources = ['run.mjs', 'model.mjs', '../constructive-block-composition/model.mjs', '../constructive-auto-interface/embedding.mjs',
  '../constructive-interconnect/geometry.mjs', '../constructive-interconnect/tracks.mjs', '../constructive-auto-interface/trees.mjs', '../constructive-auto-interface/lower.mjs',
  '../constructive-auto-interface/compact.mjs', '../constructive-shape-packing/model.mjs', '../constructive-cluster-packing/model.mjs',
  '../constructive-cluster-packing/nesting.mjs', '../constructive-cluster-packing/pockets.mjs', '../constructive-cycle-blocks/graph.mjs',
  '../constructive-factory-closure/model.mjs', '../constructive-factory-closure/run.mjs', '../constructive-scc-groups/interface.mjs',
  '../constructive-embedding-compaction/validator.mjs', '../constructive-bound-interconnect/view.mjs'];
const sourceHashes = Object.fromEntries(sources.map(p => [p, hash(new URL(p, import.meta.url))]));
let output;
try {
  const keys = ['graph', 'regions', 'incumbent', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2); if (!args[i].startsWith('--') || ![...keys, 'source-cut', 'shape-limit', 'packing-limit', 'route-family'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`); options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const cut = options['source-cut'] ?? 'max', shapeLimit = Number(options['shape-limit'] ?? 4), packingLimit = Number(options['packing-limit'] ?? 8), routeFamily = options['route-family'] ?? 'simple';
  if (!['min', 'max'].includes(cut) || !['simple', 'perimeter'].includes(routeFamily) || ![shapeLimit, packingLimit].every(n => Number.isSafeInteger(n) && n >= 1 && n <= 16)) throw new Error('Expected min|max source cut, simple|perimeter routes and limits 1..16');
  const destination = path.resolve(options['output-directory']); fs.mkdirSync(destination); output = destination; const start = performance.now();
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])])), graph = normalizeGraph(read(options.graph), catalogs), inventory = read(options.regions);
  const catalogArgs = ['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]);
  const validate = async (detail, name, { complete = false, east = false } = {}) => {
    const directory = path.join(output, name); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), detail.witness);
    const proc = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'), ...catalogArgs, '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    write(path.join(directory, 'validator-process.json'), proc);
    if (proc.timed_out || !fs.existsSync(path.join(directory, 'validation.json'))) throw new Error(`Validator did not complete in ${directory}`);
    const v = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(graph, detail, v, complete), boundary = complete ? null : regionInterface(graph, detail.witness, catalogs), face = east ? eastInterface(graph, detail, catalogs) : null;
    const accepted = proc.code === 0 && coverage.accepted && (boundary?.accepted ?? true) && (face?.accepted ?? true), factory_validated = complete && accepted && coverage.factory_validated;
    detail = { ...detail, accepted, factory_validated }; write(path.join(directory, 'detail.json'), detail); write(path.join(directory, 'coverage.json'), coverage);
    if (boundary) write(path.join(directory, 'interface.json'), { ...boundary, east_face: face });
    fs.writeFileSync(path.join(directory, 'index.html'), render(detail, v, catalogs), { flag: 'wx' });
    return { directory, accepted, factory_validated, score: witnessScore(detail.witness), covered: detail.witness.placements.length, witness_sha256: hash(path.join(directory, 'witness.json')),
      diagnostics: [...v.diagnostics, ...coverage.diagnostics, ...(boundary?.diagnostics ?? []), ...(face && !face.accepted ? [{ code: 'frontier-east-interface-lost', stage: 'frontier-assembly', entity: face.demands.filter(g => g.ports.length < g.required) }] : [])] };
  };
  const incumbentReport = read(options.incumbent), incumbent = await validate(read(path.join(incumbentReport.selected_directory, 'detail.json')), 'incumbent', { complete: true });
  if (!incumbent.factory_validated) throw new Error('Complete incumbent failed current graph/catalog validation');
  let banks = []; const preparation = [], owner = new Set();
  for (const [i, b] of inventory.banks.entries()) {
    const alternatives = [...b.variants, { directory: b.parent_directory }].map(v => ({ directory: v.directory, detail: read(path.join(v.directory, 'detail.json')) }));
    const all = new Set(alternatives.flatMap(v => v.detail.witness.placements.map(p => p.instance)));
    if ([...all].some(id => owner.has(id)) || alternatives.some(v => b.required_members.some(id => !v.detail.witness.placements.some(p => p.instance === id)))) throw new Error(`Invalid source ownership for ${b.id}`);
    all.forEach(id => owner.add(id));
    // Parent geometry is also independently checked before using it as a lookahead seed.
    const parent = await validate(alternatives.at(-1).detail, `parent-${i}`); if (!parent.accepted) throw new Error(`Retained parent rejected for ${b.id}`);
    const portfolio = sourcePortfolio(graph, alternatives, catalogs, { cut, limit: shapeLimit }), checked = await Promise.all(portfolio.variants.map((v, j) => validate(v.detail, `source-${i}-${j}`, { east: true })));
    const variants = checked.flatMap((r, j) => r.accepted ? [{ directory: r.directory, detail: portfolio.variants[j].detail }] : []);
    preparation.push({ bank: b.id, cut, available_cuts: portfolio.available_cuts, selected_covered: portfolio.covered, generated: portfolio.generated, exclusions: portfolio.exclusions, checks: checked, parent_check: parent });
    if (!variants.length) throw new Error(`No independently validated source pose for ${b.id}`);
    banks.push({ id: b.id, variants });
  }
  const waves = [];
  for (let wave = 0; wave < graph.nodes.length; wave++) {
    const groups = frontierGroups(graph, banks, catalogs); if (!groups.some(g => g.targets.length)) break;
    const nextBanks = [], reports = []; let cursor = 0, progressed = false;
    await Promise.all(Array.from({ length: 2 }, async () => {
      while (cursor < groups.length) {
        const i = cursor++, g = groups[i]; if (!g.targets.length) { nextBanks.push(...g.banks); continue; }
        const began = performance.now(), candidates = [], failures = [], seen = new Set(); let packingSearch = null;
        const add = d => { const sig = signature(d.witness); if (!seen.has(sig)) { seen.add(sig); candidates.push(d); } };
        const compose = (regions, offsets = null) => {
          try { const result = composeBlocks(catalogs, graph, regions, g.targets, { lanePolicy: 'portfolio', regionOffsets: offsets }); failures.push(...result.failures);
            for (const d of result.candidates) { add(d); add(compactInterface(d, catalogs)); } }
          catch (e) { if (!e.code) throw e; failures.push({ code: e.code, stage: e.stage, message: e.message, entity: e.entity }); }
        };
        if (g.banks.length === 1) for (const v of g.banks[0].variants) compose([{ id: g.banks[0].id, detail: v.detail }]);
        else {
          compose(g.banks.map(b => ({ id: b.id, detail: b.variants[0].detail })));
          const packed = packShapes(graph, g.banks, catalogs, { beamWidth: shapeLimit, limit: shapeLimit }); packingSearch = packed.search;
          for (const p of packed.candidates) compose(p.regions, p.offsets);
        }
        const checked = [];
        for (let j = 0; j < candidates.length; j += 4) checked.push(...await Promise.all(candidates.slice(j, j + 4).map((d, k) => validate(d, `wave-${wave}-group-${i}-${j + k}`, { east: true }))));
        const valid = checked.flatMap((r, j) => r.accepted ? [{ ...r, detail: candidates[j], ...r.score, tiles: r.score.physical_tiles, signature: r.witness_sha256 }] : []), selected = diverse(valid, shapeLimit);
        reports.push({ group: i, sources: g.banks.map(b => b.id), targets: g.targets, packing_search: packingSearch, failures, candidates: checked, selected: selected.map(r => r.directory), elapsed_ms: performance.now() - began });
        if (selected.length) { progressed = true; nextBanks.push({ id: `wave-${wave}-group-${i}`, variants: selected.map(v => ({ directory: v.directory, detail: v.detail })) }); }
        else nextBanks.push(...g.banks);
      }
    }));
    banks = nextBanks.sort((a, b) => a.id.localeCompare(b.id)); waves.push({ wave, groups: reports.sort((a, b) => a.group - b.group), banks: banks.map(b => ({ id: b.id, covered: b.variants[0].detail.witness.placements.length, variants: b.variants.length })), progressed });
    write(path.join(output, `wave-${wave}.json`), waves.at(-1)); if (!progressed) break;
  }
  const regions = banks.map(b => ({ id: b.id, detail: b.variants[0].detail })), families = [];
  const shelf = packingCandidates(graph, regions, catalogs);
  const packed = packShapes(graph, banks, catalogs, { beamWidth: shapeLimit, limit: Math.ceil(packingLimit / 2) });
  const occupied = packed.candidates.map(p => {
    const placed = placeRegions(p.regions, p.offsets, catalogs);
    return { schema_version: 1, case: 'actual-graph-cluster-packing', embedding: 'frontier-occupied-packing', witness: placed.witness, catalog_ports: placed.ports, labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])),
      port_groups: { sources: [], targets: [] }, accepted: false, factory_validated: false, packing_plan: p.plan };
  });
  const shelfPortfolio = diverse(shelf.map(detail => { const score = witnessScore(detail.witness); return { detail, ...score, tiles: score.physical_tiles, signature: signature(detail.witness) }; }), Math.ceil(packingLimit / 2)).map(c => c.detail);
  for (let i = 0; families.length < packingLimit && (i < occupied.length || i < shelfPortfolio.length); i++) {
    if (i < occupied.length) families.push({ family: 'occupied', detail: occupied[i] });
    if (families.length < packingLimit && i < shelfPortfolio.length) families.push({ family: 'shelf', detail: shelfPortfolio[i] });
  }
  const attempts = []; let index = 0;
  await Promise.all(Array.from({ length: 2 }, async () => {
    while (index < families.length) {
      const i = index++, p = families[i], checked = await validate(p.detail, `packing-${i}`), attempt = { family: p.family, initial: checked, complete: false }; attempts[i] = attempt;
      if (!checked.accepted) continue;
      write(path.join(checked.directory, 'baseline.json'), { selected_directory: checked.directory, factory_validated: false });
      const directory = path.join(output, `closure-${i}`), proc = await executeValidator(process.execPath, [new URL('../constructive-factory-closure/run.mjs', import.meta.url).pathname,
        '--graph', options.graph, '--baseline', path.join(checked.directory, 'baseline.json'), '--route-family', routeFamily, '--placement-family', 'two-sided', '--candidate-budget', '64', ...catalogArgs,
        '--validator', options.validator, '--output-directory', directory], path.join(output, `closure-${i}.log`), { timeoutMs: 120000 });
      attempt.process = proc;
      if (fs.existsSync(path.join(directory, 'report.json'))) {
        const r = read(path.join(directory, 'report.json')); attempt.closure = { report: path.join(directory, 'report.json'), covered: r.covered_facilities, score: r.score, selected_directory: r.selected_directory, failures: r.failures };
        if (proc.code === 0 && r.factory_validated) { const complete = await validate(read(path.join(r.selected_directory, 'detail.json')), `complete-${i}`, { complete: true }); attempt.complete = complete.factory_validated; attempt.completed = complete; }
      } else attempt.failure = fs.existsSync(path.join(directory, 'failure.json')) ? read(path.join(directory, 'failure.json')) : { code: 'frontier-closure-no-report' };
      write(path.join(output, `attempt-${i}.json`), attempt);
    }
  }));
  const complete = attempts.filter(a => a.complete).sort((a, b) => better(a.completed.score.tuple, b.completed.score.tuple) ? -1 : better(b.completed.score.tuple, a.completed.score.tuple) ? 1 : 0);
  const selected = complete.length && better(complete[0].completed.score.tuple, incumbent.score.tuple) ? complete[0].completed : incumbent;
  const report = { schema_version: 1, status: selected === incumbent ? 'complete-incumbent-retained' : 'complete-factory-improved', factory_validated: true, newly_complete: complete.length,
    selected_directory: selected.directory, best_new_directory: complete[0]?.completed.directory ?? null, score: selected.score, incumbent_score: incumbent.score,
    source_cut: cut, shape_limit: shapeLimit, packing_limit: packingLimit, route_family: routeFamily, preparation, waves, final_packing_search: packed.search, attempts, elapsed_ms: performance.now() - start,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })), source_hashes: sourceHashes };
  write(path.join(output, 'report.json'), report); console.log(JSON.stringify({ status: report.status, newly_complete: report.newly_complete, attempts: attempts.length, output }));
} catch (e) { const failure = { code: e.code ?? 'frontier-assembly-failed', stage: e.stage ?? 'execution', severity: 'error', message: e.message, entity: e.entity };
  if (output) write(path.join(output, 'failure.json'), failure); console.error(JSON.stringify({ code: failure.code, message: failure.message })); process.exitCode = 1; }
