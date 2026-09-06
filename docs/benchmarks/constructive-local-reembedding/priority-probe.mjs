import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { reembed, extractChains, extentOf } from './model.mjs';
import { movementPlan, bodyCells } from './movement.mjs';
import { boundaryPortSwaps } from './ports.mjs';
import { shortestPath, key } from './path.mjs';
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
  if (!Number.isSafeInteger(index) || index < 0 || !trial?.request?.moves?.length || trial.status !== 'rejected') throw new Error(`Expected a rejected schema-3 relocation at stage ${options.stage} of ${options.report}; schema-2 reports require explicit migration or their recorded commit`);
  const maxNewCrossings = Number(options['max-new-crossings'] ?? 0);
  if (!Number.isSafeInteger(maxNewCrossings) || maxNewCrossings < 0 || maxNewCrossings > 8) throw new Error('Expected --max-new-crossings 0..8');
  const portPolicy = options['port-policy'] ?? 'retained';
  if (!['retained', 'paired'].includes(portPolicy)) throw new Error('Expected --port-policy retained|paired');
  trial.request = { ...trial.request, maxNewCrossings };
  const parentPath = path.join(trial.parent_directory, 'detail.json'), parent = read(parentPath);
  const catalogPaths = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, path.join(options['catalog-directory'], (k === 'components' ? 'logistics-components' : k) + '.json')]));
  const catalogs = Object.fromEntries(Object.entries(catalogPaths).map(([k, p]) => [k, read(p)]));
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const { chains, anchors } = extractChains(parent.witness), records = [], started = performance.now();
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
  const movement = movementPlan(parent.witness, chains, anchors, trial.request.moves, { junctionPolicy: trial.request.junctionPolicy });
  const relocated = (tr, p) => movement.shiftAnchor(tr, p);
  const body = new Set(parent.witness.placements.flatMap(p => bodyCells(movement.selected.has(p.instance) ? { ...p, ...movement.shiftFacility(p) } : p)).map(key));
  const bodyOnly = [];
  for (const id of new Set(records.filter(r => !r.request.portAssignments?.length).flatMap(r => r.routes.filter(p => p.status !== 'found').map(p => p.id)))) {
    const c = chains[id], start = relocated(c.transport, c.cells[0]), end = relocated(c.transport, c.cells.at(-1));
    const r = shortestPath(start, end, body, extentOf(parent.witness), { first: null, last: null, workBudget: 200000 });
    bodyOnly.push({ id, item: c.item, start, end, status: r.status, steps: r.steps, expanded: r.expanded });
  }
  const failed = records[0].routes.find(p => p.status !== 'found');
  if (failed) {
    const c = chains[failed.id];
    const detail = { ...parent, move_diagnostic: { moves: trial.request.moves, code: `local-no-path; chain ${failed.id}`,
      failed_endpoints: [c.cells[0], c.cells.at(-1)].map(p => relocated(c.transport, p)) } };
    fs.writeFileSync(path.join(output, 'index.html'), render(detail, read(path.join(trial.parent_directory, 'validation.json')), catalogs), { flag: 'wx' });
  }
  const result = { schema_version: 1, status: records.some(r => r.candidate) ? 'unvalidated-replay-candidate' : 'bounded-orders-exhausted', factory_validated: false,
    scope: 'Private replay; optional paired port assignments and new bridges. Body-only reachability uses the retained-port control and is not a valid layout.', port_policy: portPolicy,
    parent_directory: trial.parent_directory, stage: index, records, body_only: bodyOnly, priority_trials: seen.size, priority_limit: 64,
    priority_termination: queue.every(r => seen.has(JSON.stringify([r.order, r.routePriority]))) ? 'distinct-queue-exhausted' : 'trial-budget', elapsed_ms: performance.now() - started,
    provenance: [options.report, parentPath, ...Object.values(catalogPaths)].map(p => ({ path: path.resolve(p), sha256: hash(p) })),
    source_hashes: Object.fromEntries(['priority-probe.mjs', 'model.mjs', 'movement.mjs', 'path.mjs', 'crossing.mjs', 'ports.mjs', '../constructive-bound-interconnect/view.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), result);
  console.log(JSON.stringify({ status: result.status, trials: records.length, output }));
} catch (e) { console.error(JSON.stringify({ code: 'priority-probe-failed', message: e.message })); process.exitCode = 1; }
