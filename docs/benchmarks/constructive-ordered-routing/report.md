# Frozen wiring in placement and nearest-port order

## Result

The same 59 pipe-north facility placements and rotations remain unchanged. **55/89 internal
requirements connect**, including both pipes; 34 internal and seven external requirements remain
pending. Covered routes independently validate, but this is **not a complete factory**.

| Measurement | Previous straight-first baseline | Placement-ordered probe |
| --- | ---: | ---: |
| Connected internal requirements | 60/89 | 55/89 |
| Straight / other successes | 30 / 30 | 30 / 25 |
| Probe median, three release runs | 9,349.14 ms | 3,247.98 ms |
| A* calls | 832 | 640 |
| Future-domain candidate rejections | 526 | 405 |
| Invalid-witness candidate rejections | 0 | 0 |
| Summed network cells | 746 | 528 |

The faster run and lower cell count do not establish improved routing quality: five fewer
requirements connect. This compares two diagnostic policies, not an isolated same-work speedup.
The old two-stage budget was 64 A* calls per stage/connection; the new policy uses 64 total per
connection. Preserve that budget difference when interpreting results.

## Ordering contract and hard cutover

An internal requirement is eligible at the larger of its two endpoint placement indices. Sort by
that index, then minimum compatible projected connection-cell Manhattan distance, then belt before
pipe only on equal distance, then stable requirement ID. A graph-wide transport or straight sweep
no longer preempts earlier placements. For each requirement, refresh available attachment domains
and sort actual port pairs by Manhattan distance and stable port IDs. Finish its bounded attempt
before proceeding; a farther straight pair never preempts a closer detour pair.

The first connections are F01 -> F00, F02 -> F00, F03 -> F00 and F04 -> F01, at placement indices
1, 2, 3 and 4 respectively. The first and fourth are pipes: placement order takes precedence over
the former whole-graph belt-first rule, as requested.

Report schema **v2** adds `connection_order`, per-attempt `placement_index`,
`nearest_port_distance` and `port_trials` (port IDs, distance, result and A* count). It replaces
`per_stage_connection_astar_limit` with `per_connection_astar_limit`. There is one report attempt
per internal requirement. The retained `stage` strings classify successful pairs; they no longer
describe separate sweeps. The renderer requires v2; historical v1 reports and their already-published
HTML remain unchanged. Re-rendering v1 requires the previous revision's renderer.

Connection ranking uses initial projected port distances, while actual trials use currently
available ports. Consequently the ranking lower bound may be smaller than the first actual trial.
First-valid route acceptance remains: nearest endpoints do not guarantee a shortest physical path
or the best shared-network attachment. No route engine, placement, rip-up or relocation change is
included. Removing nested graph-wide sweeps leaves a single ordered connection loop and one budget.

## First failure and next decision

The first failure is **F15 -> F09**, placement index 15, ranking distance 2. By then available port
pairs have distances **3, 4, 4, 5, 5, 5, 6, 6, 6, 7, 7, 8**. All twelve A* calls produce candidates
that pass witness validation but remove the last pending attachment option for **F20 -> F14**.
They are rolled back. These are twelve rejected candidates, not proof that every possible path
for those endpoints is impossible.

Future-domain preservation is still checked after path generation. The next research decision is
to incorporate preservation of the last remaining approach into path search, or retry bounded
alternative paths for the same nearby ports. Do not reserve every unused alternative port as an
immutable corridor. Nearest ordering alone does not solve preservation or shared-network routing.

The failure picture overlays the failed logical F15 -> F09 requirement on the **final retained
witness**, not the state at the time of rejection and not a rejected physical route. Subsequent
connections continue, including F20 -> F14. Gold geometry is belt, blue is pipe, and red dashed
geometry is only the selected failed logical connection.

## Measurement and verification

Three fresh release processes on warm files, with no concurrent compilation or tests, take
3,245.35 / 3,247.98 / 3,258.54 ms inside the probe. CLI wall times are separately recorded in
`timing.json`; builds, placement, catalog loading and JSON serialization are not part of probe time.
All three final witnesses hash to
`510bc217db1535913908a0eb1b607cec8d461d71a67a82212c82e6b9236e0a3f`.
`routing.json` preserves run zero, `summary.json` its counters and first-failure trace, and
`timing.json` the release binary hash and timings. Detailed output goes to files, not stdout.

- `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo test --workspace` pass.
  517 Rust tests pass with two pre-existing ignores. Eight probe regressions include three new
  tests for shuffled requirement order, earlier detours before later straight routes and actual
  ascending port-pair distances independent of catalog cross-product order.
- Eleven Node placement tests pass. Every attempt's placement/neighbor ordering and port trial
  ordering is audited, with recorded trial counts matching the traces.
- All 59 coordinates equal the input plus the explicit 16-cell translation, with identical rotations.
  Search canvas 84 x 130 is only a diagnostic resource envelope, not the used factory footprint.
- Actual browser rendering passes at widths 1400, 736 and 360 in light and dark themes, with no
  horizontal page overflow. The failure selector has 34 failures plus the whole-layout entry;
  status, first-failure details, port display and retained geometry are checked.

Build latency is separate: release recompilation took about 2m49s, test build about 1m59s, and the
main library test execution about 39s. The repository contains approximately 111,609 Rust source
lines including research and tests. These observations do not isolate compiler phases or prove
application runtime size. The user deferred test removal/build restructuring; neither was done.

## Scope and reproduction

Compared against `docs/designs/2026-09-02.00-constructive-layout-planner.md` and AGENTS policy:
this remains the user's explicitly approved frozen-placement diagnostic, separate from production
connected growth. Production commands, independent network validation, future-domain guards,
runtime catalogs and factory-success semantics are unchanged. No exact solver is invoked and no
global infeasibility or complete-factory claim is made.

```sh
cargo build --release -p aic-cli --bin aic-fixed-routing-probe
target/release/aic-fixed-routing-probe \
  --placement docs/benchmarks/constructive-pipe-north/heavy.json \
  --facilities data/game/normalized/facilities.json \
  --items data/game/normalized/items.json \
  --transports data/game/normalized/transports.json \
  --components data/game/normalized/logistics-components.json \
  --report-output NEW_ROUTING_JSON
node docs/benchmarks/constructive-placement-only/route-view.mjs \
  docs/benchmarks/constructive-pipe-north/heavy.json NEW_ROUTING_JSON NEW_HTML
```

Output paths must be new. Exit zero means the diagnostic completed, not that the factory is complete.
The unchanged tailnet server publishes `http://100.114.24.86:8765/fixed-routing-ordered.html`.
Browser-rendered images are saved in `target/shared-results/fixed-routing-ordered.png` and
`target/shared-results/fixed-routing-ordered-failure.png`. Previous pages are preserved.
