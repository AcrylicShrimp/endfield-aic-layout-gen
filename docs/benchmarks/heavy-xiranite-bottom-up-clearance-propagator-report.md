# Heavy Xiranite Bottom-Up Endpoint Clearance Propagator Report

## Result

The exact point-versus-variable-rectangle propagator removes the first Rung 1B representation
cliff without changing the facility-port semantics. The previous dense reified formulation found no
Phase 23 witness in four five-second runs. The propagated formulation finds a validated Phase 23
witness in every run in 960-990 ms. The first repeated five-second miss moves from Phase 23 to
Phase 30.

This is a same-semantics reformulation. Placement, directional rotation, port selection, local
connection geometry, and every endpoint-clearance decision remain solver decisions. No placement,
port, coordinate, or routing heuristic was introduced.

## Contract

Both compared Rung 1B variants require every selected facility-port connection cell to lie outside
every non-owner facility footprint.

The baseline `facility-ports` formulation expands each
`(endpoint, non-owner facility, facility geometry class)` into four fully reified inequalities and
one guarded disjunction:

```text
connection_x < facility_x
or connection_x >= facility_x + width
or connection_y < facility_y
or connection_y >= facility_y + height
```

The experimental `facility-ports-propagated` formulation posts one exact relation per
`(endpoint, non-owner facility)`. The propagator retains every target geometry class internally. It
rejects a class when none of the four separations has bound support and directly tightens both
coordinates when a selected class has one remaining separation. Complete assignments are accepted
exactly when the baseline disjunction accepts them.

## Phase 23/24 Comparison

All search runs used the same release binary, Heavy Xiranite minimum-rate workload, loose `50 x 50`
request ceiling, and five-second first-witness budget. The baseline values are from the preceding
Rung 1A/1B report. The propagated timing ranges are four fresh counterbalanced processes.

| Phase | Facilities | Endpoints | Formulation | Variables | Constraints | Outcome | Search | Decisions | Conflicts | Solver propagations |
|---:|---:|---:|---|---:|---:|---|---:|---:|---:|---:|
| 23 | 24 | 71 | Reified baseline | 10,444 | 13,574 | unknown 4/4 | 5,000 ms | about 160k | about 10k | about 23M |
| 23 | 24 | 71 | Point-rectangle propagator | 2,840 | 5,702 | feasible 4/4 | 960-990 ms | 67,165 | 3,028 | 7,849,944 |
| 24 | 25 | 75 | Reified baseline | 11,348 | 14,721 | feasible 4/4 | 370-403 ms | 15,111 | 907 | 1,438,691 |
| 24 | 25 | 75 | Point-rectangle propagator | 3,012 | 6,101 | feasible 4/4 | 103-105 ms | 7,854 | 150 | 1,083,587 |

At Phase 23, the reformulation removes 7,604 variables (72.8%) and 7,872 recorded constraints
(58.0%). The 7,604 removed variables are exactly the old directional clearance Booleans. The 1,633
new recorded constraints are the semantic endpoint/facility relations. At Phase 24, variables fall
73.5% and constraints fall 58.6%.

The Phase 23 first-witness improvement is greater than fivefold because the baseline is censored at
five seconds. Phase 24 improves by approximately 3.6 times and cuts conflicts by 83.5%.

The 24/25 timing inversion remains: Phase 23 takes about nine times longer than Phase 24 even after
the representation change. This is not evidence that 24 facilities are intrinsically harder than
25. Without endpoint clearance, Phase 24 is normally slower. The remaining inversion is therefore
an interaction between the exact constraint graph and Pumpkin's deterministic search order. The
new formulation makes both cases operationally feasible but does not remove that deeper search
sensitivity.

## Propagator Activity

The custom counters deliberately distinguish a detected unique separation from a bound update.
`forced_separation_detections` counts every visit that observes one remaining separation; it is not
a count of unique semantic events.

| Phase | Relations | Executions | Notifications | Orientation checks | Rejected classes | Unique-separation detections | Bound updates | Custom conflicts | Maximum reason predicates |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 23 | 1,633 | 5,513,393 | 6,929,161 | 5,662,816 | 366 | 106,263 | 7,747 | 349 | 8 |
| 24 | 1,800 | 786,862 | 929,020 | 795,207 | 5 | 7,621 | 434 | 5 | 8 |

The model-state reduction is successful, but the runtime profile reveals the next local target.
At Phase 23, fewer than 0.15% of executions reject a class or post a bound update. Millions of
unconditional wakeups now dominate the custom relation's own hot path.

## Cumulative Growth And Next Cliff

| Phase | Facilities | Endpoints | Variables | Constraints | Outcome | Search | Decisions | Conflicts | Solver propagations |
|---:|---:|---:|---:|---:|---|---:|---:|---:|---:|
| 25 | 26 | 79 | 3,188 | 6,513 | feasible | 228 ms | 17,464 | 490 | 2,074,580 |
| 26 | 27 | 83 | 3,368 | 6,938 | feasible | 155 ms | 9,309 | 378 | 1,365,546 |
| 27 | 28 | 87 | 3,552 | 7,376 | feasible | 194 ms | 14,459 | 305 | 1,987,247 |
| 28 | 29 | 91 | 3,919 | 8,091 | feasible | 567 ms | 40,011 | 1,220 | 5,112,753 |
| 29 | 30 | 95 | 4,302 | 8,834 | feasible | 247 ms | 13,332 | 344 | 2,178,513 |
| 30 | 33 | 105 | 4,982 | 10,396 | unknown 4/4 | 5,000 ms | about 212k-218k | about 12.1k-12.4k | about 44.3M-45.5M |
| 31 | 36 | 115 | 5,698 | 12,063 | unknown | 5,000 ms | 241,390 | 10,400 | 45,031,336 |
| 32 | 39 | 125 | 6,450 | 13,835 | unknown | 5,000 ms | 209,140 | 10,380 | 45,870,287 |

Phase 30 is the first repeated five-second miss. A predeclared 30-second confirmation finds a
validated witness at 23.711 seconds after 926,491 decisions, 45,277 conflicts, and 198,537,975 solver
propagations. The result is difficult first-witness search, not infeasibility.

This moves the first observed complete-clearance cliff seven cumulative phases forward: from
24 facilities and 71 endpoints at Phase 23 to 33 facilities and 105 endpoints at Phase 30.

## Exactness And Regression Evidence

The propagator has no reversible mutable semantic state. It subscribes to coordinate bound changes
and every orientation-selector domain change. Bound subscriptions are sufficient for the implemented
bounds reasoning; complete assignments reduce every coordinate to one bound and therefore cannot
escape validation.

Committed tests cover:

- every complete point/rectangle assignment in a controlled domain against the direct oracle;
- exhaustive two-orientation equivalence against the old fully reified formulation;
- explicit half-open boundary cells on all four sides;
- both endpoint and facility bound deductions for left, right, above, and below;
- unsupported-class rejection and supported-class retention;
- an unselected geometry class containing the point without restricting it;
- custom-reason conflict learning and backtracking to a supported sibling;
- the existing integrated feasible and blocked-connection fixtures under both formulations.

All seven focused propagator tests also pass with `pumpkin-debug-checks` enabled.

## Independent Review Resolution

Three independent reviews examined the slice.

- The proof and bug review initially blocked on incomplete regression coverage. After the exhaustive
  baseline differential, all eight bound deductions, guard, boundary, and backtracking tests were
  added, the final verdict was **PASS**. It found no unsound reason or inequality.
- The formulation review confirmed one endpoint/non-owner-facility relation as the correct semantic
  granularity for this first reformulation. It requested a clearer counter name; the misleading
  `forced_separations` field was renamed to `forced_separation_detections`. Its remaining artifact and
  report gates were then resolved.
- The optimization review confirmed the model and search improvement. It identified the broad
  wakeup rate rather than clearance semantics as the immediate local performance target.

No reviewer claim caused a game-rule change or a heuristic restriction.

## Improvements Preserved So Far

The broader exact-search research has preserved the following measured improvements. Not every
item is present in this independent low rung; the list records the path that motivated the current
formulation ladder.

1. Per-network dense routing grids were replaced by shared belt and pipe layer state.
2. Flattened placement-by-port candidates were separated into placement, rotation, port, and local
   connection state with exact bidirectional support.
3. Facility and transport physical occupancy were channelled through exact shared occupancy state.
4. External connectors were integrated into the same transport semantics instead of a separate
   hand-shaped connector family.
5. Exact dimension portfolios were introduced as completeness-preserving outer partitions rather
   than heuristic canvas choices.
6. Connectivity, endpoint support, local continuation, and guarded item-intersection rules gained
   dedicated exact propagator experiments with explicit counters and proof reasons.
7. The bottom-up ladder now identifies a semantic cliff by adding one proof obligation at a time,
   then grows the improved rung before diagnosing the next blocker.
8. This slice removes the dense directional-Boolean expansion for facility endpoint clearance and
   advances the first complete-clearance miss from Phase 23 to Phase 30.

## Recommended Next Action

Preserve this propagated Rung 1B variant as the exact comparison baseline. The next slice should
optimize only its measured hot path:

1. replace the allocated `Vec` of possible directions with a four-bit mask and read relation bounds
   once per execution;
2. compare `High` and `Medium` propagator priority as an exact scheduling ablation;
3. if the no-op execution rate remains dominant, add backtracking-safe event-selective dirty
   scheduling before considering a larger grouped propagator;
4. rerun Phase 23, Phase 24, and the Phase 30 cliff after each isolated change.

The next search target is not yet routing. This independent rung still contains no belt or pipe
grid. The immediate measured blocker is repeated facility-port clearance propagation and the search
interaction around 33 facilities.

## Artifacts

- `heavy-xiranite-bottom-up-clearance-propagator/propagated-phase23/`
- `heavy-xiranite-bottom-up-clearance-propagator/propagated-phase24/`
- `heavy-xiranite-bottom-up-clearance-propagator/repeat/`
- `heavy-xiranite-bottom-up-clearance-propagator/growth/`
- `heavy-xiranite-bottom-up-clearance-propagator/confirmation/phase30-30s/`
