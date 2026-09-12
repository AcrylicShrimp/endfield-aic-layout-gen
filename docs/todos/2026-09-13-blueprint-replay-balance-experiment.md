# Replay balance experiment

Status: complete. All fifteen runs and the separate continuation diagnostic
finished; no treatment passed the registered transfer or known-state gate.
See the [results](../reports/2026-09-13-blueprint-replay-balance-results.md).

## Context

The user requested an experiment design after the existing-success diagnostic,
and requires the established optimized execution path if training is needed.
The [design](../designs/2026-09-13-blueprint-replay-balance-experiment.md) separates
importing known training successes, replay composition, and SIL coefficient.
It preserves centered GAE and current blueprint semantics.

## Completion criteria

- [x] Define hypotheses, factors, controls, continuation identities and budgets.
- [x] Define known-state learning, autonomous completion and transfer separately.
- [x] Record continuity requirements and the missing execution optimizations.
- [x] Predeclare gates, failure interpretation, resource limits and evidence.
- [x] Restore applicable optimized worker/supply/learner execution and measure it.
- [x] Pass the enforced throughput gate: eight real workers on distinct cores,
  parallel evaluation, measured learner device, at least 1.5x complete-cycle
  throughput, and an input-bound matrix forecast; reject serial launch fallback.
- [x] Implement versioned fork/import/stratified replay with cheap diagnostics.
- [x] Pass bounded readiness including nonempty Adam/buffer continuation.
- [x] Freeze the evaluation panels, on-demand training namespace, sampling seeds
  and launch manifest; retain all actual training inputs as they are generated.
- [x] Complete five arms by three seeds, 128 updates per arm, or report incomplete pairs.
- [x] Run the bounded independent anchor continuation diagnostic and report all contrasts.
- [x] Complete relevant source verification and commit implementation/results slices.

## Activity

- User authorized implementation and execution with "Start" after the throughput
  requirements. First work unit: device-capable packed graph execution and a
  persistent native worker pool using the established shared-memory transport.
  The worker owns its stream/session and complete chunk evidence. The coordinator
  owns ordered policy sampling and frozen behavior weights. Commands are begin,
  step (selected index/log probability/value), finish (bootstrap), snapshot,
  restore, evaluation and stop; responses carry a command identity, worker/stream
  identity, prepared graph arrays or completed evidence, and timing. No unfinished
  stream crosses worker ownership without an authenticated native snapshot.
- Operational inspection found 14 GiB free on the server, below the 16 GiB
  reserve. Reclaim only regenerable build caches first; preserve retained runs.

- 2026-09-13: inspected the current centered continuation, SIL buffer/loss,
  persistent session restoration, historical cohort, native worker study,
  shared-memory/padded learner report and previous completion diagnostics.
- Registered C plus a 2-by-2 supplied-example comparison: uniform versus
  four-large/four-small replay, coefficients .05 versus .50. Import the same five
  authentic training successes only in the four diagnostic treatment arms.
- Distinguished exact recorded-label inference from real continuation. The old
  completion helper changes clocks/action limits; those overrides are excluded.
- Preserved the original cohort as a continuity panel and specified a separate
  identity-filtered geometry-transfer panel. No unseen-request claim is planned.
- At design completion, no runtime code, training default, source checkpoint or
  running process had changed.
- User follow-up explicitly requires avoiding another effectively single-core
  run. Strengthened launch gates to inspect actual process affinity and quota,
  target CUDA batched learning, include evaluation/storage in timing, and reject
  unmeasured or serial fallback. Per-worker single-threading remains compatible
  with eight genuinely concurrent workers. These requirements are not yet verified.

- Implemented and verified persistent native workers, shared graph transport,
  parallel evaluation, per-slot fresh-input publication, CPU/CUDA packed updates
  and one-batch-ahead CPU preparation. See the
  [execution slice](../reports/2026-09-13-blueprint-parallel-execution.md).
  The full 1.5x throughput gate and eight-update real fork remain pending.
- User requested selective raw-evidence cleanup. Reduced server AIC artifacts
  from 102.96 to 17.41 GiB while authenticating all 12,064 files required by the
  continuation/import/cohort contract. Future runs should retain result tables,
  selected witnesses/checkpoints, active buffer sources and recovery windows,
  without accumulating every completed evaluation trace. See the
  [retention report](../reports/2026-09-13-experiment-evidence-pruning.md).
- Admitted 804 centered parent sequences and the exact five imported sequences
  with fifteen labels. The explicit child checkpoint preserves both Adam states,
  sampling/shuffle state, global/child ordinals and eight live native sessions.
- Final complete-cycle readiness selects eight workers plus CUDA: warmed median
  7.932 s versus 18.210 s serial, a 2.296x speedup; conservative screen forecast
  5,460 s. Fixed labels, evaluation and storage are included. See the
  [runtime report](../reports/2026-09-13-blueprint-replay-runtime.md).
- Native mechanical admission, eight-update fork restoration, exact CUDA update
  replay, forced KL actor freezing and a separate CUDA execution profile passed.
  Sixteen transfer factories were admitted in seventeen attempts before training.
- Published the source-bound launch manifest and started C/seed 17. Its first
  four updates completed, including an actual child-own terminal admission.
  The 15-arm screen and separate continuation/report commands are still active
  work; there is no endpoint conclusion or promotion decision yet.
- Completed all 491,520 training actions and 7,680 episode executions without a
  correctness/resource guard failure. All runs shared 512 actual new inputs.
  Main training took 107.8 minutes; the explicit 2,160-branch continuation probe
  took 66.1 seconds. The registered report command completed successfully.
- Fifteen transfer factories remain eligible after one geometry overlap with
  the continuation bank. Only B50 has positive final transfer, with 1/0/2
  improved factories by seed. It fails the required breadth across two seeds.
  Its pre-pose target rates are 17.5/12.5/17.5%, below the known-state threshold.
  B05/U50/B50 preserve pooled small continuity; U05 does not.
- Retained endpoint/recovery checkpoints, actual input payloads, source episodes,
  selected physical witnesses and compact metrics. Recorded every paired
  contrast, stage effect, realized exposure and limitation in the results report.
  No coefficient promotion, longer training or new scientific treatment followed.
