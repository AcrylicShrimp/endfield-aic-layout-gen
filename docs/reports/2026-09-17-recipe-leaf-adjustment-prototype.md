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

For target `item-proc-battery-5`, five seeds per size produced:

| Requested facilities | Exact | Median elapsed | Maximum elapsed | Median plans evaluated |
| --- | ---: | ---: | ---: | ---: |
| 16 | 5 / 5 | 16.1 ms | 25.8 ms | 62 |
| 64 | 5 / 5 | 24.7 ms | 288.8 ms | 128 |
| 128 | 5 / 5 | 113.3 ms | 305.6 ms | 524 |

The first 16-facility seed began with one battery facility and reached the exact
count through four expansions:

```text
1 -> 5 -> 13 -> 15 -> 16 facilities
```

It evaluated 14 plans in 12.3 ms and ended with five recipe occurrences. The
equivalent aggregate CP experiment needed three lazy cuts and 8,823 ms. The leaf
prototype is about 700 times faster in this representative case while also
producing the existing path-specific instance wiring directly.

A small target check used three other produced items at 16 and 64 facilities.
All six cases reached the exact count. Five completed in 8.0--37.5 ms; the
64-facility equipment-script case evaluated 1,340 plans in 359.3 ms.

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
evaluate hundreds of full throughput plans. Production promotion should add a
bounded topology policy and cache leaf deltas or affected ancestors, while
retaining one exact final evaluation and wiring validation.

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
