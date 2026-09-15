// Explicit offline experiment. No normal training or checkpoint hooks.
import fs from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { spawn, execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const read = async p => JSON.parse(await fs.readFile(p, 'utf8'));
export const hash = bytes => createHash('sha256').update(bytes).digest('hex');
export const write = (p, v) => fs.writeFile(p, `${JSON.stringify(v)}\n`);
const check = (ok, message) => { if (!ok) throw Error(`aic-io-readiness.${message}`); };
const pad = n => String(n).padStart(8, '0');
const canonical = v => Array.isArray(v) ? v.map(canonical) : v && typeof v === 'object' ? Object.fromEntries(Object.keys(v).sort().map(k => [k, canonical(v[k])])) : v;
const digest = v => hash(JSON.stringify(canonical(v)));
export async function bytes(root) {
  let total = 0;
  for (const e of await fs.readdir(root, { withFileTypes: true })) {
    const p = path.join(root, e.name);
    check(!e.isSymbolicLink(), `output-symlink: ${p}`);
    try { total += e.isDirectory() ? await bytes(p) : (await fs.stat(p)).size; } catch (error) { if (error.code !== 'ENOENT') throw error; }
  }
  return total;
}
function processes() {
  return execFileSync('ps', ['-eo', 'pid=,ppid=,pgid=,rss=,stat='], { encoding: 'utf8' }).trim().split('\n').map(s => s.trim().split(/\s+/)).filter(r => !r[4].startsWith('Z')).map(r => r.slice(0,4).map(Number));
}
export class Gate {
  constructor(output, limits, setup = null) {
    this.output = output; this.limits = limits; this.setup = setup;
    this.started = performance.now() - (setup ? Date.now() - setup.started_at_ms : 0);
    this.groups = new Set(); this.invocations = []; this.peakRss = 0; this.peakBytes = 0;
  }
  async enforce() {
    check((performance.now() - this.started) / 1000 < this.limits.seconds, 'budget-time');
    const rss = processes().filter(([pid, , group]) => pid === process.pid || this.groups.has(group)).reduce((s, r) => s + r[3] * 1024, 0);
    this.peakRss = Math.max(this.peakRss, rss);
    check(rss <= this.limits.rss_bytes, 'budget-rss');
    this.peakBytes = Math.max(this.peakBytes, await bytes(this.output) + (this.setup ? await bytes(this.setup.root) : 0));
    check(this.peakBytes <= this.limits.output_bytes, 'budget-output');
    const disk = await fs.statfs(this.output);
    check(disk.bavail * disk.bsize >= this.limits.min_free_bytes, 'budget-free-space');
  }
  async run(name, command, args, cwd, env = {}) {
    await this.enforce();
    const out = await fs.open(path.join(this.output, `${name}.stdout`), 'wx');
    const err = await fs.open(path.join(this.output, `${name}.stderr`), 'wx');
    const start = performance.now();
    const child = spawn(command, args, { cwd, env: { ...process.env, ...env }, detached: true, stdio: ['ignore', out.fd, err.fd] });
    let failure;
    const closed = new Promise(resolve => {
      child.once('error', e => { failure = e; resolve(null); });
      child.once('close', code => resolve(code));
    });
    if (child.pid) this.groups.add(child.pid);
    let busy = false;
    let pending = Promise.resolve();
    const timer = setInterval(() => {
      if (!busy) pending = monitor();
    }, 500);
    const monitor = async () => {
      if (busy || failure) return;
      busy = true;
      try { await this.enforce(); } catch (e) { failure = e; this.stop(); }
      finally { busy = false; }
    };
    const code = await closed;
    clearInterval(timer);
    await pending;
    // A successful parent cannot leave diagnostic descendants behind.
    this.stop();
    for (let n = 0; n < 20 && this.remaining().length; n++) await new Promise(resolve => setTimeout(resolve, 50));
    check(this.remaining().length === 0, 'owned-process-cleanup');
    this.groups.clear();
    await out.close(); await err.close();
    const record = { name, command, args, cwd, pid: child.pid, code, seconds: (performance.now() - start) / 1000, error: failure?.message };
    this.invocations.push(record);
    await write(path.join(this.output, 'invocations.json'), this.invocations);
    if (failure) throw failure;
    check(code === 0, `command: ${name} exited ${code}; see ${name}.stderr`);
    await this.enforce();
    return record;
  }
  stop() {
    for (const group of this.groups) { try { process.kill(-group, 'SIGKILL'); } catch (e) { if (e.code !== 'ESRCH') throw e; } }
  }
  remaining() { return processes().filter(([, , group]) => this.groups.has(group)); }
}

export async function admissions(run, waves, streams, destination) {
  const rows = [], verified = new Map();
  let binding;
  await fs.mkdir(destination);
  for (let wave = 0; wave < waves; wave++) {
    const cp = path.join(run, 'checkpoints', pad(wave * 2 + 1));
    let m;
    try { m = await read(path.join(cp, 'manifest.json')); }
    catch (e) { throw Error(`aic-io-readiness.incomplete-evidence: ${cp}: ${e.message}`); }
    check(['aic-training-checkpoint-v1', 'aic-training-checkpoint-v8'].includes(m.schema)
      && /^[a-f0-9]{64}$/.test(m.binding)
      && ['state.json', 'replay.json', 'sessions.json', 'learner.safetensors'].every(p => m.files?.[p])
      && m.replay && !Array.isArray(m.replay), `incomplete-evidence: manifest ${cp}`);
    binding ??= m.binding;
    check(m.binding === binding, `checkpoint-binding: ${cp}`);
    for (const [file, sha] of Object.entries(m.files)) {
      check(!file.includes('/') && !file.includes('..'), `checkpoint-path: ${file}`);
      const p = path.join(cp, file);
      check(hash(await fs.readFile(p)) === sha, `checkpoint-hash: ${p}`);
    }
    for (const a of Object.values(m.replay)) {
      check(/^replay\/\d{8}\.bin$/.test(a.path), 'replay-path');
      const p = path.join(run, a.path);
      if (!verified.has(p)) verified.set(p, hash(await fs.readFile(p)));
      check(verified.get(p) === a.sha256, `replay-hash: ${p}`);
    }
    const manifestHash = hash(await fs.readFile(path.join(cp, 'manifest.json')));
    const s = await read(path.join(cp, 'state.json'));
    check(s.update === wave * 2 + 1 && s.wave === wave && s.actions === 32 && s.next === null && s.episodes.length === streams && s.samples === s.update * streams * 32, `incomplete-evidence: wave ${wave}`);
    for (const [stream, episode] of s.episodes.entries()) {
      check(episode.depth >= 0 && episode.depth <= 2 && episode.root.wave + episode.depth === wave, `lineage: ${wave}/${stream}`);
      const id = `${wave}/${stream}`, input = path.join(destination, `${wave}-${stream}.json`);
      await write(input, episode.input);
      rows.push({ id, wave, stream, depth: episode.depth, root: episode.root, input, source: episode.input.metadata.source, slot: episode.input.metadata.slot, generation_seed: episode.input.metadata.generation_seed,
        source_sha256: digest({ request: episode.input.request, original: episode.input.original }),
        admission: episode.input.admission ?? null, checkpoint_manifest_sha256: manifestHash });
    }
  }
  check(rows.length === waves * streams, 'incomplete-evidence: count');
  return rows;
}
export function splitAudit(starts, identities, panel) {
  const keys = new Map(identities.rows.map(r => [r.id, r.admission.physical.structural]));
  check(keys.size === starts.length, 'identity-coverage');
  const reserved = new Set(panel.cases.flatMap(c => [c.admission.physical.structural, c.admission.root.structural]));
  const roots = new Map(); const overlaps = [];
  for (const s of starts) {
    const root = digest(s.root), current = keys.get(s.id);
    check(current, `identity-missing: ${s.id}`);
    if (s.depth === 0) {
      if (roots.has(root)) check(roots.get(root) === current, `lineage-collision: ${s.id}`);
      roots.set(root, current);
    }
    check(roots.has(root), `lineage-missing: ${s.id}`);
    if (s.admission) check(digest(s.admission.root) === digest(s.root.reservation) && s.admission.root.structural === roots.get(root), `lineage-admission: ${s.id}`);
    if (reserved.has(current) || reserved.has(roots.get(root))) overlaps.push(s.id);
  }
  return { complete: true, starts: starts.length, fresh: starts.filter(s => s.depth === 0).length, roots: roots.size, overlaps, valid: overlaps.length === 0 };
}
async function trainConfig(source, seed, panel, root, streams = 32) {
  const c = structuredClone(source);
  Object.assign(c, { purpose: 'bounded-comparison', catalog_root: root, initialization_seed: seed, streams, workers: 16, chunk_actions: 32, duration_seconds: 1800, finish_grace_seconds: 1 });
  Object.assign(c.evaluation, { panel, interval_updates: 32, replicates: 2, max_actions: 64 });
  Object.assign(c.retention, { checkpoint_interval_updates: 1, keep_updates: [...Array.from({ length: 16 }, (_, i) => 2 * i + 1), 32] });
  c.resources.max_output_bytes = 4 * 1024 ** 3;
  return c;
}
export function retentionProjection(measuredBytes) {
  // Two streams x two waves measured; four arms x 32 streams x 16 waves planned.
  // An estimate, not a certified upper bound: geometry and successful replay vary.
  return { measured_bytes: measuredBytes, scale: (32 / 2) * (16 / 2) * 4, estimated_bytes: measuredBytes * (32 / 2) * (16 / 2) * 4 };
}
export async function main(configPath, output) {
  const config = await read(configPath);
  check(config.schema === 'aic-io-readiness-v1', 'config-schema');
  check(config.limits.seconds === 1800 && config.limits.output_bytes === 4 * 1024 ** 3 && config.limits.rss_bytes === 48 * 1024 ** 3, 'fixed-allocation');
  check(path.resolve(output).startsWith(path.join(config.final.root, 'artifacts/learning-io-readiness')), 'output-root');
  for (const arm of [config.baseline, config.final]) {
    check(path.isAbsolute(arm.root) && path.isAbsolute(arm.binary), 'absolute-paths');
    check(hash(await fs.readFile(arm.binary)) === arm.binary_sha256, `binary-binding: ${arm.binary}`);
  }
  check(config.baseline.revision === '83f4dd8' && config.final.revision === '5fa1586', 'revision-binding');
  const sources = Object.fromEntries(await Promise.all(['baseline', 'final'].map(async label => [label, await read(path.join(config[label].root, 'tools/native-training/training.json'))])));
  for (const label of ['baseline', 'final']) {
    const c = sources[label];
    check(hash(await fs.readFile(path.join(config[label].root, 'tools/native-training/training.json'))) === config[label].template_sha256, `template-binding: ${label}`);
    check(c.model.width === 40 && c.model.message_rounds === 2 && c.ppo.epochs === 4 && c.actions.masking === 'structural-bands-v1' && c.native.max_actions === 64, 'training-contract');
    for (const p of Object.values(c.catalogs)) check(hash(await fs.readFile(path.join(config[label].root, p))) === config.catalogs[p], `catalog-binding: ${p}`);
  }
  for (const key of ['optimizer', 'ppo', 'success', 'curriculum', 'actions', 'strata']) check(digest(sources.baseline[key]) === digest(sources.final[key]), `common-settings: ${key}`);
  const native = label => { const {schema, ...rest} = sources[label].native; return rest; };
  check(digest(native('baseline')) === digest(native('final')), 'native-budgets');
  const generation = label => { const {schema, identity_limits, ...rest} = sources[label].generation; return rest; };
  check(digest(generation('baseline')) === digest(generation('final')), 'generation-settings');
  check(hash(await fs.readFile(config.native_tests)) === config.native_tests_sha256, 'native-test-binding');
  for (const [p, sha] of Object.entries(config.helper_sources)) check(hash(await fs.readFile(path.join(config.final.root,p))) === sha, `helper-binding: ${p}`);
  await fs.mkdir(output); // Existing output is never resumed by this gate.
  const gate = new Gate(output, config.limits, config.setup);
  const result = { schema: 'aic-io-readiness-result-v1', status: 'incomplete', config_sha256: hash(await fs.readFile(configPath)), arms: [], config };
  await write(path.join(output, 'registered.json'), config);
  const invoke = (name, label, args, env) => gate.run(name, config[label].binary, args, config[label].root, { ...config.environment, ...env });
  const nativeTest = (name, filter, env) => gate.run(name, config.native_tests, [filter, '--ignored', '--exact', '--nocapture'], config.final.root, { ...config.environment, ...env });
  try {
    if (config.setup) {
      const prior = config.setup.root;
      check(path.isAbsolute(prior) && prior !== output && Number.isFinite(config.setup.started_at_ms) && config.setup.started_at_ms <= (await fs.stat(path.join(prior, 'registered.json'))).mtimeMs, 'setup-origin');
      check(hash(await fs.readFile(path.join(prior, 'registered.json'))) === config.setup.registered_sha256, 'setup-binding');
      check(!(await fs.readdir(prior)).some(p => /^(baseline|final)-/.test(p) || p === 'small'), 'setup-already-trained');
      for (const [name, sha] of Object.entries(config.setup.files)) check(hash(await fs.readFile(path.join(prior, name))) === sha, `setup-file: ${name}`);
      for (const name of ['matrix.json', 'panel-v1', 'panel-v2']) await fs.cp(path.join(prior, name), path.join(output,name), {recursive:true,errorOnExist:true,force:false});
      result.setup_reused = {root:prior,started_at_ms:config.setup.started_at_ms, prior: await read(path.join(prior,'result.json'))};
    } else {
      await nativeTest('matrix', 'full_factory::blueprint::declarative::learning::runtime::readiness::integrated_io_readiness_matrix', { AIC_IO_MATRIX_REPORT: path.join(output, 'matrix.json') });
      const panelConfig = await trainConfig(sources.baseline, 71, path.join(output, 'unused-panel.json'), config.baseline.root);
      await write(path.join(output, 'panel-config.json'), panelConfig);
      await invoke('prepare-panel', 'baseline', ['learning', 'prepare-panel', '--config', path.join(output, 'panel-config.json'), '--output', path.join(output, 'panel-v1'), '--cases', '16', '--seed', '9015071']);
      await invoke('migrate-panel', 'final', ['learning', 'migrate-panel', '--input', path.join(output, 'panel-v1/panel.json'), '--output', path.join(output, 'panel-v2'), '--identity-limits', path.join(config.final.root, 'tools/native-training/identity-limits.json'), '--catalog-root', path.join(config.final.root, 'data/game/normalized')]);
    }
    const matrix = await read(path.join(output, "matrix.json"));
    check(matrix.rows.length === 7 && matrix.rows.every(r => r.counts.calls <= 32), "matrix-coverage");
    result.matrix = matrix.rows.map(({retained,outcomes,...row}) => row);
    const panel = await read(path.join(output, 'panel-v2/panel.json'));
    const oldPanel = await read(path.join(output, 'panel-v1/panel.json'));
    check(panel.cases.length === 16 && panel.cases.filter(c => c.stratum === 'small').length === 8 && panel.cases.filter(c => c.stratum === 'large').length === 8, 'panel-strata');
    check(panel.cases.every((c, i) => c.input.sha256 === oldPanel.cases[i].input.sha256 && c.action_seed === oldPanel.cases[i].action_seed), 'panel-payloads');
    result.panel = { sha256: hash(await fs.readFile(path.join(output, 'panel-v2/panel.json'))), cases: panel.cases };
    // Two waves prove ordinary pruning and complete actual-input recovery first.
    const small = await trainConfig(sources.final, 71, path.join(output, 'panel-v2/panel.json'), config.final.root, 2);
    small.workers = 2;
    small.minibatch_size = 64;
    await write(path.join(output, 'small.json'), small);
    await invoke('small', 'final', ['learning', 'train', '--config', path.join(output, 'small.json'), '--output', path.join(output, 'small'), '--updates', '4', '--device', 'cuda:0']);
    const recovered = await admissions(path.join(output, 'small'), 2, 2, path.join(output, 'small-starts'));
    check(recovered.length === 4, 'small-coverage');
    check(!(await fs.readdir(path.join(output, 'small/inputs'))).includes('00000000'), 'small-pruning');
    await fs.rename(path.join(output, 'small/checkpoints/00000001'), path.join(output, 'small/checkpoints/missing-test'));
    try {
      let rejected = false;
      try { await admissions(path.join(output, 'small'), 2, 2, path.join(output, 'missing-evidence')); } catch (e) { rejected = e.message.includes('incomplete-evidence'); }
      check(rejected, 'missing-evidence-test');
    } finally { await fs.rename(path.join(output, 'small/checkpoints/missing-test'), path.join(output, 'small/checkpoints/00000001')); }
    result.small_gate = { actual_starts: 4, old_inputs_pruned: true, missing_checkpoint_rejected: true, retained_bytes: await bytes(path.join(output, 'small')) };
    // 32 streams x 16 waves x four arms; conservative projection before allocation.
    result.retention_projection = retentionProjection(result.small_gate.retained_bytes);
    check(result.retention_projection.estimated_bytes < config.limits.output_bytes, 'retention-projection');
    const allStarts = {};
    for (const seed of [71, 113]) for (const label of ['baseline', 'final']) {
      const name = `${label}-${seed}`, run = path.join(output, name), configFile = `${run}.json`;
      const c = await trainConfig(sources[label], seed, path.join(output, label === 'baseline' ? 'panel-v1/panel.json' : 'panel-v2/panel.json'), config[label].root);
      await write(configFile, c);
      const reports = [];
      for (const end of label === 'final' && seed === 71 ? [3, 32] : [32]) {
        const args = ['learning', 'train', '--config', configFile, '--output', run, '--updates', String(end), '--device', 'cuda:0'];
        if (end === 32 && reports.length) args.push('--resume');
        await invoke(`${name}-${end}`, label, args);
        const report = await read(path.join(run, 'report.json'));
        await write(path.join(output, `${name}-report-${end}.json`), report);
        check(report.completed_updates === end, `update-endpoint: ${name}`);
        reports.push(report);
      }
      const starts = await admissions(run, 16, 32, path.join(output, `${name}-starts`));
      allStarts[name] = starts;
      await write(path.join(output, `${name}-admissions.json`), starts);
      const identityFile = path.join(output, `${name}-identities.json`);
      await nativeTest(`${name}-audit`, 'full_factory::generation::physical_identity::tests::io_readiness_admitted_identities', { AIC_IO_ADMISSIONS: path.join(output, `${name}-admissions.json`), AIC_IO_IDENTITIES: identityFile });
      const audit = splitAudit(starts, await read(identityFile), panel);
      const evaluation = await read(path.join(run, 'evaluations/00000032.json'));
      check(evaluation.update === 32 && evaluation.rows.length === 32, `evaluation: ${name}`);
      const updates = [];
      for (let n = 1; n <= 32; n++) updates.push(await read(path.join(run, 'updates', `${pad(n)}.json`)));
      check(updates.every(u => u.samples === 1024 && u.ppo.critic_epochs === 4), `sample-epochs: ${name}`);
      check(updates.some(u => u.ppo.positive_replay_labels > 0), `positive-replay: ${name}`);
      result.arms.push({ name, seed, label, reports, audit, evaluation: { ...evaluation, rows: evaluation.rows.map(({ best, ...r }) => r) },
        retained_bytes: await bytes(run), updates: updates.map(({ evaluation, ...u }) => u) });
      await write(path.join(output, 'result.json'), result);
    }
    result.fresh_matching = [71, 113].map(seed => {
      const left = allStarts[`baseline-${seed}`], right = allStarts[`final-${seed}`];
      const common = left.filter((s, i) => s.depth === 0 && right[i].depth === 0);
      const unmatched = common.filter(s => s.source_sha256 !== right.find(r => r.id === s.id).source_sha256).map(s => s.id);
      return { seed, common_fresh_slots: common.length, unmatched, strictly_matched: unmatched.length === 0 };
    });
    const mean = a => a.reduce((s, x) => s + x, 0) / a.length;
    for (const arm of result.arms) {
      arm.strata = Object.fromEntries(['small', 'large'].map(stratum => {
        const rows = arm.evaluation.rows.filter(r => r.stratum === stratum);
        return [stratum, { mean: mean(rows.map(r => r.area_reduction_percent)), improvement_rate: mean(rows.map(r => +(r.best_area < r.initial_area))) }];
      }));
      arm.equal_strata_mean = mean(Object.values(arm.strata).map(s => s.mean));
    }
    result.delta_pp = mean(result.arms.filter(a => a.label === 'final').map(a => a.equal_strata_mean)) - mean(result.arms.filter(a => a.label === 'baseline').map(a => a.equal_strata_mean));
    result.quality_screen = result.arms.every(a => a.audit.valid && a.evaluation.complete) && result.delta_pp > 0 ? 'limited-favorable' : 'not-cleared';
    result.status = 'completed';
  } catch (error) { result.error = error.stack; }
  finally {
    gate.stop();
    result.seconds = (performance.now() - gate.started) / 1000;
    for (let n = 0; n < 20 && gate.remaining().length; n++) await new Promise(resolve => setTimeout(resolve, 50));
    result.resources = { peak_rss_bytes: gate.peakRss, peak_output_bytes: gate.peakBytes, output_bytes: await bytes(output), remaining_processes: gate.remaining(), invocations: gate.invocations };
    await write(path.join(output, 'result.json'), result);
  }
  console.log(JSON.stringify({ status: result.status, error: result.error, delta_pp: result.delta_pp, seconds: result.seconds }));
  if (result.status !== 'completed') process.exitCode = 1;
  return result;
}
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [flag, config, outputFlag, output] = process.argv.slice(2);
  check(flag === '--config' && outputFlag === '--output' && config && output, 'usage: --config FILE --output NEW_DIRECTORY');
  await main(path.resolve(config), path.resolve(output));
}
