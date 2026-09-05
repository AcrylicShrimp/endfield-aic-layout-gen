// Read-only accessibility diagnostic. Omission experiments are not valid planner candidates.
import { readFile } from 'node:fs/promises';

const file = process.argv[2];
if (!file) throw new Error('usage: node inspect-obstruction.mjs REPORT.json');
const report = JSON.parse(await readFile(file, 'utf8'));
const obstruction = report.last_obstruction;
if (!obstruction) throw new Error(`no recorded obstruction in '${file}'`);
const node = obstruction.candidate;
const key = (x, y) => `${x},${y}`;
const facilityCells = new Set();
for (const p of node.placements) {
  for (let x = p.x; x < p.x + p.width; x++) {
    for (let y = p.y; y < p.y + p.height; y++) facilityCells.add(key(x, y));
  }
}
function exterior(transport, omittedNetwork) {
  const blocked = new Set(facilityCells);
  for (const n of node.transport_networks.filter(n => n.transport === transport && n.id !== omittedNetwork)) {
    for (const p of n.cells) blocked.add(key(p.x, p.y));
  }
  const seen = new Set([key(-1, -1)]), queue = [[-1, -1]];
  for (let head = 0; head < queue.length; head++) {
    const [x, y] = queue[head];
    for (const [nx, ny] of [[x-1,y],[x+1,y],[x,y-1],[x,y+1]]) {
      const k = key(nx, ny);
      if (nx < -1 || ny < -1 || nx > node.bounds.width+3 || ny > node.bounds.height+3 || blocked.has(k) || seen.has(k)) continue;
      seen.add(k); queue.push([nx, ny]);
    }
  }
  return seen;
}
const results = [];
for (const b of node.boundary_requirements.filter(b => obstruction.blocked_requirements.includes(b.requirement))) {
  const points = b.attachments.map(a => a.kind === 'new-port' ? a.port.connection : a.branch_cell);
  const reachable = omitted => {
    const seen = exterior(b.transport, omitted);
    return points.some(p => seen.has(key(p.x, p.y)));
  };
  const currentlyReachable = reachable();
  results.push({
    requirement: b.requirement, instance: b.inside_instance, item: b.item,
    direction: b.direction, points, exterior_reachable: currentlyReachable,
    restored_when_omitting: currentlyReachable ? [] : node.transport_networks
      .filter(n => n.transport === b.transport && reachable(n.id))
      .map(n => ({ network: n.id, item: n.item, includes_triggering_connection: n.requirement_ids.includes(obstruction.connection) })),
  });
}
console.log(JSON.stringify({ connection: obstruction.connection, boundaries: results }, null, 2));
