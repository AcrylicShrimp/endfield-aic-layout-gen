# Failure-directed vacant endpoint assignment

## Outcome

The complete incumbent improves from 46x54 / 2,484 area / 404 transport cells / 38
turns to **45x54 / 2,430 area / 411 cells / 37 turns**. The 2.17% area improvement
outweighs the increased wiring under the unchanged lexicographic objective. All 59
facilities, seven components, original demands and external margins validate.
Large middle/right voids remain; satisfactory final area efficiency is not claimed.

Both retained-port and vacant-port controls reach area 2,430. The common first move
translates F04 and F01 upward one cell; subsequent route restoration and axis compaction
remove one column. This area gain cannot be attributed to the new port repair. The
vacant-port portfolio wins the same-area comparison on transport cells, 411 versus 415.

## Shared proposal engine

`vacantPortCandidates` enumerates retained/unclaimed compatible runtime source and
target ports for explicitly identified failed chains. Both endpoints can change in
one assignment. Body screening uses translated facility geometry; projected endpoint
distance ranks candidates. Terminal identities, commodity assignments and rates stay
fixed. Existing ownership, route and full-factory gates remain authoritative.

The experiment runner's `--port-policy vacant` retries at most eight combinations
after a failed route, without recursive retries. Every attempt records the parent
failure, chain, port assignments and score. The priority probe can inspect up to 64
combinations separately. All facilities and port choices derive from input artifacts
and runtime catalogs, not label-specific rules. The contract lists the finite budgets
and the retained limitations around junctions and pre-assignment movement screening.

## Controlled experiments

All start from zero-route-results.json, with three fair-coalescence rounds, 256
positions per round, near/far route orders, incident bridge release, independent
layers and at most eight new crossings. Four unimproved candidates may be audited.

| Policy | Ordinary moves | Additional port retries | Best area / cells / turns | Elapsed seconds |
| --- | ---: | ---: | --- | ---: |
| Retained | 1,536 | 0 | 2,430 / 415 / 34 | 23.73 |
| Vacant | 1,536 | 3,627 | **2,430 / 411 / 37** | 35.04 |

Of the 3,627 port retries, 3,602 reject, 23 are unimproved, and two become temporary
best candidates. These counts show a broad low-yield neighborhood, not an efficient
general assignment solver. Time is one concurrent optimization-batch observation,
not initial generation latency or a stable speedup benchmark. Generation takes 28.17s,
compaction 6.15s and validation/output 0.10s in the vacant run.

### Confirmed simultaneous-endpoint repair

Stage 2344 moves F19 upward two cells, with F26 pinned. The failed F27->F19 interface
is restored by selecting runtime output-belt-2 instead of output-belt-0 on F27 and
input-belt-5 instead of input-belt-1 on F19. Labels/port indices here describe the
recorded experiment only; they are not candidate-generation rules.

Replaying the same parent and route order with retained ports, source-only change,
and target-only change rejects with no-path in all three cases. Changing both produces
a raw candidate 2,430 / 416 / 39, then independently validated compaction reaches
2,430 / 413 / 39. This proves the usefulness of a simultaneous assignment in this
bounded replay, not that no alternative single-endpoint route exists.

The final selected stage 4874 descends from that stage-2344 factory and moves F16/F10
upward while pinning F05. Another selected port repair in round 2 is a sibling, not
an ancestor of the final witness. Do not combine all selected-stage motions into a
fictional cumulative history.

## Remaining obstruction

The old upper-bank failure still rejects all seven vacant source/target combinations
in both orders (24 total replay trials including other order/rip-up controls). Moving
only the failing endpoints does not open its isolated approach corridor. A water-case
probe has no vacant alternatives; its unvalidated candidates come from existing route
order controls and are not evidence for vacant-port repair.

Next identify the neighboring occupied terminals that disconnect a relaxed approach
region, then include their owners/interfaces in a bounded coordinated placement and
port-assignment neighborhood. This is more informative than treating every no-path as
a need for more A* work. Pumpkin remains an optional bounded oracle; no production
cutover or global-infeasibility claim is justified by these failures.

## Verification and visual review

219 Node tests, cargo check --workspace and 524 Rust tests pass; two existing ignores.
Eight original-graph and sixteen crossing controls pass for the selected factory.
Selected and retained-control native full renders were personally inspected, as was
the selected F26/F27/F19/F18 close-up. Both pages pass six viewport/theme checks.
The F19 neighborhood is more compact, while the central/right unoccupied regions and
the lower seed return remain visible. No renderer-only geometry is promoted.

Engine source hashes in the final report match the verified source. Reports and large
trial logs stay in target JSON files. No Rust changes or dependencies. Compared with
AGENTS.md and the constructive-planner design, this is an isolated transactional
improvement experiment: production growth and validator semantics remain unchanged,
and only complete validated area-first improvements are retained. Broader recipe
testing, cleanup and web-product milestones remain open.
