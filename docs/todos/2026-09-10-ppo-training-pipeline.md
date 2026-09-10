# PPO training pipeline and renewed input-contract comparison

## Status

Complete. Confirmation, profiling, measured bottleneck optimization, the
device/AMP decision, rich-input execution, reviewed production harness and
monitoring, readiness checks, and the renewed shallow comparison are complete.
All 24 candidate/seed jobs finished; findings and retained evidence are recorded
in the [comparison report](../reports/2026-09-10.06-shallow-ppo-input-comparison.md).

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

- Implemented the durable CLI harness and W&B monitoring, including explicit
  terminal export. Three independent reviews and follow-up checks confirmed
  fixes to state races, directory durability, result boundaries, process ownership,
  monitoring health, shared evaluation cohorts and external preflight.
- Readiness passed on macOS/Linux (23 harness tests and seven PPO tests), a
  two-contract GPU stop/resume run and a 24-worker rich-input H32 run. Native
  validity, nonempty exact CUDA continuation and actual W&B scalars passed.
  Retained evidence and frozen runtimes with local/server SHA-256 verification.

- Froze the renewed shallow protocol: 12 contracts × two seeds, four batches
  of 32 fresh attempts at H32, evaluation at 0/2/4 on 16 held-out draws. This
  keeps the old shallow comparison's maximum 4,096 training selections per
  candidate/seed. Execution uses the reviewed harness with primary W&B tracking.

- Completed all 24 jobs and 96 batches: 96,000 training actions and 36,864
  evaluation actions. All 120 checkpoint hashes and 6,072 W&B scalars passed
  the terminal record check. Best observed mean: current-history-16 at 26.37%
  area reduction (+7.48 points), narrowly ahead of current-best-3 at 26.18%.
- Corrected the W&B stale resume cursor with acknowledged remote history;
  regression tests and an actual no-resubmission SDK probe pass. Preserved
  the original experiment runtime separately and retained the final archive
  with matching workstation/server hashes. The shallow comparison is complete;
  the cancelled long run and fixed-reset screen remain stopped.

- After completion, the user selected current-history-16 as the baseline for
  subsequent training. Recorded its measured runtime decomposition and scaling
  limits in the comparison report; the completed screen protocol is unchanged.
