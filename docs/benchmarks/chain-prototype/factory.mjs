import { Work } from './work.mjs';
import { diagnostic } from './input.mjs';
import { realizeBoundary } from './factory-boundary.mjs';
import { createFactoryKernel, selectTransaction } from './factory-kernel.mjs';

export async function constructFactory(graph, validate, { now = () => performance.now() } = {}) {
  const work = new Work(graph.budget, now), decisions = [], commits = [], diagnostics = [];
  const kernel = createFactoryKernel(graph, validate, work, decisions);
  let retained = null, boundary = null;
  const seed = await kernel.seed();
  if (seed.validation?.accepted) {
    retained = seed;
    commits.push({ kind: 'seed', members: [graph.order[0]], artifact: retained.artifact, history: [] });
    for (const group of graph.transactions.slice(1)) {
      let next = null;
      for await (const candidate of kernel.candidates(retained, group)) { next = candidate; break; }
      if (!next || !next.validation.coverage.output_closed) {
        diagnostics.push(diagnostic(work.reason ? 'budget-exhausted' : 'no-complete-transaction', 'factory-growth', 'Current supplier transaction did not close; retained earlier committed geometry', group.members, 'warning')); break;
      }
      retained = next; selectTransaction(next, decisions);
      commits.push({ kind: 'supplier-component', members: retained.witness.placements.map(p => p.instance), added: group.members, artifact: retained.artifact, history: next.history });
      if (kernel.state.deepest?.artifact === retained.artifact) kernel.state.deepest = null;
    }
    if (retained.witness.placements.length === graph.nodes.size) boundary = await realizeBoundary(graph, retained, work, kernel.checked, decisions);
  } else diagnostics.push(seed.validation?.diagnostic ?? diagnostic('budget-exhausted', 'factory-seed', 'Seed could not be validated', work.reason));
  const selected = boundary?.selected ?? null;
  if (boundary && !selected) diagnostics.push(diagnostic(work.reason ? 'budget-exhausted' : 'boundary-not-closed', 'factory-boundary', 'External routing transaction failed; internal layout retained', null, 'warning'));
  if (selected) commits.push({ kind: 'external-closure', members: graph.order, artifact: selected.artifact, history: selected.history });
  return { schema_version: 1, id: graph.id, status: selected ? 'factory-validated' : 'factory-stopped', factory_validated: !!selected,
    budget: work.report(), order: graph.order, transactions: graph.transactions, commits, retained, deepest_private: kernel.state.deepest,
    boundary_trials: boundary?.trials ?? [], decisions, diagnostics, selected };
}
