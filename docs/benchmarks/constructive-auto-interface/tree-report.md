# Automatic capacity-aware split/merge interfaces

## Outcome

The former four-consumer `port-pressure.json` request now produces a complete local witness.
Supplier A exports 1/s through two 1/2/s ports; B exports 1/2/s through one port. Five splitters
distribute those three roots to eight consumer ingredient terminals. Six preplanned bridges
resolve the selected leaf ordering. No facility, recipe utilization or commodity margin changes.

`merge.json` demonstrates two 1/4/s A suppliers merging into one 1/2/s consumer input; the B
ingredient keeps its own input. `combined.json` uses three A suppliers, one B supplier and four
consumers. The same constructor selects splitters, convergers and bridges without input geometry.

| Request | Facilities | Source / target ports | Splitters | Convergers | Bridges | Used bounds | Fresh construction ms | Warm median ms |
| --- | ---: | --- | ---: | ---: | ---: | --- | ---: | ---: |
| Merge | 4 | 3 / 2 | 0 | 1 | 0 | 31 x 15 | 14.70 | 1.06 |
| Former port-pressure request | 6 | 3 / 8 | 5 | 0 | 6 | 72 x 32 | 22.71 | 4.27 |
| Combined split/merge/cross | 8 | 4 / 8 | 6 | 2 | 6 | 73 x 32 | 32.88 | 8.19 |
| Eight consumers | 10 | 3 / 16 | 13 | 0 | 28 | 212 x 64 | 68.58 | 23.57 |
| Four replicated groups | 16 | 12 / 16 | 4 | 0 | 4 | 77 x 64 | 89.59 | 41.55 |

All rows have zero pending recipe terminals after independent optimized Rust validation. These
are local belt interfaces with external boundary inputs/outputs, not the full heavy-xiranite graph.
The unchanged five-facility full-rate reference still uses 20 x 16, with no logistics components.

## Contract and implementation

Allocation now splits flow at both source and target capacity-bucket boundaries. A bucket is
specific to an instance, item and direction, and occupies one facility port. Several fragments
can share either endpoint bucket. Root flow equals the exact sum of its leaves and is bounded by
transport capacity. Stable same-item order is retained through the permutation stage.

Each multi-leaf bucket is a fixed five-column comb: root on the first leaf row, trunk at column
two, leaf pitch three. The split and merge forms are mirrors with reversed flow. Every junction
uses two active branch arms and a runtime-catalog component/rotation. Different items never share
a root. A single leaf needs no component. The lowerer emits each facility terminal once and sums
fragment rates on common trunk segments; it also supports multiple external capacity lanes.

This is a hard internal cutover: the automatic engine no longer invokes `buildDedicated`, nor
does it construct an exclusive-port fixture adapter. That lowerer remains only in the historical
fixed-fixture research. A single bucket/fragment contract now covers direct, split and merged
connections. Detail output is explicitly schema v2: `port_groups`, group references in bindings,
tree reservations and permutation origin replace `generated_spec` and per-pair port fields.
Request v1 and witness v1 are unchanged. The obsolete exclusive-port failure renderer was removed;
its historical HTML/PNG remains on the existing server and its source is recoverable from Git.

The grouping policy currently minimizes port count, not final footprint. This is an efficacy
experiment, not an assertion that every optional merge improves quality. For example, the original
four-facility interface grows from width 29 to 38 when B's two outputs share a splitter root. Three
consumers can also cause a capacity boundary to bisect a pair demand, introducing a merge that
separate partially filled ports could avoid. A later quality choice must compare validated
bucket partitions using the area-first production score, rather than treating fuller belts as
an unconditional improvement. No production optimizer behavior was changed.

## Verification

The retained `tree-matrix.json` contains 40 passing expectations:

- 12 complete local recipe-interface witnesses, including another recipe family, renamed IDs,
  split-only, merge-only, combined and replicated cases through 16 facilities;
- three independent negative controls: removing only bridges, only splitters or only convergers
  makes the unchanged Rust validator reject the candidate;
- one expected materialization-budget stop for 24 consumers;
- 24 boundary component probes: belt and pipe, split and merge, 1/2/3/4/8/12 leaves. These use
  an unconnected catalog anchor required by witness v1. Its recipe remains entirely pending:
  they validate component geometry only, not complete factories or pipe recipe integration.

Twenty-four Node tests pass across automatic interfaces, explicit fixtures and crossing modules.
Tests cover exact bucket conservation/capacity, the resolved port limit, catalog/overload rejection,
both comb orientations/layers through 12 leaves, deterministic request-array permutations and
3,136 disjoint ordered-approach endpoint pairs. A regression initially exposed input-array order
leaking into witness serialization; lowering now uses the selected bank order consistently.

`cargo check --workspace` passed; all 524 workspace tests passed with two existing ignores. No
Rust edits or new dependencies. Actual split, merge, combined and eight-consumer diagrams were
inspected, with six width/theme checks per page. Validation uses the existing release executable.

The matrix records runtime catalog, executable, source and witness hashes. Fresh construction is
one child-process observation; warm medians use 20 in-process samples after one discarded run.
Both exclude catalog IO, serialization, process startup, rendering and independent validation.
This is a development-machine measurement, not a CPU-isolated benchmark. Per-stage timings are
retained: eight consumers spent about 14.94 ms in order search, 29.51 ms in geometry and 17.63 ms
in lowering; 16 replicated facilities spent about 65.96 ms in order search. No Rust build time is
included in these numbers.

## Next observed bottleneck

Visual inspection shows that expanding every splitter before crossing and postponing every merge
until after crossing creates an excessively wide leaf permutation. Four consumers select six
bridges; eight select 28. With 24 consumers, allocation still needs only three source ports, but
the central interface has 48 fragments and 276 inversions. The permutation materializer's
conservative estimate is 48 * 276 * 16 = 211,968 path cells, exceeding its 200,000 diagnostic guard.
It stops with `interconnect-cell-budget` before emitting a physical candidate. This is an observed
resource-bound case, not the smallest failing size, game limit, or infeasibility proof.

Those inversion counts describe **this expanded leaf ordering**, not a lower bound on arbitrary
shared-tree geometry. The next experiment should consider where to split/merge relative to the
crossing region: keep compatible roots shared through crossings when useful, branch nearer the
consumers, and partition independent interaction groups. Continue using the proven fixed modules;
change their composition, not the validation semantics or resource guard. Capacity-aware partial
port filling should remain an explicit alternative when it avoids unnecessary fragments.

## Reproduction and visuals

Use `run.mjs` as documented in `report.md`, selecting `port-pressure.json`, `merge.json` or
`combined.json` and a new output directory. `audit.mjs` takes the same catalog, validator and
output-directory flags without `--request`. Detailed JSON and dedicated subprocess logs stay on
disk; stdout contains a short completion summary. `tree-matrix.json` is the current retained audit;
`matrix.json` remains the historical exclusive-port baseline.

Existing tailnet pages on port 8765: `bound-interconnect-trees-split.html`,
`bound-interconnect-trees-merge.html`, `bound-interconnect-trees-combined.html`, and
`bound-interconnect-trees-eight-consumers.html`. The HTML files visualize actual validator inputs,
not illustrative or hand-adjusted placement. S/M/X labels denote splitter/converger/bridge.

Policy comparison: this remains isolated JavaScript research, consistent with the user's deferred
Rust migration and AGENTS research boundary. The accepted 2026-09-02 connected transactional
production construction, pipe-first production routing, area-first score and independent game
validation semantics remain unchanged.
