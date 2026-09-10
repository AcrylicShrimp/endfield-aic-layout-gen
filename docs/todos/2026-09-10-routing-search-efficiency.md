# Routing search efficiency investigation

Status: complete. Findings and follow-up experiments are recorded in
`docs/reports/2026-09-10-routing-search-efficiency.md`.

## Context and contract

A recorded PPO move failed at 5,000 expansions but validated at 9,341 with the
existing port retry. The user asks how common search failures are and why routing
spends so much work. Keep the twelve-hour training runtime and settings immutable.
This is a read-only investigation of its committed records and isolated native
replays, not a training or routing-policy cutover.

Inputs are an immutable snapshot of the run's committed phase references, retained
traces, original requests, native binaries/catalogs and exact selected action DTOs.
Outputs distinguish the all-training status census from retained-training cause
samples, repeated evaluation observations and the latest fixed evaluation cohort.
Never infer a complete training cause distribution from cleaned-up trajectories.

Measure status/cause counts, actual A* expansions, elapsed execution costs, attempt
counts, failure stage, port/order changes and valid-candidate yield. Freeze one
budget failure per factory in the latest evaluation before changing allowances.
Compare the same pre-edit state/action at 5k/10k/25k/50k work, preserving default
repair and game rules. Independently validate found witnesses. Budget exhaustion
and local no-path results do not establish global infeasibility.

## Completion criteria

- [x] Census committed training/evaluation outcomes with explicit trace coverage.
- [x] Reproduce a predeclared sample of budget failures; measure rescue and cost.
- [x] Trace representative wasted work to concrete path/port/retry decisions.
- [x] Separate confirmed implementation behavior from causal hypotheses.
- [x] Record findings, reproduction tools and focused follow-up options.

## Activity

- 2026-09-10: Recorded 15/16 move reproduction and valid 10k witness committed as
  `b99b86a`; baseline 5k was consumed before the existing useful port retry.
- 2026-09-10: Investigation authorized; no live training changes are planned.

- 2026-09-10: Frozen 444-batch census and all evaluation detail counted; retained
  CLI reproduced the census exactly.
- 2026-09-10: Replayed 17 preselected cases at four budgets twice; all 68
  non-time outcomes matched and every candidate passed independent admission.
- 2026-09-10: Isolated path instrumentation and independent frontier replica
  matched ten native calls; the failing query has a disconnected relaxed goal
  component. Production routing and ongoing training remain unchanged.
