// Original-graph validation and independent score audit of opened-family probe bests.
import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { gunzipSync } from 'node:zlib';
import { args, read, write, hash, Probe } from '../../docs/benchmarks/layout-rl/io.mjs';
import { prepareRoot, checkRoot } from '../../docs/benchmarks/layout-rl/coverage.mjs';
import { measure } from './full_factory_audit.mjs';

const unpack = p => JSON.parse(gunzipSync(fs.readFileSync(p)));
const compare = (a, b) => { const i = a.findIndex((v, i) => v !== b[i]); return i < 0 ? 0 : Math.sign(a[i]-b[i]); };
async function main(o) {
  const directory = path.resolve(o.probe), protocolPath = path.join(directory, 'protocol.json');
  const report = read(path.join(directory, 'report.json')), protocol = read(protocolPath), inputs = read(protocol.inputs);
  assert.equal(report.status, 'complete'); assert.equal(hash(protocolPath), report.protocol_sha256);
  for (const [file, expected] of Object.entries(protocol.provenance)) assert.equal(hash(file), expected, file);
  const paths = read(inputs.catalogs), catalogs = Object.fromEntries(Object.entries(paths).map(([k, p]) => [k, read(p)]));
  const roots = new Map(inputs.rows.map(r => {
    for (const [key, name] of [['original', 'original-request.json'], ['request', 'request.json'], ['initial', 'initial.json']])
      assert.equal(hash(r[key]), r.files_sha256[name], r[key]);
    const root = { id: r.id, dialect: r.dialect, request: read(r.original), witness: read(r.initial) };
    return [r.id, { root, graph: prepareRoot(root, catalogs) }];
  }));
  assert.deepEqual(report.observations.map(r => r.case).sort(), [...roots.keys()].sort());
  assert.deepEqual(report.rows.filter(r => r.arm === 'geometry').map(r => r.case).sort(), [...roots.keys()].sort());
  const probe = new Probe({ ...paths, validator: path.resolve(o.validator) }, 5000), rows = [], seen = new Set();
  try {
    for (const row of report.rows) {
      const key = `${row.case}:${row.arm}`; assert(!seen.has(key)); seen.add(key);
      const { root, graph } = roots.get(row.case), w = unpack(path.join(row.directory, 'best.json.gz'));
      const episode = unpack(path.join(row.directory, 'episode.json.gz'));
      const validation = await probe.check(w), coverage = checkRoot(root, graph, w, validation);
      assert(coverage.factory_validated, JSON.stringify(coverage));
      assert.deepEqual(measure(w, catalogs), row.best_metrics);
      assert.deepEqual(measure(root.witness, catalogs).score, row.initial_metrics.score);
      assert(compare(row.best_metrics.score, row.initial_metrics.score) <= 0);
      assert.deepEqual(episode.best_metrics, row.best_metrics);
      assert.equal(episode.trace.length, row.selected); assert(row.selected <= protocol.horizon);
      let best = row.initial_metrics.score;
      for (const [i, t] of episode.trace.entries()) {
        assert.equal(t.selected, i+1); assert(t.route_expansions <= protocol.route_work);
        assert(compare(t.best_score, best) <= 0); best = t.best_score;
        assert(!t.diagnostics.some(d => d.code === 'factory.executor-error'));
        if (row.arm.startsWith('internal-pipe-')) {
          assert.equal(row.selected, 1); assert.equal(t.action.kind, 'reroute'); assert.equal(t.action.transport, 'pipe');
        }
      }
      assert.deepEqual(best, row.best_metrics.score);
      rows.push({ case: row.case, arm: row.arm, factory_validated: true, score: row.best_metrics.score,
        selected: row.selected, best_sha256: hash(path.join(row.directory, 'best.json.gz')),
        episode_sha256: hash(path.join(row.directory, 'episode.json.gz')) });
    }
    write(o.output, { schema_version: 1, status: 'passed', scope: 'all-best original-graph coverage and independent metrics; no learning claim',
      rows, report_sha256: hash(path.join(directory, 'report.json')), validator_sha256: hash(o.validator) });
    console.log(JSON.stringify({ status: 'passed', roots: roots.size, episodes: rows.length, selected: rows.reduce((s, r) => s+r.selected, 0) }));
  } finally { probe.close(); }
}
main(args(['probe', 'validator', 'output'])).catch(e => { console.error(e.stack); process.exitCode = 1; });
