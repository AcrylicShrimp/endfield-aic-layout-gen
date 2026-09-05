# Completion-first constructive cutover

Date: 2026-09-05. Status: implemented and measured; full-factory construction remains unfinished.
Implementation: `d09b327` (runtime catalogs and source-plan inputs are tracked at that revision).
Host: macOS 26.6 arm64; Rust stable `1.98.0 (88d9e12ae 2026-08-18)`; release profile.

## Decision and changed contract

The previous automatic path discovered compact immutable process modules, composed all candidates,
and selected by partial-layout score. This overcommitted geometry before future connections were
known. The replacement grows one facility or atomic strongly connected group, allocates ports from
all incident commodity demand, routes immediately, and accepts the first valid transaction.
Area/tile/turn priorities are unchanged for future improvement, but no partial-area incumbent or
candidate scoring participates in this constructor.

`layouts construct-production` / `construct_production` replace the removed automatic module command,
DTOs and implementation. The neutral geometry, network representation, routing engine, component
catalog, capacity checks and independent validator remain shared. Explicit manually selected module
diagnostics are not invoked by the new production path. Neither is global exact search.

This is the first implementation slice of
`../../designs/2026-09-05.01-completion-first-construction.md`, not the completed optimizer.

## Controlled results

Ten production regressions pass:

- deterministic chain construction with zero scored candidates;
- existing-producer reuse with one physical splitter and one commodity network;
- atomic three-facility cycle closure, including an already placed seed;
- route-budget exhaustion that preserves the seed and commits no partial cycle;
- honest step-limit status;
- internal completion that does not conceal unfinished external I/O;
- duplicate wiring identity and zero-budget rejection;
- catalog-backed separated two-planter / grinder fan-in preserving four total output-port domains;
- pipe-first growth despite the belt edge's earlier lexical identity;
- a new `2/5 item/s` demand split across two existing `1/5 item/s` port residuals, with validated
  real branching/merging components.

The moss fan-in is a controlled topology/rate fixture using the real facility and item catalogs. It
is not a completed moss production cycle or a recipe-throughput proof. Its result establishes that
four outgoing port domains need not collapse to the three distinct cells of the previous compact
module. It does not establish that generous spacing alone solves the full production graph.

## Full Heavy Xiranite experiment

Inputs are unchanged normalized runtime catalogs and
`data/examples/source-plan.game-heavy-xiranite-forge.request.json`. The graph requires **59 facilities,
89 internal requirements and 7 external requirements**. The seed is the final-product oven.

Budget: 256 growth transactions, 256 placement attempts per transaction, 4096 A* calls per transaction;
64 A* calls per fixed connection attempt; at most 1,000,000 dense routing-workspace cells. None is a
game limit. Four deterministic port/rotation variants and local-parent/exterior placement candidates
are constructive heuristics, not an exhaustive local feasibility proof.

| Measurement | Stored run |
| --- | ---: |
| Accepted expansions | 8 |
| Accepted facilities / internal requirements | 9 / 8 |
| Used footprint | 49 x 35 |
| Physical belt / pipe tiles | 125 / 39 |
| Commodity networks / logistics components | 7 / 0 |
| Accepted expansion elapsed sum | 31 ms |
| Accepted expansion A* calls | 10 |
| Failed transaction placement attempts | 95 |
| Failed transaction A* calls | 4096 |
| Completed lane bundles in failed transaction | 4039 |
| Boundary dead-end rejections | 3959 |
| Planner elapsed | 37.626 s |
| CLI wall / user / system time | 41.16 / 37.62 / 3.39 s |
| Result | growth-exhausted; complete=false; success=false |

Two preceding final-code runs produced the identical committed node and failure counters in 37.067 s
and 40.431 s planner time / 41.52 s and 44.02 s CLI wall time, with 31 ms and 30 ms across accepted
expansions. Those two runs used optional Korean HTML localization; the stored run uses the default
identifier labels to keep repository artifacts in English. Timing is host-local; workspace
verification ran concurrently with earlier runs, so these are observations, not controlled speedup claims.
The CLI total also includes input processing, JSON output and HTML generation.

The prior automatic module benchmark reached 28 facilities, whereas this first constructor reaches
9. Thus this experiment **does not demonstrate improved full-graph coverage**. It removes premature
compactness enumeration, but exposes an earlier cycle-routing limitation in the simpler growth order.
Do not compare the old module count directly with new single-facility transaction counts.

## Next measured obstruction

Only one three-facility SCC is attempted in the exhausted transaction: two moss planters and one
seed collector. Earlier accepted geometry remains unchanged, up to canonical translation.

The recorded rejected candidate has 12 facilities. While adding the planter-1 to seed-collector
`item-plant-moss-2` return (`wiring-edge:a2a0229a8f1ef3a02e2dd7e3708e0738d25f9e4c8dbc4470858708e296dba230`,
rate `1/4 item/s`), the seed-input boundary of planter-0 becomes inaccessible:

- unresolved requirement `wiring-edge:ae0aad6075f919004a83743c40d4b31bb17aed78dd3333522b333befccf54c61`;
- `item-plant-moss-seed-2`, rate `1/2 item/s`;
- planter-0 input `input-belt-2`, connection `(17,43)` in the canonical rejected witness;
- seed-collector output candidates `(-1,11)` and `(-1,10)` remain exterior-accessible.

The read-only flood-fill diagnostic confirms the planter input is disconnected from free exterior
space. Of the individual same-layer networks, omitting only `commodity:belt:item-plant-moss-2`
restores that input's reachability. Omitting a network is a diagnostic experiment, **not** a valid
layout or an approved fallback. The full rejected candidate is retained under `last_obstruction`,
separately from the last valid `final_node`.

The implementation protects immediate three-cell port escapes, but that does not prevent a longer
route from enclosing them. It retries placements and attachments, yet does not rip up earlier routes
inside the uncommitted SCC or jointly reserve that SCC's still-needed corridors. Fixed earlier local
choices are therefore the next construction issue to address, not partial-area optimization or a
proof that accepted factory history must be rolled back.

**Next bounded slice:** reproduce this three-facility SCC and retry its port/route order or rip up and
reroute its uncommitted moss/seed connections transactionally, keeping the nine accepted facilities
fixed. Require all SCC edges plus remaining boundary support before commit. Then rerun the whole
graph. Whether that suffices is unmeasured. Legal bridges, accepted-history rollback, an improvement
optimizer and physical external-I/O construction remain unimplemented in this new path.

## Reproduction and verification

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
  --visualization-output /tmp/aic-completion-first.html \
  --report-output /tmp/aic-completion-first.json > /dev/null
# Exit 1 is expected for this validated partial result.
node docs/benchmarks/constructive-completion-first/inspect-obstruction.mjs \
  /tmp/aic-completion-first.json
```

Artifacts: `heavy-xiranite.json`, `heavy-xiranite.html`, and `inspect-obstruction.mjs`.
The JSON includes the exact request, all accepted nodes, operational counters and the rejected
obstruction witness. The HTML displays accepted history only and explicitly marks partial failure.

Verification: `cargo fmt --all`, `cargo fmt --all -- --check`, `cargo check --workspace`,
`cargo test --workspace`, release CLI help, the full run above, and an invalid seed with structured
`production-input-invalid` JSON and exit 1. All eight HTML history pages were exercised with Chromium;
all 752 rendered route cells across the history expose inspection data. The final accepted page has
164 transport cells, correct facility labels and an explicit last-valid/partial-status marker.
The final workspace suite reports 460 passed and one pre-existing ignored test.
