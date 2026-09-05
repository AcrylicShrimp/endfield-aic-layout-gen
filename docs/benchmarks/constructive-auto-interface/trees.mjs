import { rate, add, parseRate, compare, capacity, dto } from '../constructive-interconnect/rates.mjs';
import { polyline } from '../constructive-interconnect/geometry.mjs';
const directions = ['north', 'east', 'south', 'west'];
const rotate = (d, r) => directions[(directions.indexOf(d) + r / 90) % 4];
const fail = (code, message, entity) => { throw Object.assign(new Error(message), { code, stage: 'tree-geometry', severity: 'error', entity }); };

// Odd trees have two odd children; even trees have only one even descendant.
// Thus every junction is three-way except at most one two-way junction.
function partition(count) {
  if (count < 2) return { top: 0, bottom: 0 };
  if (count === 2) return { top: 0, bottom: 1 };
  let top = Math.floor((count - 1) / 2);
  if (count % 2 && top % 2 === 0) top--;
  return { top, bottom: count - top - 1 };
}
export function treeShape(count, pitch = 1) {
  if (!Number.isSafeInteger(count) || count < 1 || count > 384 || !Number.isSafeInteger(pitch) || pitch < 1)
    fail('interface-tree-budget', 'Expected 1..384 leaves and positive integer pitch', count);
  const depth = n => { if (n < 2) return 0; const p = partition(n); return 1 + Math.max(depth(p.top), depth(p.bottom)); };
  return { width: 2 * depth(count) + 1, height: (count - 1) * pitch + 1, root_index: partition(count).top };
}

// Fixed west-root/east-leaf geometry using all three catalog branch arms.
// A three-leaf module is 3x3 at pitch one. Larger modules repeat that junction.
export function treeModule(group, kind, origin, catalogs, { pitch = 1, width = null } = {}) {
  const leaves = group.lanes, split = kind === 'splitter';
  if (!Array.isArray(leaves) || !['splitter', 'converger'].includes(kind) || !leaves.length ||
      !Number.isSafeInteger(origin.x) || !Number.isSafeInteger(origin.y) || origin.x < 0 || origin.y < 0 ||
      new Set(leaves.map(l => l.id)).size !== leaves.length || leaves.some(l => l.item !== group.item || l.transport !== leaves[0].transport) ||
      catalogs.items.items.find(i => i.id === group.item)?.transport !== leaves[0].transport)
    fail('interface-invalid-tree', 'Expected unique positive leaves of one runtime commodity', group.id);
  const shape = treeShape(leaves.length, pitch), actualWidth = width ?? shape.width;
  if (!Number.isSafeInteger(actualWidth) || actualWidth < shape.width) fail('interface-invalid-tree', 'Reserved width is smaller than the tree', group.id);
  const total = leaves.reduce((s, l) => add(s, parseRate(l.rate)), rate(0));
  const cap = catalogs.transports.transports.find(t => t.kind === leaves[0].transport);
  if (!cap || leaves.some(l => !parseRate(l.rate).n) || compare(total, capacity(cap.capacity)) > 0)
    fail('interface-tree-capacity', 'Tree root exceeds one transport lane or has zero demand', group.id);
  const junctions = [], endX = origin.x + actualWidth - 1;
  const build = (offset, count, x) => {
    const p = partition(count), middle = offset + p.top, y = origin.y + middle * pitch;
    if (count === 1) return { root: [x, y], paths: [{ id: leaves[offset].id, points: [[x, y], [endX, y]] }] };
    const at = [x + 1, y], arms = ['east'];
    const paths = [{ id: leaves[middle].id, points: [[x, y], at, [endX, y]] }];
    for (const [size, first, arm] of [[p.top, offset, 'north'], [p.bottom, middle + 1, 'south']]) if (size) {
      arms.push(arm); const child = build(first, size, x + 2);
      for (const q of child.paths) paths.push({ id: q.id, points: [[x, y], at, [x + 1, child.root[1]], ...q.points] });
    }
    junctions.push({ at, arms }); return { root: [x, y], paths };
  };
  const built = build(0, leaves.length, origin.x), mirror = p => ({ x: 2 * origin.x + actualWidth - 1 - p.x, y: p.y });
  const components = junctions.map((j, i) => {
    const inputs = split ? ['west'] : j.arms.map(d => d === 'east' ? 'west' : d), outputs = split ? j.arms : ['east'];
    let match;
    for (const c of catalogs.components.components.filter(c => c.kind === kind && c.transport === leaves[0].transport && c.footprint.width === 1 && c.footprint.height === 1))
      for (const rotation of c.allowed_rotations) if (!match && inputs.every(d => c.input_directions.map(d => rotate(d, rotation)).includes(d)) &&
          outputs.every(d => c.output_directions.map(d => rotate(d, rotation)).includes(d)) && compare(total, capacity(c.capacity)) <= 0) match = { c, rotation };
    if (!match) fail('interface-tree-catalog-unsupported', 'No catalog junction supports the selected three-way arms and rate', group.id);
    const position = { x: j.at[0], y: j.at[1] };
    return { id: `${group.id}:${kind}:${i}`, component: match.c.id, kind, transport: leaves[0].transport,
      rotation: match.rotation, position: split ? position : mirror(position), items: [group.item] };
  });
  const paths = built.paths.map(p => { const cells = polyline(p.points); return { id: p.id, cells: split ? cells : cells.map(mirror).reverse() }; });
  const root = { x: built.root[0], y: built.root[1] };
  return { id: `${group.id}:${kind}`, kind, item: group.item, root_rate: dto(total), x: origin.x, y: origin.y,
    width: actualWidth, height: shape.height, pitch, components, paths, root: split ? root : mirror(root),
    junction_lower_bound: Math.ceil((leaves.length - 1) / 2), minimum_junction_count: components.length === Math.ceil((leaves.length - 1) / 2) };
}
