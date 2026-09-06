import { Work } from './work.mjs';
import { WorkSlice } from './replay-work.mjs';
import { createFactoryKernel, selectTransaction } from './factory-kernel.mjs';
import { realizeBoundary } from './factory-boundary.mjs';
import { remainingFit } from './remaining-fit.mjs';
import { completionScore, compareTuple } from './replay-score.mjs';
import { preservedPrefix, rollbackWindow } from './replay-preservation.mjs';
import { diagnostic } from './input.mjs';

export async function constructReplay(graph, policy, validate, { now = () => performance.now() } = {}) {
  const work = new Work({ ...graph.budget, fit_checks: policy.fit_checks }, now); work.used.fit_checks = 0;
  const decisions = [], pools = [], trials = [], rollbacks = [], commits = [], incumbentHistory = [], diagnostics = [];
  const kernel = createFactoryKernel(graph, validate, work, decisions), seed = await kernel.seed();
  let retained = seed.validation?.accepted ? seed : null, selected = null, floor = null, paths = retained ? [seed] : [], frames = [], index = 1;
  const availableWork = () => {
    if (!work.alive()) return false;
    for (const k of ['proposals', 'route_expansions', 'validations']) if (work.used[k] >= work.limits[k]) { work.reason = k; return false; }
    return true;
  };
  if (retained) commits.push({ kind: 'seed', artifact: retained.artifact, covered: 1 });
  const pool = async (base, group, groupIndex) => {
    const seenFamilies = new Set(), id = pools.length, slice = new WorkSlice(work, policy.frontier_work), local = createFactoryKernel(graph, validate, slice, decisions,
      { proposalOrder: policy.proposal_order, context: { pool: id }, familyAllowed: family => !seenFamilies.has(family) });
    const record = { id, group: group.members, group_index: groupIndex, parent_artifact: base.artifact, candidates: [], work: null, ranking: policy.ranking }; pools.push(record);
    const candidates = [];
    for await (const candidate of local.candidates(base, group)) {
      const family = decisions[candidate.history[0].selected_ordinal].family;
      if (seenFamilies.has(family)) continue;
      seenFamilies.add(family);
      if (!candidate.validation.coverage.output_closed) throw new Error('Kernel yielded an open transaction');
      const preservation = preservedPrefix(base.witness, candidate.witness), partial = completionScore(graph, candidate.witness);
      const fit = policy.ranking === 'remaining-fit' ? remainingFit(graph, candidate.witness, new WorkSlice(work, { fit_checks: policy.fit_slice })) : null;
      const rank = fit ? [fit.complete ? 0 : 1, fit.estimated_area ?? partial.area, ...partial.tuple] : partial.tuple;
      const info = { id: record.candidates.length, family, artifact: candidate.artifact, history: candidate.history, partial_score: partial, fit, rank, preservation, continuation_trials: [] };
      record.candidates.push(info); candidates.push({ ...candidate, info });
      if (candidates.length === policy.shortlist || !work.alive()) break;
    }
    record.work = slice.report(); candidates.sort((a, b) => compareTuple(a.info.rank, b.info.rank) || a.info.id - b.info.id);
    record.ranked_candidate_ids = candidates.map(c => c.info.id);
    return { id, base, candidates, next: 0, selected: null };
  };
  while (retained && trials.length < policy.trials && availableWork()) {
    let stopped = null, boundary = null;
    for (; index < graph.transactions.length && availableWork(); index++) {
      const base = paths[index - 1];
      if (!frames[index]) frames[index] = await pool(base, graph.transactions[index], index);
      const frame = frames[index], candidate = frame.candidates[frame.next++];
      if (!candidate) { stopped = { stage: 'frontier', group: graph.transactions[index].members, pool: frame.id }; break; }
      frame.selected = candidate; selectTransaction(candidate, decisions, 'selected-replay-path'); paths[index] = candidate;
      if (candidate.witness.placements.length > retained.witness.placements.length) {
        retained = candidate; commits.push({ kind: 'retained-growth', artifact: retained.artifact, covered: retained.witness.placements.length, trial: trials.length });
      }
    }
    const internal = paths.at(-1), id = trials.length;
    if (internal.witness.placements.length === graph.nodes.size && availableWork()) {
      const slice = new WorkSlice(work, policy.frontier_work), local = createFactoryKernel(graph, validate, slice, decisions, { context: { trial: id } });
      boundary = await realizeBoundary(graph, internal, slice, local.checked, decisions);
      boundary.work = slice.report();
      if (boundary.selected) {
        const candidate = boundary.selected; preservedPrefix(internal.witness, candidate.witness);
        candidate.score = completionScore(graph, candidate.witness);
        if (!selected || compareTuple(candidate.score.tuple, selected.score.tuple) < 0) {
          selected = { ...candidate, witness: structuredClone(candidate.witness), trial: id };
          incumbentHistory.push({ trial: id, artifact: selected.artifact, score: selected.score });
        }
      } else stopped = { stage: 'boundary', work: boundary.work };
    }
    const path = frames.slice(1, paths.length).map(f => ({ pool: f.id, candidate: f.selected.info.id, artifact: f.selected.artifact }));
    const result = { id, path, covered: internal.witness.placements.length, internal_artifact: internal.artifact, complete: !!boundary?.selected,
      score: boundary?.selected?.score ?? null, selected: selected?.trial === id, stopped, boundary_trials: boundary?.trials ?? [], boundary_work: boundary?.work ?? null, work_at_end: { ...work.used } };
    trials.push(result);
    for (const p of path) pools[p.pool].candidates[p.candidate].continuation_trials.push(id);
    if (!availableWork() || trials.length === policy.trials) break;
    if (floor === null) {
      floor = paths.length;
      for (let start = 1; start < paths.length; start++) if (rollbackWindow(graph, paths[start - 1], internal, policy.rollback_facilities)) { floor = start; break; }
    }
    let replay = null;
    for (let start = paths.length - 1; start >= floor; start--) {
      const frame = frames[start];
      if (frame.next >= frame.candidates.length) continue;
      const window = rollbackWindow(graph, paths[start - 1], internal, policy.rollback_facilities);
      if (window) { replay = { after_trial: id, restart_group: start, fixed_episode_floor: floor, ...window }; break; }
    }
    if (!replay) { diagnostics.push(diagnostic('replay-window-exhausted', 'chain-replay', 'No untried candidate remains in the bounded connected suffix; not global infeasibility', { floor, covered: internal.witness.placements.length }, 'warning')); break; }
    rollbacks.push(replay); index = replay.restart_group; paths = paths.slice(0, index); frames = frames.slice(0, index + 1);
  }
  if (!retained) diagnostics.push(seed.validation?.diagnostic ?? diagnostic('replay-seed-failed', 'chain-replay', 'Seed was not validated'));
  if (work.reason) diagnostics.push(diagnostic('budget-exhausted', 'chain-replay', 'Shared search budget ended; valid retained state and complete incumbent are preserved', work.reason, 'warning'));
  else if (trials.length === policy.trials) diagnostics.push(diagnostic('replay-trial-limit', 'chain-replay', 'Configured routed-continuation count reached; retained valid state and complete incumbent are preserved', policy.trials, 'info'));
  return { schema_version: 1, id: graph.id, status: selected ? 'factory-validated' : 'replay-stopped', factory_validated: !!selected,
    policy, budget: work.report(), order: graph.order, transactions: graph.transactions, decisions, pools, trials, rollbacks, commits,
    incumbent_history: incumbentHistory, retained, selected, diagnostics, termination: diagnostics.at(-1), rollback_floor: floor };
}
