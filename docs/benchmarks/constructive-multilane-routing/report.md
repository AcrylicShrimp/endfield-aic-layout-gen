# Atomic multi-lane nearest-port routing

## Result

The same frozen 59-facility supplier-group proposal connects **64/89 internal requirements**,
up from 58/89. All six previously failing 1-item/s belt requirements now use two distinct
0.5-item/s lanes, each succeeding in two search calls with no future-domain rejection. F14/F15
remain directly connected to F09. Both pipes connect. Twenty-five internal and seven external
requirements remain pending; the covered witness validates, not the complete factory.

Three fresh release runs take 2,796.00 / 2,788.07 / 2,776.99 ms: median **2,788.07 ms**.
There are 526 A* calls, 348 future-domain rejections, zero invalid-witness rejections and 345
summed network cells. Fifty-one requirements are classified straight, including multi-lane
straight bundles; this classification now uses every accepted lane's actual path. Geometry,
runtime catalogs and the 64-call requirement budget are unchanged. No build/test load overlaps
measurements; input and binary hashes and identical witness hashes are in `timing.json`.

## Cutover

The probe no longer strips domains to one port pair and invokes the full-demand engine repeatedly.
It supplies the current requirement's complete alternative domains to `connect_nearest`, which
uses the existing network materialization, capacity, recursion, crossing and validation machinery.
Each recursive lane selects currently available pairs by endpoint Manhattan distance and stable
port IDs. Alternative candidates are not simultaneous escape-corridor reservations.

The existing production `connect_first` retains assigned-port corridor behavior and original
iteration order. Both entry points share one search/materialization implementation. A failed
second lane, budget exhaustion or rejected complete bundle never commits a partial network.
Post-hoc future-domain checks still protect all pending internal and external requirements.

Report **v3** replaces the old outer port-pair trace with one entry per physical attachment search:
`lane`, `parent_trial`, source/target ports, distance, actual lane `rate`, `straight`, `connected`
and `astar_searches`. Connected trials form the accepted full-demand branch; failed branches leave
no geometry behind. Distance order is per parent search state, not globally across recursive lanes.
`port_pairs` now counts lane/attachment trials (possibly repeated IDs), matching A* count.
The renderer requires v3; historical reports and already-published pages remain intact.
Removing the probe's redundant single-pair loop and straight-line checker makes one engine
responsible for both lane selection and physical path classification.

## Verification and next obstruction

- `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace` pass:
  517 Rust tests and two existing ignores. Fourteen Node tests pass.
- The updated regression verifies two-lane straight success, distinct ports, accepted parent/child
  trace, rollback with a one-call budget and rollback when three lanes exceed two-port capacity.
- All 59 relative placements/rotations, per-state nearest ordering, summed accepted lane rates,
  trace/A* counts, connection budgets and identical final witnesses are audited on three CLI runs.
- Browser checks pass at 1400/736/360 px in both themes. Actual overview, two-lane F22 -> F16 and
  first-failure screenshots are saved under `target/shared-results/multilane-routing*.png`.

The first failure is now **F31 -> F22**, at placement index 31: ten searches produce ten
geometrically valid candidates that exhaust the last attachment domain for F34 -> F24,
F35 -> F24 or F44 -> F30. This is not infeasibility. Next: bounded alternative-path search
that preserves at least one currently available input approach, rather than only rejecting
the shortest path afterward. Do not reserve all unused candidates as immutable corridors.

## Reproduction

Use the command in `docs/benchmarks/constructive-supplier-group-routing/report.md` after rebuilding
`cargo build --release -p aic-cli --bin aic-fixed-routing-probe`, with a new report/output path.
`routing.json`, `summary.json` and `timing.json` preserve this milestone. The page is
`http://100.114.24.86:8765/multilane-routing.html`.

Compared with AGENTS and the accepted constructive-planner design, this remains the explicit
user-approved frozen-placement diagnostic. No global solver, placement fallback, catalog change,
validation weakening or full-factory success claim is introduced.
