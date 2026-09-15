// Resume one resource-interrupted arm inside the original registered allocation.
import fs from 'node:fs/promises';
import { constants } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { Gate, admissions, bytes, hash, publishResult, read, splitAudit, write } from './io-readiness.mjs';

const check = (ok, message) => { if (!ok) throw Error(`aic-io-readiness-recovery.${message}`); };
const canonical = value => Array.isArray(value) ? value.map(canonical)
  : value && typeof value === 'object'
    ? Object.fromEntries(Object.keys(value).sort().map(key => [key, canonical(value[key])]))
    : value;
const digest = value => hash(JSON.stringify(canonical(value)));
const pad = value => String(value).padStart(8, '0');
const mean = values => values.reduce((sum, value) => sum + value, 0) / values.length;

async function requireStopped(invocation) {
  try {
    const executable = await fs.readlink(`/proc/${invocation.pid}/exe`);
    check(executable !== invocation.command, `owned-process-live: ${invocation.pid}`);
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
}

async function armEvidence(root, name, label, seed, reports, panel, gate, config) {
  const run = path.join(root, name);
  const starts = await admissions(run, 16, 32, path.join(root, `${name}-starts`));
  await write(path.join(root, `${name}-admissions.json`), starts);
  const identityFile = path.join(root, `${name}-identities.json`);
  await gate.run(`${name}-audit`, config.native_tests,
    ['full_factory::generation::physical_identity::tests::io_readiness_admitted_identities', '--ignored', '--exact', '--nocapture'],
    config.final.root, { ...config.environment, AIC_IO_ADMISSIONS: path.join(root, `${name}-admissions.json`), AIC_IO_IDENTITIES: identityFile });
  const audit = splitAudit(starts, await read(identityFile), panel);
  const evaluation = await read(path.join(run, 'evaluations/00000032.json'));
  check(evaluation.update === 32 && evaluation.rows.length === 32 && evaluation.complete === true, `evaluation: ${name}`);
  const updates = [];
  for (let ordinal = 1; ordinal <= 32; ordinal++) updates.push(await read(path.join(run, 'updates', `${pad(ordinal)}.json`)));
  check(updates.every(update => update.samples === 1024 && update.ppo.critic_epochs === 4), `sample-epochs: ${name}`);
  check(updates.some(update => update.ppo.positive_replay_labels > 0), `positive-replay: ${name}`);
  return { arm: { name, seed, label, reports, audit,
    evaluation: { ...evaluation, rows: evaluation.rows.map(({ best, ...row }) => row) },
    retained_bytes: await bytes(run), updates: updates.map(({ evaluation, ...update }) => update) }, starts };
}

export async function recover(configPath, output, expectedResultSha256, reclaimedBytes) {
  const config = await read(configPath);
  const registered = path.join(output, 'registered.json');
  check(digest(config) === digest(await read(registered)), 'registered-config');
  check(config.limits.seconds === 1800 && config.limits.output_bytes === 24 * 1024 ** 3
    && config.limits.rss_bytes === 48 * 1024 ** 3, 'registered-limits');
  const resultPath = path.join(output, 'result.json');
  check(hash(await fs.readFile(resultPath)) === expectedResultSha256, 'prior-result');
  const result = await read(resultPath);
  check(result.status === 'incomplete' && result.error.includes('budget-free-space')
    && result.arms.map(arm => arm.name).join(',') === 'baseline-71,final-71,baseline-113', 'prior-state');
  check(result.seconds > 0 && result.seconds < config.limits.seconds, 'prior-clock');
  const failed = result.resources.invocations.at(-1);
  check(failed.name === 'final-113-32' && failed.error === 'aic-io-readiness.budget-free-space', 'failed-invocation');
  await requireStopped(failed);
  const latest = await read(path.join(output, 'final-113/latest.json'));
  const checkpoint = path.join(output, 'final-113', latest);
  const state = await read(path.join(checkpoint, 'state.json'));
  check(state.update > 0 && state.update < 32 && state.samples === state.update * 1024, 'partial-checkpoint');
  await fs.copyFile(resultPath, path.join(output, 'result-before-recovery.json'), constants.COPYFILE_EXCL);
  const driverPath = fileURLToPath(import.meta.url);
  await write(path.join(output, 'recovery-registered.json'), {
    schema: 'aic-io-readiness-recovery-v1', prior_result_sha256: expectedResultSha256,
    driver_sha256: hash(await fs.readFile(driverPath)), resumed_arm: 'final-113',
    prior_seconds: result.seconds, prior_update: state.update, reclaimed_debug_cache_bytes: reclaimedBytes,
    limits: config.limits,
  });
  const setupRoots = config.setup.roots.map(entry => entry.root);
  const gate = new Gate(output, config.limits, setupRoots);
  gate.started = performance.now() - result.seconds * 1000;
  gate.invocations = result.resources.invocations;
  try {
    await gate.enforce();
    await gate.run('final-113-recovery', config.final.binary,
      ['learning', 'train', '--config', path.join(output, 'final-113.json'), '--output', path.join(output, 'final-113'),
        '--updates', '32', '--device', 'cuda:0', '--resume'], config.final.root, config.environment);
    const report = await read(path.join(output, 'final-113/report.json'));
    check(report.completed_updates === 32 && report.previous_update === state.update && report.resumed === true, 'resume-endpoint');
    await write(path.join(output, 'final-113-report-recovery-32.json'), report);
    const panel = await read(path.join(output, 'panel-v2/panel.json'));
    const evidence = await armEvidence(output, 'final-113', 'final', 113, [report], panel, gate, config);
    result.arms.push(evidence.arm);
    const starts = Object.fromEntries(await Promise.all(result.arms.map(async arm => [arm.name,
      arm.name === 'final-113' ? evidence.starts : await read(path.join(output, `${arm.name}-admissions.json`))])));
    result.fresh_matching = [71, 113].map(seed => {
      const baseline = starts[`baseline-${seed}`], final = starts[`final-${seed}`];
      const common = baseline.filter((start, index) => start.depth === 0 && final[index].depth === 0);
      const unmatched = common.filter(start => start.source_sha256 !== final.find(row => row.id === start.id).source_sha256).map(start => start.id);
      return { seed, common_fresh_slots: common.length, unmatched, strictly_matched: unmatched.length === 0 };
    });
    for (const arm of result.arms) {
      arm.strata = Object.fromEntries(['small', 'large'].map(stratum => {
        const rows = arm.evaluation.rows.filter(row => row.stratum === stratum);
        return [stratum, { mean: mean(rows.map(row => row.area_reduction_percent)),
          improvement_rate: mean(rows.map(row => +(row.best_area < row.initial_area))) }];
      }));
      arm.equal_strata_mean = mean(Object.values(arm.strata).map(stratum => stratum.mean));
    }
    result.delta_pp = mean(result.arms.filter(arm => arm.label === 'final').map(arm => arm.equal_strata_mean))
      - mean(result.arms.filter(arm => arm.label === 'baseline').map(arm => arm.equal_strata_mean));
    result.quality_screen = result.arms.every(arm => arm.audit.valid && arm.evaluation.complete)
      && result.delta_pp > 0 ? 'limited-favorable' : 'not-cleared';
    result.status = 'completed';
    delete result.error;
    result.recovery = { reason: 'min-free-space', resumed_from_update: state.update,
      reclaimed_debug_cache_bytes: reclaimedBytes, prior_result_sha256: expectedResultSha256 };
  } catch (error) {
    result.status = 'incomplete';
    result.error = error.stack;
  } finally {
    gate.stop();
    for (let count = 0; count < 20 && gate.remaining().length; count++) await new Promise(resolve => setTimeout(resolve, 50));
    const retainedSetupBytes = (await Promise.all(setupRoots.map(bytes))).reduce((sum, value) => sum + value, 0);
    const scientificRootBytes = await bytes(output);
    result.seconds = (performance.now() - gate.started) / 1000;
    result.resources = { peak_rss_bytes: Math.max(result.resources.peak_rss_bytes, gate.peakRss),
      retained_setup_bytes: retainedSetupBytes, scientific_root_bytes: scientificRootBytes,
      combined_retained_bytes: retainedSetupBytes + scientificRootBytes,
      combined_peak_output_bytes: Math.max(result.resources.combined_peak_output_bytes, gate.peakCombinedBytes),
      peak_retained_setup_bytes: Math.max(result.resources.peak_retained_setup_bytes, gate.peakSetupBytes),
      peak_scientific_root_bytes: Math.max(result.resources.peak_scientific_root_bytes, gate.peakScientificBytes),
      remaining_processes: gate.remaining(), invocations: gate.invocations };
    if (result.resources.combined_retained_bytes > config.limits.output_bytes
      || result.resources.combined_peak_output_bytes > config.limits.output_bytes) {
      result.status = 'incomplete';
      result.error ??= 'aic-io-readiness-recovery.budget-output-closeout';
    }
    await publishResult(output, result, retainedSetupBytes, config.limits);
  }
  console.log(JSON.stringify({ status: result.status, error: result.error, delta_pp: result.delta_pp,
    seconds: result.seconds, resources: result.resources }));
  if (result.status !== 'completed') process.exitCode = 1;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [configFlag, config, outputFlag, output, resultFlag, resultSha, reclaimedFlag, reclaimed] = process.argv.slice(2);
  check(configFlag === '--config' && outputFlag === '--output' && resultFlag === '--prior-result-sha256'
    && reclaimedFlag === '--reclaimed-debug-cache-bytes' && /^\d+$/.test(reclaimed ?? ''), 'usage');
  await recover(path.resolve(config), path.resolve(output), resultSha, Number(reclaimed));
}
