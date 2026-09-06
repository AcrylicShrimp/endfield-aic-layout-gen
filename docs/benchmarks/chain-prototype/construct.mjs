import { shortestPath } from '../constructive-local-reembedding/path.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { checkState, diagnostic } from './input.mjs';
import { body, canonical, key, occupancy, placement, ports, proposals, routeBounds } from './geometry.mjs';
import { Work } from './work.mjs';
const better = (a, b) => a.some((v, i) => v !== b[i] && a.slice(0, i).every((x, j) => x === b[j]) && v < b[i]);
function networkFor(w, edge, proposal) {
  let n = w.transport_networks.find(n => n.item === edge.item && n.transport === edge.transport);
  if (!n) { n = { id: `network:${JSON.stringify([edge.transport, edge.item])}`, item: edge.item, transport: edge.transport,
    requirement_ids: [], cells: [], segments: [], terminals: [], component_ids: [] }; w.transport_networks.push(n); }
  n.requirement_ids.push(edge.id);
  for (const [which, direction] of [['source', 'output'], ['target', 'input']]) {
    const p = proposal[which];
    n.terminals.push({ id: `${edge.id}:${which}`, node: edge[which], direction,
      endpoint: { kind: 'facility', instance: edge[which], port: p.id }, position: p.connection, rate: edge.rate });
  }
  return n;
}
export async function construct(graph, validate, { now = () => performance.now() } = {}) {
  const work = new Work(graph.budget, now), decisions = [], families = [], diagnostics = [];
  let selected = null;
  const checked = async (witness, meta) => {
    if (!work.take('validations')) return null;
    let result;
    try {
      result = await validate(structuredClone(witness), meta, Math.max(1, Math.floor(work.timeLeft())));
      if (!work.alive()) return { accepted: false, artifact: result.artifact, diagnostic: diagnostic('budget-exhausted', 'chain-validation', 'Validation returned after the shared deadline', meta) };
      if (result.status !== 0) throw new Error(`Independent validator process returned ${result.status}`);
      checkState(graph, witness, result.report);
      return { accepted: true, artifact: result.artifact, score: witnessScore(witness) };
    } catch (e) {
      return { accepted: false, artifact: result?.artifact ?? null, diagnostic: e.code?.startsWith('chain-') ? { code: e.code, severity: 'error', stage: e.stage, message: e.message, entity: e.entity } :
        diagnostic('validation-failed', 'chain-validation', e.message, meta) };
    }
  };
  const sink = graph.nodes.get(graph.order[0]), firstEdge = graph.growthEdges[0];
  const seedRotations = [...sink.facility_data.allowed_rotations].sort((a, b) => {
    const west = r => ports(placement(sink, r), sink).some(p => p.direction === 'input' && p.transport === firstEdge.transport && p.edge === 'west');
    return Number(west(b)) - Number(west(a)) || a - b;
  });
  const seed = { schema_version: 1, placements: [placement(sink, seedRotations[0])], utilizations: { [sink.id]: sink.utilization }, transport_networks: [], logistics_components: [] };
  const seedCheck = await checked(seed, { family: null, depth: 0, facility: sink.id });
  if (!seedCheck?.accepted) diagnostics.push(seedCheck?.diagnostic ?? diagnostic('budget-exhausted', 'chain-seed', 'Seed validation could not run', work.reason));
  else for (const family of graph.policy.families) {
    let retained = structuredClone(seed), artifact = seedCheck.artifact;
    const history = [{ depth: 0, members: [sink.id], artifact, score: seedCheck.score }];
    let stopped = null;
    for (let depth = 1; depth < graph.order.length; depth++) {
      const sourceNode = graph.nodes.get(graph.order[depth]), targetNode = graph.nodes.get(graph.order[depth - 1]), edge = graph.growthEdges[depth - 1];
      const parent = retained.placements.find(p => p.instance === targetNode.id), occupied = occupancy(retained);
      const used = new Set(retained.transport_networks.flatMap(n => n.terminals.map(t => JSON.stringify([t.endpoint.instance, t.endpoint.port]))));
      let best = null, proposed = 0;
      for (const p of proposals(parent, targetNode, sourceNode, edge, family, graph.policy, used)) {
        if (!work.take('proposals')) break;
        proposed++;
        const event = { ordinal: decisions.length, family, depth, facility: sourceNode.id, requirement: edge.id, gap: p.gap,
          proposed_pose: p.pose, source_port: p.source.id, target_port: p.target.id, outcome: null };
        decisions.push(event);
        if (body(p.pose).some(cell => occupied.has(key(cell)))) {
          event.outcome = 'proposal-overlap'; event.diagnostic = diagnostic('proposal-overlap', 'chain-placement', 'New body overlaps retained geometry', sourceNode.id, 'info'); continue;
        }
        const draft = structuredClone(retained); draft.placements.push(p.pose);
        draft.utilizations = { ...draft.utilizations, [sourceNode.id]: sourceNode.utilization };
        // Extend the logical commodity network before embedding this new requirement.
        const net = networkFor(draft, edge, p), bounds = routeBounds(draft, p.source.connection, p.target.connection, p.gap);
        event.route_bounds = bounds;
        const remaining = work.limits.route_expansions - work.used.route_expansions;
        if (remaining <= 0 || !work.alive()) {
          work.reason ??= 'route_expansions'; event.outcome = 'budget-exhausted';
          event.diagnostic = diagnostic('budget-exhausted', 'chain-routing', 'Shared work budget prevents another route', work.reason, 'warning'); break;
        }
        let path;
        try { path = shortestPath(p.source.connection, p.target.connection, occupancy(draft, edge.transport), bounds, { workBudget: remaining, allowZeroLength: true }); }
        catch (e) { path = { status: e.code ?? 'route-error', expanded: 0, message: e.message }; }
        work.used.route_expansions += path.expanded;
        event.route = { status: path.status, expanded: path.expanded, physical_cells: path.cells?.length ?? null, turns: path.turns ?? null, message: path.message ?? null };
        if (path.status !== 'found') {
          event.outcome = path.status === 'work-budget' ? 'budget-exhausted' : 'route-failed';
          event.diagnostic = diagnostic(event.outcome, 'chain-routing', `Local A* ended with ${path.status}; not global infeasibility`, edge.id, 'info');
          if (path.status === 'work-budget') { work.reason = 'route_expansions'; break; } continue;
        }
        net.cells.push(...path.cells);
        for (let i = 1; i < path.cells.length; i++) net.segments.push({ from: path.cells[i - 1], to: path.cells[i], rate: edge.rate });
        const candidate = canonical(draft), validation = await checked(candidate, { family, depth, facility: sourceNode.id, ordinal: event.ordinal });
        if (!validation) { event.outcome = 'budget-exhausted'; event.diagnostic = diagnostic('budget-exhausted', 'chain-validation', 'Shared budget prevents another validation', work.reason, 'warning'); break; }
        event.validation = validation;
        if (!validation.accepted) { event.outcome = validation.diagnostic?.code === 'chain-budget-exhausted' ? 'budget-exhausted' : 'validation-failed'; continue; }
        event.score = validation.score; event.outcome = 'validated-but-not-selected';
        if (!best || better(validation.score.tuple, best.validation.score.tuple)) best = { witness: candidate, validation, event };
      }
      if (!best) {
        stopped = work.reason ? 'budget-exhausted' : proposed ? 'no-valid-proposal' : 'no-port-proposal';
        diagnostics.push(diagnostic(work.reason ? 'budget-exhausted' : 'no-complete-family', 'chain-growth', stopped, { family, depth, facility: sourceNode.id, bound: work.reason }, 'warning'));
        break;
      }
      best.event.outcome = 'selected'; retained = best.witness; artifact = best.validation.artifact;
      best.event.committed_pose = retained.placements.find(p => p.instance === sourceNode.id);
      history.push({ depth, members: graph.order.slice(0, depth + 1), artifact, score: best.validation.score, selected_ordinal: best.event.ordinal });
    }
    const complete = retained.placements.length === graph.order.length;
    const result = { family, status: complete ? 'internal-chain-validated' : 'chain-stopped', complete_internal: complete, stopped,
      covered: retained.placements.length, artifact, score: complete ? witnessScore(retained) : null, history };
    families.push(result);
    if (complete && (!selected || better(result.score.tuple, selected.score.tuple))) selected = { family, artifact, score: result.score, witness: retained };
    if (work.reason) break;
  }
  return { schema_version: 1, id: graph.id, status: selected ? 'internal-chain-validated' : 'chain-stopped', factory_validated: false,
    scope: 'Generated simple chain; declared external obligations are pending, not a complete factory',
    order: graph.order, unattempted_families: graph.policy.families.filter(f => !families.some(r => r.family === f)),
    budget: work.report(), seed: seedCheck, families, decisions, selected, diagnostics };
}
