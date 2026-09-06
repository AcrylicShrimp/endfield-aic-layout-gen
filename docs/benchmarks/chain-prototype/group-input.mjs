import { parseGraph, fail, checkState, diagnostic } from './input.mjs';

export function prepareGroup(input, catalogs) {
  const graph = parseGraph(input, catalogs, { minimumGap: 0, allowedFamilies: ['straight', 'left', 'right', 'fold'] }), { nodes, edges, incoming, outgoing } = graph;
  const reject = () => fail('unsupported-topology', 'Expected one reciprocal two-node cycle with one branch into a simple downstream chain');
  const sinks = [...nodes.keys()].filter(id => !outgoing.get(id).length);
  if (nodes.size < 3 || edges.length !== nodes.size || sinks.length !== 1 || [...nodes.keys()].some(id => incoming.get(id).length !== 1 || outgoing.get(id).length > 2)) reject();
  const order = [], growthEdges = []; let current = sinks[0];
  while (!order.includes(current)) {
    order.push(current);
    const edge = incoming.get(current)[0];
    if (!order.includes(edge.source)) growthEdges.push(edge);
    current = edge.source;
  }
  if (order.length !== nodes.size || current !== order.at(-2)) reject();
  const cycle = order.slice(-2), branch = edges.filter(e => cycle.includes(e.source) && !cycle.includes(e.target));
  if (branch.length !== 1 || branch[0].source !== cycle[0]) reject();
  if (new Set(edges.map(e => e.transport)).size !== 1) fail('unsupported-group-transport', 'Milestone 2 supports a single transport kind per cycle/branch group');
  return { ...graph, order, growthEdges, cycle, group: order.slice(1) };
}

export function checkGroup(graph, witness, validation) {
  checkState(graph, witness, validation);
  if (witness.placements.length !== graph.nodes.size) throw Object.assign(new Error('The private dependency group is incomplete'),
    diagnostic('group-not-closed', 'group-validation', 'The private dependency group is incomplete', graph.group));
  return true;
}
