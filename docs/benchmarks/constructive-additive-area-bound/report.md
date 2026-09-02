# Additive Placement Ordering And Shared Area Bound

## Purpose

This experiment reduces repeated A* calls without restricting the constructive placement domain. Existing composite geometry remains immutable. The new source module still retains every legal rigid rotation and translation candidate.

## Method

For each rotation, placement candidates are evaluated in this order:

1. bounding boxes adjacent with at most one empty-cell gap;
2. remaining candidates in ascending facility-and-existing-route bounding-box area;
3. stable coordinate order.

The ordering establishes a compact feasible incumbent early. If any candidate worker finds a score with zero blocked boundary-port options and area `A`, it publishes `A` through a shared atomic upper bound. Every worker computes the union bounding-box area before collision checks or routing. A placement whose union area is greater than `A` cannot improve the lexicographic score because adding a route cannot shrink geometry, so it is rejected before A*.

Candidates at equal area remain active because transport tiles and turns can still improve. Candidate tie-breaking uses the original `(rotation, y, x, source port, target port)` order, so evaluation order does not alter equal-score selection.

## Result

| Metric | Dense A* baseline | Additive shared bound | Change |
| --- | ---: | ---: | ---: |
| Six-step wall time | 26.72 s | 8.45 s | 3.16x faster |
| Original hash-map baseline | 61.49 s | 8.45 s | 7.28x faster |
| Selected-candidate A* calls | 78,818 | 35,468 | 55.0% fewer |

| Step | Time | Placements | Additive first | Area-pruned | A* calls |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.402 s | 712 | 96 | 0 | 902 |
| 2 | 0.046 s | 1,564 | 240 | 956 | 326 |
| 3 | 0.194 s | 2,484 | 328 | 1,486 | 558 |
| 4 | 7.021 s | 2,988 | 392 | 0 | 33,029 |
| 5 | 0.042 s | 3,938 | 464 | 2,842 | 63 |
| 6 | 0.448 s | 4,328 | 504 | 2,578 | 590 |

The final node and every selected module, requirement, score, facility placement, port, and route are identical to the dense-workspace baseline. The final partial factory remains 15 facilities and 14 routed requirements in `53x11`.

## Next Measured Target

Step four now accounts for approximately 83% of wall time. Its best composition blocks one boundary-port option. No worker finds a zero-blocked incumbent, so the sound zero-blocked area rule cannot reject any placement. This is the next narrow target: derive a pre-route lower bound for unavoidable boundary-option loss, then share the best area separately for each proven loss tier. Transport-length lower bounds can further prune equal-area port pairs after that tier is known.

## Artifacts

- `report.json`: schema version 2 composition statistics including additive placement and area-bound counters.
- `heavy-xiranite.html`: unchanged localized six-page layout history.
