# PPO device and precision probe

## Status

Completed the action-only optimized CPU/GPU and subsequent AMP comparisons.
CUDA FP32 with microbatch 64 is the provisional choice. Richer actor inputs
remain to be checked for the newly requested input-contract comparison.

## Context

The user asked whether GPU training would be faster and suggested AMP. Follow
the [bounded probe contract](../designs/2026-09-10.00-ppo-device-precision-probe.md).
The existing learner uses CPU for both inference and optimization. Its recorded
phase times do not provide a CPU-versus-GPU comparison.

## Completion criteria

- Use the declared actual experience, pre-update state and shuffled minibatch.
- Measure every declared device/precision cell or record its explicit failure.
- Check actual optimizer steps and numeric drift, including scaled gradients.
- Record results, reproduction and retained input/output identities.
- Verify the repository and commit this diagnostic separately from stage 43.

## Activity log

- Wrote the contract before implementation. Added a standalone probe without
  changing the frozen learner or the running comparison's settings.
- Reproduced nonfinite gradients from scaled categorical entropy's masked
  finite-minimum sentinel. Zeroing masked log terms removes this problem.
- Local and Linux tests verify exact parameter and nonempty Adam-state equality
  against the frozen PPO update, including a CPU GradScaler path. This tests
  scaling order and clipping, not CUDA autocast accuracy or throughput.
- Precompute fixed selection metadata in every device cell; separately time
  observation transfer and already-resident observation arrays.
- Cancelled the earlier automatic post-audit probe waiter when the user reduced
  the confirmation and changed the task order. No device timing or GPU learning
  has run yet. The reference workload now accepts the explicit user-amended
  confirmation and does not require the cancelled full reconstruction audit.

- Removed the unused sequential device/AMP runner; preserve its reference
  minibatch utilities. Device measurements must call the optimized batched
  execution and include preparation/transfer as well as resident compute.

- The follow-up experiment is now a shallow PPO input-contract comparison,
  replacing the 12-hour long run. Account for richer actor observations when
  applying device/precision choices to that comparison.

- CUDA FP32 combined minibatch latency was 0.0846 seconds versus CPU 0.3988
  seconds. In the subsequent precision phase, FP32/BF16/FP16 measured
  0.0685/0.0721/0.0806 seconds; all numeric screens passed, with no AMP speedup.
