import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { extractChains, reembed, extentOf } from './model.mjs';
import { releaseBridgeDraft } from './components.mjs';
import { cooperativePortCandidates } from './cooperative.mjs';
import { movementPlan, bodyCells } from './movement.mjs';
import { minimumTerminalReleasePath, terminalAccessObstacles } from './terminal-access.mjs';
import { key } from './path.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, data) => fs.writeFileSync(p, JSON.stringify(data, null, 2) + '\n', { flag: 'wx' });
try {
  const args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !['report', 'catalog-directory', 'output-directory', 'scope'].includes(k) || k in options || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  if (['report', 'catalog-directory', 'output-directory'].some(k => !options[k])) throw new Error('Expected --report TERMINAL_ACCESS_REPORT --catalog-directory PATH --output-directory NEW_PATH [--scope terminals|interfaces]');
  const scope = options.scope ?? 'terminals'; if (!['terminals', 'interfaces'].includes(scope)) throw new Error('Expected --scope terminals|interfaces');
  const report = read(options.report), parentPath = path.join(report.parent_directory, 'detail.json'), parent = read(parentPath);
  if (report.chain_schema_version !== 2 || !report.terminal_access?.length) throw new Error('Expected chain schema 2 terminal-access replay');
  const access = report.terminal_access[0], request = report.records[0].request;
  if (access.status !== 'found' || report.records[0].status !== 'rejected') throw new Error('Expected a failed retained replay with a found relaxed terminal-access path');
  const paths = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, path.join(options['catalog-directory'], (k === 'components' ? 'logistics-components' : k) + '.json')]));
  const catalogs = Object.fromEntries(Object.entries(paths).map(([k, p]) => [k, read(p)]));
  for (const p of [parentPath, ...Object.values(paths)]) {
    const recorded = report.provenance.find(v => v.path === path.resolve(p));
    if (!recorded || recorded.sha256 !== hash(p)) throw new Error(`Input provenance mismatch: ${p}`);
  }
  const mismatches = Object.entries(report.source_hashes).filter(([p, sha]) => hash(new URL(p, import.meta.url)) !== sha).map(([p]) => p);
  const { witness, released } = releaseBridgeDraft(parent.witness, catalogs, request.releaseBridges ?? []), { chains, anchors } = extractChains(witness, released), chain = chains[access.id];
  if (!chain) throw new Error(`Unknown failed chain ${access.id}`);
  const ids = [...new Set([...access.blockers.flatMap(b => b.owners.map(o => o.terminal)),
    ...[chain.startTerminals, chain.endTerminals].filter(ts => ts.length === 1 && ts[0].endpoint.kind === 'facility').map(ts => ts[0].id)])];
  if (scope === 'interfaces') {
    const originalIds = new Set(ids);
    for (const c of chains) if ([...c.startTerminals, ...c.endTerminals].some(t => originalIds.has(t.id)))
      for (const ts of [c.startTerminals, c.endTerminals]) if (ts.length === 1 && ts[0].endpoint.kind === 'facility' && !ids.includes(ts[0].id)) ids.push(ts[0].id);
  }
  const began = performance.now(), neighborhood = cooperativePortCandidates(witness, catalogs, request.moves, ids), records = [];
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  for (const [assignment, candidate] of neighborhood.candidates.entries()) for (const order of ['near', 'far']) for (const prioritize of [false, true]) {
    const proposed = { ...request, chainIds: [], order, routePriority: prioritize ? [chain.id] : [], portAssignments: candidate.portAssignments };
    let result;
    try { result = reembed(parent, catalogs, proposed); }
    catch (e) { if (!e.code) throw e; result = { status: 'rejected', routes: [], diagnostics: [{ code: e.code, severity: e.severity, stage: e.stage, message: e.message, entity: e.entity }] }; }
    const record = { assignment, order, prioritize, status: result.status, diagnostics: result.diagnostics, routes: result.routes, elapsed_ms: result.elapsed_ms };
    if (result.detail) { record.candidate = path.join(output, `candidate-${records.length}.json`); write(record.candidate, result.detail); }
    records.push(record);
  }
  // Display the rejected replay that restored the most jobs; never promote partial routes.
  let representative = null;
  for (const [i, r] of records.entries()) if (r.status === 'rejected' && r.routes.some(p => p.status !== 'found') &&
    (representative === null || r.routes.filter(p => p.status === 'found').length > records[representative].routes.filter(p => p.status === 'found').length)) representative = i;
  let representativeAccess = null;
  if (representative !== null) {
    const record = records[representative], failed = chains[record.routes.find(p => p.status !== 'found').id];
    const rerouteChains = new Set(chains.filter(c => c.cells.some(p => released.has(`${c.transport}:${key(p)}`))).map(c => c.id));
    const movement = movementPlan(witness, chains, anchors, request.moves, { junctionPolicy: request.junctionPolicy, rerouteChains });
    const placements = witness.placements.map(p => ({ ...p, ...movement.shiftFacility(p) })), ports = project({ ...witness, placements }, catalogs);
    const assignments = new Map(neighborhood.candidates[record.assignment].portAssignments.map(a => [a.terminal, a.port]));
    const terminalPosition = t => assignments.has(t.id) ? ports[t.endpoint.instance].find(p => p.id === assignments.get(t.id)).connection : movement.shiftTerminal(t);
    const endpoints = [false, true].map(end => { const ts = end ? failed.endTerminals : failed.startTerminals; return ts.length === 1 ? terminalPosition(ts[0]) : movement.shiftEndpoint(failed, end); });
    const final = { ...witness, placements, transport_networks: witness.transport_networks.map(n => ({ ...n, terminals: n.terminals.map(t => ({ ...t, position: terminalPosition(t),
      endpoint: assignments.has(t.id) ? { ...t.endpoint, port: assignments.get(t.id) } : t.endpoint })) })) };
    const body = new Set(placements.flatMap(bodyCells).map(key));
    const { hard, soft } = terminalAccessObstacles(final, { shiftTerminal: t => t.position }, failed.transport, endpoints, body);
    representativeAccess = { id: failed.id, ...minimumTerminalReleasePath(...endpoints, hard, soft, extentOf(witness)) };
    const changes = neighborhood.candidates[record.assignment].portAssignments.map(a => {
      const t = witness.transport_networks.flatMap(n => n.terminals).find(t => t.id === a.terminal);
      return `${parent.labels[t.endpoint.instance] ?? t.endpoint.instance} ${t.endpoint.port}->${a.port}`;
    });
    const detail = { ...parent, move_diagnostic: { moves: request.moves, failed_endpoints: endpoints,
      code: `Cooperative ${scope} replay ${representative}; chain ${failed.id} failed. Proposed ports: ${changes.join(', ')}`,
      relaxed_routes: representativeAccess.status === 'found' ? [representativeAccess] : [] } };
    fs.writeFileSync(path.join(output, 'index.html'), render(detail, read(path.join(report.parent_directory, 'validation.json')), catalogs), { flag: 'wx' });
  }
  const result = { schema_version: 1, chain_schema_version: 2, factory_validated: false, scope, representative, representative_access: representativeAccess,
    status: records.some(r => r.candidate) ? 'unvalidated-replay-candidate' : 'bounded-cooperative-replay-exhausted',
    parent_directory: report.parent_directory, request, chain: chain.id, neighborhood, records, elapsed_ms: performance.now() - began,
    diagnostic_source_mismatches: mismatches, provenance: [options.report, parentPath, ...Object.values(paths)].map(p => ({ path: path.resolve(p), sha256: hash(p) })),
    source_hashes: Object.fromEntries([...new Set([...Object.keys(report.source_hashes), 'cooperative.mjs', 'cooperative-probe.mjs',
      '../constructive-factory-closure/model.mjs', '../constructive-placement-only/model.mjs'])].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), result);
  console.log(JSON.stringify({ status: result.status, assignments: neighborhood.candidates.length, trials: records.length, termination: neighborhood.termination, output }));
} catch (e) { console.error(JSON.stringify({ code: 'cooperative-probe-failed', message: e.message })); process.exitCode = 1; }
