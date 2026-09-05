import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { placeOnly, rotateFacility } from './model.mjs';
import { render } from './render.mjs';
const base = JSON.parse(fs.readFileSync(new URL('../../../data/examples/constructive-network.facilities.json', import.meta.url)));
const items = { schema_version: 1, items: [{ id: 'solid', transport: 'belt' }, { id: 'fluid', transport: 'pipe' }] };
const wiring = pairs => ({ schema_version: 1, success: true,
  nodes: [...new Set(pairs.flatMap(p => p.slice(0, 2)))].map(id => ({ id, kind: 'facility', recipe: 'fixture', facility: base.facilities[0].id })),
  edges: pairs.map(([source, target, item = 'solid'], i) => ({ id: `edge-${i}`, source, target, item,
    rate: { numerator: 1, denominator: 5 } })),
});
test('one-to-one chains remain top-aligned with one empty cell despite port offsets', () => {
  const catalog = structuredClone(base), f = catalog.facilities[0];
  f.footprint = { width: 3, height: 5 };
  f.ports[0].position = { x: 0, y: 4 };
  f.ports[1].position = { x: 2, y: 0 };
  const w = wiring([['a', 'b'], ['b', 'c'], ['c', 'd']]);
  const r = placeOnly(w, catalog, items, 'd');
  assert.equal(r.placements.length, 4);
  assert.equal(new Set(r.placements.map(p => p.y)).size, 1);
  for (let i = 1; i < 4; i++) assert.equal(r.placements[i].x + r.placements[i].width + 1, r.placements[i - 1].x);
  assert.equal(r.routing_performed, false);
  assert.equal(r.factory_validated, false);
  assert.deepEqual(r.transport_networks, []);
  assert.deepEqual(r, placeOnly(w, catalog, items, 'd'));
});
test('pipe-supply branches precede belt-only branches and stay north in the same column', () => {
  const w = wiring([['pipe', 'target', 'fluid'], ['belt', 'target'], ['upstream', 'belt'], ['other', 'target']]);
  const r = placeOnly(w, base, items, 'target');
  assert.deepEqual(r.placements.map(p => p.instance), ['target', 'pipe', 'belt', 'other', 'upstream']);
  assert.equal(r.placement_decisions[1].row_shift, 0);
  assert.notEqual(r.placement_decisions[2].row_shift, 0);
  assert.equal(r.placement_decisions[4].row_shift, 0);
  assert.equal(r.placement_geometry_valid, true);
  const pipe = r.placements.find(p => p.instance === 'pipe');
  for (const id of ['belt', 'other']) {
    const p = r.placements.find(p => p.instance === id);
    assert(pipe.y + pipe.height + 1 <= p.y);
  }
});
test('branch priority sees upstream pipe outputs and counts shared/cyclic edges once', () => {
  const w = wiring([['left', 'target'], ['right', 'target'], ['up', 'right'],
    ['source', 'up', 'fluid'], ['source', 'right', 'fluid'], ['up', 'source']]);
  const r = placeOnly(w, base, items, 'target');
  assert.deepEqual(r.placements.slice(0, 3).map(p => p.instance), ['target', 'right', 'left']);
  const priorities = Object.fromEntries(r.placements.map(p => [p.instance, p.branch_pipe_outputs]));
  assert.deepEqual(priorities, { target: 2, right: 2, left: 0, up: 2, source: 2 });
  assert.deepEqual(r, placeOnly(w, base, items, 'target'));
});
test('north-facing pipe inputs attract upstream suppliers north; west-facing inputs do not', () => {
  const catalog = structuredClone(base), f = catalog.facilities[0];
  f.allowed_rotations = [0];
  f.ports.push({ id: 'fluid-in', direction: 'input', transport: 'pipe', position: { x: 0, y: 0 }, edge: 'north' },
    { id: 'fluid-out', direction: 'output', transport: 'pipe', position: { x: 0, y: 0 }, edge: 'north' });
  const w = wiring([['source', 'target', 'fluid']]);
  const north = placeOnly(w, catalog, items, 'target');
  assert.equal(north.pipe_relations[0].north_preference_applicable, true);
  assert.equal(north.pipe_relations[0].supplier_north, true);
  assert.equal(north.placement_decisions[1].row_choice.north_pipe_violations, 0);
  f.ports.find(p => p.id === 'fluid-in').edge = 'west';
  const west = placeOnly(w, catalog, items, 'target');
  assert.equal(west.pipe_relations[0].north_preference_applicable, false);
  assert.equal(west.placements[0].y, west.placements[1].y);
});
test('impossible cyclic north preferences stay soft and are diagnosed', () => {
  const catalog = structuredClone(base), f = catalog.facilities[0];
  f.allowed_rotations = [0];
  f.ports.push({ id: 'fluid-in', direction: 'input', transport: 'pipe', position: { x: 0, y: 0 }, edge: 'north' });
  const r = placeOnly(wiring([['a', 'b', 'fluid'], ['b', 'a', 'fluid']]), catalog, items, 'a');
  assert.equal(r.placements.length, 2);
  assert.equal(r.placement_geometry_valid, true);
  assert(r.diagnostics.some(d => d.code === 'placement-probe-north-pipe-preference-unmet'));
  assert.equal(r.factory_validated, false);
});
test('cycles, shared consumers and disconnected components all terminate with full coverage', () => {
  const w = wiring([['a', 'b'], ['b', 'a'], ['a', 'c'], ['d', 'e'], ['self', 'self']]);
  const r = placeOnly(w, base, items, 'c');
  assert.equal(r.placements.length, w.nodes.length);
  assert.equal(new Set(r.placements.map(p => p.instance)).size, w.nodes.length);
  assert(r.non_forward_requirements.length >= 2);
  assert.deepEqual(r, placeOnly(w, base, items, 'c'));
});
test('belt-input orientation wins over pipe output and mixed faces remain explicit', () => {
  const catalog = structuredClone(base), f = catalog.facilities[0];
  f.ports[0].edge = 'south';
  f.ports[1].transport = 'pipe';
  const r = placeOnly(wiring([['a', 'b'], ['b', 'c', 'fluid']]), catalog, items, 'c');
  const p = r.placements.find(p => p.instance === 'b');
  assert.equal(p.rotation, 90);
  assert.equal(p.alignment.belt_input.aligned, 1);
  assert.equal(p.alignment.pipe_output.aligned, 0);
  assert(r.diagnostics.some(d => d.entity === 'b' && d.code === 'placement-probe-mixed-port-faces'));
});
test('non-square rotations preserve catalog edge positions', () => {
  const f = { footprint: { width: 3, height: 5 }, ports: [{ id: 'p', position: { x: 2, y: 4 }, edge: 'east' }] };
  const r = rotateFacility(f, 90);
  assert.deepEqual([r.width, r.height, r.ports[0].position, r.ports[0].edge], [5, 3, { x: 0, y: 2 }, 'south']);
});
test('invalid references and schemas fail explicitly without inventing catalog geometry', () => {
  const w = wiring([['a', 'b']]);
  assert.throws(() => placeOnly(w, base, items, 'missing'), { code: 'placement-probe-target-invalid' });
  assert.throws(() => placeOnly({ ...w, schema_version: 2 }, base, items, 'a'), { code: 'placement-probe-schema-invalid' });
  const invalid = structuredClone(w); invalid.edges[0].item = 'missing';
  assert.throws(() => placeOnly(invalid, base, items, 'a'), { code: 'placement-probe-edge-invalid' });
  const c = structuredClone(base); c.facilities[0].ports[0].position.x = -1;
  assert.throws(() => placeOnly(w, c, items, 'a'), { code: 'placement-probe-port-invalid' });
});
test('HTML labels unrouted geometry and safely embeds instance IDs', () => {
  const r = placeOnly(wiring([['a</script>', 'b']]), base, items, 'b');
  const html = render(r);
  assert(html.includes('NOT A VALIDATED FACTORY'));
  assert(!html.includes('a</script>'));
  assert(html.includes('Logical edges (NOT routes)'));
  assert(!/id="show-graph"[^>]*checked/.test(html));
});
test('CLI writes separate timings and only a compact completion; rejects output collisions', () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-placement-cli-test-'));
  const files = Object.fromEntries(['wiring', 'facilities', 'items', 'report', 'html', 'timing'].map(k => [k, path.join(dir, k)]));
  for (const [key, value] of Object.entries({ wiring: wiring([['a', 'b']]), facilities: base, items }))
    fs.writeFileSync(files[key], JSON.stringify(value));
  const cli = fileURLToPath(new URL('./place.mjs', import.meta.url));
  const args = ['--wiring', files.wiring, '--facilities', files.facilities, '--items', files.items,
    '--target', 'b', '--report-output', files.report, '--visualization-output', files.html, '--timing-output', files.timing];
  const run = values => spawnSync(process.execPath, [cli, ...values], { encoding: 'utf8', timeout: 10000 });
  const collision = run([...args.slice(0, -1), files.report]);
  assert.equal(collision.status, 1);
  assert.equal(fs.existsSync(files.report), false);
  const result = run(args);
  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim().split('\n').length, 1);
  assert.equal(JSON.parse(result.stdout).status, 'placement-only-unrouted');
  const report = JSON.parse(fs.readFileSync(files.report)), timing = JSON.parse(fs.readFileSync(files.timing));
  assert.equal(timing.placement_ms, report.placement_ms);
  assert.equal(timing.report_bytes, fs.statSync(files.report).size);
  assert.equal(timing.html_bytes, fs.statSync(files.html).size);
  for (const [key, value] of Object.entries(timing)) if (key.endsWith('_ms')) assert(value >= 0 && Number.isFinite(value));
  assert(timing.measured_body_ms >= timing.placement_ms + timing.input_loading_ms);
  assert.equal(run(args).status, 1);
  // Remove only the exact temporary files created by this test; no recursive deletion.
  for (const file of Object.values(files)) fs.unlinkSync(file);
  fs.rmdirSync(dir);
});
