# Compare lower PPO learning rates

Status: complete.

Context: the user requested comparing several lower learning rates, excluding
rates above the current 0.0003. Fix the wider shared history/critic-tail model
and compare 0.0001 and 0.00003 against its retained 0.0003 control.

Completion criteria:

- [x] Register the comparison, controls and required evidence before execution.
- [x] Verify bounded readiness and freeze the main source.
- [x] Complete both rates, two seeds and all 32 training batches.
- [x] Audit native outputs, common inputs/initial states and W&B records.
- [x] Report paired quality, KL/update behavior and learning curves.
- [x] Pass required checks, commit results and confirm training has ended.

Activity log:

- 2026-09-11: Registered two lower rates and the retained wider-shared control.
- 2026-09-11: Both native/CUDA readiness cells passed exact nonempty-Adam replay,
  learning-rate checks and 96 native result validations. All 93 Linux/CUDA
  regression tests passed. macOS regression passed 90 tests with three platform
  skips; workspace check and tests passed.
- 2026-09-11: Froze main source at `5d3ed19` in a separate server runtime and
  launched the two-rate, four-run matrix. Verified identical readiness runtime
  dependencies and equal control inputs; the only runtime code change from the
  retained control is the learning-rate label in W&B.
- 2026-09-11: All four runs completed 128 batches and 256,256 actions in 56.75
  minutes. Native final validation passed 124 factories. Verified 60,793 evidence
  identities, 4,416 matching generation positions, 5,888 identical initial
  behavior steps and the learning rates in all 198 checkpoints including controls.
- 2026-09-11: W&B verified four finished runs, matching configurations, 168 rows
  and 16,142 scalars. Final Linux/CUDA regression passed all 95 tests; local
  regression passed 92 with three platform skips. Workspace checks/tests passed.
  Reported all rates/seeds, KL and actual update work; inspected both figures.
