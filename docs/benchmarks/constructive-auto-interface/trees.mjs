import { rate, add, parseRate, compare, capacity, dto } from '../constructive-interconnect/rates.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
const directions = ['north', 'east', 'south', 'west'];
const rotate = (d, r) => directions[(directions.indexOf(d) + r / 90) % 4];
const fail = (code, message, entity) => { throw Object.assign(new Error(message), { code, stage: 'tree-geometry', severity: 'error', entity }); };

// Same five-column comb for every arity. Two active output/input arms per junction;
// the catalog's unused arm remains unused. Bounds reserve geometry before lowering.
export function treeModule(group, kind, origin, catalogs) {
  const leaves = group.lanes, split = kind === 'splitter';
  if (leaves?.length > 384) fail('interface-tree-budget', 'Diagnostic tree leaf budget exceeded', group.id);
  if (!['splitter', 'converger'].includes(kind) || !leaves.length ||
      !Number.isSafeInteger(origin.x) || !Number.isSafeInteger(origin.y) || origin.x < 0 || origin.y < 0 ||
      new Set(leaves.map(l => l.id)).size !== leaves.length || leaves.some(l => l.item !== group.item || l.transport !== leaves[0].transport) ||
      catalogs.items.items.find(i => i.id === group.item)?.transport !== leaves[0].transport)
    fail('interface-invalid-tree', 'Expected nonempty independent leaves of one item and transport', group.id);
  const total = leaves.reduce((s, l) => add(s, parseRate(l.rate)), rate(0));
  const cap = catalogs.transports.transports.find(t => t.kind === leaves[0].transport);
  if (!cap || leaves.some(l => !parseRate(l.rate).n) || compare(total, capacity(cap.capacity)) > 0)
    fail('interface-tree-capacity', 'Tree root exceeds a single transport lane or contains zero demand', group.id);
  const components = [];
  for (let i = 0; i < leaves.length - 1; i++) {
    const inputs = split ? [i === 0 ? 'west' : 'north'] : ['west', 'south'];
    const outputs = split ? ['east', 'south'] : [i === 0 ? 'east' : 'north'];
    let match;
    for (const c of catalogs.components.components.filter(c => c.kind === kind && c.transport === leaves[0].transport && c.footprint.width === 1 && c.footprint.height === 1))
      for (const rotation of c.allowed_rotations) if (!match && inputs.every(d => c.input_directions.map(d => rotate(d, rotation)).includes(d)) &&
          outputs.every(d => c.output_directions.map(d => rotate(d, rotation)).includes(d)) && compare(total, capacity(c.capacity)) <= 0) match = { c, rotation };
    if (!match) fail('interface-tree-catalog-unsupported', 'No catalog component fits the fixed comb arms and root rate', group.id);
    components.push({ id: `${group.id}:${kind}:${i}`, component: match.c.id, kind, transport: leaves[0].transport,
      rotation: match.rotation, position: { x: origin.x + 2, y: origin.y + i * 3 }, items: [group.item] });
  }
  const paths = leaves.map((l, i) => {
    const points = [[origin.x, origin.y], [origin.x + 2, origin.y], [origin.x + 2, origin.y + i * 3], [origin.x + 4, origin.y + i * 3]];
    const cells = polyline(points);
    // Mirror horizontally and reverse to turn a splitter into a converger.
    return { id: l.id, cells: split ? cells : cells.map(p => ({ x: 2 * origin.x + 4 - p.x, y: p.y })).reverse() };
  });
  return { id: `${group.id}:${kind}`, kind, item: group.item, root_rate: dto(total),
    x: origin.x, y: origin.y, width: 5, height: (leaves.length - 1) * 3 + 1, components, paths,
    root: { x: origin.x + (split ? 0 : 4), y: origin.y } };
}
