# Phase 3 Prior-Terminal Completion Report

## Result

Completing every port choice on the preceding target facility narrows the Phase 3 cliff without
eliminating it.

The preceding pair portfolio again proves 17 of 25 demand-pair regions infeasible and leaves eight
unknown. The completion portfolio expands exactly those eight parents by every remaining target
output port:

- 32 of 40 children prove infeasible in 87--142 ms;
- 8 children remain unknown after five seconds;
- no child produces an incumbent; and
- the eight unknown children are exactly the cases selecting `output-belt-4`.

The surviving leaves therefore have all four terminals of the preceding target facility fixed:

- singleton pipe demand: `input-pipe-5`;
- belt supply: `output-belt-4`;
- one powder demand: `input-belt-4`; and
- the other powder demand: one of `input-belt-0` through `input-belt-3`.

The two powder-demand terminal identities can swap, giving eight ordered leaves.

This result rules out unresolved target-facility port selection as the complete explanation. It does
not establish that `output-belt-4` itself is the root cause. The experiment is a closing control:
the output choice is now fixed, and the residual search remains expensive.

## Exact coverage

The report shares the exact same validated preceding reference object between parent and child
stages. It does not solve a new prefix and assume that a new placement is interchangeable.

The selected diagnostic state is partitioned into:

| Region kind | Count | Outcome |
|---|---:|---|
| Closed demand-pair parents | 17 | Proven infeasible |
| Completion children | 40 | 32 infeasible, 8 unknown |
| Total exact coverage regions | 57 | 49 infeasible, 8 unknown |

Only parents with a Pumpkin infeasibility proof are not expanded. The 40 children are the complete
Cartesian product of the eight retained parents and the five output-port values. The singleton pipe
port is fixed and reported in every child.

Because eight children remain unknown, the selected placement state remains unknown.

## Output-port matrix

Each output choice is tested once under each of the eight retained demand pairs.

| Target output port | Infeasible | Unknown | Search behavior |
|---|---:|---:|---|
| `output-belt-0` | 8 | 0 | 110--130 ms |
| `output-belt-1` | 8 | 0 | 91--123 ms |
| `output-belt-2` | 8 | 0 | 91--137 ms |
| `output-belt-3` | 8 | 0 | 87--142 ms |
| `output-belt-4` | 0 | 8 | 5,006--5,007 ms |

The 32 completed child proofs have a median search time of 118 ms. They require no branch decision:
root propagation reaches a contradiction after approximately 208,000--210,000 solver
propagations.

## Residual unknown leaves

| Demand pair | Decisions | Backtracks | Conflicts | Learned clauses | Propagations |
|---|---:|---:|---:|---:|---:|
| belt-0 / belt-4 | 47,763 | 3,725 | 3,724 | 3,724 | 4,233,807 |
| belt-1 / belt-4 | 43,621 | 3,750 | 3,749 | 3,749 | 4,290,226 |
| belt-2 / belt-4 | 41,960 | 3,740 | 3,739 | 3,739 | 4,000,768 |
| belt-3 / belt-4 | 42,938 | 3,154 | 3,153 | 3,153 | 4,067,354 |
| belt-4 / belt-0 | 43,845 | 3,329 | 3,328 | 3,328 | 3,963,021 |
| belt-4 / belt-1 | 40,816 | 3,546 | 3,545 | 3,545 | 3,993,682 |
| belt-4 / belt-2 | 41,338 | 3,705 | 3,704 | 3,704 | 3,953,599 |
| belt-4 / belt-3 | 52,895 | 3,890 | 3,889 | 3,889 | 4,971,241 |

Every child has the same exact model scale:

| Variables | Constraints | Incidences | Placement-routing incidences |
|---:|---:|---:|---:|
| 64,471 | 163,829 | 626,031 | 244,622 |

Child construction takes 536--1,005 ms under parallel contention, with a median of 950 ms.

## Timing

The corrected schema separates phases rather than calling the pair wave the complete outer wall
time:

| Stage | Wall time |
|---|---:|
| Prefix and pair preparation | 32,280 ms |
| 25 parent pair cases | 12,687 ms |
| Child preparation | <1 ms |
| 40 completion children | 23,828 ms |
| Complete CLI diagnosis | 68,926 ms |

The pair schema now also records derived feasible and infeasible counts directly, resolving the two
non-blocking reporting issues found by independent review of the prior slice.

## Interpretation and next exact control

The target facility's endpoint tuple is now complete in every residual leaf. The next smallest
unresolved high-level choice is the output port of the older `item-xiranite-powder` source in the
same shared commodity network. Its complete five-value domain should be enumerated under only these
eight leaves, producing another 40-case exact portfolio.

If that source port closes most leaves, the cliff remains an endpoint-tuple interaction across the
commodity network. If fully fixed powder-network facility endpoints still time out, the next target
is route/flow/topology propagation rather than another target-port partition.

A port-first monolithic brancher is not integrated here. It is a hand-selected search-order
heuristic under current project policy, and it cannot resolve leaves whose relevant target ports are
already fixed.

## Artifacts

```text
/tmp/aic-phase3-prior-terminal-completion.p49TpQ
```

The CLI generated `summary.json`, `summary.html`, and 40 standalone child HTML files.

## Reproduction

```text
target/release/aic-prior-terminal-pair \
  --workload data/benchmarks/heavy-xiranite-minimum-rate.workload.json \
  --workspace-root . \
  --placement-request data/benchmarks/requests/placement.16x16.request.json \
  --target-phase 3 \
  --used-width 16 \
  --used-height 16 \
  --facility-x 8 \
  --facility-y 5 \
  --port-assignment-index 5 \
  --facility-rotation 0 \
  --prior-facility-bit 2 \
  --terminal-pair 2,3 \
  --worker-count 12 \
  --prefix-case-time-limit-ms 10000 \
  --pair-case-time-limit-ms 5000 \
  --complete-target-ports \
  --child-case-time-limit-ms 5000 \
  --output-dir /tmp/aic-phase3-prior-terminal-completion.p49TpQ
```
