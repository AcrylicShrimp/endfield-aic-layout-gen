const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'embedding-length', entity }); };
const dot = (a, b) => a.reduce((n, v, i) => n + BigInt(v) * BigInt(b[i]), 0n);

// Integer minimum closure through an iterative level-graph max flow.
export function minimizeLinearCost(lower, upper, inequalities, costs, { nodeBudget = 100000, edgeBudget = 500000, workBudget = 20000000, tie = 'least' } = {}) {
  const start = performance.now(), n = lower.length;
  if (!['least', 'greatest'].includes(tie)) fail('length-tie-policy', 'Expected least or greatest optimal closure', tie);
  if (![nodeBudget, edgeBudget, workBudget].every(v => Number.isSafeInteger(v) && v >= 0)) fail('length-budget-schema', 'Budgets must be nonnegative safe integers');
  let work = 0;
  const step = () => { if (++work > workBudget) fail('length-work-budget', 'Threshold construction or minimum-cut work budget exhausted', { work, workBudget }); };
  if (upper.length !== n || costs.length !== n || lower.some((v, i) => !Number.isSafeInteger(v) || !Number.isSafeInteger(upper[i]) || upper[i] < v || !Number.isSafeInteger(costs[i])))
    fail('length-bound-schema', 'Expected finite integer bounds and coefficients');
  for (const e of inequalities) { step(); if (!Number.isSafeInteger(e.from) || !Number.isSafeInteger(e.to) || e.from < 0 || e.to < 0 || e.from >= n || e.to >= n || !Number.isSafeInteger(e.weight) ||
    lower[e.to] < lower[e.from] + e.weight || upper[e.to] < upper[e.from] + e.weight)
    fail('length-bound-certificate', 'Bounds must both satisfy the difference constraints', e);
  }
  let count = 2, finite = 0;
  const bases = lower.map((v, i) => { const base = count; count += upper[i] - v; finite += Math.abs(costs[i]) * (upper[i] - v); return base; });
  if (!Number.isSafeInteger(count) || count > nodeBudget) fail('length-node-budget', 'Threshold-node budget exhausted', { count, nodeBudget });
  if (!Number.isSafeInteger(finite + 1)) fail('length-integer-cost', 'Threshold capacity sum exceeds safe integer arithmetic');
  const graph = Array.from({ length: count }, () => []), source = 0, sink = 1, infinite = finite + 1;
  let edgeCount = 0;
  const edge = (from, to, capacity) => {
    if (from === to) return;
    if (++edgeCount > edgeBudget) fail('length-edge-budget', 'Threshold-edge budget exhausted', { edgeCount, edgeBudget });
    const a = { to, rev: graph[to].length, capacity }, b = { to: from, rev: graph[from].length, capacity: 0 };
    graph[from].push(a); graph[to].push(b);
  };
  const threshold = (i, k) => bases[i] + (k - lower[i] - 1);
  for (let i = 0; i < n; i++) for (let k = lower[i] + 1; k <= upper[i]; k++) {
    step();
    const u = threshold(i, k);
    if (k > lower[i] + 1) edge(u, u - 1, infinite);
    if (costs[i] > 0) edge(u, sink, costs[i]); else if (costs[i] < 0) edge(source, u, -costs[i]);
  }
  for (const e of inequalities) for (let k = lower[e.from] + 1; k <= upper[e.from]; k++) {
    step();
    const target = k + e.weight;
    if (target > lower[e.to]) edge(threshold(e.from, k), threshold(e.to, target), infinite);
  }
  const buildMs = performance.now() - start, flowStart = performance.now();
  let flow = 0, phases = 0, augmentations = 0;
  const reachable = () => {
    const level = new Int32Array(count).fill(-1), queue = [source]; level[source] = 0;
    for (let i = 0; i < queue.length; i++) for (const e of graph[queue[i]]) {
      step(); if (e.capacity > 0 && level[e.to] < 0) { level[e.to] = level[queue[i]] + 1; queue.push(e.to); }
    }
    return level;
  };
  let levels;
  while ((levels = reachable())[sink] >= 0) {
    phases++; const next = new Int32Array(count);
    while (true) {
      const vertices = [source], path = [];
      while (vertices.length && vertices.at(-1) !== sink) {
        const u = vertices.at(-1);
        while (next[u] < graph[u].length) {
          step(); const e = graph[u][next[u]];
          if (e.capacity > 0 && levels[e.to] === levels[u] + 1) break;
          next[u]++;
        }
        if (next[u] === graph[u].length) {
          vertices.pop(); if (path.length) { const last = path.pop(); next[last.from]++; }
        } else { const e = graph[u][next[u]]; path.push({ from: u, index: next[u] }); vertices.push(e.to); }
      }
      if (!vertices.length) break;
      let amount = infinite;
      for (const p of path) { step(); amount = Math.min(amount, graph[p.from][p.index].capacity); }
      for (const p of path) { step(); const e = graph[p.from][p.index]; e.capacity -= amount; graph[e.to][e.rev].capacity += amount; }
      flow += amount; augmentations++;
    }
  }
  const selected = Uint8Array.from(levels, v => v >= 0 ? 1 : 0);
  if (tie === 'greatest') {
    const reachesSink = new Uint8Array(count), queue = [sink]; reachesSink[sink] = 1;
    for (let i = 0; i < queue.length; i++) for (const e of graph[queue[i]]) {
      step();
      if (graph[e.to][e.rev].capacity > 0 && !reachesSink[e.to]) { reachesSink[e.to] = 1; queue.push(e.to); }
    }
    for (let i = 0; i < count; i++) selected[i] = 1 - reachesSink[i];
  }
  if (!selected[source] || selected[sink]) fail('length-cut-certificate', 'Cut does not separate source and sink');
  const values = lower.map((v, i) => {
    let x = v, missing = false;
    for (let k = v + 1; k <= upper[i]; k++) {
      step();
      if (selected[threshold(i, k)]) { if (missing) fail('length-cut-certificate', 'Selected thresholds are not a prefix'); x++; } else missing = true;
    }
    return x;
  });
  const exactPrimal = dot(costs, values), exactDual = dot(costs, lower) + costs.reduce((s, c, i) => s + BigInt(Math.min(0, c)) * BigInt(upper[i] - lower[i]), 0n) + BigInt(flow);
  const primal = Number(exactPrimal), dual = Number(exactDual);
  if (!Number.isSafeInteger(primal) || !Number.isSafeInteger(dual) || exactPrimal !== exactDual || inequalities.some(e => values[e.to] < values[e.from] + e.weight))
    fail('length-cut-certificate', 'Minimum-cut coordinates or primal/dual costs failed verification');
  return { values, tie, primal, dual, flow, nodes: count, edges: edgeCount, finite_capacity: finite, phases, augmentations, work,
    build_ms: buildMs, flow_ms: performance.now() - flowStart, elapsed_ms: performance.now() - start };
}
