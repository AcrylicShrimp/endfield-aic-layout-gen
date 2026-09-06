import { diagnostic } from './input.mjs';

export function preservedPrefix(before, after) {
  const error = entity => { throw Object.assign(new Error('Replay changed frozen internal geometry'), diagnostic('replay-preservation-invalid', 'chain-replay', 'Replay changed frozen internal geometry', entity)); };
  const anchor = before.placements[0], next = after.placements.find(p => p.instance === anchor.instance);
  if (!next) error(anchor.instance);
  const dx = next.x - anchor.x, dy = next.y - anchor.y, move = p => ({ ...p, x: p.x + dx, y: p.y + dy });
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  for (const p of before.placements) {
    if (!same(move(p), after.placements.find(q => q.instance === p.instance)) || !same(before.utilizations[p.instance], after.utilizations[p.instance])) error(p.instance);
  }
  for (const n of before.transport_networks) {
    const q = after.transport_networks.find(q => q.id === n.id);
    if (!q || q.item !== n.item || q.transport !== n.transport || !n.requirement_ids.every(id => q.requirement_ids.includes(id))) error(n.id);
    for (const c of n.cells) if (!q.cells.some(p => same(move(c), p))) error(n.id);
    for (const s of n.segments) if (!q.segments.some(t => same({ ...s, from: move(s.from), to: move(s.to) }, t))) error(n.id);
    for (const t of n.terminals) if (!same({ ...t, position: move(t.position) }, q.terminals.find(u => u.id === t.id))) error(t.id);
  }
  return { accepted: true, frozen_members: before.placements.map(p => p.instance), translation: { x: dx, y: dy } };
}

export function rollbackWindow(graph, before, current, limit) {
  const fixed = new Set(before.witness.placements.map(p => p.instance)), removed = current.witness.placements.map(p => p.instance).filter(id => !fixed.has(id));
  if (!removed.length || removed.length > limit) return null;
  const pending = new Set(removed), seen = new Set([removed[0]]), visit = [removed[0]];
  if (graph.transactions.some(g => g.members.some(id => pending.has(id)) && !g.members.every(id => pending.has(id)))) return null;
  for (let i = 0; i < visit.length; i++) for (const e of graph.edges) {
    const other = e.source === visit[i] ? e.target : e.target === visit[i] ? e.source : null;
    if (pending.has(other) && !seen.has(other)) { seen.add(other); visit.push(other); }
  }
  if (seen.size !== removed.length) return null;
  return { removed, frozen_members: [...fixed], restored_artifact: before.artifact,
    affected_internal_requirements: graph.edges.filter(e => pending.has(e.source) || pending.has(e.target)).map(e => e.id),
    external_closure: 'All explicit perimeter lanes replayed as one private closure transaction' };
}
