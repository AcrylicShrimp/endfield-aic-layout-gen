// Offline endpoint audit of the constructor's explicit straight-corridor policy, not game legality.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

export function reservationConflicts(node, length) {
  if (!Number.isSafeInteger(length) || length < 1 || length > 1024) throw new Error('Expected corridor length 1..1024');
  const ports = new Map();
  for (const b of node.boundary_requirements) for (const a of b.attachments) {
    if (a.kind !== 'new-port') continue;
    const key = JSON.stringify([a.port.instance, a.port.port]);
    if (!ports.has(key)) ports.set(key, { key, port: a.port, requirements: [] });
    ports.get(key).requirements.push(b.requirement);
  }
  const conflicts = [];
  for (const target of ports.values()) {
    const owners = [...ports.values()].filter(other => {
      if (other.key === target.key || other.port.transport !== target.port.transport) return false;
      const p = other.port;
      for (let d = 0; d < length; d++) {
        if (p.connection.x + d * (p.connection.x - p.position.x) === target.port.connection.x
          && p.connection.y + d * (p.connection.y - p.position.y) === target.port.connection.y) return true;
      }
      return false;
    });
    if (owners.length) conflicts.push({ endpoint: target.port, requirements: target.requirements,
      reservation_owners: owners.map(({ port, requirements }) => ({ port, requirements })) });
  }
  return conflicts;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [input, length, output] = process.argv.slice(2);
  if (process.argv.length !== 5) throw new Error('Usage: node reservations.mjs REPORT CORRIDOR_LENGTH NEW_OUTPUT_JSON');
  const bytes = fs.readFileSync(input);
  const report = JSON.parse(bytes);
  if (report.schema_version !== 2 || !report.final_node) throw new Error(`Expected schema 2 final node: ${input}`);
  const conflicts = reservationConflicts(report.final_node, Number(length));
  fs.writeFileSync(output, JSON.stringify({ schema_version: 1, stage: 'offline-future-reservations',
    input, input_sha256: createHash('sha256').update(bytes).digest('hex'), corridor_length: Number(length),
    scope: 'Pending new-port endpoints overlapped by another port reservation. The route engine exempts both selected endpoints by port identity; consider the chosen pair before inferring rejection.',
    conflicts }, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ success: true, conflicts: conflicts.length, output }));
}
