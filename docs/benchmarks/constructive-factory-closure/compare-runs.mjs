import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
const read = p => JSON.parse(fs.readFileSync(p));
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const equal = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const snapshot = r => ({ status: r.status, factory_validated: r.factory_validated, covered_facilities: r.covered_facilities,
  total_facilities: r.total_facilities, score: r.score, failures: r.failures,
  stages: r.stages.map(s => ({ node: s.node, label: s.label, trials: s.trials, failures: s.failures,
    selected: s.candidates.findIndex(c => c.directory === s.selected_directory),
    candidates: s.candidates.map(c => ({ accepted: c.accepted, factory_validated: c.factory_validated,
      score: c.score, witness_sha256: c.witness_sha256, diagnostics: c.diagnostics })) })) });
try {
  const options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !['before', 'after', 'output'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of ['before', 'after', 'output']) if (!options[k]) throw new Error(`Missing --${k}`);
  const a = read(options.before), b = read(options.after), comparisons = [];
  function compareClosure(ap, bp) {
    const ar = read(ap), br = read(bp), selectedA = hash(path.join(ar.selected_directory, 'witness.json')), selectedB = hash(path.join(br.selected_directory, 'witness.json'));
    comparisons.push({ before: ap, after: bp, identical: equal(snapshot(ar), snapshot(br)) && selectedA === selectedB,
      candidate_count: ar.stages.reduce((n, s) => n + s.candidates.length, 0), selected_witness_sha256: selectedB,
      before_elapsed_ms: ar.elapsed_ms, after_elapsed_ms: br.elapsed_ms });
  }
  let packingIdentical = true;
  if (Boolean(a.packings) !== Boolean(b.packings)) throw new Error('Expected matching closure or packing report kinds');
  if (a.packings) {
    packingIdentical = a.packings.length === b.packings.length && equal(a.score, b.score) && a.factory_validated === b.factory_validated;
    for (let i = 0; i < a.packings.length; i++) {
      const ap = a.packings[i], bp = b.packings[i];
      if (!bp || ap.witness_sha256 !== bp.witness_sha256 || ap.closures?.length !== bp.closures?.length) { packingIdentical = false; continue; }
      for (const ac of ap.closures ?? []) {
        const bc = bp.closures.find(c => c.family === ac.family);
        if (!bc?.report || !ac.report) { packingIdentical = false; continue; }
        compareClosure(ac.report, bc.report);
      }
    }
  } else compareClosure(options.before, options.after);
  const result = { schema_version: 1, all_passed: packingIdentical && comparisons.length > 0 && comparisons.every(c => c.identical), packing_identical: packingIdentical,
    comparisons, candidate_count: comparisons.reduce((n, c) => n + c.candidate_count, 0),
    provenance: ['before', 'after'].map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })) };
  fs.writeFileSync(options.output, JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: result.all_passed ? 'identical' : 'changed', closures: comparisons.length, candidates: result.candidate_count, output: options.output }));
  if (!result.all_passed) process.exitCode = 2;
} catch (e) { console.error(JSON.stringify({ code: 'closure-comparison-failed', message: e.message })); process.exitCode = 1; }
