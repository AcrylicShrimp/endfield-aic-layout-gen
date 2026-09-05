import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { rate, add, multiply, parseRate, dto, compare, capacity } from '../constructive-interconnect/rates.mjs';
import { allocateBuckets } from './allocation.mjs';
import { embedInterface } from './embedding.mjs';
import { embedTrunks } from './trunk-embedding.mjs';

const sum = xs => xs.reduce(add, rate(0));
const fail = (code, stage, message, entity = null) => {
  throw Object.assign(new Error(message), { code, severity: 'error', stage, entity });
};
const exactKeys = (o, keys) => o && typeof o === 'object' && !Array.isArray(o) &&
  Object.keys(o).length === keys.length && keys.every(k => Object.hasOwn(o, k));

// Overlapping closed vertical intervals impose strict bend-column precedence.
// Disjoint intervals can share columns; longest-path ranks minimize this route family's depth.
export function orderedApproach(lanes) {
  const sorted = [...lanes].sort((a, b) => a.start.y - b.start.y);
  if (!sorted.length || new Set(sorted.map(l => l.id)).size !== sorted.length || sorted.some((l, i) =>
      ![l.start.x, l.start.y, l.end.x, l.end.y].every(Number.isSafeInteger) ||
      l.start.x !== sorted[0].start.x || l.end.x !== sorted[0].end.x || l.end.x <= l.start.x ||
      (i && (l.start.y <= sorted[i - 1].start.y || l.end.y <= sorted[i - 1].end.y))))
    fail('interface-approach-order', 'geometry', 'Approach endpoints must be strictly ordered on two separated faces');
  const columns = new Map();
  for (const bends of [sorted.filter(l => l.end.y < l.start.y), sorted.filter(l => l.end.y > l.start.y).reverse()]) {
    for (let i = 0; i < bends.length; i++) {
      const l = bends[i], lo = Math.min(l.start.y, l.end.y), hi = Math.max(l.start.y, l.end.y);
      let column = 1;
      for (const other of bends.slice(0, i))
        if (Math.max(other.start.y, other.end.y) >= lo && Math.min(other.start.y, other.end.y) <= hi)
          column = Math.max(column, columns.get(other.id) + 1);
      columns.set(l.id, column);
    }
  }
  const depth = Math.max(0, ...columns.values());
  if (sorted[0].end.x - sorted[0].start.x <= depth)
    fail('interface-approach-order', 'geometry', `Approach needs ${depth} interior bend columns`);
  return new Map(sorted.map(l => [l.id, columns.has(l.id) ? [[l.start.x, l.start.y],
    [l.start.x + columns.get(l.id), l.start.y], [l.start.x + columns.get(l.id), l.end.y], [l.end.x, l.end.y]] :
    [[l.start.x, l.start.y], [l.end.x, l.end.y]]]));
}

export function constructInterface(catalogs, request, embedding = 'leaf') {
  if (!['leaf', 'trunk', 'portfolio'].includes(embedding)) fail('interface-unknown-embedding', 'input', 'Expected leaf, trunk or portfolio geometry', embedding);
  const start = performance.now(), timings = {};
  if (!exactKeys(request, ['schema_version', 'producers', 'consumers']) || request.schema_version !== 1 ||
      !Array.isArray(request.producers) || !Array.isArray(request.consumers) ||
      !request.producers.length || !request.consumers.length)
    fail('interface-invalid-request', 'input', 'Expected schema v1 with nonempty producer and consumer arrays');
  const all = [...request.producers, ...request.consumers];
  if (all.length > 64) fail('interface-node-budget', 'input', 'Diagnostic instance budget of 64 exceeded');
  if (all.some(n => !exactKeys(n, ['id', 'recipe', 'utilization']) || typeof n.id !== 'string' || !n.id ||
      !exactKeys(n.utilization, ['numerator', 'denominator']) ||
      !Number.isSafeInteger(n.utilization.numerator) || !Number.isSafeInteger(n.utilization.denominator) ||
      n.utilization.numerator <= 0 || n.utilization.denominator <= 0 ||
      n.utilization.numerator > n.utilization.denominator) || new Set(all.map(n => n.id)).size !== all.length)
    fail('interface-invalid-instance', 'input', 'Instances need unique IDs, recipe IDs and positive utilization at most one; geometry fields are forbidden');
  const transport = catalogs.transports.transports.find(t => t.kind === 'belt');
  if (!transport) fail('interface-missing-transport', 'input', 'Runtime belt transport is missing');
  const cap = capacity(transport.capacity), nodes = new Map();
  if (!cap.n) fail('interface-invalid-capacity', 'input', 'Runtime transport capacity must be positive');
  for (const n of all) {
    const recipe = catalogs.recipes.recipes.find(r => r.id === n.recipe);
    if (!recipe) fail('interface-unknown-recipe', 'input', `Unknown runtime recipe ${n.recipe}`, n.id);
    if ([...recipe.inputs, ...recipe.outputs].some(a => catalogs.items.items.find(i => i.id === a.item)?.transport !== 'belt'))
      fail('interface-unsupported-transport', 'scope', 'This constructor supports belt-only recipe terminals', n.id);
    if (request.producers.includes(n) && recipe.outputs.length !== 1)
      fail('interface-multiple-producer-outputs', 'scope', 'Producer must have exactly one output item', n.id);
    const facility = catalogs.facilities.facilities.find(f => f.id === recipe.facility);
    const shape = facility?.allowed_rotations.map(r => rotateFacility(facility, r)).find(s =>
      ['input', 'output'].every(d => s.ports.some(p => p.transport === 'belt' && p.direction === d && p.edge === (d === 'input' ? 'west' : 'east'))));
    if (!shape) fail('interface-unsupported-orientation', 'scope', 'No west-input/east-output catalog orientation', n.id);
    const ports = d => shape.ports.filter(p => p.transport === 'belt' && p.direction === d &&
      p.edge === (d === 'input' ? 'west' : 'east')).sort((a, b) => a.position.y - b.position.y || a.id.localeCompare(b.id));
    const values = d => recipe[d + 's'].map(a => ({ item: a.item,
      value: multiply(rate(BigInt(a.quantity) * 1000n, recipe.duration_ms), parseRate(n.utilization)) }));
    nodes.set(n.id, { ...n, recipe, shape, input: ports('input'), output: ports('output'), inputs: values('input'), outputs: values('output') });
  }
  const producerIds = request.producers.map(n => n.id).sort(), consumerIds = request.consumers.map(n => n.id).sort();
  const items = [...new Set(producerIds.flatMap(id => nodes.get(id).outputs.map(a => a.item)).concat(
    consumerIds.flatMap(id => nodes.get(id).inputs.map(a => a.item))))].sort();
  for (const item of items) {
    const supply = sum(producerIds.flatMap(id => nodes.get(id).outputs.filter(a => a.item === item).map(a => a.value)));
    const demand = sum(consumerIds.flatMap(id => nodes.get(id).inputs.filter(a => a.item === item).map(a => a.value)));
    if (compare(supply, demand)) fail('interface-item-imbalance', 'allocation', 'Producer supply and consumer demand must balance exactly', item);
  }
  timings.input_ms = performance.now() - start;
  const allocate = (producers, consumers) => allocateBuckets(nodes, items, producers, consumers, cap);
  const orderStart = performance.now();
  let best = allocate(producerIds, consumerIds), trials = 1;
  const score = a => [a.pressure.reduce((n, p) => n + p.count - nodes.get(p.id)[p.direction].length, 0), a.inversions, a.lanes.length];
  const better = (a, b) => { const x = score(a), y = score(b); for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return x[i] < y[i]; return false; };
  const budget = 512;
  while (trials < budget) {
    let next = best;
    for (const bank of ['producers', 'consumers']) for (let i = 0; i < best[bank].length; i++) for (let j = 0; j < best[bank].length; j++) {
      if (i === j || trials >= budget) continue;
      const order = [...best[bank]], [id] = order.splice(i, 1); order.splice(j, 0, id);
      const candidate = allocate(bank === 'producers' ? order : best.producers, bank === 'consumers' ? order : best.consumers); trials++;
      if (better(candidate, next)) next = candidate;
    }
    if (next === best) break; best = next;
  }
  if (best.pressure.length) {
    const message = `Capacity buckets exceed facing ports after ${trials} order trials`;
    throw Object.assign(new Error(message), { code: 'interface-aggregate-port-pressure', severity: 'error', stage: 'port-assignment', entity: best.pressure,
      evidence: { producer_order: best.producers, consumer_order: best.consumers, lanes: best.lanes,
        instances: [...nodes.values()].map(n => ({ id: n.id, recipe: n.recipe.id, facility: n.recipe.facility,
          facing_input_ports: n.input.length, facing_output_ports: n.output.length,
          inputs: n.inputs.map(a => ({ item: a.item, rate: dto(a.value) })), outputs: n.outputs.map(a => ({ item: a.item, rate: dto(a.value) })) })),
        lane_capacity: dto(cap), order_trials: trials, geometry_constructed: false, global_infeasibility_proven: false } });
  }
  timings.order_ms = performance.now() - orderStart;
  if (embedding === 'portfolio') {
    const candidates = [], failures = [];
    for (const family of ['leaf', 'trunk']) {
      const candidateStart = performance.now();
      try {
        const result = (family === 'leaf' ? embedInterface : embedTrunks)(catalogs, structuredClone(nodes), structuredClone(best), orderedApproach);
        candidates.push({ ...result, schema_version: 2, case: 'automatic-two-bank-interface', embedding: family, accepted: false, factory_validated: false,
          pending: ['independent-geometry-and-complete-recipe-validation'], decisions: { producer_order: best.producers, consumer_order: best.consumers,
            source_order: best.sourceOrder.map(l => l.id), target_order: best.targetOrder.map(l => l.id), inversions: best.inversions,
            order_trials: trials, order_budget: budget, order_budget_exhausted: trials >= budget },
          timings: { ...timings, geometry_ms: result.geometry_ms, lowering_ms: result.lowering_ms }, construction_ms: performance.now() - candidateStart });
      } catch (e) { failures.push({ embedding: family, code: e.code ?? 'interface-generation-failed', severity: 'error', stage: e.stage ?? 'geometry', message: e.message, entity: e.entity ?? null }); }
    }
    return { schema_version: 1, kind: 'unvalidated-interface-portfolio', candidates, failures, construction_ms: performance.now() - start,
      shared_timings: timings, accepted: false };
  }
  let result;
  try { result = (embedding === 'leaf' ? embedInterface : embedTrunks)(catalogs, nodes, best, orderedApproach); }
  catch (e) {
    e.evidence = { ...e.evidence, fragment_count: best.lanes.length, source_ports: best.sourceGroups.length,
      target_ports: best.targetGroups.length, requested_inversions: best.inversions,
      producer_order: best.producers, consumer_order: best.consumers, global_infeasibility_proven: false };
    throw e;
  }
  timings.geometry_ms = result.geometry_ms; timings.lowering_ms = result.lowering_ms;
  return { ...result, schema_version: 2, case: 'automatic-two-bank-interface', embedding, accepted: false, factory_validated: false,
    pending: ['independent-geometry-and-complete-recipe-validation'],
    decisions: { producer_order: best.producers, consumer_order: best.consumers,
      source_order: best.sourceOrder.map(l => l.id), target_order: best.targetOrder.map(l => l.id),
      inversions: best.inversions, order_trials: trials, order_budget: budget, order_budget_exhausted: trials >= budget },
    timings, construction_ms: performance.now() - start };
}
