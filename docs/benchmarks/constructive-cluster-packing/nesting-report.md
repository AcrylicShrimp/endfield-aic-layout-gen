# Occupied-geometry nesting

Date: 2026-09-06. Status: verified complete-factory improvement; full goal remains active.

## Outcome

The selected original heavy-xiranite factory is 125 x 86: area 10,750, 2,292 transport tiles and
245 turns. All 59 original facilities, 131 recipe terminal margins and seven original external
obligations pass independent validation. Area decreases 6.7% from the 11,524-area shelf incumbent.
Transport tiles increase from 2,120 and turns from 243; this is an improvement under the required
lexicographic area-first score. Fifteen components remain, but component count is diagnostic only.
No minimum-area claim is made. Complete source artifacts remain available during every attempt.

## Method and scope

Keep both concrete connected clusters, including every internal route, unchanged. Enumerate relative
integer translations using their actual occupied cells rather than disjoint rectangular envelopes.
Each cluster is indexed as BigInt row masks for facility bodies, belts and pipes. Reject body/body,
body/transport and same-layer collisions; belt/pipe overlap is permitted. The retained structured
builders use catalog-checked one-cell logistics components, represented in the appropriate layer.

The actual 39 x 43 and 80 x 85 regions give 15,982 translations in the recorded finite contact domain;
3,486 are collision-free. Shortlist 12 low-area offsets with four-cell Manhattan separation. This is
a bounded diverse portfolio, not exhaustive full-factory optimization. Whole-region translation,
mixed closure and independent geometry/original-graph coverage gates are shared with shelf packing.
Two closure workers each try simple and perimeter corridors. Only a complete independently rechecked
result can displace the complete incumbent. Production construction and Rust code are unchanged.

The winning relative offset is (31, -40). Its partial 53-facility envelope is 111 x 85, area 9,435;
mixed closure and the real external obligations expand it to 125 x 86. The partial score is never
treated as a complete factory score. One of 24 closure attempts completes; the other attempts remain
explicit bounded local failures. This fragility is an important limitation, not a success-rate claim.

## Output access and visual diagnosis

The lowest-area partial packing is 99 x 87, area 8,613, at offset (19, -44). It stops at 55 facilities.
Personally inspected its full rendering, the final complete rendering and a close-up of the blocked
interface. The first failed edge is F14 -> F09, carrying enriched carbon. The initial visual suspicion
about F15 being sealed by F36 was incomplete: F15 has a free adjacent column and can escape to the
outer free region. It is not the first failing supplier. Trace exact edges before attributing failure.

A read-only four-neighbor flood of the original packed witness, excluding facility bodies and belt
occupancy/components, reaches only eight cells from F14's output connections:
`(39..42, 54..55)`. It cannot escape. F09 requires a 5 x 5 body, which cannot fit into this eight-cell
pocket. This explains the fixed-interior, no-new-crossing corridor failure geometrically. It is not
global infeasibility: moving facilities or planning a legal crossing interface could change it.
The close-up highlights these eight actual free cells; it is a diagnostic overlay, not a new route.

Added a narrower necessary-condition screen for available output connection cells. Derive pending
edges from the original graph, count ceil(rate/capacity) unused ports per edge, and check catalog-
projected connection cells against both translated regions. This matches the current separate-edge
closure, not future shared trunks. It removes 126 translations, leaving 3,360. The selected complete
geometry is unchanged. The eight-cell pocket still passes this screen, correctly demonstrating that
free ports do not establish reachability or room for the consumer. No performance win is claimed.

Next experiment: account for trapped output regions and consumer footprint before spending closure
trials, and reserve placement-level escape/crossing structures where required. Then refine offsets
near successful complete layouts and compare narrower interfaces within the large cluster. Large
empty areas remain visible above the small cluster and inside the large cluster's long approach
routes. Final compactness, broader recipe testing, cleanup and the real web workflow are unfinished.

## Timing and verification

The verified run took 42.71 s for cluster reconstruction, 12 packed candidates and 24 complete closure
attempts, including repeated validation and artifact IO while workspace verification also ran.
Translation enumeration plus port screening took 67.63 ms; full packing construction took 138.29 ms.
Earlier full runs took 32.41 s without the screen and 34.92 s with it. These are optimization-portfolio
wall times, not release single-factory generation benchmarks or a controlled speed comparison.
Detailed logs and candidate geometry remain in target directories; stdout contains a short summary.

- 51 related Node tests pass, including 5,625 sampled offset comparisons against an independent
  cell-pair collision oracle, negative translations/origins, independent layers, exact per-edge lane
  counts, used/blocked ports, order independence, unchanged membership and detached candidates.
- `cargo check --workspace` passes. `cargo test --workspace`: 524 pass, two existing ignores.
- Eight independent complete-factory audit expectations pass, including omitted exports, undeclared
  externals, broken pipe geometry, duplicated facilities, wrong utilization and corrupt reports.
- Browser checks pass for full/pocket renders, zoom, drag, Fit and six theme/viewport combinations.
- Compared against the constructive design and AGENTS policy: complete incumbent retention,
  catalog semantics, graph coverage and area-first ranking remain intact. No production fallback,
  fixed game canvas, component-count penalty or global infeasibility claim is introduced.

Evidence: `nesting-matrix.json`, `nesting-unscreened-matrix.json`, `nesting-controls.json`.
Interactive result: `http://100.114.24.86:8765/bound-interconnect-closure-nested.html`.
Obstruction: `http://100.114.24.86:8765/bound-interconnect-closure-nesting-obstruction.html`.

## Reproduction

```sh
node docs/benchmarks/constructive-cluster-packing/run.mjs \
  --graph artifacts/benchmarks/constructive-chain-blocks/gap-1-placement.json \
  --blocks artifacts/benchmarks/constructive-cycle-blocks/matrix.json \
  --incumbent artifacts/benchmarks/constructive-cluster-packing/matrix.json \
  --packing-family nesting --nesting-limit 12 \
  --recipes data/game/normalized/recipes.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --validator target/release/aic-witness-probe \
  --output-directory target/cluster-nesting-reproduction
```

Use a fresh output directory. The source-block details referenced by the retained cycle matrix and
the complete incumbent must exist; these research reports are not standalone blueprint archives.
