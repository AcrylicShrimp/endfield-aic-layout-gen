# Native-valid-action MCTS on Heavy Xiranite

## Question and verdict

This experiment asked whether a learned policy/value MCTS could make useful
progress on the explicit Heavy Xiranite production graph inside a fixed
128×128 bound without letting the policy decide which native actions exist.

The corrected search made one complete physical connection. It placed all 59
facilities and seven terminals, selected a native-valid port pair, found nine
accepted extensions, connected the route, and committed that entire witness
atomically. An equal encoded-observation policy-sampling control completed no
connection. The search did not find a second route and therefore remains
`unknown`, not feasible or infeasible, for the full 102-connection layout.

The result also confirms the budget concern. A flat primitive-command tree used
thousands of begin/extend transitions without reaching `connect` until route
depth was prioritized and potential guidance was added. Even then it completed
only 1/102 connections. The next architecture should be hierarchical: a focused
inner search fixes one connection and port pair and returns only a complete
native route witness; the outer MCTS treats that witness as one atomic option.

## Implemented search contract

- The native constructive domain exposes deterministic pages for every locally
  valid `place`, `place-terminal`, `begin-route`, `extend`, and `connect`
  command. `abort-route` is excluded because a forked tree can backtrack without
  spending a native turn.
- Progressive widening owns action membership. It admits command kinds in
  round-robin order, eventually reaches every page, and visits every newly
  admitted child before applying PUCT.
- The checkpoint policy scores caller-selected native commands. It encodes the
  observation once, packs each distinct conditional factor bank once, and
  supplies only joint priors and the leaf value.
- Large placement and boundary-position pages use a deterministic full-cycle
  permutation so early pages cover the canvas while retaining exhaustive
  membership. Route and extension pages interleave connections and directions.
- A route is never committed at `begin-route`. Search clones may contain partial
  routes, but the committed session advances only when one path reaches
  `connect`; it then replays the complete begin/extend/connect witness.
- Search-only route guidance uses the potential
  `-Manhattan(open_endpoint, target) / max(width, height)`. The backed shaping
  term is `gamma * Phi(next) - Phi(current)`, so it guides direction without
  creating a positive reward cycle.
- A complete result must pass the native session and the independent physical
  validator. Failure to find a witness is reported as `unknown` with the exact
  valid-action frontier.

## Input and provenance

| Item | Value |
| --- | --- |
| Source plan | `data/examples/source-plan.game-heavy-xiranite-forge.request.json` |
| Target item | `item-xiranite-enr-powder` |
| Physical graph | 59 facilities, 7 terminals, 102 connections |
| Bound | 128×128 (16,384 cells) |
| Device | RTX 5090, `cuda:0` |
| CPU action workers | 8 |
| Checkpoint | retained constructive seed 101 |
| Learner SHA-256 | `5488334f496da5c8a4099abcc661fcf2fad0544ff7664dc94d1b90a6e36eeade` |
| Catalog SHA-256 | `d64fc5916e2d57c3052dd8f7eeefd85b4211ad222553c450391b601d64a51cca` |
| Baseline seed | 910021 |

The final bounded command was:

```text
target/release/aic-cli learning search-constructive \
  --training-output ~/Srv/aic-full-graph-online-20260920/artifacts/three-seed/seed-101 \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --catalog-root data/game/normalized \
  --output artifacts/heavy-xiranite-128x128-valid-mcts-shaped \
  --width 128 --height 128 \
  --simulations-per-move 32 --proof-simulations-per-move 1024 \
  --horizon 16 --max-actions-per-widening 32 \
  --initial-children 1 --widening 0.25 --exploration 1.25 \
  --route-potential-scale 1.0 --max-policy-evaluations 4096 \
  --baseline-seed 910021 --cpu-threads 8 --device cuda:0
```

## Controlled progression

All rows use the same graph, checkpoint, bound, device, and policy/value model.
The controls isolate action ordering, depth allocation, and route potential.

| Search | Initial / widening | Proof simulations | Policy evals | Begin applied | Extend applied | Maximum depth | Committed connections |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Native pages, row-major positions | 4 / 1.0 | 512 | 3,410 | not instrumented | not instrumented | not instrumented | 0 |
| Stratified pages | 4 / 1.0 | 512 | 3,861 | 2,092 | 2,602 | 5 | 0 |
| Depth-prioritized, no potential | 1 / 0.25 | 1,024 | 3,251 | 2,938 | 22,781 | 16 | 0 |
| Depth-prioritized plus potential | 1 / 0.25 | 1,024 | 3,275 | 2,831 | 19,411 | 16 | **1** |

The breadth-oriented run opened 26 root `begin-route` children and gave each too
little continuation depth. Narrow widening opened nine root children and drove
paths to the full 16-command horizon, but distance-neutral returns still found
no connection. Potential guidance produced six simulated `connect` transitions,
one complete route witness, and one committed connection.

## Final bounded result

| Metric | MCTS | Equal-evaluation policy sampling |
| --- | ---: | ---: |
| Encoded-observation evaluations | 3,275 | 3,275 |
| Best construction progress | **0.3988095** | 0.3928571 |
| Best completed connections | **1** | 0 |
| Fully validated layouts | 0 | 0 |
| Search/control seconds | 21.19 | 25.27 |

The committed route was connection 31 with ports `output-belt-0` and
`input-belt-0`. Its witness was:

```text
begin-route
west 1
south 1
south 1
south 1
south 1
east 1
east 1
east 1
east 1
connect
```

After that route and all placements were committed, the search could not find a
second complete witness. Its final root had admitted nine native-valid
`begin-route` children, buffered 23 more from the first 32-command page, and had
not exhausted the action set. This is a bounded search failure, not evidence of
infeasibility.

## Budget interpretation and next boundary

The primitive tree pays for three coupled choices: facility geometry, endpoint
ports, and a variable-length route. Once a port pair is selected, every segment
adds direction and length branching. Fast widening spends the budget across too
many port pairs; slow widening reaches useful depth but still repeats a long
sparse-reward search for each of 102 connections. Later routes also inherit the
occupancy left by earlier ones, so the observed first-route cost cannot be
linearly extrapolated as an optimistic full-factory estimate.

The next bounded implementation should preserve the same native legality and
policy/value scoring but introduce an explicit option boundary:

1. The outer node selects a logical connection and concrete source/target ports.
2. A focused inner tree exposes only native-valid `extend` and `connect`
   children for that fixed route, with its own bounded simulations, potential,
   and recovery-free clone state.
3. Failure returns no outer action. Success returns the complete command witness
   and its measured search cost.
4. The outer MCTS admits and commits only complete route witnesses, while
   placements and terminals remain ordinary native actions.

This changes where the budget is spent without adding an automatic router or
weakening validation. A subsequent experiment must report route-option attempts,
successes, inner node counts, and outer evaluations separately. It should stop
at a bounded decision if complete options remain too rare rather than increasing
the global budget without limit.

## Continuity checklist

| Existing benefit | Treatment | Check |
| --- | --- | --- |
| Native legality and stale-revision rejection | Pages are decoded through the existing conditional domain and replayed through `Session::step_selected` | Native page replay test |
| Learned policy/value model | Same checkpoint and exact factor likelihoods; policy no longer gates membership | Packed factored-command scoring test and CUDA run |
| Exact occupancy and route rollback | Every simulation forks `Session`; partial routes remain in clones | Atomic complete-route commit test |
| Batching and bounded resources | One observation encoding and one packed actor call per widening batch; configurable CPU pool and evaluation cap | Counters and release timing |
| Independent publication validation | Native layout plus physical validator required for `feasible` | Final classification remained `unknown` |
| Honest failure semantics | No timeout or frontier exhaustion is called infeasible | Final valid-action frontier retained in the report JSON |

## Verification and retained evidence

Local `cargo fmt --all`, `cargo check --workspace`, the native command-page test,
and test compilation passed. On `ashrimp-arch`, both focused learning tests
passed with LibTorch: native pages replay through the session, and a
begin/extend/connect witness commits atomically. The CUDA release CLI built and
ran the complete source-plan-to-report workflow.

The final output remains at
`~/Srv/aic-mcts-20260921/artifacts/heavy-xiranite-128x128-valid-mcts-shaped`.
The unshaped depth control remains beside it as
`heavy-xiranite-128x128-valid-mcts-deep`.
