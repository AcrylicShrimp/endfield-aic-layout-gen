import { polyline } from '../constructive-interconnect/geometry.mjs';
import { compare, capacity, dto } from '../constructive-interconnect/rates.mjs';
import { treeModule, treeShape } from './trees.mjs';
import { lowerInterface } from './lower.mjs';
const directions = ['north', 'east', 'south', 'west'];
const rotate = (d, r) => directions[(directions.indexOf(d) + r / 90) % 4];
const fail = (code, message, entity) => { throw Object.assign(new Error(message), { code, stage: 'shared-trunk-geometry', severity: 'error', entity }); };

export function embedTrunks(catalogs, nodes, best, approach) {
  const start = performance.now(), components = [], trees = [], paths = new Map();
  const pack = (ids, firstY) => { let y = firstY; for (const id of ids) { const n = nodes.get(id); n.y = y; y += n.shape.height + 2; } };
  pack(best.producers, 2);
  for (const id of best.producers) best.sourceGroups.filter(g => g.node === id).forEach((g, i) => {
    const n = nodes.get(id), p = n.output[i]; g.port = p.id; g.port_y = n.y + p.position.y;
  });
  const leafBase = Math.max(...best.sourceGroups.map(g => g.port_y)) + 2;
  pack(best.consumers, leafBase);
  const rank = new Map(best.targetOrder.map((l, i) => [l.id, i]));
  for (const id of best.consumers) best.targetGroups.filter(g => g.node === id).forEach((g, i) => {
    const n = nodes.get(id), p = n.input[i]; g.port = p.id; g.port_y = n.y + p.position.y; g.row = leafBase + rank.get(g.lanes[0].id) * 3;
    g.root_row = g.row + treeShape(g.lanes.length, 3).root_index * 3;
  });
  const sx = 2 + Math.max(...best.producers.map(id => nodes.get(id).shape.width));
  // Reverse columns make lower source inlets end before earlier descending trunks.
  const trunks = best.sourceGroups.map((g, i) => ({ id: g.id, item: g.item, x: sx + best.sourceGroups.length - i + 1,
    root_y: g.port_y, last_y: leafBase + Math.max(...g.lanes.map(l => rank.get(l.id))) * 3 }));
  const byTrunk = new Map(trunks.map(t => [t.id, t])), mergeX = sx + best.sourceGroups.length + 3;
  const mergeWidth = Math.max(...best.targetGroups.map(g => treeShape(g.lanes.length, 3).width));
  const tx = mergeX + mergeWidth - 1 + best.targetGroups.length + 2;
  const addComponent = (id, kind, position, items, inputs, outputs, values) => {
    for (const definition of catalogs.components.components.filter(c => c.kind === kind && c.transport === 'belt' && c.footprint.width === 1 && c.footprint.height === 1))
      for (const rotation of definition.allowed_rotations) if (inputs.every(d => definition.input_directions.map(d => rotate(d, rotation)).includes(d)) &&
        outputs.every(d => definition.output_directions.map(d => rotate(d, rotation)).includes(d)) && values.every(v => compare(v, capacity(definition.capacity)) <= 0)) {
        components.push({ id, kind, component: definition.id, transport: 'belt', position, rotation, items }); return;
      }
    fail('interface-trunk-catalog-unsupported', 'No catalog component supports the planned arms and conservative channel capacity', id);
  };
  for (const g of best.sourceGroups) {
    const trunk = byTrunk.get(g.id), leaves = [...g.lanes].sort((a, b) => rank.get(a.id) - rank.get(b.id));
    for (const l of leaves.slice(0, -1)) addComponent(`${g.id}:tap:${l.id}`, 'splitter', { x: trunk.x, y: leafBase + rank.get(l.id) * 3 },
      [g.item], ['north'], ['east', 'south'], [g.value]);
  }
  // Plan crossings from interval intersections before lowering any route cells.
  const crossings = [];
  for (const l of best.lanes) {
    const own = byTrunk.get(l.source_group), y = leafBase + rank.get(l.id) * 3;
    for (const t of trunks) if (t.x > own.x && t.root_y < y && y < t.last_y) {
      if (t.item === l.item) fail('interface-trunk-same-item-intersection', 'This trunk ordering requires same-item topology synthesis', [l.id, t.id]);
      const other = best.sourceGroups.find(g => g.id === t.id), source = best.sourceGroups.find(g => g.id === l.source_group);
      addComponent(`cross:${l.id}:${t.id}`, 'bridge', { x: t.x, y }, [l.item, t.item], ['north', 'west'], ['south', 'east'], [source.value, other.value]);
      crossings.push({ lane: l.id, trunk: t.id, x: t.x, y });
    }
  }
  const right = approach(best.targetGroups.map(g => ({ id: g.id, start: { x: mergeX + mergeWidth - 1, y: g.root_row }, end: { x: tx, y: g.port_y } })));
  const mergePaths = new Map();
  for (const g of best.targetGroups) {
    if (mergeWidth > 1) {
      const tree = treeModule(g, 'converger', { x: mergeX, y: g.row }, catalogs, { pitch: 3, width: mergeWidth }); trees.push(tree); components.push(...tree.components);
      for (const p of tree.paths) mergePaths.set(p.id, p.cells);
    } else mergePaths.set(g.lanes[0].id, [{ x: mergeX, y: g.row }]);
  }
  let estimatedCells = 0;
  for (const l of best.lanes) {
    const t = byTrunk.get(l.source_group), y = leafBase + rank.get(l.id) * 3;
    const points = [[sx, t.root_y], [t.x, t.root_y], [t.x, y], ...mergePaths.get(l.id).map(p => [p.x, p.y]), ...right.get(l.target_group)];
    estimatedCells += 1 + points.slice(1).reduce((s, p, i) => s + Math.abs(p[0] - points[i][0]) + Math.abs(p[1] - points[i][1]), 0);
    if (estimatedCells > 200000) fail('interface-trunk-cell-budget', 'Diagnostic path materialization budget exceeded', { estimatedCells });
    paths.set(l.id, polyline(points));
  }
  for (const id of best.producers) { const n = nodes.get(id); n.x = sx - n.shape.width; }
  for (const id of best.consumers) nodes.get(id).x = tx + 1;
  const externalEast = tx + 2 + Math.max(...best.consumers.map(id => nodes.get(id).shape.width));
  const geometryMs = performance.now() - start, lowerStart = performance.now();
  const orderedNodes = new Map([...best.producers, ...best.consumers].map(id => [id, nodes.get(id)]));
  const result = lowerInterface(catalogs, orderedNodes, { ...best, paths, components, externalEast });
  const serializeGroups = gs => gs.map(({ value, lanes, ...g }) => ({ ...g, rate: dto(value), lanes: lanes.map(l => l.id) }));
  return { ...result, bindings: best.lanes, port_groups: { sources: serializeGroups(best.sourceGroups), targets: serializeGroups(best.targetGroups) },
    tree_modules: trees, trunk_plan: { trunks, crossings, leaf_base: leafBase, merge_x: mergeX },
    permutation: null, permutation_origin: null, external_connected: true, geometry_ms: geometryMs, lowering_ms: performance.now() - lowerStart };
}
