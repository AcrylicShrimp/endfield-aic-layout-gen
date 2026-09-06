import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { prepareFactory } from './factory-input.mjs';
import { checkFactoryState } from './factory-check.mjs';
import { diagnostic } from './input.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';

// Intentionally does not import the constructor or read its report/history/success flag.
try {
  const catalogKeys = ['recipes', 'facilities', 'items', 'transports', 'components'];
  const keys = ['request', 'witness', ...catalogKeys, 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex'), read = p => JSON.parse(fs.readFileSync(p, 'utf8'));
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(options[k])])), graph = prepareFactory(read(options.request), catalogs), witness = read(options.witness);
  const provenance = keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) }));
  for (const relative of ['./verify-factory.mjs', './factory-input.mjs', './factory-check.mjs', './input.mjs', '../constructive-interconnect/rates.mjs', '../constructive-embedding-compaction/validator.mjs']) {
    const source = fileURLToPath(new URL(relative, import.meta.url)); provenance.push({ kind: 'implementation', path: source, sha256: hash(source) });
  }
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const validatorProcess = await executeValidator(path.resolve(options.validator), ['--witness', options.witness, ...catalogKeys.flatMap(k => [`--${k}`, options[k]]),
    '--report-output', path.join(output, 'probe.json')], path.join(output, 'validator.log'), { timeoutMs: 30000, graceMs: 1000 });
  const probe = fs.existsSync(path.join(output, 'probe.json')) ? read(path.join(output, 'probe.json')) : null;
  let coverage = null, diagnostics = [];
  try {
    if (validatorProcess.code !== 0) throw new Error(`Independent validator process returned ${validatorProcess.code}`);
    coverage = checkFactoryState(graph, witness, probe, { complete: true });
  } catch (e) { diagnostics.push(diagnostic(e.code?.startsWith('chain-') ? e.code.slice(6) : 'validation-failed', 'factory-verification', e.message, e.entity ?? null)); }
  for (const p of provenance) if (hash(p.path) !== p.sha256) throw new Error(`Input changed during verification: ${p.path}`);
  const report = { schema_version: 1, factory_validated: coverage?.factory_validated === true && !diagnostics.length,
    independent: { covered_geometry_validated: probe?.covered_geometry_validated ?? false, recipe_coverage_complete: probe?.recipe_coverage_complete ?? false,
      probe_factory_validated: probe?.factory_validated ?? false, scope: 'Rust geometry/recipe validation plus separate original-request factory coverage; constructor not invoked' },
    coverage, process: validatorProcess, diagnostics, provenance };
  fs.writeFileSync(path.join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ factory_validated: report.factory_validated, diagnostics, output }));
  if (!report.factory_validated) globalThis.process.exitCode = 2;
} catch (e) { console.error(JSON.stringify({ code: e.code ?? 'chain-verification-input-invalid', severity: 'error', stage: 'factory-verification', message: e.message, entity: null })); process.exitCode = 1; }
