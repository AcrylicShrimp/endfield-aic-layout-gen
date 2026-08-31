# Heavy Xiranite Phase 3 External Boundary-Key Legal-Support A/B

## Result

Representing each external boundary selector with its exact sparse legal-support domain removes a
real root-propagation gap, but it does not cross the Phase 3 five-second first-feasible cliff.

- bounded control: `Unknown` after 5,005 ms;
- sparse legal-support formulation: `Unknown` after 5,007 ms;
- validated witnesses: 0;
- infeasibility proofs: 0;
- invalid witnesses or evidence conflicts: 0;
- interpretation blocked: false.

The ten external selectors each changed from a declared 1,024-value interval to the same 544
values already required by their positive unary table. At root, the bounded formulation retained
534 values per selector: 54 values that also survived every boundary constraint and 480 values
with no legal table support. The sparse formulation retained exactly the same 54 legal values and
none of the 480 impossible values.

Across all ten selectors this removes 4,800 impossible root values while removing zero legal root
values. Both authoritative solves nevertheless time out without an incumbent. The gap was real,
but it is not the current dominant blocker.

## Controlled Contract

The release experiment reproduces residual facility-port tuple case 6 from the accepted Phase 3
portfolio:

- exact used dimensions `16 x 16`;
- all four facility placements and rotations fixed;
- all fifteen facility terminal ports fixed;
- ten external boundary terminals free;
- all belt/pipe route cells, arcs, item states, flows, topology, capacity, collision, and logistics
  components free;
- feasibility-only search with a fresh 5,000 ms budget per solve;
- objective auxiliary variables retained but not optimized.

Four models run sequentially in the recorded order:

1. bounded authoritative;
2. sparse authoritative;
3. bounded root observation;
4. sparse root observation.

The observer is absent from both authoritative models. Build certificates enumerate domains, so
their construction times are instrumented and are not used as a build-performance comparison.

## Exact-Equivalence Certificate

All fail-closed checks passed:

| Assertion | Result |
| --- | --- |
| One common prepared logical input and fixed-port vector cloned into all four builds | passed |
| Same facility, requirement, network, and terminal IDs | passed |
| A declaration equals independent full range `0 .. 4 * 16 * 16 - 1` | passed |
| `B domain = A table = B table = A routing options = B routing options` | passed |
| Ten unique external-terminal certificates | passed |
| Non-domain model structure equal | passed |
| Root terminal/network identity equal | passed |
| Root observation covers every external terminal | passed |
| Every B root value belongs to legal support | passed |
| Fixed placement, rotation, and fifteen-port contract | passed |
| Witness/proof consistency | passed |

The authoritative A and B models both contain 64,471 variables, 163,836 constraints, 626,038
constraint incidences, and 244,622 placement-routing incidences. Only the ten boundary-selector
integer declarations differ.

## Static and Root Domain Delta

Every external selector has the same counts:

| State | A: bounded | B: sparse |
| --- | ---: | ---: |
| Declared key values | 1,024 | 544 |
| Positive-table legal values | 544 | 544 |
| Root values | 534 | 54 |
| Root values absent from legal support | 480 | 0 |
| Legal root values removed only by B | 0 | 0 |

Aggregate over ten selectors:

| Metric | Value |
| --- | ---: |
| A/B observed external selectors | 10 / 10 |
| A root values absent from legal support | 4,800 |
| B root values absent from legal support | 0 |
| Legal root values removed only by B | 0 |
| Whole-model log2 declared-domain volume, A | 72,158.871 |
| Whole-model log2 declared-domain volume, B | 72,149.746 |
| Log2 volume reduction | 9.125 |

The sparse declaration therefore repairs exactly the observed unsupported-value gap. It does not
perform a heuristic choice or remove a legal selector value.

## Search Measurements

| Metric | A: bounded | B: sparse | B minus A |
| --- | ---: | ---: | ---: |
| Outcome | Unknown | Unknown | unresolved |
| Search ms | 5,005 | 5,007 | +2 |
| First incumbent | none | none | none |
| Branch decisions | 54,416 | 52,938 | -1,478 |
| Backtracks | 4,834 | 5,086 | +252 |
| Conflicts | 4,834 | 5,085 | +251 |
| Learned clauses | 4,834 | 5,085 | +251 |
| Solver propagations | 5,554,001 | 5,526,261 | -27,740 |

Both runs consume the same cutoff and neither resolves. Fewer B decisions or propagations cannot be
called faster progress: B also performs more backtracks and conflicts, and different propagation
cost can change how much search fits inside five seconds. The measured performance classification
is therefore `both-unknown`, with no winner.

## Remaining Root State

The sparse formulation leaves every facility placement, rotation, and facility port singleton, so
the residual cliff is no longer a facility-state selection problem. The largest unresolved state
families before the first branch are:

| Variable family | Unresolved |
| --- | ---: |
| Arm item | 9,312 |
| Objective auxiliary | 3,859 |
| Branch component | 2,432 |
| Route arm | 2,248 |
| Bridge rotation | 1,064 |
| Route arc | 1,016 |
| Flow | 1,016 |
| Boundary terminal | 550 |
| Route cell | 296 |
| Transport occupancy | 296 |
| Bridge | 266 |
| Terminal presence | 216 |

The two physical layers each retain roughly the same broad grid state:

| Layer | Unresolved route cells | Unresolved route arcs | Unresolved flows |
| --- | ---: | ---: | ---: |
| Belt | 147 | 508 | 508 |
| Pipe | 149 | 508 | 508 |

All ten external selectors still have 54 legal boundary keys. The largest endpoint fan-out is the
carbon-enriched belt network: four external supplies produce 216 possible supply options for four
fixed facility demands. The clean-water pipe network has 108 external supply options for two fixed
demands. Every possible demand is graph-reachable at root, so the current possible-graph
connectivity propagator has no contradiction to expose before additional endpoint or route choices
are made.

## What This Rules Out

The Phase 3 cliff is not caused merely by Pumpkin retaining values absent from the positive unary
boundary table. Removing all 4,800 such values exactly does not produce a witness or proof within
five seconds and barely changes the amount of search completed.

This does not rule out legal external-terminal selection as a blocker. Each terminal still chooses
among 54 legal boundary keys, and those choices remain coupled to 296 unresolved physical route
cells, 1,016 route arcs, item state, and flow. It only rules out the unsupported half of the old
integer interval as the dominant cause.

## Size Caveat and Next Exact Experiments

`16 x 16` remains a controlled diagnostic size, not an optimum or project limit. It was retained
because the preceding fixed three-facility witness has height 16. The same fixed facilities fit
from width 13 upward, while the eager physical layer materializes every cell in the selected exact
dimensions.

The next experiment will first isolate one remaining endpoint decision more narrowly than changing
the whole grid. Partition the final-product network's single external demand selector into its four
complete side sets. The 54 live keys divide into north/east/south/west counts `11 / 16 / 11 / 16`.
The four children are disjoint and cover every live key. Placement, ports, the boundary cell within
the selected side, and every route/item/flow decision remain free. This tests whether one coarse
external attachment decision unlocks route propagation without conflating it with a grid resize.

If all four side children remain `Unknown`, run the same fully fixed sparse case at exact sizes
`13 x 16`, `14 x 16`, `15 x 16`, and `16 x 16`, each with an independent five-second budget. The
fixed facility geometry proves width 13 and height 16 as lower bounds, so these four width cases
exhaust the legal fixed-height dimensions under the current `16 x 16` ceiling. That experiment
directly tests whether excess physical grid state is the next cliff.

If one side resolves, refine only its unresolved side children by exact boundary-cell equality. If
both the side and width portfolios remain unresolved, first map the brancher's identical
unregistered Boolean decision to its semantic predicate, then use a complete false/true partition;
never partition raw runtime domain ID `117486` without provenance.

## Improvements Preserved in the Current Exact Baseline

The current experiment retains the cumulative exact improvements established by earlier slices:

1. belt and pipe are shared physical layers rather than one dense grid per logical line;
2. placement and port choice are factored instead of flattened into one placement-port candidate;
3. facility occupancy is a canonical physical state coupled bidirectionally to both transport
   layers;
4. external terminals participate in the same shared commodity-network routing model;
5. exact used-dimension cases can be swept in parallel and share proven area bounds without
   changing feasibility;
6. possible-graph connectivity rejects selected demands that have no possible material path;
7. event-driven unique-support and local-continuation propagation derives mandatory route state;
8. guarded positive-item intersection propagates material incompatibility through active route and
   bridge relations;
9. placement, rotation, facility ports, endpoint support, and residual port tuples have exact
   bidirectional or exhaustive diagnostic formulations;
10. this slice declares external boundary keys with their exact legal sparse support and removes
    4,800 impossible root values.

These improvements preserve the joint placement-routing experiment. None preselects a layout,
port, corridor, or route heuristically.

## Independent Review

Before the release run, independent proof-soundness, experiment-validity, and next-strategy reviews
all passed after two accepted audit fixes:

- validate A against the independently calculated full bounded range;
- publish aggregate root-support deltas in JSON and HTML.

Additional focused tests cover duplicate, missing, and extra certificates; a contiguous but wrong A
range; missing B support; table/option mismatch; network and sparse-bound mismatch; asymmetric
root-infeasible observations; missing observed terminals; and symmetric cutoff classification.

A reported documentation-duplication concern was checked with a direct search and could not be
reproduced, so no unsupported edit was made.

All three post-run reviews passed. They independently reproduced the ten-selector arithmetic,
model and root identity checks, fixed contracts, `Unknown` outcomes, and mixed search-counter
deltas. They agreed that no runtime winner is supported. Proposed follow-ups differed in ordering:
one reviewer preferred immediately mapping the first unregistered Boolean, while the strategy
review preferred the smaller semantic four-side endpoint partition, then width sensitivity, then
raw-decision provenance. The latter ordering was selected because it changes one understood game
decision at a time and keeps the width hypothesis as the next complete matrix if that split does
not localize the cliff.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
```

All 34 main CLI tests, the dedicated prior-terminal CLI test, and 276 data-library tests pass.
The release run completed without diagnostics, invalid witnesses, or blocked interpretation.

## Artifact

The release artifact is `/tmp/aic-phase3-boundary-key-ab.0024tf`. It contains:

- `summary.json` and self-contained `summary.html`;
- stdout JSON;
- bounded authoritative and observation HTML;
- sparse authoritative and observation HTML;
- `sparse-root-digest.json` used for the remaining-state table.
