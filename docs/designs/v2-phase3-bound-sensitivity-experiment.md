# V2 Phase-3 Bound-Sensitivity Experiment

## Status

Accepted diagnostic requested after the cumulative SCC growth experiment. This experiment changes
only the caller-supplied hard width and height ceilings. It does not change solver semantics or
production behavior.

## Question

Did Heavy Xiranite cumulative SCC phase 3 fail to produce an incumbent within 5,000 ms primarily
because the 12 by 12 request ceiling is too tight, or does the exact search remain above the same
first-incumbent budget when modestly more space is legal?

## Controlled Matrix

- workload: Heavy Xiranite minimum rate;
- cumulative target: phase 3;
- formulation: `joint-shared-transport-layer-external-connectors-v2`;
- request ceilings: 12 by 12, 13 by 13, and 14 by 14;
- search budget: 5,000 ms independently for every cumulative phase;
- release build;
- one fresh process per target bound;
- placement-only non-binding hints between cumulative phases;
- JSON, self-contained HTML, and `/usr/bin/time -l` output per bound.

The 12 by 12 result from the immediately preceding experiment is the baseline. The 13 by 13 and 14
by 14 cases are new isolated runs. Bounds are experiment inputs only, not defaults or game limits.

## Required Evidence

For the target phase record model construction, first-incumbent and search times, objective and used
bounds, validation, variable/domain/constraint totals and families, routing state, coupling, process
elapsed time, and peak RSS. Compare model growth against whether additional legal space improves or
worsens first-incumbent discovery.

## Interpretation

- A validated incumbent at a larger bound shows that 12 by 12 tightness contributes to the observed
  cliff, but does not prove phase 3 infeasible at 12 by 12.
- No incumbent at a larger bound shows only that added space did not remove the five-second search
  cliff; it does not prove infeasibility.
- If all cases are `unknown`, use the structural deltas to decide whether the next experiment should
  target coordinate channeling or bound-dependent connector state.

## Decision Boundary

Stop after the three-bound comparison and report the evidence. Do not introduce a reformulation,
domain reduction, heuristic, or longer-budget conclusion in the same checkpoint.
