import { key } from './path.mjs';
export const bodyCells = p => Array.from({ length: p.width * p.height }, (_, i) => ({ x: p.x + i % p.width, y: p.y + Math.floor(i / p.width) }));
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'info', stage: 'local-movement', entity }); };
const tag = (tr, p) => `${tr}:${key(p)}`;

export function movementPlan(w, chains, anchors, move) {
  if (!move || !Array.isArray(move.instances) || !move.instances.length || move.instances.length > 16 ||
    new Set(move.instances).size !== move.instances.length || !move.instances.every(id => w.placements.some(p => p.instance === id)) ||
    !Number.isSafeInteger(move.dx) || !Number.isSafeInteger(move.dy) || 'instance' in move || 'x' in move || 'y' in move)
    fail('local-move-input', 'Expected move DTO v2: instances (1..16 unique known IDs), integer dx/dy', move);
  const selected = new Set(move.instances), moved = new Set(), adjacency = new Map(), terminals = new Map();
  for (const c of chains) {
    const a = tag(c.transport, c.cells[0]), b = tag(c.transport, c.cells.at(-1));
    for (const [from, to] of [[a, b], [b, a]]) { if (!adjacency.has(from)) adjacency.set(from, new Set()); adjacency.get(from).add(to); }
  }
  for (const n of w.transport_networks) for (const t of n.terminals) {
    const k = tag(n.transport, t.position); if (!terminals.has(k)) terminals.set(k, []); terminals.get(k).push(t);
  }
  const own = t => t.endpoint.kind === 'facility' && selected.has(t.endpoint.instance);
  for (const [k, ts] of terminals) if (ts.some(own)) {
    if (!ts.every(own)) fail('local-shared-terminal-move', 'Shared terminal owners must move together', k);
    moved.add(k);
  }
  // Connected nonterminal anchor clusters can move only with every incident terminal owner.
  const visited = new Set();
  for (const start of adjacency.keys()) if (!terminals.has(start) && !visited.has(start)) {
    const queue = [start], neighbors = new Set(); visited.add(start);
    for (let i = 0; i < queue.length; i++) for (const v of adjacency.get(queue[i]) ?? []) {
      if (terminals.has(v)) neighbors.add(v);
      else if (!visited.has(v)) { visited.add(v); queue.push(v); }
    }
    if (neighbors.size && [...neighbors].every(v => terminals.get(v).every(own))) for (const v of queue) moved.add(v);
  }
  const translate = p => ({ x: p.x + move.dx, y: p.y + move.dy });
  const movable = (tr, p) => moved.has(tag(tr, p));
  const bodies = w.placements.filter(p => selected.has(p.instance)).map(p => ({ ...p, ...translate(p) }));
  const cells = new Set(bodies.flatMap(bodyCells).map(key)), stationary = new Set(w.placements.filter(p => !selected.has(p.instance)).flatMap(bodyCells).map(key));
  if ([...cells].some(k => stationary.has(k))) fail('local-body-collision', 'Translated group intersects a retained facility', move.instances);
  const movedAnchors = { belt: new Set(), pipe: new Set() };
  for (const n of w.transport_networks) for (const p of n.cells) if (movable(n.transport, p)) movedAnchors[n.transport].add(key(translate(p)));
  for (const tr of ['belt', 'pipe']) for (const k of anchors[tr]) if (!moved.has(`${tr}:${k}`)) {
    if (cells.has(k)) fail('local-anchor-collision', 'Translated body intersects a fixed anchor', k);
    if (movedAnchors[tr].has(k)) fail('local-terminal-collision', 'Translated anchor intersects a retained anchor', k);
  }
  for (const tr of ['belt', 'pipe']) for (const k of movedAnchors[tr])
    if (stationary.has(k) || cells.has(k)) fail('local-endpoint-body', 'Translated anchor intersects a facility body', k);
  const components = w.logistics_components.filter(c => movable(c.transport, c.position)).map(c => c.id);
  const fixedChains = new Map(), changed = new Set();
  for (const c of chains) if (movable(c.transport, c.cells[0]) && movable(c.transport, c.cells.at(-1))) {
    const path = c.cells.map(translate);
    if (path.some(p => stationary.has(key(p)) || cells.has(key(p)))) fail('local-internal-body-collision', 'Translated internal route intersects a body', c.id);
    fixedChains.set(c.id, path);
  }
  const carried = { belt: new Set([...movedAnchors.belt]), pipe: new Set([...movedAnchors.pipe]) };
  for (const c of chains) if (fixedChains.has(c.id)) for (const p of fixedChains.get(c.id)) carried[c.transport].add(key(p));
  for (const tr of ['belt', 'pipe']) for (const k of anchors[tr])
    if (!moved.has(`${tr}:${k}`) && carried[tr].has(k)) fail('local-internal-anchor-collision', 'Translated internal route intersects a retained anchor', k);
  for (const c of chains) if (!fixedChains.has(c.id) && (movable(c.transport, c.cells[0]) || movable(c.transport, c.cells.at(-1)) ||
    c.cells.some(p => cells.has(key(p)) || carried[c.transport].has(key(p))))) changed.add(c.id);
  return { selected, moved, movable, translate, components, fixedChains, changed, body: cells };
}
