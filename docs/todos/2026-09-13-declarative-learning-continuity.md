# Declarative learner and short training

Status: complete.

## Context

The user requires retaining all learner optimizations while adopting declarative
joint edits, then running a short training experiment. See the
[stage contract and continuity checklist](../designs/2026-09-13-declarative-learning-continuity.md).

## Completion criteria

- Implement the new observation/action likelihood and native episode integration.
- Preserve and verify the full existing learning/execution/recovery checklist.
- Preserve the interrupted fresh-only screen and complete the corrected bounded
  seed-17 continuation with generated and inherited starts.
- Report evaluation, coverage, real runtime and unresolved statistical scope.
- Retain actual inputs/checkpoints, run required checks and commit complete work.

## Activity

- Inspected the optimized replay runtime, persistent worker pool/shared arrays,
  packed graph model, prefetching CUDA update, actor-safe critic tail, success
  buffer, durable input store, serialization, resource guards and prior reports.
- Registered continuity before implementation. The new action contract needs
  conditional port selection and new checkpoints, while retaining compatible
  execution/optimizer algorithms and historical explicit research tooling.
- Implemented catalog-derived port geometry, packed root choices and conditional
  port assignment, with a single whole-action PPO/SIL likelihood. Shared the
  existing graph encoder, worker transport, CUDA/prefetching update and storage.
- Preserved full-episode preparation credit across valid non-improvements and
  chunk boundaries. Added explicit balance over available replay strata and
  retained source chunks needed by interrupted recovery checkpoints.
- Local Python suite passes: 449 tests, 93 skipped. The first invocation correctly
  rejected an older local native binary; rebuilding it resolved all six setup
  errors. No source semantics changed for that rebuild.
- Workspace check/tests pass: 570 tests passed, five ignored. The isolated server
  verification passes all 13 mechanical/native/CUDA tests, including retained
  predecessor optimizer equivalence and new joint likelihood/credit checks.
- Prepared a separate 32-factory evaluation panel in 40 generated slots, balanced
  16/16 by product stratum. The panel is frozen independently of learning outcomes.
- The server continuity run restored eight real live 32-action drafts and exactly
  replayed a nonempty-Adam CUDA update. All twelve complete-cycle measurements finished: median serial CPU 30.780 s,
  parallel CPU 26.133 s, parallel CUDA 21.850 s; native concurrency eight.
- An inappropriate inherited 1.5 speedup assertion prevented summary publication
  despite all cycles completing. Removed that reporting-only gate and recovered
  the receipt from retained evidence without rerunning any cycle. Actual speedup
  is 1.409, recorded as a comparison rather than a preserved quantitative claim.
- Started the normal registered three-seed training command at 14:35 KST after
  all diagnostic workers/profilers ended. Measured forecast was 2,126.6 seconds.
  The new learner starts from fresh model/optimizer state and an empty replay
  buffer; compatible optimization algorithms and execution machinery are shared.
- User review identified the omitted 80/10/10 best-state start curriculum. The
  continuity checklist had failed to carry this established input requirement
  across the earlier blueprint transition. Reopened continuity; success replay
  cannot substitute for transformed initial geometries.
- The user then explicitly changed donor eligibility: retain only parents with
  strict area improvement in their latest completed pass, at both depths.
  Preserve target quotas while reporting real eligible pools, filled slots and
  fresh substitutions. Do not rank improving parents by gain.
- Preserved seed 17 through update 128 (1,251.2 seconds), including final panel
  means 30.715% small and 19.759% large. The initial SIGINT attempts did not stop
  execution; SIGTERM stopped the coordinator and workers with seed 29 at its
  published update-22 checkpoint. Corrected the earlier premature stop claim.
  The pre-correction launch remains explicitly fresh-only and stopped.
- Added an isolated local saved-record behavior analyzer. At seed-17 update 64,
  large-factory deletion choice share rose from 32.2% to 71.0%, while overlap
  occupancy fell from 95.4% to 46.3%. A fixed contiguous eight-episode training
  sample has 34 positive segments: 32 end in deletion and two in placement.
  Preceding insertion/movement is observed but not proven causally necessary.

- Implemented explicit v1-to-v2 completed-episode checkpoint import, durable
  next-wave plans, exact inherited native best admission and live restoration.
  Continuation retains the learned model, both Adam states, RNGs and success replay.
- Local changed-contract tests pass: four curriculum/cache/native tests and eight
  declarative learner regression tests. The isolated server diagnostic imports
  the actual update-128 learner and runs six updates on separate fresh inputs.
- Final pre-correction behavior: large-factory deletion share 91.65%, overlap
  occupancy 11.67%, rejection share 68.16%, mean area reduction 19.759%. This is
  evidence of learned gap compression; general facility rearrangement has not
  been established by these action counts.

- Corrected a diagnostic elapsed-time comparison to use the committed checkpoint,
  then resumed after three published updates. All six verification updates passed
  without repeating completed work; exact imported learner state was preserved.
- Normal continuation completed 32 updates / 8,192 actions / 128 episodes in
  547.332 seconds. Depth-one slots filled 13/13; depth two 6/13. Seven shortages
  became fresh starts. Further improvements occurred in 6/13 and 4/6 inherited
  episodes. Final held-out means: 34.977% small and 20.008% large.
- Closed input/recovery/retention/resource/evaluation continuity using actual
  normal-run evidence. Corrected summary timing to exclude reused evaluation,
  without editing the frozen training source or rerunning any rollout.
- User retained the current constructor-output plus policy-edit input domain;
  no additional generation redesign, long run or batch-size experiment launched.
