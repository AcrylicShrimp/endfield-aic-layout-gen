// Replay a one-input failed placement, holding its geometry and every old route fixed.
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { project, routeTemplates, inspectCoverage, better } from '../constructive-factory-closure/model.mjs';
import { shortestPath, key } from '../constructive-local-reembedding/path.mjs';
import { bodyCells } from '../constructive-local-reembedding/movement.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { compare, capacity, parseRate } from '../constructive-interconnect/rates.mjs';
import { regionInterface } from './interface.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const sourceHashes = Object.fromEntries(['route-audit.mjs', 'interface.mjs', '../constructive-local-reembedding/path.mjs', '../constructive-factory-closure/model.mjs', '../constructive-bound-interconnect/view.mjs'].map(p => [p, hash(new URL(p, import.meta.url))]));
try {
  const keys = ['graph', 'closure', 'label', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2); if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`); options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const output = path.resolve(options['output-directory']); fs.mkdirSync(output);
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])])), graph = normalizeGraph(read(options.graph), catalogs), closure = read(options.closure);
  let directory = read(closure.provenance.find(p => p.kind === 'baseline').path).selected_directory, stage;
  for (const s of closure.stages) { if (s.label === options.label && !s.selected_directory) { stage = s; break; } if (s.selected_directory) directory = s.selected_directory; }
  if (!stage) throw new Error(`No failed frontier for ${options.label}`);
  const failure = stage.failures.find(f => f.code === 'closure-port-template-exhausted'), placement = failure?.placement, edges = graph.edges.filter(e => e.target === placement?.instance);
  if (!placement || edges.length !== 1 || !edges[0].internal) throw new Error('Exact replay requires a one-input internal frontier');
  const edge = edges[0], cap = capacity(catalogs.transports.transports.find(t => t.kind === edge.transport).capacity);
  if (compare(parseRate(edge.rate), cap) > 0) throw new Error('Exact replay currently requires one capacity-bounded lane');
  const base = read(path.join(directory, 'detail.json')), w = structuredClone(base.witness); w.placements.push(placement);
  w.utilizations[placement.instance] = graph.nodes.find(n => n.id === placement.instance).utilization;
  const ports = project(w, catalogs), body = new Set(w.placements.flatMap(bodyCells).map(key)), layers = { belt: new Set(), pipe: new Set() }, used = new Set();
  for (const n of w.transport_networks) { n.cells.forEach(p => layers[n.transport].add(key(p))); n.terminals.filter(t => t.endpoint.kind === 'facility').forEach(t => used.add(JSON.stringify([t.endpoint.instance, t.endpoint.port]))); }
  for (const c of w.logistics_components) layers[c.transport].add(key(c.position));
  const ends = (id, direction) => ports[id].filter(p => p.direction === direction && p.transport === edge.transport && !used.has(JSON.stringify([id, p.id])));
  const points = [...w.placements.flatMap(bodyCells), ...w.transport_networks.flatMap(n => n.cells), ...w.logistics_components.map(c => c.position)], extent = {
    width: Math.max(...points.map(p => p.x)) + 1, height: Math.max(...points.map(p => p.y)) + 1, minX: Math.min(...points.map(p => p.x)), minY: Math.min(...points.map(p => p.y)) };
  const beforeRouteInterface = regionInterface(graph, w, catalogs);
  const bounds = { minX: 0, minY: 0, maxX: extent.width + 4, maxY: extent.height + 4 }, trials = []; let selected;
  for (const a of ends(edge.source, 'output')) for (const b of ends(edge.target, 'input')) {
    const simple = routeTemplates(a.connection, b.connection, { body, layers }, edge.transport, extent), perimeter = routeTemplates(a.connection, b.connection, { body, layers }, edge.transport, extent, null, 'perimeter');
    const found = shortestPath(a.connection, b.connection, new Set([...body, ...layers[edge.transport]]), bounds);
    if (found.status === 'same-endpoint') Object.assign(found, { status: 'found', cells: [a.connection], steps: 0, turns: 0 });
    const record = { source_port: a.id, target_port: b.id, simple: Boolean(simple), perimeter: Boolean(perimeter), astar_status: found.status, expanded: found.expanded }; trials.push(record);
    if (found.status !== 'found') continue;
    record.path = found.cells; record.steps = found.steps; record.turns = found.turns;
    const detail = structuredClone({ ...base, witness: w, catalog_ports: ports, accepted: false, factory_validated: false });
    detail.route_audit = { source: edge.source, target: edge.target, cells: found.cells };
    delete detail.block; detail.case = 'actual-graph-factory-closure';
    let n = detail.witness.transport_networks.find(n => n.item === edge.item);
    if (!n) { n = { id: `audit:${edge.item}`, item: edge.item, transport: edge.transport, cells: [], segments: [], terminals: [], requirement_ids: [], component_ids: [] }; detail.witness.transport_networks.push(n); }
    n.requirement_ids.push(edge.id); n.cells.push(...found.cells);
    for (let j = 1; j < found.cells.length; j++) n.segments.push({ from: found.cells[j - 1], to: found.cells[j], rate: edge.rate });
    for (const [instance, port, direction] of [[edge.source, a, 'output'], [edge.target, b, 'input']]) n.terminals.push({ id: `audit:${direction}`, node: instance, endpoint: { kind: 'facility', instance, port: port.id }, position: port.connection, direction, rate: edge.rate });
    const dir = path.join(output, `candidate-${trials.length - 1}`); fs.mkdirSync(dir); write(path.join(dir, 'witness.json'), detail.witness);
    const proc = await executeValidator(path.resolve(options.validator), ['--witness', path.join(dir, 'witness.json'), ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(dir, 'validation.json')], path.join(dir, 'validator.log'));
    const validation = read(path.join(dir, 'validation.json')), coverage = inspectCoverage(graph, detail, validation, false), boundary = regionInterface(graph, detail.witness, catalogs);
    record.accepted = detail.accepted = proc.code === 0 && !proc.timed_out && coverage.accepted && boundary.accepted;
    record.score = witnessScore(detail.witness); record.directory = dir; record.diagnostics = [...validation.diagnostics, ...coverage.diagnostics, ...boundary.diagnostics];
    write(path.join(dir, 'detail.json'), detail); write(path.join(dir, 'interface.json'), boundary); write(path.join(dir, 'coverage.json'), coverage);
    fs.writeFileSync(path.join(dir, 'index.html'), render(detail, validation, catalogs), { flag: 'wx' });
    if (record.accepted && (!selected || better(record.score.tuple, selected.score.tuple))) selected = record;
  }
  const report = { schema_version: 1, scope: 'fixed-one-input-placement-no-ripup-no-crossing', source_directory: directory, original_failure: failure, before_route_interface: beforeRouteInterface, bounds, trials, selected_directory: selected?.directory ?? null,
    factory_validated: false, provenance: keys.filter(k => !['label', 'output-directory'].includes(k)).map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    selected_label: options.label, source_hashes: sourceHashes };
  write(path.join(output, 'audit.json'), report); console.log(JSON.stringify({ pairs: trials.length, astar_found: trials.filter(t => t.astar_status === 'found').length, accepted: trials.filter(t => t.accepted).length, output }));
} catch (e) { console.error(JSON.stringify({ code: 'scc-route-audit-failed', message: e.message })); process.exitCode = 1; }
