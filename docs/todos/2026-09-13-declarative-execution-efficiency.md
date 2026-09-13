# Declarative execution efficiency

Status: implementation and isolated verification complete; main resume pending.

Context: the active seed-29/batch-1024 run averages about 40 seconds per update;
the user requests improving it now for future 12–48-hour training and explicitly
authorizes pausing the four-hour run for isolated experiments.

Completion criteria:
- Record and close the continuity checklist in the matching design.
- Identify costs using retained evidence and bounded isolated measurement.
- Implement and measure improvements under comparable input/learning conditions.
- Preserve source-bound learner/replay/inputs, recovery, best models and remaining
  duration; resume normal training after all diagnostic processes have ended.
- Document results, proportional verification, tradeoffs and commit the work.

Activity:
- Inspected existing execution, curriculum, replay replacement, retention and
  storage contracts and earlier parallel-execution/continuity reports.
- Recorded the retained 16-update baseline and bounded diagnostic stopping rules.

- Paused the original duration run at update 80 with a complete native wave and
  2,170.087758415 seconds charged; preserved its source and actual inputs.
- Measured the original two-update cycle, one storage roundtrip and one targeted
  terminal-ingestion profile. The 13-second figure includes evidence loading.
- Replaced repeated victim scans, full normal trajectories, recursive tensor
  conversions, repeated input transmission and immediate evidence reloads.
- Verified CPU/GPU sampled-action equivalence and exact retained PPO/Adam/RNG/
  replay transitions, including compact-source restoration between updates.
- Fixed catalog-location migration and persistent mmap descriptor ownership
  exposed by normal-command testing; retained explicit regression coverage.
- Passed 50 server tests, two normal updates, fixed evaluation, final reporting
  and 32 unfinished native-stream restorations. Warm terminal update: 45.357 to
  9.715 seconds. Rust workspace check and tests pass.
- Closed the design continuity checklist; prepare and resume the matching main
  launch, then record its source commit, startup identity and remaining budget.

Known separate correction: the pre-existing per-update fresh-generation metric
includes inherited starts and can produce a negative rejection count. Use the
endpoint input-coverage report for this comparison; repair that metric separately.
