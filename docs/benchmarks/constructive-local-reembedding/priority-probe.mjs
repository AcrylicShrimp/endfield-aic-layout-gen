import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { reembed, extractChains, extentOf } from './model.mjs';
import { movementPlan, bodyCells } from './movement.mjs';
import { boundaryPortSwaps, vacantPortCandidates } from './ports.mjs';
import { releaseBridgeDraft } from './components.mjs';
import { shortestPath, key } from './path.mjs';
import { minimumTerminalReleasePath, terminalAccessObstacles } from './terminal-access.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
try {
  const options = {}, args = process.argv.slice(2), keys = ['report', 'stage', 'catalog-directory', 'output-directory'];
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'max-new-crossings', 'port-policy'].includes(k) || k in options || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  if (keys.some(k => !(k in options))) throw new Error('Expected --report PATH --stage INDEX --catalog-directory PATH --output-directory NEW_PATH');
  const report = read(options.report), index = Number(options.stage), trial = report.stages[index];
  if (report.chain_schema_version !== 2) throw new Error('Historical chain IDs require the recorded implementation; expected chain schema 2');
  if (!Number.isSafeInteger(index) || index < 0 || !trial?.request?.moves?.length || trial.status !== 'rejected') throw new Error(`Expected a rejected schema-3 relocation at stage ${options.stage} of ${options.report}; schema-2 reports require explicit migration or their recorded commit`);
  const maxNewCrossings = Number(options['max-new-crossings'] ?? 0);
  if (!Number.isSafeInteger(maxNewCrossings) || maxNewCrossings < 0 || maxNewCrossings > 8) throw new Error('Expected --max-new-crossings 0..8');
  const portPolicy = options['port-policy'] ?? 'retained';
  if (!['retained', 'paired', 'vacant'].includes(portPolicy)) throw new Error('Expected --port-policy retained|paired|vacant');
  trial.request = { ...trial.request, maxNewCrossings };
  const parentPath = path.join(trial.parent_directory, 'detail.json'), parent = read(parentPath);
  const catalogPaths = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, path.join(options['catalog-directory'], (k === 'components' ? 'logistics-components' : k) + '.json')]));
  const catalogs = Object.fromEntries(Object.entries(catalogPaths).map(([k, p]) => [k, read(p)]));
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const { witness: routingWitness, released } = releaseBridgeDraft(parent.witness, catalogs, trial.request.releaseBridges ?? []);
  const { chains, anchors } = extractChains(routingWitness, released), records = [], started = performance.now();
  const attempt = (family, request) => {
    let r;
    try { r = reembed(parent, catalogs, request); }
    catch (e) { if (!e.code) throw e; r = { status: 'rejected', routes: [], diagnostics: [{ code: e.code, message: e.message, entity: e.entity }] }; }
    const record = { family, request, status: r.status, diagnostics: r.diagnostics, routes: r.routes, elapsed_ms: r.elapsed_ms };
    if (r.detail) { record.candidate = path.join(output, `candidate-${records.length}.json`); write(record.candidate, r.detail); }
    records.push(record); return r.routes?.find(p => p.status !== 'found');
  };
  for (const family of ['affected', 'belt', 'all']) for (const order of ['near', 'far']) attempt(family, { ...trial.request, order, routePriority: [],
    chainIds: family === 'affected' ? [] : chains.filter(c => family === 'all' || c.transport === 'belt').map(c => c.id) });
  const queue = ['near', 'far'].map(order => ({ ...trial.request, order, chainIds: chains.filter(c => c.transport === 'belt').map(c => c.id), routePriority: [] })), seen = new Set();
  for (let i = 0; i < queue.length && seen.size < 64; i++) {
    const request = queue[i], signature = JSON.stringify([request.order, request.routePriority]); if (seen.has(signature)) continue; seen.add(signature);
    const failed = attempt('failure-priority', request);
    if (failed) {
      const rest = request.routePriority.filter(id => id !== failed.id);
      queue.push({ ...request, routePriority: [failed.id, ...rest] });
      if (!request.routePriority.includes(failed.id)) queue.push({ ...request, routePriority: [...rest, failed.id] });
    }
  }
  if (portPolicy === 'paired') for (const portAssignments of boundaryPortSwaps(parent.witness, chains, trial.request.moves).slice(0, 4))
    for (const order of ['near', 'far']) attempt('paired-ports', { ...trial.request, chainIds: [], routePriority: [], portAssignments, order });
  const failedIds = [...new Set(records.slice(0, 2).flatMap(r => (r.routes ?? []).filter(p => p.status !== 'found').map(p => p.id)))];
  const vacant = portPolicy === 'vacant' ? vacantPortCandidates(parent.witness, chains, catalogs, trial.request.moves, { chainIds: failedIds }) : null;
  if (vacant) for (const p of vacant.candidates) for (const order of ['near', 'far'])
    attempt('vacant-ports', { ...trial.request, chainIds: [], routePriority: [], portAssignments: p.portAssignments, order });
  const rerouteChains = new Set(chains.filter(c => c.cells.some(p => released.has(`${c.transport}:${key(p)}`))).map(c => c.id));
  const movement = movementPlan(routingWitness, chains, anchors, trial.request.moves, { junctionPolicy: trial.request.junctionPolicy, rerouteChains });
  const body = new Set(parent.witness.placements.flatMap(p => bodyCells(movement.selected.has(p.instance) ? { ...p, ...movement.shiftFacility(p) } : p)).map(key));
  const bodyOnly = [], terminalAccess = [], bounds = extentOf(parent.witness);
  for (const id of new Set(records.filter(r => !r.request.portAssignments?.length).flatMap(r => r.routes.filter(p => p.status !== 'found').map(p => p.id)))) {
    const c = chains[id], start = movement.shiftEndpoint(c, false), end = movement.shiftEndpoint(c, true);
    const r = shortestPath(start, end, body, extentOf(parent.witness), { first: null, last: null, workBudget: 200000 });
    bodyOnly.push({ id, item: c.item, start, end, status: r.status, steps: r.steps, expanded: r.expanded });
    const { hard, soft } = terminalAccessObstacles(routingWitness, movement, c.transport, [start, end], body);
    terminalAccess.push({ id, item: c.item, transport: c.transport, start, end, bounds,
      ...minimumTerminalReleasePath(start, end, hard, soft, bounds) });
  }
  const failed = records[0].routes.find(p => p.status !== 'found');
  if (failed) {
    const c = chains[failed.id];
    const detail = { ...parent, move_diagnostic: { moves: trial.request.moves, code: `local-no-path; chain ${failed.id}`,
      failed_endpoints: [false, true].map(end => movement.shiftEndpoint(c, end)),
      relaxed_routes: terminalAccess.filter(r => r.id === failed.id && r.status === 'found') } };
    fs.writeFileSync(path.join(output, 'index.html'), render(detail, read(path.join(trial.parent_directory, 'validation.json')), catalogs), { flag: 'wx' });
  }
  const result = { schema_version: 1, chain_schema_version: 2, status: records.some(r => r.candidate) ? 'unvalidated-replay-candidate' : 'bounded-orders-exhausted', factory_validated: false,
    scope: 'Private replay; optional paired or vacant endpoint ports and new bridges. Body-only reachability uses the retained-port control and is not a valid layout.', port_policy: portPolicy, vacant_port_neighborhood: vacant,
    parent_directory: trial.parent_directory, stage: index, records, body_only: bodyOnly, terminal_access: terminalAccess,
    terminal_access_scope: 'Fixed retained canvas; bodies and other external terminals hard, facility terminal cells soft; wiring, components and arm constraints ignored. NOT valid routes.', priority_trials: seen.size, priority_limit: 64,
    priority_termination: queue.every(r => seen.has(JSON.stringify([r.order, r.routePriority]))) ? 'distinct-queue-exhausted' : 'trial-budget', elapsed_ms: performance.now() - started,
    provenance: [options.report, parentPath, ...Object.values(catalogPaths)].map(p => ({ path: path.resolve(p), sha256: hash(p) })),
    source_hashes: Object.fromEntries(['priority-probe.mjs', 'model.mjs', 'movement.mjs', 'terminal-identity.mjs', 'terminal-access.mjs', 'components.mjs', 'path.mjs', 'crossing.mjs', 'ports.mjs', 'fair.mjs', '../constructive-bound-interconnect/view.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), result);
  console.log(JSON.stringify({ status: result.status, trials: records.length, output }));
} catch (e) { console.error(JSON.stringify({ code: 'priority-probe-failed', message: e.message })); process.exitCode = 1; }
