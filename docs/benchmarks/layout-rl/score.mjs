import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { key } from '../constructive-local-reembedding/path.mjs';

export const compare = (a, b) => { for (let i = 0; i < 3; i++) if (a[i] !== b[i]) return a[i] < b[i] ? -1 : 1; return 0; };
export function score(w, catalogs) {
  const result = witnessScore(w), ports = project(w, catalogs); let turns = 0;
  for (const n of w.transport_networks) {
    const ins = new Map(), outs = new Map();
    const edge = (a, b) => {
      const d = [b.x - a.x, b.y - a.y];
      for (const [map, p] of [[outs, a], [ins, b]]) { const k = key(p); if (!map.has(k)) map.set(k, []); map.get(k).push(d); }
    };
    for (const s of n.segments) edge(s.from, s.to);
    for (const t of n.terminals) if (t.endpoint.kind === 'facility') {
      const port = ports[t.endpoint.instance]?.find(p => p.id === t.endpoint.port);
      if (!port) throw new Error(`Cannot score unknown port ${t.endpoint.instance}/${t.endpoint.port}`);
      if (t.direction === 'output') edge(port.position, t.position); else edge(t.position, port.position);
    }
    for (const p of n.cells) for (const a of ins.get(key(p)) ?? []) for (const b of outs.get(key(p)) ?? [])
      if (a[0] * b[0] + a[1] * b[1] === 0) turns++;
  }
  return { ...result, turns, tuple: [result.area, result.physical_tiles, turns], turn_definition: 'directed-perpendicular-pairs-with-facility-arms-v1' };
}

// A positive advantage means a better final incumbent, with exact tuple comparisons and ties.
export function terminalAdvantages(tuples) {
  if (tuples.length < 2) throw new Error('Terminal comparison needs at least two trajectories');
  return tuples.map(a => tuples.reduce((sum, b) => sum + compare(b, a), 0) / (tuples.length - 1));
}
