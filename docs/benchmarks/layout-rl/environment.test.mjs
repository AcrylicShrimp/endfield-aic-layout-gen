import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import assert from 'node:assert/strict';
import { constructFactory } from '../chain-prototype/factory.mjs';
import { prepareFactory } from '../chain-prototype/factory-input.mjs';
import { completionScore } from '../chain-prototype/replay-score.mjs';
import { Environment, validateEnvironmentPlan } from './environment.mjs';
import { actionsFor, observe, tokenFields, actionFields } from './features.mjs';
import { canonicalKey, transform } from './geometry.mjs';
import { score, compare, terminalAdvantages } from './score.mjs';
import { semanticKey, checkRoot } from './coverage.mjs';
import { catalogKeys, read, Probe, digest } from './io.mjs';
import { drawing } from './visualize.mjs';

const repo = fileURLToPath(new URL('../../../', import.meta.url));
const options = Object.fromEntries(catalogKeys.map(k => [k, path.join(repo, 'data/game/normalized', `${k === 'components' ? 'logistics-components' : k}.json`)]));
options.validator = path.join(repo, 'target/release/aic-witness-probe');
const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(options[k])]));
const plan = read(new URL('./plan.json', import.meta.url)).environment;
let root, graph, probe;
test.before(async () => {
  probe = new Probe(options);
  const request = read(new URL('../chain-prototype/citron-factory.json', import.meta.url));
  request.policy.families = ['fold', ...request.policy.families.filter(f => f !== 'fold')];
  graph = prepareFactory(request, catalogs);
  const constructed = await constructFactory(graph, async w => {
    const report = await probe.check(w); return { status: report.covered_geometry_validated ? 0 : 2, report, artifact: null };
  });
  assert.equal(constructed.factory_validated, true);
  const witness = constructed.selected.witness;
  root = { id: 'generated-test-root', dialect: 'chain-factory', request, witness, semantic_key: semanticKey(witness), score: score(witness, catalogs) };
});
test.after(() => probe?.close());
const environment = () => new Environment({ environment: plan, options, roots: [root] }, probe);

test('visual projection preserves source geometry, identities and score without mutating the factory', () => {
  const witness = root.witness, original = digest(witness);
  const aliases = new Map(witness.placements.map((p, i) => [p.instance, `F${i + 1}`]));
  const picture = drawing(witness, aliases, catalogs);
  assert.deepEqual(picture.score, score(witness, catalogs));
  assert.equal(picture.facilities.length, witness.placements.length);
  const dx = witness.placements[0].x - picture.facilities[0].x;
  const dy = witness.placements[0].y - picture.facilities[0].y;
  const xy = p => [p.x - dx, p.y - dy];
  for (const [i, p] of witness.placements.entries()) {
    assert.deepEqual(picture.facilities[i], { alias: aliases.get(p.instance), facility: p.facility,
      recipe: p.recipe, x: p.x - dx, y: p.y - dy, width: p.width, height: p.height, rotation: p.rotation });
  }
  assert.equal(picture.networks.length, witness.transport_networks.length);
  for (const [i, network] of witness.transport_networks.entries()) {
    assert.deepEqual(picture.networks[i].cells, network.cells.map(xy));
    assert.deepEqual(picture.networks[i].segments, network.segments.map(s => [...xy(s.from), ...xy(s.to)]));
    assert.deepEqual(picture.networks[i].terminals, network.terminals.map(t => ({ xy: xy(t.position),
      direction: t.direction, owner: t.endpoint.kind === 'facility' ? aliases.get(t.endpoint.instance) : 'External',
      port: t.endpoint.port ?? null })));
  }
  assert.deepEqual(picture.components, witness.logistics_components.map(c => ({ xy: xy(c.position),
    kind: c.kind, transport: c.transport, rotation: c.rotation })));
  assert.equal(digest(witness), original);
});

test('terminal comparisons preserve score priority, ties and delayed credit without weighted sums', () => {
  assert.equal(compare([10, 999, 999], [11, 0, 0]), -1);
  assert.equal(compare([10, 5, 999], [10, 6, 0]), -1);
  assert.deepEqual(terminalAdvantages([[10, 99, 9], [11, 0, 0], [10, 99, 9]]), [.5, -1, .5]);
  assert.deepEqual(terminalAdvantages([[10, 5, 1], [10, 5, 1]]), [0, 0]);
  assert.throws(() => terminalAdvantages([[1, 2, 3]]));
});

test('environment rejects invalid resource limits without truncating domains', () => {
  validateEnvironmentPlan(plan);
  for (const invalid of [{ ...plan, horizon: 0 }, { ...plan, translation_steps: [0] }, { ...plan, translation_steps: [1, 1] }, { ...plan, max_tokens: 0 }])
    assert.throws(() => validateEnvironmentPlan(invalid), { code: 'rl-input-invalid' });
  assert.throws(() => actionsFor(root.witness, graph, { ...plan, max_actions: 1 }), { code: 'rl-action-bound' });
});

test('common score matches the earlier arm-aware chain score and legal transforms validate', async () => {
  assert.deepEqual(score(root.witness, catalogs).tuple, completionScore(graph, root.witness).tuple);
  const key = canonicalKey(root.witness, catalogs);
  for (let q = 0; q < 4; q++) {
    const witness = transform(root.witness, q), validation = await probe.check(witness);
    assert.equal(checkRoot(root, graph, witness, validation).factory_validated, true);
    assert.equal(canonicalKey(witness, catalogs), key);
    assert.deepEqual(score(witness, catalogs).tuple, root.score.tuple);
  }
});

test('pre-action feature allowlist has no candidate outcomes and both translation signs remain available', () => {
  const w = root.witness, actions = actionsFor(w, graph, plan), s = score(w, catalogs);
  assert.ok(actions.some(a => a.dx > 0)); assert.ok(actions.some(a => a.dx < 0));
  assert.ok(actions.some(a => a.dy > 0)); assert.ok(actions.some(a => a.dy < 0));
  const features = observe(w, graph, actions, s, s, s, 16, 16, plan);
  assert.ok(features.tokens.every(t => t.length === tokenFields.length));
  assert.ok(features.actions.every(a => a.length === actionFields.length));
  const contaminated = { ...w, reward: 999, oracle_score: [0, 0, 0] };
  assert.deepEqual(observe(contaminated, graph, actions.map(a => ({ ...a, accepted: true, final_score: [0, 0, 0] })), s, s, s, 16, 16, plan), features);
  assert.throws(() => observe(w, graph, actions, s, s, s, 16, 16, { ...plan, max_tokens: 1 }), { code: 'rl-token-bound' });
});

test('a real worse valid transition changes private state but not the best factory; cached features remain exact', async () => {
  const env = environment(), original = JSON.stringify(root.witness); await env.reset(root.id, { steps: 512, elapsed_ms: 10000 });
  let worse = null;
  for (const action of env.actions) {
    const result = await env.evaluate(action, { advance: false });
    if (result.accepted && compare(result.score, env.initial.tuple) > 0) { worse = action; break; }
  }
  assert.ok(worse, 'The actual generated factory must support a strictly worse valid move');
  await env.reset(root.id, { steps: 16, elapsed_ms: 10000 });
  const result = await env.evaluate(worse);
  assert.equal(result.accepted, true); assert.ok(compare(env.currentScore.tuple, env.bestScore.tuple) > 0);
  assert.equal(digest(env.best), digest(root.witness)); assert.equal(env.work.worse_transitions, 1);
  const a = env.observation(), b = env.observation(); assert.deepEqual(a.features, b.features);
  const fresh = observe(env.current, graph, env.actions, env.currentScore, env.bestScore, env.initial, 15, 16, plan);
  assert.deepEqual(a.features, fresh);
  assert.equal(checkRoot(root, graph, env.current, await probe.check(env.current)).factory_validated, true);
  assert.equal(JSON.stringify(root.witness), original);
  assert.equal(env.work.route_expansions, env.trace.reduce((sum, r) => sum + r.route_expansions, 0));
});

test('expired and exhausted episodes cannot replace or lose the complete incumbent', async () => {
  const env = environment(); await env.reset(root.id, { steps: 1, elapsed_ms: 10000 });
  await env.step(env.actions.length - 1);
  assert.equal(env.done, true); assert.equal(env.used, 1);
  const kept = digest(env.best), late = await env.step(0);
  assert.equal(late.transition.performed, false); assert.equal(env.used, 1); assert.equal(digest(env.best), kept);
  await env.reset(root.id); env.started -= env.elapsedLimit + 1;
  const expired = await env.step(0);
  assert.equal(expired.done, true); assert.equal(env.used, 0); assert.equal(digest(env.best), digest(root.witness));
});

test('a complete validator result received after the deadline is charged but never committed', async () => {
  const env = environment(); await env.reset(root.id, { steps: 512, elapsed_ms: 10000 });
  let routed;
  for (const action of env.actions) {
    const result = await env.evaluate(action, { advance: false });
    if (result.accepted && result.validator_attempted) { routed = action; break; }
  }
  assert.ok(routed);
  await env.reset(root.id, { steps: 16, elapsed_ms: 10000 });
  env.probe = { check: async (witness, timeoutMs) => {
    assert.equal(timeoutMs, plan.validator_ms, 'Process health timeout is independent of episode remainder');
    const result = await probe.check(witness);
    env.started -= env.elapsedLimit + 1; // Deterministic deadline crossing, without a slow/flaky timer.
    return result;
  } };
  const kept = digest(env.best), result = await env.evaluate(routed);
  assert.equal(result.status, 'late-result'); assert.equal(result.accepted, false);
  assert.equal(result.validator_attempted, true); assert.equal(env.work.validations, 1);
  assert.equal(env.work.proposals, 1); assert.equal(env.incumbents.length, 1);
  assert.equal(digest(env.current), kept); assert.equal(digest(env.best), kept);
  assert.equal(env.work.route_expansions, result.route_expansions);
});

test('a near-deadline episode never shortens the independent validator health timeout', async () => {
  const env = environment(); await env.reset(root.id, { steps: 512, elapsed_ms: 10000 });
  let routed;
  for (const action of env.actions) {
    const result = await env.evaluate(action, { advance: false });
    if (result.accepted && result.validator_attempted) { routed = action; break; }
  }
  await env.reset(root.id);
  // A deterministic clock boundary avoids relying on a scheduler delivering a timer in 1 ms.
  env.elapsedLimit = 1; env.expired = () => false;
  env.probe = { check: async (witness, timeoutMs) => {
    assert.equal(timeoutMs, plan.validator_ms);
    return probe.check(witness, timeoutMs);
  } };
  const result = await env.evaluate(routed);
  assert.equal(result.accepted, true); assert.equal(result.validator_attempted, true);
});
