import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { prepareComparison, deriveComparison, comparisonSignature, checkComparisonWork, summarizeComparison } from './comparison.mjs';
import { prepareFactory } from './factory-input.mjs';
import { prepareReplayPolicy } from './replay-policy.mjs';
import { completionScore } from './replay-score.mjs';

const read = p => JSON.parse(fs.readFileSync(p, 'utf8')), hash = p => createHash('sha256').update(fs.readFileSync(p)).digest('hex');
const write = (p, v) => fs.writeFileSync(p, JSON.stringify(v, null, 2) + '\n', { flag: 'wx' });
try {
  const catalogKeys = ['recipes', 'facilities', 'items', 'transports', 'components'];
  const keys = ['plan', 'policy', ...catalogKeys, 'validator', 'output-directory'], args = process.argv.slice(2), options = {};
  for (let i = 0; i < args.length; i += 2) {
    const k = args[i].slice(2);
    if (!args[i].startsWith('--') || !keys.includes(k) || options[k] || !args[i + 1]) throw new Error(`Invalid argument ${args[i]}`);
    options[k] = args[i + 1];
  }
  for (const k of keys) if (!options[k]) throw new Error(`Missing --${k}`);
  const policy = prepareReplayPolicy(read(options.policy)), plan = prepareComparison(read(options.plan), policy), base = path.dirname(path.resolve(options.plan));
  const catalogs = Object.fromEntries(catalogKeys.map(k => [k, read(options[k])]));
  const requests = plan.requests.map(r => { const source = path.resolve(base, r.path), input = read(source); return { ...r, source, input, graph: prepareFactory(input, catalogs), signature: comparisonSignature(input) }; });
  for (const r of requests) for (const budget of plan.proposal_budgets) for (const variant of plan.variants) deriveComparison(r.input, policy, variant, budget);
  const provenance = [...keys.filter(k => k !== 'output-directory').map(k => ({ kind: k, path: path.resolve(options[k]), sha256: hash(options[k]) })),
    ...requests.map(r => ({ kind: 'original-request', path: r.source, sha256: hash(r.source) }))];
  for (const name of fs.readdirSync(fileURLToPath(new URL('./', import.meta.url))).filter(n => n.endsWith('.mjs') && !n.endsWith('.test.mjs'))) {
    const source = fileURLToPath(new URL(`./${name}`, import.meta.url)); provenance.push({ kind: 'comparison-code', path: source, sha256: hash(source) });
  }
  const observed = new Map(provenance.map(p => [p.path, p.sha256]));
  const observe = entries => {
    for (const p of entries) {
      if (observed.has(p.path) && observed.get(p.path) !== p.sha256) throw new Error(`Implementation/input changed across comparison rows: ${p.path}`);
      if (!observed.has(p.path)) { observed.set(p.path, p.sha256); provenance.push(p); }
    }
  };
  const output = path.resolve(options['output-directory']), rows = []; fs.mkdirSync(output);
  const common = [...catalogKeys.flatMap(k => [`--${k}`, path.resolve(options[k])]), '--validator', path.resolve(options.validator)];
  for (const original of requests) {
    const directory = path.join(output, original.id); fs.mkdirSync(directory);
    for (const budget of plan.proposal_budgets) {
      const checkpoint = path.join(directory, String(budget)); fs.mkdirSync(checkpoint);
      for (const variant of plan.variants) {
        const derived = deriveComparison(original.input, policy, variant, budget), runDirectory = path.join(checkpoint, variant.id); fs.mkdirSync(runDirectory);
        const requestPath = path.join(runDirectory, 'request.json'), policyPath = path.join(runDirectory, 'policy.json'); write(requestPath, derived.request);
        if (derived.policy) write(policyPath, derived.policy);
        const started = performance.now(), child = spawnSync(process.execPath, [fileURLToPath(new URL(variant.engine === 'factory' ? './run-factory.mjs' : './run-replay.mjs', import.meta.url)),
          '--request', requestPath, ...(derived.policy ? ['--policy', policyPath] : []), ...common, '--output-directory', path.join(runDirectory, 'run')], { encoding: 'utf8', timeout: original.input.budget.elapsed_ms + 10000 });
        const cliElapsed = performance.now() - started;
        if (child.error || ![0, 2].includes(child.status)) throw new Error(`Constructor failed at ${runDirectory}: ${child.error?.message ?? child.stderr}`);
        const reportPath = path.join(runDirectory, 'run/report.json'), report = read(reportPath), work = checkComparisonWork(report, derived.assigned);
        observe(report.provenance);
        if (report.factory_validated !== (child.status === 0) || comparisonSignature(read(requestPath)) !== original.signature) throw new Error(`Invalid comparison outcome or modified request: ${runDirectory}`);
        const validations = fs.readdirSync(path.join(runDirectory, 'run')).filter(n => /^validation-\d+$/.test(n));
        if (validations.length !== work.used.validations) throw new Error(`Validation count differs from immutable artifacts: ${runDirectory}`);
        let score = null, audit = null, witness = null;
        if (report.factory_validated) {
          const witnessPath = path.join(runDirectory, 'run/factory-witness.json'), auditDirectory = path.join(runDirectory, 'audit'), auditStart = performance.now();
          const verification = spawnSync(process.execPath, [fileURLToPath(new URL('./verify-factory.mjs', import.meta.url)), '--request', original.source, '--witness', witnessPath,
            ...common, '--output-directory', auditDirectory], { encoding: 'utf8', timeout: 40000 });
          if (verification.error || verification.status !== 0) throw new Error(`Independent original-request audit failed at ${runDirectory}: ${verification.error?.message ?? verification.stderr}`);
          const auditPath = path.join(auditDirectory, 'report.json'), result = read(auditPath);
          observe(result.provenance);
          if (result.factory_validated !== true) throw new Error(`Audit did not accept ${witnessPath}`);
          audit = { factory_validated: true, report: auditPath, sha256: hash(auditPath), elapsed_ms: performance.now() - auditStart, process: result.process,
            scope: 'Post-run independent audit; not candidate generation or algorithm work' };
          score = completionScore(original.graph, read(witnessPath)); witness = { path: witnessPath, sha256: hash(witnessPath) };
        }
        const termination = report.termination ?? report.diagnostics.at(-1) ?? { code: report.factory_validated ? 'chain-first-completion' : 'chain-construction-stopped', severity: 'info', stage: 'factory-growth', message: 'Constructor terminated', entity: null };
        const row = { case: original.id, original_request: original.source, semantic_sha256: original.signature, variant, proposal_budget: budget,
          complete: report.factory_validated, covered: report.retained?.validation.coverage.covered ?? 0, score, producer_score: report.selected?.score ?? null,
          work: { assigned: derived.assigned, ...work, construction_elapsed_ms: report.budget.elapsed_ms, cli_elapsed_ms: cliElapsed }, termination,
          diagnostics: report.diagnostics, first_complete_trial: report.trials?.find(t => t.complete)?.id ?? null, selected_trial: report.selected?.trial ?? null,
          trial_count: report.trials?.length ?? (report.factory_validated ? 1 : 0), replay_count: report.rollbacks?.length ?? 0,
          incumbent_history: report.incumbent_history ?? [], report: { path: reportPath, sha256: hash(reportPath) }, witness, audit };
        rows.push(row); write(path.join(runDirectory, 'comparison-row.json'), row);
        console.log(JSON.stringify({ completed_runs: rows.length, total_runs: plan.runs, case: original.id, proposals: budget, variant: variant.id, complete: row.complete, score: score?.tuple ?? null }));
      }
    }
  }
  for (const p of provenance) if (hash(p.path) !== p.sha256) throw new Error(`Input changed during comparison: ${p.path}`);
  if (rows.length !== plan.runs) throw new Error('Incomplete comparison matrix');
  const summary = summarizeComparison(rows);
  write(path.join(output, 'report.json'), { schema_version: 1, scope: 'Equal allocated total-work budgets, not equal consumed work or a free portfolio', plan, provenance, rows, summary });
  write(path.join(output, 'summary.json'), summary);
  console.log(JSON.stringify({ status: 'comparison-complete', runs: rows.length, complete: summary.complete, stopped: summary.stopped, output }));
} catch (e) { console.error(JSON.stringify({ code: e.code ?? 'chain-comparison-failed', severity: 'error', stage: 'chain-comparison', message: e.message, entity: e.entity ?? null })); process.exitCode = 1; }
