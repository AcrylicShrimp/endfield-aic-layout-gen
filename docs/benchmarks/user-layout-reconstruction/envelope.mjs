import fs from 'node:fs';
import { pathToFileURL } from 'node:url';

const fail = (message, entity = null) => { throw Object.assign(new Error(message), {
  code: 'envelope-input', severity: 'error', stage: 'completion-envelope', entity }); };
const key = (x, y) => `${x},${y}`;
const integer = Number.isSafeInteger;
const overlap = (a, b) => a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
const cells = r => Array.from({ length: r.width * r.height }, (_, i) => [r.x + i % r.width, r.y + Math.floor(i / r.width)]);
function shape(p, catalog) {
  const f = catalog.facilities?.find(f => f.id === p.facility);
  if (!f || !f.allowed_rotations?.includes(p.rotation) || ![0, 90, 180, 270].includes(p.rotation)
    || ![f.footprint?.width, f.footprint?.height].every(n => integer(n) && n > 0)) fail('Unknown facility, rotation or footprint', p.id);
  const { width, height } = f.footprint;
  return { ...p, width: p.rotation % 180 ? height : width, height: p.rotation % 180 ? width : height };
}
export function mask(snapshot, catalog) {
  if (!snapshot.placements?.length || !Array.isArray(snapshot.belt_polylines)) fail('Expected placements and belt polylines', snapshot.id);
  const occupied = new Set(), ids = new Set(); let facilityCells = 0;
  const insert = (x, y) => { if (occupied.has(key(x, y))) fail('Overlapping observed geometry', [x, y]); occupied.add(key(x, y)); };
  for (const p of snapshot.placements) {
    if (!p.id || ids.has(p.id) || !integer(p.x) || !integer(p.y)) fail('Invalid or duplicate placement', p.id);
    ids.add(p.id); const r = shape(p, catalog); facilityCells += r.width * r.height;
    for (const [x, y] of cells(r)) insert(x, y);
  }
  for (const corners of snapshot.belt_polylines) {
    if (!corners.length || !corners.every(p => Array.isArray(p) && p.length === 2 && p.every(integer))) fail('Invalid belt corners', snapshot.id);
    let [x, y] = corners[0]; insert(x, y);
    for (const [tx, ty] of corners.slice(1)) {
      if ((x !== tx && y !== ty) || (x === tx && y === ty)) fail('Diagonal or repeated belt corner', snapshot.id);
      const dx = Math.sign(tx - x), dy = Math.sign(ty - y);
      while (x !== tx || y !== ty) { x += dx; y += dy; insert(x, y); }
    }
  }
  const points = [...occupied].map(k => k.split(',').map(Number)), xs = points.map(p => p[0]), ys = points.map(p => p[1]);
  const bounds = { x: Math.min(...xs), y: Math.min(...ys), width: Math.max(...xs) - Math.min(...xs) + 1, height: Math.max(...ys) - Math.min(...ys) + 1 };
  return { occupied, bounds, facility_cells: facilityCells, belt_cells: occupied.size - facilityCells,
    partial_area: bounds.width * bounds.height, empty_cells: bounds.width * bounds.height - occupied.size };
}
export function minimumEnvelope(snapshot, remaining, catalog, maximumArea) {
  if (!integer(maximumArea) || maximumArea <= 0 || remaining.length !== 2 || new Set(remaining.map(p => p.id)).size !== 2
    || remaining.some(p => !p.id || snapshot.placements.some(q => q.id === p.id))) fail('Expected two new named bodies and a positive area ceiling');
  const m = mask(snapshot, catalog), b = m.bounds;
  const shapes = remaining.map(p => {
    const f = catalog.facilities.find(f => f.id === p.facility);
    if (!f?.allowed_rotations?.length) fail('Unknown remaining facility or empty rotations', p.id);
    // Duplicate square orientations do not change this body-only relaxation.
    return [...new Map(f.allowed_rotations.map(rotation => {
      const r = shape({ ...p, rotation }, catalog); return [`${r.width},${r.height}`, r];
    })).values()];
  });
  const counts = { frames: 0, poses: 0, pairs: 0 }, frames = [];
  for (let width = b.width; width * b.height <= maximumArea; width++) {
    for (let height = b.height; width * height <= maximumArea; height++) {
      for (let x = b.x + b.width - width; x <= b.x; x++) for (let y = b.y + b.height - height; y <= b.y; y++) frames.push({ x, y, width, height });
    }
  }
  frames.sort((a, b) => a.width * a.height - b.width * b.height || a.width - b.width || a.x - b.x || a.y - b.y);
  let selected = null;
  for (const f of frames) {
    counts.frames++;
    const poses = shapes.map(variants => variants.flatMap(r => {
      const result = [];
      for (let x = f.x; x + r.width <= f.x + f.width; x++) for (let y = f.y; y + r.height <= f.y + f.height; y++) {
        counts.poses++; const p = { ...r, x, y };
        if (cells(p).every(([cx, cy]) => !m.occupied.has(key(cx, cy)))) result.push(p);
      }
      return result;
    }));
    outer: for (const a of poses[0]) for (const c of poses[1]) {
      counts.pairs++; if (!overlap(a, c)) { selected = { area: f.width * f.height, frame: f, placements: [a, c] }; break outer; }
    }
    if (selected) break;
  }
  const { occupied, ...metrics } = m;
  const code = selected ? 'envelope-minimum-found' : 'envelope-bound-exhausted';
  return { id: snapshot.id, ...metrics, maximum_examined_area: maximumArea, selected, counts, factory_validated: false,
    diagnostic: { code, severity: selected ? 'info' : 'warning', stage: 'completion-envelope', entity: snapshot.id,
      message: selected ? 'Minimum frozen-mask body-only envelope; routing and factory validity are untested' : 'No body packing within this diagnostic bound; not factory infeasibility' } };
}
export function analyze(spec, catalog) {
  if (spec.schema_version !== 1 || spec.snapshots?.length !== 2) fail('Expected schema 1 and two snapshots');
  const inventory = s => s.placements.map(p => `${p.id}:${p.facility}`).sort().join('|');
  if (inventory(spec.snapshots[0]) !== inventory(spec.snapshots[1])) fail('Snapshots must retain identical facility identities and types');
  const results = spec.snapshots.map(s => minimumEnvelope(s, spec.remaining, catalog, spec.maximum_examined_area));
  const base = spec.snapshots.find(s => s.id === spec.observed_completion?.base);
  if (!base || inventory(spec.observed_completion) !== inventory({ placements: spec.remaining })) fail('Completion must add exactly the remaining inventory');
  const { occupied, ...observed } = mask({ id: 'observed-completion', placements: [...base.placements, ...spec.observed_completion.placements],
    belt_polylines: [...base.belt_polylines, ...spec.observed_completion.belt_polylines] }, catalog);
  return { schema_version: 1, id: spec.id, scope: spec.scope, factory_validated: false, results, observed_completion: observed };
}
if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const args = process.argv.slice(2);
    if (args.length !== 4 || args[0] !== '--case' || args[2] !== '--facilities') fail('Usage: envelope.mjs --case PATH --facilities PATH');
    const read = p => { try { return JSON.parse(fs.readFileSync(p, 'utf8')); } catch (e) { fail(`Cannot read JSON ${p}: ${e.message}`, p); } };
    console.log(JSON.stringify(analyze(read(args[1]), read(args[3])), null, 2));
  } catch (e) {
    console.error(JSON.stringify({ code: e.code === 'envelope-input' ? e.code : 'envelope-input', severity: 'error', stage: 'completion-envelope', message: e.message, entity: e.entity ?? null }));
    process.exitCode = 1;
  }
}
