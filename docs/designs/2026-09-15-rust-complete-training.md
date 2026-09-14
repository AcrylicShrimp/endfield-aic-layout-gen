# Complete native training runtime

Status: implemented and verified. See
`docs/reports/2026-09-15-rust-complete-training.md` for evidence.
The complete CLI replaces the temporary one-minibatch collection command.
See `tools/native-training/README.md` for commands and migration.

## Required behavior

Provide a native CLI for fresh training and continuation, without importing or
executing Python. Preserve the existing masked graph policy, true-terminal GAE,
full PPO epochs and shuffled coverage, epoch-wide KL stopping of actor updates,
critic-only remaining epochs, and stage-balanced positive-advantage imitation.
All rollout rows participate in every configured epoch. Replay and repeated PPO
uses never count as new samples.

Preserve fresh randomized complete factories, canonical episode identity,
evaluation geometry reservation, and 80/10/10 starts from strictly improved bests
of the immediately preceding complete wave. Selected inherited states keep their
root obligations and depth lineage. Evaluation uses independent sampling and
does not update the learner, replay, training RNG or curriculum.

Successful replay includes state-changing preparation actions before a validated
improvement. Admit complete-episode sequences after that update's optimization;
compute returns to the real terminal across chunk boundaries. Preserve semantic
deduplication, balanced-available eight-sequence sampling, stage normalization,
observed-utility retention and bounded sequence/label capacity.

Use one atomic checkpoint publication mechanism containing the learner's named
model/Adam/RNG state, update and episode identity, bounded replay and active
segments, live native sessions and selected next starts. Resume uses these actual
states, not historical replay or regeneration from seeds. Retain required best
models and bounded recovery checkpoints. Failure must preserve actual partial
work without committing an incomplete update/wave.

Inputs and reports have explicit versioned schemas and path-specific diagnostics.
Fresh initialization requires no exported trained tensor fixture and starts with
empty optimizers/replay. External game catalogs are loaded on each execution.
Do not silently accept a prior diagnostic checkpoint as a complete training state.

## Continuity checklist

| Existing benefit | Reuse and verification |
| --- | --- |
| Persistent native workers and immutable shared domains | Extend existing pool lifecycle for completed episode replacement; retain ordered batched exchanges and historical ownership |
| CUDA root/depth batching and foreach Adam | Reuse current model/kernel/backend; test whole-epoch row coverage and actor-safe critic tails |
| One-batch-ahead CPU preparation and stable input caches | Prepare bounded CPU batches ahead of device work and reuse immutable numerical representations; no per-label GPU work |
| Hydrated sparse replay and indexed eviction | Keep selected historical branches in memory, transfer batched labels, observe utility during the ordinary SIL forward pass |
| Native constructors, routing, masks and budgets | Reuse live Rust construction/editor authorities; restore required random-start behavior without reviving deleted Python adapters |
| Complete-wave identity, reserved evaluation and inherited starts | Test terminal/reset boundaries, donor lineage and reservation rejection, including continuation |
| Lean buffered persistence | Extend the existing atomic save primitives; save needed state once, use bounded retention and separate explicit audits |
| Resource controls and truthful throughput | Bound observations/replay, memory, elapsed time and retained storage; measure actual CLI collection/PPO/publication and new transitions |

## Acceptance evidence

Run focused numerical/scheduling/replay/lifecycle tests, workspace checks/tests
including the learning feature, and real release CLI executions on actual inputs.
Exercise fresh training through multiple complete episodes, nonempty own replay,
held-out evaluation and a separate-process restore. Check that all minibatches
are consumed and KL termination preserves actor/Adam through critic tails.
Measure a bounded equivalent Python/Rust workload only after integrated work and
storage scope match. Leave old long runs stopped; this authorization is to build
and verify the complete runtime, not launch another long experiment.

## Final implementation choices

- Full rollout permutation and one-batch-ahead CPU preparation feed four complete
  PPO epochs. Epoch-wide KL stops actor/SIL updates while all remaining critic
  epochs continue. New samples are counted only once.
- A versioned fresh source reuses the native constructor and the existing seven
  setup-edit algorithms. SplitMix64 replaces Python's generation/shuffle RNG;
  source identity names that change. Torch policy/shuffle/replay generators stay
  separately owned. Input generation remains the existing construction followed
  by randomized valid setup edits, pending a separate generator design decision.
- Complete-episode replay admits preparation segments only after optimization
  and after a validated improvement. Terminal returns span 32-action boundaries.
  Immutable sparse numerical chunks are written only at checkpoint cadence;
  normal full-wave saves discard unsuccessful preparation before disk I/O.
- One atomic checkpoint stores current state and the held-out best model.
  Best-model saving needs only model tensors, and does not introduce another
  optimizer snapshot or publication journal. Normal restore reads actual state
  and validates stable archives once. Historical trajectory replay is absent.
- Evaluation uses a balanced immutable panel, per-case RNGs and the existing
  batched policy. It requests observations without collecting training evidence.
- Resource polling stays on a background thread. Known file sizes drive routine
  storage accounting; cold resume scans preserve uncommitted work before retry.
  Exclusive output ownership uses the standard library's file lock.
- The obsolete `collect` CLI, collection manifest/restore, seed import and
  single-minibatch adapter were deleted. Packed external-tensor validation and
  the retained numerical oracle are test-only. Python implementation files were
  left unchanged.
