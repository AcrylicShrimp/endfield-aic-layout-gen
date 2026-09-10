// Standalone migration oracle. Never imported by training or fresh generation.
import fs from 'node:fs';
import {shortestPath, key} from '../../docs/benchmarks/constructive-local-reembedding/path.mjs';
import {extractChains, reembed} from '../../docs/benchmarks/constructive-local-reembedding/model.mjs';
import {releaseBridgeDraft} from '../../docs/benchmarks/constructive-local-reembedding/components.mjs';
import {project} from '../../docs/benchmarks/constructive-factory-closure/model.mjs';
import {describe, execute} from './full_factory_executor.mjs';
const request = JSON.parse(fs.readFileSync(0, 'utf8'));
if (request.operation === 'paths') {
  const results = request.cases.map(c => shortestPath(c.start, c.end, new Set(c.blocked.map(key)), c.bounds,
    {...c.options, crossingOptions: new Map((c.crossings ?? []).map(s => [`${key(s.position)}:${s.direction}`, s]))}));
  process.stdout.write(JSON.stringify(results));
} else if (request.operation === 'vocabulary') {
  const catalogs = Object.fromEntries(Object.entries(request.catalogs).map(([k, p]) => [k, JSON.parse(fs.readFileSync(p, 'utf8'))]));
  const result = describe(request.witness, catalogs, request.options);
  delete result.seconds; delete result.poses.seconds; delete result.poses.translation.seconds;
  if (request.execute) result.outcomes = result.actions.map(a => {
    if (a.kind === 'restore_best') return null;
    const r = execute(request.witness, catalogs, a, request.work);
    return {status: r.status, witness: r.status === 'candidate' ? r.witness : null, route_expansions: r.route_expansions,
      codes: (r.diagnostics ?? []).map(d => d.code)};
  });
  process.stdout.write(JSON.stringify(result));
} else if (request.operation === 'geometry') {
  const catalogs = Object.fromEntries(Object.entries(request.catalogs).map(([k, p]) => [k, JSON.parse(fs.readFileSync(p, 'utf8'))]));
  const w = request.witness;
  const results = request.edits.map(edit => {
    try {
      const {witness, released} = releaseBridgeDraft(w, catalogs, edit.releaseBridges ?? []);
      const routePriority = extractChains(witness, released).chains.toSorted((a, b) => Number(a.transport !== 'pipe') - Number(b.transport !== 'pipe') || a.id - b.id).map(c => c.id);
      const r = reembed({witness: w, labels: {}}, catalogs, {...edit, routePriority, endpointPolicy: 'terminal-flexible',
        junctionPolicy: 'internal', maxNewCrossings: 2, workBudget: request.work, totalWorkBudget: request.work});
      return {status: r.status, witness: r.status === 'candidate' ? r.detail.witness : null, routes: r.routes,
        codes: (r.diagnostics ?? []).map(d => d.code)};
    } catch (e) { return {status: 'error', code: e.code, message: e.message}; }
  });
  process.stdout.write(JSON.stringify({chains: extractChains(w).chains, ports: project(w, catalogs), results}));
} else { throw new Error(`Unknown explicit oracle operation: ${request.operation}`); }
