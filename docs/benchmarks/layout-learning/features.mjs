import { ports, key, vectors } from '../chain-prototype/geometry.mjs';
import { selectedPorts, fail } from './environment.mjs';

// The sole model-input allowlist. Values are geometry or preprocessed connectivity, never outcomes.
export const tokenFields = ['facility', 'belt', 'pipe', 'x', 'y', 'width', 'height', 'cos_rotation', 'sin_rotation', 'movable', 'target', 'sink', 'input_ports', 'output_ports'];
export const actionFields = ['x', 'y', 'width', 'height', 'cos_rotation', 'sin_rotation', 'source_x', 'source_y', 'source_connection_x', 'source_connection_y', 'source_nx', 'source_ny',
  'target_x', 'target_y', 'target_connection_x', 'target_connection_y', 'target_nx', 'target_ny', 'belt', 'pipe', 'required_lanes'];
export function features(graph, base, current, edge, actions) {
  const target = base.placements.find(p => p.instance === edge.target), xy = p => [p.x - target.x, p.y - target.y];
  const tokens = [], indices = new Map(), links = [];
  for (const p of current.placements) {
    const ps = ports(p, graph.nodes.get(p.instance)); indices.set(p.instance, tokens.length);
    tokens.push([1, 0, 0, ...xy(p), p.width, p.height, Math.round(Math.cos(p.rotation * Math.PI / 180)), Math.round(Math.sin(p.rotation * Math.PI / 180)),
      Number(p.instance === edge.source), Number(p.instance === edge.target), Number(p.instance === graph.order[0]), ps.filter(p => p.direction === 'input').length, ps.filter(p => p.direction === 'output').length]);
  }
  for (const e of graph.edges) links.push([indices.get(e.source), indices.get(e.target)]);
  for (const n of current.transport_networks) {
    const cells = new Map();
    for (const p of n.cells) { cells.set(key(p), tokens.length); tokens.push([0, Number(n.transport === 'belt'), Number(n.transport === 'pipe'), ...xy(p), 1, 1, 0, 0, 0, 0, 0, 0, 0]); }
    for (const s of n.segments) links.push([cells.get(key(s.from)), cells.get(key(s.to))]);
    for (const t of n.terminals) links.push(t.direction === 'output' ? [indices.get(t.endpoint.instance), cells.get(key(t.position))] : [cells.get(key(t.position)), indices.get(t.endpoint.instance)]);
  }
  if (tokens.length > 256) fail('feature-bound', 'Token guard exceeded; no geometry cropping allowed', tokens.length);
  const adjacency = tokens.map(() => tokens.map(() => 0));
  for (const [a, b] of links) { if (a === undefined || b === undefined) fail('feature-invalid', 'Connectivity references missing geometry'); adjacency[a][b] = 1; }
  const proposed = actions.map(action => {
    const p = action.pose, selected = selectedPorts(graph, base, edge, action);
    return [...xy(p), p.width, p.height, Math.round(Math.cos(p.rotation * Math.PI / 180)), Math.round(Math.sin(p.rotation * Math.PI / 180)),
      ...xy(selected.source.position), ...xy(selected.source.connection), ...vectors[selected.source.edge], ...xy(selected.target.position), ...xy(selected.target.connection), ...vectors[selected.target.edge],
      Number(edge.transport === 'belt'), Number(edge.transport === 'pipe'), 1];
  });
  return { tokens, adjacency, actions: proposed };
}
export function cheapChoice(base, actions, actionFeatures) {
  const tuples = actions.map((a, i) => {
    const ps = [...base.placements, a.pose], cells = base.transport_networks.flatMap(n => n.cells);
    const minX = Math.min(...ps.map(p => p.x), ...cells.map(p => p.x)), minY = Math.min(...ps.map(p => p.y), ...cells.map(p => p.y));
    const maxX = Math.max(...ps.map(p => p.x + p.width - 1), ...cells.map(p => p.x)), maxY = Math.max(...ps.map(p => p.y + p.height - 1), ...cells.map(p => p.y));
    const f = actionFeatures[i]; return [(maxX - minX + 1) * (maxY - minY + 1), Math.abs(f[8] - f[14]) + Math.abs(f[9] - f[15]), i];
  });
  return tuples.sort((a, b) => a[0] - b[0] || a[1] - b[1] || a[2] - b[2])[0][2];
}
