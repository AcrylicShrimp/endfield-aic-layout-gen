# Automatic two-bank interface experiment

## Scope and input

Isolated JavaScript research, not a production constructor or Rust cutover. Input schema v1:
`{schema_version:1, producers:[{id,recipe,utilization}], consumers:[{id,recipe,utilization}]}`.
Utilizations are positive exact rational DTOs at most one. IDs are unique across banks.
No positions, rotations, port IDs, pair flows, or waypoints are accepted in the request.
Game data is loaded from the five runtime catalogs on every CLI invocation.

The first slice supports single-output belt producers and belt-only recipes with west-facing
inputs and east-facing outputs under a catalog rotation. Every consumer ingredient must be
balanced exactly by the producer bank. Producer ingredients and consumer products are external.
This is a local two-layer boundary, not a whole production graph, cycle resolver or pipe planner.

## Stages and outputs

1. Check recipe/rate/transport scope and exact per-item balance.
2. Allocate compatible commodity flow monotonically in the two bank orders; split pair rates
   at transport capacity. Preserve all instance utilizations and terminal margins.
3. Try bounded deterministic bank insertions, reallocating flow each time. Assign source ports
   by target order and target ports by source order. Minimize residual lane inversions at this
   interface-order stage, not the production layout objective. Report the search budget.
4. Pack catalog facility bodies into two banks. Construct disjoint monotone approaches and an
   explicitly reserved lane-permutation module. No free-path search or post-hoc crossing repair.
5. Lower generated bindings using the existing fixture lowering helper; connect external recipe
   requirements. Attach the preplanned components, then require the independent Rust validator.

Output detail includes the generated binding specification, allocations, bank/port orders,
planned permutation, stage timings and witness v1. Candidates are not accepted until independent
covered-geometry validation AND complete recipe-terminal coverage pass. Bounds are measured output.
Detailed results go to files; stdout has one short completion summary.

## Invariants and failures

Each physical lane has one exclusive source and consumer port. Same-item lanes preserve order;
different items may cross only in preplanned catalog bridges. Exact flow allocation must preserve
all supply/demand, and every remaining recipe terminal must reach the global external boundary.
The first lowering helper only supports external residuals fitting one lane per ingredient.
No fallback silently adds facilities or changes utilization.

Failures contain stable code, severity, stage, message and entity when available. Distinguish
invalid input, unsupported recipe/rotation, imbalance, port pressure, order/cell resource budget,
external binding limits, and independent rejection. All are local experiment failures, never
global infeasibility proofs. Port pressure can require explicit fanout/fanin in a follow-up slice.
Resource caps are diagnostic safeguards, not game limits. Keep all generated geometry unaccepted
if validation fails; preserve it for visual diagnosis.

## Completion and verification

Generate the four-facility unavoidable-crossing and five-facility removable-crossing requests
without fixed geometry. Exercise renamed/permuted inputs, another catalog recipe family, scale,
and unsupported/imbalanced/port-limited controls. Validate complete candidates with the existing
optimized witness executable, render the actual witnesses and inspect the images. Record footprint
and timing without claiming the common constructor matches manually optimized fixtures.
