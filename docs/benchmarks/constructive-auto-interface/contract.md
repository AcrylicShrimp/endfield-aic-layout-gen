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
2. Allocate compatible commodity flow monotonically in the two bank orders. Split at both
   endpoint capacity-bucket boundaries. Each instance/item/direction bucket occupies one port;
   fragments sharing a bucket share its exact aggregate rate. Preserve all recipe margins.
3. Try bounded deterministic bank insertions, reallocating flow each time. Order source buckets
   by their first target and target buckets by their first source. Stable same-item order must
   survive grouping. Minimize capacity-port pressure, residual inversions and fragment count
   at this interface-order stage, not the production layout objective. Report the search budget.
4. Pack catalog facility bodies into two banks. Construct disjoint ordered approaches, fixed
   splitter combs, the preplanned lane permutation, fixed converger combs and ordered exits.
   Tree roots sum at most one lane of a single item. The same five-column, pitch-three template
   applies to every arity; check every active arm, rotation and capacity against the catalog.
   Direct single-leaf zero-inversion connections bypass tree/permutation envelopes.
5. Lower bucket terminals and fragment paths into one network per compatible commodity. Sum
   rates on shared segments; connect external recipe requirements on as many capacity lanes
   as needed, with explicit external port-pressure failure. Require independent Rust validation.

Output detail schema v2 includes fragment bindings, port groups with exact rates and membership,
tree reservations/paths, permutation plus absolute origin, stage timings and witness v1.
This deliberately removes the old `generated_spec` and exclusive `source_port`/`target_port`
fields in bindings; use source/target group IDs and `port_groups` instead. Request v1 and witness
v1 are unchanged. The automatic constructor no longer calls the historical fixture lowerer.
Candidates are not accepted until independent
covered-geometry validation AND complete recipe-terminal coverage pass. Bounds are measured output.
Detailed results go to files; stdout has one short completion summary.

## Invariants and failures

Each root bucket has one exclusive facility port, possibly serving several same-item fragments
through a tree. Same-item fragments preserve order; different items may cross only in preplanned
catalog bridges. Exact flow allocation must preserve all supply/demand, and every remaining recipe
terminal must reach the global external boundary. No fallback adds facilities or changes utilization.

Failures contain stable code, severity, stage, message and entity when available. Distinguish
invalid input, unsupported recipe/rotation, imbalance, port pressure, order/cell resource budget,
external binding limits, and independent rejection. All are local experiment failures, never
global infeasibility proofs. `interface-aggregate-port-pressure` now means capacity buckets exceed
facing ports even with sharing; the obsolete pair-count `interface-port-pressure` code is removed.
Resource caps are diagnostic safeguards, not game limits. Keep all generated geometry unaccepted
if validation fails; preserve it for visual diagnosis.

## Completion and verification

Generate four/five-facility reference requests and split-only, merge-only and combined requests
without fixed geometry. Exercise renamed/permuted inputs, another catalog recipe family, scale,
and unsupported/imbalanced/aggregate-port-limited controls. Validate complete candidates with the existing
optimized witness executable, render the actual witnesses and inspect the images. Record footprint
and timing without claiming the common constructor matches manually optimized fixtures.
