import { compare } from './score.mjs';
import { digest, fail } from './io.mjs';

export function stateKey(w) {
  const order = xs => [...xs].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  return digest({ ...w, placements: order(w.placements), logistics_components: order(w.logistics_components),
    transport_networks: order(w.transport_networks.map(n => ({ ...n, cells: order(n.cells), segments: order(n.segments), terminals: order(n.terminals),
      requirement_ids: order(n.requirement_ids), component_ids: order(n.component_ids) }))) });
}

export function rankedActions(env) {
  const observation = env.observation();
  return env.actions.map((action, i) => ({ action, i, feature: observation.features.actions[i], allowed: observation.features.action_mask[i] }))
    .filter(a => a.allowed).sort((a, b) => Number(b.action.kind === 'compact') - Number(a.action.kind === 'compact')
      || a.feature[9] - b.feature[9] || a.feature[10] - b.feature[10] || a.i - b.i);
}

export async function search(env, { policy, beam_width = 8, branch_limit = 8 } = {}) {
  if (!['greedy', 'best-first'].includes(policy) || !Number.isSafeInteger(beam_width) || beam_width < 1 || beam_width > 64
    || !Number.isSafeInteger(branch_limit) || branch_limit < 1 || branch_limit > 64) fail('input-invalid', 'Invalid nonlearning search policy');
  let frontier = [{ witness: env.current, score: env.currentScore, depth: 0, serial: 0 }], serial = 1, expanded = 0;
  const visited = new Set([stateKey(env.current)]);
  while (frontier.length && !env.done) {
    const parent = frontier.shift(); env.current = parent.witness; env.currentScore = parent.score; env.candidates.clear(); expanded++;
    const actions = rankedActions(env); let children = 0;
    for (const { action } of actions) {
      if (env.done) break;
      const result = await env.evaluate(action, { advance: false });
      if (!result.accepted) continue;
      const child = env.candidates.get(result.candidate), key = stateKey(child.witness);
      if (visited.has(key)) continue; visited.add(key);
      if (policy === 'greedy') {
        if (compare(child.score.tuple, parent.score.tuple) >= 0) continue;
        frontier = [{ ...child, depth: parent.depth + 1, serial: serial++ }]; break;
      }
      frontier.push({ ...child, depth: parent.depth + 1, serial: serial++ }); children++;
      if (children >= branch_limit) break;
    }
    if (policy === 'best-first') frontier = frontier.sort((a, b) => compare(a.score.tuple, b.score.tuple) || a.depth - b.depth || a.serial - b.serial).slice(0, beam_width);
  }
  return { ...env.summary(), search: { policy, beam_width, branch_limit, expanded_states: expanded, distinct_states: visited.size,
    stopped: env.done ? 'budget' : 'frontier-exhausted', scope: 'bounded heuristic search; no global or neighborhood infeasibility claim' } };
}
