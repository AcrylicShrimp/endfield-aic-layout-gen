# Complete Heavy Xiranite factory

Date: 2026-09-05. Result: complete under the current runtime catalog and validation contract.
Implementation commit: `a24a120`; source plan and normalized runtime catalogs are unchanged at that
revision. No game-data edits are part of this slice.

## Outcome

The unchanged Heavy Xiranite source plan now constructs **all 59 facilities, 89 internal requirements
and seven external requirements**, with no unresolved requirement. The production CLI returns
`status=complete`, `internal_complete=true`, `complete=true`, `success=true`, and exit 0.

The initial factory has a natural **165 x 140** footprint (area 23,100), 3,820 physical belt tiles,
445 physical pipe tiles, 167 bridges, 19 splitters and 19 convergers. Sixteen compatible commodity
networks carry the requested flows. There are 40 facility/SCC growth transactions followed by seven
external-connection transactions. This is a complete initial layout, not an optimized layout.

The measured construction uses 178 A* calls and 162 placement attempts. A full pre-display-correction
run took 21.688 seconds in the planner and 31.06 seconds CLI wall time (24.98 user / 5.55 system).
An earlier full run also completed in 25.262 seconds planner / 35.13 seconds CLI wall time. These
are host-local observations; compiler/test activity overlapped some runs. The stored summary records
the final run's planner timing and exact counters. Rendering and report serialization are included
in CLI time, not planner time. The final stored run took **23.569 seconds planner / 33.04 seconds CLI
wall time** (24.73 user / 5.60 system), with identical final geometry and operation counts.

Host: macOS 26.6 arm64, Rust stable `1.98.0 (88d9e12ae 2026-08-18)`, release profile.

## What closed the obstruction

The previous nine-facility checkpoint used a bridge-free router and exterior flood fill. A moss
return route could enclose a still-required seed input, so the three-facility SCC was discarded.
The accepted semantic model and runtime catalog already permit independent-axis logistics bridges.

The dedicated router now traverses an eligible existing straight channel only on its perpendicular
axis, emits the actual catalog bridge, and keeps both flows separate. It cannot turn, terminate,
merge or branch at the crossing. Existing terminals and branch components are not bridge sites.
The future-boundary screen uses the same legal crossing capability rather than treating every
same-layer transport cell as an absolute wall.

The independent validator checks both axes, direction, per-axis capacity, physical component
identity/rotation, ordinary-cell flow conservation and branch/convergence topology. It rejects
unselected crossings, single-axis bridges, overloads and invalid components. The implementation
currently crosses different commodity networks only; this avoids mixing axes during shared-path
attachment traversal inside a normalized same-item network.

That capability is enough for the full internal graph. No accepted factory region is rolled back,
no input recipe is replaced by an import, no facility is omitted, and no global exact fallback or
partial-area optimization is used. Local placement/port attempts remain first-feasible transactions.
The constructor became simpler to reason about: a geometrically enclosed port is not a dead end
when a catalog-valid crossing can reach it.

## External completion and independent coverage

After internal completion, the constructor connects the seven original import/target/surplus
requirements to physical endpoints on an envelope derived from the current geometry. It splits
rates into available port/line residuals, supports shared attachments, and validates each complete
external-edge transaction before retaining it. Budget failure preserves the previous valid node.

External endpoints are ordinary occupied belt/pipe cells. Their outward directions and coordinates
must lie on the **actual used bounding box**, not an unused caller canvas. Their node role and item
must match the original wiring. No external-to-external edge or reversed import/output role is
accepted. Rotating a complete node also rotates its external sides.

An independent Rust coverage audit compares physical terminal totals with the original resolved
logical requirements using exact rational rates. It checks missing induced internal edges, unknown
or duplicate resolved IDs and internal/external classification. A second capture-time audit uses
JavaScript `BigInt` rational arithmetic against a freshly generated original wiring graph and
confirms all facility IDs, all 96 edge IDs and every endpoint/item/direction aggregate rate.
That second audit supplements, rather than replaces, the independent Rust geometry/flow validator.

## Diagnostics, rendering and artifacts

Production request schema remains 1; report schema is now 2. Steps identify `internal-growth` or
`external-connection`. Constructive nodes add `external_requirements`, while `internal_requirements`
continues to mean facility-to-facility edges. Complete success includes external I/O; exhausted
construction remains structured failure, never global infeasibility.

The full native history is large: roughly 92 MiB JSON and 39 MiB HTML. Report-file serialization now
uses a checked buffered writer; an earlier unbuffered internal-only run spent about 195 seconds of
system time writing its report. This I/O correction does not change layout search or semantics.

Artifacts in this directory:

- `summary.json`: compact per-step counters, final geometry, coverage audit and raw artifact hashes;
- `complete-report.json.gz`: lossless native report with every accepted node and all diagnostics;
- `complete-history.html.gz`: lossless self-contained interactive HTML with 47 pages;
- `capture.mjs`: reproducible compression, metrics and independent original-wiring rate audit;
- `layout-final.png`: overview captured from the final native HTML page.

HTML bridge inspection now exposes both independent channels at one physical cell. Physical tile
totals count `(transport layer, x, y)` once, so bridges are not double-counted, while belt/pipe
co-location still counts one tile on each layer. This display correction was discovered by comparing
the browser's header with the independent capture metrics; it does not change generated geometry.

## Reproduction

Run from the workspace root:

```bash
cargo build --release -p aic-cli
/usr/bin/time -p target/release/aic-cli layouts construct-production \
  --recipes data/game/normalized/recipes.json \
  --source-plan data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  --facility-catalog data/game/normalized/facilities.json \
  --item-catalog data/game/normalized/items.json \
  --transport-catalog data/game/normalized/transports.json \
  --logistics-component-catalog data/game/normalized/logistics-components.json \
  --target-instance 'facility-instance:recipe-occurrence:/target:0' \
  --max-steps 256 --max-placement-attempts 256 --max-route-searches 4096 \
  --visualization-output /tmp/aic-complete.html \
  --report-output /tmp/aic-complete.json > /dev/null
target/release/aic-cli recipes contextual-instance-wiring \
  --file data/game/normalized/recipes.json \
  --request data/examples/source-plan.game-heavy-xiranite-forge.request.json \
  > /tmp/aic-complete-wiring.json
node docs/benchmarks/constructive-complete-graph/capture.mjs \
  /tmp/aic-complete.json /tmp/aic-complete.html /tmp/aic-complete-wiring.json \
  /tmp/aic-complete-capture
```

To inspect the stored HTML without rerunning construction:

```bash
gzip -dc docs/benchmarks/constructive-complete-graph/complete-history.html.gz > /tmp/aic-complete.html
open /tmp/aic-complete.html
```

There are no W/H inputs. The 256/256/4096 budgets and dense workspace guard are implementation
resource controls, not game geometry limits. No budget was enlarged to manufacture this result.

## Verification and remaining work

Formatting, workspace check and workspace tests pass: **464 passed, one pre-existing ignored**.
Regressions cover perpendicular crossing,
invalid/missing/overloaded bridges, crossing turns/endpoints, complete external input/output,
external lane-budget atomicity, rotated external sides, original-rate mismatch, invalid endpoint
roles, physical tile de-duplication and the earlier chain/shared-capacity/SCC cases.

Chromium loaded the native 47-page history, exercised navigation and showed `CONSTRUCTED`. The final
page contains 59 facilities and 89 internal requirements; all seven external connections are present
in the report. Both bridge-channel inspection records are available at each of the 167 crossings.

The next work is **transactional improvement of this complete valid factory**, preserving it unless
a validated candidate improves area, then physical transport tiles, then turns. General
rip-up-and-reroute, accepted-history rollback and same-commodity crossings are still unsupported
capabilities, but they were not required to complete this request. Other requests may still exhaust
constructive budgets. This result is not an optimality or global feasibility theorem.
