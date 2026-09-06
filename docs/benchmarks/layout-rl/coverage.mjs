import { prepareFactory } from '../chain-prototype/factory-input.mjs';
import { checkFactoryState } from '../chain-prototype/factory-check.mjs';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { inspectCoverage } from '../constructive-factory-closure/model.mjs';
import { fail, digest } from './io.mjs';

export function prepareRoot(root, catalogs) {
  if (root.dialect === 'chain-factory') return prepareFactory(root.request, catalogs);
  if (root.dialect === 'closure-factory') return normalizeGraph(root.request, catalogs);
  fail('input-invalid', 'Unknown original graph dialect', root.dialect);
}
export function checkRoot(root, graph, witness, report) {
  if (root.dialect === 'chain-factory') return checkFactoryState(graph, witness, report, { complete: true });
  return inspectCoverage(graph, { witness }, report, true);
}
export function semanticKey(w) {
  const sort = xs => [...xs].sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  return digest({ placements: sort(w.placements.map(({ x, y, ...p }) => p)), utilizations: Object.fromEntries(Object.entries(w.utilizations).sort()),
    networks: sort(w.transport_networks.map(n => ({ id: n.id, item: n.item, transport: n.transport, requirements: sort(n.requirement_ids), components: sort(n.component_ids),
      terminals: sort(n.terminals.map(({ position, ...t }) => t)) }))),
    components: sort(w.logistics_components.map(({ position, ...c }) => c)) });
}
