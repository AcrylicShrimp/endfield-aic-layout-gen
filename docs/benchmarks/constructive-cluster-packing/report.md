# Connected cluster packing improvement

Date: 2026-09-06. Status: complete factory improved; full research/product goal remains active.

## Outcome

The previous goal turn was progress: `151d5c3` established the complete original steady-state graph.
This slice keeps that validated incumbent and evaluates complete alternatives. The selected factory
has all 59 original facilities, all 131 recipe terminals fully covered and the same seven original
external obligations. Used geometry is 134 x 86, area 11,524; physical transport tiles 2,120; turns 243.

| Complete witness | Bounds | Area | Transport tiles | Turns |
| --- | --- | ---: | ---: | ---: |
| Previous incumbent | 100 x 129 | 12,900 | 2,401 | 253 |
| Clustered, simple corridors only | 93 x 128 | 11,904 | 2,369 | 242 |
| Selected cluster/corridor portfolio | 134 x 86 | 11,524 | 2,120 | 243 |

Area decreases 10.7% and transport tiles 11.7% relative to the incumbent. Aspect ratio changes: the
layout is wider but substantially shorter. The selected layout has one more turn than the best simple
cluster variant, which is correct under the area-first score. Fifteen logistics components remain;
component count is not a score term. No minimum-area or broadly optimal packing claim is made.

## Cluster construction

Build connected components of the source-region/ready-consumer bipartite graph. All regions supplying
one ready consumer must stay in the same component, including transitive sharing. This derives two
components from actual graph edges without facility-label or recipe-ID rules:

- Three cyclic regions feed two processing chains: 18 original facilities after belt closure.
- Five cyclic regions feed four processing chains: 35 original facilities after belt closure.

The source regions own each original facility once. The shared powder supplier is not duplicated.
Each component grows three input-closed belt frontiers with the existing structured interface planner;
each raw/compacted packed-row/source-row candidate is independently checked before acceptance.
The resulting compound blocks are 39 x 43 and 80 x 85. Their outgoing carbon obligations remain pending
until mixed-transport factory closure, not replaced by external terminals.

`placeRegions` now implements explicit whole-region translations shared by both the old vertical stack
constructor and new shelf packing. It moves every facility, port, segment, terminal and component,
normalizes commodity networks and reindexes local physical identifiers. This removes the need to
duplicate region embedding for each packing method. Mutable rate/endpoint DTOs are detached between
candidates. The old stack's geometry and construction behavior are preserved.

Eight distinct shelf candidates result from bounded canonical/reversed/dimension-ranked orders,
horizontal/vertical axes and start/end alignment. These are only sampled packing families. Every
partial packing passes independent geometry and original pending-margin coverage; each then runs
the complete mixed-transport closure with both corridor families. Complete child results are again
independently validated before comparing area, physical tiles and turns against the incumbent.

## Visual obstruction and corridor extension

The first horizontal candidate reached 58 facilities but could not attach F00. The failed edge was the
polymer liquid from F01, not a missing recipe source or insufficient pipe capacity. The actual rendered
58-facility witness and pool close-up were inspected. The pipe could travel below the neighboring
block, but returning vertically to F00's north input passed through F00's body. The two-turn family
could not represent a separate ingress column beside that body.

The explicit perimeter family adds a reserved horizontal transfer row and an independent entry/exit
column, with up to three turns. It includes the simple candidates, uses unchanged collision/capacity
and score rules, and adds no implicit crossing or overlap. The horizontal candidate then completes
at 134 x 86. The original simple corridor family remains the default. A locally richer family can lead
to different later greedy choices, so both full outcomes are retained rather than assuming it dominates
the entire construction sequence.

All 16 closure attempts were recorded. Nine complete; seven stop at a bounded local frontier. Six of
the eight packings have at least one complete closure. The selected candidate is horizontal, small
cluster first, start-aligned, with perimeter corridors. No incomplete low-area state enters selection.
One complete packing is larger than the incumbent and is also rejected by the score comparison.

Personally inspected the final full image and the earlier obstructed pool region. Large empty areas
remain below the smaller cluster and inside the larger cluster's interface rectangle. These are now
the main visible opportunity: compare nonrectangular occupied-geometry nesting with shelf bounding
rectangles, and reduce the large cluster's reserved approach/permutation width. Any new packing or
interface must still close and validate the whole graph before replacing this incumbent. Compactness
is improved, not yet accepted as the final desired quality.

## Timing and evidence

The portfolio took 31.18 s wall time for 24 cluster-growth candidates, eight packing witnesses and
16 full closure attempts, with two closure jobs in parallel. Packing translation itself took 46.83 ms.
These are optimization-experiment timings, not the time to generate one already selected factory;
they include repeated validation, witness IO and some unrelated verification activity. No stable
median or single-factory speed comparison is claimed. Detailed per-stage/closure timings and source
hashes are in `matrix.json`. Candidate logs/witnesses stay in their target directories; stdout is one
short summary. The default-constructor regression reproduced all fifty previous candidate witness
hashes exactly, not merely the same bounding box.

Verification:

- 24 cluster stage candidates and eight packing candidates independently pass geometry/coverage.
- Nine complete closure results rechecked against the original 59-facility graph, including exact
  recipes, utilizations, all terminal totals and actual external endpoint/item/direction margins.
- Eight final-factory audit expectations pass: unchanged, undeclared external substitution, omitted
  export network, broken pipe, duplicate facility, changed utilization and two malformed terminal reports.
- 47 related Node tests pass. New tests cover graph-derived sharing, input-order/display-label
  independence, all 53 intermediate owners, every translated retained segment/rate, mutable DTO
  detachment, malformed translations and the north-input perimeter obstruction without collision exceptions.
- `cargo check --workspace` passes; workspace tests: 524 passed, two existing ignores. No Rust or
  dependency changes. Logs: `target/cluster-{all-tests,cargo-check,cargo-tests}.log`.
- Browser verification passes six light/dark viewport combinations (1400/736/360), wheel zoom,
  real pointer-drag pan and Fit. Actual final and obstruction renders were personally inspected.

The production design/policy is unchanged: this is an isolated JS improvement candidate built in
private from validated cyclic subgraphs. Growth states preserve covered input validity; final acceptance
is complete-graph validation and an improved score, not a partial geometry claim. No caller W/H ceiling,
global solver fallback, rule relaxation, new game data or premature Rust cutover was introduced.

## Reproduce

Regenerate the documented cycle blocks and complete incumbent if their referenced target witnesses
are absent. From the repository root, use a fresh output path:

```sh
node docs/benchmarks/constructive-cluster-packing/run.mjs \
  --graph docs/benchmarks/constructive-chain-blocks/gap-1-placement.json \
  --blocks docs/benchmarks/constructive-cycle-blocks/matrix.json \
  --incumbent docs/benchmarks/constructive-factory-closure/matrix.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/cluster-packing-repeat
```

Frozen evidence: `matrix.json`, `controls.json`. Final page:
`http://100.114.24.86:8765/bound-interconnect-closure-clusters.html`.
The earlier failed horizontal view is `bound-interconnect-closure-horizontal-obstruction.html` on
the same server. Corresponding PNGs and head close-ups are under `target/shared-results`.
The full active goal still includes further space improvement, additional complex recipes, cleanup
and the real recipe/rate-to-layout web workflow; none is replaced by these static diagnostic pages.
