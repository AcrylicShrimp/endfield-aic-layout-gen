# Transactional region repair and 32-batch PPO comparison

## Status

Complete. Implemented executor-selected transaction boundaries and completed the
authorized 32-batch fresh training comparison with the compact one-hour baseline.
Findings: `docs/reports/2026-09-10-transactional-region-pilot.md`.

## Completion criteria

- Implement the contract in `docs/designs/2026-09-10.14-transactional-route-regions.md`.
- Verify private repair, fixed boundary contacts, bounded work and rollback.
- Replay the exact retained large move using the normal new executor.
- Freeze matched baseline inputs and run exactly 32 fresh training batches.
- Compare paired evaluation trajectories and executor coverage/cost; report
  whether the evidence supports further investigation.
- Complete workspace/CLI/artifact checks and commit focused work units.

## Activity

- Read the native executor and movement ownership implementation. Target-body
  intersections already select affected chain interiors; the missing capability
  is bounded automatic repair after a route encounters retained obstacles.
- Confirmed that initial random setup also calls the executor. Equal generation
  seeds alone are insufficient for comparison, so baseline starts will be frozen.
- Implemented bounded chain expansion, automatic compatible ports, deterministic
  routing order, shared work accounting and per-attempt diagnostics.
- Passed workspace checks and 548 Rust tests (five ignored). Three selected
  external release regressions also passed full native validation and deterministic
  replay: initial-order recovery, automatic port recovery, and three-chain growth.
- Reconstructed 819 missing starts with the unchanged baseline runtime. All
  1,056 training/evaluation source identities match retained baseline hashes.
- Added normal session duration handling and successful W&B finalization for
  normal stops. Real resource/progress failures retain error termination.
- Implementation is ready for the server readiness check and the authorized
  32-batch comparison; training findings remain pending.
- Completed CUDA readiness and an online normal-duration probe. Exact initial
  weights and nonempty Adam continuation match; normal stop is W&B `finished`.
- Completed exactly 32 fresh batches and verified 1,056 matched initial-source
  identities. Final paired reduction is 57.99% versus 53.46% (20 wins, five ties,
  seven losses); the combined executor treatment warrants further bounded study.
- Verified all 160 evaluation results, 33 checkpoints and 1,435 online scalars.
  Preserved source runtime, frozen starts, checkpoints and retained trajectories.
- Added the requested direct action contribution and cumulative future-credit
  plots, including per-action averages and action-position centering. Documented
  indirect-credit and treatment-attribution limits; no additional training ran.
