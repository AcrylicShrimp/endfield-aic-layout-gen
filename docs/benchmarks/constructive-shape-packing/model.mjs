import { footprint, collides } from '../constructive-cluster-packing/nesting.mjs';
import { project } from '../constructive-factory-closure/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { add, rate, parseRate, capacity, lanesFor } from '../constructive-interconnect/rates.mjs';
const fail = (code, message) => { throw Object.assign(new Error(message), { code, severity: 'error', stage: 'shape-packing' }); };
const compare = (a, b) => a.area - b.area || a.tiles - b.tiles || a.turns - b.turns ||
  Math.max(a.width, a.height) - Math.max(b.width, b.height) || a.signature.localeCompare(b.signature);

export function fillMetrics(w) {
  const f = footprint(w); let occupied = 0, bodies = 0;
  const count = bits => { let n = 0; while (bits) { bits &= bits - 1n; n++; } return n; };
  for (const row of f.rows) { occupied += count(row.body | row.belt | row.pipe); bodies += count(row.body); }
  return { width: f.width, height: f.height, area: f.width * f.height, occupied_xy: occupied, facility_xy: bodies,
    fill_ratio: occupied / (f.width * f.height), empty_xy: f.width * f.height - occupied };
}

export function growthCut(retained, candidate, threshold) {
  if (!Number.isFinite(threshold) || threshold < 0 || threshold > 1) fail('shape-fill-threshold', 'Expected fill threshold in 0..1');
  const next = fillMetrics(candidate);
  if (retained === null || next.fill_ratio >= threshold) return null;
  return { code: 'shape-growth-fill-cut', severity: 'info', stage: 'cluster-growth',
    message: 'Retain the previous routed region; defer this frontier to post-packing closure', threshold,
    retained_fill: fillMetrics(retained), candidate_fill: next };
}

export function clearEast(f, p, dx = 0, dy = 0) {
  const y = p.y - dy, x = p.x - dx;
  if (y < 0 || y >= f.height || x >= f.width) return true;
  return ((f.rows[y].body | f.rows[y].belt) >> BigInt(Math.max(0, x))) === 0n;
}

export function outputDemands(graph, region, catalogs) {
  const w = region.detail.witness, f = footprint(w), members = new Set(w.placements.map(p => p.instance)), ports = project(w, catalogs);
  const used = new Set(w.transport_networks.flatMap(n => n.terminals).filter(t => t.endpoint.kind === 'facility')
    .map(t => JSON.stringify([t.endpoint.instance, t.endpoint.port]))), groups = new Map();
  for (const e of graph.edges.filter(e => members.has(e.source) && !members.has(e.target))) {
    if (e.transport !== 'belt') fail('shape-pending-transport', 'This structured source bank requires belt-only pending outputs');
    if (!groups.has(e.source)) groups.set(e.source, new Map());
    const items = groups.get(e.source); items.set(e.item, add(items.get(e.item) ?? rate(0), parseRate(e.rate)));
  }
  const cap = capacity(catalogs.transports.transports.find(t => t.kind === 'belt').capacity);
  return [...groups].map(([instance, items]) => ({ instance,
    required: [...items.values()].reduce((s, value) => s + lanesFor(value, cap), 0),
    ports: ports[instance].filter(p => p.direction === 'output' && p.transport === 'belt' && p.edge === 'east' && !used.has(JSON.stringify([instance, p.id])))
      .map(p => ({ id: p.id, x: p.connection.x - f.x, y: p.connection.y - f.y })).filter(p => clearEast(f, p)) }));
}

// Preserve distinct envelope shapes before filling the remaining heuristic beam.
export function diverse(candidates, limit) {
  const sorted = [...candidates].sort(compare), selected = [], dimensions = new Set(), signatures = new Set();
  for (const c of sorted) {
    const d = `${c.width},${c.height}`;
    if (dimensions.has(d) || signatures.has(c.signature)) continue;
    dimensions.add(d); signatures.add(c.signature); selected.push(c); if (selected.length === limit) return selected;
  }
  for (const c of sorted) {
    if (signatures.has(c.signature)) continue;
    signatures.add(c.signature); selected.push(c); if (selected.length === limit) break;
  }
  return selected;
}

function merged(a, b, dx, dy) {
  const x = Math.min(0, dx), y = Math.min(0, dy), width = Math.max(a.width, dx + b.width) - x, height = Math.max(a.height, dy + b.height) - y;
  if (width > 1024 || height > 1024) fail('shape-row-budget', 'Merged experimental row extent exceeds 1024; not a factory limit');
  const rows = Array.from({ length: height }, () => ({ body: 0n, belt: 0n, pipe: 0n }));
  for (const [f, ox, oy] of [[a, -x, -y], [b, dx - x, dy - y]])
    for (let i = 0; i < f.height; i++) for (const layer of ['body', 'belt', 'pipe']) rows[i + oy][layer] |= f.rows[i][layer] << BigInt(ox);
  return { x: 0, y: 0, width, height, rows };
}
const moveGroups = (groups, dx, dy) => groups.map(g => ({ ...g, ports: g.ports.map(p => ({ ...p, x: p.x + dx, y: p.y + dy })) }));

export function packShapes(graph, banks, catalogs, { beamWidth = 8, limit = 8 } = {}) {
  if (!banks.length || banks.length > 8 || new Set(banks.map(b => b.id)).size !== banks.length || banks.some(b => !b.variants.length || b.variants.length > 32))
    fail('shape-bank-schema', 'Expected 1..8 unique region banks with 1..32 concrete variants each');
  if (![beamWidth, limit].every(n => Number.isSafeInteger(n) && n >= 1 && n <= 32)) fail('shape-search-budget', 'Expected beam and candidate limits 1..32');
  const start = performance.now(), owner = new Set();
  const prepared = banks.map(bank => {
    const members = bank.variants[0].detail.witness.placements.map(p => p.instance).sort();
    if (members.some(id => owner.has(id)) || new Set(members).size !== members.length) fail('shape-duplicate-owner', 'Region banks must have disjoint facility ownership');
    members.forEach(id => owner.add(id));
    const variants = bank.variants.map((r, index) => {
      if (JSON.stringify(r.detail.witness.placements.map(p => p.instance).sort()) !== JSON.stringify(members)) fail('shape-variant-members', 'Every shape must retain identical region membership');
      return { ...r, id: bank.id, index, f: footprint(r.detail.witness), demands: outputDemands(graph, r, catalogs), score: witnessScore(r.detail.witness) };
    });
    return { id: bank.id, variants: variants.filter(v => v.demands.every(g => g.ports.length >= g.required)) };
  });
  const stats = { tested_translations: 0, collisions: 0, output_obstructions: 0, collision_access_free: 0, beam_width: beamWidth, limit,
    input_variants: banks.map(b => ({ region: b.id, count: b.variants.length,
      shapes: b.variants.map(v => ({ directory: v.directory, ...fillMetrics(v.detail.witness) })) })), stages: [] };
  let beam = [{ f: { width: 0, height: 0, rows: [] }, choices: [], demands: [], tiles: 0, turns: 0, signature: '' }];
  for (let depth = 0; depth < banks.length; depth++) {
    const children = [];
    for (const state of beam) for (const bank of prepared.filter(b => !state.choices.some(c => c.region === b.id))) {
      const local = [];
      for (const variant of bank.variants) {
        const a = state.f, b = variant.f;
        const minDx = depth ? -b.width : 0, maxDx = depth ? a.width : 0;
        const minDy = depth ? -b.height : 0, maxDy = depth ? a.height : 0;
        for (let dy = minDy; dy <= maxDy; dy++) for (let dx = minDx; dx <= maxDx; dx++) {
          stats.tested_translations++;
          if (depth && collides(a, b, dx, dy)) { stats.collisions++; continue; }
          if (state.demands.some(g => g.ports.filter(p => clearEast(b, p, dx, dy)).length < g.required) ||
              variant.demands.some(g => g.ports.filter(p => clearEast(a, p, -dx, -dy)).length < g.required)) { stats.output_obstructions++; continue; }
          stats.collision_access_free++;
          const ox = Math.min(0, dx), oy = Math.min(0, dy), width = Math.max(a.width, dx + b.width) - ox, height = Math.max(a.height, dy + b.height) - oy;
          const choices = [...state.choices.map(c => ({ ...c, x: c.x - ox, y: c.y - oy })), { region: bank.id, variant: variant.index, x: dx - ox - b.x, y: dy - oy - b.y }];
          const signature = JSON.stringify([...choices].sort((a, b) => a.region.localeCompare(b.region)));
          local.push({ width, height, area: width * height, tiles: state.tiles + variant.score.physical_tiles, turns: state.turns + variant.score.turns,
            signature, choices, state, variant, dx, dy });
        }
      }
      children.push(...diverse(local, beamWidth));
    }
    const winners = diverse(children, depth === banks.length - 1 ? limit : beamWidth);
    stats.stages.push({ depth: depth + 1, shortlisted: children.length, retained: winners.length });
    beam = winners.map(c => {
      const { state, variant, dx, dy, ...rest } = c, ox = Math.min(0, dx), oy = Math.min(0, dy);
      const demands = [...moveGroups(state.demands.map(g => ({ ...g, ports: g.ports.filter(p => clearEast(variant.f, p, dx, dy)) })), -ox, -oy),
        ...moveGroups(variant.demands.map(g => ({ ...g, ports: g.ports.filter(p => clearEast(state.f, p, -dx, -dy)) })), dx - ox, dy - oy)];
      return { ...rest, f: merged(state.f, variant.f, dx, dy), demands };
    });
    if (!beam.length) break;
  }
  stats.elapsed_ms = performance.now() - start;
  return { search: stats, candidates: beam.filter(s => s.choices.length === banks.length).map(s => ({
    regions: s.choices.map(c => ({ id: c.region, detail: banks.find(b => b.id === c.region).variants[c.variant].detail })),
    offsets: s.choices.map(({ variant, ...c }) => c),
    plan: { family: 'joint-occupied-shapes', choices: s.choices.map(c => ({ ...c, directory: banks.find(b => b.id === c.region).variants[c.variant].directory })),
      partial_score: [s.area, s.tiles, s.turns], width: s.width, height: s.height } })) };
}
