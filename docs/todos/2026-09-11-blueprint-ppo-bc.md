# Blueprint PPO plus behavior cloning

Status: in progress.

Context: the user approved a small supervised auxiliary loss after the post-pose
recovery census identified useful moves abandoned before repair and commit.

Completion criteria:
- Freeze validated, deduplicated move/repair/commit demonstrations and holdout.
- Verify zero-coefficient parity, BC gradients, KL freeze and exact update replay.
- Complete the registered two-arm, two-seed screen and autonomous evaluation.
- Audit native witnesses and learner updates; report outcomes without retuning.
- Pass repository verification and commit the completed implementation and findings.

Activity log:
- 2026-09-11: Registered the experiment before implementation and data selection.
- 2026-09-11: Built 15 unique validated demonstrations from 23 source cases;
  split 11 training and 4 holdout demonstrations across disjoint physical groups.
- 2026-09-11: Passed dataset replay, zero-coefficient parity, gradient ownership,
  KL freeze, nonempty Adam replay, native readiness and full repository tests.
