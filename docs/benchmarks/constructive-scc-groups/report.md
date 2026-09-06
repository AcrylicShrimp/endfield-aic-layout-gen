# Actual SCC seeds and bounded stop/grow portfolios

Status: source-region preparation verified. **Not a new complete factory.** The complete
incumbent remains 104x31 / **3,224 area, 775 transport cells, 119 turns**. The user-proposed
whole-factory density partition is still an open experiment, not proven by these results.

## What changed

Historical source blocks had already absorbed leaves. They were not minimal cycles after
compatible allocation changes. The new runner discovers SCCs on the actual current graph,
extracts their exact internal flows from validated source geometry, and retains the stop
state plus up to two atomic extensions. Every selected state has immediately routed inputs,
independent physical validation, exact recipe/pending margins and pending-output access.
Extensions may use all runtime-allowed orientations; the existing direct-attachment east
policy remains unchanged for older callers. No Rust migration, new dependency or global
exact-solver fallback is involved.

This is simpler than interpreting a historical four-facility block as a minimal cycle:
mandatory cyclic membership, optional extensions and boundary obligations are explicit.
Current extension candidates are deliberately limited to the same retained parent region.
An extension is a whole SCC, not necessarily one facility. A dependent two-facility cycle
cannot be added one member at a time while satisfying all inputs.

## Allocation reconciliation before graph cuts

The first projection of the old five-member core failed: the abstract internal source
margins required 2/3 item/s of plant-moss-3, but its retained physical distribution could
deliver only 4/9 item/s under those particular retained per-source demands. The complete
commodity network was valid: every source's total production and sink's total consumption
were correct, but source-to-sink pairing differed from the stored edge decomposition.

Decompose positive concrete segment flows into explicit source/sink contributions, then
apply the existing checked graph-delta contract. Every producer and consumer item margin
is preserved exactly. This does not delete demand or duplicate a shared supplier. Only
compatible allocation changes are allowed; bridged channel ambiguity, external terminals,
multiple retained networks per item and residual circulation are outside this first family.
These are scoped diagnostic limitations, not infeasibility claims.

After reconciliation there are nine cyclic SCCs, eight of them source SCCs. Seven source
cycles have two members each. The historical larger cycle splits into source F32/F45 and
dependent F33/F46, with F38 downstream. SCC membership depends on the selected compatible
allocation; it is not an immutable property of the recipe names.

Projection uses rational capacity-bounded max flow on existing directed segments. It can
reduce flow/remove unused geometry, never add a path or increase a retained segment rate.
Independent validation remains authoritative, including component arms after flow removal.

## Measurements

The final run generates 4,113 bounded candidates and independently validates 69 selected
stop/grow variants across eight source banks. All 69 pass coverage, geometry and pending
interface checks. Each bank keeps its seed even when a larger candidate is retained.

Representative best-by-area state at each depth for all seven small source banks:

| Covered facilities | Bounds | Area | Transport cells | Facility fill | Occupied fill |
| --- | --- | --- | --- | --- | --- |
| 2: actual source cycle | 10x7 | 70 | 4 | 71.43% | 77.14% |
| 3: one extension | 15x7 | 105 | 5 | 71.43% | 76.19% |
| 4: two extensions | 18x7 | 126 | 6 | 66.67% | 71.43% |

The four-member state saves one transport cell versus the previous 126-area east-facing
processor attachment. It uses a catalog-valid shared corner connection and a different
processor orientation. Its irregular silhouette remains visible; this is not evidence
that choosing this state independently for every bank gives the best assembled factory.

The larger retained geometry remains poor:

| Covered facilities | Area | Transport cells | Facility fill | Occupied fill |
| --- | --- | --- | --- | --- |
| 2: F32/F45 source cycle | 232 | 52 | 21.55% | 43.97% |
| 4: add dependent F33/F46 atom | 528 | 189 | 18.94% | 54.73% |
| 5: add F38 | 561 | 190 | 22.28% | 56.15% |

Actual native renders were inspected. The five-member state visibly preserves three long
return corridors over the facilities. It is not a good compact result. In particular,
occupied fill rises from 43.97% to 54.73% while facility fill falls from 21.55% to 18.94%:
counting wire cells in the density numerator can reward the very detours being removed.

Use facility-only fill as a candidate-growth signal and report occupied fill separately.
The denominator and final objective must still include all wiring and components. Even a
well-filled rectangle may have poor exposed ports; boundary access and whole assembled
area must decide whether an extension helps. No fixed N% rule or final partition is
selected here. Do not maximize fill or prefer square aspect ratio over actual total area.

Single observed final wall time: **5.935s** for the whole eight-bank portfolio, including
all report/HTML output and validation. Candidate generation/selection: 4.020s; seed flow
projection: 0.009s; interface checks: 0.085s; HTML rendering/writing: 0.135s. Summed
concurrent validator task durations: 3.370s, not additive with wall time. The earlier
interface-control run under concurrent test load took 9.475s and retained exactly the
same 69 witness hashes. These are portfolio measurements, not a single-layout benchmark.

## Verification

- 119 related Node tests pass, including five new SCC/projection/interface tests.
- Rational flow is compared with exhaustive cuts for 729 capacity assignments, plus
  fractional bottlenecks, capacity/conservation and immutability assertions.
- Tests cover source versus dependent SCCs, external dependencies, exact positive-flow
  decomposition, missing paths/circulation, shared terminal cells, real catalog-based
  source extraction, open-input rejection, bridge rejection and all-orientation growth.
- Detached output access permits negative local connection cells outside normalized
  nonnegative geometry, because a source region can be translated. It still rejects
  occupied same-layer connections and malformed negative occupied geometry. All retained
  owners receive the existing exact-capacity and closed-pocket footprint checks.
- `cargo check --workspace` passes. `cargo test --workspace`: 524 pass, zero failures,
  two existing ignores. No Rust changes.
- The complete 3,224-area incumbent passes all eight independent audit controls against
  the emitted allocation-aligned graph. Its original-graph audit remains in
  `../constructive-factory-closure/output-space-retained-audit.json`. No accepted factory
  was replaced by a partial region or by a changed-rate request.
- Actual source, small grown and large dependent-cycle renders inspected; three browser
  runs each pass six viewport/theme checks. These pages explicitly identify pending
  recipe terminals and say they are not the complete production graph.
- Compared with `AGENTS.md` and the accepted constructive-planner design: this isolated
  research slice grows already routed private regions, preserves exact external/pending
  obligations, validates before acceptance, introduces no factory W/H limit and does
  not change production APIs or objective order.

## Reproduction and evidence

Requires the retained reciprocal attachment inventory from its documented prior milestone
and the existing release witness validator. Run from the repository root with a fresh
output directory:

```sh
node docs/benchmarks/constructive-scc-groups/run.mjs \
  --graph target/reciprocal-direct-attached/graph.json \
  --blocks target/reciprocal-direct-attached/blocks.json \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --growth-depth 2 --beam-width 4 \
  --output-directory target/scc-stop-grow-replay
```

`regions.json` records the final portfolio, checked allocation deltas, provenance and
source hashes. `incumbent-audit.json` records all eight complete-factory audit controls.
The emitted graph and per-variant witness/detail/coverage/interface/validator files live
in the recorded run directory; no verbose stdout replay is needed.

Rendered pages in the existing Tailscale server:

- `bound-interconnect-scc-seed.html`: two-member source cycle.
- `bound-interconnect-scc-grown.html`: four-member compact extension.
- `bound-interconnect-scc-shared.html`: still-poor five-member retained geometry.

## Next gate, not yet implemented

The old packer expects identical members in every alternative, east-facing output rays,
and a later constructor organized around two large clusters. Those contracts cannot
honestly test this new portfolio: stop/grow alternatives cover different facility sets
and may expose outputs on any side. Singleton-only closure also cannot introduce the
new dependent F33/F46 SCC after choosing its smaller upstream stop state.

Implement variable-membership, disjoint-ownership source-bank assembly with all-side
pending interfaces and atomic dependent-cycle closure. Retain multiple density/envelope
choices; compare only complete validated factories against area 3,224. Also reconstruct
the poor larger seed geometry rather than mistaking inherited loops for an optimum.
This is unfinished implementation, not a user decision or an external blocker. Recipe
breadth, satisfactory compactness and later product-goal stages remain open.
