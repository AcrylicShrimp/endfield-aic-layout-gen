import { key } from './path.mjs';
export const bodyCells = p => Array.from({ length: p.width * p.height }, (_, i) => ({ x: p.x + i % p.width, y: p.y + Math.floor(i / p.width) }));
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'info', stage: 'local-movement', entity }); };
const tag = (tr, p) => `${tr}:${key(p)}`;

export function movementPlan(w, chains, anchors, moves, { junctionPolicy = 'internal', componentChanges = new Map(), rerouteChains = new Set() } = {}) {
  if (!['internal', 'downstream'].includes(junctionPolicy)) fail('local-junction-policy', 'Expected internal or downstream junction ownership');
  if (!Array.isArray(moves) || moves.length > 4 || moves.some(move => !move || !Array.isArray(move.instances) || !move.instances.length ||
    !move.instances.every(id => w.placements.some(p => p.instance === id)) || !Number.isSafeInteger(move.dx) || !Number.isSafeInteger(move.dy) ||
    'instance' in move || 'x' in move || 'y' in move)) fail('local-move-input', 'Expected move schema 3: up to four translation primitives', moves);
  const instances = moves.flatMap(m => m.instances);
  if (instances.length > 16 || new Set(instances).size !== instances.length) fail('local-move-input', 'Expected at most sixteen distinct selected facilities', moves);
  const selected = new Set(instances), shifts = new Map(moves.flatMap(m => m.instances.map(id => [id, { dx: m.dx, dy: m.dy }])));
  const moved = new Set(), anchorShifts = new Map(), adjacency = new Map(), outgoing = new Map(), terminals = new Map();
  const componentTurns = new Map();
  const equalShift = (a, b) => a && b && a.dx === b.dx && a.dy === b.dy;
  const translate = (p, delta) => delta ? { x: p.x + delta.dx, y: p.y + delta.dy } : { x: p.x, y: p.y };
  const shiftFacility = p => translate(p, shifts.get(p.instance));
  const shiftAnchor = (tr, p) => translate(p, anchorShifts.get(tag(tr, p)));
  for (const c of chains) {
    const a = tag(c.transport, c.cells[0]), b = tag(c.transport, c.cells.at(-1));
    if (!outgoing.has(a)) outgoing.set(a, new Set()); outgoing.get(a).add(b);
    for (const [from, to] of [[a, b], [b, a]]) { if (!adjacency.has(from)) adjacency.set(from, new Set()); adjacency.get(from).add(to); }
  }
  for (const n of w.transport_networks) for (const t of n.terminals) {
    const k = tag(n.transport, t.position); if (!terminals.has(k)) terminals.set(k, []); terminals.get(k).push(t);
  }
  const own = t => t.endpoint.kind === 'facility' && selected.has(t.endpoint.instance);
  const common = ts => ts.length && ts.every(own) && ts.every(t => equalShift(shifts.get(t.endpoint.instance), shifts.get(ts[0].endpoint.instance))) ? shifts.get(ts[0].endpoint.instance) : null;
  for (const [k, ts] of terminals) if (ts.some(own)) {
    const delta = common(ts);
    if (!delta) fail('local-shared-terminal-move', 'Shared terminal owners must receive identical translations', k);
    moved.add(k); anchorShifts.set(k, delta);
  }
  // Move whole nonterminal clusters; downstream ownership may release incoming supply paths.
  const visited = new Set();
  for (const start of adjacency.keys()) if (!terminals.has(start) && !visited.has(start)) {
    const queue = [start], neighbors = new Set(); visited.add(start);
    for (let i = 0; i < queue.length; i++) for (const v of adjacency.get(queue[i]) ?? []) {
      if (terminals.has(v)) neighbors.add(v);
      else if (!visited.has(v)) { visited.add(v); queue.push(v); }
    }
    const sinks = new Set(queue.flatMap(v => [...(outgoing.get(v) ?? [])]).filter(v => terminals.has(v)));
    const internal = common([...neighbors].flatMap(v => terminals.get(v)));
    const sinkTerminals = [...sinks].flatMap(v => terminals.get(v));
    const downstream = junctionPolicy === 'downstream' && sinkTerminals.every(t => t.direction === 'input') ? common(sinkTerminals) : null;
    const delta = internal || downstream;
    if (delta) for (const v of queue) { moved.add(v); anchorShifts.set(v, delta); }
  }
  for (const c of w.logistics_components) if (componentChanges.has(c.id)) {
    const pose = componentChanges.get(c.id), k = tag(c.transport, c.position);
    moved.add(k); anchorShifts.set(k, { dx: pose.position.x - c.position.x, dy: pose.position.y - c.position.y }); componentTurns.set(k, pose.turns);
  }
  const movable = (tr, p) => moved.has(tag(tr, p));
  const bodies = w.placements.filter(p => selected.has(p.instance)).map(p => ({ ...p, ...shiftFacility(p) }));
  const cells = new Set(bodies.flatMap(bodyCells).map(key)), stationary = new Set(w.placements.filter(p => !selected.has(p.instance)).flatMap(bodyCells).map(key));
  if (cells.size !== bodies.reduce((n, p) => n + p.width * p.height, 0) || [...cells].some(k => stationary.has(k)))
    fail('local-body-collision', 'Simultaneous translated bodies overlap each other or a retained facility', instances);
  const movedAnchors = { belt: new Set(), pipe: new Set() };
  const origins = { belt: new Map(), pipe: new Map() };
  const carry = (tr, before, after) => {
    const k = key(after), original = key(before);
    if (origins[tr].has(k) && origins[tr].get(k) !== original) fail('local-carried-collision', 'Different carried cells collide on one layer', { transport: tr, position: after });
    origins[tr].set(k, original);
  };
  for (const n of w.transport_networks) for (const p of n.cells) if (movable(n.transport, p)) {
    const q = shiftAnchor(n.transport, p); carry(n.transport, p, q); movedAnchors[n.transport].add(key(q));
  }
  for (const tr of ['belt', 'pipe']) for (const k of anchors[tr]) if (!moved.has(`${tr}:${k}`)) {
    if (cells.has(k)) fail('local-anchor-collision', 'Translated body intersects a fixed anchor', k);
    if (movedAnchors[tr].has(k)) fail('local-terminal-collision', 'Translated anchor intersects a retained anchor', k);
  }
  for (const tr of ['belt', 'pipe']) for (const k of movedAnchors[tr])
    if (stationary.has(k) || cells.has(k)) fail('local-endpoint-body', 'Translated anchor intersects a facility body', k);
  const components = w.logistics_components.filter(c => movable(c.transport, c.position)).map(c => c.id);
  const fixedChains = new Map(), changed = new Set();
  for (const c of chains) if (!rerouteChains.has(c.id) && !componentTurns.get(tag(c.transport, c.cells[0])) && !componentTurns.get(tag(c.transport, c.cells.at(-1))) &&
    equalShift(anchorShifts.get(tag(c.transport, c.cells[0])), anchorShifts.get(tag(c.transport, c.cells.at(-1))))) {
    const delta = anchorShifts.get(tag(c.transport, c.cells[0])), path = c.cells.map(p => translate(p, delta));
    if (path.some(p => stationary.has(key(p)) || cells.has(key(p)))) fail('local-internal-body-collision', 'Translated internal route intersects a body', c.id);
    path.forEach((p, i) => carry(c.transport, c.cells[i], p));
    fixedChains.set(c.id, path);
  }
  const carried = { belt: new Set([...movedAnchors.belt]), pipe: new Set([...movedAnchors.pipe]) };
  for (const c of chains) if (fixedChains.has(c.id)) for (const p of fixedChains.get(c.id)) carried[c.transport].add(key(p));
  for (const tr of ['belt', 'pipe']) for (const k of anchors[tr])
    if (!moved.has(`${tr}:${k}`) && carried[tr].has(k)) fail('local-internal-anchor-collision', 'Translated internal route intersects a retained anchor', k);
  for (const c of chains) if (!fixedChains.has(c.id) && (movable(c.transport, c.cells[0]) || movable(c.transport, c.cells.at(-1)) ||
    c.cells.some(p => cells.has(key(p)) || carried[c.transport].has(key(p))))) changed.add(c.id);
  return { selected, moved, movable, shiftFacility, shiftAnchor, components, componentTurns, fixedChains, changed, body: cells };
}
