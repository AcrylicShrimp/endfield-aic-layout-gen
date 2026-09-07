import { extractChains } from '../constructive-local-reembedding/model.mjs';

const key = (x, y) => `${x},${y}`;

export function largestEmptyRectangle(occupied, bounds) {
  const heights = Array(bounds.width).fill(0);
  let best = { x: bounds.minX, y: bounds.minY, width: 0, height: 0, area: 0 };
  for (let y = bounds.minY; y <= bounds.maxY; y++) {
    for (let x = 0; x < bounds.width; x++) heights[x] = occupied.has(key(bounds.minX + x, y)) ? 0 : heights[x] + 1;
    const stack = [];
    for (let x = 0; x <= bounds.width; x++) {
      const h = x === bounds.width ? 0 : heights[x]; let left = x;
      while (stack.length && stack.at(-1).height > h) {
        const r = stack.pop(); left = r.left;
        const candidate = { x: bounds.minX + r.left, y: y - r.height + 1, width: x - r.left, height: r.height, area: (x - r.left) * r.height };
        if (candidate.area > best.area || candidate.area === best.area && (candidate.y < best.y || candidate.y === best.y && candidate.x < best.x)) best = candidate;
      }
      if (h && (!stack.length || stack.at(-1).height < h)) stack.push({ left, height: h });
    }
  }
  return best;
}

export function density(witness, catalogs, { cellBudget = 1000000 } = {}) {
  if (!Number.isSafeInteger(cellBudget) || cellBudget <= 0) throw new Error('Invalid density cell budget');
  const sets = { facility: new Set(), belt: new Set(), pipe: new Set(), component: new Set() };
  const b = { minX: Infinity, maxX: -Infinity, minY: Infinity, maxY: -Infinity };
  const extend = (x, y) => {
    if (!Number.isSafeInteger(x) || !Number.isSafeInteger(y)) throw new Error('Noninteger occupied coordinate');
    b.minX = Math.min(b.minX, x); b.maxX = Math.max(b.maxX, x); b.minY = Math.min(b.minY, y); b.maxY = Math.max(b.maxY, y);
    const area = (b.maxX - b.minX + 1) * (b.maxY - b.minY + 1);
    if (!Number.isSafeInteger(area) || area > cellBudget) throw new Error(`Density diagnostic area budget exceeded: ${area} > ${cellBudget}`);
  };
  const add = (set, x, y) => { extend(x, y); set.add(key(x, y)); };
  const body = (set, p) => {
    if (![p.width, p.height].every(n => Number.isSafeInteger(n) && n > 0) || p.width * p.height > cellBudget) throw new Error('Invalid or over-budget body');
    extend(p.x, p.y); extend(p.x + (p.width - 1), p.y + (p.height - 1));
    for (let dy = 0; dy < p.height; dy++) for (let dx = 0; dx < p.width; dx++) add(set, p.x + dx, p.y + dy);
  };
  for (const p of witness.placements) body(sets.facility, p);
  for (const n of witness.transport_networks) {
    if (!['belt', 'pipe'].includes(n.transport)) throw new Error(`Unknown transport: ${n.transport}`);
    for (const p of n.cells) add(sets[n.transport], p.x, p.y);
  }
  for (const c of witness.logistics_components) {
    const spec = catalogs.components.components.find(v => v.id === c.component);
    if (!spec || !spec.allowed_rotations.includes(c.rotation)) throw new Error(`Unknown component/rotation: ${c.component}/${c.rotation}`);
    const turn = c.rotation % 180 !== 0;
    body(sets.component, { ...c.position, width: spec.footprint[turn ? 'height' : 'width'], height: spec.footprint[turn ? 'width' : 'height'] });
  }
  if (b.minX === Infinity) throw new Error('Cannot measure an empty witness');
  const bounds = { ...b, width: b.maxX - b.minX + 1, height: b.maxY - b.minY + 1 }, area = bounds.width * bounds.height;
  if (!Number.isSafeInteger(area) || area > cellBudget) throw new Error(`Density diagnostic area budget exceeded: ${area} > ${cellBudget}`);
  const occupied = new Set(Object.values(sets).flatMap(s => [...s])), empty = area - occupied.size;
  return { definition: 'all-unused-xy-cells-inside-used-bounding-box-v1', bounds, area, occupied_xy_cells: occupied.size,
    empty_xy_cells: empty, empty_fraction: empty / area, percent_empty: 100 * empty / area,
    facility_xy_cells: sets.facility.size, facility_fraction: sets.facility.size / area,
    belt_xy_cells: sets.belt.size, pipe_xy_cells: sets.pipe.size, component_xy_cells: sets.component.size,
    transport_layer_cells: sets.belt.size + sets.pipe.size, belt_pipe_overlap_xy: [...sets.belt].filter(k => sets.pipe.has(k)).length,
    largest_empty_rectangle: largestEmptyRectangle(occupied, bounds), density_gate: empty * 100 <= area,
    optimality_certified: false };
}

export function mechanicalWaste(episode) {
  const seen = new Set(), counts = { rejected: 0, unchanged: 0, revisited: 0, novel_valid: 0 };
  if (episode.trace[0]?.parent_sha256) seen.add(episode.trace[0].parent_sha256);
  for (const t of episode.trace) {
    if (!t.accepted) counts.rejected++;
    else if (!t.witness_sha256) throw new Error('Accepted step lacks a witness identity');
    else if (t.witness_sha256 === t.parent_sha256) counts.unchanged++;
    else if (seen.has(t.witness_sha256)) counts.revisited++;
    else counts.novel_valid++;
    if (t.accepted) seen.add(t.witness_sha256);
  }
  const proposals = episode.work.proposals;
  if (proposals !== episode.trace.length) throw new Error('Trace and charged proposal counts disagree');
  const futile = counts.rejected + counts.unchanged + counts.revisited;
  return { definition: 'rejected-unchanged-or-exact-state-revisit-v1', proposals, ...counts, futile,
    futile_fraction: proposals ? futile / proposals : null, zero_action_evidence: proposals === 0,
    valid_worse_transitions: episode.work.worse_transitions, improvements: episode.work.improvements,
    note: 'Valid temporary deterioration is not automatically futile; filtering work must be reported separately.' };
}

export function routeDiagnostics(witness) {
  try {
    const { chains } = extractChains(witness);
    const rows = chains.map(c => { const first = c.cells[0], last = c.cells.at(-1);
      const manhattan = Math.abs(first.x - last.x) + Math.abs(first.y - last.y), segments = c.cells.length - 1;
      return { chain: c.id, network: witness.transport_networks[c.network].id, transport: c.transport, segments,
        endpoint_manhattan: manhattan, geometric_excess: segments - manhattan }; });
    return { status: 'measured', segments: rows.reduce((n,r) => n+r.segments, 0),
      geometric_excess: rows.reduce((n,r) => n+r.geometric_excess, 0),
      largest_excess: rows.sort((a,b) => b.geometric_excess-a.geometric_excess || a.chain-b.chain).slice(0, 10),
      redundancy_proven: false, note: 'Obstacle-free distance ignores required obstacles and arms; excess is not proof of redundant wiring.' };
  } catch (e) { return { status: 'diagnostic-unsupported', code: e.code ?? 'route-metric-failed', message: e.message, redundancy_proven: false }; }
}
