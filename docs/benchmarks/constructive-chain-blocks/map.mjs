import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { interfaceMapping } from './interfaces.mjs';
try {
  const options = {}, allowed = ['placement', 'items', 'transports', 'report-output'], args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].replace(/^--/, '');
    if (!args[i].startsWith('--') || !allowed.includes(key) || options[key] || !args[i + 1]) throw new Error(`Invalid option ${args[i]}`);
    options[key] = args[i + 1];
  }
  for (const key of allowed) if (!options[key]) throw new Error(`Missing --${key}`);
  const output = path.resolve(options['report-output']); if (fs.existsSync(output)) throw new Error(`Output already exists: ${output}`);
  const inputs = {};
  const load = key => { const file = path.resolve(options[key]), bytes = fs.readFileSync(file);
    inputs[key] = { path: file, sha256: createHash('sha256').update(bytes).digest('hex') }; return JSON.parse(bytes); };
  const placement = load('placement'), items = load('items'), transports = load('transports');
  if (placement.provenance.inputs.items.sha256 !== inputs.items.sha256) throw new Error('Runtime items differ from frozen projection');
  const report = { ...interfaceMapping(placement, items, transports), provenance: { inputs } };
  fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: 'logical-interface-map-only', pairs: report.pairs.length, terminal_groups: report.terminal_groups.length,
    count_deficits: report.facility_interfaces.filter(g => !g.capacity_count_fits_catalog).length, report_output: output }));
} catch (error) { console.error(JSON.stringify({ code: 'block-interface-map-error', stage: 'interface-mapping', message: error.message })); process.exitCode = 1; }
