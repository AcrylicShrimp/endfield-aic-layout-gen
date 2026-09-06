import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { canonical, ports } from './geometry.mjs';
import { translate } from './factory-geometry.mjs';
import { routeConnection, usedPorts } from './factory-routing.mjs';

export async function realizeBoundary(graph, internal, work, checked, decisions) {
  const lanes = graph.lanes.filter(e => e.kind === 'external').sort((a, b) => graph.order.indexOf(a.instance) - graph.order.indexOf(b.instance)
    || a.item.localeCompare(b.item) || a.id.localeCompare(b.id) || a.lane - b.lane);
  const trials = [], size = witnessScore(internal.witness);
  for (const margin of graph.boundary.margins) {
    if (!work.alive()) break;
    const frame = { minX: 0, minY: 0, maxX: size.width + 2 * margin - 1, maxY: size.height + 2 * margin - 1 };
    const perimeter = [];
    for (let x = 0; x <= frame.maxX; x++) perimeter.push({ edge: 'north', connection: { x, y: 0 }, external: true }, { edge: 'south', connection: { x, y: frame.maxY }, external: true });
    for (let y = 0; y <= frame.maxY; y++) perimeter.push({ edge: 'west', connection: { x: 0, y }, external: true }, { edge: 'east', connection: { x: frame.maxX, y }, external: true });
    const trial = { margin, frame, accepted: false, deepest_private: null }; trials.push(trial);
    const grow = async (w, index, history) => {
      if (index === lanes.length) {
        const witness = canonical(w), validation = await checked(witness, { kind: 'factory-complete', margin, external_lanes: lanes.length }, true);
        return validation?.accepted ? { witness, artifact: validation.artifact, validation, history, score: witnessScore(witness) } : null;
      }
      const e = lanes[index], owner = w.placements.find(p => p.instance === e.instance), used = usedPorts(w);
      const available = ports(owner, graph.nodes.get(e.instance)).filter(p => p.direction === e.direction && p.transport === e.transport && !used.has(JSON.stringify([e.instance, p.id])));
      const pairs = available.flatMap(facility => perimeter.map(boundary => ({ facility, boundary,
        distance: Math.abs(facility.connection.x - boundary.connection.x) + Math.abs(facility.connection.y - boundary.connection.y) }))
        .sort((a, b) => a.distance - b.distance || a.boundary.connection.y - b.boundary.connection.y || a.boundary.connection.x - b.boundary.connection.x || a.boundary.edge.localeCompare(b.boundary.edge))
        .slice(0, graph.boundary.endpoint_limit));
      pairs.sort((a, b) => a.distance - b.distance || a.facility.position.y - b.facility.position.y || a.facility.position.x - b.facility.position.x);
      for (const pair of pairs) {
        if (!work.take('proposals')) return null;
        const event = { ordinal: decisions.length, kind: 'boundary-lane', margin, requirement: e.id, lane: e.lane, facility: e.instance,
          parent_artifact: history.at(-1)?.artifact ?? internal.artifact, endpoint: pair.boundary, outcome: 'route-failed' }; decisions.push(event);
        const ends = e.direction === 'input' ? { source: pair.boundary, target: pair.facility } : { source: pair.facility, target: pair.boundary };
        const next = routeConnection(w, e, ends, event, work, { bounds: frame });
        if (!next) { if (work.reason) return null; continue; }
        const validation = await checked(next, { kind: 'boundary-private', ordinal: event.ordinal, margin, requirement: e.id, lane: e.lane });
        event.validation = validation;
        if (!validation?.accepted) { event.outcome = work.reason ? 'budget-exhausted' : 'validation-failed'; if (work.reason) return null; continue; }
        event.outcome = 'validated-private';
        const nextHistory = [...history, { artifact: validation.artifact, selected_ordinal: event.ordinal, external_lanes: index + 1 }];
        if (!trial.deepest_private || index + 1 > trial.deepest_private.external_lanes) trial.deepest_private = { external_lanes: index + 1, artifact: validation.artifact };
        const complete = await grow(next, index + 1, nextHistory);
        if (complete) { event.outcome = 'selected-boundary-path'; return complete; }
        event.outcome = 'continuation-failed'; if (work.reason) return null;
      }
      return null;
    };
    const selected = await grow(translate(internal.witness, margin, margin), 0, []);
    if (selected) { trial.accepted = true; return { selected, trials }; }
    if (work.reason) break;
  }
  return { selected: null, trials };
}
