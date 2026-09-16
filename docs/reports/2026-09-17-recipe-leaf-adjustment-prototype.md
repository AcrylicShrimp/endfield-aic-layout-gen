# Recipe source-tree selection prototype

Date: 2026-09-17

## Decision

Use bounded source-tree dynamic programming over the existing path-specific
recipe source plan as the semantic selection direction. Precompute exact
facility contributions for acyclic producer subtrees, combine sibling choices by
facility count, then run the authoritative throughput, facility and instance
wiring pipeline once on the selected plan. Do not use the aggregate CP model to
choose active recipes. Retain the CP and greedy leaf prototypes as research
evidence and small correctness references.

## Algorithm

The selector chooses a target producer and one of a seeded, duplicate-free set
of bounded root rates. For each demanded input item it records two classes of
subtree option:

- an external-input boundary with zero facilities; and
- each producer recipe, whose exact run rate and rounded facility count follow
  directly from the demanded item rate, output quantity and recipe duration.

Producer inputs recursively receive their exact demanded rates. Child option
maps are convolved by total facility count, so choices on several root branches
are selected together. For a given facility total, the retained representative
is selected by the seed. Recipe occurrence count is still bounded and reported,
but it is not minimized: doing so systematically preferred shallow factories
with replicated facilities. Choosing the zero-facility option is the precomputed
equivalent of removing a subtree; no trial contraction is needed.

The DP stores immutable subtree blueprints and materializes one full source plan
after selection. It does not copy a complete source plan per candidate. Memo
keys include item rate, remaining facility budget, depth and ancestor recipe and
item sets. Returning to an ancestor item closes that path as an external
boundary. The default maximum producer depth is four; total occurrences remain
bounded by `--max-steps`. Hard limits of 50,000 memo states and 2,000,000 child
combinations make truncation explicit in the report.

A repeated ancestor recipe has nonlocal cyclic rates and is not assigned an
additive subtree cost. If the acyclic DP cannot hit the request, a bounded exact
fallback tries direct cycle closures from the selected boundary. A closest
result after that fallback is not a global infeasibility claim.

The selected plan is passed once through contextual throughput, facility
requirements and facility-instance wiring. Predicted occurrence and facility
counts must equal the authoritative reports, and the final wiring instance count
must equal the requested count. This keeps connectivity constructive and avoids
both global recipe-subset enumeration and repeated LP solves.

## Greedy baseline and profile

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

## Precomputed subtree DP results

The DP used the same normalized catalog, 20-second target-rate horizon, 64
starts and 64-occurrence limit as the greedy baseline. Maximum producer depth
was four. Elapsed time excludes catalog loading and includes the one exact final
throughput, facility and instance-wiring validation.

For five battery seeds at each size:

| Requested facilities | Exact | Greedy median | DP median | DP maximum | Exact plans evaluated | Unique plans |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 5 / 5 | 4.2 ms | 0.768 ms | 2.930 ms | 1 each | 5 / 5 |
| 64 | 5 / 5 | 7.6 ms | 1.282 ms | 1.722 ms | 1 each | 5 / 5 |
| 128 | 5 / 5 | 33.2 ms | 1.540 ms | 2.047 ms | 1 each | 5 / 5 |

The DP was 5.5--21.6 times faster than the already optimized greedy prototype on
these medians. Every seed selected a different source plan. No run hit a memo or
combination limit; the largest run retained 219 memo states.

The three-target panel at 16 and 64 facilities again produced six exact results.
Elapsed time ranged from 0.273 to 2.194 ms, every case used one exact evaluation,
and the largest case retained 376 memo states. The former worst equipment-script
case took a 0.542 ms median over 20 repetitions, down from the optimized greedy
median of 194.6 ms. The 128-facility battery seed used for profiling took a 0.970
ms median over 20 repetitions, down from 191.2 ms under Time Profiler. These
sub-millisecond and low-millisecond measurements should be treated as
machine-local, but the removal of hundreds of exact solves is structural.
Fresh-process wall time over the same 20 repetitions, including catalog load,
process startup and report writing, had 3.456 ms and 3.943 ms medians
respectively. Production workers should still retain the validated catalog and
DP indices instead of paying that setup for every sample.

The controlled sibling-choice test demonstrates the non-greedy case directly.
One branch offers facility increments of four or two, a sibling offers three,
and the remaining target is five. Greedy selection commits to four and cannot
finish; the DP combines two and three and materializes the exact plan.

## Representative-selection bias follow-up

The first DP implementation used active recipe occurrence count before the seed
when retaining one representative for a facility total. That rule was an
unjustified directional bias: fewer occurrences often meant a shallow plan with
many copies of the same facility stage. It was removed rather than replaced by
an external-input target. External boundaries, distinct external items and
facility-instance wiring edges are now report observations.

A controlled before/after panel used 20 seeds for a 16-facility battery proposal
and 20 seeds for a 59-facility xiranite proposal. All 40 results still reached the
nominal count. Removing the occurrence minimization widened the observed recipe
and boundary distributions:

| Proposal | Observation | Before min / median / max | After min / median / max |
| --- | --- | ---: | ---: |
| Battery, 16 | Active recipe occurrences | 2 / 3 / 5 | 2 / 3 / 9 |
| Battery, 16 | External source occurrences | 3 / 3 / 4 | 3 / 3 / 7 |
| Xiranite, 59 | Active recipe occurrences | 4 / 5 / 14 | 4 / 5 / 21 |
| Xiranite, 59 | External source occurrences | 2 / 4 / 9 | 2 / 5 / 10 |

In the updated panel, battery plans had 12--24 external-input wiring edges and
xiranite plans had 20--59. These values are distribution evidence, not desired
limits. The change removes the known shallow-plan preference without installing
the remembered 59-facility/96-requirement example, one edge count or one boundary
count as a new optimization objective. Broader dataset-level sampling must decide
whether the generated population has useful coverage or a material imbalance.

## Continuity

| Existing benefit | DP treatment | Check |
| --- | --- | --- |
| Path-specific sources and repeated occurrences | Materialize the existing `RecipeSourcePlanRequest` | Final contextual reports match predicted occurrence and facility counts |
| Exact quantities, rounding and multi-output surplus | Use exact rational demand rates for acyclic costs | Authoritative throughput and facility validation runs once |
| Cycle semantics | Defer ancestor closures from additive DP | Bounded cycle candidates use the exact pipeline |
| Seeded topology variation | Seed root-start order and equal-size option selection | Five seeds produced five distinct plans at every battery size |
| Compact hot-loop state | Share immutable blueprint subtrees and memoized option maps | Integrated release measurements include selection and final validation |
| Resource and failure bounds | Limit depth, occurrences, memo states and combinations | Report exposes limits, truncation and closest status |

## Interpretation

The user's proposed simplification is supported: choose a valid production
graph and add or remove recipes at its leaves. Connectivity is constructive, so
the search does not pay to reject disconnected global assignments. The
authoritative source-plan pipeline also preserves path occurrences, cycle
handling, throughput arithmetic and instance rounding without a conversion
layer.

The precomputed subtree method now implements the proposed combination rather
than repeatedly trying complete plans. Its exact rate arithmetic matched the
authoritative pipeline on every selected acyclic plan in the controlled panel.
The depth-four policy also removed the long filling and dismantling detours that
caused state growth without being needed for count control.

Exact success on this panel does not prove that every target and facility count
is reachable. The current cyclic fallback tries one direct ancestor closure at a
time and is not an exhaustive cyclic optimizer. Depth, occurrence or resource
limits can return a closest result. Before production promotion, the selector
still needs a versioned request/report boundary, persistent catalog-owned caches
and a broader seeded yield survey. The evidence is sufficient to stop investing
in per-candidate LP evaluation and to use this DP as the semantic selector
kernel.

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
  --max-depth 4 \
  --report-output target/recipe-leaf-probe.json
```
