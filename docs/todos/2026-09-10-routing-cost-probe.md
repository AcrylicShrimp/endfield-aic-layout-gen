# Routing cost and bounded reachability probe

Status: implementation and launch complete; the new twelve-hour run is ongoing.

## Context and contract

The user asks whether larger search budgets are computationally expensive and
whether routing can do more useful work within reasonable time. Build on the
frozen 17-case investigation, stopping and preserving the former twelve-hour training runtime.

Inputs: the verified original pre-edit geometries, exact actions, original
requests/catalogs and native reference. Outputs: repeated executor and full-probe
costs, valid-candidate/area yield, and a separately compiled bounded reverse
reachability experiment. Charge every reverse expansion to the same transaction
work budget. Do not infer no-path from a truncated reverse search. All positive
factory candidates require independent original-request admission.

The isolated experiment retains actual direction and crossing geometry, relaxes
only the crossing-count limit for a necessary reachability check, and permits
repeated cells in that check. A negative result applies only to the fixed path
query. Blocker diagnostics may differ from the forward search; explicitly record
this as a repair-choice change. Promote only measured and validated improvements, then freeze a new runtime.

Additional authorized work: add a weak failure reward cost with separated area
and penalty accounting; start a new twelve-hour experiment from random initialization, with the same
initialization seed, two-layer architecture and held-out factories as the original
CNN-2 experiment. Do not import any model, optimizer or random-stream state from
the stopped run. The user explicitly corrected the earlier warm-start plan.

## Completion criteria

- [x] Repeat cost measurements at 5k/10k/25k/50k and report population limits.
- [x] Compare an isolated reverse-reachability prototype at the same work budget.
- [x] Independently validate candidates and check negative-search soundness.
- [x] Implement measured routing improvements and weak failure reward cost.
- [x] Verify fresh random initialization and a bounded CUDA readiness run.
- [x] Freeze and launch a new independent twelve-hour experiment; verify progress.
- [x] Record findings, limitations and reproduction sources; commit checked work.

## Activity

- 2026-09-10: Prior census and matrix committed as `daa3c89`.

- 2026-09-10: User explicitly requested stopping the old run and implementing all
  improvements before a new twelve-hour experiment. Old run stopped normally at
  526 committed batches / 528,256 admitted actions; tracking finalized with
  `user-stop`. Partial batch 527 and the last checkpoint are retained.
- 2026-09-10: Five-repeat reference cost probe completed; a 64-state reverse
  precheck reduces 50k work by 17.92% on the selected cohort. Uniform division
  across remaining attempts regressed candidate yield and will not be shipped.

- 2026-09-10: User explicitly requires training from scratch to compare changes;
  abandoned the warm-start implementation before committing it. New run must have
  no continuation or parent checkpoint initialization.

- 2026-09-10: Routing implementation committed as `9e3ce33`; frozen motivating
  move now validates in 2,384 expansions. Final 17-case matrix retains nine
  50k candidates while reducing work by 17.91%.
- 2026-09-10: Reward conservation/negative-credit and frozen-prefix tests pass
  within all 60 PPO tests. New configuration has no continuation field and
  freezes the original first 32 training batches for a controlled early comparison.

- 2026-09-10: Fresh CUDA readiness completed; 80 native results and exact second
  PPO update reconstruction passed. Initial tensors/random streams match the
  original untrained CNN-2 model and optimizers are empty.
- 2026-09-10: Started the new independent twelve-hour service at 19:44:55 KST,
  expected duration end 2026-09-11 07:44:55 KST. Four committed batches and online
  W&B reward/failure metrics verified. Launch report:
  `docs/reports/2026-09-10-fresh-cnn2-routing-twelve-hour-launch.md`.
