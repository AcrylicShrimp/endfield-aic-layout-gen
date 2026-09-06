import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { permutationModule, checkGeometry } from './geometry.mjs';
import { witnessScore } from '../constructive-auto-interface/compact.mjs';
import { better } from '../constructive-factory-closure/model.mjs';
import { fillMetrics } from '../constructive-shape-packing/model.mjs';
import { emptyRectangles } from '../constructive-local-reembedding/groups.mjs';
const read = p => JSON.parse(fs.readFileSync(p));
const hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
try {
  const args = process.argv.slice(2);
  if (args.length !== 4 || args[0] !== '--cases' || args[2] !== '--report-output') throw new Error('Expected --cases PATH --report-output NEW_PATH');
  const config = read(args[1]); if (config.schema_version !== 1) throw new Error('Unsupported case schema');
  const catalogs = Object.fromEntries(Object.entries(config.catalogs).map(([k, p]) => [k, read(p)]));
  const core = [];
  for (const group of read(config.core_control).waves[0].groups) {
    const detail = read(path.join(group.selected[0], 'detail.json')), old = (detail.pre_compaction_plan ?? detail).permutation;
    if (!old) continue;
    if (!checkGeometry(old, catalogs).valid) throw new Error('Historical core no longer validates against current catalogs');
    const next = permutationModule(old.source_order.map(id => old.lanes.find(l => l.id === id)), old.target_order, catalogs);
    const score = g => ({ width: g.width, height: g.height, area: g.width * g.height, physical_tiles: checkGeometry(g, catalogs).physical_tiles, bridges: g.components.length });
    core.push({ control_directory: group.selected[0], lanes: old.lanes.length, before: score(old), after: score(next),
      all_paths_shortest: next.lanes.every(l => l.cells.length - 1 === next.width - 1 + Math.abs(l.cells[0].y - l.cells.at(-1).y)),
      construction: next.construction, scope: 'fixed lane order and rates; both before compaction and facility binding' });
  }
  const construction = config.construction.map(c => {
    const r = read(c.report);
    const trials = c.kind === 'frontier' ? r.attempts : r.packings.flatMap(p => p.closures);
    const complete = c.kind === 'frontier' ? r.attempts.filter(a => a.complete).map(a => a.completed.score) : r.packings.filter(p => p.complete).map(p => p.complete_score);
    let currentSources = null;
    if (c.name.endsWith('-monotone')) {
      const base = new URL(c.kind === 'frontier' ? '../constructive-frontier-assembly/' : '../constructive-cluster-packing/', import.meta.url);
      currentSources = Object.entries(r.source_hashes).every(([p, h]) => hash(new URL(p, base)) === h);
      if (!currentSources) throw new Error(`Construction source mismatch: ${c.name}`);
    }
    return { ...c, sha256: hash(c.report), elapsed_ms: r.elapsed_ms, complete_packings: complete.length,
      complete_trials: trials.filter(t => t.complete).length, trials: trials.length, new_complete_scores: complete,
      current_sources_verified: currentSources, input_provenance: r.provenance, source_hashes: r.source_hashes };
  });
  const refinements = config.refinements.map(c => {
    const r = read(c.report), wp = path.join(r.selected_directory, 'witness.json'), w = read(wp), score = witnessScore(w);
    if (!r.factory_validated || JSON.stringify(score) !== JSON.stringify(r.score)) throw new Error(`Invalid refinement score: ${c.name}`);
    let audited = false;
    if (c.audit) {
      const a = read(c.audit);
      audited = a.all_passed && a.provenance.find(p => p.kind === 'graph')?.sha256 === hash(config.original_graph) &&
        a.cases.find(t => t.name === 'unchanged-complete-factory')?.witness_sha256 === hash(wp);
      if (!audited) throw new Error(`Original-graph audit mismatch: ${c.name}`);
    }
    return { ...c, sha256: hash(c.report), selected_directory: r.selected_directory, score, initial_score: r.initial_score,
      rounds: r.rounds, attempts: r.stages.length, elapsed_ms: r.elapsed_ms, termination: r.termination,
      original_graph_audited: audited, witness_sha256: hash(wp), fill: fillMetrics(w), empty_rectangles: emptyRectangles(w, 3),
      input_provenance: r.provenance, source_hashes: r.source_hashes };
  });
  const accepted = refinements.filter(c => c.original_graph_audited).sort((a, b) => better(a.score.tuple, b.score.tuple) ? -1 : better(b.score.tuple, a.score.tuple) ? 1 : 0);
  if (!accepted.length) throw new Error('No original-graph-audited complete candidate');
  const result = { schema_version: 1, factory_validated: true, selected_directory: accepted[0].selected_directory, score: accepted[0].score,
    witness_sha256: accepted[0].witness_sha256, core, construction, refinements, cases_path: path.resolve(args[1]), cases_sha256: hash(args[1]),
    source_hashes: Object.fromEntries(['geometry.mjs', 'tracks.mjs', 'compact-evidence.mjs'].map(p => [p, hash(new URL(p, import.meta.url))])) };
  fs.writeFileSync(args[3], JSON.stringify(result, null, 2) + '\n', { flag: 'wx' });
  console.log(JSON.stringify({ status: 'compact-crossing-evidence-verified', score: result.score.tuple, output: args[3] }));
} catch (e) { console.error(JSON.stringify({ code: 'compact-crossing-evidence-failed', message: e.message })); process.exitCode = 1; }
