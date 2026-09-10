# Best-state start curriculum experiment

Status: implementation, verification and fresh launch complete; training stopped at the user's request.

The user requested an 80/10/10 mixture of fresh, once-optimized and twice-optimized
factory starts, followed by testing and a fresh replacement experiment. The
preceding routing/reward experiment stayed active until replacement readiness passed.

Completion criteria:

- [x] Define scheduling, ancestry, admission, retention and measurement contracts.
- [x] Implement deterministic start plans and durable parent bundles.
- [x] Add per-depth and episode-position metrics.
- [x] Verify unit tests, workspace checks, native validity and CUDA PPO updates.
- [x] Stop the prior run, preserve its final checkpoint and start a fresh 12h run.
- [x] Confirm initial weights, live mixed-depth batches and online tracking.
- [x] Commit implementation and launch findings with compact provenance.

Activity log:

- 2026-09-10: Defined the three-depth contract and lagged integer quotas. Reuse
  factory best states; resample all trajectories under the current policy.
- 2026-09-10: User selected 64 factories per batch. Preserve 32 actions per
  episode and evaluate every four batches on the same fixed 32 factories.
- 2026-09-10: All 67 PPO tests and workspace checks/tests passed. Four native/CUDA
  batches exercised both inheritance depths; all 32 inherited inputs passed
  admission, and the depth-2-containing third PPO update reconstructed exactly.
- 2026-09-10: Committed implementation as `a9af915`. Stopped the predecessor at
  701 batches / 704,128 actions and verified its preserved checkpoint hash.
- 2026-09-10: Started the fresh twelve-hour run at 23:02:22 KST. All initial
  model tensors and random states match the original untrained seed-17 model.
  Four live batches committed 8,000 actions; all 32 inherited starts passed
  admission and lineage checks. Remote W&B configuration and new metrics match
  local records. See the [launch report](../reports/2026-09-10-ppo-cnn2-curriculum-twelve-hour-launch.md).
- 2026-09-10: User stopped training to prioritize data-supply optimization.
  The run finalized normally at 32 batches / 64,608 actions. Its checkpoint hash
  and completed W&B export were verified. The twelve-hour duration was not completed.
