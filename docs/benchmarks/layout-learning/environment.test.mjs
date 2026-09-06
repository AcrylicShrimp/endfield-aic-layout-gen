import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Environment, catalogKeys, fingerprint, loadGraphs, proposal, read, rewards, rng, improves } from './environment.mjs';
import { validatePlan } from './generate.mjs';
import { features, tokenFields, actionFields } from './features.mjs';
import { body, canonical, placement } from '../chain-prototype/geometry.mjs';

const directory = path.dirname(fileURLToPath(import.meta.url)), root = path.resolve(directory, '../../..');
const options = Object.fromEntries(catalogKeys.map(k => [k, path.join(root, 'data/game/normalized', `${k === 'components' ? 'logistics-components' : k}.json`)]));
options.validator = path.join(root, 'target/release/aic-witness-probe');
const planPath = path.join(directory, 'smoke.json'), plan = read(planPath);
const graphs = loadGraphs(plan, planPath, options);

test('fixed PRNG and lexicographic ordinal rewards include invalid actions and ties', () => {
  const a = rng(7), b = rng(7); assert.deepEqual(Array.from({ length: 30 }, a), Array.from({ length: 30 }, b));
  const o = tuple => ({ accepted: true, score: { tuple } });
  const outcomes = [o([20, 100, 9]), o([21, 1, 0]), o([20, 99, 5]), { accepted: false }, o([20, 99, 5])];
  assert.deepEqual(rewards(outcomes), [.75, .5, 1, 0, 1]);
  assert.equal(improves(outcomes[1], outcomes[0]), false); assert.equal(improves(outcomes[2], outcomes[0]), true);
  assert.equal(improves(outcomes[3], outcomes[0]), false);
});

test('experiment rejects invalid work limits and missing evaluation families', () => {
  validatePlan(plan);
  for (const p of [{ ...plan, actions: 1 }, { ...plan, elapsed_ms: 0 }, { ...plan, counts: { train: 1 } }, { ...plan, requests: plan.requests.filter(r => r.split === 'main') }]) assert.throws(() => validatePlan(p));
});

test('real routed fragments validate after translation; features have no post-action labels', () => {
  const output = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-learning-test-'));
  try {
    const env = new Environment(options, plan, output), graph = graphs[0].graph, random = rng(31);
    const sink = graph.nodes.get(graph.order[0]), edge = graph.growthEdges.at(-1);
    const base = { schema_version: 1, placements: [placement(sink, sink.facility_data.allowed_rotations[0])], utilizations: { [sink.id]: sink.utilization }, transport_networks: [], logistics_components: [] };
    assert.equal(env.validate(graph, base, {}).accepted, true);
    let action, outcome;
    for (let i = 0; i < 100; i++) {
      action = proposal(graph, base, edge, random, plan.max_gap);
      outcome = env.evaluate(graph, base, edge, action);
      if (outcome.accepted && (action.pose.x < 0 || action.pose.y < 0)) break;
    }
    assert.equal(outcome.accepted, true); assert.ok(action.pose.x < 0 || action.pose.y < 0);
    const current = env.witness(outcome), f = features(graph, base, current, edge, [action]);
    const probeWitness = read(path.join(output, outcome.artifact.directory, 'witness.json'));
    assert.deepEqual(probeWitness, canonical(current));
    assert.ok(probeWitness.placements.every(p => p.x >= 0 && p.y >= 0));
    assert.ok(f.tokens.every(t => t.length === tokenFields.length)); assert.equal(f.actions[0].length, actionFields.length);
    const noisy = structuredClone(current); noisy.rewards = [999]; noisy.validator_score = -99;
    const noisyEdge = { ...edge, rate: { numerator: 999, denominator: 1 } };
    assert.deepEqual(features(graph, base, noisy, noisyEdge, [{ ...action, reward: 9 }]), f);
    assert.deepEqual(Object.keys(f), ['tokens', 'adjacency', 'actions']);
    const invalid = { ...action, pose: { ...action.pose, x: 0, y: 0 } };
    const saved = structuredClone(base); assert.equal(env.evaluate(graph, base, edge, invalid).accepted, false); assert.deepEqual(base, saved);
    const rotated = structuredClone(current), rotate = p => ({ ...p, x: -p.y + 50, y: p.x + 30 });
    rotated.placements = current.placements.map(p => {
      const cells = body(p).map(rotate), node = graph.nodes.get(p.instance);
      return placement(node, (p.rotation + 90) % 360, Math.min(...cells.map(c => c.x)), Math.min(...cells.map(c => c.y)));
    });
    rotated.transport_networks = current.transport_networks.map(n => ({ ...n, cells: n.cells.map(rotate), segments: n.segments.map(s => ({ ...s, from: rotate(s.from), to: rotate(s.to) })), terminals: n.terminals.map(t => ({ ...t, position: rotate(t.position) })) }));
    assert.equal(fingerprint(graph, current), fingerprint(graph, rotated));
    assert.equal(fingerprint(graph, current), fingerprint(graph, canonical(current)));
  } finally { fs.rmSync(output, { recursive: true }); }
});
