import { fields, fail } from './input.mjs';

export function prepareReplayPolicy(input) {
  fields(input, ['schema_version', 'ranking', 'proposal_order', 'shortlist', 'trials', 'rollback_facilities', 'frontier_work', 'fit_checks', 'fit_slice'], 'replay-policy');
  if (input.schema_version !== 1 || !['partial-area', 'remaining-fit'].includes(input.ranking) || !['exhaustive', 'interleaved'].includes(input.proposal_order)) fail('replay-input-invalid', 'Invalid replay schema, ranking or proposal order');
  for (const [k, max] of [['shortlist', 16], ['trials', 64], ['rollback_facilities', 16], ['fit_checks', 1000000], ['fit_slice', 100000]]) {
    if (!Number.isSafeInteger(input[k]) || input[k] < 1 || input[k] > max) fail('replay-input-invalid', `Invalid ${k}`, k);
  }
  fields(input.frontier_work, ['proposals', 'route_expansions', 'validations'], 'frontier_work');
  for (const [k, value] of Object.entries(input.frontier_work)) if (!Number.isSafeInteger(value) || value < 1 || value > 1000000) fail('replay-input-invalid', `Invalid frontier_work.${k}`, k);
  return structuredClone(input);
}
