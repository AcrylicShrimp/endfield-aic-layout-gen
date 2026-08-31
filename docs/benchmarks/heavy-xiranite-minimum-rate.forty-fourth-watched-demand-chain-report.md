# Heavy Xiranite Minimum-Rate Watched-Demand Chain Report

## Result

Reverse dependencies from changed `(material, cell)` states to the selected demand chains that
previously inspected those states preserve the exact recursive unique-support inference and remove
most of its redundant chain work.

Across two release runs, the watched-demand variant reproduces the broad and dirty-material chain
exactly at the search and solution levels:

- 43,543 branch decisions;
- 8,577 backtracks, conflicts, and learned clauses;
- 361 forced predicates;
- objective `(144 area, 110 transport tiles, 57 turns, 12 maximum side, 20 components)`; and
- independently validated feasible output.

The scheduler changes only which demand chains are recomputed. Compared with dirty-material
scheduling, it reduces:

- grid-propagator executions from 33,376 to 22,659 (`-32.1%`);
- selected-demand checks from 166,880 to 36,971 (`-77.9%`);
- unique-support-chain steps from 303,473 to 100,809 (`-66.8%`); and
- solver-reported propagations from 7,081,503 to 7,070,786 (`-0.15%`).

The midpoint of the two first-witness observations falls from 3,725 ms to 3,446 ms (`-7.5%`). The
timing movement agrees with the deterministic work reduction, but it remains a two-run host-local
observation from a fixed suite order rather than a controlled timing claim.

## Repeated Comparison

| Metric | Dirty material A | Watched demand A | Dirty material B | Watched demand B |
| --- | ---: | ---: | ---: | ---: |
| First witness | 3,741 ms | 3,436 ms | 3,709 ms | 3,455 ms |
| Branch decisions | 43,543 | 43,543 | 43,543 | 43,543 |
| Backtracks | 8,577 | 8,577 | 8,577 | 8,577 |
| Conflicts | 8,577 | 8,577 | 8,577 | 8,577 |
| Learned clauses | 8,577 | 8,577 | 8,577 | 8,577 |
| Solver-reported propagations | 7,081,503 | 7,070,786 | 7,081,503 | 7,070,786 |
| Grid executions | 33,376 | 22,659 | 33,376 | 22,659 |
| Material passes | 143,766 | not applicable | 143,766 | not applicable |
| Demand rechecks | not measured | 39,825 | not measured | 39,825 |
| Unique-support steps | 303,473 | 100,809 | 303,473 | 100,809 |
| Forced predicates | 361 | 361 | 361 | 361 |
| Maximum reason predicates | 176 | 176 | 176 | 176 |
| Validation | passed | passed | passed | passed |

The watcher performs 394,258 subscribed notifications, records 723,454 watcher hits, and deduplicates
them into 39,825 demand rechecks. It creates 123 distinct monotone `(demand, cell)` registrations and
observes at most 1,544 dirty demand IDs before one execution. Watcher hits are scheduling events,
not proof premises.

The 10,717 reduction in solver-reported propagations exactly equals the 10,717 skipped grid
executions. It is the aggregate Pumpkin propagation counter, not evidence of a separate native
engine-work reduction.

## Exact Watcher Contract

Every selected demand is scanned backward through the same live-domain unique-support rule used by
the broad propagator. The scan records all cells that it reads, including cells where it stops on a
possible local supply, multiple incoming supports, zero incoming supports, or a repeated cell.

Forward support-loss events map to those cells:

- demand selection directly dirties that demand;
- supply loss visits the supply's `(material, cell)`;
- route-arc loss visits `(every layer material, arc head cell)`; and
- any shared arm-item domain change visits every material and head-cell incidence of that variable.

Watcher memberships are monotone and may become stale after propagation or backtracking. This is
safe because they can only schedule extra demand recomputation. Every support decision, conflict,
and explanation is rebuilt from current Pumpkin domains. No cached support count, unique arc, or
reason is used as evidence.

## Controlled Verification

Targeted tests cover:

- an incoming branch changing from two supports to one;
- a possible local supply disappearing;
- an interior item-domain value disappearing without a bound change;
- one material assignment removing another material's support; and
- direct demand selection waking only that demand when no routing inference follows;
- finite repeated-cell stopping on a legal two-cell circulation; and
- a deterministic conflicting descendant, actual solver backtrack, and equal broad-versus-watched
  solution after restoration.

The grid-analyzer suite passes both normal execution and Pumpkin's `pumpkin-debug-checks` mode,
which reruns the stateless full scan to check incremental propagation and explanations.

## Independent Reviews

Three independent reviews separately examined proof soundness and bugs, implementation and runtime
cost, and follow-up exact propagation strategies. The initial proof review blocked acceptance because
the design promised controlled cycle and backtracking coverage that the first test set did not yet
provide. The missing tests were added, and the backtracking fixture now uses an explicit warm-start
decision, records at least one backtrack, and compares broad and watched results. The proof reviewer
then returned `PASS` after rerunning both normal and debug-check suites.

No reviewer found a pruning-soundness defect. They confirmed that DomainId aliases retain all
material/cell incidences, stop cells are watched, live domains rebuild every deduction and reason,
and stale watchers are conservative across restoration. They also agreed that the next material
gain must come from stronger semantic inference rather than replacing the small watcher sets with a
faster container.

The performance review's wording findings were accepted: elapsed results are now explicitly
fixed-order observations, and Pumpkin's aggregate count is called `solver-reported propagations`.
Dense dirty flags, scratch traces, and sparse watcher storage remain valid exact implementation
ideas but are deferred because this case creates only 123 watcher registrations and the unchanged
43,543 decisions dominate the next research question. No inconclusive review claim was used to
change solver semantics.

## Cumulative Exact Improvements

This experiment continues the exact semantic-propagation sequence without preselecting placement,
ports, or routes:

1. possible-graph connectivity makes unreachable selected-demand combinations fail during search;
2. reachable-only traversal and lazy explanations remove most custom graph scans;
3. local terminal-support inference forces the sole legal incoming support with short reasons;
4. recursive unique-support chains extend that rule through consecutive route cells; and
5. watched-demand scheduling now avoids rescanning unrelated demand chains.

On the current repeated run, the original fixed-placement/fixed-terminal baseline still finds no
witness within five seconds. The watched-demand variant finds the same validated witness in about
3.45 seconds. This does not solve the full Heavy Xiranite graph; it establishes a faster exact
semantic kernel for the next growth experiment.

## Conclusion And Next Target

The watched-demand frontier is accepted as a measured implementation improvement. It does not
change the solution set or search decisions, and it removes repeated semantic work. Both paired
runs are faster, but controlled alternating runs would be required to claim repeatable wall-clock
speedup independently of suite order.

The remaining 7.07 million native propagations and unchanged 43,543 decisions show that scheduling
is no longer the main search-tree blocker. The next experiment should add or strengthen an exact
semantic inference that removes decisions, rather than further micro-optimizing this watcher. The
specific rule will be selected after independent proof, implementation, and strategy reviews of
this slice.

## Artifacts

- `heavy-xiranite-phase2-watched-demand-chain-5s-a/summary.json`
- `heavy-xiranite-phase2-watched-demand-chain-5s-b/summary.json`
- `../designs/watched-demand-grid-chain-experiment.md`
