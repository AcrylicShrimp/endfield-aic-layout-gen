import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { permutationModule } from '../constructive-interconnect/geometry.mjs';
import { rate, add, multiply, compare, parseRate, dto, capacity, lanesFor } from '../constructive-interconnect/rates.mjs';
import { buildDedicated } from '../constructive-bound-interconnect/dedicated.mjs';

const subtract = (a, b) => rate(a.n * b.d - b.n * a.d, a.d * b.d);
const minimum = (a, b) => compare(a, b) < 0 ? a : b;
const divide = (a, b) => rate(a.n * b.d, a.d * b.n);
const sum = xs => xs.reduce(add, rate(0));
const key = p => `${p.x},${p.y}`;
const fail = (code, stage, message, entity = null) => {
  throw Object.assign(new Error(message), { code, severity: 'error', stage, entity });
};
const exactKeys = (o, keys) => o && typeof o === 'object' && !Array.isArray(o) &&
  Object.keys(o).length === keys.length && keys.every(k => Object.hasOwn(o, k));

// Strictly ordered endpoints admit disjoint one-column bends: upward lanes first,
// then downward lanes in reverse order. There is one distinct interior column per lane.
export function orderedApproach(lanes) {
  const sorted = [...lanes].sort((a, b) => a.start.y - b.start.y);
  if (!sorted.length || sorted.some((l, i) => l.start.x !== sorted[0].start.x ||
      l.end.x !== sorted[0].end.x || l.end.x - l.start.x <= lanes.length ||
      (i && (l.start.y <= sorted[i - 1].start.y || l.end.y <= sorted[i - 1].end.y))))
    fail('interface-approach-order', 'geometry', 'Approach endpoints must be strictly ordered on two separated faces');
  const bends = [...sorted.filter(l => l.end.y < l.start.y), ...sorted.filter(l => l.end.y >= l.start.y).reverse()];
  return new Map(bends.map((l, i) => [l.id, [[l.start.x, l.start.y],
    [l.start.x + i + 1, l.start.y], [l.start.x + i + 1, l.end.y], [l.end.x, l.end.y]]]));
}

export function constructInterface(catalogs, request) {
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
  function allocate(producers, consumers) {
    const lanes = [];
    for (const item of items) {
      const supplies = producers.flatMap(id => nodes.get(id).outputs.filter(a => a.item === item).map(a => ({ id, left: a.value })));
      const demands = consumers.flatMap(id => nodes.get(id).inputs.filter(a => a.item === item).map(a => ({ id, left: a.value })));
      let i = 0, j = 0;
      while (i < supplies.length && j < demands.length) {
        const s = supplies[i], d = demands[j], value = minimum(s.left, d.left);
        if (lanesFor(value, cap) + lanes.length > 384) fail('interface-lane-budget', 'allocation', 'Diagnostic lane budget exceeded');
        let left = value;
        while (left.n) { const v = minimum(left, cap); lanes.push({ id: `lane-${lanes.length}`, source: s.id, target: d.id, item, transport: 'belt', rate: dto(v) }); left = subtract(left, v); }
        s.left = subtract(s.left, value); d.left = subtract(d.left, value);
        if (!s.left.n) i++; if (!d.left.n) j++;
      }
      if (i !== supplies.length || j !== demands.length) throw new Error('Internal allocation failed conservation');
    }
    const sourceOrder = producers.flatMap(id => lanes.filter(l => l.source === id).sort((a, b) => consumers.indexOf(a.target) - consumers.indexOf(b.target)));
    const targetOrder = consumers.flatMap(id => lanes.filter(l => l.target === id).sort((a, b) => producers.indexOf(a.source) - producers.indexOf(b.source)));
    const pressure = [...producers.map(id => ({ id, direction: 'output', count: lanes.filter(l => l.source === id).length })),
      ...consumers.map(id => ({ id, direction: 'input', count: lanes.filter(l => l.target === id).length }))]
      .map(p => ({ ...p, available_ports: nodes.get(p.id)[p.direction].length }))
      .filter(p => p.count > p.available_ports);
    let inversions = 0;
    for (let i = 0; i < sourceOrder.length; i++) for (let j = i + 1; j < sourceOrder.length; j++)
      if (targetOrder.indexOf(sourceOrder[i]) > targetOrder.indexOf(sourceOrder[j])) inversions++;
    return { producers, consumers, lanes, sourceOrder, targetOrder, inversions, pressure };
  }
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
    const message = `Independent pair lanes exceed facing ports after ${trials} order trials; fanout/fanin is not synthesized`;
    throw Object.assign(new Error(message), { code: 'interface-port-pressure', severity: 'error', stage: 'port-assignment', entity: best.pressure,
      evidence: { producer_order: best.producers, consumer_order: best.consumers, lanes: best.lanes,
        instances: [...nodes.values()].map(n => ({ id: n.id, recipe: n.recipe.id, facility: n.recipe.facility,
          facing_input_ports: n.input.length, facing_output_ports: n.output.length,
          inputs: n.inputs.map(a => ({ item: a.item, rate: dto(a.value) })), outputs: n.outputs.map(a => ({ item: a.item, rate: dto(a.value) })) })),
        lane_capacity: dto(cap), order_trials: trials, geometry_constructed: false, global_infeasibility_proven: false } });
  }
  timings.order_ms = performance.now() - orderStart;
  const geometryStart = performance.now();
  const pack = ids => { let y = 2; for (const id of ids) { const n = nodes.get(id); n.y = y; y += n.shape.height + 2; } };
  pack(best.producers); pack(best.consumers);
  for (const [bank, direction, order] of [[best.producers, 'output', best.sourceOrder], [best.consumers, 'input', best.targetOrder]])
    for (const id of bank) order.filter(l => l[direction === 'output' ? 'source' : 'target'] === id).forEach((l, i) => {
      const n = nodes.get(id), p = n[direction][i]; l[direction === 'output' ? 'source_port' : 'target_port'] = p.id;
      l[direction === 'output' ? 'source_y' : 'target_y'] = n.y + p.position.y;
    });
  const leftWidth = Math.max(...best.producers.map(id => nodes.get(id).shape.width)), sx = 2 + leftWidth;
  const module = best.inversions ? permutationModule(best.sourceOrder, best.targetOrder.map(l => l.id), catalogs) : null;
  const origin = { x: sx + best.lanes.length + 2, y: 2 };
  const tx = module ? origin.x + module.width - 1 + best.lanes.length + 2 : sx + best.lanes.length + 2;
  const shiftedLanes = module?.lanes.map(l => ({ ...l, cells: l.cells.map(p => ({ x: p.x + origin.x, y: p.y + origin.y })) }));
  let paths;
  if (module) {
    const left = orderedApproach(best.sourceOrder.map((l, i) => ({ id: l.id, start: { x: sx, y: l.source_y }, end: { x: origin.x, y: origin.y + i * 3 } })));
    const right = orderedApproach(best.targetOrder.map((l, i) => ({ id: l.id, start: { x: origin.x + module.width - 1, y: origin.y + i * 3 }, end: { x: tx, y: l.target_y } })));
    paths = new Map(shiftedLanes.map(l => [l.id, [...left.get(l.id), ...l.cells.map(p => [p.x, p.y]), ...right.get(l.id)]]));
  } else paths = orderedApproach(best.sourceOrder.map(l => ({ id: l.id, start: { x: sx, y: l.source_y }, end: { x: tx, y: l.target_y } })));
  const spec = { schema_version: 1, crossing_bundle: null,
    external_east: tx + 2 + Math.max(...best.consumers.map(id => nodes.get(id).shape.width)),
    nodes: [...best.producers, ...best.consumers].map(id => { const n = nodes.get(id); return { id, recipe: n.recipe.id,
      utilization: n.utilization, rotation: n.shape.rotation, position: [best.producers.includes(id) ? sx - n.shape.width : tx + 1, n.y] }; }),
    links: best.lanes.map(l => ({ id: l.id, source: l.source, target: l.target, source_port: l.source_port, target_port: l.target_port,
      target_fraction: dto(divide(parseRate(l.rate), nodes.get(l.target).inputs.find(a => a.item === l.item).value)), via: paths.get(l.id).slice(1, -1) })) };
  timings.geometry_ms = performance.now() - geometryStart;
  const lowerStart = performance.now();
  let result;
  try { result = buildDedicated(catalogs, spec); }
  catch (e) { fail('interface-binding-failed', 'lowering', e.message); }
  for (const c of module?.components ?? []) {
    const moved = { ...c, position: { x: c.position.x + origin.x, y: c.position.y + origin.y } };
    result.witness.logistics_components.push(moved);
    for (const l of shiftedLanes.filter(l => l.cells.some(p => key(p) === key(moved.position)))) {
      const net = result.witness.transport_networks.find(n => n.item === l.item);
      if (!net.component_ids.includes(c.id)) net.component_ids.push(c.id);
    }
  }
  timings.lowering_ms = performance.now() - lowerStart;
  return { ...result, case: 'automatic-two-bank-interface', accepted: false, factory_validated: false,
    pending: ['independent-geometry-and-complete-recipe-validation'], generated_spec: spec,
    decisions: { producer_order: best.producers, consumer_order: best.consumers,
      source_order: best.sourceOrder.map(l => l.id), target_order: best.targetOrder.map(l => l.id),
      inversions: best.inversions, order_trials: trials, order_budget: budget, order_budget_exhausted: trials >= budget },
    permutation: module, timings, construction_ms: performance.now() - start };
}
