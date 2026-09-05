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
4. Construct two geometry families with the same allocations. The leaf family uses disjoint
   ordered approaches, three-way splitter trees, a preplanned leaf permutation, converger trees
   and ordered exits. The trunk family retains shared descending source trunks, taps them at
   target leaf rows and preplans crossings only at different-item straight intersections.
   Trunk taps can use two active arms when this yields a smaller overall layout. Tree roots
   sum at most one lane of a single item; all active arms, rotations and capacities come from
   runtime catalogs. Tree width follows its recursive depth, not a fixed five-column comb.
   Direct single-leaf zero-inversion connections bypass leaf tree/permutation envelopes.
5. Lower bucket terminals and fragment paths into one network per compatible commodity. Sum
   rates on shared segments; connect external recipe requirements on as many capacity lanes
   as needed, with explicit external port-pressure failure. Require independent Rust validation.
6. Optionally compact each candidate by contracting whole grid strips containing no facility
   body, component, terminal, turn, branch, merge, transverse segment or rate change. Transform
   all remaining geometry and regenerate catalog port coordinates. This preserves directed
   topology and rates, but remains an unaccepted candidate until independent validation.
7. Select only complete, independently validated candidates, lexicographically by actual used
   area, unique physical transport tiles and route turns. Component count is diagnostic only.
   Preserve rejected candidates and family construction failures. No global area optimum is claimed.

Output detail schema v2 includes fragment bindings, port groups with exact rates and membership,
tree reservations/paths, permutation plus absolute origin, stage timings and witness v1.
This deliberately removes the old `generated_spec` and exclusive `source_port`/`target_port`
fields in bindings; use source/target group IDs and `port_groups` instead. Request v1 and witness
v1 are unchanged. The automatic constructor no longer calls the historical fixture lowerer.
Candidates are not accepted until independent
covered-geometry validation AND complete recipe-terminal coverage pass. Bounds are measured output.
Detailed results go to files; stdout has one short completion summary.

CLI `--embedding portfolio` is the default and compares raw and compacted leaf/trunk families.
Explicit `--embedding leaf` or `trunk` runs that raw family only for controlled experiments.
The helper API defaults to `leaf`; callers requesting the portfolio receive an unvalidated
candidate collection, never a selected layout. Report schema v2 adds per-candidate scores,
validation status, construction failures and selected embedding. The request and witness schemas
are unchanged. Compacted details replace pre-contraction tree/permutation coordinates with a
clearly named `pre_compaction_plan`; witness, port groups and catalog ports describe the new state.
In compacted port groups, `port_y` remains an actual transformed port coordinate. Reservation-only
`row` and `root_row` are deliberately removed from these groups and retained in
`pre_compaction_plan.port_groups`; bypassed tree reservations need not lie inside used bounds.
Raw `detail.accepted` stays false: the authoritative acceptance result is in `report.json` and
`validation.json`, not in the generator's pre-validation detail DTO.

Three-way trees support 1..384 leaves. At pitch one a three-leaf module occupies 3x3 including
the root/leaf connection cells. `ceil((L-1)/2)` is a junction-count lower bound for a rooted
tree with at most three children, not a minimum-area bound or a candidate-selection objective.
Integrated leaf lanes still start at pitch three for compatibility with the crossing template;
strip compaction removes spacing only when the complete geometry allows it.

Construction timing includes candidate generation and compaction, excludes independent validation
and output. Per-candidate reports separate input JSON I/O, validator process wall time, and render
plus HTML I/O. Validator-internal time remains in `validation.json`; process wall times overlap
because independent validators run concurrently. The audit's warm construction statistics exclude
validation and file I/O. `elapsed_before_report_ms` includes selected output, excludes catalog load
and the final report write. No timing includes compilation.

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
No generated candidate yields `interface-generation-failed`, not an independent-validation rejection.
Candidate processing errors are isolated so they cannot discard another independently valid candidate.

## Completion and verification

Generate four/five-facility reference requests and split-only, merge-only and combined requests
without fixed geometry. Exercise renamed/permuted inputs, another catalog recipe family, scale,
and unsupported/imbalanced/aggregate-port-limited controls. Validate complete candidates with the existing
optimized witness executable, render the actual witnesses and inspect the images. Record footprint
and timing without claiming the common constructor matches manually optimized fixtures.
