// A diagnostic transportation-matrix witness, not a production graph rewrite.
// Assumes every selected producer can supply every selected consumer of the same commodity.
import fs from 'node:fs';
import { pathToFileURL } from 'node:url';
const gcd = (a, b) => b ? gcd(b, a % b) : a;
const rational = (n, d) => { const g = gcd(n, d); return [n / g, d / g]; };
const add = (a, b) => rational(a[0] * b[1] + b[0] * a[1], a[1] * b[1]);
const subtract = (a, b) => rational(a[0] * b[1] - b[0] * a[1], a[1] * b[1]);
const dto = ([n, d]) => {
  if (n > BigInt(Number.MAX_SAFE_INTEGER) || d > BigInt(Number.MAX_SAFE_INTEGER)) throw new Error('Rate exceeds safe JSON integer range');
  return { numerator: Number(n), denominator: Number(d) };
};
export function redistribute(edges, sources, targets) {
  if (!edges.length || new Set(sources).size !== sources.length || new Set(targets).size !== targets.length
    || new Set(edges.map(e => e.item)).size !== 1 || new Set(edges.map(e => e.transport)).size !== 1
    || edges.some(e => !sources.includes(e.source) || !targets.includes(e.target))) throw new Error('Expected complete same-commodity endpoint membership');
  const total = (role, ids) => new Map(ids.map(id => [id, edges.filter(e => e[role] === id).reduce((sum, e) => {
    if (![e.rate.numerator, e.rate.denominator].every(Number.isSafeInteger) || e.rate.numerator <= 0 || e.rate.denominator <= 0) throw new Error('Invalid rate');
    return add(sum, [BigInt(e.rate.numerator), BigInt(e.rate.denominator)]);
  }, [0n, 1n])]));
  const supplies = total('source', sources), demands = total('target', targets), originalSupply = new Map(supplies), originalDemand = new Map(demands), result = [];
  let i = 0, j = 0;
  while (i < sources.length && j < targets.length) {
    const s = sources[i], t = targets[j], a = supplies.get(s), b = demands.get(t);
    if (!a[0]) { i++; continue; } if (!b[0]) { j++; continue; }
    if (s === t) throw new Error('Self-supply needs separate semantic treatment');
    const amount = a[0] * b[1] < b[0] * a[1] ? a : b;
    result.push({ source: s, target: t, item: edges[0].item, transport: edges[0].transport, rate: dto(amount) });
    supplies.set(s, subtract(a, amount)); demands.set(t, subtract(b, amount));
  }
  if ([...supplies.values(), ...demands.values()].some(r => r[0])) throw new Error('Unbalanced transportation margins');
  return { status: 'rate-conserving-flow-proposal', pair_flows_preserved: false, terminal_rates_preserved: true, physical_routes_validated: false,
    sources: [...originalSupply].map(([instance, rate]) => ({ instance, rate: dto(rate) })),
    targets: [...originalDemand].map(([instance, rate]) => ({ instance, rate: dto(rate) })), flows: result };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2), options = {}, allowed = ['placement', 'sources', 'targets', 'item', 'report-output'];
  for (let i = 0; i < args.length; i += 2) {
    const key = args[i].replace(/^--/, '');
    if (!args[i].startsWith('--') || !allowed.includes(key) || options[key] || !args[i + 1]) throw new Error(`Invalid option ${args[i]}`);
    options[key] = args[i + 1];
  }
  for (const key of allowed) if (!options[key]) throw new Error(`Missing --${key}`);
  const p = JSON.parse(fs.readFileSync(options.placement)), ids = new Map(p.placements.map(p => [p.label, p.instance]));
  const resolve = value => value.split(',').map(label => { if (!ids.has(label)) throw new Error(`Unknown label ${label}`); return ids.get(label); });
  const sources = resolve(options.sources), targets = resolve(options.targets);
  const selected = p.internal_requirements.filter(e => e.item === options.item && (sources.includes(e.source) || targets.includes(e.target)));
  const result = redistribute(selected, sources, targets);
  result.original_requirements = selected;
  result.labels = Object.fromEntries(p.placements.map(p => [p.instance, p.label]));
  result.unchanged_requirement_ids = p.internal_requirements.filter(e => !selected.includes(e)).map(e => e.id);
  fs.writeFileSync(options['report-output'], JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: result.status, original_pairs: selected.length, proposed_pairs: result.flows.length, terminal_rates_preserved: true }));
}
