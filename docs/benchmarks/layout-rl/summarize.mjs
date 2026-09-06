import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args, diagnostic, read, write, hash, digest } from './io.mjs';
import { compare } from './score.mjs';

const counts = values => Object.fromEntries([...new Set(values)].sort().map(v => [v, values.filter(x => x === v).length]));
const sum = (rows, key) => rows.reduce((s, r) => s + (r[key] ?? 0), 0);
const range = values => {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return { min: sorted[0], median: (sorted[Math.floor((sorted.length - 1) / 2)] + sorted[Math.floor(sorted.length / 2)]) / 2,
    max: sorted.at(-1), mean: values.reduce((a, b) => a + b, 0) / values.length };
};
const policyName = p => p.model_seed === null ? p.signal : `${p.signal}-${p.model_seed}/${p.decoder}`;

try {
  const options = args(['plan', 'report-output']), planPath = path.resolve(options.plan), plan = read(planPath);
  if (plan.schema_version !== 1) throw new Error('Unsupported result plan');
  const resolve = p => path.resolve(path.dirname(planPath), p), provenance = [{ path: planPath, sha256: hash(planPath) },
    { path: fileURLToPath(import.meta.url), sha256: hash(fileURLToPath(import.meta.url)) }];
  const input = p => { const absolute = resolve(p); provenance.push({ path: absolute, sha256: hash(absolute) }); return read(absolute); };
  const dataset = input(plan.roots);
  const training = relative => {
    const report = input(relative);
    if (report.status !== 'complete') throw new Error('Incomplete training');
    return { elapsed_seconds: report.elapsed_seconds, config: report.config, device: report.device, torch: report.torch,
      runs: report.runs.map(run => {
        const detail = input(path.join(path.dirname(run.checkpoint.path), 'training.json'));
        const summaries = detail.groups.flatMap(g => g.summaries), last = detail.groups.slice(-128).flatMap(g => g.summaries);
        const validation = run.curves.flatMap(c => c.validation);
        const lastActions = detail.groups.slice(-8).flatMap(g => g.artifacts.flatMap(a => {
          if (hash(a.path) !== a.sha256) throw new Error('Changed training trajectory');
          return read(a.path).trace;
        }));
        return { name: run.name, signal: run.signal, seed: run.seed, parameters: run.parameters, checkpoint: run.checkpoint,
          sampled_transitions: run.sampled_transitions, changed_weight_elements: run.changed_weight_elements,
          unique_sampled_state_actions: run.unique_sampled_state_actions, elapsed_seconds: run.elapsed_seconds,
          informative_groups: detail.groups.filter(g => g.terminal_advantages.some(a => a !== 0)).length,
          training_episodes: summaries.length, improved_episodes: summaries.filter(s => compare(s.best_score.tuple, s.initial_score.tuple) < 0).length,
          last_128_groups: { episodes: last.length, improved: last.filter(s => compare(s.best_score.tuple, s.initial_score.tuple) < 0).length },
          last_8_groups: { actions: lastActions.length, statuses: counts(lastActions.map(t => t.status)),
            action_kinds: counts(lastActions.map(t => t.action.kind)) },
          work: Object.fromEntries(Object.keys(summaries[0].work).map(k => [k, summaries.reduce((s, r) => s + r.work[k], 0)])),
          validation_curve_episodes: validation.length,
          validation_curve_work: Object.fromEntries(Object.keys(validation[0].work).map(k => [k, validation.reduce((s, r) => s + r.work[k], 0)])),
          inference_seconds: sum(detail.groups, 'inference_seconds'), update_seconds: sum(detail.groups, 'update_seconds'),
          curves: run.curves.map(c => ({ group: c.group, roots: c.validation.map(r => ({ root: r.root, score: r.best_score.tuple })) })) };
      }) };
  };
  const evaluation = relative => {
    const report = input(relative);
    if (report.status !== 'complete') throw new Error('Incomplete evaluation');
    const rows = report.rows.map((row, index) => {
      if (hash(row.artifact.path) !== row.artifact.sha256) throw new Error(`Changed episode ${row.artifact.path}`);
      const episode = read(row.artifact.path);
      return { index, root: row.root, split: row.root_metadata.split, policy: policyName(row.policy), signal: row.policy.signal,
        model_seed: row.policy.model_seed, rollout_seed: row.rollout_seed, budget: row.budget,
        initial: row.initial_score.tuple, best: row.best_score.tuple, current: row.current_score.tuple, worst: row.worst_score,
        work: row.work, termination: row.termination, elapsed_ms: row.elapsed_ms, inference_seconds: row.inference_seconds,
        statuses: counts(episode.trace.map(t => t.status)), route_statuses: counts(episode.trace.flatMap(t => t.routes ?? []).map(r => r.status)),
        diagnostics: counts(episode.trace.flatMap(t => t.diagnostics ?? []).map(d => d.code)),
        incumbent_count: row.incumbents.length, best_witness_sha256: row.best_witness_sha256, artifact: row.artifact,
        fresh_factory_validated: row.audit.coverage.factory_validated };
    });
    const groups = new Map();
    for (const row of rows) {
      const key = JSON.stringify([row.root, row.budget, row.policy]);
      if (!groups.has(key)) groups.set(key, []); groups.get(key).push(row);
    }
    const aggregate = [...groups.values()].map(group => ({ root: group[0].root, split: group[0].split, budget: group[0].budget, policy: group[0].policy,
      trials: group.length, improved: group.filter(r => compare(r.best, r.initial) < 0).length,
      scores: counts(group.map(r => JSON.stringify(r.best))), proposals: range(group.map(r => r.work.proposals)), elapsed_ms: range(group.map(r => r.elapsed_ms)) }));
    const paired = [];
    for (const signal of ['terminal', 'myopic', 'zero', 'shuffled']) for (const split of new Set(rows.map(r => r.split)))
      for (const budget of report.config.budgets) for (const baseline of ['greedy', 'best-first', 'uniform']) {
        const group = rows.filter(r => r.signal === signal && r.split === split && JSON.stringify(r.budget) === JSON.stringify(budget));
        let wins = 0, ties = 0, losses = 0;
        for (const row of group) {
          const candidates = rows.filter(r => r.root === row.root && r.policy === baseline && JSON.stringify(r.budget) === JSON.stringify(budget));
          const opponent = candidates.find(r => r.rollout_seed === row.rollout_seed) ?? (candidates.length === 1 ? candidates[0] : null);
          if (!opponent) continue;
          const c = compare(row.best, opponent.best); if (c < 0) wins++; else if (c > 0) losses++; else ties++;
        }
        if (wins + ties + losses) paired.push({ signal, split, budget, baseline, wins, ties, losses });
      }
    return { config: report.config, elapsed_seconds: report.elapsed_seconds, rows, aggregate, paired,
      all_factory_validated: rows.every(r => r.fresh_factory_validated),
      work_mode_safety_hits: rows.filter(r => r.budget.mode === 'work' && r.termination === 'elapsed-budget').length,
      time_return_overrun_ms: range(rows.filter(r => r.budget.mode === 'time').map(r => Math.max(0, r.elapsed_ms - r.budget.elapsed_ms))) };
  };
  const workers = [];
  for (const split of ['train', 'large']) {
    let reference;
    for (const n of [1, 2, 4, 8]) {
      const report = input(`${plan.workers_prefix}${split}-${n}/report.json`);
      const stable = report.rows.map(row => {
        if (hash(row.artifact.path) !== row.artifact.sha256) throw new Error('Changed worker artifact');
        const episode = read(row.artifact.path);
        return { root: episode.root, best: episode.best_witness_sha256,
          trace: episode.trace.map(({ action, status, accepted, route_expansions, validator_attempted, witness_sha256, score, best }) =>
            ({ action, status, accepted, route_expansions, validator_attempted, witness_sha256, score, best })) };
      });
      const checksum = digest(stable);
      if (reference && reference !== checksum) throw new Error('Parallel worker trajectory divergence'); reference = checksum;
      workers.push({ split, workers: n, elapsed_seconds: report.elapsed_seconds, episodes: report.rows.length,
        proposals: report.rows.reduce((s, r) => s + r.work.proposals, 0), deterministic_trace_sha256: checksum });
    }
  }
  const audit = relative => { const report = input(relative); if (report.status !== 'complete') throw new Error('Incomplete audit');
    const { rows, provenance: ignored, ...summary } = report; return { ...summary, ...(rows ? { audited_rows: rows.length } : {}) }; };
  const checkpointReproductions = (plan.checkpoint_reproductions ?? []).map(([before, after]) => {
    const a = input(before), b = input(after);
    if (a.runs.length !== b.runs.length) throw new Error('Different reproduction model sets');
    return { before: resolve(before), after: resolve(after), runs: b.runs.map((r, i) => ({ name: r.name,
      bit_identical_checkpoint: r.name === a.runs[i].name && r.checkpoint.sha256 === a.runs[i].checkpoint.sha256 })) };
  });
  const result = { schema_version: 1, host: plan.host,
    roots: dataset.roots.map(({ id, generation, family, split, score, fingerprint, semantic_key, origin, witness }) =>
      ({ id, generation, family, split, score: score.tuple, fingerprint, semantic_key, origin, facilities: witness.placements.length })),
    main: { training: training(plan.main_training), evaluation: evaluation(plan.main_evaluation), audit: audit(plan.main_audit), policy_audit: audit(plan.main_policy_audit) },
    detour: { training: training(plan.detour_training), evaluation: evaluation(plan.detour_evaluation), proof: input(plan.detour_proof),
      audit: audit(plan.detour_audit), policy_audit: audit(plan.detour_policy_audit) },
    cpu: { workers, threads: [1, 2, 4, 8].map(n => input(`${plan.threads_prefix}${n}/report.json`)) },
    checkpoint_reproductions: checkpointReproductions, provenance };
  for (const p of provenance) if (hash(p.path) !== p.sha256) throw new Error(`Input changed: ${p.path}`);
  write(path.resolve(options['report-output']), result);
  console.log(JSON.stringify({ status: 'complete', main_rows: result.main.evaluation.rows.length, detour_rows: result.detour.evaluation.rows.length }));
} catch (e) { console.error(JSON.stringify(diagnostic('summary-failed', 'reporting', e.message))); process.exitCode = 1; }
