import { key } from './path.mjs';
import { detachablePairs, joinableChains, terminalJoinPlan } from './terminal-identity.mjs';
export const bodyCells = p => Array.from({ length: p.width * p.height }, (_, i) => ({ x: p.x + i % p.width, y: p.y + Math.floor(i / p.width) }));
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'info', stage: 'local-movement', entity }); };
const tag = (tr, p) => `${tr}:${key(p)}`;

// A description may inspect hundreds of poses of one immutable witness. These
// indexes contain only source geometry; every movement plan owns its mutable sets.
export function movementIndex(w, chains) {
  const adjacency = new Map(), outgoing = new Map(), terminals = new Map(), clusters = [];
  const indexedChains = chains.map(c => ({ c, start: tag(c.transport, c.cells[0]), end: tag(c.transport, c.cells.at(-1)) }));
  const cellChains = { belt: new Map(), pipe: new Map() }, networkCells = new Map();
  for (const { c, start: a, end: b } of indexedChains) {
    if (!outgoing.has(a)) outgoing.set(a, new Set()); outgoing.get(a).add(b);
    for (const [from, to] of [[a, b], [b, a]]) { if (!adjacency.has(from)) adjacency.set(from, new Set()); adjacency.get(from).add(to); }
    for (const p of c.cells) { const k = key(p), list = cellChains[c.transport].get(k) ?? []; list.push(c.id); cellChains[c.transport].set(k, list); }
  }
  let ordinal = 0;
  for (const n of w.transport_networks) {
    for (const t of n.terminals) { const k = tag(n.transport, t.position), list = terminals.get(k) ?? []; list.push(t); terminals.set(k, list); }
    for (const p of n.cells) {
      const k = tag(n.transport, p), list = networkCells.get(k) ?? [];
      list.push({ transport: n.transport, position: p, ordinal: ordinal++ }); networkCells.set(k, list);
    }
  }
  const visited = new Set();
  for (const start of adjacency.keys()) if (!terminals.has(start) && !visited.has(start)) {
    const queue = [start], neighbors = new Set(); visited.add(start);
    for (let i = 0; i < queue.length; i++) for (const v of adjacency.get(queue[i]) ?? []) {
      if (terminals.has(v)) neighbors.add(v);
      else if (!visited.has(v)) { visited.add(v); queue.push(v); }
    }
    const sinks = new Set(queue.flatMap(v => [...(outgoing.get(v) ?? [])]).filter(v => terminals.has(v)));
    clusters.push({ anchors: queue, neighbors: [...neighbors].flatMap(v => terminals.get(v)), sinks: [...sinks].flatMap(v => terminals.get(v)) });
  }
  return { witness: w, chains, indexedChains, terminals, clusters, networkCells, cellChains,
    pairs: detachablePairs(w), joinable: joinableChains(w, chains),
    bodyKeys: new Map(w.placements.map(p => [p, bodyCells(p).map(key)])) };
}

export function movementPlan(w, chains, anchors, moves, { junctionPolicy = 'internal', componentChanges = new Map(), facilityChanges = new Map(), rerouteChains = new Set(), index = movementIndex(w, chains), joinable = index.joinable } = {}) {
  if (index.witness !== w || index.chains !== chains) fail('local-movement-index', 'Movement index must belong to this immutable witness and chain extraction');
  if (!['internal', 'downstream'].includes(junctionPolicy)) fail('local-junction-policy', 'Expected internal or downstream junction ownership');
  if (!Array.isArray(moves) || moves.length > 4 || moves.some(move => !move || !Array.isArray(move.instances) || !move.instances.length ||
    !move.instances.every(id => w.placements.some(p => p.instance === id)) || !Number.isSafeInteger(move.dx) || !Number.isSafeInteger(move.dy) ||
    'instance' in move || 'x' in move || 'y' in move)) fail('local-move-input', 'Expected move schema 3: up to four translation primitives', moves);
  const instances = moves.flatMap(m => m.instances);
  if (instances.length > 16 || new Set(instances).size !== instances.length) fail('local-move-input', 'Expected at most sixteen distinct selected facilities', moves);
  const selected = new Set(instances), shifts = new Map(moves.flatMap(m => m.instances.map(id => [id, { dx: m.dx, dy: m.dy }])));
  if ([...facilityChanges.keys()].some(id => !selected.has(id))) fail('local-facility-rotation', 'Rotated facilities must belong to explicit translation primitives');
  const moved = new Set(), anchorShifts = new Map(), { terminals, pairs } = index;
  const anchorTurns = new Map();
  const separated = new Set();
  const equalShift = (a, b) => a && b && a.dx === b.dx && a.dy === b.dy;
  const translate = (p, delta) => delta ? { x: p.x + delta.dx, y: p.y + delta.dy } : { x: p.x, y: p.y };
  const shiftFacility = p => facilityChanges.has(p.instance) ? { ...facilityChanges.get(p.instance).pose } : translate(p, shifts.get(p.instance));
  const shiftAnchor = (tr, p) => translate(p, anchorShifts.get(tag(tr, p)));
  const terminalDelta = t => {
    if (t.endpoint.kind !== 'facility') return null;
    const position = facilityChanges.get(t.endpoint.instance)?.terminalPositions.get(t.id);
    return position ? { dx: position.x - t.position.x, dy: position.y - t.position.y } : shifts.get(t.endpoint.instance);
  };
  const terminalTurns = t => t.endpoint.kind === 'facility' ? facilityChanges.get(t.endpoint.instance)?.turns ?? 0 : 0;
  const shiftTerminal = t => translate(t.position, terminalDelta(t));
  const shiftEndpoint = (c, end) => c.zero_length ? shiftTerminal(end ? c.endTerminals[0] : c.startTerminals[0]) : shiftAnchor(c.transport, end ? c.cells.at(-1) : c.cells[0]);
  const own = t => t.endpoint.kind === 'facility' && selected.has(t.endpoint.instance);
  const common = ts => ts.length && ts.every(own) && ts.every(t => equalShift(terminalDelta(t), terminalDelta(ts[0]))) ? terminalDelta(ts[0]) : null;
  for (const [k, ts] of terminals) if (ts.some(own)) {
    const delta = common(ts), turns = terminalTurns(ts[0]), sameTurns = ts.every(t => terminalTurns(t) === turns);
    if ((!delta || !sameTurns) && pairs.has(k)) { separated.add(k); continue; }
    if (!delta) fail('local-shared-terminal-move', 'Non-detachable shared terminal owners must receive identical translations', k);
    if (!sameTurns) fail('local-shared-terminal-rotation', 'Shared terminal owners have incompatible retained arm rotations in this neighborhood', k);
    moved.add(k); anchorShifts.set(k, delta);
    if (turns) anchorTurns.set(k, turns);
  }
  // Move whole nonterminal clusters; downstream ownership may release incoming supply paths.
  for (const cluster of index.clusters) {
    const internal = common(cluster.neighbors), sinkTerminals = cluster.sinks;
    const downstream = junctionPolicy === 'downstream' && sinkTerminals.every(t => t.direction === 'input') ? common(sinkTerminals) : null;
    const delta = internal || downstream;
    if (delta) for (const v of cluster.anchors) { moved.add(v); anchorShifts.set(v, delta); }
  }
  for (const c of w.logistics_components) if (componentChanges.has(c.id)) {
    const pose = componentChanges.get(c.id), k = tag(c.transport, c.position);
    moved.add(k); anchorShifts.set(k, { dx: pose.position.x - c.position.x, dy: pose.position.y - c.position.y }); anchorTurns.set(k, pose.turns);
  }
  const movable = (tr, p) => moved.has(tag(tr, p));
  const bodies = w.placements.filter(p => selected.has(p.instance)).map(p => ({ ...p, ...shiftFacility(p) }));
  const cells = new Set(bodies.flatMap(bodyCells).map(key)), stationary = new Set(w.placements.filter(p => !selected.has(p.instance)).flatMap(p => index.bodyKeys.get(p)));
  if (cells.size !== bodies.reduce((n, p) => n + p.width * p.height, 0) || [...cells].some(k => stationary.has(k)))
    fail('local-body-collision', 'Simultaneous translated bodies overlap each other or a retained facility', instances);
  const joined = terminalJoinPlan(w, joinable, shiftTerminal, c => shiftAnchor(c.transport, c.position));
  const movedAnchors = { belt: new Set(), pipe: new Set() };
  const origins = { belt: new Map(), pipe: new Map() };
  const carry = (tr, before, after) => {
    const k = key(after), original = key(before);
    if (origins[tr].has(k) && origins[tr].get(k) !== original && !(joined.allows(tr, k, original) && joined.allows(tr, k, origins[tr].get(k)))) fail('local-carried-collision', 'Different carried cells collide on one layer', { transport: tr, position: after });
    origins[tr].set(k, original);
  };
  const movedCells = [...moved].flatMap(k => index.networkCells.get(k) ?? []).sort((a, b) => a.ordinal - b.ordinal);
  for (const { transport, position: p } of movedCells) {
    const q = shiftAnchor(transport, p); carry(transport, p, q); movedAnchors[transport].add(key(q));
  }
  for (const k of separated) {
    const pair = pairs.get(k);
    for (const t of [pair.source, pair.target]) { const q = shiftTerminal(t); carry(pair.transport, t.position, q); movedAnchors[pair.transport].add(key(q)); }
  }
  for (const tr of ['belt', 'pipe']) for (const k of anchors[tr]) if (!moved.has(`${tr}:${k}`) && !separated.has(`${tr}:${k}`)) {
    if (cells.has(k)) fail('local-anchor-collision', 'Translated body intersects a fixed anchor', k);
    if (movedAnchors[tr].has(k) && !joined.allows(tr, k, k)) fail('local-terminal-collision', 'Translated anchor intersects a retained anchor', k);
  }
  for (const tr of ['belt', 'pipe']) for (const k of movedAnchors[tr])
    if (stationary.has(k) || cells.has(k)) fail('local-endpoint-body', 'Translated anchor intersects a facility body', k);
  const components = w.logistics_components.filter(c => movable(c.transport, c.position)).map(c => c.id);
  const fixedChains = new Map(), changed = new Set();
  for (const { c, start, end } of index.indexedChains) if (!rerouteChains.has(c.id) && !anchorTurns.get(start) && !anchorTurns.get(end) &&
    equalShift(anchorShifts.get(start), anchorShifts.get(end))) {
    const delta = anchorShifts.get(start), path = c.cells.map(p => translate(p, delta));
    if (path.some(p => stationary.has(key(p)) || cells.has(key(p)))) fail('local-internal-body-collision', 'Translated internal route intersects a body', c.id);
    path.forEach((p, i) => carry(c.transport, c.cells[i], p));
    fixedChains.set(c.id, path);
  }
  const carried = { belt: new Set([...movedAnchors.belt]), pipe: new Set([...movedAnchors.pipe]) };
  for (const c of chains) if (fixedChains.has(c.id)) for (const p of fixedChains.get(c.id)) carried[c.transport].add(key(p));
  for (const tr of ['belt', 'pipe']) for (const k of anchors[tr])
    if (!moved.has(`${tr}:${k}`) && !separated.has(`${tr}:${k}`) && carried[tr].has(k) && !joined.allows(tr, k, k)) fail('local-internal-anchor-collision', 'Translated internal route intersects a retained anchor', k);
  const intersects = new Set();
  for (const tr of ['belt', 'pipe']) for (const k of [...cells, ...carried[tr]])
    for (const id of index.cellChains[tr].get(k) ?? []) intersects.add(id);
  for (const { c, start, end } of index.indexedChains) if (!fixedChains.has(c.id) &&
    (separated.has(start) || moved.has(start) || moved.has(end) || intersects.has(c.id))) changed.add(c.id);
  return { selected, moved, movable, joined: joined.chains, shiftFacility, shiftAnchor, shiftTerminal, shiftEndpoint, separated, components, anchorTurns, fixedChains, changed, body: cells };
}
