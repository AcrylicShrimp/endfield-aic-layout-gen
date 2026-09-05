import { permutationModule, polyline } from '../constructive-interconnect/geometry.mjs';
import { dto } from '../constructive-interconnect/rates.mjs';
import { treeModule, treeShape } from './trees.mjs';
import { lowerInterface } from './lower.mjs';
const pair = p => [p.x, p.y];

export function embedInterface(catalogs, nodes, best, approach) {
  const begin = performance.now();
  const pack = ids => { let y = 2; for (const id of ids) { const n = nodes.get(id); n.y = y; y += n.shape.height + 2; } };
  pack(best.producers); pack(best.consumers);
  for (const [bank, direction, groups, order] of [[best.producers, 'output', best.sourceGroups, best.sourceOrder],
    [best.consumers, 'input', best.targetGroups, best.targetOrder]]) {
    for (const id of bank) groups.filter(g => g.node === id).forEach((g, i) => {
      const n = nodes.get(id), p = n[direction][i]; g.port = p.id; g.port_y = n.y + p.position.y;
      g.row = order.indexOf(g.lanes[0]) * 3 + 2;
      g.root_row = g.row + treeShape(g.lanes.length, 3).root_index * 3;
    });
  }
  const sx = 2 + Math.max(...best.producers.map(id => nodes.get(id).shape.width));
  const hasTrees = [...best.sourceGroups, ...best.targetGroups].some(g => g.lanes.length > 1);
  const permutation = best.inversions ? permutationModule(best.sourceOrder, best.targetOrder.map(l => l.id), catalogs) : null;
  const components = [], trees = [], paths = new Map();
  let tx, permutationOrigin = null;
  if (!hasTrees && !permutation) {
    tx = sx + best.lanes.length + 2;
    const source = new Map(best.sourceGroups.map(g => [g.id, g])), target = new Map(best.targetGroups.map(g => [g.id, g]));
    for (const [id, points] of approach(best.sourceOrder.map(l => ({ id: l.id,
      start: { x: sx, y: source.get(l.source_group).port_y }, end: { x: tx, y: target.get(l.target_group).port_y } })))) paths.set(id, polyline(points));
  } else {
    const treeWidth = Math.max(...[...best.sourceGroups, ...best.targetGroups].map(g => treeShape(g.lanes.length, 3).width));
    const splitX = sx + best.sourceGroups.length + 2, coreX = splitX + treeWidth;
    if (permutation) permutationOrigin = { x: coreX, y: 2 };
    const coreWidth = permutation?.width ?? 1, mergeX = coreX + coreWidth;
    tx = mergeX + treeWidth - 1 + best.targetGroups.length + 2;
    const left = approach(best.sourceGroups.map(g => ({ id: g.id, start: { x: sx, y: g.port_y }, end: { x: splitX, y: g.root_row } })));
    const right = approach(best.targetGroups.map(g => ({ id: g.id, start: { x: mergeX + treeWidth - 1, y: g.root_row }, end: { x: tx, y: g.port_y } })));
    const splitPaths = new Map(), mergePaths = new Map();
    for (const [groups, kind, x, map] of [[best.sourceGroups, 'splitter', splitX, splitPaths], [best.targetGroups, 'converger', mergeX, mergePaths]])
      for (const g of groups) {
        if (hasTrees) {
          const tree = treeModule(g, kind, { x, y: g.row }, catalogs, { pitch: 3, width: treeWidth }); trees.push(tree); components.push(...tree.components);
          for (const p of tree.paths) map.set(p.id, p.cells);
        } else map.set(g.lanes[0].id, [{ x, y: g.row }]);
      }
    const corePaths = new Map((permutation?.lanes ?? best.sourceOrder.map((l, i) => ({ ...l, cells: [{ x: 0, y: i * 3 }] })))
      .map(l => [l.id, l.cells.map(p => ({ x: p.x + coreX, y: p.y + 2 }))]));
    for (const c of permutation?.components ?? []) {
      const module = permutation.modules.find(m => m.id === c.id);
      components.push({ ...c, position: { x: c.position.x + coreX, y: c.position.y + 2 },
        items: module.lanes.map(id => best.lanes.find(l => l.id === id).item) });
    }
    for (const l of best.lanes) paths.set(l.id, polyline([...left.get(l.source_group), ...splitPaths.get(l.id).map(pair),
      ...corePaths.get(l.id).map(pair), ...mergePaths.get(l.id).map(pair), ...right.get(l.target_group)]));
  }
  for (const id of best.producers) { const n = nodes.get(id); n.x = sx - n.shape.width; }
  for (const id of best.consumers) nodes.get(id).x = tx + 1;
  const externalEast = tx + 2 + Math.max(...best.consumers.map(id => nodes.get(id).shape.width));
  const geometryMs = performance.now() - begin, lowerStart = performance.now();
  let lowered;
  const orderedNodes = new Map([...best.producers, ...best.consumers].map(id => [id, nodes.get(id)]));
  try { lowered = lowerInterface(catalogs, orderedNodes, { ...best, paths, components, externalEast }); }
  catch (e) { throw Object.assign(e, { code: e.code ?? 'interface-binding-failed', stage: 'lowering', severity: 'error' }); }
  const serializeGroups = groups => groups.map(({ value, lanes, ...g }) => ({ ...g, rate: dto(value), lanes: lanes.map(l => l.id) }));
  return { ...lowered, bindings: best.lanes, port_groups: { sources: serializeGroups(best.sourceGroups), targets: serializeGroups(best.targetGroups) },
    tree_modules: trees, permutation, permutation_origin: permutationOrigin,
    external_connected: true, geometry_ms: geometryMs, lowering_ms: performance.now() - lowerStart };
}
