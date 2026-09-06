// A direct monotone permutation channel. Input/output rows stay three cells apart;
// each moving lane gets one vertical track and a one-cell exit jog toward its port.
// Non-inverted, overlapping downward intervals use descending source order;
// upward intervals use ascending order. These precedence graphs are disjoint DAGs.
export function planTracks(source, target) {
  const dest = new Map(target.map((id, i) => [id, i]));
  const moving = source.flatMap((id, i) => dest.get(id) === i ? [] : [{ id, from: i, to: dest.get(id) }]);
  const predecessors = new Map(moving.map(l => [l.id, new Set()])), constraints = [];
  for (let a = 0; a < moving.length; a++) for (let b = a + 1; b < moving.length; b++) {
    const i = moving[a], j = moving[b];
    if (i.to > j.to) continue; // An inversion requires exactly one crossing, in either track order.
    let before, after;
    if (i.to > i.from && j.to > j.from && i.to > j.from) [before, after] = [j.id, i.id];
    if (i.to < i.from && j.to < j.from && j.to < i.from) [before, after] = [i.id, j.id];
    if (before !== undefined) { predecessors.get(after).add(before); constraints.push({ before, after }); }
  }
  const order = [], remaining = new Set(moving.map(l => l.id));
  while (remaining.size) {
    const id = [...remaining].find(id => [...predecessors.get(id)].every(p => !remaining.has(p)));
    if (id === undefined) throw Object.assign(new Error('Monotone track precedence unexpectedly contains a cycle'), { code: 'interconnect-track-cycle' });
    order.push(id); remaining.delete(id);
  }
  const columns = new Map(), occupied = new Map();
  for (const id of order) {
    const l = moving.find(l => l.id === id), a = l.from * 3, b = l.to * 3 - Math.sign(l.to - l.from);
    const lo = Math.min(a, b), hi = Math.max(a, b);
    let x = Math.max(1, ...[...predecessors.get(id)].map(p => columns.get(p) + 1));
    while ((occupied.get(x) ?? []).some(r => lo <= r.hi && r.lo <= hi)) x++;
    columns.set(id, x); occupied.set(x, [...(occupied.get(x) ?? []), { lo, hi }]);
  }
  const width = moving.length ? Math.max(...columns.values()) + 3 : 2, exit = width - 2;
  return { width, height: (source.length - 1) * 3 + 1, order, constraints, columns: Object.fromEntries(columns),
    lanes: source.map((id, i) => {
      const to = dest.get(id), y = i * 3, end = to * 3;
      if (to === i) return { id, points: [[0, y], [width - 1, y]] };
      const x = columns.get(id), row = end - Math.sign(to - i);
      return { id, points: [[0, y], [x, y], [x, row], [exit, row], [exit, end], [width - 1, end]] };
    }) };
}
