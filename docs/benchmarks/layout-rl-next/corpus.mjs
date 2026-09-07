import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { args, catalogKeys, read, write, hash, digest, sourcePaths, Probe } from '../layout-rl/io.mjs';
import { rate, parseRate, dto, multiply, compare, capacity } from '../constructive-interconnect/rates.mjs';
import { prepareFactory } from '../chain-prototype/factory-input.mjs';
import { constructFactory } from '../chain-prototype/factory.mjs';
import { prepareRoot, checkRoot, semanticKey } from '../layout-rl/coverage.mjs';
import { canonicalKey } from '../layout-rl/geometry.mjs';
import { score } from '../layout-rl/score.mjs';

export const diagnostic = (code, message, entity = null, severity = 'error') =>
  ({ code: `rl-next-${code}`, severity, stage: 'corpus', message, entity });
const fail = (code, message, entity) => { throw Object.assign(new Error(message), diagnostic(code, message, entity)); };
const subtract = (a, b) => rate(a.n * b.d - b.n * a.d, a.d * b.d);
const divide = (a, b) => rate(a.n * b.d, a.d * b.n);
const minimum = (a, b) => compare(a, b) < 0 ? a : b;

export function validatePlan(plan) {
  if (plan.schema_version !== 1 || !plan.catalogs || !Array.isArray(plan.families) || !plan.families.length
    || !Array.isArray(plan.depths) || !plan.depths.length || plan.depths.some(d => !Number.isSafeInteger(d) || d < 1 || d > 8)
    || !Array.isArray(plan.utilizations) || !plan.utilizations.length || !Array.isArray(plan.first_families) || !plan.first_families.length
    || !Number.isSafeInteger(plan.total_elapsed_ms) || plan.total_elapsed_ms < 1 || plan.total_elapsed_ms > 600000
    || !Number.isSafeInteger(plan.minimum_new_training_families) || plan.minimum_new_training_families < 1)
    fail('input-invalid', 'Invalid corpus plan');
  for (const family of plan.families) if (!/^[a-z0-9-]+$/.test(family.id) || typeof family.recipe !== 'string'
    || !['train', 'validation', 'test'].includes(family.split)) fail('input-invalid', 'Invalid product family', family);
  if (new Set(plan.families.map(f => f.id)).size !== plan.families.length || new Set(plan.families.map(f => f.recipe)).size !== plan.families.length)
    fail('input-invalid', 'Duplicate product family identity or recipe');
  for (const u of plan.utilizations) if (!Number.isSafeInteger(u.numerator) || !Number.isSafeInteger(u.denominator)
    || u.numerator < 1 || u.denominator < u.numerator) fail('input-invalid', 'Invalid utilization', u);
  if (plan.first_families.some(f => !plan.policy?.families?.includes(f))) fail('input-invalid', 'Constructor variant is absent from policy');
  const count = plan.families.length * plan.depths.length * plan.utilizations.length * plan.first_families.length;
  if (count > 256) fail('input-invalid', 'Corpus plan exceeds 256 explicit construction attempts', count);
}

// Procurement depth defines a new request before placement; rejected requests are never cropped.
export function makeRequest(plan, family, depth, utilization, first, catalogs) {
  const recipes = catalogs.recipes.recipes, items = new Map(catalogs.items.items.map(i => [i.id, i]));
  const raw = new Set(catalogs.recipes.external_items);
  const eligible = r => r.outputs.length === 1 && [...r.inputs, ...r.outputs].every(a => items.get(a.item)?.transport === 'belt');
  const sink = recipes.find(r => r.id === family.recipe);
  if (!sink || !eligible(sink)) fail('unsupported-request', 'Expected a catalog single-output belt recipe', family.recipe);
  const beltCapacity = capacity(catalogs.transports.transports.find(t => t.kind === 'belt').capacity);
  const request = { schema_version: 2, id: `${family.id}-d${depth}-u${utilization.numerator}-${utilization.denominator}-${first}`,
    nodes: [], edges: [], external: [], policy: { ...structuredClone(plan.policy), families: [first, ...plan.policy.families.filter(f => f !== first)] },
    boundary: structuredClone(plan.boundary), budget: structuredClone(plan.construction_budget) };
  const procurement = [];
  const obligation = (instance, direction, item, value, reason) => {
    const id = `X${request.external.length}`;
    request.external.push({ id, instance, direction, item, rate: dto(value) });
    procurement.push({ id, reason });
  };
  function addNode(recipe, u, level, ancestors) {
    if (request.nodes.length >= 16) fail('unsupported-request', 'Requested expansion exceeds the unchanged 16-node constructor guard', request.id);
    const id = `N${request.nodes.length}`;
    request.nodes.push({ id, recipe: recipe.id, utilization: dto(u) });
    const visited = new Set([...ancestors, recipe.id]);
    for (const input of recipe.inputs) {
      let demand = multiply(rate(BigInt(input.quantity) * 1000n, recipe.duration_ms), u);
      const producer = recipes.filter(r => eligible(r) && r.outputs[0].item === input.item && !visited.has(r.id)).sort((a, b) => a.id.localeCompare(b.id))[0];
      if (level >= depth || raw.has(input.item) || !producer) {
        obligation(id, 'input', input.item, demand, level >= depth ? 'declared-depth' : raw.has(input.item) ? 'catalog-external' : 'no-supported-acyclic-producer');
        continue;
      }
      const full = rate(BigInt(producer.outputs[0].quantity) * 1000n, producer.duration_ms);
      while (demand.n > 0n) {
        const flow = minimum(demand, minimum(full, beltCapacity));
        const source = addNode(producer, divide(flow, full), level + 1, visited);
        request.edges.push({ id: `E${request.edges.length}`, source, target: id, item: input.item, rate: dto(flow) });
        demand = subtract(demand, flow);
      }
    }
    return id;
  }
  const root = addNode(sink, parseRate(utilization), 0, new Set());
  obligation(root, 'output', sink.outputs[0].item, multiply(rate(BigInt(sink.outputs[0].quantity) * 1000n, sink.duration_ms), parseRate(utilization)), 'final-product');
  if (request.nodes.length < 2) fail('unsupported-request', 'Request has fewer than two facilities', request.id);
  prepareFactory(request, catalogs);
  return { request, procurement };
}

export async function buildCorpus(options) {
  const planPath = path.resolve(options.plan), plan = read(planPath), resolve = p => path.resolve(path.dirname(planPath), p);
  validatePlan(plan);
  const previousPath = resolve(plan.previous_roots), previous = read(previousPath);
  for (const p of previous.provenance) if (hash(p.path) !== p.sha256) fail('stale-input', 'Previous corpus provenance changed', p.path);
  const paths = Object.fromEntries(catalogKeys.map(k => [k, resolve(plan.catalogs[k])]));
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(paths[k])])), validator = resolve(plan.validator);
  if (catalogKeys.some(k => hash(paths[k]) !== hash(previous.options[k]))) fail('stale-input', 'New and previous corpora use different catalogs');
  const sources = new Set([planPath, previousPath, validator, ...Object.values(paths), ...sourcePaths(fileURLToPath(import.meta.url))]);
  for (const p of previous.provenance) sources.add(p.path);
  const provenance = [...sources].map(p => ({ path: p, sha256: hash(p) }));
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const probe = new Probe({ ...paths, validator }), roots = [], rows = [], seen = new Map(), started = performance.now();
  async function accept(candidate) {
    const fingerprint = canonicalKey(candidate.witness, catalogs), retained = seen.get(fingerprint);
    if (retained) {
      if (candidate.split !== retained.split) fail('split-leakage', 'Equivalent geometry crosses a split', { candidate: candidate.id, retained: retained.id });
      return { status: 'duplicate-root', retained: retained.id, fingerprint };
    }
    const graph = prepareRoot(candidate, catalogs), validation = await probe.check(candidate.witness);
    const coverage = checkRoot(candidate, graph, candidate.witness, validation);
    if (!coverage.factory_validated) fail('validation-failed', 'Fresh complete-factory audit failed', candidate.id);
    const root = { ...candidate, fingerprint, semantic_key: semanticKey(candidate.witness), score: score(candidate.witness, catalogs), factory_validated: true };
    roots.push(root); seen.set(fingerprint, root);
    write(path.join(output, `${root.id}.validation.json`), { validation, coverage });
    return { status: 'accepted', fingerprint, score: root.score, facilities: root.witness.placements.length, audit: { validation, coverage } };
  }
  try {
    for (const root of previous.roots) rows.push({ id: root.id, origin: 'previous', split: root.split, family: root.family, ...await accept(root) });
    for (const family of plan.families) for (const depth of plan.depths) for (const utilization of plan.utilizations) for (const first of plan.first_families) {
      const id = `new-${family.id}-d${depth}-u${utilization.numerator}-${utilization.denominator}-${first}`;
      const row = { id, origin: 'generated', family: family.id, split: family.split, depth, utilization, first };
      if (performance.now() - started >= plan.total_elapsed_ms) {
        rows.push({ ...row, status: 'unattempted', diagnostic: diagnostic('corpus-budget', 'Total corpus elapsed guard exhausted', id, 'warning') }); continue;
      }
      let generated;
      try { generated = makeRequest(plan, family, depth, utilization, first, catalogs); }
      catch (e) {
        if (e.code !== 'rl-next-unsupported-request') throw e;
        rows.push({ ...row, status: 'unsupported-request', diagnostic: diagnostic('unsupported-request', e.message, id, 'warning') }); continue;
      }
      const { request, procurement } = generated, directory = path.join(output, id); fs.mkdirSync(directory);
      write(path.join(directory, 'request.json'), request); write(path.join(directory, 'procurement.json'), procurement);
      const graph = prepareFactory(request, catalogs), result = await constructFactory(graph, async witness => {
        const report = await probe.check(witness); return { status: report.covered_geometry_validated ? 0 : 2, report, artifact: null };
      });
      const { selected, retained, ...rest } = result;
      write(path.join(directory, 'construction.json'), { ...rest, selected: selected ? { score: selected.score } : null,
        retained: retained ? { facilities: retained.witness.placements.length } : null });
      const details = { request: path.join(directory, 'request.json'), facilities: request.nodes.length, edges: request.edges.length, budget: result.budget };
      if (!selected) rows.push({ ...row, ...details, status: 'construction-stopped', diagnostics: [
        diagnostic('construction-stopped', 'No complete factory within the fixed constructor choices and budget', id, 'warning'), ...result.diagnostics] });
      else {
        const witnessPath = path.join(directory, 'witness.json'); write(witnessPath, selected.witness);
        const accepted = await accept({ id, family: `new:${family.id}`, split: family.split, generation: 'catalog-tree', dialect: 'chain-factory',
          request, witness: selected.witness, origin: { witness: witnessPath, request: details.request, report: path.join(directory, 'construction.json'), depth, first } });
        rows.push({ ...row, ...details, ...accepted });
      }
      console.log(JSON.stringify({ completed: rows.filter(r => r.origin === 'generated').length, id, status: rows.at(-1).status, facilities: request.nodes.length }));
    }
    for (const p of provenance) if (hash(p.path) !== p.sha256) fail('stale-input', 'Corpus input changed during construction', p.path);
    const newRoots = roots.filter(r => r.generation === 'catalog-tree');
    const families = Object.fromEntries(['train', 'validation', 'test'].map(split => [split, [...new Set(newRoots.filter(r => r.split === split).map(r => r.family))]]));
    const gate = families.train.length >= plan.minimum_new_training_families && families.validation.length > 0 && families.test.length > 0;
    const report = { schema_version: 1, status: gate ? 'complete' : 'corpus-gate-failed', gate, new_families: families,
      roots: roots.length, new_roots: newRoots.length, elapsed_ms: performance.now() - started,
      counts: Object.fromEntries([...new Set(rows.map(r => r.status))].map(s => [s, rows.filter(r => r.status === s).length])), rows, provenance };
    write(path.join(output, 'report.json'), report);
    if (!gate) fail('corpus-gate', 'Broader corpus did not meet the predeclared family gate; inspect report.json', output);
    write(path.join(output, 'roots.json'), { schema_version: 1, factory_validated: true, roots, options: { ...paths, validator },
      environment: previous.environment, provenance, elapsed_ms: report.elapsed_ms, import_rows: rows.map(({ audit, ...r }) => r) });
    console.log(JSON.stringify({ status: 'complete', roots: roots.length, new_roots: newRoots.length, families, output }));
  } finally { probe.close(); }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { await buildCorpus(args(['plan', 'output-directory'])); }
  catch (e) { console.error(JSON.stringify(diagnostic(e.code?.replace(/^rl-next-/, '') ?? 'input-invalid', e.message, e.entity))); process.exitCode = 1; }
}
