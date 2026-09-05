// Explicit offline experiment. Blocks are geometry proposals, never validated factory modules.
const fail = (code, entity, message) => { throw Object.assign(new Error(message), { code, entity, stage: 'chain-block-placement' }); };
const cmp = (a, b) => { for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] - b[i]; return 0; };
const ports = (p, transport, direction) => p.catalog_ports.filter(q => q.transport === transport && q.direction === direction);
const shift = (p, dx, dy) => ({ ...p, x: p.x + dx, y: p.y + dy, catalog_ports: p.catalog_ports.map(q => ({ ...q,
  position: { x: q.position.x + dx, y: q.position.y + dy }, connection: { x: q.connection.x + dx, y: q.connection.y + dy } })) });
const bounds = ps => ({ width: Math.max(...ps.map(p => p.x + p.width)) - Math.min(...ps.map(p => p.x)),
  height: Math.max(...ps.map(p => p.y + p.height)) - Math.min(...ps.map(p => p.y)) });
const clear = (a, b, gap) => a.x + a.width + gap <= b.x || b.x + b.width + gap <= a.x
  || a.y + a.height + gap <= b.y || b.y + b.height + gap <= a.y;

export function chainBlocks(input, items, { contract = true, minimumGap = 3, sweeps = 2 } = {}) {
  if (input.schema_version !== 1 || !input.placement_geometry_valid || input.routing_performed
    || items.schema_version !== 1 || !Number.isSafeInteger(minimumGap) || minimumGap < 1 || minimumGap > 16
    || !Number.isSafeInteger(sweeps) || sweeps < 0 || sweeps > 8)
    fail('chain-block-input-invalid', 'input', 'Expected an unrouted v1 proposal, items v1, gap 1..16 and sweeps 0..8');
  const start = performance.now(), source = structuredClone(input);
  const ps = [...source.placements].sort((a, b) => a.label.localeCompare(b.label));
  const byId = new Map(ps.map(p => [p.instance, p]));
  if (byId.size !== ps.length || !ps.length || source.required_facilities !== ps.length || !byId.has(source.target_instance))
    fail('chain-block-membership-invalid', 'placements', 'Missing target or inconsistent/duplicate facilities');
  for (const p of ps) if (![p.x, p.y, p.width, p.height].every(Number.isSafeInteger) || p.width < 1 || p.height < 1
    || !Array.isArray(p.catalog_ports)) fail('chain-block-geometry-invalid', p.instance, 'Invalid facility geometry');
  const itemKind = new Map(items.items.map(i => [i.id, i.transport]));
  const edges = [...source.internal_requirements].sort((a, b) => a.id.localeCompare(b.id));
  const external = source.external_requirements;
  const all = [...edges, ...external].map(e => ({ ...e, transport: itemKind.get(e.item) }));
  if (new Set(all.map(e => e.id)).size !== all.length) fail('chain-block-edge-invalid', 'requirements', 'Duplicate requirement IDs');
  for (const e of all) if (!['belt', 'pipe'].includes(e.transport) || !e.rate
    || ![e.rate.numerator, e.rate.denominator].every(Number.isSafeInteger) || e.rate.numerator <= 0 || e.rate.denominator <= 0
    || !(byId.has(e.source) || byId.has(e.target))) fail('chain-block-edge-invalid', e.id, 'Invalid item, rate or endpoints');
  for (const e of edges) if (!byId.has(e.source) || !byId.has(e.target) || e.transport !== itemKind.get(e.item))
    fail('chain-block-edge-invalid', e.id, 'Internal endpoints or transport do not match runtime items');
  const belt = all.filter(e => e.transport === 'belt');
  const successors = id => new Set(belt.filter(e => e.source === id).map(e => e.target));
  const predecessors = id => new Set(belt.filter(e => e.target === id).map(e => e.source));
  const reachable = (from, to) => {
    const seen = new Set(), queue = [from];
    while (queue.length) { const id = queue.pop(); if (id === to) return true;
      if (seen.has(id)) continue; seen.add(id); queue.push(...edges.filter(e => e.source === id).map(e => e.target)); }
    return false;
  };
  const extraction = edges.map(e => {
    let reason = !contract ? 'singleton-control' : e.transport !== 'belt' ? 'non-belt'
      : successors(e.source).size !== 1 ? 'belt-fan-out' : predecessors(e.target).size !== 1 ? 'belt-fan-in'
        : edges.filter(q => q.source === e.source && q.target === e.target && q.transport === 'belt').length !== 1 ? 'multiple-belt-demands'
          : reachable(e.target, e.source) ? 'directed-cycle' : null;
    if (!reason && (!ports(byId.get(e.source), 'belt', 'output').some(p => p.edge === 'east')
      || !ports(byId.get(e.target), 'belt', 'input').some(p => p.edge === 'west'))) reason = 'non-opposed-belt-ports';
    return { requirement: e.id, source: e.source, target: e.target, contracted: !reason, reason };
  });
  const joined = extraction.filter(e => e.contracted), next = new Map(joined.map(e => [e.source, e.target]));
  const hasPrevious = new Set(joined.map(e => e.target)), blocks = [], owner = new Map();
  for (const p of ps) if (!hasPrevious.has(p.instance)) {
    const members = [], links = [];
    let id = p.instance;
    while (id) {
      const original = byId.get(id), prior = members.at(-1);
      let member = shift(original, -original.x, -original.y);
      if (prior) {
        const output = ports(prior, 'belt', 'output').filter(p => p.edge === 'east');
        const inputPorts = ports(member, 'belt', 'input').filter(p => p.edge === 'west');
        const rows = [...new Set(output.flatMap(a => inputPorts.map(b => a.connection.y - b.connection.y)))];
        const ranked = rows.map(y => ({ y, matches: output.filter(a => inputPorts.some(b => b.connection.y + y === a.connection.y)).length,
          center: Math.abs(prior.y + prior.height / 2 - y - member.height / 2) }))
          .sort((a, b) => b.matches - a.matches || a.center - b.center || a.y - b.y);
        member = shift(member, prior.x + prior.width + 1, ranked[0].y);
        links.push({ requirement: joined.find(e => e.source === prior.instance && e.target === id).requirement,
          aligned_candidate_rows: ranked[0].matches, capacity_validated: false });
      }
      members.push(member); id = next.get(id);
    }
    const minY = Math.min(...members.map(p => p.y));
    const normalized = members.map(p => shift(p, 0, -minY)), box = bounds(normalized);
    const block = { id: `B${String(blocks.length).padStart(2, '0')}`, ...box, members: normalized, links };
    blocks.push(block); for (const m of members) owner.set(m.instance, block.id);
  }
  if (owner.size !== ps.length) fail('chain-block-partition-invalid', 'blocks', 'Every facility must belong to exactly one acyclic chain');
  for (const b of blocks) {
    b.internal_requirements = edges.filter(e => owner.get(e.source) === b.id && owner.get(e.target) === b.id).map(e => e.id);
    b.boundaries = all.filter(e => (owner.get(e.source) === b.id || owner.get(e.target) === b.id)
      && owner.get(e.source) !== owner.get(e.target)).map(e => {
        const output = owner.get(e.source) === b.id, member = b.members.find(p => p.instance === (output ? e.source : e.target));
        return { requirement: e.id, instance: member.instance, direction: output ? 'output' : 'input', item: e.item,
          transport: e.transport, rate: e.rate, other: output ? e.target : e.source,
          candidate_ports: ports(member, e.transport, output ? 'output' : 'input').map(p => p.id) };
      });
    // A controlled heuristic, not a proven number of required routing lanes.
    b.channel = Math.max(minimumGap, Math.ceil(b.boundaries.filter(e => e.transport === 'belt').length / 2));
  }
  const extractionMs = performance.now() - start;
  const blockById = new Map(blocks.map(b => [b.id, b]));
  const cross = edges.filter(e => owner.get(e.source) !== owner.get(e.target));
  const incident = new Map(blocks.map(b => [b.id, cross.filter(e => owner.get(e.source) === b.id || owner.get(e.target) === b.id)]));
  const memberById = new Map(blocks.flatMap(b => b.members.map(p => [p.instance, p])));
  const relativePorts = new Map(cross.map(e => [e.id, [ports(memberById.get(e.source), e.transport, 'output'),
    ports(memberById.get(e.target), e.transport, 'input')]]));
  const placed = new Map(), order = [], decisions = [];
  const rect = (id, at) => ({ ...at, width: blockById.get(id).width, height: blockById.get(id).height });
  function score(id, at, es) {
    let backward = 0, north = 0, distance = 0, maximum = 0;
    for (const e of es) {
      const a = owner.get(e.source) === id ? at : placed.get(owner.get(e.source));
      const b = owner.get(e.target) === id ? at : placed.get(owner.get(e.target));
      if (!a || !b) continue;
      const pa = memberById.get(e.source), pb = memberById.get(e.target);
      const [outputs, inputs] = relativePorts.get(e.id);
      if (e.transport === 'belt' && a.x + pa.x + pa.width > b.x + pb.x) backward++;
      if (e.transport === 'pipe' && inputs.some(p => p.edge === 'north') && a.y + pa.y + pa.height + 1 > b.y + pb.y) north++;
      let d = Infinity;
      for (const p of outputs) for (const q of inputs) d = Math.min(d,
        Math.abs(a.x + p.connection.x - b.x - q.connection.x) + Math.abs(a.y + p.connection.y - b.y - q.connection.y));
      if (!Number.isFinite(d)) d = Math.abs(a.x + pa.x + pa.width / 2 - b.x - pb.x - pb.width / 2)
        + Math.abs(a.y + pa.y + pa.height / 2 - b.y - pb.y - pb.height / 2);
      distance += d; maximum = Math.max(maximum, d);
    }
    const box = bounds([...placed].filter(([key]) => key !== id).map(([key, value]) => rect(key, value)).concat(rect(id, at)));
    return [backward, north, distance, maximum, box.width * box.height];
  }
  function choose(id, es) {
    const block = blockById.get(id), old = placed.get(id), xs = new Set([old?.x ?? 0]), ys = new Set([old?.y ?? 0]);
    for (const e of es) {
      const ownOutput = owner.get(e.source) === id, otherId = owner.get(ownOutput ? e.target : e.source);
      const peer = placed.get(otherId); if (!peer) continue;
      const other = blockById.get(otherId), gap = Math.max(block.channel, other.channel);
      xs.add(peer.x - block.width - gap); xs.add(peer.x + other.width + gap); xs.add(peer.x);
      ys.add(peer.y - block.height - gap); ys.add(peer.y + other.height + gap);
      const [op, ip] = relativePorts.get(e.id), own = ownOutput ? op : ip, otherPorts = ownOutput ? ip : op;
      for (const a of own) for (const b of otherPorts) ys.add(peer.y + b.connection.y - a.connection.y);
    }
    for (const [otherId, peer] of placed) if (otherId !== id) {
      const other = blockById.get(otherId), gap = Math.max(block.channel, other.channel);
      ys.add(peer.y - block.height - gap); ys.add(peer.y + other.height + gap);
    }
    let best = null, count = 0;
    for (const x of xs) for (const y of ys) {
      const at = { x, y }, shape = rect(id, at);
      if (![...placed].every(([key, pos]) => key === id || clear(shape, rect(key, pos), Math.max(block.channel, blockById.get(key).channel)))) continue;
      count++;
      const s = score(id, at, es), rank = [...s, Math.abs(x) + Math.abs(y), y, x];
      if (!best || cmp(rank, best.rank) < 0) best = { at, score: s, rank };
    }
    if (!best) fail('chain-block-candidates-exhausted', id, 'No free block translation among the finite geometric events');
    return { ...best, candidates: count };
  }
  const target = owner.get(source.target_instance);
  placed.set(target, { x: 0, y: 0 }); order.push(target);
  while (placed.size < blocks.length) {
    const pending = blocks.filter(b => !placed.has(b.id));
    const links = b => incident.get(b.id).filter(e => placed.has(owner.get(e.source)) || placed.has(owner.get(e.target))).length;
    pending.sort((a, b) => links(b) - links(a)
      || b.boundaries.filter(e => e.transport === 'pipe' && e.direction === 'output').length - a.boundaries.filter(e => e.transport === 'pipe' && e.direction === 'output').length
      || incident.get(b.id).length - incident.get(a.id).length || a.id.localeCompare(b.id));
    const id = pending[0].id, chosen = choose(id, incident.get(id));
    placed.set(id, chosen.at); order.push(id); decisions.push({ block: id, stage: 'growth', ...chosen });
  }
  const growthMs = performance.now() - start - extractionMs;
  let relocations = 0;
  for (let pass = 0; pass < sweeps; pass++) {
    let changes = 0;
    for (const id of order) {
      const chosen = choose(id, incident.get(id)), previous = score(id, placed.get(id), incident.get(id));
      // Strict score improvement keeps equal-score placements stable.
      if (cmp(chosen.score, previous) < 0) {
        placed.set(id, chosen.at); decisions.push({ block: id, stage: 'relocation', pass, previous_score: previous, ...chosen }); changes++;
      }
    }
    relocations += changes; if (!changes) break;
  }
  const minX = Math.min(...[...placed.values()].map(p => p.x)), minY = Math.min(...[...placed.values()].map(p => p.y));
  const placements = order.flatMap(id => {
    const at = placed.get(id); return [...blockById.get(id).members].reverse().map(p => ({ ...shift(p, at.x - minX, at.y - minY), block: id }));
  }).map((p, placement_index) => ({ ...p, placement_index }));
  const world = new Map(placements.map(p => [p.instance, p]));
  for (let i = 0; i < placements.length; i++) for (let j = i + 1; j < placements.length; j++)
    if (!clear(placements[i], placements[j], 1)) fail('chain-block-clearance-invalid', placements[i].instance, 'Facility clearance failed');
  const outputBlocks = blocks.map(b => ({ ...b, x: placed.get(b.id).x - minX, y: placed.get(b.id).y - minY,
    members: b.members.map(p => p.instance) }));
  const bnds = bounds(placements);
  const nonForward = edges.filter(e => world.get(e.source).x + world.get(e.source).width > world.get(e.target).x).map(e => e.id);
  const diagnostics = placements.filter(p => Object.values(p.alignment).some(g => g.aligned < g.required)).map(p => ({
    code: 'chain-block-mixed-port-faces', stage: 'chain-block-placement', severity: 'warning', entity: p.instance,
    message: 'Some required interfaces do not face east/west. Side ports remain unassigned obligations.' }));
  for (const e of all) for (const [id, direction] of [[e.source, 'output'], [e.target, 'input']])
    if (world.has(id) && !ports(world.get(id), e.transport, direction).length) diagnostics.push({
      code: 'chain-block-missing-port-domain', stage: 'chain-block-placement', severity: 'warning', entity: e.id,
      message: 'No catalog candidate for this endpoint. Placement may use a center-distance fallback; routing remains untested.' });
  return { ...source, placements, bounds: bnds, placement_ms: performance.now() - start,
    placement_decisions: [], pipe_relations: source.pipe_relations.map(e => ({ ...e,
      supplier_north: world.get(e.source).y + world.get(e.source).height + 1 <= world.get(e.target).y })),
    non_forward_requirements: nonForward, diagnostics,
    policy: { flow_direction: 'east', orientation_priority: 'frozen-catalog-belt-before-pipe',
      traversal: 'block-adjacency-most-connected-first', port_assignment: 'none', empty_cell_gap: 1,
      block_channel_minimum: minimumGap, row_alignment: 'straight-chain-port-alignment' },
    block_experiment: { schema_version: 1, contract, minimum_gap: minimumGap, sweeps, relocations,
      score_order: ['backward-belt-edges', 'north-pipe-violations', 'endpoint-distance-sum', 'maximum-endpoint-distance', 'facility-area'],
      capacity_validated: false, blocks: outputBlocks, extraction, decisions,
      timings: { extraction_ms: extractionMs, growth_ms: growthMs, relocation_ms: performance.now() - start - extractionMs - growthMs },
      summary: { blocks: blocks.length, chains: blocks.filter(b => b.members.length > 1).length,
        contracted_edges: joined.length, chain_facilities: blocks.filter(b => b.members.length > 1).reduce((n, b) => n + b.members.length, 0),
        facility_area: bnds.width * bnds.height, non_forward_requirements: nonForward.length } } };
}
