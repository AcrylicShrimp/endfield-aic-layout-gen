import { key, direction } from './path.mjs';
import { capacity, compare, parseRate } from '../constructive-interconnect/rates.mjs';
const names = ['east', 'south', 'west', 'north'];
const rotate = (name, rotation) => names[(names.indexOf(name) + rotation / 90) % 4];

// Enumerate actual one-cell modules before choosing a route, not post-hoc crossing repairs.
export function crossingOptions(carriers, job, catalogs, excluded) {
  const visits = new Map(), options = new Map();
  for (const c of carriers.filter(c => c.transport === job.transport)) for (const [i, p] of c.cells.entries()) {
    const k = key(p); if (!visits.has(k)) visits.set(k, []); visits.get(k).push({ c, i, p });
  }
  const poses = catalogs.components.components.filter(c => c.kind === 'bridge' && c.transport === job.transport &&
    c.footprint.width === 1 && c.footprint.height === 1 && compare(capacity(c.capacity), parseRate(job.rate)) >= 0)
    .flatMap(c => c.allowed_rotations.map(rotation => ({ c, rotation })));
  for (const [k, xs] of visits) {
    if (xs.length !== 1 || excluded.has(k)) continue;
    const { c, i, p } = xs[0];
    if (c.id === job.id || i === 0 || i === c.cells.length - 1) continue;
    const d = direction(c.cells[i - 1], p);
    if (d < 0 || direction(p, c.cells[i + 1]) !== d) continue;
    for (const next of [(d + 1) % 4, (d + 3) % 4]) {
      const pose = poses.find(({ c: def, rotation }) => compare(capacity(def.capacity), parseRate(c.rate)) >= 0 &&
        [d, next].every(axis => def.input_directions.map(n => rotate(n, rotation)).includes(names[(axis + 2) % 4]) &&
          def.output_directions.map(n => rotate(n, rotation)).includes(names[axis])));
      if (!pose) continue;
      options.set(`${k}:${next}`, { position: { ...p }, direction: next, component: pose.c.id, rotation: pose.rotation,
        transport: job.transport, kind: 'bridge', carrier_chain: c.id, carrier_network: c.network });
    }
  }
  return options;
}
