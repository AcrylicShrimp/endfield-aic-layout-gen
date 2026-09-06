import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { ports, key } from './geometry.mjs';

export const compareTuple = (a, b) => { for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] - b[i]; return 0; };
export function completionScore(graph, w) {
  const score = witnessScore(w); let turns = 0;
  for (const n of w.transport_networks) {
    const incidence = new Map(n.cells.map(c => [key(c), { incoming: [], outgoing: [] }]));
    const segment = (from, to) => {
      const direction = [to.x - from.x, to.y - from.y];
      incidence.get(key(from))?.outgoing.push(direction); incidence.get(key(to))?.incoming.push(direction);
    };
    for (const s of n.segments) segment(s.from, s.to);
    for (const t of n.terminals.filter(t => t.endpoint.kind === 'facility')) {
      const p = w.placements.find(p => p.instance === t.endpoint.instance), port = ports(p, graph.nodes.get(p.instance)).find(q => q.id === t.endpoint.port);
      if (!port) throw new Error(`Cannot score unknown port ${t.endpoint.port}`);
      if (t.direction === 'output') segment(port.position, t.position); else segment(t.position, port.position);
    }
    for (const cell of incidence.values()) {
      if (cell.incoming.length > 1 || cell.outgoing.length > 1) throw new Error('Chain turn metric does not support physical junctions');
      if (cell.incoming.length && cell.outgoing.length && cell.incoming[0][0] * cell.outgoing[0][0] + cell.incoming[0][1] * cell.outgoing[0][1] === 0) turns++;
    }
  }
  return { ...score, turns, turn_definition: 'transport-cells-including-facility-arms-v1', tuple: [score.area, score.physical_tiles, turns] };
}
