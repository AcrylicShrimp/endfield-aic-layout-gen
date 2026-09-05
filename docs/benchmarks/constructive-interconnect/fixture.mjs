import { rate, capacity, multiply, parseRate, dto, compare } from './rates.mjs';
import { permutationModule } from './geometry.mjs';

export function recipeInterface(row, spec, catalogs) {
  if (!Number.isSafeInteger(spec.consumer_instances) || spec.consumer_instances < 1 || spec.consumer_instances > 32)
    throw new Error('Diagnostic consumer replication bound must be 1..32');
  const utilization = parseRate(spec.utilization);
  if (!utilization.n || compare(utilization, rate(1)) > 0) throw new Error('Expected utilization in (0,1]');
  const commodities = row.input.commodities.filter(c => c.transport === 'belt');
  if (!commodities.length) throw new Error(`No belt input in ${row.recipe}`);
  const cap = capacity(catalogs.transports.transports.find(t => t.kind === 'belt').capacity);
  const lanes = [], flows = [];
  for (const c of commodities) for (let consumer = 0; consumer < spec.consumer_instances; consumer++) {
    let remaining = multiply(parseRate(c.rate), utilization), laneIndex = 0;
    flows.push({ source: c.item, consumer: `consumer-${consumer}`, item: c.item, rate: dto(remaining) });
    while (remaining.n) {
      const value = compare(remaining, cap) > 0 ? cap : remaining;
      lanes.push({ id: `C${consumer}-${commodities.indexOf(c)}-${laneIndex++}`, item: c.item, transport: c.transport,
        rate: dto(value), consumer: `consumer-${consumer}` });
      remaining = rate(remaining.n * value.d - value.n * remaining.d, remaining.d * value.d);
    }
  }
  const target = Array.from({ length: spec.consumer_instances }, (_, i) => lanes.filter(l => l.consumer === `consumer-${i}`).map(l => l.id)).flat();
  return { recipe: row.recipe, facility: row.facility, consumer_instances: spec.consumer_instances,
    utilization: spec.utilization, distinct_supply_items: commodities.length, flows,
    supply_facilities_bound: false, consumer_facilities_placed: false, fanout_fanin_realized: false,
    geometry: permutationModule(lanes, target, catalogs) };
}
