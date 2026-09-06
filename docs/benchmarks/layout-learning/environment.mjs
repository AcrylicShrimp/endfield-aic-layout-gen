import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { prepare, checkState } from '../chain-prototype/input.mjs';
import { body, canonical, key, occupancy, placement, ports, routeBounds } from '../chain-prototype/geometry.mjs';
import { shortestPath } from '../constructive-local-reembedding/path.mjs';
import { compareTuple, completionScore } from '../chain-prototype/replay-score.mjs';

export const read = p => JSON.parse(fs.readFileSync(p, 'utf8'));
export const write = (p, value) => fs.writeFileSync(p, JSON.stringify(value) + '\n', { flag: 'wx' });
export const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
export const digest = v => createHash('sha256').update(JSON.stringify(v)).digest('hex');
export const diagnostic = (code, stage, message, entity = null, severity = 'error') => ({ code: `learning-${code}`, severity, stage, message, entity });
export const fail = (code, message, entity = null) => { throw Object.assign(new Error(message), diagnostic(code, 'environment', message, entity)); };
export function args(keys) {
  const argv = process.argv.slice(2), o = {};
  for (let i = 0; i < argv.length; i += 2) {
    const k = argv[i].slice(2);
    if (!argv[i].startsWith('--') || !keys.includes(k) || o[k] || !argv[i + 1]) fail('input-invalid', `Invalid argument ${argv[i]}`);
    o[k] = path.resolve(argv[i + 1]);
  }
  for (const k of keys) if (!o[k]) fail('input-invalid', `Missing --${k}`);
  return o;
}
export const catalogKeys = ['recipes', 'facilities', 'items', 'transports', 'components'];
export function loadGraphs(plan, planPath, options) {
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(options[k])]));
  return plan.requests.map(r => {
    const requestPath = path.resolve(path.dirname(planPath), r.path), graph = prepare(read(requestPath), catalogs);
    if (graph.nodes.size > 3) fail('unsupported-graph', 'This experiment supports two or three facilities', requestPath);
    return { ...r, path: requestPath, graph };
  });
}
export function rng(seed) {
  let s = seed >>> 0;
  const next = () => { s += 0x6D2B79F5; let t = s; t = Math.imul(t ^ t >>> 15, t | 1); t ^= t + Math.imul(t ^ t >>> 7, t | 61); return ((t ^ t >>> 14) >>> 0) / 4294967296; };
  next.pick = a => a[Math.floor(next() * a.length)];
  next.int = (a, b) => a + Math.floor(next() * (b - a + 1));
  next.shuffle = a => { a = [...a]; for (let i = a.length - 1; i > 0; i--) { const j = next.int(0, i); [a[i], a[j]] = [a[j], a[i]]; } return a; };
  return next;
}
export function proposal(graph, base, edge, random, maxGap) {
  const node = graph.nodes.get(edge.source), targetNode = graph.nodes.get(edge.target);
  const targetPose = base.placements.find(p => p.instance === edge.target);
  const shape = placement(node, random.pick(node.facility_data.allowed_rotations));
  const side = random.int(0, 3), gap = random.int(0, maxGap);
  const x = side === 0 ? targetPose.x - shape.width - gap : side === 1 ? targetPose.x + targetPose.width + gap : targetPose.x + random.int(-shape.width + 1, targetPose.width - 1);
  const y = side === 2 ? targetPose.y - shape.height - gap : side === 3 ? targetPose.y + targetPose.height + gap : targetPose.y + random.int(-shape.height + 1, targetPose.height - 1);
  const pose = placement(node, shape.rotation, x, y);
  const used = new Set(base.transport_networks.flatMap(n => n.terminals.map(t => JSON.stringify([t.endpoint.instance, t.endpoint.port]))));
  const sources = ports(pose, node).filter(p => p.direction === 'output' && p.transport === edge.transport);
  const targets = ports(targetPose, targetNode).filter(p => p.direction === 'input' && p.transport === edge.transport && !used.has(JSON.stringify([edge.target, p.id])));
  if (!sources.length || !targets.length) fail('no-ports', 'No compatible unassigned port in local group', edge.id);
  return { pose, source_port: random.pick(sources).id, target_port: random.pick(targets).id };
}
export function selectedPorts(graph, base, edge, action) {
  const node = graph.nodes.get(edge.source), target = base.placements.find(p => p.instance === edge.target);
  const expected = placement(node, action.pose.rotation, action.pose.x, action.pose.y);
  if (JSON.stringify(expected) !== JSON.stringify(action.pose) || !node.facility_data.allowed_rotations.includes(action.pose.rotation)) fail('invalid-action', 'Pose differs from catalog geometry', edge.id);
  const source = ports(action.pose, node).find(p => p.id === action.source_port && p.direction === 'output' && p.transport === edge.transport);
  const targetPort = ports(target, graph.nodes.get(edge.target)).find(p => p.id === action.target_port && p.direction === 'input' && p.transport === edge.transport);
  if (!source || !targetPort) fail('invalid-action', 'Unknown or incompatible action port', edge.id);
  if (base.transport_networks.some(n => n.terminals.some(t => t.endpoint.instance === edge.target && t.endpoint.port === targetPort.id))) fail('invalid-action', 'Target port is already assigned', edge.id);
  return { source, target: targetPort };
}
export class Environment {
  constructor(options, plan, output) {
    this.options = options; this.plan = plan; this.output = output; this.started = performance.now();
    this.work = { candidates: 0, validations: 0, route_expansions: 0, route_ms: 0, validator_ms: 0, evidence_write_ms: 0, outcomes: {} };
  }
  guard() { if (performance.now() - this.started >= this.plan.elapsed_ms) fail('budget-exhausted', 'Generation/audit elapsed limit reached; not global infeasibility', this.plan.elapsed_ms); }
  validate(graph, witness, meta) {
    this.guard(); const index = this.work.validations++, directory = path.join(this.output, `validation-${String(index).padStart(5, '0')}`);
    // The probe DTO uses nonnegative coordinates. Translate the whole layout, never crop it.
    const normalized = canonical(witness);
    let start = performance.now(); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), normalized);
    write(path.join(directory, 'local-witness.json'), witness); write(path.join(directory, 'decision.json'), meta);
    this.work.evidence_write_ms += performance.now() - start; start = performance.now();
    const result = spawnSync(this.options.validator, ['--witness', path.join(directory, 'witness.json'), ...catalogKeys.flatMap(k => [`--${k}`, this.options[k]]), '--report-output', path.join(directory, 'validation.json')],
      { encoding: 'utf8', timeout: Math.max(1, Math.min(this.plan.validator_ms, Math.floor(this.plan.elapsed_ms - (performance.now() - this.started)))), killSignal: 'SIGKILL', maxBuffer: 1024 * 1024 });
    const elapsed = performance.now() - start; this.work.validator_ms += elapsed; start = performance.now();
    write(path.join(directory, 'process.json'), { status: result.status, signal: result.signal, error: result.error?.message ?? null, stdout: result.stdout, stderr: result.stderr, elapsed_ms: elapsed });
    this.work.evidence_write_ms += performance.now() - start;
    const artifact = { directory: path.basename(directory), witness_sha256: hash(path.join(directory, 'witness.json')), local_witness_sha256: hash(path.join(directory, 'local-witness.json')),
      report_sha256: fs.existsSync(path.join(directory, 'validation.json')) ? hash(path.join(directory, 'validation.json')) : null };
    if (result.error?.code === 'ETIMEDOUT') return { accepted: false, status: 'validator-timeout', artifact };
    if (result.error) fail('validator-process', result.error.message, this.options.validator);
    const reportPath = path.join(directory, 'validation.json');
    if (result.status !== 0 || !fs.existsSync(reportPath)) return { accepted: false, status: 'validation-rejected', artifact };
    const report = read(reportPath);
    try { checkState(graph, normalized, report); } catch (e) { return { accepted: false, status: 'ledger-rejected', artifact, message: e.message }; }
    this.guard();
    return { accepted: true, status: 'accepted', artifact, score: completionScore(graph, witness) };
  }
  evaluate(graph, base, edge, action) {
    this.guard(); this.work.candidates++;
    const done = outcome => { this.work.outcomes[outcome.status] = (this.work.outcomes[outcome.status] ?? 0) + 1; return outcome; };
    const occupied = occupancy(base);
    if (body(action.pose).some(p => occupied.has(key(p)))) return done({ accepted: false, status: 'body-overlap' });
    const chosen = selectedPorts(graph, base, edge, action), witness = structuredClone(base);
    witness.placements.push(action.pose); witness.utilizations[edge.source] = graph.nodes.get(edge.source).utilization;
    let net = witness.transport_networks.find(n => n.item === edge.item && n.transport === edge.transport);
    if (!net) { net = { id: `network:${JSON.stringify([edge.transport, edge.item])}`, item: edge.item, transport: edge.transport, requirement_ids: [], cells: [], segments: [], terminals: [], component_ids: [] }; witness.transport_networks.push(net); }
    net.requirement_ids.push(edge.id);
    for (const [which, direction] of [['source', 'output'], ['target', 'input']]) net.terminals.push({ id: `${edge.id}:${which}`, node: edge[which], direction,
      endpoint: { kind: 'facility', instance: edge[which], port: chosen[which].id }, position: chosen[which].connection, rate: edge.rate });
    const start = performance.now(), bounds = routeBounds(witness, chosen.source.connection, chosen.target.connection, this.plan.max_gap);
    const route = shortestPath(chosen.source.connection, chosen.target.connection, occupancy(witness, edge.transport), bounds, { workBudget: this.plan.route_expansions_per_action, allowZeroLength: true });
    this.work.route_ms += performance.now() - start; this.work.route_expansions += route.expanded;
    if (route.status !== 'found') return done({ accepted: false, status: `route-${route.status}`, expanded: route.expanded });
    net.cells.push(...route.cells);
    for (let i = 1; i < route.cells.length; i++) net.segments.push({ from: route.cells[i - 1], to: route.cells[i], rate: edge.rate });
    const checked = this.validate(graph, witness, { edge: edge.id, action });
    return done({ ...checked, expanded: route.expanded });
  }
  witness(outcome) { return read(path.join(this.output, outcome.artifact.directory, 'local-witness.json')); }
}
export function rewards(outcomes) {
  const scores = [...new Map(outcomes.filter(o => o.accepted).map(o => [JSON.stringify(o.score.tuple), o.score.tuple])).values()].sort(compareTuple);
  return outcomes.map(o => !o.accepted ? 0 : scores.length === 1 ? 1 : 1 - 0.5 * scores.findIndex(s => !compareTuple(s, o.score.tuple)) / (scores.length - 1));
}
export const improves = (outcome, current) => outcome.accepted && compareTuple(outcome.score.tuple, current.score.tuple) < 0;

// Canonicalize only legal global rotations, including routes and chosen catalog port IDs.
export function fingerprint(graph, witness) {
  const variants = [];
  for (let q = 0; q < 4; q++) {
    if (witness.placements.some(p => !graph.nodes.get(p.instance).facility_data.allowed_rotations.includes((p.rotation + q * 90) % 360))) continue;
    const rotate = p => { let { x, y } = p; for (let i = 0; i < q; i++) [x, y] = [-y, x]; return { x, y }; };
    const facs = witness.placements.map(p => ({ role: graph.order.indexOf(p.instance), recipe: p.recipe, rotation: (p.rotation + q * 90) % 360, cells: body(p).map(rotate) }));
    const nets = witness.transport_networks.map(n => ({ item: n.item, transport: n.transport, cells: n.cells.map(rotate), segments: n.segments.map(s => ({ from: rotate(s.from), to: rotate(s.to) })),
      terminals: n.terminals.map(t => ({ role: graph.order.indexOf(t.endpoint.instance), port: t.endpoint.port, direction: t.direction, position: rotate(t.position) })) }));
    const all = [...facs.flatMap(f => f.cells), ...nets.flatMap(n => n.cells)], minX = Math.min(...all.map(p => p.x)), minY = Math.min(...all.map(p => p.y));
    const move = p => [p.x - minX, p.y - minY], sort = a => a.sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
    variants.push(JSON.stringify({ facilities: facs.map(f => ({ ...f, cells: sort(f.cells.map(move)) })), networks: nets.map(n => ({ ...n, cells: sort(n.cells.map(move)),
      segments: sort(n.segments.map(s => [move(s.from), move(s.to)])), terminals: n.terminals.map(t => ({ ...t, position: move(t.position) })) })) }));
  }
  return digest(variants.sort()[0]);
}
