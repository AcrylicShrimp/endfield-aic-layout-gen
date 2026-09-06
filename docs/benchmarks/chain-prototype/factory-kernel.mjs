import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { diagnostic } from './input.mjs';
import { checkFactoryState } from './factory-check.mjs';
import { factoryProposals } from './factory-geometry.mjs';
import { routeConnection, usedPorts } from './factory-routing.mjs';
import { pendingPortAccess } from './factory-access.mjs';
import { body, canonical, key, occupancy, placement, ports, rankedPorts } from './geometry.mjs';

export function createFactoryKernel(graph, validate, work, decisions, { proposalOrder = 'exhaustive', context = {}, familyAllowed = () => true } = {}) {
  const state = { deepest: null };
  const checked = async (witness, meta, complete = false) => {
    if (!work.take('validations')) return null;
    let result;
    try {
      result = await validate(structuredClone(witness), { ...context, ...meta }, Math.max(1, Math.floor(work.timeLeft())));
      if (!work.alive()) throw new Error('Validation returned after the shared deadline');
      if (result.status !== 0) throw new Error(`Independent validator process returned ${result.status}`);
      const coverage = checkFactoryState(graph, witness, result.report, { complete });
      return { accepted: true, artifact: result.artifact, coverage, score: witnessScore(witness),
        independent: { covered_geometry_validated: result.report.covered_geometry_validated, recipe_coverage_complete: result.report.recipe_coverage_complete,
          probe_factory_validated: result.report.factory_validated } };
    } catch (e) {
      return { accepted: false, artifact: result?.artifact ?? null,
        diagnostic: diagnostic(work.reason ? 'budget-exhausted' : e.code?.startsWith('chain-') ? e.code.slice(6) : 'validation-failed', 'factory-validation', e.message, e.entity ?? meta) };
    }
  };
  function* poses(w, parent, node, attach, used, allowed) {
    const sources = graph.policy.families.map(family => ({ family, iterator: factoryProposals(w, parent, graph.nodes.get(parent.instance), node, attach, family, graph.policy, used) }));
    if (proposalOrder === 'exhaustive') {
      for (const { family, iterator } of sources) while (allowed(family)) { const value = iterator.next(); if (value.done) break; yield { family, p: value.value }; }
      return;
    }
    let active = sources;
    while (active.length) {
      const next = [];
      for (const source of active.filter(s => allowed(s.family))) { const value = source.iterator.next(); if (!value.done) { next.push(source); yield { family: source.family, p: value.value }; } }
      active = next;
    }
  }
  async function* close(w, edges, parent, ordinals = []) {
    if (!edges.length) { yield { witness: w, ordinals }; return; }
    const edge = edges[0], used = usedPorts(w), endpoints = {};
    for (const [which, direction] of [['source', 'output'], ['target', 'input']]) {
      const owner = w.placements.find(p => p.instance === edge[which]);
      endpoints[which] = rankedPorts(ports(owner, graph.nodes.get(owner.instance)).filter(p => p.direction === direction && p.transport === edge.transport
        && !used.has(JSON.stringify([owner.instance, p.id]))), owner, graph.policy.ports_per_side);
    }
    const pairs = endpoints.source.flatMap(source => endpoints.target.map(target => ({ source, target,
      distance: Math.abs(source.connection.x - target.connection.x) + Math.abs(source.connection.y - target.connection.y) })));
    pairs.sort((a, b) => a.distance - b.distance || a.target.position.y - b.target.position.y || a.target.position.x - b.target.position.x
      || a.source.position.y - b.source.position.y || a.source.position.x - b.source.position.x);
    for (const pair of pairs) {
      if (!work.take('proposals')) return;
      const event = { ordinal: decisions.length, ...context, kind: 'closing-ports', parent_ordinal: parent.ordinal, requirement: edge.id, outcome: 'route-failed' }; decisions.push(event);
      const next = routeConnection(w, edge, pair, event, work);
      if (next) { event.outcome = 'routed-private'; yield* close(next, edges.slice(1), parent, [...ordinals, event.ordinal]); }
      if (work.reason) return;
    }
  }
  async function* candidates(base, group, index = 0, history = [], rootFamily = null) {
    if (index === group.members.length) { yield { ...base, history }; return; }
    const depth = base.witness.placements.length, id = group.members[index], node = graph.nodes.get(id), edge = graph.growthEdges[depth - 1];
    const attach = graph.lanes.find(e => e.kind === 'internal' && e.id === edge.id), parent = base.witness.placements.find(p => p.instance === edge.target);
    const members = new Set(base.witness.placements.map(p => p.instance)), used = usedPorts(base.witness), occupied = occupancy(base.witness);
    const closing = graph.lanes.filter(e => e.kind === 'internal' && e.id !== attach.id && ((e.source === id && members.has(e.target)) || (e.target === id && members.has(e.source))));
    for (const { family, p } of poses(base.witness, parent, node, attach, used, index === 0 ? familyAllowed : () => familyAllowed(rootFamily))) {
      if (!work.take('proposals')) return;
      const event = { ordinal: decisions.length, ...context, kind: 'placement', depth, facility: id, group: group.members, family, gap: p.gap, slide: p.slide,
        fold_side: p.fold_side ?? null, proposed_pose: p.pose, parent_artifact: base.artifact, introduced_edges: [attach.id, ...closing.map(e => e.id)], outcome: 'proposal-overlap' }; decisions.push(event);
      if (body(p.pose).some(c => occupied.has(key(c)))) {
        event.diagnostic = diagnostic('proposal-overlap', 'factory-placement', 'New body overlaps retained geometry', id, 'info'); continue;
      }
      const draft = structuredClone(base.witness); draft.placements.push(p.pose); draft.utilizations = { ...draft.utilizations, [id]: node.utilization };
      const attached = routeConnection(draft, attach, p, event, work);
      if (!attached) { event.outcome = work.reason ? 'budget-exhausted' : 'route-failed'; if (work.reason) return; continue; }
      event.outcome = 'closing-failed';
      for await (const closed of close(attached, closing, event)) {
        const witness = canonical(closed.witness), access = pendingPortAccess(graph, witness, work);
        (event.access_trials ??= []).push({ closing_ordinals: closed.ordinals, ...access });
        if (!access.accepted) {
          event.outcome = work.reason ? 'budget-exhausted' : 'pending-port-obstructed';
          event.diagnostic = diagnostic(work.reason ? 'budget-exhausted' : 'pending-port-obstructed', 'factory-access', 'Pending-port access heuristic rejected this candidate; not global infeasibility',
            access.demands?.filter(d => d.available < d.required) ?? access.status, 'info');
          if (work.reason) return; continue;
        }
        const validation = await checked(witness, { kind: 'private-growth', ordinal: event.ordinal,
          closing_ordinals: closed.ordinals, parent_artifact: base.artifact, depth, group: group.members, facility: id });
        event.validation = validation; (event.validation_trials ??= []).push({ closing_ordinals: closed.ordinals, ...validation });
        for (const ordinal of closed.ordinals) decisions[ordinal].validation = validation;
        if (!validation?.accepted) { event.outcome = work.reason ? 'budget-exhausted' : 'validation-failed'; if (work.reason) return; continue; }
        event.outcome = 'validated-private';
        const nextHistory = [...history, { depth, members: graph.order.slice(0, depth + 1), artifact: validation.artifact, selected_ordinal: event.ordinal, closing_ordinals: closed.ordinals }];
        if (!state.deepest || witness.placements.length > state.deepest.covered) state.deepest = { covered: witness.placements.length, artifact: validation.artifact, history: nextHistory, committed: false };
        yield* candidates({ witness, artifact: validation.artifact, validation }, group, index + 1, nextHistory, rootFamily ?? family);
        if (!familyAllowed(rootFamily ?? family)) break;
        event.outcome = 'continuation-failed'; if (work.reason) return;
      }
    }
  }
  const seed = async () => {
    const sink = graph.nodes.get(graph.order[0]), transport = graph.growthEdges[0].transport;
    const rotations = [...sink.facility_data.allowed_rotations].sort((a, b) => {
      const west = r => ports(placement(sink, r), sink).some(p => p.direction === 'input' && p.transport === transport && p.edge === 'west');
      return Number(west(b)) - Number(west(a)) || a - b;
    });
    const witness = { schema_version: 1, placements: [placement(sink, rotations[0])], utilizations: { [sink.id]: sink.utilization }, transport_networks: [], logistics_components: [] };
    const validation = await checked(witness, { kind: 'seed', depth: 0 });
    return { witness, artifact: validation?.artifact ?? null, validation };
  };
  return { state, checked, candidates, seed };
}

export function selectTransaction(candidate, decisions, outcome = 'selected-transaction-path') {
  for (const h of candidate.history) for (const ordinal of [h.selected_ordinal, ...h.closing_ordinals]) decisions[ordinal].outcome = outcome;
}
