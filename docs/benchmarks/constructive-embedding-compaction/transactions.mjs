import { createHash } from 'node:crypto';

// All arrays in witness v1 are unordered entity/geometry collections, not path vertex sequences.
export function witnessKey(witness) {
  const canonical = value => Array.isArray(value) ? '[' + value.map(canonical).sort().join(',') + ']' :
    value !== null && typeof value === 'object' ? '{' + Object.keys(value).sort().map(k => JSON.stringify(k) + ':' + canonical(value[k])).join(',') + '}' : JSON.stringify(value);
  return createHash('sha256').update(canonical(witness)).digest('hex');
}

// A same-score prefix stays private but remains eligible for the perpendicular pass.
export async function exploreAxisPair(incumbent, order, generateAndValidate, better) {
  if (!['xy', 'yx'].includes(order)) throw new Error('Expected xy or yx axis order');
  let selected = incumbent;
  const stages = [];
  const attempt = async (parent, axis, tie, branch, phase) => {
    const result = await generateAndValidate(parent, axis, tie, branch);
    stages.push({ parent_directory: parent.directory, axis, tie, branch, phase, result });
    if (result?.factory_validated && better(result.score.tuple, selected.score.tuple)) selected = result;
    return result;
  };
  for (const firstTie of ['least', 'greatest']) {
    const first = await attempt(incumbent, order[0], firstTie, firstTie, 'prefix');
    if (!first?.factory_validated) continue;
    for (const secondTie of ['least', 'greatest']) await attempt(first, order[1], secondTie, `${firstTie}-${secondTie}`, 'endpoint');
  }
  return { selected, stages };
}

export async function explorePlateau(incumbent, order, generateAndValidate, better,
  { rounds = 8, stateBudget = 16, key = r => witnessKey(r.detail.witness) } = {}) {
  if (!Number.isSafeInteger(rounds) || rounds < 1 || rounds > 32 || !Number.isSafeInteger(stateBudget) || stateBudget < 1 || stateBudget > 128)
    throw Object.assign(new Error('Expected rounds 1..32 and state budget 1..128'), { code: 'compaction-plateau-budget' });
  let selected = incumbent, termination = 'round-budget', frontier = [selected], processed = 0, deferred = 0, discarded = 0, rejected = false;
  const seen = new Set([key(selected)]), plateau = [], stages = [];
  for (let round = 0; round < rounds && frontier.length; round++) {
    const next = [];
    for (const [index, parent] of frontier.entries()) {
      // A sibling may already have improved the incumbent during this round.
      if (better(selected.score.tuple, parent.score.tuple)) { discarded++; continue; }
      if (processed >= stateBudget) {
        termination = 'state-budget';
        deferred += frontier.slice(index).filter(r => !better(selected.score.tuple, r.score.tuple)).length;
        discarded += frontier.slice(index).filter(r => better(selected.score.tuple, r.score.tuple)).length;
        break;
      }
      const state = processed++, transaction = await exploreAxisPair(parent, order,
        (input, axis, tie, branch) => generateAndValidate(input, axis, tie, branch, { round, state }), better);
      for (const { result, ...context } of transaction.stages) {
        if (!result) { rejected = true; continue; }
        const semanticKey = key(result), duplicate = seen.has(semanticKey);
        stages.push({ round, state, ...context, ...Object.fromEntries(Object.entries(result).filter(([k]) => k !== 'detail')), semantic_key: semanticKey, duplicate });
        if (!result.factory_validated) { rejected = true; continue; }
        if (better(result.score.tuple, selected.score.tuple)) selected = result;
        if (!duplicate) { seen.add(semanticKey); next.push(result); }
      }
    }
    frontier = next.filter(r => !better(selected.score.tuple, r.score.tuple));
    discarded += next.length - frontier.length;
    plateau.push({ round, processed_states: processed, seen_states: seen.size, next_states: frontier.length, score: selected.score });
    if (termination === 'state-budget') { deferred += frontier.length; break; }
    if (!frontier.length) { termination = rejected ? 'plateau-exhausted-with-diagnostics' : 'extremal-plateau-exhausted'; break; }
  }
  if (termination === 'round-budget') deferred += frontier.length;
  return { selected, termination, processed_states: processed, seen_states: seen.size, deferred_states: deferred, discarded_states: discarded, plateau,
    stages: stages.map(s => ({ ...s, selected: s.directory === selected.directory })) };
}
