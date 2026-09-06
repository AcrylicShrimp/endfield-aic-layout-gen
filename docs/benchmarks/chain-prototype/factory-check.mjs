import { add, compare, parseRate, rate } from '../constructive-interconnect/rates.mjs';
import { diagnostic, marginKey, positive } from './input.mjs';
import { externalNode, terminalId } from './factory-input.mjs';

// Independent of construction decisions: only the original graph, witness and Rust report.
export function checkFactoryState(graph, w, validation, { complete = false } = {}) {
  const error = (message, entity = null) => { throw Object.assign(new Error(message), diagnostic('factory-coverage-invalid', 'factory-validation', message, entity)); };
  if (!validation?.covered_geometry_validated || !Array.isArray(validation.recipe_terminals)) error('Independent geometry validation did not pass');
  const members = new Set(w.placements.map(p => p.instance));
  if (members.size !== w.placements.length || !graph.order.slice(0, members.size).every(id => members.has(id))) error('Unexpected growth inventory');
  for (const p of w.placements) {
    const node = graph.nodes.get(p.instance);
    if (!node || node.recipe !== p.recipe || node.facility !== p.facility || compare(positive(w.utilizations[p.instance], p.instance), parseRate(node.utilization))) error('Changed facility recipe or utilization', p.instance);
  }
  const required = graph.lanes.filter(e => e.kind === 'external' || (members.has(e.source) && members.has(e.target)));
  const descriptors = new Map(required.flatMap(e => ['source', 'target'].map(which => [terminalId(e, which), { e, which }])));
  const seen = new Set(), completed = new Map(), routed = new Map();
  const addFlow = (instance, direction, item, value) => { const k = marginKey(instance, direction, item); routed.set(k, add(routed.get(k) ?? rate(0), value)); };
  for (const net of w.transport_networks) {
    const usedRequirements = new Set();
    for (const t of net.terminals) {
      const descriptor = descriptors.get(t.id);
      if (!descriptor || seen.has(t.id)) error('Unexpected or duplicate terminal', t.id);
      const { e, which } = descriptor, direction = which === 'source' ? 'output' : 'input';
      if (e.item !== net.item || e.transport !== net.transport || !net.requirement_ids.includes(e.id) || t.node !== e[which]
        || t.direction !== direction || compare(positive(t.rate, t.id), parseRate(e.rate))) error('Changed requirement terminal', t.id);
      const facility = e.kind === 'internal' || (e.direction === 'input' ? which === 'target' : which === 'source');
      if (facility) {
        if (t.endpoint.kind !== 'facility' || t.endpoint.instance !== e[which] || !members.has(e[which])) error('Changed facility endpoint', t.id);
        addFlow(e[which], direction, e.item, parseRate(e.rate));
      } else if (t.endpoint.kind !== 'external' || t.endpoint.node !== externalNode(e.id) || !['north', 'east', 'south', 'west'].includes(t.endpoint.side)) error('Changed external identity or side', t.id);
      seen.add(t.id); usedRequirements.add(e.id);
      const key = terminalId(e, 'lane'); completed.set(key, (completed.get(key) ?? 0) + 1);
    }
    if (new Set(net.requirement_ids).size !== net.requirement_ids.length || net.requirement_ids.length !== usedRequirements.size) error('Unexpected requirement identities', net.id);
  }
  for (const [key, count] of completed) if (count !== 2) error('Incomplete physical lane terminal pair', key);
  for (const e of required.filter(e => e.kind === 'internal')) if (!completed.has(terminalId(e, 'lane'))) error('Missing induced internal edge', e.id);
  const external = required.filter(e => e.kind === 'external'), done = external.filter(e => completed.has(terminalId(e, 'lane')));
  if (done.length && members.size !== graph.nodes.size) error('External realization started before internal inventory completion');
  const ledgerSeen = new Set();
  for (const t of validation.recipe_terminals) {
    const k = marginKey(t.instance, t.direction, t.item), expected = graph.expected.get(k), value = routed.get(k) ?? rate(0);
    if (!members.has(t.instance) || !expected || ledgerSeen.has(k) || compare(parseRate(t.required_rate), expected)
      || compare(parseRate(t.routed_rate), value) || compare(add(parseRate(t.pending_rate), value), expected)) error('Independent recipe ledger differs from original request', k);
    ledgerSeen.add(k);
  }
  if (ledgerSeen.size !== [...graph.expected.keys()].filter(k => members.has(JSON.parse(k)[0])).length) error('Incomplete independent recipe ledger');
  const factory = members.size === graph.nodes.size && done.length === external.length && validation.recipe_coverage_complete === true
    && validation.recipe_terminals.every(t => t.pending_rate.numerator === 0);
  if (complete && !factory) error('Factory is not complete', { covered: members.size, external_lanes: done.length, required_external_lanes: external.length });
  return { accepted: true, factory_validated: factory, covered: members.size, internal_complete: members.size === graph.nodes.size,
    output_closed: graph.edges.every(e => !members.has(e.source) || members.has(e.target)),
    external_lanes: { required: external.length, completed: done.length }, pending_external: external.filter(e => !completed.has(terminalId(e, 'lane'))).map(e => ({ id: e.id, lane: e.lane, rate: e.rate })) };
}
