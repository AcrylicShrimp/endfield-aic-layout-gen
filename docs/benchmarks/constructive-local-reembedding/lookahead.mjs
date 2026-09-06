import { createHash } from 'node:crypto';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { better } from '../constructive-factory-closure/model.mjs';
const fail = message => { throw Object.assign(new Error(message), { code: 'lookahead-input', severity: 'error', stage: 'lookahead-selection' }); };

export function selectLookaheadSeeds(entries, limit = 2) {
  if (!Array.isArray(entries) || !Number.isSafeInteger(limit) || limit < 1 || limit > 4) fail('Expected seed entries and limit 1..4');
  const unique = new Map();
  for (const entry of entries) {
    const score = witnessScore(entry.witness), signature = createHash('sha256').update(JSON.stringify(entry.witness)).digest('hex');
    if (!Number.isSafeInteger(entry.stage) || entry.stage < 0) fail('Expected a nonnegative seed source stage');
    const old = unique.get(signature);
    if (!old || entry.stage < old.stage) unique.set(signature, { ...entry, score, signature });
  }
  const seeds = [...unique.values()].sort((a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : a.stage - b.stage);
  return { eligible_records: entries.length, unique_witnesses: seeds.length, duplicates: entries.length - seeds.length, seeds: seeds.slice(0, limit) };
}

export function selectGlobalFactory(root, candidates) {
  if (root.factory_validated !== true) fail('The global root must pass fresh factory validation');
  const rootScore = witnessScore(root.witness);
  let selected = root, score = rootScore; const comparisons = [];
  for (const candidate of candidates) {
    const candidateScore = candidate.factory_validated === true ? witnessScore(candidate.witness) : null;
    const improves = candidateScore !== null && better(candidateScore.tuple, score.tuple);
    comparisons.push({ directory: candidate.directory ?? null, factory_validated: candidate.factory_validated === true,
      score: candidateScore, improves_global_root: candidateScore !== null && better(candidateScore.tuple, rootScore.tuple), improves_current: improves });
    if (improves) { selected = candidate; score = candidateScore; }
  }
  return { selected, score, comparisons };
}

export async function mapLookaheadBranches(entries, workers, execute) {
  if (!Array.isArray(entries) || !Number.isSafeInteger(workers) || workers < 1 || workers > 4) fail('Expected branch entries and workers 1..4');
  let next = 0; const results = Array(entries.length);
  const settled = await Promise.allSettled(Array.from({ length: Math.min(workers, entries.length) }, async () => {
    while (next < entries.length) { const index = next++; results[index] = await execute(entries[index], index); }
  }));
  const failure = settled.find(r => r.status === 'rejected'); if (failure) throw failure.reason;
  return results;
}
