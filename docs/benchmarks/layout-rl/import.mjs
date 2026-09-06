import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args, catalogKeys, diagnostic, fail, hash, read, write, sourcePaths, Probe } from './io.mjs';
import { prepareRoot, checkRoot, semanticKey } from './coverage.mjs';
import { canonicalKey } from './geometry.mjs';
import { score } from './score.mjs';

export async function importRoots(options) {
  const planPath = path.resolve(options.plan), plan = read(planPath), output = path.resolve(options['output-directory']);
  if (plan.schema_version !== 1 || !Number.isSafeInteger(plan.seed) || !plan.catalogs || !Array.isArray(plan.closure_sources)) fail('input-invalid', 'Invalid root import plan', planPath);
  const resolve = p => path.resolve(path.dirname(planPath), p), paths = Object.fromEntries(catalogKeys.map(k => [k, resolve(plan.catalogs[k])]));
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(paths[k])])), validator = resolve(plan.validator), comparison = resolve(plan.chain_comparison);
  const sources = new Set([planPath, validator, comparison, ...Object.values(paths), ...sourcePaths(fileURLToPath(import.meta.url))]);
  const candidates = [], importRows = [], roots = [], seen = new Map();
  for (const [index, row] of read(comparison).rows.entries()) {
    if (!row.complete) { importRows.push({ source: 'chain-comparison', index, status: 'producer-stopped' }); continue; }
    const witnessPath = row.witness.path, requestPath = row.original_request;
    if (hash(witnessPath) !== row.witness.sha256) fail('stale-input', 'Generator witness hash mismatch', witnessPath);
    sources.add(witnessPath); sources.add(requestPath); sources.add(row.report.path);
    candidates.push({ id: `chain-${row.case}-${index}`, generation: 'chain', dialect: 'chain-factory', family: row.case,
      witness: read(witnessPath), request: read(requestPath), origin: { witness: witnessPath, request: requestPath, report: row.report.path, variant: row.variant.id } });
  }
  for (const source of plan.closure_sources) {
    const reportPath = resolve(source.report), requestPath = resolve(source.request), report = read(reportPath), witnessPath = path.join(report.selected_directory, 'witness.json');
    sources.add(reportPath); sources.add(requestPath); sources.add(witnessPath);
    candidates.push({ id: source.id, generation: 'closure', dialect: 'closure-factory', family: 'heavy-xiranite', witness: read(witnessPath), request: read(requestPath),
      origin: { witness: witnessPath, request: requestPath, report: reportPath } });
  }
  const provenance = [...sources].map(p => ({ path: p, sha256: hash(p) }));
  fs.mkdirSync(output); const probe = new Probe({ ...paths, validator }, plan.environment.validator_ms), started = performance.now();
  try {
    for (const candidate of candidates) {
      const fp = canonicalKey(candidate.witness, catalogs);
      if (seen.has(fp)) { importRows.push({ id: candidate.id, status: 'duplicate-root', retained: seen.get(fp), fingerprint: fp }); continue; }
      const graph = prepareRoot(candidate, catalogs), validation = await probe.check(candidate.witness), coverage = checkRoot(candidate, graph, candidate.witness, validation);
      if (!coverage.factory_validated) fail('root-rejected', 'Original generated root failed fresh complete validation', { id: candidate.id, coverage });
      const root = { ...candidate, fingerprint: fp, semantic_key: semanticKey(candidate.witness), score: score(candidate.witness, catalogs), factory_validated: true };
      roots.push(root); seen.set(fp, root.id);
      write(path.join(output, `${root.id}.validation.json`), { validation, coverage });
      importRows.push({ id: root.id, status: 'accepted', fingerprint: fp, score: root.score });
    }
    // Whole-root split, deterministic within family; no descendant is ever assigned separately.
    for (const family of new Set(roots.filter(r => r.generation === 'chain').map(r => r.family))) {
      const group = roots.filter(r => r.family === family).sort((a, b) => a.fingerprint.localeCompare(b.fingerprint));
      for (const [i, root] of group.entries()) root.split = group.length === 2 ? (i === 0 ? 'train' : 'test') : i === 0 ? 'test' : i === 1 ? 'validation' : 'train';
    }
    for (const root of roots.filter(r => r.generation === 'closure')) root.split = 'out-of-family';
    for (const p of provenance) if (hash(p.path) !== p.sha256) fail('stale-input', 'Input changed during root import', p.path);
    write(path.join(output, 'roots.json'), { schema_version: 1, factory_validated: true, roots, options: { ...paths, validator }, environment: plan.environment,
      provenance, elapsed_ms: performance.now() - started, import_rows: importRows });
    console.log(JSON.stringify({ status: 'complete', roots: roots.length, splits: Object.fromEntries([...new Set(roots.map(r => r.split))].map(s => [s, roots.filter(r => r.split === s).length])), output }));
  } finally { probe.close(); }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { await importRoots(args(['plan', 'output-directory'])); }
  catch (e) { console.error(JSON.stringify(diagnostic(e.code?.replace(/^rl-/, '') ?? 'import-failed', 'import', e.message, e.entity))); process.exitCode = 1; }
}
