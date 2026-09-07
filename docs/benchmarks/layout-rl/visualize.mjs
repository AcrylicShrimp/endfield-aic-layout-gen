import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args, read, write, hash, digest, catalogKeys, Probe, diagnostic } from './io.mjs';
import { score, compare } from './score.mjs';
import { prepareRoot, checkRoot, semanticKey } from './coverage.mjs';

export function drawing(witness, aliases, catalogs) {
  const points = [...witness.placements.flatMap(p => [[p.x, p.y], [p.x + p.width, p.y + p.height]]),
    ...witness.transport_networks.flatMap(n => n.cells.flatMap(p => [[p.x, p.y], [p.x + 1, p.y + 1]])),
    ...witness.logistics_components.flatMap(c => [[c.position.x, c.position.y], [c.position.x + 1, c.position.y + 1]])];
  const minX = Math.min(...points.map(p => p[0])), minY = Math.min(...points.map(p => p[1]));
  const xy = p => [p.x - minX, p.y - minY];
  return { score: score(witness, catalogs),
    facilities: witness.placements.map(p => ({ alias: aliases.get(p.instance), facility: p.facility, recipe: p.recipe,
      x: p.x - minX, y: p.y - minY, width: p.width, height: p.height, rotation: p.rotation })),
    networks: witness.transport_networks.map(n => ({ item: n.item, kind: n.transport,
      cells: n.cells.map(xy), segments: n.segments.map(s => [...xy(s.from), ...xy(s.to)]),
      terminals: n.terminals.map(t => ({ xy: xy(t.position), direction: t.direction,
        owner: t.endpoint.kind === 'facility' ? aliases.get(t.endpoint.instance) : 'External', port: t.endpoint.port ?? null })) })),
    components: witness.logistics_components.map(c => ({ xy: xy(c.position), kind: c.kind, transport: c.transport, rotation: c.rotation })) };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const options = args(['plan', 'output-directory']), planPath = path.resolve(options.plan), plan = read(planPath), resolve = p => path.resolve(path.dirname(planPath), p);
    if (plan.schema_version !== 1 || !plan.cases?.length) throw new Error('Invalid visual comparison plan');
    const rootsPath = resolve(plan.roots), resultsPath = resolve(plan.results), dataset = read(rootsPath), results = read(resultsPath);
    for (const p of dataset.provenance) if (hash(p.path) !== p.sha256) throw new Error(`Root provenance changed: ${p.path}`);
    const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(dataset.options[k])])), probe = new Probe(dataset.options), cases = [], audits = [];
    const inputs = [planPath, rootsPath, resultsPath, fileURLToPath(import.meta.url), fileURLToPath(new URL('./visual-comparison.html', import.meta.url))];
    try {
      for (const selected of plan.cases) {
        const row = results.main.evaluation.rows.find(r => r.index === selected.row), root = dataset.roots.find(r => r.id === selected.root);
        if (!row || !root || row.root !== root.id || row.signal !== 'terminal' || row.budget.mode !== 'work' || row.budget.proposals !== 128)
          throw new Error(`Unexpected selected evaluation row: ${selected.row}`);
        if (hash(row.artifact.path) !== row.artifact.sha256) throw new Error(`Changed episode: ${row.artifact.path}`);
        inputs.push(row.artifact.path);
        const episode = read(row.artifact.path), graph = prepareRoot(root, catalogs), aliases = new Map(root.witness.placements.map((p, i) =>
          [p.instance, p.instance.length <= 5 ? p.instance : `F${String(i + 1).padStart(2, '0')}`]));
        if (digest(episode.best_witness) !== row.best_witness_sha256 || semanticKey(episode.best_witness) !== root.semantic_key)
          throw new Error(`Selected geometry or original semantics changed: ${root.id}`);
        for (const [side, witness, expected] of [['before', root.witness, row.initial], ['after', episode.best_witness, row.best]]) {
          const validation = await probe.check(witness), coverage = checkRoot(root, graph, witness, validation);
          if (!coverage.factory_validated || compare(score(witness, catalogs).tuple, expected)) throw new Error(`Invalid visual source: ${root.id}/${side}`);
          audits.push({ root: root.id, side, witness_sha256: digest(witness), validation, coverage });
        }
        cases.push({ label: selected.label, root: root.id, policy: row.policy, rollout_seed: row.rollout_seed, proposals: row.work.proposals,
          elapsed_ms: row.elapsed_ms, row: row.index, before: drawing(root.witness, aliases, catalogs), after: drawing(episode.best_witness, aliases, catalogs) });
      }
    } finally { probe.close(); }
    const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
    const payload = { schema_version: 1, selection: plan.selection, cases };
    const template = fs.readFileSync(fileURLToPath(new URL('./visual-comparison.html', import.meta.url)), 'utf8');
    const fragment = template.replace('__COMPARISON_DATA__', JSON.stringify(payload).replaceAll('<', '\\u003c'));
    if (Buffer.byteLength(fragment) >= 1000000) throw new Error('Inline diagram exceeds 1 MB');
    fs.writeFileSync(path.join(output, 'optimization-before-after.html'), fragment, { flag: 'wx' });
    write(path.join(output, 'report.json'), { schema_version: 1, selection: plan.selection, audits,
      cases: cases.map(({ before, after, ...c }) => ({ ...c, before: before.score, after: after.score })),
      provenance: inputs.map(p => ({ path: p, sha256: hash(p) })) });
    console.log(JSON.stringify({ status: 'complete', cases: cases.length, independently_validated_witnesses: audits.length, bytes: Buffer.byteLength(fragment), output }));
  } catch (e) { console.error(JSON.stringify(diagnostic('visualization-failed', 'visual-comparison', e.message))); process.exitCode = 1; }
}
