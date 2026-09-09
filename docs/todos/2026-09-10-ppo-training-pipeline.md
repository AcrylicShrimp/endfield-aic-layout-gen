# PPO training pipeline and renewed input-contract comparison

## Status

Active user-requested goal. Confirmation, profiling, initial optimization and
the device/AMP setup decision are complete. Production harness and monitoring
requirements and implementation are next, followed by independent readiness
reviews/tests and the renewed shallow input comparison.

## Context

The user requested profiling and optimization before long training, then placed
bottleneck optimization immediately after profiling. The user subsequently
added production-grade training and monitoring as a mandatory stage after the
experimental setup is finalized and before any long run. Detailed requirements
are deliberately deferred to that point.

## Execution order and completion criteria

1. Complete and retain the user-shortened learning confirmation.
2. Profile the existing pipeline on retained representative experience.
3. Optimize measured bottlenecks, including removal of duplicate full-update
   reconstruction from normal training.
4. Compare CPU and GPU execution on the optimized pipeline.
5. Compare AMP precision and finalize the training setup.
6. Define the production training-harness and monitoring requirements, contracts
   and acceptance criteria, then implement them.
7. Obtain independent reviews, verify actionable findings, fix confirmed issues,
   and complete thorough tests and a representative actual execution check.
8. After readiness passes, repeat the previous input-contract model screen
   with PPO, a critic and GAE under a common shallow training/evaluation budget.
   This replaces the previously planned 12-hour long training run.

Preserve native factory validity, actual behavior probabilities, PPO/GAE
semantics, finite updates and meaningful held-out evaluation throughout. Keep
the implementation and findings in focused commits, with regenerable run output
outside tracked source directories.

## Activity log

- Created the active goal at the user's explicit request.
- Recorded the reordered profile/optimize/device/AMP sequence.
- Added the user-requested production-harness, monitoring, independent-review
  and testing gate before long training. The independent review stage explicitly
  authorizes separate reviewers at that stage; it does not require immediate
  delegation while the current experiment is running.
- Detailed production requirements will be developed when the experimental
  setup is finalized, as requested by the user.

- Completed the 32/16 confirmation (`b1e4ab7`), CPU profiling and initial
  execution optimization. Native collection, two PPO updates and exact
  nonempty checkpoint continuation passed. CPU/GPU comparison is next.
- At the user's request, normal training also drops repeated observation hashes
  and input/mask/reward validation; retain the already-computed finite gradient
  norm guard before optimizer steps. Standalone development checks stay separate.

- The user replaced the 12-hour long run with a renewed shallow comparison of
  the previous input-contract model variants now that PPO learns. Preserve
  the device/AMP and production-readiness stages. Extend and verify batched
  actor execution beyond action-only before the comparison. Select the exact
  short budget and freeze the comparison protocol before execution.

- Extended the batched actor to all twelve input contracts. All reference
  observation/gradient checks and twelve native two-batch continuation checks
  pass. Geometry, full-current and best-layout/history device matrices select
  CUDA FP32 / 64; AMP resident computation is consistently slower. The execution
  setup is finalized, so production requirements can now be developed.
