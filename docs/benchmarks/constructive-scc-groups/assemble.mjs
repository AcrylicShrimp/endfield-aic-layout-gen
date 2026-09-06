import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { inspectCoverage, better } from '../constructive-factory-closure/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
import { regionInterface } from './interface.mjs';
import { packBanks, atomicUpgrades } from './pack.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const sourceHashes = Object.fromEntries(['assemble.mjs', 'pack.mjs', 'interface.mjs', 'project.mjs', '../constructive-factory-closure/model.mjs', '../constructive-factory-closure/run.mjs',
  '../constructive-factory-closure/output-space.mjs', '../constructive-block-composition/model.mjs', '../constructive-cluster-packing/nesting.mjs', '../constructive-bound-interconnect/view.mjs'].map(p => [p, hash(new URL(p, import.meta.url))]));
let output;
try {
  const keys = ['graph', 'regions', 'incumbent', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'beam-width', 'candidate-limit', 'order', 'route-family'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const beamWidth = Number(options['beam-width'] ?? 8), limit = Number(options['candidate-limit'] ?? 8), order = options.order ?? 'largest-first', routeFamily = options['route-family'] ?? 'simple';
  if (!['largest-first', 'smallest-first'].includes(order) || !['simple', 'perimeter'].includes(routeFamily)) throw new Error('Expected order largest-first|smallest-first and route family simple|perimeter');
  const destination = path.resolve(options['output-directory']); fs.mkdirSync(destination); output = destination; const start = performance.now();
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const graph = normalizeGraph(read(options.graph), catalogs), inventory = read(options.regions), incumbentReport = read(options.incumbent);
  const catalogArgs = ['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]);
  const validate = async (detail, name, complete = false) => {
    const directory = path.join(output, name); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), detail.witness);
    const proc = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'), ...catalogArgs, '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    if (proc.timed_out || !fs.existsSync(path.join(directory, 'validation.json'))) throw new Error(`Validator did not complete in ${directory}`);
    const v = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(graph, detail, v, complete), boundary = complete ? null : regionInterface(graph, detail.witness, catalogs);
    const accepted = proc.code === 0 && coverage.accepted && (boundary?.accepted ?? true), factory_validated = complete && accepted && coverage.factory_validated;
    detail = { ...detail, accepted, factory_validated };
    write(path.join(directory, 'detail.json'), detail); write(path.join(directory, 'coverage.json'), coverage);
    if (boundary) write(path.join(directory, 'interface.json'), boundary);
    fs.writeFileSync(path.join(directory, 'index.html'), render(detail, v, catalogs), { flag: 'wx' });
    return { directory, accepted, factory_validated, covered: detail.witness.placements.length, score: witnessScore(detail.witness),
      diagnostics: [...v.diagnostics, ...coverage.diagnostics, ...(boundary?.diagnostics ?? [])], witness_sha256: hash(path.join(directory, 'witness.json')) };
  };
  const incumbent = await validate(read(path.join(incumbentReport.selected_directory, 'detail.json')), 'incumbent', true);
  if (!incumbent.factory_validated) throw new Error('Complete incumbent failed current graph/catalog validation');
  // Recheck each retained variant; never trust a persisted accepted flag alone.
  const banks = [], sourceChecks = []; let ordinal = 0;
  for (const b of inventory.banks) {
    const variants = [];
    for (const v of b.variants) {
      const detail = read(path.join(v.directory, 'detail.json')), checked = await validate(detail, `source-${ordinal++}`); sourceChecks.push(checked);
      if (!checked.accepted) throw new Error(`Retained source variant rejected: ${v.directory}`);
      variants.push({ directory: v.directory, detail });
    }
    banks.push({ id: b.id, required_members: b.required_members, variants });
  }
  const packed = packBanks(graph, banks, catalogs, { beamWidth, limit, reverse: order === 'smallest-first' });
  write(path.join(output, 'packing-search.json'), packed.search);
  const constructions = []; let next = 0;
  await Promise.all(Array.from({ length: 2 }, async () => {
    while (next < packed.candidates.length) {
      const i = next++, candidate = packed.candidates[i], record = { index: i, stages: [], closures: [], complete: false }; constructions[i] = record;
      let current = await validate(candidate.detail, `packing-${i}`), choices = candidate.choices;
      record.initial = current; if (!current.accepted) continue;
      let atomicReady = true;
      for (let step = 0; step <= graph.nodes.length; step++) {
        const began = performance.now(), generated = atomicUpgrades(graph, choices, packed.prepared, catalogs);
        if (!generated.atoms.length) break;
        const checked = await Promise.all(generated.candidates.map((c, j) => validate(c.detail, `packing-${i}-atomic-${step}-${j}`)));
        const valid = checked.map((r, j) => ({ ...r, j })).filter(r => r.accepted).sort((a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : 0);
        record.stages.push({ atoms: generated.atoms, candidates: checked, failures: generated.failures, elapsed_ms: performance.now() - began, selected_directory: valid[0]?.directory ?? null });
        if (!valid.length) { atomicReady = false; break; }
        current = valid[0]; choices = generated.candidates[current.j].choices;
      }
      record.atomic_ready = atomicReady; record.preclosure = current;
      if (!atomicReady) continue;
      const baselinePath = path.join(current.directory, `baseline-for-${i}.json`); write(baselinePath, { selected_directory: current.directory, factory_validated: false });
      const directory = path.join(output, `closure-${i}`);
      const proc = await executeValidator(process.execPath, [new URL('../constructive-factory-closure/run.mjs', import.meta.url).pathname,
        '--graph', options.graph, '--baseline', baselinePath, '--route-family', routeFamily, '--placement-family', 'two-sided', '--candidate-budget', '64', ...catalogArgs,
        '--validator', options.validator, '--output-directory', directory], path.join(output, `closure-${i}.log`), { timeoutMs: 120000 });
      const attempt = { ...proc, route_family: routeFamily }; record.closures.push(attempt);
      if (fs.existsSync(path.join(directory, 'report.json'))) {
        const r = read(path.join(directory, 'report.json')); attempt.report = path.join(directory, 'report.json'); attempt.covered = r.covered_facilities;
        attempt.score = r.score; attempt.failures = r.failures; attempt.selected_directory = r.selected_directory;
        if (proc.code === 0 && r.factory_validated) {
          const check = await validate(read(path.join(r.selected_directory, 'detail.json')), `complete-${i}`, true);
          record.complete = check.factory_validated; record.completed = check;
        }
      } else attempt.failure = fs.existsSync(path.join(directory, 'failure.json')) ? read(path.join(directory, 'failure.json')) : { code: 'scc-closure-process-no-report' };
      write(path.join(output, `construction-${i}.json`), record);
    }
  }));
  const improved = constructions.filter(c => c.complete && better(c.completed.score.tuple, incumbent.score.tuple)).sort((a, b) => better(a.completed.score.tuple, b.completed.score.tuple) ? -1 : 1);
  const selected = improved[0]?.completed ?? incumbent;
  const report = { schema_version: 1, status: improved.length ? 'complete-factory-improved' : 'complete-incumbent-retained', factory_validated: true,
    newly_complete: constructions.filter(c => c.complete).length, selected_directory: selected.directory, score: selected.score, incumbent_score: incumbent.score,
    source_checks: sourceChecks, packing_search: packed.search, constructions, elapsed_ms: performance.now() - start,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: sourceHashes };
  write(path.join(output, 'assembly.json'), report);
  console.log(JSON.stringify({ status: report.status, newly_complete: report.newly_complete, attempts: constructions.length, output }));
} catch (e) {
  const failure = { code: e.code ?? 'scc-assembly-execution-failed', severity: 'error', stage: e.stage ?? 'execution', message: e.message, entity: e.entity };
  if (output) write(path.join(output, 'failure.json'), failure); console.error(JSON.stringify({ code: failure.code, message: failure.message })); process.exitCode = 1;
}
