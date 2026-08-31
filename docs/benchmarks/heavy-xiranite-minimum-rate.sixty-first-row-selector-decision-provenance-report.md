# Phase 3 Row-Selector Decision Provenance Report

## Question

The standard positive table strengthened facility endpoint propagation but made 3.62 times as many
branch decisions and did not find a Phase 3 incumbent. Does Pumpkin actually spend those decisions
on the table's 29,568 hidden row selectors?

This diagnostic retains every row-selector domain ID while posting the same non-reified table
clauses as Pumpkin 0.5. It then observes predicates returned by the unchanged default brancher. It
does not remove, reorder, prefer, phase, or fix any decision.

## Controlled workload

- Minimum-rate Heavy Xiranite, cumulative Phase 3 with four facilities.
- Exact used dimensions `16 x 16`.
- Feasibility-only search with no prior hint.
- Three fresh release processes, each with the standard 5,000 ms search budget.
- Current exact stack: possible-graph connectivity, watched demand support, local continuation, and
  grouped guarded positive-item intersection propagation.
- Placement, rotation, port selection, boundary terminals, belt/pipe routing, flow, capacity,
  occupancy, crossing, and topology remain solver decisions.

## Result

The row-selector branching hypothesis is confirmed for this positive-table formulation.

| Metric | Run 1 | Run 2 | Run 3 | Median |
|---|---:|---:|---:|---:|
| Outcome | unknown | unknown | unknown | unknown |
| Construction | 526 ms | 506 ms | 513 ms | 513 ms |
| Search | 5,011 ms | 5,012 ms | 5,011 ms | 5,011 ms |
| Total decisions | 11,152 | 11,156 | 11,155 | 11,155 |
| Row-selector decisions | 8,147 | 8,147 | 8,147 | 8,147 |
| Non-row decisions | 3,005 | 3,009 | 3,008 | 3,008 |
| Row=true decisions | 6 | 6 | 6 | 6 |
| Row=false decisions | 8,141 | 8,141 | 8,141 | 8,141 |
| Unclassified row decisions | 0 | 0 | 0 | 0 |
| Maximum consecutive row decisions | 1,597 | 1,597 | 1,597 | 1,597 |
| Row predicates seen in conflict analysis | 157,858 | 157,858 | 157,858 | 157,858 |
| Backtracks | 122 | 122 | 122 | 122 |
| Conflicts | 121 | 121 | 121 | 121 |

Row selectors account for `8,147 / 11,155 = 73.0%` of median branch-decision events. Of the observed
row-selector events, 8,141 (`99.93%`) choose false polarity, excluding one row on the current branch.
This is not a count of distinct selectors; a selector may be revisited after backtracking.

The longest decision-event sequence with no intervening non-row decision contains 1,597 row-selector
decisions. The counter does not reset on a backtrack or conflict, so this is not a claim that 1,597
selectors were simultaneously active on one search path. Similarly, the 157,858 conflict-analysis
appearances are callback events, not unique predicates, unique selectors, or unique conflicts.

All 29,568 selectors remain unresolved after root propagation in every Phase 3 run. This is not a
claim that the endpoint relation performs no useful propagation after other decisions. It means the
initial model provides no selector-level pruning before search chooses placement, port, geometry,
or routing facts.

## What this proves

The positive table represents one logical endpoint choice by introducing one Boolean per legal
tuple. Pumpkin's generic brancher then sees those Booleans as ordinary solver domains. Instead of
reasoning directly about the three semantic variables:

```text
facility placement + logical port + physical endpoint geometry
```

within the observed five-second prefix, the branch-decision stream is dominated by repeated choices
of the following form:

```text
not row 1, not row 2, not row 3, ...
```

Within that prefix, row selectors account for most decision events. This confirms
selector-branching dilution as a concrete formulation cost and makes a sparse support channel the
leading next exact experiment. It does not establish that row decisions consume 73.0% of CPU time
or that removing them alone will yield a Phase 3 incumbent.

The provenance directly explains the preceding positive-table decision-count increase. It is
consistent with, but does not by itself attribute, the earlier CPU-work increase; hidden clause
processing and the changed search tree remain additional costs. All runs are censored at five
seconds without an incumbent or a complete tree, so these proportions do not predict later search.

## Instrumentation validity

The tracked table poster copies Pumpkin 0.5's non-reified encoding:

1. create one Boolean selector per row;
2. post every selector-to-column-value implication;
3. post every column-value-to-support disjunction; and
4. require at least one selector.

An exhaustive controlled test compares every assignment of a small ternary relation against the
native table and obtains the same solution set. The tracked formulation also solved the known
feasible Heavy Xiranite Phase 0 `7 x 7` case and passed independent witness validation.

The observer wraps the unchanged default brancher and only classifies its returned predicate. The
default brancher already subscribes to conflict-predicate events, so counting those events does not
add a new solver notification class. Compared with the preceding native positive-table median, the
tracked run changes total decisions by +0.4%, maximum RSS by less than +0.1%, and retired instructions by less
than +1%. These small deltas are consistent with measurement overhead and timeout noise; no material
search-policy perturbation was observed.

The integrated layout schema advances from version 20 to 21 because the structured exact-search
statistics add optional row-selector provenance fields. Normal solves emit `null` for those fields.

## Current exact improvements retained

This diagnostic preserves the exact improvements accumulated before the endpoint experiment:

- shared belt and pipe layer state rather than one dense grid per logical line;
- separated placement and port variables instead of a placement-times-port product domain;
- canonical facility occupancy coupled bidirectionally to transport occupancy;
- external inputs and final outputs represented in the same transport network semantics;
- exact dimension partitioning outside the solver without dropping any legal used size;
- possible-path connectivity propagation;
- watched unique demand support and local continuation propagation; and
- grouped guarded positive-item intersection propagation.

The standard positive table itself is not counted as an improvement. Its integrated CPU and memory
cost was worse and it remains research-only.

## Independent review disposition

Three read-only reviewers independently checked this slice.

- Semantic/proof review passed. It verified clause and selector creation order against Pumpkin 0.5,
  checked that observation does not modify the returned decision, and confirmed the root
  measurement occurs after root propagation.
- Experiment review passed the execution and arithmetic but blocked the first report wording. It
  required decision events to be distinguished from CPU time, unique selectors, and single-path
  depth. Those overclaims were removed.
- Strategy review passed the selector-dilution conclusion as a formulation cost and recommended a
  channel-only sparse ternary support propagator as the next exact experiment, with the standard
  table retained only as a propagation oracle.

## Decision

Stop optimizing Pumpkin's standard table. The next exact formulation target is a sparse semantic
endpoint-support propagator over the same complete `(placement, port, geometry)` relation.

The propagator should maintain supports directly and propagate in all three directions without one
branchable Boolean per tuple. It must:

- preserve every legal complete assignment;
- preserve shared placement authority across all terminals of one facility;
- preserve port identity even when ports alias one geometry;
- explain every removal and conflict soundly under learning and backtracking;
- match the positive-table oracle on the controlled propagation cases; and
- beat the nested Element baseline on faithful Phase 3 search without recreating the table's hidden
  memory cost.

Changing branch order to defer selector variables could also address the symptom, but it is a
hand-written search policy and is not part of this exact formulation diagnostic. No branch-order
change or custom propagator is implemented in this slice.

## Artifacts

- `/tmp/aic-row-selector-phase3-final.dCuNkM/run-{1,2,3}`
- `/tmp/aic-row-selector-probe-final.10JrYm`

Every run directory contains structured JSON, summary HTML, automatic layout/failure HTML, stdout,
and macOS `/usr/bin/time -l` process statistics.

- Release binary SHA-256:
  `4f1548d4b551c1c749637cdfd7d11f4683eaa4bf5daf41eb9f458bc37a5bb902`
- Source base before this slice: `1e69a2f`

## Reproduction

```text
target/release/aic-cli research compare-integrated-endpoint-channel \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 3 \
  --used-width 16 \
  --used-height 16 \
  --encoding positive-table \
  --track-row-selectors \
  --case-time-limit-ms 5000 \
  --output-dir <fresh-directory>
```

Run this command three times in isolated release processes and wrap each invocation in
`/usr/bin/time -l` for RSS, retired instructions, and cycle measurements.
