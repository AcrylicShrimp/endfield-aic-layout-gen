import { createHash } from 'node:crypto';
import { fields, fail } from './input.mjs';
import { prepareReplayPolicy } from './replay-policy.mjs';

const stable = value => Array.isArray(value) ? value.map(stable) : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map(k => [k, stable(value[k])])) : value;
const byId = values => [...values].sort((a, b) => a.id.localeCompare(b.id));
export function comparisonSignature(input) {
  const value = { schema_version: input.schema_version, id: input.id, nodes: byId(input.nodes), edges: byId(input.edges), external: byId(input.external),
    policy: { ...input.policy, families: [...input.policy.families].sort() }, boundary: input.boundary };
  return createHash('sha256').update(JSON.stringify(stable(value))).digest('hex');
}
export function prepareComparison(input, replayPolicy) {
  fields(input, ['schema_version', 'requests', 'proposal_budgets', 'factory_first_families', 'replay_orders', 'replay_rankings', 'replay_trial_limits'], 'comparison-plan');
  if (input.schema_version !== 1 || !Array.isArray(input.requests) || !input.requests.length || input.requests.length > 16) fail('comparison-input-invalid', 'Expected schema 1 with 1–16 comparison requests');
  for (const r of input.requests) {
    fields(r, ['id', 'path'], 'request');
    if (typeof r.id !== 'string' || !/^[a-zA-Z0-9_-]{1,128}$/.test(r.id) || typeof r.path !== 'string' || !r.path.trim() || r.path.includes('\0')) fail('comparison-input-invalid', 'Invalid comparison request identity or path', r.id);
  }
  if (new Set(input.requests.map(r => r.id)).size !== input.requests.length) fail('comparison-input-invalid', 'Duplicate comparison identity');
  const lists = { factory_first_families: ['straight', 'left', 'right', 'fold'], replay_orders: ['exhaustive', 'interleaved'], replay_rankings: ['partial-area', 'remaining-fit'] };
  for (const [k, allowed] of Object.entries(lists)) if (!Array.isArray(input[k]) || !input[k].length || new Set(input[k]).size !== input[k].length || input[k].some(v => !allowed.includes(v))) fail('comparison-input-invalid', `Invalid ${k}`, k);
  if (!Array.isArray(input.proposal_budgets) || !input.proposal_budgets.length || input.proposal_budgets.length > 16
    || input.proposal_budgets.some((n, i) => !Number.isSafeInteger(n) || n < 1 || i > 0 && n <= input.proposal_budgets[i - 1])) fail('comparison-input-invalid', 'Expected increasing positive proposal checkpoints');
  if (!Array.isArray(input.replay_trial_limits) || !input.replay_trial_limits.length || new Set(input.replay_trial_limits).size !== input.replay_trial_limits.length
    || input.replay_trial_limits.some(n => !Number.isSafeInteger(n) || n < 1 || n > replayPolicy.trials)) fail('comparison-input-invalid', 'Trial controls must not enlarge the original replay trial limit');
  const variants = input.factory_first_families.map(first => ({ id: `m3-${first}`, engine: 'factory', first }));
  for (const proposal_order of input.replay_orders) for (const ranking of input.replay_rankings) for (const trials of input.replay_trial_limits)
    variants.push({ id: `m4-${proposal_order}-${ranking}-${trials}`, engine: 'replay', proposal_order, ranking, trials });
  const count = input.requests.length * input.proposal_budgets.length * variants.length;
  if (count > 1024) fail('comparison-input-invalid', 'Comparison exceeds the explicit 1024-run diagnostic guard', count);
  return { ...structuredClone(input), variants, runs: count };
}
export function deriveComparison(original, replayPolicy, variant, proposals) {
  if (!Number.isSafeInteger(proposals) || proposals < 1 || proposals > original.budget.proposals) fail('comparison-input-invalid', 'Proposal checkpoint exceeds the original request cap', proposals);
  const request = structuredClone(original); request.budget.proposals = proposals;
  let policy = null;
  if (variant.engine === 'factory') {
    if (!request.policy.families.includes(variant.first)) fail('comparison-input-invalid', 'Control family is absent from the original request', variant.first);
    request.policy.families = [variant.first, ...request.policy.families.filter(f => f !== variant.first)];
  } else if (variant.engine === 'replay') policy = prepareReplayPolicy({ ...replayPolicy, proposal_order: variant.proposal_order, ranking: variant.ranking, trials: variant.trials });
  else fail('comparison-input-invalid', 'Unknown comparison engine', variant.engine);
  if (comparisonSignature(request) !== comparisonSignature(original)) fail('comparison-integrity', 'Derived request changed factory semantics');
  return { request, policy, assigned: { ...request.budget, fit_checks: replayPolicy.fit_checks } };
}
export function checkComparisonWork(report, assigned) {
  const used = { ...report.budget.used, fit_checks: report.budget.used.fit_checks ?? 0 };
  for (const k of ['proposals', 'route_expansions', 'validations', 'fit_checks']) {
    if (!Number.isSafeInteger(used[k]) || used[k] < 0 || used[k] > assigned[k]) fail('comparison-integrity', 'Reported work exceeds the assigned total allowance', k);
    if ((report.budget.limits[k] ?? (k === 'fit_checks' ? assigned.fit_checks : null)) !== assigned[k]) fail('comparison-integrity', 'Reported total allowance differs from the comparison', k);
  }
  if (report.budget.limits.elapsed_ms !== assigned.elapsed_ms) fail('comparison-integrity', 'Elapsed allowance changed');
  if (report.decisions.length !== used.proposals) fail('comparison-integrity', 'Proposal count differs from the decision trace');
  const expanded = report.decisions.reduce((n, d) => n + (d.route?.expanded ?? 0) + (d.access_trials ?? []).reduce((s, a) => s + a.expanded, 0), 0);
  if (expanded !== used.route_expansions) fail('comparison-integrity', 'Expansion count differs from A* and access traces');
  if ((report.pools ?? []).flatMap(p => p.candidates).reduce((n, c) => n + (c.fit?.checks ?? 0), 0) !== used.fit_checks) fail('comparison-integrity', 'Fit work differs from candidate estimates');
  return { used, unused: Object.fromEntries(Object.entries(used).map(([k, n]) => [k, assigned[k] - n])) };
}
export function summarizeComparison(rows) {
  const keys = new Set(), signatures = new Map(), allowances = new Map();
  for (const r of rows) {
    const k = JSON.stringify([r.case, r.variant.id, r.proposal_budget]);
    if (keys.has(k)) fail('comparison-integrity', 'Duplicate comparison row', k); keys.add(k);
    if (r.complete !== (r.score !== null) || r.complete && r.audit?.factory_validated !== true) fail('comparison-integrity', 'Complete score lacks independent original-request acceptance', k);
    if (r.complete && r.score.turn_definition !== 'transport-cells-including-facility-arms-v1') fail('comparison-integrity', 'Mixed turn definitions in comparison', k);
    if (signatures.has(r.case) && signatures.get(r.case) !== r.semantic_sha256) fail('comparison-integrity', 'Different inventory or boundary semantics in one comparison group', r.case);
    signatures.set(r.case, r.semantic_sha256);
    const group = JSON.stringify([r.case, r.proposal_budget]), assigned = JSON.stringify(stable(r.work.assigned));
    if (allowances.has(group) && allowances.get(group) !== assigned) fail('comparison-integrity', 'Unequal allocated total budgets in one comparison group', group);
    allowances.set(group, assigned);
  }
  const budgets = [...new Set(rows.map(r => r.proposal_budget))].sort((a, b) => a - b), variants = [...new Set(rows.map(r => r.variant.id))];
  return { rows: rows.length, complete: rows.filter(r => r.complete).length, stopped: rows.filter(r => !r.complete).length,
    completion_curve: variants.map(variant => ({ variant, checkpoints: budgets.map(proposals => {
      const matches = rows.filter(r => r.variant.id === variant && r.proposal_budget === proposals);
      return { proposals, completed: matches.filter(r => r.complete).length, requests: matches.length };
    }) })),
    per_request: [...new Set(rows.map(r => r.case))].map(id => ({ id, curves: variants.map(variant => ({ variant,
      checkpoints: rows.filter(r => r.case === id && r.variant.id === variant).sort((a, b) => a.proposal_budget - b.proposal_budget)
        .map(r => ({ proposals: r.proposal_budget, complete: r.complete, score: r.score?.tuple ?? null, used: r.work.used, termination: r.termination })) })) })) };
}
