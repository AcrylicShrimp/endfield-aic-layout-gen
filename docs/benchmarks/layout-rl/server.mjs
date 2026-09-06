import fs from 'node:fs';
import path from 'node:path';
import { createInterface } from 'node:readline';
import { fileURLToPath } from 'node:url';
import { args, diagnostic, fail, hash, read, write, Probe, sourcePaths } from './io.mjs';
import { Environment } from './environment.mjs';
import { checkRoot } from './coverage.mjs';
import { tokenFields, actionFields } from './features.mjs';
import { search } from './search.mjs';

export async function serve(options) {
  const datasetPath = path.resolve(options.roots), dataset = read(datasetPath), output = path.resolve(options['output-directory']);
  if (dataset.schema_version !== 1 || !dataset.factory_validated || !Array.isArray(dataset.roots)) fail('input-invalid', 'Invalid imported roots', datasetPath);
  for (const p of dataset.provenance) if (hash(p.path) !== p.sha256) fail('stale-input', 'Root import provenance changed; re-import roots', p.path);
  fs.mkdirSync(output);
  const provenance = [...sourcePaths(fileURLToPath(import.meta.url))].map(p => ({ path: p, sha256: hash(p) }));
  const datasetHash = hash(datasetPath);
  write(path.join(output, 'provenance.json'), { roots: { path: datasetPath, sha256: datasetHash }, implementation: provenance });
  let probe = new Probe(dataset.options, dataset.environment.validator_ms);
  const env = new Environment(dataset, probe); let episode = 0;
  const emit = value => process.stdout.write(JSON.stringify(value) + '\n');
  const input = createInterface({ input: process.stdin });
  try {
    for await (const line of input) {
      let request;
      try {
        request = JSON.parse(line);
        if (request.schema_version !== 1 || !Number.isSafeInteger(request.id)) fail('input-invalid', 'Invalid environment request envelope');
        let result;
        if (request.op === 'hello') result = { roots: dataset.roots.map(({ id, split, generation, family, score, fingerprint }) => ({ id, split, generation, family, score, fingerprint })), token_fields: tokenFields, action_fields: actionFields };
        else if (request.op === 'reset') {
          if (probe.closed) { probe = new Probe(dataset.options, dataset.environment.validator_ms); env.probe = probe; }
          result = await env.reset(request.root, request.budget); episode++;
        } else if (request.op === 'step') result = await env.step(request.action);
        else if (request.op === 'search') result = await search(env, request.policy);
        else if (request.op === 'summary') result = env.summary({ trace: request.trace === true, witnesses: request.witnesses === true });
        else if (request.op === 'save') {
          const name = `episode-${String(episode).padStart(7, '0')}`, report = env.summary({ trace: true, witnesses: true });
          write(path.join(output, `${name}.json`), report); result = { path: path.join(output, `${name}.json`), sha256: hash(path.join(output, `${name}.json`)) };
        } else if (request.op === 'audit') {
          if (probe.closed) { probe = new Probe(dataset.options, dataset.environment.validator_ms); env.probe = probe; }
          const began = performance.now(), validation = await probe.check(env.best), coverage = checkRoot(env.root, env.root.graph, env.best, validation);
          result = { validation, coverage, elapsed_ms: performance.now() - began };
          if (!coverage.factory_validated) fail('audit-failed', 'Retained incumbent failed fresh original-request validation', env.root.id);
        } else if (request.op === 'close') { emit({ schema_version: 1, id: request.id, result: { status: 'closed' } }); break; }
        else fail('input-invalid', 'Unknown environment operation', request.op);
        emit({ schema_version: 1, id: request.id, result });
      } catch (e) {
        emit({ schema_version: 1, id: request?.id ?? null, error: diagnostic(e.code?.replace(/^rl-/, '') ?? 'server-failed', 'environment', e.message, e.entity) });
      }
    }
    for (const p of provenance) if (hash(p.path) !== p.sha256) fail('stale-input', 'Environment source changed while worker was running', p.path);
    if (hash(datasetPath) !== datasetHash) fail('stale-input', 'Imported root data changed while worker was running', datasetPath);
  } finally { probe.close(); input.close(); }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { await serve(args(['roots', 'output-directory'])); }
  catch (e) { console.error(JSON.stringify(diagnostic(e.code?.replace(/^rl-/, '') ?? 'server-failed', 'startup', e.message, e.entity))); process.exitCode = 1; }
}
