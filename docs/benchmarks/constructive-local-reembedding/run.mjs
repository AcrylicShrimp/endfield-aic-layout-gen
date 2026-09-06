import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { reembed, extractChains, relocationCandidates, extentOf } from './model.mjs';
import { groupCandidates } from './groups.mjs';
import { compoundCandidates } from './compound.mjs';
import { componentPoseCandidates, releaseBridgeDraft } from './components.mjs';
import { coalescenceCandidates, incidentBridgeIds } from './coalescence.mjs';
import { boundaryPortSwaps } from './ports.mjs';
import { compactEmbedding } from '../constructive-embedding-compaction/model.mjs';
import { executeValidator } from '../constructive-embedding-compaction/validator.mjs';
import { normalizeGraph } from '../constructive-cycle-blocks/graph.mjs';
import { inspectCoverage, better } from '../constructive-factory-closure/model.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { render } from '../constructive-bound-interconnect/view.mjs';
const read = p => JSON.parse(fs.readFileSync(p)), write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
let output, created = false;
try {
  const keys = ['graph', 'baseline', 'recipes', 'facilities', 'items', 'transports', 'components', 'validator', 'output-directory'], options = {}, args = process.argv.slice(2);
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || ![...keys, 'rounds', 'mode', 'move-limit', 'move-family', 'endpoint-policy', 'junction-policy', 'max-new-crossings', 'port-policy', 'layer-policy', 'audit-unimproved', 'bridge-policy'].includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const rounds = Number(options.rounds ?? 2), mode = options.mode ?? 'joint', limit = Number(options['move-limit'] ?? 64);
  const moveFamily = options['move-family'] ?? 'single';
  const endpointPolicy = options['endpoint-policy'] ?? 'retained';
  const junctionPolicy = options['junction-policy'] ?? 'internal';
  const maxNewCrossings = Number(options['max-new-crossings'] ?? 0);
  const portPolicy = options['port-policy'] ?? 'retained';
  const layerPolicy = options['layer-policy'] ?? 'coupled';
  const bridgePolicy = options['bridge-policy'] ?? 'retained';
  if (!['retained', 'release'].includes(bridgePolicy)) throw new Error('Expected --bridge-policy retained|release');
  if (bridgePolicy === 'release' && !['move', 'joint'].includes(mode)) throw new Error('Bridge release requires a facility movement mode');
  const auditBudget = Number(options['audit-unimproved'] ?? 0); let audited = 0;
  if (!Number.isSafeInteger(auditBudget) || auditBudget < 0 || auditBudget > 64) throw new Error('Expected --audit-unimproved 0..64');
  if (!['coupled', 'independent'].includes(layerPolicy)) throw new Error('Expected --layer-policy coupled|independent');
  if (!['retained', 'paired'].includes(portPolicy)) throw new Error('Expected --port-policy retained|paired');
  if (!Number.isSafeInteger(maxNewCrossings) || maxNewCrossings < 0 || maxNewCrossings > 8) throw new Error('Expected --max-new-crossings 0..8');
  if (!['internal', 'downstream'].includes(junctionPolicy)) throw new Error('Expected --junction-policy internal|downstream');
  if (!['retained', 'terminal-flexible'].includes(endpointPolicy)) throw new Error('Expected --endpoint-policy retained|terminal-flexible');
  if (!['single', 'group', 'cone', 'compound', 'coalescence'].includes(moveFamily)) throw new Error('Expected --move-family single|group|cone|compound|coalescence');
  if (!Number.isSafeInteger(rounds) || rounds < 1 || rounds > 8 || !Number.isSafeInteger(limit) || limit < 1 || limit > 512 || !['fixed', 'move', 'joint', 'component'].includes(mode))
    throw new Error('Expected --rounds 1..8, --mode fixed|move|joint|component and --move-limit 1..512');
  output = path.resolve(options['output-directory']); fs.mkdirSync(output); created = true; const began = performance.now();
  const catalogs = Object.fromEntries(['recipes', 'facilities', 'items', 'transports', 'components'].map(k => [k, read(options[k])]));
  const graph = normalizeGraph(read(options.graph), catalogs), baseline = read(options.baseline), stages = [], failures = [], neighborhoods = [];
  let generationMs = 0, compactionMs = 0, validationMs = 0;
  const validate = async (detail, name) => {
    const start = performance.now(), directory = path.join(output, name); fs.mkdirSync(directory); write(path.join(directory, 'witness.json'), detail.witness);
    const processResult = await executeValidator(path.resolve(options.validator), ['--witness', path.join(directory, 'witness.json'),
      ...['recipes', 'facilities', 'items', 'transports', 'components'].flatMap(k => [`--${k}`, options[k]]), '--report-output', path.join(directory, 'validation.json')], path.join(directory, 'validator.log'));
    write(path.join(directory, 'validator-process.json'), processResult);
    if (processResult.timed_out || !fs.existsSync(path.join(directory, 'validation.json'))) throw new Error(`Validator failed to finish in ${directory}`);
    const v = read(path.join(directory, 'validation.json')), coverage = inspectCoverage(graph, detail, v, true);
    detail.accepted = detail.factory_validated = processResult.code === 0 && coverage.factory_validated;
    write(path.join(directory, 'coverage.json'), coverage); write(path.join(directory, 'detail.json'), detail);
    fs.writeFileSync(path.join(directory, 'index.html'), render(detail, v, catalogs), { flag: 'wx' });
    validationMs += performance.now() - start;
    return { detail, directory, factory_validated: detail.factory_validated, score: witnessScore(detail.witness), witness_sha256: hash(path.join(directory, 'witness.json')),
      validator_ms: v.elapsed_ms, diagnostics: [...v.diagnostics, ...coverage.diagnostics] };
  };
  let selected = await validate(read(path.join(baseline.selected_directory, 'detail.json')), 'incumbent');
  if (!selected.factory_validated) throw new Error('Complete incumbent failed independent graph validation');
  const initialScore = selected.score;
  const attempt = async (parent, request, context) => {
    request = { ...request, endpointPolicy, junctionPolicy, maxNewCrossings };
    const record = { ...context, request, parent_directory: parent.directory }; stages.push(record);
    try {
      const r = reembed(parent.detail, catalogs, request); generationMs += r.elapsed_ms;
      record.generation_ms = r.elapsed_ms; record.status = r.status; record.routes = r.routes; record.affected_chains = r.affected_chains; record.diagnostics = r.diagnostics;
      if (r.status !== 'candidate') return;
      let candidate = r.detail, score = r.score; record.raw_score = score;
      // Re-routing frees old bends/order. Compaction is optional postprocessing of that new geometry.
      const start = performance.now();
      try {
        let compacted = compactEmbedding(compactEmbedding(candidate, catalogs, 'x', { layerPolicy }), catalogs, 'y', { layerPolicy });
        const compactScore = witnessScore(compacted.witness);
        if (better(compactScore.tuple, score.tuple)) { candidate = compacted; score = compactScore; }
      } catch (e) { record.compaction_diagnostic = { code: e.code ?? 'local-compaction-failed', message: e.message }; }
      record.compaction_ms = performance.now() - start; compactionMs += record.compaction_ms; record.score = score;
      if (!better(score.tuple, selected.score.tuple)) {
        record.status = 'unimproved';
        if (audited < auditBudget && (request.moves?.length || request.componentPoses?.length)) {
          audited++; const checked = await validate(candidate, `audit-${stages.length - 1}`);
          Object.assign(record, Object.fromEntries(Object.entries(checked).filter(([k]) => k !== 'detail')));
          record.status = checked.factory_validated ? 'unimproved-validated' : 'validation-rejected';
        }
        return;
      }
      const checked = await validate(candidate, `candidate-${stages.length - 1}`);
      Object.assign(record, Object.fromEntries(Object.entries(checked).filter(([k]) => k !== 'detail')));
      record.status = checked.factory_validated ? 'selected' : 'validation-rejected';
      if (checked.factory_validated) selected = checked;
    } catch (e) {
      record.status = 'rejected'; record.diagnostics = [{ code: e.code ?? 'local-candidate-failed', severity: e.severity ?? 'info', stage: e.stage ?? 'local-reembedding', message: e.message, entity: e.entity ?? null }];
      failures.push({ ...context, ...record.diagnostics[0] });
    }
  };
  let termination = 'round-budget';
  for (let round = 0; round < rounds; round++) {
    const before = selected;
    if (mode === 'fixed' || mode === 'joint') {
      // Keep endpoint identity stable while recomputing chain ids after each accepted reroute/compaction.
      const count = extractChains(selected.detail.witness).chains.length;
      for (let id = 0; id < count; id++) await attempt(selected, { chainIds: [id] }, { round, phase: 'fixed-path', ordinal: id });
    }
    if (mode === 'component') {
      const parent = selected, start = performance.now(), w = parent.detail.witness;
      const portfolio = componentPoseCandidates(w, extractChains(w).chains, catalogs, [], extentOf(w), { limit });
      generationMs += performance.now() - start;
      neighborhoods.push({ round, parent_directory: parent.directory, component_poses_examined: portfolio.examined, selected_positions: portfolio.candidates.length });
      for (const [ordinal, proposal] of portfolio.candidates.entries()) for (const order of ['near', 'far'])
        await attempt(parent, { componentPoses: proposal.componentPoses, order }, { round, phase: 'component-pose', ordinal, proposal_kind: proposal.kind });
    }
    if (mode === 'move' || mode === 'joint') {
      const parent = selected, start = performance.now(), portfolio = moveFamily === 'coalescence' ? coalescenceCandidates(parent.detail, graph, catalogs, { limit, junctionPolicy }) : moveFamily === 'compound' ? compoundCandidates(parent.detail, graph, catalogs, { limit, junctionPolicy }) : moveFamily !== 'single' ? groupCandidates(parent.detail, graph, catalogs, { limit, cones: moveFamily === 'cone', junctionPolicy }) :
        { candidates: relocationCandidates(parent.detail, { limit }) }, moves = portfolio.candidates; generationMs += performance.now() - start;
      neighborhoods.push({ round, parent_directory: parent.directory, groups: portfolio.groups, voids: portfolio.voids, selected_positions: moves.length,
        screened: portfolio.screened, feasible_positions: portfolio.feasible_positions, geometry_rejections: portfolio.rejected,
        pairs_considered: portfolio.pairs_considered, pairs_available: portfolio.pairs_available, pair_budget: portfolio.pair_budget, pair_budget_exhausted: portfolio.pair_budget_exhausted,
        pose_budget: portfolio.pose_budget, poses_examined: portfolio.poses_examined, pose_budget_exhausted: portfolio.pose_budget_exhausted,
        void_positions: moves.filter(m => m.void_candidate).length, increasing_distance_positions: moves.filter(m => m.endpoint_delta > 0).length,
        kinds: Object.fromEntries([...new Set(moves.map(m => m.kind))].map(kind => [kind, moves.filter(m => m.kind === kind).length])) });
      for (const diagnostic of portfolio.diagnostics ?? []) failures.push({ round, phase: 'group-screen', ...diagnostic });
      const chains = portPolicy === 'paired' ? extractChains(parent.detail.witness).chains : [];
      const bridgeDraft = bridgePolicy === 'release' ? releaseBridgeDraft(parent.detail.witness, catalogs, parent.detail.witness.logistics_components.filter(c => c.kind === 'bridge').map(c => c.id)) : null;
      const collapsed = bridgeDraft ? extractChains(bridgeDraft.witness, bridgeDraft.released).chains : [];
      for (const [ordinal, move] of moves.entries()) {
        const translations = ['compound', 'coalescence'].includes(moveFamily) ? move.moves : [move];
        const assignments = [[], ...(portPolicy === 'paired' ? boundaryPortSwaps(parent.detail.witness, chains, translations).slice(0, 4) : [])];
        for (const [port_variant, portAssignments] of assignments.entries()) for (const order of ['near', 'far'])
          await attempt(parent, { moves: translations, order, portAssignments, releaseBridges: bridgePolicy === 'release' ? move.releaseBridges ?? incidentBridgeIds(parent.detail.witness, collapsed, new Set(translations.flatMap(m => m.instances))) : [] }, { round, phase: 'relocate', ordinal, port_variant, proposal_kind: move.kind });
      }
    }
    if (selected === before) { termination = 'unimproved-neighborhood'; break; }
  }
  const report = { schema_version: 1, chain_schema_version: 2, status: better(selected.score.tuple, initialScore.tuple) ? 'complete-factory-improved' : 'complete-incumbent-retained', factory_validated: true,
    selected_directory: selected.directory, initial_score: initialScore, score: selected.score, mode, rounds, move_limit: limit, move_family: moveFamily, move_schema_version: 3, endpoint_policy: endpointPolicy, junction_policy: junctionPolicy, max_new_crossings: maxNewCrossings, port_policy: portPolicy, layer_policy: layerPolicy, bridge_policy: bridgePolicy, termination,
    generation_ms: generationMs, compaction_ms: compactionMs, validation_output_ms: validationMs, elapsed_ms: performance.now() - began, unimproved_audit_budget: auditBudget, unimproved_audited: audited, neighborhoods, stages, failures,
    provenance: keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    source_hashes: Object.fromEntries(['model.mjs', 'movement.mjs', 'terminal-identity.mjs', 'groups.mjs', 'compound.mjs', 'coalescence.mjs', 'components.mjs', 'path.mjs', 'crossing.mjs', 'ports.mjs', 'run.mjs', '../constructive-embedding-compaction/model.mjs', '../constructive-embedding-compaction/length.mjs',
      '../constructive-embedding-compaction/validator.mjs', '../constructive-factory-closure/model.mjs', '../constructive-cycle-blocks/graph.mjs',
      '../constructive-auto-interface/compact.mjs', '../constructive-bound-interconnect/view.mjs', '../constructive-placement-only/model.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  write(path.join(output, 'report.json'), report); fs.copyFileSync(path.join(selected.directory, 'index.html'), path.join(output, 'index.html'));
  console.log(JSON.stringify({ status: report.status, score: report.score.tuple, attempts: stages.length, elapsed_ms: report.elapsed_ms, output }));
} catch (e) {
  const diagnostic = { code: e.code ?? 'local-execution-failed', severity: 'error', stage: e.stage ?? 'local-reembedding', message: e.message };
  if (created) write(path.join(output, 'failure.json'), diagnostic);
  console.error(JSON.stringify(diagnostic)); process.exitCode = 1;
}
