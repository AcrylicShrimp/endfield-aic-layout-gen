# Heavy Xiranite Minimum-Rate Dirty-Material Chain Report

## Result

Dirty-material scheduling is exact but removes no work from the controlled Phase 2 solve.

Across two release runs, the broad chain, support-loss selective chain, and dirty-material chain
all reproduce exactly:

- 43,543 branch decisions;
- 8,577 backtracks, conflicts, and learned clauses;
- 7,081,503 native solver propagations;
- 33,376 grid-propagator executions;
- 143,766 material passes;
- 303,473 unique-support-chain steps; and
- 361 forced predicates with a maximum 176-predicate reason.

The dirty-material variant is rejected as a runtime improvement. It proves that material identity
alone is too coarse a scheduling boundary for this workload.

## Repeated Comparison

| Metric | Broad A | Selective A | Dirty material A | Broad B | Selective B | Dirty material B |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| First witness | 3,618 ms | 3,637 ms | 3,621 ms | 3,644 ms | 3,598 ms | 3,598 ms |
| Branch decisions | 43,543 | 43,543 | 43,543 | 43,543 | 43,543 | 43,543 |
| Backtracks | 8,577 | 8,577 | 8,577 | 8,577 | 8,577 | 8,577 |
| Native solver propagations | 7,081,503 | 7,081,503 | 7,081,503 | 7,081,503 | 7,081,503 | 7,081,503 |
| Grid executions | 33,376 | 33,376 | 33,376 | 33,376 | 33,376 | 33,376 |
| Material passes | 143,766 | 143,766 | 143,766 | 143,766 | 143,766 | 143,766 |
| Unique-support steps | 303,473 | 303,473 | 303,473 | 303,473 | 303,473 | 303,473 |
| Validation | passed | passed | passed | passed | passed | passed |

The dirty-material median first-witness time is 3,610 ms. The broad-chain median is 3,631 ms and
the selective-chain median is 3,618 ms. These small elapsed differences are noise because the
entire deterministic search tree and every measured propagator work counter are identical.

## Why Material Filtering Fails

The fixed Phase 2 model has three belt commodities and five pipe commodities. Demand and local
supply events are material-specific, but the dominant route events are not:

- excluding one route arc removes a possible predecessor for every material on that layer; and
- changing one shared arm-item domain can remove several material codes at once.

The chain propagator's own item equalities also generate cross-material arm-domain changes.
Consequently almost every useful propagation wave dirties the full belt or pipe material set, and
the supposedly filtered execution degenerates to the broad per-layer scan.

## Independent Reviews

Three independent reviews examined proof soundness, model semantics, implementation risks, and
the next exact strategy. They agreed that the dirty-material dependency map is conservative and
that terminals, fungible same-item networks, bridges, disconnected balanced subnetworks, and legal
circulation do not invalidate the inference.

The reviews also agree that a true cell-local implementation must not merely enqueue a changed
cell. It needs reverse dependencies from every traversed or stopping `(material, cell)` to each
selected demand whose current chain depends on that cell. Reasons must always be rebuilt from live
Pumpkin domains, and watcher state must remain conservative across backtracking.

A reviewer suggested skipping enqueue when an event added no new entry to the dirty-material set.
A release counterexample rejected that optimization: a self-notification raised while the
propagator was already running did not survive as a future execution reservation, leaving eight
dirty material passes unprocessed and changing the search tree from 8,577 to 11,511 backtracks.
The committed variant therefore always returns `Enqueue` for a subscribed support-loss event.
Future watcher code must treat the dirty set and Pumpkin's queue membership as distinct states.

The next accepted experiment is therefore a watched-demand frontier:

1. record every cell inspected by each selected demand chain, including supply and branch stop
   cells;
2. map a support loss at `(material, cell)` back to only those demand roots;
3. recompute the unchanged chain rule for those roots; and
4. retain stale watchers as a safe over-approximation across backtracking while never using them
   as proof evidence.

## Artifacts

- `heavy-xiranite-phase2-dirty-material-chain-5s/summary.json`
- `heavy-xiranite-phase2-dirty-material-chain-repeat-5s/summary.json`
