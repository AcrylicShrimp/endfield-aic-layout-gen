import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import test from 'node:test';
import { prepareComparison, deriveComparison, comparisonSignature, checkComparisonWork, summarizeComparison } from './comparison.mjs';
import { prepareReplayPolicy } from './replay-policy.mjs';

const root = fileURLToPath(new URL('../../../', import.meta.url)), read = p => JSON.parse(fs.readFileSync(p));
const originalPlan = () => read(new URL('./comparison-plan.json', import.meta.url));
const policy = prepareReplayPolicy(read(new URL('./replay-policy.json', import.meta.url)));
const request = name => read(new URL(`./${name}-factory.json`, import.meta.url));
const assigned = { proposals: 128, route_expansions: 200000, validations: 512, elapsed_ms: 30000, fit_checks: 8192 };
test('the primary experiment assigns 192 separate runs without a free best-of-variants portfolio', () => {
  const plan = prepareComparison(originalPlan(), policy);
  assert.equal(plan.runs, 192); assert.equal(plan.variants.length, 12); assert.deepEqual(plan.proposal_budgets, [128, 256, 512, 1024]);
  assert.equal(plan.variants.filter(v => v.engine === 'factory').length, 4); assert.equal(plan.variants.filter(v => v.engine === 'replay').length, 8);
  assert.equal(new Set(plan.variants.map(v => v.id)).size, 12);
});
test('the explicit Partial replay example is one uniform measured control, not a request-specific dispatcher', () => {
  const example = prepareReplayPolicy(read(new URL('./replay-partial-policy.json', import.meta.url)));
  assert.deepEqual(example, { ...policy, ranking: 'partial-area' });
  for (const name of ['battery', 'citron', 'iron-nine', 'iron-eight']) {
    const d = deriveComparison(request(name), policy, { engine: 'replay', proposal_order: 'interleaved', ranking: 'partial-area', trials: 8 }, 1024);
    assert.deepEqual(d.policy, example);
  }
});
test('derived controls preserve exact inventory, flow, geometry policy and boundary semantics at equal total caps', () => {
  const plan = prepareComparison(originalPlan(), policy);
  for (const name of ['battery', 'citron', 'iron-nine', 'iron-eight']) {
    const original = request(name), before = JSON.stringify(original), signature = comparisonSignature(original);
    for (const v of plan.variants) for (const proposals of plan.proposal_budgets) {
      const d = deriveComparison(original, policy, v, proposals);
      assert.equal(comparisonSignature(d.request), signature); assert.deepEqual(d.assigned, { ...original.budget, proposals, fit_checks: policy.fit_checks });
      assert.deepEqual(d.request.nodes, original.nodes); assert.deepEqual(d.request.edges, original.edges); assert.deepEqual(d.request.external, original.external); assert.deepEqual(d.request.boundary, original.boundary);
      if (v.engine === 'replay') assert.equal(d.policy.trials, v.trials); else assert.equal(d.request.policy.families[0], v.first);
    }
    assert.equal(JSON.stringify(original), before);
  }
});
test('signature is array-order independent but distinguishes inventory, utilized rates and boundary choices', () => {
  const a = request('iron-nine'), b = structuredClone(a); b.nodes.reverse(); b.edges.reverse(); b.external.reverse(); b.policy.families.reverse();
  assert.equal(comparisonSignature(a), comparisonSignature(b)); assert.notEqual(comparisonSignature(a), comparisonSignature(request('iron-eight')));
  for (const mutate of [s => s.nodes[0].utilization.denominator++, s => s.external[0].rate.denominator++, s => s.boundary.margins = [2]]) {
    const changed = structuredClone(a); mutate(changed); assert.notEqual(comparisonSignature(a), comparisonSignature(changed));
  }
});
test('invalid plans and unapproved cap increases reject before executing any constructor', () => {
  for (const mutate of [s => s.schema_version = 2, s => s.requests[0].id = '../escape', s => s.requests.push(s.requests[0]), s => s.proposal_budgets = [256, 128],
    s => s.factory_first_families = ['straight', 'straight'], s => s.replay_orders = ['random'], s => s.replay_trial_limits = [9], s => s.max_width = 10]) {
    const p = originalPlan(); mutate(p); assert.throws(() => prepareComparison(p, policy));
  }
  assert.throws(() => deriveComparison(request('battery'), policy, { engine: 'factory', first: 'straight' }, 1025));
  const p = request('battery'); p.policy.families = ['straight']; assert.throws(() => deriveComparison(p, policy, { engine: 'factory', first: 'right' }, 128));
});
test('work verification checks allocated vectors, direct route/access work and reported fit checks', () => {
  const r = { budget: { limits: { proposals: 128, route_expansions: 200000, validations: 512, elapsed_ms: 30000 }, used: { proposals: 2, route_expansions: 3, validations: 1 } },
    decisions: [{ route: { expanded: 2 } }, { access_trials: [{ expanded: 1 }] }] };
  assert.deepEqual(checkComparisonWork(r, assigned).used, { proposals: 2, route_expansions: 3, validations: 1, fit_checks: 0 });
  assert.equal(checkComparisonWork(r, assigned).unused.proposals, 126);
  for (const mutate of [s => s.budget.used.validations = 513, s => s.budget.limits.proposals = 129, s => s.decisions.pop(), s => s.decisions[0].route.expanded++,
    s => { s.budget.used.fit_checks = 1; }]) { const changed = structuredClone(r); mutate(changed); assert.throws(() => checkComparisonWork(changed, assigned)); }
});
function row(overrides = {}) {
  return { case: 'iron-nine', semantic_sha256: 'nine', variant: { id: 'm3-straight' }, proposal_budget: 128, complete: true,
    score: { tuple: [36, 1, 1], turn_definition: 'transport-cells-including-facility-arms-v1' }, audit: { factory_validated: true },
    work: { assigned, used: { proposals: 2, route_expansions: 3, validations: 1, fit_checks: 0 } }, termination: { code: 'chain-first-completion' }, ...overrides };
}
test('summary refuses unaudited scores, mixed metrics, mixed inventories, unequal budgets and duplicate rows', () => {
  const a = row(); assert.deepEqual(summarizeComparison([a]), summarizeComparison([structuredClone(a)]));
  for (const changed of [row({ audit: null }), row({ score: null }), row({ complete: false }), row({ score: { ...a.score, turn_definition: 'segment-only' } })]) assert.throws(() => summarizeComparison([changed]));
  assert.throws(() => summarizeComparison([a, structuredClone(a)]));
  assert.throws(() => summarizeComparison([a, row({ variant: { id: 'second' }, semantic_sha256: 'eight' })]));
  assert.throws(() => summarizeComparison([a, row({ variant: { id: 'second' }, work: { ...a.work, assigned: { ...assigned, validations: 1024 } } })]));
  const separate = summarizeComparison([a, row({ case: 'iron-eight', semantic_sha256: 'eight' })]); assert.equal(separate.per_request.length, 2);
});
test('completion curves preserve stopped rows and separate every variant and inventory', () => {
  const rows = [row(), row({ proposal_budget: 256, work: { assigned: { ...assigned, proposals: 256 }, used: {} } }),
    row({ variant: { id: 'fit' }, complete: false, score: null, audit: null })];
  const summary = summarizeComparison(rows); assert.equal(summary.complete, 2); assert.equal(summary.stopped, 1);
  assert.equal(summary.completion_curve[0].checkpoints[0].completed, 1); assert.equal(summary.completion_curve[1].checkpoints[0].completed, 0);
  assert.equal(summary.per_request[0].curves[1].checkpoints[0].score, null);
});
test('comparison CLI audits complete rows, normalizes turns, retains failures and rejects immutable-output reuse', t => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), 'aic-comparison-cli-')); t.after(() => fs.rmSync(directory, { recursive: true, force: true }));
  const common = ['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, path.join(root, 'data/game/normalized', `${k === 'components' ? 'logistics-components' : k}.json`)]);
  for (const complete of [true, false]) {
    const source = request('citron'), cap = complete ? 1024 : 1; source.budget.proposals = cap;
    const sourcePath = path.join(directory, `${complete}-source.json`), planPath = path.join(directory, `${complete}-plan.json`), output = path.join(directory, `${complete}-output`);
    fs.writeFileSync(sourcePath, JSON.stringify(source), { flag: 'wx' });
    const p = { ...originalPlan(), requests: [{ id: 'citron', path: sourcePath }], proposal_budgets: [cap], factory_first_families: ['fold'], replay_orders: ['interleaved'], replay_rankings: ['partial-area'], replay_trial_limits: [1] };
    fs.writeFileSync(planPath, JSON.stringify(p), { flag: 'wx' });
    const args = [fileURLToPath(new URL('./run-comparison.mjs', import.meta.url)), '--plan', planPath,
      '--policy', fileURLToPath(new URL('./replay-policy.json', import.meta.url)), ...common, '--validator', path.join(root, 'target/release/aic-witness-probe'), '--output-directory', output];
    const run = () => spawnSync(process.execPath, args, { encoding: 'utf8', timeout: 30000 });
    const result = run(); assert.ifError(result.error); assert.equal(result.status, 0, result.stderr);
    const reportPath = path.join(output, 'report.json'), report = read(reportPath), before = fs.readFileSync(reportPath, 'utf8');
    assert.equal(report.rows.length, 2); assert.equal(report.summary.complete, complete ? 2 : 0);
    for (const r of report.rows) {
      assert.equal(r.complete, complete); assert.equal(r.work.assigned.proposals, cap); assert.ok(r.work.used.proposals <= cap);
      if (complete) { assert.equal(r.audit.factory_validated, true); assert.ok(fs.existsSync(r.audit.report)); }
      else { assert.equal(r.score, null); assert.equal(r.audit, null); assert.equal(r.witness, null); }
    }
    if (complete) { assert.equal(report.rows[0].producer_score.turns, 0); assert.ok(report.rows[0].score.turns > 0); }
    assert.equal(run().status, 1); assert.equal(fs.readFileSync(reportPath, 'utf8'), before);
  }
});
