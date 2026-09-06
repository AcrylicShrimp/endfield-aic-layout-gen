import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const digest = v => createHash('sha256').update(JSON.stringify(v)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
try {
  const keys = ['baseline', 'catalog-directory', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const report = read(options.baseline), original = read(path.join(report.selected_directory, 'detail.json')).witness;
  const bridges = original.logistics_components.filter(c => c.kind === 'bridge');
  if (!report.factory_validated || !bridges.length) throw new Error('Expected a validated baseline with bridges');
  const before = digest(original), output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const results = [], validate = async (w, name, valid, component = null) => {
    const wp = path.join(output, `${name}.json`), vp = path.join(output, `${name}-validation.json`); write(wp, w);
    const p = await executeValidator(path.resolve(options.validator), ['--witness', wp,
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, path.join(options['catalog-directory'], `${k === 'components' ? 'logistics-components' : k}.json`)]),
      '--report-output', vp], path.join(output, `${name}.log`));
    const v = read(vp);
    results.push({ name, component, passed: !p.timed_out && p.code === (valid ? 0 : 2) && v.covered_geometry_validated === valid,
      diagnostics: v.diagnostics, witness_sha256: digest(w) });
  };
  await validate(original, 'unchanged', true);
  for (const [i, c] of bridges.entries()) {
    const owners = original.transport_networks.filter(n => n.component_ids.includes(c.id));
    if (owners.length !== 2) throw new Error(`Expected two commodity owners for bridge ${c.id}`);
    const removed = structuredClone(original);
    removed.logistics_components = removed.logistics_components.filter(q => q.id !== c.id);
    for (const n of removed.transport_networks) n.component_ids = n.component_ids.filter(id => id !== c.id);
    await validate(removed, `bridge-${i}-absent`, false, c.id);
    for (const [j, owner] of owners.entries()) {
      const missing = structuredClone(original), n = missing.transport_networks.find(n => n.id === owner.id);
      n.component_ids = n.component_ids.filter(id => id !== c.id);
      await validate(missing, `bridge-${i}-owner-${j}-absent`, false, c.id);
    }
  }
  const result = { schema_version: 1, all_passed: results.every(r => r.passed) && digest(original) === before,
    input_unchanged: digest(original) === before, baseline: path.resolve(options.baseline), cases: results };
  write(path.join(output, 'audit.json'), result);
  console.log(JSON.stringify({ status: result.all_passed ? 'crossing-controls-passed' : 'crossing-controls-failed', cases: results.length, output }));
  if (!result.all_passed) process.exitCode = 1;
} catch (e) { console.error(JSON.stringify({ code: 'crossing-audit-failed', message: e.message })); process.exitCode = 1; }
