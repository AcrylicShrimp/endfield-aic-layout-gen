import { footprint, collides, blockedAt } from '../constructive-cluster-packing/nesting.mjs';
import { placeRegions } from '../constructive-block-composition/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { diverse } from '../constructive-shape-packing/model.mjs';
import { regionInterface } from './interface.mjs';
import { readyAtoms } from './project.mjs';
const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), { code, message, entity, stage: 'scc-assembly' }); };
const compare = (a, b) => a.area - b.area || a.tiles - b.tiles || a.turns - b.turns || a.signature.localeCompare(b.signature);
const moveDemands = (gs, x, y) => gs.map(g => ({ ...g, ports: g.ports.map(p => ({ ...p, x: p.x + x, y: p.y + y })) }));
const free = (g, f, dx, dy) => g.ports.filter(p => !blockedAt(f, p.x - dx, p.y - dy, g.transport));

function merge(a, b, dx, dy) {
  const x = Math.min(0, dx), y = Math.min(0, dy), width = Math.max(a.width, dx + b.width) - x, height = Math.max(a.height, dy + b.height) - y;
  if (width > 1024 || height > 1024) fail('scc-packing-row-budget', 'Diagnostic occupied-row extent exceeds 1024; not a factory limit');
  const rows = Array.from({ length: height }, () => ({ body: 0n, belt: 0n, pipe: 0n }));
  for (const [f, ox, oy] of [[a, -x, -y], [b, dx - x, dy - y]]) for (let i = 0; i < f.height; i++)
    for (const layer of ['body', 'belt', 'pipe']) rows[i + oy][layer] |= f.rows[i][layer] << BigInt(ox);
  return { x: 0, y: 0, width, height, rows };
}

export function prepareBanks(graph, banks, catalogs) {
  if (!banks.length || new Set(banks.map(b => b.id)).size !== banks.length) fail('scc-bank-schema', 'Expected nonempty unique banks');
  const owners = new Map();
  return banks.map(bank => {
    if (!bank.required_members.length || !bank.variants.length) fail('scc-bank-schema', 'Every bank needs a mandatory seed and alternatives', bank.id);
    const variants = bank.variants.map((v, index) => {
      const members = v.detail.witness.placements.map(p => p.instance), own = new Set(members);
      if (own.size !== members.length || bank.required_members.some(id => !own.has(id)) || members.some(id => !graph.nodes.some(n => n.id === id)))
        fail('scc-variant-members', 'Variant must contain its seed exactly once and only known members', bank.id);
      if (graph.edges.some(e => own.has(e.target) && !own.has(e.source))) fail('scc-variant-open-input', 'Variant must contain all input predecessors', bank.id);
      for (const id of members) {
        if (owners.has(id) && owners.get(id) !== bank.id) fail('scc-bank-owner-overlap', 'Optional owners must be disjoint across banks', id);
        owners.set(id, bank.id);
      }
      const boundary = regionInterface(graph, v.detail.witness, catalogs), f = footprint(v.detail.witness), score = witnessScore(v.detail.witness);
      if (!boundary.accepted) fail('scc-variant-interface', 'Source variant has no usable pending interface', { bank: bank.id, diagnostics: boundary.diagnostics });
      const demands = boundary.access.map(g => ({ instance: g.instance, transport: g.transport, required: g.required,
        ports: g.ports.filter(p => !p.obstruction).map(p => ({ id: p.port, x: p.connection.x - f.x, y: p.connection.y - f.y })) }));
      return { ...v, index, members, f, demands, score };
    });
    return { ...bank, variants };
  });
}

// Interleave low/high coverage strata so a partial-area beam cannot discard all
// larger extensions. This is a bounded portfolio policy, not a dominance proof.
export function coverageBeam(candidates, limit) {
  const groups = new Map();
  for (const c of candidates) { if (!groups.has(c.covered)) groups.set(c.covered, []); groups.get(c.covered).push(c); }
  for (const [n, cs] of groups) groups.set(n, diverse(cs, limit));
  const counts = [...groups.keys()].sort((a, b) => a - b), order = [];
  while (counts.length) { order.push(counts.shift()); if (counts.length) order.push(counts.pop()); }
  const result = [];
  while (result.length < limit) {
    let added = false;
    for (const n of order) if (groups.get(n).length) { result.push(groups.get(n).shift()); added = true; if (result.length === limit) break; }
    if (!added) break;
  }
  return result;
}

export function assemble(graph, choices, catalogs) {
  const regions = choices.map(c => ({ id: c.region, detail: c.variant.detail }));
  const minX = Math.min(...choices.map(c => c.x + c.variant.f.x)), minY = Math.min(...choices.map(c => c.y + c.variant.f.y));
  const normalized = choices.map(c => ({ ...c, x: c.x + 1 - minX, y: c.y + 1 - minY }));
  const placed = placeRegions(regions, normalized.map(c => ({ region: c.region, x: c.x, y: c.y })), catalogs);
  const members = placed.witness.placements.map(p => p.instance), own = new Set(members);
  return { schema_version: 1, case: 'actual-graph-cluster-packing', embedding: 'scc-variable-membership', accepted: false, factory_validated: false,
    witness: placed.witness, catalog_ports: placed.ports, labels: Object.fromEntries(graph.nodes.map(n => [n.id, n.label])), port_groups: { sources: [], targets: [] },
    block: { id: 'scc-assembly', members, pending_edges: graph.edges.filter(e => own.has(e.source) !== own.has(e.target)) },
    assembly_plan: { choices: normalized.map(c => ({ region: c.region, variant: c.variant.index, directory: c.variant.directory, x: c.x, y: c.y, members: c.variant.members })) } };
}

export function packBanks(graph, banks, catalogs, { beamWidth = 8, limit = 8, reverse = false } = {}) {
  if (![beamWidth, limit].every(n => Number.isSafeInteger(n) && n >= 1 && n <= 32)) fail('scc-packing-budget', 'Expected beam/candidate limit in 1..32');
  const prepared = prepareBanks(graph, banks, catalogs), start = performance.now();
  const order = [...prepared].sort((a, b) => Math.max(...b.variants.map(v => v.f.height)) - Math.max(...a.variants.map(v => v.f.height)) || a.id.localeCompare(b.id));
  if (reverse) order.reverse();
  const stats = { tested_translations: 0, collisions: 0, output_obstructions: 0, stages: [], order: order.map(b => b.id), beam_width: beamWidth, limit };
  let beam = [{ f: { width: 0, height: 0, rows: [] }, choices: [], demands: [], covered: 0, tiles: 0, turns: 0 }];
  for (const bank of order) {
    const buckets = new Map();
    for (const state of beam) for (const variant of bank.variants) {
      const a = state.f, b = variant.f, initial = !state.choices.length;
      for (let dy = initial ? 0 : -b.height - 1; dy <= (initial ? 0 : a.height + 1); dy++)
        for (let dx = initial ? 0 : -b.width - 1; dx <= (initial ? 0 : a.width + 1); dx++) {
          stats.tested_translations++;
          if (collides(a, b, dx, dy)) { stats.collisions++; continue; }
          if (state.demands.some(g => free(g, b, dx, dy).length < g.required) || variant.demands.some(g => free(g, a, -dx, -dy).length < g.required)) { stats.output_obstructions++; continue; }
          const ox = Math.min(0, dx), oy = Math.min(0, dy), width = Math.max(a.width, dx + b.width) - ox, height = Math.max(a.height, dy + b.height) - oy;
          const covered = state.covered + variant.members.length, area = width * height, tiles = state.tiles + variant.score.physical_tiles, turns = state.turns + variant.score.turns;
          const bucketKey = `${covered}:${width}:${height}`, bucket = buckets.get(bucketKey) ?? [];
          if (bucket.length === beamWidth && (area > bucket.at(-1).area || area === bucket.at(-1).area && tiles > bucket.at(-1).tiles)) continue;
          const choices = [...state.choices.map(c => ({ ...c, x: c.x - ox, y: c.y - oy })), { region: bank.id, variant, x: dx - ox - b.x, y: dy - oy - b.y }];
          const signature = JSON.stringify(choices.map(c => [c.region, c.variant.index, c.x, c.y]));
          if (bucket.some(c => c.signature === signature)) continue;
          bucket.push({ area, width, height, covered, tiles, turns, signature, choices, state, variant, dx, dy }); bucket.sort(compare);
          if (bucket.length > beamWidth) bucket.pop(); buckets.set(bucketKey, bucket);
        }
    }
    const target = bank === order.at(-1) ? limit : beamWidth, screenBudget = target * 16;
    const ranked = coverageBeam([...buckets.values()].flat(), screenBudget), selected = [], rejections = []; let checked = 0;
    for (const c of ranked) {
      checked++;
      const detail = assemble(graph, c.choices, catalogs), boundary = regionInterface(graph, detail.witness, catalogs);
      if (!boundary.accepted) { if (rejections.length < 4) rejections.push({ covered: c.covered, area: c.area, diagnostics: boundary.diagnostics }); continue; }
      selected.push(c); if (selected.length === target) break;
    }
    stats.stages.push({ bank: bank.id, envelope_buckets: buckets.size, boundary_checked: checked, boundary_rejected: checked - selected.length,
      boundary_budget: screenBudget, rejections, retained: selected.map(c => ({ covered: c.covered, area: c.area, width: c.width, height: c.height })) });
    beam = selected.map(c => {
      const { state, variant, dx, dy, ...rest } = c, ox = Math.min(0, dx), oy = Math.min(0, dy);
      return { ...rest, f: merge(state.f, variant.f, dx, dy), demands: [
        ...moveDemands(state.demands.map(g => ({ ...g, ports: free(g, variant.f, dx, dy) })), -ox, -oy),
        ...moveDemands(variant.demands.map(g => ({ ...g, ports: free(g, state.f, -dx, -dy) })), dx - ox, dy - oy)] };
    });
    if (!beam.length) break;
  }
  stats.elapsed_ms = performance.now() - start;
  return { prepared, search: stats, candidates: beam.filter(c => c.choices.length === banks.length).map(c => ({ detail: assemble(graph, c.choices, catalogs), choices: c.choices })) };
}

export function atomicUpgrades(graph, choices, banks, catalogs, limit = 4) {
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 32) fail('scc-atomic-budget', 'Expected atomic candidate limit in 1..32');
  const members = choices.flatMap(c => c.variant.members), own = new Set(members), atoms = readyAtoms(graph, members).filter(ids =>
    ids.length > 1 || graph.edges.some(e => e.internal && e.source === ids[0] && e.target === ids[0])), candidates = [], failures = [];
  for (const atom of atoms) {
    let templates = 0;
    for (const old of choices) for (const variant of banks.find(b => b.id === old.region).variants) {
      const added = variant.members.filter(id => !own.has(id));
      if (added.length !== atom.length || added.some(id => !atom.includes(id)) || old.variant.members.some(id => !variant.members.includes(id))) continue;
      templates++;
      const others = choices.filter(c => c !== old);
      // Use other regions' actual coordinates, not the normalized diagnostic copy.
      const otherW = others.length ? placeRegions(others.map(c => ({ id: c.region, detail: c.variant.detail })), others.map(c => ({ region: c.region, x: c.x, y: c.y })), catalogs).witness : null;
      const f = otherW ? footprint(otherW) : { x: 0, y: 0, width: 0, height: 0, rows: [] }, b = variant.f;
      const local = [];
      for (let y = f.y - b.height - 1; y <= f.y + f.height + 1; y++) for (let x = f.x - b.width - 1; x <= f.x + f.width + 1; x++) {
        if (collides(f, b, x - f.x, y - f.y)) continue;
        const width = Math.max(f.x + f.width, x + b.width) - Math.min(f.x, x), height = Math.max(f.y + f.height, y + b.height) - Math.min(f.y, y);
        local.push({ width, height, area: width * height, tiles: variant.score.physical_tiles, turns: variant.score.turns, signature: `${x},${y}`, x: x - b.x, y: y - b.y });
      }
      local.sort(compare);
      let accepted = 0;
      for (const p of local) {
        const next = choices.map(c => c === old ? { ...c, variant, x: p.x, y: p.y } : c), detail = assemble(graph, next, catalogs), boundary = regionInterface(graph, detail.witness, catalogs);
        if (!boundary.accepted) continue;
        detail.atomic_upgrade = { bank: old.region, added: atom, retained_members: old.variant.members };
        candidates.push({ detail, choices: next }); if (++accepted === limit) break;
      }
    }
    if (!templates) failures.push({ code: 'scc-atomic-template-missing', stage: 'scc-assembly', entity: atom });
  }
  return { atoms, candidates, failures };
}
