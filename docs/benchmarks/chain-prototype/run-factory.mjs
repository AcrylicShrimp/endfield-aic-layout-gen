import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { constructFactory } from './factory.mjs';
import { prepareFactory } from './factory-input.mjs';
import { diagnostic } from './input.mjs';
import { ports } from './geometry.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';

const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const read = p => JSON.parse(fs.readFileSync(p, 'utf8'));
const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value, null, 2) + '\n', { flag: 'wx' });
try {
  const catalogKeys = ['recipes', 'facilities', 'items', 'transports', 'components'];
  const keys = ['request', ...catalogKeys, 'validator', 'output-directory'], args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(options[k])]));
  const graph = prepareFactory(read(options.request), catalogs), output = path.resolve(options['output-directory']);
  const provenance = keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) }));
  for (const relative of ['./run-factory.mjs', './factory.mjs', './factory-input.mjs', './factory-check.mjs', './factory-geometry.mjs', './factory-routing.mjs', './factory-boundary.mjs', './factory-access.mjs',
    './geometry.mjs', './input.mjs', './work.mjs', '../constructive-local-reembedding/path.mjs', '../constructive-placement-only/model.mjs', '../constructive-interconnect/rates.mjs',
    '../constructive-auto-interface/compact.mjs', '../constructive-embedding-compaction/validator.mjs', '../constructive-bound-interconnect/view.mjs']) {
    const source = fileURLToPath(new URL(relative, import.meta.url)); provenance.push({ kind: 'implementation', path: source, sha256: hash(source) });
  }
  fs.mkdirSync(output); let ordinal = 0;
  const result = await constructFactory(graph, async (witness, meta, remainingMs) => {
    const directory = path.join(output, `validation-${String(ordinal++).padStart(4, '0')}`); fs.mkdirSync(directory);
    write(path.join(directory, 'witness.json'), witness); write(path.join(directory, 'decision.json'), meta);
    const process = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...catalogKeys.flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'), { timeoutMs: Math.min(30000, remainingMs), graceMs: 1000 });
    write(path.join(directory, 'process.json'), process);
    return { status: process.code, report: fs.existsSync(path.join(directory, 'validation.json')) ? read(path.join(directory, 'validation.json')) : null,
      artifact: { directory, witness_sha256: hash(path.join(directory, 'witness.json')) } };
  });
  if (result.retained) write(path.join(output, 'retained-witness.json'), result.retained.witness);
  if (result.selected) {
    const w = result.selected.witness, validation = read(path.join(result.selected.artifact.directory, 'validation.json'));
    write(path.join(output, 'factory-witness.json'), w);
    const detail = { schema_version: 1, case: 'generated-chain-factory', accepted: true, factory_validated: true, witness: w,
      catalog_ports: Object.fromEntries(w.placements.map(p => [p.instance, ports(p, graph.nodes.get(p.instance))])), labels: Object.fromEntries(w.placements.map(p => [p.instance, p.instance])) };
    const scriptJson = v => JSON.stringify(v).replaceAll('<', '\\u003c');
    const script = `<script>document.querySelector('h1').textContent=${scriptJson(`Validated factory · ${graph.id}`)};
document.title='Validated constructive factory';document.getElementById('normal').remove();document.getElementById('overload').remove();
document.getElementById('itemB').style.display=sources.length<2?'none':'';
const originalDraw=draw;draw=()=>{originalDraw();document.getElementById('scope').textContent='Complete original graph, exact recipe rates and explicit external perimeter lanes checked. Rust geometry and recipe coverage PASS; separate original-request coverage PASS. Power excluded, bootstrap unverified. Generated incrementally without screenshot poses; no optimality claim.';};draw();</script>`;
    fs.writeFileSync(path.join(output, 'index.html'), render(detail, validation, catalogs).replace('</html>', script + '</html>'), { flag: 'wx' });
  }
  for (const p of provenance) if (hash(p.path) !== p.sha256) throw new Error(`Input changed during run: ${p.path}`);
  const { retained, selected, ...rest } = result;
  const reference = v => v ? Object.fromEntries(Object.entries(v).filter(([k]) => k !== 'witness')) : null;
  write(path.join(output, 'report.json'), { ...rest, retained: reference(retained), selected: reference(selected), provenance });
  console.log(JSON.stringify({ status: result.status, factory_validated: result.factory_validated, covered: retained?.witness.placements.length ?? 0,
    score: selected?.score ?? null, budget: result.budget, diagnostics: result.diagnostics, output }));
  if (!selected) process.exitCode = 2;
} catch (e) {
  console.error(JSON.stringify(e.code?.startsWith('chain-') ? { code: e.code, severity: 'error', stage: e.stage, message: e.message, entity: e.entity } : diagnostic('input-invalid', 'factory-cli', e.message)));
  process.exitCode = 1;
}
