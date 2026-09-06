import { shortestPath } from '../constructive-local-reembedding/path.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { checkState, diagnostic } from './input.mjs';
import { checkGroup } from './group-input.mjs';
import { Work } from './work.mjs';
import { body, canonical, key, occupancy, placement, ports, proposals, rankedPorts, routeBounds } from './geometry.mjs';

const usedPorts = w => new Set(w.transport_networks.flatMap(n => n.terminals.map(t => JSON.stringify([t.endpoint.instance, t.endpoint.port]))));
function pairs(w, graph, edge) {
  const used = usedPorts(w), endpoints = {};
  for (const [which, direction] of [['source', 'output'], ['target', 'input']]) {
    const owner = w.placements.find(p => p.instance === edge[which]);
    endpoints[which] = rankedPorts(ports(owner, graph.nodes.get(owner.instance)).filter(p => p.direction === direction && p.transport === edge.transport && !used.has(JSON.stringify([owner.instance, p.id]))), owner, graph.policy.ports_per_side);
  }
  // Order actual terminal positions, not their recipe IDs or input-array positions.
  return endpoints.source.flatMap(source => endpoints.target.map(target => ({ source, target }))).sort((a, b) => {
    const distance = p => Math.abs(p.source.connection.x - p.target.connection.x) + Math.abs(p.source.connection.y - p.target.connection.y);
    return distance(a) - distance(b) || a.target.position.y - b.target.position.y || a.target.position.x - b.target.position.x
      || a.source.position.y - b.source.position.y || a.source.position.x - b.source.position.x;
  });
}

export async function constructGroup(graph, validate, { now = () => performance.now() } = {}) {
  const work = new Work(graph.budget, now), decisions = [], seedTrials = [], diagnostics = [];
  let base = null, selected = null, deepest = null;
  const checked = async (witness, meta) => {
    if (!work.take('validations')) return null;
    let result;
    try {
      result = await validate(structuredClone(witness), meta, Math.max(1, Math.floor(work.timeLeft())));
      if (!work.alive()) throw new Error('Validation returned after the shared deadline');
      if (result.status !== 0) throw new Error(`Independent validator process returned ${result.status}`);
      (witness.placements.length === graph.nodes.size ? checkGroup : checkState)(graph, witness, result.report);
      return { accepted: true, artifact: result.artifact, score: witnessScore(witness) };
    } catch (e) {
      return { accepted: false, artifact: result?.artifact ?? null,
        diagnostic: diagnostic(work.reason ? 'budget-exhausted' : e.code?.startsWith('chain-') ? e.code.slice(6) : 'validation-failed', 'group-validation', e.message, e.entity ?? meta) };
    }
  };
  const route = (parent, edge, pair, event) => {
    if (!work.alive()) return null;
    const remaining = work.limits.route_expansions - work.used.route_expansions;
    if (remaining <= 0) { work.reason = 'route_expansions'; return null; }
    const w = structuredClone(parent);
    let n = w.transport_networks.find(n => n.item === edge.item && n.transport === edge.transport);
    if (!n) {
      n = { id: `network:${JSON.stringify([edge.transport, edge.item])}`, item: edge.item, transport: edge.transport,
        requirement_ids: [], cells: [], segments: [], terminals: [], component_ids: [] };
      w.transport_networks.push(n);
    }
    n.requirement_ids.push(edge.id);
    for (const [which, direction] of [['source', 'output'], ['target', 'input']]) n.terminals.push({
      id: `${edge.id}:${which}`, node: edge[which], direction, endpoint: { kind: 'facility', instance: edge[which], port: pair[which].id },
      position: pair[which].connection, rate: edge.rate,
    });
    const bounds = routeBounds(w, pair.source.connection, pair.target.connection, event.gap ?? 0);
    let result;
    try { result = shortestPath(pair.source.connection, pair.target.connection, occupancy(w, edge.transport), bounds, { workBudget: remaining, allowZeroLength: true }); }
    catch (e) { result = { status: e.code ?? 'route-error', expanded: 0, message: e.message }; }
    work.used.route_expansions += result.expanded;
    event.route = { requirement: edge.id, source_port: pair.source.id, target_port: pair.target.id, bounds,
      status: result.status, expanded: result.expanded, physical_cells: result.cells?.length ?? null, message: result.message ?? null };
    if (result.status !== 'found') {
      if (result.status === 'work-budget') work.reason = 'route_expansions';
      event.diagnostic = diagnostic(work.reason ? 'budget-exhausted' : 'route-failed', 'group-routing', `Local A* ended with ${result.status}; not global infeasibility`, edge.id, 'info');
      return null;
    }
    n.cells.push(...result.cells);
    for (let i = 1; i < result.cells.length; i++) n.segments.push({ from: result.cells[i - 1], to: result.cells[i], rate: edge.rate });
    return w;
  };
  async function* closeEdges(w, edges, parentEvent, ordinals = []) {
    if (!edges.length) { yield { witness: w, ordinals }; return; }
    const edge = edges[0], choices = pairs(w, graph, edge);
    if (!choices.length) parentEvent.diagnostic = diagnostic('no-port-proposal', 'group-ports', 'No unused pair for a closing edge', edge.id, 'info');
    for (const pair of choices) {
      if (!work.take('proposals')) return;
      const event = { ordinal: decisions.length, kind: 'closing-ports', parent_ordinal: parentEvent.ordinal,
        depth: parentEvent.depth, facility: parentEvent.facility, gap: parentEvent.gap, outcome: 'route-failed' };
      decisions.push(event);
      const next = route(w, edge, pair, event);
      if (next) { event.outcome = 'routed-private'; yield* closeEdges(next, edges.slice(1), parentEvent, [...ordinals, event.ordinal]); }
      if (work.reason) return;
    }
  }
  const grow = async (retained, history) => {
    const depth = retained.placements.length;
    if (depth === graph.nodes.size) return { witness: retained, artifact: history.at(-1).artifact, score: witnessScore(retained), history };
    const sourceNode = graph.nodes.get(graph.order[depth]), attach = graph.growthEdges[depth - 1];
    const targetNode = graph.nodes.get(attach.target), parent = retained.placements.find(p => p.instance === attach.target);
    const members = new Set(retained.placements.map(p => p.instance)), occupied = occupancy(retained), used = usedPorts(retained);
    const closing = graph.edges.filter(e => e.id !== attach.id && ((e.source === sourceNode.id && members.has(e.target)) || (e.target === sourceNode.id && members.has(e.source))));
    for (const family of graph.policy.families) for (const p of proposals(parent, targetNode, sourceNode, attach, family, graph.policy, used)) {
      if (!work.take('proposals')) return null;
      const event = { ordinal: decisions.length, kind: 'placement', depth, facility: sourceNode.id, parent_artifact: history.at(-1).artifact,
        family, gap: p.gap, fold_side: p.fold_side ?? null, proposed_pose: p.pose, introduced_edges: [attach.id, ...closing.map(e => e.id)], outcome: 'proposal-overlap' };
      decisions.push(event);
      if (body(p.pose).some(c => occupied.has(key(c)))) {
        event.diagnostic = diagnostic('proposal-overlap', 'group-placement', 'New body overlaps retained geometry', sourceNode.id, 'info'); continue;
      }
      const draft = structuredClone(retained); draft.placements.push(p.pose);
      draft.utilizations = { ...draft.utilizations, [sourceNode.id]: sourceNode.utilization };
      const attached = route(draft, attach, p, event);
      if (!attached) { event.outcome = work.reason ? 'budget-exhausted' : 'route-failed'; if (work.reason) return null; continue; }
      event.outcome = 'closing-failed';
      for await (const closed of closeEdges(attached, closing, event)) {
        const candidate = canonical(closed.witness), validation = await checked(candidate, { kind: 'private-growth', depth, facility: sourceNode.id, ordinal: event.ordinal, closing_ordinals: closed.ordinals, parent_artifact: history.at(-1).artifact });
        if (!validation) { event.outcome = 'budget-exhausted'; return null; }
        event.validation = validation;
        (event.validation_trials ??= []).push({ closing_ordinals: closed.ordinals, ...validation });
        for (const ordinal of closed.ordinals) {
          decisions[ordinal].validation = validation;
          decisions[ordinal].outcome = validation.accepted ? 'validated-private' : work.reason ? 'budget-exhausted' : 'validation-failed';
        }
        if (!validation.accepted) { event.outcome = work.reason ? 'budget-exhausted' : 'validation-failed'; continue; }
        event.outcome = 'validated-private';
        const nextHistory = [...history, { depth, members: graph.order.slice(0, depth + 1), artifact: validation.artifact,
          selected_ordinal: event.ordinal, score: validation.score, committed: false }];
        if (candidate.placements.length < graph.nodes.size && (!deepest || candidate.placements.length > deepest.covered)) deepest = { covered: candidate.placements.length, artifact: validation.artifact, committed: false, history: nextHistory };
        const complete = await grow(candidate, nextHistory);
        if (complete) { event.outcome = 'selected-group-path'; for (const ordinal of closed.ordinals) decisions[ordinal].outcome = 'selected-group-path'; return complete; }
        event.outcome = 'continuation-failed';
        if (work.reason) return null;
      }
    }
    return null;
  };
  const sink = graph.nodes.get(graph.order[0]), transport = graph.growthEdges[0].transport;
  const rotations = [...sink.facility_data.allowed_rotations].sort((a, b) => {
    const west = r => ports(placement(sink, r), sink).some(p => p.direction === 'input' && p.transport === transport && p.edge === 'west');
    return Number(west(b)) - Number(west(a)) || a - b;
  });
  for (const rotation of rotations) {
    const witness = { schema_version: 1, placements: [placement(sink, rotation)], utilizations: { [sink.id]: sink.utilization }, transport_networks: [], logistics_components: [] };
    const validation = await checked(witness, { kind: 'seed', depth: 0, rotation });
    seedTrials.push({ rotation, validation });
    if (!validation?.accepted) { if (validation?.diagnostic) diagnostics.push(validation.diagnostic); if (work.reason) break; continue; }
    const history = [{ depth: 0, members: [sink.id], artifact: validation.artifact, score: validation.score, committed: true }];
    base ??= { witness, artifact: validation.artifact, score: validation.score };
    selected = await grow(witness, history);
    if (selected) { base = { witness, artifact: validation.artifact, score: validation.score }; break; }
    if (work.reason) break;
  }
  if (!selected) diagnostics.push(diagnostic(work.reason ? 'budget-exhausted' : 'no-complete-group', 'group-growth',
    work.reason ? `Shared ${work.reason} budget exhausted; base retained` : 'No generated private continuation closed the group; not global infeasibility', { group: graph.group }, 'warning'));
  if (selected) selected.history.at(-1).committed = true;
  return { schema_version: 1, id: graph.id, status: selected ? 'internal-group-validated' : 'group-stopped', factory_validated: false,
    scope: 'Generated atomic cycle/branch group; external boundaries pending and bootstrap unverified', order: graph.order, cycle: graph.cycle,
    budget: work.report(), seed_trials: seedTrials, base, selected, deepest_private: deepest, decisions, diagnostics,
    commits: base ? [{ kind: 'seed', artifact: base.artifact, members: [sink.id] }, ...(selected ? [{ kind: 'closed-group', artifact: selected.artifact, members: graph.order }] : [])] : [] };
}
