import { pendingOutputAccess } from '../constructive-factory-closure/model.mjs';
import { pendingOutputSpace } from '../constructive-factory-closure/output-space.mjs';
import { bodyCells } from '../constructive-local-reembedding/movement.mjs';
const key = p => `${p.x},${p.y}`;

// A detached region can be translated. Its local zero is not a factory boundary.
export function regionInterface(graph, witness, catalogs) {
  const body = new Set(witness.placements.flatMap(bodyCells).map(key)), layers = { belt: new Set(), pipe: new Set() };
  for (const n of witness.transport_networks) for (const p of n.cells) layers[n.transport].add(key(p));
  for (const c of witness.logistics_components) layers[c.transport].add(key(c.position));
  if ([...body, ...layers.belt, ...layers.pipe].some(k => k.split(',').some(v => Number(v) < 0)))
    throw Object.assign(new Error('Expected normalized nonnegative region geometry'), { code: 'scc-interface-coordinates' });
  const access = pendingOutputAccess(graph, witness, catalogs).map(g => {
    const ports = g.ports.map(p => ({ ...p, obstruction: p.obstruction === 'negative-coordinate' ? null : p.obstruction }));
    return { ...g, ports, available: ports.filter(p => !p.obstruction).length };
  });
  const space = pendingOutputSpace(graph, witness, catalogs, access, { body, layers });
  const diagnostics = [
    ...access.filter(g => g.available < g.required).map(g => ({ code: 'scc-pending-output-obstructed', severity: 'error', stage: 'region-interface',
      message: 'Not enough unused output connections for exact pending item margins', entity: g })),
    ...space.failures.map(entity => ({ code: 'scc-pending-output-pocket', severity: 'error', stage: 'region-interface',
      message: 'Pending output enters a closed pocket without room for its consumer', entity })),
  ];
  return { accepted: !diagnostics.length, coordinate_frame: 'local-translatable', access, space, diagnostics };
}
