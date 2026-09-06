import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { analyze, mask, minimumEnvelope } from './envelope.mjs';
const read = p => JSON.parse(fs.readFileSync(new URL(p, import.meta.url)));
const spec = read('./completion-envelope.json');
const catalog = read('../../../data/game/normalized/facilities.json');

test('same-inventory partial ranking reverses in the frozen-mask body relaxation', () => {
  const r = analyze(spec, catalog);
  assert.deepEqual(r.results.map(x => [x.partial_area, x.empty_cells, x.selected.area]), [[168, 49, 210], [180, 59, 192]]);
  assert.equal(r.observed_completion.partial_area, 208);
  assert.equal(r.observed_completion.facility_cells, 144);
  assert.equal(r.factory_validated, false);
  assert.deepEqual(analyze(spec, catalog), r);
  for (let i = 0; i < 2; i++) {
    const s = spec.snapshots[i], result = r.results[i];
    const packed = mask({ ...s, placements: [...s.placements, ...result.selected.placements] }, catalog);
    assert.equal(packed.partial_area, result.selected.area);
  }
});
test('a tighter area bound is exhaustion, never factory infeasibility', () => {
  for (const [s, bound] of [[spec.snapshots[0], 209], [spec.snapshots[1], 191]]) {
    const r = minimumEnvelope(s, spec.remaining, catalog, bound);
    assert.equal(r.selected, null);
    assert.equal(r.diagnostic.code, 'envelope-bound-exhausted');
    assert.equal(r.factory_validated, false);
  }
});
test('translation preserves the result, including expansion into negative coordinates', () => {
  const s = structuredClone(spec.snapshots[1]);
  for (const p of s.placements) { p.x -= 20; p.y -= 30; }
  for (const line of s.belt_polylines) for (const p of line) { p[0] -= 20; p[1] -= 30; }
  const r = minimumEnvelope(s, spec.remaining, catalog, 260);
  assert.equal(r.selected.area, 192);
  assert.deepEqual(r.selected.frame, { x: -20, y: -30, width: 16, height: 12 });
});
test('malformed observations and mismatched inventory are rejected without repair', () => {
  const cases = [
    s => s.snapshots[0].placements[0].facility = 'missing',
    s => s.snapshots[0].placements[0].rotation = 45,
    s => s.snapshots[0].placements[0].x = 0.5,
    s => s.snapshots[0].placements[1].x = 6,
    s => s.snapshots[0].belt_polylines.push([[1, 4]]),
    s => s.snapshots[0].belt_polylines.push([[40, 40], [41, 41]]),
    s => s.snapshots[0].belt_polylines.push([[40, 40], [41, 40], [40, 40]]),
    s => s.snapshots[1].placements[0].id = 'different',
    s => s.remaining.pop(),
    s => s.remaining[0].id = 'seed',
    s => s.observed_completion.placements[0].facility = 'grinder-1-mode-normal',
    s => s.maximum_examined_area = -1,
  ];
  for (const change of cases) {
    const s = structuredClone(spec); change(s);
    assert.throws(() => analyze(s, catalog), e => e.code === 'envelope-input' && e.stage === 'completion-envelope');
  }
});
test('independent absolute-placement brute force agrees on six small two-obstacle cases', () => {
  const toy = { facilities: [
    { id: 'unit', footprint: { width: 1, height: 1 }, allowed_rotations: [0] },
    { id: 'domino', footprint: { width: 2, height: 1 }, allowed_rotations: [0, 90] },
  ] };
  const remaining = [{ id: 'a', facility: 'domino' }, { id: 'b', facility: 'domino' }];
  for (const [ox, oy] of [[1, 0], [0, 1], [1, 1], [2, 0], [2, 1], [2, 2]]) {
    const snapshot = { id: `${ox},${oy}`, placements: [{ id: 's', facility: 'unit', rotation: 0, x: 0, y: 0 },
      { id: 't', facility: 'unit', rotation: 0, x: ox, y: oy }], belt_polylines: [] };
    // Any envelope of area <=12 containing the origin lies within [-11,11]^2.
    // Enumerate body cell sets directly, without using the production frame/pose code.
    const fixed = ['0,0', `${ox},${oy}`], poses = [];
    for (let x = -11; x <= 11; x++) for (let y = -11; y <= 11; y++) {
      for (const [dx, dy] of [[1, 0], [0, 1]]) {
        const p = [[x, y], [x + dx, y + dy]];
        if (p.some(q => fixed.includes(q.join(',')))) continue;
        const points = [[0, 0], [ox, oy], ...p], xs = points.map(q => q[0]), ys = points.map(q => q[1]);
        if ((Math.max(...xs) - Math.min(...xs) + 1) * (Math.max(...ys) - Math.min(...ys) + 1) <= 12) poses.push(p);
      }
    }
    let best = Infinity;
    for (const a of poses) for (const b of poses) {
      if (a.some(p => b.some(q => p[0] === q[0] && p[1] === q[1]))) continue;
      const points = [[0, 0], [ox, oy], ...a, ...b], xs = points.map(q => q[0]), ys = points.map(q => q[1]);
      best = Math.min(best, (Math.max(...xs) - Math.min(...xs) + 1) * (Math.max(...ys) - Math.min(...ys) + 1));
    }
    assert.equal(minimumEnvelope(snapshot, remaining, toy, 12).selected.area, best);
  }
});
