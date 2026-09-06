// Compare frozen generator outputs. Never invokes a constructor, optimizer or router.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { rotateFacility } from '../constructive-placement-only/model.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { parseRate } from '../constructive-interconnect/rates.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const delta = { north: [0, -1], east: [1, 0], south: [0, 1], west: [-1, 0] };
const exactRate = r => { const v = parseRate(r); return `${v.n}/${v.d}`; };
try {
  const args = process.argv.slice(2), options = {}, keys = ['reference', 'production', 'directional', 'experimental', 'wiring',
    'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'];
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const provenance = keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) }));
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const reference = read(options.reference), production = read(options.production), directional = read(options.directional), experimental = read(options.experimental), wiring = read(options.wiring);
  assert.equal(wiring.success, true);
  const nodes = wiring.nodes.filter(n => n.kind === 'facility'), byId = new Map(nodes.map(n => [n.id, n]));
  const mapping = new Map(), labels = {}, counts = new Map();
  for (const p of reference.witness.placements) {
    const index = counts.get(p.recipe) ?? 0; counts.set(p.recipe, index + 1);
    const node = nodes.filter(n => n.recipe === p.recipe).sort((a, b) => a.index - b.index)[index];
    assert.ok(node, `No wiring identity for reference ${p.instance}`); mapping.set(p.instance, node.id); labels[node.id] = p.instance;
  }
  assert.equal(mapping.size, nodes.length, 'Reference must contain exactly the requested facility inventory');
  const referenceWitness = structuredClone(reference.witness);
  for (const p of referenceWitness.placements) p.instance = mapping.get(p.instance);
  referenceWitness.utilizations = Object.fromEntries(Object.entries(referenceWitness.utilizations).map(([id, u]) => [mapping.get(id), u]));
  for (const n of referenceWitness.transport_networks) for (const t of n.terminals) {
    assert.equal(t.endpoint.kind, 'facility'); t.node = mapping.get(t.node); t.endpoint.instance = mapping.get(t.endpoint.instance);
  }
  const fromNode = node => ({ schema_version: 1, placements: node.placements, transport_networks: node.transport_networks, logistics_components: node.logistics_components,
    utilizations: Object.fromEntries(node.placements.map(p => [p.instance, byId.get(p.instance).work_seconds_per_second])) });
  const internal = production.steps.filter(s => s.stage === 'internal-growth').at(-1)?.node;
  assert.ok(internal && production.complete, 'Expected an internally and externally completed default-production control');
  assert.equal(experimental.retained_regions.length, 1, 'Expected one connected experimental internal region');
  const experimentalPath = path.join(experimental.retained_regions[0].directory, 'detail.json');
  provenance.push({ kind: 'experimental-detail', path: experimentalPath, sha256: hash(experimentalPath) });
  const expDetail = read(experimentalPath);
  const jobs = [
    { id: 'user', witness: referenceWitness, scope: 'internal-only', complete_inventory: true },
    { id: 'production-internal', witness: fromNode(internal), scope: 'internal-only', complete_inventory: true },
    { id: 'experimental-internal', witness: expDetail.witness, scope: 'internal-only', complete_inventory: true },
    { id: 'directional-partial', witness: fromNode(directional.final_node), scope: 'incomplete-growth', complete_inventory: false },
    { id: 'production-full', witness: fromNode(production.final_node), scope: 'external-included', complete_inventory: true }
  ];
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output); const results = [];
  for (const j of jobs) {
    const directory = path.join(output, j.id); fs.mkdirSync(directory);
    assert.equal(new Set(j.witness.placements.map(p => p.instance)).size, j.witness.placements.length);
    if (j.complete_inventory) assert.deepEqual(j.witness.placements.map(p => p.instance).sort(), nodes.map(n => n.id).sort());
    for (const p of j.witness.placements) {
      assert.equal(p.recipe, byId.get(p.instance).recipe);
      assert.deepEqual(j.witness.utilizations[p.instance], byId.get(p.instance).work_seconds_per_second);
    }
    write(path.join(directory, 'witness.json'), j.witness);
    const check = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    assert.equal(check.code, 0, `${j.id} geometry validation`);
    const validation = read(path.join(directory, 'validation.json'));
    const pending = validation.recipe_terminals.filter(t => t.pending_rate.numerator > 0);
    if (j.scope === 'internal-only') {
      assert.ok(j.witness.transport_networks.every(n => n.terminals.every(t => t.endpoint.kind === 'facility')));
      const expected = wiring.edges.filter(e => !byId.has(e.source) || !byId.has(e.target)).map(e => [byId.has(e.source) ? e.source : e.target, byId.has(e.source) ? 'output' : 'input', e.item, exactRate(e.rate)]).sort();
      const actual = pending.map(t => [t.instance, t.direction, t.item, exactRate(t.pending_rate)]).sort();
      assert.deepEqual(actual, expected, `${j.id} must leave exactly the original external margins pending`);
    }
    if (j.scope === 'external-included') assert.equal(pending.length, 0);
    const ports = Object.fromEntries(j.witness.placements.map(p => [p.instance,
      rotateFacility(catalogs.facilities.facilities.find(f => f.id === p.facility), p.rotation).ports.map(q => ({ ...q,
        position: { x: p.x + q.position.x, y: p.y + q.position.y }, connection: { x: p.x + q.position.x + delta[q.edge][0], y: p.y + q.position.y + delta[q.edge][1] } }))]));
    const detail = { schema_version: 1, case: 'generator-comparison', witness: j.witness, catalog_ports: ports, labels, accepted: true, factory_validated: false };
    write(path.join(directory, 'detail.json'), detail);
    fs.writeFileSync(path.join(directory, 'index.html'), render(detail, validation, catalogs).replace('<h1>Recipe-bound interconnect · actual catalog facilities</h1>', `<h1>Frozen comparison · ${j.id} · ${j.scope}</h1>`), { flag: 'wx' });
    results.push({ id: j.id, scope: j.scope, directory, facilities: j.witness.placements.length, geometry_validated: true, comparable: j.scope === 'internal-only',
      score: j.scope === 'incomplete-growth' ? null : witnessScore(j.witness), pending_recipe_terminals: pending.length, witness_sha256: hash(path.join(directory, 'witness.json')) });
  }
  for (const p of provenance) assert.equal(hash(p.path), p.sha256, `Input changed: ${p.path}`);
  const report = { schema_version: 1, status: 'frozen-comparison-validated', optimization_performed: false,
    scope: 'Same recipe inventory/utilization and internal-only geometry. Power and external geometry excluded from comparable scores. Segment-only turn metric used uniformly.',
    statuses: { production: production.status, directional: directional.status, experimental: experimental.status }, results, provenance };
  write(path.join(output, 'report.json'), report); console.log(JSON.stringify(report.results.map(({ id, score, comparable, facilities }) => ({ id, score, comparable, facilities }))));
} catch (e) { console.error(JSON.stringify({ code: 'reference-comparison-invalid', severity: 'error', stage: 'reference-comparison', message: e.message })); process.exitCode = 1; }
