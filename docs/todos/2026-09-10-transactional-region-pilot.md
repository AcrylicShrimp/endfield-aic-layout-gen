# Transactional region repair and 32-batch PPO comparison

## Status

In progress. User authorized executor-selected transaction boundaries and a
32-batch fresh training comparison with the retained compact one-hour baseline.

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
