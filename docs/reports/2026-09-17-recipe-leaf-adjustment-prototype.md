# Recipe leaf-adjustment prototype

Date: 2026-09-17

## Decision

Use bounded leaf adjustment over the existing path-specific recipe source plan
as the semantic selection direction. Do not use the aggregate CP model to choose
active recipes. Retain the CP prototype only as research evidence and a small
correctness oracle.

## Algorithm

The prototype chooses a target producer and a bounded target rate, then creates
the smallest ready source plan: the root recipe is active and each of its inputs
is an explicit external-input leaf. It repeatedly applies one boundary move:

- `expand`: replace an external-input leaf with one recipe that produces the
  demanded item, and make that recipe's inputs new external leaves; or
- `contract`: replace a recipe leaf whose children are all boundary leaves with
  one external-input leaf.

Every candidate is evaluated by the existing contextual throughput and facility
requirement pipeline. A move is admitted only when it changes the facility count
toward the request and strictly reduces absolute count error. Candidate ties are
seeded. Several bounded root-rate starts avoid relying on one local basin.

The search cache is keyed by target quantity and canonical path selections.
Facility-instance wiring is built once for the selected result, rather than for
every leaf candidate. The final instance count must equal the selected facility
count. Cycles continue to use the existing source-plan ancestor-reference
semantics; a cycle reference does not recursively create more leaves.

This keeps the operation local. It does not enumerate recipe subsets, rebuild a
global graph model or ask a solver to discover connectivity.

## Controlled results

All cases used the normalized 305-recipe game catalog, a release build, a
20-second target-rate horizon, at most 64 starts and at most 64 accepted leaf
moves per start. Elapsed time excludes catalog loading and includes the final
facility-instance wiring validation.

For target `item-proc-battery-5`, five seeds per size produced the following
results before and after removing repeated source-catalog construction from the
internal production-graph path:

| Requested facilities | Exact | Median before | Median after | Maximum before | Maximum after | Median plans evaluated |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 5 / 5 | 16.1 ms | 4.2 ms | 25.8 ms | 5.3 ms | 62 |
| 64 | 5 / 5 | 24.7 ms | 7.6 ms | 288.8 ms | 198.0 ms | 128 |
| 128 | 5 / 5 | 113.3 ms | 33.2 ms | 305.6 ms | 186.7 ms | 524 |

The first 16-facility seed began with one battery facility and reached the exact
count through four expansions:

```text
1 -> 5 -> 13 -> 15 -> 16 facilities
```

It evaluated 14 plans in 12.3 ms before the internal-path optimization and 1.7
ms afterward, ending with five recipe occurrences in both cases. The equivalent
aggregate CP experiment needed three lazy cuts and 8,823 ms. Even the original
leaf prototype was about 700 times faster in this representative case while also
producing the existing path-specific instance wiring directly.

A small target check used three other produced items at 16 and 64 facilities.
All six cases reached the exact count. Five completed in 8.0--37.5 ms; the
64-facility equipment-script case evaluated 1,340 plans in 359.3 ms before the
internal-path optimization. Ten repetitions afterward had a 194.6 ms median,
with a 189.9--206.9 ms range and the same selected result.

## Time profile

Xcode Time Profiler sampled the two slow deterministic cases at 1 ms. Before
the optimization, candidate evaluation occupied 83.4% of samples for the
64-facility equipment-script case and 77.0% for the 128-facility battery case.
Within candidate evaluation, recursive construction of the complete reachable
source catalog occupied 43.5% and 44.1% respectively. The catalog contains UI
choices and is not consumed by production-graph or throughput calculation.

The production-graph path now performs the same request validation and source
hierarchy construction without materializing that catalog. The public source
check still returns it. On the same instrumented invocations:

| Case | Before | After | Reduction | Evaluated plans |
| --- | ---: | ---: | ---: | ---: |
| Equipment script, 64 facilities, seed 7 | 358.4 ms | 190.4 ms | 46.9% | 1,340 |
| Battery, 128 facilities, seed 0 | 315.6 ms | 191.2 ms | 39.4% | 926 |

No source-catalog construction remained inside candidate evaluation after the
change. The next largest cost is the exact throughput solve: it occupied 71--72%
of candidate-evaluation samples, with `microlp` present in 54--56%. This is the
cost that a precomputed subtree method should avoid for rejected combinations.
The final selected plan should still receive one authoritative throughput,
facility and wiring evaluation.

## Interpretation

The user's proposed simplification is supported: choose a valid production
graph and add or remove recipes at its leaves. Connectivity is constructive, so
the search does not pay to reject disconnected global assignments. The
authoritative source-plan pipeline also preserves path occurrences, cycle
handling, throughput arithmetic and instance rounding without a conversion
layer.

The current prototype is a greedy research binary. Exact success on this small
panel does not prove that every target and facility count is reachable. Some
starts produce deep filling and dismantling chains, and the worst cases still
evaluate hundreds of full throughput plans. For a fixed target rate, acyclic
path branches have independent facility contributions: an expansion replaces
one external leaf with a producer occurrence and its child choices while leaving
the ancestor demand unchanged. A bounded tree dynamic program can therefore
precompute each leaf subtree's reachable facility increments and combine sibling
choices to hit the requested total. This also permits several leaf expansions
whose combined increment is useful even when no individual greedy move is.
Cycle-bearing choices need a guarded exact evaluation because their rates can be
coupled. Production promotion should use that precomputation, a bounded topology
policy, and one exact final evaluation and wiring validation.

## Reproduction

Build the release probe:

```sh
cargo build --release -p aic-cli --bin aic-recipe-leaf-probe
```

Run the representative case:

```sh
target/release/aic-recipe-leaf-probe \
  --recipes data/game/normalized/recipes.json \
  --target-item item-proc-battery-5 \
  --facilities 16 \
  --seed 0 \
  --attempts 64 \
  --max-steps 64 \
  --report-output target/recipe-leaf-probe.json
```
