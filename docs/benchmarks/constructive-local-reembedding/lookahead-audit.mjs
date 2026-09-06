import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { executeLookaheadProcess } from './lookahead-process.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
try {
  const options = {}, args = process.argv.slice(2), keys = ['seed-report', 'output-directory'];
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const original = read(options['seed-report']), before = hash(options['seed-report']);
  const detailPath = path.join(original.selected_directory, 'detail.json'), originalDetail = read(detailPath), beforeDetail = hash(detailPath);
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const inputArgs = original.provenance.filter(p => p.kind !== 'baseline').flatMap(p => [`--${p.kind}`, p.path]);
  const runner = fileURLToPath(new URL('lookahead-run.mjs', import.meta.url)), results = [];
  const run = async (name, seedReport, { baseline = options['seed-report'], extra = [], code = 0, error = null, status = null } = {}) => {
    const seedPath = path.join(output, `${name}-seed-report.json`), directory = path.join(output, name); write(seedPath, seedReport);
    const p = await executeLookaheadProcess(process.execPath, [runner, ...inputArgs, '--baseline', baseline, '--seed-report', seedPath,
      '--output-directory', directory, ...extra], path.join(output, `${name}.log`), { timeoutMs: 30000 });
    const report = fs.existsSync(path.join(directory, 'report.json')) ? read(path.join(directory, 'report.json')) : null;
    const failure = fs.existsSync(path.join(directory, 'failure.json')) ? read(path.join(directory, 'failure.json')) : null;
    const observedError = failure?.code ?? fs.readFileSync(path.join(output, `${name}.log`), 'utf8').trim().split('\n').map(s => { try { return JSON.parse(s).code; } catch { return null; } }).find(Boolean);
    const passed = p.code === code && !p.timed_out && (!error || observedError === error) && (!status || report?.status === status);
    results.push({ name, passed, process: p, directory_created: fs.existsSync(directory), error: observedError ?? null,
      status: report?.status ?? null, branch_statuses: report?.branches.map(b => b.status) ?? [],
      witness_sha256: report?.witness_sha256 ?? null, seed_diagnostics: report?.seed_diagnostics ?? [] });
    return report;
  };
  await run('invalid-budget', original, { extra: ['--workers', '0'], code: 1, error: 'lookahead-input' });
  const stale = structuredClone(original); stale.source_hashes['model.mjs'] = '0'.repeat(64);
  await run('stale-engine', stale, { code: 1, error: 'lookahead-source-mismatch' });
  const mismatch = structuredClone(original); mismatch.provenance.find(p => p.kind === 'graph').sha256 = '0'.repeat(64);
  await run('different-graph', mismatch, { code: 1, error: 'lookahead-input-mismatch' });
  const empty = { ...original, stages: [] };
  const retained = await run('no-seeds', empty, { status: 'complete-incumbent-retained' });
  results.at(-1).passed &&= retained?.termination === 'no-eligible-seeds' && retained?.factory_validated === true;
  const badDirectory = path.join(output, 'bad-external'); fs.mkdirSync(badDirectory);
  const badDetail = structuredClone(originalDetail), terminal = badDetail.witness.transport_networks.flatMap(n => n.terminals).find(t => t.endpoint.kind === 'external');
  terminal.node = terminal.endpoint.node = 'undeclared-lookahead-external'; write(path.join(badDirectory, 'detail.json'), badDetail);
  const sourceStage = original.stages.find(s => s.status === 'unimproved-validated' && s.block_transform);
  if (!sourceStage) throw new Error('Expected a block-transform audit in the seed report');
  const forged = { ...original, stages: [{ ...sourceStage, directory: badDirectory, factory_validated: true }] };
  const rejected = await run('forged-seed-validity', forged, { status: 'complete-incumbent-retained' });
  results.at(-1).passed &&= rejected?.branches.length === 1 && rejected.branches[0].status === 'seed-validation-rejected' && rejected.witness_sha256 === retained.witness_sha256;
  const missing = { ...original, stages: [{ ...sourceStage, directory: path.join(output, 'missing-artifact') }] };
  const missingResult = await run('missing-seed-artifact', missing, { status: 'complete-incumbent-retained' });
  results.at(-1).passed &&= missingResult?.seed_diagnostics[0]?.code === 'lookahead-seed-artifact' && missingResult.witness_sha256 === retained.witness_sha256;
  const badBaseline = path.join(output, 'invalid-root.json'); write(badBaseline, { selected_directory: badDirectory, factory_validated: true });
  await run('forged-root-validity', empty, { baseline: badBaseline, code: 1, error: 'lookahead-root-invalid' });
  for (const r of results.slice(0, 3)) r.passed &&= !r.directory_created;
  const unchanged = hash(options['seed-report']) === before && hash(detailPath) === beforeDetail;
  const result = { schema_version: 1, all_passed: results.every(r => r.passed) && unchanged, input_unchanged: unchanged,
    seed_report: path.resolve(options['seed-report']), seed_report_sha256: before, cases: results };
  write(path.join(output, 'audit.json'), result);
  console.log(JSON.stringify({ status: result.all_passed ? 'lookahead-controls-passed' : 'lookahead-controls-failed', cases: results.length, output }));
  if (!result.all_passed) process.exitCode = 1;
} catch (e) { console.error(JSON.stringify({ code: 'lookahead-audit-failed', message: e.message })); process.exitCode = 1; }
