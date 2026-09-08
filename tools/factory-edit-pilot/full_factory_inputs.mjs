// Offline original-request import; never reconstruct obligations from candidate geometry.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { prepareRoot, checkRoot } from '../../docs/benchmarks/layout-rl/coverage.mjs';
import { args, catalogKeys, read, write, hash, Probe } from '../../docs/benchmarks/layout-rl/io.mjs';
import { rate, add, dto, parseRate } from '../../docs/benchmarks/constructive-interconnect/rates.mjs';

export function originalRequest(graph, dialect) {
  if (!['chain-factory','closure-factory'].includes(dialect)) throw new Error(`Unknown original request dialect: ${dialect}`);
  const nodes = dialect === 'chain-factory' ? [...graph.nodes.values()] : graph.nodes;
  const edges = dialect === 'chain-factory' ? graph.lanes : graph.edges;
  const instances = nodes.map(n => ({ instance: n.id, recipe: n.recipe, utilization: n.utilization }));
  const members = new Set(instances.map(n => n.instance)), totals = new Map();
  for (const edge of edges) for (const [node, direction] of [[edge.source, 'output'], [edge.target, 'input']]) {
    const endpoint = members.has(node) ? { kind: 'facility', instance: node } : { kind: 'external', node };
    const key = JSON.stringify([endpoint, direction, edge.item]);
    const entry = totals.get(key) ?? { endpoint, direction, item: edge.item, rate: rate(0) };
    entry.rate = add(entry.rate, parseRate(edge.rate)); totals.set(key, entry);
  }
  return { schema_version: 1, instances, terminals: [...totals.values()].map(t => ({ ...t, rate: dto(t.rate) })) };
}

async function main(options) {
  const source = path.resolve(options.roots), dataset = read(source), output = path.resolve(options['output-directory']);
  if (dataset.schema_version !== 1 || !Array.isArray(dataset.roots)) throw new Error(`Invalid root source: ${source}`);
  const paths = Object.fromEntries(catalogKeys.map(k => [k, path.resolve(dataset.options[k])]));
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(paths[k])]));
  const validator = path.resolve(options.validator);
  fs.mkdirSync(output);
  write(path.join(output, 'catalogs.json'), paths);
  const probe = new Probe({ ...paths, validator }), rows = [];
  try {
    for (const root of dataset.roots) {
      if (!/^[a-z0-9-]+$/.test(root.id)) throw new Error(`Invalid root ID: ${root.id}`);
      const graph = prepareRoot(root, catalogs), request = originalRequest(graph, root.dialect);
      const validation = await probe.check(root.witness);
      const coverage = checkRoot(root, graph, root.witness, validation);
      if (!coverage.factory_validated) throw new Error(`Original root rejected: ${root.id}: ${JSON.stringify(coverage)}`);
      const directory = path.join(output, root.id); fs.mkdirSync(directory);
      write(path.join(directory, 'request.json'), request);
      write(path.join(directory, 'initial.json'), root.witness);
      write(path.join(directory, 'original-request.json'), root.request);
      write(path.join(directory, 'reference.json'), { validation, coverage });
      rows.push({ id: root.id, dialect: root.dialect, instances: request.instances.length,
        request: path.join(directory, 'request.json'), initial: path.join(directory, 'initial.json'),
        original: path.join(directory, 'original-request.json'),
        files_sha256: Object.fromEntries(['request.json','initial.json','original-request.json','reference.json'].map(n => [n, hash(path.join(directory,n))])) });
    }
  } finally { probe.close(); }
  const sources = [source, validator, ...Object.values(paths), fileURLToPath(import.meta.url)];
  write(path.join(output,'inputs.json'), { schema_version:1, source, rows, catalogs:path.join(output,'catalogs.json'),
    provenance:sources.map(p => ({path:p,sha256:hash(p)})), scope:'fresh acceptance of retained complete factories; no training or unseen-data claim' });
  console.log(JSON.stringify({status:'complete', cases:rows.length, facilities:rows.map(r=>r.instances), output}));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(args(['roots','validator','output-directory'])).catch(e => {
    console.error(JSON.stringify({code:'factory.import-failed',severity:'error',stage:'factory-import',message:e.message}));process.exitCode=1;
  });
}
