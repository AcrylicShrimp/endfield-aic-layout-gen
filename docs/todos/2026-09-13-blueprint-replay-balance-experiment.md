# Replay balance experiment

Status: implementation in progress; main training is gated on measured readiness.

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
- [ ] Restore applicable optimized worker/supply/learner execution and measure it.
- [ ] Pass the enforced throughput gate: eight real workers on distinct cores,
  parallel evaluation, measured learner device, at least 1.5x complete-cycle
  throughput, and an input-bound matrix forecast; reject serial launch fallback.
- [ ] Implement versioned fork/import/stratified replay with cheap diagnostics.
- [ ] Pass bounded readiness including nonempty Adam/buffer continuation.
- [ ] Freeze actual training/evaluation inputs, sampling seeds and launch manifest.
- [ ] Complete five arms by three seeds, 128 updates per arm, or report incomplete pairs.
- [ ] Run the bounded independent anchor continuation diagnostic and report all contrasts.
- [ ] Complete relevant source verification and commit implementation/results slices.

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
