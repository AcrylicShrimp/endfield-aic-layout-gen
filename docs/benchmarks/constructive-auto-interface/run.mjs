import fs from 'node:fs';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { constructInterface } from './model.mjs';
import { compactInterface, witnessScore } from './compact.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';

const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
const read = p => JSON.parse(fs.readFileSync(p));
async function validate(detail, catalogs, options, directory) {
  const ioStart = performance.now();
  fs.mkdirSync(directory);
  write(path.join(directory, 'witness.json'), detail.witness);
  const inputIoMs = performance.now() - ioStart;
  const start = performance.now();
  const code = await new Promise((resolve, reject) => {
    const log = fs.createWriteStream(path.join(directory, 'validator.log'), { flags: 'wx' });
    const child = spawn(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')]);
    const guard = setTimeout(() => child.kill(), 30000);
    child.stdout.pipe(log, { end: false }); child.stderr.pipe(log, { end: false });
    child.on('error', e => { clearTimeout(guard); log.end(); reject(e); });
    child.on('close', code => { clearTimeout(guard); log.end(); resolve(code); });
  });
  const validationProcessMs = performance.now() - start;
  const reportPath = path.join(directory, 'validation.json');
  if (!fs.existsSync(reportPath)) {
    detail = { ...detail, accepted: false, factory_validated: false }; write(path.join(directory, 'detail.json'), detail);
    return { detail, directory, accepted: false, code: 'interface-validator-process-failed', exit_code: code,
      validation_process_ms: validationProcessMs, input_io_ms: inputIoMs };
  }
  const validation = read(reportPath), accepted = code === 0 && validation.covered_geometry_validated && validation.recipe_coverage_complete;
  detail = { ...detail, accepted, factory_validated: false }; write(path.join(directory, 'detail.json'), detail);
  const renderStart = performance.now();
  fs.writeFileSync(path.join(directory, 'index.html'), render(detail, validation, catalogs), { flag: 'wx' });
  return { detail, validation, directory, accepted, score: witnessScore(detail.witness), validation_process_ms: validationProcessMs,
    input_io_ms: inputIoMs, render_io_ms: performance.now() - renderStart };
}
try {
  const required = ['request', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'];
  const args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...required, 'embedding'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of required) if (!options[k]) throw new Error(`Missing --${k}`);
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  try {
    const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
    const mode = options.embedding ?? 'portfolio', start = performance.now(), generated = constructInterface(catalogs, read(options.request), mode);
    const candidates = mode === 'portfolio' ? [...generated.candidates] : [generated], failures = generated.failures ?? [];
    if (mode === 'portfolio') for (const detail of [...candidates]) {
      try { candidates.push(compactInterface(detail, catalogs)); }
      catch (e) { failures.push({ embedding: detail.embedding + '-compacted', code: 'interface-compaction-failed', severity: 'error', stage: 'compaction', message: e.message }); }
    }
    const constructionMs = performance.now() - start;
    // Independent candidates validate concurrently; no unvalidated score enters selection.
    const results = await Promise.all(candidates.map(async detail => {
      const directory = path.join(output, detail.embedding);
      try { return await validate(detail, catalogs, options, directory); }
      catch (e) { return { detail, directory, accepted: false, code: 'interface-candidate-processing-failed', message: e.message }; }
    }));
    const valid = results.filter(r => r.accepted);
    const compare = (a, b) => { for (let i = 0; i < a.score.tuple.length; i++) if (a.score.tuple[i] !== b.score.tuple[i]) return a.score.tuple[i] - b.score.tuple[i]; return 0; };
    valid.sort(compare);
    const selected = valid[0], report = { schema_version: 2, status: selected ? 'complete-local-interface' :
      candidates.length ? 'independent-validation-rejected' : 'interface-generation-failed',
      accepted: Boolean(selected), factory_validated: false, request: path.resolve(options.request), construction_ms: constructionMs,
      selected_embedding: selected?.detail.embedding ?? null, failures,
      candidates: results.map(r => ({ embedding: r.detail.embedding, accepted: r.accepted, score: r.score, directory: r.directory,
        validation_process_ms: r.validation_process_ms, input_io_ms: r.input_io_ms, render_io_ms: r.render_io_ms,
        diagnostics: r.validation?.diagnostics ?? [{ code: r.code, severity: 'error', stage: 'validation',
          message: r.message ?? 'Validator process produced no report', exit_code: r.exit_code }] })) };
    if (selected) {
      const { detail, validation } = selected;
      Object.assign(report, { facilities: detail.witness.placements.length, lanes: detail.bindings.length,
        bridges: detail.witness.logistics_components.filter(c => c.kind === 'bridge').length,
        splitters: detail.witness.logistics_components.filter(c => c.kind === 'splitter').length,
        convergers: detail.witness.logistics_components.filter(c => c.kind === 'converger').length,
        decisions: detail.decisions, pending_recipe_terminals: validation.recipe_terminals.filter(t => t.pending_rate.numerator > 0).length });
      for (const [name, value] of [['detail.json', detail], ['witness.json', detail.witness], ['validation.json', validation]]) write(path.join(output, name), value);
      fs.writeFileSync(path.join(output, 'index.html'), render(detail, validation, catalogs), { flag: 'wx' });
    }
    report.elapsed_before_report_ms = performance.now() - start;
    write(path.join(output, 'report.json'), report);
    console.log(JSON.stringify({ status: report.status, selected: report.selected_embedding, facilities: report.facilities,
      score: selected?.score.tuple, construction_ms: constructionMs, output }));
    if (!selected) process.exitCode = 2;
  } catch (e) {
    write(path.join(output, 'failure.json'), { code: e.code ?? 'interface-execution-failed', severity: 'error', stage: e.stage ?? 'execution',
      message: e.message, entity: e.entity ?? null, evidence: e.evidence ?? null, request: path.resolve(options.request) }); throw e;
  }
} catch (e) { console.error(JSON.stringify({ code: e.code ?? 'interface-execution-failed', message: e.message })); process.exitCode = 1; }
