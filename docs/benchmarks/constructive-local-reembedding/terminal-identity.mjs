import { key } from './path.mjs';

// Only isolated one-source/one-sink cells have an unambiguous implicit connection.
export function detachablePairs(w) {
  const slots = new Map(), used = new Set(), ids = new Map(), components = new Set(w.logistics_components.map(c => `${c.transport}:${key(c.position)}`));
  for (const [network, n] of w.transport_networks.entries()) {
    for (const s of n.segments) for (const p of [s.from, s.to]) used.add(`${n.transport}:${key(p)}`);
    for (const t of n.terminals) {
      ids.set(t.id, (ids.get(t.id) ?? 0) + 1);
      const k = `${n.transport}:${key(t.position)}`; if (!slots.has(k)) slots.set(k, []); slots.get(k).push({ t, network, transport: n.transport, item: n.item });
    }
  }
  const result = new Map();
  for (const [k, entries] of slots) {
    if (entries.length !== 2 || used.has(k) || components.has(k) || entries[0].network !== entries[1].network) continue;
    const [a, b] = entries, source = a.t.direction === 'output' ? a : b, target = source === a ? b : a;
    if (source.t.direction !== 'output' || target.t.direction !== 'input' || source.t.endpoint.kind !== 'facility' || target.t.endpoint.kind !== 'facility' ||
      typeof source.t.id !== 'string' || typeof target.t.id !== 'string' || ids.get(source.t.id) !== 1 || ids.get(target.t.id) !== 1 || !source.t.rate || !target.t.rate ||
      source.t.rate.numerator <= 0 || source.t.rate.denominator <= 0 || target.t.rate.numerator <= 0 || target.t.rate.denominator <= 0 ||
      BigInt(source.t.rate.numerator) * BigInt(target.t.rate.denominator) !== BigInt(target.t.rate.numerator) * BigInt(source.t.rate.denominator)) continue;
    result.set(k, { network: source.network, transport: source.transport, item: source.item, source: source.t, target: target.t });
  }
  return result;
}

// A direct, isolated facility-to-facility chain can be the inverse of detachment.
export function joinableChains(w, chains) {
  const slots = new Map(), ids = new Map(), incident = new Map();
  const tag = (tr, p) => `${tr}:${key(p)}`;
  const components = new Set(w.logistics_components.map(c => tag(c.transport, c.position)));
  for (const n of w.transport_networks) for (const t of n.terminals) {
    const k = tag(n.transport, t.position); slots.set(k, (slots.get(k) ?? 0) + 1); ids.set(t.id, (ids.get(t.id) ?? 0) + 1);
  }
  for (const c of chains) for (const p of [c.cells[0], c.cells.at(-1)]) {
    const k = tag(c.transport, p); incident.set(k, (incident.get(k) ?? 0) + 1);
  }
  const positive = r => r && r.numerator > 0 && r.denominator > 0;
  const equal = (a, b) => positive(a) && positive(b) && BigInt(a.numerator) * BigInt(b.denominator) === BigInt(b.numerator) * BigInt(a.denominator);
  return chains.filter(c => {
    if (c.zero_length || c.cells.length < 2 || c.startTerminals?.length !== 1 || c.endTerminals?.length !== 1) return false;
    const a = c.startTerminals[0], b = c.endTerminals[0];
    return a.direction === 'output' && b.direction === 'input' && [a, b].every(t => {
      const k = tag(c.transport, t.position);
      return t.endpoint.kind === 'facility' && typeof t.id === 'string' && ids.get(t.id) === 1 &&
        slots.get(k) === 1 && incident.get(k) === 1 && !components.has(k) && equal(t.rate, c.rate);
    });
  });
}

export function terminalJoinPlan(w, eligible, shiftTerminal, shiftComponent) {
  const tag = (tr, p) => `${tr}:${key(p)}`, slots = new Map(), chains = new Set(), origins = new Map();
  const components = new Set(w.logistics_components.map(c => tag(c.transport, shiftComponent(c))));
  for (const n of w.transport_networks) for (const t of n.terminals) {
    const k = tag(n.transport, shiftTerminal(t)), list = slots.get(k) ?? []; list.push(t.id); slots.set(k, list);
  }
  for (const c of eligible) {
    const a = c.startTerminals[0], b = c.endTerminals[0], k = tag(c.transport, shiftTerminal(a)), ts = slots.get(k);
    if (k !== tag(c.transport, shiftTerminal(b)) || components.has(k) || ts.length !== 2 || !ts.includes(a.id) || !ts.includes(b.id)) continue;
    chains.add(c.id); origins.set(k, new Set([key(a.position), key(b.position)]));
  }
  return { chains, allows: (tr, destination, original) => origins.get(`${tr}:${destination}`)?.has(original) === true };
}
