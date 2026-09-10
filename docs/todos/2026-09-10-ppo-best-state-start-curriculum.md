# Best-state start curriculum experiment

Status: in progress.

The user requested an 80/10/10 mixture of fresh, once-optimized and twice-optimized
factory starts, followed by testing and a fresh replacement experiment. The
running routing/reward experiment stays active until the replacement is verified.

Completion criteria:

- [x] Define scheduling, ancestry, admission, retention and measurement contracts.
- [x] Implement deterministic start plans and durable parent bundles.
- [x] Add per-depth and episode-position metrics.
- [x] Verify unit tests, workspace checks, native validity and CUDA PPO updates.
- [ ] Stop the prior run, preserve its final checkpoint and start a fresh 12h run.
- [ ] Confirm initial weights, live mixed-depth batches and online tracking.
- [ ] Commit implementation and launch findings with compact provenance.

Activity log:

- 2026-09-10: Defined the three-depth contract and lagged integer quotas. Reuse
  factory best states; resample all trajectories under the current policy.
- 2026-09-10: User selected 64 factories per batch. Preserve 32 actions per
  episode and evaluate every four batches on the same fixed 32 factories.
- 2026-09-10: All 67 PPO tests and workspace checks/tests passed. Four native/CUDA
  batches exercised both inheritance depths; all 32 inherited inputs passed
  admission, and the depth-2-containing third PPO update reconstructed exactly.
