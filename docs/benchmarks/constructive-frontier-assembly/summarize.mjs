import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fillMetrics } from '../constructive-shape-packing/model.mjs';
import { emptyRectangles } from '../constructive-local-reembedding/groups.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const keys = ['min', 'max', 'min-first', 'max-first', 'min-refined', 'max-refined', 'min-audit', 'max-audit', 'incumbent', 'output'];
try {
  const args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const results = [];
  for (const cut of ['min', 'max']) {
    const r = read(options[cut]), first = read(options[`${cut}-first`]), refined = read(options[`${cut}-refined`]), audit = read(options[`${cut}-audit`]);
    const current = Object.entries(r.source_hashes).every(([p, h]) => hash(new URL(p, import.meta.url)) === h);
    const complete = r.attempts.filter(a => a.complete).map(a => a.completed);
    const hashes = x => x.attempts.filter(a => a.complete).map(a => a.completed.witness_sha256).sort();
    const reproduced = JSON.stringify(hashes(r)) === JSON.stringify(hashes(first));
    const rw = read(path.join(refined.selected_directory, 'detail.json')).witness;
    if (!current || !reproduced || !refined.factory_validated || !audit.all_passed) throw new Error(`Unverified ${cut} result`);
    if (audit.cases.find(c => c.name === 'unchanged-complete-factory')?.witness_sha256 !== hash(path.join(refined.selected_directory, 'witness.json')))
      throw new Error(`Audit witness differs from ${cut} refinement`);
    results.push({ source_cut: cut, newly_complete: r.newly_complete, closure_attempts: r.attempts.length,
      initial_members: r.preparation.map(b => b.selected_covered), available_cuts: r.preparation.map(b => b.available_cuts),
      waves: r.waves.map(w => ({ wave: w.wave, covered: w.banks.map(b => b.covered), groups: w.groups.map(g => {
        if (!g.selected.length) return { selected: null, targets: g.targets.length };
        const d = read(path.join(g.selected[0], 'detail.json')), p = d.pre_compaction_plan ?? d;
        return { first_retained_directory: g.selected[0], targets: g.targets.length, fill: fillMetrics(d.witness),
          permutation: p.permutation ? { width: p.permutation.width, height: p.permutation.height,
            crossings: p.permutation.modules.length, construction: p.permutation.construction,
            scope: 'abstract pre-compaction template, not final measured crossing area' } : null };
      }) })),
      new_complete: complete, elapsed_ms: r.elapsed_ms, initial_observation_ms: first.elapsed_ms,
      current_source_hashes_match: current, complete_witnesses_reproduced: reproduced,
      refinement: { selected_directory: refined.selected_directory, initial_score: refined.initial_score, score: refined.score,
        mode: refined.mode, rounds: refined.rounds, move_limit: refined.move_limit, move_family: refined.move_family,
        endpoint_policy: refined.endpoint_policy, attempts: refined.stages.length, elapsed_ms: refined.elapsed_ms,
        termination: refined.termination, fill: fillMetrics(rw), empty_rectangles: emptyRectangles(rw, 3),
        witness_sha256: hash(path.join(refined.selected_directory, 'witness.json')), source_hashes: refined.source_hashes },
      audit: { all_passed: audit.all_passed, cases: audit.cases.map(c => ({ name: c.name, passed: c.passed })) } });
  }
  const incumbent = read(options.incumbent);
  const summary = { schema_version: 1, scope: 'interface-first source-cut controls; no density threshold or global minimum claim',
    incumbent: { selected_directory: incumbent.selected_directory, score: incumbent.score }, results,
    provenance: keys.filter(k => k !== 'output').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })) };
  fs.writeFileSync(options.output, JSON.stringify(summary, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: 'frontier-results-verified', cases: results.length, output: options.output }));
} catch (e) { console.error(JSON.stringify({ code: 'frontier-summary-failed', message: e.message })); process.exitCode = 1; }
