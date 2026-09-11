# Compare lower PPO learning rates

Status: in progress.

Context: the user requested comparing several lower learning rates, excluding
rates above the current 0.0003. Fix the wider shared history/critic-tail model
and compare 0.0001 and 0.00003 against its retained 0.0003 control.

Completion criteria:

- [x] Register the comparison, controls and required evidence before execution.
- [ ] Verify bounded readiness and freeze the main source.
- [ ] Complete both rates, two seeds and all 32 training batches.
- [ ] Audit native outputs, common inputs/initial states and W&B records.
- [ ] Report paired quality, KL/update behavior and learning curves.
- [ ] Pass required checks, commit results and confirm training has ended.

Activity log:

- 2026-09-11: Registered two lower rates and the retained wider-shared control.
- 2026-09-11: Both native/CUDA readiness cells passed exact nonempty-Adam replay,
  learning-rate checks and 96 native result validations. All 93 Linux/CUDA
  regression tests passed. macOS regression passed 90 tests with three platform
  skips; workspace check and tests passed.
