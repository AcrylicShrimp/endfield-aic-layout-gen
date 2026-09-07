# Ordered interfaces and a corrected flow premise

## Result and scope

Twelve isolated eastward belt corridors admit 52 independent non-sharing lanes covering 46 logical
requirements. A bounded CP-SAT order/port oracle composes their orders simultaneously. The candidate
still has 59 facilities and all 89 internal plus seven external requirements; it is **not a complete
factory**, and cross-corridor geometry, return connections, pipes and external wiring remain open.
The nested 82 x 116 placement is explicitly unaccepted, not a normal placement output.

For independent monotone paths between two ordered faces, source and destination endpoint orders
must agree. Opposite endpoint order forces an intersection in that corridor. This restricted fact
does not prove nonplanarity of the unconstrained graph or infeasibility of the production request.
The integer oracle chooses block ranks and distinct concrete catalog ports; a separate checker
validates permutations, port identities and isolated unit-grid paths. No global grid search runs.

| Fixed-flow scope | Status | Variables | Constraints | Build ms | Search ms |
| --- | --- | ---: | ---: | ---: | ---: |
| All 13 forward corridors | INFEASIBLE | 653 | 1068 | 27.87 | 991.12 |
| 12 orderable corridors | OPTIMAL | 522 | 813 | 22.26 | 786.51 |
| Corridor 42:47 alone | OPTIMAL | 227 | 365 | 11.07 | 625.99 |

OR-Tools 9.15.6755, one worker, five-second search limit, single sequential observations. OPTIMAL
means satisfaction completed, not area optimality. The sufficient assumption core is corridor
33:38 under the model's background port constraints; it is not a minimal global impossibility
proof. JSON analysis/composition took 52.54 ms excluding the solver process. Build/test and browser
time are excluded. Catalog/input hashes are checked by the diagnostic CLI.

## Same-item reassignment changes the graph

F33 and F38 both produce `item-plant-moss-3`. Their original pair-flow K2,2 was **not imposed by
the recipes**. Holding those edge rates fixed over-constrained the research problem. Exact rational
transportation allocation with sources F33,F32,F38 and targets F26,F23,F45,F46 yields:

| Producer | Consumer | Items/s |
| --- | --- | ---: |
| F33 | F26 | 1/3 |
| F33 | F23 | 1/9 |
| F32 | F23 | 2/9 |
| F32 | F45 | 2/9 |
| F38 | F45 | 1/9 |
| F38 | F46 | 1/3 |

Every selected producer still supplies 4/9 and every consumer receives 1/3. Eight local flows become
six without strict order inversions. Other commodity components remain unchanged. This assumes
complete same-item compatibility among these selected endpoints; it cannot substitute another
item, change a recipe, discard demand or silently reassign a constrained pair. No physical route
or production graph is changed by `redistribute.mjs`.

The retained fixed-flow reports are historical baseline evidence, not proof that a junction must
be built for this example. The next step is a real different-item fixture and explicit planned
crossing geometry. The user now allows crossings inside such validated structural modules while
ordinary connections should remain ordered. N:M support must include capacity-aware branching and
merging, not just a larger picture of a 2:2 crossing.

## Verification and reproduction

Node tests cover inversions, bounded unknown, fixed-pair obstructions, geometry rejection, complete
requirement accounting, joint oracle post-checking and exact-margin reassignment. Five Python tests
compare the oracle to independent small permutations and verify its scoped failures. Workspace
check and all 519 Rust tests passed (two existing ignores); this slice changes no Rust behavior.
Actual before/after, obstruction and whole-candidate browser images were inspected, with six
viewport/theme checks. Compare against AGENTS and the constructive planner design: diagnostic
tooling only; production construction and exact baseline remain unchanged.

```sh
node --test docs/benchmarks/constructive-zero-crossing/model.test.mjs
uv run --with 'ortools>=9,<10' python -m unittest discover \
  -s docs/benchmarks/constructive-zero-crossing -p '*_test.py'
node docs/benchmarks/constructive-zero-crossing/run.mjs \
  --placement artifacts/benchmarks/constructive-chain-blocks/gap-6-placement.json \
  --items data/game/normalized/items.json --facilities data/game/normalized/facilities.json \
  --transports data/game/normalized/transports.json \
  --order-report artifacts/benchmarks/constructive-zero-crossing/order-slice.json \
  --report-output NEW_JSON --visualization-output NEW_HTML
```

Generate a discovery JSON without `--order-report` to run `order_sat.py`; its explicit scopes are
`all-forward`, `orderable-slice` and `corridor`. New output paths are required. Detailed data stays
in JSON files and stdout is a compact completion summary.
