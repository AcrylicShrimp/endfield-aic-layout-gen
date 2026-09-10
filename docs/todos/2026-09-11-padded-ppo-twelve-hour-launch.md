# Fresh padded PPO twelve-hour launch

Status: launch complete; training stopped by user after 258 batches.

Context: the user accepted the measured data-supply and padded CNN optimizations
and requested a completely fresh twelve-hour training run. Implementation
`d20b416` passed 79 native/CUDA PPO tests and the bounded native validity,
curriculum and exact within-runtime continuation audit.

Launch contract: use the existing twelve-hour configuration with sixteen CPU
collectors, a CUDA FP32 learner, 64 factory attempts per batch, 32 actions per
episode, 64-selection PPO minibatches and the 80/10/10 best-state curriculum.
Initialize actor, critic, Adam and random generators afresh at seed 17. Import
no trained checkpoint or predecessor curriculum bank. Retain the fixed input
manifest and evaluation cohort for comparison. Freeze source/configuration in
a new runtime and use a new run directory and W&B identity. Run under a
server-owned service for 43,200 seconds, with normal checkpoint-preserving
finalization and the existing resource limits.

Completion criteria for launch:

- [x] Freeze current sources, native binaries, configuration and input identities.
- [x] Prepare a new protocol without continuation or prior state.
- [x] Launch an independent twelve-hour user service.
- [x] Verify fresh initial parameters and empty Adam states.
- [x] Verify committed learning, curriculum lineage and live W&B export.
- [x] Record launch time, nominal end, service/run identity and provenance.

Activity log:

- 2026-09-11: User authorized the fresh run after accepting padded/indexed learner
  measurements. The previous 32-batch checkpoint remains preserved and stopped.
  Launch verification does not claim that twelve-hour training has completed.
- 2026-09-11: Started at 01:10:31 KST, nominal duration end 13:10:31 KST. All
  initial tensors and RNG states match fresh seed-17 initialization; Adam is
  empty. Four training batches / 8,000 actions, 32 inherited inputs and 565
  remote W&B scalars verified. The service was still running with six committed
  batches at the cloud-check snapshot. See the
  [launch report](../reports/2026-09-11-padded-ppo-twelve-hour-launch.md).
- 2026-09-11: User requested stopping before actor/critic combination experiments.
  The stop completed at 258 batches and 520,160 actions; final checkpoint hash,
  service exit and tracking finalization verified. See the
  [stop report](../reports/2026-09-11-padded-ppo-user-stop.md).
