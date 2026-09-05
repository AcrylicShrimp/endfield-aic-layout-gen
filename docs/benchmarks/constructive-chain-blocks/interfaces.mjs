// Exact rate arithmetic over an existing logical graph; no inferred recipe or transport data.
const gcd = (a, b) => b ? gcd(b, a % b) : a;
const normalize = (n, d) => { const g = gcd(n, d); return [n / g, d / g]; };
const rate = r => {
  if (![r?.numerator, r?.denominator].every(Number.isSafeInteger) || r.numerator <= 0 || r.denominator <= 0)
    throw new Error('Expected a positive safe-integer rate');
  return [BigInt(r.numerator), BigInt(r.denominator)];
};
const add = ([a, b], [c, d]) => normalize(a * d + c * b, b * d);
const jsonRate = ([n, d]) => {
  if (n > BigInt(Number.MAX_SAFE_INTEGER) || d > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('Aggregated rate exceeds JSON safe-integer range');
  return { numerator: Number(n), denominator: Number(d) };
};
export function interfaceMapping(placement, items, transports) {
  if (!placement.block_experiment || items.schema_version !== 1 || transports.schema_version !== 1)
    throw new Error('Expected block placement, runtime items v1 and transports v1');
  const facilities = new Map(placement.placements.map(p => [p.instance, p]));
  const kinds = new Map(items.items.map(i => [i.id, i.transport]));
  const capacities = new Map(transports.transports.map(t => {
    const { quantity, duration_ms } = t.capacity;
    if (![quantity, duration_ms].every(Number.isSafeInteger) || quantity <= 0 || duration_ms <= 0) throw new Error('Invalid runtime transport capacity');
    return [t.kind, normalize(BigInt(quantity) * 1000n, BigInt(duration_ms))];
  }));
  const lanes = (r, kind) => { const c = capacities.get(kind); if (!c) throw new Error(`Missing transport capacity: ${kind}`);
    const n = r[0] * c[1], d = r[1] * c[0]; const result = (n + d - 1n) / d;
    if (result > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('Lane count exceeds safe integer range'); return Number(result); };
  const pairs = new Map(), endpoints = new Map();
  const edges = [...placement.internal_requirements, ...placement.external_requirements].sort((a, b) => a.id.localeCompare(b.id));
  for (const e of edges) {
    const a = facilities.get(e.source), b = facilities.get(e.target), kind = kinds.get(e.item), r = rate(e.rate);
    if (!kind || (!a && !b)) throw new Error(`Invalid edge ${e.id}`);
    for (const [p, direction, other] of [[a, 'output', b], [b, 'input', a]]) if (p) {
      const key = JSON.stringify([p.instance, direction, e.item, kind]);
      if (!endpoints.has(key)) endpoints.set(key, { instance: p.instance, label: p.label, block: p.block, direction,
        item: e.item, transport: kind, total: [0n, 1n], separate: 0, requirements: [], boundary_requirements: [],
        candidate_ports: p.catalog_ports.filter(q => q.direction === direction && q.transport === kind).map(q => ({ id: q.id, edge: q.edge, connection: q.connection })) });
      const endpoint = endpoints.get(key); endpoint.total = add(endpoint.total, r); endpoint.separate += lanes(r, kind);
      endpoint.requirements.push(e.id);
      if (!other || other.block !== p.block) endpoint.boundary_requirements.push(e.id);
    }
    if (!a || !b || a.block !== b.block) {
      const source = a?.block ?? e.source, target = b?.block ?? e.target, key = JSON.stringify([source, target, e.item, kind]);
      if (!pairs.has(key)) pairs.set(key, { source, target, item: e.item, transport: kind, total: [0n, 1n], requirements: [] });
      const pair = pairs.get(key); pair.total = add(pair.total, r);
      pair.requirements.push({ requirement: e.id, source: a?.label ?? e.source, target: b?.label ?? e.target, rate: e.rate });
    }
  }
  const terminalGroups = [...endpoints.values()].map(({ total, separate, ...e }) => ({ ...e,
    rate: jsonRate(total), minimum_shared_lanes: lanes(total, e.transport), separate_edge_lanes: separate }));
  const summaries = new Map();
  for (const e of terminalGroups) {
    const key = JSON.stringify([e.instance, e.direction, e.transport]);
    if (!summaries.has(key)) summaries.set(key, { instance: e.instance, label: e.label, block: e.block, direction: e.direction,
      transport: e.transport, minimum_ports: 0, catalog_ports: e.candidate_ports.length, commodities: [] });
    const s = summaries.get(key); s.minimum_ports += e.minimum_shared_lanes;
    s.commodities.push({ item: e.item, rate: e.rate, minimum_ports: e.minimum_shared_lanes });
  }
  return { schema_version: 1, kind: 'block-interface-demand-map', port_assignment_performed: false,
    semantics: 'Rates come from the existing logical graph. Shared lane/port counts are necessary capacity bounds with compatible commodity merging; not a simultaneous port assignment or route proof.',
    pairs: [...pairs.values()].map(({ total, ...p }) => ({ ...p, rate: jsonRate(total),
      minimum_aggregate_trunk_lanes: lanes(total, p.transport), source_facilities: [...new Set(p.requirements.map(e => e.source))],
      target_facilities: [...new Set(p.requirements.map(e => e.target))] })),
    terminal_groups: terminalGroups,
    facility_interfaces: [...summaries.values()].map(s => ({ ...s, capacity_count_fits_catalog: s.minimum_ports <= s.catalog_ports })) };
}
