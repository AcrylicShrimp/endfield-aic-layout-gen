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
