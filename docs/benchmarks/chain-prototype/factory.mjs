import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { Work } from './work.mjs';
import { diagnostic } from './input.mjs';
import { checkFactoryState } from './factory-check.mjs';
import { factoryProposals } from './factory-geometry.mjs';
import { routeConnection, usedPorts } from './factory-routing.mjs';
import { realizeBoundary } from './factory-boundary.mjs';
import { pendingPortAccess } from './factory-access.mjs';
import { body, canonical, key, occupancy, placement, ports, rankedPorts } from './geometry.mjs';

export async function constructFactory(graph, validate, { now = () => performance.now() } = {}) {
  const work = new Work(graph.budget, now), decisions = [], commits = [], diagnostics = [];
  let retained = null, deepest = null, boundary = null;
  const checked = async (witness, meta, complete = false) => {
    if (!work.take('validations')) return null;
    let result;
    try {
      result = await validate(structuredClone(witness), meta, Math.max(1, Math.floor(work.timeLeft())));
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
      const event = { ordinal: decisions.length, kind: 'closing-ports', parent_ordinal: parent.ordinal, requirement: edge.id, outcome: 'route-failed' }; decisions.push(event);
      const next = routeConnection(w, edge, pair, event, work);
      if (next) { event.outcome = 'routed-private'; yield* close(next, edges.slice(1), parent, [...ordinals, event.ordinal]); }
      if (work.reason) return;
    }
  }
  const transaction = async (base, group, index = 0, history = []) => {
    if (index === group.members.length) return { ...base, history };
    const depth = base.witness.placements.length, id = group.members[index], node = graph.nodes.get(id), edge = graph.growthEdges[depth - 1];
    const attach = graph.lanes.find(e => e.kind === 'internal' && e.id === edge.id), parent = base.witness.placements.find(p => p.instance === edge.target);
    const members = new Set(base.witness.placements.map(p => p.instance)), used = usedPorts(base.witness), occupied = occupancy(base.witness);
    const closing = graph.lanes.filter(e => e.kind === 'internal' && e.id !== attach.id && ((e.source === id && members.has(e.target)) || (e.target === id && members.has(e.source))));
    for (const family of graph.policy.families) for (const p of factoryProposals(base.witness, parent, graph.nodes.get(parent.instance), node, attach, family, graph.policy, used)) {
      if (!work.take('proposals')) return null;
      const event = { ordinal: decisions.length, kind: 'placement', depth, facility: id, group: group.members, family, gap: p.gap, slide: p.slide,
        fold_side: p.fold_side ?? null, proposed_pose: p.pose, parent_artifact: base.artifact, introduced_edges: [attach.id, ...closing.map(e => e.id)], outcome: 'proposal-overlap' }; decisions.push(event);
      if (body(p.pose).some(c => occupied.has(key(c)))) {
        event.diagnostic = diagnostic('proposal-overlap', 'factory-placement', 'New body overlaps retained geometry', id, 'info'); continue;
      }
      const draft = structuredClone(base.witness); draft.placements.push(p.pose); draft.utilizations = { ...draft.utilizations, [id]: node.utilization };
      const attached = routeConnection(draft, attach, p, event, work);
      if (!attached) { event.outcome = work.reason ? 'budget-exhausted' : 'route-failed'; if (work.reason) return null; continue; }
      event.outcome = 'closing-failed';
      for await (const closed of close(attached, closing, event)) {
        const witness = canonical(closed.witness), access = pendingPortAccess(graph, witness, work);
        (event.access_trials ??= []).push({ closing_ordinals: closed.ordinals, ...access });
        if (!access.accepted) {
          event.outcome = work.reason ? 'budget-exhausted' : 'pending-port-obstructed';
          event.diagnostic = diagnostic(work.reason ? 'budget-exhausted' : 'pending-port-obstructed', 'factory-access', 'Pending-port access heuristic rejected this candidate; not global infeasibility',
            access.demands?.filter(d => d.available < d.required) ?? access.status, 'info');
          if (work.reason) return null; continue;
        }
        const validation = await checked(witness, { kind: 'private-growth', ordinal: event.ordinal,
          closing_ordinals: closed.ordinals, parent_artifact: base.artifact, depth, group: group.members, facility: id });
        event.validation = validation; (event.validation_trials ??= []).push({ closing_ordinals: closed.ordinals, ...validation });
        for (const ordinal of closed.ordinals) decisions[ordinal].validation = validation;
        if (!validation?.accepted) { event.outcome = work.reason ? 'budget-exhausted' : 'validation-failed'; if (work.reason) return null; continue; }
        event.outcome = 'validated-private';
        const nextHistory = [...history, { depth, members: graph.order.slice(0, depth + 1), artifact: validation.artifact, selected_ordinal: event.ordinal }];
        if (!deepest || witness.placements.length > deepest.covered) deepest = { covered: witness.placements.length, artifact: validation.artifact, history: nextHistory, committed: false };
        const result = await transaction({ witness, artifact: validation.artifact, validation }, group, index + 1, nextHistory);
        if (result) { event.outcome = 'selected-transaction-path'; for (const ordinal of closed.ordinals) decisions[ordinal].outcome = 'selected-transaction-path'; return result; }
        event.outcome = 'continuation-failed'; if (work.reason) return null;
      }
    }
    return null;
  };
  const sink = graph.nodes.get(graph.order[0]), transport = graph.growthEdges[0].transport;
  const rotations = [...sink.facility_data.allowed_rotations].sort((a, b) => {
    const west = r => ports(placement(sink, r), sink).some(p => p.direction === 'input' && p.transport === transport && p.edge === 'west');
    return Number(west(b)) - Number(west(a)) || a - b;
  });
  const seed = { schema_version: 1, placements: [placement(sink, rotations[0])], utilizations: { [sink.id]: sink.utilization }, transport_networks: [], logistics_components: [] };
  const seedValidation = await checked(seed, { kind: 'seed', depth: 0 });
  if (seedValidation?.accepted) {
    retained = { witness: seed, artifact: seedValidation.artifact, validation: seedValidation };
    commits.push({ kind: 'seed', members: [sink.id], artifact: retained.artifact, history: [] });
    for (const group of graph.transactions.slice(1)) {
      const next = await transaction(retained, group);
      if (!next || !next.validation.coverage.output_closed) {
        diagnostics.push(diagnostic(work.reason ? 'budget-exhausted' : 'no-complete-transaction', 'factory-growth', 'Current supplier transaction did not close; retained earlier committed geometry', group.members, 'warning')); break;
      }
      retained = next;
      commits.push({ kind: 'supplier-component', members: retained.witness.placements.map(p => p.instance), added: group.members, artifact: retained.artifact, history: next.history });
      if (deepest?.artifact === retained.artifact) deepest = null;
    }
    if (retained.witness.placements.length === graph.nodes.size) boundary = await realizeBoundary(graph, retained, work, checked, decisions);
  } else diagnostics.push(seedValidation?.diagnostic ?? diagnostic('budget-exhausted', 'factory-seed', 'Seed could not be validated', work.reason));
  const selected = boundary?.selected ?? null;
  if (boundary && !selected) diagnostics.push(diagnostic(work.reason ? 'budget-exhausted' : 'boundary-not-closed', 'factory-boundary', 'External routing transaction failed; internal layout retained', null, 'warning'));
  if (selected) commits.push({ kind: 'external-closure', members: graph.order, artifact: selected.artifact, history: selected.history });
  return { schema_version: 1, id: graph.id, status: selected ? 'factory-validated' : 'factory-stopped', factory_validated: !!selected,
    budget: work.report(), order: graph.order, transactions: graph.transactions, commits, retained, deepest_private: deepest,
    boundary_trials: boundary?.trials ?? [], decisions, diagnostics, selected };
}
