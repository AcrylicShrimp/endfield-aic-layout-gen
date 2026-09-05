import { rate, add, compare, dto } from '../constructive-interconnect/rates.mjs';
const sub = (a, b) => rate(a.n * b.d - b.n * a.d, a.d * b.d);
const min = (...xs) => xs.reduce((a, b) => compare(a, b) < 0 ? a : b);

// Split at BOTH terminal capacity boundaries. A fragment can share its source
// bucket, its target bucket, or both; a bucket never mixes items.
export function allocateBuckets(nodes, items, producers, consumers, cap) {
  const lanes = [], sourceGroups = [], targetGroups = [];
  for (const item of items) {
    const entries = (ids, direction) => ids.flatMap(id => nodes.get(id)[direction].filter(a => a.item === item)
      .map(a => ({ id, left: a.value, room: rate(0), group: null })));
    const supplies = entries(producers, 'outputs'), demands = entries(consumers, 'inputs');
    let i = 0, j = 0;
    const bucket = (e, groups, side) => {
      if (e.room.n) return;
      e.room = cap; e.group = { id: `${side}-${groups.length}`, node: e.id, item, lanes: [], value: rate(0) }; groups.push(e.group);
    };
    while (i < supplies.length && j < demands.length) {
      if (lanes.length >= 384) throw Object.assign(new Error('Diagnostic fragment budget exceeded'),
        { code: 'interface-lane-budget', severity: 'error', stage: 'allocation' });
      const s = supplies[i], d = demands[j]; bucket(s, sourceGroups, 'source'); bucket(d, targetGroups, 'target');
      const value = min(s.left, d.left, s.room, d.room);
      const l = { id: `lane-${lanes.length}`, source: s.id, target: d.id, item, transport: 'belt', rate: dto(value),
        source_group: s.group.id, target_group: d.group.id };
      lanes.push(l);
      for (const e of [s, d]) { e.group.lanes.push(l); e.group.value = add(e.group.value, value); e.room = sub(e.room, value); e.left = sub(e.left, value); }
      if (!s.left.n) i++; if (!d.left.n) j++;
    }
    if (i !== supplies.length || j !== demands.length) throw new Error('Internal allocation failed conservation');
  }
  const order = (groups, bank, opposite, field) => bank.flatMap(id => groups.filter(g => g.node === id)
    .sort((a, b) => Math.min(...a.lanes.map(l => opposite.indexOf(l[field]))) - Math.min(...b.lanes.map(l => opposite.indexOf(l[field])))));
  const orderedSources = order(sourceGroups, producers, consumers, 'target');
  const orderedTargets = order(targetGroups, consumers, producers, 'source');
  const sourceOrder = orderedSources.flatMap(g => g.lanes), targetOrder = orderedTargets.flatMap(g => g.lanes);
  const pressure = [...producers.map(id => ({ id, direction: 'output', count: sourceGroups.filter(g => g.node === id).length })),
    ...consumers.map(id => ({ id, direction: 'input', count: targetGroups.filter(g => g.node === id).length }))]
    .map(p => ({ ...p, available_ports: nodes.get(p.id)[p.direction].length })).filter(p => p.count > p.available_ports);
  let inversions = 0;
  const targetRank = new Map(targetOrder.map((l, i) => [l.id, i]));
  for (let i = 0; i < sourceOrder.length; i++) for (let j = i + 1; j < sourceOrder.length; j++)
    if (targetRank.get(sourceOrder[i].id) > targetRank.get(sourceOrder[j].id)) inversions++;
  return { producers, consumers, lanes, sourceGroups: orderedSources, targetGroups: orderedTargets, sourceOrder, targetOrder, inversions, pressure };
}
