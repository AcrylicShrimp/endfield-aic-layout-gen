# Run the shared PPO capacity comparison

Status: complete.

Context: run the wider shared model, including all four history/tail combinations,
and compare measured performance against the completed width-32 controls.

Completion criteria:

- [x] Register workload, matched controls, interpretation and output contracts.
- [x] Verify CUDA/native readiness and exact resumed updates for both histories.
- [x] Freeze and complete four cells, two seeds and 32 batches per run.
- [x] Audit main results, matched inputs, checkpoint identities and W&B records.
- [x] Report paired capacity effects and learning curves for every combination.
- [x] Complete required checks, commit findings and confirm training is finished.

Activity log:

- 2026-09-11: Registered the complete four-cell comparison and added an explicit
  shared-capacity matrix mode. The previous model-only slice is `a9baf9a`.
- 2026-09-11: Linux/CUDA regression tests passed all 90 tests. Both readiness
  cells completed two batches; all 96 native result records passed validation.
  Each second-batch replay exactly matched nonempty Adam state, predictions,
  updates and the one-actor/four-critic-epoch schedule with the actor frozen in
  the tail. macOS regression passed 87 tests with three platform-specific skips;
  workspace check/tests passed.
- 2026-09-11: All eight main runs completed 256 batches and 512,512 actions in
  95.98 minutes. Final native validation passed all 248 admitted factories.
  The cross-capacity audit verified 121,033 file identities and identical inputs
  at all 4,416 generation positions; all four exact joint-update prefixes passed.
  W&B confirmed eight finished runs, 336 rows and 32,496 matching numeric scalars.
- 2026-09-11: Reported every condition/seed against both retained controls and
  inspected both learning and effect figures. Final local regression passed
  89 of 92 tests with three platform skips; workspace check/tests and targeted
  catalog identity rejection tests passed. Retained generated evidence outside
  Git and recorded compact provenance in the final report.
