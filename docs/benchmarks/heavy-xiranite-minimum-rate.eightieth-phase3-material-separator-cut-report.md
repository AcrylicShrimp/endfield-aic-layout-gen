# Heavy Xiranite Phase 3 Material Separator Cut

## Result

The exact row-4 material separator portfolio is valid, but it does not split the live search
space. Root propagation in the unrestricted control already forces the first separator predicate:

```text
selected material source: 48 -> 64
first south crossing:      64 -> 80
selected:                  1
flow:                      1..4
item code:                 5
```

Consequently, separator case 0 repeats a fact already present at the control root. Cases 1 through
15 all exclude that fact as part of their canonical prefix and are therefore rejected at the root.
The portfolio proves a leaf-local routing prefix, but it neither finds a witness nor explains the
remaining five-second first-feasible cliff.

## Exact Experiment Contract

The experiment continues the surviving source-only Phase 3 leaf for
`network:pipe:item-liquid-xiranite-poly` under exact fixed dimensions `16 x 16`. The selected source
continuation is `48 -> 64`; every demand continuation remains unrestricted.

A complete horizontal separator after row 4 contains the 16 south-directed arcs
`64 -> 80` through `79 -> 95`. For each arc `e`, the material-crossing predicate is:

```text
Q(e) <-> selected(e) = 1 AND from_item(e) = 5
```

Child `i` requires `Q(i)` and excludes every `Q(j)` for `j < i`. This is a non-empty, pairwise
disjoint, exhaustive first-true partition because the selected source and demand are on opposite
sides of the complete separator. Later separator arcs, all routing on either side, demand
continuations, flow magnitudes, branches, convergers, cycles, bridges, placement, ports, and other
networks remain solver decisions.

The encoding adds no selector variables or table constraints. A selected child uses two native
unary constraints, while every preceding exclusion is a native binary predicate clause:

```text
selected(j) = 0 OR from_item(j) != 5
```

The unrestricted control posts none of these separator restrictions.

## Controlled Result

All authoritative and observation runs agree. Every certificate, exact-cover gate, fixture gate,
model-identity gate, hidden-domain gate where observable, and parent-child evidence gate passes.
There are no invalid witnesses or proof conflicts.

| Case | First material crossing | Outcome | Root conflict | Search | Decisions | Backtracks | Conflicts | Learned clauses | Solver propagations |
| ---: | --- | --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Control | unrestricted | Unknown | no | 5,007 ms | 52,704 | 3,725 | 3,724 | 3,724 | 5,310,594 |
| 0 | `64 -> 80` | Unknown | no | 5,007 ms | 51,621 | 3,663 | 3,662 | 3,662 | 5,196,490 |
| 1 | `65 -> 81` | Proven infeasible | yes | 121 ms | 0 | 0 | 1 | 1 | 205,010 |
| 2 | `66 -> 82` | Proven infeasible | yes | 113 ms | 0 | 0 | 1 | 1 | 205,806 |
| 3 | `67 -> 83` | Proven infeasible | yes | 117 ms | 0 | 0 | 1 | 1 | 206,252 |
| 4 | `68 -> 84` | Proven infeasible | yes | 124 ms | 0 | 0 | 1 | 1 | 206,694 |
| 5 | `69 -> 85` | Proven infeasible | yes | 126 ms | 0 | 0 | 1 | 1 | 207,136 |
| 6 | `70 -> 86` | Proven infeasible | yes | 129 ms | 0 | 0 | 1 | 1 | 209,399 |
| 7 | `71 -> 87` | Proven infeasible | yes | 131 ms | 0 | 0 | 1 | 1 | 216,212 |
| 8 | `72 -> 88` | Proven infeasible | yes | 130 ms | 0 | 0 | 1 | 1 | 209,926 |
| 9 | `73 -> 89` | Proven infeasible | yes | 131 ms | 0 | 0 | 1 | 1 | 210,636 |
| 10 | `74 -> 90` | Proven infeasible | yes | 132 ms | 0 | 0 | 1 | 1 | 211,209 |
| 11 | `75 -> 91` | Proven infeasible | yes | 126 ms | 0 | 0 | 1 | 1 | 211,782 |
| 12 | `76 -> 92` | Proven infeasible | yes | 95 ms | 0 | 0 | 1 | 1 | 212,355 |
| 13 | `77 -> 93` | Proven infeasible | yes | 136 ms | 0 | 0 | 1 | 1 | 245,263 |
| 14 | `78 -> 94` | Proven infeasible | yes | 132 ms | 0 | 0 | 1 | 1 | 245,270 |
| 15 | `79 -> 95` | Proven infeasible | yes | 130 ms | 0 | 0 | 1 | 1 | 245,271 |

The result totals zero validated feasible cases, fifteen proven-infeasible cases, one `Unknown`
case, and zero invalid cases.

## Why Fifteen Proofs Do Not Mean Fifteen Independent Routing Discoveries

The unrestricted control already has `Q(64 -> 80) = true` after root propagation. Case 0 adds two
constraints expressing that same fact, so its root separator domains are identical to the control.
Every later canonical child contains `not Q(64 -> 80)` and immediately contradicts the root.

The small control-to-case-0 counter differences are not evidence of a runtime improvement:

| Metric | Control | Case 0 | Delta |
| --- | ---: | ---: | ---: |
| Decisions | 52,704 | 51,621 | -2.05% |
| Conflicts | 3,724 | 3,662 | -1.66% |
| Solver propagations | 5,310,594 | 5,196,490 | -2.15% |
| First witness | none | none | unchanged |
| Proof | none | none | unchanged |

Both runs consume the full five-second cutoff and share the same registered root domains, hidden
domain count, first delegated decision, and root material-separator state. The row-4 partition is
therefore exact but root-redundant.

## Model Delta

The control has 63,385 variables, 161,634 constraints, 618,980 incidences, and 242,663
placement-routing incidences. Every child retains all variables and all placement-routing
incidences.

| Model | Variables | Constraints | Incidences | Placement-routing incidences |
| --- | ---: | ---: | ---: | ---: |
| Control | 63,385 | 161,634 | 618,980 | 242,663 |
| Case 0 | 63,385 | 161,636 | 618,982 | 242,663 |
| Case 15 | 63,385 | 161,651 | 619,012 | 242,663 |

Child `i` adds exactly `2 + i` constraints and `2 + 2i` incidences. The measurement matches the
contract and confirms that the experiment changes only the exact separator restriction.

## What Is and Is Not Proven

Proven inside the inherited fixed leaf:

- every feasible solution must start the selected material route with `48 -> 64 -> 80`;
- none of the other fifteen row-4 arcs can be the first selected-material south crossing;
- the complete row-4 canonical partition is exact and introduces no heuristic restriction;
- the native model already propagates this first crossing before search.

Not proven:

- that the surviving leaf is feasible or infeasible;
- that flow on `64 -> 80` has a unique magnitude;
- that the route is unique, shortest, straight, cycle-free, or unbranched;
- that no later row-4 crossing is also used;
- that route-local freedom is the dominant remaining blocker;
- that `16 x 16` is an optimum, default, or required game size.

The `16 x 16` dimensions remain a controlled diagnostic fixture. The inherited predecessor
placement uses height 16, and a separate exact sensitivity run found the same unresolved cliff from
`13 x 16` through `16 x 16`. Production solving continues to enumerate exact dimension cases.

## Next Exact Diagnostic

The smallest next split is the immediate selected-material continuation leaving cell 80:

1. `Q(80 -> 81)` is true; or
2. `Q(80 -> 81)` is false and `Q(80 -> 96)` is true.

Cell 80 is not a terminal, the incoming `64 -> 80` material flow is positive, its west reverse arc
cannot be selected with the incoming arc, and no west-neighbor arc exists at the boundary. The east
and south cases therefore form an exact two-way cover while preserving splitters in the first
child and leaving every downstream decision free.

If both children remain unresolved, the next complete diagnostic is the row-5 material separator.
If two consecutive route refinements are root-redundant or reproduce the same unresolved trace,
route-local partitioning should stop and the next experiment should target cross-network or shared
topology coupling. A scalar flow-magnitude partition of `64 -> 80` is also exact, but it tests
circulating or excess-flow freedom rather than the next geometric route choice and is secondary to
the immediate continuation split.

## Improvements Preserved in the Exact Baseline

The run retains:

1. shared belt and pipe physical layers;
2. factored placement and port variables;
3. canonical physical occupancy coupled bidirectionally to transport occupancy;
4. external terminals inside commodity-network routing;
5. exact parallel dimension cases and proof-derived bound sharing;
6. possible-graph connectivity propagation;
7. event-driven unique-support and local-continuation propagation;
8. guarded positive-item intersection propagation;
9. exact placement, rotation, port, endpoint, boundary, and residual-tuple partitions;
10. exact sparse legal external-boundary domains;
11. exact source and endpoint continuation controls;
12. complete raw variable-domain certificates and fail-closed fixture/model gates.

No layout, route, corridor, path order, port choice, or other heuristic is introduced.

## Independent Review

Three independent reviews examined the exact proof, implementation, artifact isolation, and next
strategy. The experiment review initially blocked the release because the accepted fixture was not
fail-closed and the raw variable certificates synthesized bounds and omitted Pipe/dimension gates.
The implementation was corrected to require the exact Phase 3 fixture and record the declared
variable family and bounds. The re-review passed.

Post-run reviews agree that the fifteen root proofs are sound and that case 0 remains unresolved.
They also agree that the row-4 split itself did not reduce the live search space because the control
already forced `64 -> 80`. One reviewer proposed a four-way flow-magnitude split; the strategy
reviewer proposed the smaller geometric two-way continuation at cell 80. The latter is selected as
the next diagnostic because it directly advances the route chain and distinguishes an immediate
turn choice before applying another full separator.

## Timing and Artifacts

The new experiment took 24,776 ms:

- authoritative control: 5,562 ms wall;
- observation control: 5,568 ms wall;
- authoritative child wave: 6,805 ms wall;
- observation child wave: 6,839 ms wall.

The complete chained diagnostic took 356,440 ms because the CLI reconstructs every accepted parent
portfolio before the new wave.

Artifacts are stored in `/tmp/aic-phase3-material-separator.MykAN5`:

- `summary.json`, `stdout.json`, and self-contained `summary.html`;
- unrestricted control authoritative and observation wireframes;
- authoritative and observation wireframes for all sixteen children;
- `root-cut.json`, the focused control/case-0 root comparison.

## Verification

```text
cargo fmt --all
cargo check --workspace
cargo test --workspace
cargo build --release -p aic-cli --bin aic-prior-terminal-pair
git diff --check
```

The final pre-run workspace verification passed 34 main CLI tests, 2 prior-terminal CLI tests, and
298 data-library tests. A strict workspace Clippy run was attempted separately and remains blocked
by pre-existing unrelated warnings; Clippy is not used as evidence for this slice.
