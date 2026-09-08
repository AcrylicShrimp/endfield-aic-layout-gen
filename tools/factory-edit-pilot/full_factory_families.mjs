// Explicit family admission before policy training; all inspected families are opened.
import fs from 'node:fs';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { args, read, write, hash, sourcePaths, catalogKeys, Probe } from '../../docs/benchmarks/layout-rl/io.mjs';
import { prepareRoot, checkRoot } from '../../docs/benchmarks/layout-rl/coverage.mjs';
import { internalTopology, sizeStratum } from '../../docs/benchmarks/layout-direct/support.mjs';
import { measure } from './full_factory_audit.mjs';

const diagnostic = (code, stage, message, entity = null) => ({ code: `family.${code}`, severity: 'error', stage, message, entity });
const integer = (n, max) => Number.isSafeInteger(n) && n > 0 && n <= max;
const id = s => typeof s === 'string' && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(s);

export function validateStudy(p) {
  if (p.schema_version !== 1 || typeof p.cli !== 'string' || typeof p.validator !== 'string'
    || catalogKeys.some(k => typeof p.catalogs?.[k] !== 'string')
    || !integer(p.flow_elapsed_ms, 30000) || !integer(p.validator_ms, 30000)
    || !integer(p.max_facilities, 256) || !integer(p.construction?.elapsed_ms, 300000)
    || !integer(p.construction.max_steps, 256) || !integer(p.construction.max_placement_attempts, 4096)
    || !integer(p.construction.max_route_searches, 65536) || !integer(p.construction.analysis_workers, 16)
    || !Array.isArray(p.families) || !p.families.length) throw new Error('Invalid schema-1 family study or operational bounds');
  const names = new Set(), sources = new Map(); let attempts = 0;
  for (const f of p.families) {
    if (!id(f.id) || names.has(f.id) || !['train', 'development'].includes(f.split)
      || !Array.isArray(f.sources) || !f.sources.length || !Array.isArray(f.rates) || !f.rates.length)
      throw new Error(`Invalid, duplicate or sealed family: ${f.id}`);
    names.add(f.id); const local = new Set(), rates = new Set();
    for (const s of f.sources) {
      if (!id(s.id) || local.has(s.id) || typeof s.path !== 'string' || !s.path.length)
        throw new Error(`Invalid source in ${f.id}`);
      local.add(s.id);
      const normalized = path.normalize(s.path);
      if (sources.has(normalized)) throw new Error(`Source repeats across study: ${s.path}`);
      sources.set(normalized, f.id);
    }
    for (const r of f.rates) {
      if (!integer(r.quantity, 1000000) || !integer(r.duration_ms, 1000000000)) throw new Error(`Invalid rate in ${f.id}`);
      const key = r.quantity / r.duration_ms;
      if (rates.has(key)) throw new Error(`Duplicate rate in ${f.id}`);
      rates.add(key);
    }
    attempts += f.rates.length * f.sources.length;
  }
  if (attempts > 1000) throw new Error('Family study exceeds 1000 declared attempts');
}

export function wiringRequest(wiring) {
  if (wiring.schema_version !== 1 || wiring.success !== true || !Array.isArray(wiring.nodes) || !Array.isArray(wiring.edges))
    throw new Error('Expected successful schema-1 wiring');
  const nodes = wiring.nodes.filter(n => n.kind === 'facility'), ids = new Set(nodes.map(n => n.id));
  return { placements: nodes.map(n => ({ instance: n.id, recipe: n.recipe, facility: n.facility })),
    internal_requirements: wiring.edges.filter(e => ids.has(e.source) && ids.has(e.target)),
    external_requirements: wiring.edges.filter(e => !ids.has(e.source) || !ids.has(e.target)) };
}

function run(binary, argv, timeout, prefix) {
  const began = performance.now();
  return new Promise((resolve, reject) => {
    execFile(binary, argv, { timeout, maxBuffer: 64 * 1024 * 1024, killSignal: 'SIGKILL' }, (error, stdout, stderr) => {
      if (error && typeof error.code !== 'number' && !error.killed) { reject(error); return; }
      const result = { code: error?.code ?? 0, signal: error?.signal ?? null,
        timed_out: error?.killed === true, elapsed_ms: performance.now() - began };
      fs.writeFileSync(`${prefix}.stdout`, stdout, { flag: 'wx' });
      fs.writeFileSync(`${prefix}.stderr`, stderr, { flag: 'wx' });
      write(`${prefix}.json`, { ...result, binary, argv });
      resolve({ ...result, stdout });
    });
  });
}

export async function study(options) {
  const filename = path.resolve(options.study), p = read(filename), resolve = s => path.resolve(path.dirname(filename), s);
  validateStudy(p);
  const paths = Object.fromEntries(catalogKeys.map(k => [k, resolve(p.catalogs[k])]));
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(paths[k])]));
  const cli = resolve(p.cli), validator = resolve(p.validator), output = path.resolve(options['output-directory']);
  const inputs = new Set([filename, cli, validator, ...Object.values(paths), ...sourcePaths(fileURLToPath(import.meta.url)),
    ...p.families.flatMap(f => f.sources.map(s => resolve(s.path)))]);
  const provenance = [...inputs].map(file => ({ path: file, sha256: hash(file) }));
  // Reject content aliases as well as spelling aliases before observing any outcome.
  const sourceHashes = new Set();
  for (const f of p.families) for (const s of f.sources) {
    const sha = hash(resolve(s.path));
    if (sourceHashes.has(sha)) throw new Error(`Duplicate source content: ${s.path}`);
    sourceHashes.add(sha);
  }
  fs.mkdirSync(output); write(path.join(output, 'provenance.json'), provenance);
  const rows = [], roots = [], began = performance.now();
  try {
    for (const f of p.families) for (const s of f.sources) for (const [rateIndex, rate] of f.rates.entries()) {
      const rootId = `${f.id}-${s.id}-rate-${rateIndex}`, directory = path.join(output, rootId);
      fs.mkdirSync(directory);
      const row = { id: rootId, family: f.id, split: f.split, rate, source: resolve(s.path), directory,
        status: 'pending', factory_validated: false, diagnostics: [] };
      const source = read(row.source), sourcePlan = { ...source, target: { ...source.target, ...rate } };
      const requestPath = path.join(directory, 'source-plan.json'); write(requestPath, sourcePlan);
      const flow = await run(cli, ['recipes', 'contextual-instance-wiring', '--file', paths.recipes, '--request', requestPath],
        p.flow_elapsed_ms, path.join(directory, 'flow-process'));
      row.flow_ms = flow.elapsed_ms;
      if (flow.timed_out) {
        row.status = 'flow-budget'; row.diagnostics.push(diagnostic('flow-budget', 'wiring', 'Flow deadline exhausted', requestPath));
      } else {
        let wiring;
        try { wiring = JSON.parse(flow.stdout); }
        catch { if (flow.code === 0) throw new Error(`Invalid wiring JSON: ${requestPath}`); }
        if (wiring) write(path.join(directory, 'wiring.json'), wiring);
        if (flow.code !== 0 || !wiring?.success) {
          row.status = 'flow-rejected'; row.diagnostics = wiring?.diagnostics ?? [diagnostic('flow-rejected', 'wiring', 'CLI rejected source plan; see flow-process.stderr', requestPath)];
        } else {
          const request = wiringRequest(wiring), nodes = wiring.nodes.filter(n => n.kind === 'facility');
          row.facilities = nodes.length; row.size_stratum = sizeStratum(nodes.length);
          row.internal_pipe_requirements = request.internal_requirements.filter(e => catalogs.items.items.find(i => i.id === e.item)?.transport === 'pipe').length;
          row.pipe_requirements = wiring.edges.filter(e => catalogs.items.items.find(i => i.id === e.item)?.transport === 'pipe').length;
          write(path.join(directory, 'original-request.json'), request);
          if (nodes.length > p.max_facilities) {
            row.status = 'instance-guard'; row.diagnostics.push(diagnostic('instance-guard', 'admission', 'Whole request exceeds declared facility guard', nodes.length));
          } else {
            row.topology = internalTopology(nodes, wiring.edges);
            const root = { id: rootId, family: f.id, split: f.split, dialect: 'closure-factory', request };
            const graph = prepareRoot(root, catalogs);
            const targetIds = new Set(wiring.nodes.filter(n => n.kind === 'target').map(n => n.id));
            const target = wiring.edges.find(e => targetIds.has(e.target))?.source;
            if (!nodes.some(n => n.id === target)) throw new Error(`Missing original target supplier: ${requestPath}`);
            const c = p.construction, reportPath = path.join(directory, 'construction.json');
            const construction = await run(cli, ['layouts', 'construct-production', '--recipes', paths.recipes, '--source-plan', requestPath,
              '--facility-catalog', paths.facilities, '--item-catalog', paths.items, '--transport-catalog', paths.transports,
              '--logistics-component-catalog', paths.components, '--target-instance', target, '--max-steps', String(c.max_steps),
              '--max-placement-attempts', String(c.max_placement_attempts), '--max-route-searches', String(c.max_route_searches),
              '--analysis-workers', String(c.analysis_workers), '--downstream-closed-growth',
              '--report-output', reportPath, '--visualization-output', path.join(directory, 'construction.html')],
            c.elapsed_ms, path.join(directory, 'construction-process'));
            row.construction_ms = construction.elapsed_ms;
            if (construction.timed_out) {
              row.status = 'construction-budget'; row.diagnostics.push(diagnostic('construction-budget', 'construction', 'Constructor deadline exhausted', rootId));
            } else if (!fs.existsSync(reportPath)) {
              row.status = 'construction-rejected'; row.diagnostics.push(diagnostic('construction-rejected', 'construction', 'Constructor returned no report; see construction-process.stderr', rootId));
            } else {
              const report = read(reportPath); row.construction_status = report.status;
              row.covered_facilities = report.final_node?.member_instances.length ?? 0;
              if (construction.code !== 0 || report.success !== true || report.complete !== true) {
                row.status = 'construction-stopped'; row.diagnostics = report.diagnostics;
              } else {
                const n = report.final_node;
                root.witness = { schema_version: 1, placements: n.placements, transport_networks: n.transport_networks,
                  logistics_components: n.logistics_components, utilizations: Object.fromEntries(nodes.map(n => [n.id, n.work_seconds_per_second])) };
                const probe = new Probe({ ...paths, validator }, p.validator_ms), started = performance.now();
                try {
                  const validation = await probe.check(root.witness), coverage = checkRoot(root, graph, root.witness, validation);
                  write(path.join(directory, 'validation.json'), { validation, coverage });
                  row.factory_validated = coverage.factory_validated === true;
                  row.status = row.factory_validated ? 'accepted' : 'validation-rejected';
                  row.diagnostics = [...validation.diagnostics, ...coverage.diagnostics];
                } finally { probe.close(); }
                row.validation_ms = performance.now() - started;
                if (row.factory_validated) {
                  row.metrics = measure(root.witness, catalogs); roots.push(root);
                  const rootPath = path.join(directory, 'root.json'); write(rootPath, root);
                  row.root = { path: rootPath, sha256: hash(rootPath) };
                }
              }
            }
          }
        }
      }
      rows.push(row); write(path.join(directory, 'attempt.json'), row);
      console.log(JSON.stringify({ id: rootId, status: row.status, facilities: row.facilities, pipes: row.internal_pipe_requirements, score: row.metrics?.score }));
    }
    for (const source of provenance) if (hash(source.path) !== source.sha256) throw new Error(`Source changed during admission: ${source.path}`);
    write(path.join(output, 'roots.json'), { schema_version: 1, options: paths, roots });
    write(path.join(output, 'report.json'), { schema_version: 1, status: 'measured', scope: 'opened-family-admission-only',
      attempted: rows.length, accepted: roots.length, elapsed_ms: performance.now() - began, rows, provenance });
    return { attempted: rows.length, accepted: roots.length, output };
  } catch (e) {
    write(path.join(output, 'failure.json'), { ...diagnostic('study-failed', 'admission', e.message), completed_attempts: rows.length });
    throw e;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  study(args(['study', 'output-directory'])).then(r => console.log(JSON.stringify(r))).catch(e => {
    console.error(JSON.stringify(diagnostic('study-failed', 'admission', e.message))); process.exitCode = 1;
  });
}
