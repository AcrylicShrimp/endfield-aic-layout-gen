import { rate, add, capacity, dto, lanesFor } from './rates.mjs';

export function census(catalogs) {
  const { recipes, items, facilities, transports, components } = catalogs;
  if (recipes.schema_version !== 1 || items.schema_version !== 1 || facilities.schema_version !== 3 ||
      transports.schema_version !== 1 || components.schema_version !== 1) throw new Error('Unsupported catalog schema');
  const index = (rows, key) => {
    const m = new Map(rows.map(r => [r[key], r]));
    if (m.size !== rows.length) throw new Error(`Duplicate catalog ${key}`); return m;
  };
  const itemIndex = index(items.items, 'id'), facilityIndex = index(facilities.facilities, 'id');
  const transportIndex = index(transports.transports, 'kind');
  index(recipes.recipes, 'id'); index(components.components, 'id');
  const rows = recipes.recipes.map(r => {
    const f = facilityIndex.get(r.facility);
    if (!f || !Number.isSafeInteger(r.duration_ms) || r.duration_ms <= 0) throw new Error(`Invalid recipe ${r.id}`);
    const side = direction => {
      const totals = new Map();
      for (const i of r[direction + 's']) {
        if (!itemIndex.has(i.item) || !Number.isSafeInteger(i.quantity) || i.quantity <= 0) throw new Error(`Invalid item in ${r.id}`);
        totals.set(i.item, add(totals.get(i.item) ?? rate(0), rate(BigInt(i.quantity) * 1000n, r.duration_ms)));
      }
      const commodities = [...totals].map(([item, value]) => {
        const transport = itemIndex.get(item).transport, t = transportIndex.get(transport);
        if (!t) throw new Error(`Missing transport for ${r.id}/${item}`);
        return { item, transport, rate: dto(value), minimum_lanes: lanesFor(value, capacity(t.capacity)) };
      });
      return { distinct_items: totals.size, commodities, by_transport: Object.fromEntries([...transportIndex.keys()].map(t => {
        const subset = commodities.filter(c => c.transport === t);
        const ports = f.ports.filter(p => p.direction === direction && p.transport === t);
        return [t, { distinct_items: subset.length, minimum_lanes: subset.reduce((s, c) => s + c.minimum_lanes, 0),
          catalog_ports: ports.length, ports_by_base_edge: Object.fromEntries(['north', 'east', 'south', 'west'].map(e => [e, ports.filter(p => p.edge === e).length])) }];
      })) };
    };
    return { recipe: r.id, facility: f.id, input: side('input'), output: side('output') };
  });
  const maxima = {};
  const record = (name, select) => {
    const maximum = Math.max(0, ...rows.map(select));
    maxima[name] = { maximum, witnesses: rows.filter(r => select(r) === maximum).map(r => r.recipe) };
  };
  for (const side of ['input', 'output']) {
    record(`${side}_distinct_items`, r => r[side].distinct_items);
    for (const t of transportIndex.keys()) {
      record(`${side}_${t}_distinct_items`, r => r[side].by_transport[t].distinct_items);
      record(`${side}_${t}_minimum_lanes`, r => r[side].by_transport[t].minimum_lanes);
    }
  }
  const deficits = rows.flatMap(r => ['input', 'output'].flatMap(side => Object.entries(r[side].by_transport)
    .filter(([, v]) => v.minimum_lanes > v.catalog_ports)
    .map(([transport, v]) => ({ recipe: r.recipe, side, transport, ...v }))));
  const facilityPorts = facilities.facilities.map(f => ({ facility: f.id, by_transport: Object.fromEntries([...transportIndex.keys()].map(t =>
    [t, Object.fromEntries(['input', 'output'].map(side => [side, f.ports.filter(p => p.direction === side && p.transport === t).length]))])) }));
  const portMaxima = Object.fromEntries([...transportIndex.keys()].map(t => [t, Object.fromEntries(['input', 'output'].map(side => {
    const maximum = Math.max(0, ...facilityPorts.map(f => f.by_transport[t][side]));
    return [side, { maximum, witnesses: facilityPorts.filter(f => f.by_transport[t][side] === maximum).map(f => f.facility) }];
  }))]));
  return { schema_version: 1, scope: 'one-recipe-one-facility-at-full-utilization',
    counts: { recipes: rows.length, items: items.items.length, facilities: facilities.facilities.length, transports: transports.transports.length, components: components.components.length },
    facility_port_budgets: facilityPorts, facility_port_maxima: portMaxima,
    components: components.components.map(c => ({ component: c.id, kind: c.kind, transport: c.transport, footprint: c.footprint,
      allowed_rotations: c.allowed_rotations, allowed_input_arms: c.input_directions, allowed_output_arms: c.output_directions, capacity: dto(capacity(c.capacity)) })),
    maxima, port_count_deficits: deficits, recipes: rows,
    replicated_group_crossing_upper_bound: null,
    caveats: ['Necessary lane counts are not routing or simultaneous port certificates.',
      'Replicated facility groups have no catalog-only finite size or crossing-count bound.',
      'A single port carries only its assigned commodity; different item rates cannot share a lane budget.'] };
}
