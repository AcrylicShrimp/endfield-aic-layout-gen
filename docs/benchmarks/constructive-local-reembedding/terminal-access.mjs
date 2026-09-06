import { Heap, key, directions, distance } from './path.mjs';

// Exact only for the declared hard-body/soft-terminal finite-grid relaxation.
export function minimumTerminalReleasePath(start, end, hard, soft, bounds, { workBudget = 200000 } = {}) {
  const { minX, minY, maxX, maxY } = bounds ?? {}, width = maxX - minX + 1, height = maxY - minY + 1, size = width * height;
  const entity = { start, end };
  const diagnostic = (status, message) => ({ code: `terminal-access-${status}`, severity: 'info', stage: 'terminal-access', message, entity });
  if (![minX, minY, maxX, maxY, start?.x, start?.y, end?.x, end?.y, workBudget].every(Number.isSafeInteger) ||
    width < 1 || height < 1 || !Number.isSafeInteger(size) || size > 250000 || workBudget < 1 || !(hard instanceof Set) || !(soft instanceof Map) ||
    [...soft].some(([k, owners]) => typeof k !== 'string' || !Array.isArray(owners) || !owners.length))
    throw Object.assign(new Error('Expected integer endpoints, a finite grid, hard Set, nonempty soft-owner arrays and positive work budget'),
      { ...diagnostic('invalid-input', 'Invalid relaxed terminal-access input'), severity: 'error' });
  const inside = p => p.x >= minX && p.y >= minY && p.x <= maxX && p.y <= maxY;
  const done = (status, expanded, message, extra = {}) => ({ status, expanded, ...extra, diagnostic: diagnostic(status, message) });
  if (!inside(start) || !inside(end) || hard.has(key(start)) || hard.has(key(end)))
    return done('blocked-endpoint', 0, 'Endpoint is outside the finite canvas or in a hard obstacle');
  const id = p => (p.y - minY) * width + p.x - minX, point = i => ({ x: minX + i % width, y: minY + Math.floor(i / width) });
  const first = id(start), last = id(end), weight = size + 1;
  const best = new Float64Array(size).fill(Infinity), parent = new Int32Array(size).fill(-1), heap = new Heap();
  best[first] = 0; heap.push({ id: first, g: 0, f: distance(start, end) });
  let expanded = 0;
  while (heap.values.length) {
    const current = heap.pop(); if (best[current.id] !== current.g) continue;
    if (expanded === workBudget) return done('work-budget', expanded, 'Expansion budget exhausted; relaxed reachability remains unknown');
    expanded++;
    if (current.id === last) {
      const cells = []; for (let i = last; i !== -1; i = parent[i]) cells.push(point(i)); cells.reverse();
      const blockers = cells.slice(1, -1).filter(p => soft.has(key(p))).map(position => ({ position, owners: structuredClone(soft.get(key(position))) }));
      return done('found', expanded, 'Minimum terminal-cell-contact path in the relaxation; NOT a valid factory route',
        { cells, steps: cells.length - 1, blocker_cells: blockers.length, blockers });
    }
    const p = point(current.id);
    for (const [dx, dy] of directions) {
      const q = { x: p.x + dx, y: p.y + dy }; if (!inside(q) || hard.has(key(q))) continue;
      const qid = id(q), g = current.g + 1 + (qid !== first && qid !== last && soft.has(key(q)) ? weight : 0);
      if (g >= best[qid]) continue;
      best[qid] = g; parent[qid] = current.id; heap.push({ id: qid, g, f: g + distance(q, end) });
    }
  }
  return done('no-path', expanded, 'No path exists in this finite relaxation; no global factory infeasibility claim');
}

export function terminalAccessObstacles(w, movement, transport, endpoints, body) {
  const hard = new Set(body), soft = new Map(), exempt = new Set(endpoints.map(key));
  for (const [network, n] of w.transport_networks.entries()) if (n.transport === transport) for (const t of n.terminals) {
    const k = key(movement.shiftTerminal(t));
    if (t.endpoint.kind === 'external') { if (!exempt.has(k)) hard.add(k); continue; }
    if (!soft.has(k)) soft.set(k, []);
    soft.get(k).push({ network, terminal: t.id, instance: t.endpoint.instance, port: t.endpoint.port, item: n.item, direction: t.direction });
  }
  return { hard, soft };
}
