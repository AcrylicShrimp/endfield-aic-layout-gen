import test from 'node:test';
import assert from 'node:assert/strict';
import { exploreAxisPair, explorePlateau, witnessKey } from './transactions.mjs';
const result = (directory, area = 10, valid = true) => ({ directory, score: { tuple: [area, 1, 1] }, factory_validated: valid });
const better = (a, b) => a[0] < b[0];
test('a private equal-score prefix can improve after the other axis', async () => {
  const incumbent = Object.freeze(result('incumbent')), visited = [];
  const r = await exploreAxisPair(incumbent, 'xy', async (parent, axis, tie, branch) => {
    visited.push({ parent: parent.directory, axis, tie, branch });
    return result(branch, axis === 'y' && parent.directory === 'greatest' ? 9 : 10);
  }, better);
  assert.equal(visited.length, 6); assert.equal(r.selected.directory, 'greatest-least');
  assert.equal(incumbent.score.tuple[0], 10);
});
test('invalid prefixes are not expanded and invalid endpoints never replace the incumbent', async () => {
  const incumbent = result('incumbent');
  const r = await exploreAxisPair(incumbent, 'xy', async (_, axis, tie, branch) => result(branch, axis === 'y' ? 1 : 10, axis !== 'y' && tie === 'least'), better);
  assert.equal(r.stages.length, 4); assert.equal(r.selected, incumbent);
});
test('semantic keys ignore array/object order but retain coordinates, multiplicity and rates', () => {
  const a = { placements: [{ instance: 'a', x: 1 }, { instance: 'b', x: 2 }], transport_networks: [{ segments: [{ from: { x: 1, y: 0 }, to: { x: 2, y: 0 }, rate: { numerator: 1, denominator: 2 } }] }] };
  const b = structuredClone(a); b.placements.reverse();
  assert.equal(witnessKey(a), witnessKey(b));
  b.transport_networks[0].segments[0].rate.numerator = 2; assert.notEqual(witnessKey(a), witnessKey(b));
  b.transport_networks[0].segments[0].rate.numerator = 1; b.placements.push(b.placements[0]); assert.notEqual(witnessKey(a), witnessKey(b));
});
test('inferior siblings cannot consume the budget needed by an improved state', async () => {
  const scores = { S: 10, A: 10, B: 10, C: 9, E: 8 }, expanded = new Set();
  const generate = async (parent, axis, tie, _, context) => {
    let name = parent.directory;
    if (axis === 'x') {
      expanded.add(`${context.state}:${name}`);
      name = name === 'S' ? tie === 'least' ? 'A' : 'B' : name === 'A' ? 'C' : name === 'C' ? 'E' : name;
    }
    return result(name, scores[name]);
  };
  const r = await explorePlateau(result('S'), 'xy', generate, better, { stateBudget: 3, key: r => r.directory });
  assert.equal(r.selected.directory, 'E'); assert.equal(r.processed_states, 3);
  assert.equal(r.termination, 'state-budget'); assert.equal(r.deferred_states, 1); assert.equal(r.discarded_states, 1);
  assert.deepEqual([...expanded], ['0:S', '1:A', '2:C']);
});
test('round limits and actual plateau exhaustion are different outcomes', async () => {
  const generate = async (_, axis, tie) => result(axis + tie);
  const limited = await explorePlateau(result('S'), 'xy', generate, better, { rounds: 1, key: r => r.directory });
  assert.equal(limited.termination, 'round-budget'); assert.equal(limited.deferred_states, 4);
  const closed = await explorePlateau(result('S'), 'xy', generate, better, { key: r => r.directory });
  assert.equal(closed.termination, 'extremal-plateau-exhausted'); assert.equal(closed.processed_states, 5); assert.equal(closed.deferred_states, 0);
  const rejected = await explorePlateau(result('S'), 'xy', async () => null, better, { key: r => r.directory });
  assert.equal(rejected.termination, 'plateau-exhausted-with-diagnostics'); assert.equal(rejected.selected.directory, 'S');
  const invalid = await explorePlateau(result('S'), 'xy', async () => result('bad', 1, false), better, { key: r => r.directory });
  assert.equal(invalid.termination, 'plateau-exhausted-with-diagnostics'); assert.equal(invalid.seen_states, 1);
});
