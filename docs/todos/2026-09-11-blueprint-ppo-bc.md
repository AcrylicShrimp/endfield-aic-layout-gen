# Blueprint PPO plus behavior cloning

Status: complete.

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
- 2026-09-11: Completed both two-seed arms: 65,536 total on-policy actions;
  every one of 256 learner updates replayed exactly and all retained witnesses
  passed native admission. Both initial model/optimizer/RNG states match.
- 2026-09-11: Audited 96 ordinary timed cases and 128 held-out completion cases.
  Final mean area reduction rose from 2.08% to 10.00%; held-out pre/post target
  completion rose from 0/16 and 3/16 to 6/16 and 7/16. Same-pose commits stayed
  at 3/16, so this is a narrow sequence-learning result, not solved recovery.
- 2026-09-11: Recorded the C1-only demonstration scope, correlated holdout groups,
  extra supervised compute, timing limits, and restoration used on overlap starts.
  Retained complete source-bound evidence, plots and a compact provenance manifest.
