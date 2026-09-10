# Run the shared PPO capacity comparison

Status: in progress.

Context: run the wider shared model, including all four history/tail combinations,
and compare measured performance against the completed width-32 controls.

Completion criteria:

- [x] Register workload, matched controls, interpretation and output contracts.
- [x] Verify CUDA/native readiness and exact resumed updates for both histories.
- [ ] Freeze and complete four cells, two seeds and 32 batches per run.
- [ ] Audit main results, matched inputs, checkpoint identities and W&B records.
- [ ] Report paired capacity effects and learning curves for every combination.
- [ ] Complete required checks, commit findings and confirm training is finished.

Activity log:

- 2026-09-11: Registered the complete four-cell comparison and added an explicit
  shared-capacity matrix mode. The previous model-only slice is `a9baf9a`.
- 2026-09-11: Linux/CUDA regression tests passed all 90 tests. Both readiness
  cells completed two batches; all 96 native result records passed validation.
  Each second-batch replay exactly matched nonempty Adam state, predictions,
  updates and the one-actor/four-critic-epoch schedule with the actor frozen in
  the tail. macOS regression passed 87 tests with three platform-specific skips;
  workspace check/tests passed.
