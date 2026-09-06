export const key = p => `${p.x},${p.y}`;
export const directions = [[1, 0], [0, 1], [-1, 0], [0, -1]];
export const direction = (a, b) => directions.findIndex(([x, y]) => b.x - a.x === x && b.y - a.y === y);
export const distance = (a, b) => Math.abs(a.x - b.x) + Math.abs(a.y - b.y);
const fail = (code, message) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'local-path' }); };

class Heap {
  values = [];
  less(a, b) { return a.f < b.f || (a.f === b.f && (a.g < b.g || (a.g === b.g && a.id < b.id))); }
  push(a) {
    const v = this.values; let i = v.length; v.push(a);
    while (i) { const p = (i - 1) >> 1; if (!this.less(a, v[p])) break; v[i] = v[p]; i = p; } v[i] = a;
  }
  pop() {
    const v = this.values, first = v[0], last = v.pop();
    if (v.length) { let i = 0; while (i * 2 + 1 < v.length) { let c = i * 2 + 1;
      if (c + 1 < v.length && this.less(v[c + 1], v[c])) c++;
      if (!this.less(v[c], last)) break; v[i] = v[c]; i = c;
    } v[i] = last; } return first;
  }
}

// Manhattan is an admissible lower bound; direction states account for turn costs and fixed arms.
export function shortestPath(start, end, blocked, bounds, { first = null, last = null, workBudget = 200000, crossingOptions = new Map(), crossingLimit = 0, allowZeroLength = false } = {}) {
  const { minX, minY, maxX, maxY } = bounds, width = maxX - minX + 1, height = maxY - minY + 1;
  if (![minX, minY, maxX, maxY, start.x, start.y, end.x, end.y, workBudget].every(Number.isSafeInteger) ||
    width < 1 || height < 1 || width * height > 250000 || workBudget < 1 ||
    [first, last].some(d => d !== null && (!Number.isInteger(d) || d < 0 || d > 3)) ||
    !(crossingOptions instanceof Map) || !Number.isSafeInteger(crossingLimit) || crossingLimit < 0 || crossingLimit > 8 || typeof allowZeroLength !== 'boolean')
    fail('path-invalid-input', 'Invalid grid, endpoint, direction, crossing or work budget');
  const inside = p => p.x >= minX && p.y >= minY && p.x <= maxX && p.y <= maxY;
  if (!inside(start) || !inside(end) || blocked.has(key(start)) || blocked.has(key(end))) return { status: 'blocked-endpoint', expanded: 0 };
  if (key(start) === key(end)) return allowZeroLength && first === null && last === null ?
    { status: 'found', cells: [{ ...start }], crossings: [], steps: 0, turns: 0, expanded: 0 } : { status: 'same-endpoint', expanded: 0 };
  const weight = width * height * 4 * (crossingLimit + 1) + 1, heap = new Heap(), best = new Map(), parents = new Map();
  const id = (p, d, used) => (((p.y - minY) * width + p.x - minX) * 5 + d) * (crossingLimit + 1) + used;
  const initial = id(start, 4, 0); best.set(initial, 0); heap.push({ ...start, d: 4, used: 0, id: initial, g: 0, f: distance(start, end) * weight });
  let expanded = 0;
  while (heap.values.length) {
    const p = heap.pop(); if (best.get(p.id) !== p.g) continue;
    if (++expanded > workBudget) return { status: 'work-budget', expanded: expanded - 1 };
    if (p.x === end.x && p.y === end.y && (last === null || p.d === last)) {
      const cells = [], crossings = []; let current = p;
      while (current) { cells.push({ x: current.x, y: current.y }); if (current.via) crossings.push(current.via); current = parents.get(current.id); }
      cells.reverse(); crossings.reverse();
      // An optimal ordinary path is simple. Arm-constrained looping is rejected, not silently accepted.
      if (new Set(cells.map(key)).size !== cells.length) return { status: 'repeated-cell', expanded };
      return { status: 'found', cells, crossings, steps: Math.floor(p.g / weight), turns: p.g % weight, expanded };
    }
    for (let d = 0; d < 4; d++) {
      if (p.via && d !== p.d) continue;
      if (p.d === 4 && first !== null && d !== first) continue;
      if (p.d !== 4 && (d + 2) % 4 === p.d) continue;
      let q = { x: p.x + directions[d][0], y: p.y + directions[d][1] }, via = null;
      if (blocked.has(key(q))) {
        via = crossingOptions.get(`${key(q)}:${d}`);
        if (!via || p.used >= crossingLimit) continue;
        if (!via.position || key(via.position) !== key(q) || via.direction !== d) fail('path-invalid-crossing', 'Crossing module does not match its entry cell and direction');
      }
      if (!inside(q) || (blocked.has(key(q)) && !via) || key(q) === key(start)) continue;
      if (key(q) === key(end) && last !== null && d !== last) continue;
      const used = p.used + Number(!!via), g = p.g + weight + (p.d !== 4 && p.d !== d ? 1 : 0), qid = id(q, d, used);
      if (best.has(qid) && best.get(qid) <= g) continue;
      best.set(qid, g); parents.set(qid, p); heap.push({ ...q, d, used, via, id: qid, g, f: g + distance(q, end) * weight });
    }
  }
  return { status: 'no-path', expanded };
}
