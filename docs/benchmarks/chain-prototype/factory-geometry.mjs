import { ports, proposals } from './geometry.mjs';

export function translate(w, dx, dy) {
  const move = p => ({ ...p, x: p.x + dx, y: p.y + dy });
  return { ...structuredClone(w), placements: w.placements.map(move), transport_networks: w.transport_networks.map(n => ({ ...n,
    cells: n.cells.map(move), segments: n.segments.map(s => ({ ...s, from: move(s.from), to: move(s.to) })), terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) })),
    logistics_components: w.logistics_components.map(c => ({ ...c, position: move(c.position) })) };
}
export function* factoryProposals(w, parent, parentNode, sourceNode, edge, family, policy, used) {
  // Try aligned chains across all port/gap choices before sliding a chain away.
  for (const shifted of [false, true]) for (const p of proposals(parent, parentNode, sourceNode, edge, family, policy, used)) {
    const axis = ['east', 'west'].includes(p.target.edge) ? 'y' : 'x', size = axis === 'y' ? 'height' : 'width';
    const offsets = new Set([0]);
    for (const q of w.placements) {
      offsets.add(q[axis] - p.pose[size] - p.pose[axis]);
      offsets.add(q[axis] + q[size] - p.pose[axis]);
    }
    for (const slide of [...offsets].sort((a, b) => Math.abs(a) - Math.abs(b) || a - b).slice(0, policy.slides_per_pose)) {
      if ((slide !== 0) !== shifted) continue;
      const pose = { ...p.pose, [axis]: p.pose[axis] + slide };
      yield { ...p, pose, source: ports(pose, sourceNode).find(q => q.id === p.source.id), slide: { axis, offset: slide } };
    }
  }
}
