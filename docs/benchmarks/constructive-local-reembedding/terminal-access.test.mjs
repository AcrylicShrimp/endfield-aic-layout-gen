import test from 'node:test';
import assert from 'node:assert/strict';
import { minimumTerminalReleasePath as solve, terminalAccessObstacles } from './terminal-access.mjs';
import { key } from './path.mjs';
const bounds = { minX: 0, minY: 0, maxX: 2, maxY: 2 }, start = { x: 0, y: 0 }, end = { x: 2, y: 2 };

test('all 2187 hard/soft/free 3x3 grids match exhaustive simple paths', () => {
  const cells = Array.from({ length: 9 }, (_, i) => ({ x: i % 3, y: Math.floor(i / 3) }));
  for (let mask = 0; mask < 3 ** 7; mask++) {
    const hard = new Set(), soft = new Map(); let digits = mask;
    for (const p of cells.slice(1, -1)) { const digit = digits % 3; digits = Math.floor(digits / 3);
      if (digit === 1) hard.add(key(p)); if (digit === 2) soft.set(key(p), [{ instance: key(p) }]); }
    let best = null;
    const visit = (p, seen, blocked, steps) => {
      if (key(p) === key(end)) { if (!best || blocked < best[0] || blocked === best[0] && steps < best[1]) best = [blocked, steps]; return; }
      for (const q of cells) if (Math.abs(p.x - q.x) + Math.abs(p.y - q.y) === 1 && !hard.has(key(q)) && !seen.has(key(q))) {
        seen.add(key(q)); visit(q, seen, blocked + Number(soft.has(key(q))), steps + 1); seen.delete(key(q));
      }
    };
    visit(start, new Set([key(start)]), 0, 0);
    const r = solve(start, end, hard, soft, bounds);
    assert.equal(r.status, best ? 'found' : 'no-path', `grid ${mask}`);
    if (best) {
      assert.deepEqual([r.blocker_cells, r.steps], best, `grid ${mask}`);
      assert.equal(new Set(r.cells.map(key)).size, r.cells.length);
      assert.ok(r.cells.every(p => !hard.has(key(p))));
      assert.deepEqual(solve(start, end, hard, soft, bounds), r);
    }
  }
});

test('endpoint exemptions never override hard bodies; bounds and work exhaustion are explicit', () => {
  const soft = new Map([[key(start), [{}]], [key(end), [{}]]]);
  assert.equal(solve(start, end, new Set(), soft, bounds).blocker_cells, 0);
  assert.equal(solve(start, start, new Set(), soft, bounds).steps, 0);
  assert.equal(solve(start, end, new Set([key(start)]), soft, bounds).status, 'blocked-endpoint');
  assert.equal(solve(start, { x: -1, y: 0 }, new Set(), soft, bounds).status, 'blocked-endpoint');
  const budget = solve(start, end, new Set(), soft, bounds, { workBudget: 1 });
  assert.equal(budget.status, 'work-budget'); assert.equal(budget.expanded, 1);
  assert.equal(budget.diagnostic.code, 'terminal-access-work-budget');
  for (const options of [{ workBudget: 0 }, { workBudget: 1.5 }]) assert.throws(() => solve(start, end, new Set(), soft, bounds, options), { code: 'terminal-access-invalid-input' });
  for (const b of [{ ...bounds, maxX: 250000 }, { ...bounds, maxX: -1 }, { ...bounds, minX: 0.5 }])
    assert.throws(() => solve(start, end, new Set(), soft, b), { code: 'terminal-access-invalid-input' });
  assert.throws(() => solve(start, end, new Set(), new Map([['1,0', []]]), bounds), { code: 'terminal-access-invalid-input' });
});

test('shared owners count once per cell, are all reported, and cannot mutate input', () => {
  const owners = [{ instance: 'a' }, { instance: 'b' }], soft = new Map([['1,0', owners]]), hard = new Set();
  const r = solve(start, { x: 2, y: 0 }, hard, soft, { ...bounds, maxY: 0 });
  assert.equal(r.blocker_cells, 1); assert.deepEqual(r.blockers[0].owners, owners);
  r.blockers[0].owners[0].instance = 'changed'; assert.equal(owners[0].instance, 'a');
  assert.equal(hard.size, 0); assert.equal(soft.size, 1);
  assert.equal(solve(start, { x: 2, y: 0 }, new Set(['1,0']), soft, { ...bounds, maxY: 0 }).status, 'no-path');
});

test('replay obstacles translate facility owners, preserve bodies and hard external terminals, and isolate layers', () => {
  const terminal = (id, kind, x) => ({ id, position: { x, y: 0 }, direction: 'input', endpoint: { kind, instance: id, port: 'input' } });
  const w = { transport_networks: [
    { transport: 'belt', item: 'ore', terminals: [terminal('a', 'facility', 0), terminal('b', 'facility', 0), terminal('e', 'external', 2), terminal('f', 'external', 3)] },
    { transport: 'pipe', item: 'water', terminals: [terminal('p', 'facility', 4), terminal('q', 'external', 5)] },
  ] }, before = structuredClone(w), body = new Set(['2,0']);
  const movement = { shiftTerminal: t => ({ x: t.position.x + Number(t.endpoint.kind === 'facility'), y: 0 }) };
  const { hard, soft } = terminalAccessObstacles(w, movement, 'belt', [{ x: 2, y: 0 }], body);
  assert.deepEqual([...hard], ['2,0', '3,0']); assert.deepEqual([...soft.keys()], ['1,0']);
  assert.deepEqual(soft.get('1,0').map(o => o.instance), ['a', 'b']);
  assert.equal(soft.get('1,0')[0].terminal, 'a'); assert.deepEqual(w, before); assert.equal(body.size, 1);
});
