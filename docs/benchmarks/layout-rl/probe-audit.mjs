import fs from 'node:fs';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { args, catalogKeys, diagnostic, read, write, Probe, hash } from './io.mjs';

try {
  const options = args(['roots', 'output-directory']), dataset = read(options.roots), output = path.resolve(options['output-directory']);
  fs.mkdirSync(output); const probe = new Probe(dataset.options), rows = [];
  const began = performance.now();
  try {
    const cases = dataset.roots.map(r => ({ id: r.id, witness: r.witness, valid: true }));
    const negative = structuredClone(dataset.roots[0].witness); negative.placements[0].x = -1;
    cases.push({ id: 'negative-coordinate', witness: negative, valid: false });
    const missing = structuredClone(dataset.roots[0].witness); missing.placements.push(missing.placements[0]);
    cases.push({ id: 'duplicate-facility', witness: missing, valid: false });
    for (const c of cases) {
      const input = path.join(output, `${c.id}.witness.json`), reportPath = path.join(output, `${c.id}.file.json`); write(input, c.witness);
      let start = performance.now(); const stream = await probe.check(c.witness), streamMs = performance.now() - start;
      start = performance.now(); const file = spawnSync(dataset.options.validator, ['--witness', input, ...catalogKeys.flatMap(k => [`--${k}`, dataset.options[k]]), '--report-output', reportPath],
        { timeout: 5000, encoding: 'utf8', maxBuffer: 1024 * 1024 });
      const fileMs = performance.now() - start;
      if (file.error || file.status !== (c.valid ? 0 : 2)) throw new Error(`File probe failed in ${input}: ${file.stderr}`);
      const { provenance, elapsed_ms: fileElapsed, ...a } = read(reportPath), { elapsed_ms: streamElapsed, ...b } = stream;
      if (JSON.stringify(a) !== JSON.stringify(b) || stream.covered_geometry_validated !== c.valid) throw new Error(`Stream/file semantic mismatch: ${c.id}`);
      write(path.join(output, `${c.id}.stream.json`), stream);
      rows.push({ id: c.id, semantic_match: true, geometry_valid: c.valid, stream_ms: streamMs, file_ms: fileMs, probe_stream_ms: streamElapsed, probe_file_ms: fileElapsed });
    }
    const report = { schema_version: 1, status: 'complete', roots_sha256: hash(options.roots), validator_sha256: hash(dataset.options.validator), rows, elapsed_ms: performance.now() - began };
    write(path.join(output, 'report.json'), report);
    console.log(JSON.stringify({ status: report.status, compared: rows.length, output }));
  } finally { probe.close(); }
} catch (e) { console.error(JSON.stringify(diagnostic('probe-audit-failed', 'validation', e.message))); process.exitCode = 1; }
